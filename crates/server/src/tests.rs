use super::*;
use protocol::{ServerMessage, decode_server_message};
use std::{path::Path, time::Duration};
use tokio::time::timeout;
use wtransport::{ClientConfig, Connection, Endpoint, endpoint::endpoint_side::Client};

fn client(identity: &Identity) -> Endpoint<Client> {
    Endpoint::client(
        ClientConfig::builder()
            .with_bind_default()
            .with_server_certificate_hashes([identity.certificate_chain().as_slice()[0].hash()])
            .build(),
    )
    .unwrap()
}

async fn welcome(connection: &Connection) -> ServerMessage {
    let mut stream = timeout(Duration::from_secs(2), connection.accept_uni())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0; 512];
    while let Some(count) = stream.read(&mut buffer).await.unwrap() {
        bytes.extend_from_slice(&buffer[..count]);
    }
    decode_server_message(&bytes).unwrap()
}

async fn write_identity(identity: &Identity, certificate: &Path, key: &Path) {
    identity
        .certificate_chain()
        .store_pemfile(certificate)
        .await
        .unwrap();
    identity
        .private_key()
        .store_secret_pemfile(key)
        .await
        .unwrap();
}

fn replace_preserving_metadata(path: &Path, contents: &str) {
    let modified = std::fs::metadata(path).unwrap().modified().unwrap();
    let mut contents = contents.as_bytes().to_vec();
    assert!(contents.len() < 4096);
    contents.resize(4096, b'\n');
    std::fs::write(path, contents).unwrap();
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
}

#[tokio::test]
async fn certificate_polling_preserves_connections_and_retries_invalid_replacements() {
    let directory = std::env::temp_dir().join(format!(
        "pyboat-poll-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    tokio::fs::create_dir(&directory).await.unwrap();
    let certificate = directory.join("cert.pem");
    let key = directory.join("key.pem");
    let identity = Identity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    write_identity(&identity, &certificate, &key).await;
    replace_preserving_metadata(
        &certificate,
        &identity.certificate_chain().as_slice()[0].to_pem(),
    );
    replace_preserving_metadata(&key, &identity.private_key().to_secret_pem());
    let endpoint = server::endpoint(
        "127.0.0.1:0".parse().unwrap(),
        Identity::load_pemfiles(&certificate, &key).await.unwrap(),
    )
    .unwrap();
    let url = format!("https://{}/game", endpoint.local_addr().unwrap());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task_certificate = certificate.clone();
    let task_key = key.clone();
    let task = tokio::spawn(async move {
        server::serve_until(&endpoint, async {
            tokio::select! {
                _ = stopped => Ok::<(), io::Error>(()),
                _ = poll_certificates(&endpoint, &task_certificate, &task_key, Duration::from_millis(20)) => unreachable!(),
            }
        }).await.unwrap();
    });
    let first_endpoint = client(&identity);
    let first = first_endpoint.connect(&url).await.unwrap();
    let ServerMessage::Welcome {
        player_id,
        match_seed,
        ..
    } = welcome(&first).await
    else {
        panic!("expected welcome")
    };
    assert_eq!(player_id, "player-1");
    let before = timeout(Duration::from_secs(2), first.receive_datagram())
        .await
        .unwrap()
        .unwrap();
    let ServerMessage::Snapshot {
        tick: before_tick, ..
    } = decode_server_message(&before).unwrap()
    else {
        panic!("expected snapshot")
    };
    let renewed = Identity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    replace_preserving_metadata(
        &certificate,
        &renewed.certificate_chain().as_slice()[0].to_pem(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let old_endpoint = client(&identity);
    let old = old_endpoint.connect(&url).await.unwrap();
    assert!(
        matches!(welcome(&old).await, ServerMessage::Welcome { match_seed: seed, .. } if seed == match_seed)
    );
    old.close(0_u32.into(), b"complete");
    tokio::fs::write(&key, b"invalid private key")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let old = old_endpoint.connect(&url).await.unwrap();
    assert!(
        matches!(welcome(&old).await, ServerMessage::Welcome { match_seed: seed, .. } if seed == match_seed)
    );
    old.close(0_u32.into(), b"complete");
    tokio::fs::remove_file(&key).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let old = old_endpoint.connect(&url).await.unwrap();
    assert!(
        matches!(welcome(&old).await, ServerMessage::Welcome { match_seed: seed, .. } if seed == match_seed)
    );
    old.close(0_u32.into(), b"complete");
    renewed
        .private_key()
        .store_secret_pemfile(&key)
        .await
        .unwrap();
    replace_preserving_metadata(&key, &renewed.private_key().to_secret_pem());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let new_endpoint = client(&renewed);
    let replacement = timeout(Duration::from_secs(2), new_endpoint.connect(&url))
        .await
        .expect("polling did not install the renewed certificate")
        .unwrap();
    assert!(
        matches!(welcome(&replacement).await, ServerMessage::Welcome { match_seed: seed, .. } if seed == match_seed)
    );
    let next_identity = Identity::self_signed(["localhost", "127.0.0.1"]).unwrap();
    replace_preserving_metadata(
        &certificate,
        &next_identity.certificate_chain().as_slice()[0].to_pem(),
    );
    replace_preserving_metadata(&key, &next_identity.private_key().to_secret_pem());
    tokio::time::sleep(Duration::from_millis(100)).await;
    let next_endpoint = client(&next_identity);
    let next = timeout(Duration::from_secs(2), next_endpoint.connect(&url))
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(welcome(&next).await, ServerMessage::Welcome { match_seed: seed, .. } if seed == match_seed)
    );
    timeout(Duration::from_secs(2), async {
        loop {
            let bytes = first.receive_datagram().await.unwrap();
            if let ServerMessage::Snapshot { tick, boats, .. } =
                decode_server_message(&bytes).unwrap()
                && tick > before_tick
                && boats.iter().any(|boat| boat.id == player_id)
            {
                break;
            }
        }
    })
    .await
    .unwrap();
    first.close(0_u32.into(), b"complete");
    replacement.close(0_u32.into(), b"complete");
    next.close(0_u32.into(), b"complete");
    stop.send(()).unwrap();
    task.await.unwrap();
    tokio::fs::remove_dir_all(directory).await.unwrap();
}
