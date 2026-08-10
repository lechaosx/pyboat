use std::{collections::HashMap, error::Error, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use pyboat_protocol::{ServerMessage, encode_server_message};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, watch};
use tokio::time::interval;
use tokio_tungstenite::{accept_async, tungstenite::Message};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
type Connections = Arc<Mutex<HashMap<String, watch::Sender<Option<Message>>>>>;

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub async fn serve(listener: TcpListener) -> ServerResult<()> {
    let connections = Connections::default();
    let mut next_player_number = 1_u64;
    let mut tick = 0_u64;
    let mut ticker = interval(Duration::from_secs_f64(1.0 / 60.0));
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let player_id = format!("player-{next_player_number}");
                next_player_number += 1;
                let connections = connections.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, player_id, connections).await {
                        eprintln!("WebSocket connection failed: {error}");
                    }
                });
            }
            _ = ticker.tick() => {
                tick += 1;
                let snapshot = encode_server_message(&ServerMessage::Snapshot {
                    tick,
                    boats: Vec::new(),
                    removed_player_ids: Vec::new(),
                })?;
                let snapshot = Message::Text(snapshot.into());
                for sender in connections.lock().await.values() {
                    sender.send_replace(Some(snapshot.clone()));
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    player_id: String,
    connections: Connections,
) -> ConnectionResult<()> {
    let mut connection = accept_async(stream).await?;
    let welcome = encode_server_message(&ServerMessage::Welcome {
        player_id: player_id.clone(),
    })?;
    connection.send(Message::Text(welcome.into())).await?;

    // A watch channel retains only the newest snapshot when a client cannot keep up.
    let (outbound, mut outbound_updates) = watch::channel(None);
    connections.lock().await.insert(player_id.clone(), outbound);
    let result = loop {
        tokio::select! {
            incoming = connection.next() => match incoming {
                Some(Ok(message)) if !message.is_close() => {}
                Some(Ok(_)) | None => break Ok(()),
                Some(Err(error)) => break Err(error),
            },
            changed = outbound_updates.changed() => {
                if changed.is_err() {
                    break Ok(());
                }
                let message = outbound_updates.borrow_and_update().clone();
                if let Some(message) = message {
                    if let Err(error) = connection.send(message).await {
                        break Err(error);
                    }
                }
            }
        }
    };
    connections.lock().await.remove(&player_id);
    result?;
    Ok(())
}
