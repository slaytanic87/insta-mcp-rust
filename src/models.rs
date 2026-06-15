use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MediaType {
    Image,
    Video,
    CarouselAlbum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightMetric {
    Impressions,
    Reach,
    Likes,
    Comments,
    Shares,
    Saved,
    VideoViews,
    ProfileVisits,
    WebsiteClicks,
}

impl InsightMetric {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Impressions => "impressions",
            Self::Reach => "reach",
            Self::Likes => "likes",
            Self::Comments => "comments",
            Self::Shares => "shares",
            Self::Saved => "saved",
            Self::VideoViews => "video_views",
            Self::ProfileVisits => "profile_visits",
            Self::WebsiteClicks => "website_clicks",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightPeriod {
    Day,
    Week,
    Days28,
    Lifetime,
}

impl InsightPeriod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Days28 => "days_28",
            Self::Lifetime => "lifetime",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInsight {
    pub name: String,
    pub period: String,
    pub values: Vec<HashMap<String, Value>>,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInsight {
    pub name: String,
    pub period: String,
    pub values: Option<Vec<HashMap<String, Value>>>,
    pub total_value: Option<HashMap<String, Value>>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitInfo {
    pub app_id: String,
    pub call_count: i64,
    pub total_cputime: i64,
    pub total_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramProfile {
    pub id: String,
    pub username: String,
    pub name: Option<String>,
    pub biography: Option<String>,
    pub website: Option<String>,
    pub followers_count: Option<i64>,
    pub follows_count: Option<i64>,
    pub media_count: Option<i64>,
    pub profile_picture_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMedia {
    pub id: String,
    pub media_type: MediaType,
    pub media_url: Option<String>,
    pub permalink: Option<String>,
    pub thumbnail_url: Option<String>,
    pub caption: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
    pub like_count: Option<i64>,
    pub comments_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTag {
    pub username: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishMediaRequest {
    pub image_url: Option<Url>,
    pub video_url: Option<Url>,
    pub caption: Option<String>,
    pub location_id: Option<String>,
    pub user_tags: Option<Vec<UserTag>>,
}

impl PublishMediaRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.image_url.is_none() && self.video_url.is_none() {
            return Err("Either image_url or video_url is required".to_string());
        }

        if let Some(caption) = &self.caption {
            if caption.len() > 2200 {
                return Err("Caption must be 2200 characters or less".to_string());
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishMediaResponse {
    pub id: String,
    pub status: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    pub code: Option<i64>,
    pub error_subcode: Option<i64>,
    pub fbtrace_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacebookPage {
    pub id: String,
    pub name: String,
    pub access_token: Option<String>,
    pub category: Option<String>,
    pub instagram_business_account: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMessage {
    pub id: String,
    #[serde(rename = "from")]
    pub from_id: HashMap<String, Value>,
    pub to: Vec<HashMap<String, Value>>,
    pub message: Option<String>,
    pub created_time: DateTime<Utc>,
    pub attachments: Option<Vec<HashMap<String, Value>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramConversation {
    pub id: String,
    pub updated_time: DateTime<Utc>,
    pub messages: Option<Vec<InstagramMessage>>,
    pub participants: Option<Vec<HashMap<String, String>>>,
    pub message_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDmRequest {
    pub recipient_id: String,
    pub message: String,
    pub message_type: Option<String>,
}

impl SendDmRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.message.len() > 1000 {
            return Err("Message must be 1000 characters or less".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDmResponse {
    pub message_id: String,
    pub recipient_id: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDefinition {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDefinition {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArgument>,
}
