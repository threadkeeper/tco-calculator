use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    decimal::DecimalValue,
    resource::{ProjectType, PurchaseOption, Resource},
};

pub const MAX_PROJECT_RESOURCES: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectSettings {
    pub project_type: ProjectType,
    pub aws_region: Option<String>,
    pub azure_region: String,
    pub currency: String,
    pub source_compute_discount: DecimalValue,
    pub source_license_discount: DecimalValue,
    pub source_storage_discount: DecimalValue,
    pub azure_compute_discount: DecimalValue,
    pub azure_license_discount: DecimalValue,
    pub azure_storage_discount: DecimalValue,
    pub selected_parity_adjustment: DecimalValue,
    pub default_annual_hours: DecimalValue,
    pub default_mi_purchase_option: PurchaseOption,
    pub enterprise_license_sa_usd_per_two_core_pack: Option<DecimalValue>,
    pub standard_license_sa_usd_per_two_core_pack: Option<DecimalValue>,
    pub remaining_coverage_months: Option<u8>,
    pub electricity_rate_usd_per_kwh: Option<DecimalValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EditableProject {
    pub name: String,
    pub description: Option<String>,
    pub settings: ProjectSettings,
    pub resources: Vec<Resource>,
    pub aws_price_snapshot_id: Option<String>,
    pub azure_price_snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectDocument {
    pub id: Uuid,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub settings: ProjectSettings,
    pub resources: Vec<Resource>,
    pub aws_price_snapshot_id: Option<String>,
    pub azure_price_snapshot_id: Option<String>,
    pub formula_version: String,
    pub schema_version: String,
    pub created_at: String,
    pub updated_at: String,
    pub etag: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ValidationIssue {
    pub pointer: String,
    pub code: &'static str,
    pub message: String,
}

impl EditableProject {
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        let name_length = self.name.chars().count();

        if !(1..=100).contains(&name_length) {
            issues.push(issue(
                "/name",
                "length",
                "Project name must contain 1 to 100 characters.",
            ));
        }

        if self
            .description
            .as_ref()
            .is_some_and(|description| description.chars().count() > 500)
        {
            issues.push(issue(
                "/description",
                "length",
                "Project description must not exceed 500 characters.",
            ));
        }

        if self.resources.len() > MAX_PROJECT_RESOURCES {
            issues.push(issue(
                "/resources",
                "limit",
                "A project may contain at most 100 resources.",
            ));
        }

        for (index, resource) in self.resources.iter().enumerate() {
            if resource.project_type() != self.settings.project_type {
                issues.push(issue(
                    &format!("/resources/{index}/source_type"),
                    "project_type_mismatch",
                    "Resource source type must match the project type.",
                ));
            }

            let shared = resource.shared();
            if !(1..=10_000).contains(&shared.quantity) {
                issues.push(issue(
                    &format!("/resources/{index}/quantity"),
                    "range",
                    "Quantity must be between 1 and 10,000.",
                ));
            }
        }

        for (pointer, value) in [
            (
                "/settings/source_compute_discount",
                self.settings.source_compute_discount,
            ),
            (
                "/settings/source_license_discount",
                self.settings.source_license_discount,
            ),
            (
                "/settings/source_storage_discount",
                self.settings.source_storage_discount,
            ),
            (
                "/settings/azure_compute_discount",
                self.settings.azure_compute_discount,
            ),
            (
                "/settings/azure_license_discount",
                self.settings.azure_license_discount,
            ),
            (
                "/settings/azure_storage_discount",
                self.settings.azure_storage_discount,
            ),
            (
                "/settings/selected_parity_adjustment",
                self.settings.selected_parity_adjustment,
            ),
        ] {
            if !value.is_percent() {
                issues.push(issue(
                    pointer,
                    "range",
                    "Discounts and adjustments must be between 0 and 1.",
                ));
            }
        }

        issues
    }
}

fn issue(pointer: &str, code: &'static str, message: &str) -> ValidationIssue {
    ValidationIssue {
        pointer: pointer.to_owned(),
        code,
        message: message.to_owned(),
    }
}
