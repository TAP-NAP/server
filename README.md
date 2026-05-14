# TAP App Attest Server

Rust server for the TAPCamDemo App Attest backend contract. The first version
implements attestation registration only:

- issue App Attest challenges
- verify Apple App Attest attestation objects
- store verified credentials in Redis
- report credential status back to AppAttestKit clients

Challenge records default to a one-hour window:

```text
CHALLENGE_TTL_SECONDS=3600
```

## Configuration

Copy the example environment file and fill in your Apple app identifiers:

```sh
cp .env.example .env
```

`.env.example` is written for the `scripts/server.sh` Docker deployment. Fill in
your Apple identifiers and keep these Docker-facing values:

```text
SERVER_ADDR=0.0.0.0:8080
REDIS_URL=redis://tap-app-attest-redis:6379
TEAM_ID=YOUR_APPLE_TEAM_ID
BUNDLE_ID=com.example.tapcam
APP_ATTEST_ENV=production
APPLE_APP_ATTEST_ROOT_CA_PATH=/app/certs/apple_app_attestation_root_ca.pem
```

`scripts/server.sh` reads and validates `.env`, then passes the same file into
the app container. It does not keep a second set of app environment values.

## Start Directly

Start Redis first:

```sh
redis-server --appendonly yes
```

In another terminal, export env vars and run the server:

```sh
set -a
source .env
set +a

export SERVER_ADDR=127.0.0.1:8080
export REDIS_URL=redis://127.0.0.1:6379
export APPLE_APP_ATTEST_ROOT_CA_PATH=certs/apple_app_attestation_root_ca.pem

cargo run
```

Those three overrides are only for direct local `cargo run`. The checked-in
`.env.example` is intentionally shaped for Docker deployment.

Health check:

```sh
curl http://127.0.0.1:8080/healthz
```

Issue an attestation challenge:

```sh
curl -sS -X POST http://127.0.0.1:8080/app-attest/challenges \
  -H 'Content-Type: application/json' \
  -d '{"purpose":"attestation","credentialName":"photo_keyid"}'
```

## Local Docker Compose

For local development or a quick smoke test:

```sh
docker compose up --build
```

This starts:

- `app`: the Rust HTTP server on port `8080`
- `redis`: Redis 7 with AOF persistence enabled

Stop it with:

```sh
docker compose down
```

Remove the Compose-managed Redis volume only when you intentionally want to
delete local Compose data:

```sh
docker compose down -v
```

## Server Script Deployment

For a server where you already have this repository and want Redis data stored
outside containers, build the app image first:

```sh
docker build -t tap-app-attest-server:latest .
```

Then use the single deployment entrypoint:

```sh
./scripts/server.sh help
./scripts/server.sh init
./scripts/server.sh start
```

The script starts:

- app container: `tap-app-attest-server`
- Redis container: `tap-app-attest-redis`
- Docker network: `tap-app-attest-net`

Redis data is stored on the host:

```text
/opt/tap-app-attest/data/redis
```

Redis backups are stored on the host:

```text
/opt/tap-app-attest/backups/redis
```

### Data Semantics

`stop` does not delete Redis data:

```sh
./scripts/server.sh stop
```

A later normal start loads the existing Redis data directory, so challenge and
credential records are preserved:

```sh
./scripts/server.sh start
```

To create a Redis backup while Redis is running:

```sh
./scripts/server.sh backup
```

To start from a backup, the script replaces the managed Redis data directory
with the given RDB file and then starts Redis and the app:

```sh
./scripts/server.sh start --restore-from /opt/tap-app-attest/backups/redis/redis-20260514-120000.rdb
```

To remove containers and the Docker network but keep Redis data:

```sh
./scripts/server.sh clean
```

To start from an empty Redis state:

```sh
./scripts/server.sh clean --data
./scripts/server.sh start
```

To delete containers, network, image, Redis data, and backups:

```sh
./scripts/server.sh clean --all
```

Status and logs:

```sh
./scripts/server.sh status
./scripts/server.sh logs
./scripts/server.sh logs --redis
```

By default the script binds the app to `127.0.0.1:8080` on the host. Put Caddy,
Nginx, or another HTTPS reverse proxy in front of it for a public domain.

## Implemented Endpoints

```text
GET  /healthz
POST /app-attest/challenges
POST /app-attest/attestations
POST /app-attest/credentials/status
```

`/app-attest/challenges` accepts both `attestation` and `assertion` purposes so
the API shape is ready for TAPCamDemo's later assertion flow. This v1 server
only verifies attestation objects.

## More Docs

- `docs/REFERENCE_CONTRACTS.md`: upstream contracts this server follows
- `docs/REDIS_SCHEMA.md`: Redis keys, fields, TTLs, and persistence behavior
- `docs/ROADMAP.md`: planned assertion and TAP Depth HEIC verification work
