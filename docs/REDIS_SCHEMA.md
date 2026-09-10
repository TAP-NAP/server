# Redis Schema

Redis is the datastore for App Attest challenge and credential state. In the
script-managed deployment, Redis runs in a container but stores data on the
host:

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
sudo tap backup
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

`POST /tapcam/capture-signatures/verify` does not create challenge records. The
capture signature flow signs the submitted `signingBinding` directly and does
not use a long-term assertion challenge.

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

`tap stop` stops the backend, saves an RDB backup, then stops Redis. It retains
Redis data. `tap start`, `tap restart` and `tap update server` reuse the same
persistent directory; backend updates leave Redis running.

`tap backup` exports an RDB snapshot while Redis is running. Copy backups off
the host. To restore a snapshot, stop both containers first and follow Redis's
RDB restore procedure with an empty data directory; existing AOF files take
precedence over an RDB. The console has no automatic restore or data-deletion
command. See the [host console instructions](../deploy/README.md).
