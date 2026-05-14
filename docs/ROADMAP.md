# Roadmap

This file tracks shipped behavior, planned work, and canceled or replaced
design goals. It should reflect the current `main` branch, not aspirational
implementation status.

## v1: Attestation Registration

- [x] `GET /healthz`.
- [x] `POST /app-attest/challenges`.
- [x] `POST /app-attest/attestations`.
- [x] `POST /app-attest/credentials/status`.
- [x] Issue secure one-time App Attest challenges.
- [x] Verify Apple App Attest attestation objects.
- [x] Check request `keyId` against the credential id proven by the attestation
  object.
- [x] Store attestation challenge and credential records in Redis.
- [x] Store original `attestationObject` with the credential record.
- [x] Store `keyId -> publicKeyX962Base64Url` in Redis.
- [x] Return AppAttestKit-compatible credential status.
- [x] Run as Docker app plus Redis using `scripts/server.sh`.

## v2: [NEW] TAPCam Capture Signature Verification

- [x] Add `POST /tapcam/capture-signatures/verify`.
- [x] Accept request body:

  ```json
  {
    "keyId": "...",
    "assertionObject": "...",
    "signingBinding": {
      "schemaID": "urn:tapnap:tapcam:app-attest-capture-signing:v1",
      "operation": "tapcam.capture.sign",
      "captureID": "...",
      "bodySHA256": "..."
    }
  }
  ```

- [x] Return valid response:

  ```json
  {
    "status": "valid",
    "keyId": "...",
    "signingBindingSHA256": "..."
  }
  ```

- [x] Return invalid verification results with HTTP 200, `status: "invalid"`,
  and a machine-readable reason.
- [x] Canonicalize `keyId` and check `tap:credential:{keyId}` exists.
- [x] Require credential `status=active`.
- [x] Validate `signingBinding.schemaID`.
- [x] Validate `signingBinding.operation == "tapcam.capture.sign"`.
- [x] Validate non-empty `captureID` and `bodySHA256`.
- [x] Serialize `signingBinding` as canonical JSON matching TAPCamDemo rules:
  sorted keys and unescaped slashes.
- [x] Compute `signingBindingSHA256`.
- [x] Verify App Attest assertion signature using stored
  `publicKeyX962Base64Url`.
- [x] Use `AssertionCounterPolicy::Unchecked`; offline captures may upload out
  of order, so counter monotonicity must not decide validity.

Canceled or replaced goals:

- [ ] ~~Support static long-term assertion challenge~~ — canceled because a
  long-lived challenge does not prove freshness in offline capture signing and
  should not be treated as replay protection.
- [ ] ~~Support one-time assertion challenge for capture signing~~ — canceled
  for the current offline workflow; a future online request-signing flow can
  define a separate challenge policy if needed.
- [ ] ~~Decode full AppAttestAssertionEnvelope as the public API~~ — replaced by
  an explicit `keyId`, `assertionObject`, `signingBinding` contract.
- [ ] ~~Name this endpoint docsign~~ — replaced because the endpoint is
  TAPCam-specific, not a generic document signing API.

## v3: TAP Depth HEIC Full Verification

- [ ] Accept original HEIC bytes at `POST /tapcam/captures/verify`.
- [ ] Read XMP `tapdepth:Manifest`.
- [ ] Decode proof value and assertion envelope.
- [ ] Recompute RGB, depth, and metadata digests according to the TAP Depth HEIC
  contract.
- [ ] Compare recomputed digest package with signed `bodySHA256`.
- [ ] Call TAPCam capture signature verification.
- [ ] Add replay protection or captureID deduplication policy.
- [ ] Return structured verification levels and TAP error codes.

## v4: Long-Term Audit Storage

- [ ] Keep Redis for challenge TTL state.
- [ ] Add Postgres or object storage if the service needs compliance retention,
  multi-tenant querying, or long audit history.
- [ ] Store credential history and verification events outside Redis if long-term
  audit becomes a product requirement.
