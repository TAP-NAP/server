use crate::models::{AppAttestPurpose, ChallengeRecord, ChallengeStatus};
use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD},
    Engine,
};
use chrono::{DateTime, Duration as ChronoDuration, Timelike, Utc};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

pub const CHALLENGE_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct IssuedChallenge {
    pub record: ChallengeRecord,
}

/// Challenges are server-generated nonces. The client hashes these raw bytes
/// before calling Apple's App Attest APIs, so the raw value must remain
/// available server-side until attestation verification finishes.
pub fn issue_challenge(
    purpose: AppAttestPurpose,
    credential_name: String,
    ttl: Duration,
) -> IssuedChallenge {
    let mut raw = vec![0_u8; CHALLENGE_BYTES];
    OsRng.fill_bytes(&mut raw);

    let created_at = utc_now_seconds();
    let expires_at = created_at + chrono_duration(ttl);
    let raw_challenge_base64_url = encode_base64_url(&raw);
    let challenge_sha256_base64_url = sha256_base64_url(&raw);
    let challenge_id = Uuid::new_v4().to_string();

    IssuedChallenge {
        record: ChallengeRecord {
            challenge_id,
            raw_challenge_base64_url,
            challenge_sha256_base64_url,
            purpose,
            credential_name,
            associated_key_id: None,
            associated_capture_id: None,
            status: ChallengeStatus::Issued,
            created_at,
            expires_at,
            used_at: None,
        },
    }
}

pub fn encode_base64_url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn decode_base64_url(value: &str) -> Result<Vec<u8>, base64::DecodeError> {
    URL_SAFE_NO_PAD.decode(value)
}

pub fn canonicalize_key_id(value: &str) -> Option<String> {
    decode_key_id(value).map(|bytes| encode_base64_url(&bytes))
}

pub fn decode_key_id(value: &str) -> Option<Vec<u8>> {
    let value = value.trim();
    URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| URL_SAFE.decode(value))
        .or_else(|_| STANDARD_NO_PAD.decode(value))
        .or_else(|_| STANDARD.decode(value))
        .ok()
}

pub fn sha256_base64_url(bytes: &[u8]) -> String {
    encode_base64_url(&Sha256::digest(bytes))
}

pub fn sha256_bytes(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

pub fn is_expired(expires_at: DateTime<Utc>) -> bool {
    expires_at <= Utc::now()
}

fn chrono_duration(duration: Duration) -> ChronoDuration {
    ChronoDuration::from_std(duration).expect("challenge ttl must fit in chrono duration")
}

fn utc_now_seconds() -> DateTime<Utc> {
    Utc::now()
        .with_nanosecond(0)
        .expect("zero nanosecond is a valid timestamp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_challenge_is_32_bytes_and_hash_is_stored() {
        let issued = issue_challenge(
            AppAttestPurpose::Attestation,
            "photo_keyid".to_string(),
            Duration::from_secs(3_600),
        );
        let raw = decode_base64_url(&issued.record.raw_challenge_base64_url).unwrap();
        assert_eq!(raw.len(), CHALLENGE_BYTES);
        assert_eq!(
            issued.record.challenge_sha256_base64_url,
            sha256_base64_url(&raw)
        );
        assert_eq!(issued.record.status, ChallengeStatus::Issued);
    }

    #[test]
    fn base64_url_has_no_padding() {
        let encoded = encode_base64_url(b"hello world");
        assert!(!encoded.contains('='));
        assert_eq!(decode_base64_url(&encoded).unwrap(), b"hello world");
    }

    #[test]
    fn key_id_canonicalization_accepts_standard_base64() {
        assert_eq!(
            canonicalize_key_id("aGVsbG8gd29ybGQ=").unwrap(),
            encode_base64_url(b"hello world")
        );
    }
}
