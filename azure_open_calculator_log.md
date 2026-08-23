# Azure Pricing Calculator live-run log

Status: complete
Run date: 2026-08-23

## Scope

Create an Azure Pricing Calculator estimate from the latest saved calculation selected by the user, exercise the calculator through its visible controls, and evaluate whether the workflow is suitable for an Azure-hosted browser automation service.

## Data handling

- The user explicitly authorized this one-time browser entry of target-only calculator configuration.
- The calculator item labels match the source project's workload names as explicitly requested; this log does not repeat those names.
- This log records actions and outcomes but omits project and workload names, source inventory, exact target values, totals, owner identifiers, tenant identifiers, cookies, tokens, credentials, and request or response bodies.
- No calculator credentials are requested, captured, stored, or automated. Any calculator sign-in must be completed directly by the user in the Microsoft-owned browser page.
- The named administrative account is not treated as a service credential. A proposed hosted service requires a separately approved identity and security design.

## Steps

| UTC | Step | Outcome |
| --- | --- | --- |
| 09:42 | Read the latest saved calculation after the user refreshed prices, recalculated, and saved. | Seven mapped target rows were present with fresh pricing status. |
| 09:42 | Derived calculator groups from the server-rendered workbook detail. | Seven distinct groups are required; none can be combined without changing a calculator-relevant field. |
| 09:42 | Applied data-minimization and authentication boundaries. | Only target controls will be entered. Calculator login and administrative credentials remain user-controlled. |
| 09:46 | Verified calculator authentication after the user completed Microsoft-hosted sign-in. | Account actions were enabled in a clean, empty estimate. No account identifier or session material was captured. |
| 09:46 | Inspected the signed-in SQL Managed Instance defaults before configuration. | The calculator defaulted to a three-year reservation with Azure Hybrid Benefit; the run overrides both explicitly to match the authoritative PAYG, license-included revision. |
| 09:46 | Resolved redundancy, usage, memory, and storage mappings. | Catalog `zone_redundant: false` maps to Locally Redundant; full-year usage maps to 730 hours/month; selected RAM uses the calculator's discrete memory control. Data storage must round upward to 32-GB units, and point-in-time backup remains at the calculator's 1-GB minimum. |
| 09:46 | Added and verified the first calculator item. | Region, tier, hardware, vCores, selected memory, redundancy, quantity, usage, PAYG compute, license included, and rounded storage matched the intended target-only mapping. |
| 09:48 | Added and individually verified the remaining six calculator items. | Each distinct target configuration was entered through visible calculator controls; long-term retention was set to zero and no additional IOPS were added. |
| 09:48 | Ran an all-items assertion before save. | Seven items were present and all required fields matched the target-only manifest with zero mismatches. |
| 09:59 | Replaced temporary calculator line labels with the source project's workload names. | Seven project names were mapped by authoritative project row order; a full post-rename assertion found zero name or configuration mismatches. The signed-in account identifier was not used as a line label or recorded. |
| 10:00 | Saved the estimate under the source project's estimate name. | The Microsoft-hosted calculator confirmed that the estimate was saved. No share link was created or captured. |
| 10:01 | Opened the saved-estimates view and reopened the saved estimate. | The saved entry restored all seven SQL Managed Instance items; a post-open manifest assertion found zero name or configuration mismatches. |
| 10:08 | Reconciled the first item's calculator total with the application result. | Compute, license, and additional-memory components matched. The difference was isolated to data storage plus the calculator's minimum point-in-time-restore entry. |
| 10:08 | Queried current public Azure SQL Managed Instance storage meters and reviewed the normalizer. | The application classifies an Additional IOPS meter as data storage because the predicate accepts an IOPS-per-month unit, then selects it as the minimum rate. The application also omits the calculator's 32-GB storage rounding, free first storage unit, and minimum backup entry. |
| 10:08 | Cross-checked current Microsoft documentation. | Official guidance treats configured storage and additional IOPS as separate billable dimensions, requires configured storage in 32-GB multiples, and identifies backup storage as a separate dimension. |

## Architecture note

This run is evidence about technical browser behavior, not approval for a production service. A Container Apps design that adds browser binaries, persistent profiles, a shared user account, customer-derived egress, or calculator login would change the approved runtime, identity, privacy, dependency, and operational boundaries and requires written specification, Security, Privacy, architecture, software, and service-owner approval.

The signed-in route adds account persistence but does not provide a supported estimate-write API or a managed-identity authentication path. A shared-user browser session would remain an account credential and session-management design, not Azure workload identity. The current application result must not be used for customer-facing SQL Managed Instance storage estimates until the storage-meter classification and billing-granularity defects are corrected and covered by focused parity tests.