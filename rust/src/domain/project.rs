use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use super::{
    decimal::DecimalValue,
    resource::{
        EbsVolumeType, ProjectType, PurchaseOption, Resource, VmDiskRole, VmInstanceStoreUse,
    },
};
use crate::calculation::engine::CalculationRevision;

pub const MAX_PROJECT_RESOURCES: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SqlPaygSettings {
    pub enterprise_licensed_cores: u32,
    pub standard_licensed_cores: u32,
    pub software_assurance_annual_usd: DecimalValue,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub sql_payg: Option<SqlPaygSettings>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
    pub document_type: String,
    pub owner_id: String,
    pub name: String,
    pub description: Option<String>,
    pub settings: ProjectSettings,
    pub resources: Vec<Resource>,
    pub aws_price_snapshot_id: Option<String>,
    pub azure_price_snapshot_id: Option<String>,
    pub latest_calculation_revision: Option<CalculationRevision>,
    pub formula_version: String,
    pub schema_version: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, rename = "_etag", skip_serializing)]
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
        if self.settings.project_type == ProjectType::SqlPayg && !self.resources.is_empty() {
            issues.push(issue(
                "/resources",
                "not_allowed",
                "SQL Pay As You Go projects do not contain workload resources.",
            ));
        }

        for (pointer, value, prefix) in [
            (
                "/aws_price_snapshot_id",
                self.aws_price_snapshot_id.as_deref(),
                "aws-",
            ),
            (
                "/azure_price_snapshot_id",
                self.azure_price_snapshot_id.as_deref(),
                "azure-",
            ),
        ] {
            if value.is_some_and(|snapshot_id| !is_snapshot_id(snapshot_id, prefix)) {
                issues.push(issue(
                    pointer,
                    "format",
                    "Snapshot IDs must be server-issued content-addressed identifiers.",
                ));
            }
        }

        validate_settings(&self.settings, &mut issues);

        let mut resource_ids = HashSet::new();
        let mut volume_ids = HashSet::new();

        for (index, resource) in self.resources.iter().enumerate() {
            if resource.project_type() != self.settings.project_type {
                issues.push(issue(
                    &format!("/resources/{index}/source_type"),
                    "project_type_mismatch",
                    "Resource source type must match the project type.",
                ));
            }

            let shared = resource.shared();
            if !resource_ids.insert(shared.id) {
                issues.push(issue(
                    &format!("/resources/{index}/id"),
                    "duplicate",
                    "Resource IDs must be unique within a project.",
                ));
            }
            if !(1..=160).contains(&shared.workload_name.chars().count()) {
                issues.push(issue(
                    &format!("/resources/{index}/workload_name"),
                    "length",
                    "Workload name must contain 1 to 160 characters.",
                ));
            }
            if shared
                .server_name
                .as_ref()
                .is_some_and(|server_name| server_name.chars().count() > 160)
            {
                issues.push(issue(
                    &format!("/resources/{index}/server_name"),
                    "length",
                    "Server name must not exceed 160 characters.",
                ));
            }
            if !(1..=10_000).contains(&shared.quantity) {
                issues.push(issue(
                    &format!("/resources/{index}/quantity"),
                    "range",
                    "Quantity must be between 1 and 10,000.",
                ));
            }
            if let Some(sql) = resource.sql() {
                validate_decimal_range(
                    &mut issues,
                    &format!("/resources/{index}/sql_data_gb_per_instance"),
                    sql.sql_data_gb_per_instance,
                    Decimal::ZERO,
                    Decimal::from(1_000_000_000_u64),
                    false,
                    "SQL data size must be between 0 and 1,000,000,000 GB.",
                );
            }
            validate_decimal_range(
                &mut issues,
                &format!("/resources/{index}/source_ram_gb_per_instance"),
                shared.source_ram_gb_per_instance,
                Decimal::ZERO,
                Decimal::from(1_000_000_u64),
                true,
                "Source RAM must be greater than 0 and no more than 1,000,000 GB.",
            );
            validate_decimal_range(
                &mut issues,
                &format!("/resources/{index}/annual_hours_per_instance"),
                shared.annual_hours_per_instance,
                Decimal::ZERO,
                Decimal::from(8_784_u32),
                false,
                "Annual hours must be between 0 and 8,784.",
            );

            match resource {
                Resource::Ec2(resource) => {
                    if resource.instance_type.trim().is_empty() {
                        issues.push(issue(
                            &format!("/resources/{index}/instance_type"),
                            "required",
                            "EC2 instance type is required.",
                        ));
                    }
                    if !(1..=50).contains(&resource.volumes.len()) {
                        issues.push(issue(
                            &format!("/resources/{index}/volumes"),
                            "limit",
                            "EC2 resources require 1 to 50 volumes.",
                        ));
                    }
                    for (volume_index, volume) in resource.volumes.iter().enumerate() {
                        let prefix = format!("/resources/{index}/volumes/{volume_index}");
                        if !volume_ids.insert(volume.id) {
                            issues.push(issue(
                                &format!("{prefix}/id"),
                                "duplicate",
                                "Volume IDs must be unique within a project.",
                            ));
                        }
                        if !(1..=80).contains(&volume.label.chars().count()) {
                            issues.push(issue(
                                &format!("{prefix}/label"),
                                "length",
                                "Volume label must contain 1 to 80 characters.",
                            ));
                        }
                        if volume
                            .aws_volume_id
                            .as_ref()
                            .is_some_and(|id| id.chars().count() > 128)
                        {
                            issues.push(issue(
                                &format!("{prefix}/aws_volume_id"),
                                "length",
                                "AWS volume ID must not exceed 128 characters.",
                            ));
                        }
                        if volume.capacity_gb.0 < Decimal::ZERO {
                            issues.push(issue(
                                &format!("{prefix}/capacity_gb"),
                                "range",
                                "Volume capacity must not be negative.",
                            ));
                        }
                        if volume.volume_type != EbsVolumeType::Ephemeral
                            && volume.provisioned_iops.is_none()
                        {
                            issues.push(issue(
                                &format!("{prefix}/provisioned_iops"),
                                "required",
                                "gp3 and io2 volumes require provisioned IOPS.",
                            ));
                        }
                        if volume
                            .throughput_mibps
                            .is_some_and(|throughput| throughput.0 < Decimal::ZERO)
                        {
                            issues.push(issue(
                                &format!("{prefix}/throughput_mibps"),
                                "range",
                                "Volume throughput must not be negative.",
                            ));
                        }
                    }
                }
                Resource::Ec2Vm(resource) => {
                    if resource.instance_type.trim().is_empty() {
                        issues.push(issue(
                            &format!("/resources/{index}/instance_type"),
                            "required",
                            "EC2 instance type is required.",
                        ));
                    }
                    if resource.requirements.instance_store_use == VmInstanceStoreUse::Used {
                        if resource
                            .requirements
                            .required_local_temp_disk_gb
                            .is_none_or(|capacity| capacity.0 <= Decimal::ZERO)
                        {
                            issues.push(issue(
                                &format!(
                                    "/resources/{index}/requirements/required_local_temp_disk_gb"
                                ),
                                "required",
                                "Used instance storage requires a positive local capacity.",
                            ));
                        }
                        if resource
                            .requirements
                            .ephemeral_data_loss_acceptable
                            .is_none()
                        {
                            issues.push(issue(
                                &format!(
                                    "/resources/{index}/requirements/ephemeral_data_loss_acceptable"
                                ),
                                "required",
                                "Used instance storage requires an explicit ephemeral data-loss decision.",
                            ));
                        }
                    }
                    if let Some(target) = resource.requirements.requested_target_arm_sku.as_deref()
                        && (target.is_empty()
                            || target.len() > 100
                            || !target.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
                            }))
                    {
                        issues.push(issue(
                            &format!("/resources/{index}/requirements/requested_target_arm_sku"),
                            "format",
                            "Requested Azure VM target must be a bounded ARM SKU name.",
                        ));
                    }
                    if !(1..=50).contains(&resource.volumes.len()) {
                        issues.push(issue(
                            &format!("/resources/{index}/volumes"),
                            "limit",
                            "EC2 virtual machine resources require 1 to 50 volumes.",
                        ));
                    }
                    let os_disks = resource
                        .volumes
                        .iter()
                        .filter(|volume| volume.role == VmDiskRole::Os)
                        .count();
                    if os_disks != 1 {
                        issues.push(issue(
                            &format!("/resources/{index}/volumes"),
                            "os_disk",
                            "EC2 virtual machine resources require exactly one OS disk.",
                        ));
                    }
                    for (volume_index, volume) in resource.volumes.iter().enumerate() {
                        let prefix = format!("/resources/{index}/volumes/{volume_index}");
                        if !volume_ids.insert(volume.id) {
                            issues.push(issue(
                                &format!("{prefix}/id"),
                                "duplicate",
                                "Volume IDs must be unique within a project.",
                            ));
                        }
                        if !(1..=80).contains(&volume.label.chars().count()) {
                            issues.push(issue(
                                &format!("{prefix}/label"),
                                "length",
                                "Volume label must contain 1 to 80 characters.",
                            ));
                        }
                        if volume
                            .aws_volume_id
                            .as_ref()
                            .is_some_and(|id| id.chars().count() > 128)
                        {
                            issues.push(issue(
                                &format!("{prefix}/aws_volume_id"),
                                "length",
                                "AWS volume ID must not exceed 128 characters.",
                            ));
                        }
                        if volume.capacity_gb.0 < Decimal::ZERO {
                            issues.push(issue(
                                &format!("{prefix}/capacity_gb"),
                                "range",
                                "Volume capacity must not be negative.",
                            ));
                        }
                        if volume.volume_type == EbsVolumeType::Ephemeral {
                            issues.push(issue(
                                &format!("{prefix}/volume_type"),
                                "unsupported",
                                "Instance storage must use the VM instance-store requirements rather than a persistent volume row.",
                            ));
                        }
                        if volume.role == VmDiskRole::Unknown {
                            issues.push(issue(
                                &format!("{prefix}/role"),
                                "required",
                                "Persistent volume role must be confirmed before calculation.",
                            ));
                        }
                        if volume.volume_type != EbsVolumeType::Ephemeral
                            && volume.provisioned_iops.is_none()
                        {
                            issues.push(issue(
                                &format!("{prefix}/provisioned_iops"),
                                "required",
                                "gp3 and io2 volumes require provisioned IOPS.",
                            ));
                        }
                        if volume
                            .throughput_mibps
                            .is_some_and(|throughput| throughput.0 < Decimal::ZERO)
                        {
                            issues.push(issue(
                                &format!("{prefix}/throughput_mibps"),
                                "range",
                                "Volume throughput must not be negative.",
                            ));
                        }
                    }
                }
                Resource::Rds(resource) => {
                    for (field, value) in [
                        ("instance_type", resource.instance_type.as_str()),
                        ("commercial_term", resource.commercial_term.as_str()),
                        ("storage_class", resource.storage_class.as_str()),
                    ] {
                        if value.trim().is_empty() {
                            issues.push(issue(
                                &format!("/resources/{index}/{field}"),
                                "required",
                                "RDS catalog selections are required.",
                            ));
                        }
                    }
                    if resource.source_max_iops > 1_000_000_000 {
                        issues.push(issue(
                            &format!("/resources/{index}/source_max_iops"),
                            "range",
                            "Source maximum IOPS must not exceed 1,000,000,000.",
                        ));
                    }
                }
                Resource::OnPrem(resource) => {
                    for (field, value) in [
                        ("source_vcpu", resource.source_vcpu),
                        ("licensable_cores", resource.licensable_cores),
                    ] {
                        if !(1..=100_000).contains(&value) {
                            issues.push(issue(
                                &format!("/resources/{index}/{field}"),
                                "range",
                                "Core counts must be between 1 and 100,000.",
                            ));
                        }
                    }
                    if resource.source_max_iops > 1_000_000_000 {
                        issues.push(issue(
                            &format!("/resources/{index}/source_max_iops"),
                            "range",
                            "Source maximum IOPS must not exceed 1,000,000,000.",
                        ));
                    }
                    if resource.hardware_capex_usd.0 < Decimal::ZERO {
                        issues.push(issue(
                            &format!("/resources/{index}/hardware_capex_usd"),
                            "range",
                            "Hardware CAPEX must not be negative.",
                        ));
                    }
                    validate_decimal_range(
                        &mut issues,
                        &format!("/resources/{index}/depreciation_years"),
                        resource.depreciation_years,
                        Decimal::ZERO,
                        Decimal::from(50_u8),
                        true,
                        "Depreciation years must be greater than 0 and no more than 50.",
                    );
                    if resource
                        .average_power_kw_override
                        .is_some_and(|power| power.0 <= Decimal::ZERO)
                    {
                        issues.push(issue(
                            &format!("/resources/{index}/average_power_kw_override"),
                            "range",
                            "Average power override must be greater than 0.",
                        ));
                    }
                }
            }
        }

        issues
    }
}

fn is_snapshot_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn validate_settings(settings: &ProjectSettings, issues: &mut Vec<ValidationIssue>) {
    if settings.currency != "USD" {
        issues.push(issue(
            "/settings/currency",
            "unsupported",
            "Currency must be USD.",
        ));
    }
    if settings.azure_region.trim().is_empty() {
        issues.push(issue(
            "/settings/azure_region",
            "required",
            "Azure region is required.",
        ));
    }
    match settings.project_type {
        ProjectType::Ec2 | ProjectType::Ec2Vm | ProjectType::Rds
            if settings.aws_region.as_deref().is_none_or(str::is_empty) =>
        {
            issues.push(issue(
                "/settings/aws_region",
                "required",
                "AWS region is required for AWS projects.",
            ));
        }
        ProjectType::OnPrem | ProjectType::SqlPayg if settings.aws_region.is_some() => {
            issues.push(issue(
                "/settings/aws_region",
                "not_allowed",
                "AWS region must be absent for on-premises and SQL Pay As You Go projects.",
            ));
        }
        _ => {}
    }

    for (pointer, value) in [
        (
            "/settings/source_compute_discount",
            settings.source_compute_discount,
        ),
        (
            "/settings/source_license_discount",
            settings.source_license_discount,
        ),
        (
            "/settings/source_storage_discount",
            settings.source_storage_discount,
        ),
        (
            "/settings/azure_compute_discount",
            settings.azure_compute_discount,
        ),
        (
            "/settings/azure_license_discount",
            settings.azure_license_discount,
        ),
        (
            "/settings/azure_storage_discount",
            settings.azure_storage_discount,
        ),
        (
            "/settings/selected_parity_adjustment",
            settings.selected_parity_adjustment,
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

    validate_decimal_range(
        issues,
        "/settings/default_annual_hours",
        settings.default_annual_hours,
        Decimal::ZERO,
        Decimal::from(8_784_u32),
        false,
        "Default annual hours must be between 0 and 8,784.",
    );

    if settings.project_type == ProjectType::OnPrem {
        if settings.source_compute_discount != DecimalValue::ZERO
            || settings.source_storage_discount != DecimalValue::ZERO
        {
            issues.push(issue(
                "/settings",
                "on_prem_discount_not_allowed",
                "On-premises compute and storage discounts must be zero.",
            ));
        }
        for (pointer, value) in [
            (
                "/settings/enterprise_license_sa_usd_per_two_core_pack",
                settings.enterprise_license_sa_usd_per_two_core_pack,
            ),
            (
                "/settings/standard_license_sa_usd_per_two_core_pack",
                settings.standard_license_sa_usd_per_two_core_pack,
            ),
        ] {
            if value.is_none_or(|price| price.0 <= Decimal::ZERO) {
                issues.push(issue(
                    pointer,
                    "required",
                    "On-premises License + SA pack prices must be greater than 0.",
                ));
            }
        }
        if !matches!(settings.remaining_coverage_months, Some(12 | 24 | 36)) {
            issues.push(issue(
                "/settings/remaining_coverage_months",
                "required",
                "Remaining coverage must be 12, 24, or 36 months.",
            ));
        }
        if settings
            .electricity_rate_usd_per_kwh
            .is_none_or(|rate| rate.0 < Decimal::ZERO)
        {
            issues.push(issue(
                "/settings/electricity_rate_usd_per_kwh",
                "required",
                "On-premises electricity rate must be zero or greater.",
            ));
        }
    }

    if settings.project_type == ProjectType::SqlPayg {
        match settings.sql_payg.as_ref() {
            Some(sql_payg) => {
                if sql_payg.enterprise_licensed_cores > 100_000
                    || sql_payg.standard_licensed_cores > 100_000
                {
                    issues.push(issue(
                        "/settings/sql_payg",
                        "range",
                        "Enterprise and Standard licensed cores must not exceed 100,000.",
                    ));
                }
                if sql_payg.enterprise_licensed_cores == 0 && sql_payg.standard_licensed_cores == 0
                {
                    issues.push(issue(
                        "/settings/sql_payg",
                        "required",
                        "Enter at least one Enterprise or Standard licensed core.",
                    ));
                }
                if sql_payg.software_assurance_annual_usd.0 < Decimal::ZERO {
                    issues.push(issue(
                        "/settings/sql_payg/software_assurance_annual_usd",
                        "range",
                        "Annual Software Assurance spend must not be negative.",
                    ));
                }
            }
            None => issues.push(issue(
                "/settings/sql_payg",
                "required",
                "SQL Pay As You Go licensing inputs are required.",
            )),
        }
    } else if settings.sql_payg.is_some() {
        issues.push(issue(
            "/settings/sql_payg",
            "not_allowed",
            "SQL Pay As You Go licensing inputs are allowed only for that project type.",
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_decimal_range(
    issues: &mut Vec<ValidationIssue>,
    pointer: &str,
    value: DecimalValue,
    minimum: Decimal,
    maximum: Decimal,
    minimum_exclusive: bool,
    message: &str,
) {
    let below_minimum = if minimum_exclusive {
        value.0 <= minimum
    } else {
        value.0 < minimum
    };
    if below_minimum || value.0 > maximum {
        issues.push(issue(pointer, "range", message));
    }
}

fn issue(pointer: &str, code: &'static str, message: &str) -> ValidationIssue {
    ValidationIssue {
        pointer: pointer.to_owned(),
        code,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{EditableProject, is_snapshot_id};
    use crate::domain::resource::Resource;

    #[test]
    fn snapshot_ids_are_provider_scoped_lowercase_sha256_values() {
        let digest = "a".repeat(64);
        assert!(is_snapshot_id(&format!("aws-{digest}"), "aws-"));
        assert!(!is_snapshot_id(&format!("azure-{digest}"), "aws-"));
        assert!(!is_snapshot_id(&format!("aws-{}", "A".repeat(64)), "aws-"));
        assert!(!is_snapshot_id("aws-short", "aws-"));
    }

    #[test]
    fn server_names_must_not_exceed_160_characters() {
        let mut project: EditableProject = serde_json::from_value(json!({
            "name": "Server name validation",
            "description": null,
            "settings": {
                "project_type": "ec2",
                "aws_region": "eu-west-1",
                "azure_region": "swedencentral",
                "currency": "USD",
                "source_compute_discount": "0",
                "source_license_discount": "0",
                "source_storage_discount": "0",
                "azure_compute_discount": "0",
                "azure_license_discount": "0",
                "azure_storage_discount": "0",
                "selected_parity_adjustment": "0",
                "default_annual_hours": "8760",
                "default_mi_purchase_option": "ahb",
                "enterprise_license_sa_usd_per_two_core_pack": null,
                "standard_license_sa_usd_per_two_core_pack": null,
                "remaining_coverage_months": null,
                "electricity_rate_usd_per_kwh": null
            },
            "resources": [{
                "source_type": "ec2",
                "id": "11111111-1111-4111-8111-111111111111",
                "workload_name": "SQL workload",
                "server_name": "a".repeat(161),
                "quantity": 1,
                "sql_edition": "enterprise",
                "license_basis": "byol",
                "sql_data_gb_per_instance": "1024",
                "source_ram_gb_per_instance": "256",
                "annual_hours_per_instance": "8760",
                "mi_purchase_option": "ahb",
                "instance_type": "r6id.8xlarge",
                "volumes": []
            }],
            "aws_price_snapshot_id": null,
            "azure_price_snapshot_id": null
        }))
        .expect("test project should deserialize");

        assert!(project.validate().iter().any(|issue| {
            issue.pointer == "/resources/0/server_name" && issue.code == "length"
        }));

        let Resource::Ec2(resource) = &mut project.resources[0] else {
            panic!("test resource should be EC2");
        };
        resource.shared.server_name = Some("a".repeat(160));
        assert!(
            !project
                .validate()
                .iter()
                .any(|issue| issue.pointer == "/resources/0/server_name")
        );
    }

    fn ec2_vm_project(volumes: serde_json::Value) -> EditableProject {
        serde_json::from_value(json!({
            "name": "EC2 virtual machine project",
            "description": null,
            "settings": {
                "project_type": "ec2_vm",
                "aws_region": "eu-west-1",
                "azure_region": "swedencentral",
                "currency": "USD",
                "source_compute_discount": "0",
                "source_license_discount": "0",
                "source_storage_discount": "0",
                "azure_compute_discount": "0",
                "azure_license_discount": "0",
                "azure_storage_discount": "0",
                "selected_parity_adjustment": "0",
                "default_annual_hours": "8760",
                "default_mi_purchase_option": "ahb",
                "enterprise_license_sa_usd_per_two_core_pack": null,
                "standard_license_sa_usd_per_two_core_pack": null,
                "remaining_coverage_months": null,
                "electricity_rate_usd_per_kwh": null
            },
            "resources": [{
                "source_type": "ec2_vm",
                "id": "11111111-1111-4111-8111-111111111111",
                "workload_name": "VM1",
                "server_name": null,
                "quantity": 1,
                "source_ram_gb_per_instance": "384",
                "annual_hours_per_instance": "8760",
                "instance_type": "r6id.12xlarge",
                "volumes": volumes
            }],
            "aws_price_snapshot_id": null,
            "azure_price_snapshot_id": null
        }))
        .expect("test project should deserialize")
    }

    fn vm_volume(id: &str, role: &str) -> serde_json::Value {
        json!({
            "id": id,
            "label": "Disk",
            "aws_volume_id": null,
            "volume_type": "gp3",
            "role": role,
            "capacity_gb": "1024",
            "provisioned_iops": 3000,
            "throughput_mibps": "125"
        })
    }

    #[test]
    fn ec2_vm_resources_accept_one_os_disk_and_data_disks() {
        let project = ec2_vm_project(json!([
            vm_volume("22222222-2222-4222-8222-222222222222", "os"),
            vm_volume("33333333-3333-4333-8333-333333333333", "data")
        ]));
        assert!(project.validate().is_empty(), "{:?}", project.validate());
    }

    #[test]
    fn ec2_vm_resources_require_exactly_one_os_disk() {
        for volumes in [
            json!([vm_volume("22222222-2222-4222-8222-222222222222", "data")]),
            json!([
                vm_volume("22222222-2222-4222-8222-222222222222", "os"),
                vm_volume("33333333-3333-4333-8333-333333333333", "os")
            ]),
        ] {
            let project = ec2_vm_project(volumes);
            assert!(
                project
                    .validate()
                    .iter()
                    .any(|issue| issue.pointer == "/resources/0/volumes"
                        && issue.code == "os_disk")
            );
        }
    }

    #[test]
    fn ec2_vm_persistent_volumes_require_provisioned_iops() {
        let mut volume = vm_volume("22222222-2222-4222-8222-222222222222", "os");
        volume["provisioned_iops"] = json!(null);
        let project = ec2_vm_project(json!([volume]));
        assert!(project.validate().iter().any(|issue| issue.pointer
            == "/resources/0/volumes/0/provisioned_iops"
            && issue.code == "required"));
    }
}
