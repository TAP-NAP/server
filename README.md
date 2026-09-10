# TAP App Attest Server

## Purpose

Rust service for TAPCam App Attest credential registration and capture-signature
verification. It issues challenges, verifies Apple attestations, stores
credentials in Redis, and verifies that an active registered key signed a
capture's `signingBinding`.

[TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts) is the
normative source for product, artifact, and backend requirements. This
repository documents the service implementation and operation; begin with the
[backend contract](https://github.com/TAP-NAP/TAPArtifactContracts/blob/main/BackendContract.md)
and the shared
[backend App Attest gate](https://github.com/TAP-NAP/TAPArtifactContracts/blob/main/bindings/capture-binding-and-proof-v1.md#backend-app-attest-gate).

## Usage and build

For direct development, install Rust and Redis, then configure the example:

```sh
cp .env.example .env
# Edit TEAM_ID, BUNDLE_ID and APP_ATTEST_ENV to match the signing app.
redis-server --appendonly yes
```

In another terminal at the repository root:

```sh
set -a
source .env
set +a
cargo run
```

The example binds to `127.0.0.1:8080`. Run unit tests with `cargo test`, or build
the container with `docker build -t tap-app-attest-server:latest .`.

For a Linux host, use the [deployment console](deploy/README.md). It pulls a
prebuilt backend image and website; deployment is a manual host operation.
The host configuration is `/etc/tapnap.conf`, separate from local `.env`.

| Endpoint | Current behavior |
| --- | --- |
| `GET /healthz` | Check service and Redis availability |
| `POST /app-attest/challenges` | Issue a challenge for `attestation` or `assertion` |
| `POST /app-attest/attestations` | Verify and register an App Attest credential |
| `POST /app-attest/credentials/status` | Return the JSON string `"accepted"`, `"revoked"`, or `"unknown"` |
| `POST /tapcam/capture-signatures/verify` | Verify a registered key's capture signature |

The capture endpoint accepts `keyId`, `assertionObject`, and the four-field
`signingBinding` defined by the
[binding contract](https://github.com/TAP-NAP/TAPArtifactContracts/blob/main/bindings/capture-binding-and-proof-v1.md#signingbinding-and-app-attest-input).
Success returns `status: "valid"`, canonical `keyId`, and
`signingBindingSHA256`. Semantic verification failures return HTTP 200 with
`status: "invalid"` and a reason; malformed input and service errors use error
HTTP statuses. Browser cross-origin access to this endpoint permits
`https://verifier.tapnap.net`.

## Principles

Registration follows `challenge -> Apple attestation verification -> Redis
credential`. Challenges contain 32 random bytes, expire after one hour by
default, and are consumed atomically. Registration verifies the App ID,
environment, and key identity using `SHA256(rawChallenge)` as client data hash.
The Apple root CA is read from the configured local file; the Docker image
includes its own copy under `/app/certs/`.

Capture verification follows `signingBinding -> canonical JSON -> registered
public key + App Attest assertion -> valid/invalid`. The caller first verifies
the local artifact binding and submits only the shared signature request. This
service does not receive original media or recompute its content digest; its
result must be combined with the caller's local result.

The current implementation uses the configured App ID for assertion checks and
requires an active stored credential. Capture verification uses
`AssertionCounterPolicy::Unchecked` to allow out-of-order offline submissions;
it has no freshness challenge, counter update, or capture-ID deduplication.
Accepting the `assertion` challenge purpose does not add a general protected
business-request endpoint. Account authorization and long-term event auditing
are also outside the implemented API.

Redis stores credentials without a TTL. See the
[Redis schema](docs/REDIS_SCHEMA.md) for record fields and backup semantics.
Per-request business logs default to off; set `REQUEST_LOGS` and `RUST_LOG` only
as needed. Logs omit raw challenges and attestation/assertion payloads.

## Directory map

| Path | Role |
| --- | --- |
| `src/main.rs`, `src/config.rs` | Startup, environment configuration and shutdown |
| `src/routes.rs`, `src/models.rs`, `src/error.rs` | HTTP endpoints, JSON models and error responses |
| `src/challenge.rs`, `src/verifier.rs` | Challenge primitives and App Attest verification calls |
| `src/store/` | Redis records and atomic challenge consumption |
| `certs/` | Apple App Attest root CA bundled in the image |
| `deploy/` | Host console and operating instructions |
| `docs/REDIS_SCHEMA.md` | Persistent data format and lifecycle |
| `Cargo.toml`, `Cargo.lock`, `Dockerfile`, `.env.example` | Build dependencies, image and local configuration |

Unit tests live beside the Rust implementation in `src/`.

## Repository dependencies

| Repository | Relationship |
| --- | --- |
| [TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts) | Normative product, artifact and HTTP contracts; documentation dependency |
| [attestation_assertion_verifier](https://github.com/TAP-NAP/attestation_assertion_verifier) | Rust Git dependency providing `apple_app_attest_attestation`, the reusable App Attest cryptographic verifier |
| [AppAttestKit](https://github.com/TAP-NAP/AppAttestKit) | Swift HTTP client whose registration/status models this API supports; not a Rust build dependency |
| [TAPCamDemo](https://github.com/TAP-NAP/TAPCamDemo) | Native producer and registration client through AppAttestKit |
| [TAPCamVerifier](https://github.com/TAP-NAP/TAPCamVerifier) | Browser capture-verification client; its generated `ecs-web` branch also supplies the static website deployed by `tap` |

Other Rust dependencies are declared in `Cargo.toml`; Redis is a runtime
service. Host Docker, Nginx and Certbot requirements are in the deployment
instructions. The static website is a deployment input, not a source of
normative contract definitions.
