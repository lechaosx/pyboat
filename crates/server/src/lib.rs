use std::{error::Error, io};

use futures_util::{SinkExt, StreamExt};
use pyboat_protocol::{ServerMessage, encode_server_message};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::Message};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub async fn serve(listener: TcpListener) -> io::Result<()> {
    let mut next_player_number = 1_u64;
    loop {
        let (stream, _) = listener.accept().await?;
        let player_id = format!("player-{next_player_number}");
        next_player_number += 1;
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, player_id).await {
                eprintln!("WebSocket connection failed: {error}");
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, player_id: String) -> ConnectionResult<()> {
    let mut connection = accept_async(stream).await?;
    let welcome = encode_server_message(&ServerMessage::Welcome { player_id })?;
    connection.send(Message::Text(welcome.into())).await?;
    while let Some(message) = connection.next().await {
        if message?.is_close() {
            break;
        }
    }
    Ok(())
}
