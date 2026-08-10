use std::{sync::Arc, time::Duration};

use thiserror::Error;

use crate::{
    calculation::{
        engine::{CalculationEngine, CalculationError},
        target_selector::CapabilityCatalog,
    },
    config::{AppEnvironment, Config, FORMULA_VERSION},
    persistence::{
        cosmos::CosmosProjectRepository,
        repository::{InMemoryProjectRepository, ProjectRepository, RepositoryError},
    },
    pricing::{
        local_fixture::{self, LocalFixtureError},
        repository::{InMemorySnapshotRepository, SnapshotRepositoryError},
    },
    rate_limit::TokenBucket,
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
    pub snapshots: InMemorySnapshotRepository,
    pub guest_rate_limit: TokenBucket,
    pub refresh_rate_limit: TokenBucket,
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
        Self::with_projects(config, projects, PersistenceBackend::Cosmos)
    }

    pub fn in_memory(config: Config) -> Result<Self, StateError> {
        Self::with_projects(
            config,
            Arc::new(InMemoryProjectRepository::new()),
            PersistenceBackend::MemoryLocal,
        )
    }

    fn with_projects(
        config: Config,
        projects: Arc<dyn ProjectRepository>,
        persistence_backend: PersistenceBackend,
    ) -> Result<Self, StateError> {
        let capabilities: CapabilityCatalog =
            serde_json::from_str(include_str!("../../app/catalogs/sql-mi-capabilities.json"))?;
        let calculations = CalculationEngine::new(Arc::new(capabilities), FORMULA_VERSION)?;
        let snapshots = InMemorySnapshotRepository::new();
        if config.environment == AppEnvironment::Local {
            let (aws, azure) = local_fixture::load()?;
            snapshots.put_aws(aws)?;
            snapshots.put_azure(azure)?;
        }
        let guest_rate_limit =
            TokenBucket::new(config.guest_requests_per_minute, Duration::from_secs(60));
        let refresh_rate_limit = TokenBucket::new(
            config.provider_refreshes_per_hour,
            Duration::from_secs(60 * 60),
        );
        let calculation_slots =
            Arc::new(tokio::sync::Semaphore::new(config.calculation_concurrency));

        Ok(Self {
            config: Arc::new(config),
            calculations,
            projects,
            persistence_backend,
            snapshots,
            guest_rate_limit,
            refresh_rate_limit,
            calculation_slots,
        })
    }
}
