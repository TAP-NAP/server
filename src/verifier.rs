use crate::{
    challenge::{decode_base64_url, decode_key_id, encode_base64_url, sha256_bytes},
    config::Config,
    error::AppError,
    models::{AppEnvironment, AttestationRequest, CredentialRecord, CredentialStatus},
};
use apple_app_attest_attestation::{parse_and_verify_attestation, AttestationVerificationContext};
use chrono::{Timelike, Utc};
use std::sync::Arc;

#[derive(Clone)]
pub struct AttestationVerifier {
    config: Arc<Config>,
}

impl AttestationVerifier {
    pub fn new(config: Arc<Config>) -> Result<Self, AppError> {
        Ok(Self { config })
    }

    pub fn verify_registration(
        &self,
        request: &AttestationRequest,
        raw_challenge: &[u8],
    ) -> Result<CredentialRecord, AppError> {
        let attestation_object = decode_base64_url(&request.attestation_object).map_err(|err| {
            AppError::InvalidInput(format!("attestationObject is not valid base64url: {err}"))
        })?;

        let client_data_hash = sha256_bytes(raw_challenge);
        let context = AttestationVerificationContext {
            client_data_hash,
            team_id: self.config.team_id.clone(),
            bundle_id: self.config.bundle_id.clone(),
            environment: self.config.app_attest_environment,
        };

        let success =
            parse_and_verify_attestation(&attestation_object, &self.config.apple_root_ca, &context)
                .map_err(map_attestation_error)?;

        let attested_key_id = encode_base64_url(&success.parsed_auth_data.credential_id);
        verify_request_key_id_matches_attestation(
            &request.key_id,
            &success.parsed_auth_data.credential_id,
        )?;

        let now = Utc::now()
            .with_nanosecond(0)
            .expect("zero nanosecond is a valid timestamp");
        Ok(CredentialRecord {
            credential_name: request.credential_name.clone(),
            key_id: attested_key_id.clone(),
            credential_id: attested_key_id,
            public_key_x962_base64_url: encode_base64_url(
                &success.first_certificate_values.public_key_x962,
            ),
            public_key_sha256_base64_url: encode_base64_url(
                &success.first_certificate_values.public_key_sha256,
            ),
            credential_public_key_cose_base64_url: encode_base64_url(
                &success.parsed_auth_data.credential_public_key_cose,
            ),
            receipt_base64_url: encode_base64_url(&success.receipt_raw),
            team_id: self.config.team_id.clone(),
            bundle_id: self.config.bundle_id.clone(),
            environment: AppEnvironment::from(self.config.app_attest_environment),
            last_counter: None,
            status: CredentialStatus::Active,
            attestation_challenge_id: request.challenge_id.clone(),
            created_at: now,
            updated_at: now,
        })
    }
}

fn verify_request_key_id_matches_attestation(
    request_key_id: &str,
    attested_credential_id: &[u8],
) -> Result<(), AppError> {
    let request_key_id_bytes =
        decode_key_id(request_key_id).ok_or_else(|| AppError::KeyIdMismatch {
            message: "request keyId is not valid base64 or base64url".to_string(),
        })?;

    if request_key_id_bytes != attested_credential_id {
        return Err(AppError::KeyIdMismatch {
            message: "request keyId does not match the credential id proven by attestation"
                .to_string(),
        });
    }

    Ok(())
}

fn map_attestation_error(error: apple_app_attest_attestation::AttestationError) -> AppError {
    AppError::AttestationRejected {
        validation_stage: error.validation_stage.to_string(),
        error_code: error.error_code.to_string(),
        message: error.message.to_string(),
    }
}

impl From<apple_app_attest_attestation::AppAttestEnvironment> for AppEnvironment {
    fn from(value: apple_app_attest_attestation::AppAttestEnvironment) -> Self {
        match value {
            apple_app_attest_attestation::AppAttestEnvironment::Development => Self::Development,
            apple_app_attest_attestation::AppAttestEnvironment::Production => Self::Production,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};

    #[test]
    fn request_key_id_must_match_attested_credential_id() {
        let attested = b"attested-key-id";
        let matching_key_id = STANDARD.encode(attested);

        assert!(verify_request_key_id_matches_attestation(&matching_key_id, attested).is_ok());

        let error = verify_request_key_id_matches_attestation(&STANDARD.encode(b"other"), attested)
            .expect_err("mismatched key id should be rejected");
        assert!(matches!(error, AppError::KeyIdMismatch { .. }));
    }
}
