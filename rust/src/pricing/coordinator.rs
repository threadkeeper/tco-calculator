use std::{collections::HashMap, fmt::Write, sync::Arc, time::Duration};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::calculation::{
    target_selector::CapabilityCatalog,
    vm_target_selector::{ManagedDiskCatalog, VmCapabilityCatalog},
};

use super::{
    live::PARSER_SCHEMA_VERSION,
    loader::LivePricingLoader,
    provider::{Provider, ProviderError, ResolutionStatus},
    repository::{
        DurableSnapshotRepository, InMemorySnapshotRepository, RefreshLeaseDecision,
        RefreshLeaseOutcome, RefreshLeaseRepository, SnapshotRepositoryError,
    },
    snapshot::{AwsPriceSnapshot, AzurePriceSnapshot, utc_now_rfc3339},
};

const AWS_SERVICE: &str = "Amazon EC2, RDS, and EBS";
const AWS_FILTER: &str = "current SQL Server compute and reviewed storage meters";
const AZURE_SERVICE: &str = "Azure SQL Managed Instance, Virtual Machines, and Managed Disks";
const AZURE_FILTER: &str =
    "reviewed SQL MI, Windows VM, and managed-disk capability and price dimensions";
const DISTRIBUTED_WAIT_BUDGET: Duration = Duration::from_secs(120);
const INITIAL_LEASE_BACKOFF: Duration = Duration::from_millis(250);
const MAX_LEASE_BACKOFF: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum PricingCoordinatorError {
    #[error(transparent)]
    Repository(#[from] SnapshotRepositoryError),
}

#[derive(Debug)]
pub struct SnapshotResolution<T> {
    pub snapshot: Option<Arc<T>>,
    pub warnings: Vec<String>,
}

#[async_trait]
trait SnapshotLoader: Send + Sync {
    async fn load_aws_snapshot(
        &self,
        source_region: &str,
    ) -> Result<AwsPriceSnapshot, ProviderError>;

    async fn load_azure_snapshot(
        &self,
        target_region: &str,
        catalogs: &AzurePricingCatalogs,
    ) -> Result<AzurePriceSnapshot, ProviderError>;
}

#[async_trait]
impl SnapshotLoader for LivePricingLoader {
    async fn load_aws_snapshot(
        &self,
        source_region: &str,
    ) -> Result<AwsPriceSnapshot, ProviderError> {
        LivePricingLoader::load_aws_snapshot(self, source_region).await
    }

    async fn load_azure_snapshot(
        &self,
        target_region: &str,
        catalogs: &AzurePricingCatalogs,
    ) -> Result<AzurePriceSnapshot, ProviderError> {
        LivePricingLoader::load_azure_snapshot(
            self,
            target_region,
            &catalogs.sql_mi,
            &catalogs.virtual_machines,
            &catalogs.managed_disks,
        )
        .await
    }
}

#[derive(Clone)]
pub struct AzurePricingCatalogs {
    sql_mi: Arc<CapabilityCatalog>,
    virtual_machines: Arc<VmCapabilityCatalog>,
    managed_disks: Arc<ManagedDiskCatalog>,
}

impl AzurePricingCatalogs {
    pub fn new(
        sql_mi: Arc<CapabilityCatalog>,
        virtual_machines: Arc<VmCapabilityCatalog>,
        managed_disks: Arc<ManagedDiskCatalog>,
    ) -> Self {
        Self {
            sql_mi,
            virtual_machines,
            managed_disks,
        }
    }
}

#[derive(Clone)]
pub struct PricingCoordinator {
    repository: InMemorySnapshotRepository,
    durable: Option<Arc<dyn DurableSnapshotRepository>>,
    leases: Option<Arc<dyn RefreshLeaseRepository>>,
    loader: Option<Arc<dyn SnapshotLoader>>,
    azure_catalogs: AzurePricingCatalogs,
    aws_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AwsPriceSnapshot>>>>>,
    azure_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AzurePriceSnapshot>>>>>,
}

impl PricingCoordinator {
    pub fn new(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<dyn DurableSnapshotRepository>>,
        leases: Option<Arc<dyn RefreshLeaseRepository>>,
        loader: Option<LivePricingLoader>,
        azure_catalogs: AzurePricingCatalogs,
    ) -> Self {
        Self::with_loader(
            repository,
            durable,
            leases,
            loader.map(|loader| Arc::new(loader) as Arc<dyn SnapshotLoader>),
            azure_catalogs,
        )
    }

    fn with_loader(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<dyn DurableSnapshotRepository>>,
        leases: Option<Arc<dyn RefreshLeaseRepository>>,
        loader: Option<Arc<dyn SnapshotLoader>>,
        azure_catalogs: AzurePricingCatalogs,
    ) -> Self {
        Self {
            repository,
            durable,
            leases,
            loader,
            azure_catalogs,
            aws_flights: Arc::new(Mutex::new(HashMap::new())),
            azure_flights: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn resolve_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<SnapshotResolution<AwsPriceSnapshot>, PricingCoordinatorError> {
        let snapshot = self.find_aws_snapshot(currency, source_region).await?;
        Ok(resolve_cached(snapshot, Provider::Aws))
    }

    pub async fn resolve_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<SnapshotResolution<AzurePriceSnapshot>, PricingCoordinatorError> {
        let snapshot = self.find_azure_snapshot(currency, target_region).await?;
        Ok(resolve_cached(snapshot, Provider::Azure))
    }

    pub async fn get_aws(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, PricingCoordinatorError> {
        let fallback = self.repository.get_aws(snapshot_id)?;
        let Some(durable) = &self.durable else {
            return Ok(fallback);
        };
        if let Some(snapshot) = self.repository.get_aws_hot(snapshot_id)? {
            return Ok(Some(snapshot));
        }
        match durable.get_aws(snapshot_id).await {
            Ok(Some(snapshot)) => self.cache_durable_aws(snapshot),
            Ok(None) => Ok(fallback),
            Err(_) if fallback.is_some() => Ok(fallback),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn get_azure(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, PricingCoordinatorError> {
        let fallback = self.repository.get_azure(snapshot_id)?;
        let Some(durable) = &self.durable else {
            return Ok(fallback);
        };
        if let Some(snapshot) = self.repository.get_azure_hot(snapshot_id)? {
            return Ok(Some(snapshot));
        }
        match durable.get_azure(snapshot_id).await {
            Ok(Some(snapshot)) => self.cache_durable_azure(snapshot),
            Ok(None) => Ok(fallback),
            Err(_) if fallback.is_some() => Ok(fallback),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn list_latest_aws(
        &self,
    ) -> Result<Vec<Arc<AwsPriceSnapshot>>, PricingCoordinatorError> {
        let fallback = self.repository.list_latest_aws()?;
        let Some(durable) = &self.durable else {
            return Ok(fallback);
        };
        match durable.list_latest_aws().await {
            Ok(snapshots) => {
                for mut snapshot in snapshots {
                    snapshot.metadata.status = super::provider::ResolutionStatus::Cached;
                    self.repository.put_aws(snapshot)?;
                }
                let snapshots = self.repository.list_latest_aws()?;
                if snapshots.is_empty() {
                    Ok(fallback)
                } else {
                    Ok(snapshots)
                }
            }
            Err(_) if !fallback.is_empty() => Ok(fallback),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn refresh_aws(
        &self,
        currency: &str,
        source_region: &str,
        target_region: &str,
    ) -> Result<SnapshotResolution<AwsPriceSnapshot>, PricingCoordinatorError> {
        let Some(loader) = &self.loader else {
            let snapshot = self.find_aws_snapshot(currency, source_region).await?;
            return Ok(refresh_not_configured(snapshot, Provider::Aws));
        };
        let key = RefreshKey::new(
            Provider::Aws,
            currency,
            Some(source_region),
            target_region,
            AWS_SERVICE,
            AWS_FILTER,
        );
        let flight = self
            .start_aws_flight(
                key,
                Arc::clone(loader),
                currency.to_owned(),
                source_region.to_owned(),
            )
            .await;
        match flight.wait().await {
            Ok(snapshot) => Ok(SnapshotResolution {
                snapshot: Some(snapshot),
                warnings: Vec::new(),
            }),
            Err(RefreshFailure::Provider(error)) => {
                let snapshot = self.find_aws_snapshot(currency, source_region).await?;
                Ok(provider_fallback(snapshot, Provider::Aws, error))
            }
            Err(RefreshFailure::Repository) => {
                let snapshot = match self.find_aws_snapshot(currency, source_region).await {
                    Ok(snapshot) => snapshot,
                    Err(_) => self.repository.find_aws(currency, source_region)?,
                };
                Ok(repository_fallback(snapshot, Provider::Aws))
            }
        }
    }

    pub async fn refresh_azure(
        &self,
        currency: &str,
        source_region: Option<&str>,
        target_region: &str,
    ) -> Result<SnapshotResolution<AzurePriceSnapshot>, PricingCoordinatorError> {
        let Some(loader) = &self.loader else {
            let snapshot = self.find_azure_snapshot(currency, target_region).await?;
            return Ok(refresh_not_configured(snapshot, Provider::Azure));
        };
        let key = RefreshKey::new(
            Provider::Azure,
            currency,
            source_region,
            target_region,
            AZURE_SERVICE,
            AZURE_FILTER,
        );
        let flight = self
            .start_azure_flight(
                key,
                Arc::clone(loader),
                currency.to_owned(),
                target_region.to_owned(),
            )
            .await;
        match flight.wait().await {
            Ok(snapshot) => Ok(SnapshotResolution {
                snapshot: Some(snapshot),
                warnings: Vec::new(),
            }),
            Err(RefreshFailure::Provider(error)) => {
                let snapshot = self.find_azure_snapshot(currency, target_region).await?;
                Ok(provider_fallback(snapshot, Provider::Azure, error))
            }
            Err(RefreshFailure::Repository) => {
                let snapshot = match self.find_azure_snapshot(currency, target_region).await {
                    Ok(snapshot) => snapshot,
                    Err(_) => self.repository.find_azure(currency, target_region)?,
                };
                Ok(repository_fallback(snapshot, Provider::Azure))
            }
        }
    }

    async fn find_aws_snapshot(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, PricingCoordinatorError> {
        let fallback = self.repository.find_aws(currency, source_region)?;
        let Some(durable) = &self.durable else {
            return Ok(fallback);
        };
        if let Some(snapshot) = self.repository.find_aws_hot(currency, source_region)? {
            return Ok(Some(snapshot));
        }
        match durable.find_aws(currency, source_region).await {
            Ok(Some(snapshot)) => self.cache_durable_aws(snapshot),
            Ok(None) => Ok(fallback),
            Err(_) if fallback.is_some() => Ok(fallback),
            Err(error) => Err(error.into()),
        }
    }

    async fn find_azure_snapshot(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, PricingCoordinatorError> {
        let fallback = self.repository.find_azure(currency, target_region)?;
        let Some(durable) = &self.durable else {
            return Ok(fallback);
        };
        if let Some(snapshot) = self.repository.find_azure_hot(currency, target_region)? {
            return Ok(Some(snapshot));
        }
        match durable.find_azure(currency, target_region).await {
            Ok(Some(snapshot)) => self.cache_durable_azure(snapshot),
            Ok(None) => Ok(fallback),
            Err(_) if fallback.is_some() => Ok(fallback),
            Err(error) => Err(error.into()),
        }
    }

    fn cache_durable_aws(
        &self,
        mut snapshot: AwsPriceSnapshot,
    ) -> Result<Option<Arc<AwsPriceSnapshot>>, PricingCoordinatorError> {
        let snapshot_id = snapshot.metadata.snapshot_id.clone();
        snapshot.metadata.status = super::provider::ResolutionStatus::Cached;
        self.repository.put_aws(snapshot)?;
        self.repository.get_aws(&snapshot_id).map_err(Into::into)
    }

    fn cache_durable_azure(
        &self,
        mut snapshot: AzurePriceSnapshot,
    ) -> Result<Option<Arc<AzurePriceSnapshot>>, PricingCoordinatorError> {
        let snapshot_id = snapshot.metadata.snapshot_id.clone();
        snapshot.metadata.status = super::provider::ResolutionStatus::Cached;
        self.repository.put_azure(snapshot)?;
        self.repository.get_azure(&snapshot_id).map_err(Into::into)
    }

    async fn start_aws_flight(
        &self,
        key: RefreshKey,
        loader: Arc<dyn SnapshotLoader>,
        currency: String,
        source_region: String,
    ) -> Arc<Flight<AwsPriceSnapshot>> {
        let mut flights = self.aws_flights.lock().await;
        if let Some(flight) = flights.get(&key) {
            return Arc::clone(flight);
        }
        let flight = Arc::new(Flight::new());
        flights.insert(key.clone(), Arc::clone(&flight));
        drop(flights);

        let repository = self.repository.clone();
        let durable = self.durable.clone();
        let leases = self.leases.clone();
        let flights = Arc::clone(&self.aws_flights);
        let task_flight = Arc::clone(&flight);
        let cache_key_sha256 = key.sha256();
        tokio::spawn(async move {
            let result = refresh_aws_task(
                RefreshTaskContext {
                    repository: &repository,
                    durable: durable.as_deref(),
                    leases: leases.as_deref(),
                    loader: loader.as_ref(),
                    cache_key_sha256: &cache_key_sha256,
                },
                &currency,
                &source_region,
            )
            .await;
            task_flight.complete(result).await;
            flights.lock().await.remove(&key);
        });
        flight
    }

    async fn start_azure_flight(
        &self,
        key: RefreshKey,
        loader: Arc<dyn SnapshotLoader>,
        currency: String,
        target_region: String,
    ) -> Arc<Flight<AzurePriceSnapshot>> {
        let mut flights = self.azure_flights.lock().await;
        if let Some(flight) = flights.get(&key) {
            return Arc::clone(flight);
        }
        let flight = Arc::new(Flight::new());
        flights.insert(key.clone(), Arc::clone(&flight));
        drop(flights);

        let repository = self.repository.clone();
        let durable = self.durable.clone();
        let leases = self.leases.clone();
        let azure_catalogs = self.azure_catalogs.clone();
        let flights = Arc::clone(&self.azure_flights);
        let task_flight = Arc::clone(&flight);
        let cache_key_sha256 = key.sha256();
        tokio::spawn(async move {
            let result = refresh_azure_task(
                RefreshTaskContext {
                    repository: &repository,
                    durable: durable.as_deref(),
                    leases: leases.as_deref(),
                    loader: loader.as_ref(),
                    cache_key_sha256: &cache_key_sha256,
                },
                &azure_catalogs,
                &currency,
                &target_region,
            )
            .await;
            task_flight.complete(result).await;
            flights.lock().await.remove(&key);
        });
        flight
    }
}

#[derive(Clone, Copy)]
struct RefreshTaskContext<'a> {
    repository: &'a InMemorySnapshotRepository,
    durable: Option<&'a dyn DurableSnapshotRepository>,
    leases: Option<&'a dyn RefreshLeaseRepository>,
    loader: &'a dyn SnapshotLoader,
    cache_key_sha256: &'a str,
}

#[derive(Clone, Copy)]
struct DistributedRefreshContext<'a> {
    repository: &'a InMemorySnapshotRepository,
    durable: &'a dyn DurableSnapshotRepository,
    leases: &'a dyn RefreshLeaseRepository,
    loader: &'a dyn SnapshotLoader,
    cache_key_sha256: &'a str,
}

impl<'a> RefreshTaskContext<'a> {
    fn distributed(self) -> Option<DistributedRefreshContext<'a>> {
        Some(DistributedRefreshContext {
            repository: self.repository,
            durable: self.durable?,
            leases: self.leases?,
            loader: self.loader,
            cache_key_sha256: self.cache_key_sha256,
        })
    }
}

async fn refresh_aws_task(
    context: RefreshTaskContext<'_>,
    currency: &str,
    source_region: &str,
) -> Result<Arc<AwsPriceSnapshot>, RefreshFailure> {
    let deadline = tokio::time::Instant::now() + DISTRIBUTED_WAIT_BUDGET;
    let Some(distributed) = context.distributed() else {
        let snapshot = load_aws_before(context.loader, source_region, deadline)
            .await
            .map_err(RefreshFailure::Provider)?;
        return persist_aws(context.repository, context.durable, snapshot)
            .await
            .map_err(|_| RefreshFailure::Repository);
    };
    let request_started_at = utc_now_rfc3339().map_err(|_| RefreshFailure::Repository)?;
    let owner_token = Uuid::new_v4().to_string();
    let mut backoff = INITIAL_LEASE_BACKOFF;
    loop {
        let decision =
            claim_before_deadline(distributed, &owner_token, &request_started_at, deadline).await?;
        match decision {
            RefreshLeaseDecision::Acquired => {
                return owner_refresh_aws(distributed, &owner_token, source_region, deadline).await;
            }
            RefreshLeaseDecision::Succeeded(snapshot_id) => {
                let snapshot =
                    tokio::time::timeout_at(deadline, distributed.durable.get_aws(&snapshot_id))
                        .await
                        .map_err(|_| refresh_timeout())?
                        .map_err(|_| RefreshFailure::Repository)?
                        .filter(|snapshot| snapshot.matches_scope(currency, source_region))
                        .ok_or(RefreshFailure::Repository)?;
                return cache_waiter_aws(distributed.repository, snapshot);
            }
            RefreshLeaseDecision::Failed(error) => {
                return Err(RefreshFailure::Provider(error));
            }
            RefreshLeaseDecision::Pending => wait_for_lease(&mut backoff, deadline).await?,
        }
    }
}

async fn owner_refresh_aws(
    context: DistributedRefreshContext<'_>,
    owner_token: &str,
    source_region: &str,
    deadline: tokio::time::Instant,
) -> Result<Arc<AwsPriceSnapshot>, RefreshFailure> {
    match load_aws_before(context.loader, source_region, deadline).await {
        Ok(snapshot) => {
            let snapshot = context
                .durable
                .put_aws(&snapshot)
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            context
                .leases
                .publish_refresh_lease(
                    context.cache_key_sha256,
                    owner_token,
                    &RefreshLeaseOutcome::Succeeded(snapshot.metadata.snapshot_id.clone()),
                )
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            context
                .repository
                .put_aws(snapshot.clone())
                .map_err(|_| RefreshFailure::Repository)?;
            Ok(Arc::new(snapshot))
        }
        Err(error) => {
            context
                .leases
                .publish_refresh_lease(
                    context.cache_key_sha256,
                    owner_token,
                    &RefreshLeaseOutcome::Failed(error),
                )
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            Err(RefreshFailure::Provider(error))
        }
    }
}

async fn refresh_azure_task(
    context: RefreshTaskContext<'_>,
    catalogs: &AzurePricingCatalogs,
    currency: &str,
    target_region: &str,
) -> Result<Arc<AzurePriceSnapshot>, RefreshFailure> {
    let deadline = tokio::time::Instant::now() + DISTRIBUTED_WAIT_BUDGET;
    let Some(distributed) = context.distributed() else {
        let snapshot = load_azure_before(context.loader, target_region, catalogs, deadline)
            .await
            .map_err(RefreshFailure::Provider)?;
        return persist_azure(context.repository, context.durable, snapshot)
            .await
            .map_err(|_| RefreshFailure::Repository);
    };
    let request_started_at = utc_now_rfc3339().map_err(|_| RefreshFailure::Repository)?;
    let owner_token = Uuid::new_v4().to_string();
    let mut backoff = INITIAL_LEASE_BACKOFF;
    loop {
        let decision =
            claim_before_deadline(distributed, &owner_token, &request_started_at, deadline).await?;
        match decision {
            RefreshLeaseDecision::Acquired => {
                return owner_refresh_azure(
                    distributed,
                    catalogs,
                    &owner_token,
                    target_region,
                    deadline,
                )
                .await;
            }
            RefreshLeaseDecision::Succeeded(snapshot_id) => {
                let snapshot =
                    tokio::time::timeout_at(deadline, distributed.durable.get_azure(&snapshot_id))
                        .await
                        .map_err(|_| refresh_timeout())?
                        .map_err(|_| RefreshFailure::Repository)?
                        .filter(|snapshot| snapshot.matches_scope(currency, target_region))
                        .ok_or(RefreshFailure::Repository)?;
                return cache_waiter_azure(distributed.repository, snapshot);
            }
            RefreshLeaseDecision::Failed(error) => {
                return Err(RefreshFailure::Provider(error));
            }
            RefreshLeaseDecision::Pending => wait_for_lease(&mut backoff, deadline).await?,
        }
    }
}

async fn owner_refresh_azure(
    context: DistributedRefreshContext<'_>,
    catalogs: &AzurePricingCatalogs,
    owner_token: &str,
    target_region: &str,
    deadline: tokio::time::Instant,
) -> Result<Arc<AzurePriceSnapshot>, RefreshFailure> {
    match load_azure_before(context.loader, target_region, catalogs, deadline).await {
        Ok(snapshot) => {
            context
                .durable
                .put_azure(&snapshot)
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            context
                .leases
                .publish_refresh_lease(
                    context.cache_key_sha256,
                    owner_token,
                    &RefreshLeaseOutcome::Succeeded(snapshot.metadata.snapshot_id.clone()),
                )
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            context
                .repository
                .put_azure(snapshot.clone())
                .map_err(|_| RefreshFailure::Repository)?;
            Ok(Arc::new(snapshot))
        }
        Err(error) => {
            context
                .leases
                .publish_refresh_lease(
                    context.cache_key_sha256,
                    owner_token,
                    &RefreshLeaseOutcome::Failed(error),
                )
                .await
                .map_err(|_| RefreshFailure::Repository)?;
            Err(RefreshFailure::Provider(error))
        }
    }
}

async fn claim_before_deadline(
    context: DistributedRefreshContext<'_>,
    owner_token: &str,
    request_started_at: &str,
    deadline: tokio::time::Instant,
) -> Result<RefreshLeaseDecision, RefreshFailure> {
    tokio::time::timeout_at(
        deadline,
        context.leases.claim_refresh_lease(
            context.cache_key_sha256,
            owner_token,
            request_started_at,
        ),
    )
    .await
    .map_err(|_| refresh_timeout())?
    .map_err(|_| RefreshFailure::Repository)
}

async fn load_aws_before(
    loader: &dyn SnapshotLoader,
    source_region: &str,
    deadline: tokio::time::Instant,
) -> Result<AwsPriceSnapshot, ProviderError> {
    tokio::time::timeout_at(deadline, loader.load_aws_snapshot(source_region))
        .await
        .unwrap_or(Err(ProviderError::TemporarilyUnavailable))
}

async fn load_azure_before(
    loader: &dyn SnapshotLoader,
    target_region: &str,
    catalogs: &AzurePricingCatalogs,
    deadline: tokio::time::Instant,
) -> Result<AzurePriceSnapshot, ProviderError> {
    tokio::time::timeout_at(
        deadline,
        loader.load_azure_snapshot(target_region, catalogs),
    )
    .await
    .unwrap_or(Err(ProviderError::TemporarilyUnavailable))
}

fn refresh_timeout() -> RefreshFailure {
    RefreshFailure::Provider(ProviderError::TemporarilyUnavailable)
}

fn cache_waiter_aws(
    repository: &InMemorySnapshotRepository,
    mut snapshot: AwsPriceSnapshot,
) -> Result<Arc<AwsPriceSnapshot>, RefreshFailure> {
    let snapshot_id = snapshot.metadata.snapshot_id.clone();
    snapshot.metadata.status = ResolutionStatus::Cached;
    repository
        .put_aws(snapshot)
        .map_err(|_| RefreshFailure::Repository)?;
    repository
        .get_aws(&snapshot_id)
        .map_err(|_| RefreshFailure::Repository)?
        .ok_or(RefreshFailure::Repository)
}

fn cache_waiter_azure(
    repository: &InMemorySnapshotRepository,
    mut snapshot: AzurePriceSnapshot,
) -> Result<Arc<AzurePriceSnapshot>, RefreshFailure> {
    let snapshot_id = snapshot.metadata.snapshot_id.clone();
    snapshot.metadata.status = ResolutionStatus::Cached;
    repository
        .put_azure(snapshot)
        .map_err(|_| RefreshFailure::Repository)?;
    repository
        .get_azure(&snapshot_id)
        .map_err(|_| RefreshFailure::Repository)?
        .ok_or(RefreshFailure::Repository)
}

async fn wait_for_lease(
    backoff: &mut Duration,
    deadline: tokio::time::Instant,
) -> Result<(), RefreshFailure> {
    let now = tokio::time::Instant::now();
    if now >= deadline {
        return Err(refresh_timeout());
    }
    tokio::time::sleep((*backoff).min(deadline - now)).await;
    *backoff = backoff.saturating_mul(2).min(MAX_LEASE_BACKOFF);
    if tokio::time::Instant::now() >= deadline {
        Err(refresh_timeout())
    } else {
        Ok(())
    }
}

async fn persist_aws(
    repository: &InMemorySnapshotRepository,
    durable: Option<&dyn DurableSnapshotRepository>,
    mut snapshot: AwsPriceSnapshot,
) -> Result<Arc<AwsPriceSnapshot>, SnapshotRepositoryError> {
    if let Some(durable) = durable {
        snapshot = durable.put_aws(&snapshot).await?;
    }
    repository.put_aws(snapshot.clone())?;
    Ok(Arc::new(snapshot))
}

async fn persist_azure(
    repository: &InMemorySnapshotRepository,
    durable: Option<&dyn DurableSnapshotRepository>,
    snapshot: AzurePriceSnapshot,
) -> Result<Arc<AzurePriceSnapshot>, SnapshotRepositoryError> {
    if let Some(durable) = durable {
        durable.put_azure(&snapshot).await?;
    }
    repository.put_azure(snapshot.clone())?;
    Ok(Arc::new(snapshot))
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RefreshKey {
    provider: Provider,
    currency: String,
    source_region: Option<String>,
    target_region: String,
    service: &'static str,
    normalized_filter: &'static str,
    parser_schema_version: &'static str,
}

impl RefreshKey {
    fn new(
        provider: Provider,
        currency: &str,
        source_region: Option<&str>,
        target_region: &str,
        service: &'static str,
        normalized_filter: &'static str,
    ) -> Self {
        Self {
            provider,
            currency: currency.to_owned(),
            source_region: source_region.map(str::to_owned),
            target_region: target_region.to_owned(),
            service,
            normalized_filter,
            parser_schema_version: PARSER_SCHEMA_VERSION,
        }
    }

    fn sha256(&self) -> String {
        let mut hasher = Sha256::new();
        hash_component(
            &mut hasher,
            Some(match self.provider {
                Provider::Aws => "aws",
                Provider::Azure => "azure",
            }),
        );
        hash_component(&mut hasher, Some(&self.currency));
        hash_component(&mut hasher, self.source_region.as_deref());
        hash_component(&mut hasher, Some(&self.target_region));
        hash_component(&mut hasher, Some(self.service));
        hash_component(&mut hasher, Some(self.normalized_filter));
        hash_component(&mut hasher, Some(self.parser_schema_version));
        let mut encoded = String::with_capacity(64);
        for byte in hasher.finalize() {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }
}

fn hash_component(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
        None => hasher.update([0]),
    }
}

struct Flight<T> {
    result: Mutex<Option<Result<Arc<T>, RefreshFailure>>>,
    ready: Notify,
}

impl<T> Flight<T> {
    fn new() -> Self {
        Self {
            result: Mutex::new(None),
            ready: Notify::new(),
        }
    }

    async fn complete(&self, result: Result<Arc<T>, RefreshFailure>) {
        *self.result.lock().await = Some(result);
        self.ready.notify_waiters();
    }

    async fn wait(&self) -> Result<Arc<T>, RefreshFailure> {
        loop {
            let ready = self.ready.notified();
            if let Some(result) = self.result.lock().await.clone() {
                return result;
            }
            ready.await;
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum RefreshFailure {
    Provider(ProviderError),
    Repository,
}

fn resolve_cached<T>(snapshot: Option<Arc<T>>, provider: Provider) -> SnapshotResolution<T> {
    let warnings = if snapshot.is_some() {
        Vec::new()
    } else {
        vec![format!(
            "No usable {} price snapshot exists for this scope. Refresh prices to retrieve current public prices.",
            provider_name(provider)
        )]
    };
    SnapshotResolution { snapshot, warnings }
}

fn refresh_not_configured<T>(
    snapshot: Option<Arc<T>>,
    provider: Provider,
) -> SnapshotResolution<T> {
    let warning = if snapshot.is_some() {
        "The immutable local fixture cannot be refreshed; the existing snapshot was returned."
            .to_owned()
    } else {
        format!(
            "Live {} price transport is unavailable in this environment and no usable snapshot exists.",
            provider_name(provider)
        )
    };
    SnapshotResolution {
        snapshot,
        warnings: vec![warning],
    }
}

fn provider_fallback<T>(
    snapshot: Option<Arc<T>>,
    provider: Provider,
    error: ProviderError,
) -> SnapshotResolution<T> {
    let suffix = if snapshot.is_some() {
        "the most recent usable snapshot was returned."
    } else {
        "no usable snapshot exists."
    };
    SnapshotResolution {
        snapshot,
        warnings: vec![format!(
            "Live {} price refresh failed ({}); {suffix}",
            provider_name(provider),
            provider_error_code(error)
        )],
    }
}

fn repository_fallback<T>(snapshot: Option<Arc<T>>, provider: Provider) -> SnapshotResolution<T> {
    let suffix = if snapshot.is_some() {
        "the most recent usable snapshot was returned."
    } else {
        "no usable snapshot exists."
    };
    SnapshotResolution {
        snapshot,
        warnings: vec![format!(
            "The durable {} price cache is temporarily unavailable; {suffix}",
            provider_name(provider)
        )],
    }
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Aws => "AWS",
        Provider::Azure => "Azure",
    }
}

fn provider_error_code(error: ProviderError) -> &'static str {
    match error {
        ProviderError::NotFound => "price_not_found",
        ProviderError::Unsupported => "scope_unsupported",
        ProviderError::TemporarilyUnavailable => "provider_temporarily_unavailable",
        ProviderError::SchemaChanged => "provider_schema_changed",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
    use tokio::sync::Semaphore;

    use super::*;
    use crate::pricing::{
        local_fixture,
        provider::ResolutionStatus,
        snapshot::{SnapshotCreationMetadata, utc_now_rfc3339},
    };

    #[derive(Clone, Copy)]
    enum TestBehavior {
        Success,
        Failure(ProviderError),
    }

    struct TestLoader {
        aws_behavior: TestBehavior,
        azure_behavior: TestBehavior,
        aws_calls: AtomicUsize,
        azure_calls: AtomicUsize,
        aws_gate: Option<Arc<Semaphore>>,
        aws_started: Notify,
    }

    #[derive(Default)]
    struct TestDurableRepository {
        aws: Mutex<HashMap<String, AwsPriceSnapshot>>,
        azure: Mutex<HashMap<String, AzurePriceSnapshot>>,
        fail_reads: AtomicBool,
        fail_writes: AtomicBool,
        reads: AtomicUsize,
        aws_writes: AtomicUsize,
    }

    struct TestLeaseRepository {
        decisions: Mutex<VecDeque<RefreshLeaseDecision>>,
        fail_claims: AtomicBool,
        claims: AtomicUsize,
        published: Mutex<Vec<RefreshLeaseOutcome>>,
    }

    impl TestLeaseRepository {
        fn new(decisions: impl IntoIterator<Item = RefreshLeaseDecision>) -> Arc<Self> {
            Arc::new(Self {
                decisions: Mutex::new(decisions.into_iter().collect()),
                fail_claims: AtomicBool::new(false),
                claims: AtomicUsize::new(0),
                published: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl RefreshLeaseRepository for TestLeaseRepository {
        async fn claim_refresh_lease(
            &self,
            _cache_key_sha256: &str,
            _owner_token: &str,
            _request_started_at: &str,
        ) -> Result<RefreshLeaseDecision, SnapshotRepositoryError> {
            self.claims.fetch_add(1, Ordering::SeqCst);
            if self.fail_claims.load(Ordering::SeqCst) {
                return Err(SnapshotRepositoryError::Unavailable);
            }
            Ok(self
                .decisions
                .lock()
                .await
                .pop_front()
                .unwrap_or(RefreshLeaseDecision::Pending))
        }

        async fn publish_refresh_lease(
            &self,
            _cache_key_sha256: &str,
            _owner_token: &str,
            outcome: &RefreshLeaseOutcome,
        ) -> Result<(), SnapshotRepositoryError> {
            self.published.lock().await.push(outcome.clone());
            Ok(())
        }
    }

    #[async_trait]
    impl DurableSnapshotRepository for TestDurableRepository {
        async fn put_aws(
            &self,
            snapshot: &AwsPriceSnapshot,
        ) -> Result<AwsPriceSnapshot, SnapshotRepositoryError> {
            self.aws_writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_writes.load(Ordering::SeqCst) {
                return Err(SnapshotRepositoryError::Unavailable);
            }
            self.aws
                .lock()
                .await
                .insert(snapshot.metadata.snapshot_id.clone(), snapshot.clone());
            Ok(snapshot.clone())
        }

        async fn put_azure(
            &self,
            snapshot: &AzurePriceSnapshot,
        ) -> Result<(), SnapshotRepositoryError> {
            if self.fail_writes.load(Ordering::SeqCst) {
                return Err(SnapshotRepositoryError::Unavailable);
            }
            self.azure
                .lock()
                .await
                .insert(snapshot.metadata.snapshot_id.clone(), snapshot.clone());
            Ok(())
        }

        async fn get_aws(
            &self,
            snapshot_id: &str,
        ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
            self.check_read()?;
            Ok(self.aws.lock().await.get(snapshot_id).cloned())
        }

        async fn get_azure(
            &self,
            snapshot_id: &str,
        ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
            self.check_read()?;
            Ok(self.azure.lock().await.get(snapshot_id).cloned())
        }

        async fn find_aws(
            &self,
            currency: &str,
            source_region: &str,
        ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
            self.check_read()?;
            Ok(self
                .aws
                .lock()
                .await
                .values()
                .filter(|snapshot| snapshot.matches_scope(currency, source_region))
                .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
                .cloned())
        }

        async fn find_azure(
            &self,
            currency: &str,
            target_region: &str,
        ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
            self.check_read()?;
            Ok(self
                .azure
                .lock()
                .await
                .values()
                .filter(|snapshot| snapshot.matches_scope(currency, target_region))
                .max_by(|left, right| left.metadata.retrieved_at.cmp(&right.metadata.retrieved_at))
                .cloned())
        }

        async fn list_latest_aws(&self) -> Result<Vec<AwsPriceSnapshot>, SnapshotRepositoryError> {
            self.check_read()?;
            Ok(self.aws.lock().await.values().cloned().collect())
        }
    }

    impl TestDurableRepository {
        fn check_read(&self) -> Result<(), SnapshotRepositoryError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_reads.load(Ordering::SeqCst) {
                Err(SnapshotRepositoryError::Unavailable)
            } else {
                Ok(())
            }
        }

        async fn seed_aws(&self, snapshot: AwsPriceSnapshot) {
            self.aws
                .lock()
                .await
                .insert(snapshot.metadata.snapshot_id.clone(), snapshot);
        }
    }

    impl TestLoader {
        fn new(aws_behavior: TestBehavior, azure_behavior: TestBehavior) -> Arc<Self> {
            Arc::new(Self {
                aws_behavior,
                azure_behavior,
                aws_calls: AtomicUsize::new(0),
                azure_calls: AtomicUsize::new(0),
                aws_gate: None,
                aws_started: Notify::new(),
            })
        }

        fn gated() -> (Arc<Self>, Arc<Semaphore>) {
            let gate = Arc::new(Semaphore::new(0));
            (
                Arc::new(Self {
                    aws_behavior: TestBehavior::Success,
                    azure_behavior: TestBehavior::Success,
                    aws_calls: AtomicUsize::new(0),
                    azure_calls: AtomicUsize::new(0),
                    aws_gate: Some(Arc::clone(&gate)),
                    aws_started: Notify::new(),
                }),
                gate,
            )
        }
    }

    #[async_trait]
    impl SnapshotLoader for TestLoader {
        async fn load_aws_snapshot(
            &self,
            source_region: &str,
        ) -> Result<AwsPriceSnapshot, ProviderError> {
            self.aws_calls.fetch_add(1, Ordering::SeqCst);
            self.aws_started.notify_one();
            if let Some(gate) = &self.aws_gate {
                gate.acquire()
                    .await
                    .expect("test refresh gate remains open")
                    .forget();
            }
            match self.aws_behavior {
                TestBehavior::Success => Ok(aws_snapshot(
                    source_region,
                    &utc_now_rfc3339().expect("current timestamp"),
                    PARSER_SCHEMA_VERSION,
                )),
                TestBehavior::Failure(error) => Err(error),
            }
        }

        async fn load_azure_snapshot(
            &self,
            target_region: &str,
            _catalogs: &AzurePricingCatalogs,
        ) -> Result<AzurePriceSnapshot, ProviderError> {
            self.azure_calls.fetch_add(1, Ordering::SeqCst);
            match self.azure_behavior {
                TestBehavior::Success => Ok(azure_snapshot(target_region)),
                TestBehavior::Failure(error) => Err(error),
            }
        }
    }

    #[tokio::test]
    async fn cache_resolution_never_calls_the_live_loader() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &utc_now_rfc3339().expect("current timestamp"),
                "cached-test-v1",
            ))
            .expect("store cache");
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let coordinator = coordinator(repository, Arc::clone(&loader));

        let resolution = coordinator
            .resolve_aws("USD", "eu-west-1")
            .await
            .expect("resolve cache");

        assert!(resolution.snapshot.is_some());
        assert!(resolution.warnings.is_empty());
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn explicit_refresh_bypasses_cache_and_persists_live_snapshot() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &utc_now_rfc3339().expect("current timestamp"),
                "cached-test-v1",
            ))
            .expect("store cache");
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let durable = Arc::new(TestDurableRepository::default());
        let coordinator = coordinator_with_durable(
            repository.clone(),
            Arc::clone(&loader),
            Arc::clone(&durable),
        );

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("refresh AWS");
        let snapshot = resolution.snapshot.expect("live snapshot");

        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
        assert_eq!(durable.aws_writes.load(Ordering::SeqCst), 1);
        assert_eq!(snapshot.metadata.status, ResolutionStatus::Fresh);
        assert_eq!(
            snapshot.metadata.parser_schema_version,
            PARSER_SCHEMA_VERSION
        );
        assert_eq!(
            repository
                .find_aws("USD", "eu-west-1")
                .expect("load persisted snapshot")
                .expect("persisted live snapshot")
                .metadata
                .snapshot_id,
            snapshot.metadata.snapshot_id
        );
    }

    #[tokio::test]
    async fn provider_failure_returns_stale_cache() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &timestamp(Duration::days(2)),
                "stale-test-v1",
            ))
            .expect("store stale cache");
        let loader = TestLoader::new(
            TestBehavior::Failure(ProviderError::TemporarilyUnavailable),
            TestBehavior::Success,
        );
        let coordinator = coordinator(repository, Arc::clone(&loader));

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("fall back to stale cache");

        assert_eq!(
            resolution.snapshot.expect("stale snapshot").metadata.status,
            ResolutionStatus::Stale
        );
        assert!(
            resolution.warnings[0].contains("provider_temporarily_unavailable"),
            "warning must expose only the sanitized provider reason code"
        );
    }

    #[tokio::test]
    async fn expired_cache_is_not_returned_after_provider_failure() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &timestamp(Duration::days(8)),
                "expired-test-v1",
            ))
            .expect("store expired cache");
        let loader = TestLoader::new(
            TestBehavior::Failure(ProviderError::SchemaChanged),
            TestBehavior::Success,
        );
        let coordinator = coordinator(repository, loader);

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("return unavailable resolution");

        assert!(resolution.snapshot.is_none());
        assert!(resolution.warnings[0].contains("provider_schema_changed"));
    }

    #[tokio::test]
    async fn aws_and_azure_refreshes_have_independent_flights() {
        let repository = InMemorySnapshotRepository::new();
        let loader = TestLoader::new(
            TestBehavior::Failure(ProviderError::NotFound),
            TestBehavior::Failure(ProviderError::Unsupported),
        );
        let coordinator = coordinator(repository, Arc::clone(&loader));

        let (aws, azure) = tokio::join!(
            coordinator.refresh_aws("USD", "eu-west-1", "swedencentral"),
            coordinator.refresh_azure("USD", Some("eu-west-1"), "swedencentral")
        );

        assert!(aws.expect("AWS resolution").snapshot.is_none());
        assert!(azure.expect("Azure resolution").snapshot.is_none());
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
        assert_eq!(loader.azure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_identical_refreshes_execute_once() {
        let repository = InMemorySnapshotRepository::new();
        let (loader, gate) = TestLoader::gated();
        let coordinator = coordinator(repository, Arc::clone(&loader));
        let started = loader.aws_started.notified();
        let first = tokio::spawn({
            let coordinator = coordinator.clone();
            async move {
                coordinator
                    .refresh_aws("USD", "eu-west-1", "swedencentral")
                    .await
            }
        });
        started.await;
        let tracked_flight = {
            let flights = coordinator.aws_flights.lock().await;
            Arc::clone(flights.values().next().expect("active AWS flight"))
        };
        let second = tokio::spawn({
            let coordinator = coordinator.clone();
            async move {
                coordinator
                    .refresh_aws("USD", "eu-west-1", "swedencentral")
                    .await
            }
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while Arc::strong_count(&tracked_flight) < 5 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("second request joins the active flight");

        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
        gate.add_permits(1);
        assert!(
            first
                .await
                .expect("first task completes")
                .expect("first refresh resolves")
                .snapshot
                .is_some()
        );
        assert!(
            second
                .await
                .expect("second task completes")
                .expect("second refresh resolves")
                .snapshot
                .is_some()
        );
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn durable_cache_hydrates_scope_and_id_lookups_after_restart() {
        let durable = Arc::new(TestDurableRepository::default());
        let snapshot = aws_snapshot(
            "eu-west-1",
            &utc_now_rfc3339().expect("current timestamp"),
            "durable-test-v1",
        );
        let snapshot_id = snapshot.metadata.snapshot_id.clone();
        durable.seed_aws(snapshot).await;
        let coordinator = coordinator_with_dependencies(
            InMemorySnapshotRepository::new(),
            Some(Arc::clone(&durable)),
            None,
        );

        let resolved = coordinator
            .resolve_aws("USD", "eu-west-1")
            .await
            .expect("resolve durable cache")
            .snapshot
            .expect("durable scope snapshot");
        assert_eq!(durable.reads.load(Ordering::SeqCst), 1);

        let restarted = coordinator_with_dependencies(
            InMemorySnapshotRepository::new(),
            Some(Arc::clone(&durable)),
            None,
        );
        let by_id = restarted
            .get_aws(&snapshot_id)
            .await
            .expect("load durable ID")
            .expect("durable ID snapshot");

        assert_eq!(resolved.metadata.status, ResolutionStatus::Cached);
        assert_eq!(resolved.metadata.snapshot_id, snapshot_id);
        assert_eq!(by_id.metadata.snapshot_id, snapshot_id);
        assert_eq!(durable.reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn hot_snapshot_avoids_durable_read() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &utc_now_rfc3339().expect("current timestamp"),
                "hot-test-v1",
            ))
            .expect("store hot snapshot");
        let durable = Arc::new(TestDurableRepository::default());
        durable.fail_reads.store(true, Ordering::SeqCst);
        let coordinator =
            coordinator_with_dependencies(repository, Some(Arc::clone(&durable)), None);

        let resolution = coordinator
            .resolve_aws("USD", "eu-west-1")
            .await
            .expect("resolve hot snapshot");

        assert!(resolution.snapshot.is_some());
        assert_eq!(durable.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn expired_durable_snapshot_is_not_hydrated() {
        let repository = InMemorySnapshotRepository::new();
        let durable = Arc::new(TestDurableRepository::default());
        durable
            .seed_aws(aws_snapshot(
                "eu-west-1",
                &timestamp(Duration::days(8)),
                "expired-durable-test-v1",
            ))
            .await;
        let coordinator = coordinator_with_dependencies(repository, Some(durable), None);

        let resolution = coordinator
            .resolve_aws("USD", "eu-west-1")
            .await
            .expect("resolve expired durable cache");

        assert!(resolution.snapshot.is_none());
    }

    #[tokio::test]
    async fn durable_write_failure_returns_existing_stale_snapshot() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &timestamp(Duration::days(2)),
                "stale-write-fallback-v1",
            ))
            .expect("store stale fallback");
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let durable = Arc::new(TestDurableRepository::default());
        durable.fail_writes.store(true, Ordering::SeqCst);
        let coordinator =
            coordinator_with_durable(repository.clone(), loader, Arc::clone(&durable));

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("return stale cache after write failure");

        assert_eq!(
            resolution.snapshot.expect("stale fallback").metadata.status,
            ResolutionStatus::Stale
        );
        assert!(resolution.warnings[0].contains("durable AWS price cache"));
        assert_eq!(durable.aws_writes.load(Ordering::SeqCst), 1);
        assert_eq!(
            repository
                .list_latest_aws()
                .expect("list hot snapshots")
                .into_iter()
                .next()
                .expect("existing stale snapshot")
                .metadata
                .parser_schema_version,
            "stale-write-fallback-v1"
        );
    }

    #[tokio::test]
    async fn durable_read_failure_without_fallback_is_an_error() {
        let durable = Arc::new(TestDurableRepository::default());
        durable.fail_reads.store(true, Ordering::SeqCst);
        let coordinator = coordinator_with_dependencies(
            InMemorySnapshotRepository::new(),
            Some(Arc::clone(&durable)),
            None,
        );

        let result = coordinator.resolve_aws("USD", "eu-west-1").await;

        assert!(matches!(
            result,
            Err(PricingCoordinatorError::Repository(
                SnapshotRepositoryError::Unavailable
            ))
        ));
        assert_eq!(durable.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn distributed_lease_owner_refreshes_persists_and_publishes() {
        let repository = InMemorySnapshotRepository::new();
        let durable = Arc::new(TestDurableRepository::default());
        let leases = TestLeaseRepository::new([RefreshLeaseDecision::Acquired]);
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let coordinator = coordinator_with_lease(
            repository,
            Arc::clone(&loader),
            Arc::clone(&durable),
            Arc::clone(&leases),
        );

        let snapshot = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("lease owner refresh")
            .snapshot
            .expect("fresh snapshot");

        assert_eq!(snapshot.metadata.status, ResolutionStatus::Fresh);
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
        assert_eq!(durable.aws_writes.load(Ordering::SeqCst), 1);
        assert_eq!(leases.claims.load(Ordering::SeqCst), 1);
        assert_eq!(
            *leases.published.lock().await,
            vec![RefreshLeaseOutcome::Succeeded(
                snapshot.metadata.snapshot_id.clone()
            )]
        );
    }

    #[tokio::test]
    async fn distributed_lease_waiter_hydrates_without_provider_call() {
        let durable = Arc::new(TestDurableRepository::default());
        let snapshot = aws_snapshot(
            "eu-west-1",
            &utc_now_rfc3339().expect("current timestamp"),
            "lease-waiter-test-v1",
        );
        let snapshot_id = snapshot.metadata.snapshot_id.clone();
        durable.seed_aws(snapshot).await;
        let leases = TestLeaseRepository::new([RefreshLeaseDecision::Succeeded(snapshot_id)]);
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let coordinator = coordinator_with_lease(
            InMemorySnapshotRepository::new(),
            Arc::clone(&loader),
            Arc::clone(&durable),
            leases,
        );

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("lease waiter resolution");

        assert_eq!(
            resolution
                .snapshot
                .expect("waiter snapshot")
                .metadata
                .status,
            ResolutionStatus::Cached
        );
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 0);
        assert_eq!(durable.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn distributed_lease_owner_publishes_provider_failure() {
        let durable = Arc::new(TestDurableRepository::default());
        let leases = TestLeaseRepository::new([RefreshLeaseDecision::Acquired]);
        let loader = TestLoader::new(
            TestBehavior::Failure(ProviderError::SchemaChanged),
            TestBehavior::Success,
        );
        let coordinator = coordinator_with_lease(
            InMemorySnapshotRepository::new(),
            loader,
            durable,
            Arc::clone(&leases),
        );

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("provider failure resolution");

        assert!(resolution.snapshot.is_none());
        assert!(resolution.warnings[0].contains("provider_schema_changed"));
        assert_eq!(
            *leases.published.lock().await,
            vec![RefreshLeaseOutcome::Failed(ProviderError::SchemaChanged)]
        );
    }

    #[tokio::test]
    async fn provider_work_is_cancelled_at_the_shared_deadline() {
        let (loader, _gate) = TestLoader::gated();

        let result = load_aws_before(
            loader.as_ref(),
            "eu-west-1",
            tokio::time::Instant::now() + std::time::Duration::from_millis(10),
        )
        .await;

        assert!(matches!(result, Err(ProviderError::TemporarilyUnavailable)));
    }

    #[tokio::test]
    async fn distributed_lease_failure_preserves_stale_fallback() {
        let repository = InMemorySnapshotRepository::new();
        repository
            .put_aws(aws_snapshot(
                "eu-west-1",
                &timestamp(Duration::days(2)),
                "lease-fallback-test-v1",
            ))
            .expect("store stale fallback");
        let durable = Arc::new(TestDurableRepository::default());
        let leases = TestLeaseRepository::new([]);
        leases.fail_claims.store(true, Ordering::SeqCst);
        let loader = TestLoader::new(TestBehavior::Success, TestBehavior::Success);
        let coordinator = coordinator_with_lease(
            repository,
            Arc::clone(&loader),
            durable,
            Arc::clone(&leases),
        );

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("stale fallback after lease failure");

        assert_eq!(
            resolution.snapshot.expect("stale snapshot").metadata.status,
            ResolutionStatus::Stale
        );
        assert!(resolution.warnings[0].contains("durable AWS price cache"));
        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 0);
        assert_eq!(leases.claims.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn refresh_cache_key_hash_is_canonical_and_scope_complete() {
        let first = RefreshKey::new(
            Provider::Aws,
            "USD",
            Some("eu-west-1"),
            "swedencentral",
            AWS_SERVICE,
            AWS_FILTER,
        );
        let same = first.clone();
        let other_target = RefreshKey::new(
            Provider::Aws,
            "USD",
            Some("eu-west-1"),
            "uksouth",
            AWS_SERVICE,
            AWS_FILTER,
        );

        assert_eq!(first.sha256(), same.sha256());
        assert_ne!(first.sha256(), other_target.sha256());
        assert!(
            first
                .sha256()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
    }

    fn coordinator(
        repository: InMemorySnapshotRepository,
        loader: Arc<TestLoader>,
    ) -> PricingCoordinator {
        coordinator_with_dependencies(repository, None, Some(loader))
    }

    fn coordinator_with_durable(
        repository: InMemorySnapshotRepository,
        loader: Arc<TestLoader>,
        durable: Arc<TestDurableRepository>,
    ) -> PricingCoordinator {
        coordinator_with_dependencies(repository, Some(durable), Some(loader))
    }

    fn coordinator_with_lease(
        repository: InMemorySnapshotRepository,
        loader: Arc<TestLoader>,
        durable: Arc<TestDurableRepository>,
        leases: Arc<TestLeaseRepository>,
    ) -> PricingCoordinator {
        let durable: Arc<dyn DurableSnapshotRepository> = durable;
        let leases: Arc<dyn RefreshLeaseRepository> = leases;
        let loader: Arc<dyn SnapshotLoader> = loader;
        PricingCoordinator::with_loader(
            repository,
            Some(durable),
            Some(leases),
            Some(loader),
            capabilities(),
        )
    }

    fn coordinator_with_dependencies(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<TestDurableRepository>>,
        loader: Option<Arc<TestLoader>>,
    ) -> PricingCoordinator {
        let durable = durable.map(|durable| durable as Arc<dyn DurableSnapshotRepository>);
        let loader = loader.map(|loader| loader as Arc<dyn SnapshotLoader>);
        PricingCoordinator::with_loader(repository, durable, None, loader, capabilities())
    }

    fn capabilities() -> AzurePricingCatalogs {
        AzurePricingCatalogs::new(
            Arc::new(
                serde_json::from_str(include_str!(
                    "../../../app/catalogs/sql-mi-capabilities.json"
                ))
                .expect("embedded SQL MI capability catalog"),
            ),
            Arc::new(
                serde_json::from_str(include_str!(
                    "../../../app/catalogs/azure-vm-capabilities.json"
                ))
                .expect("embedded Azure VM capability catalog"),
            ),
            Arc::new(
                serde_json::from_str(include_str!(
                    "../../../app/catalogs/azure-managed-disk-capabilities.json"
                ))
                .expect("embedded managed-disk capability catalog"),
            ),
        )
    }

    fn aws_snapshot(
        source_region: &str,
        retrieved_at: &str,
        parser_schema_version: &str,
    ) -> AwsPriceSnapshot {
        AwsPriceSnapshot::create(
            SnapshotCreationMetadata {
                status: ResolutionStatus::Fresh,
                retrieved_at: retrieved_at.to_owned(),
                source_published_at: None,
                currency: "USD".to_owned(),
                source_urls: vec!["https://example.invalid/aws-prices".to_owned()],
                parser_schema_version: parser_schema_version.to_owned(),
                warnings: Vec::new(),
            },
            source_region,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .expect("synthetic AWS snapshot")
    }

    fn azure_snapshot(target_region: &str) -> AzurePriceSnapshot {
        let (_, fixture) = local_fixture::load().expect("frozen Azure fixture");
        AzurePriceSnapshot::create(
            SnapshotCreationMetadata {
                status: ResolutionStatus::Fresh,
                retrieved_at: utc_now_rfc3339().expect("current timestamp"),
                source_published_at: fixture.metadata.source_published_at,
                currency: fixture.metadata.currency,
                source_urls: fixture.metadata.source_urls,
                parser_schema_version: PARSER_SCHEMA_VERSION.to_owned(),
                warnings: fixture.metadata.warnings,
            },
            target_region,
            fixture.mi_rates,
        )
        .expect("synthetic Azure snapshot")
    }

    fn timestamp(age: Duration) -> String {
        (OffsetDateTime::now_utc() - age)
            .format(&Rfc3339)
            .expect("test timestamp")
    }
}
