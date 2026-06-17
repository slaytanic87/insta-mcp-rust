use std::{collections::HashMap, sync::Arc};

use mcpr::{
    Tool,
    schema::ToolInputSchema,
    server::{Server, ServerConfig},
    transport::sse::SSETransport,
};
use serde_json::Value;
use tracing::error;

use crate::{
    config::Settings,
    error::AppError,
    instagram_client::InstagramClient,
    models::{
        AccountInsight, InsightMetric, InsightPeriod, InstagramMedia, InstagramProfile,
        PublishMediaRequest, SendDmRequest, SendDmResponse,
    },
};

fn get_profile_info() -> Tool {
    Tool {
        name: "get_profile_info".to_string(),
        description: Some("Get Instagram business profile information including followers, bio, and account details".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some(
                [(
                    "account_id".to_string(),
                    serde_json::json!({ "type": "string", "description": "Instagram business account ID (optional)" })
                )].into_iter().collect::<HashMap<_, _>>(),
            ),
            required: None,
        }
    }
}

fn get_media_posts() -> Tool {
    Tool {
        name: "get_media_posts".to_string(),
        description: Some("Get recent media posts from Instagram account with engagement metrics".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some(
                [
                    ("account_id".to_string(), serde_json::json!({ "type": "string" })),
                    ("limit".to_string(), serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 })),
                    ("after".to_string(), serde_json::json!({ "type": "string" }))
                ].into_iter().collect::<HashMap<_, _>>(),
            ),
            required: None,
        }
    }
}

fn get_media_insights() -> Tool {
Tool {
        name: "get_media_insights".to_string(),
        description: Some("Get detailed insights and analytics for a specific Instagram post".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some(
                [
                    ("media_id".to_string(), serde_json::json!({ "type": "string" })),
                    ("metrics".to_string(), serde_json::json!({
                            "type": "array",
                            "items": { "type": "string", "enum": ["reach", "likes", "comments", "shares", "saved", "video_views"] }
                        })),
                    ("after".to_string(), serde_json::json!({ "type": "string" }))
                ].into_iter().collect::<HashMap<_, _>>(),
            ),
            required: Some(vec!["media_id".to_string()]),
        }
    }
}

fn publish_media() -> Tool {
    Tool {
        name: "publish_media".to_string(),
        description: Some("Upload and publish an image or video to Instagram".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some(
                [
                    ("image_url".to_string(), serde_json::json!({ "type": "string", "format": "uri" })),
                    ("video_url".to_string(), serde_json::json!({ "type": "string", "format": "uri" })),
                    ("caption".to_string(), serde_json::json!({ "type": "string" })),
                    ("location_id".to_string(), serde_json::json!({ "type": "string" }))
                ].into_iter().collect::<HashMap<_, _>>(),
            ),
            required: Some(vec!["image_url".to_string(), "video_url".to_string()]),
        }
    }
}

fn get_account_pages() -> Tool {
    Tool {
        name: "get_account_pages".to_string(),
        description: Some("Get Facebook pages connected to the account".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some([].into_iter().collect::<HashMap<_, _>>()),
            required: None,
        }
    }
}

fn get_account_insights() -> Tool {
    Tool {
        name: "get_account_insights".to_string(),
        description: Some("Get account-level insights and analytics".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some(
                [
                    ("account_id".to_string(), serde_json::json!({ "type": "string"})),
                    ("metrics".to_string(), serde_json::json!({ "type": "array", "items": { "type": "string" }})),
                    ("period".to_string(), serde_json::json!({ "type": "string", "enum": ["day", "lifetime"], "default": "day" })),
                ].into_iter().collect::<HashMap<_, _>>(),
            ),
            required: None,
        }
    }
}

fn validate_access_token() -> Tool {
    Tool {
        name: "validate_access_token".to_string(),
        description: Some("Validate the Instagram API access token".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some([].into_iter().collect::<HashMap<_, _>>()),
            required: None,
        }
    }    
}

fn get_conversations() -> Tool {
    Tool {
        name: "validate_access_token".to_string(),
        description: Some("Get Instagram DM conversations (requires instagram_manage_messages Advanced Access)".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some([
                    ("page_id".to_string(), serde_json::json!({ "type": "string" })),
                    ("limit".to_string(), serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 })),
            ].into_iter().collect::<HashMap<_, _>>()),
            required: None,
        }
    }    
}

fn get_conversation_messages() -> Tool {
    Tool {
        name: "get_conversation_messages".to_string(),
        description: Some("Get messages for a specific conversation".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some([
                    ("conversation_id".to_string(), serde_json::json!({ "type": "string" })),
                    ("limit".to_string(), serde_json::json!({ "type": "integer", "minimum": 1, "maximum": 100, "default": 25 })),
            ].into_iter().collect::<HashMap<_, _>>()),
            required: Some(vec!["conversation_id".to_string()]),
        }
    }
}

fn send_dm() -> Tool {
    Tool {
        name: "send_dm".to_string(),
        description: Some("Send Instagram direct message (requires Advanced Access)".to_string()),
        input_schema: ToolInputSchema {
            r#type: "object".to_string(),
            properties: Some([
                    ("recipient_id".to_string(), serde_json::json!({ "type": "string" })),
                    ("message".to_string(), serde_json::json!({ "type": "string", "maxLength": 1000 })),
            ].into_iter().collect::<HashMap<_, _>>()),
            required: Some(vec!["recipient_id".to_string(), "message".to_string()]),
        }
    }    
}

pub async fn mcp_server_run_sse(settings: Arc<Settings>, client: Arc<InstagramClient>) {
    let server_config = ServerConfig::new()
    .with_name(&settings.mcp_server_name)
    .with_version(&settings.mcp_server_version)
    .with_tool(get_profile_info())
    .with_tool(get_media_posts())
    .with_tool(get_media_insights())
    .with_tool(publish_media())
    .with_tool(get_account_pages())
    .with_tool(get_account_insights())
    .with_tool(validate_access_token())
    .with_tool(get_conversations())
    .with_tool(get_conversation_messages())
    .with_tool(send_dm());

    let transport = SSETransport::new("http://127.0.0.1:9090");
    let mut server: Server<SSETransport> = Server::<SSETransport>::new(server_config);

    let mut client_vl = client.clone();
    server
        .register_tool_handler("get_profile_info", move |params: Value| {
            let account_id = params
                .get("account_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let profile: InstagramProfile = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_profile_info(account_id))
                .unwrap();
            Ok(serde_json::json!(profile))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_media_posts", move |params: Value| {
            let account_id = params
                .get("account_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
            let after = params
                .get("after")
                .and_then(Value::as_str)
                .map(str::to_string);
            let posts: Vec<InstagramMedia> = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_media_posts(account_id, limit, after))
                .unwrap();
            Ok(serde_json::json!({ "posts": posts, "count": posts.len() }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_media_insights", move |params: Value| {
            let media_id: String = params
                .get("media_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Validation("media_id is required".to_string()))
                .unwrap()
                .to_string();

            let metrics: Option<Vec<InsightMetric>> = params
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
            let insights = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_media_insights(media_id.clone(), metrics))
                .unwrap();
            Ok(serde_json::json!({ "media_id": media_id, "insights": insights }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("publish_media", move |params: Value| {
            let request: PublishMediaRequest = serde_json::from_value(params).unwrap();
            let response = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.publish_media(request))
                .unwrap();
            Ok(serde_json::json!(response))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_account_pages", move |_| {
            let pages = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_account_pages())
                .unwrap();
            Ok(serde_json::json!({ "pages": pages, "count": pages.len() }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_account_insights", move |params: Value| {
            let account_id = params
                .get("account_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let metrics = params.get("metrics").and_then(Value::as_array).map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            });
            let period = match params
                .get("period")
                .and_then(Value::as_str)
                .unwrap_or("day")
            {
                "lifetime" => InsightPeriod::Lifetime,
                _ => InsightPeriod::Day,
            };
            let insights: Vec<AccountInsight> = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_account_insights(account_id, metrics, period))
                .unwrap();
            Ok(serde_json::json!({ "insights": insights }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("validate_access_token", move |params: Value| {
            let valid: bool = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.validate_access_token());
            Ok(serde_json::json!({ "valid": valid }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_conversations", move |params: Value| {
            let page_id = params
                .get("page_id")
                .and_then(Value::as_str)
                .map(str::to_string);
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
            let conversations = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_conversations(page_id, limit))
                .unwrap();

            Ok(serde_json::json!({
                "conversations": conversations,
                "count": conversations.len()
            }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("get_conversation_messages", move |params: Value| {
            let conversation_id: String = params
                .get("conversation_id")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Validation("conversation_id is required".to_string()))
                .unwrap()
                .to_string();
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(25) as u32;
            let messages = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.get_conversation_messages(conversation_id.clone(), limit))
                .unwrap();
            Ok(serde_json::json!({
                "conversation_id": conversation_id,
                "messages": messages,
                "count": messages.len()
            }))
        })
        .unwrap();

    client_vl = client.clone();
    server
        .register_tool_handler("send_dm", move |params: Value| {
            let request: SendDmRequest = serde_json::from_value(params).unwrap();
            let response: SendDmResponse = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(client_vl.send_dm(request))
                .unwrap();
            Ok(serde_json::json!(response))
        })
        .unwrap();

    let result = server.start(transport);
    if result.is_err() {
        error!("Server exited with error: {}", result.err().unwrap());
    }
}
