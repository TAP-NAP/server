# Reference Contracts

This server implements the first App Attest backend slice required by
TAPCamDemo and AppAttestKit.

## Primary Contracts

- TAPCamDemo App Attest backend contract:
  https://github.com/TAP-NAP/TAPCamDemo/blob/main/Docs/AppAttest/BackendContract.md
- TAP Depth HEIC server verification contract:
  https://github.com/TAP-NAP/TAPCamDemo/blob/doc_cn/Docs/TAPDepthHEIC/ServerVerificationContract.md
- AppAttestKit HTTP models and backend:
  https://github.com/TAP-NAP/AppAttestKit
- Rust verifier crate:
  https://github.com/TAP-NAP/attestation_assertion_verifier

## v1 Compatibility Notes

- `POST /app-attest/challenges` accepts both `attestation` and `assertion`
  purposes because AppAttestKit asks the same backend for both flows.
- `POST /app-attest/attestations` verifies only attestation objects and stores
  the App Attest public key needed by future assertion verification.
- `POST /app-attest/credentials/status` returns a bare JSON enum string because
  AppAttestKit decodes `AppAttestServerCredentialStatus` directly.
- Assertion verification and TAP Depth HEIC full file verification are roadmap
  items, not v1 trust decisions.

## Client Data Hash Rule

AppAttestKit calls Apple's `attestKey` with:

```text
clientDataHash = SHA256(rawChallenge)
```

The server therefore stores the raw challenge until registration and passes the
same hash bytes to `apple_app_attest_attestation`.
