use crate::{
    challenge::{canonicalize_key_id, decode_base64_url, is_expired, issue_challenge},
    error::AppError,
    models::{
        AppAttestPurpose, AttestationRequest, AttestationResponse, ChallengeRequest,
        ChallengeResponse, CredentialStatus, CredentialStatusRequest, HealthResponse,
        RegistrationStatus, ServerCredentialStatus,
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

    let credential_id = credential.credential_id.clone();
    state.store.save_credential(&credential).await?;

    Ok(Json(AttestationResponse {
        credential_id: Some(credential_id),
        status: RegistrationStatus::Accepted,
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
            credential_id: Some(credential.credential_id),
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
