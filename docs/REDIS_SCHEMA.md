# Redis Schema

Redis is the only v1 datastore. Enable AOF and back up Redis in production
because credentials are durable security state.

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
  "keyId": "apple key id",
  "credentialId": "base64url credential id from authData",
  "publicKeyX962Base64Url": "base64url X9.62 P-256 public key",
  "publicKeySHA256Base64Url": "base64url sha256 public key",
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
