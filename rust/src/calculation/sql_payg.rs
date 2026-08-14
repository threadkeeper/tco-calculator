use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::decimal::DecimalValue;

pub const RATE_SOURCE_URL: &str = "https://prices.azure.com/api/retail/prices";
pub const RATE_VERIFIED_ON: &str = "2026-08-07";
const ANNUAL_HOURS: u32 = 8_760;
const ENTERPRISE_RATE_MANTISSA: i64 = 375;
const STANDARD_RATE_MANTISSA: i64 = 100;
const RATE_SCALE: u32 = 3;

#[derive(Clone, Copy, Debug)]
pub struct SqlPaygInput {
    pub enterprise_licensed_cores: u32,
    pub standard_licensed_cores: u32,
    pub software_assurance_annual_usd: DecimalValue,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlPaygOutcome {
    NoDiscountNeeded,
    DiscountRequired,
    FullDiscountRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SqlPaygAnalysis {
    pub enterprise_licensed_cores: u32,
    pub standard_licensed_cores: u32,
    pub software_assurance_annual_usd: DecimalValue,
    pub annual_hours: u32,
    pub enterprise_payg_usd_per_core_hour: DecimalValue,
    pub standard_payg_usd_per_core_hour: DecimalValue,
    pub payg_gross_annual_usd: DecimalValue,
    pub required_payg_discount: DecimalValue,
    pub payg_at_breakeven_usd: DecimalValue,
    pub outcome: SqlPaygOutcome,
    pub rate_source_url: String,
    pub rate_verified_on: String,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SqlPaygError {
    #[error("at least one SQL Server licensed core is required")]
    NoLicensedCores,
    #[error("annual Software Assurance spend must not be negative")]
    NegativeSoftwareAssurance,
}

pub fn calculate(input: SqlPaygInput) -> Result<SqlPaygAnalysis, SqlPaygError> {
    if input.enterprise_licensed_cores == 0 && input.standard_licensed_cores == 0 {
        return Err(SqlPaygError::NoLicensedCores);
    }
    if input.software_assurance_annual_usd.0 < Decimal::ZERO {
        return Err(SqlPaygError::NegativeSoftwareAssurance);
    }

    let enterprise_rate = Decimal::new(ENTERPRISE_RATE_MANTISSA, RATE_SCALE);
    let standard_rate = Decimal::new(STANDARD_RATE_MANTISSA, RATE_SCALE);
    let payg_hourly = Decimal::from(input.enterprise_licensed_cores) * enterprise_rate
        + Decimal::from(input.standard_licensed_cores) * standard_rate;
    let payg_gross_annual = payg_hourly * Decimal::from(ANNUAL_HOURS);
    let raw_discount = Decimal::ONE - input.software_assurance_annual_usd.0 / payg_gross_annual;
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

    Ok(SqlPaygAnalysis {
        enterprise_licensed_cores: input.enterprise_licensed_cores,
        standard_licensed_cores: input.standard_licensed_cores,
        software_assurance_annual_usd: input.software_assurance_annual_usd,
        annual_hours: ANNUAL_HOURS,
        enterprise_payg_usd_per_core_hour: DecimalValue(enterprise_rate),
        standard_payg_usd_per_core_hour: DecimalValue(standard_rate),
        payg_gross_annual_usd: DecimalValue(payg_gross_annual),
        required_payg_discount: DecimalValue(required_discount),
        payg_at_breakeven_usd: DecimalValue(payg_at_breakeven),
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
        })
        .expect("input should be valid");

        assert_eq!(analysis.required_payg_discount, decimal("1"));
        assert_eq!(analysis.outcome, SqlPaygOutcome::FullDiscountRequired);
        assert_eq!(analysis.payg_at_breakeven_usd, DecimalValue::ZERO);
    }

    #[test]
    fn rejects_an_empty_license_estate() {
        let error = calculate(SqlPaygInput {
            enterprise_licensed_cores: 0,
            standard_licensed_cores: 0,
            software_assurance_annual_usd: decimal("1"),
        })
        .expect_err("empty estate should fail");

        assert_eq!(error, SqlPaygError::NoLicensedCores);
    }
}
