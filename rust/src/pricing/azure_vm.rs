use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    calculation::vm_target_selector::{ManagedDiskCatalog, VmCapabilityCatalog},
    domain::{decimal::DecimalValue, resource::VmPurchaseOption},
    pricing::snapshot::{
        AzureManagedDiskPriceDimension, AzureManagedDiskRateRecord, AzureVmRateRecord,
        RateProvenance,
    },
};

const MONTHLY_BILLING_HOURS: u32 = 730;
const PREMIUM_SSD_OFFER: &str = "premium_ssd_lrs";
const PREMIUM_SSD_V2_OFFER: &str = "premium_ssd_v2_lrs";

#[derive(Clone, Copy)]
pub struct AzureVmPricingContext<'a> {
    pub target_region: &'a str,
    pub currency: &'a str,
}

#[derive(Clone, Copy)]
pub struct AzureVmRetailPagePayload<'a> {
    pub source_url: &'a str,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct AzureVmPricingNormalization {
    pub vm_records: Vec<AzureVmRateRecord>,
    pub managed_disk_records: Vec<AzureManagedDiskRateRecord>,
    pub source_urls: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AzureVmPricingNormalizationError {
    #[error("Azure VM pricing payload JSON is malformed")]
    MalformedJson,
    #[error("Azure VM pricing scope is invalid")]
    InvalidScope,
    #[error("Azure VM or managed-disk Retail Price value is invalid")]
    InvalidValue,
    #[error("Azure VM Retail Prices returned conflicting current meters")]
    ConflictingVmRate,
    #[error("a required Azure managed-disk price dimension is missing")]
    MissingManagedDiskRate,
    #[error("Azure managed-disk Retail Prices returned conflicting current meters")]
    ConflictingManagedDiskRate,
    #[error("no reviewed Azure VM has a complete Windows consumption price")]
    MissingVmRates,
}

#[derive(Deserialize)]
struct RetailPage {
    #[serde(rename = "Items")]
    items: Vec<RetailItem>,
}

#[derive(Deserialize)]
struct RetailItem {
    #[serde(rename = "serviceName")]
    service_name: String,
    #[serde(rename = "armRegionName")]
    arm_region_name: String,
    #[serde(rename = "currencyCode")]
    currency_code: String,
    #[serde(rename = "armSkuName")]
    arm_sku_name: String,
    #[serde(rename = "productName")]
    product_name: String,
    #[serde(rename = "skuName")]
    sku_name: String,
    #[serde(rename = "meterName")]
    meter_name: String,
    #[serde(rename = "unitOfMeasure")]
    unit_of_measure: String,
    #[serde(rename = "type")]
    price_type: String,
    #[serde(rename = "retailPrice")]
    retail_price: serde_json::Value,
    #[serde(rename = "tierMinimumUnits")]
    tier_minimum_units: serde_json::Value,
    #[serde(rename = "effectiveStartDate")]
    effective_start_date: String,
    #[serde(rename = "meterId")]
    meter_id: String,
    #[serde(rename = "isPrimaryMeterRegion")]
    is_primary_meter_region: bool,
    #[serde(rename = "reservationTerm")]
    reservation_term: Option<String>,
    #[serde(default, rename = "savingsPlan")]
    savings_plan: Vec<SavingsPlanPrice>,
}

#[derive(Deserialize)]
struct SavingsPlanPrice {
    #[serde(rename = "retailPrice")]
    retail_price: serde_json::Value,
    term: String,
}

#[derive(Clone)]
struct RetailRate {
    rate: Decimal,
    raw_price_lexeme: String,
    unit_of_measure: String,
    effective_start_date: String,
    source_url: String,
    meter_ids: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum VmCommitment {
    ReservationOneYear,
    ReservationThreeYear,
    SavingsOneYear,
    SavingsThreeYear,
}

pub fn normalize_azure_vm_pricing(
    context: AzureVmPricingContext<'_>,
    vm_catalog: &VmCapabilityCatalog,
    disk_catalog: &ManagedDiskCatalog,
    pages: &[AzureVmRetailPagePayload<'_>],
) -> Result<AzureVmPricingNormalization, AzureVmPricingNormalizationError> {
    if context.target_region.is_empty() || context.currency != "USD" || pages.is_empty() {
        return Err(AzureVmPricingNormalizationError::InvalidScope);
    }
    let expected_vms = vm_catalog
        .candidates
        .iter()
        .filter(|candidate| {
            candidate
                .azure_region
                .eq_ignore_ascii_case(context.target_region)
        })
        .map(|candidate| {
            (
                candidate.arm_sku_name.to_ascii_lowercase(),
                candidate.arm_sku_name.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if expected_vms.is_empty()
        || !disk_catalog.offers.iter().any(|offer| {
            offer.offer_key == PREMIUM_SSD_OFFER
                && offer
                    .azure_regions
                    .iter()
                    .any(|region| region.eq_ignore_ascii_case(context.target_region))
        })
        || !disk_catalog.offers.iter().any(|offer| {
            offer.offer_key == PREMIUM_SSD_V2_OFFER
                && offer
                    .azure_regions
                    .iter()
                    .any(|region| region.eq_ignore_ascii_case(context.target_region))
        })
    {
        return Err(AzureVmPricingNormalizationError::InvalidScope);
    }

    let expected_tiers = disk_catalog
        .offers
        .iter()
        .find(|offer| offer.offer_key == PREMIUM_SSD_OFFER)
        .ok_or(AzureVmPricingNormalizationError::InvalidScope)?
        .tiers
        .iter()
        .map(|tier| tier.tier_key.as_str())
        .collect::<BTreeSet<_>>();

    let mut linux_vm_rates = BTreeMap::<String, RetailRate>::new();
    let mut windows_vm_rates = BTreeMap::<String, RetailRate>::new();
    let mut commitment_vm_rates = BTreeMap::<(String, VmCommitment), RetailRate>::new();
    let mut tier_rates = BTreeMap::<String, RetailRate>::new();
    let mut v2_rates = BTreeMap::<(AzureManagedDiskPriceDimension, Decimal), RetailRate>::new();
    let mut source_urls = BTreeSet::new();

    for page in pages {
        if page.source_url.is_empty() {
            return Err(AzureVmPricingNormalizationError::InvalidValue);
        }
        let source_url = page.source_url;
        source_urls.insert(source_url.to_owned());
        let retail_page: RetailPage = serde_json::from_slice(page.body)
            .map_err(|_| AzureVmPricingNormalizationError::MalformedJson)?;
        for item in retail_page.items {
            if item.arm_region_name != context.target_region
                || item.currency_code != context.currency
                || !item.is_primary_meter_region
            {
                continue;
            }
            if is_windows_vm_rate(&item, &expected_vms) {
                let key = item.arm_sku_name.to_ascii_lowercase();
                let rate = retail_rate(&item, source_url)?;
                merge_latest_rate(
                    &mut windows_vm_rates,
                    key,
                    rate,
                    AzureVmPricingNormalizationError::ConflictingVmRate,
                )?;
            } else if is_linux_vm_rate(&item, &expected_vms) {
                let key = item.arm_sku_name.to_ascii_lowercase();
                for plan in &item.savings_plan {
                    let Some(commitment) = savings_plan_commitment(&plan.term) else {
                        continue;
                    };
                    let rate = savings_plan_rate(&item, plan, source_url)?;
                    merge_latest_rate(
                        &mut commitment_vm_rates,
                        (key.clone(), commitment),
                        rate,
                        AzureVmPricingNormalizationError::ConflictingVmRate,
                    )?;
                }
                let rate = retail_rate(&item, source_url)?;
                merge_latest_rate(
                    &mut linux_vm_rates,
                    key,
                    rate,
                    AzureVmPricingNormalizationError::ConflictingVmRate,
                )?;
            } else if let Some(commitment) = reservation_commitment(&item, &expected_vms) {
                let key = item.arm_sku_name.to_ascii_lowercase();
                let rate = reservation_hourly_rate(&item, commitment, source_url)?;
                merge_latest_rate(
                    &mut commitment_vm_rates,
                    (key, commitment),
                    rate,
                    AzureVmPricingNormalizationError::ConflictingVmRate,
                )?;
            } else if item.price_type == "Consumption"
                && let Some(tier_key) = premium_ssd_tier(&item, &expected_tiers)
            {
                let rate = retail_rate(&item, source_url)?;
                merge_latest_rate(
                    &mut tier_rates,
                    tier_key.to_owned(),
                    rate,
                    AzureVmPricingNormalizationError::ConflictingManagedDiskRate,
                )?;
            } else if item.price_type == "Consumption"
                && let Some(dimension) = premium_ssd_v2_dimension(&item)
            {
                let tier_minimum = parse_nonnegative_decimal(&item.tier_minimum_units)?.0;
                let rate = retail_rate_allow_zero(&item, source_url)?;
                merge_latest_rate(
                    &mut v2_rates,
                    (dimension, tier_minimum),
                    rate,
                    AzureVmPricingNormalizationError::ConflictingManagedDiskRate,
                )?;
            }
        }
    }

    let mut vm_records = Vec::new();
    let mut warnings = Vec::new();
    for (key, sku) in &expected_vms {
        let (Some(linux_payg), Some(windows_payg)) =
            (linux_vm_rates.get(key), windows_vm_rates.get(key))
        else {
            warnings.push(format!(
                "Azure VM {sku} has no complete Linux and Windows pay-as-you-go rate pair."
            ));
            continue;
        };
        let license_hourly = windows_payg.rate - linux_payg.rate;
        if license_hourly <= Decimal::ZERO {
            return Err(AzureVmPricingNormalizationError::InvalidValue);
        }
        push_vm_option_pair(
            &mut vm_records,
            context.target_region,
            sku,
            key,
            VmPurchaseOption::Payg,
            VmPurchaseOption::Ahb,
            linux_payg,
            windows_payg,
            license_hourly,
        )?;

        for (commitment, licensed_option, ahb_option, label) in [
            (
                VmCommitment::ReservationOneYear,
                VmPurchaseOption::OneYear,
                VmPurchaseOption::AhbOneYear,
                "1-year Reservation",
            ),
            (
                VmCommitment::ReservationThreeYear,
                VmPurchaseOption::ThreeYear,
                VmPurchaseOption::AhbThreeYear,
                "3-year Reservation",
            ),
            (
                VmCommitment::SavingsOneYear,
                VmPurchaseOption::SavingsOneYear,
                VmPurchaseOption::AhbSavingsOneYear,
                "1-year Savings Plan",
            ),
            (
                VmCommitment::SavingsThreeYear,
                VmPurchaseOption::SavingsThreeYear,
                VmPurchaseOption::AhbSavingsThreeYear,
                "3-year Savings Plan",
            ),
        ] {
            let Some(compute_rate) = commitment_vm_rates.get(&(key.clone(), commitment)) else {
                warnings.push(format!(
                    "Azure VM {sku} has no exact {label} rate; that purchase option is unavailable."
                ));
                continue;
            };
            if compute_rate.rate >= linux_payg.rate {
                return Err(AzureVmPricingNormalizationError::InvalidValue);
            }
            push_vm_option_pair(
                &mut vm_records,
                context.target_region,
                sku,
                key,
                licensed_option,
                ahb_option,
                compute_rate,
                windows_payg,
                license_hourly,
            )?;
        }
    }
    if vm_records.is_empty() {
        return Err(AzureVmPricingNormalizationError::MissingVmRates);
    }
    vm_records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    warnings.sort();

    let mut managed_disk_records = Vec::new();
    for tier_key in expected_tiers {
        let rate = tier_rates
            .remove(tier_key)
            .ok_or(AzureVmPricingNormalizationError::MissingManagedDiskRate)?;
        let provenance = provenance(&rate);
        managed_disk_records.push(AzureManagedDiskRateRecord {
            stable_key: format!(
                "{}|{}|{}|capacity_tier",
                context.target_region,
                PREMIUM_SSD_OFFER,
                tier_key.to_ascii_lowercase()
            ),
            offer_key: PREMIUM_SSD_OFFER.to_owned(),
            tier_key: Some(tier_key.to_owned()),
            dimension: AzureManagedDiskPriceDimension::CapacityTier,
            normalized_monthly_rate: DecimalValue(rate.rate),
            unit_of_measure: rate.unit_of_measure,
            raw_price_lexeme: rate.raw_price_lexeme,
            provenance,
        });
    }

    managed_disk_records.push(v2_record(
        context.target_region,
        AzureManagedDiskPriceDimension::CapacityGb,
        decimal(0),
        &mut v2_rates,
    )?);
    managed_disk_records.push(v2_record(
        context.target_region,
        AzureManagedDiskPriceDimension::AdditionalIops,
        decimal(3_000),
        &mut v2_rates,
    )?);
    managed_disk_records.push(v2_record(
        context.target_region,
        AzureManagedDiskPriceDimension::AdditionalThroughput,
        decimal(125),
        &mut v2_rates,
    )?);

    managed_disk_records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(AzureVmPricingNormalization {
        vm_records,
        managed_disk_records,
        source_urls: source_urls.into_iter().collect(),
        warnings,
    })
}

fn is_windows_vm_rate(item: &RetailItem, expected: &BTreeMap<String, &str>) -> bool {
    item.service_name == "Virtual Machines"
        && item.price_type == "Consumption"
        && item.product_name.starts_with("Virtual Machines ")
        && item.product_name.ends_with(" Windows")
        && item.unit_of_measure == "1 Hour"
        && expected.contains_key(&item.arm_sku_name.to_ascii_lowercase())
        && !is_discounted_vm_meter(&item.sku_name)
        && !is_discounted_vm_meter(&item.meter_name)
}

fn is_linux_vm_rate(item: &RetailItem, expected: &BTreeMap<String, &str>) -> bool {
    item.service_name == "Virtual Machines"
        && item.price_type == "Consumption"
        && item.product_name.starts_with("Virtual Machines ")
        && !item.product_name.ends_with(" Windows")
        && item.unit_of_measure == "1 Hour"
        && expected.contains_key(&item.arm_sku_name.to_ascii_lowercase())
        && !is_discounted_vm_meter(&item.sku_name)
        && !is_discounted_vm_meter(&item.meter_name)
}

fn reservation_commitment(
    item: &RetailItem,
    expected: &BTreeMap<String, &str>,
) -> Option<VmCommitment> {
    if item.service_name != "Virtual Machines"
        || item.price_type != "Reservation"
        || item.product_name.ends_with(" Windows")
        || item.unit_of_measure != "1 Hour"
        || !expected.contains_key(&item.arm_sku_name.to_ascii_lowercase())
        || is_discounted_vm_meter(&item.sku_name)
        || is_discounted_vm_meter(&item.meter_name)
    {
        return None;
    }
    match item.reservation_term.as_deref()? {
        "1 Year" => Some(VmCommitment::ReservationOneYear),
        "3 Years" => Some(VmCommitment::ReservationThreeYear),
        _ => None,
    }
}

fn savings_plan_commitment(term: &str) -> Option<VmCommitment> {
    match term {
        "1 Year" => Some(VmCommitment::SavingsOneYear),
        "3 Years" => Some(VmCommitment::SavingsThreeYear),
        _ => None,
    }
}

fn savings_plan_rate(
    item: &RetailItem,
    plan: &SavingsPlanPrice,
    source_url: &str,
) -> Result<RetailRate, AzureVmPricingNormalizationError> {
    let (rate, raw_price_lexeme) = parse_nonnegative_decimal(&plan.retail_price)?;
    if rate <= Decimal::ZERO || item.effective_start_date.is_empty() || item.meter_id.is_empty() {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    Ok(RetailRate {
        rate,
        raw_price_lexeme,
        unit_of_measure: item.unit_of_measure.clone(),
        effective_start_date: item.effective_start_date.clone(),
        source_url: source_url.to_owned(),
        meter_ids: BTreeSet::from([item.meter_id.clone()]),
    })
}

fn reservation_hourly_rate(
    item: &RetailItem,
    commitment: VmCommitment,
    source_url: &str,
) -> Result<RetailRate, AzureVmPricingNormalizationError> {
    let mut rate = retail_rate(item, source_url)?;
    let term_hours = match commitment {
        VmCommitment::ReservationOneYear => 8_760,
        VmCommitment::ReservationThreeYear => 26_280,
        VmCommitment::SavingsOneYear | VmCommitment::SavingsThreeYear => {
            return Err(AzureVmPricingNormalizationError::InvalidValue);
        }
    };
    rate.rate /= Decimal::from(term_hours);
    rate.raw_price_lexeme = rate.rate.normalize().to_string();
    Ok(rate)
}

#[allow(clippy::too_many_arguments)]
fn push_vm_option_pair(
    records: &mut Vec<AzureVmRateRecord>,
    target_region: &str,
    arm_sku_name: &str,
    normalized_sku: &str,
    licensed_option: VmPurchaseOption,
    ahb_option: VmPurchaseOption,
    compute_rate: &RetailRate,
    windows_payg: &RetailRate,
    license_hourly: Decimal,
) -> Result<(), AzureVmPricingNormalizationError> {
    if compute_rate.unit_of_measure != "1 Hour"
        || windows_payg.unit_of_measure != "1 Hour"
        || compute_rate.rate <= Decimal::ZERO
        || license_hourly <= Decimal::ZERO
    {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    records.push(vm_record(
        target_region,
        arm_sku_name,
        normalized_sku,
        licensed_option,
        compute_rate,
        windows_payg,
        license_hourly,
    ));
    records.push(vm_record(
        target_region,
        arm_sku_name,
        normalized_sku,
        ahb_option,
        compute_rate,
        windows_payg,
        Decimal::ZERO,
    ));
    Ok(())
}

fn vm_record(
    target_region: &str,
    arm_sku_name: &str,
    normalized_sku: &str,
    purchase_option: VmPurchaseOption,
    compute_rate: &RetailRate,
    windows_payg: &RetailRate,
    license_hourly: Decimal,
) -> AzureVmRateRecord {
    let hourly_rate = compute_rate.rate + license_hourly;
    let mut meter_ids = compute_rate.meter_ids.clone();
    meter_ids.extend(windows_payg.meter_ids.iter().cloned());
    AzureVmRateRecord {
        stable_key: format!(
            "{target_region}|{normalized_sku}|{}",
            vm_purchase_option_key(purchase_option)
        ),
        arm_sku_name: arm_sku_name.to_owned(),
        purchase_option,
        hourly_rate: DecimalValue(hourly_rate),
        license_hourly: DecimalValue(license_hourly),
        unit_of_measure: compute_rate.unit_of_measure.clone(),
        raw_price_lexeme: hourly_rate.normalize().to_string(),
        provenance: RateProvenance {
            source_url: compute_rate.source_url.clone(),
            effective_at: Some(
                compute_rate
                    .effective_start_date
                    .clone()
                    .max(windows_payg.effective_start_date.clone()),
            ),
            source_version: None,
            meter_ids: meter_ids.into_iter().collect(),
        },
    }
}

fn vm_purchase_option_key(option: VmPurchaseOption) -> &'static str {
    match option {
        VmPurchaseOption::Payg => "payg",
        VmPurchaseOption::Ahb => "ahb",
        VmPurchaseOption::OneYear => "one-year",
        VmPurchaseOption::AhbOneYear => "ahbone-year",
        VmPurchaseOption::ThreeYear => "three-year",
        VmPurchaseOption::AhbThreeYear => "ahbthree-year",
        VmPurchaseOption::SavingsOneYear => "sv-one-year",
        VmPurchaseOption::AhbSavingsOneYear => "ahbsv-one-year",
        VmPurchaseOption::SavingsThreeYear => "sv-three-year",
        VmPurchaseOption::AhbSavingsThreeYear => "ahbsv-three-year",
    }
}

fn is_discounted_vm_meter(value: &str) -> bool {
    value.contains("Spot") || value.contains("Low Priority")
}

fn premium_ssd_tier<'a>(item: &'a RetailItem, expected_tiers: &BTreeSet<&str>) -> Option<&'a str> {
    if item.service_name != "Storage"
        || item.product_name != "Premium SSD Managed Disks"
        || item.unit_of_measure != "1/Month"
        || !item.sku_name.ends_with(" LRS")
        || item.meter_name != format!("{} Disk", item.sku_name)
    {
        return None;
    }
    let tier = item.sku_name.strip_suffix(" LRS")?;
    expected_tiers.contains(tier).then_some(tier)
}

fn premium_ssd_v2_dimension(item: &RetailItem) -> Option<AzureManagedDiskPriceDimension> {
    if item.service_name != "Storage"
        || item.product_name != "Azure Premium SSD v2"
        || item.sku_name != "Premium LRS"
    {
        return None;
    }
    match (item.meter_name.as_str(), item.unit_of_measure.as_str()) {
        ("Premium LRS Provisioned Capacity", "1 GiB/Hour") => {
            Some(AzureManagedDiskPriceDimension::CapacityGb)
        }
        ("Premium LRS Provisioned IOPS", "1/Hour") => {
            Some(AzureManagedDiskPriceDimension::AdditionalIops)
        }
        ("Premium LRS Provisioned Throughput (MBps)", "1/Hour") => {
            Some(AzureManagedDiskPriceDimension::AdditionalThroughput)
        }
        _ => None,
    }
}

fn retail_rate(
    item: &RetailItem,
    source_url: &str,
) -> Result<RetailRate, AzureVmPricingNormalizationError> {
    let rate = retail_rate_allow_zero(item, source_url)?;
    if rate.rate <= Decimal::ZERO {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    Ok(rate)
}

fn retail_rate_allow_zero(
    item: &RetailItem,
    source_url: &str,
) -> Result<RetailRate, AzureVmPricingNormalizationError> {
    let (rate, raw_price_lexeme) = parse_nonnegative_decimal(&item.retail_price)?;
    if item.effective_start_date.is_empty() || item.meter_id.is_empty() {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    Ok(RetailRate {
        rate,
        raw_price_lexeme,
        unit_of_measure: item.unit_of_measure.clone(),
        effective_start_date: item.effective_start_date.clone(),
        source_url: source_url.to_owned(),
        meter_ids: BTreeSet::from([item.meter_id.clone()]),
    })
}

fn merge_latest_rate<K: Ord>(
    rates: &mut BTreeMap<K, RetailRate>,
    key: K,
    incoming: RetailRate,
    conflict: AzureVmPricingNormalizationError,
) -> Result<(), AzureVmPricingNormalizationError> {
    match rates.get_mut(&key) {
        Some(existing) if incoming.effective_start_date > existing.effective_start_date => {
            *existing = incoming;
        }
        Some(existing) if incoming.effective_start_date == existing.effective_start_date => {
            if incoming.rate != existing.rate
                || incoming.unit_of_measure != existing.unit_of_measure
            {
                return Err(conflict);
            }
            existing.meter_ids.extend(incoming.meter_ids);
        }
        Some(_) => {}
        None => {
            rates.insert(key, incoming);
        }
    }
    Ok(())
}

fn v2_record(
    target_region: &str,
    dimension: AzureManagedDiskPriceDimension,
    paid_tier_minimum: Decimal,
    rates: &mut BTreeMap<(AzureManagedDiskPriceDimension, Decimal), RetailRate>,
) -> Result<AzureManagedDiskRateRecord, AzureVmPricingNormalizationError> {
    if paid_tier_minimum > Decimal::ZERO {
        let included = rates
            .get(&(dimension, Decimal::ZERO))
            .ok_or(AzureVmPricingNormalizationError::MissingManagedDiskRate)?;
        if included.rate != Decimal::ZERO {
            return Err(AzureVmPricingNormalizationError::InvalidValue);
        }
    }
    let rate = rates
        .remove(&(dimension, paid_tier_minimum))
        .ok_or(AzureVmPricingNormalizationError::MissingManagedDiskRate)?;
    if rate.rate <= Decimal::ZERO {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    let dimension_key = match dimension {
        AzureManagedDiskPriceDimension::CapacityGb => "capacity_gb",
        AzureManagedDiskPriceDimension::AdditionalIops => "additional_iops",
        AzureManagedDiskPriceDimension::AdditionalThroughput => "additional_throughput",
        AzureManagedDiskPriceDimension::CapacityTier => {
            return Err(AzureVmPricingNormalizationError::InvalidValue);
        }
    };
    let provenance = provenance(&rate);
    Ok(AzureManagedDiskRateRecord {
        stable_key: format!("{target_region}|{PREMIUM_SSD_V2_OFFER}|{dimension_key}"),
        offer_key: PREMIUM_SSD_V2_OFFER.to_owned(),
        tier_key: None,
        dimension,
        normalized_monthly_rate: DecimalValue(rate.rate * Decimal::from(MONTHLY_BILLING_HOURS)),
        unit_of_measure: rate.unit_of_measure,
        raw_price_lexeme: rate.raw_price_lexeme,
        provenance,
    })
}

fn provenance(rate: &RetailRate) -> RateProvenance {
    RateProvenance {
        source_url: rate.source_url.clone(),
        effective_at: Some(rate.effective_start_date.clone()),
        source_version: None,
        meter_ids: rate.meter_ids.iter().cloned().collect(),
    }
}

fn parse_nonnegative_decimal(
    value: &serde_json::Value,
) -> Result<(Decimal, String), AzureVmPricingNormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let parsed =
        Decimal::from_str(&raw).map_err(|_| AzureVmPricingNormalizationError::InvalidValue)?;
    if parsed < Decimal::ZERO {
        return Err(AzureVmPricingNormalizationError::InvalidValue);
    }
    Ok((parsed, raw))
}

fn decimal(value: u32) -> Decimal {
    Decimal::from(value)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    #[test]
    fn normalizes_only_exact_windows_vm_and_managed_disk_meters() {
        let payload = fixture_payload(false, true);
        let normalized = normalize_azure_vm_pricing(
            context(),
            &vm_catalog(),
            &disk_catalog(),
            &[AzureVmRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices?fixture=1",
                body: &payload,
            }],
        )
        .expect("frozen Azure VM prices normalize");

        assert_eq!(normalized.vm_records.len(), VmPurchaseOption::ALL.len());
        let payg = option_record(&normalized, VmPurchaseOption::Payg);
        assert_eq!(payg.arm_sku_name, "Standard_D2s_v7");
        assert_eq!(payg.hourly_rate.to_string(), "0.227");
        assert_eq!(payg.license_hourly.to_string(), "0.092");
        assert_eq!(payg.raw_price_lexeme, "0.227");
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::Ahb)
                .hourly_rate
                .to_string(),
            "0.135"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::OneYear)
                .hourly_rate
                .to_string(),
            "0.172"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::AhbOneYear)
                .hourly_rate
                .to_string(),
            "0.080"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::ThreeYear)
                .hourly_rate
                .to_string(),
            "0.152"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::AhbThreeYear)
                .hourly_rate
                .to_string(),
            "0.060"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::SavingsOneYear)
                .hourly_rate
                .to_string(),
            "0.1876205"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::AhbSavingsOneYear)
                .hourly_rate
                .to_string(),
            "0.0956205"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::SavingsThreeYear)
                .hourly_rate
                .to_string(),
            "0.1563545"
        );
        assert_eq!(
            option_record(&normalized, VmPurchaseOption::AhbSavingsThreeYear)
                .hourly_rate
                .to_string(),
            "0.0643545"
        );
        assert_eq!(normalized.managed_disk_records.len(), 17);
        let p30 = normalized
            .managed_disk_records
            .iter()
            .find(|record| record.tier_key.as_deref() == Some("P30"))
            .expect("P30 rate");
        assert_eq!(p30.normalized_monthly_rate.to_string(), "148.68");
        let capacity = normalized
            .managed_disk_records
            .iter()
            .find(|record| record.dimension == AzureManagedDiskPriceDimension::CapacityGb)
            .expect("Premium SSD v2 capacity rate");
        assert_eq!(capacity.normalized_monthly_rate.to_string(), "0.080300");
        let iops = normalized
            .managed_disk_records
            .iter()
            .find(|record| record.dimension == AzureManagedDiskPriceDimension::AdditionalIops)
            .expect("Premium SSD v2 IOPS rate");
        assert_eq!(iops.normalized_monthly_rate.to_string(), "0.005110");
        let throughput = normalized
            .managed_disk_records
            .iter()
            .find(|record| record.dimension == AzureManagedDiskPriceDimension::AdditionalThroughput)
            .expect("Premium SSD v2 throughput rate");
        assert_eq!(throughput.normalized_monthly_rate.to_string(), "0.040150");
    }

    #[test]
    fn rejects_conflicting_current_windows_vm_meters() {
        let payload = fixture_payload(true, true);
        let error = normalize_azure_vm_pricing(
            context(),
            &vm_catalog(),
            &disk_catalog(),
            &[AzureVmRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices?fixture=1",
                body: &payload,
            }],
        )
        .expect_err("conflicting meters are refused");
        assert_eq!(error, AzureVmPricingNormalizationError::ConflictingVmRate);
    }

    #[test]
    fn leaves_missing_exact_reservation_terms_unavailable() {
        let mut fixture: Value =
            serde_json::from_slice(&fixture_payload(false, true)).expect("fixture value");
        fixture["Items"]
            .as_array_mut()
            .expect("fixture items")
            .retain(|item| item["type"] != "Reservation");
        let payload = serde_json::to_vec(&fixture).expect("fixture JSON");

        let normalized = normalize_azure_vm_pricing(
            context(),
            &vm_catalog(),
            &disk_catalog(),
            &[AzureVmRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices?fixture=1",
                body: &payload,
            }],
        )
        .expect("partial commitment availability normalizes");

        assert_eq!(normalized.vm_records.len(), 6);
        assert!(
            normalized
                .vm_records
                .iter()
                .all(|record| !matches!(
                    record.purchase_option,
                    VmPurchaseOption::OneYear
                        | VmPurchaseOption::AhbOneYear
                        | VmPurchaseOption::ThreeYear
                        | VmPurchaseOption::AhbThreeYear
                ))
        );
        assert_eq!(normalized.warnings.len(), 2);
        assert!(
            normalized
                .warnings
                .iter()
                .any(|warning| warning.contains("1-year Reservation"))
        );
        assert!(
            normalized
                .warnings
                .iter()
                .any(|warning| warning.contains("3-year Reservation"))
        );
    }

    #[test]
    fn requires_every_managed_disk_price_dimension() {
        let payload = fixture_payload(false, false);
        let error = normalize_azure_vm_pricing(
            context(),
            &vm_catalog(),
            &disk_catalog(),
            &[AzureVmRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices?fixture=1",
                body: &payload,
            }],
        )
        .expect_err("missing throughput is refused");
        assert_eq!(
            error,
            AzureVmPricingNormalizationError::MissingManagedDiskRate
        );
    }

    fn context() -> AzureVmPricingContext<'static> {
        AzureVmPricingContext {
            target_region: "swedencentral",
            currency: "USD",
        }
    }

    fn vm_catalog() -> VmCapabilityCatalog {
        serde_json::from_value(json!({
            "schema_version": "1",
            "candidates": [{
                "arm_sku_name": "Standard_D2s_v7",
                "display_family": "Dsv7",
                "lineage": "general_purpose",
                "generation": "v7",
                "generation_rank": 7,
                "lifecycle": "current",
                "azure_region": "swedencentral",
                "cpu_architecture": "x64",
                "windows_eligible": true,
                "vcpus": 2,
                "memory_gb": "8",
                "max_data_disk_count": 4,
                "premium_io": true,
                "uncached_disk_iops": 10000,
                "uncached_disk_throughput_mbps": 200,
                "local_temp_disk_gb": null,
                "source_url": "https://learn.microsoft.com/",
                "documentation_url": "https://learn.microsoft.com/",
                "reviewed_date": "2026-08-24"
            }]
        }))
        .expect("VM catalog")
    }

    fn disk_catalog() -> ManagedDiskCatalog {
        let mut catalog: Value = serde_json::from_str(include_str!(
            "../../../app/catalogs/azure-managed-disk-capabilities.json"
        ))
        .expect("managed-disk catalog JSON");
        catalog
            .as_object_mut()
            .expect("catalog object")
            .retain(|key, _| matches!(key.as_str(), "schema_version" | "offers"));
        serde_json::from_value(catalog).expect("managed-disk catalog")
    }

    fn fixture_payload(conflicting_vm: bool, include_throughput: bool) -> Vec<u8> {
        let mut linux_payg = vm_item(
            "0.135",
            "Virtual Machines Dsv7-series Linux",
            "Standard_D2s_v7",
        );
        linux_payg["savingsPlan"] = json!([
            {
                "unitPrice": "0.0643545",
                "retailPrice": "0.0643545",
                "term": "3 Years"
            },
            {
                "unitPrice": "0.0956205",
                "retailPrice": "0.0956205",
                "term": "1 Year"
            }
        ]);
        let mut items = vec![
            vm_item(
                "0.227",
                "Virtual Machines Dsv7-series Windows",
                "Standard_D2s_v7",
            ),
            linux_payg,
            reservation_item("1 Year", "700.8"),
            reservation_item("3 Years", "1576.8"),
            vm_item(
                "0.04195",
                "Virtual Machines Dsv7-series Windows",
                "Standard_D2s_v7 Spot",
            ),
        ];
        if conflicting_vm {
            items.push(vm_item(
                "0.228",
                "Virtual Machines Dsv7-series Windows",
                "Standard_D2s_v7",
            ));
        }
        for (tier, price) in [
            ("P1", "0.78"),
            ("P2", "1.56"),
            ("P3", "3.12"),
            ("P4", "5.81"),
            ("P6", "11.23"),
            ("P10", "21.68"),
            ("P15", "41.81"),
            ("P20", "80.54"),
            ("P30", "148.68"),
            ("P40", "284.94"),
            ("P50", "545.10"),
            ("P60", "1040.64"),
            ("P70", "1982.17"),
            ("P80", "3964.34"),
        ] {
            items.push(disk_item(
                "Premium SSD Managed Disks",
                &format!("{tier} LRS"),
                &format!("{tier} LRS Disk"),
                "1/Month",
                "0",
                price,
            ));
        }
        items.extend([
            disk_item(
                "Azure Premium SSD v2",
                "Premium LRS",
                "Premium LRS Provisioned Capacity",
                "1 GiB/Hour",
                "0",
                "0.000110",
            ),
            disk_item(
                "Azure Premium SSD v2",
                "Premium LRS",
                "Premium LRS Provisioned IOPS",
                "1/Hour",
                "0",
                "0",
            ),
            disk_item(
                "Azure Premium SSD v2",
                "Premium LRS",
                "Premium LRS Provisioned IOPS",
                "1/Hour",
                "3000",
                "0.000007",
            ),
            disk_item(
                "Azure Premium SSD v2",
                "Premium LRS",
                "Premium LRS Provisioned Throughput (MBps)",
                "1/Hour",
                "0",
                "0",
            ),
        ]);
        if include_throughput {
            items.push(disk_item(
                "Azure Premium SSD v2",
                "Premium LRS",
                "Premium LRS Provisioned Throughput (MBps)",
                "1/Hour",
                "125",
                "0.000055",
            ));
        }
        serde_json::to_vec(&json!({ "Items": items })).expect("fixture JSON")
    }

    fn vm_item(price: &str, product: &str, sku: &str) -> Value {
        retail_item(
            "Virtual Machines",
            "Standard_D2s_v7",
            product,
            sku,
            sku,
            "1 Hour",
            "0",
            price,
        )
    }

    fn reservation_item(term: &str, price: &str) -> Value {
        let mut item = vm_item(
            price,
            "Virtual Machines Dsv7-series Linux",
            "Standard_D2s_v7",
        );
        item["type"] = json!("Reservation");
        item["reservationTerm"] = json!(term);
        item
    }

    fn option_record(
        normalized: &AzureVmPricingNormalization,
        purchase_option: VmPurchaseOption,
    ) -> &AzureVmRateRecord {
        normalized
            .vm_records
            .iter()
            .find(|record| record.purchase_option == purchase_option)
            .expect("VM purchase option")
    }

    fn disk_item(
        product: &str,
        sku: &str,
        meter: &str,
        unit: &str,
        tier: &str,
        price: &str,
    ) -> Value {
        retail_item("Storage", "", product, sku, meter, unit, tier, price)
    }

    #[allow(clippy::too_many_arguments)]
    fn retail_item(
        service: &str,
        arm_sku: &str,
        product: &str,
        sku: &str,
        meter: &str,
        unit: &str,
        tier: &str,
        price: &str,
    ) -> Value {
        json!({
            "serviceName": service,
            "armRegionName": "swedencentral",
            "currencyCode": "USD",
            "armSkuName": arm_sku,
            "productName": product,
            "skuName": sku,
            "meterName": meter,
            "unitOfMeasure": unit,
            "type": "Consumption",
            "retailPrice": price,
            "tierMinimumUnits": tier,
            "effectiveStartDate": "2026-04-01T00:00:00Z",
            "meterId": format!("meter-{service}-{meter}-{tier}"),
            "isPrimaryMeterRegion": true
        })
    }
}
