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

The backend address defaults to `DEPLOY_HOST`. To use a different DNS name or
IPv4 address, set the repository Actions variable `WEBTRANSPORT_HOST`, for example
`boats.dlabaja.eu`. The selected address must reach the server. The frontend
embeds the address when building; changing it requires redeployment.

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

Allow inbound **TCP 80** for Let's Encrypt validation and **UDP 8765** for game
transport in the provider and server firewalls. TCP 80 must be available for
Certbot's temporary HTTP listener whenever issuance or renewal is needed. The
deployment fails if another service owns that port. A DNS name must resolve to
this server; an IPv4 address works directly without DNS.

The SSH user needs Docker access, `flock`, and permission to use `crontab`, with
the host's cron daemon running. The workflow obtains the initial certificate
before starting the backend and installs an hourly renewal job in that user's
crontab. Certificate state and private keys stay on the server under
`~/.local/share/pyboat`, restricted to the SSH user and Docker.

The workflow uses the `pyboat` stack and GHCR image tagged with the commit SHA.
It waits for the
backend rollout before publishing Pages; a rollout restarts the match and
disconnects players. On first deployment, confirm the SSH user can reach an active
Swarm manager and that its ARM64 node can pull GHCR images. Workflow credentials
authorize the image pull even when the package is private.

Push to `master` or run the **Deploy** workflow manually. Check the Actions result,
then open the Pages URL in a browser supporting WebGPU and WebTransport and verify
that multiplayer connects. Running-container convergence does not prove that the
public UDP path or browser certificate validation succeeds.

### Certificate renewal

The renewal job asks Certbot whether the certificate is due, installs a renewed
PEM pair together, and restarts `pyboat_server` to load it. Restarting ends the
current match. A failed restart is retried on the next hourly run. IPv4
certificates use Let's Encrypt's six-day profile, so keep TCP 80 reachable
throughout operation.

Run a renewal check manually on the server with:

```sh
~/.local/share/pyboat/certificates.sh renew
```

Monitor `~/.local/share/pyboat/certificates.log` for renewal failures; the ACME
account is registered without an email address. Keep the persistent
`~/.local/share/pyboat/letsencrypt` directory when redeploying so Certbot can reuse
its account and certificates.
