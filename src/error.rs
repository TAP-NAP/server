use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("challenge not found")]
    ChallengeNotFound,
    #[error("challenge is not usable: {0}")]
    ChallengeInvalid(String),
    #[error("request keyId does not match attested key: {message}")]
    KeyIdMismatch { message: String },
    #[error("credential not found")]
    CredentialNotFound,
    #[error("storage error: {0}")]
    Storage(String),
    #[error("attestation verifier rejected the object")]
    AttestationRejected {
        validation_stage: String,
        error_code: String,
        message: String,
    },
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    message: String,
    details: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream: Option<UpstreamError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamError {
    validation_stage: String,
    error_code: String,
    message: String,
}

impl AppError {
    fn status_and_body(&self) -> (StatusCode, ErrorBody) {
        match self {
            Self::InvalidInput(message) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "INPUT_INVALID",
                    message: message.clone(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::ChallengeNotFound => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "CHALLENGE_NOT_FOUND",
                    message: "challenge was not found or has expired".to_string(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::ChallengeInvalid(message) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "CHALLENGE_INVALID",
                    message: message.clone(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::KeyIdMismatch { message } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "KEY_ID_MISMATCH",
                    message: message.clone(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::CredentialNotFound => (
                StatusCode::NOT_FOUND,
                ErrorBody {
                    code: "KEY_NOT_REGISTERED",
                    message: "credential is not registered".to_string(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::Storage(message) => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code: "STORAGE_ERROR",
                    message: message.clone(),
                    details: json!({}),
                    upstream: None,
                },
            ),
            Self::AttestationRejected {
                validation_stage,
                error_code,
                message,
            } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code: "ATTESTATION_INVALID",
                    message: "attestation verification failed".to_string(),
                    details: json!({}),
                    upstream: Some(UpstreamError {
                        validation_stage: validation_stage.clone(),
                        error_code: error_code.clone(),
                        message: message.clone(),
                    }),
                },
            ),
            Self::Internal(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code: "INTERNAL_ERROR",
                    message: message.clone(),
                    details: json!({}),
                    upstream: None,
                },
            ),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = self.status_and_body();
        (status, Json(ErrorEnvelope { error: body })).into_response()
    }
}

impl From<redis::RedisError> for AppError {
    fn from(value: redis::RedisError) -> Self {
        Self::Storage(value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::Internal(value.to_string())
    }
}
