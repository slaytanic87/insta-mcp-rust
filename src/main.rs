mod config;
mod error;
mod instagram_client;
mod mcp_server_sse;
mod mcp_server_stdio;
mod models;
mod token_refresh_ticker;

use std::sync::Arc;

use config::Settings;
use instagram_client::InstagramClient;
use mcp_server_stdio::InstagramMcpServer;
use tracing::{error, info, warn};

use crate::{
    mcp_server_sse::mcp_server_run_sse,
    token_refresh_ticker::create_and_run_instagram_token_validity_task,
};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let settings = match Settings::from_env() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    init_logging(settings.log_level.clone(), settings.log_format.clone());

    info!(
        server_name = %settings.mcp_server_name,
        server_version = %settings.mcp_server_version,
        mcp_transport = %settings.mcp_transport,
        "Starting Instagram MCP server"
    );

    let client: Arc<InstagramClient> = match InstagramClient::new(settings.clone()) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            error!(error = %e, "Failed to initialize Instagram client");
            std::process::exit(1);
        }
    };

    if !client.validate_access_token().await {
        warn!("Instagram access token validation failed on startup");
    } else {
        info!("Instagram access token validated successfully");
        create_and_run_instagram_token_validity_task(client.clone());
    }

    if settings.mcp_transport.to_lowercase() == "sse" {
        mcp_server_run_sse(client).await.unwrap();
    } else if settings.mcp_transport.to_lowercase() == "stdio" {
        let server = InstagramMcpServer::new(settings, client);
        if let Err(e) = server.run_stdio().await {
            error!(error = %e, "Server exited with error");
        }
        std::process::exit(1);
    } else {
        error!(
            "Only stdio or sse MCP_TRANSPORT values are supported, received: {} as value",
            settings.mcp_transport
        );
    }
}

fn init_logging(log_level: String, log_format: String) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    if log_format.eq_ignore_ascii_case("json") {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .with_current_span(false)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
}
