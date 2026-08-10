use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::project::{EditableProject, ProjectDocument};

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("project was not found")]
    NotFound,
    #[error("project ETag did not match")]
    PreconditionFailed,
    #[error("project document exceeds the persistence limit")]
    PayloadTooLarge,
    #[error("persistence service is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn list(&self, owner_id: &str) -> Result<Vec<ProjectDocument>, RepositoryError>;

    async fn create(
        &self,
        owner_id: &str,
        project: EditableProject,
    ) -> Result<ProjectDocument, RepositoryError>;

    async fn get(
        &self,
        owner_id: &str,
        project_id: Uuid,
    ) -> Result<ProjectDocument, RepositoryError>;

    async fn update(
        &self,
        owner_id: &str,
        project_id: Uuid,
        if_match: &str,
        project: EditableProject,
    ) -> Result<ProjectDocument, RepositoryError>;

    async fn delete(&self, owner_id: &str, project_id: Uuid) -> Result<(), RepositoryError>;
}
