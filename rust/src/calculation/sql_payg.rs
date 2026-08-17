use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::decimal::DecimalValue;

pub const RATE_SOURCE_URL: &str = "https://prices.azure.com/api/retail/prices";
pub const RATE_VERIFIED_ON: &str = "2026-08-07";
const MAX_ANNUAL_HOURS: u32 = 8_784;
const ENTERPRISE_RATE_MANTISSA: i64 = 375;
const STANDARD_RATE_MANTISSA: i64 = 100;
const RATE_SCALE: u32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct SqlPaygInput {
    pub enterprise_licensed_cores: u32,
    pub standard_licensed_cores: u32,
    pub software_assurance_annual_usd: DecimalValue,
    pub annual_hours: DecimalValue,
    pub applied_payg_discount: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlPaygOutcome {
    NoDiscountNeeded,
    DiscountRequired,
    FullDiscountRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(from = "SqlPaygAnalysisWire")]
pub struct SqlPaygAnalysis {
    pub enterprise_licensed_cores: u32,
    pub standard_licensed_cores: u32,
    pub software_assurance_annual_usd: DecimalValue,
    pub annual_hours: DecimalValue,
    pub enterprise_payg_usd_per_core_hour: DecimalValue,
    pub standard_payg_usd_per_core_hour: DecimalValue,
    pub payg_gross_annual_usd: DecimalValue,
    pub required_payg_discount: DecimalValue,
    pub payg_at_breakeven_usd: DecimalValue,
    pub applied_payg_discount: DecimalValue,
    pub payg_net_annual_usd: DecimalValue,
    pub annual_savings_usd: DecimalValue,
    pub outcome: SqlPaygOutcome,
    pub rate_source_url: String,
    pub rate_verified_on: String,
}

#[derive(Deserialize)]
struct SqlPaygAnalysisWire {
    enterprise_licensed_cores: u32,
    standard_licensed_cores: u32,
    software_assurance_annual_usd: DecimalValue,
    annual_hours: AnnualHoursWire,
    enterprise_payg_usd_per_core_hour: DecimalValue,
    standard_payg_usd_per_core_hour: DecimalValue,
    payg_gross_annual_usd: DecimalValue,
    required_payg_discount: DecimalValue,
    payg_at_breakeven_usd: DecimalValue,
    applied_payg_discount: Option<DecimalValue>,
    payg_net_annual_usd: Option<DecimalValue>,
    annual_savings_usd: Option<DecimalValue>,
    outcome: SqlPaygOutcome,
    rate_source_url: String,
    rate_verified_on: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AnnualHoursWire {
    Decimal(DecimalValue),
    LegacyInteger(u32),
}

impl From<AnnualHoursWire> for DecimalValue {
    fn from(value: AnnualHoursWire) -> Self {
        match value {
            AnnualHoursWire::Decimal(hours) => hours,
            AnnualHoursWire::LegacyInteger(hours) => Self(Decimal::from(hours)),
        }
    }
}

impl From<SqlPaygAnalysisWire> for SqlPaygAnalysis {
    fn from(value: SqlPaygAnalysisWire) -> Self {
        let payg_net_annual_usd = value
            .payg_net_annual_usd
            .unwrap_or(value.payg_gross_annual_usd);
        let annual_savings_usd = value.annual_savings_usd.unwrap_or(DecimalValue(
            value.software_assurance_annual_usd.0 - payg_net_annual_usd.0,
        ));
        Self {
            enterprise_licensed_cores: value.enterprise_licensed_cores,
            standard_licensed_cores: value.standard_licensed_cores,
            software_assurance_annual_usd: value.software_assurance_annual_usd,
            annual_hours: value.annual_hours.into(),
            enterprise_payg_usd_per_core_hour: value.enterprise_payg_usd_per_core_hour,
            standard_payg_usd_per_core_hour: value.standard_payg_usd_per_core_hour,
            payg_gross_annual_usd: value.payg_gross_annual_usd,
            required_payg_discount: value.required_payg_discount,
            payg_at_breakeven_usd: value.payg_at_breakeven_usd,
            applied_payg_discount: value.applied_payg_discount.unwrap_or(DecimalValue::ZERO),
            payg_net_annual_usd,
            annual_savings_usd,
            outcome: value.outcome,
            rate_source_url: value.rate_source_url,
            rate_verified_on: value.rate_verified_on,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SqlPaygError {
    #[error("at least one SQL Server licensed core is required")]
    NoLicensedCores,
    #[error("annual Software Assurance spend must not be negative")]
    NegativeSoftwareAssurance,
    #[error("annual usage hours must be between 0 and 8,784")]
    AnnualHoursOutOfRange,
    #[error("applied PAYG discount must be between 0 and 1")]
    AppliedDiscountOutOfRange,
}

pub fn calculate(input: SqlPaygInput) -> Result<SqlPaygAnalysis, SqlPaygError> {
    if input.enterprise_licensed_cores == 0 && input.standard_licensed_cores == 0 {
        return Err(SqlPaygError::NoLicensedCores);
    }
    if input.software_assurance_annual_usd.0 < Decimal::ZERO {
        return Err(SqlPaygError::NegativeSoftwareAssurance);
    }
    if input.annual_hours.0 < Decimal::ZERO
        || input.annual_hours.0 > Decimal::from(MAX_ANNUAL_HOURS)
    {
        return Err(SqlPaygError::AnnualHoursOutOfRange);
    }
    if !input.applied_payg_discount.is_percent() {
        return Err(SqlPaygError::AppliedDiscountOutOfRange);
    }

    let enterprise_rate = Decimal::new(ENTERPRISE_RATE_MANTISSA, RATE_SCALE);
    let standard_rate = Decimal::new(STANDARD_RATE_MANTISSA, RATE_SCALE);
    let payg_hourly = Decimal::from(input.enterprise_licensed_cores) * enterprise_rate
        + Decimal::from(input.standard_licensed_cores) * standard_rate;
    let payg_gross_annual = payg_hourly * input.annual_hours.0;
    let raw_discount = if payg_gross_annual.is_zero() {
        Decimal::ZERO
    } else {
        Decimal::ONE - input.software_assurance_annual_usd.0 / payg_gross_annual
    };
    let required_discount = raw_discount.max(Decimal::ZERO);
    let (outcome, payg_at_breakeven) = if raw_discount <= Decimal::ZERO {
        (SqlPaygOutcome::NoDiscountNeeded, payg_gross_annual)
    } else if required_discount == Decimal::ONE {
        (
            SqlPaygOutcome::FullDiscountRequired,
            input.software_assurance_annual_usd.0,
        )
    } else {
        (
            SqlPaygOutcome::DiscountRequired,
            input.software_assurance_annual_usd.0,
        )
    };
    let payg_net_annual = payg_gross_annual * (Decimal::ONE - input.applied_payg_discount.0);
    let annual_savings = input.software_assurance_annual_usd.0 - payg_net_annual;

    Ok(SqlPaygAnalysis {
        enterprise_licensed_cores: input.enterprise_licensed_cores,
        standard_licensed_cores: input.standard_licensed_cores,
        software_assurance_annual_usd: input.software_assurance_annual_usd,
        annual_hours: input.annual_hours,
        enterprise_payg_usd_per_core_hour: DecimalValue(enterprise_rate),
        standard_payg_usd_per_core_hour: DecimalValue(standard_rate),
        payg_gross_annual_usd: DecimalValue(payg_gross_annual),
        required_payg_discount: DecimalValue(required_discount),
        payg_at_breakeven_usd: DecimalValue(payg_at_breakeven),
        applied_payg_discount: input.applied_payg_discount,
        payg_net_annual_usd: DecimalValue(payg_net_annual),
        annual_savings_usd: DecimalValue(annual_savings),
        outcome,
        rate_source_url: RATE_SOURCE_URL.to_owned(),
        rate_verified_on: RATE_VERIFIED_ON.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(value: &str) -> DecimalValue {
        value.parse().expect("test decimal should be valid")
    }

    #[test]
    fn discount_equalizes_always_on_payg_with_annual_sa() {
        let analysis = calculate(SqlPaygInput {
            enterprise_licensed_cores: 8,
            standard_licensed_cores: 16,
            software_assurance_annual_usd: decimal("20000"),
            annual_hours: decimal("8760"),
            applied_payg_discount: DecimalValue::ZERO,
        })
        .expect("input should be valid");

        assert_eq!(analysis.payg_gross_annual_usd, decimal("40296.000"));
        assert_eq!(analysis.outcome, SqlPaygOutcome::DiscountRequired);
        assert_eq!(analysis.payg_at_breakeven_usd, decimal("20000"));
        assert_eq!(
            analysis.required_payg_discount,
            decimal("0.5036728211236847329759777645")
        );
    }

    #[test]
    fn no_discount_is_needed_when_sa_exceeds_payg() {
        let analysis = calculate(SqlPaygInput {
            enterprise_licensed_cores: 4,
            standard_licensed_cores: 0,
            software_assurance_annual_usd: decimal("20000"),
            annual_hours: decimal("8760"),
            applied_payg_discount: DecimalValue::ZERO,
        })
        .expect("input should be valid");

        assert_eq!(analysis.required_payg_discount, DecimalValue::ZERO);
        assert_eq!(analysis.outcome, SqlPaygOutcome::NoDiscountNeeded);
        assert_eq!(analysis.payg_at_breakeven_usd, decimal("13140.000"));
    }

    #[test]
    fn zero_sa_requires_a_full_discount() {
        let analysis = calculate(SqlPaygInput {
            enterprise_licensed_cores: 0,
            standard_licensed_cores: 4,
            software_assurance_annual_usd: DecimalValue::ZERO,
            annual_hours: decimal("8760"),
            applied_payg_discount: DecimalValue::ZERO,
        })
        .expect("input should be valid");

        assert_eq!(analysis.required_payg_discount, decimal("1"));
        assert_eq!(analysis.outcome, SqlPaygOutcome::FullDiscountRequired);
        assert_eq!(analysis.payg_at_breakeven_usd, DecimalValue::ZERO);
    }

    #[test]
    fn applies_monthly_utilization_and_discount_to_signed_savings() {
        let analysis = calculate(SqlPaygInput {
            enterprise_licensed_cores: 8,
            standard_licensed_cores: 16,
            software_assurance_annual_usd: decimal("20000"),
            annual_hours: decimal("1920"),
            applied_payg_discount: decimal("0.25"),
        })
        .expect("input should be valid");

        assert_eq!(analysis.annual_hours, decimal("1920"));
        assert_eq!(analysis.payg_gross_annual_usd, decimal("8832.000"));
        assert_eq!(analysis.payg_net_annual_usd, decimal("6624.00000"));
        assert_eq!(analysis.annual_savings_usd, decimal("13376.00000"));
    }

    #[test]
    fn reports_negative_savings_when_net_payg_exceeds_sa() {
        let analysis = calculate(SqlPaygInput {
            enterprise_licensed_cores: 8,
            standard_licensed_cores: 16,
            software_assurance_annual_usd: decimal("5000"),
            annual_hours: decimal("1920"),
            applied_payg_discount: decimal("0.25"),
        })
        .expect("input should be valid");

        assert_eq!(analysis.payg_net_annual_usd, decimal("6624.00000"));
        assert_eq!(analysis.annual_savings_usd, decimal("-1624.00000"));
    }

    #[test]
    fn deserializes_legacy_saved_analysis_with_derived_comparison_fields() {
        let analysis: SqlPaygAnalysis = serde_json::from_value(serde_json::json!({
            "enterprise_licensed_cores": 8,
            "standard_licensed_cores": 16,
            "software_assurance_annual_usd": "20000",
            "annual_hours": 8760,
            "enterprise_payg_usd_per_core_hour": "0.375",
            "standard_payg_usd_per_core_hour": "0.100",
            "payg_gross_annual_usd": "40296.000",
            "required_payg_discount": "0.5036728211236847329759777645",
            "payg_at_breakeven_usd": "20000",
            "outcome": "discount_required",
            "rate_source_url": RATE_SOURCE_URL,
            "rate_verified_on": RATE_VERIFIED_ON
        }))
        .expect("legacy analysis should remain readable");

        assert_eq!(analysis.annual_hours, decimal("8760"));
        assert_eq!(analysis.applied_payg_discount, DecimalValue::ZERO);
        assert_eq!(analysis.payg_net_annual_usd, decimal("40296.000"));
        assert_eq!(analysis.annual_savings_usd, decimal("-20296.000"));
    }

    #[test]
    fn rejects_an_empty_license_estate() {
        let error = calculate(SqlPaygInput {
            enterprise_licensed_cores: 0,
            standard_licensed_cores: 0,
            software_assurance_annual_usd: decimal("1"),
            annual_hours: decimal("8760"),
            applied_payg_discount: DecimalValue::ZERO,
        })
        .expect_err("empty estate should fail");

        assert_eq!(error, SqlPaygError::NoLicensedCores);
    }
}
