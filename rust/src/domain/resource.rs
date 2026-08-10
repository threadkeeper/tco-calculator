use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::decimal::DecimalValue;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Ec2,
    Rds,
    OnPrem,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SqlEdition {
    Standard,
    Enterprise,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseBasis {
    LicenseIncluded,
    Byol,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PurchaseOption {
    #[serde(rename = "payg")]
    Payg,
    #[serde(rename = "ahb")]
    Ahb,
    #[serde(rename = "one-year")]
    OneYear,
    #[serde(rename = "ahbone-year")]
    AhbOneYear,
    #[serde(rename = "three-year")]
    ThreeYear,
    #[serde(rename = "ahbthree-year")]
    AhbThreeYear,
    #[serde(rename = "sv-one-year")]
    SavingsOneYear,
    #[serde(rename = "ahbsv-one-year")]
    AhbSavingsOneYear,
}

impl PurchaseOption {
    pub const ALL: [Self; 8] = [
        Self::Payg,
        Self::Ahb,
        Self::OneYear,
        Self::AhbOneYear,
        Self::ThreeYear,
        Self::AhbThreeYear,
        Self::SavingsOneYear,
        Self::AhbSavingsOneYear,
    ];
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SharedResource {
    pub id: Uuid,
    pub workload_name: String,
    pub quantity: u32,
    pub sql_edition: SqlEdition,
    pub license_basis: LicenseBasis,
    pub sql_data_gb_per_instance: DecimalValue,
    pub source_ram_gb_per_instance: DecimalValue,
    pub annual_hours_per_instance: DecimalValue,
    pub mi_purchase_option: PurchaseOption,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum Resource {
    Ec2(Ec2Resource),
    Rds(RdsResource),
    OnPrem(OnPremResource),
}

impl Resource {
    pub fn shared(&self) -> &SharedResource {
        match self {
            Self::Ec2(resource) => &resource.shared,
            Self::Rds(resource) => &resource.shared,
            Self::OnPrem(resource) => &resource.shared,
        }
    }

    pub fn project_type(&self) -> ProjectType {
        match self {
            Self::Ec2(_) => ProjectType::Ec2,
            Self::Rds(_) => ProjectType::Rds,
            Self::OnPrem(_) => ProjectType::OnPrem,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ec2Resource {
    #[serde(flatten)]
    pub shared: SharedResource,
    pub instance_type: String,
    pub volumes: Vec<EbsVolume>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EbsVolume {
    pub id: Uuid,
    pub label: String,
    pub aws_volume_id: Option<String>,
    pub volume_type: EbsVolumeType,
    pub capacity_gb: DecimalValue,
    pub provisioned_iops: Option<u64>,
    pub throughput_mibps: Option<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EbsVolumeType {
    Gp3,
    Io2,
    Ephemeral,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RdsResource {
    #[serde(flatten)]
    pub shared: SharedResource,
    pub instance_type: String,
    pub deployment: RdsDeployment,
    pub commercial_term: String,
    pub storage_class: String,
    pub source_max_iops: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RdsDeployment {
    SingleAz,
    MultiAz,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OnPremResource {
    #[serde(flatten)]
    pub shared: SharedResource,
    pub source_vcpu: u32,
    pub licensable_cores: u32,
    pub source_max_iops: u64,
    pub hardware_capex_usd: DecimalValue,
    pub depreciation_years: DecimalValue,
    pub average_power_kw_override: Option<DecimalValue>,
}
