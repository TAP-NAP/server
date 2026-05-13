use crate::{
    error::AppError,
    models::{ChallengeRecord, ChallengeStatus, CredentialRecord},
};
use chrono::{DateTime, Utc};
use redis::{aio::ConnectionManager, AsyncCommands, Script};
use std::time::Duration;

const CHALLENGE_PREFIX: &str = "tap:challenge:";
const CREDENTIAL_PREFIX: &str = "tap:credential:";

#[derive(Clone)]
pub struct RedisStore {
    connection: ConnectionManager,
}

impl RedisStore {
    pub async fn connect(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self { connection })
    }

    pub async fn ping(&self) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let _: String = redis::cmd("PING").query_async(&mut connection).await?;
        Ok(())
    }

    pub async fn put_challenge(
        &self,
        record: &ChallengeRecord,
        ttl: Duration,
    ) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let key = challenge_key(&record.challenge_id);
        let payload = serde_json::to_string(record)?;
        let _: () = connection.set_ex(key, payload, ttl.as_secs()).await?;
        Ok(())
    }

    pub async fn get_challenge(
        &self,
        challenge_id: &str,
    ) -> Result<Option<ChallengeRecord>, AppError> {
        let mut connection = self.connection.clone();
        let key = challenge_key(challenge_id);
        let payload: Option<String> = connection.get(key).await?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .transpose()
    }

    /// Atomically moves an issued challenge out of the reusable state. The
    /// verifier runs after this step, so a valid challenge cannot be replayed
    /// by racing two attestation requests.
    pub async fn consume_challenge(
        &self,
        challenge_id: &str,
        key_id: &str,
        used_at: DateTime<Utc>,
    ) -> Result<ChallengeRecord, AppError> {
        let mut connection = self.connection.clone();
        let key = challenge_key(challenge_id);
        let used_at = used_at.to_rfc3339();
        let result: String = Script::new(
            r#"
            local raw = redis.call("GET", KEYS[1])
            if not raw then
                return "ERR:MISSING"
            end

            local record = cjson.decode(raw)
            if record["status"] ~= "issued" then
                return "ERR:STATUS:" .. tostring(record["status"])
            end

            record["status"] = "used"
            record["usedAt"] = ARGV[1]
            record["associatedKeyId"] = ARGV[2]
            redis.call("SET", KEYS[1], cjson.encode(record), "KEEPTTL")
            return "OK:" .. raw
            "#,
        )
        .key(key)
        .arg(used_at)
        .arg(key_id)
        .invoke_async(&mut connection)
        .await?;

        if result == "ERR:MISSING" {
            return Err(AppError::ChallengeNotFound);
        }
        if let Some(status) = result.strip_prefix("ERR:STATUS:") {
            return Err(AppError::ChallengeInvalid(format!(
                "challenge status is {status}"
            )));
        }
        let payload = result.strip_prefix("OK:").ok_or_else(|| {
            AppError::Storage(format!("unexpected Redis script response: {result}"))
        })?;

        Ok(serde_json::from_str(payload)?)
    }

    pub async fn mark_challenge_failed(
        &self,
        challenge_id: &str,
        key_id: &str,
    ) -> Result<(), AppError> {
        self.update_challenge_status(challenge_id, key_id, ChallengeStatus::Failed)
            .await
    }

    async fn update_challenge_status(
        &self,
        challenge_id: &str,
        key_id: &str,
        status: ChallengeStatus,
    ) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let key = challenge_key(challenge_id);
        let status = serde_json::to_string(&status)?
            .trim_matches('"')
            .to_string();
        let _: String = Script::new(
            r#"
            local raw = redis.call("GET", KEYS[1])
            if not raw then
                return "MISSING"
            end

            local record = cjson.decode(raw)
            record["status"] = ARGV[1]
            record["associatedKeyId"] = ARGV[2]
            redis.call("SET", KEYS[1], cjson.encode(record), "KEEPTTL")
            return "OK"
            "#,
        )
        .key(key)
        .arg(status)
        .arg(key_id)
        .invoke_async(&mut connection)
        .await?;
        Ok(())
    }

    pub async fn save_credential(&self, record: &CredentialRecord) -> Result<(), AppError> {
        let mut connection = self.connection.clone();
        let key = credential_key(&record.key_id);
        let payload = serde_json::to_string(record)?;
        let _: () = connection.set(key, payload).await?;
        Ok(())
    }

    pub async fn get_credential(&self, key_id: &str) -> Result<Option<CredentialRecord>, AppError> {
        let mut connection = self.connection.clone();
        let payload: Option<String> = connection.get(credential_key(key_id)).await?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(AppError::from))
            .transpose()
    }
}

fn challenge_key(challenge_id: &str) -> String {
    format!("{CHALLENGE_PREFIX}{challenge_id}")
}

fn credential_key(key_id: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{key_id}")
}
