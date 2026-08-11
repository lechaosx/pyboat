use futures_util::{SinkExt, StreamExt};
use pyboat_browser::MultiplayerClient;
use pyboat_protocol::{ClientMessage, ServerMessage, decode_server_message, encode_client_message};
use pyboat_simulation::DT_SECONDS;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

type Connection = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn next_server_message(connection: &mut Connection) -> ServerMessage {
    let message = timeout(Duration::from_millis(200), connection.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    decode_server_message(message.to_text().unwrap()).unwrap()
}

#[tokio::test]
async fn browser_client_predicts_and_reconciles_against_the_real_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let (mut connection, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let mut client = MultiplayerClient::new();

    client.accept(next_server_message(&mut connection).await);
    client.accept(next_server_message(&mut connection).await);
    let player_id = client.player_id().unwrap().to_owned();
    let initial = client.boat(&player_id).unwrap().snapshot.clone();
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
                    connection
                        .send(Message::Text(encoded.into()))
                        .await
                        .unwrap();
                }
            }

            let message = next_server_message(&mut connection).await;
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
    assert!((authoritative.x - initial.x).hypot(authoritative.y - initial.y) > 0.01);
    let predicted = client.camera_m().unwrap();
    assert!(predicted.x.is_finite() && predicted.y.is_finite());
    assert!((predicted.x - authoritative.x).hypot(predicted.y - authoritative.y) < 1.0);

    connection.close(None).await.unwrap();
    server.abort();
}
