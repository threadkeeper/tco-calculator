use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Arc, RwLock, RwLockWriteGuard},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::domain::decimal::DecimalValue;

pub const CALCULATOR_LAUNCH_DOCUMENT_TYPE: &str = "azure_calculator_launch";
pub const CALCULATOR_MANIFEST_VERSION: u16 = 1;
pub const CALCULATOR_PROTOCOL_VERSION: u16 = 1;
pub const CALCULATOR_CONTRACT_VERSION: &str = "2026-08-23";
pub const MINIMUM_COMPANION_VERSION: &str = "1.0.0";
pub const MAX_CALCULATOR_MANIFEST_ITEMS: usize = 25;
pub const MAX_CALCULATOR_MANIFEST_BYTES: usize = 256 * 1024;
const ACTIVE_TTL_SECONDS: i64 = 10 * 60;
const CONSUMED_TTL_SECONDS: i64 = 24 * 60 * 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalculatorManifest {
    pub schema_version: u16,
    pub calculator_contract_version: String,
    pub calculator_url: String,
    pub generated_at: String,
    pub currency: String,
    pub locale: String,
    pub items: Vec<CalculatorManifestItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalculatorManifestItem {
    pub item_key: String,
    pub display_name: String,
    pub product: CalculatorProduct,
    pub region: String,
    pub deployment_model: CalculatorDeploymentModel,
    pub service_tier: CalculatorServiceTier,
    pub hardware_family: CalculatorHardwareFamily,
    pub vcores: u32,
    pub selected_memory_gb: DecimalValue,
    pub zone_redundant: bool,
    pub quantity: u32,
    pub hours_per_month: DecimalValue,
    pub purchase_option: CalculatorPurchaseOption,
    pub azure_hybrid_benefit: bool,
    pub data_storage_gb: DecimalValue,
    pub backup_storage_gb: DecimalValue,
    pub expected_public_annual: CalculatorExpectedPublicAnnual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorProduct {
    AzureSqlManagedInstance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorDeploymentModel {
    SingleInstance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorServiceTier {
    NextGenerationGeneralPurpose,
    BusinessCritical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorHardwareFamily {
    PremiumSeries,
    PremiumSeriesMemoryOptimized,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorPurchaseOption {
    Payg,
    OneYearReservation,
    ThreeYearReservation,
    OneYearSavingsPlan,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalculatorExpectedPublicAnnual {
    pub compute: DecimalValue,
    pub additional_memory: DecimalValue,
    pub license: DecimalValue,
    pub storage: DecimalValue,
    pub total_before_parity: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculatorLaunchStatus {
    Ready,
    Claimed,
    Consumed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalculatorLaunchDocument {
    pub id: Uuid,
    pub document_type: String,
    pub owner_id: String,
    pub source_project_id: Option<Uuid>,
    pub source_project_etag: Option<String>,
    pub source_formula_version: Option<String>,
    pub source_azure_snapshot_id: Option<String>,
    pub status: CalculatorLaunchStatus,
    pub protocol_version: u16,
    pub manifest_version: u16,
    pub calculator_contract_version: String,
    pub minimum_companion_version: String,
    pub manifest_sha256: Option<String>,
    pub manifest: Option<CalculatorManifest>,
    pub companion_instance_id: Option<Uuid>,
    pub companion_version: Option<String>,
    pub created_at: String,
    pub claim_expires_at: String,
    pub updated_at: String,
    pub ttl: i64,
    #[serde(default, rename = "_etag", skip_serializing)]
    pub etag: String,
}

#[derive(Clone, Debug)]
pub struct NewCalculatorLaunch {
    pub id: Uuid,
    pub source_project_id: Uuid,
    pub source_project_etag: String,
    pub source_formula_version: String,
    pub source_azure_snapshot_id: String,
    pub manifest: CalculatorManifest,
}

#[derive(Clone, Debug)]
pub struct CreatedCalculatorLaunch {
    pub document: CalculatorLaunchDocument,
    pub created: bool,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CalculatorLaunchError {
    #[error("calculator launch was not found")]
    NotFound,
    #[error("calculator launch conflicts with an existing launch")]
    Conflict,
    #[error("calculator launch has expired")]
    Expired,
    #[error("calculator launch ETag did not match")]
    PreconditionFailed,
    #[error("calculator launch manifest is invalid")]
    InvalidManifest,
    #[error("calculator launch manifest exceeds the persistence limit")]
    PayloadTooLarge,
    #[error("calculator launch persistence is unavailable")]
    Unavailable,
}

pub trait CalculatorLaunchClock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

#[derive(Debug)]
pub struct SystemCalculatorLaunchClock;

impl CalculatorLaunchClock for SystemCalculatorLaunchClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[async_trait]
pub trait CalculatorLaunchRepository: Send + Sync {
    async fn create(
        &self,
        owner_id: &str,
        launch: NewCalculatorLaunch,
    ) -> Result<CreatedCalculatorLaunch, CalculatorLaunchError>;

    async fn claim(
        &self,
        owner_id: &str,
        launch_id: Uuid,
        companion_instance_id: Uuid,
        companion_version: &str,
    ) -> Result<CalculatorLaunchDocument, CalculatorLaunchError>;

    async fn acknowledge(
        &self,
        owner_id: &str,
        launch_id: Uuid,
        companion_instance_id: Uuid,
        if_match: &str,
    ) -> Result<(), CalculatorLaunchError>;

    async fn purge_project(
        &self,
        owner_id: &str,
        project_id: Uuid,
    ) -> Result<(), CalculatorLaunchError>;
}

#[derive(Clone)]
pub struct InMemoryCalculatorLaunchRepository {
    launches: Arc<RwLock<LaunchMap>>,
    clock: Arc<dyn CalculatorLaunchClock>,
}

#[derive(Clone)]
struct StoredLaunch {
    document: CalculatorLaunchDocument,
    version: u64,
}

type LaunchMap = HashMap<(String, Uuid), StoredLaunch>;

impl Default for InMemoryCalculatorLaunchRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryCalculatorLaunchRepository {
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemCalculatorLaunchClock))
    }

    pub fn with_clock(clock: Arc<dyn CalculatorLaunchClock>) -> Self {
        Self {
            launches: Arc::new(RwLock::new(HashMap::new())),
            clock,
        }
    }

    #[cfg(test)]
    fn read_launches(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, LaunchMap>, CalculatorLaunchError> {
        self.launches
            .read()
            .map_err(|_| CalculatorLaunchError::Unavailable)
    }

    fn write_launches(&self) -> Result<RwLockWriteGuard<'_, LaunchMap>, CalculatorLaunchError> {
        self.launches
            .write()
            .map_err(|_| CalculatorLaunchError::Unavailable)
    }
}

#[async_trait]
impl CalculatorLaunchRepository for InMemoryCalculatorLaunchRepository {
    async fn create(
        &self,
        owner_id: &str,
        launch: NewCalculatorLaunch,
    ) -> Result<CreatedCalculatorLaunch, CalculatorLaunchError> {
        let now = self.clock.now();
        validate_manifest(&launch.manifest)?;
        let manifest_sha256 = manifest_sha256(&launch.manifest)?;
        let key = (owner_id.to_owned(), launch.id);
        let mut launches = self.write_launches()?;

        if let Some(stored) = launches.get(&key) {
            let document = &stored.document;
            let same_binding = document.source_project_id == Some(launch.source_project_id)
                && document.source_project_etag.as_deref()
                    == Some(launch.source_project_etag.as_str())
                && document.manifest_sha256.as_deref() == Some(manifest_sha256.as_str());
            if same_binding && document.status == CalculatorLaunchStatus::Ready {
                ensure_not_expired(document, now)?;
                return Ok(CreatedCalculatorLaunch {
                    document: document.clone(),
                    created: false,
                });
            }
            return Err(CalculatorLaunchError::Conflict);
        }

        for stored in launches.values() {
            if stored.document.owner_id == owner_id
                && matches!(
                    stored.document.status,
                    CalculatorLaunchStatus::Ready | CalculatorLaunchStatus::Claimed
                )
                && !is_expired(&stored.document, now)?
            {
                return Err(CalculatorLaunchError::Conflict);
            }
        }

        let timestamp = format_timestamp(now)?;
        let document = CalculatorLaunchDocument {
            id: launch.id,
            document_type: CALCULATOR_LAUNCH_DOCUMENT_TYPE.to_owned(),
            owner_id: owner_id.to_owned(),
            source_project_id: Some(launch.source_project_id),
            source_project_etag: Some(launch.source_project_etag),
            source_formula_version: Some(launch.source_formula_version),
            source_azure_snapshot_id: Some(launch.source_azure_snapshot_id),
            status: CalculatorLaunchStatus::Ready,
            protocol_version: CALCULATOR_PROTOCOL_VERSION,
            manifest_version: CALCULATOR_MANIFEST_VERSION,
            calculator_contract_version: CALCULATOR_CONTRACT_VERSION.to_owned(),
            minimum_companion_version: MINIMUM_COMPANION_VERSION.to_owned(),
            manifest_sha256: Some(manifest_sha256),
            manifest: Some(launch.manifest),
            companion_instance_id: None,
            companion_version: None,
            created_at: timestamp.clone(),
            claim_expires_at: format_timestamp(now + Duration::seconds(ACTIVE_TTL_SECONDS))?,
            updated_at: timestamp,
            ttl: ACTIVE_TTL_SECONDS,
            etag: launch_etag(1),
        };
        validate_document(&document, owner_id, Some(launch.id))?;
        launches.insert(
            key,
            StoredLaunch {
                document: document.clone(),
                version: 1,
            },
        );
        Ok(CreatedCalculatorLaunch {
            document,
            created: true,
        })
    }

    async fn claim(
        &self,
        owner_id: &str,
        launch_id: Uuid,
        companion_instance_id: Uuid,
        companion_version: &str,
    ) -> Result<CalculatorLaunchDocument, CalculatorLaunchError> {
        let now = self.clock.now();
        let key = (owner_id.to_owned(), launch_id);
        let mut launches = self.write_launches()?;
        let stored = launches
            .get_mut(&key)
            .ok_or(CalculatorLaunchError::NotFound)?;
        validate_document(&stored.document, owner_id, Some(launch_id))?;
        ensure_not_expired(&stored.document, now)?;

        match stored.document.status {
            CalculatorLaunchStatus::Ready => {
                stored.version = stored
                    .version
                    .checked_add(1)
                    .ok_or(CalculatorLaunchError::Unavailable)?;
                stored.document.status = CalculatorLaunchStatus::Claimed;
                stored.document.companion_instance_id = Some(companion_instance_id);
                stored.document.companion_version = Some(companion_version.to_owned());
                stored.document.updated_at = format_timestamp(now)?;
                stored.document.claim_expires_at =
                    format_timestamp(now + Duration::seconds(ACTIVE_TTL_SECONDS))?;
                stored.document.ttl = ACTIVE_TTL_SECONDS;
                stored.document.etag = launch_etag(stored.version);
                validate_document(&stored.document, owner_id, Some(launch_id))?;
                Ok(stored.document.clone())
            }
            CalculatorLaunchStatus::Claimed
                if stored.document.companion_instance_id == Some(companion_instance_id) =>
            {
                Ok(stored.document.clone())
            }
            CalculatorLaunchStatus::Claimed | CalculatorLaunchStatus::Consumed => {
                Err(CalculatorLaunchError::Conflict)
            }
        }
    }

    async fn acknowledge(
        &self,
        owner_id: &str,
        launch_id: Uuid,
        companion_instance_id: Uuid,
        if_match: &str,
    ) -> Result<(), CalculatorLaunchError> {
        let now = self.clock.now();
        let key = (owner_id.to_owned(), launch_id);
        let mut launches = self.write_launches()?;
        let stored = launches
            .get_mut(&key)
            .ok_or(CalculatorLaunchError::NotFound)?;
        validate_document(&stored.document, owner_id, Some(launch_id))?;

        if stored.document.status == CalculatorLaunchStatus::Consumed
            && stored.document.companion_instance_id == Some(companion_instance_id)
        {
            return Ok(());
        }
        ensure_not_expired(&stored.document, now)?;
        if stored.document.status != CalculatorLaunchStatus::Claimed
            || stored.document.companion_instance_id != Some(companion_instance_id)
        {
            return Err(CalculatorLaunchError::Conflict);
        }
        if stored.document.etag != if_match {
            return Err(CalculatorLaunchError::PreconditionFailed);
        }

        stored.version = stored
            .version
            .checked_add(1)
            .ok_or(CalculatorLaunchError::Unavailable)?;
        stored.document.status = CalculatorLaunchStatus::Consumed;
        stored.document.source_project_id = None;
        stored.document.source_project_etag = None;
        stored.document.source_formula_version = None;
        stored.document.source_azure_snapshot_id = None;
        stored.document.manifest_sha256 = None;
        stored.document.manifest = None;
        stored.document.updated_at = format_timestamp(now)?;
        stored.document.claim_expires_at =
            format_timestamp(now + Duration::seconds(CONSUMED_TTL_SECONDS))?;
        stored.document.ttl = CONSUMED_TTL_SECONDS;
        stored.document.etag = launch_etag(stored.version);
        validate_document(&stored.document, owner_id, Some(launch_id))?;
        Ok(())
    }

    async fn purge_project(
        &self,
        owner_id: &str,
        project_id: Uuid,
    ) -> Result<(), CalculatorLaunchError> {
        self.write_launches()?.retain(|(stored_owner, _), stored| {
            stored_owner != owner_id
                || stored.document.source_project_id != Some(project_id)
                || stored.document.status == CalculatorLaunchStatus::Consumed
        });
        Ok(())
    }
}

fn validate_manifest(manifest: &CalculatorManifest) -> Result<(), CalculatorLaunchError> {
    if manifest.schema_version != CALCULATOR_MANIFEST_VERSION
        || manifest.calculator_contract_version != CALCULATOR_CONTRACT_VERSION
        || manifest.calculator_url != "https://azure.microsoft.com/en-us/pricing/calculator/"
        || manifest.currency != "USD"
        || manifest.locale != "en-US"
        || manifest.items.is_empty()
        || manifest.items.len() > MAX_CALCULATOR_MANIFEST_ITEMS
    {
        return Err(CalculatorLaunchError::InvalidManifest);
    }
    let bytes = serde_json::to_vec(manifest).map_err(|_| CalculatorLaunchError::InvalidManifest)?;
    if bytes.len() > MAX_CALCULATOR_MANIFEST_BYTES {
        return Err(CalculatorLaunchError::PayloadTooLarge);
    }
    Ok(())
}

fn manifest_sha256(manifest: &CalculatorManifest) -> Result<String, CalculatorLaunchError> {
    let bytes = serde_json::to_vec(manifest).map_err(|_| CalculatorLaunchError::InvalidManifest)?;
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").map_err(|_| CalculatorLaunchError::Unavailable)?;
    }
    Ok(encoded)
}

fn validate_document(
    document: &CalculatorLaunchDocument,
    owner_id: &str,
    expected_id: Option<Uuid>,
) -> Result<(), CalculatorLaunchError> {
    if document.document_type != CALCULATOR_LAUNCH_DOCUMENT_TYPE
        || document.owner_id != owner_id
        || expected_id.is_some_and(|id| document.id != id)
        || document.protocol_version != CALCULATOR_PROTOCOL_VERSION
        || document.manifest_version != CALCULATOR_MANIFEST_VERSION
        || document.calculator_contract_version != CALCULATOR_CONTRACT_VERSION
        || document.minimum_companion_version != MINIMUM_COMPANION_VERSION
        || document.ttl <= 0
        || document.etag.is_empty()
    {
        return Err(CalculatorLaunchError::Unavailable);
    }
    let active_payload_present = document.source_project_id.is_some()
        && document.source_project_etag.is_some()
        && document.source_formula_version.is_some()
        && document.source_azure_snapshot_id.is_some()
        && document.manifest_sha256.is_some()
        && document.manifest.is_some();
    match document.status {
        CalculatorLaunchStatus::Ready
            if active_payload_present
                && document.companion_instance_id.is_none()
                && document.companion_version.is_none() => {}
        CalculatorLaunchStatus::Claimed
            if active_payload_present
                && document.companion_instance_id.is_some()
                && document.companion_version.is_some() => {}
        CalculatorLaunchStatus::Consumed
            if !active_payload_present
                && document.companion_instance_id.is_some()
                && document.companion_version.is_some() => {}
        _ => return Err(CalculatorLaunchError::Unavailable),
    }
    if let (Some(manifest), Some(expected_hash)) = (&document.manifest, &document.manifest_sha256) {
        validate_manifest(manifest)?;
        if manifest_sha256(manifest)? != *expected_hash {
            return Err(CalculatorLaunchError::Unavailable);
        }
    }
    parse_timestamp(&document.created_at)?;
    parse_timestamp(&document.claim_expires_at)?;
    parse_timestamp(&document.updated_at)?;
    Ok(())
}

fn ensure_not_expired(
    document: &CalculatorLaunchDocument,
    now: OffsetDateTime,
) -> Result<(), CalculatorLaunchError> {
    if is_expired(document, now)? {
        Err(CalculatorLaunchError::Expired)
    } else {
        Ok(())
    }
}

fn is_expired(
    document: &CalculatorLaunchDocument,
    now: OffsetDateTime,
) -> Result<bool, CalculatorLaunchError> {
    Ok(now >= parse_timestamp(&document.claim_expires_at)?)
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime, CalculatorLaunchError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|_| CalculatorLaunchError::Unavailable)
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, CalculatorLaunchError> {
    value
        .format(&Rfc3339)
        .map_err(|_| CalculatorLaunchError::Unavailable)
}

fn launch_etag(version: u64) -> String {
    format!("\"calculator-launch-{version}\"")
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Mutex};

    use rust_decimal::Decimal;

    use super::*;

    #[derive(Debug)]
    struct FixedClock(Mutex<OffsetDateTime>);

    impl FixedClock {
        fn new() -> Self {
            Self(Mutex::new(
                OffsetDateTime::parse("2026-08-23T12:00:00Z", &Rfc3339).expect("timestamp"),
            ))
        }
    }

    impl CalculatorLaunchClock for FixedClock {
        fn now(&self) -> OffsetDateTime {
            *self.0.lock().expect("clock")
        }
    }

    #[tokio::test]
    async fn exactly_one_companion_instance_claims_a_launch() {
        let repository =
            InMemoryCalculatorLaunchRepository::with_clock(Arc::new(FixedClock::new()));
        let launch_id =
            Uuid::parse_str("01234567-89ab-4cde-8f01-23456789abcd").expect("launch UUID");
        repository
            .create("owner", new_launch(launch_id))
            .await
            .expect("create");
        let first = Uuid::parse_str("11111111-1111-4111-8111-111111111111").expect("instance");
        let second = Uuid::parse_str("22222222-2222-4222-8222-222222222222").expect("instance");

        let (left, right) = tokio::join!(
            repository.claim("owner", launch_id, first, "1.0.0"),
            repository.claim("owner", launch_id, second, "1.0.0")
        );

        let outcomes = [left, right];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|result| matches!(result, Err(CalculatorLaunchError::Conflict)))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn acknowledgement_purges_manifest_and_is_idempotent_for_the_claimant() {
        let repository =
            InMemoryCalculatorLaunchRepository::with_clock(Arc::new(FixedClock::new()));
        let launch_id = Uuid::new_v4();
        let instance_id = Uuid::new_v4();
        repository
            .create("owner", new_launch(launch_id))
            .await
            .expect("create");
        let claimed = repository
            .claim("owner", launch_id, instance_id, "1.0.0")
            .await
            .expect("claim");

        repository
            .acknowledge("owner", launch_id, instance_id, &claimed.etag)
            .await
            .expect("acknowledge");
        repository
            .acknowledge("owner", launch_id, instance_id, &claimed.etag)
            .await
            .expect("idempotent acknowledgement");

        assert_eq!(
            repository
                .claim("owner", launch_id, instance_id, "1.0.0")
                .await,
            Err(CalculatorLaunchError::Conflict)
        );
        let stored = repository.read_launches().expect("launches");
        let document = &stored
            .get(&("owner".to_owned(), launch_id))
            .expect("tombstone")
            .document;
        assert_eq!(document.status, CalculatorLaunchStatus::Consumed);
        assert!(document.manifest.is_none());
        assert!(document.source_project_id.is_none());
    }

    fn new_launch(id: Uuid) -> NewCalculatorLaunch {
        NewCalculatorLaunch {
            id,
            source_project_id: Uuid::new_v4(),
            source_project_etag: "\"project-1\"".to_owned(),
            source_formula_version: "1.3.0".to_owned(),
            source_azure_snapshot_id: "azure-snapshot".to_owned(),
            manifest: CalculatorManifest {
                schema_version: CALCULATOR_MANIFEST_VERSION,
                calculator_contract_version: CALCULATOR_CONTRACT_VERSION.to_owned(),
                calculator_url: "https://azure.microsoft.com/en-us/pricing/calculator/".to_owned(),
                generated_at: "2026-08-23T12:00:00Z".to_owned(),
                currency: "USD".to_owned(),
                locale: "en-US".to_owned(),
                items: vec![CalculatorManifestItem {
                    item_key: "001".to_owned(),
                    display_name: "Workload 001".to_owned(),
                    product: CalculatorProduct::AzureSqlManagedInstance,
                    region: "eastus".to_owned(),
                    deployment_model: CalculatorDeploymentModel::SingleInstance,
                    service_tier: CalculatorServiceTier::NextGenerationGeneralPurpose,
                    hardware_family: CalculatorHardwareFamily::PremiumSeries,
                    vcores: 8,
                    selected_memory_gb: decimal("64"),
                    zone_redundant: false,
                    quantity: 1,
                    hours_per_month: decimal("730"),
                    purchase_option: CalculatorPurchaseOption::Payg,
                    azure_hybrid_benefit: false,
                    data_storage_gb: decimal("256"),
                    backup_storage_gb: DecimalValue::ZERO,
                    expected_public_annual: CalculatorExpectedPublicAnnual {
                        compute: decimal("1000"),
                        additional_memory: decimal("0"),
                        license: decimal("500"),
                        storage: decimal("100"),
                        total_before_parity: decimal("1600"),
                    },
                }],
            },
        }
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("decimal"))
    }
}
