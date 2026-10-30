# Authoritative server

Follow the [root quick start](../../README.md#browser-multiplayer) for local use.
This file covers non-default server operation.

## Listen address

Pass a listen address as the first argument:

```sh
cargo run -p server -- 0.0.0.0:9000
```

Point the browser at the same port:

```text
http://localhost:5173/?webtransportPort=9000&webtransportCertificateHash=d7:3a:...:72:48
```

## Certificates

Without certificate arguments, the server creates a short-lived self-signed
certificate for local hash pinning. For a deployed certificate, pass its PEM
certificate and private key after the address:

```sh
cargo run -p server -- 0.0.0.0:8765 cert.pem key.pem
```

On Unix, SIGHUP reloads these files for new connections while preserving the
current match and existing connections. Invalid replacements leave the loaded
certificate in use and report an error on stderr. SIGTERM stops the match.

## Hosting

The frontend deploys to GitHub Pages on every push to `master`. Set the
repository's Pages source to **GitHub Actions** and its Actions variable
`WEBTRANSPORT_HOST` to the backend's public hostname or IPv4 address, for example
`boats.dlabaja.eu`. That address needs a matching browser-trusted certificate.
It is embedded during the frontend build; changing it requires redeployment.

The backend runs as a static ARM64 binary on NixOS under systemd. The **Deploy
backend (discard current match)** workflow is manual: select a ref and check
`discard_current_match` before running it. It tests and builds a static ARM64
executable, uploads it, and restarts the service. **Deployment permanently
loses the game in progress and disconnects all players.**

Frontend releases proceed independently. A frontend protocol change can require
a deliberate backend deployment before multiplayer works again.

Create the `production` GitHub environment with these secrets:

| Secret | Value |
| --- | --- |
| `DEPLOY_HOST` | Server's externally reachable SSH hostname or IPv4 address |
| `DEPLOY_USER` | SSH user with passwordless sudo for installation and systemd |
| `DEPLOY_PORT` | Externally reachable SSH port |
| `DEPLOY_SSH_KEY` | Private deployment key authorized for that user |
| `DEPLOY_KNOWN_HOSTS` | Verified OpenSSH known-hosts entry, including `[host]:port` for a non-default port |

`DEPLOY_HOST` serves SSH access; `WEBTRANSPORT_HOST` serves browser traffic.
Local builds without `WEBTRANSPORT_HOST` use the page's hostname and retain the
`webtransportPort` and `webtransportCertificateHash` query parameters.

### Host and router setup

The Pi's NixOS configuration lives in
[nixos-config](https://github.com/lechaosx/nixos-config/tree/main/hosts/raspberrypi).
That configuration owns the service, certificate issuance and renewal, and host
firewall. The game binary is deployed separately by this repository's backend
workflow.

Forward these router ports to the Pi's reserved LAN address:

| Traffic | Public port | Pi port |
| --- | --- | --- |
| Let's Encrypt HTTP validation | TCP 80 | TCP 80 |
| WebTransport game | UDP 8765 | UDP 8765 |
| GitHub Actions deployment | TCP `DEPLOY_PORT` | TCP 22 |

The public backend address must lead to the Pi. Keep TCP 80 forwarded for
certificate renewal. GitHub Pages serves the frontend; the Pi's game transport
does not need TCP 443.

After installing NixOS, verify the Pi's new SSH host key locally and update
`DEPLOY_KNOWN_HOSTS` before deploying. The configured deployment user's public
key must match `DEPLOY_SSH_KEY`. Certificate issuance must succeed before the
first backend deployment.

Renewal reloads the running server's TLS certificate without discarding the
match. Inspect `journalctl -u acme-order-renew-pyboat.service` for issuance or renewal errors
and `journalctl -u pyboat.service` for game startup or reload errors.

After deployment, open Pages in a browser supporting WebGPU and WebTransport
and verify multiplayer connects. An active service does not prove that router
forwarding or browser certificate validation works.
