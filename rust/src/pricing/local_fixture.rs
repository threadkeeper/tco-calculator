use serde::Deserialize;
use thiserror::Error;

use super::snapshot::{
    AwsEbsRateRecord, AwsEc2RateRecord, AwsPriceSnapshot, AwsRdsRateRecord, AzureMiRateRecord,
    AzurePriceSnapshot, SnapshotCreationMetadata, SnapshotError, utc_now_rfc3339,
};
use crate::pricing::provider::ResolutionStatus;

#[derive(Debug, Error)]
pub enum LocalFixtureError {
    #[error("embedded local price fixture is malformed")]
    Json(#[from] serde_json::Error),
    #[error("embedded local price fixture is invalid")]
    Snapshot(#[from] SnapshotError),
}

#[derive(Deserialize)]
struct LocalPriceFixture {
    aws: LocalAwsFixture,
    azure: LocalAzureFixture,
}

#[derive(Deserialize)]
struct LocalAwsFixture {
    metadata: SnapshotCreationMetadata,
    source_region: String,
    ec2_rates: Vec<AwsEc2RateRecord>,
    rds_rates: Vec<AwsRdsRateRecord>,
    ebs_rates: Vec<AwsEbsRateRecord>,
}

#[derive(Deserialize)]
struct LocalAzureFixture {
    metadata: SnapshotCreationMetadata,
    target_region: String,
    mi_rates: Vec<AzureMiRateRecord>,
}

pub fn load() -> Result<(AwsPriceSnapshot, AzurePriceSnapshot), LocalFixtureError> {
    let fixture: LocalPriceFixture = serde_json::from_str(include_str!(
        "../../../app/catalogs/local-price-fixture.json"
    ))?;
    let aws = AwsPriceSnapshot::create(
        fixture.aws.metadata,
        fixture.aws.source_region,
        fixture.aws.ec2_rates,
        fixture.aws.rds_rates,
        fixture.aws.ebs_rates,
    )?;
    let azure = AzurePriceSnapshot::create(
        fixture.azure.metadata,
        fixture.azure.target_region,
        fixture.azure.mi_rates,
    )?;
    Ok((aws, azure))
}

pub fn load_for_runtime() -> Result<(AwsPriceSnapshot, AzurePriceSnapshot), LocalFixtureError> {
    let (aws, azure) = load()?;
    let retrieved_at = utc_now_rfc3339()?;
    let aws = AwsPriceSnapshot::create(
        SnapshotCreationMetadata {
            status: ResolutionStatus::Cached,
            retrieved_at: retrieved_at.clone(),
            source_published_at: aws.metadata.source_published_at,
            currency: aws.metadata.currency,
            source_urls: aws.metadata.source_urls,
            parser_schema_version: aws.metadata.parser_schema_version,
            warnings: aws.metadata.warnings,
        },
        aws.source_region,
        aws.ec2_rates,
        aws.rds_rates,
        aws.ebs_rates,
    )?;
    let azure = AzurePriceSnapshot::create(
        SnapshotCreationMetadata {
            status: ResolutionStatus::Cached,
            retrieved_at,
            source_published_at: azure.metadata.source_published_at,
            currency: azure.metadata.currency,
            source_urls: azure.metadata.source_urls,
            parser_schema_version: azure.metadata.parser_schema_version,
            warnings: azure.metadata.warnings,
        },
        azure.target_region,
        azure.mi_rates,
    )?;
    Ok((aws, azure))
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use rust_decimal::Decimal;

    use super::*;
    use crate::{
        calculation::{
            engine::{CalculationEngine, CalculationInput},
            target_selector::{CapabilityCatalog, MappingStatus},
        },
        config::FORMULA_VERSION,
        domain::{
            decimal::DecimalValue,
            project::ProjectSettings,
            resource::{
                EbsVolume, EbsVolumeType, Ec2Resource, LicenseBasis, ProjectType, PurchaseOption,
                RdsDeployment, RdsResource, Resource, SharedResource, SqlEdition,
            },
        },
    };

    #[test]
    fn embedded_fixture_contains_the_reviewed_parity_anchor() {
        let (aws, azure) = load().expect("valid embedded fixture");
        let ec2 = aws.ec2_rate("r6id.8xlarge").expect("EC2 anchor");
        let mi = azure
            .mi_rate(
                "managed-vcore-next-gen-general-purpose-premium-series-32",
                PurchaseOption::Ahb,
            )
            .expect("SQL MI anchor");

        assert_eq!(ec2.rate.source_vcpu, 32);
        assert_eq!(ec2.rate.catalog_memory_gb.to_string(), "256.0000000000");
        assert_eq!(ec2.rate.compute_hourly.to_string(), "2.6880000000");
        assert_eq!(mi.rate.compute_hourly.to_string(), "5.632");
        assert_eq!(
            mi.rate.additional_memory_per_gb_hourly.to_string(),
            "0.011663"
        );
        assert!(
            azure.has_complete_mi_rate_set(
                "managed-vcore-next-gen-general-purpose-premium-series-32"
            )
        );
    }

    #[test]
    fn embedded_fixture_calculates_the_local_anchor_end_to_end() {
        let (aws, azure) = load().expect("valid embedded fixture");
        let capabilities: CapabilityCatalog = serde_json::from_str(include_str!(
            "../../../app/catalogs/sql-mi-capabilities.json"
        ))
        .expect("valid capability catalog");
        let engine = CalculationEngine::new(Arc::new(capabilities), FORMULA_VERSION)
            .expect("calculation engine");
        let resource = Resource::Ec2(Ec2Resource {
            shared: SharedResource {
                id: uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111")
                    .expect("resource UUID"),
                workload_name: "Synthetic parity anchor".to_owned(),
                quantity: 1,
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal("1024"),
                source_ram_gb_per_instance: decimal("256"),
                annual_hours_per_instance: decimal("8760"),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            instance_type: "r6id.8xlarge".to_owned(),
            volumes: vec![EbsVolume {
                id: uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222")
                    .expect("volume UUID"),
                label: "Instance storage".to_owned(),
                aws_volume_id: None,
                volume_type: EbsVolumeType::Ephemeral,
                capacity_gb: DecimalValue::ZERO,
                provisioned_iops: None,
                throughput_mibps: None,
            }],
        });
        let settings = ProjectSettings {
            project_type: ProjectType::Ec2,
            aws_region: Some("eu-west-1".to_owned()),
            azure_region: "swedencentral".to_owned(),
            currency: "USD".to_owned(),
            source_compute_discount: DecimalValue::ZERO,
            source_license_discount: DecimalValue::ZERO,
            source_storage_discount: DecimalValue::ZERO,
            azure_compute_discount: DecimalValue::ZERO,
            azure_license_discount: DecimalValue::ZERO,
            azure_storage_discount: DecimalValue::ZERO,
            selected_parity_adjustment: DecimalValue::ZERO,
            default_annual_hours: decimal("8760"),
            default_mi_purchase_option: PurchaseOption::Ahb,
            enterprise_license_sa_usd_per_two_core_pack: None,
            standard_license_sa_usd_per_two_core_pack: None,
            remaining_coverage_months: None,
            electricity_rate_usd_per_kwh: None,
            sql_payg: None,
        };

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some(FORMULA_VERSION),
            })
            .expect("calculate local anchor");
        let result = &revision.resource_results[0];
        let selected = result
            .target_selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref())
            .expect("selected target");

        assert_eq!(result.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(
            selected.configuration_key,
            "managed-vcore-next-gen-general-purpose-premium-series-32"
        );
        assert_eq!(selected.included_memory_gb, decimal("224"));
        assert_eq!(selected.selected_memory_gb, decimal("256"));
        assert_eq!(
            result.source_costs.as_ref().expect("source costs").total,
            decimal("23546.8800000000")
        );
        let azure_costs = result.azure_costs.as_ref().expect("Azure costs");
        assert_eq!(azure_costs.additional_ram_gb, decimal("32"));
        assert_eq!(azure_costs.additional_ram_gross, decimal("3269.3721600000"));
        assert_eq!(azure_costs.total_before_parity, decimal("54287.3049600000"));
    }

    #[test]
    fn embedded_fixture_calculates_the_local_rds_anchor_end_to_end() {
        let (aws, azure) = load().expect("valid embedded fixture");
        let rds = aws
            .rds_rate(
                "db.m6i.8xlarge",
                RdsDeployment::SingleAz,
                "on-demand",
                "gp3",
            )
            .expect("RDS anchor");
        assert_eq!(rds.rate.source_vcpu, 32);
        assert_eq!(rds.rate.effective_compute_hourly, decimal("5.1040000000"));
        assert_eq!(rds.rate.storage_monthly_per_gb, decimal("0.1270000000"));
        assert_eq!(
            rds.rate.standard_license_core_hourly,
            decimal("0.1200000000")
        );
        assert_eq!(
            rds.rate.enterprise_license_core_hourly,
            decimal("0.3750000000")
        );

        let capabilities: CapabilityCatalog = serde_json::from_str(include_str!(
            "../../../app/catalogs/sql-mi-capabilities.json"
        ))
        .expect("valid capability catalog");
        let engine = CalculationEngine::new(Arc::new(capabilities), FORMULA_VERSION)
            .expect("calculation engine");
        let resource = Resource::Rds(RdsResource {
            shared: SharedResource {
                id: uuid::Uuid::parse_str("33333333-3333-3333-3333-333333333333")
                    .expect("resource UUID"),
                workload_name: "Synthetic RDS anchor".to_owned(),
                quantity: 1,
                sql_edition: SqlEdition::Enterprise,
                license_basis: LicenseBasis::Byol,
                sql_data_gb_per_instance: decimal("1024"),
                source_ram_gb_per_instance: decimal("128"),
                annual_hours_per_instance: decimal("8760"),
                mi_purchase_option: PurchaseOption::Ahb,
            },
            instance_type: "db.m6i.8xlarge".to_owned(),
            deployment: RdsDeployment::SingleAz,
            commercial_term: "on-demand".to_owned(),
            storage_class: "gp3".to_owned(),
            source_max_iops: 0,
        });
        let settings = ProjectSettings {
            project_type: ProjectType::Rds,
            aws_region: Some("eu-west-1".to_owned()),
            azure_region: "swedencentral".to_owned(),
            currency: "USD".to_owned(),
            source_compute_discount: DecimalValue::ZERO,
            source_license_discount: DecimalValue::ZERO,
            source_storage_discount: DecimalValue::ZERO,
            azure_compute_discount: DecimalValue::ZERO,
            azure_license_discount: DecimalValue::ZERO,
            azure_storage_discount: DecimalValue::ZERO,
            selected_parity_adjustment: DecimalValue::ZERO,
            default_annual_hours: decimal("8760"),
            default_mi_purchase_option: PurchaseOption::Ahb,
            enterprise_license_sa_usd_per_two_core_pack: None,
            standard_license_sa_usd_per_two_core_pack: None,
            remaining_coverage_months: None,
            electricity_rate_usd_per_kwh: None,
            sql_payg: None,
        };

        let revision = engine
            .calculate(CalculationInput {
                settings: &settings,
                resources: &[resource],
                aws_snapshot: Some(&aws),
                azure_snapshot: Some(&azure),
                expected_formula_version: Some(FORMULA_VERSION),
            })
            .expect("calculate local RDS anchor");
        let result = &revision.resource_results[0];
        let selected = result
            .target_selection
            .as_ref()
            .and_then(|selection| selection.selected.as_ref())
            .expect("selected target");

        assert_eq!(result.mapping_status, Some(MappingStatus::Mapped));
        assert_eq!(
            selected.configuration_key,
            "managed-vcore-next-gen-general-purpose-premium-series-32"
        );
        assert_eq!(selected.included_memory_gb, decimal("224"));
        assert_eq!(selected.selected_memory_gb, decimal("224"));
        assert_eq!(
            result.source_costs.as_ref().expect("source costs").total,
            decimal("46271.6160000000")
        );
        let azure_costs = result.azure_costs.as_ref().expect("Azure costs");
        assert_eq!(azure_costs.additional_ram_gb, DecimalValue::ZERO);
        assert_eq!(azure_costs.total_before_parity, decimal("51017.9328000000"));
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(Decimal::from_str(value).expect("valid decimal"))
    }
}
