use std::{env, net::SocketAddr, path::PathBuf};

use reqwest::Url;
use thiserror::Error;
use uuid::Uuid;

pub const APP_VERSION: &str = include_str!("../../VERSION").trim_ascii();
pub const FORMULA_VERSION: &str = "1.3.0";
pub const SCHEMA_VERSION: &str = "1.0.0";
pub const FOUNDRY_API_VERSION: &str = "2024-10-21";
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
    pub assistant: Option<AssistantSettings>,
    pub web_asset_dir: PathBuf,
    pub guest_requests_per_minute: u32,
    pub provider_refreshes_per_hour: u32,
    pub provider_max_response_bytes: usize,
    pub calculation_concurrency: usize,
    pub assistant_requests_per_minute: u32,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssistantSettings {
    pub endpoint: Url,
    pub deployment: String,
    pub api_version: String,
    pub concurrency: usize,
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
    #[error("ASSISTANT_ENABLED must be true or false")]
    InvalidAssistantEnabled,
    #[error("Foundry assistant settings require ASSISTANT_ENABLED=true")]
    AssistantSettingsWhileDisabled,
    #[error("live assistant inference is not supported with APP_ENV=local")]
    AssistantInLocalEnvironment,
    #[error("enabled assistant requires endpoint, deployment, and API version together")]
    IncompleteAssistantSettings,
    #[error("FOUNDRY_ENDPOINT must be an HTTPS Azure Foundry endpoint")]
    InvalidFoundryEndpoint,
    #[error("FOUNDRY_MODEL_DEPLOYMENT must be a valid deployment name")]
    InvalidFoundryDeployment,
    #[error("FOUNDRY_API_VERSION must match the approved stable API version")]
    InvalidFoundryApiVersion,
    #[error("ASSISTANT_CONCURRENCY must be between 1 and 8")]
    InvalidAssistantConcurrency,
    #[error("ASSISTANT_REQUESTS_PER_MINUTE must be between 1 and 60")]
    InvalidAssistantRequestQuota,
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
        let assistant = assistant_settings(
            environment,
            env::var("ASSISTANT_ENABLED").ok().as_deref(),
            env::var("FOUNDRY_ENDPOINT").ok(),
            env::var("FOUNDRY_MODEL_DEPLOYMENT").ok(),
            env::var("FOUNDRY_API_VERSION").ok(),
            env::var("ASSISTANT_CONCURRENCY").ok().as_deref(),
        )?;
        let web_asset_dir = env::var_os("WEB_ASSET_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("rust/static"));
        let guest_requests_per_minute = positive_u32("GUEST_REQUESTS_PER_MINUTE", 60)?;
        let provider_refreshes_per_hour =
            validate_provider_refresh_quota(positive_u32("PROVIDER_REFRESHES_PER_HOUR", 40)?)?;
        let provider_max_response_bytes =
            provider_response_limit(env::var("PROVIDER_MAX_RESPONSE_BYTES").ok().as_deref())?;
        let calculation_concurrency = positive_u32("CALCULATION_CONCURRENCY", 10)? as usize;
        let assistant_requests_per_minute = env::var("ASSISTANT_REQUESTS_PER_MINUTE")
            .unwrap_or_else(|_| "10".to_owned())
            .parse::<u32>()
            .ok()
            .filter(|value| (1..=60).contains(value))
            .ok_or(ConfigError::InvalidAssistantRequestQuota)?;

        Ok(Self {
            bind_address,
            environment,
            local_auth,
            cosmos,
            assistant,
            web_asset_dir,
            guest_requests_per_minute,
            provider_refreshes_per_hour,
            provider_max_response_bytes,
            calculation_concurrency,
            assistant_requests_per_minute,
        })
    }
}

fn assistant_settings(
    environment: AppEnvironment,
    enabled: Option<&str>,
    endpoint: Option<String>,
    deployment: Option<String>,
    api_version: Option<String>,
    concurrency: Option<&str>,
) -> Result<Option<AssistantSettings>, ConfigError> {
    let enabled = match enabled.unwrap_or("false") {
        "true" => true,
        "false" => false,
        _ => return Err(ConfigError::InvalidAssistantEnabled),
    };
    let has_setting = endpoint.is_some()
        || deployment.is_some()
        || api_version.is_some()
        || concurrency.is_some();
    if !enabled {
        return if has_setting {
            Err(ConfigError::AssistantSettingsWhileDisabled)
        } else {
            Ok(None)
        };
    }
    if environment == AppEnvironment::Local {
        return Err(ConfigError::AssistantInLocalEnvironment);
    }

    let (endpoint, deployment, api_version) = match (endpoint, deployment, api_version) {
        (Some(endpoint), Some(deployment), Some(api_version)) => {
            (endpoint, deployment, api_version)
        }
        _ => return Err(ConfigError::IncompleteAssistantSettings),
    };
    let endpoint = Url::parse(&endpoint).map_err(|_| ConfigError::InvalidFoundryEndpoint)?;
    if !valid_foundry_endpoint(&endpoint) {
        return Err(ConfigError::InvalidFoundryEndpoint);
    }
    if !(1..=64).contains(&deployment.len())
        || !deployment
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_'))
    {
        return Err(ConfigError::InvalidFoundryDeployment);
    }
    if api_version != FOUNDRY_API_VERSION {
        return Err(ConfigError::InvalidFoundryApiVersion);
    }
    let concurrency = concurrency
        .unwrap_or("2")
        .parse::<usize>()
        .ok()
        .filter(|value| (1..=8).contains(value))
        .ok_or(ConfigError::InvalidAssistantConcurrency)?;

    Ok(Some(AssistantSettings {
        endpoint,
        deployment,
        api_version,
        concurrency,
    }))
}

fn valid_foundry_endpoint(endpoint: &Url) -> bool {
    endpoint.scheme() == "https"
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.port().is_none()
        && endpoint.path() == "/"
        && endpoint.query().is_none()
        && endpoint.fragment().is_none()
        && endpoint.host_str().is_some_and(|host| {
            host.ends_with(".openai.azure.com") || host.ends_with(".services.ai.azure.com")
        })
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

    #[test]
    fn assistant_is_off_by_default_and_rejects_ignored_settings() {
        assert_eq!(
            assistant_settings(AppEnvironment::Production, None, None, None, None, None)
                .expect("disabled by default"),
            None
        );
        assert!(matches!(
            assistant_settings(
                AppEnvironment::Production,
                Some("false"),
                Some("https://tco.openai.azure.com/".to_owned()),
                None,
                None,
                None,
            ),
            Err(ConfigError::AssistantSettingsWhileDisabled)
        ));
    }

    #[test]
    fn enabled_assistant_requires_the_pinned_private_data_plane_contract() {
        let settings = assistant_settings(
            AppEnvironment::Production,
            Some("true"),
            Some("https://tco.services.ai.azure.com/".to_owned()),
            Some("tco-model-router".to_owned()),
            Some(FOUNDRY_API_VERSION.to_owned()),
            Some("3"),
        )
        .expect("valid settings")
        .expect("enabled assistant");
        assert_eq!(settings.deployment, "tco-model-router");
        assert_eq!(settings.concurrency, 3);

        assert!(matches!(
            assistant_settings(
                AppEnvironment::Local,
                Some("true"),
                Some("https://tco.services.ai.azure.com/".to_owned()),
                Some("tco-model-router".to_owned()),
                Some(FOUNDRY_API_VERSION.to_owned()),
                None,
            ),
            Err(ConfigError::AssistantInLocalEnvironment)
        ));
        assert!(matches!(
            assistant_settings(
                AppEnvironment::Production,
                Some("true"),
                Some("https://example.invalid/".to_owned()),
                Some("tco-model-router".to_owned()),
                Some(FOUNDRY_API_VERSION.to_owned()),
                None,
            ),
            Err(ConfigError::InvalidFoundryEndpoint)
        ));
    }
}
