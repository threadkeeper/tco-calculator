use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::{
    calculation::engine::CalculationRevision,
    config::{FORMULA_VERSION, SCHEMA_VERSION},
    domain::project::{EditableProject, ProjectDocument},
};

pub const MAX_PROJECT_DOCUMENT_BYTES: usize = 1_800_000;
const PROJECT_DOCUMENT_TYPE: &str = "project";

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
        calculation_revision: Option<CalculationRevision>,
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
        calculation_revision: Option<CalculationRevision>,
    ) -> Result<ProjectDocument, RepositoryError>;

    async fn delete(&self, owner_id: &str, project_id: Uuid) -> Result<(), RepositoryError>;
}

#[derive(Clone, Default)]
pub struct InMemoryProjectRepository {
    projects: Arc<RwLock<HashMap<(String, Uuid), StoredProject>>>,
}

#[derive(Clone)]
struct StoredProject {
    document: ProjectDocument,
    version: u64,
}

impl InMemoryProjectRepository {
    pub fn new() -> Self {
        Self::default()
    }

    fn read_projects(
        &self,
    ) -> Result<
        std::sync::RwLockReadGuard<'_, HashMap<(String, Uuid), StoredProject>>,
        RepositoryError,
    > {
        self.projects
            .read()
            .map_err(|_| RepositoryError::Unavailable)
    }

    fn write_projects(
        &self,
    ) -> Result<
        std::sync::RwLockWriteGuard<'_, HashMap<(String, Uuid), StoredProject>>,
        RepositoryError,
    > {
        self.projects
            .write()
            .map_err(|_| RepositoryError::Unavailable)
    }
}

#[async_trait]
impl ProjectRepository for InMemoryProjectRepository {
    async fn list(&self, owner_id: &str) -> Result<Vec<ProjectDocument>, RepositoryError> {
        let projects = self.read_projects()?;
        let mut documents = projects
            .iter()
            .filter(|((stored_owner_id, _), _)| stored_owner_id == owner_id)
            .map(|(_, stored)| stored.document.clone())
            .collect::<Vec<_>>();
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
        let now = current_timestamp()?;
        let id = Uuid::new_v4();
        let version = 1;
        let document = ProjectDocument {
            id,
            document_type: PROJECT_DOCUMENT_TYPE.to_owned(),
            owner_id: owner_id.to_owned(),
            name: project.name,
            description: project.description,
            settings: project.settings,
            resources: project.resources,
            aws_price_snapshot_id: project.aws_price_snapshot_id,
            azure_price_snapshot_id: project.azure_price_snapshot_id,
            latest_calculation_revision: calculation_revision,
            formula_version: FORMULA_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
            created_at: now.clone(),
            updated_at: now,
            etag: etag(version),
        };
        enforce_document_size(&document)?;

        self.write_projects()?.insert(
            (owner_id.to_owned(), id),
            StoredProject {
                document: document.clone(),
                version,
            },
        );
        Ok(document)
    }

    async fn get(
        &self,
        owner_id: &str,
        project_id: Uuid,
    ) -> Result<ProjectDocument, RepositoryError> {
        self.read_projects()?
            .get(&(owner_id.to_owned(), project_id))
            .map(|stored| stored.document.clone())
            .ok_or(RepositoryError::NotFound)
    }

    async fn update(
        &self,
        owner_id: &str,
        project_id: Uuid,
        if_match: &str,
        project: EditableProject,
        calculation_revision: Option<CalculationRevision>,
    ) -> Result<ProjectDocument, RepositoryError> {
        let key = (owner_id.to_owned(), project_id);
        let mut projects = self.write_projects()?;
        let stored = projects.get_mut(&key).ok_or(RepositoryError::NotFound)?;
        if stored.document.etag != if_match {
            return Err(RepositoryError::PreconditionFailed);
        }

        let pricing_inputs_unchanged = pricing_inputs_unchanged(&stored.document, &project)?;
        let latest_calculation_revision = if pricing_inputs_unchanged {
            stored.document.latest_calculation_revision.clone()
        } else {
            calculation_revision
        };
        let version = stored
            .version
            .checked_add(1)
            .ok_or(RepositoryError::Unavailable)?;
        let document = ProjectDocument {
            id: project_id,
            document_type: PROJECT_DOCUMENT_TYPE.to_owned(),
            owner_id: owner_id.to_owned(),
            name: project.name,
            description: project.description,
            settings: project.settings,
            resources: project.resources,
            aws_price_snapshot_id: project.aws_price_snapshot_id,
            azure_price_snapshot_id: project.azure_price_snapshot_id,
            latest_calculation_revision,
            formula_version: FORMULA_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
            created_at: stored.document.created_at.clone(),
            updated_at: current_timestamp()?,
            etag: etag(version),
        };
        enforce_document_size(&document)?;

        *stored = StoredProject {
            document: document.clone(),
            version,
        };
        Ok(document)
    }

    async fn delete(&self, owner_id: &str, project_id: Uuid) -> Result<(), RepositoryError> {
        self.write_projects()?
            .remove(&(owner_id.to_owned(), project_id))
            .map(|_| ())
            .ok_or(RepositoryError::NotFound)
    }
}

fn current_timestamp() -> Result<String, RepositoryError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|_| RepositoryError::Unavailable)
}

fn etag(version: u64) -> String {
    format!("\"{version}\"")
}

fn enforce_document_size(document: &ProjectDocument) -> Result<(), RepositoryError> {
    let size = serde_json::to_vec(document)
        .map_err(|_| RepositoryError::Unavailable)?
        .len();
    if size >= MAX_PROJECT_DOCUMENT_BYTES {
        return Err(RepositoryError::PayloadTooLarge);
    }
    Ok(())
}

pub(crate) fn pricing_inputs_unchanged(
    stored: &ProjectDocument,
    editable: &EditableProject,
) -> Result<bool, RepositoryError> {
    let stored_inputs = serde_json::to_vec(&(
        &stored.settings,
        &stored.resources,
        &stored.aws_price_snapshot_id,
        &stored.azure_price_snapshot_id,
    ))
    .map_err(|_| RepositoryError::Unavailable)?;
    let editable_inputs = serde_json::to_vec(&(
        &editable.settings,
        &editable.resources,
        &editable.aws_price_snapshot_id,
        &editable.azure_price_snapshot_id,
    ))
    .map_err(|_| RepositoryError::Unavailable)?;
    Ok(stored_inputs == editable_inputs)
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::domain::{
        decimal::DecimalValue,
        project::ProjectSettings,
        resource::{
            LicenseBasis, OnPremResource, ProjectType, PurchaseOption, Resource, SharedResource,
            SqlEdition,
        },
    };

    #[tokio::test]
    async fn project_operations_are_owner_scoped() {
        let repository = InMemoryProjectRepository::new();
        let created = repository
            .create("entra:tenant-a:user", project("Private project"), None)
            .await
            .expect("create project");

        assert!(matches!(
            repository.get("entra:tenant-b:user", created.id).await,
            Err(RepositoryError::NotFound)
        ));
        assert!(
            repository
                .list("entra:tenant-b:user")
                .await
                .expect("list projects")
                .is_empty()
        );
        assert!(matches!(
            repository.delete("entra:tenant-b:user", created.id).await,
            Err(RepositoryError::NotFound)
        ));
        assert_eq!(
            repository
                .get("entra:tenant-a:user", created.id)
                .await
                .expect("owner can read")
                .name,
            "Private project"
        );
    }

    #[tokio::test]
    async fn updates_require_the_current_etag_and_invalidate_results() {
        let repository = InMemoryProjectRepository::new();
        let created = repository
            .create("entra:tenant:user", project("Version one"), None)
            .await
            .expect("create project");

        assert!(matches!(
            repository
                .update(
                    "entra:tenant:user",
                    created.id,
                    "\"stale\"",
                    project("Rejected"),
                    None,
                )
                .await,
            Err(RepositoryError::PreconditionFailed)
        ));

        let updated = repository
            .update(
                "entra:tenant:user",
                created.id,
                &created.etag,
                project("Version two"),
                None,
            )
            .await
            .expect("update project");

        assert_eq!(updated.name, "Version two");
        assert_ne!(updated.etag, created.etag);
        assert_eq!(updated.created_at, created.created_at);
    }

    #[tokio::test]
    async fn serialized_documents_at_the_limit_are_rejected() {
        let repository = InMemoryProjectRepository::new();
        let mut oversized = project("Oversized");
        if let Resource::OnPrem(resource) = &mut oversized.resources[0] {
            resource.shared.workload_name = "x".repeat(MAX_PROJECT_DOCUMENT_BYTES);
        }

        assert!(matches!(
            repository
                .create("entra:tenant:user", oversized, None)
                .await,
            Err(RepositoryError::PayloadTooLarge)
        ));
    }

    #[tokio::test]
    async fn metadata_edits_retain_revisions_and_pricing_edits_invalidate_them() {
        let repository = InMemoryProjectRepository::new();
        let original = project("Original");
        let created = repository
            .create(
                "entra:tenant:user",
                original.clone(),
                Some(revision("original revision")),
            )
            .await
            .expect("create project");

        let mut renamed = original.clone();
        renamed.name = "Renamed".to_owned();
        let metadata_update = repository
            .update(
                "entra:tenant:user",
                created.id,
                &created.etag,
                renamed.clone(),
                None,
            )
            .await
            .expect("rename project");
        assert_eq!(
            metadata_update
                .latest_calculation_revision
                .as_ref()
                .expect("retained revision")
                .warnings,
            ["original revision"]
        );

        renamed.settings.selected_parity_adjustment = DecimalValue(Decimal::new(1, 1));
        let pricing_update = repository
            .update(
                "entra:tenant:user",
                created.id,
                &metadata_update.etag,
                renamed,
                None,
            )
            .await
            .expect("update pricing inputs");
        assert!(pricing_update.latest_calculation_revision.is_none());
    }

    #[test]
    fn only_pricing_inputs_control_revision_retention() {
        let stored_project = project("Original");
        let mut renamed = project("Renamed");
        renamed.resources = stored_project.resources.clone();
        let stored = document(stored_project);

        assert!(pricing_inputs_unchanged(&stored, &renamed).expect("compare metadata edit"));

        renamed.settings.default_annual_hours = DecimalValue(Decimal::from(4_000_u32));
        assert!(!pricing_inputs_unchanged(&stored, &renamed).expect("compare pricing edit"));
    }

    fn document(project: EditableProject) -> ProjectDocument {
        ProjectDocument {
            id: Uuid::new_v4(),
            document_type: PROJECT_DOCUMENT_TYPE.to_owned(),
            owner_id: "entra:tenant:user".to_owned(),
            name: project.name,
            description: project.description,
            settings: project.settings,
            resources: project.resources,
            aws_price_snapshot_id: project.aws_price_snapshot_id,
            azure_price_snapshot_id: project.azure_price_snapshot_id,
            latest_calculation_revision: None,
            formula_version: FORMULA_VERSION.to_owned(),
            schema_version: SCHEMA_VERSION.to_owned(),
            created_at: "2026-07-31T00:00:00Z".to_owned(),
            updated_at: "2026-07-31T00:00:00Z".to_owned(),
            etag: etag(1),
        }
    }

    fn revision(warning: &str) -> CalculationRevision {
        CalculationRevision {
            formula_version: FORMULA_VERSION.to_owned(),
            aws_snapshot_id: None,
            azure_snapshot_id: None,
            resource_results: Vec::new(),
            portfolio_totals: crate::calculation::engine::PortfolioTotals {
                aws_all_rows_total: Some(DecimalValue::ZERO),
                aws_mapped_rows_total: DecimalValue::ZERO,
                azure_mapped_rows_total: DecimalValue::ZERO,
                required_portfolio_adjustment: DecimalValue::ZERO,
                selected_parity_adjustment: DecimalValue::ZERO,
                portfolio_after_selected_parity: DecimalValue::ZERO,
                portfolio_difference: DecimalValue::ZERO,
                comparable_resource_count: 0,
                no_mapping_resource_count: 0,
                price_unavailable_resource_count: 0,
            },
            warnings: vec![warning.to_owned()],
        }
    }

    fn project(name: &str) -> EditableProject {
        EditableProject {
            name: name.to_owned(),
            description: None,
            settings: ProjectSettings {
                project_type: ProjectType::OnPrem,
                aws_region: None,
                azure_region: "swedencentral".to_owned(),
                currency: "USD".to_owned(),
                source_compute_discount: DecimalValue::ZERO,
                source_license_discount: DecimalValue::ZERO,
                source_storage_discount: DecimalValue::ZERO,
                azure_compute_discount: DecimalValue::ZERO,
                azure_license_discount: DecimalValue::ZERO,
                azure_storage_discount: DecimalValue::ZERO,
                selected_parity_adjustment: DecimalValue::ZERO,
                default_annual_hours: DecimalValue(Decimal::from(8_760_u32)),
                default_mi_purchase_option: PurchaseOption::Payg,
                enterprise_license_sa_usd_per_two_core_pack: Some(DecimalValue(Decimal::from(
                    1_000_u32,
                ))),
                standard_license_sa_usd_per_two_core_pack: Some(DecimalValue(Decimal::from(
                    500_u32,
                ))),
                remaining_coverage_months: Some(12),
                electricity_rate_usd_per_kwh: Some(DecimalValue(Decimal::new(12, 2))),
            },
            resources: vec![Resource::OnPrem(OnPremResource {
                shared: SharedResource {
                    id: Uuid::new_v4(),
                    workload_name: "Synthetic workload".to_owned(),
                    quantity: 1,
                    sql_edition: SqlEdition::Enterprise,
                    license_basis: LicenseBasis::Byol,
                    sql_data_gb_per_instance: DecimalValue(Decimal::from(1_024_u32)),
                    source_ram_gb_per_instance: DecimalValue(Decimal::from(128_u32)),
                    annual_hours_per_instance: DecimalValue(Decimal::from(8_760_u32)),
                    mi_purchase_option: PurchaseOption::Payg,
                },
                source_vcpu: 16,
                licensable_cores: 16,
                source_max_iops: 20_000,
                hardware_capex_usd: DecimalValue(Decimal::from(20_000_u32)),
                depreciation_years: DecimalValue(Decimal::from(5_u8)),
                average_power_kw_override: None,
            })],
            aws_price_snapshot_id: None,
            azure_price_snapshot_id: None,
        }
    }
}
