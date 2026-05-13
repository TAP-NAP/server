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

Required values:

```text
TEAM_ID=YOUR_APPLE_TEAM_ID
BUNDLE_ID=com.example.tapcam
APP_ATTEST_ENV=production
```

For local direct startup, `REDIS_URL` usually points to localhost:

```text
REDIS_URL=redis://127.0.0.1:6379
APPLE_APP_ATTEST_ROOT_CA_PATH=certs/apple_app_attestation_root_ca.pem
```

For Docker Compose, `docker-compose.yml` deliberately uses container-internal
values even if your `.env` has local direct-start values:

```text
REDIS_URL=redis://redis:6379
APPLE_APP_ATTEST_ROOT_CA_PATH=/app/certs/apple_app_attestation_root_ca.pem
```

Inside Docker, `127.0.0.1` means "this same container", so the app container
must connect to Redis through the Compose service name `redis`, not localhost.

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

cargo run
```

The server listens on `SERVER_ADDR`, defaulting to:

```text
0.0.0.0:8080
```

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

## Start With Docker

On macOS, the easiest Homebrew path is Docker Desktop:

```sh
brew install --cask docker
open -a Docker
```

Wait until Docker Desktop finishes starting, then check:

```sh
docker version
docker compose version
```

Installing only `brew install docker` gives you the Docker CLI, but not a
running Docker Engine on macOS. You still need Docker Desktop or another Linux
VM backend such as Colima.

Fill in `.env`, then run:

```sh
docker compose up --build
```

If your Docker installation uses the legacy Compose v1 binary, use the
hyphenated command instead:

```sh
docker-compose up --build
```

This starts:

- `app`: the Rust HTTP server on port `8080`
- `redis`: Redis 7 with AOF persistence enabled

Health check:

```sh
curl http://127.0.0.1:8080/healthz
```

Stop the stack:

```sh
docker compose down
```

Legacy Compose v1:

```sh
docker-compose down
```

Remove Redis data as well:

```sh
docker compose down -v
```

Legacy Compose v1:

```sh
docker-compose down -v
```

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
- `docs/REDIS_SCHEMA.md`: Redis keys, fields, TTLs, and status values
- `docs/ROADMAP.md`: planned assertion and TAP Depth HEIC verification work
