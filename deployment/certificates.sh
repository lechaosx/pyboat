#!/usr/bin/env bash
set -euo pipefail
umask 077

state_directory=$(cd -- "$(dirname -- "$0")" && pwd)
certificate_directory="$state_directory/certificates"
[[ "$state_directory" =~ ^/[a-zA-Z0-9_./-]+$ ]]

case "${1:-}" in
  install)
    address=${2:?Backend hostname or IPv4 address is required}
    [[ "$address" =~ ^[a-zA-Z0-9]([a-zA-Z0-9.-]*[a-zA-Z0-9])?$ ]]
    command -v crontab >/dev/null
    command -v flock >/dev/null
    if [[ ! -f "$state_directory/address" ]]; then
      printf '%s\n' "$address" > "$state_directory/address.next"
      mv "$state_directory/address.next" "$state_directory/address"
    fi
    existing_crontab=$(crontab -l 2>/dev/null || true)
    {
      printf '%s\n' "$existing_crontab" | sed '/ # pyboat-certificates$/d'
      printf '17 * * * * %s renew >>%s 2>&1 # pyboat-certificates\n' \
        "$state_directory/certificates.sh" "$state_directory/certificates.log"
    } | crontab -
    "$0" renew "$address"
    ;;
  renew)
    exec 9>"$state_directory/certificates.lock"
    flock 9
    if [[ -n "${2:-}" ]]; then
      address=$2
    else
      read -r address < "$state_directory/address"
    fi
    [[ "$address" =~ ^[a-zA-Z0-9]([a-zA-Z0-9.-]*[a-zA-Z0-9])?$ ]]
    mkdir -p "$state_directory/letsencrypt" "$certificate_directory"
    chmod 700 "$state_directory" "$state_directory/letsencrypt" "$certificate_directory"
    identifier=(--domain "$address")
    if [[ "$address" =~ ^([0-9]+\.){3}[0-9]+$ ]]; then
      identifier=(--ip-address "$address" --preferred-profile shortlived)
    fi
    certificate_status=0
    docker run --rm --publish 80:80 --user "$(id -u):$(id -g)" \
      --mount "type=bind,source=$state_directory/letsencrypt,target=/etc/letsencrypt" \
      certbot/certbot:v5.9.0 certonly --standalone --preferred-challenges http \
      --non-interactive --agree-tos --register-unsafely-without-email \
      --work-dir /tmp/work --logs-dir /tmp/logs \
      --cert-name "$address" --keep-until-expiring "${identifier[@]}" || certificate_status=$?
    lineage="$state_directory/letsencrypt/live/$address"
    if ((certificate_status == 0)); then
      printf '%s\n' "$address" > "$state_directory/address.next"
      mv "$state_directory/address.next" "$state_directory/address"
    fi
    if ((certificate_status == 0)) && \
       { ! cmp -s "$lineage/fullchain.pem" "$certificate_directory/fullchain.pem" || \
         ! cmp -s "$lineage/privkey.pem" "$certificate_directory/privkey.pem"; }; then
      generation=$(mktemp -d "$certificate_directory/generation.XXXXXX")
      install -m 600 "$lineage/fullchain.pem" "$generation/fullchain.pem"
      install -m 600 "$lineage/privkey.pem" "$generation/privkey.pem"
      ln -sfn current/fullchain.pem "$certificate_directory/fullchain.pem"
      ln -sfn current/privkey.pem "$certificate_directory/privkey.pem"
      ln -sfn "${generation##*/}" "$certificate_directory/current.next"
      touch "$state_directory/restart-pending"
      mv -Tf "$certificate_directory/current.next" "$certificate_directory/current"
    fi
    if [[ -f "$state_directory/restart-pending" ]]; then
      service=$(docker service ls --filter name=pyboat_server --format '{{.Name}}')
      if printf '%s\n' "$service" | grep -qx pyboat_server; then
        docker service update --force --detach=false pyboat_server
      fi
      rm "$state_directory/restart-pending"
      find "$certificate_directory" -mindepth 1 -maxdepth 1 -type d -name 'generation.*' \
        ! -name "$(readlink "$certificate_directory/current")" -exec rm -r -- {} +
    fi
    exit "$certificate_status"
    ;;
  *)
    echo 'Usage: certificates.sh install HOST | renew' >&2
    exit 2
    ;;
esac
