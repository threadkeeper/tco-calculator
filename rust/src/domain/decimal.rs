use std::{fmt, str::FromStr};

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DecimalValue(#[serde(with = "rust_decimal::serde::str")] pub Decimal);

impl DecimalValue {
    pub const ZERO: Self = Self(Decimal::ZERO);

    pub fn is_percent(self) -> bool {
        self.0 >= Decimal::ZERO && self.0 <= Decimal::ONE
    }
}

impl fmt::Display for DecimalValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for DecimalValue {
    type Err = rust_decimal::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Decimal::from_str(value).map(Self)
    }
}
