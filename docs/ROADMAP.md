# Roadmap

## v1: Attestation Registration

- Issue secure one-time challenges.
- Verify Apple App Attest attestation objects.
- Store `keyId -> publicKeyX962` in Redis.
- Return AppAttestKit-compatible credential status.
- Run as Docker app plus Redis.

## v2: Assertion Verification

- Decode `AppAttestAssertionEnvelope`.
- Rebuild canonical `requestBinding` JSON and pass raw client data to
  `apple_app_attest_attestation::parse_and_verify_assertion`.
- Use the stored `publicKeyX962`.
- Default counter policy for TAP Depth HEIC file authenticity should be
  `positive`, not strict, so older photos can be verified out of order.
- Support both one-time assertion challenges and static long-term challenge
  policies.

## v3: TAP Depth HEIC Full Verification

- Accept original HEIC bytes at `POST /tapcam/captures/verify`.
- Read XMP `tapdepth:Manifest`.
- Decode proof value and assertion envelope.
- Recompute RGB, depth, and metadata digests according to the TAP Depth HEIC
  contract.
- Verify request binding, challenge policy, credential status, and App Attest
  assertion signature.
- Return structured verification levels and TAP error codes.

## v4: Long-Term Audit Storage

Redis keeps v1 small. If the service needs compliance retention, multi-tenant
querying, or long audit history, add Postgres or object storage for credential
history and verification events while keeping Redis for challenge TTL state.
