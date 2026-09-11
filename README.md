# TAP App Attest Server

English | [简体中文](README.zh-CN.md)

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

## Usage

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
cargo run --locked
```

The program reads process environment variables; it does not load `.env`
automatically. The example binds to `127.0.0.1:8080`. Test or build the image:

```sh
cargo test --locked
docker build -t tap-app-attest-server:latest .
```

For a Linux host, use the [deployment console](deploy/README.md). It pulls a
prebuilt backend image and website; deployment is a manual host operation.
The host configuration is `/etc/tapnap.conf`, separate from local `.env`.

`GET /healthz` checks service and Redis availability. The implemented App Attest
routes are in [src/routes.rs](src/routes.rs); request fields and response meaning
are defined in the backend contract above. The capture-verification endpoint's
cross-origin policy permits `https://verifier.tapnap.net`.

## How it works

Registration follows `challenge -> Apple attestation verification -> Redis
credential`. The service checks the configured app identity and environment,
consumes challenges atomically, and uses a local Apple root CA. The Docker image
includes that certificate under `/app/certs/`.

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

## Directory structure

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
| [TAPArtifactContracts](https://github.com/TAP-NAP/TAPArtifactContracts) | Normative product, artifact and HTTP requirements; documentation dependency, not executable code. |
| [attestation_assertion_verifier](https://github.com/TAP-NAP/attestation_assertion_verifier) | Rust Git dependency providing `apple_app_attest_attestation`. `Cargo.lock` fixes the resolved revision. |
| [TAPCamDemo](https://github.com/TAP-NAP/TAPCamDemo) | Native producer whose camera app calls the registration and credential-status HTTP endpoints. |
| [TAPCamVerifier](https://github.com/TAP-NAP/TAPCamVerifier) | Browser client of capture verification. Its generated `ecs-web` branch also supplies the static website deployed by `tap`. |

The two clients are not Rust build dependencies. Other Rust packages are
listed in `Cargo.toml`; Redis is a required runtime service. Linux host tools
are listed in the [deployment guide](deploy/README.md). The website is a
deployment input and does not define this service's protocol.
