use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

use crate::calculation::target_selector::CapabilityCatalog;

use super::{
    live::PARSER_SCHEMA_VERSION,
    loader::LivePricingLoader,
    provider::{Provider, ProviderError},
    repository::{InMemorySnapshotRepository, SnapshotRepositoryError},
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
    loader: Option<Arc<dyn SnapshotLoader>>,
    capabilities: Arc<CapabilityCatalog>,
    aws_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AwsPriceSnapshot>>>>>,
    azure_flights: Arc<Mutex<HashMap<RefreshKey, Arc<Flight<AzurePriceSnapshot>>>>>,
}

impl PricingCoordinator {
    pub fn new(
        repository: InMemorySnapshotRepository,
        loader: Option<LivePricingLoader>,
        capabilities: Arc<CapabilityCatalog>,
    ) -> Self {
        Self::with_loader(
            repository,
            loader.map(|loader| Arc::new(loader) as Arc<dyn SnapshotLoader>),
            capabilities,
        )
    }

    fn with_loader(
        repository: InMemorySnapshotRepository,
        loader: Option<Arc<dyn SnapshotLoader>>,
        capabilities: Arc<CapabilityCatalog>,
    ) -> Self {
        Self {
            repository,
            loader,
            capabilities,
            aws_flights: Arc::new(Mutex::new(HashMap::new())),
            azure_flights: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn resolve_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<SnapshotResolution<AwsPriceSnapshot>, PricingCoordinatorError> {
        let snapshot = self.repository.find_aws(currency, source_region)?;
        Ok(resolve_cached(snapshot, Provider::Aws))
    }

    pub fn resolve_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<SnapshotResolution<AzurePriceSnapshot>, PricingCoordinatorError> {
        let snapshot = self.repository.find_azure(currency, target_region)?;
        Ok(resolve_cached(snapshot, Provider::Azure))
    }

    pub async fn refresh_aws(
        &self,
        currency: &str,
        source_region: &str,
        target_region: &str,
    ) -> Result<SnapshotResolution<AwsPriceSnapshot>, PricingCoordinatorError> {
        let Some(loader) = &self.loader else {
            let snapshot = self.repository.find_aws(currency, source_region)?;
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
                let snapshot = self.repository.find_aws(currency, source_region)?;
                Ok(provider_fallback(snapshot, Provider::Aws, error))
            }
            Err(RefreshFailure::Repository(error)) => Err(error.into()),
        }
    }

    pub async fn refresh_azure(
        &self,
        currency: &str,
        source_region: Option<&str>,
        target_region: &str,
    ) -> Result<SnapshotResolution<AzurePriceSnapshot>, PricingCoordinatorError> {
        let Some(loader) = &self.loader else {
            let snapshot = self.repository.find_azure(currency, target_region)?;
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
                let snapshot = self.repository.find_azure(currency, target_region)?;
                Ok(provider_fallback(snapshot, Provider::Azure, error))
            }
            Err(RefreshFailure::Repository(error)) => Err(error.into()),
        }
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
        let flights = Arc::clone(&self.aws_flights);
        let task_flight = Arc::clone(&flight);
        tokio::spawn(async move {
            let result = match loader.load_aws_snapshot(&source_region).await {
                Ok(snapshot) => match repository.put_aws(snapshot.clone()) {
                    Ok(()) => Ok(Arc::new(snapshot)),
                    Err(error) => Err(RefreshFailure::Repository(error)),
                },
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
        let capabilities = Arc::clone(&self.capabilities);
        let flights = Arc::clone(&self.azure_flights);
        let task_flight = Arc::clone(&flight);
        tokio::spawn(async move {
            let result = match loader
                .load_azure_snapshot(&target_region, &capabilities)
                .await
            {
                Ok(snapshot) => match repository.put_azure(snapshot.clone()) {
                    Ok(()) => Ok(Arc::new(snapshot)),
                    Err(error) => Err(RefreshFailure::Repository(error)),
                },
                Err(error) => Err(RefreshFailure::Provider(error)),
            };
            task_flight.complete(result).await;
            flights.lock().await.remove(&key);
        });
        flight
    }
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
    Repository(SnapshotRepositoryError),
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    #[test]
    fn cache_resolution_never_calls_the_live_loader() {
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
        let coordinator = coordinator(repository.clone(), Arc::clone(&loader));

        let resolution = coordinator
            .refresh_aws("USD", "eu-west-1", "swedencentral")
            .await
            .expect("refresh AWS");
        let snapshot = resolution.snapshot.expect("live snapshot");

        assert_eq!(loader.aws_calls.load(Ordering::SeqCst), 1);
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

    fn coordinator(
        repository: InMemorySnapshotRepository,
        loader: Arc<TestLoader>,
    ) -> PricingCoordinator {
        let loader: Arc<dyn SnapshotLoader> = loader;
        PricingCoordinator::with_loader(repository, Some(loader), capabilities())
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
