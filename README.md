# TAP App Attest Server

Rust backend for TAPCam App Attest registration and capture-signature verification:

- issue one-hour App Attest challenges;
- verify attestation objects against Apple's App Attest root CA;
- store verified credentials in Redis and report their status;
- verify that a registered key signed a TAPCam capture binding.

## Deploy and operate

The host [tap console](deploy/README.md) manages the Docker backend and Redis,
pulls the GitHub-built website, configures Nginx and sets up Let's Encrypt.
It is one Bash script installed on ECS, independent of any source checkout.

```sh
sudo tap config
sudo tap setup
sudo tap status
sudo tap update
```

ACR builds the backend automatically from GitHub; `tap update server` manually
pulls that image on ECS. `tap update web` pulls the frontend's generated
`ecs-web` branch. No builds run on ECS and GitHub receives no ECS SSH key.

Apple's root CA is copied into the image at
`/app/certs/apple_app_attestation_root_ca.pem`. The Docker environment supplies
its path and Rust reads the file at startup; no host certificate mount is needed.
`APP_ATTEST_ENV=production` must match the App's App Attest environment, and is
unrelated to the website's HTTPS certificate.

Redis persists at `/opt/tap-app-attest/data/redis`; `tap backup` exports RDB
snapshots to `/opt/tap-app-attest/backups/redis`. Starts and updates retain data.

The backend logs safe operation summaries without printing attestation/assertion
objects or raw challenges. Per-request business logs default to off. Change
`REQUEST_LOGS` and `RUST_LOG` in `tap config`, then run `tap restart` and `tap logs`
when diagnosing requests.

## Local development

For a local image build:

```sh
docker build -t tap-app-attest-server:latest .
```

To run Rust directly, start a local Redis and configure the example:

```sh
cp .env.example .env
# Edit TEAM_ID and BUNDLE_ID to match your App.
redis-server --appendonly yes
```

In another terminal:

```sh
set -a
source .env
set +a
cargo run
```

Run backend checks with `cargo test`. Local `.env` is only for direct development;
the installed host console reads `/etc/tapnap.conf`.

## Endpoints

```text
GET  /healthz
POST /app-attest/challenges
POST /app-attest/attestations
POST /app-attest/credentials/status
POST /tapcam/capture-signatures/verify
```

`/app-attest/challenges` accepts `attestation` and `assertion` purposes for
AppAttestKit compatibility. Capture verification signs the `signingBinding`
payload directly and does not use a long-term assertion challenge.

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

Success returns `status: "valid"`, `keyId` and `signingBindingSHA256`. Semantic
verification failures return HTTP 200 with `status: "invalid"` and a reason.
This proves that the registered key signed the binding; it does not upload or
re-hash the original photo bytes.

- [Upstream contracts](docs/REFERENCE_CONTRACTS.md)
- [Redis schema and persistence](docs/REDIS_SCHEMA.md)
- [Backend roadmap](docs/ROADMAP.md)
