use futures_util::{SinkExt, StreamExt};
use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_server_message, encode_client_message,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

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
async fn concurrent_connections_receive_distinct_python_compatible_ids() {
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

    assert_eq!(
        first_welcome,
        ServerMessage::Welcome {
            player_id: "player-1".to_owned(),
        }
    );
    assert_eq!(
        second_welcome,
        ServerMessage::Welcome {
            player_id: "player-2".to_owned(),
        }
    );

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
            if removed == ["player-2"] {
                assert!(boats.iter().all(|boat| boat.id != "player-2"));
                break;
            }
        }
    })
    .await
    .unwrap();

    first.close(None).await.unwrap();
    server.abort();
}
