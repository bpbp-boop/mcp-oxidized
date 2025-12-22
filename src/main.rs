mod config;

use config::Config;
use rmcp::{ServerHandler, ServiceExt, model::ServerInfo};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimal MCP server implementation
/// Tools and resources will be added in later stories
#[derive(Clone)]
struct OxidizedServer {
    _config: Config,
}

impl ServerHandler for OxidizedServer {
    fn get_info(&self) -> ServerInfo {
        use rmcp::model::{Implementation, ProtocolVersion, ServerCapabilities};

        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::default(),
            server_info: Implementation {
                name: "mcp-oxidized".to_string(),
                title: Some("Oxidized MCP Server".to_string()),
                version: VERSION.to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "MCP server for Oxidized network device configuration backup system".to_string(),
            ),
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber with env filter support
    // Default level: INFO, can be overridden via RUST_LOG env var
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr) // stdout reserved for MCP JSON-RPC
        .init();

    info!("mcp-oxidized v{} starting", VERSION);

    // Load configuration
    let config = match Config::load() {
        Ok(cfg) => {
            info!("Configuration loaded successfully");
            info!("Oxidized URL: {}", cfg.oxidized_url);
            if cfg.oxidized_user.is_some() {
                info!("Authentication: enabled");
            } else {
                info!("Authentication: disabled (zero-config mode)");
            }
            cfg
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Create MCP server instance
    let server = OxidizedServer { _config: config };

    info!("MCP server initialized, starting stdio transport");

    // Run the server with stdio transport
    if let Err(e) = server.serve(rmcp::transport::stdio()).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }

    info!("mcp-oxidized server shutting down");
}
