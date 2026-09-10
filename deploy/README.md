# TAPNap host console

Install one Bash script on the Linux host. `tap` calls Docker, Git, Nginx,
Certbot and systemd directly. The console itself is pure Shell; Certbot has its
own packaged dependencies, including Python.

```text
server main → ACR overseas automatic build → backend image
Verifier main → GitHub tests/build → ecs-web static branch
ECS: tap update → pull image + static files → Docker backend + host Nginx
```

Production deployment stays manual. GitHub never connects to ECS by SSH.
Redis runs in Docker with its existing host data directory. The Apple App
Attest root CA is inside the backend image; Rust reads and validates it.
Let's Encrypt certificates belong to host Nginx and are independent of it.

## Install and set up

The host needs Bash, Docker, Git, curl, tar, Nginx, Certbot and its packaged
systemd renewal timer. `flock`, `free` and the other commands are ordinary Linux
utilities. Use the distribution's packages; configure Docker's mirror in the
Docker daemon if needed. For private ACR images, run `sudo docker login REGISTRY`
once. The default backend image is the public Hangzhou ACR `v1` image.

After these changes are published, download just the script on ECS:

```sh
curl -fL https://raw.githubusercontent.com/TAP-NAP/server/main/deploy/tap -o /tmp/tap
sudo install -m 0755 /tmp/tap /usr/local/bin/tap
sudo tap config
sudo tap setup
```

`config` opens `/etc/tapnap.conf` in `${EDITOR:-vi}`. Every setting has a comment;
check the Apple identifiers and fill in the Let's Encrypt account email. Use
`production` for the release App; the current Debug App uses `development`.
After changing backend settings, run `tap restart`. Changing domain or host
port requires `tap setup` to also regenerate Nginx configuration.

Before `setup`, point `www.tapnap.net` and `tapnap.net` to ECS and allow incoming
80/443. The frontend's `publish-ecs` job must have created `ecs-web` first.
`setup` pulls the backend, starts Redis and Rust, downloads the website, writes
`/etc/nginx/conf.d/tapnap.conf`, obtains HTTPS certificates using Certbot's
webroot mode, and enables the installed Certbot renewal timer. It can be rerun;
it keeps Redis data and reuses unexpired certificates. On a host with an
existing site, remove conflicting server blocks for these two domains first.

The script checks Nginx configuration before reloading it. Certbot stores the
renewal parameters and reloads Nginx after a successful renewal. There is no
custom certificate scheduler or certificate command to maintain. See
[Certbot renewal documentation](https://eff-certbot.readthedocs.io/en/stable/using.html#renewing-certificates).

## Daily commands

Use `sudo` for host operations; `help` and the download-only check also work locally.

| Command | Operation |
| --- | --- |
| `tap config` | Edit the commented host/backend settings |
| `tap setup` | Configure backend, website, Nginx and HTTPS |
| `tap start` / `tap restart` | Apply settings and recreate Rust; retain Redis |
| `tap stop` | Stop Rust, export a Redis backup, stop Redis; retain data |
| `tap update` | Pull and deploy both backend and website |
| `tap update server` | Pull the backend image and recreate Rust |
| `tap update web` | Download the generated static branch and publish it |
| `tap update web --check` | Download and inspect only; no host configuration or TLS |
| `tap rollback web` | Restore the previous local website; repeated calls stay there |
| `tap status` | Tables of running services/settings, credentials and resource usage |
| `tap logs [server\|redis\|nginx]` | Follow logs; default is Rust |
| `tap backup` | Save a Redis RDB and print its path and download command |
| `tap certs` | Count App Attest credential records; show the first 20 key IDs |
| `tap certs show KEY_ID` | Show one record's metadata without the large attestation payload |

`restart` uses the locally available image; `update server` pulls ACR first.
If a replacement backend fails its health check, the script restores the old
container when available. Redis image updates are separate from backend updates;
changing `REDIS_IMAGE` does not replace an existing Redis container.

Website download failures leave the current site in place. Publication switches
one symlink after validating the HTML pages and versioned WASM. Old hashed assets
remain available for open browser tabs. Releases and shared assets are retained;
`tap status` includes disk usage so you can remove unneeded old files manually.
Credential counts use Redis SCAN and may change while registrations are arriving.

## Files and network

- `/etc/tapnap.conf`: administrator-owned settings; no ACR/GitHub tokens.
- `/opt/tap-app-attest/data/redis`: Redis AOF/RDB persistence; reused from the old script.
- `/opt/tap-app-attest/backups/redis`: private RDB backups; download them off the host.
- `/var/www/tapnap`: static releases, `current`/`previous` symlinks and shared assets.
- `/etc/nginx/conf.d/tapnap.conf`: generated static site, API proxy and redirects.
- `/etc/letsencrypt`: Certbot-owned HTTPS certificates and renewal settings.

Nginx proxies `/healthz`, `/app-attest/` and
`/tapcam/capture-signatures/verify` to `127.0.0.1:8080` by default. Redis has no
public port. HTML revalidates, hashed assets cache for a year, and API responses
use `no-store`. CDN setup remains in the cloud console; bypass caching for API
routes and forward ACME HTTP challenges to the origin.

For an existing installation, install the new `tap` at the path used by your
current command (including `/opt/tapnap/tap` if it is a symlink), copy the Apple
settings from the old `.env` into `tap config`, and run `tap restart`. The old
`scripts/server.sh` and Python frontend tool are removed. There is no automatic
configuration import, data deletion or restore command. To restore an RDB, stop
both containers and restore into an empty Redis data directory using the Redis
procedure; an old AOF must not override the restored RDB.
