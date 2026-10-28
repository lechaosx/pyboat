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

## Hosting

The deployment workflow publishes the game to GitHub Pages and runs the backend
in the existing ARM64 Docker Swarm. Configure the repository's Pages source as
**GitHub Actions**.

Set the repository Actions variable `WEBTRANSPORT_HOST` to the backend's DNS name
or IPv4 address, for example `boats.dlabaja.eu`. It must reach the server and be
covered by a browser-trusted certificate. The frontend embeds the address when
building; changing it requires redeployment.

Local builds without `WEBTRANSPORT_HOST` use the page's hostname and retain the
`webtransportPort` and `webtransportCertificateHash` query parameters.

Create the `production` GitHub environment with these secrets:

| Secret | Value |
| --- | --- |
| `DEPLOY_HOST` | Server's SSH hostname or IPv4 address |
| `DEPLOY_USER` | SSH user permitted to manage its Docker Swarm |
| `DEPLOY_PORT` | SSH port |
| `DEPLOY_SSH_KEY` | Private deployment key authorized for that user |
| `DEPLOY_KNOWN_HOSTS` | Verified OpenSSH known-hosts entry, including `[host]:port` for a non-default port |

Set the `production` environment's Actions variable `PYBOAT_CERTIFICATE_DIR` to
an absolute directory on the server containing `fullchain.pem` and `privkey.pem`.
Mount a directory whose certificate files and any symlink targets are accessible
inside that directory; Let's Encrypt's `live/` directory alone usually points
outside itself. Keep the
private key restricted to the server administrator and the container. Certificate
issuance and renewal must already be configured for the selected address.

Allow inbound **UDP 8765** in the provider and server firewalls. The workflow uses
the `pyboat` stack and GHCR image tagged with the commit SHA. It waits for the
backend rollout before publishing Pages; a rollout restarts the match and
disconnects players. On first deployment, confirm the SSH user can reach an active
Swarm manager and that its ARM64 node can pull GHCR images. Workflow credentials
authorize the image pull even when the package is private.

Push to `master` or run the **Deploy** workflow manually. Check the Actions result,
then open the Pages URL in a browser supporting WebGPU and WebTransport and verify
that multiplayer connects. Running-container convergence does not prove that the
public UDP path or browser certificate validation succeeds.

### Certificate renewal

The server reads certificates at startup. After each successful renewal, update
the files in the mounted directory and run this on the Swarm manager as a renewal
deploy hook:

```sh
docker service update --force --detach=false pyboat_server
```

Restarting ends the current match. Run the same hook after a manual certificate
replacement, and monitor renewal failures before the current certificate expires.
