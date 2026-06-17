use std::sync::{Arc, OnceLock};

use mcp_core::{
    server::Server,
    tool_text_response,
    transport::ServerSseTransport,
    types::{CallToolRequest, CallToolResponse, ServerCapabilities, Tool, ToolCapabilities},
};
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

static SSE_CLIENT: OnceLock<Arc<InstagramClient>> = OnceLock::new();

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
        name: "validate_access_token".to_string(),
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

fn get_profile_info_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");

        let account_id = request
            .arguments
            .as_ref()
            .unwrap()
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let profile: InstagramProfile = client.get_profile_info(account_id).await.unwrap();
        tool_text_response!(serde_json::json!(profile))
    })
}

fn get_media_posts_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let account_id = request
            .arguments
            .as_ref()
            .unwrap()
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let limit = request
            .arguments
            .as_ref()
            .unwrap()
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(25) as u32;
        let after = request
            .arguments
            .as_ref()
            .unwrap()
            .get("after")
            .and_then(Value::as_str)
            .map(str::to_string);
        let posts: Vec<InstagramMedia> = client
            .get_media_posts(account_id, limit, after)
            .await
            .unwrap();
        tool_text_response!(serde_json::json!({ "posts": posts, "count": posts.len() }))
    })
}

fn get_media_insights_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let media_id: String = request
            .arguments
            .as_ref()
            .unwrap()
            .get("media_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("media_id is required".to_string()))
            .unwrap()
            .to_string();

        let metrics: Option<Vec<InsightMetric>> = request
            .arguments
            .as_ref()
            .unwrap()
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
        let insights = client
            .get_media_insights(media_id.clone(), metrics)
            .await
            .unwrap();
        tool_text_response!(serde_json::json!({ "media_id": media_id, "insights": insights }))
    })
}

fn publish_media_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let request: PublishMediaRequest =
            serde_json::from_value(serde_json::json!(request.arguments.unwrap()))
                .unwrap();
        let response = client.publish_media(request).await.unwrap();
        tool_text_response!(serde_json::json!(response))
    })
}

fn get_account_pages_handler(
    _: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let pages = client.get_account_pages().await.unwrap();
        tool_text_response!(serde_json::json!({ "pages": pages, "count": pages.len() }))
    })
}

fn get_account_insights_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    let client = SSE_CLIENT
        .get()
        .expect("SSE client must be initialized before tool handlers run");
    let client = Arc::clone(&client);
    Box::pin(async move {
        let account_id = request
            .arguments
            .as_ref()
            .unwrap()
            .get("account_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let metrics = request
            .arguments
            .as_ref()
            .unwrap()
            .get("metrics")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            });
        let period = match request
            .arguments
            .as_ref()
            .unwrap()
            .get("period")
            .and_then(Value::as_str)
            .unwrap_or("day")
        {
            "lifetime" => InsightPeriod::Lifetime,
            _ => InsightPeriod::Day,
        };
        let insights: Vec<AccountInsight> = client
            .get_account_insights(account_id, metrics, period)
            .await
            .unwrap();
        tool_text_response!(serde_json::json!({ "insights": insights }))
    })
}

fn validate_access_token_handler(
    _: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let valid: bool = client.validate_access_token().await;
        tool_text_response!(serde_json::json!({ "valid": valid }))
    })
}

fn get_conversations_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let page_id = request
            .arguments
            .as_ref()
            .unwrap()
            .get("page_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let limit = request
            .arguments
            .as_ref()
            .unwrap()
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(25) as u32;
        let conversations = client.get_conversations(page_id, limit).await.unwrap();
        tool_text_response!(serde_json::json!({
            "conversations": conversations,
            "count": conversations.len()
        }))
    })
}

fn get_conversation_messages_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");

        let conversation_id: String = request
            .arguments
            .as_ref()
            .unwrap()
            .get("conversation_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("conversation_id is required".to_string()))
            .unwrap()
            .to_string();
        let limit = request
            .arguments
            .as_ref()
            .unwrap()
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(25) as u32;
        let messages = client
            .get_conversation_messages(conversation_id.clone(), limit)
            .await
            .unwrap();
        tool_text_response!(serde_json::json!({
            "conversation_id": conversation_id,
            "messages": messages,
            "count": messages.len()
        }))
    })
}

fn send_dm_handler(
    request: CallToolRequest,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CallToolResponse> + Send + 'static>> {
    Box::pin(async move {
        let client = SSE_CLIENT
            .get()
            .expect("SSE client must be initialized before tool handlers run");
        let request: SendDmRequest =
            serde_json::from_value(serde_json::json!(request.arguments.unwrap()))
                .unwrap();
        let response: SendDmResponse = client.send_dm(request).await.unwrap();
        tool_text_response!(serde_json::json!(response))
    })
}

pub async fn mcp_server_run_sse(settings: Arc<Settings>, client: Arc<InstagramClient>) {
    let _ = SSE_CLIENT.set(Arc::clone(&client));

    let server_protocol = Server::builder(
        settings.mcp_server_name.clone(),
        settings.mcp_server_version.clone(),
        mcp_core::types::ProtocolVersion::V2025_03_26,
    )
    .set_capabilities(ServerCapabilities {
        tools: Some(ToolCapabilities::default()),
        ..Default::default()
    })
    .register_tool(get_profile_info(), get_profile_info_handler)
    .register_tool(get_media_posts(), get_media_posts_handler)
    .register_tool(get_media_insights(), get_media_insights_handler)
    .register_tool(publish_media(), publish_media_handler)
    .register_tool(get_account_pages(), get_account_pages_handler)
    .register_tool(get_account_insights(), get_account_insights_handler)
    .register_tool(validate_access_token(), validate_access_token_handler)
    .register_tool(get_conversations(), get_conversations_handler)
    .register_tool(
        get_conversation_messages(),
        get_conversation_messages_handler,
    )
    .register_tool(send_dm(), send_dm_handler)
    .build();
    let transport = ServerSseTransport::new("127.0.0.1".to_string(), 9091, server_protocol);

    let _ = Server::start(transport).await;
}
