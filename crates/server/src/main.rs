use std::{env, error::Error, io};

use wtransport::{Identity, tls::Sha256DigestFmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    #[cfg(unix)]
    let (mut reload, mut terminate) = {
        use tokio::signal::unix::{SignalKind, signal};
        (
            signal(SignalKind::hangup())?,
            signal(SignalKind::terminate())?,
        )
    };
    let mut arguments = env::args().skip(1);
    let address = arguments
        .next()
        .unwrap_or_else(|| "127.0.0.1:8765".to_owned());
    let certificate_path = arguments.next();
    let private_key_path = arguments.next();
    let identity = match (&certificate_path, &private_key_path) {
        (Some(certificate_path), Some(private_key_path)) => {
            Identity::load_pemfiles(certificate_path, private_key_path).await?
        }
        (None, None) => Identity::self_signed(["localhost", "127.0.0.1", "::1"])?,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "certificate and private-key paths must be provided together",
            )
            .into());
        }
    };
    let certificate_hash = identity
        .certificate_chain()
        .as_slice()
        .first()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "certificate chain is empty"))?
        .hash()
        .fmt(Sha256DigestFmt::DottedHex);
    let endpoint = server::endpoint(address.parse()?, identity)?;
    println!(
        "Server listening on https://{}/game",
        endpoint.local_addr()?
    );
    println!("Browser certificate hash: {certificate_hash}");
    #[cfg(unix)]
    {
        let shutdown = async {
            loop {
                tokio::select! {
                    result = tokio::signal::ctrl_c() => return result,
                    _ = terminate.recv() => return Ok(()),
                    _ = reload.recv() => {
                        if let (Some(certificate), Some(key)) = (&certificate_path, &private_key_path) {
                            let result: server::ServerResult<()> = async {
                                let identity = Identity::load_pemfiles(certificate, key).await?;
                                let config = server::tls_config(endpoint.local_addr()?, identity)?;
                                endpoint.reload_config(config, false)?;
                                Ok(())
                            }.await;
                            match result {
                                Ok(()) => eprintln!("TLS certificate reloaded"),
                                Err(error) => eprintln!("TLS certificate reload failed: {error}"),
                            }
                        }
                    }
                }
            }
        };
        server::serve_until(&endpoint, shutdown).await?;
    }
    #[cfg(not(unix))]
    server::serve_until(&endpoint, tokio::signal::ctrl_c()).await?;
    Ok(())
}
