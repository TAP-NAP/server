# TAP App Attest Server

Rust server for the TAPCamDemo App Attest backend contract. This version
implements App Attest registration plus TAPCam capture signature verification:

- issue one-hour App Attest challenges
- verify Apple App Attest attestation objects
- store verified credentials in Redis
- report credential status back to AppAttestKit clients
- verify that a registered App Attest key signed a TAPCam capture binding

The deployment path is intentionally single-lane: build one Docker image, then
manage the app and Redis through `scripts/server.sh`. This repository no longer
uses Docker Compose, so Redis persistence, backup, restore, and cleanup all have
one set of semantics.

## Configuration

Create `.env` from the example:

```sh
cp .env.example .env
```

Fill in your Apple identifiers:

```env
SERVER_ADDR=0.0.0.0:8080
REDIS_URL=redis://tap-app-attest-redis:6379
TEAM_ID=YOUR_APPLE_TEAM_ID
BUNDLE_ID=com.example.tapcam
APP_ATTEST_ENV=production
APPLE_APP_ATTEST_ROOT_CA_PATH=/app/certs/apple_app_attestation_root_ca.pem
CHALLENGE_TTL_SECONDS=3600
RUST_LOG=tap_app_attest_server=info,tower_http=info
```

`SERVER_ADDR` and `REDIS_URL` should keep these Docker values for
`scripts/server.sh`. The script reads and validates `.env`, then passes it into
the app container. It only overrides `APPLE_APP_ATTEST_ROOT_CA_PATH` at container
startup because the certificate is copied into the image at a fixed path.

## Build

Build the image expected by `scripts/server.sh`:

```sh
docker build -t tap-app-attest-server:latest .
```

The image contains the Rust server binary and:

```text
/app/certs/apple_app_attestation_root_ca.pem
```

## Start

Initialize host directories and the Docker network:

```sh
./scripts/server.sh init
```

Start Redis and the app:

```sh
./scripts/server.sh start
```

Run the built-in smoke test:

```sh
./scripts/server.sh self-test
```

`self-test` checks Docker, Redis ping, app health, challenge creation, Redis key
write, and challenge TTL. It deletes the test challenge key before exiting.

## Operations

Check status:

```sh
./scripts/server.sh status
```

Follow logs:

```sh
./scripts/server.sh logs
./scripts/server.sh logs --redis
```

Stop the service:

```sh
./scripts/server.sh stop
```

`stop` backs up Redis by default, then stops Redis. To skip that backup:

```sh
./scripts/server.sh stop --no-backup
```

Create a Redis backup while Redis is running:

```sh
./scripts/server.sh backup
```

Start from an existing backup:

```sh
./scripts/server.sh start --restore-from /opt/tap-app-attest/backups/redis/redis-20260514-120000.rdb
```

This replaces the managed Redis data directory with the given RDB file, then
starts Redis and the app.

Clean containers and the Docker network, keeping Redis data and backups:

```sh
./scripts/server.sh clean
```

Start from an empty Redis state:

```sh
./scripts/server.sh clean --data
./scripts/server.sh start
```

Remove containers, network, app image, Redis data, and Redis backups:

```sh
./scripts/server.sh clean --all
```

## Data Semantics

Redis data lives on the host:

```text
/opt/tap-app-attest/data/redis
```

Redis backups live on the host:

```text
/opt/tap-app-attest/backups/redis
```

`stop` does not delete Redis data. A normal later `start` loads the same Redis
data directory, so existing challenge and credential records are preserved.
Only `clean --data` and `clean --all` delete Redis data.

## Public HTTPS

The script binds the app to the host loopback address:

```text
127.0.0.1:8080
```

Put Caddy, Nginx, or another HTTPS reverse proxy in front of it for your public
domain. Do not expose Redis publicly.

## Direct Local Run

For local Rust development without Docker, start Redis yourself and override the
Docker-facing `.env` values:

```sh
redis-server --appendonly yes

set -a
source .env
set +a

export SERVER_ADDR=127.0.0.1:8080
export REDIS_URL=redis://127.0.0.1:6379
export APPLE_APP_ATTEST_ROOT_CA_PATH=certs/apple_app_attestation_root_ca.pem

cargo run
```

## Endpoints

```text
GET  /healthz
POST /app-attest/challenges
POST /app-attest/attestations
POST /app-attest/credentials/status
POST /tapcam/capture-signatures/verify
```

`/app-attest/challenges` accepts both `attestation` and `assertion` purposes so
the AppAttestKit challenge contract stays compatible. Capture signature
verification does not use a long-term assertion challenge; it verifies the
signature over the submitted `signingBinding` payload.

Verify a TAPCam capture signature:

```sh
curl -sS -X POST http://127.0.0.1:8080/tapcam/capture-signatures/verify \
  -H 'content-type: application/json' \
  -d '{
    "keyId": "...",
    "assertionObject": "...",
    "signingBinding": {
      "schemaID": "urn:tapnap:tapcam:app-attest-capture-signing:v1",
      "operation": "tapcam.capture.sign",
      "captureID": "...",
      "bodySHA256": "..."
    }
  }'
```

Success returns:

```json
{
  "status": "valid",
  "keyId": "...",
  "signingBindingSHA256": "..."
}
```

Semantic verification failures return HTTP 200 with `status: "invalid"` and a
reason. This endpoint proves that the registered App Attest key signed the
`signingBinding`; it does not upload or re-hash the original photo bytes.

## More Docs

- `docs/REFERENCE_CONTRACTS.md`: upstream contracts this server follows
- `docs/REDIS_SCHEMA.md`: Redis keys, fields, TTLs, and persistence behavior
- `docs/ROADMAP.md`: shipped capture signature verification and planned TAP
  Depth HEIC verification work
