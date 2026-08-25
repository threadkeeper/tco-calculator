use std::{sync::Arc, time::Duration};

use thiserror::Error;

use crate::{
    assistant::{
        foundry::FoundryModelClient,
        model::{DisabledModelClient, ModelClient},
    },
    calculation::{
        engine::{CalculationEngine, CalculationError},
        target_selector::CapabilityCatalog,
        vm_target_selector::{ManagedDiskCatalog, VmCapabilityCatalog},
    },
    config::{AppEnvironment, Config, FORMULA_VERSION},
    persistence::{
        calculator_launch::{CalculatorLaunchRepository, InMemoryCalculatorLaunchRepository},
        cosmos::{CosmosProjectRepository, CosmosSnapshotRepository},
        privacy_consent::{InMemoryPrivacyConsentRepository, PrivacyConsentRepository},
        project_share::{InMemoryProjectShareRepository, ProjectShareRepository},
        repository::{InMemoryProjectRepository, ProjectRepository, RepositoryError},
    },
    pricing::{
        coordinator::{AzurePricingCatalogs, PricingCoordinator},
        http::PricingHttpClient,
        loader::LivePricingLoader,
        local_fixture::{self, LocalFixtureError},
        provider::ProviderError,
        repository::{
            DurableSnapshotRepository, InMemorySnapshotRepository, RefreshLeaseRepository,
            SnapshotRepositoryError,
        },
    },
    rate_limit::{RefreshQuota, RefreshQuotaRepository, TokenBucket},
};

#[derive(Debug, Error)]
pub enum StateError {
    #[error("assistant model client could not be initialized")]
    Assistant,
    #[error("an embedded Azure capability catalog is invalid")]
    CapabilityCatalog(#[from] serde_json::Error),
    #[error("calculation engine could not be initialized")]
    Calculation(#[from] CalculationError),
    #[error(transparent)]
    LocalFixture(#[from] LocalFixtureError),
    #[error(transparent)]
    SnapshotRepository(#[from] SnapshotRepositoryError),
    #[error(transparent)]
    Repository(#[from] RepositoryError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceBackend {
    MemoryLocal,
    Cosmos,
}

/// Durable pricing-cache repositories, which are only available together on the
/// Cosmos-backed path and are all absent for the in-memory local path.
struct PricingCacheRepositories {
    snapshots: Arc<dyn DurableSnapshotRepository>,
    leases: Arc<dyn RefreshLeaseRepository>,
    quota: Arc<dyn RefreshQuotaRepository>,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub assistant_model: Arc<dyn ModelClient>,
    pub assistant_enabled: bool,
    pub assistant_slots: Arc<tokio::sync::Semaphore>,
    pub assistant_rate_limit: TokenBucket,
    pub calculations: CalculationEngine,
    pub calculator_launches: Arc<dyn CalculatorLaunchRepository>,
    pub projects: Arc<dyn ProjectRepository>,
    pub privacy_consents: Arc<dyn PrivacyConsentRepository>,
    pub project_shares: Arc<dyn ProjectShareRepository>,
    pub persistence_backend: PersistenceBackend,
    pub pricing: PricingCoordinator,
    pub live_pricing: Option<LivePricingLoader>,
    pub guest_rate_limit: TokenBucket,
    pub refresh_rate_limit: RefreshQuota,
    pub calculation_slots: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    pub async fn new(config: Config) -> Result<Self, StateError> {
        if config.environment == AppEnvironment::Local {
            return Self::in_memory(config);
        }
        let cosmos = config.cosmos.as_ref().ok_or(RepositoryError::Unavailable)?;
        let project_repository =
            CosmosProjectRepository::new(&cosmos.endpoint, &cosmos.application_region).await?;
        let projects: Arc<dyn ProjectRepository> = Arc::new(project_repository.clone());
        let privacy_consents: Arc<dyn PrivacyConsentRepository> =
            Arc::new(project_repository.clone());
        let project_shares: Arc<dyn ProjectShareRepository> = Arc::new(project_repository);
        let pricing_cache = Arc::new(
            CosmosSnapshotRepository::new(&cosmos.endpoint, &cosmos.application_region).await?,
        );
        let snapshots: Arc<dyn DurableSnapshotRepository> = pricing_cache.clone();
        let leases: Arc<dyn RefreshLeaseRepository> = pricing_cache.clone();
        let refresh_quota: Arc<dyn RefreshQuotaRepository> = pricing_cache;
        Self::with_projects(
            config,
            projects,
            privacy_consents,
            project_shares,
            Some(PricingCacheRepositories {
                snapshots,
                leases,
                quota: refresh_quota,
            }),
            PersistenceBackend::Cosmos,
        )
    }

    pub fn in_memory(config: Config) -> Result<Self, StateError> {
        Self::with_projects(
            config,
            Arc::new(InMemoryProjectRepository::new()),
            Arc::new(InMemoryPrivacyConsentRepository::new()),
            Arc::new(InMemoryProjectShareRepository::new()),
            None,
            PersistenceBackend::MemoryLocal,
        )
    }

    fn with_projects(
        config: Config,
        projects: Arc<dyn ProjectRepository>,
        privacy_consents: Arc<dyn PrivacyConsentRepository>,
        project_shares: Arc<dyn ProjectShareRepository>,
        pricing_cache: Option<PricingCacheRepositories>,
        persistence_backend: PersistenceBackend,
    ) -> Result<Self, StateError> {
        let (assistant_model, assistant_enabled, assistant_concurrency): (
            Arc<dyn ModelClient>,
            bool,
            usize,
        ) = match &config.assistant {
            Some(settings) => (
                Arc::new(
                    FoundryModelClient::new(
                        settings.endpoint.clone(),
                        &settings.deployment,
                        &settings.api_version,
                    )
                    .map_err(|_| StateError::Assistant)?,
                ),
                true,
                settings.concurrency,
            ),
            None => (Arc::new(DisabledModelClient), false, 1),
        };
        let (durable_snapshots, refresh_leases, refresh_quota_repository) = match pricing_cache {
            Some(cache) => (Some(cache.snapshots), Some(cache.leases), Some(cache.quota)),
            None => (None, None, None),
        };
        let capabilities = Arc::new(serde_json::from_str::<CapabilityCatalog>(include_str!(
            "../../app/catalogs/sql-mi-capabilities.json"
        ))?);
        let vm_capabilities = Arc::new(serde_json::from_str::<VmCapabilityCatalog>(include_str!(
            "../../app/catalogs/azure-vm-capabilities.json"
        ))?);
        let disk_capabilities = Arc::new(serde_json::from_str::<ManagedDiskCatalog>(
            include_str!("../../app/catalogs/azure-managed-disk-capabilities.json"),
        )?);
        let calculations = CalculationEngine::with_vm_catalogs(
            Arc::clone(&capabilities),
            Arc::clone(&vm_capabilities),
            Arc::clone(&disk_capabilities),
            FORMULA_VERSION,
        )?;
        let snapshots = InMemorySnapshotRepository::new();
        let live_pricing = if config.environment == AppEnvironment::Local {
            None
        } else {
            Some(LivePricingLoader::new(PricingHttpClient::new(
                config.provider_max_response_bytes,
            )?))
        };
        if config.environment == AppEnvironment::Local {
            let (aws, azure) = local_fixture::load_for_runtime()?;
            snapshots.put_aws(aws)?;
            snapshots.put_azure(azure)?;
        }
        let pricing = PricingCoordinator::new(
            snapshots.clone(),
            durable_snapshots,
            refresh_leases,
            live_pricing.clone(),
            AzurePricingCatalogs::new(
                Arc::clone(&capabilities),
                vm_capabilities,
                disk_capabilities,
            ),
        );
        let guest_rate_limit =
            TokenBucket::new(config.guest_requests_per_minute, Duration::from_secs(60));
        let refresh_rate_limit =
            RefreshQuota::new(config.provider_refreshes_per_hour, refresh_quota_repository);
        let calculation_slots =
            Arc::new(tokio::sync::Semaphore::new(config.calculation_concurrency));
        let assistant_rate_limit = TokenBucket::new(
            config.assistant_requests_per_minute,
            Duration::from_secs(60),
        );

        Ok(Self {
            config: Arc::new(config),
            assistant_model,
            assistant_enabled,
            assistant_slots: Arc::new(tokio::sync::Semaphore::new(assistant_concurrency)),
            assistant_rate_limit,
            calculations,
            calculator_launches: Arc::new(InMemoryCalculatorLaunchRepository::new()),
            projects,
            privacy_consents,
            project_shares,
            persistence_backend,
            pricing,
            live_pricing,
            guest_rate_limit,
            refresh_rate_limit,
            calculation_slots,
        })
    }
}
