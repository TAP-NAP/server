# Redis Schema

Redis is the only v1 datastore. In the script-managed deployment, Redis runs in
a container but stores data on the host:

```text
/opt/tap-app-attest/data/redis
```

Backups are Redis RDB snapshots stored under:

```text
/opt/tap-app-attest/backups/redis
```

Credentials are durable security state, so production should keep host-level
backups of this directory or regularly run:

```sh
./scripts/server.sh backup
```

## Challenge Records

Key:

```text
tap:challenge:{challengeId}
```

Value is JSON with Redis TTL. Default TTL is one hour:

```text
CHALLENGE_TTL_SECONDS=3600
```

Fields:

```json
{
  "challengeId": "uuid",
  "rawChallengeBase64Url": "base64url raw 32 bytes",
  "challengeSha256Base64Url": "base64url sha256(raw challenge)",
  "purpose": "attestation",
  "credentialName": "photo_keyid",
  "associatedKeyId": null,
  "associatedCaptureID": null,
  "status": "issued",
  "createdAt": "2026-05-12T00:00:00Z",
  "expiresAt": "2026-05-12T01:00:00Z",
  "usedAt": null
}
```

Statuses:

- `issued`: can be consumed once.
- `used`: consumed by an attestation attempt.
- `failed`: consumed, but registration failed.
- `revoked`: administratively disabled.

`consume_challenge` uses a Redis Lua script so two concurrent requests cannot
reuse the same challenge. Used or failed records remain until their TTL ends so
short-term debugging can still inspect them.

## Credential Records

Key:

```text
tap:credential:{keyId}
```

Value is JSON without TTL:

```json
{
  "credentialName": "photo_keyid",
  "keyId": "base64url credential id from authData",
  "attestationObjectBase64Url": "original request attestationObject",
  "publicKeyX962Base64Url": "base64url X9.62 P-256 public key",
  "credentialPublicKeyCoseBase64Url": "base64url COSE key from authData",
  "receiptBase64Url": "base64url Apple receipt",
  "teamId": "TEAMID1234",
  "bundleId": "com.example.tapcam",
  "environment": "production",
  "lastCounter": null,
  "status": "active",
  "attestationChallengeId": "uuid",
  "createdAt": "2026-05-12T00:00:00Z",
  "updatedAt": "2026-05-12T00:00:00Z"
}
```

Statuses:

- `active`: accepted by `credentials/status`.
- `revoked`: reported to clients as revoked.
- `disabled`: also reported as revoked.

`keyId` is the canonical base64url credential id proven by the attestation
object and is also the Redis key suffix. We intentionally do not store duplicate
`credentialId` or public-key-SHA256 identifier fields; the public verification
material is kept as `publicKeyX962Base64Url` and
`credentialPublicKeyCoseBase64Url`.

## Deployment State Semantics

`./scripts/server.sh stop` stops the containers but does not delete Redis data.
A later `./scripts/server.sh start` loads the existing data directory.

`./scripts/server.sh start --restore-from <backup.rdb>` replaces the managed
Redis data directory with the given RDB file, then starts Redis and the app.

`./scripts/server.sh clean` removes containers and the Docker network only.
`./scripts/server.sh clean --data` also deletes Redis data. `clean --all`
deletes Redis data, backups, and the app image.
