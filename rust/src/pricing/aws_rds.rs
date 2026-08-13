use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::Read,
    str::FromStr,
};

use rust_decimal::Decimal;
use serde::{
    Deserialize, Serialize,
    de::{DeserializeSeed, Error as _, IgnoredAny, MapAccess, Visitor},
};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    calculation::cost::RdsRate,
    domain::{decimal::DecimalValue, resource::RdsDeployment},
    pricing::snapshot::{AwsRdsRateRecord, RateProvenance},
};

const STANDARD_LICENSE_FALLBACK: Decimal = Decimal::from_parts(12, 0, 0, false, 2);
const ENTERPRISE_LICENSE_FALLBACK: Decimal = Decimal::from_parts(375, 0, 0, false, 3);

#[derive(Clone, Copy)]
pub struct RdsNormalizationContext<'a> {
    pub region_code: &'a str,
}

#[derive(Clone, Copy)]
pub struct RdsLeafPayload<'a> {
    pub source_url: &'a str,
    pub source_version: Option<&'a str>,
    pub effective_at: Option<&'a str>,
    pub body: &'a [u8],
}

#[derive(Debug)]
pub struct RdsNormalization {
    pub records: Vec<AwsRdsRateRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RdsNormalizationError {
    #[error("RDS selected price leaf JSON is malformed")]
    MalformedJson,
    #[error("RDS selected price leaf has an unsupported offer code")]
    UnsupportedOffer,
    #[error("RDS price or sizing value is invalid")]
    InvalidValue,
    #[error("RDS price leaf returned an unsupported price unit")]
    UnsupportedUnit,
    #[error("RDS Reserved term attributes are invalid")]
    InvalidCommercialTerm,
    #[error("RDS price leaf returned conflicting records for one normalized component")]
    ConflictingComponent,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RdsProjectionError {
    #[error("RDS offer code is unsupported")]
    UnsupportedOffer,
    #[error("RDS offer JSON is malformed")]
    MalformedJson,
    #[error("RDS offer manifest is invalid")]
    InvalidManifest,
}

pub struct ProjectedRdsOffer {
    pub source_version: String,
    pub effective_at: String,
    pub body: Vec<u8>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SelectedLeaf {
    source_offer_code: String,
    dimensions: Vec<RawDimension>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawDimension {
    sku: String,
    product_family: String,
    #[serde(default)]
    database_engine: Option<String>,
    #[serde(default)]
    database_edition: Option<String>,
    #[serde(default)]
    license_model: Option<String>,
    #[serde(default)]
    license_type: Option<String>,
    #[serde(default)]
    deployment_model: Option<String>,
    #[serde(default)]
    deployment_option: Option<String>,
    #[serde(default)]
    instance_type: Option<String>,
    #[serde(default)]
    memory: Option<String>,
    #[serde(default)]
    vcpu: Option<String>,
    #[serde(default)]
    volume_name: Option<String>,
    #[serde(default)]
    volume_type: Option<String>,
    term_type: String,
    offer_term_code: String,
    #[serde(default)]
    lease_contract_length: Option<String>,
    #[serde(default)]
    purchase_option: Option<String>,
    #[serde(default)]
    offering_class: Option<String>,
    rate_code: String,
    unit: String,
    price: serde_json::Value,
}

struct RawOfferProjection {
    format_version: String,
    offer_code: String,
    source_version: String,
    published_at: String,
    dimensions: Vec<RawDimension>,
}

struct OfferProjectionSeed<'a> {
    expected_offer_code: &'a str,
}

impl<'de> DeserializeSeed<'de> for OfferProjectionSeed<'_> {
    type Value = RawOfferProjection;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(OfferProjectionVisitor {
            expected_offer_code: self.expected_offer_code,
        })
    }
}

struct OfferProjectionVisitor<'a> {
    expected_offer_code: &'a str,
}

impl<'de> Visitor<'de> for OfferProjectionVisitor<'_> {
    type Value = RawOfferProjection;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an AWS regional offer document")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut format_version = None;
        let mut offer_code = None;
        let mut source_version = None;
        let mut published_at = None;
        let mut products = None;
        let mut dimensions = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "formatVersion" => {
                    set_once(&mut format_version, map.next_value()?, "formatVersion")?;
                }
                "offerCode" => {
                    set_once(&mut offer_code, map.next_value()?, "offerCode")?;
                }
                "version" => {
                    set_once(&mut source_version, map.next_value()?, "version")?;
                }
                "publicationDate" => {
                    set_once(&mut published_at, map.next_value()?, "publicationDate")?;
                }
                "products" => {
                    if products.is_some() {
                        return Err(A::Error::duplicate_field("products"));
                    }
                    products = Some(map.next_value_seed(ProductsSeed {
                        expected_offer_code: self.expected_offer_code,
                    })?);
                }
                "terms" => {
                    if dimensions.is_some() {
                        return Err(A::Error::duplicate_field("terms"));
                    }
                    let products = products.as_ref().ok_or_else(|| {
                        A::Error::custom("RDS products must precede terms for streaming projection")
                    })?;
                    dimensions = Some(map.next_value_seed(TermsSeed { products })?);
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(RawOfferProjection {
            format_version: required_field(format_version, "formatVersion")?,
            offer_code: required_field(offer_code, "offerCode")?,
            source_version: required_field(source_version, "version")?,
            published_at: required_field(published_at, "publicationDate")?,
            dimensions: required_field(dimensions, "terms")?,
        })
    }
}

fn set_once<E: serde::de::Error>(
    slot: &mut Option<String>,
    value: String,
    field: &'static str,
) -> Result<(), E> {
    if slot.replace(value).is_some() {
        return Err(E::duplicate_field(field));
    }
    Ok(())
}

fn required_field<T, E: serde::de::Error>(value: Option<T>, field: &'static str) -> Result<T, E> {
    value.ok_or_else(|| E::missing_field(field))
}

#[derive(Clone)]
struct ProjectedProduct {
    sku: String,
    product_family: String,
    attributes: ProductAttributes,
}

#[derive(Deserialize)]
struct RawProduct {
    sku: String,
    #[serde(rename = "productFamily")]
    product_family: Option<String>,
    attributes: ProductAttributes,
}

#[derive(Clone, Default, Deserialize)]
struct ProductAttributes {
    #[serde(rename = "databaseEngine")]
    database_engine: Option<String>,
    #[serde(rename = "databaseEdition")]
    database_edition: Option<String>,
    #[serde(rename = "licenseModel")]
    license_model: Option<String>,
    #[serde(rename = "licenseType")]
    license_type: Option<String>,
    #[serde(rename = "deploymentModel")]
    deployment_model: Option<String>,
    #[serde(rename = "deploymentOption")]
    deployment_option: Option<String>,
    operation: Option<String>,
    #[serde(rename = "instanceType")]
    instance_type: Option<String>,
    memory: Option<String>,
    vcpu: Option<String>,
    #[serde(rename = "volumeName")]
    volume_name: Option<String>,
    #[serde(rename = "volumeType")]
    volume_type: Option<String>,
}

struct ProductsSeed<'a> {
    expected_offer_code: &'a str,
}

impl<'de> DeserializeSeed<'de> for ProductsSeed<'_> {
    type Value = BTreeMap<String, ProjectedProduct>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ProductsVisitor {
            expected_offer_code: self.expected_offer_code,
        })
    }
}

struct ProductsVisitor<'a> {
    expected_offer_code: &'a str,
}

impl<'de> Visitor<'de> for ProductsVisitor<'_> {
    type Value = BTreeMap<String, ProjectedProduct>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an AWS offer product map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut products = BTreeMap::new();
        while let Some(sku) = map.next_key::<String>()? {
            let product = map.next_value::<RawProduct>()?;
            if product.sku != sku {
                return Err(A::Error::custom(
                    "RDS product SKU does not match its map key",
                ));
            }
            if is_selected_product(self.expected_offer_code, &product) {
                let Some(product_family) = product.product_family else {
                    continue;
                };
                products.insert(
                    sku,
                    ProjectedProduct {
                        sku: product.sku,
                        product_family,
                        attributes: product.attributes,
                    },
                );
            }
        }
        Ok(products)
    }
}

fn is_selected_product(offer_code: &str, product: &RawProduct) -> bool {
    match offer_code {
        "AmazonRDS" => {
            product.attributes.database_engine.as_deref() == Some("SQL Server")
                && matches!(
                    product.attributes.database_edition.as_deref(),
                    Some("Standard" | "Enterprise")
                )
                && match product.product_family.as_deref() {
                    Some("Database Instance") => customer_provided_media(&product.attributes),
                    Some("Database Storage") => true,
                    _ => false,
                }
        }
        "AmazonRDSOCPULicenseFees" => {
            product.product_family.as_deref() == Some("Optimized License")
                && product.attributes.license_type.as_deref() == Some("SQLServer")
                && matches!(
                    product.attributes.database_edition.as_deref(),
                    Some("Standard" | "Enterprise")
                )
        }
        _ => false,
    }
}

fn customer_provided_media(attributes: &ProductAttributes) -> bool {
    attributes.license_model.as_deref() == Some("Bring your own media")
        || attributes.deployment_model.as_deref() == Some("Custom")
            && matches!(
                (
                    attributes.database_edition.as_deref(),
                    attributes.operation.as_deref()
                ),
                (Some("Standard"), Some("CreateDBInstance:0405"))
                    | (Some("Enterprise"), Some("CreateDBInstance:0406"))
            )
}

struct TermsSeed<'a> {
    products: &'a BTreeMap<String, ProjectedProduct>,
}

impl<'de> DeserializeSeed<'de> for TermsSeed<'_> {
    type Value = Vec<RawDimension>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(TermsVisitor {
            products: self.products,
        })
    }
}

struct TermsVisitor<'a> {
    products: &'a BTreeMap<String, ProjectedProduct>,
}

impl<'de> Visitor<'de> for TermsVisitor<'_> {
    type Value = Vec<RawDimension>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an AWS offer terms map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut dimensions = Vec::new();
        while let Some(term_type) = map.next_key::<String>()? {
            if matches!(term_type.as_str(), "OnDemand" | "Reserved") {
                dimensions.extend(map.next_value_seed(SkuTermsSeed {
                    term_type: &term_type,
                    products: self.products,
                })?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(dimensions)
    }
}

struct SkuTermsSeed<'a> {
    term_type: &'a str,
    products: &'a BTreeMap<String, ProjectedProduct>,
}

impl<'de> DeserializeSeed<'de> for SkuTermsSeed<'_> {
    type Value = Vec<RawDimension>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(SkuTermsVisitor {
            term_type: self.term_type,
            products: self.products,
        })
    }
}

struct SkuTermsVisitor<'a> {
    term_type: &'a str,
    products: &'a BTreeMap<String, ProjectedProduct>,
}

impl<'de> Visitor<'de> for SkuTermsVisitor<'_> {
    type Value = Vec<RawDimension>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an AWS offer SKU terms map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut dimensions = Vec::new();
        while let Some(sku) = map.next_key::<String>()? {
            let Some(product) = self.products.get(&sku) else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            let terms = map.next_value::<BTreeMap<String, RawOfferTerm>>()?;
            for term in terms.into_values() {
                if term.sku != sku {
                    return Err(A::Error::custom("RDS term SKU does not match its map key"));
                }
                for dimension in term.price_dimensions.values() {
                    let price = dimension
                        .price_per_unit
                        .get("USD")
                        .cloned()
                        .ok_or_else(|| A::Error::custom("RDS price dimension has no USD rate"))?;
                    dimensions.push(project_dimension(
                        self.term_type,
                        product,
                        &term,
                        dimension,
                        price,
                    ));
                }
            }
        }
        Ok(dimensions)
    }
}

#[derive(Deserialize)]
struct RawOfferTerm {
    #[serde(rename = "offerTermCode")]
    offer_term_code: String,
    sku: String,
    #[serde(rename = "priceDimensions")]
    price_dimensions: BTreeMap<String, RawPriceDimension>,
    #[serde(default, rename = "termAttributes")]
    term_attributes: RawTermAttributes,
}

#[derive(Default, Deserialize)]
struct RawTermAttributes {
    #[serde(rename = "LeaseContractLength")]
    lease_contract_length: Option<String>,
    #[serde(rename = "PurchaseOption")]
    purchase_option: Option<String>,
    #[serde(rename = "OfferingClass")]
    offering_class: Option<String>,
}

#[derive(Deserialize)]
struct RawPriceDimension {
    #[serde(rename = "rateCode")]
    rate_code: String,
    unit: String,
    #[serde(rename = "pricePerUnit")]
    price_per_unit: BTreeMap<String, serde_json::Value>,
}

fn project_dimension(
    term_type: &str,
    product: &ProjectedProduct,
    term: &RawOfferTerm,
    dimension: &RawPriceDimension,
    price: serde_json::Value,
) -> RawDimension {
    RawDimension {
        sku: product.sku.clone(),
        product_family: product.product_family.clone(),
        database_engine: product.attributes.database_engine.clone(),
        database_edition: product.attributes.database_edition.clone(),
        license_model: product.attributes.license_model.clone(),
        license_type: product.attributes.license_type.clone(),
        deployment_model: product.attributes.deployment_model.clone(),
        deployment_option: product.attributes.deployment_option.clone(),
        instance_type: product.attributes.instance_type.clone(),
        memory: product.attributes.memory.clone(),
        vcpu: product.attributes.vcpu.clone(),
        volume_name: product.attributes.volume_name.clone(),
        volume_type: product.attributes.volume_type.clone(),
        term_type: term_type.to_owned(),
        offer_term_code: term.offer_term_code.clone(),
        lease_contract_length: term.term_attributes.lease_contract_length.clone(),
        purchase_option: term.term_attributes.purchase_option.clone(),
        offering_class: term.term_attributes.offering_class.clone(),
        rate_code: dimension.rate_code.clone(),
        unit: dimension.unit.clone(),
        price,
    }
}

pub fn project_rds_offer(
    reader: impl Read,
    expected_offer_code: &str,
) -> Result<ProjectedRdsOffer, RdsProjectionError> {
    if !matches!(
        expected_offer_code,
        "AmazonRDS" | "AmazonRDSOCPULicenseFees"
    ) {
        return Err(RdsProjectionError::UnsupportedOffer);
    }
    let mut deserializer = serde_json::Deserializer::from_reader(reader);
    let projection = OfferProjectionSeed {
        expected_offer_code,
    }
    .deserialize(&mut deserializer)
    .map_err(|_| RdsProjectionError::MalformedJson)?;
    deserializer
        .end()
        .map_err(|_| RdsProjectionError::MalformedJson)?;
    if projection.format_version != "v1.0"
        || projection.offer_code != expected_offer_code
        || projection.source_version.is_empty()
        || OffsetDateTime::parse(&projection.published_at, &Rfc3339).is_err()
    {
        return Err(RdsProjectionError::InvalidManifest);
    }
    let body = serde_json::to_vec(&SelectedLeaf {
        source_offer_code: projection.offer_code,
        dimensions: projection.dimensions,
    })
    .map_err(|_| RdsProjectionError::MalformedJson)?;
    Ok(ProjectedRdsOffer {
        source_version: projection.source_version,
        effective_at: projection.published_at,
        body,
    })
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OfferKey {
    sku: String,
    offer_term_code: String,
    commercial_term: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ComputeKey {
    instance_type: String,
    deployment: Deployment,
    commercial_term: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StorageKey {
    deployment: Deployment,
    storage_class: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Deployment {
    SingleAz,
    MultiAz,
}

impl Deployment {
    fn as_key(self) -> &'static str {
        match self {
            Self::SingleAz => "single-az",
            Self::MultiAz => "multi-az",
        }
    }

    fn as_domain(self) -> RdsDeployment {
        match self {
            Self::SingleAz => RdsDeployment::SingleAz,
            Self::MultiAz => RdsDeployment::MultiAz,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourceIdentity {
    source_url: String,
    source_version: Option<String>,
    effective_at: Option<String>,
}

#[derive(Clone)]
struct ComputeAccumulator {
    key: ComputeKey,
    source_vcpu: u32,
    memory_gb: DecimalValue,
    recurring_hourly: Decimal,
    upfront: Decimal,
    meter_ids: BTreeSet<String>,
    source: SourceIdentity,
}

#[derive(Clone)]
struct ComputeComponent {
    source_vcpu: u32,
    memory_gb: DecimalValue,
    effective_hourly: Decimal,
    meter_ids: BTreeSet<String>,
    source: SourceIdentity,
}

#[derive(Clone)]
struct StorageComponent {
    monthly_per_gb: Decimal,
    meter_ids: BTreeSet<String>,
}

#[derive(Clone)]
struct LicenseComponent {
    core_hourly: Decimal,
    meter_ids: BTreeSet<String>,
}

pub fn normalize_rds_leaves(
    context: RdsNormalizationContext<'_>,
    leaves: &[RdsLeafPayload<'_>],
) -> Result<RdsNormalization, RdsNormalizationError> {
    if context.region_code.is_empty() {
        return Err(RdsNormalizationError::InvalidValue);
    }

    let mut compute_offers = BTreeMap::<OfferKey, ComputeAccumulator>::new();
    let mut storage = BTreeMap::<StorageKey, StorageComponent>::new();
    let mut standard_license = None;
    let mut enterprise_license = None;

    for payload in leaves {
        if payload.source_url.is_empty() {
            return Err(RdsNormalizationError::InvalidValue);
        }
        let leaf: SelectedLeaf = serde_json::from_slice(payload.body)
            .map_err(|_| RdsNormalizationError::MalformedJson)?;
        match leaf.source_offer_code.as_str() {
            "AmazonRDS" => {
                for dimension in &leaf.dimensions {
                    if is_compute_dimension(dimension) {
                        add_compute_dimension(&mut compute_offers, payload, dimension)?;
                    } else if is_storage_dimension(dimension) {
                        add_storage_dimension(&mut storage, dimension)?;
                    }
                }
            }
            "AmazonRDSOCPULicenseFees" => {
                for dimension in &leaf.dimensions {
                    add_license_dimension(
                        dimension,
                        &mut standard_license,
                        &mut enterprise_license,
                    )?;
                }
            }
            _ => return Err(RdsNormalizationError::UnsupportedOffer),
        }
    }

    let (compute, conflicting_compute) = collapse_compute_offers(compute_offers)?;
    let mut warnings = conflicting_compute
        .into_iter()
        .map(|key| {
            format!(
                "RDS {} {} {} has conflicting edition-specific compute rates and was omitted.",
                key.instance_type,
                key.deployment.as_key(),
                key.commercial_term
            )
        })
        .collect::<Vec<_>>();
    let standard_license = license_or_fallback(
        standard_license,
        STANDARD_LICENSE_FALLBACK,
        "Standard",
        &mut warnings,
    );
    let enterprise_license = license_or_fallback(
        enterprise_license,
        ENTERPRISE_LICENSE_FALLBACK,
        "Enterprise",
        &mut warnings,
    );

    let mut records = Vec::new();
    for (compute_key, compute_component) in compute {
        let mut matched_storage = false;
        for (storage_key, storage_component) in &storage {
            if storage_key.deployment != compute_key.deployment {
                continue;
            }
            matched_storage = true;
            let mut meter_ids = compute_component.meter_ids.clone();
            meter_ids.extend(storage_component.meter_ids.iter().cloned());
            meter_ids.extend(standard_license.meter_ids.iter().cloned());
            meter_ids.extend(enterprise_license.meter_ids.iter().cloned());

            records.push(AwsRdsRateRecord {
                stable_key: format!(
                    "{}|{}|{}|{}|{}",
                    context.region_code,
                    compute_key.instance_type,
                    compute_key.deployment.as_key(),
                    compute_key.commercial_term,
                    storage_key.storage_class
                ),
                instance_type: compute_key.instance_type.clone(),
                deployment: compute_key.deployment.as_domain(),
                commercial_term: compute_key.commercial_term.clone(),
                storage_class: storage_key.storage_class.clone(),
                rate: RdsRate {
                    source_vcpu: compute_component.source_vcpu,
                    catalog_memory_gb: compute_component.memory_gb,
                    effective_compute_hourly: DecimalValue(compute_component.effective_hourly),
                    storage_monthly_per_gb: DecimalValue(storage_component.monthly_per_gb),
                    standard_license_core_hourly: DecimalValue(standard_license.core_hourly),
                    enterprise_license_core_hourly: DecimalValue(enterprise_license.core_hourly),
                },
                provenance: RateProvenance {
                    source_url: compute_component.source.source_url.clone(),
                    effective_at: compute_component.source.effective_at.clone(),
                    source_version: compute_component.source.source_version.clone(),
                    meter_ids: meter_ids.into_iter().collect(),
                },
            });
        }
        if !matched_storage {
            warnings.push(format!(
                "RDS {} {} has no matching SQL Server storage rate.",
                compute_key.instance_type,
                compute_key.deployment.as_key()
            ));
        }
    }

    warnings.sort();
    warnings.dedup();
    records.sort_by(|left, right| left.stable_key.cmp(&right.stable_key));
    Ok(RdsNormalization { records, warnings })
}

fn is_compute_dimension(dimension: &RawDimension) -> bool {
    dimension.product_family == "Database Instance"
        && dimension.database_engine.as_deref() == Some("SQL Server")
        && matches!(
            dimension.database_edition.as_deref(),
            Some("Standard" | "Enterprise")
        )
        && (dimension.deployment_model.as_deref() == Some("Custom")
            || dimension.license_model.as_deref() == Some("Bring your own media"))
}

fn is_storage_dimension(dimension: &RawDimension) -> bool {
    dimension.product_family == "Database Storage"
        && dimension.database_engine.as_deref() == Some("SQL Server")
        && matches!(
            dimension.database_edition.as_deref(),
            Some("Standard" | "Enterprise")
        )
}

fn add_compute_dimension(
    offers: &mut BTreeMap<OfferKey, ComputeAccumulator>,
    payload: &RdsLeafPayload<'_>,
    dimension: &RawDimension,
) -> Result<(), RdsNormalizationError> {
    let instance_type = required(dimension.instance_type.as_deref())?;
    let deployment = parse_deployment(required(dimension.deployment_option.as_deref())?)?;
    let commercial_term = commercial_term(dimension)?;
    let source_vcpu = required(dimension.vcpu.as_deref())?
        .parse::<u32>()
        .map_err(|_| RdsNormalizationError::InvalidValue)?;
    let memory_gb = parse_memory(required(dimension.memory.as_deref())?)?;
    if source_vcpu == 0 {
        return Err(RdsNormalizationError::InvalidValue);
    }
    let price = parse_nonnegative_decimal(&dimension.price)?;
    let (recurring_hourly, upfront) = match dimension.unit.as_str() {
        "Hrs" => (price, Decimal::ZERO),
        "Quantity" if dimension.term_type == "Reserved" => (Decimal::ZERO, price),
        _ => return Err(RdsNormalizationError::UnsupportedUnit),
    };
    let key = OfferKey {
        sku: dimension.sku.clone(),
        offer_term_code: dimension.offer_term_code.clone(),
        commercial_term: commercial_term.clone(),
    };
    let source = SourceIdentity {
        source_url: payload.source_url.to_owned(),
        source_version: payload.source_version.map(str::to_owned),
        effective_at: payload.effective_at.map(str::to_owned),
    };
    let component = offers.entry(key).or_insert_with(|| ComputeAccumulator {
        key: ComputeKey {
            instance_type: instance_type.to_owned(),
            deployment,
            commercial_term,
        },
        source_vcpu,
        memory_gb,
        recurring_hourly: Decimal::ZERO,
        upfront: Decimal::ZERO,
        meter_ids: BTreeSet::new(),
        source: source.clone(),
    });
    if component.key.instance_type != instance_type
        || component.key.deployment != deployment
        || component.source_vcpu != source_vcpu
        || component.memory_gb != memory_gb
        || component.source != source
    {
        return Err(RdsNormalizationError::ConflictingComponent);
    }
    component.recurring_hourly += recurring_hourly;
    component.upfront += upfront;
    component.meter_ids.insert(dimension.rate_code.clone());
    Ok(())
}

fn add_storage_dimension(
    storage: &mut BTreeMap<StorageKey, StorageComponent>,
    dimension: &RawDimension,
) -> Result<(), RdsNormalizationError> {
    if dimension.term_type != "OnDemand" || dimension.unit != "GB-Mo" {
        return Err(RdsNormalizationError::UnsupportedUnit);
    }
    let deployment = parse_deployment(required(dimension.deployment_option.as_deref())?)?;
    let storage_class = storage_class(dimension)?;
    let monthly_per_gb = parse_nonnegative_decimal(&dimension.price)?;
    let component = storage
        .entry(StorageKey {
            deployment,
            storage_class,
        })
        .or_insert_with(|| StorageComponent {
            monthly_per_gb,
            meter_ids: BTreeSet::new(),
        });
    if component.monthly_per_gb != monthly_per_gb {
        return Err(RdsNormalizationError::ConflictingComponent);
    }
    component.meter_ids.insert(dimension.rate_code.clone());
    Ok(())
}

fn add_license_dimension(
    dimension: &RawDimension,
    standard: &mut Option<LicenseComponent>,
    enterprise: &mut Option<LicenseComponent>,
) -> Result<(), RdsNormalizationError> {
    if dimension.product_family != "Optimized License"
        || dimension.license_type.as_deref() != Some("SQLServer")
    {
        return Ok(());
    }
    if dimension.term_type != "OnDemand" || dimension.unit != "vCPU-Hour" {
        return Err(RdsNormalizationError::UnsupportedUnit);
    }
    let slot = match dimension.database_edition.as_deref() {
        Some("Standard") => standard,
        Some("Enterprise") => enterprise,
        _ => return Ok(()),
    };
    let core_hourly = parse_nonnegative_decimal(&dimension.price)?;
    if core_hourly <= Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    match slot {
        Some(component) if component.core_hourly != core_hourly => {
            return Err(RdsNormalizationError::ConflictingComponent);
        }
        Some(component) => {
            component.meter_ids.insert(dimension.rate_code.clone());
        }
        None => {
            let mut meter_ids = BTreeSet::new();
            meter_ids.insert(dimension.rate_code.clone());
            *slot = Some(LicenseComponent {
                core_hourly,
                meter_ids,
            });
        }
    }
    Ok(())
}

fn collapse_compute_offers(
    offers: BTreeMap<OfferKey, ComputeAccumulator>,
) -> Result<(BTreeMap<ComputeKey, ComputeComponent>, BTreeSet<ComputeKey>), RdsNormalizationError> {
    let mut compute = BTreeMap::<ComputeKey, ComputeComponent>::new();
    let mut conflicts = BTreeSet::new();
    for offer in offers.into_values() {
        let effective_hourly = offer.recurring_hourly
            + offer.upfront / commercial_term_hours(&offer.key.commercial_term)?;
        let key = offer.key;
        if conflicts.contains(&key) {
            continue;
        }
        let Some(component) = compute.get_mut(&key) else {
            compute.insert(
                key,
                ComputeComponent {
                    source_vcpu: offer.source_vcpu,
                    memory_gb: offer.memory_gb,
                    effective_hourly,
                    meter_ids: offer.meter_ids,
                    source: offer.source,
                },
            );
            continue;
        };
        if component.source_vcpu != offer.source_vcpu
            || component.memory_gb != offer.memory_gb
            || component.source != offer.source
        {
            return Err(RdsNormalizationError::ConflictingComponent);
        }
        if component.effective_hourly != effective_hourly {
            compute.remove(&key);
            conflicts.insert(key);
        } else {
            component.meter_ids.extend(offer.meter_ids);
        }
    }
    Ok((compute, conflicts))
}

fn commercial_term(dimension: &RawDimension) -> Result<String, RdsNormalizationError> {
    match dimension.term_type.as_str() {
        "OnDemand" => Ok("on-demand".to_owned()),
        "Reserved" => {
            let lease = match required(dimension.lease_contract_length.as_deref())? {
                "1yr" => "1yr",
                "3yr" => "3yr",
                _ => return Err(RdsNormalizationError::InvalidCommercialTerm),
            };
            let purchase = normalize_token(required(dimension.purchase_option.as_deref())?)?;
            let offering = normalize_token(required(dimension.offering_class.as_deref())?)?;
            Ok(format!("reserved-{lease}-{offering}-{purchase}"))
        }
        _ => Err(RdsNormalizationError::InvalidCommercialTerm),
    }
}

fn commercial_term_hours(term: &str) -> Result<Decimal, RdsNormalizationError> {
    if term == "on-demand" {
        return Ok(Decimal::ONE);
    }
    if term.starts_with("reserved-1yr-") {
        return Ok(Decimal::from(8_760_u32));
    }
    if term.starts_with("reserved-3yr-") {
        return Ok(Decimal::from(26_280_u32));
    }
    Err(RdsNormalizationError::InvalidCommercialTerm)
}

fn normalize_token(value: &str) -> Result<String, RdsNormalizationError> {
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
        return Err(RdsNormalizationError::InvalidCommercialTerm);
    }
    Ok(normalized)
}

fn parse_deployment(value: &str) -> Result<Deployment, RdsNormalizationError> {
    if value == "Single-AZ" {
        Ok(Deployment::SingleAz)
    } else if value.starts_with("Multi-AZ") {
        Ok(Deployment::MultiAz)
    } else {
        Err(RdsNormalizationError::InvalidValue)
    }
}

fn storage_class(dimension: &RawDimension) -> Result<String, RdsNormalizationError> {
    if let Some(name) = dimension
        .volume_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return normalize_token(name).map_err(|_| RdsNormalizationError::InvalidValue);
    }
    match dimension.volume_type.as_deref() {
        Some("General Purpose") | Some("General Purpose (SSD)") => Ok("gp2".to_owned()),
        Some("General Purpose-GP3") | Some("General Purpose-GP3 SSD") => Ok("gp3".to_owned()),
        Some("Provisioned IOPS") | Some("Provisioned IOPS (SSD)") => Ok("io1".to_owned()),
        Some("Provisioned IOPS-IO2") => Ok("io2".to_owned()),
        Some("Magnetic") => Ok("magnetic".to_owned()),
        _ => Err(RdsNormalizationError::InvalidValue),
    }
}

fn parse_memory(value: &str) -> Result<DecimalValue, RdsNormalizationError> {
    let raw = value
        .strip_suffix(" GiB")
        .ok_or(RdsNormalizationError::InvalidValue)?;
    let memory = Decimal::from_str(raw).map_err(|_| RdsNormalizationError::InvalidValue)?;
    if memory <= Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    Ok(DecimalValue(memory))
}

fn parse_nonnegative_decimal(value: &serde_json::Value) -> Result<Decimal, RdsNormalizationError> {
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    let value = Decimal::from_str(&raw).map_err(|_| RdsNormalizationError::InvalidValue)?;
    if value < Decimal::ZERO {
        return Err(RdsNormalizationError::InvalidValue);
    }
    Ok(value)
}

fn required(value: Option<&str>) -> Result<&str, RdsNormalizationError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or(RdsNormalizationError::InvalidValue)
}

fn license_or_fallback(
    component: Option<LicenseComponent>,
    fallback: Decimal,
    edition: &str,
    warnings: &mut Vec<String>,
) -> LicenseComponent {
    component.unwrap_or_else(|| {
        warnings.push(format!(
            "RDS {edition} OCPU license meter is unavailable; using the workbook-compatible fallback of {fallback} USD per source vCPU-hour."
        ));
        LicenseComponent {
            core_hourly: fallback,
            meter_ids: BTreeSet::new(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_raw_offer_into_selected_sql_server_dimensions() {
        let projected = project_rds_offer(std::io::Cursor::new(raw_rds_offer()), "AmazonRDS")
            .expect("project raw RDS offer");

        assert_eq!(projected.source_version, "20260806022930");
        assert_eq!(projected.effective_at, "2026-08-06T02:29:30Z");
        let normalized = normalize_rds_leaves(
            context(),
            &[RdsLeafPayload {
                source_url: "https://pricing.us-east-1.amazonaws.com/rds.json",
                source_version: Some(&projected.source_version),
                effective_at: Some(&projected.effective_at),
                body: &projected.body,
            }],
        )
        .expect("normalize projected RDS offer");

        assert_eq!(normalized.records.len(), 1);
        assert_eq!(normalized.warnings.len(), 2);
        assert_eq!(
            normalized.records[0]
                .rate
                .effective_compute_hourly
                .to_string(),
            "5.104"
        );
        assert_eq!(
            normalized.records[0]
                .rate
                .storage_monthly_per_gb
                .to_string(),
            "0.127"
        );
        assert_eq!(normalized.records[0].provenance.meter_ids.len(), 2);
    }

    #[test]
    fn ignores_unselected_products_without_product_family() {
        let mut offer: serde_json::Value =
            serde_json::from_slice(&raw_rds_offer()).expect("parse raw RDS offer");
        offer["products"]["ignored-sku"]
            .as_object_mut()
            .expect("ignored product")
            .remove("productFamily");
        let body = serde_json::to_vec(&offer).expect("serialize raw RDS offer");

        let projected = project_rds_offer(std::io::Cursor::new(body), "AmazonRDS")
            .expect("ignore product without family");

        assert_eq!(projected.source_version, "20260806022930");
    }

    #[test]
    fn normalizes_byom_compute_storage_and_ocpu_license_rates() {
        let rds = leaf(
            "AmazonRDS",
            &[
                compute(
                    "standard-sku",
                    "base-standard",
                    "Standard",
                    "Hrs",
                    "5.104",
                    None,
                ),
                compute(
                    "enterprise-sku",
                    "base-enterprise",
                    "Enterprise",
                    "Hrs",
                    "5.104",
                    None,
                ),
                storage("gp3-standard", "Standard", "0.127"),
                storage("gp3-enterprise", "Enterprise", "0.127"),
            ],
        );
        let licenses = leaf(
            "AmazonRDSOCPULicenseFees",
            &[
                license("standard-license", "Standard", "0.12"),
                license("enterprise-license", "Enterprise", "0.375"),
            ],
        );

        let normalized = normalize_rds_leaves(
            context(),
            &[payload(&rds, "AmazonRDS"), payload(&licenses, "OCPU")],
        )
        .expect("normalize selected RDS leaves");

        assert_eq!(normalized.records.len(), 1);
        assert!(normalized.warnings.is_empty());
        let record = &normalized.records[0];
        assert_eq!(
            record.stable_key,
            "eu-west-1|db.m6i.8xlarge|single-az|on-demand|gp3"
        );
        assert_eq!(record.rate.effective_compute_hourly.to_string(), "5.104");
        assert_eq!(record.rate.storage_monthly_per_gb.to_string(), "0.127");
        assert_eq!(record.rate.standard_license_core_hourly.to_string(), "0.12");
        assert_eq!(
            record.rate.enterprise_license_core_hourly.to_string(),
            "0.375"
        );
        assert_eq!(record.provenance.meter_ids.len(), 6);
    }

    #[test]
    fn amortizes_reserved_upfront_and_uses_explicit_license_fallbacks() {
        let rds = leaf(
            "AmazonRDS",
            &[
                compute(
                    "reserved-sku",
                    "reserved-hourly",
                    "Standard",
                    "Hrs",
                    "2.00",
                    Some(("1yr", "All Upfront", "standard")),
                ),
                compute(
                    "reserved-sku",
                    "reserved-upfront",
                    "Standard",
                    "Quantity",
                    "8760.00",
                    Some(("1yr", "All Upfront", "standard")),
                ),
                storage("gp3", "Standard", "0.127"),
            ],
        );

        let normalized = normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")])
            .expect("normalize Reserved RDS leaf");

        assert_eq!(normalized.records.len(), 1);
        assert_eq!(normalized.warnings.len(), 2);
        assert_eq!(
            normalized.records[0].commercial_term,
            "reserved-1yr-standard-all-upfront"
        );
        assert_eq!(
            normalized.records[0]
                .rate
                .effective_compute_hourly
                .to_string(),
            "3.00"
        );
    }

    #[test]
    fn omits_compute_keys_with_conflicting_edition_specific_rates() {
        let mut unambiguous_compute = compute(
            "unambiguous-sku",
            "unambiguous-rate",
            "Standard",
            "Hrs",
            "4.20",
            None,
        );
        unambiguous_compute["instance_type"] = serde_json::json!("db.m7i.8xlarge");
        let rds = leaf(
            "AmazonRDS",
            &[
                compute(
                    "standard-sku",
                    "standard-rate",
                    "Standard",
                    "Hrs",
                    "5.10",
                    None,
                ),
                compute(
                    "enterprise-sku",
                    "enterprise-rate",
                    "Enterprise",
                    "Hrs",
                    "6.20",
                    None,
                ),
                unambiguous_compute,
                storage("gp3-standard", "Standard", "0.127"),
                storage("gp3-enterprise", "Enterprise", "0.127"),
            ],
        );

        let normalized = normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")])
            .expect("omit only the ambiguous compute key");

        assert_eq!(normalized.records.len(), 1);
        assert_eq!(
            normalized.records[0].stable_key,
            "eu-west-1|db.m7i.8xlarge|single-az|on-demand|gp3"
        );
        assert!(
            normalized
                .warnings
                .iter()
                .any(|warning| warning.contains("conflicting edition-specific compute rates"))
        );
    }

    #[test]
    fn rejects_structural_conflicts_for_the_same_compute_key() {
        let standard = compute(
            "standard-sku",
            "standard-rate",
            "Standard",
            "Hrs",
            "5.10",
            None,
        );
        let mut enterprise = compute(
            "enterprise-sku",
            "enterprise-rate",
            "Enterprise",
            "Hrs",
            "5.10",
            None,
        );
        enterprise["vcpu"] = serde_json::json!("64");
        let rds = leaf("AmazonRDS", &[standard, enterprise]);

        assert!(matches!(
            normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")]),
            Err(RdsNormalizationError::ConflictingComponent)
        ));
    }

    #[test]
    fn rejects_non_hourly_compute_dimensions() {
        let rds = leaf(
            "AmazonRDS",
            &[compute(
                "bad-sku", "bad-unit", "Standard", "Requests", "1.00", None,
            )],
        );

        assert!(matches!(
            normalize_rds_leaves(context(), &[payload(&rds, "AmazonRDS")]),
            Err(RdsNormalizationError::UnsupportedUnit)
        ));
    }

    fn context() -> RdsNormalizationContext<'static> {
        RdsNormalizationContext {
            region_code: "eu-west-1",
        }
    }

    fn payload<'a>(body: &'a [u8], version: &'static str) -> RdsLeafPayload<'a> {
        RdsLeafPayload {
            source_url: "https://example.invalid/rds-selected-leaf",
            source_version: Some(version),
            effective_at: Some("2026-01-01T00:00:00Z"),
            body,
        }
    }

    fn leaf(offer: &str, dimensions: &[serde_json::Value]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "source_offer_code": offer,
            "dimensions": dimensions
        }))
        .expect("serialize selected RDS leaf")
    }

    fn compute(
        sku: &str,
        rate_code: &str,
        edition: &str,
        unit: &str,
        price: &str,
        reserved: Option<(&str, &str, &str)>,
    ) -> serde_json::Value {
        let (term_type, lease, purchase, offering) = match reserved {
            Some((lease, purchase, offering)) => {
                ("Reserved", Some(lease), Some(purchase), Some(offering))
            }
            None => ("OnDemand", None, None, None),
        };
        serde_json::json!({
            "sku": sku,
            "product_family": "Database Instance",
            "database_engine": "SQL Server",
            "database_edition": edition,
            "license_model": "NA",
            "deployment_model": "Custom",
            "deployment_option": "Single-AZ",
            "instance_type": "db.m6i.8xlarge",
            "memory": "128 GiB",
            "vcpu": "32",
            "term_type": term_type,
            "offer_term_code": "offer",
            "lease_contract_length": lease,
            "purchase_option": purchase,
            "offering_class": offering,
            "rate_code": rate_code,
            "unit": unit,
            "price": price
        })
    }

    fn storage(rate_code: &str, edition: &str, price: &str) -> serde_json::Value {
        serde_json::json!({
            "sku": format!("storage-{edition}"),
            "product_family": "Database Storage",
            "database_engine": "SQL Server",
            "database_edition": edition,
            "deployment_option": "Single-AZ",
            "volume_name": "gp3",
            "volume_type": "General Purpose-GP3",
            "term_type": "OnDemand",
            "offer_term_code": "offer",
            "rate_code": rate_code,
            "unit": "GB-Mo",
            "price": price
        })
    }

    fn license(rate_code: &str, edition: &str, price: &str) -> serde_json::Value {
        serde_json::json!({
            "sku": format!("license-{edition}"),
            "product_family": "Optimized License",
            "database_edition": edition,
            "license_type": "SQLServer",
            "term_type": "OnDemand",
            "offer_term_code": "offer",
            "rate_code": rate_code,
            "unit": "vCPU-Hour",
            "price": price
        })
    }

    fn raw_rds_offer() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "formatVersion": "v1.0",
            "disclaimer": "synthetic public-pricing fixture",
            "offerCode": "AmazonRDS",
            "version": "20260806022930",
            "publicationDate": "2026-08-06T02:29:30Z",
            "products": {
                "compute-sku": {
                    "sku": "compute-sku",
                    "productFamily": "Database Instance",
                    "attributes": {
                        "databaseEngine": "SQL Server",
                        "databaseEdition": "Standard",
                        "licenseModel": "NA",
                        "deploymentModel": "Custom",
                        "deploymentOption": "Single-AZ",
                        "operation": "CreateDBInstance:0405",
                        "instanceType": "db.m6i.8xlarge",
                        "memory": "128 GiB",
                        "vcpu": "32"
                    }
                },
                "licensed-compute-sku": {
                    "sku": "licensed-compute-sku",
                    "productFamily": "Database Instance",
                    "attributes": {
                        "databaseEngine": "SQL Server",
                        "databaseEdition": "Standard",
                        "licenseModel": "NA",
                        "deploymentModel": "Custom",
                        "deploymentOption": "Single-AZ",
                        "operation": "CreateDBInstance:0402",
                        "instanceType": "db.m6i.8xlarge",
                        "memory": "128 GiB",
                        "vcpu": "32"
                    }
                },
                "storage-sku": {
                    "sku": "storage-sku",
                    "productFamily": "Database Storage",
                    "attributes": {
                        "databaseEngine": "SQL Server",
                        "databaseEdition": "Standard",
                        "deploymentOption": "Single-AZ",
                        "volumeName": "gp3",
                        "volumeType": "General Purpose-GP3"
                    }
                },
                "ignored-sku": {
                    "sku": "ignored-sku",
                    "productFamily": "Database Instance",
                    "attributes": {
                        "databaseEngine": "MySQL"
                    }
                }
            },
            "terms": {
                "OnDemand": {
                    "compute-sku": {
                        "compute-offer": {
                            "offerTermCode": "JRTCKXETXF",
                            "sku": "compute-sku",
                            "effectiveDate": "2026-08-01T00:00:00Z",
                            "priceDimensions": {
                                "compute.offer.dimension": {
                                    "rateCode": "compute.offer.dimension",
                                    "description": "synthetic compute",
                                    "beginRange": "0",
                                    "endRange": "Inf",
                                    "unit": "Hrs",
                                    "pricePerUnit": { "USD": "5.104" },
                                    "appliesTo": []
                                }
                            },
                            "termAttributes": {}
                        }
                    },
                    "licensed-compute-sku": {
                        "licensed-compute-offer": {
                            "offerTermCode": "JRTCKXETXF",
                            "sku": "licensed-compute-sku",
                            "effectiveDate": "2026-08-01T00:00:00Z",
                            "priceDimensions": {
                                "licensed.compute.offer.dimension": {
                                    "rateCode": "licensed.compute.offer.dimension",
                                    "description": "synthetic AWS-provided media compute",
                                    "beginRange": "0",
                                    "endRange": "Inf",
                                    "unit": "Hrs",
                                    "pricePerUnit": { "USD": "10.08" },
                                    "appliesTo": []
                                }
                            },
                            "termAttributes": {}
                        }
                    },
                    "storage-sku": {
                        "storage-offer": {
                            "offerTermCode": "JRTCKXETXF",
                            "sku": "storage-sku",
                            "effectiveDate": "2026-08-01T00:00:00Z",
                            "priceDimensions": {
                                "storage.offer.dimension": {
                                    "rateCode": "storage.offer.dimension",
                                    "description": "synthetic storage",
                                    "beginRange": "0",
                                    "endRange": "Inf",
                                    "unit": "GB-Mo",
                                    "pricePerUnit": { "USD": "0.127" },
                                    "appliesTo": []
                                }
                            },
                            "termAttributes": {}
                        }
                    },
                    "ignored-sku": {
                        "this": "subtree is skipped without deserialization"
                    }
                }
            },
            "attributesList": []
        }))
        .expect("serialize raw RDS offer")
    }
}
