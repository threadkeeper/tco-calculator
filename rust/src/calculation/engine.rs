use std::sync::Arc;

use thiserror::Error;

use crate::domain::{project::ProjectSettings, resource::Resource};

use super::target_selector::CapabilityCatalog;

#[derive(Clone)]
pub struct CalculationEngine {
    capabilities: Arc<CapabilityCatalog>,
    formula_version: String,
}

pub struct CalculationInput<'a> {
    pub settings: &'a ProjectSettings,
    pub resources: &'a [Resource],
    pub aws_snapshot_id: Option<&'a str>,
    pub azure_snapshot_id: Option<&'a str>,
}

#[derive(Debug)]
pub struct CalculationRevision {
    pub formula_version: String,
}

#[derive(Debug, Error)]
pub enum CalculationError {
    #[error("capability catalog must contain at least one candidate")]
    EmptyCapabilityCatalog,
    #[error("calculation behavior is not implemented in pass 1")]
    NotImplemented,
}

impl CalculationEngine {
    pub fn new(
        capabilities: Arc<CapabilityCatalog>,
        formula_version: impl Into<String>,
    ) -> Result<Self, CalculationError> {
        if capabilities.candidates.is_empty() {
            return Err(CalculationError::EmptyCapabilityCatalog);
        }

        Ok(Self {
            capabilities,
            formula_version: formula_version.into(),
        })
    }

    pub fn calculate(
        &self,
        _input: CalculationInput<'_>,
    ) -> Result<CalculationRevision, CalculationError> {
        let _ = (&self.capabilities, &self.formula_version);
        Err(CalculationError::NotImplemented)
    }
}
