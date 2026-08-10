use std::{env, net::SocketAddr};

use thiserror::Error;

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
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("APP_ENV must be local, development, or production")]
    InvalidEnvironment,
    #[error("HTTP_BIND must be a valid socket address")]
    InvalidBindAddress,
    #[error("local authentication settings are allowed only when APP_ENV=local")]
    LocalAuthOutsideLocalEnvironment,
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

        let has_local_auth_setting = [
            "LOCAL_AUTH_TENANT_ID",
            "LOCAL_AUTH_OWNER_ID",
            "LOCAL_AUTH_DISPLAY_NAME",
        ]
        .iter()
        .any(|name| env::var_os(name).is_some());

        if environment != AppEnvironment::Local && has_local_auth_setting {
            return Err(ConfigError::LocalAuthOutsideLocalEnvironment);
        }

        let bind_address = env::var("HTTP_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_owned())
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress)?;

        Ok(Self {
            bind_address,
            environment,
        })
    }
}
