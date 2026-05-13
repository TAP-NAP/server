use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppAttestPurpose {
    Attestation,
    Assertion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeStatus {
    Issued,
    Used,
    Failed,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialStatus {
    Active,
    Revoked,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppEnvironment {
    Development,
    Production,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeRequest {
    pub purpose: AppAttestPurpose,
    pub credential_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeResponse {
    pub challenge_id: String,
    pub challenge: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttestationRequest {
    pub credential_name: String,
    pub key_id: String,
    pub challenge_id: String,
    pub attestation_object: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttestationResponse {
    pub credential_id: Option<String>,
    pub status: RegistrationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistrationStatus {
    Accepted,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatusRequest {
    pub credential_name: String,
    pub key_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerCredentialStatus {
    Accepted,
    Revoked,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub redis: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeRecord {
    pub challenge_id: String,
    pub raw_challenge_base64_url: String,
    pub challenge_sha256_base64_url: String,
    pub purpose: AppAttestPurpose,
    pub credential_name: String,
    pub associated_key_id: Option<String>,
    pub associated_capture_id: Option<String>,
    pub status: ChallengeStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRecord {
    pub credential_name: String,
    pub key_id: String,
    pub credential_id: String,
    pub public_key_x962_base64_url: String,
    pub public_key_sha256_base64_url: String,
    pub credential_public_key_cose_base64_url: String,
    pub receipt_base64_url: String,
    pub team_id: String,
    pub bundle_id: String,
    pub environment: AppEnvironment,
    pub last_counter: Option<u32>,
    pub status: CredentialStatus,
    pub attestation_challenge_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CredentialRecord {
    pub fn is_active_for(&self, credential_name: &str) -> bool {
        self.status == CredentialStatus::Active && self.credential_name == credential_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_request_matches_app_attest_kit_json() {
        let request: ChallengeRequest =
            serde_json::from_str(r#"{"purpose":"attestation","credentialName":"photo_keyid"}"#)
                .unwrap();
        assert_eq!(request.purpose, AppAttestPurpose::Attestation);
        assert_eq!(request.credential_name, "photo_keyid");
    }

    #[test]
    fn credential_status_serializes_as_json_string() {
        assert_eq!(
            serde_json::to_string(&ServerCredentialStatus::Accepted).unwrap(),
            r#""accepted""#
        );
    }
}
