use std::{env, error::Error, io};

use wtransport::{Identity, tls::Sha256DigestFmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut arguments = env::args().skip(1);
    let address = arguments
        .next()
        .unwrap_or_else(|| "127.0.0.1:8765".to_owned());
    let certificate_path = arguments.next();
    let private_key_path = arguments.next();
    let identity = match (certificate_path, private_key_path) {
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
    let certificate_hash = identity.certificate_chain().as_slice()[0]
        .hash()
        .fmt(Sha256DigestFmt::DottedHex);
    let endpoint = server::endpoint(address.parse()?, identity)?;
    println!(
        "Server listening on https://{}/game",
        endpoint.local_addr()?
    );
    println!("Browser certificate hash: {certificate_hash}");
    server::serve_until(endpoint, tokio::signal::ctrl_c()).await?;
    Ok(())
}
