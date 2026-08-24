//! Tool contract and dispatch.
//!
//! Each tool is declared once with a stable name, a concise description, a closed JSON Schema,
//! a risk class, and the single phase it belongs to. Tools call existing application services
//! directly so authorization and financial rules cannot diverge from the normal API.
//!
//! Model-visible schemas never contain identity, tenant, partition, ETag, credential,
//! endpoint, price-snapshot, or confirmation fields. Those arrive through
//! [`TurnContext`](super::context::TurnContext).

use std::collections::BTreeSet;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    calculation::{
        engine::{CalculationRevision, PortfolioTotals, PricingStatus},
        target_selector::MappingStatus,
    },
    domain::{
        decimal::DecimalValue,
        project::{EditableProject, ProjectSettings, SqlPaygSettings, ValidationIssue},
        resource::{
            EbsVolume, EbsVolumeType, Ec2Resource, Ec2VmResource, LicenseBasis, OnPremResource,
            ProjectType, PurchaseOption, RdsDeployment, RdsResource, Resource, SharedResource,
            SqlEdition, SqlWorkload, VmDiskRole, VmVolume,
        },
    },
    persistence::repository::RepositoryError,
    state::AppState,
};

use super::{
    budget::MAX_TOOL_RESULT_CHARS,
    context::{TurnContext, TurnPhase},
    help,
    model::ToolSchema,
};

/// Documented Azure OpenAI limit for a function description.
pub const MAX_TOOL_DESCRIPTION_CHARS: usize = 1_024;
pub const MAX_PROPOSAL_CHANGES: usize = 500;
pub const MAX_EXTRACTION_NOTES: usize = 100;
pub const MAX_EXTRACTION_NOTE_CHARS: usize = 500;
const EC2_CATALOG_RAM_UNAVAILABLE_UNCERTAINTY: &str = "Source RAM was not visible and authoritative AWS catalog memory was unavailable, so no project draft was created.";
/// The EC2 virtual machine workload is structurally non-SQL, so these inputs are removed from its
/// tool schema and rejected if a model sends them anyway.
const SQL_ONLY_FIELDS: &[&str] = &[
    "sql_edition",
    "license_basis",
    "sql_data_gb_per_instance",
    "mi_purchase_option",
];
/// AWS includes 3,000 IOPS with every gp3 volume, so assuming that baseline keeps an image
/// draft valid at no additional cost. io2 has no included tier and bills every provisioned
/// IOPS, so an unseen value falls back to the smallest amount AWS accepts rather than a guess.
const GP3_BASELINE_IOPS: u64 = 3_000;
const IO2_MINIMUM_IOPS: u64 = 100;
const GP3_ASSUMED_IOPS_UNCERTAINTY: &str = "Provisioned IOPS were not visible for at least one gp3 volume, so the draft uses the AWS gp3 baseline of 3,000 IOPS, which AWS includes at no additional cost.";
const IO2_ASSUMED_IOPS_UNCERTAINTY: &str = "Provisioned IOPS were not visible for at least one io2 volume, so the draft uses the AWS io2 minimum of 100 IOPS. Confirm the provisioned value because io2 bills every provisioned IOPS.";

/// Effect class of a tool, used by preflight to decide batching and confirmation rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolRisk {
    /// Reads reviewed or owner-scoped data. No state changes.
    Read,
    /// Changes only the unsaved browser draft and stays undoable.
    Draft,
    /// Writes owner-scoped persisted state.
    Persist,
    /// Deletes or shares owner-scoped state.
    Destructive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRequirement {
    Any,
    Selected,
    Unselected,
}

impl ToolRisk {
    pub fn is_mutating(self) -> bool {
        matches!(self, Self::Persist | Self::Destructive)
    }

    /// Whether an explicit user confirmation is required before execution.
    pub fn requires_confirmation(self) -> bool {
        self.is_mutating()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    /// Closed JSON Schema for the arguments. Runtime typed validation still applies.
    pub parameters: &'static str,
    pub phase: TurnPhase,
    pub risk: ToolRisk,
    pub project_requirement: ProjectRequirement,
}

impl ToolDefinition {
    pub fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name,
            description: self.description,
            parameters: self.parameters.to_owned(),
            strict: false,
        }
    }

    pub fn is_available(&self, context: &TurnContext) -> bool {
        let phase_available = self.phase == context.phase()
            || (self.phase == TurnPhase::ReadPlan && self.risk == ToolRisk::Read);
        let project_available = match self.project_requirement {
            ProjectRequirement::Any => true,
            ProjectRequirement::Selected => context.project().is_some(),
            ProjectRequirement::Unselected => context.project().is_none(),
        };
        phase_available && project_available
    }
}

const APPLICATION_HELP_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "required": ["question"],
  "properties": {
    "question": {
      "type": "string",
      "minLength": 1,
      "maxLength": 1000,
      "description": "The user's question about a visible field, button, state, or supported workflow."
    },
    "control_id": {
      "type": "string",
      "minLength": 1,
      "maxLength": 64,
      "description": "Optional stable control identifier returned by an earlier help result."
    }
  }
}"#;

const AGENT_CAPABILITIES_SCHEMA: &str = r#"{
    "type": "object",
    "additionalProperties": false,
    "properties": {}
}"#;

const CURRENT_PROJECT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {}
}"#;

const PROJECT_PATCH_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "patch": {
      "type": "object",
      "additionalProperties": false,
      "description": "Candidate changes to the selected project. Omitted members keep their current value.",
      "properties": {
        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
        "description": { "type": "string", "maxLength": 500 },
        "settings": {
          "type": "object",
          "description": "Complete replacement project settings using the documented project schema."
        },
        "resources": {
          "type": "array",
          "maxItems": 100,
          "items": { "type": "object" },
                    "description": "Complete replacement workload list using the documented project schema. For image-extracted sql_data_gb_per_instance, source_ram_gb_per_instance, or capacity_gb values, send the visible measurement as {\"value\":\"1\",\"unit\":\"tb\"}; supported units are gb, gib, tb, and tib. The host normalizes it to GB."
        }
      }
    }
  }
}"#;

const STAGE_PROJECT_PATCH_SCHEMA: &str = r#"{
    "type": "object",
    "additionalProperties": false,
    "required": ["patch", "omissions", "uncertainties"],
    "properties": {
        "patch": {
            "type": "object",
            "additionalProperties": false,
            "description": "Candidate changes to the selected project. Omitted members keep their current value.",
            "properties": {
                "name": { "type": "string", "minLength": 1, "maxLength": 100 },
                "description": { "type": "string", "maxLength": 500 },
                "settings": {
                    "type": "object",
                    "description": "Complete replacement project settings using the documented project schema."
                },
                "resources": {
                    "type": "array",
                    "maxItems": 100,
                    "items": { "type": "object" },
                    "description": "Complete replacement workload list using the documented project schema. For image-extracted sql_data_gb_per_instance, source_ram_gb_per_instance, or capacity_gb values, send the visible measurement as {\"value\":\"1\",\"unit\":\"tb\"}; supported units are gb, gib, tb, and tib. The host normalizes it to GB."
                }
            }
        },
        "omissions": {
            "type": "array",
            "maxItems": 100,
            "items": { "type": "string", "minLength": 1, "maxLength": 500 },
            "description": "Visible source values that could not be mapped to supported project fields."
        },
        "uncertainties": {
            "type": "array",
            "maxItems": 100,
            "items": { "type": "string", "minLength": 1, "maxLength": 500 },
            "description": "Candidate mappings that require user review because the source was ambiguous."
        }
    }
}"#;

const STAGE_NEW_PROJECT_DRAFT_SCHEMA: &str = r#"{
    "type": "object",
    "additionalProperties": false,
    "required": ["project_type", "omissions", "uncertainties"],
    "properties": {
        "project_type": {
            "type": "string",
            "enum": ["ec2", "rds", "on_prem", "sql_payg"],
            "description": "Source estate for the new unsaved project draft."
        },
        "name": { "type": "string", "minLength": 1, "maxLength": 100 },
        "description": { "type": ["string", "null"], "maxLength": 500 },
        "settings": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "aws_region": { "type": ["string", "null"] },
                "azure_region": { "type": "string" },
                "source_compute_discount": { "type": "string" },
                "source_license_discount": { "type": "string" },
                "source_storage_discount": { "type": "string" },
                "azure_compute_discount": { "type": "string" },
                "azure_license_discount": { "type": "string" },
                "azure_storage_discount": { "type": "string" },
                "selected_parity_adjustment": { "type": "string" },
                "default_annual_hours": { "type": "string" },
                "default_mi_purchase_option": {
                    "type": "string",
                    "enum": ["payg", "ahb", "one-year", "ahbone-year", "three-year", "ahbthree-year", "sv-one-year", "ahbsv-one-year"]
                },
                "enterprise_license_sa_usd_per_two_core_pack": {
                    "type": ["string", "null"],
                    "description": "Canonical USD decimal string without currency symbols, grouping separators, units, or whitespace."
                },
                "standard_license_sa_usd_per_two_core_pack": {
                    "type": ["string", "null"],
                    "description": "Canonical USD decimal string without currency symbols, grouping separators, units, or whitespace."
                },
                "remaining_coverage_months": { "type": ["integer", "null"], "enum": [12, 24, 36, null] },
                "electricity_rate_usd_per_kwh": {
                    "type": ["string", "null"],
                    "description": "Canonical USD decimal string without currency symbols, grouping separators, units, or whitespace."
                },
                "sql_payg": {
                    "type": ["object", "null"],
                    "additionalProperties": false,
                    "properties": {
                        "enterprise_licensed_cores": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100000,
                            "description": "Visible Enterprise, Enterprise Edition, or EE licensed/core count. Preserve it exactly as an unquoted JSON integer."
                        },
                        "standard_licensed_cores": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 100000,
                            "description": "Visible Standard, Standard Edition, SE, or context-qualified STE licensed/core count. Preserve it exactly as an unquoted JSON integer."
                        },
                        "software_assurance_annual_usd": {
                            "type": "string",
                            "description": "Visible Software Assurance, SA annual renewal, or SA annual spend as a canonical USD decimal string. For visible USD 50,000, send 50000."
                        }
                    },
                    "description": "Visible SQL Pay As You Go licensing inputs. Read each labeled value exactly, use only for sql_payg, never replace visible values with defaults, and report missing values as omissions."
                }
            }
        },
        "resources": {
            "type": "array",
            "maxItems": 100,
            "description": "Workloads visible in the request. Every source_type must equal project_type. EC2 fields: shared fields, instance_type, volumes. RDS fields: shared fields, instance_type, deployment, commercial_term, storage_class, source_max_iops. On-premises fields: shared fields, source_vcpu, licensable_cores, source_max_iops, hardware_capex_usd, depreciation_years, average_power_kw_override. Report visible unsupported fields as omissions. SQL PAYG must use an empty array. Omit or use an empty array for one host-defaulted starter workload.",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["source_type"],
                "properties": {
                    "source_type": { "type": "string", "enum": ["ec2", "rds", "on_prem"] },
                    "workload_name": { "type": "string", "minLength": 1, "maxLength": 160 },
                    "quantity": { "type": "integer", "minimum": 1, "maximum": 10000 },
                    "sql_edition": { "type": "string", "enum": ["standard", "enterprise"] },
                    "license_basis": { "type": "string", "enum": ["license_included", "byol"] },
                    "sql_data_gb_per_instance": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["value", "unit"],
                        "properties": {
                            "value": {
                                "type": "string",
                                "description": "Visible decimal value without grouping separators or whitespace. Preserve the visible number without converting it."
                            },
                            "unit": {
                                "type": "string",
                                "enum": ["gb", "gib", "tb", "tib"],
                                "description": "Visible storage unit, normalized only to this lowercase enum."
                            }
                        },
                        "description": "Visible SQL data capacity with its source unit. The host deterministically normalizes TB and TiB values to GB."
                    },
                    "source_ram_gb_per_instance": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["value", "unit"],
                        "properties": {
                            "value": {
                                "type": "string",
                                "description": "Visible decimal value without grouping separators or whitespace. Preserve the visible number without converting it."
                            },
                            "unit": {
                                "type": "string",
                                "enum": ["gb", "gib", "tb", "tib"],
                                "description": "Visible memory unit, normalized only to this lowercase enum."
                            }
                        },
                        "description": "Visible source RAM with its source unit. The host deterministically normalizes TB and TiB values to GB. For EC2, omit this field when RAM is not visible; the host pre-fills standard RAM from the selected instance type's regional AWS metadata."
                    },
                    "annual_hours_per_instance": {
                        "type": "string",
                        "description": "Canonical decimal string without grouping separators or units. For visible 6,240 hours, send 6240."
                    },
                    "mi_purchase_option": {
                        "type": "string",
                        "enum": ["payg", "ahb", "one-year", "ahbone-year", "three-year", "ahbthree-year", "sv-one-year", "ahbsv-one-year"]
                    },
                    "instance_type": {
                        "type": "string",
                        "description": "EC2 or RDS only. Never send for on-premises resources."
                    },
                    "volumes": {
                        "type": "array",
                        "maxItems": 50,
                        "description": "EC2 only. Never send for RDS or on-premises resources; report unsupported visible storage values as omissions.",
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {
                                "label": { "type": "string", "minLength": 1, "maxLength": 80 },
                                "aws_volume_id": { "type": ["string", "null"], "maxLength": 128 },
                                "volume_type": { "type": "string", "enum": ["gp3", "io2", "ephemeral"] },
                                "capacity_gb": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["value", "unit"],
                                    "properties": {
                                        "value": {
                                            "type": "string",
                                            "description": "Visible decimal value without grouping separators or whitespace. Preserve the visible number without converting it."
                                        },
                                        "unit": {
                                            "type": "string",
                                            "enum": ["gb", "gib", "tb", "tib"],
                                            "description": "Visible volume-capacity unit, normalized only to this lowercase enum."
                                        }
                                    },
                                    "description": "Visible EBS volume capacity with its source unit. The host deterministically normalizes TB and TiB values to GB."
                                },
                                "provisioned_iops": { "type": ["integer", "null"], "minimum": 0 },
                                "throughput_mibps": {
                                    "type": ["string", "null"],
                                    "description": "Canonical decimal string without grouping separators or units. For visible 500 MiB/s, send 500."
                                }
                            }
                        }
                    },
                    "deployment": {
                        "type": "string",
                        "enum": ["single_az", "multi_az"],
                        "description": "RDS only."
                    },
                    "commercial_term": {
                        "type": "string",
                        "description": "RDS only."
                    },
                    "storage_class": {
                        "type": "string",
                        "description": "RDS only."
                    },
                    "source_vcpu": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100000,
                        "description": "On-premises only. Use the visible vCPU or logical CPU count as an unquoted JSON integer. When only physical Processor cores or CPU cores is visible, use that exact count; never use quantity, RAM, utilization, or unrelated values. Report visible EC2 or RDS vCPU values as omissions."
                    },
                    "licensable_cores": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100000,
                        "description": "On-premises only. Use only a visible Licensable cores or SQL licensable cores value as an unquoted JSON integer and keep it distinct from source_vcpu."
                    },
                    "source_max_iops": {
                        "type": "integer",
                        "minimum": 0,
                        "maximum": 1000000000,
                        "description": "RDS or on-premises only. For EC2, put IOPS on the corresponding volume."
                    },
                    "hardware_capex_usd": {
                        "type": "string",
                        "description": "On-premises only. Canonical USD decimal string without currency symbols, grouping separators, units, or whitespace."
                    },
                    "depreciation_years": {
                        "type": "string",
                        "description": "On-premises only. Canonical decimal string without grouping separators or units."
                    },
                    "average_power_kw_override": {
                        "type": ["string", "null"],
                        "description": "On-premises only. Canonical decimal string without grouping separators or units. Do not convert watts or other power units."
                    }
                }
            }
        },
        "omissions": {
            "type": "array",
            "maxItems": 100,
            "items": { "type": "string", "minLength": 1, "maxLength": 500 }
        },
        "uncertainties": {
            "type": "array",
            "maxItems": 100,
            "items": { "type": "string", "minLength": 1, "maxLength": 500 }
        }
    }
}"#;

/// Every registered tool. Dispatch matches explicitly over this list.
pub const TOOLS: &[ToolDefinition] = &[
    ToolDefinition {
        name: "get_agent_capabilities",
        description: "Read the host-authored description of this agent's programming, available tools, action boundaries, selected-project state, image support, action history, and request-scoped memory. Use this for questions about abilities, tools, autonomy, or how the agent works.",
        parameters: AGENT_CAPABILITIES_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
        project_requirement: ProjectRequirement::Any,
    },
    ToolDefinition {
        name: "get_application_help",
        description: "Read the reviewed application help catalog for a field, button, state, or workflow. Returns product behaviour statements and the control identifiers they came from. This is the only approved source of product behaviour claims.",
        parameters: APPLICATION_HELP_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
        project_requirement: ProjectRequirement::Any,
    },
    ToolDefinition {
        name: "get_current_project",
        description: "Read the project the user currently has open. Returns its settings and workloads with names and provider identifiers removed. The project is chosen by the application, not by this call.",
        parameters: CURRENT_PROJECT_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
        project_requirement: ProjectRequirement::Selected,
    },
    ToolDefinition {
        name: "validate_project_patch",
        description: "Check candidate changes to the open project against server-side validation without saving anything. Returns field-level errors, or confirms the candidate project is valid.",
        parameters: PROJECT_PATCH_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
        project_requirement: ProjectRequirement::Selected,
    },
    ToolDefinition {
        name: "calculate_project_draft",
        description: "Run the authoritative server-side calculation for the open project with candidate changes applied, without saving anything. Returns deterministic totals and per-workload mapping and pricing status. Never calculate or estimate money yourself.",
        parameters: PROJECT_PATCH_SCHEMA,
        phase: TurnPhase::ReadPlan,
        risk: ToolRisk::Read,
        project_requirement: ProjectRequirement::Selected,
    },
    ToolDefinition {
        name: "stage_project_patch",
        description: "Stage validated changes to the open project with bounded omissions and uncertainties for an explicit user preview. An empty patch is allowed only when the report identifies data that could not be mapped. This never saves or changes the project.",
        parameters: STAGE_PROJECT_PATCH_SCHEMA,
        phase: TurnPhase::Propose,
        risk: ToolRisk::Draft,
        project_requirement: ProjectRequirement::Selected,
    },
    ToolDefinition {
        name: "stage_new_project_draft",
        description: "Stage a complete validated new project as an unsaved browser draft when no project is open. The host supplies deterministic defaults and identifiers. Report source omissions and uncertainties. This never saves the project.",
        parameters: STAGE_NEW_PROJECT_DRAFT_SCHEMA,
        phase: TurnPhase::Propose,
        risk: ToolRisk::Draft,
        project_requirement: ProjectRequirement::Unselected,
    },
];

pub fn find(name: &str) -> Option<&'static ToolDefinition> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// Tools exposed to the model for a phase. A model never sees a capability it cannot use.
pub fn schemas_for_context(context: &TurnContext) -> Vec<ToolSchema> {
    TOOLS
        .iter()
        .filter(|tool| tool.is_available(context))
        .map(|tool| {
            let mut schema = tool.schema();
            if tool.name == "stage_new_project_draft"
                && let Some(project_type) = context.classified_project_type()
            {
                schema.parameters = scoped_new_project_draft_schema(project_type)
                    .unwrap_or_else(|| tool.parameters.to_owned());
            }
            schema
        })
        .collect()
}

fn scoped_new_project_draft_schema(project_type: ProjectType) -> Option<String> {
    let mut schema: Value = serde_json::from_str(STAGE_NEW_PROJECT_DRAFT_SCHEMA).ok()?;
    let project_type_name = match project_type {
        ProjectType::Ec2 => "ec2",
        ProjectType::Ec2Vm => "ec2_vm",
        ProjectType::Rds => "rds",
        ProjectType::OnPrem => "on_prem",
        ProjectType::SqlPayg => "sql_payg",
    };
    *schema.pointer_mut("/properties/project_type/enum")? = json!([project_type_name]);

    let resources = schema
        .pointer_mut("/properties/resources")?
        .as_object_mut()?;
    if project_type == ProjectType::SqlPayg {
        resources.insert("maxItems".to_owned(), json!(0));
        resources.insert(
            "description".to_owned(),
            json!("SQL PAYG projects use settings.sql_payg and cannot contain workload resources."),
        );
        return serde_json::to_string(&schema).ok();
    }

    let properties = resources
        .get_mut("items")?
        .get_mut("properties")?
        .as_object_mut()?;
    properties
        .get_mut("source_type")?
        .as_object_mut()?
        .insert("enum".to_owned(), json!([project_type_name]));

    // The VM workload carries no SQL inputs, so the model cannot propose them at all.
    if project_type == ProjectType::Ec2Vm {
        for field in SQL_ONLY_FIELDS {
            properties.remove(*field);
        }
    }

    let allowed_specific_fields: &[&str] = match project_type {
        ProjectType::Ec2 | ProjectType::Ec2Vm => &["instance_type", "volumes"],
        ProjectType::Rds => &[
            "instance_type",
            "deployment",
            "commercial_term",
            "storage_class",
            "source_max_iops",
        ],
        ProjectType::OnPrem => &[
            "source_vcpu",
            "licensable_cores",
            "source_max_iops",
            "hardware_capex_usd",
            "depreciation_years",
            "average_power_kw_override",
        ],
        ProjectType::SqlPayg => unreachable!("SQL PAYG returned before resource scoping"),
    };
    const SOURCE_SPECIFIC_FIELDS: &[&str] = &[
        "instance_type",
        "volumes",
        "deployment",
        "commercial_term",
        "storage_class",
        "source_vcpu",
        "licensable_cores",
        "source_max_iops",
        "hardware_capex_usd",
        "depreciation_years",
        "average_power_kw_override",
    ];
    for field in SOURCE_SPECIFIC_FIELDS {
        if !allowed_specific_fields.contains(field) {
            properties.remove(*field);
        }
    }

    serde_json::to_string(&schema).ok()
}

/// Candidate project changes a model may propose.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<ProjectSettings>,
    pub resources: Option<Vec<Resource>>,
}

impl ProjectPatch {
    /// Apply the candidate changes to a host-supplied base project.
    pub fn apply(&self, base: &EditableProject) -> EditableProject {
        EditableProject {
            name: self.name.clone().unwrap_or_else(|| base.name.clone()),
            description: self
                .description
                .clone()
                .or_else(|| base.description.clone()),
            settings: self
                .settings
                .clone()
                .unwrap_or_else(|| base.settings.clone()),
            resources: self
                .resources
                .clone()
                .unwrap_or_else(|| base.resources.clone()),
            aws_price_snapshot_id: base.aws_price_snapshot_id.clone(),
            azure_price_snapshot_id: base.azure_price_snapshot_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplicationHelpInput {
    pub question: String,
    #[serde(default)]
    pub control_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentProjectInput {}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatchInput {
    #[serde(default)]
    pub patch: ProjectPatch,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageProjectPatchInput {
    #[serde(default)]
    pub patch: ProjectPatch,
    pub omissions: Vec<String>,
    pub uncertainties: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewProjectSettingsInput {
    pub aws_region: Option<String>,
    pub azure_region: Option<String>,
    pub source_compute_discount: Option<DecimalValue>,
    pub source_license_discount: Option<DecimalValue>,
    pub source_storage_discount: Option<DecimalValue>,
    pub azure_compute_discount: Option<DecimalValue>,
    pub azure_license_discount: Option<DecimalValue>,
    pub azure_storage_discount: Option<DecimalValue>,
    pub selected_parity_adjustment: Option<DecimalValue>,
    pub default_annual_hours: Option<DecimalValue>,
    pub default_mi_purchase_option: Option<PurchaseOption>,
    pub enterprise_license_sa_usd_per_two_core_pack: Option<DecimalValue>,
    pub standard_license_sa_usd_per_two_core_pack: Option<DecimalValue>,
    pub remaining_coverage_months: Option<u8>,
    pub electricity_rate_usd_per_kwh: Option<DecimalValue>,
    pub sql_payg: Option<NewSqlPaygSettingsInput>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewSqlPaygSettingsInput {
    pub enterprise_licensed_cores: Option<u32>,
    pub standard_licensed_cores: Option<u32>,
    pub software_assurance_annual_usd: Option<DecimalValue>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewVolumeInput {
    pub label: Option<String>,
    pub aws_volume_id: Option<String>,
    pub volume_type: Option<EbsVolumeType>,
    pub capacity_gb: Option<DecimalValue>,
    pub provisioned_iops: Option<u64>,
    pub throughput_mibps: Option<DecimalValue>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewResourceInput {
    pub source_type: ProjectType,
    pub workload_name: Option<String>,
    pub quantity: Option<u32>,
    pub sql_edition: Option<SqlEdition>,
    pub license_basis: Option<LicenseBasis>,
    pub sql_data_gb_per_instance: Option<DecimalValue>,
    pub source_ram_gb_per_instance: Option<DecimalValue>,
    pub annual_hours_per_instance: Option<DecimalValue>,
    pub mi_purchase_option: Option<PurchaseOption>,
    pub instance_type: Option<String>,
    pub volumes: Option<Vec<NewVolumeInput>>,
    pub deployment: Option<RdsDeployment>,
    pub commercial_term: Option<String>,
    pub storage_class: Option<String>,
    pub source_vcpu: Option<u32>,
    pub licensable_cores: Option<u32>,
    pub source_max_iops: Option<u64>,
    pub hardware_capex_usd: Option<DecimalValue>,
    pub depreciation_years: Option<DecimalValue>,
    pub average_power_kw_override: Option<DecimalValue>,
}

impl NewResourceInput {
    fn fields_match_source_type(&self) -> bool {
        let ec2_fields = self.volumes.is_some();
        let rds_fields = self.deployment.is_some()
            || self.commercial_term.is_some()
            || self.storage_class.is_some();
        let on_prem_fields = self.source_vcpu.is_some()
            || self.licensable_cores.is_some()
            || self.hardware_capex_usd.is_some()
            || self.depreciation_years.is_some()
            || self.average_power_kw_override.is_some();
        match self.source_type {
            ProjectType::Ec2 => !rds_fields && !on_prem_fields,
            // The VM workload is structurally non-SQL, so any SQL input is a mismatch.
            ProjectType::Ec2Vm => {
                !rds_fields
                    && !on_prem_fields
                    && self.sql_edition.is_none()
                    && self.license_basis.is_none()
                    && self.sql_data_gb_per_instance.is_none()
                    && self.mi_purchase_option.is_none()
            }
            ProjectType::Rds => !ec2_fields && !on_prem_fields,
            ProjectType::OnPrem => self.instance_type.is_none() && !ec2_fields && !rds_fields,
            ProjectType::SqlPayg => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageNewProjectDraftInput {
    pub project_type: ProjectType,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub settings: NewProjectSettingsInput,
    #[serde(default)]
    pub resources: Vec<NewResourceInput>,
    pub omissions: Vec<String>,
    pub uncertainties: Vec<String>,
}

const STORAGE_MEASUREMENT_FIELDS: &[&str] = &[
    "sql_data_gb_per_instance",
    "source_ram_gb_per_instance",
    "capacity_gb",
];

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum StorageUnit {
    Gb,
    Gib,
    Tb,
    Tib,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageMeasurement {
    value: DecimalValue,
    unit: StorageUnit,
}

impl StorageMeasurement {
    fn into_gb(self) -> DecimalValue {
        match self.unit {
            StorageUnit::Gb | StorageUnit::Gib => self.value,
            StorageUnit::Tb | StorageUnit::Tib => {
                DecimalValue(self.value.0 * Decimal::from(1_024_u32))
            }
        }
    }
}

/// Typed, validated arguments for one registered tool.
#[derive(Clone, Debug)]
pub enum ToolInput {
    AgentCapabilities,
    ApplicationHelp(ApplicationHelpInput),
    CurrentProject,
    ValidateProjectPatch(ProjectPatchInput),
    CalculateProjectDraft(ProjectPatchInput),
    StageProjectPatch(StageProjectPatchInput),
    StageNewProjectDraft(StageNewProjectDraftInput),
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum InvalidToolArguments {
    #[error("the tool arguments were not valid JSON")]
    MalformedJson,
    #[error("the tool arguments contained an unknown field")]
    UnknownField,
    #[error("the tool arguments omitted a required field")]
    MissingField,
    #[error("a tool argument had the wrong JSON type")]
    TypeMismatch,
    #[error("a tool argument contained an unsupported enum variant")]
    UnknownVariant,
    #[error("a scalar tool argument contained an unsupported value")]
    InvalidScalarValue,
    #[error("the tool arguments did not match the typed input shape")]
    InvalidShape,
    #[error("a bounded tool argument was outside its allowed range")]
    InputBounds,
    #[error("an extraction note was outside its allowed bounds")]
    ExtractionNotes,
    #[error("a resource contained fields for a different source type")]
    ResourceFieldMismatch,
    #[error("the tool is not registered")]
    UnregisteredTool,
}

/// Parse untrusted model arguments into a typed input for a registered tool.
pub fn parse_input(
    definition: &ToolDefinition,
    arguments: &str,
) -> Result<ToolInput, InvalidToolArguments> {
    let arguments = if arguments.trim().is_empty() {
        "{}"
    } else {
        arguments
    };
    match definition.name {
        "get_agent_capabilities" => {
            let _input: CurrentProjectInput = parse_json_input(arguments)?;
            Ok(ToolInput::AgentCapabilities)
        }
        "get_application_help" => {
            let input: ApplicationHelpInput = parse_json_input(arguments)?;
            let question_chars = input.question.trim().chars().count();
            if !(1..=help::MAX_QUESTION_CHARS).contains(&question_chars) {
                return Err(InvalidToolArguments::InputBounds);
            }
            if input
                .control_id
                .as_ref()
                .is_some_and(|control_id| !(1..=64).contains(&control_id.chars().count()))
            {
                return Err(InvalidToolArguments::InputBounds);
            }
            Ok(ToolInput::ApplicationHelp(input))
        }
        "get_current_project" => {
            let _input: CurrentProjectInput = parse_json_input(arguments)?;
            Ok(ToolInput::CurrentProject)
        }
        "validate_project_patch" => parse_json_input_with_storage_normalization(arguments)
            .map(ToolInput::ValidateProjectPatch),
        "calculate_project_draft" => parse_json_input_with_storage_normalization(arguments)
            .map(ToolInput::CalculateProjectDraft),
        "stage_project_patch" => {
            let mut input: StageProjectPatchInput =
                parse_json_input_with_storage_normalization(arguments)?;
            normalize_extraction_notes(&mut input.omissions)?;
            normalize_extraction_notes(&mut input.uncertainties)?;
            Ok(ToolInput::StageProjectPatch(input))
        }
        "stage_new_project_draft" => {
            let mut input: StageNewProjectDraftInput =
                parse_json_input_with_storage_normalization(arguments)?;
            normalize_extraction_notes(&mut input.omissions)?;
            normalize_extraction_notes(&mut input.uncertainties)?;
            if input.resources.iter().any(|resource| {
                resource.source_type != input.project_type || !resource.fields_match_source_type()
            }) {
                return Err(InvalidToolArguments::ResourceFieldMismatch);
            }
            Ok(ToolInput::StageNewProjectDraft(input))
        }
        _ => Err(InvalidToolArguments::UnregisteredTool),
    }
}

fn parse_json_input<T: DeserializeOwned>(arguments: &str) -> Result<T, InvalidToolArguments> {
    serde_json::from_str(arguments).map_err(classify_json_input_error)
}

fn parse_json_input_with_storage_normalization<T: DeserializeOwned>(
    arguments: &str,
) -> Result<T, InvalidToolArguments> {
    let mut value: Value = serde_json::from_str(arguments).map_err(classify_json_input_error)?;
    normalize_storage_measurements(&mut value)?;
    serde_json::from_value(value).map_err(classify_json_input_error)
}

fn normalize_storage_measurements(value: &mut Value) -> Result<(), InvalidToolArguments> {
    match value {
        Value::Array(values) => {
            for value in values {
                normalize_storage_measurements(value)?;
            }
        }
        Value::Object(fields) => {
            for (field, value) in fields {
                if STORAGE_MEASUREMENT_FIELDS.contains(&field.as_str()) && value.is_object() {
                    let measurement: StorageMeasurement =
                        serde_json::from_value(value.clone()).map_err(classify_json_input_error)?;
                    *value = serde_json::to_value(measurement.into_gb())
                        .map_err(|_| InvalidToolArguments::InvalidShape)?;
                } else {
                    normalize_storage_measurements(value)?;
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn classify_json_input_error(error: serde_json::Error) -> InvalidToolArguments {
    if error.is_syntax() || error.is_eof() {
        return InvalidToolArguments::MalformedJson;
    }

    let message = error.to_string();
    if message.starts_with("unknown field ") {
        InvalidToolArguments::UnknownField
    } else if message.starts_with("missing field ") {
        InvalidToolArguments::MissingField
    } else if message.starts_with("invalid type:") {
        InvalidToolArguments::TypeMismatch
    } else if message.starts_with("unknown variant ") {
        InvalidToolArguments::UnknownVariant
    } else if message.starts_with("invalid value:") {
        InvalidToolArguments::InvalidScalarValue
    } else {
        InvalidToolArguments::InvalidShape
    }
}

fn normalize_extraction_notes(notes: &mut Vec<String>) -> Result<(), InvalidToolArguments> {
    if notes.len() > MAX_EXTRACTION_NOTES {
        return Err(InvalidToolArguments::ExtractionNotes);
    }
    for note in notes {
        let trimmed = note.trim();
        if trimmed.is_empty()
            || trimmed.chars().count() > MAX_EXTRACTION_NOTE_CHARS
            || trimmed.chars().any(char::is_control)
        {
            return Err(InvalidToolArguments::ExtractionNotes);
        }
        if trimmed.len() != note.len() {
            *note = trimmed.to_owned();
        }
    }
    Ok(())
}

/// Structured result of one tool call. Failure codes are stable and carry no internal detail.
#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    Ok {
        result: Value,
    },
    Staged {
        proposal: Option<Box<AssistantProposal>>,
        omissions: Vec<String>,
        uncertainties: Vec<String>,
    },
    Invalid {
        errors: Vec<ValidationIssue>,
    },
    Unavailable {
        code: &'static str,
    },
    Error {
        code: &'static str,
    },
}

impl ToolOutcome {
    pub fn proposal(&self) -> Option<&AssistantProposal> {
        match self {
            Self::Staged {
                proposal: Some(proposal),
                ..
            } => Some(proposal),
            Self::Staged { proposal: None, .. }
            | Self::Ok { .. }
            | Self::Invalid { .. }
            | Self::Unavailable { .. }
            | Self::Error { .. } => None,
        }
    }

    pub fn extraction_notes(&self) -> Option<(&[String], &[String])> {
        match self {
            Self::Staged {
                omissions,
                uncertainties,
                ..
            } => Some((omissions, uncertainties)),
            Self::Ok { .. }
            | Self::Invalid { .. }
            | Self::Unavailable { .. }
            | Self::Error { .. } => None,
        }
    }

    pub fn history_status(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "completed",
            Self::Staged {
                proposal: Some(_), ..
            } => "staged",
            Self::Staged { proposal: None, .. } => "reported",
            Self::Invalid { .. } => "invalid",
            Self::Unavailable { .. } => "unavailable",
            Self::Error { .. } => "error",
        }
    }

    /// Serialize the outcome for model context, replacing anything oversized with a code.
    pub fn to_bounded_json(&self) -> String {
        let serialized = if let Self::Staged { proposal, .. } = self {
            serde_json::to_string(&json!({
                "status": "ok",
                "result": {
                    "action": proposal.as_ref().map(|proposal| proposal.action()),
                    "staged": proposal.is_some(),
                    "report_recorded": true
                }
            }))
        } else {
            serde_json::to_string(self)
        };
        let Ok(serialized) = serialized else {
            return r#"{"status":"error","code":"result_not_serializable"}"#.to_owned();
        };
        if serialized.chars().count() > MAX_TOOL_RESULT_CHARS {
            return r#"{"status":"error","code":"result_too_large"}"#.to_owned();
        }
        serialized
    }
}

/// Execute one preflighted tool call against existing application services.
pub async fn dispatch(state: &AppState, context: &TurnContext, input: &ToolInput) -> ToolOutcome {
    match input {
        ToolInput::AgentCapabilities => agent_capabilities(context),
        ToolInput::ApplicationHelp(input) => application_help(input),
        ToolInput::CurrentProject => current_project(state, context).await,
        ToolInput::ValidateProjectPatch(input) => {
            validate_project_patch(state, context, &input.patch).await
        }
        ToolInput::CalculateProjectDraft(input) => {
            calculate_project_draft(state, context, &input.patch).await
        }
        ToolInput::StageProjectPatch(input) => stage_project_patch(state, context, input).await,
        ToolInput::StageNewProjectDraft(input) => {
            stage_new_project_draft(state, context, input).await
        }
    }
}

fn agent_capabilities(context: &TurnContext) -> ToolOutcome {
    into_ok(&json!({
        "programming": "A bounded Rust-hosted reasoning loop backed by Microsoft Foundry inference. The host owns identity, authorization, validation, calculations, persistence, and tool execution.",
        "available_tools": schemas_for_context(context)
            .into_iter()
            .map(|tool| tool.name)
            .collect::<Vec<_>>(),
        "selected_project": context.project().is_some(),
        "supported_actions": if context.project().is_some() {
            vec!["read the selected project", "validate project changes", "calculate a project draft", "stage a reviewed project patch", "analyze a JPEG or PNG for project inputs"]
        } else {
            vec!["explain application controls", "describe agent capabilities", "stage a validated new unsaved project draft", "analyze a JPEG or PNG into a new unsaved project draft"]
        },
        "action_boundary": "Project changes are staged for review. Persisted changes require a separate explicit confirmation.",
        "memory_boundary": "Conversation, image, and action history are request-scoped. There is no hidden or cross-session memory."
    }))
}

fn application_help(input: &ApplicationHelpInput) -> ToolOutcome {
    let response = match input.control_id.as_deref() {
        Some(control_id) => match help::explain_control(control_id) {
            Some(response) => response,
            None => {
                return ToolOutcome::Error {
                    code: "unknown_control_id",
                };
            }
        },
        None => match help::answer_question(&input.question) {
            Ok(response) => response,
            Err(errors) => return ToolOutcome::Invalid { errors },
        },
    };
    into_ok(&response)
}

async fn current_project(state: &AppState, context: &TurnContext) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };
    into_ok(&ProjectView::from_project(&project))
}

async fn validate_project_patch(
    state: &AppState,
    context: &TurnContext,
    patch: &ProjectPatch,
) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };

    let candidate = patch.apply(&project);
    let issues = candidate.validate();
    if issues.is_empty() {
        into_ok(&json!({ "valid": true }))
    } else {
        ToolOutcome::Invalid { errors: issues }
    }
}

async fn calculate_project_draft(
    state: &AppState,
    context: &TurnContext,
    patch: &ProjectPatch,
) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };

    let candidate = patch.apply(&project);
    let issues = candidate.validate();
    if !issues.is_empty() {
        return ToolOutcome::Invalid { errors: issues };
    }

    let Ok(permit) = state.calculation_slots.clone().try_acquire_owned() else {
        return ToolOutcome::Unavailable {
            code: "calculation_busy",
        };
    };
    let revision = crate::api::calculations::calculate_project(state, &candidate, None).await;
    drop(permit);

    match revision {
        Ok(revision) => into_ok(&CalculationView::from_revision(&revision)),
        Err(_) => ToolOutcome::Error {
            code: "calculation_failed",
        },
    }
}

async fn stage_project_patch(
    state: &AppState,
    context: &TurnContext,
    input: &StageProjectPatchInput,
) -> ToolOutcome {
    let Some(project) = read_selected_project(state, context).await else {
        return ToolOutcome::Unavailable {
            code: "no_selected_project",
        };
    };
    let project = match project {
        Ok(project) => project,
        Err(outcome) => return outcome,
    };

    let candidate = input.patch.apply(&project);
    let issues = candidate.validate();
    if !issues.is_empty() {
        return ToolOutcome::Invalid { errors: issues };
    }

    let changes = match project_patch_changes(&project, &candidate, &input.patch) {
        Ok(changes) => changes,
        Err(()) => {
            return ToolOutcome::Invalid {
                errors: vec![ValidationIssue {
                    pointer: "/patch".to_owned(),
                    code: "limit",
                    message: format!(
                        "A proposal may change at most {MAX_PROPOSAL_CHANGES} individual fields."
                    ),
                }],
            };
        }
    };
    if changes.is_empty() && input.omissions.is_empty() && input.uncertainties.is_empty() {
        return ToolOutcome::Invalid {
            errors: vec![ValidationIssue {
                pointer: "/patch".to_owned(),
                code: "required",
                message: "The extraction report must propose a change or identify an omission or uncertainty."
                    .to_owned(),
            }],
        };
    }

    ToolOutcome::Staged {
        proposal: (!changes.is_empty()).then(|| {
            Box::new(
                ProjectPatchProposal {
                    action: "apply_project_patch",
                    patch: input.patch.clone(),
                    changes,
                }
                .into(),
            )
        }),
        omissions: input.omissions.clone(),
        uncertainties: input.uncertainties.clone(),
    }
}

async fn stage_new_project_draft(
    state: &AppState,
    context: &TurnContext,
    input: &StageNewProjectDraftInput,
) -> ToolOutcome {
    let aws_region = input.settings.aws_region.as_deref().unwrap_or("eu-west-1");
    let aws_snapshot = if matches!(input.project_type, ProjectType::Ec2 | ProjectType::Ec2Vm) {
        state
            .pricing
            .resolve_aws("USD", aws_region)
            .await
            .ok()
            .and_then(|resolution| resolution.snapshot)
    } else {
        None
    };
    stage_new_project_draft_with_ec2_memory(context, input, &|instance_type| {
        aws_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.ec2_rate(instance_type))
            .map(|record| record.rate.catalog_memory_gb)
    })
}

fn stage_new_project_draft_with_ec2_memory(
    context: &TurnContext,
    input: &StageNewProjectDraftInput,
    ec2_catalog_memory: &dyn Fn(&str) -> Option<DecimalValue>,
) -> ToolOutcome {
    if context.project().is_some() {
        return ToolOutcome::Unavailable {
            code: "project_already_selected",
        };
    }
    let ec2_catalog_ram_missing =
        matches!(input.project_type, ProjectType::Ec2 | ProjectType::Ec2Vm)
            && if input.resources.is_empty() {
                ec2_catalog_memory("r6id.8xlarge").is_none()
            } else {
                input.resources.iter().any(|resource| {
                    resource.source_ram_gb_per_instance.is_none()
                        && ec2_catalog_memory(
                            resource.instance_type.as_deref().unwrap_or("r6id.8xlarge"),
                        )
                        .is_none()
                })
            };
    if ec2_catalog_ram_missing {
        let mut uncertainties = input.uncertainties.clone();
        if uncertainties.len() < MAX_EXTRACTION_NOTES {
            uncertainties.push(EC2_CATALOG_RAM_UNAVAILABLE_UNCERTAINTY.to_owned());
        }
        return ToolOutcome::Staged {
            proposal: None,
            omissions: input.omissions.clone(),
            uncertainties,
        };
    }

    let project = new_project_draft(input, ec2_catalog_memory);
    let issues = project.validate();
    if !issues.is_empty() {
        return ToolOutcome::Invalid { errors: issues };
    }

    let mut uncertainties = input.uncertainties.clone();
    let assumed_iops = |assumed: EbsVolumeType| {
        matches!(input.project_type, ProjectType::Ec2 | ProjectType::Ec2Vm)
            && input.resources.iter().any(|resource| {
                resource.volumes.iter().flatten().any(|volume| {
                    volume.provisioned_iops.is_none() && volume.volume_type == Some(assumed)
                })
            })
    };
    if assumed_iops(EbsVolumeType::Gp3) {
        uncertainties.push(GP3_ASSUMED_IOPS_UNCERTAINTY.to_owned());
    }
    if assumed_iops(EbsVolumeType::Io2) {
        uncertainties.push(IO2_ASSUMED_IOPS_UNCERTAINTY.to_owned());
    }
    if input.project_type == ProjectType::OnPrem
        && (input
            .settings
            .enterprise_license_sa_usd_per_two_core_pack
            .is_none()
            || input
                .settings
                .standard_license_sa_usd_per_two_core_pack
                .is_none())
    {
        uncertainties.push(
            "Missing License + SA pack prices use the reviewed SQL Server 2022 public-book reference verified 2026-08-07."
                .to_owned(),
        );
    }
    if input.project_type == ProjectType::SqlPayg
        && input
            .settings
            .sql_payg
            .as_ref()
            .is_none_or(|settings| settings.software_assurance_annual_usd.is_none())
    {
        uncertainties.push(
            "Annual Software Assurance or renewal spend was not visible and is temporarily set to USD 0 for review."
                .to_owned(),
        );
    }

    ToolOutcome::Staged {
        proposal: Some(Box::new(
            NewProjectDraftProposal {
                action: "open_project_draft",
                project,
            }
            .into(),
        )),
        omissions: input.omissions.clone(),
        uncertainties,
    }
}

fn new_project_draft(
    input: &StageNewProjectDraftInput,
    ec2_catalog_memory: &dyn Fn(&str) -> Option<DecimalValue>,
) -> EditableProject {
    let on_prem = input.project_type == ProjectType::OnPrem;
    let aws_project = matches!(
        input.project_type,
        ProjectType::Ec2 | ProjectType::Ec2Vm | ProjectType::Rds
    );
    let sql_payg = (input.project_type == ProjectType::SqlPayg).then(|| {
        let settings = input.settings.sql_payg.as_ref();
        SqlPaygSettings {
            enterprise_licensed_cores: settings
                .and_then(|settings| settings.enterprise_licensed_cores)
                .unwrap_or_default(),
            standard_licensed_cores: settings
                .and_then(|settings| settings.standard_licensed_cores)
                .unwrap_or_default(),
            software_assurance_annual_usd: settings
                .and_then(|settings| settings.software_assurance_annual_usd)
                .unwrap_or_default(),
        }
    });
    let settings = ProjectSettings {
        project_type: input.project_type,
        aws_region: if aws_project {
            Some(
                input
                    .settings
                    .aws_region
                    .clone()
                    .unwrap_or_else(|| "eu-west-1".to_owned()),
            )
        } else {
            None
        },
        azure_region: input
            .settings
            .azure_region
            .clone()
            .unwrap_or_else(|| "swedencentral".to_owned()),
        currency: "USD".to_owned(),
        source_compute_discount: input.settings.source_compute_discount.unwrap_or_default(),
        source_license_discount: input.settings.source_license_discount.unwrap_or_default(),
        source_storage_discount: input.settings.source_storage_discount.unwrap_or_default(),
        azure_compute_discount: input.settings.azure_compute_discount.unwrap_or_default(),
        azure_license_discount: input.settings.azure_license_discount.unwrap_or_default(),
        azure_storage_discount: input.settings.azure_storage_discount.unwrap_or_default(),
        selected_parity_adjustment: input
            .settings
            .selected_parity_adjustment
            .unwrap_or_default(),
        default_annual_hours: input
            .settings
            .default_annual_hours
            .unwrap_or_else(|| decimal(8_760)),
        default_mi_purchase_option: input
            .settings
            .default_mi_purchase_option
            .unwrap_or(PurchaseOption::Ahb),
        enterprise_license_sa_usd_per_two_core_pack: input
            .settings
            .enterprise_license_sa_usd_per_two_core_pack
            .or(on_prem.then(|| decimal(20_557))),
        standard_license_sa_usd_per_two_core_pack: input
            .settings
            .standard_license_sa_usd_per_two_core_pack
            .or(on_prem.then(|| decimal(5_363))),
        remaining_coverage_months: input
            .settings
            .remaining_coverage_months
            .or(on_prem.then_some(12)),
        electricity_rate_usd_per_kwh: input
            .settings
            .electricity_rate_usd_per_kwh
            .or(on_prem.then_some(DecimalValue::ZERO)),
        sql_payg,
    };
    let resources = if input.project_type == ProjectType::SqlPayg {
        Vec::new()
    } else if input.resources.is_empty() {
        vec![new_resource(
            &NewResourceInput {
                source_type: input.project_type,
                workload_name: None,
                quantity: None,
                sql_edition: None,
                license_basis: None,
                sql_data_gb_per_instance: None,
                source_ram_gb_per_instance: None,
                annual_hours_per_instance: None,
                mi_purchase_option: None,
                instance_type: None,
                volumes: None,
                deployment: None,
                commercial_term: None,
                storage_class: None,
                source_vcpu: None,
                licensable_cores: None,
                source_max_iops: None,
                hardware_capex_usd: None,
                depreciation_years: None,
                average_power_kw_override: None,
            },
            ec2_catalog_memory,
        )]
    } else {
        input
            .resources
            .iter()
            .map(|resource| new_resource(resource, ec2_catalog_memory))
            .collect()
    };

    EditableProject {
        name: input
            .name
            .clone()
            .unwrap_or_else(|| "SQL TCO estimate".to_owned()),
        description: input.description.clone(),
        settings,
        resources,
        aws_price_snapshot_id: None,
        azure_price_snapshot_id: None,
    }
}

fn new_resource(
    input: &NewResourceInput,
    ec2_catalog_memory: &dyn Fn(&str) -> Option<DecimalValue>,
) -> Resource {
    let ec2_instance_type = input.instance_type.as_deref().unwrap_or("r6id.8xlarge");
    let shared = SharedResource {
        id: Uuid::new_v4(),
        workload_name: input.workload_name.clone().unwrap_or_else(|| {
            if input.source_type == ProjectType::Ec2Vm {
                "Virtual machine".to_owned()
            } else {
                "SQL workload".to_owned()
            }
        }),
        server_name: None,
        quantity: input.quantity.unwrap_or(1),
        source_ram_gb_per_instance: input
            .source_ram_gb_per_instance
            .or_else(|| {
                matches!(input.source_type, ProjectType::Ec2 | ProjectType::Ec2Vm)
                    .then(|| ec2_catalog_memory(ec2_instance_type))
                    .flatten()
            })
            .unwrap_or_else(|| {
                decimal(if input.source_type == ProjectType::Rds {
                    128
                } else {
                    256
                })
            }),
        annual_hours_per_instance: input
            .annual_hours_per_instance
            .unwrap_or_else(|| decimal(8_760)),
    };
    let sql = SqlWorkload {
        sql_edition: input.sql_edition.unwrap_or(SqlEdition::Enterprise),
        license_basis: input.license_basis.unwrap_or(LicenseBasis::Byol),
        sql_data_gb_per_instance: input
            .sql_data_gb_per_instance
            .unwrap_or_else(|| decimal(1_024)),
        mi_purchase_option: input.mi_purchase_option.unwrap_or(PurchaseOption::Ahb),
    };

    match input.source_type {
        ProjectType::Ec2 => {
            let volumes = input
                .volumes
                .as_ref()
                .map(|volumes| volumes.iter().map(new_volume).collect())
                .unwrap_or_else(|| vec![new_volume(&NewVolumeInput::default())]);
            Resource::Ec2(Ec2Resource {
                shared,
                sql,
                instance_type: ec2_instance_type.to_owned(),
                volumes,
            })
        }
        ProjectType::Rds => Resource::Rds(RdsResource {
            shared,
            sql,
            instance_type: input
                .instance_type
                .clone()
                .unwrap_or_else(|| "db.m6i.8xlarge".to_owned()),
            deployment: input.deployment.unwrap_or(RdsDeployment::SingleAz),
            commercial_term: input
                .commercial_term
                .clone()
                .unwrap_or_else(|| "on-demand".to_owned()),
            storage_class: input
                .storage_class
                .clone()
                .unwrap_or_else(|| "gp3".to_owned()),
            source_max_iops: input.source_max_iops.unwrap_or(0),
        }),
        ProjectType::OnPrem => Resource::OnPrem(OnPremResource {
            shared,
            sql,
            source_vcpu: input.source_vcpu.unwrap_or(32),
            licensable_cores: input.licensable_cores.unwrap_or(32),
            source_max_iops: input.source_max_iops.unwrap_or(0),
            hardware_capex_usd: input.hardware_capex_usd.unwrap_or_default(),
            depreciation_years: input.depreciation_years.unwrap_or_else(|| decimal(5)),
            average_power_kw_override: input.average_power_kw_override,
        }),
        ProjectType::Ec2Vm => {
            let volumes = match input.volumes.as_deref() {
                Some(volumes) if !volumes.is_empty() => volumes
                    .iter()
                    .enumerate()
                    .map(|(index, volume)| new_vm_volume(volume, index))
                    .collect(),
                _ => vec![new_vm_volume(&NewVolumeInput::default(), 0)],
            };
            Resource::Ec2Vm(Ec2VmResource {
                shared,
                instance_type: ec2_instance_type.to_owned(),
                volumes,
            })
        }
        ProjectType::SqlPayg => {
            unreachable!("SQL Pay As You Go projects cannot contain workload resources")
        }
    }
}

fn new_volume(input: &NewVolumeInput) -> EbsVolume {
    let volume_type = input.volume_type.unwrap_or(EbsVolumeType::Ephemeral);
    EbsVolume {
        id: Uuid::new_v4(),
        label: input
            .label
            .clone()
            .unwrap_or_else(|| "Instance storage".to_owned()),
        aws_volume_id: input.aws_volume_id.clone(),
        volume_type,
        capacity_gb: input.capacity_gb.unwrap_or_default(),
        provisioned_iops: input.provisioned_iops.or(match volume_type {
            EbsVolumeType::Gp3 => Some(GP3_BASELINE_IOPS),
            EbsVolumeType::Io2 => Some(IO2_MINIMUM_IOPS),
            EbsVolumeType::Ephemeral => None,
        }),
        throughput_mibps: input.throughput_mibps,
    }
}

/// The first persistent volume is the operating system disk and the rest are data disks, which is
/// the reviewed default in section 4.4 of the product specification.
fn new_vm_volume(input: &NewVolumeInput, index: usize) -> VmVolume {
    let is_os = index == 0;
    let volume = new_volume(&NewVolumeInput {
        // An operating system disk is persistent by definition, so it is never instance storage.
        volume_type: input.volume_type.or(is_os.then_some(EbsVolumeType::Gp3)),
        label: input
            .label
            .clone()
            .or_else(|| Some(if is_os { "OS disk" } else { "Data disk" }.to_owned())),
        ..input.clone()
    });
    VmVolume {
        id: volume.id,
        label: volume.label,
        aws_volume_id: volume.aws_volume_id,
        volume_type: volume.volume_type,
        role: if is_os {
            VmDiskRole::Os
        } else {
            VmDiskRole::Data
        },
        capacity_gb: volume.capacity_gb,
        provisioned_iops: volume.provisioned_iops,
        throughput_mibps: volume.throughput_mibps,
    }
}

fn decimal(value: i64) -> DecimalValue {
    DecimalValue(Decimal::from(value))
}

async fn read_selected_project(
    state: &AppState,
    context: &TurnContext,
) -> Option<Result<EditableProject, ToolOutcome>> {
    let selected = context.project()?;
    let document = match state.projects.get(context.owner_id(), selected.id).await {
        Ok(document) => document,
        Err(RepositoryError::NotFound) => {
            return Some(Err(ToolOutcome::Error {
                code: "project_not_found",
            }));
        }
        Err(_) => {
            return Some(Err(ToolOutcome::Unavailable {
                code: "storage_unavailable",
            }));
        }
    };

    Some(Ok(EditableProject {
        name: document.name,
        description: document.description,
        settings: document.settings,
        resources: document.resources,
        aws_price_snapshot_id: document.aws_price_snapshot_id,
        azure_price_snapshot_id: document.azure_price_snapshot_id,
    }))
}

#[derive(Clone, Debug, Serialize)]
pub enum AssistantProposal {
    ProjectPatch(ProjectPatchProposal),
    NewProjectDraft(NewProjectDraftProposal),
}

impl AssistantProposal {
    pub fn action(&self) -> &'static str {
        match self {
            Self::ProjectPatch(proposal) => proposal.action,
            Self::NewProjectDraft(proposal) => proposal.action,
        }
    }
}

impl From<ProjectPatchProposal> for AssistantProposal {
    fn from(proposal: ProjectPatchProposal) -> Self {
        Self::ProjectPatch(proposal)
    }
}

impl From<NewProjectDraftProposal> for AssistantProposal {
    fn from(proposal: NewProjectDraftProposal) -> Self {
        Self::NewProjectDraft(proposal)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectPatchProposal {
    pub action: &'static str,
    pub patch: ProjectPatch,
    pub changes: Vec<ProjectPatchChange>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NewProjectDraftProposal {
    pub action: &'static str,
    pub project: EditableProject,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectPatchChange {
    pub pointer: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

fn project_patch_changes(
    base: &EditableProject,
    candidate: &EditableProject,
    patch: &ProjectPatch,
) -> Result<Vec<ProjectPatchChange>, ()> {
    let base = serde_json::to_value(base).map_err(|_| ())?;
    let candidate = serde_json::to_value(candidate).map_err(|_| ())?;
    let roots = [
        patch.name.is_some().then_some("/name"),
        patch.description.is_some().then_some("/description"),
        patch.settings.is_some().then_some("/settings"),
        patch.resources.is_some().then_some("/resources"),
    ];
    let mut changes = Vec::new();
    for pointer in roots.into_iter().flatten() {
        collect_changes(
            pointer,
            base.pointer(pointer),
            candidate.pointer(pointer),
            &mut changes,
        )?;
    }
    Ok(changes)
}

fn collect_changes(
    pointer: &str,
    before: Option<&Value>,
    after: Option<&Value>,
    changes: &mut Vec<ProjectPatchChange>,
) -> Result<(), ()> {
    if before == after {
        return Ok(());
    }
    match (before, after) {
        (Some(Value::Object(before)), Some(Value::Object(after))) => {
            let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let key = key.replace('~', "~0").replace('/', "~1");
                collect_changes(
                    &format!("{pointer}/{key}"),
                    before.get(key.replace("~1", "/").replace("~0", "~").as_str()),
                    after.get(key.replace("~1", "/").replace("~0", "~").as_str()),
                    changes,
                )?;
            }
            Ok(())
        }
        (Some(Value::Array(before)), Some(Value::Array(after))) => {
            for index in 0..before.len().max(after.len()) {
                collect_changes(
                    &format!("{pointer}/{index}"),
                    before.get(index),
                    after.get(index),
                    changes,
                )?;
            }
            Ok(())
        }
        _ => {
            if changes.len() >= MAX_PROPOSAL_CHANGES {
                return Err(());
            }
            changes.push(ProjectPatchChange {
                pointer: pointer.to_owned(),
                before: before.cloned(),
                after: after.cloned(),
            });
            Ok(())
        }
    }
}

fn into_ok<T: Serialize>(value: &T) -> ToolOutcome {
    match serde_json::to_value(value) {
        Ok(result) => ToolOutcome::Ok { result },
        Err(_) => ToolOutcome::Error {
            code: "result_not_serializable",
        },
    }
}

/// Model-visible projection of a project.
#[derive(Debug, Serialize)]
struct ProjectView {
    settings: ProjectSettings,
    resources: Vec<Resource>,
    resource_count: usize,
    has_aws_price_snapshot: bool,
    has_azure_price_snapshot: bool,
}

impl ProjectView {
    fn from_project(project: &EditableProject) -> Self {
        let resources = project
            .resources
            .iter()
            .enumerate()
            .map(|(index, resource)| redact_resource(resource, index))
            .collect::<Vec<_>>();
        Self {
            settings: project.settings.clone(),
            resource_count: resources.len(),
            resources,
            has_aws_price_snapshot: project.aws_price_snapshot_id.is_some(),
            has_azure_price_snapshot: project.azure_price_snapshot_id.is_some(),
        }
    }
}

fn redact_resource(resource: &Resource, index: usize) -> Resource {
    let mut redacted = resource.clone();
    let workload_name = format!("workload-{}", index + 1);
    match &mut redacted {
        Resource::Ec2(ec2) => {
            ec2.shared.workload_name = workload_name;
            for (volume_index, volume) in ec2.volumes.iter_mut().enumerate() {
                volume.label = format!("volume-{}", volume_index + 1);
                volume.aws_volume_id = None;
            }
        }
        Resource::Ec2Vm(vm) => {
            vm.shared.workload_name = workload_name;
            for (volume_index, volume) in vm.volumes.iter_mut().enumerate() {
                volume.label = format!("volume-{}", volume_index + 1);
                volume.aws_volume_id = None;
            }
        }
        Resource::Rds(rds) => rds.shared.workload_name = workload_name,
        Resource::OnPrem(on_prem) => on_prem.shared.workload_name = workload_name,
    }
    redacted
}

/// Bounded projection of an authoritative calculation.
#[derive(Debug, Serialize)]
struct CalculationView {
    formula_version: String,
    portfolio_totals: PortfolioTotals,
    warnings: Vec<String>,
    resources: Vec<CalculatedResourceView>,
}

#[derive(Debug, Serialize)]
struct CalculatedResourceView {
    resource_id: Uuid,
    mapping_status: Option<MappingStatus>,
    aws_pricing_status: PricingStatus,
    azure_pricing_status: PricingStatus,
    selected_target_configuration_key: Option<String>,
    unresolved_component_codes: Vec<String>,
}

impl CalculationView {
    fn from_revision(revision: &CalculationRevision) -> Self {
        Self {
            formula_version: revision.formula_version.clone(),
            portfolio_totals: revision.portfolio_totals.clone(),
            warnings: revision.warnings.clone(),
            resources: revision
                .resource_results
                .iter()
                .map(|result| CalculatedResourceView {
                    resource_id: result.resource_id,
                    mapping_status: result.mapping_status,
                    aws_pricing_status: result.aws_pricing_status,
                    azure_pricing_status: result.azure_pricing_status,
                    selected_target_configuration_key: result
                        .target_selection
                        .as_ref()
                        .and_then(|selection| selection.selected.as_ref())
                        .map(|target| target.configuration_key.clone()),
                    unresolved_component_codes: result
                        .unresolved_components
                        .iter()
                        .map(|component| component.code.clone())
                        .collect(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        calculation::target_selector::CapabilityCatalog,
        config::{AppEnvironment, Config},
        pricing::{
            coordinator::PricingCoordinator,
            local_fixture,
            repository::{
                DurableSnapshotRepository, InMemorySnapshotRepository, SnapshotRepositoryError,
            },
            snapshot::{AwsPriceSnapshot, AzurePriceSnapshot},
        },
    };

    const FORBIDDEN_FIELD_TERMS: &[&str] = &[
        "owner_id",
        "tenant_id",
        "\"tid\"",
        "\"oid\"",
        "etag",
        "if_match",
        "partition",
        "endpoint",
        "api_key",
        "access_token",
        "credential",
        "connection_string",
        "snapshot_id",
        "confirmation",
        "revision",
    ];

    fn schema(name: &str) -> Value {
        let definition = find(name).expect("the tool must be registered");
        serde_json::from_str(definition.parameters).expect("the schema must be valid JSON")
    }

    fn turn_context(phase: TurnPhase, with_project: bool) -> TurnContext {
        let context = TurnContext::new("entra:tenant:owner", Uuid::nil(), phase);
        if with_project {
            context.with_project(super::super::context::SelectedProject {
                id: Uuid::nil(),
                etag: "etag".to_owned(),
                aws_price_snapshot_id: None,
                azure_price_snapshot_id: None,
            })
        } else {
            context
        }
    }

    struct FindOnlyAwsSnapshotRepository {
        snapshot: AwsPriceSnapshot,
    }

    #[async_trait]
    impl DurableSnapshotRepository for FindOnlyAwsSnapshotRepository {
        async fn put_aws(
            &self,
            snapshot: &AwsPriceSnapshot,
        ) -> Result<AwsPriceSnapshot, SnapshotRepositoryError> {
            Ok(snapshot.clone())
        }

        async fn put_azure(
            &self,
            _snapshot: &AzurePriceSnapshot,
        ) -> Result<(), SnapshotRepositoryError> {
            Ok(())
        }

        async fn get_aws(
            &self,
            snapshot_id: &str,
        ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
            Ok((self.snapshot.metadata.snapshot_id == snapshot_id).then(|| self.snapshot.clone()))
        }

        async fn get_azure(
            &self,
            _snapshot_id: &str,
        ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
            Ok(None)
        }

        async fn find_aws(
            &self,
            currency: &str,
            source_region: &str,
        ) -> Result<Option<AwsPriceSnapshot>, SnapshotRepositoryError> {
            Ok(self
                .snapshot
                .matches_scope(currency, source_region)
                .then(|| self.snapshot.clone()))
        }

        async fn find_azure(
            &self,
            _currency: &str,
            _target_region: &str,
        ) -> Result<Option<AzurePriceSnapshot>, SnapshotRepositoryError> {
            Ok(None)
        }

        async fn list_latest_aws(&self) -> Result<Vec<AwsPriceSnapshot>, SnapshotRepositoryError> {
            Ok(Vec::new())
        }
    }

    fn test_state() -> AppState {
        AppState::in_memory(Config {
            bind_address: "127.0.0.1:0".parse().expect("bind address"),
            environment: AppEnvironment::Local,
            local_auth: None,
            cosmos: None,
            assistant: None,
            calculator_companion: None,
            web_asset_dir: PathBuf::from("rust/static"),
            guest_requests_per_minute: 60,
            provider_refreshes_per_hour: 8,
            provider_max_response_bytes: 64 * 1024 * 1024,
            calculation_concurrency: 10,
            assistant_requests_per_minute: 10,
        })
        .expect("in-memory application state")
    }

    fn reported_ec2_snapshot() -> AwsPriceSnapshot {
        let (mut snapshot, _) = local_fixture::load_for_runtime().expect("local price fixture");
        let anchor = snapshot
            .ec2_rate("r6id.8xlarge")
            .expect("EC2 fixture anchor")
            .clone();
        snapshot.ec2_rates = [
            ("r6id.12xlarge", 48, 384),
            ("r6id.8xlarge", 32, 256),
            ("r5.8xlarge", 32, 256),
            ("z1d.2xlarge", 8, 64),
        ]
        .into_iter()
        .map(|(instance_type, source_vcpu, memory_gb)| {
            let mut record = anchor.clone();
            record.stable_key = format!("eu-west-1|on-demand|shared|windows|{instance_type}");
            record.instance_type = instance_type.to_owned();
            record.rate.source_vcpu = source_vcpu;
            record.rate.catalog_memory_gb = decimal(memory_gb);
            record
        })
        .collect();
        snapshot
    }

    #[test]
    fn every_tool_schema_is_a_closed_object() {
        for tool in TOOLS {
            let schema: Value =
                serde_json::from_str(tool.parameters).expect("the schema must be valid JSON");
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(
                schema["additionalProperties"], false,
                "{} must reject unknown properties",
                tool.name
            );
            for required in schema["required"].as_array().unwrap_or(&Vec::new()) {
                let name = required.as_str().expect("required entries are strings");
                assert!(
                    schema["properties"].get(name).is_some(),
                    "{} requires undeclared property {name}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tool_names_are_unique_and_descriptions_fit_the_documented_limit() {
        let mut names = std::collections::HashSet::new();
        for tool in TOOLS {
            assert!(names.insert(tool.name), "duplicate {}", tool.name);
            assert!(!tool.description.trim().is_empty(), "{}", tool.name);
            assert!(
                tool.description.chars().count() <= MAX_TOOL_DESCRIPTION_CHARS,
                "{} description exceeds {MAX_TOOL_DESCRIPTION_CHARS} characters",
                tool.name
            );
        }
    }

    #[test]
    fn no_model_visible_schema_exposes_a_host_owned_field() {
        for tool in TOOLS {
            let surface = format!("{} {}", tool.parameters, tool.description).to_ascii_lowercase();
            for term in FORBIDDEN_FIELD_TERMS {
                assert!(
                    !surface.contains(term),
                    "{} exposes host-owned field {term}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn tools_are_exposed_only_in_their_approved_phases() {
        let stage = find("stage_project_patch").expect("stage tool");
        assert_eq!(stage.phase, TurnPhase::Propose);
        assert_eq!(stage.risk, ToolRisk::Draft);
        assert!(!stage.risk.is_mutating());
        assert!(!stage.risk.requires_confirmation());

        let read = turn_context(TurnPhase::ReadPlan, true);
        let propose = turn_context(TurnPhase::Propose, true);
        let execute = turn_context(TurnPhase::Execute, true);
        assert_eq!(schemas_for_context(&read).len(), 5);
        assert_eq!(schemas_for_context(&propose).len(), 6);
        assert_eq!(schemas_for_context(&execute).len(), 5);
        assert!(
            schemas_for_context(&read)
                .iter()
                .all(|schema| schema.name != "stage_project_patch")
        );
        assert!(
            schemas_for_context(&execute)
                .iter()
                .all(|schema| schema.name != "stage_project_patch")
        );
        assert!(
            schemas_for_context(&propose)
                .iter()
                .all(|schema| schema.name != "stage_new_project_draft")
        );
    }

    #[test]
    fn a_turn_without_a_project_exposes_only_context_free_tools() {
        let read = turn_context(TurnPhase::ReadPlan, false);
        let propose = turn_context(TurnPhase::Propose, false);
        let execute = turn_context(TurnPhase::Execute, false);
        assert_eq!(
            schema_names(&read),
            ["get_agent_capabilities", "get_application_help"]
        );
        assert_eq!(
            schema_names(&propose),
            [
                "get_agent_capabilities",
                "get_application_help",
                "stage_new_project_draft"
            ]
        );
        assert_eq!(
            schema_names(&execute),
            ["get_agent_capabilities", "get_application_help"]
        );
    }

    fn schema_names(context: &TurnContext) -> Vec<&'static str> {
        schemas_for_context(context)
            .into_iter()
            .map(|schema| schema.name)
            .collect()
    }

    fn classified_draft_schema(project_type: ProjectType) -> Value {
        let context =
            turn_context(TurnPhase::Propose, false).with_classified_project_type(project_type);
        let schema = schemas_for_context(&context)
            .into_iter()
            .find(|schema| schema.name == "stage_new_project_draft")
            .expect("classified draft schema");
        serde_json::from_str(&schema.parameters).expect("valid classified draft schema")
    }

    #[test]
    fn classified_draft_schemas_expose_only_matching_resource_fields() {
        let cases = [
            (
                ProjectType::Ec2,
                "ec2",
                &["instance_type", "volumes"][..],
                &["deployment", "source_vcpu"][..],
            ),
            (
                ProjectType::Rds,
                "rds",
                &["instance_type", "deployment", "source_max_iops"][..],
                &["volumes", "source_vcpu"][..],
            ),
            (
                ProjectType::OnPrem,
                "on_prem",
                &["source_vcpu", "licensable_cores", "hardware_capex_usd"][..],
                &["instance_type", "volumes", "deployment"][..],
            ),
        ];

        for (project_type, expected_name, present, absent) in cases {
            let schema = classified_draft_schema(project_type);
            assert_eq!(
                schema["properties"]["project_type"]["enum"],
                json!([expected_name])
            );
            assert_eq!(
                schema["properties"]["resources"]["items"]["properties"]["source_type"]["enum"],
                json!([expected_name])
            );
            let properties = schema["properties"]["resources"]["items"]["properties"]
                .as_object()
                .expect("resource properties");
            for field in present {
                assert!(
                    properties.contains_key(*field),
                    "{expected_name} needs {field}"
                );
            }
            for field in absent {
                assert!(
                    !properties.contains_key(*field),
                    "{expected_name} must not expose {field}"
                );
            }
        }
    }

    #[test]
    fn classified_sql_payg_schema_disallows_resources() {
        let schema = classified_draft_schema(ProjectType::SqlPayg);

        assert_eq!(
            schema["properties"]["project_type"]["enum"],
            json!(["sql_payg"])
        );
        assert_eq!(schema["properties"]["resources"]["maxItems"], json!(0));
    }

    #[test]
    fn a_new_on_prem_project_is_staged_as_a_valid_unsaved_draft() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"on_prem",
                "name":"Datacenter estimate",
                "resources":[{
                    "source_type":"on_prem",
                    "workload_name":"SQL estate",
                    "source_vcpu":64,
                    "licensable_cores":64
                }],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|_| None,
        );
        let proposal = outcome.proposal().expect("draft proposal");
        let AssistantProposal::NewProjectDraft(proposal) = proposal else {
            panic!("expected a new-project proposal");
        };

        assert_eq!(proposal.action, "open_project_draft");
        assert_eq!(proposal.project.name, "Datacenter estimate");
        assert_eq!(proposal.project.settings.project_type, ProjectType::OnPrem);
        assert_eq!(proposal.project.settings.aws_region, None);
        assert_eq!(proposal.project.settings.azure_region, "swedencentral");
        assert!(proposal.project.aws_price_snapshot_id.is_none());
        assert!(proposal.project.azure_price_snapshot_id.is_none());
        assert!(proposal.project.validate().is_empty());
        assert_ne!(proposal.project.resources[0].shared().id, Uuid::nil());
    }

    #[test]
    fn ec2_drafts_use_each_instance_types_catalog_ram_and_preserve_overrides() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2",
                "settings":{"aws_region":"eu-west-1"},
                "resources":[
                    {"source_type":"ec2","workload_name":"VM1","instance_type":"m6i.xlarge"},
                    {"source_type":"ec2","workload_name":"VM2","instance_type":"r6i.xlarge"},
                    {"source_type":"ec2","workload_name":"VM3","instance_type":"r6i.xlarge","source_ram_gb_per_instance":"48"}
                ],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed EC2 draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|instance_type| match instance_type {
                "m6i.xlarge" => Some(decimal(16)),
                "r6i.xlarge" => Some(decimal(32)),
                _ => None,
            },
        );
        let AssistantProposal::NewProjectDraft(proposal) =
            outcome.proposal().expect("catalog-hydrated draft proposal")
        else {
            panic!("expected a new-project proposal");
        };

        let source_ram = proposal
            .project
            .resources
            .iter()
            .map(|resource| resource.shared().source_ram_gb_per_instance)
            .collect::<Vec<_>>();
        assert_eq!(source_ram, vec![decimal(16), decimal(32), decimal(48)]);
    }

    #[tokio::test]
    async fn an_ec2_draft_resolves_catalog_ram_when_the_region_is_absent_from_the_latest_list() {
        let mut state = test_state();
        state.pricing = PricingCoordinator::new(
            InMemorySnapshotRepository::new(),
            Some(Arc::new(FindOnlyAwsSnapshotRepository {
                snapshot: reported_ec2_snapshot(),
            })),
            None,
            None,
            Arc::new(CapabilityCatalog {
                schema_version: "test".to_owned(),
                candidates: Vec::new(),
            }),
        );
        assert!(
            state
                .pricing
                .list_latest_aws()
                .await
                .expect("latest AWS snapshots")
                .is_empty()
        );

        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2",
                "settings":{"aws_region":"eu-west-1"},
                "resources":[
                    {"source_type":"ec2","instance_type":"r6id.12xlarge"},
                    {"source_type":"ec2","instance_type":"r6id.8xlarge"},
                    {"source_type":"ec2","instance_type":"r5.8xlarge"},
                    {"source_type":"ec2","instance_type":"z1d.2xlarge"}
                ],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed EC2 draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome =
            stage_new_project_draft(&state, &turn_context(TurnPhase::Propose, false), &input).await;
        let AssistantProposal::NewProjectDraft(proposal) =
            outcome.proposal().expect("catalog-hydrated draft proposal")
        else {
            panic!("expected a new-project proposal");
        };

        assert_eq!(proposal.action, "open_project_draft");
        assert_eq!(
            proposal
                .project
                .resources
                .iter()
                .map(|resource| resource.shared().source_ram_gb_per_instance)
                .collect::<Vec<_>>(),
            vec![decimal(384), decimal(256), decimal(256), decimal(64)]
        );
    }

    #[test]
    fn an_ec2_draft_without_visible_or_catalog_ram_reports_why_it_is_not_staged() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2",
                "resources":[{"source_type":"ec2","instance_type":"unknown.large"}],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed EC2 draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|_| None,
        );

        assert!(outcome.proposal().is_none());
        let (omissions, uncertainties) = outcome
            .extraction_notes()
            .expect("the failed draft returns its extraction report");
        assert!(omissions.is_empty());
        assert_eq!(uncertainties, [EC2_CATALOG_RAM_UNAVAILABLE_UNCERTAINTY]);
        assert_eq!(outcome.history_status(), "reported");
    }

    #[test]
    fn the_vm_draft_schema_hides_every_sql_input() {
        let schema = classified_draft_schema(ProjectType::Ec2Vm);

        assert_eq!(
            schema["properties"]["project_type"]["enum"],
            json!(["ec2_vm"])
        );
        let properties = schema["properties"]["resources"]["items"]["properties"]
            .as_object()
            .expect("resource properties");
        assert_eq!(properties["source_type"]["enum"], json!(["ec2_vm"]));
        for field in ["instance_type", "volumes"] {
            assert!(properties.contains_key(field), "ec2_vm needs {field}");
        }
        for field in SQL_ONLY_FIELDS {
            assert!(
                !properties.contains_key(*field),
                "ec2_vm must not expose {field}"
            );
        }
    }

    #[test]
    fn a_vm_resource_carrying_a_sql_input_is_refused_before_any_draft_is_built() {
        let definition = find("stage_new_project_draft").expect("registered");

        for field in [
            r#""sql_edition":"standard""#,
            r#""license_basis":"license_included""#,
            r#""sql_data_gb_per_instance":"512""#,
            r#""mi_purchase_option":"payg""#,
        ] {
            let arguments = format!(
                r#"{{
                    "project_type":"ec2_vm",
                    "resources":[{{"source_type":"ec2_vm","instance_type":"t3.large",{field}}}],
                    "omissions":[],
                    "uncertainties":[]
                }}"#
            );
            assert!(
                matches!(
                    parse_input(definition, &arguments),
                    Err(InvalidToolArguments::ResourceFieldMismatch)
                ),
                "{field} must not reach a VM draft"
            );
        }
    }

    #[test]
    fn a_vm_draft_stages_one_operating_system_disk_ahead_of_its_data_disks() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2_vm",
                "resources":[{
                    "source_type":"ec2_vm",
                    "workload_name":"VM1",
                    "instance_type":"r6id.12xlarge",
                    "volumes":[
                        {"volume_type":"gp3","capacity_gb":"1024"},
                        {"volume_type":"gp3","capacity_gb":"2048"}
                    ]
                }],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed VM draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|instance_type| (instance_type == "r6id.12xlarge").then(|| decimal(384)),
        );
        let AssistantProposal::NewProjectDraft(proposal) =
            outcome.proposal().expect("VM draft proposal")
        else {
            panic!("expected a new-project proposal");
        };

        assert_eq!(proposal.project.settings.project_type, ProjectType::Ec2Vm);
        assert_eq!(
            proposal.project.settings.aws_region.as_deref(),
            Some("eu-west-1")
        );
        assert!(proposal.project.validate().is_empty());

        let resource = &proposal.project.resources[0];
        assert!(
            resource.sql().is_none(),
            "a VM workload never carries SQL Server"
        );
        assert_eq!(resource.shared().source_ram_gb_per_instance, decimal(384));
        assert_eq!(resource.shared().annual_hours_per_instance, decimal(8_760));

        let Resource::Ec2Vm(resource) = resource else {
            panic!("expected an EC2 virtual machine resource");
        };
        assert_eq!(resource.instance_type, "r6id.12xlarge");
        assert_eq!(
            resource
                .volumes
                .iter()
                .map(|volume| volume.role)
                .collect::<Vec<_>>(),
            vec![VmDiskRole::Os, VmDiskRole::Data]
        );
        assert_eq!(
            resource
                .volumes
                .iter()
                .map(|volume| volume.provisioned_iops)
                .collect::<Vec<_>>(),
            vec![Some(GP3_BASELINE_IOPS), Some(GP3_BASELINE_IOPS)]
        );

        let (_, uncertainties) = outcome
            .extraction_notes()
            .expect("the staged draft returns its extraction report");
        assert_eq!(uncertainties, [GP3_ASSUMED_IOPS_UNCERTAINTY]);
    }

    #[test]
    fn a_vm_draft_without_visible_storage_still_stages_a_persistent_operating_system_disk() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2_vm",
                "resources":[{"source_type":"ec2_vm","instance_type":"t3.large"}],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed VM draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|_| Some(decimal(8)),
        );
        let AssistantProposal::NewProjectDraft(proposal) =
            outcome.proposal().expect("VM draft proposal")
        else {
            panic!("expected a new-project proposal");
        };

        assert!(proposal.project.validate().is_empty());
        let Resource::Ec2Vm(resource) = &proposal.project.resources[0] else {
            panic!("expected an EC2 virtual machine resource");
        };
        let [volume] = resource.volumes.as_slice() else {
            panic!("expected exactly one default volume");
        };
        assert_eq!(volume.role, VmDiskRole::Os);
        assert_eq!(volume.volume_type, EbsVolumeType::Gp3);
        assert_eq!(volume.label, "OS disk");
        assert_eq!(volume.provisioned_iops, Some(GP3_BASELINE_IOPS));
    }

    #[test]
    fn a_new_project_cannot_supply_host_ids_or_cross_source_fields() {
        let definition = find("stage_new_project_draft").expect("registered");
        assert!(
            parse_input(
                definition,
                r#"{"project_type":"on_prem","resources":[{"source_type":"on_prem","id":"11111111-1111-1111-1111-111111111111"}],"omissions":[],"uncertainties":[]}"#,
            )
            .is_err()
        );
        assert!(
            parse_input(
                definition,
                r#"{"project_type":"on_prem","resources":[{"source_type":"on_prem","instance_type":"r6i.2xlarge"}],"omissions":[],"uncertainties":[]}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn display_formatted_numbers_are_not_canonical_decimal_inputs() {
        let definition = find("stage_new_project_draft").expect("registered");
        let error = parse_input(
            definition,
            r#"{
                "project_type":"ec2",
                "resources":[{
                    "source_type":"ec2",
                    "annual_hours_per_instance":"6,240"
                }],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect_err("grouping separators are not accepted by the domain decimal type");

        assert_eq!(error, InvalidToolArguments::InvalidScalarValue);
    }

    #[test]
    fn image_storage_measurements_are_normalized_to_gb_by_the_host() {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "project_type":"ec2",
                "resources":[{
                    "source_type":"ec2",
                    "instance_type":"r5.2xlarge",
                    "sql_data_gb_per_instance":{"value":"1.5","unit":"tb"},
                    "volumes":[{
                        "volume_type":"gp3",
                        "capacity_gb":{"value":"2","unit":"tib"},
                        "provisioned_iops":3000
                    }]
                }],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed image draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        let outcome = stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|_| Some(decimal(64)),
        );
        let AssistantProposal::NewProjectDraft(proposal) =
            outcome.proposal().expect("normalized draft proposal")
        else {
            panic!("expected a new-project proposal");
        };
        let Resource::Ec2(resource) = &proposal.project.resources[0] else {
            panic!("expected an EC2 resource");
        };

        assert_eq!(resource.sql.sql_data_gb_per_instance, decimal(1_536));
        assert_eq!(resource.volumes[0].capacity_gb, decimal(2_048));
    }

    fn stage_image_volume(volume: &str) -> ToolOutcome {
        let definition = find("stage_new_project_draft").expect("registered");
        let input = parse_input(
            definition,
            &format!(
                r#"{{
                    "project_type":"ec2",
                    "resources":[{{
                        "source_type":"ec2",
                        "instance_type":"r6id.12xlarge",
                        "volumes":[{volume}]
                    }}],
                    "omissions":[],
                    "uncertainties":[]
                }}"#
            ),
        )
        .expect("typed image draft input");
        let ToolInput::StageNewProjectDraft(input) = input else {
            panic!("expected new-project input");
        };

        stage_new_project_draft_with_ec2_memory(
            &turn_context(TurnPhase::Propose, false),
            &input,
            &|_| Some(decimal(384)),
        )
    }

    fn staged_volume(outcome: &ToolOutcome) -> (&EbsVolume, &[String]) {
        let ToolOutcome::Staged {
            proposal,
            uncertainties,
            ..
        } = outcome
        else {
            panic!("expected a staged draft outcome");
        };
        let AssistantProposal::NewProjectDraft(proposal) =
            proposal.as_deref().expect("a reviewable project draft")
        else {
            panic!("expected a new-project proposal");
        };
        let Resource::Ec2(resource) = &proposal.project.resources[0] else {
            panic!("expected an EC2 resource");
        };
        (&resource.volumes[0], uncertainties)
    }

    #[test]
    fn image_gp3_volume_without_visible_iops_uses_the_included_aws_baseline() {
        let outcome =
            stage_image_volume(r#"{"volume_type":"gp3","capacity_gb":{"value":"1","unit":"tb"}}"#);
        let (volume, uncertainties) = staged_volume(&outcome);

        assert_eq!(volume.provisioned_iops, Some(3_000));
        assert_eq!(volume.capacity_gb, decimal(1_024));
        assert!(
            uncertainties
                .iter()
                .any(|note| note == GP3_ASSUMED_IOPS_UNCERTAINTY)
        );
    }

    #[test]
    fn image_io2_volume_without_visible_iops_uses_the_aws_minimum_and_flags_review() {
        let outcome = stage_image_volume(
            r#"{"volume_type":"io2","capacity_gb":{"value":"800","unit":"gb"}}"#,
        );
        let (volume, uncertainties) = staged_volume(&outcome);

        assert_eq!(volume.provisioned_iops, Some(100));
        assert!(
            uncertainties
                .iter()
                .any(|note| note == IO2_ASSUMED_IOPS_UNCERTAINTY)
        );
    }

    #[test]
    fn image_io2_volume_keeps_iops_that_were_extracted_from_the_image() {
        let outcome = stage_image_volume(
            r#"{"volume_type":"io2","capacity_gb":{"value":"800","unit":"gb"},"provisioned_iops":24000}"#,
        );
        let (volume, uncertainties) = staged_volume(&outcome);

        assert_eq!(volume.provisioned_iops, Some(24_000));
        assert!(
            !uncertainties
                .iter()
                .any(|note| note == IO2_ASSUMED_IOPS_UNCERTAINTY)
        );
    }

    #[test]
    fn selected_project_image_storage_measurements_use_the_same_normalization() {
        let definition = find("stage_project_patch").expect("registered");
        let input = parse_input(
            definition,
            r#"{
                "patch":{
                    "resources":[{
                        "source_type":"ec2",
                        "id":"11111111-1111-1111-1111-111111111111",
                        "workload_name":"SQL workload",
                        "server_name":null,
                        "quantity":1,
                        "sql_edition":"standard",
                        "license_basis":"byol",
                        "sql_data_gb_per_instance":{"value":"2","unit":"tb"},
                        "source_ram_gb_per_instance":"64",
                        "annual_hours_per_instance":"8760",
                        "mi_purchase_option":"ahb",
                        "instance_type":"r5.2xlarge",
                        "volumes":[{
                            "id":"22222222-2222-2222-2222-222222222222",
                            "label":"Data volume",
                            "aws_volume_id":null,
                            "volume_type":"gp3",
                            "capacity_gb":{"value":"1","unit":"tb"},
                            "provisioned_iops":3000,
                            "throughput_mibps":null
                        }]
                    }]
                },
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect("typed selected-project image patch");
        let ToolInput::StageProjectPatch(input) = input else {
            panic!("expected staged project patch input");
        };
        let Resource::Ec2(resource) = &input.patch.resources.expect("replacement resources")[0]
        else {
            panic!("expected an EC2 resource");
        };

        assert_eq!(resource.sql.sql_data_gb_per_instance, decimal(2_048));
        assert_eq!(resource.volumes[0].capacity_gb, decimal(1_024));
    }

    #[test]
    fn image_storage_schema_preserves_units_and_unsupported_units_fail_closed() {
        let schema = classified_draft_schema(ProjectType::Ec2);
        let measurement =
            &schema["properties"]["resources"]["items"]["properties"]["sql_data_gb_per_instance"];
        assert_eq!(measurement["type"], json!("object"));
        assert_eq!(
            measurement["properties"]["unit"]["enum"],
            json!(["gb", "gib", "tb", "tib"])
        );

        let definition = find("stage_new_project_draft").expect("registered");
        let error = parse_input(
            definition,
            r#"{
                "project_type":"rds",
                "resources":[{
                    "source_type":"rds",
                    "sql_data_gb_per_instance":{"value":"512","unit":"mb"}
                }],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect_err("unsupported source units must not be guessed");

        assert_eq!(error, InvalidToolArguments::UnknownVariant);
    }

    #[test]
    fn new_project_resources_must_match_the_locked_project_family() {
        let definition = find("stage_new_project_draft").expect("registered");
        let error = parse_input(
            definition,
            r#"{
                "project_type":"rds",
                "resources":[{"source_type":"ec2","instance_type":"r6i.2xlarge"}],
                "omissions":[],
                "uncertainties":[]
            }"#,
        )
        .expect_err("a resource cannot override the project family");

        assert_eq!(error, InvalidToolArguments::ResourceFieldMismatch);
    }

    #[test]
    fn mutating_risk_classes_always_require_confirmation() {
        assert!(ToolRisk::Persist.requires_confirmation());
        assert!(ToolRisk::Destructive.requires_confirmation());
        assert!(!ToolRisk::Read.requires_confirmation());
        assert!(!ToolRisk::Draft.requires_confirmation());
        assert!(!ToolRisk::Draft.is_mutating());
    }

    #[test]
    fn help_arguments_are_bounded_at_runtime() {
        let definition = find("get_application_help").expect("registered");

        assert!(parse_input(definition, r#"{"question":"What is NO MAPPING?"}"#).is_ok());
        assert!(parse_input(definition, r#"{"question":"   "}"#).is_err());
        assert!(parse_input(definition, r#"{"question":""}"#).is_err());
        assert!(parse_input(definition, "{}").is_err());
        assert!(parse_input(definition, "not json").is_err());

        let oversized = format!(
            r#"{{"question":"{}"}}"#,
            "a".repeat(help::MAX_QUESTION_CHARS + 1)
        );
        assert!(parse_input(definition, &oversized).is_err());
    }

    #[test]
    fn unknown_and_extra_arguments_are_rejected() {
        let help_tool = find("get_application_help").expect("registered");
        assert!(
            parse_input(
                help_tool,
                r#"{"question":"hello","owner_id":"entra:tenant:owner"}"#
            )
            .is_err()
        );

        let project_tool = find("get_current_project").expect("registered");
        assert!(parse_input(project_tool, r#"{"project_id":"any"}"#).is_err());
        assert!(parse_input(project_tool, "{}").is_ok());
        assert!(parse_input(project_tool, "").is_ok());

        let capabilities_tool = find("get_agent_capabilities").expect("registered");
        assert!(parse_input(capabilities_tool, "{}").is_ok());
        assert!(parse_input(capabilities_tool, "").is_ok());
        assert!(parse_input(capabilities_tool, r#"{"question":"what can you do"}"#).is_err());
    }

    #[test]
    fn a_patch_cannot_carry_price_snapshot_or_result_fields() {
        let definition = find("calculate_project_draft").expect("registered");

        assert!(
            parse_input(
                definition,
                r#"{"patch":{"aws_price_snapshot_id":"aws-2026-01-01"}}"#
            )
            .is_err()
        );
        assert!(
            parse_input(
                definition,
                r#"{"patch":{"latest_calculation_revision":{"formula_version":"1.0.0"}}}"#
            )
            .is_err()
        );
        assert!(parse_input(definition, r#"{"patch":{"name":"Estimate"}}"#).is_ok());
    }

    #[test]
    fn an_unregistered_tool_name_never_parses() {
        let borrowed = ToolDefinition {
            name: "run_sql",
            description: "unregistered",
            parameters: CURRENT_PROJECT_SCHEMA,
            phase: TurnPhase::ReadPlan,
            risk: ToolRisk::Read,
            project_requirement: ProjectRequirement::Any,
        };

        assert!(parse_input(&borrowed, "{}").is_err());
        assert!(find("run_sql").is_none());
    }

    #[test]
    fn the_patch_schema_declares_the_documented_workload_ceiling() {
        let schema = schema("validate_project_patch");

        assert_eq!(
            schema["properties"]["patch"]["properties"]["resources"]["maxItems"],
            json!(crate::domain::project::MAX_PROJECT_RESOURCES)
        );
        assert_eq!(
            schema["properties"]["patch"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn staged_patch_arguments_remain_closed_and_typed() {
        let definition = find("stage_project_patch").expect("registered");

        assert!(
            parse_input(
                definition,
                r#"{"patch":{"name":"Imported estimate"},"omissions":[],"uncertainties":[]}"#
            )
            .is_ok()
        );
        assert!(
            parse_input(
                definition,
                r#"{"patch":{"name":"Estimate"},"omissions":[],"uncertainties":[],"confirmed":true}"#
            )
            .is_err()
        );
        assert!(
            parse_input(
                definition,
                &format!(
                    r#"{{"patch":{{}},"omissions":["{}"],"uncertainties":[]}}"#,
                    "a".repeat(MAX_EXTRACTION_NOTE_CHARS + 1)
                )
            )
            .is_err()
        );
    }

    #[test]
    fn proposal_diffs_are_leaf_level_and_bounded() {
        let before = json!({"settings":{"azure_region":"swedencentral"}});
        let after = json!({"settings":{"azure_region":"southafricanorth"}});
        let mut changes = Vec::new();

        collect_changes("", Some(&before), Some(&after), &mut changes)
            .expect("one leaf change fits the bound");

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].pointer, "/settings/azure_region");
        assert_eq!(changes[0].before, Some(json!("swedencentral")));
        assert_eq!(changes[0].after, Some(json!("southafricanorth")));
    }

    #[test]
    fn a_staged_preview_is_not_serialized_into_model_context() {
        let outcome = ToolOutcome::Staged {
            proposal: Some(Box::new(
                ProjectPatchProposal {
                    action: "apply_project_patch",
                    patch: ProjectPatch {
                        name: Some("Confidential project".to_owned()),
                        ..ProjectPatch::default()
                    },
                    changes: vec![ProjectPatchChange {
                        pointer: "/name".to_owned(),
                        before: Some(json!("Current confidential name")),
                        after: Some(json!("Confidential project")),
                    }],
                }
                .into(),
            )),
            omissions: vec!["Private omitted value".to_owned()],
            uncertainties: Vec::new(),
        };

        assert_eq!(
            outcome.to_bounded_json(),
            r#"{"result":{"action":"apply_project_patch","report_recorded":true,"staged":true},"status":"ok"}"#
        );
        assert!(!outcome.to_bounded_json().contains("Private omitted value"));
    }

    #[test]
    fn an_oversized_tool_result_is_replaced_with_a_stable_code() {
        let outcome = ToolOutcome::Ok {
            result: json!({ "text": "a".repeat(MAX_TOOL_RESULT_CHARS) }),
        };

        assert_eq!(
            outcome.to_bounded_json(),
            r#"{"status":"error","code":"result_too_large"}"#
        );
    }

    #[test]
    fn a_help_outcome_serializes_with_its_control_references() {
        let input = ApplicationHelpInput {
            question: "What does the Azure region control?".to_owned(),
            control_id: None,
        };

        let serialized = application_help(&input).to_bounded_json();

        assert!(serialized.contains(r#""status":"ok""#));
        assert!(serialized.contains("project.azure-region"));
    }

    #[test]
    fn an_unknown_control_identifier_returns_a_stable_code() {
        let input = ApplicationHelpInput {
            question: "anything".to_owned(),
            control_id: Some("project.not-a-control".to_owned()),
        };

        assert_eq!(
            application_help(&input).to_bounded_json(),
            r#"{"status":"error","code":"unknown_control_id"}"#
        );
    }
}
