use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::{
    decimal::DecimalValue,
    project::ProjectSettings,
    resource::{
        EbsVolumeType, Ec2Resource, Ec2VmResource, LicenseBasis, OnPremResource, RdsResource,
        SqlEdition, VmVolume,
    },
};

use super::vm_target_selector::SelectedManagedDisk;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceCostBreakdown {
    pub compute_gross: DecimalValue,
    pub compute_net: DecimalValue,
    pub license_gross: DecimalValue,
    pub license_net: DecimalValue,
    pub storage_gross: DecimalValue,
    pub storage_net: DecimalValue,
    pub hardware_annual: DecimalValue,
    pub electricity_annual: DecimalValue,
    pub total: DecimalValue,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OnPremExplanation {
    pub estimated_power_kw: DecimalValue,
    pub effective_power_kw: DecimalValue,
    pub power_override_applied: bool,
    pub annual_kwh: DecimalValue,
    pub electricity_monthly_average: DecimalValue,
    pub license_pack_count: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct OnPremCostResult {
    pub costs: SourceCostBreakdown,
    pub explanation: OnPremExplanation,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AzureCostBreakdown {
    pub compute_gross: DecimalValue,
    pub additional_ram_gb: DecimalValue,
    pub additional_ram_gross: DecimalValue,
    pub compute_plus_ram_net: DecimalValue,
    pub license_gross: DecimalValue,
    pub license_net: DecimalValue,
    pub storage_gross: DecimalValue,
    pub storage_net: DecimalValue,
    pub total_before_parity: DecimalValue,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SavingsBreakdown {
    pub compute_savings: DecimalValue,
    pub license_savings: DecimalValue,
    pub storage_savings: DecimalValue,
    pub total_savings: DecimalValue,
    pub required_adjustment: DecimalValue,
    pub selected_adjustment: DecimalValue,
    pub azure_after_selected_parity: DecimalValue,
    pub difference: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Ec2Rate {
    pub source_vcpu: u32,
    pub catalog_memory_gb: DecimalValue,
    pub compute_hourly: DecimalValue,
    pub standard_license_hourly: Option<DecimalValue>,
    pub enterprise_license_hourly: Option<DecimalValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EbsRate {
    pub volume_type: EbsVolumeType,
    pub capacity_monthly_per_gb: DecimalValue,
    pub included_iops: u64,
    pub iops_monthly_per_unit: Option<DecimalValue>,
    pub iops_tiers: Vec<IopsPriceTier>,
    pub included_throughput_mibps: DecimalValue,
    pub throughput_monthly_per_mibps: Option<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct IopsPriceTier {
    pub up_to_inclusive: Option<u64>,
    pub monthly_per_iops: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct RdsRate {
    pub source_vcpu: u32,
    pub catalog_memory_gb: DecimalValue,
    pub effective_compute_hourly: DecimalValue,
    pub storage_monthly_per_gb: DecimalValue,
    pub standard_license_core_hourly: DecimalValue,
    pub enterprise_license_core_hourly: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct AzureRate {
    pub compute_hourly: DecimalValue,
    pub license_hourly: DecimalValue,
    pub storage_monthly_per_gb: DecimalValue,
    pub additional_memory_per_gb_hourly: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct AzureManagedDiskRateSet {
    pub capacity_monthly: DecimalValue,
    pub additional_iops_monthly_per_unit: Option<DecimalValue>,
    pub additional_throughput_monthly_per_mbps: Option<DecimalValue>,
}

const AZURE_MI_STORAGE_UNIT_GB: i64 = 32;

pub fn azure_mi_configured_storage_gb(required_storage_gb: DecimalValue) -> DecimalValue {
    let storage_unit_gb = Decimal::from(AZURE_MI_STORAGE_UNIT_GB);
    let storage_units = (required_storage_gb.0 / storage_unit_gb)
        .ceil()
        .max(Decimal::ONE);
    DecimalValue(storage_units * storage_unit_gb)
}

pub fn azure_mi_billable_storage_gb(configured_storage_gb: DecimalValue) -> DecimalValue {
    DecimalValue(
        (configured_storage_gb.0 - Decimal::from(AZURE_MI_STORAGE_UNIT_GB)).max(Decimal::ZERO),
    )
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CostError {
    #[error("a required source SQL license rate is unavailable")]
    MissingSourceLicenseRate,
    #[error("a required EBS rate is unavailable for {0:?}")]
    MissingEbsRate(EbsVolumeType),
    #[error("persistent EBS volumes require an explicit provisioned IOPS value")]
    MissingProvisionedIops,
    #[error("the EBS IOPS tier schedule is invalid or incomplete")]
    InvalidIopsTiers,
    #[error("a required Azure managed-disk price dimension is unavailable")]
    MissingAzureManagedDiskRate,
    #[error("the Azure VM license rate exceeds its total hourly rate")]
    InvalidAzureVmRateComponents,
    #[error("on-premises License + SA settings are incomplete")]
    MissingOnPremLicenseSettings,
    #[error("on-premises electricity settings are incomplete")]
    MissingElectricityRate,
    #[error("a price rate must not be negative")]
    NegativeRate,
}

pub fn calculate_ec2_source(
    resource: &Ec2Resource,
    rate: Ec2Rate,
    ebs_rates: &[EbsRate],
    settings: &ProjectSettings,
) -> Result<SourceCostBreakdown, CostError> {
    validate_rate(rate.compute_hourly)?;
    let quantity = Decimal::from(resource.shared.quantity);
    let hours = resource.shared.annual_hours_per_instance.0;
    let compute_gross = quantity * hours * rate.compute_hourly.0;
    let compute_net = apply_discount(compute_gross, settings.source_compute_discount);
    let license_hourly = source_license_hourly(
        resource.sql.license_basis,
        resource.sql.sql_edition,
        rate.standard_license_hourly,
        rate.enterprise_license_hourly,
    )?;
    let license_gross = quantity * hours * license_hourly;
    let license_net = apply_discount(license_gross, settings.source_license_discount);
    let monthly_storage = resource
        .volumes
        .iter()
        .try_fold(Decimal::ZERO, |total, volume| {
            if volume.volume_type == EbsVolumeType::Ephemeral {
                return Ok(total);
            }
            let rate = ebs_rates
                .iter()
                .find(|rate| rate.volume_type == volume.volume_type)
                .ok_or(CostError::MissingEbsRate(volume.volume_type))?;
            Ok(total + calculate_ebs_volume_monthly(volume, rate)?)
        })?;
    let storage_gross = quantity * Decimal::from(12) * monthly_storage;
    let storage_net = apply_discount(storage_gross, settings.source_storage_discount);

    Ok(source_costs(
        compute_gross,
        compute_net,
        license_gross,
        license_net,
        storage_gross,
        storage_net,
        Decimal::ZERO,
        Decimal::ZERO,
    ))
}

pub fn calculate_ec2_vm_source(
    resource: &Ec2VmResource,
    rate: Ec2Rate,
    ebs_rates: &[EbsRate],
    settings: &ProjectSettings,
) -> Result<SourceCostBreakdown, CostError> {
    validate_rate(rate.compute_hourly)?;
    let quantity = Decimal::from(resource.shared.quantity);
    let hours = resource.shared.annual_hours_per_instance.0;
    let compute_gross = quantity * hours * rate.compute_hourly.0;
    let compute_net = apply_discount(compute_gross, settings.source_compute_discount);
    let monthly_storage = resource
        .volumes
        .iter()
        .try_fold(Decimal::ZERO, |total, volume| {
            if volume.volume_type == EbsVolumeType::Ephemeral {
                return Ok(total);
            }
            let rate = ebs_rates
                .iter()
                .find(|rate| rate.volume_type == volume.volume_type)
                .ok_or(CostError::MissingEbsRate(volume.volume_type))?;
            Ok(total + calculate_vm_volume_monthly(volume, rate)?)
        })?;
    let storage_gross = quantity * Decimal::from(12) * monthly_storage;
    let storage_net = apply_discount(storage_gross, settings.source_storage_discount);

    Ok(source_costs(
        compute_gross,
        compute_net,
        Decimal::ZERO,
        Decimal::ZERO,
        storage_gross,
        storage_net,
        Decimal::ZERO,
        Decimal::ZERO,
    ))
}

pub fn calculate_ebs_volume_monthly(
    volume: &crate::domain::resource::EbsVolume,
    rate: &EbsRate,
) -> Result<Decimal, CostError> {
    calculate_volume_monthly(
        volume.volume_type,
        volume.capacity_gb,
        volume.provisioned_iops,
        volume.throughput_mibps,
        rate,
    )
}

pub fn calculate_vm_volume_monthly(
    volume: &VmVolume,
    rate: &EbsRate,
) -> Result<Decimal, CostError> {
    calculate_volume_monthly(
        volume.volume_type,
        volume.capacity_gb,
        volume.provisioned_iops,
        volume.throughput_mibps,
        rate,
    )
}

fn calculate_volume_monthly(
    volume_type: EbsVolumeType,
    capacity_gb: DecimalValue,
    provisioned_iops: Option<u64>,
    throughput_mibps: Option<DecimalValue>,
    rate: &EbsRate,
) -> Result<Decimal, CostError> {
    if volume_type == EbsVolumeType::Ephemeral {
        return Ok(Decimal::ZERO);
    }
    validate_rate(rate.capacity_monthly_per_gb)?;
    let provisioned_iops = provisioned_iops.ok_or(CostError::MissingProvisionedIops)?;
    let capacity = capacity_gb.0 * rate.capacity_monthly_per_gb.0;
    let billable_iops = provisioned_iops.saturating_sub(rate.included_iops);
    let iops = if rate.volume_type == EbsVolumeType::Io2 {
        tiered_iops_cost(billable_iops, &rate.iops_tiers)?
    } else {
        match rate.iops_monthly_per_unit {
            Some(iops_rate) => {
                validate_rate(iops_rate)?;
                Decimal::from(billable_iops) * iops_rate.0
            }
            None if billable_iops == 0 => Decimal::ZERO,
            None => return Err(CostError::MissingEbsRate(rate.volume_type)),
        }
    };
    let throughput = throughput_mibps.map_or(Decimal::ZERO, |throughput| {
        (throughput.0 - rate.included_throughput_mibps.0).max(Decimal::ZERO)
    });
    let throughput_cost = match rate.throughput_monthly_per_mibps {
        Some(throughput_rate) => {
            validate_rate(throughput_rate)?;
            throughput * throughput_rate.0
        }
        None if rate.volume_type == EbsVolumeType::Io2 => Decimal::ZERO,
        None if throughput == Decimal::ZERO => Decimal::ZERO,
        None => return Err(CostError::MissingEbsRate(rate.volume_type)),
    };

    Ok(capacity + iops + throughput_cost)
}

pub fn calculate_azure_managed_disk_monthly(
    disk: &SelectedManagedDisk,
    rate: AzureManagedDiskRateSet,
) -> Result<DecimalValue, CostError> {
    validate_rate(rate.capacity_monthly)?;
    if disk.tier_key.is_some() {
        return Ok(rate.capacity_monthly);
    }

    let iops_rate = rate
        .additional_iops_monthly_per_unit
        .ok_or(CostError::MissingAzureManagedDiskRate)?;
    let throughput_rate = rate
        .additional_throughput_monthly_per_mbps
        .ok_or(CostError::MissingAzureManagedDiskRate)?;
    validate_rate(iops_rate)?;
    validate_rate(throughput_rate)?;

    Ok(DecimalValue(
        disk.capacity_gb.0 * rate.capacity_monthly.0
            + Decimal::from(disk.billed_additional_iops) * iops_rate.0
            + disk.billed_additional_throughput_mbps.0 * throughput_rate.0,
    ))
}

pub fn source_max_iops(resource: &Ec2Resource) -> Result<u64, CostError> {
    resource
        .volumes
        .iter()
        .filter(|volume| volume.volume_type != EbsVolumeType::Ephemeral)
        .try_fold(0, |maximum, volume| {
            let iops = volume
                .provisioned_iops
                .ok_or(CostError::MissingProvisionedIops)?;
            Ok(maximum.max(iops))
        })
}

pub fn calculate_rds_source(
    resource: &RdsResource,
    rate: RdsRate,
    settings: &ProjectSettings,
) -> Result<SourceCostBreakdown, CostError> {
    validate_rate(rate.effective_compute_hourly)?;
    validate_rate(rate.storage_monthly_per_gb)?;
    let quantity = Decimal::from(resource.shared.quantity);
    let hours = resource.shared.annual_hours_per_instance.0;
    let compute_gross = quantity * hours * rate.effective_compute_hourly.0;
    let compute_net = apply_discount(compute_gross, settings.source_compute_discount);
    let core_rate = match resource.sql.sql_edition {
        SqlEdition::Standard => rate.standard_license_core_hourly,
        SqlEdition::Enterprise => rate.enterprise_license_core_hourly,
    };
    validate_rate(core_rate)?;
    let license_gross = if resource.sql.license_basis == LicenseBasis::Byol {
        Decimal::ZERO
    } else {
        quantity * hours * Decimal::from(rate.source_vcpu) * core_rate.0
    };
    let license_net = apply_discount(license_gross, settings.source_license_discount);
    let storage_gross = quantity
        * resource.sql.sql_data_gb_per_instance.0
        * Decimal::from(12)
        * rate.storage_monthly_per_gb.0;
    let storage_net = apply_discount(storage_gross, settings.source_storage_discount);

    Ok(source_costs(
        compute_gross,
        compute_net,
        license_gross,
        license_net,
        storage_gross,
        storage_net,
        Decimal::ZERO,
        Decimal::ZERO,
    ))
}

pub fn calculate_on_prem_source(
    resource: &OnPremResource,
    settings: &ProjectSettings,
) -> Result<OnPremCostResult, CostError> {
    let license_price = match resource.sql.sql_edition {
        SqlEdition::Standard => settings.standard_license_sa_usd_per_two_core_pack,
        SqlEdition::Enterprise => settings.enterprise_license_sa_usd_per_two_core_pack,
    }
    .ok_or(CostError::MissingOnPremLicenseSettings)?;
    let coverage_months = settings
        .remaining_coverage_months
        .ok_or(CostError::MissingOnPremLicenseSettings)?;
    let electricity_rate = settings
        .electricity_rate_usd_per_kwh
        .ok_or(CostError::MissingElectricityRate)?;
    validate_rate(license_price)?;
    validate_rate(electricity_rate)?;

    let quantity = Decimal::from(resource.shared.quantity);
    let hours = resource.shared.annual_hours_per_instance.0;
    let estimated_power_kw = Decimal::new(100, 3)
        + Decimal::new(125, 4) * Decimal::from(resource.source_vcpu)
        + Decimal::new(375, 6) * resource.shared.source_ram_gb_per_instance.0
        + Decimal::new(10, 3) * resource.sql.sql_data_gb_per_instance.0 / Decimal::from(1024);
    let effective_power_kw = resource
        .average_power_kw_override
        .map_or(estimated_power_kw, |power| power.0);
    let annual_kwh = quantity * hours * effective_power_kw;
    let electricity_annual = annual_kwh * electricity_rate.0;
    let hardware_annual = quantity * resource.hardware_capex_usd.0 / resource.depreciation_years.0;
    let billable_cores = resource.licensable_cores.max(4);
    let license_pack_count = billable_cores.div_ceil(2);
    let license_gross =
        quantity * Decimal::from(license_pack_count) * license_price.0 * Decimal::from(12)
            / Decimal::from(coverage_months);
    let license_net = apply_discount(license_gross, settings.source_license_discount);
    let costs = source_costs(
        hardware_annual,
        hardware_annual,
        license_gross,
        license_net,
        Decimal::ZERO,
        Decimal::ZERO,
        hardware_annual,
        electricity_annual,
    );

    Ok(OnPremCostResult {
        costs,
        explanation: OnPremExplanation {
            estimated_power_kw: DecimalValue(estimated_power_kw),
            effective_power_kw: DecimalValue(effective_power_kw),
            power_override_applied: resource.average_power_kw_override.is_some(),
            annual_kwh: DecimalValue(annual_kwh),
            electricity_monthly_average: DecimalValue(electricity_annual / Decimal::from(12)),
            license_pack_count,
        },
    })
}

pub fn calculate_azure(
    quantity: u32,
    annual_hours: DecimalValue,
    azure_storage_gb_per_instance: DecimalValue,
    included_memory_gb: DecimalValue,
    selected_memory_gb: DecimalValue,
    rate: AzureRate,
    settings: &ProjectSettings,
) -> Result<AzureCostBreakdown, CostError> {
    for value in [
        rate.compute_hourly,
        rate.license_hourly,
        rate.storage_monthly_per_gb,
        rate.additional_memory_per_gb_hourly,
    ] {
        validate_rate(value)?;
    }
    let quantity = Decimal::from(quantity);
    let hours = annual_hours.0;
    let compute_gross = quantity * hours * rate.compute_hourly.0;
    let additional_ram_gb = (selected_memory_gb.0 - included_memory_gb.0).max(Decimal::ZERO);
    let additional_ram_gross =
        quantity * hours * additional_ram_gb * rate.additional_memory_per_gb_hourly.0;
    let compute_plus_ram_net = apply_discount(
        compute_gross + additional_ram_gross,
        settings.azure_compute_discount,
    );
    let license_gross = quantity * hours * rate.license_hourly.0;
    let license_net = apply_discount(license_gross, settings.azure_license_discount);
    let billable_storage_gb = azure_mi_billable_storage_gb(azure_storage_gb_per_instance);
    let storage_gross =
        quantity * billable_storage_gb.0 * Decimal::from(12) * rate.storage_monthly_per_gb.0;
    let storage_net = apply_discount(storage_gross, settings.azure_storage_discount);
    let total_before_parity = compute_plus_ram_net + license_net + storage_net;

    Ok(AzureCostBreakdown {
        compute_gross: DecimalValue(compute_gross),
        additional_ram_gb: DecimalValue(additional_ram_gb),
        additional_ram_gross: DecimalValue(additional_ram_gross),
        compute_plus_ram_net: DecimalValue(compute_plus_ram_net),
        license_gross: DecimalValue(license_gross),
        license_net: DecimalValue(license_net),
        storage_gross: DecimalValue(storage_gross),
        storage_net: DecimalValue(storage_net),
        total_before_parity: DecimalValue(total_before_parity),
    })
}

pub fn calculate_azure_vm(
    quantity: u32,
    annual_hours: DecimalValue,
    vm_total_hourly_rate: DecimalValue,
    vm_license_hourly_rate: DecimalValue,
    managed_disk_monthly_per_instance: DecimalValue,
    settings: &ProjectSettings,
) -> Result<AzureCostBreakdown, CostError> {
    validate_rate(vm_total_hourly_rate)?;
    validate_rate(vm_license_hourly_rate)?;
    validate_rate(managed_disk_monthly_per_instance)?;
    if vm_license_hourly_rate.0 > vm_total_hourly_rate.0 {
        return Err(CostError::InvalidAzureVmRateComponents);
    }
    let quantity = Decimal::from(quantity);
    let compute_hourly_rate = vm_total_hourly_rate.0 - vm_license_hourly_rate.0;
    let compute_gross = quantity * annual_hours.0 * compute_hourly_rate;
    let compute_net = apply_discount(compute_gross, settings.azure_compute_discount);
    let license_gross = quantity * annual_hours.0 * vm_license_hourly_rate.0;
    let license_net = apply_discount(license_gross, settings.azure_license_discount);
    let storage_gross = quantity * Decimal::from(12) * managed_disk_monthly_per_instance.0;
    let storage_net = apply_discount(storage_gross, settings.azure_storage_discount);

    Ok(AzureCostBreakdown {
        compute_gross: DecimalValue(compute_gross),
        additional_ram_gb: DecimalValue::ZERO,
        additional_ram_gross: DecimalValue::ZERO,
        compute_plus_ram_net: DecimalValue(compute_net),
        license_gross: DecimalValue(license_gross),
        license_net: DecimalValue(license_net),
        storage_gross: DecimalValue(storage_gross),
        storage_net: DecimalValue(storage_net),
        total_before_parity: DecimalValue(compute_net + license_net + storage_net),
    })
}

pub fn calculate_savings(
    source: &SourceCostBreakdown,
    azure: &AzureCostBreakdown,
    selected_adjustment: DecimalValue,
) -> SavingsBreakdown {
    let required_adjustment = if azure.total_before_parity.0 == Decimal::ZERO {
        Decimal::ZERO
    } else {
        Decimal::ONE - source.total.0 / azure.total_before_parity.0
    };
    let azure_after_selected_parity =
        azure.total_before_parity.0 * (Decimal::ONE - selected_adjustment.0);

    SavingsBreakdown {
        compute_savings: DecimalValue(source.compute_net.0 - azure.compute_plus_ram_net.0),
        license_savings: DecimalValue(source.license_net.0 - azure.license_net.0),
        storage_savings: DecimalValue(source.storage_net.0 - azure.storage_net.0),
        total_savings: DecimalValue(source.total.0 - azure.total_before_parity.0),
        required_adjustment: DecimalValue(required_adjustment),
        selected_adjustment,
        azure_after_selected_parity: DecimalValue(azure_after_selected_parity),
        difference: DecimalValue(azure_after_selected_parity - source.total.0),
    }
}

fn source_license_hourly(
    basis: LicenseBasis,
    edition: SqlEdition,
    standard: Option<DecimalValue>,
    enterprise: Option<DecimalValue>,
) -> Result<Decimal, CostError> {
    if basis == LicenseBasis::Byol {
        return Ok(Decimal::ZERO);
    }
    let rate = match edition {
        SqlEdition::Standard => standard,
        SqlEdition::Enterprise => enterprise,
    }
    .ok_or(CostError::MissingSourceLicenseRate)?;
    validate_rate(rate)?;
    Ok(rate.0)
}

fn tiered_iops_cost(iops: u64, tiers: &[IopsPriceTier]) -> Result<Decimal, CostError> {
    if iops == 0 {
        return Ok(Decimal::ZERO);
    }
    let mut previous_limit = 0_u64;
    let mut remaining = iops;
    let mut total = Decimal::ZERO;

    for tier in tiers {
        validate_rate(tier.monthly_per_iops)?;
        let units = match tier.up_to_inclusive {
            Some(limit) if limit > previous_limit => remaining.min(limit - previous_limit),
            Some(_) => return Err(CostError::InvalidIopsTiers),
            None => remaining,
        };
        total += Decimal::from(units) * tier.monthly_per_iops.0;
        remaining -= units;
        if remaining == 0 {
            return Ok(total);
        }
        match tier.up_to_inclusive {
            Some(limit) => previous_limit = limit,
            None => return Err(CostError::InvalidIopsTiers),
        }
    }

    Err(CostError::InvalidIopsTiers)
}

fn validate_rate(value: DecimalValue) -> Result<(), CostError> {
    if value.0 < Decimal::ZERO {
        Err(CostError::NegativeRate)
    } else {
        Ok(())
    }
}

fn apply_discount(gross: Decimal, discount: DecimalValue) -> Decimal {
    gross * (Decimal::ONE - discount.0)
}

#[allow(clippy::too_many_arguments)]
fn source_costs(
    compute_gross: Decimal,
    compute_net: Decimal,
    license_gross: Decimal,
    license_net: Decimal,
    storage_gross: Decimal,
    storage_net: Decimal,
    hardware_annual: Decimal,
    electricity_annual: Decimal,
) -> SourceCostBreakdown {
    SourceCostBreakdown {
        compute_gross: DecimalValue(compute_gross),
        compute_net: DecimalValue(compute_net),
        license_gross: DecimalValue(license_gross),
        license_net: DecimalValue(license_net),
        storage_gross: DecimalValue(storage_gross),
        storage_net: DecimalValue(storage_net),
        hardware_annual: DecimalValue(hardware_annual),
        electricity_annual: DecimalValue(electricity_annual),
        total: DecimalValue(compute_net + license_net + storage_net + electricity_annual),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use uuid::Uuid;

    use super::*;
    use crate::domain::resource::{
        EbsVolume, ProjectType, PurchaseOption, RdsDeployment, SharedResource, SqlWorkload,
    };

    #[test]
    fn gp3_honors_included_iops_and_throughput_per_volume() {
        let rate = EbsRate {
            volume_type: EbsVolumeType::Gp3,
            capacity_monthly_per_gb: decimal("0.08"),
            included_iops: 3_000,
            iops_monthly_per_unit: Some(decimal("0.005")),
            iops_tiers: Vec::new(),
            included_throughput_mibps: decimal("125"),
            throughput_monthly_per_mibps: Some(decimal("0.04")),
        };
        let baseline = volume(EbsVolumeType::Gp3, "100", Some(3_000), Some("125"));
        let above = volume(EbsVolumeType::Gp3, "100", Some(4_000), Some("225"));

        assert_eq!(calculate_ebs_volume_monthly(&baseline, &rate), Ok(d("8")));
        assert_eq!(calculate_ebs_volume_monthly(&above, &rate), Ok(d("17")));
    }

    #[test]
    fn io2_tier_boundaries_are_exact() {
        let rate = EbsRate {
            volume_type: EbsVolumeType::Io2,
            capacity_monthly_per_gb: DecimalValue::ZERO,
            included_iops: 0,
            iops_monthly_per_unit: None,
            iops_tiers: vec![
                IopsPriceTier {
                    up_to_inclusive: Some(32_000),
                    monthly_per_iops: decimal("0.065"),
                },
                IopsPriceTier {
                    up_to_inclusive: Some(64_000),
                    monthly_per_iops: decimal("0.046"),
                },
                IopsPriceTier {
                    up_to_inclusive: None,
                    monthly_per_iops: decimal("0.032"),
                },
            ],
            included_throughput_mibps: DecimalValue::ZERO,
            throughput_monthly_per_mibps: None,
        };

        assert_eq!(
            calculate_ebs_volume_monthly(
                &volume(EbsVolumeType::Io2, "0", Some(32_000), None),
                &rate
            ),
            Ok(d("2080"))
        );
        assert_eq!(
            calculate_ebs_volume_monthly(
                &volume(EbsVolumeType::Io2, "0", Some(32_001), None),
                &rate
            ),
            Ok(d("2080.046"))
        );
        assert_eq!(
            calculate_ebs_volume_monthly(
                &volume(EbsVolumeType::Io2, "0", Some(64_001), None),
                &rate
            ),
            Ok(d("3552.032"))
        );
        assert_eq!(
            calculate_ebs_volume_monthly(
                &volume(EbsVolumeType::Io2, "0", Some(32_000), Some("1000")),
                &rate
            ),
            Ok(d("2080"))
        );
    }

    #[test]
    fn ec2_iops_uses_maximum_persistent_volume_not_sum() {
        let mut resource = ec2_resource();
        resource.volumes = vec![
            volume(EbsVolumeType::Gp3, "100", Some(3_000), Some("125")),
            volume(EbsVolumeType::Io2, "100", Some(20_000), None),
            volume(EbsVolumeType::Ephemeral, "100", None, None),
        ];

        assert_eq!(source_max_iops(&resource), Ok(20_000));
    }

    #[test]
    fn rds_multi_az_quantity_is_not_doubled() {
        let mut resource = rds_resource();
        resource.shared.quantity = 2;
        resource.deployment = RdsDeployment::MultiAz;
        let costs = calculate_rds_source(
            &resource,
            RdsRate {
                source_vcpu: 4,
                catalog_memory_gb: decimal("16"),
                effective_compute_hourly: decimal("2"),
                storage_monthly_per_gb: decimal("0.10"),
                standard_license_core_hourly: decimal("0.12"),
                enterprise_license_core_hourly: decimal("0.375"),
            },
            &settings(ProjectType::Rds),
        )
        .expect("RDS costs");

        assert_eq!(costs.compute_gross, decimal("35040"));
    }

    #[test]
    fn on_prem_costs_use_pack_rounding_power_formula_and_no_hardware_discount() {
        let mut settings = settings(ProjectType::OnPrem);
        settings.source_compute_discount = decimal("0.75");
        settings.source_storage_discount = decimal("0.75");
        settings.source_license_discount = decimal("0.05");
        settings.standard_license_sa_usd_per_two_core_pack = Some(decimal("1000"));
        settings.remaining_coverage_months = Some(36);
        settings.electricity_rate_usd_per_kwh = Some(decimal("0.20"));
        let mut resource = on_prem_resource();
        resource.licensable_cores = 5;

        let result = calculate_on_prem_source(&resource, &settings).expect("on-prem costs");

        assert_eq!(result.explanation.license_pack_count, 3);
        assert_eq!(
            result.explanation.estimated_power_kw,
            decimal("0.1569765625")
        );
        assert_eq!(result.costs.hardware_annual, decimal("3000"));
        assert_eq!(result.costs.license_net, decimal("950"));
        assert_eq!(result.explanation.annual_kwh, decimal("1375.1146875"));
        assert_eq!(
            result.explanation.electricity_monthly_average,
            decimal("22.918578125")
        );
        assert_eq!(result.costs.electricity_annual, decimal("275.0229375"));
        assert_eq!(result.costs.total, decimal("4225.0229375"));
    }

    #[test]
    fn power_override_takes_precedence() {
        let mut configured = settings(ProjectType::OnPrem);
        configured.standard_license_sa_usd_per_two_core_pack = Some(decimal("1000"));
        configured.remaining_coverage_months = Some(12);
        configured.electricity_rate_usd_per_kwh = Some(decimal("0.20"));
        let mut resource = on_prem_resource();
        resource.average_power_kw_override = Some(decimal("0.5"));

        let result = calculate_on_prem_source(&resource, &configured).expect("on-prem costs");

        assert!(result.explanation.power_override_applied);
        assert_eq!(result.explanation.effective_power_kw, decimal("0.5"));
        assert_eq!(result.explanation.annual_kwh, decimal("4380"));
    }

    #[test]
    fn additional_ram_is_charged_once_before_compute_discount() {
        let mut configured = settings(ProjectType::Ec2);
        configured.azure_compute_discount = decimal("0.10");
        let azure = calculate_azure(
            2,
            decimal("8760"),
            decimal("100"),
            decimal("224"),
            decimal("256"),
            AzureRate {
                compute_hourly: decimal("1"),
                license_hourly: DecimalValue::ZERO,
                storage_monthly_per_gb: decimal("0.10"),
                additional_memory_per_gb_hourly: decimal("0.011663"),
            },
            &configured,
        )
        .expect("Azure costs");

        assert_eq!(azure.additional_ram_gb, decimal("32"));
        assert_eq!(azure.additional_ram_gross, decimal("6538.744320"));
        assert_eq!(azure.compute_plus_ram_net, decimal("21652.869888"));
    }

    #[test]
    fn azure_vm_costs_split_compute_and_windows_license_before_discounts() {
        let mut configured = settings(ProjectType::Ec2Vm);
        configured.azure_compute_discount = decimal("0.10");
        configured.azure_license_discount = decimal("0.20");
        configured.azure_storage_discount = decimal("0.25");

        let azure = calculate_azure_vm(
            2,
            decimal("8760"),
            decimal("0.227"),
            decimal("0.092"),
            decimal("100"),
            &configured,
        )
        .expect("Azure VM costs");

        assert_eq!(azure.compute_gross, decimal("2365.200"));
        assert_eq!(azure.compute_plus_ram_net, decimal("2128.6800"));
        assert_eq!(azure.license_gross, decimal("1611.840"));
        assert_eq!(azure.license_net, decimal("1289.4720"));
        assert_eq!(azure.storage_gross, decimal("2400"));
        assert_eq!(azure.storage_net, decimal("1800.00"));
        assert_eq!(azure.total_before_parity, decimal("5218.1520"));
    }

    #[test]
    fn azure_vm_costs_reject_license_above_total_rate() {
        let result = calculate_azure_vm(
            1,
            decimal("8760"),
            decimal("0.1"),
            decimal("0.2"),
            decimal("0"),
            &settings(ProjectType::Ec2Vm),
        );

        assert_eq!(result, Err(CostError::InvalidAzureVmRateComponents));
    }

    #[test]
    fn azure_mi_storage_matches_calculator_units_and_included_capacity() {
        let configured_storage_gb = azure_mi_configured_storage_gb(decimal("7221"));

        assert_eq!(configured_storage_gb, decimal("7232"));
        assert_eq!(
            azure_mi_billable_storage_gb(configured_storage_gb),
            decimal("7200")
        );
        assert_eq!(
            azure_mi_billable_storage_gb(azure_mi_configured_storage_gb(DecimalValue::ZERO)),
            DecimalValue::ZERO
        );

        let azure = calculate_azure(
            1,
            decimal("8760"),
            configured_storage_gb,
            decimal("224"),
            decimal("256"),
            AzureRate {
                compute_hourly: decimal("5.632"),
                license_hourly: decimal("3.198912"),
                storage_monthly_per_gb: decimal("0.13685"),
                additional_memory_per_gb_hourly: decimal("0.011663"),
            },
            &settings(ProjectType::Ec2),
        )
        .expect("Azure costs");

        assert_eq!(azure.storage_gross, decimal("11823.84"));
        assert_eq!(azure.total_before_parity, decimal("92452.00128"));
    }

    #[test]
    fn required_adjustment_reaches_parity_without_rounding() {
        let source = source_costs(
            d("100"),
            d("100"),
            d("0"),
            d("0"),
            d("0"),
            d("0"),
            d("0"),
            d("0"),
        );
        let azure = AzureCostBreakdown {
            compute_gross: decimal("125"),
            additional_ram_gb: DecimalValue::ZERO,
            additional_ram_gross: DecimalValue::ZERO,
            compute_plus_ram_net: decimal("125"),
            license_gross: DecimalValue::ZERO,
            license_net: DecimalValue::ZERO,
            storage_gross: DecimalValue::ZERO,
            storage_net: DecimalValue::ZERO,
            total_before_parity: decimal("125"),
        };

        let savings = calculate_savings(&source, &azure, decimal("0.2"));

        assert_eq!(savings.required_adjustment, decimal("0.2"));
        assert_eq!(savings.azure_after_selected_parity, decimal("100.0"));
        assert_eq!(savings.difference, DecimalValue::ZERO);
    }

    fn ec2_resource() -> Ec2Resource {
        Ec2Resource {
            shared: shared(),
            sql: sql_workload(),
            instance_type: "r6id.8xlarge".to_owned(),
            volumes: Vec::new(),
        }
    }

    fn rds_resource() -> RdsResource {
        RdsResource {
            shared: shared(),
            sql: sql_workload(),
            instance_type: "db.r6i.xlarge".to_owned(),
            deployment: RdsDeployment::SingleAz,
            commercial_term: "on_demand".to_owned(),
            storage_class: "gp3".to_owned(),
            source_max_iops: 0,
        }
    }

    fn on_prem_resource() -> OnPremResource {
        OnPremResource {
            shared: shared(),
            sql: sql_workload(),
            source_vcpu: 4,
            licensable_cores: 4,
            source_max_iops: 0,
            hardware_capex_usd: decimal("9000"),
            depreciation_years: decimal("3"),
            average_power_kw_override: None,
        }
    }

    fn shared() -> SharedResource {
        SharedResource {
            id: Uuid::new_v4(),
            workload_name: "Synthetic workload".to_owned(),
            server_name: None,
            quantity: 1,
            source_ram_gb_per_instance: decimal("16"),
            annual_hours_per_instance: decimal("8760"),
        }
    }

    fn sql_workload() -> SqlWorkload {
        SqlWorkload {
            sql_edition: SqlEdition::Standard,
            license_basis: LicenseBasis::LicenseIncluded,
            sql_data_gb_per_instance: decimal("100"),
            mi_purchase_option: PurchaseOption::Ahb,
        }
    }

    fn volume(
        volume_type: EbsVolumeType,
        capacity_gb: &str,
        provisioned_iops: Option<u64>,
        throughput_mibps: Option<&str>,
    ) -> EbsVolume {
        EbsVolume {
            id: Uuid::new_v4(),
            label: "D".to_owned(),
            aws_volume_id: None,
            volume_type,
            capacity_gb: decimal(capacity_gb),
            provisioned_iops,
            throughput_mibps: throughput_mibps.map(decimal),
        }
    }

    fn settings(project_type: ProjectType) -> ProjectSettings {
        ProjectSettings {
            project_type,
            aws_region: (project_type != ProjectType::OnPrem).then(|| "eu-west-1".to_owned()),
            azure_region: "swedencentral".to_owned(),
            currency: "USD".to_owned(),
            source_compute_discount: decimal("0"),
            source_license_discount: decimal("0"),
            source_storage_discount: decimal("0"),
            azure_compute_discount: decimal("0"),
            azure_license_discount: decimal("0"),
            azure_storage_discount: decimal("0"),
            selected_parity_adjustment: decimal("0"),
            default_annual_hours: decimal("8760"),
            default_mi_purchase_option: PurchaseOption::Ahb,
            enterprise_license_sa_usd_per_two_core_pack: None,
            standard_license_sa_usd_per_two_core_pack: None,
            remaining_coverage_months: None,
            electricity_rate_usd_per_kwh: None,
            sql_payg: None,
        }
    }

    fn decimal(value: &str) -> DecimalValue {
        DecimalValue(d(value))
    }

    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).expect("valid decimal")
    }
}
