use std::env;

use envie::Envie;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct Settings {
    pub facebook_app_id: String,
    pub facebook_app_secret: String,
    pub instagram_business_account_id: Option<String>,
    pub instagram_api_version: String,
    pub instagram_api_base_url: String,
    pub rate_limit_requests_per_hour: u32,
    pub rate_limit_posts_per_day: u32,
    pub rate_limit_enable_backoff: bool,
    pub log_level: String,
    pub log_format: String,
    pub log_file: Option<String>,
    pub cache_enabled: bool,
    pub cache_ttl_seconds: u64,
    pub redis_url: Option<String>,
    pub enable_request_validation: bool,
    pub max_request_size_mb: u64,
    pub allowed_image_formats: Vec<String>,
    pub allowed_video_formats: Vec<String>,
    pub debug_mode: bool,
    pub mock_api_responses: bool,
    pub enable_metrics: bool,
    pub mcp_server_name: String,
    pub mcp_server_version: String,
    pub mcp_transport: String,
    pub database_url: Option<String>,
    pub database_echo: bool,
    pub webhook_verify_token: Option<String>,
    pub webhook_secret: Option<String>,
    pub webhook_url: Option<String>,
}

impl Settings {
    pub fn from_env() -> Result<Self, AppError> {
        let facebook_app_id = env_required("FACEBOOK_APP_ID", "Facebook app ID")?;
        let facebook_app_secret = env_required("FACEBOOK_APP_SECRET", "Facebook app secret")?;

        let instagram_api_version = env_or("INSTAGRAM_API_VERSION", "v19.0");
        if !instagram_api_version.starts_with('v') {
            return Err(AppError::Config(
                "INSTAGRAM_API_VERSION must start with 'v' (e.g., v19.0)".to_string(),
            ));
        }

        let log_level = env_or("LOG_LEVEL", "INFO").to_uppercase();
        let valid_levels = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"];
        if !valid_levels.contains(&log_level.as_str()) {
            return Err(AppError::Config(format!(
                "LOG_LEVEL must be one of {:?}",
                valid_levels
            )));
        }

        let log_format = env_or("LOG_FORMAT", "json").to_lowercase();
        let valid_formats = ["json", "text"];
        if !valid_formats.contains(&log_format.as_str()) {
            return Err(AppError::Config(format!(
                "LOG_FORMAT must be one of {:?}",
                valid_formats
            )));
        }

        Ok(Self {
            facebook_app_id,
            facebook_app_secret,
            instagram_business_account_id: env_optional("INSTAGRAM_BUSINESS_ACCOUNT_ID"),
            instagram_api_version,
            instagram_api_base_url: env_or("INSTAGRAM_API_BASE_URL", "https://graph.facebook.com"),
            rate_limit_requests_per_hour: env_parse_or("RATE_LIMIT_REQUESTS_PER_HOUR", 200),
            rate_limit_posts_per_day: env_parse_or("RATE_LIMIT_POSTS_PER_DAY", 25),
            rate_limit_enable_backoff: env_bool_or("RATE_LIMIT_ENABLE_BACKOFF", true),
            log_level,
            log_format,
            log_file: env_optional("LOG_FILE")
                .or_else(|| Some("logs/instagram_mcp.log".to_string())),
            cache_enabled: env_bool_or("CACHE_ENABLED", true),
            cache_ttl_seconds: env_parse_or("CACHE_TTL_SECONDS", 300_u64),
            redis_url: env_optional("REDIS_URL")
                .or_else(|| Some("redis://localhost:6379/0".to_string())),
            enable_request_validation: env_bool_or("ENABLE_REQUEST_VALIDATION", true),
            max_request_size_mb: env_parse_or("MAX_REQUEST_SIZE_MB", 10_u64),
            allowed_image_formats: env_csv_or(
                "ALLOWED_IMAGE_FORMATS",
                &["jpg", "jpeg", "png", "gif"],
            ),
            allowed_video_formats: env_csv_or("ALLOWED_VIDEO_FORMATS", &["mp4", "mov"]),
            debug_mode: env_bool_or("DEBUG_MODE", false),
            mock_api_responses: env_bool_or("MOCK_API_RESPONSES", false),
            enable_metrics: env_bool_or("ENABLE_METRICS", true),
            mcp_server_name: env_or("MCP_SERVER_NAME", "instagram-mcp-server"),
            mcp_server_version: env_or("MCP_SERVER_VERSION", "1.0.0"),
            mcp_transport: env_or("MCP_TRANSPORT", "stdio"),
            database_url: env_optional("DATABASE_URL")
                .or_else(|| Some("sqlite:///instagram_mcp.db".to_string())),
            database_echo: env_bool_or("DATABASE_ECHO", false),
            webhook_verify_token: env_optional("WEBHOOK_VERIFY_TOKEN"),
            webhook_secret: env_optional("WEBHOOK_SECRET"),
            webhook_url: env_optional("WEBHOOK_URL"),
        })
    }

    pub fn instagram_api_url(&self) -> String {
        format!(
            "{}/{}",
            self.instagram_api_base_url, self.instagram_api_version
        )
    }

    pub fn max_request_size_bytes(&self) -> u64 {
        self.max_request_size_mb * 1024 * 1024
    }
}

pub fn get_access_token_from_env() -> String {
    let env = Envie::load().expect("Failed to load env. file");
    env.get("INSTAGRAM_ACCESS_TOKEN")
        .expect("INSTAGRAM_ACCESS_TOKEN not set")
}

fn env_required(key: &str, label: &str) -> Result<String, AppError> {
    env::var(key).map_err(|_| AppError::Config(format!("Missing required {} ({})", label, key)))
}

fn env_optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_bool_or(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => matches!(v.to_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

fn env_parse_or<T>(key: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<T>().ok())
        .unwrap_or(default)
}

fn env_csv_or(key: &str, defaults: &[&str]) -> Vec<String> {
    env::var(key)
        .ok()
        .map(|v| {
            v.split(',')
                .map(|item| item.trim().to_lowercase())
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|v| (*v).to_string()).collect())
}
