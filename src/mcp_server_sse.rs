use std::sync::{Arc, OnceLock};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, handler::server::{router::prompt::PromptRouter, tool::ToolRouter, wrapper::Parameters}, model::{
        CallToolResult, ContentBlock, Implementation, InitializeRequestParams, InitializeResult, ListResourceTemplatesResult, ListResourcesResult, MetaObject, PaginatedRequestParams, PromptMessage, ProtocolVersion, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, Role, ServerCapabilities, ServerInfo,
    }, prompt, prompt_router, schemars, service::RequestContext, tool, tool_handler, tool_router, transport::{
        StreamableHttpServerConfig, StreamableHttpService,
        streamable_http_server::session::local::{LocalSessionManager, SessionConfig},
    },
};

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use crate::{
    instagram_client::InstagramClient,
    models::{
        FacebookPage, InsightMetric, InsightPeriod, InstagramMedia, InstagramProfile, MediaInsight,
        PublishMediaRequest, PublishMediaResponse, SendDmRequest, SendDmResponse,
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
        let profile_info_rs = client.get_profile_info(account_id).await;
        if let Err(e) = profile_info_rs {
            return Err(McpError::internal_error(
                "get_profile_info failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let profile_info = profile_info_rs.unwrap();
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
        let media_posts_rs = client
            .get_media_posts(account_id, limit.unwrap_or(25), after)
            .await;

        if let Err(e) = media_posts_rs {
            return Err(McpError::internal_error(
                "get_media_posts failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let media_posts: Vec<InstagramMedia> = media_posts_rs.unwrap();
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

        let insights_rs = client.get_media_insights(media_id, mapped_metrics).await;
        if let Err(e) = insights_rs {
            return Err(McpError::internal_error(
                "get_media_insights failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let insights: Vec<MediaInsight> = insights_rs.unwrap();
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
        let publish_result_rs = client.publish_media(request).await;

        if let Err(e) = publish_result_rs {
            return Err(McpError::internal_error(
                "publish_media failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let publish_result: PublishMediaResponse = publish_result_rs.unwrap();

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
        let pages_rs = client.get_account_pages().await;

        if let Err(e) = pages_rs {
            return Err(McpError::internal_error(
                "get_account_pages failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let pages: Vec<FacebookPage> = pages_rs.unwrap();

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
        let insights_rs = client
            .get_account_insights(account_id, metrics, period_enum)
            .await;
        if let Err(e) = insights_rs {
            return Err(McpError::internal_error(
                "get_account_insights failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }

        Ok(CallToolResult::structured(
            serde_json::json!({ "insights": insights_rs.unwrap() }),
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
        let conversations_rs = client.get_conversations(page_id, limit.unwrap_or(25)).await;

        if let Err(e) = conversations_rs {
            return Err(McpError::internal_error(
                "get_conversations failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let conversations = conversations_rs.unwrap();
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
        let messages_rs = client
            .get_conversation_messages(conversation_id.clone(), limit.unwrap_or(25))
            .await;

        if let Err(e) = messages_rs {
            return Err(McpError::internal_error(
                "get_conversation_messages failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let messages = messages_rs.unwrap();

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
        let response_rs = client.send_dm(request).await;

        if let Err(e) = response_rs {
            return Err(McpError::internal_error(
                "send_dm failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let response: SendDmResponse = response_rs.unwrap();

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
            Some(id) => Ok(CallToolResult::success(vec![ContentBlock::text(id)])),
            None => Ok(CallToolResult::success(vec![ContentBlock::text(
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

        let insights_rs = client.get_media_insights(media_id.to_string(), None).await;
        if let Err(e) = insights_rs {
            return Err(McpError::internal_error(
                "get_media_insights failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }

        let insights = insights_rs.unwrap();
        let prompt = format!(
            "Analyze the engagement metrics for Instagram post {}:

            Insights Data:
            {}

            Please provide:
            1. Overall engagement performance assessment
            2. Key metrics analysis (impressions, reach, likes, comments, shares)
            3. Engagement rate calculation and interpretation
            4. Recommendations for improving future posts
            5. Comparison with typical performance benchmarks",
            media_id,
            serde_json::to_string_pretty(&insights).unwrap_or_else(|_| "[]".to_string())
        );
        Ok(vec![PromptMessage::new_text(Role::User, prompt)])
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
        let posts_rs = client.get_media_posts(None, 20, None).await;

        if let Err(e) = posts_rs {
            return Err(McpError::internal_error(
                "get_media_posts failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let posts = posts_rs.unwrap();

        let account_insights_rs = client
            .get_account_insights(None, None, InsightPeriod::Day)
            .await;

        if let Err(e) = account_insights_rs {
            return Err(McpError::internal_error(
                "get_account_insights failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let account_insights = account_insights_rs.unwrap();

        let preview_posts = posts.into_iter().take(5).collect::<Vec<_>>();
        let focus_area_str = focus_area.clone().unwrap_or("engagement".to_string());
        let time_period_str = time_period.clone().unwrap_or("week".to_string());

        let prompt = format!(
            r#"Generate a content strategy for Instagram focusing on {} over the {}:

            Recent Posts Performance:
            {}

            Account Insights:
            {}

            Please provide:
            1. Content performance analysis
            2. Optimal posting times and frequency
            3. Content type recommendations (images, videos, carousels)
            4. Caption and hashtag strategies
            5. Engagement tactics to improve {}
            6. Specific action items for the next {}"#,
            focus_area_str,
            time_period_str,
            serde_json::to_string_pretty(&preview_posts).unwrap_or_else(|_| "[]".to_string()),
            serde_json::to_string_pretty(&account_insights).unwrap_or_else(|_| "[]".to_string()),
            focus_area_str,
            time_period_str,
        );

        Ok(vec![PromptMessage::new_text(Role::User, prompt)])
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

        let posts_rs = client.get_media_posts(None, post_count_value, None).await;

        if let Err(e) = posts_rs {
            return Err(McpError::internal_error(
                "get_media_posts failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        let posts = posts_rs.unwrap();

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
            r#"Analyze hashtag performance for the last {} Instagram posts:

            Hashtag Data:
            {}

            Please provide:
            1. Most frequently used hashtags
            2. Hashtag performance correlation with engagement
            3. Hashtag diversity analysis
            4. Recommendations for hashtag optimization
            5. Suggested new hashtags to try
            6. Hashtag strategy improvements"#,
            post_count_value,
            serde_json::to_string_pretty(&hashtags_data).unwrap_or_else(|_| "[]".to_string())
        );

        Ok(vec![PromptMessage::new_text(Role::User, prompt)])
    }
}

#[tool_handler(meta = MetaObject(rmcp::object!({"tool_meta_key": "tool_meta_value"})))]
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
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_instructions(
            r#"This server provides tools and prompts to interact with instagram.
               Tools: get_profile_info, get_media_posts, get_media_insights, publish_media, get_account_pages, get_account_insights, validate_access_token, get_conversations, get_conversation_messages, send_dm
               Prompts: analyze_engagement, content_strategy, hashtag_analysis"#.to_string()
        )
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
                Resource::new("instagram://profile", "Instagram Profile")
                    .with_description("Current Instagram business profile information")
                    .with_mime_type("application/json"),
                Resource::new("instagram://media/recent", "Recent Media Posts")
                    .with_description("Recent Instagram posts with engagement metrics")
                    .with_mime_type("application/json"),
                Resource::new("instagram://insights/account", "Account Insights")
                    .with_description("Account-level analytics and insights")
                    .with_mime_type("application/json"),
                Resource::new("instagram://pages", "Connected Pages")
                    .with_description("Facebook pages connected to the account")
                    .with_mime_type("application/json"),
            ],
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
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
                )]).into())
            }
            "instagram://media/recent" => {
                let media_posts: Vec<InstagramMedia> =
                    client.get_media_posts(None, 25, None).await.unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&media_posts).unwrap(),
                    uri,
                )]).into())
            }
            "instagram://insights/account" => {
                let account_insights = client
                    .get_account_insights(None, None, InsightPeriod::Day)
                    .await
                    .unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&account_insights).unwrap(),
                    uri,
                )]).into())
            }
            "instagram://pages" => {
                let pages = client.get_account_pages().await.unwrap();
                Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                    serde_json::to_string(&pages).unwrap(),
                    uri,
                )]).into())
            }
            _ => Err(McpError::resource_not_found(
                "resource not found",
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
            ..Default::default()
        })
    }
}

pub async fn mcp_server_run_sse(client: Arc<InstagramClient>) -> Result<(), std::io::Error> {
    let _ = INSTAGRAM_CLIENT.set(Arc::clone(&client));

    let mut session_config = SessionConfig::default();
    session_config.keep_alive = Some(std::time::Duration::from_hours(60));

    let mut session_manager = LocalSessionManager::default();
    session_manager.session_config = session_config;

    let service = StreamableHttpService::new(
        || Ok(InstagramMcpServerSse::new()),
        session_manager.into(),
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
