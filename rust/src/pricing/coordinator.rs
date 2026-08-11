use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

use crate::calculation::target_selector::CapabilityCatalog;

use super::{
    live::PARSER_SCHEMA_VERSION,
    loader::LivePricingLoader,
    provider::{Provider, ProviderError},
    repository::{DurableSnapshotRepository, InMemorySnapshotRepository, SnapshotRepositoryError},
    snapshot::{AwsPriceSnapshot, AzurePriceSnapshot},
};

const AWS_SERVICE: &str = "Amazon EC2, RDS, and EBS";
const AWS_FILTER: &str = "current SQL Server compute and reviewed storage meters";
const AZURE_SERVICE: &str = "Azure SQL Managed Instance";
const AZURE_FILTER: &str = "reviewed SQL MI capability configurations and eight purchase options";

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
        capabilities: &CapabilityCatalog,
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
        capabilities: &CapabilityCatalog,
    ) -> Result<AzurePriceSnapshot, ProviderError> {
        LivePricingLoader::load_azure_snapshot(self, target_region, capabilities).await
    }
}

#[derive(Clone)]
pub struct PricingCoordinator {
    repository: InMemorySnapshotRepository,
    durable: Option<Arc<dyn DurableSnapshotRepository>>,
    loader: Option<Arc<dyn SnapshotLoader>>,
    capabilities: Arc<CapabilityCatalog>,
    aws_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AwsPriceSnapshot>>>>>,
    azure_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AzurePriceSnapshot>>>>>,
}

impl PricingCoordinator {
    pub fn new(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<dyn DurableSnapshotRepository>>,
        loader: Option<LivePricingLoader>,
        capabilities: Arc<CapabilityCatalog>,
    ) -> Self {
        Self::with_loader(
            repository,
            durable,
            loader.map(|loader| Arc::new(loader) as Arc<dyn SnapshotLoader>),
            capabilities,
        )
    }

    fn with_loader(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<dyn DurableSnapshotRepository>>,
        loader: Option<Arc<dyn SnapshotLoader>>,
        capabilities: Arc<CapabilityCatalog>,
    ) -> Self {
        Self {
            repository,
            durable,
            loader,
            capabilities,
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
            .start_aws_flight(key, Arc::clone(loader), source_region.to_owned())
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
                let snapshot = self.repository.find_aws(currency, source_region)?;
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
            .start_azure_flight(key, Arc::clone(loader), target_region.to_owned())
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
                let snapshot = self.repository.find_azure(currency, target_region)?;
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
        let flights = Arc::clone(&self.aws_flights);
        let task_flight = Arc::clone(&flight);
        tokio::spawn(async move {
            let result = match loader.load_aws_snapshot(&source_region).await {
                Ok(snapshot) => {
                    match persist_aws(&repository, durable.as_deref(), snapshot).await {
                        Ok(snapshot) => Ok(snapshot),
                        Err(_) => Err(RefreshFailure::Repository),
                    }
                }
                Err(error) => Err(RefreshFailure::Provider(error)),
            };
            task_flight.complete(result).await;
            flights.lock().await.remove(&key);
        });
        flight
    }

    async fn start_azure_flight(
        &self,
        key: RefreshKey,
        loader: Arc<dyn SnapshotLoader>,
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
        let capabilities = Arc::clone(&self.capabilities);
        let flights = Arc::clone(&self.azure_flights);
        let task_flight = Arc::clone(&flight);
        tokio::spawn(async move {
            let result = match loader
                .load_azure_snapshot(&target_region, &capabilities)
                .await
            {
                Ok(snapshot) => {
                    match persist_azure(&repository, durable.as_deref(), snapshot).await {
                        Ok(snapshot) => Ok(snapshot),
                        Err(_) => Err(RefreshFailure::Repository),
                    }
                }
                Err(error) => Err(RefreshFailure::Provider(error)),
            };
            task_flight.complete(result).await;
            flights.lock().await.remove(&key);
        });
        flight
    }
}

async fn persist_aws(
    repository: &InMemorySnapshotRepository,
    durable: Option<&dyn DurableSnapshotRepository>,
    snapshot: AwsPriceSnapshot,
) -> Result<Arc<AwsPriceSnapshot>, SnapshotRepositoryError> {
    if let Some(durable) = durable {
        durable.put_aws(&snapshot).await?;
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

    #[async_trait]
    impl DurableSnapshotRepository for TestDurableRepository {
        async fn put_aws(
            &self,
            snapshot: &AwsPriceSnapshot,
        ) -> Result<(), SnapshotRepositoryError> {
            self.aws_writes.fetch_add(1, Ordering::SeqCst);
            if self.fail_writes.load(Ordering::SeqCst) {
                return Err(SnapshotRepositoryError::Unavailable);
            }
            self.aws
                .lock()
                .await
                .insert(snapshot.metadata.snapshot_id.clone(), snapshot.clone());
            Ok(())
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
            _capabilities: &CapabilityCatalog,
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

    fn coordinator_with_dependencies(
        repository: InMemorySnapshotRepository,
        durable: Option<Arc<TestDurableRepository>>,
        loader: Option<Arc<TestLoader>>,
    ) -> PricingCoordinator {
        let durable = durable.map(|durable| durable as Arc<dyn DurableSnapshotRepository>);
        let loader = loader.map(|loader| loader as Arc<dyn SnapshotLoader>);
        PricingCoordinator::with_loader(repository, durable, loader, capabilities())
    }

    fn capabilities() -> Arc<CapabilityCatalog> {
        Arc::new(
            serde_json::from_str(include_str!(
                "../../../app/catalogs/sql-mi-capabilities.json"
            ))
            .expect("embedded capability catalog"),
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
