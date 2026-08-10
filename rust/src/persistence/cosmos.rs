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
use uuid::Uuid;

use crate::{
    calculation::engine::CalculationRevision,
    domain::project::{EditableProject, ProjectDocument},
};

use super::repository::{
    PROJECT_DOCUMENT_TYPE, ProjectRepository, RepositoryError, current_timestamp,
    new_project_document, updated_project_document, validate_stored_document,
};

const DATABASE_ID: &str = "tco";
const CONTAINER_ID: &str = "projects";
const OPERATION_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct CosmosProjectRepository {
    container: ContainerClient,
}

impl CosmosProjectRepository {
    pub async fn new(endpoint: &str, application_region: &str) -> Result<Self, RepositoryError> {
        let endpoint = endpoint
            .parse::<AccountEndpoint>()
            .map_err(|_| RepositoryError::Unavailable)?;
        let credential =
            ManagedIdentityCredential::new(None).map_err(|_| RepositoryError::Unavailable)?;
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
            .map_err(map_cosmos_error)?;
        let container = client
            .database_client(DATABASE_ID)
            .container_client(CONTAINER_ID)
            .await
            .map_err(map_cosmos_error)?;
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
            .query_items::<ProjectDocument>(query, FeedScope::partition(owner_id), None)
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
            .create_item(owner_id, &id.to_string(), &document, Some(options))
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
            .read_item(owner_id, &project_id.to_string(), None)
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
            .replace_item(owner_id, &project_id.to_string(), &document, Some(options))
            .await
            .map_err(map_cosmos_error)?;
        document.etag = Self::response_etag(&response)?;
        Ok(document)
    }

    async fn delete(&self, owner_id: &str, project_id: Uuid) -> Result<(), RepositoryError> {
        self.container
            .delete_item(owner_id, &project_id.to_string(), None)
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
