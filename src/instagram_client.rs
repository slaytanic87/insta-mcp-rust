use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;

use chrono::{Duration, Utc};
use governor::clock::DefaultClock;
use governor::state::InMemoryState;
use governor::state::direct::NotKeyed;
use governor::{Quota, RateLimiter};
use image::ImageReader;
use reqwest::{Client, Error, StatusCode};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::config::Settings;
use crate::error::AppError;
use crate::models::{
    AccountInsight, FacebookPage, InsightMetric, InsightPeriod, InstagramConversation,
    InstagramMedia, InstagramMessage, InstagramProfile, MediaInsight, PublishMediaRequest,
    PublishMediaResponse, RateLimitInfo, SendDmRequest, SendDmResponse,
};

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Value,
    expires_at: chrono::DateTime<Utc>,
}

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

#[derive(Clone)]
pub struct InstagramClient {
    settings: Arc<Settings>,
    http: Client,
    limiter: Arc<Limiter>,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl InstagramClient {
    pub fn new(settings: Arc<Settings>) -> Result<Self, AppError> {
        let requests_per_hour = settings.rate_limit_requests_per_hour.max(1);
        let quota = Quota::with_period(std::time::Duration::from_secs(
            3600 / requests_per_hour as u64,
        ))
        .ok_or_else(|| AppError::Config("Invalid rate limit period".to_string()))?
        .allow_burst(
            NonZeroU32::new(requests_per_hour)
                .ok_or_else(|| AppError::Config("Invalid rate limit burst".to_string()))?,
        );

        let http = Client::builder()
            .pool_max_idle_per_host(5)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| AppError::Http(e.to_string()))?;

        info!(api_version = %settings.instagram_api_version, "Instagram client initialized");

        Ok(Self {
            settings,
            http,
            limiter: Arc::new(RateLimiter::direct(quota)),
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn cache_key(&self, endpoint: &str, params: &HashMap<String, String>) -> String {
        let mut parts = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>();
        parts.sort();
        format!("{}?{}", endpoint, parts.join("&"))
    }

    async fn get_cached(&self, key: &str) -> Option<Value> {
        if !self.settings.cache_enabled {
            return None;
        }

        let cache = self.cache.read().await;
        let entry = cache.get(key)?;
        if entry.expires_at > Utc::now() {
            return Some(entry.data.clone());
        }

        None
    }

    async fn set_cached(&self, key: String, data: Value) {
        if !self.settings.cache_enabled {
            return;
        }

        let expires_at = Utc::now() + Duration::seconds(self.settings.cache_ttl_seconds as i64);
        let entry = CacheEntry { data, expires_at };
        self.cache.write().await.insert(key, entry);
    }

    async fn validate_image_aspect_ratio(&self, image_url: &str) -> Result<(), AppError> {
        let bytes = self
            .http
            .get(image_url)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| AppError::Http(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| AppError::Http(e.to_string()))?;

        let reader = ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| AppError::Validation(e.to_string()))?;
        let image = reader
            .decode()
            .map_err(|e| AppError::Validation(format!("Failed to decode image: {}", e)))?;

        let (width, height) = (image.width() as f64, image.height() as f64);
        if height == 0.0 {
            return Err(AppError::Validation("Invalid image dimensions".to_string()));
        }

        let ratio = width / height;
        let accepted = [
            ("4:5 (portrait)", 0.78, 0.82),
            ("1:1 (square)", 0.98, 1.02),
            ("1.91:1 (landscape)", 1.89, 1.93),
        ];

        for (name, min, max) in accepted {
            if ratio >= min && ratio <= max {
                debug!(width = width as i64, height = height as i64, ratio = %ratio, accepted_as = name, "Image aspect ratio valid");
                return Ok(());
            }
        }

        Err(AppError::Validation(format!(
            "Image aspect ratio {:.2} is not supported by Instagram. Accepted ratios: 4:5, 1:1, 1.91:1.",
            ratio
        )))
    }

    async fn make_request(
        &self,
        method: &str,
        endpoint: &str,
        mut params: HashMap<String, String>,
        body: Option<Value>,
        use_cache: bool,
        use_facebook_api: bool,
    ) -> Result<Value, AppError> {
        params.insert(
            "access_token".to_string(),
            crate::config::get_access_token_from_env(),
        );

        let cache_key = self.cache_key(endpoint, &params);
        if method.eq_ignore_ascii_case("GET") && use_cache {
            if let Some(value) = self.get_cached(&cache_key).await {
                debug!(endpoint = %endpoint, "Cache hit");
                return Ok(value);
            }
        }

        self.limiter.until_ready().await;

        let base_url: String = if use_facebook_api {
            "https://graph.facebook.com/v25.0".to_string()
        } else {
            self.settings.instagram_api_url()
        };
        let url: String = format!("{}/{}", base_url, endpoint);

        debug!(method = %method, endpoint = %endpoint, "Making API request");

        let response = match method {
            "GET" => {
                let url_with_param: Url = reqwest::Url::parse_with_params(&url, &params).unwrap();
                self.http.get(url_with_param).send().await?
            }
            "POST" => {
                let url_with_param: Url = reqwest::Url::parse_with_params(&url, &params).unwrap();
                let mut request = self.http.post(url_with_param);
                if let Some(payload) = body {
                    request = request.json(&payload);
                }
                request.send().await?
            }
            _ => {
                return Err(AppError::Validation(format!(
                    "Unsupported HTTP method: {}",
                    method
                )));
            }
        };

        if response.status().as_u16() == 429 {
            warn!(endpoint = %endpoint, "Rate limit exceeded");
            return Err(AppError::RateLimitExceeded);
        }

        let status: reqwest::StatusCode = response.status();
        let value: Value = response.json().await?;

        if !status.is_success() {
            if let Some(error_obj) = value.get("error") {
                return Err(AppError::InstagramApi {
                    message: error_obj
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown error")
                        .to_string(),
                    error_code: error_obj.get("code").and_then(Value::as_i64),
                    error_subcode: error_obj.get("error_subcode").and_then(Value::as_i64),
                });
            }
            return Err(AppError::Http(format!(
                "Request failed with status {}",
                status
            )));
        }

        if let Some(error_obj) = value.get("error") {
            return Err(AppError::InstagramApi {
                message: error_obj
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown error")
                    .to_string(),
                error_code: error_obj.get("code").and_then(Value::as_i64),
                error_subcode: error_obj.get("error_subcode").and_then(Value::as_i64),
            });
        }

        if method.eq_ignore_ascii_case("GET") && use_cache {
            self.set_cached(cache_key, value.clone()).await;
        }

        Ok(value)
    }

    pub async fn get_profile_info(
        &self,
        account_id: Option<String>,
    ) -> Result<InstagramProfile, AppError> {
        let account: String = account_id
            .or_else(|| self.settings.instagram_business_account_id.clone())
            .ok_or_else(|| {
                AppError::Config("Instagram business account ID not configured".to_string())
            })?;

        let fields = [
            "id",
            "username",
            "name",
            "biography",
            "website",
            "profile_picture_url",
            "followers_count",
            "follows_count",
            "media_count",
        ]
        .join(",");

        let mut params = HashMap::new();
        params.insert("fields".to_string(), fields);

        let data = self
            .make_request("GET", &account, params, None, true, false)
            .await?;
        serde_json::from_value(data).map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn get_media_posts(
        &self,
        account_id: Option<String>,
        limit: u32,
        after: Option<String>,
    ) -> Result<Vec<InstagramMedia>, AppError> {
        let account = account_id
            .or_else(|| self.settings.instagram_business_account_id.clone())
            .ok_or_else(|| {
                AppError::Config("Instagram business account ID not configured".to_string())
            })?;

        let fields = [
            "id",
            "media_type",
            "media_url",
            "permalink",
            "thumbnail_url",
            "caption",
            "timestamp",
            "like_count",
            "comments_count",
        ]
        .join(",");

        let mut params = HashMap::new();
        params.insert("fields".to_string(), fields);
        params.insert("limit".to_string(), limit.min(100).to_string());
        if let Some(cursor) = after {
            params.insert("after".to_string(), cursor);
        }

        let data = self
            .make_request(
                "GET",
                &format!("{}/media", account),
                params,
                None,
                true,
                false,
            )
            .await?;

        let items = data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<InstagramMedia>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn get_media_insights(
        &self,
        media_id: String,
        metrics: Option<Vec<InsightMetric>>,
    ) -> Result<Vec<MediaInsight>, AppError> {
        let default_metrics = vec![
            InsightMetric::Reach,
            InsightMetric::Likes,
            InsightMetric::Comments,
            InsightMetric::Shares,
            InsightMetric::Saved,
        ];
        let metric_values = metrics.unwrap_or(default_metrics);
        let metric_param = metric_values
            .iter()
            .map(InsightMetric::as_str)
            .collect::<Vec<_>>()
            .join(",");

        let mut params = HashMap::new();
        params.insert("metric".to_string(), metric_param);

        let data = self
            .make_request(
                "GET",
                &format!("{}/insights", media_id),
                params,
                None,
                true,
                false,
            )
            .await?;

        let items = data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<MediaInsight>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn publish_media(
        &self,
        request: PublishMediaRequest,
    ) -> Result<PublishMediaResponse, AppError> {
        request.validate().map_err(AppError::Validation)?;

        let account = self
            .settings
            .instagram_business_account_id
            .clone()
            .ok_or_else(|| {
                AppError::Config("Instagram business account ID not configured".to_string())
            })?;

        if let Some(image_url) = &request.image_url {
            self.validate_image_aspect_ratio(image_url.as_str()).await?;
        }

        let mut container_payload = json!({
            "caption": request.caption.unwrap_or_default(),
        });

        if let Some(image_url) = request.image_url {
            container_payload["image_url"] = json!(image_url.as_str());
        }
        if let Some(video_url) = request.video_url {
            container_payload["video_url"] = json!(video_url.as_str());
        }
        if let Some(location_id) = request.location_id {
            container_payload["location_id"] = json!(location_id);
        }

        let container_response = self
            .make_request(
                "POST",
                &format!("{}/media", account),
                HashMap::new(),
                Some(container_payload),
                false,
                false,
            )
            .await?;

        let container_id = container_response
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Json("Missing container id".to_string()))?
            .to_string();

        let publish_response = self
            .make_request(
                "POST",
                &format!("{}/media_publish", account),
                HashMap::new(),
                Some(json!({ "creation_id": container_id })),
                false,
                false,
            )
            .await?;

        let id = publish_response
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Json("Missing publish id".to_string()))?
            .to_string();

        Ok(PublishMediaResponse {
            id,
            status: "published".to_string(),
            success: true,
        })
    }

    pub async fn get_account_pages(&self) -> Result<Vec<FacebookPage>, AppError> {
        let mut params = HashMap::new();
        params.insert(
            "fields".to_string(),
            "id,name,instagram_business_account".to_string(),
        );

        let data = self
            .make_request("GET", "me/accounts", params, None, true, false)
            .await?;

        let items = data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<FacebookPage>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn get_account_insights(
        &self,
        account_id: Option<String>,
        metrics: Option<Vec<String>>,
        period: InsightPeriod,
    ) -> Result<Vec<AccountInsight>, AppError> {
        let account = account_id
            .or_else(|| self.settings.instagram_business_account_id.clone())
            .ok_or_else(|| {
                AppError::Config("Instagram business account ID not configured".to_string())
            })?;

        let metric_values = metrics.unwrap_or_else(|| {
            vec![
                "reach".to_string(),
                "profile_views".to_string(),
                "website_clicks".to_string(),
            ]
        });

        let mut params = HashMap::new();
        params.insert("metric".to_string(), metric_values.join(","));
        params.insert("period".to_string(), period.as_str().to_string());
        params.insert("metric_type".to_string(), "total_value".to_string());

        let data = self
            .make_request(
                "GET",
                &format!("{}/insights", account),
                params,
                None,
                true,
                false,
            )
            .await?;

        let items = data
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<AccountInsight>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn validate_access_token(&self) -> bool {
        let mut params = HashMap::new();
        params.insert("fields".to_string(), "id".to_string());
        self.make_request("GET", "me", params, None, false, false)
            .await
            .is_ok()
    }

    pub async fn get_conversations(
        &self,
        page_id: Option<String>,
        limit: u32,
    ) -> Result<Vec<InstagramConversation>, AppError> {
        let page = match page_id {
            Some(v) => v,
            None => {
                let pages = self.get_account_pages().await?;
                let first = pages
                    .first()
                    .ok_or_else(|| {
                        AppError::Validation(
                            "No Facebook pages found. Connect a Facebook page to your Instagram account."
                                .to_string(),
                        )
                    })?
                    .id
                    .clone();
                info!(page_id = %first, "Using page ID");
                first
            }
        };

        let mut params = HashMap::new();
        params.insert("platform".to_string(), "instagram".to_string());
        params.insert(
            "fields".to_string(),
            "id,updated_time,message_count".to_string(),
        );
        params.insert("limit".to_string(), limit.min(100).to_string());

        let data = self
            .make_request(
                "GET",
                &format!("{}/conversations", page),
                params,
                None,
                true,
                true,
            )
            .await;

        let value = match data {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("#2") || msg.contains("unavailable") || msg.contains("temporarily")
                {
                    return Err(AppError::Validation(format!(
                        "{}\n\nThis likely indicates instagram_manage_messages requires Advanced Access via Meta App Review. See INSTAGRAM_DM_SETUP.md.",
                        e
                    )));
                }
                return Err(e);
            }
        };

        let items = value
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<InstagramConversation>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn get_conversation_messages(
        &self,
        conversation_id: String,
        limit: u32,
    ) -> Result<Vec<InstagramMessage>, AppError> {
        let mut params = HashMap::new();
        params.insert(
            "fields".to_string(),
            "messages{id,from,to,message,created_time,attachments}".to_string(),
        );
        params.insert("limit".to_string(), limit.min(100).to_string());

        let data = self
            .make_request("GET", &conversation_id, params, None, true, true)
            .await?;

        let items = data
            .get("messages")
            .and_then(|v| v.get("data"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        items
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<InstagramMessage>, _>>()
            .map_err(|e| AppError::Json(e.to_string()))
    }

    pub async fn send_dm(&self, request: SendDmRequest) -> Result<SendDmResponse, AppError> {
        request.validate().map_err(AppError::Validation)?;

        let payload = json!({
            "recipient": { "id": request.recipient_id },
            "message": { "text": request.message },
        });

        let recipient_id = payload
            .get("recipient")
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let data = self
            .make_request(
                "POST",
                "me/messages",
                HashMap::new(),
                Some(payload),
                false,
                true,
            )
            .await;

        let value = match data {
            Ok(v) => v,
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("#2")
                    || msg.contains("unavailable")
                    || msg.contains("temporarily")
                    || msg.contains("permissions")
                    || msg.contains("access")
                {
                    return Err(AppError::Validation(format!(
                        "{}\n\nThis likely indicates instagram_manage_messages requires Advanced Access via Meta App Review. See INSTAGRAM_DM_SETUP.md.",
                        e
                    )));
                }
                return Err(e);
            }
        };

        Ok(SendDmResponse {
            message_id: value
                .get("message_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            recipient_id,
            success: true,
        })
    }

    pub async fn check_token_validy(&self) -> Result<bool, Error> {
        let mut params = HashMap::new();
        params.insert("access_token", crate::config::get_access_token_from_env());
        let url_with_param: Url =
            reqwest::Url::parse_with_params("https://graph.facebook.com/v25.0/me", &params)
                .unwrap();
        let response = self.http.get(url_with_param).send().await?;
        Ok(response.status().eq(&StatusCode::OK.as_u16()))
    }

    pub async fn refresh_long_lived_token(&self) -> Result<String, Error> {
        let mut params = HashMap::new();
        params.insert("grant_type", "fb_exchange_token".to_string());
        params.insert("client_id", self.settings.facebook_app_id.clone());
        params.insert("client_secret", self.settings.facebook_app_secret.clone());
        params.insert(
            "fb_exchange_token",
            crate::config::get_access_token_from_env(),
        );

        let url_with_param: Url = reqwest::Url::parse_with_params(
            "https://graph.facebook.com/v25.0/oauth/access_token",
            &params,
        )
        .unwrap();
        let response = self.http.get(url_with_param).send().await?;
        let json_response: serde_json::Value = response.json().await?;
        Ok(json_response["access_token"]
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    pub fn _get_rate_limit_info(&self) -> RateLimitInfo {
        RateLimitInfo {
            app_id: self.settings.facebook_app_id.clone(),
            call_count: 0,
            total_cputime: 0,
            total_time: 0,
        }
    }

    pub fn log_request_error(&self, context: &str, error: &AppError) {
        error!(context = %context, error = %error, "Instagram API operation failed");
    }
}
