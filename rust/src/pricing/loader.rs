use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, de::IgnoredAny};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::calculation::target_selector::CapabilityCatalog;
use crate::pricing::{
    aws_ebs::{
        EbsMeterMapPayload, EbsNormalization, EbsNormalizationContext, EbsNormalizationError,
        normalize_ebs_meter_map,
    },
    aws_ec2::{
        Ec2LeafPayload, Ec2Normalization, Ec2NormalizationContext, Ec2NormalizationError,
        normalize_ec2_leaves,
    },
    aws_rds::{
        RdsLeafPayload, RdsNormalization, RdsNormalizationContext, RdsNormalizationError,
        normalize_rds_leaves,
    },
    azure_sql_mi::{
        AzureMiConfiguration, AzureRetailPagePayload, AzureSqlMiNormalization,
        AzureSqlMiNormalizationContext, AzureSqlMiNormalizationError, normalize_azure_sql_mi,
    },
    http::{PricingHttpClient, PricingSource},
    live::{
        AZURE_RETAIL_MAX_PAGES, AwsRegionScope, PARSER_SCHEMA_VERSION, aws_region_index_url,
        aws_region_offer_url, aws_region_scope, azure_calculator_url, azure_region_scope,
        azure_retail_continuation_url, azure_retail_url, ebs_meter_map_url, ec2_leaf_url,
        ec2_metadata_url, ec2_selector_url,
    },
    provider::{ProviderError, ResolutionStatus},
    snapshot::{
        AwsPriceSnapshot, AzurePriceSnapshot, RateProvenance, SnapshotCreationMetadata,
        SnapshotError, utc_now_rfc3339,
    },
};

const AZURE_RETAIL_PAGE_SIZE: usize = 1_000;
const EC2_SOFTWARE_BRANCHES: [&str; 3] = ["NA", "SQL Std", "SQL Ent"];

#[derive(Deserialize)]
struct Ec2Metadata {
    manifest: Ec2Manifest,
    #[serde(rename = "regionAttributes")]
    region_attributes: Ec2RegionAttributes,
}

#[derive(Deserialize)]
struct Ec2RegionAttributes {
    #[serde(rename = "Location")]
    locations: Vec<String>,
}

#[derive(Deserialize)]
struct Ec2SelectorDocument {
    manifest: Ec2Manifest,
    aggregations: Vec<Ec2Aggregation>,
}

#[derive(Deserialize)]
struct Ec2Aggregation {
    selectors: Ec2Selectors,
    total_count: u64,
}

#[derive(Deserialize)]
struct Ec2Selectors {
    #[serde(rename = "TermType")]
    term_type: String,
    #[serde(rename = "Tenancy")]
    tenancy: String,
    #[serde(rename = "Operating System")]
    operating_system: String,
    #[serde(rename = "Pre Installed S/W")]
    preinstalled_software: String,
    #[serde(rename = "License Model")]
    license_model: String,
    #[serde(rename = "LeaseContractLength")]
    lease_contract_length: String,
    #[serde(rename = "PurchaseOption")]
    purchase_option: String,
    #[serde(rename = "OfferingClass")]
    offering_class: String,
    #[serde(rename = "Current Generation")]
    current_generation: String,
}

#[derive(Deserialize)]
struct Ec2ManifestEnvelope {
    manifest: Ec2Manifest,
}

#[derive(Deserialize, Eq, PartialEq)]
struct Ec2Manifest {
    #[serde(rename = "serviceId")]
    service_id: String,
    #[serde(rename = "accessType")]
    access_type: String,
    #[serde(rename = "esIndex")]
    source_version: Option<String>,
    #[serde(rename = "hawkFilePublicationDate")]
    published_at: String,
    #[serde(rename = "currencyCode")]
    currency_code: String,
    source: String,
}

#[derive(Deserialize)]
struct AwsRegionIndex {
    #[serde(rename = "formatVersion")]
    format_version: String,
    #[serde(rename = "publicationDate")]
    published_at: String,
    regions: BTreeMap<String, AwsRegionIndexEntry>,
}

#[derive(Deserialize)]
struct AwsRegionIndexEntry {
    #[serde(rename = "regionCode")]
    region_code: String,
    #[serde(rename = "currentVersionUrl")]
    current_version_url: String,
}

#[derive(Deserialize)]
struct AzureRetailPageEnvelope {
    #[serde(rename = "BillingCurrency")]
    billing_currency: String,
    #[serde(rename = "Items")]
    items: Vec<IgnoredAny>,
    #[serde(rename = "NextPageLink")]
    next_page_link: serde_json::Value,
    #[serde(rename = "Count")]
    count: usize,
}

#[derive(Clone)]
pub struct LivePricingLoader {
    source: Arc<dyn PricingSource>,
}

impl LivePricingLoader {
    pub fn new(http: PricingHttpClient) -> Self {
        Self {
            source: Arc::new(http),
        }
    }

    #[cfg(test)]
    fn with_source(source: Arc<dyn PricingSource>) -> Self {
        Self { source }
    }

    pub async fn load_aws_ebs(
        &self,
        source_region: &str,
    ) -> Result<EbsNormalization, ProviderError> {
        let scope = aws_region_scope(source_region)?;
        let source_url = ebs_meter_map_url()?;
        let payload = self.source.fetch(&source_url).await?;
        normalize_ebs_meter_map(
            EbsNormalizationContext {
                region_code: scope.code,
                location: scope.location,
            },
            EbsMeterMapPayload {
                source_url: &payload.source_url,
                body: &payload.body,
            },
        )
        .map_err(map_ebs_error)
    }

    pub async fn load_aws_snapshot(
        &self,
        source_region: &str,
    ) -> Result<AwsPriceSnapshot, ProviderError> {
        let (ec2, rds, ebs) = tokio::try_join!(
            self.load_aws_ec2(source_region),
            self.load_aws_rds(source_region),
            self.load_aws_ebs(source_region),
        )?;
        let mut warnings = ec2.warnings;
        warnings.extend(rds.warnings);
        warnings.extend(ebs.warnings);
        let provenance = ec2
            .records
            .iter()
            .map(|record| &record.provenance)
            .chain(rds.records.iter().map(|record| &record.provenance))
            .chain(ebs.records.iter().map(|record| &record.provenance));
        let (source_urls, source_published_at) = snapshot_sources(provenance);
        AwsPriceSnapshot::create(
            live_snapshot_metadata(warnings, source_urls, source_published_at)?,
            source_region,
            ec2.records,
            rds.records,
            ebs.records,
        )
        .map_err(map_snapshot_error)
    }

    pub async fn load_aws_ec2(
        &self,
        source_region: &str,
    ) -> Result<Ec2Normalization, ProviderError> {
        let scope = aws_region_scope(source_region)?;
        let metadata_url = ec2_metadata_url()?;
        let selector_url = ec2_selector_url(scope)?;
        let (metadata_payload, selector_payload) = tokio::try_join!(
            self.source.fetch(&metadata_url),
            self.source.fetch(&selector_url),
        )?;
        let manifest = project_ec2_metadata(&metadata_payload.body, scope.location)?;
        validate_ec2_selector(&selector_payload.body, &manifest)?;

        let compute_url = ec2_leaf_url(scope, EC2_SOFTWARE_BRANCHES[0])?;
        let standard_url = ec2_leaf_url(scope, EC2_SOFTWARE_BRANCHES[1])?;
        let enterprise_url = ec2_leaf_url(scope, EC2_SOFTWARE_BRANCHES[2])?;
        let (compute, standard, enterprise) = tokio::try_join!(
            self.source.fetch(&compute_url),
            self.source.fetch(&standard_url),
            self.source.fetch(&enterprise_url),
        )?;
        for leaf in [&compute, &standard, &enterprise] {
            validate_ec2_leaf(&leaf.body, &manifest)?;
        }

        let normalized = normalize_ec2_leaves(
            Ec2NormalizationContext {
                region_code: scope.code,
                location: scope.location,
                effective_at: Some(&manifest.published_at),
                source_version: manifest.source_version.as_deref(),
            },
            &[
                Ec2LeafPayload {
                    source_url: &compute.source_url,
                    body: &compute.body,
                },
                Ec2LeafPayload {
                    source_url: &standard.source_url,
                    body: &standard.body,
                },
                Ec2LeafPayload {
                    source_url: &enterprise.source_url,
                    body: &enterprise.body,
                },
            ],
        )
        .map_err(map_ec2_error)?;
        if normalized.records.is_empty() {
            return Err(ProviderError::NotFound);
        }
        Ok(normalized)
    }

    pub async fn load_aws_rds(
        &self,
        source_region: &str,
    ) -> Result<RdsNormalization, ProviderError> {
        let scope = aws_region_scope(source_region)?;
        let rds_index_url = aws_region_index_url("AmazonRDS")?;
        let license_index_url = aws_region_index_url("AmazonRDSOCPULicenseFees")?;
        let (rds_index_result, license_index_result) = tokio::join!(
            self.source.fetch(&rds_index_url),
            self.source.fetch(&license_index_url),
        );
        let rds_index = rds_index_result?;
        let rds_offer_url = resolve_rds_offer_url(&rds_index.body, scope, "AmazonRDS")?;
        let license_offer_url = match license_index_result {
            Ok(index) => {
                match resolve_rds_offer_url(&index.body, scope, "AmazonRDSOCPULicenseFees") {
                    Ok(url) => Some(url),
                    Err(ProviderError::NotFound) => None,
                    Err(error) => return Err(error),
                }
            }
            Err(ProviderError::NotFound) => None,
            Err(error) => return Err(error),
        };

        let rds = self
            .source
            .fetch_rds_offer(&rds_offer_url, "AmazonRDS")
            .await?;
        let license = match license_offer_url {
            Some(url) => match self
                .source
                .fetch_rds_offer(&url, "AmazonRDSOCPULicenseFees")
                .await
            {
                Ok(payload) => Some(payload),
                Err(ProviderError::NotFound) => None,
                Err(error) => return Err(error),
            },
            None => None,
        };
        let mut leaves = vec![RdsLeafPayload {
            source_url: &rds.source_url,
            source_version: rds.source_version.as_deref(),
            effective_at: rds.effective_at.as_deref(),
            body: &rds.body,
        }];
        if let Some(license) = license.as_ref() {
            leaves.push(RdsLeafPayload {
                source_url: &license.source_url,
                source_version: license.source_version.as_deref(),
                effective_at: license.effective_at.as_deref(),
                body: &license.body,
            });
        }
        let normalized = normalize_rds_leaves(
            RdsNormalizationContext {
                region_code: scope.code,
            },
            &leaves,
        )
        .map_err(map_rds_error)?;
        if normalized.records.is_empty() {
            return Err(ProviderError::NotFound);
        }
        Ok(normalized)
    }

    pub async fn load_azure_sql_mi(
        &self,
        target_region: &str,
        capabilities: &CapabilityCatalog,
    ) -> Result<AzureSqlMiNormalization, ProviderError> {
        let scope = azure_region_scope(target_region)?;
        let configurations = capabilities
            .candidates
            .iter()
            .filter(|candidate| candidate.azure_region == scope.arm_name)
            .map(|candidate| AzureMiConfiguration {
                configuration_key: &candidate.configuration_key,
                service_tier: candidate.service_tier,
                hardware_family: &candidate.hardware_family,
                vcores: candidate.vcores,
                zone_redundant: candidate.zone_redundant,
            })
            .collect::<Vec<_>>();
        if configurations.is_empty() {
            return Err(ProviderError::NotFound);
        }

        let calculator_url = azure_calculator_url()?;
        let retail_url = azure_retail_url(scope, "USD")?;
        let (calculator, first_retail_page) = tokio::try_join!(
            self.source.fetch(&calculator_url),
            self.source.fetch(&retail_url),
        )?;

        let mut next_url = project_azure_retail_page(&first_retail_page.body, scope, "USD")?;
        let mut seen_urls = BTreeSet::from([retail_url.to_string()]);
        let mut retail_pages = vec![first_retail_page];
        while let Some(url) = next_url {
            if retail_pages.len() >= AZURE_RETAIL_MAX_PAGES || !seen_urls.insert(url.to_string()) {
                return Err(ProviderError::SchemaChanged);
            }
            let page = self.source.fetch(&url).await?;
            next_url = project_azure_retail_page(&page.body, scope, "USD")?;
            retail_pages.push(page);
        }

        let retail_payloads = retail_pages
            .iter()
            .map(|page| AzureRetailPagePayload {
                source_url: &page.source_url,
                body: &page.body,
            })
            .collect::<Vec<_>>();
        let normalized = normalize_azure_sql_mi(
            AzureSqlMiNormalizationContext {
                target_region: scope.arm_name,
                calculator_region_slug: scope.calculator_slug,
                currency: "USD",
                calculator_source_url: &calculator.source_url,
                calculator_source_version: calculator.source_version.as_deref(),
                effective_at: calculator.effective_at.as_deref(),
            },
            &configurations,
            &calculator.body,
            &retail_payloads,
        )
        .map_err(map_azure_sql_mi_error)?;
        if normalized.records.is_empty() {
            return Err(ProviderError::NotFound);
        }
        Ok(normalized)
    }

    pub async fn load_azure_snapshot(
        &self,
        target_region: &str,
        capabilities: &CapabilityCatalog,
    ) -> Result<AzurePriceSnapshot, ProviderError> {
        let normalized = self.load_azure_sql_mi(target_region, capabilities).await?;
        let (_, source_published_at) =
            snapshot_sources(normalized.records.iter().map(|record| &record.provenance));
        AzurePriceSnapshot::create(
            live_snapshot_metadata(
                normalized.warnings,
                normalized.source_urls,
                source_published_at,
            )?,
            target_region,
            normalized.records,
        )
        .map_err(map_snapshot_error)
    }
}

fn snapshot_sources<'a>(
    provenance: impl Iterator<Item = &'a RateProvenance>,
) -> (Vec<String>, Option<String>) {
    let mut source_urls = BTreeSet::new();
    let mut source_published_at = None;
    for value in provenance {
        source_urls.insert(value.source_url.clone());
        if let Some(effective_at) = &value.effective_at
            && source_published_at
                .as_ref()
                .is_none_or(|current| effective_at > current)
        {
            source_published_at = Some(effective_at.clone());
        }
    }
    (source_urls.into_iter().collect(), source_published_at)
}

fn live_snapshot_metadata(
    warnings: Vec<String>,
    source_urls: Vec<String>,
    source_published_at: Option<String>,
) -> Result<SnapshotCreationMetadata, ProviderError> {
    Ok(SnapshotCreationMetadata {
        status: ResolutionStatus::Fresh,
        retrieved_at: utc_now_rfc3339().map_err(map_snapshot_error)?,
        source_published_at,
        currency: "USD".to_owned(),
        source_urls,
        parser_schema_version: PARSER_SCHEMA_VERSION.to_owned(),
        warnings,
    })
}

fn project_azure_retail_page(
    body: &[u8],
    scope: crate::pricing::live::AzureRegionScope,
    currency: &str,
) -> Result<Option<reqwest::Url>, ProviderError> {
    let page: AzureRetailPageEnvelope =
        serde_json::from_slice(body).map_err(|_| ProviderError::SchemaChanged)?;
    if page.billing_currency != currency
        || page.count != page.items.len()
        || page.count > AZURE_RETAIL_PAGE_SIZE
    {
        return Err(ProviderError::SchemaChanged);
    }
    match page.next_page_link {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(next_page_link) if page.count > 0 => {
            azure_retail_continuation_url(scope, currency, &next_page_link)
                .map(Some)
                .map_err(|_| ProviderError::SchemaChanged)
        }
        _ => Err(ProviderError::SchemaChanged),
    }
}

fn resolve_rds_offer_url(
    body: &[u8],
    scope: AwsRegionScope,
    offer_code: &str,
) -> Result<reqwest::Url, ProviderError> {
    let index: AwsRegionIndex =
        serde_json::from_slice(body).map_err(|_| ProviderError::SchemaChanged)?;
    if index.format_version != "v1.0"
        || OffsetDateTime::parse(&index.published_at, &Rfc3339).is_err()
    {
        return Err(ProviderError::SchemaChanged);
    }
    let entry = index
        .regions
        .get(scope.code)
        .ok_or(ProviderError::NotFound)?;
    if entry.region_code != scope.code {
        return Err(ProviderError::SchemaChanged);
    }
    aws_region_offer_url(scope, offer_code, &entry.current_version_url)
        .map_err(|_| ProviderError::SchemaChanged)
}

fn project_ec2_metadata(body: &[u8], location: &str) -> Result<Ec2Manifest, ProviderError> {
    let metadata: Ec2Metadata =
        serde_json::from_slice(body).map_err(|_| ProviderError::SchemaChanged)?;
    validate_ec2_manifest(&metadata.manifest, true)?;
    if !metadata
        .region_attributes
        .locations
        .iter()
        .any(|candidate| candidate == location)
    {
        return Err(ProviderError::NotFound);
    }
    Ok(metadata.manifest)
}

fn validate_ec2_selector(body: &[u8], expected: &Ec2Manifest) -> Result<(), ProviderError> {
    let document: Ec2SelectorDocument =
        serde_json::from_slice(body).map_err(|_| ProviderError::SchemaChanged)?;
    validate_ec2_manifest(&document.manifest, true)?;
    if !same_ec2_publication(expected, &document.manifest) {
        return Err(ProviderError::SchemaChanged);
    }

    let mut found = [false; EC2_SOFTWARE_BRANCHES.len()];
    for aggregation in document.aggregations {
        let selectors = aggregation.selectors;
        if selectors.term_type != "OnDemand"
            || selectors.tenancy != "Shared"
            || selectors.operating_system != "Windows"
            || selectors.license_model != "No License required"
            || !selectors.lease_contract_length.is_empty()
            || !selectors.purchase_option.is_empty()
            || !selectors.offering_class.is_empty()
            || selectors.current_generation != "Yes"
        {
            continue;
        }
        let Some(index) = EC2_SOFTWARE_BRANCHES
            .iter()
            .position(|branch| *branch == selectors.preinstalled_software)
        else {
            continue;
        };
        if found[index] {
            return Err(ProviderError::SchemaChanged);
        }
        if aggregation.total_count == 0 {
            return Err(ProviderError::NotFound);
        }
        found[index] = true;
    }
    if found.into_iter().all(|present| present) {
        Ok(())
    } else {
        Err(ProviderError::NotFound)
    }
}

fn validate_ec2_leaf(body: &[u8], expected: &Ec2Manifest) -> Result<(), ProviderError> {
    let envelope: Ec2ManifestEnvelope =
        serde_json::from_slice(body).map_err(|_| ProviderError::SchemaChanged)?;
    validate_ec2_manifest(&envelope.manifest, false)?;
    if same_ec2_publication(expected, &envelope.manifest) {
        Ok(())
    } else {
        Err(ProviderError::SchemaChanged)
    }
}

fn validate_ec2_manifest(
    manifest: &Ec2Manifest,
    require_source_version: bool,
) -> Result<(), ProviderError> {
    let source_version_invalid = match manifest.source_version.as_deref() {
        Some("") => true,
        None => require_source_version,
        Some(_) => false,
    };
    if manifest.service_id != "ec2"
        || manifest.access_type != "publish"
        || manifest.currency_code != "USD"
        || manifest.source != "ec2-calc"
        || source_version_invalid
        || OffsetDateTime::parse(&manifest.published_at, &Rfc3339).is_err()
    {
        return Err(ProviderError::SchemaChanged);
    }
    Ok(())
}

fn same_ec2_publication(expected: &Ec2Manifest, actual: &Ec2Manifest) -> bool {
    expected.service_id == actual.service_id
        && expected.access_type == actual.access_type
        && expected.published_at == actual.published_at
        && expected.currency_code == actual.currency_code
        && expected.source == actual.source
        && actual
            .source_version
            .as_ref()
            .is_none_or(|version| Some(version) == expected.source_version.as_ref())
}

fn map_ebs_error(error: EbsNormalizationError) -> ProviderError {
    match error {
        EbsNormalizationError::MissingLocation | EbsNormalizationError::MissingRequiredMeter => {
            ProviderError::NotFound
        }
        EbsNormalizationError::MalformedJson
        | EbsNormalizationError::UnsupportedMeterMap
        | EbsNormalizationError::UnsupportedOffer
        | EbsNormalizationError::InvalidValue
        | EbsNormalizationError::UnsupportedUnit
        | EbsNormalizationError::ConflictingComponent
        | EbsNormalizationError::InvalidIopsTiers => ProviderError::SchemaChanged,
    }
}

fn map_ec2_error(error: Ec2NormalizationError) -> ProviderError {
    match error {
        Ec2NormalizationError::MissingLocation => ProviderError::NotFound,
        Ec2NormalizationError::MalformedJson
        | Ec2NormalizationError::InvalidRateCode
        | Ec2NormalizationError::InvalidValue
        | Ec2NormalizationError::UnsupportedUnit
        | Ec2NormalizationError::ConflictingComponent => ProviderError::SchemaChanged,
    }
}

fn map_rds_error(error: RdsNormalizationError) -> ProviderError {
    match error {
        RdsNormalizationError::MalformedJson
        | RdsNormalizationError::UnsupportedOffer
        | RdsNormalizationError::InvalidValue
        | RdsNormalizationError::UnsupportedUnit
        | RdsNormalizationError::InvalidCommercialTerm
        | RdsNormalizationError::ConflictingComponent => ProviderError::SchemaChanged,
    }
}

fn map_azure_sql_mi_error(error: AzureSqlMiNormalizationError) -> ProviderError {
    match error {
        AzureSqlMiNormalizationError::InvalidScope => ProviderError::Unsupported,
        AzureSqlMiNormalizationError::MissingConfiguration
        | AzureSqlMiNormalizationError::MissingRegionPrice => ProviderError::NotFound,
        AzureSqlMiNormalizationError::MalformedJson
        | AzureSqlMiNormalizationError::DuplicateConfiguration
        | AzureSqlMiNormalizationError::MissingPurchaseOption
        | AzureSqlMiNormalizationError::InvalidReference
        | AzureSqlMiNormalizationError::MissingOffer
        | AzureSqlMiNormalizationError::InvalidValue
        | AzureSqlMiNormalizationError::ConflictingRetailRate => ProviderError::SchemaChanged,
    }
}

fn map_snapshot_error(_error: SnapshotError) -> ProviderError {
    ProviderError::SchemaChanged
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    use async_trait::async_trait;
    use reqwest::Url;

    use super::*;
    use crate::pricing::http::HttpPayload;

    struct RecordedSource {
        payloads: BTreeMap<String, HttpPayload>,
    }

    #[async_trait]
    impl PricingSource for RecordedSource {
        async fn fetch(&self, source_url: &Url) -> Result<HttpPayload, ProviderError> {
            Ok(self
                .payloads
                .get(source_url.as_str())
                .unwrap_or_else(|| panic!("unexpected pricing URL: {source_url}"))
                .clone())
        }
    }

    #[tokio::test]
    async fn loads_reviewed_ebs_meter_map_without_network() {
        let expected_url = ebs_meter_map_url().expect("EBS URL").to_string();
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource {
            payloads: BTreeMap::from([(
                expected_url.clone(),
                HttpPayload {
                    source_url: expected_url,
                    source_version: Some("test-etag".to_owned()),
                    effective_at: Some("Sun, 10 Aug 2026 11:35:09 GMT".to_owned()),
                    body: meter_map(),
                },
            )]),
        }));

        let normalized = loader
            .load_aws_ebs("eu-west-1")
            .await
            .expect("load EBS prices");

        assert!(normalized.warnings.is_empty());
        assert_eq!(normalized.records.len(), 2);
        assert_eq!(
            normalized.records[0]
                .rate
                .throughput_monthly_per_mibps
                .expect("gp3 throughput")
                .to_string(),
            "0.044"
        );
        assert_eq!(normalized.records[1].rate.iops_tiers.len(), 3);
    }

    #[tokio::test]
    async fn loads_selected_ec2_leaves_without_network() {
        let scope = aws_region_scope("eu-west-1").expect("AWS scope");
        let mut payloads = BTreeMap::new();
        insert_payload(
            &mut payloads,
            ec2_metadata_url().expect("metadata URL"),
            ec2_metadata(),
        );
        insert_payload(
            &mut payloads,
            ec2_selector_url(scope).expect("selector URL"),
            ec2_selector([1, 1, 1]),
        );
        for (software, price, rate_code) in [
            ("NA", "2.00", "base.offer.dimension"),
            ("SQL Std", "3.20", "standard.offer.dimension"),
            ("SQL Ent", "5.00", "enterprise.offer.dimension"),
        ] {
            insert_payload(
                &mut payloads,
                ec2_leaf_url(scope, software).expect("leaf URL"),
                ec2_leaf(software, price, rate_code),
            );
        }
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource { payloads }));

        let normalized = loader
            .load_aws_ec2("eu-west-1")
            .await
            .expect("load EC2 prices");

        assert!(normalized.warnings.is_empty());
        assert_eq!(normalized.records.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.instance_type, "m.test");
        assert_eq!(record.rate.compute_hourly.to_string(), "2.00");
        assert_eq!(
            record
                .rate
                .standard_license_hourly
                .expect("Standard license")
                .to_string(),
            "1.20"
        );
        assert_eq!(
            record
                .rate
                .enterprise_license_hourly
                .expect("Enterprise license")
                .to_string(),
            "3.00"
        );
        assert_eq!(
            record.provenance.source_version.as_deref(),
            Some("plc-ec2-usd-test")
        );
        assert_eq!(
            record.provenance.effective_at.as_deref(),
            Some("2026-08-10T11:35:09Z")
        );
    }

    #[tokio::test]
    async fn loads_streamed_rds_offers_without_network() {
        let scope = aws_region_scope("eu-west-1").expect("AWS scope");
        let rds_path = "/offers/v1.0/aws/AmazonRDS/20260806022930/eu-west-1/index.json";
        let license_path =
            "/offers/v1.0/aws/AmazonRDSOCPULicenseFees/20260529185918/eu-west-1/index.json";
        let mut payloads = BTreeMap::new();
        insert_payload(
            &mut payloads,
            aws_region_index_url("AmazonRDS").expect("RDS index URL"),
            rds_region_index(rds_path, "2026-08-06T02:29:30Z"),
        );
        insert_payload(
            &mut payloads,
            aws_region_index_url("AmazonRDSOCPULicenseFees").expect("license index URL"),
            rds_region_index(license_path, "2026-05-29T18:59:18Z"),
        );
        insert_payload(
            &mut payloads,
            aws_region_offer_url(scope, "AmazonRDS", rds_path).expect("RDS offer URL"),
            raw_rds_offer(),
        );
        insert_payload(
            &mut payloads,
            aws_region_offer_url(scope, "AmazonRDSOCPULicenseFees", license_path)
                .expect("license offer URL"),
            raw_rds_license_offer(),
        );
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource { payloads }));

        let normalized = loader
            .load_aws_rds("eu-west-1")
            .await
            .expect("load RDS prices");

        assert!(normalized.warnings.is_empty());
        assert_eq!(normalized.records.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.instance_type, "db.m6i.8xlarge");
        assert_eq!(record.rate.effective_compute_hourly.to_string(), "5.104");
        assert_eq!(record.rate.storage_monthly_per_gb.to_string(), "0.127");
        assert_eq!(record.rate.standard_license_core_hourly.to_string(), "0.12");
        assert_eq!(
            record.rate.enterprise_license_core_hourly.to_string(),
            "0.375"
        );
        assert_eq!(
            record.provenance.source_version.as_deref(),
            Some("20260806022930")
        );
    }

    #[tokio::test]
    async fn loads_paginated_azure_sql_mi_prices_without_network() {
        let scope = azure_region_scope("swedencentral").expect("Azure scope");
        let calculator_url = azure_calculator_url().expect("calculator URL");
        let retail_url = azure_retail_url(scope, "USD").expect("Retail Prices URL");
        let next_page_link = "https://prices.azure.com:443/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=1000";
        let next_url = azure_retail_continuation_url(scope, "USD", next_page_link)
            .expect("Retail Prices continuation URL");
        let mut payloads = BTreeMap::new();
        insert_payload(
            &mut payloads,
            calculator_url.clone(),
            azure_calculator_payload(),
        );
        let calculator_payload = payloads
            .get_mut(calculator_url.as_str())
            .expect("calculator payload");
        calculator_payload.source_version = Some("calculator-etag".to_owned());
        calculator_payload.effective_at = Some("Sun, 10 Aug 2026 11:35:09 GMT".to_owned());
        insert_payload(
            &mut payloads,
            retail_url,
            azure_retail_page(vec![azure_storage_item()], Some(next_page_link)),
        );
        insert_payload(
            &mut payloads,
            next_url,
            azure_retail_page(vec![azure_memory_item()], None),
        );
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource { payloads }));

        let normalized = loader
            .load_azure_sql_mi("swedencentral", &azure_capabilities())
            .await
            .expect("load Azure SQL MI prices");

        assert_eq!(normalized.records.len(), 8);
        assert_eq!(normalized.source_urls.len(), 3);
        assert_eq!(normalized.warnings.len(), 1);
        let record = &normalized.records[0];
        assert_eq!(record.rate.compute_hourly.to_string(), "5.632");
        assert_eq!(record.rate.storage_monthly_per_gb.to_string(), "0.13685");
        assert_eq!(
            record.rate.additional_memory_per_gb_hourly.to_string(),
            "0.011663"
        );
        assert_eq!(
            record.provenance.source_version.as_deref(),
            Some("calculator-etag")
        );

        let snapshot = loader
            .load_azure_snapshot("swedencentral", &azure_capabilities())
            .await
            .expect("create Azure SQL MI snapshot");
        assert_eq!(snapshot.metadata.status, ResolutionStatus::Fresh);
        assert_eq!(
            snapshot.metadata.parser_schema_version,
            PARSER_SCHEMA_VERSION
        );
        assert_eq!(snapshot.metadata.source_urls.len(), 3);
        assert_eq!(snapshot.mi_rates.len(), 8);
    }

    #[tokio::test]
    async fn rejects_an_azure_retail_pagination_cycle() {
        let scope = azure_region_scope("swedencentral").expect("Azure scope");
        let calculator_url = azure_calculator_url().expect("calculator URL");
        let retail_url = azure_retail_url(scope, "USD").expect("Retail Prices URL");
        let next_page_link = "https://prices.azure.com:443/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27swedencentral%27&$skip=1000";
        let next_url = azure_retail_continuation_url(scope, "USD", next_page_link)
            .expect("Retail Prices continuation URL");
        let mut payloads = BTreeMap::new();
        insert_payload(&mut payloads, calculator_url, azure_calculator_payload());
        insert_payload(
            &mut payloads,
            retail_url,
            azure_retail_page(vec![azure_storage_item()], Some(next_page_link)),
        );
        insert_payload(
            &mut payloads,
            next_url,
            azure_retail_page(vec![azure_memory_item()], Some(next_page_link)),
        );
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource { payloads }));

        assert_eq!(
            loader
                .load_azure_sql_mi("swedencentral", &azure_capabilities())
                .await
                .expect_err("pagination cycle must fail"),
            ProviderError::SchemaChanged
        );
    }

    #[tokio::test]
    async fn requires_a_reviewed_azure_configuration_before_fetching() {
        let loader = LivePricingLoader::with_source(Arc::new(RecordedSource {
            payloads: BTreeMap::new(),
        }));
        let mut capabilities = azure_capabilities();
        capabilities.candidates[0].azure_region = "eastus".to_owned();

        assert_eq!(
            loader
                .load_azure_sql_mi("swedencentral", &capabilities)
                .await
                .expect_err("unreviewed target must fail"),
            ProviderError::NotFound
        );
    }

    #[test]
    fn rejects_invalid_azure_retail_page_metadata() {
        let scope = azure_region_scope("swedencentral").expect("Azure scope");
        let malformed_count = serde_json::to_vec(&serde_json::json!({
            "BillingCurrency": "USD",
            "Items": [],
            "NextPageLink": null,
            "Count": 1
        }))
        .expect("serialize malformed count page");
        assert_eq!(
            project_azure_retail_page(&malformed_count, scope, "USD"),
            Err(ProviderError::SchemaChanged)
        );

        let scope_drift = azure_retail_page(
            vec![azure_storage_item()],
            Some(
                "https://prices.azure.com/api/retail/prices?currencyCode=USD&$filter=serviceName%20eq%20%27SQL%20Managed%20Instance%27%20and%20armRegionName%20eq%20%27eastus%27&$skip=1000",
            ),
        );
        assert_eq!(
            project_azure_retail_page(&scope_drift, scope, "USD"),
            Err(ProviderError::SchemaChanged)
        );
    }

    #[test]
    fn rejects_missing_or_inconsistent_ec2_selector_branches() {
        let manifest = project_ec2_metadata(&ec2_metadata(), "EU (Ireland)")
            .expect("project metadata manifest");

        assert_eq!(
            validate_ec2_selector(&ec2_selector([1, 0, 1]), &manifest),
            Err(ProviderError::NotFound)
        );
        let mut inconsistent = ec2_selector([1, 1, 1]);
        let document =
            serde_json::from_slice::<serde_json::Value>(&inconsistent).expect("selector document");
        let mut document = document;
        document["manifest"]["hawkFilePublicationDate"] =
            serde_json::Value::String("2026-08-11T11:35:09Z".to_owned());
        inconsistent = serde_json::to_vec(&document).expect("serialize inconsistent selector");
        assert_eq!(
            validate_ec2_selector(&inconsistent, &manifest),
            Err(ProviderError::SchemaChanged)
        );
    }

    fn insert_payload(
        payloads: &mut BTreeMap<String, HttpPayload>,
        source_url: Url,
        body: Vec<u8>,
    ) {
        let source_url = source_url.to_string();
        payloads.insert(
            source_url.clone(),
            HttpPayload {
                source_url,
                source_version: None,
                effective_at: None,
                body,
            },
        );
    }

    fn ec2_metadata() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "manifest": ec2_manifest(true),
            "regionAttributes": {
                "Location": ["EU (Ireland)"]
            }
        }))
        .expect("serialize EC2 metadata")
    }

    fn ec2_selector(counts: [u64; 3]) -> Vec<u8> {
        let aggregations = EC2_SOFTWARE_BRANCHES
            .iter()
            .zip(counts)
            .map(|(software, total_count)| {
                serde_json::json!({
                    "selectors": {
                        "TermType": "OnDemand",
                        "Tenancy": "Shared",
                        "Operating System": "Windows",
                        "Pre Installed S/W": software,
                        "License Model": "No License required",
                        "LeaseContractLength": "",
                        "PurchaseOption": "",
                        "OfferingClass": "",
                        "Current Generation": "Yes"
                    },
                    "total_count": total_count
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_vec(&serde_json::json!({
            "manifest": ec2_manifest(true),
            "aggregations": aggregations
        }))
        .expect("serialize EC2 selector")
    }

    fn ec2_leaf(software: &str, price: &str, rate_code: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "manifest": ec2_manifest(false),
            "regions": {
                "EU (Ireland)": {
                    "dimension": {
                        "rateCode": rate_code,
                        "price": price,
                        "Unit": "Hrs",
                        "Instance Type": "m.test",
                        "Memory": "64 GiB",
                        "vCPU": "8",
                        "Physical Processor": "Intel Xeon",
                        "Operating System": "Windows",
                        "Pre Installed S/W": software,
                        "TermType": "OnDemand",
                        "Tenancy": "Shared",
                        "Current Generation": "Yes",
                        "License Model": "No License required"
                    }
                }
            }
        }))
        .expect("serialize EC2 leaf")
    }

    fn ec2_manifest(include_source_version: bool) -> serde_json::Value {
        let mut manifest = serde_json::json!({
            "serviceId": "ec2",
            "accessType": "publish",
            "hawkFilePublicationDate": "2026-08-10T11:35:09Z",
            "ETLIngestionTriggerDate": "2026-08-10T11:35:09Z",
            "currencyCode": "USD",
            "source": "ec2-calc"
        });
        if include_source_version {
            manifest["esIndex"] = serde_json::Value::String("plc-ec2-usd-test".to_owned());
        }
        manifest
    }

    fn rds_region_index(current_version_url: &str, published_at: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "formatVersion": "v1.0",
            "disclaimer": "synthetic public-pricing fixture",
            "publicationDate": published_at,
            "regions": {
                "eu-west-1": {
                    "regionCode": "eu-west-1",
                    "currentVersionUrl": current_version_url
                }
            }
        }))
        .expect("serialize RDS region index")
    }

    fn raw_rds_offer() -> Vec<u8> {
        raw_offer(
            "AmazonRDS",
            "20260806022930",
            "2026-08-06T02:29:30Z",
            serde_json::json!({
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
                }
            }),
            serde_json::json!({
                "OnDemand": {
                    "compute-sku": raw_offer_term(
                        "compute-sku", "compute.rate", "Hrs", "5.104"
                    ),
                    "storage-sku": raw_offer_term(
                        "storage-sku", "storage.rate", "GB-Mo", "0.127"
                    )
                }
            }),
        )
    }

    fn raw_rds_license_offer() -> Vec<u8> {
        raw_offer(
            "AmazonRDSOCPULicenseFees",
            "20260529185918",
            "2026-05-29T18:59:18Z",
            serde_json::json!({
                "standard-license": {
                    "sku": "standard-license",
                    "productFamily": "Optimized License",
                    "attributes": {
                        "databaseEdition": "Standard",
                        "licenseType": "SQLServer"
                    }
                },
                "enterprise-license": {
                    "sku": "enterprise-license",
                    "productFamily": "Optimized License",
                    "attributes": {
                        "databaseEdition": "Enterprise",
                        "licenseType": "SQLServer"
                    }
                }
            }),
            serde_json::json!({
                "OnDemand": {
                    "standard-license": raw_offer_term(
                        "standard-license", "standard.license.rate", "vCPU-Hour", "0.12"
                    ),
                    "enterprise-license": raw_offer_term(
                        "enterprise-license", "enterprise.license.rate", "vCPU-Hour", "0.375"
                    )
                }
            }),
        )
    }

    fn raw_offer(
        offer_code: &str,
        version: &str,
        published_at: &str,
        products: serde_json::Value,
        terms: serde_json::Value,
    ) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "formatVersion": "v1.0",
            "disclaimer": "synthetic public-pricing fixture",
            "offerCode": offer_code,
            "version": version,
            "publicationDate": published_at,
            "products": products,
            "terms": terms,
            "attributesList": []
        }))
        .expect("serialize raw RDS offer")
    }

    fn raw_offer_term(sku: &str, rate_code: &str, unit: &str, price: &str) -> serde_json::Value {
        serde_json::json!({
            "offer": {
                "offerTermCode": "JRTCKXETXF",
                "sku": sku,
                "effectiveDate": "2026-08-01T00:00:00Z",
                "priceDimensions": {
                    rate_code: {
                        "rateCode": rate_code,
                        "description": "synthetic rate",
                        "beginRange": "0",
                        "endRange": "Inf",
                        "unit": unit,
                        "pricePerUnit": { "USD": price },
                        "appliesTo": []
                    }
                },
                "termAttributes": {}
            }
        })
    }

    fn azure_capabilities() -> CapabilityCatalog {
        serde_json::from_value(serde_json::json!({
            "schema_version": "1.0.0",
            "candidates": [{
                "configuration_key": "managed-vcore-next-gen-general-purpose-premium-series-32",
                "azure_region": "swedencentral",
                "service_tier": "next_generation_general_purpose",
                "hardware_family": "Premium Series",
                "vcores": 32,
                "zone_redundant": false,
                "included_memory_gb": "224",
                "supported_memory_gb": ["224", "256"],
                "storage_architecture": "Remote LRS",
                "maximum_storage_gb": "16384",
                "source_url": "https://learn.microsoft.com/azure/azure-sql/managed-instance/resource-limits",
                "reviewed_date": "2026-07-31"
            }]
        }))
        .expect("deserialize Azure capability fixture")
    }

    fn azure_calculator_payload() -> Vec<u8> {
        let references = serde_json::json!({
            "payg": ["compute--rate", "software--rate"],
            "ahb": "compute--rate",
            "one-year": ["compute--rate", "software--rate"],
            "ahbone-year": "compute--rate",
            "three-year": ["compute--rate", "software--rate"],
            "ahbthree-year": "compute--rate",
            "sv-one-year": ["compute--rate", "software--rate"],
            "ahbsv-one-year": "compute--rate"
        });
        serde_json::to_vec(&serde_json::json!({
            "skus": {
                "managed-vcore-next-gen-general-purpose-premium-series-32": references
            },
            "offers": {
                "compute": {
                    "offerType": "compute",
                    "prices": {
                        "rate": { "sweden-central": { "value": "5.632" } }
                    }
                },
                "software": {
                    "offerType": "software",
                    "prices": {
                        "rate": { "sweden-central": { "value": "3.198912" } }
                    }
                }
            }
        }))
        .expect("serialize Azure calculator payload")
    }

    fn azure_retail_page(items: Vec<serde_json::Value>, next_page_link: Option<&str>) -> Vec<u8> {
        let count = items.len();
        serde_json::to_vec(&serde_json::json!({
            "BillingCurrency": "USD",
            "CustomerEntityId": "Default",
            "CustomerEntityType": "Retail",
            "Items": items,
            "NextPageLink": next_page_link,
            "Count": count
        }))
        .expect("serialize Azure Retail Prices page")
    }

    fn azure_storage_item() -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    fn azure_memory_item() -> serde_json::Value {
        serde_json::json!({
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
        })
    }

    fn meter_map() -> Vec<u8> {
        let mut meters = BTreeMap::new();
        for (name, price, rate_code) in [
            ("Storage General Purpose gp3 GB Mo", "0.088", "gp3-capacity"),
            (
                "Provisioned EBS IOPS gp3 Volumes per IOPS Mo",
                "0.0055",
                "gp3-iops",
            ),
            (
                "Provisioned Throughput gp3 per GiBps mo",
                "45.056",
                "gp3-throughput",
            ),
            (
                "Storage Provisioned IOPS io2 GB month",
                "0.138",
                "io2-capacity",
            ),
            (
                "Provisioned EBS IOPS io2 Volumes per IOPS Mo",
                "0.072",
                "io2-tier-1",
            ),
            (
                "Provisioned EBS IOPS Tier 2 io2 Volumes per IOPS Mo",
                "0.0504",
                "io2-tier-2",
            ),
            (
                "Provisioned EBS IOPS Tier 3 io2 Volumes per IOPS Mo",
                "0.03528",
                "io2-tier-3",
            ),
        ] {
            meters.insert(
                name,
                serde_json::json!({
                    "rateCode": rate_code,
                    "price": price,
                    "RegionlessRateCode": format!("regionless-{rate_code}")
                }),
            );
        }
        serde_json::to_vec(&serde_json::json!({
            "manifest": {
                "serviceId": "ec2",
                "accessType": "publish",
                "esIndex": "plc-ec2-usd-test",
                "hawkFilePublicationDate": "2026-08-10T11:35:09Z",
                "currencyCode": "USD",
                "source": "ebs-calculator",
                "isMapped": "true"
            },
            "regions": {
                "EU (Ireland)": meters
            }
        }))
        .expect("serialize EBS meter map")
    }
}
