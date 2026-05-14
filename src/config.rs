use apple_app_attest_attestation::AppAttestEnvironment;
use std::{env, fs, net::SocketAddr, time::Duration};

#[cfg(test)]
use std::path::PathBuf;
use thiserror::Error;

const DEFAULT_CHALLENGE_TTL_SECONDS: u64 = 3_600;

#[derive(Debug, Clone)]
pub struct Config {
    pub server_addr: SocketAddr,
    pub redis_url: String,
    pub team_id: String,
    pub bundle_id: String,
    pub app_attest_environment: AppAttestEnvironment,
    pub apple_root_ca: Vec<u8>,
    pub challenge_ttl: Duration,
    pub request_logs: bool,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("environment variable {0} is required")]
    Missing(&'static str),
    #[error("environment variable {name} is invalid: {message}")]
    Invalid { name: &'static str, message: String },
    #[error("failed to read Apple App Attestation Root CA at {path}: {source}")]
    ReadRootCa {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let server_addr = parse_socket_addr(
            "SERVER_ADDR",
            env::var("SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        )?;
        let redis_url = non_empty("REDIS_URL")?;
        let team_id = non_empty("TEAM_ID")?;
        let bundle_id = non_empty("BUNDLE_ID")?;
        let app_attest_environment = parse_environment(
            env::var("APP_ATTEST_ENV").unwrap_or_else(|_| "production".to_string()),
        )?;
        let root_ca_path = non_empty("APPLE_APP_ATTEST_ROOT_CA_PATH")?;
        let apple_root_ca = fs::read(&root_ca_path).map_err(|source| ConfigError::ReadRootCa {
            path: root_ca_path,
            source,
        })?;
        let challenge_ttl = parse_duration_seconds(
            "CHALLENGE_TTL_SECONDS",
            env::var("CHALLENGE_TTL_SECONDS")
                .unwrap_or_else(|_| DEFAULT_CHALLENGE_TTL_SECONDS.to_string()),
        )?;
        let request_logs = parse_bool(
            "REQUEST_LOGS",
            env::var("REQUEST_LOGS").unwrap_or_else(|_| "false".to_string()),
        )?;

        Ok(Self {
            server_addr,
            redis_url,
            team_id,
            bundle_id,
            app_attest_environment,
            apple_root_ca,
            challenge_ttl,
            request_logs,
        })
    }

    #[cfg(test)]
    pub fn for_tests(root_ca_path: PathBuf) -> Self {
        Self {
            server_addr: "127.0.0.1:0".parse().unwrap(),
            redis_url: "redis://localhost:6379".to_string(),
            team_id: "0352187391".to_string(),
            bundle_id: "com.apple.example_app_attest".to_string(),
            app_attest_environment: AppAttestEnvironment::Production,
            apple_root_ca: fs::read(root_ca_path).unwrap_or_default(),
            challenge_ttl: Duration::from_secs(DEFAULT_CHALLENGE_TTL_SECONDS),
            request_logs: false,
        }
    }
}

fn non_empty(name: &'static str) -> Result<String, ConfigError> {
    let value = env::var(name).map_err(|_| ConfigError::Missing(name))?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ConfigError::Missing(name));
    }
    Ok(trimmed.to_string())
}

fn parse_socket_addr(name: &'static str, value: String) -> Result<SocketAddr, ConfigError> {
    value.parse().map_err(|err| ConfigError::Invalid {
        name,
        message: format!("expected host:port socket address: {err}"),
    })
}

fn parse_environment(value: String) -> Result<AppAttestEnvironment, ConfigError> {
    match value.trim() {
        "production" => Ok(AppAttestEnvironment::Production),
        "development" => Ok(AppAttestEnvironment::Development),
        other => Err(ConfigError::Invalid {
            name: "APP_ATTEST_ENV",
            message: format!("expected production or development, got {other:?}"),
        }),
    }
}

fn parse_duration_seconds(name: &'static str, value: String) -> Result<Duration, ConfigError> {
    let seconds: u64 = value.parse().map_err(|err| ConfigError::Invalid {
        name,
        message: format!("expected integer seconds: {err}"),
    })?;
    if seconds == 0 {
        return Err(ConfigError::Invalid {
            name,
            message: "must be greater than zero".to_string(),
        });
    }
    Ok(Duration::from_secs(seconds))
}

fn parse_bool(name: &'static str, value: String) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(ConfigError::Invalid {
            name,
            message: format!("expected true or false, got {other:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_challenge_ttl_is_one_hour() {
        assert_eq!(
            parse_duration_seconds("CHALLENGE_TTL_SECONDS", "3600".to_string()).unwrap(),
            Duration::from_secs(3_600)
        );
    }

    #[test]
    fn parses_environment() {
        assert_eq!(
            parse_environment("production".to_string()).unwrap(),
            AppAttestEnvironment::Production
        );
        assert_eq!(
            parse_environment("development".to_string()).unwrap(),
            AppAttestEnvironment::Development
        );
    }

    #[test]
    fn parses_bool_flags() {
        assert!(parse_bool("REQUEST_LOGS", "true".to_string()).unwrap());
        assert!(parse_bool("REQUEST_LOGS", "on".to_string()).unwrap());
        assert!(!parse_bool("REQUEST_LOGS", "false".to_string()).unwrap());
        assert!(!parse_bool("REQUEST_LOGS", "0".to_string()).unwrap());
        assert!(parse_bool("REQUEST_LOGS", "maybe".to_string()).is_err());
    }
}
