use mcp_oxidized::config::Config;
use mcp_oxidized::error::{Actionable, OxidizedError};
use mcp_oxidized::oxidized::OxidizedClient;
use mcp_oxidized::resources;
use mcp_oxidized::tools;
use rmcp::model::{
    Annotated, CallToolRequestParam, CallToolResult, Content, ErrorCode, Implementation,
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParam,
    ProtocolVersion, RawResource, RawResourceTemplate, ReadResourceRequestParam,
    ReadResourceResult, ResourceContents, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;
use tracing::{error, info, instrument};
use tracing_subscriber::{EnvFilter, fmt};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP server implementation for Oxidized network device backup system.
///
/// Provides resources for node discovery, configuration access, and statistics:
/// - `oxidized://nodes` - List all nodes (paginated)
/// - `oxidized://node/{name}` - Get specific node details
/// - `oxidized://node/{name}/config` - Get current configuration (FR5)
/// - `oxidized://node/{name}/versions` - Get version history (FR6)
/// - `oxidized://node/{name}/versions/{oid}` - Get specific version config (FR7)
/// - `oxidized://stats` - Global statistics
///
/// Provides tools for backup, queue management, and configuration analysis:
/// - `fetch_node_config` - Trigger immediate backup (FR15)
/// - `prioritize_node` - Prioritize node in queue (FR16)
/// - `reload_sources` - Reload source inventory (FR17)
/// - `diff_configs` - Compare two configuration versions (FR9)
/// - `search_configs` - Search for patterns across configurations (FR10-FR13)
#[derive(Clone)]
struct OxidizedServer {
    client: Arc<OxidizedClient>,
}

impl OxidizedServer {
    /// Create a new OxidizedServer with the given configuration.
    fn new(config: Config) -> Self {
        Self {
            client: Arc::new(OxidizedClient::new(&config)),
        }
    }

    /// Convert OxidizedError to MCP ErrorData with LLM-optimized message.
    fn to_mcp_error(err: OxidizedError) -> McpError {
        let code = match &err {
            OxidizedError::NodeNotFound(_, _) => ErrorCode::INVALID_PARAMS,
            OxidizedError::AuthFailed => ErrorCode::INVALID_REQUEST,
            OxidizedError::ApiUnreachable { .. } => ErrorCode::INTERNAL_ERROR,
            OxidizedError::InvalidRegex(_) => ErrorCode::INVALID_PARAMS,
            OxidizedError::ConfigError(_) => ErrorCode::INVALID_REQUEST,
            OxidizedError::ParseError { .. } => ErrorCode::PARSE_ERROR,
            OxidizedError::HttpError { status_code, .. } => {
                if *status_code >= 500 {
                    ErrorCode::INTERNAL_ERROR
                } else {
                    ErrorCode::INVALID_REQUEST
                }
            }
        };

        McpError::new(code, err.to_llm_message(), None)
    }
}

impl ServerHandler for OxidizedServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder()
                .enable_resources()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "mcp-oxidized".to_string(),
                title: Some("Oxidized MCP Server".to_string()),
                version: VERSION.to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "MCP server for Oxidized network device configuration backup system. \
                 Use oxidized://nodes to list devices, oxidized://node/{name} for details, \
                 and oxidized://stats for backup statistics."
                    .to_string(),
            ),
        }
    }

    #[instrument(skip(self, _context), fields(request_id = %resources::generate_request_id()))]
    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        async move {
            let resources = vec![
                Annotated::new(
                    RawResource {
                        uri: "oxidized://nodes".to_string(),
                        name: "nodes".to_string(),
                        title: Some("All Nodes".to_string()),
                        description: Some(
                            "List all network devices in the Oxidized inventory with pagination"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                        size: None,
                        icons: None,
                        meta: None,
                    },
                    None,
                ),
                Annotated::new(
                    RawResource {
                        uri: "oxidized://stats".to_string(),
                        name: "stats".to_string(),
                        title: Some("Statistics".to_string()),
                        description: Some(
                            "Global backup statistics including success rate and last run time"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                        size: None,
                        icons: None,
                        meta: None,
                    },
                    None,
                ),
            ];

            Ok(ListResourcesResult {
                resources,
                next_cursor: None,
                meta: None,
            })
        }
    }

    #[instrument(skip(self, _context), fields(request_id = %resources::generate_request_id()))]
    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourceTemplatesResult, McpError>> + Send + '_ {
        async move {
            let templates = vec![
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "oxidized://node/{name}".to_string(),
                        name: "node".to_string(),
                        title: Some("Node Details".to_string()),
                        description: Some(
                            "Get detailed information about a specific network device by name"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "oxidized://node/{name}/config".to_string(),
                        name: "node_config".to_string(),
                        title: Some("Node Configuration".to_string()),
                        description: Some(
                            "Get the current configuration of a network device with size metadata"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "oxidized://node/{name}/versions".to_string(),
                        name: "node_versions".to_string(),
                        title: Some("Configuration Versions".to_string()),
                        description: Some(
                            "List all available configuration versions for a node, sorted newest first"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                    },
                    None,
                ),
                Annotated::new(
                    RawResourceTemplate {
                        uri_template: "oxidized://node/{name}/versions/{oid}".to_string(),
                        name: "node_version".to_string(),
                        title: Some("Historical Configuration".to_string()),
                        description: Some(
                            "Get configuration at a specific version by Git commit OID"
                                .to_string(),
                        ),
                        mime_type: Some("application/json".to_string()),
                    },
                    None,
                ),
            ];

            Ok(ListResourceTemplatesResult {
                resource_templates: templates,
                next_cursor: None,
                meta: None,
            })
        }
    }

    #[instrument(skip(self, _context), fields(request_id = %resources::generate_request_id(), uri = %request.uri))]
    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        let client = Arc::clone(&self.client);

        async move {
            let uri = &request.uri;

            // Parse the URI and route to appropriate handler
            if uri == "oxidized://nodes" {
                // List all nodes
                let result = resources::list_nodes(&*client, None, None, None)
                    .await
                    .map_err(Self::to_mcp_error)?;

                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to serialize nodes: {}", e),
                        None,
                    )
                })?;

                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::TextResourceContents {
                        uri: uri.clone(),
                        mime_type: Some("application/json".to_string()),
                        text: json,
                        meta: None,
                    }],
                })
            } else if uri == "oxidized://stats" {
                // Get statistics
                let result = resources::get_stats(&*client)
                    .await
                    .map_err(Self::to_mcp_error)?;

                let json = serde_json::to_string_pretty(&result).map_err(|e| {
                    McpError::new(
                        ErrorCode::INTERNAL_ERROR,
                        format!("Failed to serialize stats: {}", e),
                        None,
                    )
                })?;

                Ok(ReadResourceResult {
                    contents: vec![ResourceContents::TextResourceContents {
                        uri: uri.clone(),
                        mime_type: Some("application/json".to_string()),
                        text: json,
                        meta: None,
                    }],
                })
            } else if let Some(path) = uri.strip_prefix("oxidized://node/") {
                // Parse node resource paths:
                // - {name} -> node details
                // - {name}/config -> node configuration
                // - {name}/versions -> version list
                // - {name}/versions/{oid} -> specific version

                if let Some(rest) = path.strip_suffix("/config") {
                    // oxidized://node/{name}/config - Get node configuration
                    let node_name = rest;
                    let result = resources::get_node_config(&*client, node_name)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize config: {}", e),
                            None,
                        )
                    })?;

                    Ok(ReadResourceResult {
                        contents: vec![ResourceContents::TextResourceContents {
                            uri: uri.clone(),
                            mime_type: Some("application/json".to_string()),
                            text: json,
                            meta: None,
                        }],
                    })
                } else if path.contains("/versions/") {
                    // oxidized://node/{name}/versions/{oid} - Get specific version
                    let parts: Vec<&str> = path.splitn(3, '/').collect();
                    if parts.len() == 3 && parts[1] == "versions" {
                        let node_name = parts[0];
                        let oid = parts[2];
                        let result = resources::get_node_version(&*client, node_name, oid)
                            .await
                            .map_err(Self::to_mcp_error)?;

                        let json = serde_json::to_string_pretty(&result).map_err(|e| {
                            McpError::new(
                                ErrorCode::INTERNAL_ERROR,
                                format!("Failed to serialize version config: {}", e),
                                None,
                            )
                        })?;

                        Ok(ReadResourceResult {
                            contents: vec![ResourceContents::TextResourceContents {
                                uri: uri.clone(),
                                mime_type: Some("application/json".to_string()),
                                text: json,
                                meta: None,
                            }],
                        })
                    } else {
                        Err(McpError::new(
                            ErrorCode::INVALID_PARAMS,
                            format!(
                                "[Error] Invalid version path: '{}'\n\
                                 [Context] Expected format: oxidized://node/{{name}}/versions/{{oid}}\n\
                                 [Next Step] Provide both node name and version OID.",
                                uri
                            ),
                            None,
                        ))
                    }
                } else if let Some(node_name) = path.strip_suffix("/versions") {
                    // oxidized://node/{name}/versions - Get version list
                    let result = resources::get_node_versions(&*client, node_name)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize versions: {}", e),
                            None,
                        )
                    })?;

                    Ok(ReadResourceResult {
                        contents: vec![ResourceContents::TextResourceContents {
                            uri: uri.clone(),
                            mime_type: Some("application/json".to_string()),
                            text: json,
                            meta: None,
                        }],
                    })
                } else if !path.contains('/') {
                    // oxidized://node/{name} - Get node details (no slashes in name)
                    let node_name = path;
                    let result = resources::get_node(&*client, node_name)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize node: {}", e),
                            None,
                        )
                    })?;

                    Ok(ReadResourceResult {
                        contents: vec![ResourceContents::TextResourceContents {
                            uri: uri.clone(),
                            mime_type: Some("application/json".to_string()),
                            text: json,
                            meta: None,
                        }],
                    })
                } else {
                    // Unknown subpath
                    Err(McpError::new(
                        ErrorCode::INVALID_PARAMS,
                        format!(
                            "[Error] Unknown node resource path: '{}'\n\
                             [Context] Attempted to read an unsupported node subresource.\n\
                             [Suggestions] Valid paths: /config, /versions, /versions/{{oid}}\n\
                             [Next Step] Use oxidized://node/{{name}}/config, /versions, or /versions/{{oid}}.",
                            uri
                        ),
                        None,
                    ))
                }
            } else {
                // Unknown resource URI
                Err(McpError::new(
                    ErrorCode::INVALID_PARAMS,
                    format!(
                        "[Error] Unknown resource URI: '{}'\n\
                         [Context] Attempted to read a resource that does not exist.\n\
                         [Suggestions] Available resources: oxidized://nodes, oxidized://stats, oxidized://node/{{name}}, oxidized://node/{{name}}/config, oxidized://node/{{name}}/versions\n\
                         [Next Step] Use one of the available resource URIs.",
                        uri
                    ),
                    None,
                ))
            }
        }
    }

    #[instrument(skip(self, _context), fields(request_id = %resources::generate_request_id()))]
    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        async move {
            // Helper to convert serde_json::Value to JsonObject (Map<String, Value>)
            fn value_to_json_object(
                v: serde_json::Value,
            ) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
                match v {
                    serde_json::Value::Object(map) => std::sync::Arc::new(map),
                    _ => std::sync::Arc::new(serde_json::Map::new()),
                }
            }

            let tools = vec![
                Tool {
                    name: Cow::Borrowed("fetch_node_config"),
                    title: Some("Fetch Node Configuration".to_string()),
                    description: Some(Cow::Borrowed(
                        "Trigger an immediate backup of a node's configuration. \
                         The fresh configuration will be available shortly after.",
                    )),
                    input_schema: value_to_json_object(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "node": {
                                "type": "string",
                                "description": "The node name to backup"
                            }
                        },
                        "required": ["node"]
                    })),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
                Tool {
                    name: Cow::Borrowed("prioritize_node"),
                    title: Some("Prioritize Node".to_string()),
                    description: Some(Cow::Borrowed(
                        "Move a node to the front of the backup queue. \
                         The node will be processed before other pending nodes.",
                    )),
                    input_schema: value_to_json_object(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "node": {
                                "type": "string",
                                "description": "The node name to prioritize"
                            }
                        },
                        "required": ["node"]
                    })),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
                Tool {
                    name: Cow::Borrowed("reload_sources"),
                    title: Some("Reload Sources".to_string()),
                    description: Some(Cow::Borrowed(
                        "Reload the Oxidized source inventory. \
                         New devices will be immediately available after this operation.",
                    )),
                    input_schema: value_to_json_object(serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    })),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
                Tool {
                    name: Cow::Borrowed("diff_configs"),
                    title: Some("Diff Configurations".to_string()),
                    description: Some(Cow::Borrowed(
                        "Compare two configuration versions of a node. \
                         Returns a structured diff with additions, deletions, and modifications \
                         in an LLM-friendly format.",
                    )),
                    input_schema: value_to_json_object(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "node": {
                                "type": "string",
                                "description": "The node name to compare configurations for"
                            },
                            "version1": {
                                "type": "string",
                                "description": "The first version OID (older version)"
                            },
                            "version2": {
                                "type": "string",
                                "description": "The second version OID (newer version)"
                            }
                        },
                        "required": ["node", "version1", "version2"]
                    })),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
                Tool {
                    name: Cow::Borrowed("search_configs"),
                    title: Some("Search Configurations".to_string()),
                    description: Some(Cow::Borrowed(
                        "Search for regex patterns across network device configurations. \
                         Returns matches with line numbers and context lines. \
                         Supports case-sensitive/insensitive search and optional node filtering.",
                    )),
                    input_schema: value_to_json_object(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "pattern": {
                                "type": "string",
                                "description": "Regex pattern to search for (e.g., 'ip address 10\\.0\\.' or 'snmp-server community')"
                            },
                            "nodes": {
                                "type": "array",
                                "items": {"type": "string"},
                                "description": "Optional list of node names to limit search to. If not provided, searches all nodes."
                            },
                            "case_sensitive": {
                                "type": "boolean",
                                "default": true,
                                "description": "Whether the search is case-sensitive (default: true)"
                            },
                            "limit": {
                                "type": "integer",
                                "default": 100,
                                "minimum": 1,
                                "maximum": 1000,
                                "description": "Maximum number of matches to return (default: 100)"
                            }
                        },
                        "required": ["pattern"]
                    })),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
            ];

            Ok(ListToolsResult {
                tools,
                next_cursor: None,
                meta: None,
            })
        }
    }

    #[instrument(skip(self, _context), fields(request_id = %resources::generate_request_id(), tool = %request.name))]
    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        let client = Arc::clone(&self.client);

        async move {
            let tool_name = request.name.as_ref();
            let args = &request.arguments;

            match tool_name {
                "fetch_node_config" => {
                    let node = args
                        .as_ref()
                        .and_then(|a| a.get("node"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'node'.\n\
                                 [Context] Tool 'fetch_node_config' requires a node name.\n\
                                 [Next Step] Provide a valid node name parameter.",
                                None,
                            )
                        })?;

                    let result = tools::fetch_node_config(&client, node)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize result: {}", e),
                            None,
                        )
                    })?;

                    Ok(CallToolResult {
                        content: vec![Content::text(json)],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                "prioritize_node" => {
                    let node = args
                        .as_ref()
                        .and_then(|a| a.get("node"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'node'.\n\
                                 [Context] Tool 'prioritize_node' requires a node name.\n\
                                 [Next Step] Provide a valid node name parameter.",
                                None,
                            )
                        })?;

                    let result = tools::prioritize_node(&client, node)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize result: {}", e),
                            None,
                        )
                    })?;

                    Ok(CallToolResult {
                        content: vec![Content::text(json)],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                "reload_sources" => {
                    // No parameters needed for this tool
                    let result = tools::reload_sources(&client)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    let json = serde_json::to_string_pretty(&result).map_err(|e| {
                        McpError::new(
                            ErrorCode::INTERNAL_ERROR,
                            format!("Failed to serialize result: {}", e),
                            None,
                        )
                    })?;

                    Ok(CallToolResult {
                        content: vec![Content::text(json)],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                "diff_configs" => {
                    let node = args
                        .as_ref()
                        .and_then(|a| a.get("node"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'node'.\n\
                                 [Context] Tool 'diff_configs' requires a node name.\n\
                                 [Next Step] Provide a valid node name parameter.",
                                None,
                            )
                        })?;

                    let version1 = args
                        .as_ref()
                        .and_then(|a| a.get("version1"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'version1'.\n\
                                 [Context] Tool 'diff_configs' requires the first version OID.\n\
                                 [Next Step] Provide a valid version1 OID parameter.",
                                None,
                            )
                        })?;

                    let version2 = args
                        .as_ref()
                        .and_then(|a| a.get("version2"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'version2'.\n\
                                 [Context] Tool 'diff_configs' requires the second version OID.\n\
                                 [Next Step] Provide a valid version2 OID parameter.",
                                None,
                            )
                        })?;

                    let result = tools::diff_configs(&client, node, version1, version2)
                        .await
                        .map_err(Self::to_mcp_error)?;

                    // Return both the LLM-friendly format and the structured JSON
                    let llm_output = result.to_llm_format();

                    Ok(CallToolResult {
                        content: vec![Content::text(llm_output)],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                "search_configs" => {
                    let pattern = args
                        .as_ref()
                        .and_then(|a| a.get("pattern"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            McpError::new(
                                ErrorCode::INVALID_PARAMS,
                                "[Error] Missing required parameter 'pattern'.\n\
                                 [Context] Tool 'search_configs' requires a regex pattern.\n\
                                 [Next Step] Provide a valid regex pattern parameter.",
                                None,
                            )
                        })?;

                    // Optional: nodes array
                    let nodes: Option<Vec<String>> = args
                        .as_ref()
                        .and_then(|a| a.get("nodes"))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        });

                    // Optional: case_sensitive (default: true)
                    let case_sensitive = args
                        .as_ref()
                        .and_then(|a| a.get("case_sensitive"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true);

                    // Optional: limit (default: 100, min: 1, max: 1000)
                    let limit = args
                        .as_ref()
                        .and_then(|a| a.get("limit"))
                        .and_then(|v| v.as_u64())
                        .map(|l| l.clamp(1, 1000) as u32)
                        .unwrap_or(100);

                    let result =
                        tools::search_configs(&client, pattern, nodes, case_sensitive, limit)
                            .await
                            .map_err(Self::to_mcp_error)?;

                    let llm_output = result.to_llm_format();

                    Ok(CallToolResult {
                        content: vec![Content::text(llm_output)],
                        structured_content: None,
                        is_error: Some(false),
                        meta: None,
                    })
                }
                _ => Err(McpError::new(
                    ErrorCode::METHOD_NOT_FOUND,
                    format!(
                        "[Error] Unknown tool: '{}'\n\
                         [Context] Attempted to call a tool that does not exist.\n\
                         [Suggestions] Available tools: fetch_node_config, prioritize_node, reload_sources, diff_configs, search_configs\n\
                         [Next Step] Use one of the available tool names.",
                        tool_name
                    ),
                    None,
                )),
            }
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
    let server = OxidizedServer::new(config);

    info!("MCP server initialized, starting stdio transport");
    info!(
        "Resources available: oxidized://nodes, oxidized://node/{{name}}, oxidized://node/{{name}}/config, oxidized://node/{{name}}/versions, oxidized://stats"
    );
    info!(
        "Tools available: fetch_node_config, prioritize_node, reload_sources, diff_configs, search_configs"
    );

    // Run the server with stdio transport
    // The serve() call returns a running service that we must keep alive with waiting()
    let service = match server.serve(rmcp::transport::stdio()).await {
        Ok(s) => s,
        Err(e) => {
            error!("Server error: {}", e);
            std::process::exit(1);
        }
    };

    // Wait for the service to complete (keeps the connection alive)
    if let Err(e) = service.waiting().await {
        error!("Service error: {}", e);
        std::process::exit(1);
    }

    info!("mcp-oxidized server shutting down");
}
