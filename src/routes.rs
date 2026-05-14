use crate::{
    challenge::{
        canonicalize_key_id, decode_base64_url, is_expired, issue_challenge, sha256_base64_url,
    },
    error::AppError,
    models::{
        AppAttestPurpose, AttestationRequest, AttestationResponse, CaptureSignatureInvalidReason,
        CaptureSignatureStatus, CaptureSignatureVerifyRequest, CaptureSignatureVerifyResponse,
        CaptureSigningBinding, ChallengeRequest, ChallengeResponse, CredentialStatus,
        CredentialStatusRequest, HealthResponse, RegistrationStatus, ServerCredentialStatus,
    },
    AppState,
};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::{Timelike, Utc};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/app-attest/challenges", post(create_challenge))
        .route("/app-attest/attestations", post(register_attestation))
        .route(
            "/tapcam/capture-signatures/verify",
            post(verify_capture_signature),
        )
        .route("/app-attest/credentials/status", post(credential_status))
        .with_state(state)
}

async fn healthz(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    state.store.ping().await?;
    Ok(Json(HealthResponse {
        status: "ok",
        redis: "ok",
    }))
}

async fn create_challenge(
    State(state): State<AppState>,
    Json(request): Json<ChallengeRequest>,
) -> Result<Json<ChallengeResponse>, AppError> {
    let credential_name = require_non_empty("credentialName", request.credential_name)?;
    let issued = issue_challenge(request.purpose, credential_name, state.config.challenge_ttl);
    state
        .store
        .put_challenge(&issued.record, state.config.challenge_ttl)
        .await?;

    Ok(Json(ChallengeResponse {
        challenge_id: issued.record.challenge_id,
        challenge: issued.record.raw_challenge_base64_url,
        expires_at: issued.record.expires_at,
    }))
}

async fn register_attestation(
    State(state): State<AppState>,
    Json(mut request): Json<AttestationRequest>,
) -> Result<Json<AttestationResponse>, AppError> {
    request.credential_name = require_non_empty("credentialName", request.credential_name)?;
    request.key_id = require_non_empty("keyId", request.key_id)?;
    request.challenge_id = require_non_empty("challengeId", request.challenge_id)?;
    request.attestation_object =
        require_non_empty("attestationObject", request.attestation_object)?;

    let consumed = match state
        .store
        .consume_challenge(&request.challenge_id, &request.key_id, utc_now_seconds())
        .await
    {
        Ok(record) => record,
        Err(AppError::ChallengeInvalid(_)) => {
            return idempotent_attestation_response(&state, &request).await;
        }
        Err(error) => return Err(error),
    };

    let validation = validate_attestation_challenge(&consumed, &request);
    if let Err(error) = validation {
        state
            .store
            .mark_challenge_failed(&request.challenge_id, &request.key_id)
            .await?;
        return Err(error);
    }

    let raw_challenge = match decode_base64_url(&consumed.raw_challenge_base64_url) {
        Ok(raw) => raw,
        Err(err) => {
            state
                .store
                .mark_challenge_failed(&request.challenge_id, &request.key_id)
                .await?;
            return Err(AppError::Internal(format!(
                "stored raw challenge is invalid base64url: {err}"
            )));
        }
    };

    let credential = match state.verifier.verify_registration(&request, &raw_challenge) {
        Ok(credential) => credential,
        Err(error) => {
            state
                .store
                .mark_challenge_failed(&request.challenge_id, &request.key_id)
                .await?;
            return Err(error);
        }
    };

    let credential_id = credential.key_id.clone();
    state.store.save_credential(&credential).await?;

    Ok(Json(AttestationResponse {
        credential_id: Some(credential_id),
        status: RegistrationStatus::Accepted,
    }))
}

async fn verify_capture_signature(
    State(state): State<AppState>,
    Json(mut request): Json<CaptureSignatureVerifyRequest>,
) -> Result<Json<CaptureSignatureVerifyResponse>, AppError> {
    request.key_id = require_non_empty("keyId", request.key_id)?;
    request.assertion_object = require_non_empty("assertionObject", request.assertion_object)?;

    let canonical_key_id = require_canonical_key_id(&request.key_id)?;
    request.key_id = canonical_key_id.clone();

    let signing_binding_json = canonical_signing_binding_json(&request.signing_binding)?;
    let signing_binding_sha256 = sha256_base64_url(&signing_binding_json);

    let Some(credential) = state.store.get_credential(&canonical_key_id).await? else {
        return Ok(Json(capture_signature_invalid(
            canonical_key_id,
            signing_binding_sha256,
            CaptureSignatureInvalidReason::KeyNotRegistered,
        )));
    };

    if credential.status != CredentialStatus::Active {
        return Ok(Json(capture_signature_invalid(
            canonical_key_id,
            signing_binding_sha256,
            CaptureSignatureInvalidReason::CredentialNotActive,
        )));
    }

    if let Some(reason) = validate_capture_signing_binding(&request.signing_binding) {
        return Ok(Json(capture_signature_invalid(
            canonical_key_id,
            signing_binding_sha256,
            reason,
        )));
    }

    let valid = state.verifier.verify_capture_signature(
        &request.assertion_object,
        &credential,
        signing_binding_json,
    )?;

    if !valid {
        return Ok(Json(capture_signature_invalid(
            canonical_key_id,
            signing_binding_sha256,
            CaptureSignatureInvalidReason::SignatureInvalid,
        )));
    }

    Ok(Json(CaptureSignatureVerifyResponse {
        status: CaptureSignatureStatus::Valid,
        key_id: canonical_key_id,
        signing_binding_sha256,
        reason: None,
    }))
}

async fn credential_status(
    State(state): State<AppState>,
    Json(request): Json<CredentialStatusRequest>,
) -> Result<Json<ServerCredentialStatus>, AppError> {
    let credential_name = require_non_empty("credentialName", request.credential_name)?;
    let Some(key_id) = request.key_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(Json(ServerCredentialStatus::Unknown));
    };
    let Some(canonical_key_id) = canonicalize_key_id(&key_id) else {
        return Ok(Json(ServerCredentialStatus::Unknown));
    };

    let status = match state.store.get_credential(&canonical_key_id).await? {
        Some(record) if record.credential_name == credential_name => match record.status {
            CredentialStatus::Active => ServerCredentialStatus::Accepted,
            CredentialStatus::Revoked | CredentialStatus::Disabled => {
                ServerCredentialStatus::Revoked
            }
        },
        _ => ServerCredentialStatus::Unknown,
    };

    Ok(Json(status))
}

async fn idempotent_attestation_response(
    state: &AppState,
    request: &AttestationRequest,
) -> Result<Json<AttestationResponse>, AppError> {
    let Some(canonical_key_id) = canonicalize_key_id(&request.key_id) else {
        return Err(AppError::KeyIdMismatch {
            message: "request keyId is not valid base64 or base64url".to_string(),
        });
    };
    let Some(credential) = state.store.get_credential(&canonical_key_id).await? else {
        return Err(AppError::ChallengeInvalid(
            "challenge has already been used".to_string(),
        ));
    };

    if credential.is_active_for(&request.credential_name)
        && credential.attestation_challenge_id == request.challenge_id
    {
        return Ok(Json(AttestationResponse {
            credential_id: Some(credential.key_id),
            status: RegistrationStatus::Accepted,
        }));
    }

    Err(AppError::ChallengeInvalid(
        "challenge has already been used by another registration".to_string(),
    ))
}

fn validate_attestation_challenge(
    challenge: &crate::models::ChallengeRecord,
    request: &AttestationRequest,
) -> Result<(), AppError> {
    if challenge.purpose != AppAttestPurpose::Attestation {
        return Err(AppError::ChallengeInvalid(format!(
            "expected attestation challenge, got {:?}",
            challenge.purpose
        )));
    }
    if challenge.credential_name != request.credential_name {
        return Err(AppError::ChallengeInvalid(
            "challenge credentialName does not match request".to_string(),
        ));
    }
    if is_expired(challenge.expires_at) {
        return Err(AppError::ChallengeInvalid(
            "challenge has expired".to_string(),
        ));
    }
    Ok(())
}

fn validate_capture_signing_binding(
    binding: &CaptureSigningBinding,
) -> Option<CaptureSignatureInvalidReason> {
    if binding.schema_id != "urn:tapnap:tapcam:app-attest-capture-signing:v1" {
        return Some(CaptureSignatureInvalidReason::SchemaInvalid);
    }
    if binding.operation != "tapcam.capture.sign" {
        return Some(CaptureSignatureInvalidReason::OperationInvalid);
    }
    if binding.capture_id.trim().is_empty() || binding.body_sha256.trim().is_empty() {
        return Some(CaptureSignatureInvalidReason::BindingInvalid);
    }
    None
}

fn canonical_signing_binding_json(binding: &CaptureSigningBinding) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec(binding).map_err(AppError::from)
}

fn capture_signature_invalid(
    key_id: String,
    signing_binding_sha256: String,
    reason: CaptureSignatureInvalidReason,
) -> CaptureSignatureVerifyResponse {
    CaptureSignatureVerifyResponse {
        status: CaptureSignatureStatus::Invalid,
        key_id,
        signing_binding_sha256,
        reason: Some(reason),
    }
}

fn require_canonical_key_id(value: &str) -> Result<String, AppError> {
    canonicalize_key_id(value).ok_or_else(|| AppError::KeyIdMismatch {
        message: "request keyId is not valid base64 or base64url".to_string(),
    })
}

fn require_non_empty(field: &'static str, value: String) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput(format!("{field} cannot be empty")));
    }
    Ok(trimmed.to_string())
}

fn utc_now_seconds() -> chrono::DateTime<Utc> {
    Utc::now()
        .with_nanosecond(0)
        .expect("zero nanosecond is a valid timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_binding() -> CaptureSigningBinding {
        CaptureSigningBinding {
            body_sha256: "body-hash".to_string(),
            capture_id: "capture/1".to_string(),
            operation: "tapcam.capture.sign".to_string(),
            schema_id: "urn:tapnap:tapcam:app-attest-capture-signing:v1".to_string(),
        }
    }

    #[test]
    fn capture_signing_binding_canonical_json_matches_sorted_key_order() {
        let canonical =
            String::from_utf8(canonical_signing_binding_json(&sample_binding()).unwrap()).unwrap();

        assert_eq!(
            canonical,
            r#"{"bodySHA256":"body-hash","captureID":"capture/1","operation":"tapcam.capture.sign","schemaID":"urn:tapnap:tapcam:app-attest-capture-signing:v1"}"#
        );
    }

    #[test]
    fn signing_binding_sha256_hashes_canonical_json() {
        let canonical = canonical_signing_binding_json(&sample_binding()).unwrap();

        assert_eq!(
            sha256_base64_url(&canonical),
            sha256_base64_url(
                br#"{"bodySHA256":"body-hash","captureID":"capture/1","operation":"tapcam.capture.sign","schemaID":"urn:tapnap:tapcam:app-attest-capture-signing:v1"}"#
            )
        );
    }

    #[test]
    fn invalid_capture_signing_binding_is_classified() {
        let mut binding = sample_binding();
        binding.schema_id = "other".to_string();
        assert_eq!(
            validate_capture_signing_binding(&binding),
            Some(CaptureSignatureInvalidReason::SchemaInvalid)
        );

        let mut binding = sample_binding();
        binding.operation = "other".to_string();
        assert_eq!(
            validate_capture_signing_binding(&binding),
            Some(CaptureSignatureInvalidReason::OperationInvalid)
        );

        let mut binding = sample_binding();
        binding.capture_id = " ".to_string();
        assert_eq!(
            validate_capture_signing_binding(&binding),
            Some(CaptureSignatureInvalidReason::BindingInvalid)
        );
    }
}
