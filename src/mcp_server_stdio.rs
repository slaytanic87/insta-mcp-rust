use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info};

use crate::config::Settings;
use crate::error::AppError;
use crate::instagram_client::InstagramClient;
use crate::models::{
    InsightMetric, InsightPeriod, InstagramMedia, McpToolResult, PromptArgument, PromptDefinition,
    PublishMediaRequest, ResourceDefinition, SendDmRequest, SendDmResponse, ToolDefinition,
};

#[derive(Debug, Deserialize)]
struct RpcRequest {
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Option<Value>,
    result: Option<Value>,
    error: Option<Value>,
}

pub struct InstagramMcpServer {
    settings: Arc<Settings>,
    client: Arc<InstagramClient>,
}

impl InstagramMcpServer {
    pub fn new(settings: Arc<Settings>, client: Arc<InstagramClient>) -> Self {
        Self { settings, client }
    }

    pub async fn run_stdio(self) -> Result<(), AppError> {
        info!(
            server_name = %self.settings.mcp_server_name,
            version = %self.settings.mcp_server_version,
            "Starting Instagram MCP server over stdio"
        );

        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();
        let mut writer = tokio::io::BufWriter::new(stdout);

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            let request: Result<RpcRequest, _> = serde_json::from_str(&line);
            let response = match request {
                Ok(req) => self.handle_request(req).await,
                Err(e) => RpcResponse {
                    jsonrpc: "2.0",
                    id: None,
                    result: None,
                    error: Some(json!({
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    })),
                },
            };

            let output = serde_json::to_string(&response)?;
            writer.write_all(output.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }

        Ok(())
    }

    async fn handle_request(&self, request: RpcRequest) -> RpcResponse {
        let id = request.id.clone();

        let result = match request.method.as_str() {
            "initialize" => Ok(json!({
                "server_name": self.settings.mcp_server_name,
                "server_version": self.settings.mcp_server_version,
                "capabilities": {
                    "tools": true,
                    "resources": true,
                    "prompts": true
                }
            })),
            "tools/list" => {
                Ok(serde_json::to_value(self.list_tools()).unwrap_or_else(|_| json!([])))
            }
            "tools/call" => {
                self.call_tool(request.params.unwrap_or_else(|| json!({})))
                    .await
            }
            "resources/list" => {
                Ok(serde_json::to_value(self.list_resources()).unwrap_or_else(|_| json!([])))
            }
            "resources/read" => {
                self.read_resource(request.params.unwrap_or_else(|| json!({})))
                    .await
            }
            "prompts/list" => {
                Ok(serde_json::to_value(self.list_prompts()).unwrap_or_else(|_| json!([])))
            }
            "prompts/get" => {
                self.get_prompt(request.params.unwrap_or_else(|| json!({})))
                    .await
            }
            method => Err(AppError::Validation(format!("Unknown method: {}", method))),
        };

        match result {
            Ok(value) => RpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(value),
                error: None,
            },
            Err(e) => {
                error!(error = %e, "Request handling failed");
                RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({ "code": -32000, "message": e.to_string() })),
                }
            }
        }
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "get_profile_info".to_string(),
                description: "Get Instagram business profile information including followers, bio, and account details".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "account_id": { "type": "string", "description": "Instagram business account ID (optional)" }
                    }
                }),
            },
            ToolDefinition {
                name: "get_media_posts".to_string(),
                description: "Get recent media posts from Instagram account with engagement metrics".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "account_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 },
                        "after": { "type": "string" }
                    }
                }),
            },
            ToolDefinition {
                name: "get_media_insights".to_string(),
                description: "Get detailed insights and analytics for a specific Instagram post".to_string(),
                input_schema: json!({
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
            },
            ToolDefinition {
                name: "publish_media".to_string(),
                description: "Upload and publish an image or video to Instagram".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "image_url": { "type": "string", "format": "uri" },
                        "video_url": { "type": "string", "format": "uri" },
                        "caption": { "type": "string" },
                        "location_id": { "type": "string" }
                    },
                    "anyOf": [{"required": ["image_url"]}, {"required": ["video_url"]}]
                }),
            },
            ToolDefinition {
                name: "get_account_pages".to_string(),
                description: "Get Facebook pages connected to the account".to_string(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            ToolDefinition {
                name: "get_account_insights".to_string(),
                description: "Get account-level insights and analytics".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "account_id": { "type": "string" },
                        "metrics": { "type": "array", "items": { "type": "string" } },
                        "period": { "type": "string", "enum": ["day", "lifetime"], "default": "day" }
                    }
                }),
            },
            ToolDefinition {
                name: "validate_access_token".to_string(),
                description: "Validate the Instagram API access token".to_string(),
                input_schema: json!({"type": "object", "properties": {}}),
            },
            ToolDefinition {
                name: "get_conversations".to_string(),
                description: "Get Instagram DM conversations (requires instagram_manage_messages Advanced Access)".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "page_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }
                    }
                }),
            },
            ToolDefinition {
                name: "get_conversation_messages".to_string(),
                description: "Get messages for a specific conversation".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "conversation_id": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 25 }
                    },
                    "required": ["conversation_id"]
                }),
            },
            ToolDefinition {
                name: "send_dm".to_string(),
                description: "Send Instagram direct message (requires Advanced Access)".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "recipient_id": { "type": "string" },
                        "message": { "type": "string", "maxLength": 1000 }
                    },
                    "required": ["recipient_id", "message"]
                }),
            },
        ]
    }

    async fn call_tool(&self, params: Value) -> Result<Value, AppError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("Missing tool name".to_string()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        let now = Utc::now().to_rfc3339();
        let mut metadata = HashMap::new();
        metadata.insert("tool".to_string(), json!(name));
        metadata.insert("timestamp".to_string(), json!(now));

        let run = async {
            match name {
                "get_profile_info" => {
                    let account_id = arguments
                        .get("account_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let profile = self.client.get_profile_info(account_id).await?;
                    Ok(json!(profile))
                }
                "get_media_posts" => {
                    let account_id = arguments
                        .get("account_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
                    let after = arguments
                        .get("after")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let posts: Vec<InstagramMedia> = self
                        .client
                        .get_media_posts(account_id, limit, after)
                        .await?;
                    Ok(json!({ "posts": posts, "count": posts.len() }))
                }
                "get_media_insights" => {
                    let media_id = arguments
                        .get("media_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| AppError::Validation("media_id is required".to_string()))?
                        .to_string();

                    let metrics = arguments
                        .get("metrics")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(Value::as_str)
                                .filter_map(|s| match s {
                                    "impressions" => Some(InsightMetric::Impressions),
                                    "reach" => Some(InsightMetric::Reach),
                                    "likes" => Some(InsightMetric::Likes),
                                    "comments" => Some(InsightMetric::Comments),
                                    "shares" => Some(InsightMetric::Shares),
                                    "saved" => Some(InsightMetric::Saved),
                                    "video_views" => Some(InsightMetric::VideoViews),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                        });

                    let insights = self
                        .client
                        .get_media_insights(media_id.clone(), metrics)
                        .await?;
                    Ok(json!({ "media_id": media_id, "insights": insights }))
                }
                "publish_media" => {
                    let request: PublishMediaRequest = serde_json::from_value(arguments)?;
                    let response = self.client.publish_media(request).await?;
                    Ok(json!(response))
                }
                "get_account_pages" => {
                    let pages = self.client.get_account_pages().await?;
                    Ok(json!({ "pages": pages, "count": pages.len() }))
                }
                "get_account_insights" => {
                    let account_id = arguments
                        .get("account_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let metrics = arguments
                        .get("metrics")
                        .and_then(Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect::<Vec<_>>()
                        });
                    let period = match arguments
                        .get("period")
                        .and_then(Value::as_str)
                        .unwrap_or("day")
                    {
                        "lifetime" => InsightPeriod::Lifetime,
                        _ => InsightPeriod::Day,
                    };
                    let insights = self
                        .client
                        .get_account_insights(account_id, metrics, period)
                        .await?;
                    Ok(json!({ "insights": insights }))
                }
                "validate_access_token" => {
                    let valid = self.client.validate_access_token().await;
                    Ok(json!({ "valid": valid }))
                }
                "get_conversations" => {
                    let page_id = arguments
                        .get("page_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
                    let conversations = self.client.get_conversations(page_id, limit).await?;
                    metadata.insert(
                        "note".to_string(),
                        json!("Requires instagram_manage_messages permission"),
                    );
                    Ok(json!({
                        "conversations": conversations,
                        "count": conversations.len()
                    }))
                }
                "get_conversation_messages" => {
                    let conversation_id: String = arguments
                        .get("conversation_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            AppError::Validation("conversation_id is required".to_string())
                        })?
                        .to_string();
                    let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
                    let messages = self
                        .client
                        .get_conversation_messages(conversation_id.clone(), limit)
                        .await?;
                    Ok(json!({
                        "conversation_id": conversation_id,
                        "messages": messages,
                        "count": messages.len()
                    }))
                }
                "send_dm" => {
                    let request: SendDmRequest = serde_json::from_value(arguments)?;
                    let response: SendDmResponse = self.client.send_dm(request).await?;
                    metadata.insert(
                        "note".to_string(),
                        json!("24-hour response window applies. Requires Advanced Access."),
                    );
                    Ok(json!(response))
                }
                _ => Err(AppError::Validation(format!("Unknown tool: {}", name))),
            }
        }
        .await;

        let result = match run {
            Ok(data) => McpToolResult {
                success: true,
                data: Some(data),
                error: None,
                metadata: Some(metadata),
            },
            Err(e) => {
                self.client.log_request_error(name, &e);
                McpToolResult {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                    metadata: Some(metadata),
                }
            }
        };

        serde_json::to_value(result).map_err(|e| AppError::Json(e.to_string()))
    }

    fn list_resources(&self) -> Vec<ResourceDefinition> {
        vec![
            ResourceDefinition {
                uri: "instagram://profile".to_string(),
                name: "Instagram Profile".to_string(),
                description: "Current Instagram business profile information".to_string(),
                mime_type: "application/json".to_string(),
            },
            ResourceDefinition {
                uri: "instagram://media/recent".to_string(),
                name: "Recent Media Posts".to_string(),
                description: "Recent Instagram posts with engagement metrics".to_string(),
                mime_type: "application/json".to_string(),
            },
            ResourceDefinition {
                uri: "instagram://insights/account".to_string(),
                name: "Account Insights".to_string(),
                description: "Account-level analytics and insights".to_string(),
                mime_type: "application/json".to_string(),
            },
            ResourceDefinition {
                uri: "instagram://pages".to_string(),
                name: "Connected Pages".to_string(),
                description: "Facebook pages connected to the account".to_string(),
                mime_type: "application/json".to_string(),
            },
        ]
    }

    async fn read_resource(&self, params: Value) -> Result<Value, AppError> {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("Missing resource uri".to_string()))?;

        match uri {
            "instagram://profile" => {
                let profile = self.client.get_profile_info(None).await?;
                Ok(json!(profile))
            }
            "instagram://media/recent" => {
                let posts = self.client.get_media_posts(None, 10, None).await?;
                Ok(json!(posts))
            }
            "instagram://insights/account" => {
                let insights = self
                    .client
                    .get_account_insights(None, None, InsightPeriod::Day)
                    .await?;
                Ok(json!(insights))
            }
            "instagram://pages" => {
                let pages = self.client.get_account_pages().await?;
                Ok(json!(pages))
            }
            _ => Err(AppError::Validation(format!(
                "Unknown resource URI: {}",
                uri
            ))),
        }
    }

    fn list_prompts(&self) -> Vec<PromptDefinition> {
        vec![
            PromptDefinition {
                name: "analyze_engagement".to_string(),
                description: "Analyze Instagram post engagement and provide insights".to_string(),
                arguments: vec![
                    PromptArgument {
                        name: "media_id".to_string(),
                        description: "Instagram media ID to analyze".to_string(),
                        required: true,
                    },
                    PromptArgument {
                        name: "comparison_period".to_string(),
                        description: "Period to compare against (e.g., last_week, last_month)"
                            .to_string(),
                        required: false,
                    },
                ],
            },
            PromptDefinition {
                name: "content_strategy".to_string(),
                description:
                    "Generate content strategy recommendations based on account performance"
                        .to_string(),
                arguments: vec![
                    PromptArgument {
                        name: "focus_area".to_string(),
                        description: "Area to focus on (e.g., engagement, reach, growth)"
                            .to_string(),
                        required: false,
                    },
                    PromptArgument {
                        name: "time_period".to_string(),
                        description: "Time period to analyze (e.g., week, month)".to_string(),
                        required: false,
                    },
                ],
            },
            PromptDefinition {
                name: "hashtag_analysis".to_string(),
                description: "Analyze hashtag performance and suggest improvements".to_string(),
                arguments: vec![PromptArgument {
                    name: "post_count".to_string(),
                    description: "Number of recent posts to analyze".to_string(),
                    required: false,
                }],
            },
        ]
    }

    async fn get_prompt(&self, params: Value) -> Result<Value, AppError> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("Missing prompt name".to_string()))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));

        match name {
            "analyze_engagement" => {
                let media_id = arguments
                    .get("media_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AppError::Validation("media_id is required".to_string()))?;
                let insights = self
                    .client
                    .get_media_insights(media_id.to_string(), None)
                    .await?;

                Ok(json!(format!(
                    "Analyze the engagement metrics for Instagram post {}:\n\nInsights Data:\n{}\n\nPlease provide:\n1. Overall engagement performance assessment\n2. Key metrics analysis (impressions, reach, likes, comments, shares)\n3. Engagement rate calculation and interpretation\n4. Recommendations for improving future posts\n5. Comparison with typical performance benchmarks",
                    media_id,
                    serde_json::to_string_pretty(&insights).unwrap_or_else(|_| "[]".to_string())
                )))
            }
            "content_strategy" => {
                let focus_area = arguments
                    .get("focus_area")
                    .and_then(Value::as_str)
                    .unwrap_or("engagement");
                let time_period = arguments
                    .get("time_period")
                    .and_then(Value::as_str)
                    .unwrap_or("week");

                let posts = self.client.get_media_posts(None, 20, None).await?;
                let account_insights = self
                    .client
                    .get_account_insights(None, None, InsightPeriod::Day)
                    .await?;

                let preview_posts = posts.into_iter().take(5).collect::<Vec<_>>();

                Ok(json!(format!(
                    "Generate a content strategy for Instagram focusing on {} over the {}:\n\nRecent Posts Performance:\n{}\n\nAccount Insights:\n{}\n\nPlease provide:\n1. Content performance analysis\n2. Optimal posting times and frequency\n3. Content type recommendations (images, videos, carousels)\n4. Caption and hashtag strategies\n5. Engagement tactics to improve {}\n6. Specific action items for the next {}",
                    focus_area,
                    time_period,
                    serde_json::to_string_pretty(&preview_posts)
                        .unwrap_or_else(|_| "[]".to_string()),
                    serde_json::to_string_pretty(&account_insights)
                        .unwrap_or_else(|_| "[]".to_string()),
                    focus_area,
                    time_period,
                )))
            }
            "hashtag_analysis" => {
                let post_count = arguments
                    .get("post_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(10) as u32;
                let posts = self.client.get_media_posts(None, post_count, None).await?;

                let hashtags_data = posts
                    .into_iter()
                    .filter_map(|post| {
                        let caption = post.caption?;
                        let hashtags = caption
                            .split_whitespace()
                            .filter(|word| word.starts_with('#'))
                            .map(str::to_string)
                            .collect::<Vec<_>>();
                        Some(json!({
                            "post_id": post.id,
                            "hashtags": hashtags,
                            "likes": post.like_count,
                            "comments": post.comments_count,
                        }))
                    })
                    .collect::<Vec<_>>();

                Ok(json!(format!(
                    "Analyze hashtag performance for the last {} Instagram posts:\n\nHashtag Data:\n{}\n\nPlease provide:\n1. Most frequently used hashtags\n2. Hashtag performance correlation with engagement\n3. Hashtag diversity analysis\n4. Recommendations for hashtag optimization\n5. Suggested new hashtags to try\n6. Hashtag strategy improvements",
                    post_count,
                    serde_json::to_string_pretty(&hashtags_data)
                        .unwrap_or_else(|_| "[]".to_string())
                )))
            }
            _ => Err(AppError::Validation(format!("Unknown prompt: {}", name))),
        }
    }
}
