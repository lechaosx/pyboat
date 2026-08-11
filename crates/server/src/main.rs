use std::{env, error::Error};

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8765".to_owned());
    let listener = TcpListener::bind(&address).await?;
    println!("Server listening on ws://{}", listener.local_addr()?);
    pyboat_server::serve_until(listener, tokio::signal::ctrl_c()).await?;
    Ok(())
}
