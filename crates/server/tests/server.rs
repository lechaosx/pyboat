use futures_util::{SinkExt, StreamExt};
use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_server_message, encode_client_message,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::{Duration, advance, timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, protocol::frame::coding::CloseCode},
};

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn next_snapshot(client: &mut Client) -> (u64, Vec<BoatSnapshot>, Vec<String>) {
    let message = timeout(Duration::from_millis(100), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match decode_server_message(message.to_text().unwrap()).unwrap() {
        ServerMessage::Snapshot {
            tick,
            boats,
            removed_player_ids,
        } => (tick, boats, removed_player_ids),
        ServerMessage::Welcome { .. } => panic!("expected snapshot message"),
    }
}

#[tokio::test]
async fn concurrent_connections_receive_distinct_ids_and_one_match_seed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));

    let (mut first, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let (mut second, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let first_message = first.next().await.unwrap().unwrap();
    let second_message = second.next().await.unwrap().unwrap();
    assert!(first_message.is_text());
    assert!(second_message.is_text());
    let first_welcome = decode_server_message(first_message.to_text().unwrap()).unwrap();
    let second_welcome = decode_server_message(second_message.to_text().unwrap()).unwrap();

    let ServerMessage::Welcome {
        player_id: first_player_id,
        match_seed: first_match_seed,
    } = first_welcome
    else {
        panic!("expected first welcome message");
    };
    let ServerMessage::Welcome {
        player_id: second_player_id,
        match_seed: second_match_seed,
    } = second_welcome
    else {
        panic!("expected second welcome message");
    };
    assert_eq!(first_player_id, "player-1");
    assert_eq!(second_player_id, "player-2");
    assert_eq!(first_match_seed, second_match_seed);

    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn server_runs_an_authoritative_match_and_removes_disconnected_boats() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let (mut first, _) = connect_async(format!("ws://{address}")).await.unwrap();
    first.next().await.unwrap().unwrap();

    let (initial_tick, initial_boats, initial_removed) = next_snapshot(&mut first).await;
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
    first.send(Message::Text(input.into())).await.unwrap();
    let (acknowledged_tick, acknowledged) = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&mut first).await;
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

    let moved = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&mut first).await;
            if tick >= acknowledged_tick + 30 {
                break boats
                    .into_iter()
                    .find(|boat| boat.id == "player-1")
                    .unwrap();
            }
        }
    })
    .await
    .unwrap();
    assert!(
        (moved.x - acknowledged.x).hypot(moved.y - acknowledged.y) > 0.01,
        "an acknowledged command should advance the authoritative boat"
    );

    let (mut second, _) = connect_async(format!("ws://{address}")).await.unwrap();
    second.next().await.unwrap().unwrap();
    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, _) = next_snapshot(&mut first).await;
            if boats.iter().any(|boat| boat.id == "player-2") {
                break;
            }
        }
    })
    .await
    .unwrap();
    second.close(None).await.unwrap();

    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, removed) = next_snapshot(&mut first).await;
            assert!(removed.is_empty());
            if boats.iter().all(|boat| boat.id != "player-2") {
                break;
            }
        }
    })
    .await
    .unwrap();

    first.close(None).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn snapshots_are_complete_current_interest_sets() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let mut clients = Vec::new();
    for _ in 0..5 {
        let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
        client.next().await.unwrap().unwrap();
        clients.push(client);
    }

    let (registered_tick, _, _) = next_snapshot(&mut clients[4]).await;
    let boats = timeout(Duration::from_secs(1), async {
        loop {
            let (tick, boats, _) = next_snapshot(&mut clients[0]).await;
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
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(by_id["player-1"].acknowledged_input, Some(0));
    assert_eq!(by_id["player-3"].forward, None);
    assert_eq!(by_id["player-3"].turn, None);
    if let Some(boat) = by_id.get("player-4") {
        assert_eq!(boat.forward, None);
        assert_eq!(boat.turn, None);
    }

    clients[4].close(None).await.unwrap();
    clients[3].close(None).await.unwrap();
    timeout(Duration::from_secs(1), async {
        loop {
            let (_, boats, removed) = next_snapshot(&mut clients[0]).await;
            assert!(removed.is_empty());
            if boats.iter().all(|boat| boat.id != "player-4") {
                break;
            }
        }
    })
    .await
    .unwrap();

    for client in clients.iter_mut().take(3) {
        client.close(None).await.unwrap();
    }
    server.abort();
}

#[tokio::test]
async fn server_clamps_viewport_interest_to_prediction_and_visibility_bounds() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let mut clients = Vec::new();
    for _ in 0..5 {
        let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
        client.next().await.unwrap().unwrap();
        clients.push(client);
    }
    let (registered_tick, _, _) = next_snapshot(&mut clients[4]).await;

    let cases: &[(f32, &[&str], &[&str])] = &[
        (
            10.0,
            &["player-1", "player-2"],
            &["player-3", "player-4", "player-5"],
        ),
        (100.0, &["player-1", "player-2", "player-3"], &["player-5"]),
        (1.0, &["player-1"], &["player-3", "player-4", "player-5"]),
    ];
    for (radius, required_ids, forbidden_ids) in cases {
        let viewport = encode_client_message(&ClientMessage::Viewport { radius: *radius }).unwrap();
        clients[0]
            .send(Message::Text(viewport.into()))
            .await
            .unwrap();
        timeout(Duration::from_secs(1), async {
            loop {
                let (tick, boats, _) = next_snapshot(&mut clients[0]).await;
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

    for client in &mut clients {
        client.close(None).await.unwrap();
    }
    server.abort();
}

#[tokio::test]
async fn input_backlog_keeps_only_the_newest_quarter_second() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    client.next().await.unwrap().unwrap();
    let (initial_tick, _, _) = next_snapshot(&mut client).await;

    for sequence in 1..=100 {
        let input = encode_client_message(&ClientMessage::Input {
            sequence,
            forward: if sequence == 100 { -1.0 } else { 1.0 },
            turn: 0.0,
        })
        .unwrap();
        client.send(Message::Text(input.into())).await.unwrap();
    }

    let (acknowledged_tick, acknowledged) = timeout(Duration::from_secs(3), async {
        loop {
            let (tick, boats, _) = next_snapshot(&mut client).await;
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

    client.close(None).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn server_rejects_client_messages_larger_than_one_kibibyte() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    client.next().await.unwrap().unwrap();

    let mut input = encode_client_message(&ClientMessage::Input {
        sequence: 1,
        forward: 1.0,
        turn: 0.0,
    })
    .unwrap();
    input.push_str(&" ".repeat(1024));
    client.send(Message::Text(input.into())).await.unwrap();

    let rejected = timeout(Duration::from_secs(1), async {
        loop {
            match client.next().await {
                Some(Ok(message)) if message.is_text() => {
                    let ServerMessage::Snapshot { boats, .. } =
                        decode_server_message(message.to_text().unwrap()).unwrap()
                    else {
                        panic!("welcome must be sent only once");
                    };
                    if boats.iter().any(|boat| boat.acknowledged_input == Some(1)) {
                        break false;
                    }
                }
                Some(Ok(message)) if message.is_close() => break true,
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break true,
            }
        }
    })
    .await
    .unwrap();

    assert!(rejected);
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn incomplete_handshake_closes_without_consuming_player_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let stream = TcpStream::connect(address).await.unwrap();

    // The handshake task must poll once to register its deadline before the clock
    // advances.
    tokio::task::yield_now().await;
    tokio::task::yield_now().await;
    advance(Duration::from_secs(6)).await;
    let became_readable = timeout(Duration::from_secs(1), stream.readable()).await;

    assert!(became_readable.is_ok());
    let mut byte = [0_u8];
    assert_eq!(stream.try_read(&mut byte).unwrap(), 0);

    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let welcome =
        decode_server_message(client.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
    assert!(matches!(
        welcome,
        ServerMessage::Welcome { player_id, .. } if player_id == "player-1"
    ));
    client.close(None).await.unwrap();
    server.abort();
}

#[tokio::test]
async fn shutdown_signal_stops_server_and_closes_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (shutdown, shutdown_signal) = oneshot::channel();
    let server = tokio::spawn(pyboat_server::serve_until(listener, shutdown_signal));
    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    client.next().await.unwrap().unwrap();

    shutdown.send(()).unwrap();
    timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let close = timeout(Duration::from_secs(1), async {
        loop {
            match client.next().await {
                Some(Ok(Message::Close(close))) => break close,
                Some(Ok(_)) => {}
                Some(Err(error)) => panic!("connection failed before close frame: {error}"),
                None => panic!("connection ended before close frame"),
            }
        }
    })
    .await
    .unwrap();
    let close = close.expect("shutdown close frame must include a reason");
    assert_eq!(close.code, CloseCode::Away);
    assert_eq!(close.reason, "Server shutting down");
}

#[tokio::test]
async fn shutdown_flushes_close_frame_before_server_runtime_stops() {
    let (address_sender, address_receiver) = std::sync::mpsc::sync_channel(1);
    let (shutdown, shutdown_signal) = oneshot::channel();
    let server = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                address_sender.send(listener.local_addr().unwrap()).unwrap();
                pyboat_server::serve_until(listener, shutdown_signal).await
            })
    });
    let address = address_receiver.recv().unwrap();
    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    client.next().await.unwrap().unwrap();

    shutdown.send(()).unwrap();
    server.join().unwrap().unwrap();

    let close = timeout(Duration::from_secs(1), async {
        loop {
            match client.next().await {
                Some(Ok(Message::Close(close))) => break close,
                Some(Ok(_)) => {}
                Some(Err(error)) => panic!("connection failed before close frame: {error}"),
                None => panic!("connection ended before close frame"),
            }
        }
    })
    .await
    .unwrap()
    .expect("shutdown close frame must include a reason");
    assert_eq!(close.code, CloseCode::Away);
    assert_eq!(close.reason, "Server shutting down");
}
