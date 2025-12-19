//! HTMLWordPress API Server
//! High-performance WordPress optimization service

pub mod config;
pub mod handlers;
pub mod optimizer;
pub mod css_optimizer;
pub mod seo_optimizer;
pub mod schema_generator;
pub mod image_optimizer;
pub mod webp_converter;
pub mod resource_optimizer;
mod error;
mod test_verification;

use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "htmlwordpress_api=debug,info".into()),
        ))
        .init();

    // Load config
    dotenvy::dotenv().ok();
    let config = config::Config::from_env();

    tracing::info!("Starting HTMLWordPress API on {}", config.address());

    let state = config::AppState {
        api_key: config.api_key.clone(),
    };

    // Build router
    let app = Router::new()
        .route("/health", get(handlers::health))
        .route("/api/v1/health", get(handlers::health))
        .route("/api/v1/optimize", post(handlers::optimize))
        .route("/api/v1/optimize/bulk", post(handlers::optimize_bulk))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(config.address())
        .await
        .expect("Failed to bind");

    tracing::info!("Server listening on http://{}", config.address());

    axum::serve(listener, app).await.expect("Server error");
}
