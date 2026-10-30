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
Production uses UDP 443; local builds retain UDP 8765 unless `WEBTRANSPORT_PORT`
is set at build time or `webtransportPort` is supplied in the browser URL.

The backend runs in rootless Podman on NixOS. The **Deploy backend (discard
current match)** workflow is manual: select a ref and check
`discard_current_match` before running it. It tests and publishes an ARM64
image to GHCR, pulls that exact digest on the Pi, and restarts the user service.
**Deployment permanently loses the game in progress and disconnects all players.**

On the first run, change the GHCR package's visibility to **Public** after the
image is published, then rerun the failed deployment job. The Pi pulls images
anonymously. Its local `localhost/pyboat:deployed` tag changes only during manual
deployment; service starts never pull an image.
Deployment retains the previous image as `localhost/pyboat:previous` and removes
unused, untagged images after a successful restart. To deliberately roll back,
log in as `github`, run `podman tag localhost/pyboat:previous localhost/pyboat:deployed`,
then `systemctl --user restart pyboat.service`; this also discards the match.

Frontend releases proceed independently. A frontend protocol change can require
a deliberate backend deployment before multiplayer works again.

Create these variables and secret in the `production` GitHub environment:

| Setting | Type | Value |
| --- | --- | --- |
| `DEPLOY_HOST` | Variable | Server's externally reachable SSH hostname or IPv4 address |
| `DEPLOY_USER` | Variable | `github` |
| `DEPLOY_PORT` | Variable | Externally reachable SSH port |
| `DEPLOY_KNOWN_HOSTS` | Variable | Verified OpenSSH host key recorded under the alias `raspberrypi` |
| `DEPLOY_SSH_KEY` | Secret | Private deployment key authorized for that user |

`DEPLOY_HOST` serves SSH access; `WEBTRANSPORT_HOST` serves browser traffic.
Local builds without `WEBTRANSPORT_HOST` use the page's hostname and retain the
`webtransportPort` and `webtransportCertificateHash` query parameters.

### Host and router setup

The Pi's NixOS configuration lives in
[nixos-config](https://github.com/lechaosx/nixos-config/tree/main/hosts/raspberrypi).
That configuration provides Podman, a lingering `github` deployment account,
and a firewall for unprivileged web ports. This
repository's [deployment files](../../deploy) own the game and certificate
renewal. The backend workflow installs them as user services without sudo.

Forward these router ports to the Pi's reserved LAN address:

| Traffic | Public port | Pi port |
| --- | --- | --- |
| Let's Encrypt HTTP validation | TCP 80 | TCP 8080 |
| WebTransport game | UDP 443 | UDP 8443 |
| GitHub Actions deployment | TCP `DEPLOY_PORT` | TCP 22 |

The public backend address must lead to the Pi. Keep TCP 80 forwarded for
certificate renewal. GitHub Pages serves the frontend; the Pi's game transport
does not need TCP 443.

After installing NixOS, verify the Pi's new SSH host key locally and update
`DEPLOY_KNOWN_HOSTS` before deploying. The configured deployment user's public
key must match `DEPLOY_SSH_KEY`. The deployment requests a certificate before
starting the game, so DNS and HTTP forwarding must already work. Certificates
use Let's Encrypt's short-lived profile and an ACME account without an email
address; monitor renewal failures through the user journal.

Renewal reloads the running server's TLS certificate without discarding the
match. Logged in as `github`, inspect
`journalctl --user -u pyboat-certificate.service` for issuance or renewal errors
and `journalctl --user -u pyboat.service` for game startup or reload errors.

After deployment, open Pages in a browser supporting WebGPU and WebTransport
and verify multiplayer connects. An active service does not prove that router
forwarding or browser certificate validation works.
