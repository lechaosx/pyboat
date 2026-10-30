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

The server checks the contents of both files every minute. Valid replacements
apply to new connections while preserving the current match and existing
connections. Invalid or incomplete replacements leave the loaded certificate
in use and are retried on the next check, with errors reported on stderr.
On Unix, SIGTERM stops the match.

## Hosting

The frontend deploys to GitHub Pages on every push to `master`. Set the
repository's Pages source to **GitHub Actions** and its Actions variable
`WEBTRANSPORT_HOST` to the backend's public hostname or IPv4 address, for example
`boats.dlabaja.eu`. That address needs a matching browser-trusted certificate.
It is embedded during the frontend build; changing it requires redeployment.
Production uses UDP 443; local builds retain UDP 8765 unless `WEBTRANSPORT_PORT`
is set at build time or `webtransportPort` is supplied in the browser URL.

The backend runs with socketless `podman-compose` in rootless Podman on NixOS.
The **Deploy backend (discard
current match)** workflow is manual: run it from `master`, set `revision` to
the game commit, tag, or branch, and check `discard_current_match`.
It tests and publishes an ARM64
image to GHCR, pulls that exact digest on the Pi, and restarts the user service.
**Deployment permanently loses the game in progress and disconnects all players.**

On the first run, change the GHCR package's visibility to **Public** after the
image is published, then rerun the failed deployment job. The Pi pulls images
anonymously. Deployment saves the exact image digest in `.env`; service starts
never pull an image. Deployment prunes unused, untagged images after restarting.
To roll back, keep the workflow on `master` and set `revision` to an earlier
commit. Current deployment files package the selected game source; the selected
commit does not need to contain a deployment workflow. It must support the
current server arguments and browser protocol. Revisions predating certificate
polling do not reload renewed certificates automatically.

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
That configuration provides Podman, `podman-compose`, a lingering `github`
deployment account, and a firewall for unprivileged web ports. This repository's
[Compose manifest](../../deploy/compose.yaml) owns the game and certificate renewal.
A user service starts these containers at boot. The game stays stopped after a
crash; starting it again is a deliberate action.

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
key must match `DEPLOY_SSH_KEY`. Bootstrap requests a certificate before the
first deployment, so DNS and HTTP forwarding must already work. Certificates
use Let's Encrypt's short-lived profile and an ACME account without an email
address. acme.sh's daemon schedules renewal independently of game deployments.
Its persistent account state stays in `~/.local/share/pyboat/acme`; only exported
certificates in `tls` are mounted read-only into the game.

### Host bootstrap

Log in as `github` and prepare persistent storage:

```sh
mkdir -p ~/.local/share/pyboat/acme ~/.local/share/pyboat/tls
chmod 700 ~/.local/share/pyboat/acme ~/.local/share/pyboat/tls
```

Copy `deploy/compose.yaml` into `~/.local/share/pyboat/`, then obtain and export the
certificate for the public backend address:

```sh
cd ~/.local/share/pyboat
host=boats.dlabaja.eu
podman-compose pull certificates
podman-compose run --rm -T --name pyboat-certificate-setup certificates \
  --issue --standalone --httpport 8080 \
  --server letsencrypt --certificate-profile shortlived -d "$host" </dev/null
podman-compose run --rm -T --name pyboat-certificate-setup certificates \
  --install-cert -d "$host" \
  --key-file /certificates/key.pem --fullchain-file /certificates/fullchain.pem </dev/null
```

Run the manual backend workflow to install the boot service and start both
containers. When changing the public backend address, stop the certificate
service with `podman-compose stop certificates`, repeat issuance and export for
the new address, then start it with
`podman-compose up -d certificates` so validation commands share port 8080
without overlapping. Update `WEBTRANSPORT_HOST` and deploy the frontend.

### Operation

Renewal reloads the running server's TLS certificate without discarding the
match. Logged in as `github`, run `cd ~/.local/share/pyboat`, then inspect
`podman-compose logs certificates` for issuance or renewal errors and
`podman-compose logs game` for game startup or reload errors.
`journalctl --user -u pyboat-compose.service` reports container startup errors.

After deployment, open Pages in a browser supporting WebGPU and WebTransport
and verify multiplayer connects. An active service does not prove that router
forwarding or browser certificate validation works.
