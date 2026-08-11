use std::{env, net::SocketAddr, path::PathBuf};

use thiserror::Error;
use uuid::Uuid;

pub const APP_VERSION: &str = include_str!("../../VERSION").trim_ascii();
pub const FORMULA_VERSION: &str = "1.0.0";
pub const SCHEMA_VERSION: &str = "1.0.0";
pub const MAX_PROVIDER_REFRESHES_PER_HOUR: u32 = 10_000;

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
    pub cosmos: Option<CosmosSettings>,
    pub web_asset_dir: PathBuf,
    pub guest_requests_per_minute: u32,
    pub provider_refreshes_per_hour: u32,
    pub provider_max_response_bytes: usize,
    pub calculation_concurrency: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalAuthSettings {
    pub tenant_id: Uuid,
    pub object_id: Uuid,
    pub display_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CosmosSettings {
    pub endpoint: String,
    pub application_region: String,
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
    #[error("Cosmos settings are required outside APP_ENV=local")]
    MissingCosmosSettings,
    #[error("Cosmos settings are not supported with APP_ENV=local")]
    CosmosSettingsInLocalEnvironment,
    #[error("COSMOSDB_ENDPOINT must be an HTTPS Azure Cosmos DB account endpoint")]
    InvalidCosmosEndpoint,
    #[error("AZURE_REGION must be a valid Azure region name")]
    InvalidAzureRegion,
    #[error("request quota settings must be within supported positive ranges")]
    InvalidQuota,
    #[error("PROVIDER_MAX_RESPONSE_BYTES must be between 1 MiB and 256 MiB")]
    InvalidProviderResponseLimit,
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
        let cosmos = cosmos_settings(
            environment,
            env::var("COSMOSDB_ENDPOINT").ok(),
            env::var("AZURE_REGION").ok(),
        )?;
        let web_asset_dir = env::var_os("WEB_ASSET_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rust/static"));
        let guest_requests_per_minute = positive_u32("GUEST_REQUESTS_PER_MINUTE", 60)?;
        let provider_refreshes_per_hour =
            validate_provider_refresh_quota(positive_u32("PROVIDER_REFRESHES_PER_HOUR", 8)?)?;
        let provider_max_response_bytes =
            provider_response_limit(env::var("PROVIDER_MAX_RESPONSE_BYTES").ok().as_deref())?;
        let calculation_concurrency = positive_u32("CALCULATION_CONCURRENCY", 10)? as usize;

        Ok(Self {
            bind_address,
            environment,
            local_auth,
            cosmos,
            web_asset_dir,
            guest_requests_per_minute,
            provider_refreshes_per_hour,
            provider_max_response_bytes,
            calculation_concurrency,
        })
    }
}

fn cosmos_settings(
    environment: AppEnvironment,
    endpoint: Option<String>,
    application_region: Option<String>,
) -> Result<Option<CosmosSettings>, ConfigError> {
    if environment == AppEnvironment::Local {
        return if endpoint.is_none() && application_region.is_none() {
            Ok(None)
        } else {
            Err(ConfigError::CosmosSettingsInLocalEnvironment)
        };
    }
    let (endpoint, application_region) = match (endpoint, application_region) {
        (Some(endpoint), Some(application_region)) => (endpoint, application_region),
        _ => return Err(ConfigError::MissingCosmosSettings),
    };
    let parsed = reqwest::Url::parse(&endpoint).map_err(|_| ConfigError::InvalidCosmosEndpoint)?;
    let valid_endpoint = parsed.scheme() == "https"
        && parsed
            .host_str()
            .is_some_and(|host| host.ends_with(".documents.azure.com"))
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none();
    if !valid_endpoint {
        return Err(ConfigError::InvalidCosmosEndpoint);
    }
    if application_region.is_empty()
        || application_region.len() > 64
        || !application_region
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == ' ')
    {
        return Err(ConfigError::InvalidAzureRegion);
    }
    Ok(Some(CosmosSettings {
        endpoint,
        application_region,
    }))
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

fn validate_provider_refresh_quota(value: u32) -> Result<u32, ConfigError> {
    (1..=MAX_PROVIDER_REFRESHES_PER_HOUR)
        .contains(&value)
        .then_some(value)
        .ok_or(ConfigError::InvalidQuota)
}

fn provider_response_limit(value: Option<&str>) -> Result<usize, ConfigError> {
    const MIB: usize = 1024 * 1024;
    let value = value.unwrap_or("67108864");
    value
        .parse::<usize>()
        .ok()
        .filter(|value| (MIB..=256 * MIB).contains(value))
        .ok_or(ConfigError::InvalidProviderResponseLimit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_local_mode_requires_an_allowlisted_cosmos_endpoint() {
        assert!(matches!(
            cosmos_settings(AppEnvironment::Development, None, None),
            Err(ConfigError::MissingCosmosSettings)
        ));
        assert!(matches!(
            cosmos_settings(
                AppEnvironment::Development,
                Some("https://example.invalid/".to_owned()),
                Some("southafricanorth".to_owned()),
            ),
            Err(ConfigError::InvalidCosmosEndpoint)
        ));

        let settings = cosmos_settings(
            AppEnvironment::Production,
            Some("https://tco.documents.azure.com/".to_owned()),
            Some("southafricanorth".to_owned()),
        )
        .expect("valid production Cosmos settings")
        .expect("Cosmos settings");
        assert_eq!(settings.application_region, "southafricanorth");
    }

    #[test]
    fn local_mode_rejects_ignored_cosmos_settings() {
        assert!(matches!(
            cosmos_settings(
                AppEnvironment::Local,
                Some("https://tco.documents.azure.com/".to_owned()),
                Some("southafricanorth".to_owned()),
            ),
            Err(ConfigError::CosmosSettingsInLocalEnvironment)
        ));
    }

    #[test]
    fn provider_response_limit_is_bounded() {
        assert_eq!(
            provider_response_limit(None).expect("default response limit"),
            64 * 1024 * 1024
        );
        assert_eq!(
            provider_response_limit(Some("1048576")).expect("minimum response limit"),
            1024 * 1024
        );
        assert!(matches!(
            provider_response_limit(Some("1048575")),
            Err(ConfigError::InvalidProviderResponseLimit)
        ));
        assert!(matches!(
            provider_response_limit(Some("268435457")),
            Err(ConfigError::InvalidProviderResponseLimit)
        ));
    }

    #[test]
    fn provider_refresh_quota_is_bounded() {
        assert!(matches!(validate_provider_refresh_quota(1), Ok(1)));
        assert!(matches!(
            validate_provider_refresh_quota(MAX_PROVIDER_REFRESHES_PER_HOUR),
            Ok(MAX_PROVIDER_REFRESHES_PER_HOUR)
        ));
        assert!(matches!(
            validate_provider_refresh_quota(0),
            Err(ConfigError::InvalidQuota)
        ));
        assert!(matches!(
            validate_provider_refresh_quota(MAX_PROVIDER_REFRESHES_PER_HOUR + 1),
            Err(ConfigError::InvalidQuota)
        ));
    }
}
