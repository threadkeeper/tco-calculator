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
        repository::{DurableSnapshotRepository, SnapshotRepositoryError},
        snapshot::{AwsPriceSnapshot, AzurePriceSnapshot},
    },
};

use super::{
    pricing_cache::{
        AWS_SNAPSHOT_DOCUMENT_TYPE, AZURE_SNAPSHOT_DOCUMENT_TYPE, AwsSnapshotDocument,
        AzureSnapshotDocument, PRICING_CACHE_CONTAINER_ID, PRICING_CACHE_PARTITION,
        PricingCacheDocumentError,
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

    async fn get_document<T>(&self, snapshot_id: &str) -> Result<Option<T>, SnapshotRepositoryError>
    where
        T: SnapshotDocument + DeserializeOwned,
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
        T: SnapshotDocument + DeserializeOwned + Send + 'static,
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
}

#[async_trait]
impl DurableSnapshotRepository for CosmosSnapshotRepository {
    async fn put_aws(&self, snapshot: &AwsPriceSnapshot) -> Result<(), SnapshotRepositoryError> {
        self.put_document(AwsSnapshotDocument::new(snapshot.clone()))
            .await
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
        self.get_document::<AwsSnapshotDocument>(snapshot_id)
            .await?
            .map(AwsSnapshotDocument::into_snapshot)
            .transpose()
            .map_err(map_cache_document_error)
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
        let query = Query::from(
            "SELECT * FROM c WHERE c.document_type = @document_type AND c.currency = @currency AND c.source_region = @source_region",
        )
        .with_parameter("@document_type", AWS_SNAPSHOT_DOCUMENT_TYPE)
        .and_then(|query| query.with_parameter("@currency", currency))
        .and_then(|query| query.with_parameter("@source_region", source_region))
        .map_err(map_snapshot_cosmos_error)?;
        newest_document(self.query_documents::<AwsSnapshotDocument>(query).await?)?
            .map(AwsSnapshotDocument::into_snapshot)
            .transpose()
            .map_err(map_cache_document_error)
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
            .with_parameter("@document_type", AWS_SNAPSHOT_DOCUMENT_TYPE)
            .map_err(map_snapshot_cosmos_error)?;
        let mut latest: std::collections::BTreeMap<(String, String), AwsSnapshotDocument> =
            std::collections::BTreeMap::new();
        for document in self.query_documents::<AwsSnapshotDocument>(query).await? {
            let key = (document.currency.clone(), document.source_region.clone());
            match latest.get(&key) {
                Some(current)
                    if !retrieved_later(document.retrieved_at(), current.retrieved_at())? => {}
                _ => {
                    latest.insert(key, document);
                }
            }
        }
        latest
            .into_values()
            .map(AwsSnapshotDocument::into_snapshot)
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_cache_document_error)
    }
}

trait SnapshotDocument: Clone {
    fn id(&self) -> &str;
    fn retrieved_at(&self) -> &str;
    fn validate(self) -> Result<(), SnapshotRepositoryError>;
}

impl SnapshotDocument for AwsSnapshotDocument {
    fn id(&self) -> &str {
        &self.id
    }

    fn retrieved_at(&self) -> &str {
        &self.retrieved_at
    }

    fn validate(self) -> Result<(), SnapshotRepositoryError> {
        self.into_snapshot()
            .map(|_| ())
            .map_err(map_cache_document_error)
    }
}

impl SnapshotDocument for AzureSnapshotDocument {
    fn id(&self) -> &str {
        &self.id
    }

    fn retrieved_at(&self) -> &str {
        &self.retrieved_at
    }

    fn validate(self) -> Result<(), SnapshotRepositoryError> {
        self.into_snapshot()
            .map(|_| ())
            .map_err(map_cache_document_error)
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
    let incoming = OffsetDateTime::parse(incoming, &Rfc3339)
        .map_err(|_| SnapshotRepositoryError::InvalidData)?;
    let existing = OffsetDateTime::parse(existing, &Rfc3339)
        .map_err(|_| SnapshotRepositoryError::InvalidData)?;
    Ok(incoming > existing)
}

fn valid_snapshot_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
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
}
