# Reference Contracts

This server implements the App Attest backend slice required by TAPCamDemo and
AppAttestKit, plus the TAPCam capture signature verification endpoint.

## Primary Contracts

- TAPCamDemo App Attest backend contract:
  https://github.com/TAP-NAP/TAPCamDemo/blob/main/Docs/AppAttest/BackendContract.md
- TAP Depth HEIC server verification contract:
  https://github.com/TAP-NAP/TAPCamDemo/blob/doc_cn/Docs/TAPDepthHEIC/ServerVerificationContract.md
- AppAttestKit HTTP models and backend:
  https://github.com/TAP-NAP/AppAttestKit
- Rust verifier crate:
  https://github.com/TAP-NAP/attestation_assertion_verifier

## App Attest Compatibility Notes

- `POST /app-attest/challenges` accepts both `attestation` and `assertion`
  purposes because AppAttestKit asks the same backend for both flows.
- `POST /app-attest/attestations` verifies attestation objects and stores the
  App Attest public key needed by assertion verification.
- `POST /app-attest/credentials/status` returns a bare JSON enum string because
  AppAttestKit decodes `AppAttestServerCredentialStatus` directly.
- The server stores the original `attestationObject` and checks that request
  `keyId` matches the credential id proven inside the attestation object.

## TAPCam Capture Signature Contract

`POST /tapcam/capture-signatures/verify` verifies that an already registered and
active App Attest `keyId` signed a TAPCam capture binding:

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

The server serializes `signingBinding` as canonical JSON with sorted keys and
unescaped slashes, then uses those bytes as App Attest assertion client data.
The returned `signingBindingSHA256` is the base64url SHA-256 digest of that
canonical JSON.

There is no long-term assertion challenge in this flow. Offline captures can be
uploaded out of order, so assertion counter monotonicity is unchecked and replay
or `captureID` deduplication remains part of the future full HEIC verification
policy.

This endpoint does not verify the original image bytes or recompute
`bodySHA256`; it only proves that the registered App Attest key signed the
submitted binding.

## Client Data Hash Rule

AppAttestKit calls Apple's `attestKey` with:

```text
clientDataHash = SHA256(rawChallenge)
```

The server therefore stores the raw challenge until registration and passes the
same hash bytes to `apple_app_attest_attestation`.
