use std::{env, error::Error, io, path::Path, time::Duration};

use wtransport::{Identity, tls::Sha256DigestFmt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    #[cfg(unix)]
    let mut terminate = {
        use tokio::signal::unix::{SignalKind, signal};
        signal(SignalKind::terminate())?
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
    let shutdown = async {
        let reload = async {
            if let (Some(certificate), Some(key)) = (&certificate_path, &private_key_path) {
                poll_certificates(
                    &endpoint,
                    Path::new(certificate),
                    Path::new(key),
                    Duration::from_secs(60),
                )
                .await;
            }
            std::future::pending::<()>().await;
        };
        #[cfg(unix)]
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
            _ = reload => unreachable!(),
        }
        #[cfg(not(unix))]
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = reload => unreachable!(),
        }
    };
    server::serve_until(&endpoint, shutdown).await?;
    Ok(())
}

async fn poll_certificates(
    endpoint: &wtransport::Endpoint<wtransport::endpoint::endpoint_side::Server>,
    certificate: &Path,
    key: &Path,
    period: Duration,
) {
    use wtransport::tls::{
        Certificate, CertificateChain, PrivateKey,
        rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    };

    let mut loaded = None;
    let mut ticker = tokio::time::interval(period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let result: server::ServerResult<()> = async {
            let files = (
                tokio::fs::read(certificate).await?,
                tokio::fs::read(key).await?,
            );
            if loaded.as_ref() == Some(&files) {
                return Ok(());
            }
            let certificates = CertificateDer::pem_slice_iter(&files.0)
                .map(|certificate| Ok(Certificate::from_der(certificate?.to_vec())?))
                .collect::<server::ServerResult<Vec<_>>>()?;
            let key = PrivateKeyDer::from_pem_slice(&files.1)?;
            let identity = Identity::new(
                CertificateChain::new(certificates),
                PrivateKey::from_der_pkcs8(key.secret_der().to_vec()),
            );
            endpoint.reload_config(server::tls_config(endpoint.local_addr()?, identity)?, false)?;
            loaded = Some(files);
            eprintln!("TLS certificate reloaded");
            Ok(())
        }
        .await;
        if let Err(error) = result {
            eprintln!("TLS certificate reload failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests;
