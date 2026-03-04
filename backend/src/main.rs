use essence::api;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// MCP server imports
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager,
    StreamableHttpService, StreamableHttpServerConfig,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "essence=info".to_string());
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| log_level.into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Essence web retrieval engine");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let max_request_size_mb: usize = std::env::var("MAX_REQUEST_SIZE_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    // Create the MCP server service (Streamable HTTP transport)
    let mcp_service = StreamableHttpService::new(
        || Ok(essence::mcp::EssenceMcpServer::new()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let app = api::create_router()
        .nest_service("/mcp", mcp_service)
        .layer(RequestBodyLimitLayer::new(max_request_size_mb * 1024 * 1024))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Server listening on {}", addr);
    info!("REST API available at http://{}/api/v1/", addr);
    info!("MCP server available at http://{}/mcp", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    info!("Server stopped");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { info!("Received Ctrl+C signal"); },
        _ = terminate => { info!("Received terminate signal"); },
    }

    warn!("Initiating graceful shutdown");
}
