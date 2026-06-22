use std::sync::{Arc, Mutex, OnceLock};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::prompt::PromptRouter, tool::ToolRouter, wrapper::Parameters},
    model::{
        AnnotateAble, CallToolResult, Content, Implementation, InitializeRequestParams,
        InitializeResult, ListResourceTemplatesResult, ListResourcesResult, Meta,
        PaginatedRequestParams, PromptMessage, PromptMessageRole, ProtocolVersion, RawResource,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents, ServerCapabilities,
        ServerInfo,
    },
    prompt, prompt_router, schemars,
    service::RequestContext,
    task_manager::OperationProcessor,
    tool, tool_handler, tool_router,
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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnalyzeEngagementPrompt {
    #[schemars(description = "Instagram media ID to analyze")]
    media_id: String,
    #[schemars(description = "Period to compare against (e.g., last_week, last_month)")]
    comparison_period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ContentStrategyPrompt {
    #[schemars(description = "Area to focus on (e.g., engagement, reach, growth)")]
    focus_area: Option<String>,
    #[schemars(description = "Time period to analyze (e.g., week, month)")]
    time_period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct HashtagAnalysisPrompt {
    #[schemars(description = "Number of recent posts to analyze")]
    post_count: Option<u32>,
}

#[derive(Clone)]
struct InstagramMcpServerSse {
    tool_router: ToolRouter<InstagramMcpServerSse>,
    prompt_router: PromptRouter<InstagramMcpServerSse>,
    processor: Arc<Mutex<OperationProcessor>>,
}

#[tool_router]
impl InstagramMcpServerSse {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            processor: Arc::new(Mutex::new(OperationProcessor::new())),
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
    #[prompt(
        name = "analyze_engagement",
        description = "Analyze Instagram post engagement and provide insights"
    )]
    async fn analyze_engagement(
        &self,
        Parameters(AnalyzeEngagementPrompt { media_id, .. }): Parameters<AnalyzeEngagementPrompt>,
        _context: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");

        let insights = client
            .get_media_insights(media_id.to_string(), None)
            .await
            .unwrap();
        let prompt = format!(
            "Analyze the engagement metrics for Instagram post {}:\n\nInsights Data:\n{}\n\nPlease provide:\n1. Overall engagement performance assessment\n2. Key metrics analysis (impressions, reach, likes, comments, shares)\n3. Engagement rate calculation and interpretation\n4. Recommendations for improving future posts\n5. Comparison with typical performance benchmarks",
            media_id,
            serde_json::to_string_pretty(&insights).unwrap_or_else(|_| "[]".to_string())
        );
        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt,
        )])
    }

    #[prompt(
        name = "content_strategy",
        description = "Generate content strategy recommendations based on account performance"
    )]
    async fn content_strategy(
        &self,
        Parameters(ContentStrategyPrompt {
            focus_area,
            time_period,
        }): Parameters<ContentStrategyPrompt>,
        _context: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let posts = client.get_media_posts(None, 20, None).await.unwrap();
        let account_insights = client
            .get_account_insights(None, None, InsightPeriod::Day)
            .await
            .unwrap();
        let preview_posts = posts.into_iter().take(5).collect::<Vec<_>>();
        let focus_area_str = focus_area.clone().unwrap_or("engagement".to_string());
        let time_period_str = time_period.clone().unwrap_or("week".to_string());

        let prompt = format!(
            "Generate a content strategy for Instagram focusing on {} over the {}:\n\nRecent Posts Performance:\n{}\n\nAccount Insights:\n{}\n\nPlease provide:\n1. Content performance analysis\n2. Optimal posting times and frequency\n3. Content type recommendations (images, videos, carousels)\n4. Caption and hashtag strategies\n5. Engagement tactics to improve {}\n6. Specific action items for the next {}",
            focus_area_str,
            time_period_str,
            serde_json::to_string_pretty(&preview_posts).unwrap_or_else(|_| "[]".to_string()),
            serde_json::to_string_pretty(&account_insights).unwrap_or_else(|_| "[]".to_string()),
            focus_area_str,
            time_period_str,
        );

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt,
        )])
    }

    #[prompt(
        name = "hashtag_analysis",
        description = "Analyze hashtag performance and suggest improvements"
    )]
    async fn hashtag_analysis(
        &self,
        Parameters(HashtagAnalysisPrompt { post_count }): Parameters<HashtagAnalysisPrompt>,
        _context: RequestContext<RoleServer>,
    ) -> Result<Vec<PromptMessage>, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let post_count_value: u32 = post_count.unwrap_or(10);

        let posts: Vec<InstagramMedia> = client
            .get_media_posts(None, post_count_value, None)
            .await
            .unwrap();
        let hashtags_data: Vec<Value> = posts
            .into_iter()
            .filter_map(|post| {
                let caption = post.caption?;
                let hashtags = caption
                    .split_whitespace()
                    .filter(|word| word.starts_with('#'))
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                Some(serde_json::json!({
                    "post_id": post.id,
                    "hashtags": hashtags,
                    "likes": post.like_count,
                    "comments": post.comments_count,
                }))
            })
            .collect::<Vec<_>>();
        let prompt = format!(
            "Analyze hashtag performance for the last {} Instagram posts:\n\nHashtag Data:\n{}\n\nPlease provide:\n1. Most frequently used hashtags\n2. Hashtag performance correlation with engagement\n3. Hashtag diversity analysis\n4. Recommendations for hashtag optimization\n5. Suggested new hashtags to try\n6. Hashtag strategy improvements",
            post_count_value,
            serde_json::to_string_pretty(&hashtags_data).unwrap_or_else(|_| "[]".to_string())
        );

        Ok(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            prompt,
        )])
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

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
            .enable_prompts()
            .enable_resources()
            .enable_tools()
            .build()
        )
        .with_server_info(Implementation::from_build_env())
        .with_protocol_version(ProtocolVersion::V_2024_11_05)
        .with_instructions(r#"This server provides tools and prompts to interact with instagram.
         Tools: get_profile_info, get_media_posts, get_media_insights, publish_media, get_account_pages, get_account_insights, validate_access_token, get_conversations, get_conversation_messages, send_dm
         Prompts: analyze_engagement, content_strategy, hashtag_analysis
         "#.to_string())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            meta: None,
            next_cursor: None,
            resources: vec![
                RawResource::new("instagram://profile", "Instagram Profile")
                    .with_description("Current Instagram business profile information")
                    .with_mime_type("application/json")
                    .no_annotation(),
                RawResource::new("instagram://media/recent", "Recent Media Posts")
                    .with_description("Recent Instagram posts with engagement metrics")
                    .with_mime_type("application/json")
                    .no_annotation(),
                RawResource::new("instagram://insights/account", "Account Insights")
                    .with_description("Account-level analytics and insights")
                    .with_mime_type("application/json")
                    .no_annotation(),
                RawResource::new("instagram://pages", "Connected Pages")
                    .with_description("Facebook pages connected to the account")
                    .with_mime_type("application/json")
                    .no_annotation(),
            ],
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let client = INSTAGRAM_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let uri = &request.uri;
        match uri.as_str() {
            "instagram://profile" => {
                let profile_info: InstagramProfile = client.get_profile_info(None).await.unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&profile_info).unwrap(),
                    uri,
                )]))
            }
            "instagram://media/recent" => {
                let media_posts: Vec<InstagramMedia> =
                    client.get_media_posts(None, 25, None).await.unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&media_posts).unwrap(),
                    uri,
                )]))
            }
            "instagram://insights/account" => {
                let account_insights = client
                    .get_account_insights(None, None, InsightPeriod::Day)
                    .await
                    .unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&account_insights).unwrap(),
                    uri,
                )]))
            }
            "instagram://pages" => {
                let pages = client.get_account_pages().await.unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&pages).unwrap(),
                    uri,
                )]))
            }
            _ => Err(McpError::resource_not_found(
                "resource_not_found",
                Some(serde_json::json!({"uri": uri})),
            )),
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: Vec::new(),
            meta: None,
        })
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
    let tcp_listener = tokio::net::TcpListener::bind("127.0.0.1:9091").await?;
    let _ = axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async move {
            tokio::signal::ctrl_c().await.unwrap();
        })
        .await?;
    Ok(())
}
