use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Aws,
    Azure,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStatus {
    Fresh,
    Cached,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct PriceRequest {
    pub currency: String,
    pub source_region: Option<String>,
    pub target_region: String,
}

#[derive(Clone, Debug)]
pub struct PriceResolution {
    pub provider: Provider,
    pub status: ResolutionStatus,
    pub snapshot_id: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Error, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderError {
    #[error("requested price was not found")]
    NotFound,
    #[error("requested price scope is unsupported")]
    Unsupported,
    #[error("provider is temporarily unavailable")]
    TemporarilyUnavailable,
    #[error("provider response schema changed")]
    SchemaChanged,
}

#[async_trait]
pub trait PriceProvider: Send + Sync {
    fn provider(&self) -> Provider;

    async fn resolve(&self, request: PriceRequest) -> Result<PriceResolution, ProviderError>;
}
