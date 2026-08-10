use serde::{Deserialize, Serialize};

use crate::domain::decimal::DecimalValue;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CapabilityCatalog {
    pub schema_version: String,
    pub candidates: Vec<TargetCandidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TargetCandidate {
    pub configuration_key: String,
    pub azure_region: String,
    pub service_tier: ServiceTier,
    pub hardware_family: String,
    pub vcores: u32,
    pub zone_redundant: bool,
    pub included_memory_gb: DecimalValue,
    pub supported_memory_gb: Vec<DecimalValue>,
    pub storage_architecture: String,
    pub maximum_storage_gb: Option<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceTier {
    NextGenerationGeneralPurpose,
    BusinessCritical,
}
