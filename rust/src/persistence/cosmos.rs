use std::time::Duration;

use async_trait::async_trait;
use azure_data_cosmos::{
    AccountEndpoint, AccountReference, CosmosClient, CosmosError, Query, RoutingStrategy,
    clients::ContainerClient,
    feed::FeedScope,
    models::ItemResponse,
    options::{
        AvailabilityStrategy, EndToEndOperationLatencyPolicy, ItemWriteOptions,
        OperationOptionsBuilder, Precondition, Region, ThrottlingRetryOptionsBuilder,
    },
};
use azure_identity::ManagedIdentityCredential;
use futures::TryStreamExt;
use serde::{Serialize, de::DeserializeOwned};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    calculation::engine::CalculationRevision,
    domain::project::{EditableProject, ProjectDocument},
    pricing::{
        repository::{
            DurableSnapshotRepository, RefreshLeaseDecision, RefreshLeaseOutcome,
            RefreshLeaseRepository, SnapshotRepositoryError,
        },
        snapshot::{AwsPriceSnapshot, AzurePriceSnapshot},
    },
    rate_limit::{RefreshQuotaDecision, RefreshQuotaError, RefreshQuotaRepository},
};

use super::{
    pricing_cache::{
        AWS_STATE_DOCUMENT_TYPE, AZURE_SNAPSHOT_DOCUMENT_TYPE, AwsEbsPriceDocument,
        AwsEc2PriceDocument, AwsPriceDocuments, AwsRdsPriceDocument, AwsSnapshotStateDocument,
        AzureSnapshotDocument, PRICING_CACHE_CONTAINER_ID, PRICING_CACHE_PARTITION,
        PricingCacheDocumentError, RefreshLeaseDocument, RefreshQuotaDocument,
        RefreshQuotaMutation, aws_component_document_id, aws_state_document_id,
    },
    privacy_consent::{
        PRIVACY_CONSENT_DOCUMENT_ID, PrivacyConsentDocument, PrivacyConsentError,
        PrivacyConsentProfile, PrivacyConsentRepository, new_document as new_consent_document,
        validate_document as validate_consent_document,
    },
    project_share::{
        CreatedProjectShare, PROJECT_SHARE_DOCUMENT_TYPE, ProjectShareCredentials,
        ProjectShareDocument, ProjectShareError, ProjectShareRepository, new_share_document,
        resolve_document, share_partition, validate_document as validate_share_document,
    },
    repository::{
        PROJECT_DOCUMENT_TYPE, ProjectRepository, RepositoryError, current_timestamp,
        new_project_document, updated_project_document, validate_stored_document,
    },
};

const DATABASE_ID: &str = "tco";
const PROJECTS_CONTAINER_ID: &str = "projects";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(20);
const SNAPSHOT_WRITE_ATTEMPTS: usize = 3;
const CONDITIONAL_WRITE_ATTEMPTS: usize = 5;
const MAX_COSMOS_ITEM_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct CosmosProjectRepository {
    container: ContainerClient,
}

impl CosmosProjectRepository {
    pub async fn new(endpoint: &str, application_region: &str) -> Result<Self, RepositoryError> {
        let container = cosmos_container(endpoint, application_region, PROJECTS_CONTAINER_ID)
            .await
            .map_err(|_| RepositoryError::Unavailable)?;
        Ok(Self { container })
    }

    fn response_etag(response: &ItemResponse) -> Result<String, RepositoryError> {
        response
            .headers()
            .etag()
            .map(ToString::to_string)
            .filter(|etag| !etag.is_empty())
            .ok_or(RepositoryError::Unavailable)
    }

    fn validate_document(
        document: &ProjectDocument,
        owner_id: &str,
        expected_id: Option<Uuid>,
    ) -> Result<(), RepositoryError> {
        validate_stored_document(document, owner_id, expected_id)
    }
}

async fn cosmos_container(
    endpoint: &str,
    application_region: &str,
    container_id: &str,
) -> Result<ContainerClient, ()> {
    let endpoint = endpoint.parse::<AccountEndpoint>().map_err(|_| ())?;
    let credential = ManagedIdentityCredential::new(None).map_err(|_| ())?;
    let account = AccountReference::with_credential(endpoint, credential);
    let throttling = ThrottlingRetryOptionsBuilder::new()
        .with_max_retry_count(3)
        .with_max_retry_wait_time(Duration::from_secs(10))
        .build();
    let operation = OperationOptionsBuilder::new()
        .with_availability_strategy(AvailabilityStrategy::Disabled)
        .with_end_to_end_latency_policy(EndToEndOperationLatencyPolicy::new(OPERATION_TIMEOUT))
        .with_max_failover_retry_count(1)
        .with_max_session_retry_count(1)
        .with_throttling_retry_options(throttling)
        .build();
    let client = CosmosClient::builder()
        .with_default_operation_options(operation)
        .build(
            account,
            RoutingStrategy::ProximityTo(Region::new(application_region.to_owned())),
        )
        .await
        .map_err(|_| ())?;
    client
        .database_client(DATABASE_ID)
        .container_client(container_id)
        .await
        .map_err(|_| ())
}

#[async_trait]
impl ProjectRepository for CosmosProjectRepository {
    async fn check_health(&self) -> Result<(), RepositoryError> {
        self.container
            .read(None)
            .await
            .map(|_| ())
            .map_err(map_cosmos_error)
    }

    async fn list(&self, owner_id: &str) -> Result<Vec<ProjectDocument>, RepositoryError> {
        let query = Query::from("SELECT * FROM c WHERE c.document_type = @document_type")
            .with_parameter("@document_type", PROJECT_DOCUMENT_TYPE)
            .map_err(map_cosmos_error)?;
        let mut items = self
            .container
            .query_items::<ProjectDocument>(query, FeedScope::partition(owner_id.to_owned()), None)
            .await
            .map_err(map_cosmos_error)?;
        let mut documents = Vec::new();
        while let Some(document) = items.try_next().await.map_err(map_cosmos_error)? {
            Self::validate_document(&document, owner_id, None)?;
            documents.push(document);
        }
        documents.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(documents)
    }

    async fn create(
        &self,
        owner_id: &str,
        project: EditableProject,
        calculation_revision: Option<CalculationRevision>,
    ) -> Result<ProjectDocument, RepositoryError> {
        let id = Uuid::new_v4();
        let mut document = new_project_document(
            owner_id,
            id,
            project,
            calculation_revision,
            current_timestamp()?,
            String::new(),
        )?;
        let options =
            ItemWriteOptions::default().with_precondition(Precondition::if_none_match("*"));
        let response = self
            .container
            .create_item(
                owner_id.to_owned(),
                &id.to_string(),
                &document,
                Some(options),
            )
            .await
            .map_err(map_cosmos_error)?;
        document.etag = Self::response_etag(&response)?;
        Ok(document)
    }

    async fn get(
        &self,
        owner_id: &str,
        project_id: Uuid,
    ) -> Result<ProjectDocument, RepositoryError> {
        let response = self
            .container
            .read_item(owner_id.to_owned(), &project_id.to_string(), None)
            .await
            .map_err(map_cosmos_error)?;
        let etag = Self::response_etag(&response)?;
        let mut document = response
            .into_model::<ProjectDocument>()
            .map_err(map_cosmos_error)?;
        document.etag = etag;
        Self::validate_document(&document, owner_id, Some(project_id))?;
        Ok(document)
    }

    async fn update(
        &self,
        owner_id: &str,
        project_id: Uuid,
        if_match: &str,
        project: EditableProject,
        calculation_revision: Option<CalculationRevision>,
    ) -> Result<ProjectDocument, RepositoryError> {
        let current = self.get(owner_id, project_id).await?;
        if current.etag != if_match {
            return Err(RepositoryError::PreconditionFailed);
        }
        let mut document = updated_project_document(
            &current,
            project,
            calculation_revision,
            current_timestamp()?,
            String::new(),
        )?;
        let options =
            ItemWriteOptions::default().with_precondition(Precondition::if_match(if_match));
        let response = self
            .container
            .replace_item(
                owner_id.to_owned(),
                &project_id.to_string(),
                &document,
                Some(options),
            )
            .await
            .map_err(map_cosmos_error)?;
        document.etag = Self::response_etag(&response)?;
        Ok(document)
    }

    async fn delete(&self, owner_id: &str, project_id: Uuid) -> Result<(), RepositoryError> {
        self.container
            .delete_item(owner_id.to_owned(), &project_id.to_string(), None)
            .await
            .map(|_| ())
            .map_err(map_cosmos_error)
    }
}

#[async_trait]
impl PrivacyConsentRepository for CosmosProjectRepository {
    async fn get(
        &self,
        owner_id: &str,
    ) -> Result<Option<PrivacyConsentDocument>, PrivacyConsentError> {
        let response = match self
            .container
            .read_item(owner_id.to_owned(), PRIVACY_CONSENT_DOCUMENT_ID, None)
            .await
        {
            Ok(response) => response,
            Err(error) if error.status().is_not_found() => return Ok(None),
            Err(error) => return Err(map_consent_cosmos_error(error)),
        };
        let document = response
            .into_model::<PrivacyConsentDocument>()
            .map_err(map_consent_cosmos_error)?;
        validate_consent_document(&document, owner_id)?;
        Ok(Some(document))
    }

    async fn save(
        &self,
        owner_id: &str,
        profile: PrivacyConsentProfile,
    ) -> Result<PrivacyConsentDocument, PrivacyConsentError> {
        let document = new_consent_document(owner_id, profile)?;
        self.container
            .upsert_item(
                owner_id.to_owned(),
                PRIVACY_CONSENT_DOCUMENT_ID,
                &document,
                None,
            )
            .await
            .map_err(map_consent_cosmos_error)?;
        Ok(document)
    }
}

#[async_trait]
impl ProjectShareRepository for CosmosProjectRepository {
    async fn create(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
        project: EditableProject,
    ) -> Result<CreatedProjectShare, ProjectShareError> {
        let credentials = ProjectShareCredentials {
            share_id: Uuid::new_v4(),
            secret: Uuid::new_v4(),
        };
        let document = new_share_document(
            source_owner_id,
            source_project_id,
            project,
            &credentials,
            OffsetDateTime::now_utc(),
        )?;
        let options =
            ItemWriteOptions::default().with_precondition(Precondition::if_none_match("*"));
        self.container
            .create_item(
                document.partition_key.clone(),
                &document.id.to_string(),
                &document,
                Some(options),
            )
            .await
            .map_err(map_share_cosmos_error)?;
        Ok(CreatedProjectShare {
            credentials,
            expires_at: document.expires_at,
        })
    }

    async fn resolve(
        &self,
        credentials: &ProjectShareCredentials,
    ) -> Result<EditableProject, ProjectShareError> {
        let response = self
            .container
            .read_item(share_partition(), &credentials.share_id.to_string(), None)
            .await
            .map_err(map_share_cosmos_error)?;
        let document = response
            .into_model::<ProjectShareDocument>()
            .map_err(map_share_cosmos_error)?;
        let outcome = resolve_document(&document, credentials, OffsetDateTime::now_utc());
        if matches!(outcome, Err(ProjectShareError::Expired))
            && let Err(error) = self
                .container
                .delete_item(share_partition(), &credentials.share_id.to_string(), None)
                .await
        {
            let status = error.status();
            tracing::warn!(
                status_code = u16::from(status.status_code()),
                "Expired Cosmos project share cleanup failed"
            );
        }
        outcome
    }

    async fn revoke(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
        share_id: Uuid,
    ) -> Result<(), ProjectShareError> {
        let partition = share_partition();
        let response = self
            .container
            .read_item(partition.clone(), &share_id.to_string(), None)
            .await
            .map_err(map_share_cosmos_error)?;
        let document = response
            .into_model::<ProjectShareDocument>()
            .map_err(map_share_cosmos_error)?;
        validate_share_document(&document, share_id)?;
        if document.source_owner_id != source_owner_id
            || document.source_project_id != source_project_id
        {
            return Err(ProjectShareError::NotFound);
        }
        self.container
            .delete_item(partition, &share_id.to_string(), None)
            .await
            .map(|_| ())
            .map_err(map_share_cosmos_error)
    }

    async fn revoke_project(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
    ) -> Result<(), ProjectShareError> {
        let query = Query::from(
            "SELECT * FROM c WHERE c.document_type = @document_type AND c.source_owner_id = @source_owner_id AND c.source_project_id = @source_project_id",
        )
        .with_parameter("@document_type", PROJECT_SHARE_DOCUMENT_TYPE)
        .and_then(|query| query.with_parameter("@source_owner_id", source_owner_id))
        .and_then(|query| query.with_parameter("@source_project_id", source_project_id.to_string()))
        .map_err(map_share_cosmos_error)?;
        let partition = share_partition();
        let mut items = self
            .container
            .query_items::<ProjectShareDocument>(
                query,
                FeedScope::partition(partition.clone()),
                None,
            )
            .await
            .map_err(map_share_cosmos_error)?;
        while let Some(document) = items.try_next().await.map_err(map_share_cosmos_error)? {
            validate_share_document(&document, document.id)?;
            self.container
                .delete_item(partition.clone(), &document.id.to_string(), None)
                .await
                .map_err(map_share_cosmos_error)?;
        }
        Ok(())
    }
}

fn map_cosmos_error(error: CosmosError) -> RepositoryError {
    let status = error.status();
    tracing::warn!(
        status_code = u16::from(status.status_code()),
        sub_status = status.sub_status().map(|value| value.value()),
        "Cosmos project operation failed"
    );
    if status.is_not_found() {
        RepositoryError::NotFound
    } else if status.is_precondition_failed() {
        RepositoryError::PreconditionFailed
    } else if u16::from(status.status_code()) == 413 {
        RepositoryError::PayloadTooLarge
    } else {
        RepositoryError::Unavailable
    }
}

fn map_share_cosmos_error(error: CosmosError) -> ProjectShareError {
    let status = error.status();
    tracing::warn!(
        status_code = u16::from(status.status_code()),
        sub_status = status.sub_status().map(|value| value.value()),
        "Cosmos project share operation failed"
    );
    if status.is_not_found() {
        ProjectShareError::NotFound
    } else if u16::from(status.status_code()) == 413 {
        ProjectShareError::PayloadTooLarge
    } else {
        ProjectShareError::Unavailable
    }
}

fn map_consent_cosmos_error(error: CosmosError) -> PrivacyConsentError {
    let status = error.status();
    tracing::warn!(
        status_code = u16::from(status.status_code()),
        sub_status = status.sub_status().map(|value| value.value()),
        "Cosmos privacy consent operation failed"
    );
    PrivacyConsentError::Unavailable
}

#[derive(Clone)]
pub struct CosmosSnapshotRepository {
    container: ContainerClient,
}

impl CosmosSnapshotRepository {
    pub async fn new(
        endpoint: &str,
        application_region: &str,
    ) -> Result<Self, SnapshotRepositoryError> {
        let container = cosmos_container(endpoint, application_region, PRICING_CACHE_CONTAINER_ID)
            .await
            .map_err(|_| SnapshotRepositoryError::Unavailable)?;
        Ok(Self { container })
    }

    async fn put_document<T>(&self, document: T) -> Result<(), SnapshotRepositoryError>
    where
        T: SnapshotDocument + DeserializeOwned + Serialize,
    {
        document.clone().validate()?;
        match self
            .container
            .create_item(PRICING_CACHE_PARTITION, document.id(), &document, None)
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) if is_conflict(&error) => {}
            Err(error) => return Err(map_snapshot_cosmos_error(error)),
        }

        for _ in 0..SNAPSHOT_WRITE_ATTEMPTS {
            let response = match self
                .container
                .read_item(PRICING_CACHE_PARTITION, document.id(), None)
                .await
            {
                Ok(response) => response,
                Err(error) if error.status().is_not_found() => {
                    match self
                        .container
                        .create_item(PRICING_CACHE_PARTITION, document.id(), &document, None)
                        .await
                    {
                        Ok(_) => return Ok(()),
                        Err(error) if is_conflict(&error) => continue,
                        Err(error) => return Err(map_snapshot_cosmos_error(error)),
                    }
                }
                Err(error) => return Err(map_snapshot_cosmos_error(error)),
            };
            let etag = response
                .headers()
                .etag()
                .map(ToString::to_string)
                .filter(|etag| !etag.is_empty())
                .ok_or(SnapshotRepositoryError::InvalidData)?;
            let existing = response
                .into_model::<T>()
                .map_err(map_snapshot_cosmos_error)?;
            existing.clone().validate()?;
            if !retrieved_later(document.retrieved_at(), existing.retrieved_at())? {
                return Ok(());
            }
            let options =
                ItemWriteOptions::default().with_precondition(Precondition::if_match(etag));
            match self
                .container
                .replace_item(
                    PRICING_CACHE_PARTITION,
                    document.id(),
                    &document,
                    Some(options),
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(error)
                    if error.status().is_precondition_failed() || error.status().is_not_found() =>
                {
                    continue;
                }
                Err(error) => return Err(map_snapshot_cosmos_error(error)),
            }
        }
        Err(SnapshotRepositoryError::Unavailable)
    }

    async fn put_aws_component<T>(&self, document: T) -> Result<T, SnapshotRepositoryError>
    where
        T: AwsComponentPersistenceDocument + DeserializeOwned + Serialize,
    {
        document.clone().validate()?;
        validate_cosmos_item_size(&document)?;
        let options =
            ItemWriteOptions::default().with_precondition(Precondition::if_none_match("*"));
        match self
            .container
            .create_item(
                PRICING_CACHE_PARTITION,
                document.id(),
                &document,
                Some(options),
            )
            .await
        {
            Ok(_) => Ok(document),
            Err(error) if is_conflict(&error) || error.status().is_precondition_failed() => self
                .get_document::<T>(document.id())
                .await?
                .ok_or(SnapshotRepositoryError::Unavailable),
            Err(error) => Err(map_snapshot_cosmos_error(error)),
        }
    }

    async fn get_document<T>(&self, snapshot_id: &str) -> Result<Option<T>, SnapshotRepositoryError>
    where
        T: ValidatedDocument + DeserializeOwned,
    {
        let response = match self
            .container
            .read_item(PRICING_CACHE_PARTITION, snapshot_id, None)
            .await
        {
            Ok(response) => response,
            Err(error) if error.status().is_not_found() => return Ok(None),
            Err(error) => return Err(map_snapshot_cosmos_error(error)),
        };
        let document = response
            .into_model::<T>()
            .map_err(map_snapshot_cosmos_error)?;
        document.clone().validate()?;
        Ok(Some(document))
    }

    async fn query_documents<T>(&self, query: Query) -> Result<Vec<T>, SnapshotRepositoryError>
    where
        T: ValidatedDocument + DeserializeOwned + Send + 'static,
    {
        let mut items = self
            .container
            .query_items::<T>(
                query,
                FeedScope::partition(PRICING_CACHE_PARTITION.to_owned()),
                None,
            )
            .await
            .map_err(map_snapshot_cosmos_error)?;
        let mut documents = Vec::new();
        while let Some(document) = items.try_next().await.map_err(map_snapshot_cosmos_error)? {
            document.clone().validate()?;
            documents.push(document);
        }
        Ok(documents)
    }

    async fn load_aws_state(
        &self,
        state: AwsSnapshotStateDocument,
    ) -> Result<AwsPriceSnapshot, SnapshotRepositoryError> {
        AwsSnapshotStateDocument::validate(&state).map_err(map_cache_document_error)?;
        let ec2_id = aws_component_document_id("ec2", &state.ec2_content_sha256);
        let rds_id = aws_component_document_id("rds", &state.rds_content_sha256);
        let ebs_id = aws_component_document_id("ebs", &state.ebs_content_sha256);
        let (ec2, rds, ebs) = tokio::try_join!(
            self.get_document::<AwsEc2PriceDocument>(&ec2_id),
            self.get_document::<AwsRdsPriceDocument>(&rds_id),
            self.get_document::<AwsEbsPriceDocument>(&ebs_id),
        )?;
        state
            .into_snapshot(
                ec2.ok_or(SnapshotRepositoryError::InvalidData)?,
                rds.ok_or(SnapshotRepositoryError::InvalidData)?,
                ebs.ok_or(SnapshotRepositoryError::InvalidData)?,
            )
            .map_err(map_cache_document_error)
    }

    async fn delete_superseded_aws_components(
        &self,
        previous: Option<&AwsSnapshotStateDocument>,
        current: &AwsSnapshotStateDocument,
    ) {
        let Some(previous) = previous else {
            return;
        };
        for (service, previous_hash, current_hash) in [
            (
                "ec2",
                &previous.ec2_content_sha256,
                &current.ec2_content_sha256,
            ),
            (
                "rds",
                &previous.rds_content_sha256,
                &current.rds_content_sha256,
            ),
            (
                "ebs",
                &previous.ebs_content_sha256,
                &current.ebs_content_sha256,
            ),
        ] {
            if previous_hash == current_hash {
                continue;
            }
            let id = aws_component_document_id(service, previous_hash);
            if let Err(error) = self
                .container
                .delete_item(PRICING_CACHE_PARTITION, &id, None)
                .await
                && !error.status().is_not_found()
            {
                let status = error.status();
                tracing::warn!(
                    status_code = u16::from(status.status_code()),
                    sub_status = status.sub_status().map(|value| value.value()),
                    service,
                    "Superseded AWS price component cleanup failed"
                );
            }
        }
    }

    async fn read_refresh_lease(
        &self,
        lease_id: &str,
    ) -> Result<Option<(RefreshLeaseDocument, String)>, SnapshotRepositoryError> {
        let response = match self
            .container
            .read_item(PRICING_CACHE_PARTITION, lease_id, None)
            .await
        {
            Ok(response) => response,
            Err(error) if error.status().is_not_found() => return Ok(None),
            Err(error) => return Err(map_snapshot_cosmos_error(error)),
        };
        let etag = response
            .headers()
            .etag()
            .map(ToString::to_string)
            .filter(|etag| !etag.is_empty())
            .ok_or(SnapshotRepositoryError::InvalidData)?;
        let document = response
            .into_model::<RefreshLeaseDocument>()
            .map_err(map_snapshot_cosmos_error)?;
        document.validate().map_err(map_cache_document_error)?;
        Ok(Some((document, etag)))
    }

    async fn replace_refresh_lease(
        &self,
        document: &RefreshLeaseDocument,
        etag: &str,
    ) -> Result<bool, SnapshotRepositoryError> {
        document.validate().map_err(map_cache_document_error)?;
        let options = ItemWriteOptions::default().with_precondition(Precondition::if_match(etag));
        match self
            .container
            .replace_item(
                PRICING_CACHE_PARTITION,
                &document.id,
                document,
                Some(options),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if error.status().is_precondition_failed() || error.status().is_not_found() =>
            {
                Ok(false)
            }
            Err(error) => Err(map_snapshot_cosmos_error(error)),
        }
    }

    async fn read_refresh_quota(
        &self,
        quota_id: &str,
    ) -> Result<Option<(RefreshQuotaDocument, String)>, RefreshQuotaError> {
        let response = match self
            .container
            .read_item(PRICING_CACHE_PARTITION, quota_id, None)
            .await
        {
            Ok(response) => response,
            Err(error) if error.status().is_not_found() => return Ok(None),
            Err(error) => return Err(map_quota_cosmos_error(error)),
        };
        let etag = response
            .headers()
            .etag()
            .map(ToString::to_string)
            .filter(|etag| !etag.is_empty())
            .ok_or(RefreshQuotaError::InvalidData)?;
        let document = response
            .into_model::<RefreshQuotaDocument>()
            .map_err(|_| RefreshQuotaError::InvalidData)?;
        document.validate().map_err(map_quota_document_error)?;
        Ok(Some((document, etag)))
    }

    async fn replace_refresh_quota(
        &self,
        document: &RefreshQuotaDocument,
        etag: &str,
    ) -> Result<bool, RefreshQuotaError> {
        document.validate().map_err(map_quota_document_error)?;
        let options = ItemWriteOptions::default().with_precondition(Precondition::if_match(etag));
        match self
            .container
            .replace_item(
                PRICING_CACHE_PARTITION,
                &document.id,
                document,
                Some(options),
            )
            .await
        {
            Ok(_) => Ok(true),
            Err(error)
                if error.status().is_precondition_failed() || error.status().is_not_found() =>
            {
                Ok(false)
            }
            Err(error) => Err(map_quota_cosmos_error(error)),
        }
    }
}

#[async_trait]
impl DurableSnapshotRepository for CosmosSnapshotRepository {
    async fn put_aws(
        &self,
        snapshot: &AwsPriceSnapshot,
    ) -> Result<AwsPriceSnapshot, SnapshotRepositoryError> {
        let documents = AwsPriceDocuments::new(snapshot).map_err(map_cache_document_error)?;
        let previous = self
            .get_document::<AwsSnapshotStateDocument>(&documents.state.id)
            .await?;
        let (ec2, rds, ebs) = tokio::try_join!(
            self.put_aws_component(documents.ec2),
            self.put_aws_component(documents.rds),
            self.put_aws_component(documents.ebs),
        )?;
        let (published_state, _) =
            AwsPriceDocuments::state_for_components(snapshot, &ec2, &rds, &ebs)
                .map_err(map_cache_document_error)?;
        self.put_document(published_state.clone()).await?;
        let current = self
            .get_document::<AwsSnapshotStateDocument>(&published_state.id)
            .await?
            .ok_or(SnapshotRepositoryError::Unavailable)?;
        let current_snapshot = self.load_aws_state(current.clone()).await?;
        if same_aws_state_version(&current, &published_state) {
            self.delete_superseded_aws_components(previous.as_ref(), &current)
                .await;
        }
        Ok(current_snapshot)
    }

    async fn put_azure(
        &self,
        snapshot: &AzurePriceSnapshot,
    ) -> Result<(), SnapshotRepositoryError> {
        self.put_document(AzureSnapshotDocument::new(snapshot.clone()))
            .await
    }

    async fn get_aws(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
        if !valid_snapshot_id(snapshot_id, "aws-") {
            return Ok(None);
        }
        let query = Query::from(
            "SELECT * FROM c WHERE c.document_type = @document_type AND c.snapshot_id = @snapshot_id",
        )
        .with_parameter("@document_type", AWS_STATE_DOCUMENT_TYPE)
        .and_then(|query| query.with_parameter("@snapshot_id", snapshot_id))
        .map_err(map_snapshot_cosmos_error)?;
        let mut states = self
            .query_documents::<AwsSnapshotStateDocument>(query)
            .await?;
        match states.pop() {
            Some(state) if states.is_empty() => self.load_aws_state(state).await.map(Some),
            Some(_) => Err(SnapshotRepositoryError::InvalidData),
            None => Ok(None),
        }
    }

    async fn get_azure(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
        if !valid_snapshot_id(snapshot_id, "azure-") {
            return Ok(None);
        }
        self.get_document::<AzureSnapshotDocument>(snapshot_id)
            .await?
            .map(AzureSnapshotDocument::into_snapshot)
            .transpose()
            .map_err(map_cache_document_error)
    }

    async fn find_aws(
        &self,
        currency: &str,
        source_region: &str,
    ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
        let state = self
            .get_document::<AwsSnapshotStateDocument>(&aws_state_document_id(
                currency,
                source_region,
            ))
            .await?;
        match state {
            Some(state) => self.load_aws_state(state).await.map(Some),
            None => Ok(None),
        }
    }

    async fn find_azure(
        &self,
        currency: &str,
        target_region: &str,
    ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
        let query = Query::from(
            "SELECT * FROM c WHERE c.document_type = @document_type AND c.currency = @currency AND c.target_region = @target_region",
        )
        .with_parameter("@document_type", AZURE_SNAPSHOT_DOCUMENT_TYPE)
        .and_then(|query| query.with_parameter("@currency", currency))
        .and_then(|query| query.with_parameter("@target_region", target_region))
        .map_err(map_snapshot_cosmos_error)?;
        newest_document(self.query_documents::<AzureSnapshotDocument>(query).await?)?
            .map(AzureSnapshotDocument::into_snapshot)
            .transpose()
            .map_err(map_cache_document_error)
    }

    async fn list_latest_aws(&self) -> Result<Vec<AwsPriceSnapshot>, SnapshotRepositoryError> {
        let query = Query::from("SELECT * FROM c WHERE c.document_type = @document_type")
            .with_parameter("@document_type", AWS_STATE_DOCUMENT_TYPE)
            .map_err(map_snapshot_cosmos_error)?;
        let states = self
            .query_documents::<AwsSnapshotStateDocument>(query)
            .await?;
        let mut snapshots = Vec::with_capacity(states.len());
        for state in states {
            snapshots.push(self.load_aws_state(state).await?);
        }
        Ok(snapshots)
    }
}

#[async_trait]
impl RefreshLeaseRepository for CosmosSnapshotRepository {
    async fn claim_refresh_lease(
        &self,
        cache_key_sha256: &str,
        owner_token: &str,
        request_started_at: &str,
    ) -> Result<RefreshLeaseDecision, SnapshotRepositoryError> {
        let request_started_at = parse_utc_timestamp(request_started_at)?;
        for _ in 0..CONDITIONAL_WRITE_ATTEMPTS {
            let now = OffsetDateTime::now_utc();
            let candidate = RefreshLeaseDocument::new(cache_key_sha256, owner_token, now)
                .map_err(map_cache_document_error)?;
            match self
                .container
                .create_item(PRICING_CACHE_PARTITION, &candidate.id, &candidate, None)
                .await
            {
                Ok(_) => return Ok(RefreshLeaseDecision::Acquired),
                Err(error) if is_conflict(&error) => {}
                Err(error) => return Err(map_snapshot_cosmos_error(error)),
            }

            let Some((existing, etag)) = self.read_refresh_lease(&candidate.id).await? else {
                continue;
            };
            if let Some(decision) = existing
                .decision(request_started_at, now)
                .map_err(map_cache_document_error)?
            {
                return Ok(decision);
            }
            if self.replace_refresh_lease(&candidate, &etag).await? {
                return Ok(RefreshLeaseDecision::Acquired);
            }
        }
        Ok(RefreshLeaseDecision::Pending)
    }

    async fn publish_refresh_lease(
        &self,
        cache_key_sha256: &str,
        owner_token: &str,
        outcome: &RefreshLeaseOutcome,
    ) -> Result<(), SnapshotRepositoryError> {
        let lease_id = format!("refresh-lease-{cache_key_sha256}");
        for _ in 0..CONDITIONAL_WRITE_ATTEMPTS {
            let Some((existing, etag)) = self.read_refresh_lease(&lease_id).await? else {
                return Err(SnapshotRepositoryError::Unavailable);
            };
            if existing.owner_token != owner_token {
                return Err(SnapshotRepositoryError::Unavailable);
            }
            if existing
                .matches_outcome(owner_token, outcome)
                .map_err(map_cache_document_error)?
            {
                return Ok(());
            }
            let completed = existing
                .complete(owner_token, outcome, OffsetDateTime::now_utc())
                .map_err(|_| SnapshotRepositoryError::Unavailable)?;
            if self.replace_refresh_lease(&completed, &etag).await? {
                return Ok(());
            }
        }
        Err(SnapshotRepositoryError::Unavailable)
    }
}

#[async_trait]
impl RefreshQuotaRepository for CosmosSnapshotRepository {
    async fn consume_refresh_quota(
        &self,
        identity_sha256: &str,
        operation_token: &str,
        limit: u32,
    ) -> Result<RefreshQuotaDecision, RefreshQuotaError> {
        let initial = RefreshQuotaDocument::new(
            identity_sha256,
            operation_token,
            limit,
            OffsetDateTime::now_utc(),
        )
        .map_err(map_quota_document_error)?;
        let quota_id = initial.id.clone();

        for _ in 0..CONDITIONAL_WRITE_ATTEMPTS {
            let current = match self.read_refresh_quota(&quota_id).await {
                Ok(current) => current,
                Err(RefreshQuotaError::Unavailable) => continue,
                Err(error) => return Err(error),
            };
            let Some((current, etag)) = current else {
                let candidate = RefreshQuotaDocument::new(
                    identity_sha256,
                    operation_token,
                    limit,
                    OffsetDateTime::now_utc(),
                )
                .map_err(map_quota_document_error)?;
                let options =
                    ItemWriteOptions::default().with_precondition(Precondition::if_none_match("*"));
                match self
                    .container
                    .create_item(
                        PRICING_CACHE_PARTITION,
                        &candidate.id,
                        &candidate,
                        Some(options),
                    )
                    .await
                {
                    Ok(_) => return Ok(RefreshQuotaDecision::Allowed),
                    Err(error)
                        if is_conflict(&error) || error.status().is_precondition_failed() => {}
                    Err(error) => {
                        map_quota_cosmos_error(error);
                    }
                }
                continue;
            };

            match current
                .consume(
                    identity_sha256,
                    operation_token,
                    limit,
                    OffsetDateTime::now_utc(),
                )
                .map_err(map_quota_document_error)?
            {
                RefreshQuotaMutation::Allowed => return Ok(RefreshQuotaDecision::Allowed),
                RefreshQuotaMutation::Limited(retry_after_seconds) => {
                    return Ok(RefreshQuotaDecision::Limited {
                        retry_after_seconds,
                    });
                }
                RefreshQuotaMutation::Replace(updated) => {
                    match self.replace_refresh_quota(&updated, &etag).await {
                        Ok(true) => return Ok(RefreshQuotaDecision::Allowed),
                        Ok(false) | Err(RefreshQuotaError::Unavailable) => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
        }
        Err(RefreshQuotaError::Unavailable)
    }
}

trait ValidatedDocument: Clone {
    fn id(&self) -> &str;
    fn validate(self) -> Result<(), SnapshotRepositoryError>;
}

trait SnapshotDocument: ValidatedDocument {
    fn retrieved_at(&self) -> &str;
}

impl ValidatedDocument for AwsSnapshotStateDocument {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(self) -> Result<(), SnapshotRepositoryError> {
        AwsSnapshotStateDocument::validate(&self).map_err(map_cache_document_error)
    }
}

impl SnapshotDocument for AwsSnapshotStateDocument {
    fn retrieved_at(&self) -> &str {
        &self.metadata.retrieved_at
    }
}

trait AwsComponentPersistenceDocument: ValidatedDocument {}

macro_rules! impl_aws_component_persistence_document {
    ($type:ty) => {
        impl ValidatedDocument for $type {
            fn id(&self) -> &str {
                &self.id
            }

            fn validate(self) -> Result<(), SnapshotRepositoryError> {
                <$type>::validate(&self).map_err(map_cache_document_error)
            }
        }

        impl AwsComponentPersistenceDocument for $type {}
    };
}

impl_aws_component_persistence_document!(AwsEc2PriceDocument);
impl_aws_component_persistence_document!(AwsRdsPriceDocument);
impl_aws_component_persistence_document!(AwsEbsPriceDocument);

impl ValidatedDocument for AzureSnapshotDocument {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(self) -> Result<(), SnapshotRepositoryError> {
        self.into_snapshot()
            .map(|_| ())
            .map_err(map_cache_document_error)
    }
}

impl SnapshotDocument for AzureSnapshotDocument {
    fn retrieved_at(&self) -> &str {
        &self.retrieved_at
    }
}

fn newest_document<T: SnapshotDocument>(
    documents: Vec<T>,
) -> Result<Option<T>, SnapshotRepositoryError> {
    let mut newest: Option<T> = None;
    for document in documents {
        match newest.as_ref() {
            Some(current) if !retrieved_later(document.retrieved_at(), current.retrieved_at())? => {
            }
            _ => newest = Some(document),
        }
    }
    Ok(newest)
}

fn retrieved_later(incoming: &str, existing: &str) -> Result<bool, SnapshotRepositoryError> {
    let incoming = parse_utc_timestamp(incoming)?;
    let existing = parse_utc_timestamp(existing)?;
    Ok(incoming > existing)
}

fn parse_utc_timestamp(value: &str) -> Result<OffsetDateTime, SnapshotRepositoryError> {
    let timestamp =
        OffsetDateTime::parse(value, &Rfc3339).map_err(|_| SnapshotRepositoryError::InvalidData)?;
    if timestamp.offset() != time::UtcOffset::UTC {
        return Err(SnapshotRepositoryError::InvalidData);
    }
    Ok(timestamp)
}

fn valid_snapshot_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn same_aws_state_version(
    current: &AwsSnapshotStateDocument,
    published: &AwsSnapshotStateDocument,
) -> bool {
    current.snapshot_id == published.snapshot_id
        && current.ec2_content_sha256 == published.ec2_content_sha256
        && current.rds_content_sha256 == published.rds_content_sha256
        && current.ebs_content_sha256 == published.ebs_content_sha256
}

fn validate_cosmos_item_size(value: &impl Serialize) -> Result<(), SnapshotRepositoryError> {
    let bytes = serde_json::to_vec(value).map_err(|_| SnapshotRepositoryError::InvalidData)?;
    if bytes.len() >= MAX_COSMOS_ITEM_BYTES {
        Err(SnapshotRepositoryError::PayloadTooLarge)
    } else {
        Ok(())
    }
}

fn is_conflict(error: &CosmosError) -> bool {
    u16::from(error.status().status_code()) == 409
}

fn map_snapshot_cosmos_error(error: CosmosError) -> SnapshotRepositoryError {
    let status = error.status();
    tracing::warn!(
        status_code = u16::from(status.status_code()),
        sub_status = status.sub_status().map(|value| value.value()),
        "Cosmos pricing-cache operation failed"
    );
    if u16::from(status.status_code()) == 413 {
        SnapshotRepositoryError::PayloadTooLarge
    } else {
        SnapshotRepositoryError::Unavailable
    }
}

fn map_cache_document_error(_error: PricingCacheDocumentError) -> SnapshotRepositoryError {
    SnapshotRepositoryError::InvalidData
}

fn map_quota_cosmos_error(error: CosmosError) -> RefreshQuotaError {
    let status = error.status();
    tracing::warn!(
        status_code = u16::from(status.status_code()),
        sub_status = status.sub_status().map(|value| value.value()),
        "Cosmos refresh-quota operation failed"
    );
    RefreshQuotaError::Unavailable
}

fn map_quota_document_error(_error: PricingCacheDocumentError) -> RefreshQuotaError {
    RefreshQuotaError::InvalidData
}

#[cfg(test)]
mod pricing_cache_tests {
    use super::*;

    #[test]
    fn only_a_strictly_newer_retrieval_replaces_identical_content() {
        assert!(
            retrieved_later("2026-08-10T12:00:01Z", "2026-08-10T12:00:00Z")
                .expect("valid timestamps")
        );
        assert!(
            !retrieved_later("2026-08-10T12:00:00Z", "2026-08-10T12:00:00Z")
                .expect("valid timestamps")
        );
        assert!(
            !retrieved_later("2026-08-10T11:59:59Z", "2026-08-10T12:00:00Z")
                .expect("valid timestamps")
        );
    }

    #[test]
    fn point_reads_accept_only_canonical_snapshot_ids() {
        assert!(valid_snapshot_id(
            &format!("aws-{}", "a".repeat(64)),
            "aws-"
        ));
        assert!(valid_snapshot_id(
            &format!("azure-{}", "0".repeat(64)),
            "azure-"
        ));
        assert!(!valid_snapshot_id("aws-not-a-hash", "aws-"));
        assert!(!valid_snapshot_id(
            &format!("aws-{}", "g".repeat(64)),
            "aws-"
        ));
        assert!(!valid_snapshot_id(
            &format!("aws-{}", "A".repeat(64)),
            "aws-"
        ));
    }

    #[test]
    fn oversized_component_documents_are_rejected_before_cosmos() {
        assert!(validate_cosmos_item_size(&vec![0_u8; 16]).is_ok());
        assert_eq!(
            validate_cosmos_item_size(&"x".repeat(MAX_COSMOS_ITEM_BYTES)),
            Err(SnapshotRepositoryError::PayloadTooLarge)
        );
    }

    #[test]
    fn older_writer_cannot_clean_components_after_newer_state_wins() {
        let (aws, _) = crate::pricing::local_fixture::load().expect("valid local snapshot");
        let published = AwsPriceDocuments::new(&aws)
            .expect("published documents")
            .state;
        let mut current = published.clone();
        current.snapshot_id = format!("aws-{}", "0".repeat(64));
        current.metadata.snapshot_id = current.snapshot_id.clone();
        current.metadata.content_sha256 = "0".repeat(64);

        assert!(!same_aws_state_version(&current, &published));
        assert!(same_aws_state_version(&published, &published));
    }
}
