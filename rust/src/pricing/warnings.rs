use crate::domain::resource::{RdsDeployment, Resource, SqlEdition};

const EC2_STANDARD_PREFIX: &str = "EC2 Standard SQL rate for ";
const EC2_ENTERPRISE_PREFIX: &str = "EC2 Enterprise SQL rate for ";
const EC2_FALLBACK_SUFFIX: &str = " uses the regional four-core-minimum fallback.";
const RDS_STORAGE_PREFIX: &str = "RDS ";
const RDS_STORAGE_SUFFIX: &str = " has no matching SQL Server storage rate.";
const RDS_STANDARD_LICENSE_PREFIX: &str = "RDS Standard OCPU license meter is unavailable;";
const RDS_ENTERPRISE_LICENSE_PREFIX: &str = "RDS Enterprise OCPU license meter is unavailable;";

pub fn relevant_for_resources(warnings: &[String], resources: &[Resource]) -> Vec<String> {
    warnings
        .iter()
        .filter(|warning| is_relevant(warning, resources))
        .cloned()
        .collect()
}

fn is_relevant(warning: &str, resources: &[Resource]) -> bool {
    if let Some(instance_type) = warning
        .strip_prefix(EC2_STANDARD_PREFIX)
        .and_then(|value| value.strip_suffix(EC2_FALLBACK_SUFFIX))
    {
        return has_ec2(resources, instance_type, SqlEdition::Standard);
    }
    if let Some(instance_type) = warning
        .strip_prefix(EC2_ENTERPRISE_PREFIX)
        .and_then(|value| value.strip_suffix(EC2_FALLBACK_SUFFIX))
    {
        return has_ec2(resources, instance_type, SqlEdition::Enterprise);
    }
    if let Some(instance_and_deployment) = warning
        .strip_prefix(RDS_STORAGE_PREFIX)
        .and_then(|value| value.strip_suffix(RDS_STORAGE_SUFFIX))
    {
        let Some((instance_type, deployment)) = instance_and_deployment.split_once(' ') else {
            return true;
        };
        return resources.iter().any(|resource| {
            matches!(resource, Resource::Rds(rds) if rds.instance_type == instance_type && rds_deployment_key(rds.deployment) == deployment)
        });
    }
    if warning.starts_with(RDS_STANDARD_LICENSE_PREFIX) {
        return has_rds_edition(resources, SqlEdition::Standard);
    }
    if warning.starts_with(RDS_ENTERPRISE_LICENSE_PREFIX) {
        return has_rds_edition(resources, SqlEdition::Enterprise);
    }
    true
}

fn has_ec2(resources: &[Resource], instance_type: &str, edition: SqlEdition) -> bool {
    resources.iter().any(|resource| {
        matches!(resource, Resource::Ec2(ec2) if ec2.instance_type == instance_type && ec2.sql.sql_edition == edition)
    })
}

fn has_rds_edition(resources: &[Resource], edition: SqlEdition) -> bool {
    resources
        .iter()
        .any(|resource| matches!(resource, Resource::Rds(rds) if rds.sql.sql_edition == edition))
}

fn rds_deployment_key(deployment: RdsDeployment) -> &'static str {
    match deployment {
        RdsDeployment::SingleAz => "single-az",
        RdsDeployment::MultiAz => "multi-az",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn rds_project_keeps_only_matching_deployment_and_edition_warnings() {
        let resource: Resource = serde_json::from_value(json!({
            "source_type": "rds",
            "id": "00000000-0000-0000-0000-000000000000",
            "workload_name": "Synthetic workload",
            "quantity": 1,
            "sql_edition": "enterprise",
            "license_basis": "byol",
            "sql_data_gb_per_instance": "1024",
            "source_ram_gb_per_instance": "4096",
            "annual_hours_per_instance": "8760",
            "mi_purchase_option": "ahb",
            "instance_type": "db.x2m.32xlarge",
            "deployment": "single_az",
            "commercial_term": "on-demand",
            "storage_class": "gp3",
            "source_max_iops": 2300
        }))
        .expect("valid resource");
        let warnings = vec![
            "EC2 Enterprise SQL rate for c4.large uses the regional four-core-minimum fallback."
                .to_owned(),
            "RDS db.x2m.32xlarge multi-az has no matching SQL Server storage rate.".to_owned(),
            "RDS db.x2m.32xlarge single-az has no matching SQL Server storage rate.".to_owned(),
            "RDS Standard OCPU license meter is unavailable; using a fallback.".to_owned(),
            "RDS Enterprise OCPU license meter is unavailable; using a fallback.".to_owned(),
            "Azure pricing snapshot is stale but still within the usable window.".to_owned(),
        ];

        assert_eq!(
            relevant_for_resources(&warnings, &[resource]),
            vec![
                "RDS db.x2m.32xlarge single-az has no matching SQL Server storage rate.",
                "RDS Enterprise OCPU license meter is unavailable; using a fallback.",
                "Azure pricing snapshot is stale but still within the usable window.",
            ]
        );
    }

    #[test]
    fn ec2_project_keeps_only_matching_instance_and_edition_warning() {
        let resource: Resource = serde_json::from_value(json!({
            "source_type": "ec2",
            "id": "00000000-0000-0000-0000-000000000000",
            "workload_name": "Synthetic workload",
            "quantity": 1,
            "sql_edition": "standard",
            "license_basis": "byol",
            "sql_data_gb_per_instance": "1024",
            "source_ram_gb_per_instance": "64",
            "annual_hours_per_instance": "8760",
            "mi_purchase_option": "ahb",
            "instance_type": "c4.large",
            "volumes": []
        }))
        .expect("valid resource");
        let warnings = vec![
            "EC2 Standard SQL rate for c4.large uses the regional four-core-minimum fallback."
                .to_owned(),
            "EC2 Enterprise SQL rate for c4.large uses the regional four-core-minimum fallback."
                .to_owned(),
            "EC2 Standard SQL rate for c5.large uses the regional four-core-minimum fallback."
                .to_owned(),
            "RDS Standard OCPU license meter is unavailable; using a fallback.".to_owned(),
        ];

        assert_eq!(
            relevant_for_resources(&warnings, &[resource]),
            vec![
                "EC2 Standard SQL rate for c4.large uses the regional four-core-minimum fallback."
            ]
        );
    }
}
