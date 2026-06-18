use std::sync::{Arc, OnceLock};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, InitializeRequestParams, InitializeResult, Meta},
    prompt, prompt_handler, schemars,
    service::RequestContext,
    task_handler, tool, tool_handler, tool_router,
    transport::{
        StreamableHttpServerConfig, StreamableHttpService,
        streamable_http_server::session::local::LocalSessionManager,
    },
};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    config::Settings,
    error::AppError,
    instagram_client::InstagramClient,
    models::{
        AccountInsight, InsightMetric, InsightPeriod, InstagramMedia, InstagramProfile,
        PublishMediaRequest, SendDmRequest, SendDmResponse,
    },
};

static INSTAGRAM_CLIENT: OnceLock<Arc<InstagramClient>> = OnceLock::new();
/*
fn get_profile_info() -> Tool {
    Tool {
            name: "get_profile_info".to_string(),
            description: Some("Get Instagram business profile information including followers, bio, and account details".to_string()),
            input_schema: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "account_id": { "type": "string", "description": "Instagram business account ID (optional)" }
                        }
            }),
            annotations: None
        }
}

fn get_media_posts() -> Tool {
    Tool {
        name: "get_media_posts".to_string(),
        description: Some(
            "Get recent media posts from Instagram account with engagement metrics".to_string(),
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "account_id": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 },
                "after": { "type": "string" }
            }
        }),
        annotations: None,
    }
}

fn get_media_insights() -> Tool {
    Tool {
        name: "get_media_insights".to_string(),
        description: Some(
            "Get detailed insights and analytics for a specific Instagram post".to_string(),
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "media_id": { "type": "string" },
                "metrics": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["reach", "likes", "comments", "shares", "saved", "video_views"] }
                }
            },
            "required": ["media_id"]
        }),
        annotations: None,
    }
}

fn publish_media() -> Tool {
    Tool {
        name: "publish_media".to_string(),
        description: Some("Upload and publish an image or video to Instagram".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "image_url": { "type": "string", "format": "uri" },
                "video_url": { "type": "string", "format": "uri" },
                "caption": { "type": "string" },
                "location_id": { "type": "string" }
            },
            "anyOf": [{"required": ["image_url"]}, {"required": ["video_url"]}]
        }),
        annotations: None,
    }
}

fn get_account_pages() -> Tool {
    Tool {
        name: "get_account_pages".to_string(),
        description: Some("Get Facebook pages connected to the account".to_string()),
        input_schema: serde_json::json!({"type": "object", "properties": {}}),
        annotations: None,
    }
}

fn get_account_insights() -> Tool {
    Tool {
        name: "get_account_insights".to_string(),
        description: Some("Get account-level insights and analytics".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "account_id": { "type": "string" },
                "metrics": { "type": "array", "items": { "type": "string" } },
                "period": { "type": "string", "enum": ["day", "lifetime"], "default": "day" }
            }
        }),
        annotations: None,
    }
}

fn validate_access_token() -> Tool {
    Tool {
        name: "validate_access_token".to_string(),
        description: Some("Validate the Instagram API access token".to_string()),
        input_schema: serde_json::json!({"type": "object", "properties": {}}),
        annotations: None,
    }
}

fn get_conversations() -> Tool {
    Tool {
        name: "get_conversations".to_string(),
        description: Some(
            "Get Instagram DM conversations (requires instagram_manage_messages Advanced Access)"
                .to_string(),
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "page_id": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }
            }
        }),
        annotations: None,
    }
}

fn get_conversation_messages() -> Tool {
    Tool {
        name: "get_conversation_messages".to_string(),
        description: Some("Get messages for a specific conversation".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "conversation_id": { "type": "string" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }
            },
            "required": ["conversation_id"]
        }),
        annotations: None,
    }
}

fn send_dm() -> Tool {
    Tool {
        name: "send_dm".to_string(),
        description: Some("Send Instagram direct message (requires Advanced Access)".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "recipient_id": { "type": "string" },
                "message": { "type": "string", "maxLength": 1000 }
            },
            "required": ["recipient_id", "message"]
        }),
        annotations: None,
    }
}
*/

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetProfileInfo {
    #[schemars(description = "Instagram business account ID (optional)")]
    account_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetMediaPosts {
    #[schemars(description = "Instagram business account ID (optional)")]
    account_id: Option<String>,
    #[schemars(
        description = "Maximum number of media posts to retrieve (minimum = 1, maximum = 100, default = 25)"
    )]
    limit: Option<u32>,
    after: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetMediaInsights {
    #[schemars(description = "Instagram media ID to retrieve insights for")]
    media_id: String,
    #[schemars(
        description = "List of metrics to retrieve insights for (e.g. reach, likes, comments, shares, saved, video_views)"
    )]
    metrics: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PublishMedia {
    #[schemars(
        description = "URL of the image to publish (required if video_url is not provided)"
    )]
    image_url: Option<String>,
    #[schemars(
        description = "URL of the video to publish (required if image_url is not provided)"
    )]
    video_url: Option<String>,
    #[schemars(description = "Caption for the media post")]
    caption: Option<String>,
    #[schemars(description = "Location ID to tag in the media post")]
    location_id: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetAccountPages {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetAccountInsights {
    #[schemars(description = "Instagram business account ID to retrieve insights for")]
    account_id: Option<String>,
    #[schemars(
        description = "List of metrics to retrieve insights for (e.g. followers_count, impressions, reach)"
    )]
    metrics: Option<Vec<String>>,
    #[schemars(description = "Time period for insights data (day, lifetime - default: day)")]
    period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ValidateAccessToken {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetConversations {
    #[schemars(description = "Facebook page ID to retrieve conversations for")]
    page_id: Option<String>,
    #[schemars(
        description = "Maximum number of conversations to retrieve (minimum = 1, maximum = 100, default = 25)"
    )]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetConversationMessages {
    #[schemars(description = "Instagram DM conversation ID to retrieve messages for")]
    conversation_id: String,
    #[schemars(
        description = "Maximum number of messages to retrieve (minimum = 1, maximum = 100, default = 25)"
    )]
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendDm {
    #[schemars(description = "Recipient Instagram user ID to send the DM to")]
    recipient_id: String,
    #[schemars(
        description = "Message content to send in the DM (maximum length: 1000 characters)"
    )]
    message: String,
}

#[derive(Debug, Clone)]
struct InstagramMcpServerSse {
    tool_router: ToolRouter<InstagramMcpServerSse>,
}

#[tool_router]
impl InstagramMcpServerSse {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Get Instagram business profile information including followers, bio, and account details"
    )]
    async fn get_profile_info(
        &self,
        Parameters(GetProfileInfo { account_id }): Parameters<GetProfileInfo>,
    ) -> String {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let profile_info = client.get_profile_info(account_id).await.unwrap();
        serde_json::json!(profile_info).to_string()
    }

    /// Returns the `Mcp-Session-Id` of the current session (streamable HTTP only).
    #[tool(description = "Get the session ID for this connection")]
    fn get_session_id(&self, ctx: RequestContext<RoleServer>) -> Result<CallToolResult, McpError> {
        let session_id = ctx
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.headers.get("mcp-session-id"))
            .map(|v| v.to_str().unwrap_or("(non-ascii)").to_owned());
        match session_id {
            Some(id) => Ok(CallToolResult::success(vec![Content::text(id)])),
            None => Ok(CallToolResult::success(vec![Content::text(
                "No session (not running over streamable HTTP?)",
            )])),
        }
    }
}

#[tool_handler(meta = Meta(rmcp::object!({"tool_meta_key": "tool_meta_value"})))]
impl ServerHandler for InstagramMcpServerSse {
    async fn initialize(
        &self,
        _request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if let Some(http_request_part) = context.extensions.get::<axum::http::request::Parts>() {
            let initialize_headers = &http_request_part.headers;
            let initialize_uri = &http_request_part.uri;
            tracing::info!(?initialize_headers, %initialize_uri, "initialize from http server");
        }
        Ok(self.get_info())
    }
}

pub async fn mcp_server_run_sse(client: Arc<InstagramClient>) {
    let _ = INSTAGRAM_CLIENT.set(Arc::clone(&client));

    let service = StreamableHttpService::new(
        || Ok(InstagramMcpServerSse::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new().nest_service("/sse", service);
    let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:9091")
        .await
        .unwrap();
    let _ = axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
        })
        .await
        .unwrap();
}
