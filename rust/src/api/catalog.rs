use std::collections::{BTreeMap, BTreeSet};

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use serde::{Deserialize, Serialize};

use crate::{
    domain::resource::{EbsVolumeType, RdsDeployment},
    pricing::{
        provider::ResolutionStatus,
        snapshot::{AwsPriceSnapshot, SnapshotMetadata},
    },
    problem::Problem,
    state::AppState,
};

const REGIONS_INSTANCE: &str = "/api/v1/catalog/aws/regions";
const EC2_INSTANCE: &str = "/api/v1/catalog/aws/ec2/instances";
const RDS_INSTANCE: &str = "/api/v1/catalog/aws/rds/instances";
const RDS_OPTIONS_INSTANCE: &str = "/api/v1/catalog/aws/rds/options";
const EBS_INSTANCE: &str = "/api/v1/catalog/aws/ebs/types";

#[derive(Serialize)]
pub struct CatalogResponse<T> {
    status: ResolutionStatus,
    currency: String,
    retrieved_at: Option<String>,
    source_urls: Vec<String>,
    warnings: Vec<String>,
    items: T,
}

#[derive(Serialize)]
pub struct Region {
    code: String,
    label: String,
}

#[derive(Serialize)]
pub struct PurchaseOption {
    key: &'static str,
    label: &'static str,
    ahb: bool,
}

#[derive(Serialize)]
pub struct Ec2Instance {
    instance_type: String,
    source_vcpu: u32,
    memory_gib: crate::domain::decimal::DecimalValue,
}

#[derive(Serialize)]
pub struct RdsInstance {
    instance_type: String,
    source_vcpu: u32,
    memory_gib: crate::domain::decimal::DecimalValue,
}

#[derive(Serialize)]
pub struct RdsOption {
    deployment: RdsDeployment,
    commercial_term: String,
    storage_class: String,
}

#[derive(Serialize)]
pub struct EbsType {
    key: EbsVolumeType,
    label: &'static str,
    price_required: bool,
    pricing_available: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionQuery {
    region: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RdsOptionsQuery {
    region: String,
    instance_type: String,
    deployment: RdsDeployment,
}

pub async fn aws_regions(
    State(state): State<AppState>,
) -> Result<Json<CatalogResponse<Vec<Region>>>, Problem> {
    let snapshots = state
        .snapshots
        .list_latest_aws()
        .map_err(|_| Problem::internal(REGIONS_INSTANCE))?;
    if snapshots.is_empty() {
        return Ok(Json(unavailable(Vec::new())));
    }

    let items = snapshots
        .iter()
        .map(|snapshot| Region {
            code: snapshot.source_region.clone(),
            label: region_label(&snapshot.source_region).to_owned(),
        })
        .collect();
    Ok(Json(combined_catalog(&snapshots, items)))
}

pub async fn ec2_instances(
    State(state): State<AppState>,
    query: Result<Query<RegionQuery>, QueryRejection>,
) -> Result<Json<CatalogResponse<Vec<Ec2Instance>>>, Problem> {
    let snapshot = snapshot_for_region(
        &state,
        parse_query(query, EC2_INSTANCE)?.region,
        EC2_INSTANCE,
    )?;
    let Some(snapshot) = snapshot else {
        return Ok(Json(unavailable(Vec::new())));
    };
    let items = snapshot
        .ec2_rates
        .iter()
        .map(|record| Ec2Instance {
            instance_type: record.instance_type.clone(),
            source_vcpu: record.rate.source_vcpu,
            memory_gib: record.rate.catalog_memory_gb,
        })
        .collect();
    Ok(Json(catalog_from_metadata(&snapshot.metadata, items)))
}

pub async fn rds_instances(
    State(state): State<AppState>,
    query: Result<Query<RegionQuery>, QueryRejection>,
) -> Result<Json<CatalogResponse<Vec<RdsInstance>>>, Problem> {
    let snapshot = snapshot_for_region(
        &state,
        parse_query(query, RDS_INSTANCE)?.region,
        RDS_INSTANCE,
    )?;
    let Some(snapshot) = snapshot else {
        return Ok(Json(unavailable(Vec::new())));
    };
    let mut unique = BTreeMap::new();
    for record in &snapshot.rds_rates {
        unique
            .entry(record.instance_type.clone())
            .or_insert(RdsInstance {
                instance_type: record.instance_type.clone(),
                source_vcpu: record.rate.source_vcpu,
                memory_gib: record.rate.catalog_memory_gb,
            });
    }
    Ok(Json(catalog_from_metadata(
        &snapshot.metadata,
        unique.into_values().collect(),
    )))
}

pub async fn rds_options(
    State(state): State<AppState>,
    query: Result<Query<RdsOptionsQuery>, QueryRejection>,
) -> Result<Json<CatalogResponse<Vec<RdsOption>>>, Problem> {
    let query = parse_query(query, RDS_OPTIONS_INSTANCE)?;
    let snapshot = snapshot_for_region(&state, query.region, RDS_OPTIONS_INSTANCE)?;
    let Some(snapshot) = snapshot else {
        return Ok(Json(unavailable(Vec::new())));
    };
    let mut unique = BTreeSet::new();
    for record in &snapshot.rds_rates {
        if record.instance_type == query.instance_type && record.deployment == query.deployment {
            unique.insert((record.commercial_term.clone(), record.storage_class.clone()));
        }
    }
    let items = unique
        .into_iter()
        .map(|(commercial_term, storage_class)| RdsOption {
            deployment: query.deployment,
            commercial_term,
            storage_class,
        })
        .collect();
    Ok(Json(catalog_from_metadata(&snapshot.metadata, items)))
}

pub async fn ebs_types(
    State(state): State<AppState>,
    query: Result<Query<RegionQuery>, QueryRejection>,
) -> Result<Json<CatalogResponse<Vec<EbsType>>>, Problem> {
    let snapshot = snapshot_for_region(
        &state,
        parse_query(query, EBS_INSTANCE)?.region,
        EBS_INSTANCE,
    )?;
    let Some(snapshot) = snapshot else {
        return Ok(Json(unavailable(Vec::new())));
    };
    let has_gp3 = snapshot
        .ebs_rates
        .iter()
        .any(|record| record.rate.volume_type == EbsVolumeType::Gp3);
    let has_io2 = snapshot
        .ebs_rates
        .iter()
        .any(|record| record.rate.volume_type == EbsVolumeType::Io2);
    let items = vec![
        EbsType {
            key: EbsVolumeType::Ephemeral,
            label: "Instance storage",
            price_required: false,
            pricing_available: true,
        },
        EbsType {
            key: EbsVolumeType::Gp3,
            label: "gp3",
            price_required: true,
            pricing_available: has_gp3,
        },
        EbsType {
            key: EbsVolumeType::Io2,
            label: "io2",
            price_required: true,
            pricing_available: has_io2,
        },
    ];
    Ok(Json(catalog_from_metadata(&snapshot.metadata, items)))
}

pub async fn purchase_options() -> Json<CatalogResponse<[PurchaseOption; 8]>> {
    Json(CatalogResponse {
        status: ResolutionStatus::Fresh,
        currency: "USD".to_owned(),
        retrieved_at: None,
        source_urls: Vec::new(),
        warnings: Vec::new(),
        items: [
            PurchaseOption {
                key: "payg",
                label: "PAYG",
                ahb: false,
            },
            PurchaseOption {
                key: "ahb",
                label: "PAYG + Azure Hybrid Benefit",
                ahb: true,
            },
            PurchaseOption {
                key: "one-year",
                label: "1-Year Reserved",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbone-year",
                label: "1-Year Reserved + AHB",
                ahb: true,
            },
            PurchaseOption {
                key: "three-year",
                label: "3-Year Reserved",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbthree-year",
                label: "3-Year Reserved + AHB",
                ahb: true,
            },
            PurchaseOption {
                key: "sv-one-year",
                label: "1-Year Savings Plan",
                ahb: false,
            },
            PurchaseOption {
                key: "ahbsv-one-year",
                label: "1-Year Savings Plan + AHB",
                ahb: true,
            },
        ],
    })
}

fn parse_query<T>(query: Result<Query<T>, QueryRejection>, instance: &str) -> Result<T, Problem> {
    query
        .map(|Query(value)| value)
        .map_err(|_| Problem::malformed_request(instance))
}

fn snapshot_for_region(
    state: &AppState,
    region: String,
    instance: &str,
) -> Result<Option<std::sync::Arc<AwsPriceSnapshot>>, Problem> {
    if region.trim().is_empty() {
        return Err(Problem::malformed_request(instance));
    }
    state
        .snapshots
        .find_aws("USD", &region)
        .map_err(|_| Problem::internal(instance))
}

fn catalog_from_metadata<T>(metadata: &SnapshotMetadata, items: T) -> CatalogResponse<T> {
    CatalogResponse {
        status: metadata.status,
        currency: metadata.currency.clone(),
        retrieved_at: Some(metadata.retrieved_at.clone()),
        source_urls: metadata.source_urls.clone(),
        warnings: metadata.warnings.clone(),
        items,
    }
}

fn combined_catalog<T>(
    snapshots: &[std::sync::Arc<AwsPriceSnapshot>],
    items: T,
) -> CatalogResponse<T> {
    let mut source_urls = snapshots
        .iter()
        .flat_map(|snapshot| snapshot.metadata.source_urls.iter().cloned())
        .collect::<Vec<_>>();
    source_urls.sort();
    source_urls.dedup();
    let mut warnings = snapshots
        .iter()
        .flat_map(|snapshot| snapshot.metadata.warnings.iter().cloned())
        .collect::<Vec<_>>();
    warnings.sort();
    warnings.dedup();
    CatalogResponse {
        status: snapshots
            .iter()
            .map(|snapshot| snapshot.metadata.status)
            .min_by_key(|status| status_rank(*status))
            .unwrap_or(ResolutionStatus::Unavailable),
        currency: "USD".to_owned(),
        retrieved_at: snapshots
            .iter()
            .map(|snapshot| snapshot.metadata.retrieved_at.clone())
            .min(),
        source_urls,
        warnings,
        items,
    }
}

fn unavailable<T>(items: T) -> CatalogResponse<T> {
    CatalogResponse {
        status: ResolutionStatus::Unavailable,
        currency: "USD".to_owned(),
        retrieved_at: None,
        source_urls: Vec::new(),
        warnings: vec!["No usable catalog snapshot exists for this scope.".to_owned()],
        items,
    }
}

fn status_rank(status: ResolutionStatus) -> u8 {
    match status {
        ResolutionStatus::Unavailable => 0,
        ResolutionStatus::Stale => 1,
        ResolutionStatus::Cached => 2,
        ResolutionStatus::Fresh => 3,
    }
}

fn region_label(region: &str) -> &str {
    match region {
        "eu-west-1" => "EU (Ireland)",
        _ => region,
    }
}
