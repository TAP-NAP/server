# TAPNap host console

## Purpose

The `tap` Bash script operates the TAP App Attest backend, Redis, and the
TAPCamVerifier website on a Linux host. It calls Docker, Git, Nginx, Certbot
and systemd directly and can be installed without a source checkout.

```text
backend image + TAPCamVerifier ecs-web static branch
  -> manual tap update on the host
  -> Docker backend + persistent Redis + host Nginx
```

Backend images are built before deployment, using the configured registry's
build pipeline. The website is built by TAPCamVerifier's GitHub workflow.
The console pulls both artifacts; it performs no builds and requires no
GitHub-to-host SSH access.

## Usage and build

Install Bash, Docker, Git, curl, tar, Nginx, Certbot and its packaged systemd
renewal timer using the host distribution's packages. `flock`, `free` and the
other commands are ordinary Linux utilities. Configure any Docker mirror in
the Docker daemon. Private registry images require `sudo docker login REGISTRY`;
the script defaults to the public Hangzhou ACR `v1` image.

Install the console:

```sh
curl -fL https://raw.githubusercontent.com/TAP-NAP/server/main/deploy/tap -o /tmp/tap
sudo install -m 0755 /tmp/tap /usr/local/bin/tap
sudo tap config
sudo tap setup
```

`config` opens `/etc/tapnap.conf` in `${EDITOR:-vi}`. Check the Apple team and
bundle identifiers, set `APP_ATTEST_ENV` to match the signing app, and provide
the Let's Encrypt account email. Backend changes take effect with
`tap restart`; domain and host-port changes need `tap setup` to regenerate
Nginx configuration. The Apple App Attest environment is independent of the
website's HTTPS certificate.

Before `setup`, point the configured domains (default `www.tapnap.net` and
`tapnap.net`) to the host and allow incoming ports 80/443. The frontend's
`publish-ecs` workflow must have created `ecs-web`. Remove conflicting Nginx
server blocks for the configured domains.

`setup` pulls the backend image, starts Redis and Rust, downloads the website,
writes Nginx configuration, obtains HTTPS certificates with Certbot webroot,
and enables the installed renewal timer. Rerunning it retains Redis data and
reuses unexpired certificates. Nginx configuration is checked before reload;
Certbot checks and reloads Nginx after a successful renewal.

Use `sudo` for host operations. `help` and the download-only check also work
locally without root.

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
| `tap status` | Show running services/settings, credential counts and resource usage |
| `tap logs [server\|redis\|nginx]` | Follow logs; default is Rust |
| `tap backup` | Save a Redis RDB and print its path and download command |
| `tap certs` | Count App Attest credential records; show the first 20 key IDs |
| `tap certs show KEY_ID` | Show one record's metadata without the large attestation payload |

For request diagnostics, edit `REQUEST_LOGS` and `RUST_LOG` with `tap config`,
then use `tap restart` and `tap logs`. Per-request business logs default to off.

To replace an existing console, install the script at the path used by the
current command, including `/opt/tapnap/tap` if the command is a symlink to it.
Copy any existing Apple settings into `tap config`, then run `tap restart`.
There is no automatic configuration import.

## Principles

Deployment remains manual. `restart` uses a local image if available, pulling
only if absent; `update server` pulls first. If a replacement backend fails
its health check, the script restores the old container when available.
Backend updates retain Redis. Changing `REDIS_IMAGE` does not replace an
existing Redis container.

`setup`, `start`, `restart` and backend updates enable and start the Docker
system service before using it. `setup` also enables Nginx and the Certbot
renewal timer. Docker starts the backend and Redis after a host reboot using
`unless-stopped`; reused Redis containers receive this policy too. Boot uses
the installed images and website, without downloading or running `tap setup`.
An explicit `tap stop` keeps the containers stopped across reboots until
`tap start`. `tap status` shows the Docker/Nginx boot settings and container
restart policies.

Website downloads are validated before publication, including required HTML,
source revision and versioned WASM. Publication switches one symlink. Failed
downloads retain the current site, and old hashed assets remain available for
open browser tabs. Releases and shared assets remain on disk until the host
administrator removes unneeded files.

Redis uses AOF persistence. `tap backup` exports an RDB snapshot while Redis
runs; copy backups off the host. `tap stop` backs up running Redis before
stopping it. The console has no restore or data-deletion command. To restore
an RDB, stop both containers and follow Redis's restore procedure using an
empty data directory; existing AOF files take precedence over the RDB. The
[Redis schema](../docs/REDIS_SCHEMA.md) describes the stored security state.

The Apple App Attest root CA is bundled in the backend image and read by Rust.
Host Nginx owns the independent Let's Encrypt certificates, with renewal
managed by Certbot's installed timer.

## Directory map

The source console is [`deploy/tap`](tap). Its host paths are:

| Host path | Role |
| --- | --- |
| `/etc/tapnap.conf` | Administrator-owned settings; no ACR/GitHub tokens |
| `/opt/tap-app-attest/data/redis` | Persistent Redis AOF/RDB data |
| `/opt/tap-app-attest/backups/redis` | Private RDB backups |
| `/var/www/tapnap` | Static releases, `current`/`previous` symlinks and shared assets |
| `/etc/nginx/conf.d/tapnap.conf` | Generated website, API proxy and redirects |
| `/etc/letsencrypt` | Certbot-owned HTTPS certificates and renewal settings |

Nginx proxies `/healthz`, `/app-attest/` and
`/tapcam/capture-signatures/verify` to host loopback port 8080 by default.
Redis has no public port. HTML revalidates, hashed assets cache for a year,
and API responses use `no-store`. Configure any CDN in the cloud console:
bypass API caching and forward ACME HTTP challenges to the origin.

## Repository dependencies

- [server](https://github.com/TAP-NAP/server) supplies this console and the
  backend Dockerfile. `APP_IMAGE` selects the prebuilt image to deploy.
- [TAPCamVerifier](https://github.com/TAP-NAP/TAPCamVerifier) supplies the
  generated `ecs-web` static branch. Git is the artifact transport; the host
  does not build or execute repository build scripts.
- [TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts) is the
  normative source for product, artifact and backend requirements. It has no
  runtime or deployment dependency.

See the [service README](../README.md) for Rust and client-repository
dependencies, and [Certbot's renewal documentation](https://eff-certbot.readthedocs.io/en/stable/using.html#renewing-certificates)
for certificate operations.
