use std::{sync::Arc, time::Duration};

use thiserror::Error;

use crate::{
    calculation::{
        engine::{CalculationEngine, CalculationError},
        target_selector::CapabilityCatalog,
    },
    config::{AppEnvironment, Config, FORMULA_VERSION},
    persistence::{
        cosmos::{CosmosProjectRepository, CosmosSnapshotRepository},
        repository::{InMemoryProjectRepository, ProjectRepository, RepositoryError},
    },
    pricing::{
        coordinator::PricingCoordinator,
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
    #[error("embedded SQL MI capability catalog is invalid")]
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

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub calculations: CalculationEngine,
    pub projects: Arc<dyn ProjectRepository>,
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
        let projects = Arc::new(
            CosmosProjectRepository::new(&cosmos.endpoint, &cosmos.application_region).await?,
        );
        let pricing_cache = Arc::new(
            CosmosSnapshotRepository::new(&cosmos.endpoint, &cosmos.application_region).await?,
        );
        let snapshots: Arc<dyn DurableSnapshotRepository> = pricing_cache.clone();
        let leases: Arc<dyn RefreshLeaseRepository> = pricing_cache.clone();
        let refresh_quota: Arc<dyn RefreshQuotaRepository> = pricing_cache;
        Self::with_projects(
            config,
            projects,
            Some(snapshots),
            Some(leases),
            Some(refresh_quota),
            PersistenceBackend::Cosmos,
        )
    }

    pub fn in_memory(config: Config) -> Result<Self, StateError> {
        Self::with_projects(
            config,
            Arc::new(InMemoryProjectRepository::new()),
            None,
            None,
            None,
            PersistenceBackend::MemoryLocal,
        )
    }

    fn with_projects(
        config: Config,
        projects: Arc<dyn ProjectRepository>,
        durable_snapshots: Option<Arc<dyn DurableSnapshotRepository>>,
        refresh_leases: Option<Arc<dyn RefreshLeaseRepository>>,
        refresh_quota_repository: Option<Arc<dyn RefreshQuotaRepository>>,
        persistence_backend: PersistenceBackend,
    ) -> Result<Self, StateError> {
        let capabilities = Arc::new(serde_json::from_str::<CapabilityCatalog>(include_str!(
            "../../app/catalogs/sql-mi-capabilities.json"
        ))?);
        let calculations = CalculationEngine::new(Arc::clone(&capabilities), FORMULA_VERSION)?;
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
            Arc::clone(&capabilities),
        );
        let guest_rate_limit =
            TokenBucket::new(config.guest_requests_per_minute, Duration::from_secs(60));
        let refresh_rate_limit =
            RefreshQuota::new(config.provider_refreshes_per_hour, refresh_quota_repository);
        let calculation_slots =
            Arc::new(tokio::sync::Semaphore::new(config.calculation_concurrency));

        Ok(Self {
            config: Arc::new(config),
            calculations,
            projects,
            persistence_backend,
            pricing,
            live_pricing,
            guest_rate_limit,
            refresh_rate_limit,
            calculation_slots,
        })
    }
}
