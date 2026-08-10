use futures_util::StreamExt;
use pyboat_protocol::{ServerMessage, decode_server_message};
use tokio::net::TcpListener;
use tokio::time::{Duration, timeout};
use tokio_tungstenite::connect_async;

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
async fn connected_client_receives_increasing_fixed_tick_snapshots() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(pyboat_server::serve(listener));
    let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
    client.next().await.unwrap().unwrap();

    let first = timeout(Duration::from_millis(100), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let second = timeout(Duration::from_millis(100), client.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let first = decode_server_message(first.to_text().unwrap()).unwrap();
    let second = decode_server_message(second.to_text().unwrap()).unwrap();

    let (
        ServerMessage::Snapshot {
            tick: first_tick,
            boats: first_boats,
            removed_player_ids: first_removed,
        },
        ServerMessage::Snapshot {
            tick: second_tick,
            boats: second_boats,
            removed_player_ids: second_removed,
        },
    ) = (first, second)
    else {
        panic!("expected snapshot messages");
    };
    assert!(second_tick > first_tick);
    assert!(first_boats.is_empty());
    assert!(second_boats.is_empty());
    assert!(first_removed.is_empty());
    assert!(second_removed.is_empty());

    client.close(None).await.unwrap();
    server.abort();
}
