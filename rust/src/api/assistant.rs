//! Deterministic application help used directly by the UI and as a future agent tool.

use std::{cmp::Reverse, collections::HashSet};

use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::header,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{domain::project::ValidationIssue, problem::Problem};

const HELP_INSTANCE: &str = "/api/v1/assistant/help";
const MAX_QUESTION_CHARS: usize = 1_000;
const MAX_MATCHES: usize = 3;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelpRequest {
    question: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HelpReference {
    pub control_id: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct HelpResponse {
    pub answer: String,
    pub references: Vec<HelpReference>,
}

#[derive(Clone, Copy, Debug)]
struct HelpEntry {
    control_id: &'static str,
    label: &'static str,
    keywords: &'static [&'static str],
    explanation: &'static str,
}

const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        control_id: "projects.new",
        label: "New project",
        keywords: &[
            "new project",
            "create project",
            "new estimate",
            "create estimate",
        ],
        explanation: "New project starts an estimate for exactly one source type: Amazon EC2, Amazon RDS, or on premises.",
    },
    HelpEntry {
        control_id: "project.name",
        label: "Project name",
        keywords: &["project name", "estimate name", "rename"],
        explanation: "Project name identifies the estimate. It must contain 1 to 100 characters and does not affect any calculation.",
    },
    HelpEntry {
        control_id: "project.description",
        label: "Description",
        keywords: &["description", "project context", "notes"],
        explanation: "Description stores optional project context up to 500 characters. It is not used in pricing, sizing, or target selection.",
    },
    HelpEntry {
        control_id: "project.source-type",
        label: "Source estate",
        keywords: &["source estate", "source type", "ec2 or rds", "on premises"],
        explanation: "Source estate fixes the project as EC2, RDS, or on premises. Every workload in the project must use that same source type.",
    },
    HelpEntry {
        control_id: "project.aws-region",
        label: "AWS region",
        keywords: &["aws region", "source region", "amazon region"],
        explanation: "AWS region selects the public AWS prices used for every cloud workload in the project. On-premises projects do not use an AWS region.",
    },
    HelpEntry {
        control_id: "project.azure-region",
        label: "Azure region",
        keywords: &["azure region", "target region", "managed instance region"],
        explanation: "Azure region selects the public Azure SQL Managed Instance prices and available target catalog used by every workload in the project.",
    },
    HelpEntry {
        control_id: "project.save",
        label: "Save project",
        keywords: &["save", "save project", "save changes", "persist"],
        explanation: "Save project validates the current inputs and writes the project under the signed-in owner. Concurrent edits are protected by the project ETag.",
    },
    HelpEntry {
        control_id: "project.share",
        label: "Share project",
        keywords: &["share", "share project", "share link", "revoke link"],
        explanation: "Share project creates a reusable 30-day capability link. A recipient opens an unsaved snapshot and cannot modify the source project.",
    },
    HelpEntry {
        control_id: "project.calculate",
        label: "Calculate",
        keywords: &[
            "calculate",
            "run estimate",
            "recalculate",
            "annual comparison",
        ],
        explanation: "Calculate resolves approved price snapshots and runs the authoritative server-side decimal calculation for every workload and the portfolio totals.",
    },
    HelpEntry {
        control_id: "pricing.refresh",
        label: "Refresh prices",
        keywords: &[
            "refresh price",
            "fetch price",
            "latest price",
            "pricing status",
        ],
        explanation: "Refresh prices asks the server to resolve current public-list prices. The result records source, retrieval time, currency, and whether a cached snapshot was used.",
    },
    HelpEntry {
        control_id: "settings.default-annual-hours",
        label: "Default annual hours",
        keywords: &["default annual hours", "hours per year", "8760", "8784"],
        explanation: "Default annual hours supplies the initial yearly runtime for new workloads. Each workload can override it from 0 through 8,784 hours.",
    },
    HelpEntry {
        control_id: "settings.purchase-option",
        label: "Default Azure purchase option",
        keywords: &[
            "purchase option",
            "reservation",
            "reserved",
            "savings plan",
            "hybrid benefit",
            "ahb",
        ],
        explanation: "The default Azure purchase option initializes new workloads with the selected pay-as-you-go, reservation, savings-plan, and Azure Hybrid Benefit combination. Eligibility must be confirmed outside the calculator.",
    },
    HelpEntry {
        control_id: "settings.source-discounts",
        label: "Source discounts",
        keywords: &[
            "source discount",
            "aws discount",
            "compute discount",
            "license discount",
            "storage discount",
        ],
        explanation: "Source compute, license, and storage discounts independently reduce the matching public-list source components. Enter percentages from 0 through 100.",
    },
    HelpEntry {
        control_id: "settings.azure-discounts",
        label: "Azure discounts",
        keywords: &[
            "azure discount",
            "target discount",
            "azure compute",
            "azure license",
            "azure storage",
        ],
        explanation: "Azure compute, license, and storage discounts independently reduce the matching public-list Azure components. Enter percentages from 0 through 100.",
    },
    HelpEntry {
        control_id: "settings.parity-adjustment",
        label: "Selected parity adjustment",
        keywords: &["parity", "adjustment", "break even", "break-even"],
        explanation: "Selected parity adjustment applies the chosen 0 to 100 percent scenario to the comparison. The result also shows the signed adjustment required for cost parity.",
    },
    HelpEntry {
        control_id: "resources.add",
        label: "Add workload",
        keywords: &["add workload", "add resource", "new workload", "resource"],
        explanation: "Add workload creates another source workload of the project's fixed type. A project can contain at most 100 workloads.",
    },
    HelpEntry {
        control_id: "resource.workload-name",
        label: "Workload name",
        keywords: &["workload name", "resource name", "server name"],
        explanation: "Workload name labels a row for the user. It must contain 1 to 160 characters and is not sent to public pricing providers.",
    },
    HelpEntry {
        control_id: "resource.quantity",
        label: "Quantity",
        keywords: &["quantity", "instance count", "server count", "how many"],
        explanation: "Quantity prices identical source and target instances from 1 through 10,000. The calculator preserves quantity and does not consolidate HA pairs.",
    },
    HelpEntry {
        control_id: "resource.sql-edition",
        label: "SQL edition",
        keywords: &["sql edition", "standard edition", "enterprise edition"],
        explanation: "SQL edition identifies the source as Standard or Enterprise and controls applicable source licensing and target-selection rules.",
    },
    HelpEntry {
        control_id: "resource.license-basis",
        label: "Source license basis",
        keywords: &[
            "license basis",
            "license included",
            "byol",
            "bring your own license",
        ],
        explanation: "Source license basis states whether the source public price includes SQL Server licensing or uses BYOL. It does not prove Azure Hybrid Benefit eligibility.",
    },
    HelpEntry {
        control_id: "resource.sql-data",
        label: "SQL data per instance",
        keywords: &["sql data", "data size", "database size", "storage size"],
        explanation: "SQL data per instance records the modeled database capacity in GB. It is checked against Azure SQL Managed Instance storage limits during target selection.",
    },
    HelpEntry {
        control_id: "resource.source-ram",
        label: "Source RAM per instance",
        keywords: &["source ram", "memory", "ram gb"],
        explanation: "Source RAM per instance is a target-sizing constraint. The deterministic selector chooses a candidate that satisfies the required memory together with vCPU, IOPS, and capacity constraints.",
    },
    HelpEntry {
        control_id: "resource.annual-hours",
        label: "Annual hours per instance",
        keywords: &["annual hours", "runtime", "running hours", "uptime"],
        explanation: "Annual hours per instance controls yearly usage-based compute cost and must be between 0 and 8,784.",
    },
    HelpEntry {
        control_id: "resource.mi-purchase-option",
        label: "Workload Azure purchase option",
        keywords: &[
            "workload purchase",
            "mi purchase",
            "azure hybrid benefit",
            "reservation term",
        ],
        explanation: "The workload Azure purchase option overrides the project default for that row. The server resolves only an approved catalog option and keeps eligibility as a user responsibility.",
    },
    HelpEntry {
        control_id: "ec2.instance-type",
        label: "EC2 instance type",
        keywords: &["ec2 instance", "instance type", "ec2 size"],
        explanation: "EC2 instance type selects the source compute shape and its public Windows SQL Server price, vCPU, and memory characteristics.",
    },
    HelpEntry {
        control_id: "ec2.ebs-volumes",
        label: "EBS volumes",
        keywords: &["ebs", "volume", "gp3", "io2", "ephemeral"],
        explanation: "EBS volumes model gp3, io2, or ephemeral storage per EC2 instance. Capacity, provisioned IOPS, and throughput are priced and validated by volume type.",
    },
    HelpEntry {
        control_id: "ec2.volume-iops",
        label: "Provisioned IOPS",
        keywords: &["provisioned iops", "iops", "input output"],
        explanation: "Provisioned IOPS is required for gp3 and io2 volumes. It contributes to source storage pricing and constrains the Azure target tier and size.",
    },
    HelpEntry {
        control_id: "ec2.volume-throughput",
        label: "Volume throughput",
        keywords: &["throughput", "mibps", "mib per second"],
        explanation: "Volume throughput records the EBS throughput setting in MiB/s when supported. It affects source storage pricing but is not treated as an Azure target-selection constraint in v1.",
    },
    HelpEntry {
        control_id: "rds.instance-type",
        label: "RDS instance type",
        keywords: &["rds instance", "db instance", "rds size"],
        explanation: "RDS instance type selects the source database shape and its public SQL Server compute price, vCPU, and memory characteristics.",
    },
    HelpEntry {
        control_id: "rds.deployment",
        label: "RDS deployment",
        keywords: &["single az", "multi az", "deployment"],
        explanation: "RDS deployment selects Single-AZ or Multi-AZ public pricing. It does not change the number of Azure SQL Managed Instance targets beyond the entered quantity.",
    },
    HelpEntry {
        control_id: "rds.storage",
        label: "RDS storage configuration",
        keywords: &[
            "rds storage",
            "storage class",
            "source max iops",
            "rds iops",
        ],
        explanation: "RDS storage class and maximum IOPS describe the source storage. In v1, IOPS constrains Azure target selection but provisioned RDS IOPS and throughput charges are excluded from source cost.",
    },
    HelpEntry {
        control_id: "on-prem.compute",
        label: "On-premises compute",
        keywords: &[
            "source vcpu",
            "licensable core",
            "physical core",
            "on prem compute",
        ],
        explanation: "Source vCPU drives target sizing while licensable cores drive the modeled SQL Server License plus Software Assurance cost. They remain separate inputs.",
    },
    HelpEntry {
        control_id: "on-prem.hardware",
        label: "Hardware cost and depreciation",
        keywords: &["hardware capex", "hardware cost", "depreciation"],
        explanation: "Hardware CAPEX is annualized over the entered depreciation years and multiplied by workload quantity as part of on-premises source cost.",
    },
    HelpEntry {
        control_id: "on-prem.power",
        label: "Power and electricity",
        keywords: &["power", "electricity", "kilowatt", "kwh"],
        explanation: "The calculator estimates server power unless an average kW override is supplied, then applies annual hours, quantity, and the project electricity rate in USD/kWh.",
    },
    HelpEntry {
        control_id: "on-prem.license-sa",
        label: "License plus Software Assurance",
        keywords: &[
            "software assurance",
            "license sa",
            "two core pack",
            "coverage months",
        ],
        explanation: "Standard and Enterprise License plus Software Assurance prices are entered per two-core pack. Remaining coverage months control the annualized source licensing period and do not prove Azure benefit eligibility.",
    },
    HelpEntry {
        control_id: "results.annual-comparison",
        label: "Annual comparison",
        keywords: &[
            "annual comparison",
            "source total",
            "azure total",
            "savings",
            "result",
        ],
        explanation: "Annual comparison displays authoritative server-calculated source and Azure totals. Results are estimates based on public prices and the entered assumptions, not quotes.",
    },
    HelpEntry {
        control_id: "results.explanation",
        label: "Calculation explanation",
        keywords: &[
            "explain mapping",
            "why selected",
            "calculation explanation",
            "target decision",
        ],
        explanation: "Calculation explanation shows deterministic selection and cost steps, including accepted constraints and rejected candidates. It is not generated by a model.",
    },
    HelpEntry {
        control_id: "results.no-mapping",
        label: "NO MAPPING",
        keywords: &["no mapping", "unavailable target", "cannot map"],
        explanation: "NO MAPPING means no approved Azure SQL Managed Instance candidate satisfies the requested tier and known vCPU, RAM, IOPS, and storage constraints. Source cost remains visible.",
    },
    HelpEntry {
        control_id: "privacy.notice",
        label: "Privacy and data use",
        keywords: &["privacy", "data use", "contact permission", "consent"],
        explanation: "Privacy and data use describes what the application processes, safeguards, retention, and optional contact permission. Required notice acceptance is versioned for signed-in use.",
    },
    HelpEntry {
        control_id: "guest.browser-data",
        label: "Browser data",
        keywords: &["browser data", "local draft", "guest draft", "clear local"],
        explanation: "A guest draft is stored only in IndexedDB on the current browser profile. Clear local draft permanently removes that browser copy.",
    },
];

/// Answer a bounded natural-language question from the reviewed help catalog.
pub async fn help(request: Result<Json<HelpRequest>, JsonRejection>) -> Result<Response, Problem> {
    let request = request.map_err(|error| super::json_rejection(error, HELP_INSTANCE))?;
    let help = answer_question(&request.question)
        .map_err(|issues| Problem::validation(HELP_INSTANCE, issues))?;
    let mut response = Json(help).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    Ok(response)
}

fn answer_question(question: &str) -> Result<HelpResponse, Vec<ValidationIssue>> {
    let question = question.trim();
    let question_chars = question.chars().count();
    if !(1..=MAX_QUESTION_CHARS).contains(&question_chars) {
        return Err(vec![ValidationIssue {
            pointer: "/question".to_owned(),
            code: "length",
            message: format!("Question must contain 1 to {MAX_QUESTION_CHARS} characters."),
        }]);
    }

    let normalized = normalize(question);
    let question_terms = terms(&normalized);
    let mut scored = HELP_ENTRIES
        .iter()
        .filter_map(|entry| {
            let score = score_entry(entry, &normalized, &question_terms);
            (score > 0).then_some((score, entry))
        })
        .collect::<Vec<_>>();
    scored.sort_by_key(|(score, entry)| (Reverse(*score), entry.control_id));

    let matches = scored
        .into_iter()
        .take(MAX_MATCHES)
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return Ok(HelpResponse {
            answer: "I could not match that question to an application control. Ask about a field or command such as Azure region, quantity, discounts, purchase options, Calculate, Share project, or NO MAPPING."
                .to_owned(),
            references: Vec::new(),
        });
    }

    let answer = matches
        .iter()
        .map(|entry| format!("{}: {}", entry.label, entry.explanation))
        .collect::<Vec<_>>()
        .join("\n\n");
    let references = matches
        .iter()
        .map(|entry| HelpReference {
            control_id: entry.control_id,
            label: entry.label,
        })
        .collect();
    Ok(HelpResponse { answer, references })
}

fn score_entry(entry: &HelpEntry, normalized: &str, question_terms: &HashSet<&str>) -> usize {
    let phrase_score = entry
        .keywords
        .iter()
        .filter(|keyword| normalized.contains(**keyword))
        .map(|keyword| keyword.split_whitespace().count() * 4)
        .sum::<usize>();
    let label = normalize(entry.label);
    let label_score = terms(&label).intersection(question_terms).count() * 2;
    let keyword_score = entry
        .keywords
        .iter()
        .flat_map(|keyword| keyword.split(|character: char| !character.is_ascii_alphanumeric()))
        .filter(|term| term.len() >= 3 && question_terms.contains(*term))
        .count();
    phrase_score + label_score + keyword_score
}

fn normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn terms(value: &str) -> HashSet<&str> {
    const STOP_WORDS: &[&str] = &[
        "about", "does", "field", "help", "how", "tell", "that", "the", "this", "what", "when",
        "where", "which", "why", "with",
    ];
    value
        .split_whitespace()
        .filter(|term| term.len() >= 3 && !STOP_WORDS.contains(term))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_language_question_matches_the_reviewed_control() {
        let response = answer_question("What does the Azure region control?")
            .expect("a bounded help question should be valid");

        assert_eq!(response.references[0].control_id, "project.azure-region");
        assert!(
            response
                .answer
                .contains("public Azure SQL Managed Instance prices")
        );
    }

    #[test]
    fn financial_explanation_stays_deterministic() {
        let response = answer_question("Why was this target selected?")
            .expect("a bounded help question should be valid");

        assert_eq!(response.references[0].control_id, "results.explanation");
        assert!(response.answer.contains("It is not generated by a model"));
    }

    #[test]
    fn unknown_question_returns_a_safe_fallback() {
        let response = answer_question("quasar nebula spectroscopy")
            .expect("an unknown but bounded question should be valid");

        assert!(response.references.is_empty());
        assert!(response.answer.contains("could not match"));
    }

    #[test]
    fn question_length_is_bounded() {
        let empty = answer_question("   ").expect_err("an empty question must be rejected");
        assert_eq!(empty[0].pointer, "/question");

        let oversized = "a".repeat(MAX_QUESTION_CHARS + 1);
        let issues = answer_question(&oversized).expect_err("an oversized question must fail");
        assert_eq!(issues[0].code, "length");
    }

    #[test]
    fn help_control_ids_are_unique() {
        let mut identifiers = HashSet::new();
        for entry in HELP_ENTRIES {
            assert!(
                identifiers.insert(entry.control_id),
                "duplicate {}",
                entry.control_id
            );
            assert!(!entry.label.trim().is_empty());
            assert!(!entry.keywords.is_empty());
            assert!(!entry.explanation.trim().is_empty());
        }
    }
}
