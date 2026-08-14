use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::{Arc, RwLock},
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::domain::project::EditableProject;

use super::repository::MAX_PROJECT_DOCUMENT_BYTES;

pub(crate) const PROJECT_SHARE_DOCUMENT_TYPE: &str = "project_share";
pub(crate) const PROJECT_SHARE_PARTITION: &str = "project-shares";
const SHARE_LIFETIME_DAYS: i64 = 30;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProjectShareDocument {
    pub id: Uuid,
    pub document_type: String,
    #[serde(rename = "owner_id")]
    pub partition_key: String,
    pub source_owner_id: String,
    pub source_project_id: Uuid,
    pub secret_sha256: String,
    pub project: EditableProject,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectShareCredentials {
    pub share_id: Uuid,
    pub secret: Uuid,
}

#[derive(Clone)]
pub struct CreatedProjectShare {
    pub credentials: ProjectShareCredentials,
    pub expires_at: String,
}

#[derive(Debug, Error)]
pub enum ProjectShareError {
    #[error("project share was not found")]
    NotFound,
    #[error("project share has expired")]
    Expired,
    #[error("project share document exceeds the persistence limit")]
    PayloadTooLarge,
    #[error("project share persistence is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait ProjectShareRepository: Send + Sync {
    async fn create(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
        project: EditableProject,
    ) -> Result<CreatedProjectShare, ProjectShareError>;

    async fn resolve(
        &self,
        credentials: &ProjectShareCredentials,
    ) -> Result<EditableProject, ProjectShareError>;

    async fn revoke(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
        share_id: Uuid,
    ) -> Result<(), ProjectShareError>;

    async fn revoke_project(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
    ) -> Result<(), ProjectShareError>;
}

#[derive(Clone, Default)]
pub struct InMemoryProjectShareRepository {
    shares: Arc<RwLock<HashMap<Uuid, ProjectShareDocument>>>,
}

impl InMemoryProjectShareRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ProjectShareRepository for InMemoryProjectShareRepository {
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
        self.shares
            .write()
            .map_err(|_| ProjectShareError::Unavailable)?
            .insert(credentials.share_id, document.clone());
        Ok(CreatedProjectShare {
            credentials,
            expires_at: document.expires_at,
        })
    }

    async fn resolve(
        &self,
        credentials: &ProjectShareCredentials,
    ) -> Result<EditableProject, ProjectShareError> {
        let document = self
            .shares
            .read()
            .map_err(|_| ProjectShareError::Unavailable)?
            .get(&credentials.share_id)
            .cloned()
            .ok_or(ProjectShareError::NotFound)?;
        let outcome = resolve_document(&document, credentials, OffsetDateTime::now_utc());
        if matches!(outcome, Err(ProjectShareError::Expired)) {
            self.shares
                .write()
                .map_err(|_| ProjectShareError::Unavailable)?
                .remove(&credentials.share_id);
        }
        outcome
    }

    async fn revoke(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
        share_id: Uuid,
    ) -> Result<(), ProjectShareError> {
        let mut shares = self
            .shares
            .write()
            .map_err(|_| ProjectShareError::Unavailable)?;
        let document = shares.get(&share_id).ok_or(ProjectShareError::NotFound)?;
        if document.source_owner_id != source_owner_id
            || document.source_project_id != source_project_id
        {
            return Err(ProjectShareError::NotFound);
        }
        shares.remove(&share_id);
        Ok(())
    }

    async fn revoke_project(
        &self,
        source_owner_id: &str,
        source_project_id: Uuid,
    ) -> Result<(), ProjectShareError> {
        self.shares
            .write()
            .map_err(|_| ProjectShareError::Unavailable)?
            .retain(|_, document| {
                document.source_owner_id != source_owner_id
                    || document.source_project_id != source_project_id
            });
        Ok(())
    }
}

pub(crate) fn share_partition() -> String {
    PROJECT_SHARE_PARTITION.to_owned()
}

pub(crate) fn new_share_document(
    source_owner_id: &str,
    source_project_id: Uuid,
    project: EditableProject,
    credentials: &ProjectShareCredentials,
    now: OffsetDateTime,
) -> Result<ProjectShareDocument, ProjectShareError> {
    let expires = now
        .checked_add(Duration::days(SHARE_LIFETIME_DAYS))
        .ok_or(ProjectShareError::Unavailable)?;
    let document = ProjectShareDocument {
        id: credentials.share_id,
        document_type: PROJECT_SHARE_DOCUMENT_TYPE.to_owned(),
        partition_key: share_partition(),
        source_owner_id: source_owner_id.to_owned(),
        source_project_id,
        secret_sha256: digest_secret(credentials.secret),
        project,
        created_at: format_timestamp(now)?,
        expires_at: format_timestamp(expires)?,
    };
    validate_document(&document, credentials.share_id)?;
    Ok(document)
}

pub(crate) fn resolve_document(
    document: &ProjectShareDocument,
    credentials: &ProjectShareCredentials,
    now: OffsetDateTime,
) -> Result<EditableProject, ProjectShareError> {
    validate_document(document, credentials.share_id)?;
    if document.secret_sha256 != digest_secret(credentials.secret) {
        return Err(ProjectShareError::NotFound);
    }
    let expires_at = OffsetDateTime::parse(&document.expires_at, &Rfc3339)
        .map_err(|_| ProjectShareError::Unavailable)?;
    if now >= expires_at {
        return Err(ProjectShareError::Expired);
    }
    Ok(document.project.clone())
}

pub(crate) fn validate_document(
    document: &ProjectShareDocument,
    expected_id: Uuid,
) -> Result<(), ProjectShareError> {
    if document.id != expected_id
        || document.document_type != PROJECT_SHARE_DOCUMENT_TYPE
        || document.partition_key != share_partition()
        || document.source_owner_id.is_empty()
        || document.secret_sha256.len() != 64
    {
        return Err(ProjectShareError::Unavailable);
    }
    let size = serde_json::to_vec(document)
        .map_err(|_| ProjectShareError::Unavailable)?
        .len();
    if size >= MAX_PROJECT_DOCUMENT_BYTES {
        return Err(ProjectShareError::PayloadTooLarge);
    }
    Ok(())
}

fn digest_secret(secret: Uuid) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn format_timestamp(timestamp: OffsetDateTime) -> Result<String, ProjectShareError> {
    timestamp
        .format(&Rfc3339)
        .map_err(|_| ProjectShareError::Unavailable)
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
    async fn shares_require_the_secret_and_owner_scoped_revocation() {
        let repository = InMemoryProjectShareRepository::new();
        let source_project_id = Uuid::new_v4();
        let created = repository
            .create("entra:tenant:owner", source_project_id, project("Shared"))
            .await
            .expect("create share");

        assert_eq!(
            repository
                .resolve(&created.credentials)
                .await
                .expect("resolve share")
                .name,
            "Shared"
        );
        let wrong_secret = ProjectShareCredentials {
            share_id: created.credentials.share_id,
            secret: Uuid::new_v4(),
        };
        assert!(matches!(
            repository.resolve(&wrong_secret).await,
            Err(ProjectShareError::NotFound)
        ));

        repository
            .revoke_project("entra:tenant:owner", source_project_id)
            .await
            .expect("revoke project shares");
        assert!(matches!(
            repository.resolve(&created.credentials).await,
            Err(ProjectShareError::NotFound)
        ));
        assert!(matches!(
            repository
                .revoke(
                    "entra:tenant:other",
                    source_project_id,
                    created.credentials.share_id,
                )
                .await,
            Err(ProjectShareError::NotFound)
        ));
    }

    #[test]
    fn expired_shares_are_rejected() {
        let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid timestamp");
        let credentials = ProjectShareCredentials {
            share_id: Uuid::new_v4(),
            secret: Uuid::new_v4(),
        };
        let document = new_share_document(
            "entra:tenant:owner",
            Uuid::new_v4(),
            project("Expired"),
            &credentials,
            now,
        )
        .expect("create share document");

        assert!(matches!(
            resolve_document(&document, &credentials, now + Duration::days(30)),
            Err(ProjectShareError::Expired)
        ));
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
                source_compute_discount: DecimalValue(Decimal::ZERO),
                source_license_discount: DecimalValue(Decimal::ZERO),
                source_storage_discount: DecimalValue(Decimal::ZERO),
                azure_compute_discount: DecimalValue(Decimal::ZERO),
                azure_license_discount: DecimalValue(Decimal::ZERO),
                azure_storage_discount: DecimalValue(Decimal::ZERO),
                selected_parity_adjustment: DecimalValue(Decimal::ZERO),
                default_annual_hours: DecimalValue(Decimal::from(8_760_u32)),
                default_mi_purchase_option: PurchaseOption::Payg,
                enterprise_license_sa_usd_per_two_core_pack: None,
                standard_license_sa_usd_per_two_core_pack: None,
                remaining_coverage_months: None,
                electricity_rate_usd_per_kwh: Some(DecimalValue(
                    Decimal::from_str_exact("0.12").expect("decimal"),
                )),
                sql_payg: None,
            },
            resources: vec![Resource::OnPrem(OnPremResource {
                shared: SharedResource {
                    id: Uuid::new_v4(),
                    workload_name: "SQL Server".to_owned(),
                    server_name: None,
                    quantity: 1,
                    sql_data_gb_per_instance: DecimalValue(Decimal::from(500_u32)),
                    source_ram_gb_per_instance: DecimalValue(Decimal::from(64_u32)),
                    annual_hours_per_instance: DecimalValue(Decimal::from(8_760_u32)),
                    sql_edition: SqlEdition::Enterprise,
                    license_basis: LicenseBasis::LicenseIncluded,
                    mi_purchase_option: PurchaseOption::Payg,
                },
                source_vcpu: 16,
                licensable_cores: 16,
                source_max_iops: 20_000,
                hardware_capex_usd: DecimalValue(Decimal::from(10_000_u32)),
                depreciation_years: DecimalValue(Decimal::from(5_u8)),
                average_power_kw_override: None,
            })],
            aws_price_snapshot_id: None,
            azure_price_snapshot_id: None,
        }
    }
}
