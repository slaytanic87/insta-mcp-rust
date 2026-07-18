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
    pub log_level: String,
    pub log_format: String,
    pub cache_enabled: bool,
    pub cache_ttl_seconds: u64,
    pub mcp_server_name: String,
    pub mcp_server_version: String,
    pub mcp_transport: String,
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
            log_level,
            log_format,
            cache_enabled: env_bool_or("CACHE_ENABLED", true),
            cache_ttl_seconds: env_parse_or("CACHE_TTL_SECONDS", 300_u64),
            mcp_server_name: env_or("MCP_SERVER_NAME", "instagram-mcp-server"),
            mcp_server_version: env_or("MCP_SERVER_VERSION", "1.0.0"),
            mcp_transport: env_or("MCP_TRANSPORT", "stdio")
        })
    }

    pub fn instagram_api_url(&self) -> String {
        format!(
            "{}/{}",
            self.instagram_api_base_url, self.instagram_api_version
        )
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
