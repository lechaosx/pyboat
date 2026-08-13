use pyboat_browser::MultiplayerClient;
use pyboat_protocol::{ClientMessage, ServerMessage, decode_server_message, encode_client_message};
use pyboat_simulation::DT_SECONDS;
use tokio::time::{Duration, timeout};
use wtransport::{ClientConfig, Connection, Endpoint, Identity};

async fn next_server_message(connection: &Connection) -> ServerMessage {
    let message = timeout(Duration::from_secs(1), async {
        tokio::select! {
            datagram = connection.receive_datagram() => datagram.unwrap().to_vec(),
            stream = connection.accept_uni() => {
                let mut stream = stream.unwrap();
                let mut message = Vec::new();
                let mut buffer = [0; 512];
                while let Some(read) = stream.read(&mut buffer).await.unwrap() {
                    message.extend_from_slice(&buffer[..read]);
                }
                message
            }
        }
    })
    .await
    .unwrap();
    decode_server_message(&message).unwrap()
}

#[tokio::test]
async fn browser_client_predicts_and_reconciles_against_the_real_server() {
    let identity = Identity::self_signed(["127.0.0.1"]).unwrap();
    let certificate_hash = identity.certificate_chain().as_slice()[0].hash();
    let endpoint = pyboat_server::endpoint("127.0.0.1:0".parse().unwrap(), identity).unwrap();
    let address = endpoint.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(endpoint));
    let client_endpoint = Endpoint::client(
        ClientConfig::builder()
            .with_bind_default()
            .with_server_certificate_hashes([certificate_hash])
            .build(),
    )
    .unwrap();
    let connection = client_endpoint
        .connect(&format!("https://127.0.0.1:{}/game", address.port()))
        .await
        .unwrap();
    let mut client = MultiplayerClient::new();

    client.accept(next_server_message(&connection).await);
    client.accept(next_server_message(&connection).await);
    let player_id = client.player_id().unwrap().to_owned();
    client.set_motion_intent(1.0, 0.5);

    let mut last_sequence = 0;
    let mut received_snapshots = 0;
    let mut skipped_snapshots = 0;
    let authoritative = timeout(Duration::from_secs(2), async {
        loop {
            if last_sequence < 30 {
                for input in client.take_inputs(DT_SECONDS) {
                    let ClientMessage::Input { sequence, .. } = &input else {
                        panic!("fixed-tick input generation returned a viewport message");
                    };
                    last_sequence = *sequence;
                    let encoded = encode_client_message(&input).unwrap();
                    connection.send_datagram(encoded).unwrap();
                }
            }

            let message = next_server_message(&connection).await;
            let local = match &message {
                ServerMessage::Snapshot { boats, .. } => {
                    received_snapshots += 1;
                    boats
                        .iter()
                        .find(|boat| boat.id == player_id)
                        .unwrap()
                        .clone()
                }
                ServerMessage::Welcome { .. } => panic!("welcome must be sent only once"),
            };
            let fully_acknowledged = local.acknowledged_input == Some(30);
            if received_snapshots % 4 == 0 || fully_acknowledged {
                client.accept(message);
            } else {
                skipped_snapshots += 1;
            }
            if fully_acknowledged {
                break local;
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(last_sequence, 30);
    assert!(skipped_snapshots > 0);
    assert_eq!(
        client.boat(&player_id).unwrap().snapshot.acknowledged_input,
        Some(30)
    );
    assert!(authoritative.x.is_finite() && authoritative.y.is_finite());
    let predicted = client.camera_m().unwrap();
    assert!(predicted.x.is_finite() && predicted.y.is_finite());
    assert!((predicted.x - authoritative.x).hypot(predicted.y - authoritative.y) < 1.0);

    connection.close(0_u32.into(), b"test complete");
    server.abort();
}
