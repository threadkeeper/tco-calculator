use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::decimal::DecimalValue;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectType {
    Ec2,
    Ec2Vm,
    Rds,
    OnPrem,
    SqlPayg,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum VmPurchaseOption {
    #[default]
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
    #[serde(rename = "sv-three-year")]
    SavingsThreeYear,
    #[serde(rename = "ahbsv-three-year")]
    AhbSavingsThreeYear,
}

impl VmPurchaseOption {
    pub const ALL: [Self; 10] = [
        Self::Payg,
        Self::Ahb,
        Self::OneYear,
        Self::AhbOneYear,
        Self::ThreeYear,
        Self::AhbThreeYear,
        Self::SavingsOneYear,
        Self::AhbSavingsOneYear,
        Self::SavingsThreeYear,
        Self::AhbSavingsThreeYear,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Payg => "payg",
            Self::Ahb => "ahb",
            Self::OneYear => "one-year",
            Self::AhbOneYear => "ahbone-year",
            Self::ThreeYear => "three-year",
            Self::AhbThreeYear => "ahbthree-year",
            Self::SavingsOneYear => "sv-one-year",
            Self::AhbSavingsOneYear => "ahbsv-one-year",
            Self::SavingsThreeYear => "sv-three-year",
            Self::AhbSavingsThreeYear => "ahbsv-three-year",
        }
    }

    pub const fn uses_ahb(self) -> bool {
        matches!(
            self,
            Self::Ahb
                | Self::AhbOneYear
                | Self::AhbThreeYear
                | Self::AhbSavingsOneYear
                | Self::AhbSavingsThreeYear
        )
    }
}

/// Fields every workload resource carries, including the non-SQL VM variant.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SharedResource {
    pub id: Uuid,
    pub workload_name: String,
    #[serde(default)]
    pub server_name: Option<String>,
    pub quantity: u32,
    pub source_ram_gb_per_instance: DecimalValue,
    pub annual_hours_per_instance: DecimalValue,
}

/// SQL-only inputs. Flattened alongside `SharedResource` so the persisted shape of the SQL
/// workloads is unchanged, while `ec2_vm` is structurally unable to carry them.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SqlWorkload {
    pub sql_edition: SqlEdition,
    pub license_basis: LicenseBasis,
    pub sql_data_gb_per_instance: DecimalValue,
    pub mi_purchase_option: PurchaseOption,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "source_type", rename_all = "snake_case")]
pub enum Resource {
    Ec2(Ec2Resource),
    Ec2Vm(Ec2VmResource),
    Rds(RdsResource),
    OnPrem(OnPremResource),
}

impl Resource {
    pub fn shared(&self) -> &SharedResource {
        match self {
            Self::Ec2(resource) => &resource.shared,
            Self::Ec2Vm(resource) => &resource.shared,
            Self::Rds(resource) => &resource.shared,
            Self::OnPrem(resource) => &resource.shared,
        }
    }

    /// `None` for the non-SQL VM workload.
    pub fn sql(&self) -> Option<&SqlWorkload> {
        match self {
            Self::Ec2(resource) => Some(&resource.sql),
            Self::Rds(resource) => Some(&resource.sql),
            Self::OnPrem(resource) => Some(&resource.sql),
            Self::Ec2Vm(_) => None,
        }
    }

    pub fn project_type(&self) -> ProjectType {
        match self {
            Self::Ec2(_) => ProjectType::Ec2,
            Self::Ec2Vm(_) => ProjectType::Ec2Vm,
            Self::Rds(_) => ProjectType::Rds,
            Self::OnPrem(_) => ProjectType::OnPrem,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ec2Resource {
    #[serde(flatten)]
    pub shared: SharedResource,
    #[serde(flatten)]
    pub sql: SqlWorkload,
    pub instance_type: String,
    pub volumes: Vec<EbsVolume>,
}

/// AWS EC2 Windows virtual machine without SQL Server. Carries no SQL edition, license basis,
/// SQL data size, or SQL MI purchase option.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ec2VmResource {
    #[serde(flatten)]
    pub shared: SharedResource,
    pub instance_type: String,
    #[serde(default)]
    pub vm_purchase_option: VmPurchaseOption,
    #[serde(default)]
    pub requirements: Ec2VmRequirements,
    pub volumes: Vec<VmVolume>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Ec2VmRequirements {
    #[serde(default)]
    pub burst_policy: VmBurstPolicy,
    #[serde(default)]
    pub instance_store_use: VmInstanceStoreUse,
    #[serde(default)]
    pub required_local_temp_disk_gb: Option<DecimalValue>,
    #[serde(default)]
    pub ephemeral_data_loss_acceptable: Option<bool>,
    #[serde(default)]
    pub high_frequency_requirement: VmHighFrequencyRequirement,
    #[serde(default)]
    pub requested_target_arm_sku: Option<String>,
}

impl Default for Ec2VmRequirements {
    fn default() -> Self {
        Self {
            burst_policy: VmBurstPolicy::NotApplicable,
            instance_store_use: VmInstanceStoreUse::NotUsed,
            required_local_temp_disk_gb: None,
            ephemeral_data_loss_acceptable: None,
            high_frequency_requirement: VmHighFrequencyRequirement::NotApplicable,
            requested_target_arm_sku: None,
        }
    }
}

impl Ec2VmRequirements {
    pub fn defaults_for(instance_type: &str) -> Self {
        Self {
            burst_policy: if instance_type.starts_with("t3.") {
                VmBurstPolicy::ConfirmedBurstCompatible
            } else {
                VmBurstPolicy::NotApplicable
            },
            high_frequency_requirement: if instance_type.starts_with("z1d.") {
                VmHighFrequencyRequirement::CapacityFitAccepted
            } else {
                VmHighFrequencyRequirement::NotApplicable
            },
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VmBurstPolicy {
    ConfirmedBurstCompatible,
    RequiresSustainedCpu,
    Unknown,
    #[default]
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VmInstanceStoreUse {
    Unknown,
    #[default]
    NotUsed,
    Used,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VmHighFrequencyRequirement {
    Required,
    Unknown,
    CapacityFitAccepted,
    #[default]
    NotApplicable,
}

/// A persistent EBS volume on a VM workload, mapped one-to-one to a target managed disk.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VmVolume {
    pub id: Uuid,
    pub label: String,
    pub aws_volume_id: Option<String>,
    pub volume_type: EbsVolumeType,
    pub role: VmDiskRole,
    pub capacity_gb: DecimalValue,
    pub provisioned_iops: Option<u64>,
    pub throughput_mibps: Option<DecimalValue>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VmDiskRole {
    Os,
    Data,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct RdsResource {
    #[serde(flatten)]
    pub shared: SharedResource,
    #[serde(flatten)]
    pub sql: SqlWorkload,
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
#[serde(deny_unknown_fields)]
pub struct OnPremResource {
    #[serde(flatten)]
    pub shared: SharedResource,
    #[serde(flatten)]
    pub sql: SqlWorkload,
    pub source_vcpu: u32,
    pub licensable_cores: u32,
    pub source_max_iops: u64,
    pub hardware_capex_usd: DecimalValue,
    pub depreciation_years: DecimalValue,
    pub average_power_kw_override: Option<DecimalValue>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn ec2_document() -> Value {
        json!({
            "source_type": "ec2",
            "id": "11111111-1111-1111-1111-111111111111",
            "workload_name": "Synthetic workload",
            "server_name": null,
            "quantity": 2,
            "sql_edition": "enterprise",
            "license_basis": "byol",
            "sql_data_gb_per_instance": "1024",
            "source_ram_gb_per_instance": "256",
            "annual_hours_per_instance": "8760",
            "mi_purchase_option": "ahb",
            "instance_type": "r6id.8xlarge",
            "volumes": [{
                "id": "22222222-2222-2222-2222-222222222222",
                "label": "Data",
                "aws_volume_id": null,
                "volume_type": "gp3",
                "capacity_gb": "1024",
                "provisioned_iops": 3000,
                "throughput_mibps": "125"
            }]
        })
    }

    /// Splitting `SqlWorkload` out of `SharedResource` must not change the persisted document
    /// shape: both flattened structs still read from and write to the same flat object.
    #[test]
    fn ec2_persisted_document_round_trips_unchanged() {
        let document = ec2_document();
        let resource: Resource =
            serde_json::from_value(document.clone()).expect("existing EC2 document deserializes");

        let Resource::Ec2(ec2) = &resource else {
            panic!("expected an EC2 resource");
        };
        assert_eq!(ec2.shared.quantity, 2);
        assert_eq!(ec2.sql.sql_edition, SqlEdition::Enterprise);
        assert_eq!(ec2.sql.license_basis, LicenseBasis::Byol);
        assert_eq!(ec2.sql.mi_purchase_option, PurchaseOption::Ahb);

        let serialized = serde_json::to_value(&resource).expect("EC2 resource serializes");
        assert_eq!(serialized, document);
    }

    #[test]
    fn sql_accessor_is_present_for_sql_workloads() {
        let resource: Resource =
            serde_json::from_value(ec2_document()).expect("existing EC2 document deserializes");
        assert!(resource.sql().is_some());
        assert_eq!(resource.project_type(), ProjectType::Ec2);
    }

    #[test]
    fn ec2_sql_document_rejects_vm_only_fields() {
        let mut document = ec2_document();
        document["requirements"] = json!({});
        assert!(
            serde_json::from_value::<Resource>(document.clone()).is_err(),
            "EC2 SQL payloads must reject VM requirements"
        );

        document
            .as_object_mut()
            .expect("resource object")
            .remove("requirements");
        document["volumes"][0]["role"] = json!("os");
        assert!(
            serde_json::from_value::<Resource>(document).is_err(),
            "EC2 SQL payloads must reject VM disk roles"
        );
    }

    #[test]
    fn other_sql_documents_reject_vm_only_fields() {
        let documents = [
            json!({
                "source_type": "rds",
                "id": "55555555-5555-5555-5555-555555555555",
                "workload_name": "Synthetic RDS workload",
                "server_name": null,
                "quantity": 1,
                "sql_edition": "standard",
                "license_basis": "license_included",
                "sql_data_gb_per_instance": "512",
                "source_ram_gb_per_instance": "32",
                "annual_hours_per_instance": "8760",
                "mi_purchase_option": "payg",
                "instance_type": "db.m6i.2xlarge",
                "deployment": "single_az",
                "commercial_term": "on_demand",
                "storage_class": "gp3",
                "source_max_iops": 3000
            }),
            json!({
                "source_type": "on_prem",
                "id": "66666666-6666-6666-6666-666666666666",
                "workload_name": "Synthetic on-premises workload",
                "server_name": null,
                "quantity": 1,
                "sql_edition": "enterprise",
                "license_basis": "byol",
                "sql_data_gb_per_instance": "1024",
                "source_ram_gb_per_instance": "128",
                "annual_hours_per_instance": "8760",
                "mi_purchase_option": "ahb",
                "source_vcpu": 16,
                "licensable_cores": 16,
                "source_max_iops": 5000,
                "hardware_capex_usd": "25000",
                "depreciation_years": "5",
                "average_power_kw_override": null
            }),
        ];

        for mut document in documents {
            serde_json::from_value::<Resource>(document.clone())
                .expect("existing SQL resource document deserializes");
            document["requirements"] = json!({});
            assert!(
                serde_json::from_value::<Resource>(document).is_err(),
                "SQL payloads must reject VM requirements"
            );
        }
    }

    /// The virtual machine workload is structurally unable to carry SQL inputs, so a draft can
    /// never fabricate an edition, license basis, or SQL data size for it.
    #[test]
    fn ec2_vm_document_carries_no_sql_fields() {
        let document = json!({
            "source_type": "ec2_vm",
            "id": "33333333-3333-3333-3333-333333333333",
            "workload_name": "VM1",
            "server_name": null,
            "quantity": 1,
            "source_ram_gb_per_instance": "384",
            "annual_hours_per_instance": "8760",
            "instance_type": "r6id.12xlarge",
            "volumes": [{
                "id": "44444444-4444-4444-4444-444444444444",
                "label": "OS",
                "aws_volume_id": null,
                "volume_type": "gp3",
                "role": "os",
                "capacity_gb": "1024",
                "provisioned_iops": 3000,
                "throughput_mibps": "125"
            }]
        });

        let resource: Resource =
            serde_json::from_value(document.clone()).expect("EC2 VM document deserializes");
        assert!(resource.sql().is_none());
        assert_eq!(resource.project_type(), ProjectType::Ec2Vm);
        let Resource::Ec2Vm(vm) = &resource else {
            panic!("expected an EC2 VM resource");
        };
        assert_eq!(vm.vm_purchase_option, VmPurchaseOption::Payg);
        assert_eq!(vm.requirements.burst_policy, VmBurstPolicy::NotApplicable);
        assert_eq!(
            vm.requirements.instance_store_use,
            VmInstanceStoreUse::NotUsed
        );
        assert_eq!(
            vm.requirements.high_frequency_requirement,
            VmHighFrequencyRequirement::NotApplicable
        );

        let serialized = serde_json::to_value(&resource).expect("EC2 VM resource serializes");
        let object = serialized.as_object().expect("resource object");
        assert!(object.contains_key("requirements"));
        assert_eq!(object.get("vm_purchase_option"), Some(&json!("payg")));
        for sql_field in [
            "sql_edition",
            "license_basis",
            "sql_data_gb_per_instance",
            "mi_purchase_option",
        ] {
            assert!(
                !object.contains_key(sql_field),
                "EC2 VM resources must not serialize {sql_field}"
            );
        }
    }

    #[test]
    fn ec2_vm_document_rejects_sql_fields() {
        let mut document = json!({
            "source_type": "ec2_vm",
            "id": "33333333-3333-3333-3333-333333333333",
            "workload_name": "VM1",
            "server_name": null,
            "quantity": 1,
            "source_ram_gb_per_instance": "8",
            "annual_hours_per_instance": "8760",
            "instance_type": "t3.large",
            "requirements": {},
            "volumes": [{
                "id": "44444444-4444-4444-4444-444444444444",
                "label": "OS",
                "aws_volume_id": null,
                "volume_type": "gp3",
                "role": "os",
                "capacity_gb": "1024",
                "provisioned_iops": 3000,
                "throughput_mibps": "125"
            }]
        });

        for field in [
            "sql_edition",
            "license_basis",
            "sql_data_gb_per_instance",
            "mi_purchase_option",
        ] {
            document[field] = json!("not-accepted");
            assert!(
                serde_json::from_value::<Resource>(document.clone()).is_err(),
                "EC2 VM payloads must reject {field}"
            );
            document
                .as_object_mut()
                .expect("resource object")
                .remove(field);
        }
    }
}
