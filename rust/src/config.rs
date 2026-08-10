use std::{env, net::SocketAddr, path::PathBuf};

use thiserror::Error;
use uuid::Uuid;

pub const APP_VERSION: &str = include_str!("../../VERSION").trim_ascii();
pub const FORMULA_VERSION: &str = "1.0.0";
pub const SCHEMA_VERSION: &str = "1.0.0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppEnvironment {
    Local,
    Development,
    Production,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_address: SocketAddr,
    pub environment: AppEnvironment,
    pub local_auth: Option<LocalAuthSettings>,
    pub web_asset_dir: PathBuf,
    pub guest_requests_per_minute: u32,
    pub provider_refreshes_per_hour: u32,
    pub calculation_concurrency: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalAuthSettings {
    pub tenant_id: Uuid,
    pub object_id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("APP_ENV must be local, development, or production")]
    InvalidEnvironment,
    #[error("HTTP_BIND must be a valid socket address")]
    InvalidBindAddress,
    #[error("local authentication settings are allowed only when APP_ENV=local")]
    LocalAuthOutsideLocalEnvironment,
    #[error("local authentication requires tenant ID, owner ID, and display name together")]
    IncompleteLocalAuth,
    #[error("local authentication tenant and owner IDs must be UUIDs")]
    InvalidLocalAuthId,
    #[error("local authentication display name must contain 1 to 200 characters")]
    InvalidLocalAuthDisplayName,
    #[error("request quota settings must be positive integers")]
    InvalidQuota,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let environment = match env::var("APP_ENV")
            .unwrap_or_else(|_| "local".to_owned())
            .as_str()
        {
            "local" => AppEnvironment::Local,
            "development" => AppEnvironment::Development,
            "production" => AppEnvironment::Production,
            _ => return Err(ConfigError::InvalidEnvironment),
        };

        let tenant_id = env::var("LOCAL_AUTH_TENANT_ID").ok();
        let object_id = env::var("LOCAL_AUTH_OWNER_ID").ok();
        let display_name = env::var("LOCAL_AUTH_DISPLAY_NAME").ok();
        let has_local_auth_setting =
            tenant_id.is_some() || object_id.is_some() || display_name.is_some();

        if environment != AppEnvironment::Local && has_local_auth_setting {
            return Err(ConfigError::LocalAuthOutsideLocalEnvironment);
        }
        let local_auth = match (tenant_id, object_id, display_name) {
            (None, None, None) => None,
            (Some(tenant_id), Some(object_id), Some(display_name)) => {
                let name_length = display_name.chars().count();
                if !(1..=200).contains(&name_length) || display_name.chars().any(char::is_control) {
                    return Err(ConfigError::InvalidLocalAuthDisplayName);
                }
                Some(LocalAuthSettings {
                    tenant_id: Uuid::parse_str(&tenant_id)
                        .map_err(|_| ConfigError::InvalidLocalAuthId)?,
                    object_id: Uuid::parse_str(&object_id)
                        .map_err(|_| ConfigError::InvalidLocalAuthId)?,
                    display_name,
                })
            }
            _ => return Err(ConfigError::IncompleteLocalAuth),
        };

        let bind_address = env::var("HTTP_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress)?;
        let web_asset_dir = env::var_os("WEB_ASSET_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rust/static"));
        let guest_requests_per_minute = positive_u32("GUEST_REQUESTS_PER_MINUTE", 60)?;
        let provider_refreshes_per_hour = positive_u32("PROVIDER_REFRESHES_PER_HOUR", 6)?;
        let calculation_concurrency = positive_u32("CALCULATION_CONCURRENCY", 10)? as usize;

        Ok(Self {
            bind_address,
            environment,
            local_auth,
            web_asset_dir,
            guest_requests_per_minute,
            provider_refreshes_per_hour,
            calculation_concurrency,
        })
    }
}

fn positive_u32(name: &str, default: u32) -> Result<u32, ConfigError> {
    match env::var(name) {
        Ok(value) => value
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(ConfigError::InvalidQuota),
        Err(env::VarError::NotPresent) => Ok(default),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidQuota),
    }
}
