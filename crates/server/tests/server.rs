use std::collections::HashMap;

use protocol::{
    BoatSnapshot, ClientMessage, PROTOCOL_VERSION, ServerMessage, decode_server_message,
    encode_client_message,
};
use simulation::WorldGeometry;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};
use wtransport::endpoint::{Endpoint, endpoint_side::Client};
use wtransport::error::ConnectionError;
use wtransport::tls::Sha256Digest;
use wtransport::{ClientConfig, Connection, Identity};

struct TestServer {
    address: std::net::SocketAddr,
    certificate_hash: Sha256Digest,
    task: JoinHandle<server::ServerResult<()>>,
}

struct TestClient {
    _endpoint: Endpoint<Client>,
    connection: Connection,
}

fn server_endpoint() -> (
    Endpoint<wtransport::endpoint::endpoint_side::Server>,
    Sha256Digest,
) {
    let identity = Identity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    let certificate_hash = identity.certificate_chain().as_slice()[0].hash();
    let endpoint = server::endpoint("127.0.0.1:0".parse().unwrap(), identity).unwrap();
    (endpoint, certificate_hash)
}

fn start_server() -> TestServer {
    start_server_with_world_geometry(WorldGeometry::default())
}

fn start_server_with_world_geometry(world_geometry: WorldGeometry) -> TestServer {
    let (endpoint, certificate_hash) = server_endpoint();
    let address = endpoint.local_addr().unwrap();
    let task = tokio::spawn(server::serve_with_world_geometry(endpoint, world_geometry));
    TestServer {
        address,
        certificate_hash,
        task,
    }
}

fn client_endpoint(certificate_hash: Sha256Digest) -> Endpoint<Client> {
    Endpoint::client(
        ClientConfig::builder()
            .with_bind_default()
            .with_server_certificate_hashes([certificate_hash])
            .build(),
    )
    .unwrap()
}

async fn connect(server: &TestServer) -> (TestClient, ServerMessage) {
    let endpoint = client_endpoint(server.certificate_hash.clone());
    let connection = endpoint
        .connect(&format!("https://127.0.0.1:{}/game", server.address.port()))
        .await
        .unwrap();
    let welcome = receive_reliable(&connection).await;
    (
        TestClient {
            _endpoint: endpoint,
            connection,
        },
        decode_server_message(&welcome).unwrap(),
    )
}

async fn receive_reliable(connection: &Connection) -> Vec<u8> {
    let mut stream = timeout(Duration::from_secs(1), connection.accept_uni())
        .await
        .unwrap()
        .unwrap();
    let mut message = Vec::new();
    let mut buffer = [0; 256];
    while let Some(read) = stream.read(&mut buffer).await.unwrap() {
        message.extend_from_slice(&buffer[..read]);
    }
    message
}

async fn send_reliable(connection: &Connection, message: &[u8]) {
    let mut stream = connection.open_uni().await.unwrap().await.unwrap();
    stream.write_all(message).await.unwrap();
    stream.finish().await.unwrap();
}

async fn next_snapshot(client: &TestClient) -> (u64, Vec<BoatSnapshot>, Vec<String>) {
    let message = timeout(Duration::from_secs(1), async {
        tokio::select! {
            datagram = client.connection.receive_datagram() => datagram.unwrap().to_vec(),
            stream = client.connection.accept_uni() => {
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
    match decode_server_message(&message).unwrap() {
        ServerMessage::Snapshot {
            tick,
            boats,
            removed_player_ids,
        } => (tick, boats, removed_player_ids),
        ServerMessage::Welcome { .. } => panic!("expected snapshot message"),
    }
}

fn close(client: &TestClient) {
    client.connection.close(0_u32.into(), b"test complete");
}

#[tokio::test]
async fn concurrent_connections_receive_distinct_ids_and_one_match_configuration() {
    let server = start_server();
    let (first, first_welcome) = connect(&server).await;
    let (second, second_welcome) = connect(&server).await;

    let ServerMessage::Welcome {
        player_id: first_player_id,
        match_seed: first_match_seed,
        world_geometry: first_world_geometry,
    } = first_welcome
    else {
        panic!("expected first welcome message");
    };
    let ServerMessage::Welcome {
        player_id: second_player_id,
        match_seed: second_match_seed,
        world_geometry: second_world_geometry,
    } = second_welcome
    else {
        panic!("expected second welcome message");
    };
    assert_eq!(first_player_id, "player-1");
    assert_eq!(second_player_id, "player-2");
    assert_eq!(first_match_seed, second_match_seed);
    assert_eq!(first_world_geometry, WorldGeometry::default());
    assert_eq!(first_world_geometry, second_world_geometry);

    close(&first);
    close(&second);
    server.task.abort();
}

#[tokio::test]
async fn server_welcome_propagates_selected_cylindrical_world_geometry() {
    let world_geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let server = start_server_with_world_geometry(world_geometry);
    let (client, welcome) = connect(&server).await;

    let ServerMessage::Welcome {
        world_geometry: received,
        ..
    } = welcome
    else {
        panic!("expected welcome message");
    };
    assert_eq!(received, world_geometry);

    close(&client);
    server.task.abort();
}

#[tokio::test]
async fn server_runs_an_authoritative_match_and_removes_disconnected_boats() {
    let server = start_server();
    let (first, _) = connect(&server).await;

    let (initial_tick, initial_boats, initial_removed) = next_snapshot(&first).await;
    let initial = initial_boats
        .iter()
        .find(|boat| boat.id == "player-1")
        .unwrap();
    assert_eq!(initial.acknowledged_input, Some(0));
    assert!(initial_removed.is_empty());

    let input = encode_client_message(&ClientMessage::Input {
        sequence: 7,
        forward: 1.0,
        turn: 0.25,
    })
    .unwrap();
    first.connection.send_datagram(input).unwrap();
    let (acknowledged_tick, acknowledged) = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&first).await;
            let local = boats
                .into_iter()
                .find(|boat| boat.id == "player-1")
                .unwrap();
            if local.acknowledged_input == Some(7) {
                break (tick, local);
            }
        }
    })
    .await
    .unwrap();
    assert!(acknowledged_tick > initial_tick);
    assert_eq!(acknowledged.forward, Some(1.0));
    assert_eq!(acknowledged.turn, Some(0.25));

    let (moved_tick, moved) = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&first).await;
            if tick >= acknowledged_tick + 30 {
                break (
                    tick,
                    boats
                        .into_iter()
                        .find(|boat| boat.id == "player-1")
                        .unwrap(),
                );
            }
        }
    })
    .await
    .unwrap();
    assert!(moved_tick >= acknowledged_tick + 30);
    assert!(moved.x.is_finite() && moved.y.is_finite());

    let (second, _) = connect(&server).await;
    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, _) = next_snapshot(&first).await;
            if boats.iter().any(|boat| boat.id == "player-2") {
                break;
            }
        }
    })
    .await
    .unwrap();
    close(&second);

    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, removed) = next_snapshot(&first).await;
            assert!(removed.is_empty());
            if boats.iter().all(|boat| boat.id != "player-2") {
                break;
            }
        }
    })
    .await
    .unwrap();

    close(&first);
    server.task.abort();
}

#[tokio::test]
async fn snapshots_are_complete_current_interest_sets() {
    let server = start_server();
    let mut clients = Vec::new();
    for _ in 0..5 {
        clients.push(connect(&server).await.0);
    }

    let (registered_tick, _, _) = next_snapshot(&clients[4]).await;
    let boats = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&clients[0]).await;
            if tick >= registered_tick {
                break boats;
            }
        }
    })
    .await
    .unwrap();
    let ids = boats
        .iter()
        .map(|boat| boat.id.as_str())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"player-1"));
    assert!(ids.contains(&"player-2"));
    assert!(ids.contains(&"player-3"));
    assert!(!ids.contains(&"player-5"));
    let by_id = boats
        .iter()
        .map(|boat| (boat.id.as_str(), boat))
        .collect::<HashMap<_, _>>();
    assert_eq!(by_id["player-1"].acknowledged_input, Some(0));
    assert_eq!(by_id["player-3"].forward, None);
    assert_eq!(by_id["player-3"].turn, None);
    if let Some(boat) = by_id.get("player-4") {
        assert_eq!(boat.forward, None);
        assert_eq!(boat.turn, None);
    }

    close(&clients[4]);
    close(&clients[3]);
    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, removed) = next_snapshot(&clients[0]).await;
            assert!(removed.is_empty());
            if boats.iter().all(|boat| boat.id != "player-4") {
                break;
            }
        }
    })
    .await
    .unwrap();

    for client in clients.iter().take(3) {
        close(client);
    }
    server.task.abort();
}

#[tokio::test]
async fn server_clamps_viewport_interest_to_prediction_and_visibility_bounds() {
    let server = start_server();
    let mut clients = Vec::new();
    for _ in 0..5 {
        clients.push(connect(&server).await.0);
    }
    let (registered_tick, _, _) = next_snapshot(&clients[4]).await;

    let cases: &[(f32, &[&str], &[&str])] = &[
        (
            10.0,
            &["player-1", "player-2"],
            &["player-3", "player-4", "player-5"],
        ),
        (100.0, &["player-1", "player-2", "player-3"], &["player-5"]),
        (1.0, &["player-1"], &["player-3", "player-4", "player-5"]),
    ];
    for (index, (radius, required_ids, forbidden_ids)) in cases.iter().enumerate() {
        let viewport = encode_client_message(&ClientMessage::Viewport {
            sequence: index as i64 + 1,
            radius: *radius,
        })
        .unwrap();
        send_reliable(&clients[0].connection, &viewport).await;
        timeout(Duration::from_secs(1), async {
            loop {
                let (tick, boats, _) = next_snapshot(&clients[0]).await;
                let ids = boats
                    .iter()
                    .map(|boat| boat.id.as_str())
                    .collect::<Vec<_>>();
                if tick >= registered_tick
                    && required_ids.iter().all(|id| ids.contains(id))
                    && forbidden_ids.iter().all(|id| !ids.contains(id))
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
    }

    for client in &clients {
        close(client);
    }
    server.task.abort();
}

#[tokio::test]
async fn stale_viewport_stream_cannot_overwrite_a_newer_update() {
    let server = start_server();
    let mut clients = Vec::new();
    for _ in 0..5 {
        clients.push(connect(&server).await.0);
    }
    let (registered_tick, _, _) = next_snapshot(&clients[4]).await;
    let mut stale_stream = clients[0]
        .connection
        .open_uni()
        .await
        .unwrap()
        .await
        .unwrap();
    let stale = encode_client_message(&ClientMessage::Viewport {
        sequence: -1,
        radius: 100.0,
    })
    .unwrap();
    stale_stream.write_all(&stale).await.unwrap();
    let current = encode_client_message(&ClientMessage::Viewport {
        sequence: 0,
        radius: 1.0,
    })
    .unwrap();
    send_reliable(&clients[0].connection, &current).await;

    let applied_tick = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&clients[0]).await;
            if tick >= registered_tick
                && boats
                    .iter()
                    .all(|boat| !matches!(boat.id.as_str(), "player-3" | "player-4" | "player-5"))
            {
                break tick;
            }
        }
    })
    .await
    .unwrap();
    stale_stream.finish().await.unwrap();

    let mut checked = 0;
    while checked < 5 {
        let (tick, boats, _) = next_snapshot(&clients[0]).await;
        if tick <= applied_tick {
            continue;
        }
        assert!(
            boats
                .iter()
                .all(|boat| !matches!(boat.id.as_str(), "player-3" | "player-4" | "player-5")),
            "stale viewport update expanded interest",
        );
        checked += 1;
    }

    for client in &clients {
        close(client);
    }
    server.task.abort();
}

#[tokio::test]
async fn input_backlog_keeps_only_the_newest_quarter_second() {
    let server = start_server();
    let (client, _) = connect(&server).await;
    let (initial_tick, _, _) = next_snapshot(&client).await;

    for sequence in 1..=100 {
        let input = encode_client_message(&ClientMessage::Input {
            sequence,
            forward: if sequence == 100 { -1.0 } else { 1.0 },
            turn: 0.0,
        })
        .unwrap();
        client.connection.send_datagram(input).unwrap();
    }

    let (acknowledged_tick, acknowledged) = timeout(Duration::from_secs(3), async {
        loop {
            let (tick, boats, _) = next_snapshot(&client).await;
            let local = boats
                .into_iter()
                .find(|boat| boat.id == "player-1")
                .unwrap();
            if local.acknowledged_input == Some(100) {
                break (tick, local);
            }
        }
    })
    .await
    .unwrap();

    assert!(acknowledged_tick <= initial_tick + 30);
    assert_eq!(acknowledged.forward, Some(-1.0));

    close(&client);
    server.task.abort();
}

#[tokio::test]
async fn unfinished_reliable_stream_does_not_block_input_datagrams() {
    let server = start_server();
    let (client, _) = connect(&server).await;
    next_snapshot(&client).await;

    let mut unfinished = client.connection.open_uni().await.unwrap().await.unwrap();
    unfinished.write_all(&[PROTOCOL_VERSION]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(25)).await;

    let input = encode_client_message(&ClientMessage::Input {
        sequence: 1,
        forward: 1.0,
        turn: 0.0,
    })
    .unwrap();
    client.connection.send_datagram(input).unwrap();

    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, _) = next_snapshot(&client).await;
            let local = boats
                .into_iter()
                .find(|boat| boat.id == "player-1")
                .unwrap();
            if local.acknowledged_input == Some(1) {
                break;
            }
        }
    })
    .await
    .unwrap();

    drop(unfinished);
    close(&client);
    server.task.abort();
}

#[tokio::test]
async fn reset_reliable_stream_does_not_block_later_viewport_updates() {
    let server = start_server();
    let mut clients = Vec::new();
    for _ in 0..5 {
        clients.push(connect(&server).await.0);
    }
    let (registered_tick, _, _) = next_snapshot(&clients[4]).await;

    let mut reset_stream = clients[0]
        .connection
        .open_uni()
        .await
        .unwrap()
        .await
        .unwrap();
    reset_stream.write_all(&[PROTOCOL_VERSION]).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    reset_stream.reset(0_u32.into()).unwrap();

    let viewport = encode_client_message(&ClientMessage::Viewport {
        sequence: 1,
        radius: 1.0,
    })
    .unwrap();
    send_reliable(&clients[0].connection, &viewport).await;

    timeout(Duration::from_secs(1), async {
        let mut checked = 0;
        while checked < 5 {
            let (tick, boats, _) = next_snapshot(&clients[0]).await;
            if tick >= registered_tick
                && boats
                    .iter()
                    .all(|boat| !matches!(boat.id.as_str(), "player-3" | "player-4" | "player-5"))
            {
                checked += 1;
            }
        }
    })
    .await
    .unwrap();

    for client in &clients {
        close(client);
    }
    server.task.abort();
}

#[tokio::test]
async fn server_rejects_client_messages_larger_than_one_kibibyte() {
    let server = start_server();
    let (client, _) = connect(&server).await;
    send_reliable(&client.connection, &vec![0; 1025]).await;

    timeout(Duration::from_secs(1), client.connection.closed())
        .await
        .unwrap();
    server.task.abort();
}

#[tokio::test]
async fn unsupported_reliable_protocol_version_closes_the_session() {
    let server = start_server();
    let (client, _) = connect(&server).await;
    let mut viewport = encode_client_message(&ClientMessage::Viewport {
        sequence: 1,
        radius: 10.0,
    })
    .unwrap();
    viewport[0] = PROTOCOL_VERSION - 1;
    send_reliable(&client.connection, &viewport).await;

    timeout(Duration::from_secs(1), client.connection.closed())
        .await
        .unwrap();
    server.task.abort();
}

#[tokio::test]
async fn failed_session_does_not_consume_player_identity() {
    let server = start_server();
    let endpoint = client_endpoint(server.certificate_hash.clone());
    let invalid = endpoint
        .connect(&format!(
            "https://127.0.0.1:{}/wrong",
            server.address.port()
        ))
        .await;
    assert!(invalid.is_err());

    let (client, welcome) = connect(&server).await;
    assert!(matches!(
        welcome,
        ServerMessage::Welcome { player_id, .. } if player_id == "player-1"
    ));
    close(&client);
    server.task.abort();
}

#[tokio::test]
async fn shutdown_signal_stops_server_and_closes_connections() {
    let (endpoint, certificate_hash) = server_endpoint();
    let address = endpoint.local_addr().unwrap();
    let (shutdown, shutdown_signal) = oneshot::channel();
    let task = tokio::spawn(server::serve_until(endpoint, shutdown_signal));
    let server = TestServer {
        address,
        certificate_hash,
        task,
    };
    let (client, _) = connect(&server).await;

    shutdown.send(()).unwrap();
    timeout(Duration::from_secs(1), server.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let close = timeout(Duration::from_secs(1), client.connection.closed())
        .await
        .unwrap();
    let ConnectionError::ApplicationClosed(close) = close else {
        panic!("expected application close, got {close:?}");
    };
    assert_eq!(close.code(), 1_u32.into());
    assert_eq!(close.reason(), b"Server shutting down");
}

#[tokio::test]
async fn shutdown_flushes_close_before_server_runtime_stops() {
    let (address_sender, address_receiver) = std::sync::mpsc::sync_channel(1);
    let (shutdown, shutdown_signal) = oneshot::channel();
    let server = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let (endpoint, certificate_hash) = server_endpoint();
                address_sender
                    .send((endpoint.local_addr().unwrap(), certificate_hash))
                    .unwrap();
                server::serve_until(endpoint, shutdown_signal).await
            })
    });
    let (address, certificate_hash) = address_receiver.recv().unwrap();
    let endpoint = client_endpoint(certificate_hash);
    let connection = endpoint
        .connect(&format!("https://127.0.0.1:{}/game", address.port()))
        .await
        .unwrap();
    receive_reliable(&connection).await;

    shutdown.send(()).unwrap();
    server.join().unwrap().unwrap();

    let close = timeout(Duration::from_secs(1), connection.closed())
        .await
        .unwrap();
    let ConnectionError::ApplicationClosed(close) = close else {
        panic!("expected application close, got {close:?}");
    };
    assert_eq!(close.code(), 1_u32.into());
    assert_eq!(close.reason(), b"Server shutting down");
}
