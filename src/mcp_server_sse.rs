use std::sync::{Arc, OnceLock};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::prompt::PromptRouter, tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, InitializeRequestParams, InitializeResult, Meta},
    prompt, prompt_handler, schemars, prompt_router,
    service::RequestContext,
    task_handler, tool, tool_handler, tool_router,
    transport::{
        StreamableHttpServerConfig, StreamableHttpService,
        streamable_http_server::session::local::LocalSessionManager,
    },
};

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::{
    instagram_client::InstagramClient,
    models::{
        AccountInsight, InsightMetric, InsightPeriod, InstagramMedia, InstagramProfile,
        PublishMediaRequest, SendDmRequest, SendDmResponse,
    },
};

static INSTAGRAM_CLIENT: OnceLock<Arc<InstagramClient>> = OnceLock::new();

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
    image_url: Option<Value>,
    #[schemars(
        description = "URL of the video to publish (required if image_url is not provided)"
    )]
    video_url: Option<Value>,
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
    prompt_router: PromptRouter<InstagramMcpServerSse>,
}

#[tool_router]
impl InstagramMcpServerSse {

    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
        }
    }

    #[tool(
        description = "Get Instagram business profile information including followers, bio, and account details"
    )]
    async fn get_profile_info(
        &self,
        Parameters(GetProfileInfo { account_id }): Parameters<GetProfileInfo>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let profile_info: InstagramProfile = client.get_profile_info(account_id).await.unwrap();
        Ok(CallToolResult::structured(serde_json::json!(profile_info)))
    }

    #[tool(description = "Get recent media posts from Instagram account with engagement metrics")]
    async fn get_media_posts(
        &self,
        Parameters(GetMediaPosts {
            account_id,
            limit,
            after,
        }): Parameters<GetMediaPosts>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let media_posts: Vec<InstagramMedia> = client
            .get_media_posts(account_id, limit.unwrap_or(25), after)
            .await
            .unwrap();
        Ok(CallToolResult::structured(serde_json::json!(media_posts)))
    }

    #[tool(description = "Get detailed insights and analytics for a specific Instagram post")]
    async fn get_media_insights(
        &self,
        Parameters(GetMediaInsights { media_id, metrics }): Parameters<GetMediaInsights>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");

        let mapped_metrics: Option<Vec<InsightMetric>> = metrics.map(|m| {
            m.into_iter()
                .filter_map(|metric_str| match metric_str.as_str() {
                    "reach" => Some(InsightMetric::Reach),
                    "impressions" => Some(InsightMetric::Impressions),
                    "likes" => Some(InsightMetric::Likes),
                    "comments" => Some(InsightMetric::Comments),
                    "shares" => Some(InsightMetric::Shares),
                    "saved" => Some(InsightMetric::Saved),
                    "video_views" => Some(InsightMetric::VideoViews),
                    _ => None,
                })
                .collect()
        });

        let insights = client
            .get_media_insights(media_id, mapped_metrics)
            .await
            .unwrap();
        Ok(CallToolResult::structured(serde_json::json!(insights)))
    }

    #[tool(description = "Upload and publish an image or video to Instagram")]
    async fn publish_media(
        &self,
        Parameters(PublishMedia {
            image_url,
            video_url,
            caption,
            location_id,
        }): Parameters<PublishMedia>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let image_url_obj: Option<Url> = if let Some(url) = image_url {
            serde_json::from_value(url).unwrap()
        } else {
            None
        };
        let video_url_obj: Option<Url> = if let Some(url) = video_url {
            serde_json::from_value(url).unwrap()
        } else {
            None
        };

        let request = PublishMediaRequest {
            image_url: image_url_obj,
            video_url: video_url_obj,
            caption,
            location_id,
            user_tags: None,
        };
        let publish_result = client.publish_media(request).await.unwrap();
        Ok(CallToolResult::structured(serde_json::json!(
            publish_result
        )))
    }

    #[tool(description = "Get Facebook pages connected to the account")]
    async fn get_account_pages(
        &self,
        Parameters(GetAccountPages {}): Parameters<GetAccountPages>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let pages = client.get_account_pages().await.unwrap();
        Ok(CallToolResult::structured(serde_json::json!(pages)))
    }

    #[tool(description = "Get account-level insights and analytics")]
    async fn get_account_insights(
        &self,
        Parameters(GetAccountInsights {
            account_id,
            metrics,
            period,
        }): Parameters<GetAccountInsights>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let period_enum = if let Some(period_str) = period {
            match period_str.as_str() {
                "lifetime" => InsightPeriod::Lifetime,
                _ => InsightPeriod::Day,
            }
        } else {
            InsightPeriod::Day
        };
        let insights: Vec<AccountInsight> = client
            .get_account_insights(account_id, metrics, period_enum)
            .await
            .unwrap();
        Ok(CallToolResult::structured(
            serde_json::json!({ "insights": insights }),
        ))
    }

    #[tool(description = "Validate the Instagram API access token")]
    async fn validate_access_token(
        &self,
        Parameters(ValidateAccessToken {}): Parameters<ValidateAccessToken>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let valid: bool = client.validate_access_token().await;
        Ok(CallToolResult::structured(
            serde_json::json!({ "valid": valid }),
        ))
    }

    #[tool(
        description = "Get Instagram DM conversations (requires instagram_manage_messages Advanced Access)"
    )]
    async fn get_conversations(
        &self,
        Parameters(GetConversations { page_id, limit }): Parameters<GetConversations>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let conversations = client
            .get_conversations(page_id, limit.unwrap_or(25))
            .await
            .unwrap();
        Ok(CallToolResult::structured(serde_json::json!({
            "conversations": conversations,
            "count": conversations.len()
        })))
    }

    #[tool(description = "Get messages for a specific conversation")]
    async fn get_conversation_messages(
        &self,
        Parameters(GetConversationMessages {
            conversation_id,
            limit,
        }): Parameters<GetConversationMessages>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let messages = client
            .get_conversation_messages(conversation_id.clone(), limit.unwrap_or(25))
            .await
            .unwrap();
        Ok(CallToolResult::structured(serde_json::json!({
            "conversation_id": conversation_id,
            "messages": messages,
            "count": messages.len()
        })))
    }

    #[tool(description = "Send Instagram direct message (requires Advanced Access)")]
    async fn send_dm(
        &self,
        Parameters(SendDm {
            recipient_id,
            message,
        }): Parameters<SendDm>,
    ) -> Result<CallToolResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let request = SendDmRequest {
            recipient_id,
            message,
            message_type: None,
        };
        let response: SendDmResponse = client.send_dm(request).await.unwrap();
        Ok(CallToolResult::structured(serde_json::json!(response)))
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

#[prompt_router]
impl InstagramMcpServerSse {

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

pub async fn mcp_server_run_sse(client: Arc<InstagramClient>) -> Result<(), std::io::Error> {
    let _ = INSTAGRAM_CLIENT.set(Arc::clone(&client));

    let service = StreamableHttpService::new(
        || Ok(InstagramMcpServerSse::new()),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );

    let router = axum::Router::new().nest_service("/sse", service);
    let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:9091")
        .await?;
    let _ = axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
        })
        .await?;
    Ok(())
}
