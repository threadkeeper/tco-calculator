use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    calculation::{cost::AzureRate, target_selector::ServiceTier},
    domain::{decimal::DecimalValue, resource::PurchaseOption},
    pricing::snapshot::{AzureMiRateRecord, RateProvenance},
};

#[derive(Clone, Copy)]
pub struct AzureSqlMiNormalizationContext<'a> {
    pub target_region: &'a str,
    pub calculator_region_slug: &'a str,
    pub currency: &'a str,
    pub calculator_source_url: &'a str,
    pub calculator_source_version: Option<&'a str>,
    pub effective_at: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub struct AzureMiConfiguration<'a> {
    pub configuration_key: &'a str,
    pub service_tier: ServiceTier,
    pub hardware_family: &'a str,
    pub vcores: u32,
    pub zone_redundant: bool,
}

#[derive(Clone, Copy)]
pub struct AzureRetailPagePayload<'a> {
    pub source_url: &'a str,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct AzureSqlMiNormalization {
    pub records: Vec<AzureMiRateRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum AzureSqlMiNormalizationError {
    #[error("Azure SQL MI pricing payload JSON is malformed")]
    MalformedJson,
    #[error("Azure SQL MI pricing scope is invalid")]
    InvalidScope,
    #[error("Azure SQL MI calculator configuration is duplicated")]
    DuplicateConfiguration,
    #[error("Azure SQL MI calculator configuration is missing")]
    MissingConfiguration,
    #[error("Azure SQL MI calculator purchase option is missing")]
    MissingPurchaseOption,
    #[error("Azure SQL MI calculator component reference is malformed")]
    InvalidReference,
    #[error("Azure SQL MI calculator component offer is missing")]
    MissingOffer,
    #[error("Azure SQL MI calculator regional component price is missing")]
    MissingRegionPrice,
    #[error("Azure SQL MI calculator or Retail Price value is invalid")]
    InvalidValue,
    #[error("Azure SQL MI Retail Prices returned conflicting current meters")]
    ConflictingRetailRate,
}

#[derive(Deserialize)]
struct CalculatorPayload {
    skus: BTreeMap<String, BTreeMap<String, ReferenceList>>,
    offers: BTreeMap<String, CalculatorOffer>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReferenceList {
    One(String),
    Many(Vec<String>),
}

impl ReferenceList {
    fn values(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Self::One(reference) => Box::new(std::iter::once(reference.as_str())),
            Self::Many(references) => Box::new(references.iter().map(String::as_str)),
        }
    }
}

#[derive(Deserialize)]
struct CalculatorOffer {
    #[serde(rename = "offerType")]
    offer_type: String,
    prices: BTreeMap<String, BTreeMap<String, CalculatorRegionPrice>>,
}

#[derive(Deserialize)]
struct CalculatorRegionPrice {
    value: serde_json::Value,
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
    #[serde(rename = "effectiveStartDate")]
    effective_start_date: String,
    #[serde(rename = "meterId")]
    meter_id: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RetailTier {
    GeneralPurpose,
    NextGenerationGeneralPurpose,
    BusinessCritical,
}

#[derive(Clone)]
struct RetailRate {
    rate: Decimal,
    effective_start_date: String,
    meter_ids: BTreeSet<String>,
}

#[derive(Default)]
struct RetailRates {
    storage: BTreeMap<(RetailTier, bool), RetailRate>,
    memory: BTreeMap<bool, RetailRate>,
}

#[derive(Clone)]
struct OptionRate {
    compute_hourly: Decimal,
    license_hourly: Decimal,
    component_references: BTreeSet<String>,
}

pub fn normalize_azure_sql_mi(
    context: AzureSqlMiNormalizationContext<'_>,
    configurations: &[AzureMiConfiguration<'_>],
    calculator_body: &[u8],
    retail_pages: &[AzureRetailPagePayload<'_>],
) -> Result<AzureSqlMiNormalization, AzureSqlMiNormalizationError> {
    validate_context(context, configurations)?;
    let calculator: CalculatorPayload = serde_json::from_slice(calculator_body)
        .map_err(|_| AzureSqlMiNormalizationError::MalformedJson)?;
    let retail = normalize_retail_rates(context, retail_pages)?;

    let mut warnings = Vec::new();
    let mut records = Vec::new();
    for configuration in configurations {
        let option_rates = calculator_option_rates(context, configuration, &calculator)?;
        let tier = retail_tier(configuration.service_tier);
        let storage = if let Some(rate) = retail.storage.get(&(tier, configuration.zone_redundant))
        {
            rate
        } else if configuration.service_tier == ServiceTier::NextGenerationGeneralPurpose {
            let Some(rate) = retail
                .storage
                .get(&(RetailTier::GeneralPurpose, configuration.zone_redundant))
            else {
                warnings.push(format!(
                    "Azure SQL MI {} has no applicable data-storage meter.",
                    configuration.configuration_key
                ));
                continue;
            };
            warnings.push(format!(
                "Azure SQL MI {} uses the General Purpose data-storage meter fallback for Next Generation General Purpose.",
                configuration.configuration_key
            ));
            rate
        } else {
            warnings.push(format!(
                "Azure SQL MI {} has no applicable data-storage meter.",
                configuration.configuration_key
            ));
            continue;
        };
        let Some(memory) = retail.memory.get(&configuration.zone_redundant) else {
            warnings.push(format!(
                "Azure SQL MI {} has no applicable Premium-series additional-memory meter.",
                configuration.configuration_key
            ));
            continue;
        };

        for purchase_option in PurchaseOption::ALL {
            let option_rate = option_rates
                .get(&purchase_option)
                .ok_or(AzureSqlMiNormalizationError::MissingPurchaseOption)?;
            let mut meter_ids = option_rate.component_references.clone();
            meter_ids.extend(storage.meter_ids.iter().cloned());
            meter_ids.extend(memory.meter_ids.iter().cloned());
            records.push(AzureMiRateRecord {
                stable_key: format!(
                    "{}|{}|{}|{}|{}|{}",
                    context.target_region,
                    service_tier_key(configuration.service_tier),
                    normalize_key(configuration.hardware_family)?,
                    configuration.vcores,
                    configuration.zone_redundant,
                    purchase_option_key(purchase_option)
                ),
                configuration_key: configuration.configuration_key.to_owned(),
                purchase_option,
                rate: AzureRate {
                    compute_hourly: DecimalValue(option_rate.compute_hourly),
                    license_hourly: DecimalValue(option_rate.license_hourly),
                    storage_monthly_per_gb: DecimalValue(storage.rate),
                    additional_memory_per_gb_hourly: DecimalValue(memory.rate),
                },
                provenance: RateProvenance {
                    source_url: context.calculator_source_url.to_owned(),
                    effective_at: context.effective_at.map(str::to_owned),
                    source_version: context.calculator_source_version.map(str::to_owned),
                    meter_ids: meter_ids.into_iter().collect(),
                },
            });
        }
    }

    warnings.sort();
    warnings.dedup();
    records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(AzureSqlMiNormalization { records, warnings })
}

fn validate_context(
    context: AzureSqlMiNormalizationContext<'_>,
    configurations: &[AzureMiConfiguration<'_>],
) -> Result<(), AzureSqlMiNormalizationError> {
    if context.target_region.is_empty()
        || context.calculator_region_slug.is_empty()
        || context.currency != "USD"
        || context.calculator_source_url.is_empty()
    {
        return Err(AzureSqlMiNormalizationError::InvalidScope);
    }
    let mut keys = BTreeSet::new();
    for configuration in configurations {
        if configuration.configuration_key.is_empty()
            || configuration.hardware_family.is_empty()
            || configuration.vcores == 0
        {
            return Err(AzureSqlMiNormalizationError::InvalidValue);
        }
        if !keys.insert(configuration.configuration_key) {
            return Err(AzureSqlMiNormalizationError::DuplicateConfiguration);
        }
    }
    Ok(())
}

fn calculator_option_rates(
    context: AzureSqlMiNormalizationContext<'_>,
    configuration: &AzureMiConfiguration<'_>,
    calculator: &CalculatorPayload,
) -> Result<BTreeMap<PurchaseOption, OptionRate>, AzureSqlMiNormalizationError> {
    let sku = calculator
        .skus
        .get(configuration.configuration_key)
        .ok_or(AzureSqlMiNormalizationError::MissingConfiguration)?;
    let mut rates = BTreeMap::new();
    for purchase_option in PurchaseOption::ALL {
        let references = sku
            .get(purchase_option_key(purchase_option))
            .ok_or(AzureSqlMiNormalizationError::MissingPurchaseOption)?;
        let mut compute_hourly = Decimal::ZERO;
        let mut license_hourly = Decimal::ZERO;
        let mut component_references = BTreeSet::new();
        let mut reference_count = 0_usize;
        for reference in references.values() {
            reference_count += 1;
            let (offer_key, price_key) = reference
                .split_once("--")
                .filter(|(offer, price)| !offer.is_empty() && !price.is_empty())
                .ok_or(AzureSqlMiNormalizationError::InvalidReference)?;
            if price_key.contains("--") {
                return Err(AzureSqlMiNormalizationError::InvalidReference);
            }
            let offer = calculator
                .offers
                .get(offer_key)
                .ok_or(AzureSqlMiNormalizationError::MissingOffer)?;
            let region_price = offer
                .prices
                .get(price_key)
                .and_then(|regions| regions.get(context.calculator_region_slug))
                .ok_or(AzureSqlMiNormalizationError::MissingRegionPrice)?;
            let price = parse_nonnegative_decimal(&region_price.value)?;
            if offer.offer_type == "software" {
                license_hourly += price;
            } else {
                compute_hourly += price;
            }
            component_references.insert(reference.to_owned());
        }
        if reference_count == 0 || compute_hourly <= Decimal::ZERO {
            return Err(AzureSqlMiNormalizationError::InvalidValue);
        }
        if matches!(
            purchase_option,
            PurchaseOption::Ahb
                | PurchaseOption::AhbOneYear
                | PurchaseOption::AhbThreeYear
                | PurchaseOption::AhbSavingsOneYear
        ) && license_hourly != Decimal::ZERO
        {
            return Err(AzureSqlMiNormalizationError::InvalidValue);
        }
        rates.insert(
            purchase_option,
            OptionRate {
                compute_hourly,
                license_hourly,
                component_references,
            },
        );
    }
    Ok(rates)
}

fn normalize_retail_rates(
    context: AzureSqlMiNormalizationContext<'_>,
    pages: &[AzureRetailPagePayload<'_>],
) -> Result<RetailRates, AzureSqlMiNormalizationError> {
    let mut rates = RetailRates::default();
    for payload in pages {
        if payload.source_url.is_empty() {
            return Err(AzureSqlMiNormalizationError::InvalidScope);
        }
        let page: RetailPage = serde_json::from_slice(payload.body)
            .map_err(|_| AzureSqlMiNormalizationError::MalformedJson)?;
        for item in page.items {
            if item.service_name != "SQL Managed Instance"
                || item.arm_region_name != context.target_region
            {
                continue;
            }
            if item.currency_code != context.currency {
                return Err(AzureSqlMiNormalizationError::InvalidValue);
            }
            if item.price_type != "Consumption" {
                continue;
            }
            let zone_redundant = is_zone_redundant(&item.sku_name, &item.meter_name);
            if is_data_storage(&item) {
                let Some(tier) = parse_retail_tier(&item.product_name) else {
                    continue;
                };
                if !item.unit_of_measure.contains("Month") {
                    continue;
                }
                let rate = retail_rate(&item)?;
                merge_minimum_rate(&mut rates.storage, (tier, zone_redundant), rate)?;
            } else if is_additional_memory(&item) {
                if parse_retail_tier(&item.product_name) != Some(RetailTier::GeneralPurpose)
                    || !is_premium_series(&item.product_name)
                    || item.unit_of_measure != "1 GB/Hour"
                {
                    continue;
                }
                let rate = retail_rate(&item)?;
                merge_latest_rate(&mut rates.memory, zone_redundant, rate)?;
            }
        }
    }
    Ok(rates)
}

fn retail_rate(item: &RetailItem) -> Result<RetailRate, AzureSqlMiNormalizationError> {
    let rate = parse_nonnegative_decimal(&item.retail_price)?;
    if rate <= Decimal::ZERO || item.effective_start_date.is_empty() || item.meter_id.is_empty() {
        return Err(AzureSqlMiNormalizationError::InvalidValue);
    }
    let mut meter_ids = BTreeSet::new();
    meter_ids.insert(item.meter_id.clone());
    Ok(RetailRate {
        rate,
        effective_start_date: item.effective_start_date.clone(),
        meter_ids,
    })
}

fn merge_minimum_rate<K: Ord>(
    rates: &mut BTreeMap<K, RetailRate>,
    key: K,
    incoming: RetailRate,
) -> Result<(), AzureSqlMiNormalizationError> {
    match rates.get_mut(&key) {
        Some(existing) if incoming.rate < existing.rate => {
            *existing = incoming;
        }
        Some(existing) if incoming.rate == existing.rate => {
            existing.meter_ids.extend(incoming.meter_ids);
        }
        Some(_) => {}
        None => {
            rates.insert(key, incoming);
        }
    }
    Ok(())
}

fn merge_latest_rate<K: Ord>(
    rates: &mut BTreeMap<K, RetailRate>,
    key: K,
    incoming: RetailRate,
) -> Result<(), AzureSqlMiNormalizationError> {
    match rates.get_mut(&key) {
        Some(existing) if incoming.effective_start_date > existing.effective_start_date => {
            *existing = incoming;
        }
        Some(existing) if incoming.effective_start_date == existing.effective_start_date => {
            if incoming.rate != existing.rate {
                return Err(AzureSqlMiNormalizationError::ConflictingRetailRate);
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

fn is_data_storage(item: &RetailItem) -> bool {
    !item.product_name.contains("PITR Backup")
        && !item.product_name.contains("LTR Backup")
        && item.product_name.contains("Storage")
}

fn is_additional_memory(item: &RetailItem) -> bool {
    item.meter_name.contains("Memory")
}

fn is_premium_series(product_name: &str) -> bool {
    product_name.contains("Premium Series")
        && !product_name.contains("Premium Series Memory Optimized")
}

fn is_zone_redundant(sku_name: &str, meter_name: &str) -> bool {
    sku_name.contains("Zone Redundan")
        || meter_name.contains("Zone Redundan")
        || sku_name.split_whitespace().any(|part| part == "ZR")
        || meter_name.split_whitespace().any(|part| part == "ZR")
}

fn parse_retail_tier(product_name: &str) -> Option<RetailTier> {
    if product_name.contains("Next Generation General Purpose")
        || product_name.contains("next-gen-general-purpose")
    {
        Some(RetailTier::NextGenerationGeneralPurpose)
    } else if product_name.contains("Business Critical")
        || product_name.contains("business-critical")
    {
        Some(RetailTier::BusinessCritical)
    } else if product_name.contains("General Purpose") || product_name.contains("general-purpose") {
        Some(RetailTier::GeneralPurpose)
    } else {
        None
    }
}

fn retail_tier(service_tier: ServiceTier) -> RetailTier {
    match service_tier {
        ServiceTier::NextGenerationGeneralPurpose => RetailTier::NextGenerationGeneralPurpose,
        ServiceTier::BusinessCritical => RetailTier::BusinessCritical,
    }
}

fn service_tier_key(service_tier: ServiceTier) -> &'static str {
    match service_tier {
        ServiceTier::NextGenerationGeneralPurpose => "next-gen-general-purpose",
        ServiceTier::BusinessCritical => "business-critical",
    }
}

fn purchase_option_key(option: PurchaseOption) -> &'static str {
    match option {
        PurchaseOption::Payg => "payg",
        PurchaseOption::Ahb => "ahb",
        PurchaseOption::OneYear => "one-year",
        PurchaseOption::AhbOneYear => "ahbone-year",
        PurchaseOption::ThreeYear => "three-year",
        PurchaseOption::AhbThreeYear => "ahbthree-year",
        PurchaseOption::SavingsOneYear => "sv-one-year",
        PurchaseOption::AhbSavingsOneYear => "ahbsv-one-year",
    }
}

fn normalize_key(value: &str) -> Result<String, AzureSqlMiNormalizationError> {
    let mut normalized = String::new();
    let mut previous_separator = false;
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator && !normalized.is_empty() {
            normalized.push('-');
            previous_separator = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return Err(AzureSqlMiNormalizationError::InvalidValue);
    }
    Ok(normalized)
}

fn parse_nonnegative_decimal(
    value: &serde_json::Value,
) -> Result<Decimal, AzureSqlMiNormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let value = Decimal::from_str(&raw).map_err(|_| AzureSqlMiNormalizationError::InvalidValue)?;
    if value < Decimal::ZERO {
        return Err(AzureSqlMiNormalizationError::InvalidValue);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIGURATION: &str = "managed-vcore-next-gen-general-purpose-premium-series-32";

    #[test]
    fn composes_all_eight_options_and_workbook_retail_fallbacks() {
        let calculator = calculator_payload(true);
        let retail = retail_page();
        let normalized = normalize_azure_sql_mi(
            context(),
            &[configuration()],
            &calculator,
            &[AzureRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices",
                body: &retail,
            }],
        )
        .expect("normalize Azure SQL MI prices");

        assert_eq!(normalized.records.len(), 8);
        assert_eq!(normalized.warnings.len(), 1);
        let payg = record(&normalized, PurchaseOption::Payg);
        assert_eq!(payg.rate.compute_hourly.to_string(), "5.632");
        assert_eq!(payg.rate.license_hourly.to_string(), "3.198912");
        assert_eq!(payg.rate.storage_monthly_per_gb.to_string(), "0.13685");
        assert_eq!(
            payg.rate.additional_memory_per_gb_hourly.to_string(),
            "0.011663"
        );
        assert_eq!(
            record(&normalized, PurchaseOption::Ahb).rate.license_hourly,
            DecimalValue::ZERO
        );
        assert_eq!(
            record(&normalized, PurchaseOption::SavingsOneYear)
                .rate
                .license_hourly
                .to_string(),
            "2.23936"
        );
    }

    #[test]
    fn rejects_incomplete_calculator_purchase_matrix() {
        let calculator = calculator_payload(false);
        let retail = retail_page();

        assert!(matches!(
            normalize_azure_sql_mi(
                context(),
                &[configuration()],
                &calculator,
                &[AzureRetailPagePayload {
                    source_url: "https://prices.azure.com/api/retail/prices",
                    body: &retail,
                }],
            ),
            Err(AzureSqlMiNormalizationError::MissingPurchaseOption)
        ));
    }

    #[test]
    fn missing_retail_component_keeps_configuration_unavailable() {
        let calculator = calculator_payload(true);
        let empty_retail = serde_json::to_vec(&serde_json::json!({ "Items": [] }))
            .expect("serialize empty Retail Prices page");

        let normalized = normalize_azure_sql_mi(
            context(),
            &[configuration()],
            &calculator,
            &[AzureRetailPagePayload {
                source_url: "https://prices.azure.com/api/retail/prices",
                body: &empty_retail,
            }],
        )
        .expect("normalize missing Retail Price components");

        assert!(normalized.records.is_empty());
        assert_eq!(normalized.warnings.len(), 1);
    }

    fn context() -> AzureSqlMiNormalizationContext<'static> {
        AzureSqlMiNormalizationContext {
            target_region: "swedencentral",
            calculator_region_slug: "sweden-central",
            currency: "USD",
            calculator_source_url: "https://azure.microsoft.com/api/v3/pricing/azure-sql/calculator/",
            calculator_source_version: Some("test-v1"),
            effective_at: Some("2026-01-01T00:00:00Z"),
        }
    }

    fn configuration() -> AzureMiConfiguration<'static> {
        AzureMiConfiguration {
            configuration_key: CONFIGURATION,
            service_tier: ServiceTier::NextGenerationGeneralPurpose,
            hardware_family: "Premium Series",
            vcores: 32,
            zone_redundant: false,
        }
    }

    fn record(normalized: &AzureSqlMiNormalization, option: PurchaseOption) -> &AzureMiRateRecord {
        normalized
            .records
            .iter()
            .find(|record| record.purchase_option == option)
            .expect("purchase option record")
    }

    fn calculator_payload(include_last_option: bool) -> Vec<u8> {
        let mut options = serde_json::Map::new();
        options.insert(
            "payg".to_owned(),
            serde_json::json!(["compute-payg--rate", "software-payg--rate"]),
        );
        options.insert("ahb".to_owned(), serde_json::json!("compute-payg--rate"));
        options.insert(
            "one-year".to_owned(),
            serde_json::json!(["compute-one-year--rate", "software-payg--rate"]),
        );
        options.insert(
            "ahbone-year".to_owned(),
            serde_json::json!("compute-one-year--rate"),
        );
        options.insert(
            "three-year".to_owned(),
            serde_json::json!(["compute-three-year--rate", "software-payg--rate"]),
        );
        options.insert(
            "ahbthree-year".to_owned(),
            serde_json::json!("compute-three-year--rate"),
        );
        options.insert(
            "sv-one-year".to_owned(),
            serde_json::json!(["compute-savings--rate", "software-savings--rate"]),
        );
        if include_last_option {
            options.insert(
                "ahbsv-one-year".to_owned(),
                serde_json::json!("compute-savings--rate"),
            );
        }
        let mut skus = serde_json::Map::new();
        skus.insert(CONFIGURATION.to_owned(), serde_json::Value::Object(options));
        let offers = serde_json::json!({
            "compute-payg": offer("compute", "5.632"),
            "compute-one-year": offer("reservation", "3.6602739712"),
            "compute-three-year": offer("reservation", "2.5339421600"),
            "compute-savings": offer("savings", "4.5056"),
            "software-payg": offer("software", "3.198912"),
            "software-savings": offer("software", "2.23936")
        });
        serde_json::to_vec(&serde_json::json!({
            "skus": skus,
            "offers": offers
        }))
        .expect("serialize calculator payload")
    }

    fn offer(offer_type: &str, value: &str) -> serde_json::Value {
        serde_json::json!({
            "offerType": offer_type,
            "prices": {
                "rate": {
                    "sweden-central": { "value": value }
                }
            }
        })
    }

    fn retail_page() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "Items": [
                {
                    "serviceName": "SQL Managed Instance",
                    "armRegionName": "swedencentral",
                    "currencyCode": "USD",
                    "productName": "SQL Managed Instance General Purpose Storage",
                    "skuName": "Data Stored",
                    "meterName": "Data Stored",
                    "unitOfMeasure": "1 GB/Month",
                    "type": "Consumption",
                    "retailPrice": "0.13685",
                    "effectiveStartDate": "2026-01-01T00:00:00Z",
                    "meterId": "storage-meter"
                },
                {
                    "serviceName": "SQL Managed Instance",
                    "armRegionName": "swedencentral",
                    "currencyCode": "USD",
                    "productName": "SQL Managed Instance General Purpose Compute Premium Series",
                    "skuName": "vCore",
                    "meterName": "Additional Memory",
                    "unitOfMeasure": "1 GB/Hour",
                    "type": "Consumption",
                    "retailPrice": "0.011663",
                    "effectiveStartDate": "2026-01-01T00:00:00Z",
                    "meterId": "memory-meter"
                }
            ]
        }))
        .expect("serialize Retail Prices page")
    }
}
