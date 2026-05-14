mod challenge;
mod config;
mod error;
mod models;
mod openapi;
mod routes;
mod store;
mod verifier;

use crate::config::Config;
use crate::routes::router;
use crate::store::redis::RedisStore;
use crate::verifier::AttestationVerifier;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub store: RedisStore,
    pub verifier: AttestationVerifier,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tap_app_attest_server=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Arc::new(Config::from_env()?);
    let store = RedisStore::connect(&config.redis_url).await?;
    let verifier = AttestationVerifier::new(config.clone())?;

    let state = AppState {
        config: config.clone(),
        store,
        verifier,
    };

    let app = router(state).layer(TraceLayer::new_for_http());
    let listener = TcpListener::bind(&config.server_addr).await?;
    tracing::info!(addr = %config.server_addr, "starting server");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
