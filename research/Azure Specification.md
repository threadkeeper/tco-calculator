# Azure SQL TCO Web Application Specification

Status: Implementation-ready MVP specification  
Target implementer: GitHub Copilot coding agent using GPT 5.6 Sol  
Backend: Rust  
Frontend: TypeScript with Svelte 5 and SvelteKit  
Hosting: Microsoft Azure  
Default Azure SQL migration region: Sweden Central (`swedencentral`), selected once per project from the reviewed public-region catalog

## 1. Purpose

Build a production-quality web application that reproduces the validated AWS SQL Server to Azure SQL Managed Instance total-cost-of-ownership calculations currently implemented in `SQL TCO Calculator.xlsx` and `Add-CloudTcoConverters.py`.

The application must:

1. Calculate AWS RDS, AWS EC2, and On-premises SQL Server source costs.
2. Derive an Azure SQL Managed Instance target deterministically.
3. Fetch current AWS and Azure public prices at runtime.
4. show independent `Fetching AWS prices` and `Fetching Azure prices` states.
5. Let anonymous guest users calculate without saving.
6. Let Microsoft Entra-authenticated users create, open, update, and delete saved projects.
7. Explain, per resource, exactly why an Azure SKU was selected or why no valid mapping exists.
8. Deploy as one Azure Container App containing a Rust API and a statically built Svelte frontend.
9. Provide Entra-authenticated users with a Foundry-backed assistant that explains the application, accepts an image as the primary v1 assisted project-input method, and performs allowlisted application actions through server-owned controls.

This is an operational calculator, not a marketing site. The first screen must be the usable project screen or project list.

## 2. Normative Language

`MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, and `MAY` are normative.

Questions that can alter product behavior are recorded in `design clarificaitons.md`. Until answered, the defaults stated in this specification are binding implementation assumptions.

## 3. Source of Truth

Business behavior must be ported from these workspace artifacts:

- `Add-CloudTcoConverters.py`: formulas, defaults, validation, target selection, and no-mapping behavior.
- `SQL TCO Calculator.xlsx`: validated presentation and sample outcomes.
- `Generate-Ec2Csv.ps1`: public AWS EC2 pricing-calculator catalog ingestion.
- `Generate-RdsCsv.ps1`: public AWS Price List Bulk API ingestion.
- `Generate-SqlMiCsv.ps1`: Azure Retail Prices API ingestion and configured SQL MI option composition.
- `Generate-Ec2SqlMiMapping.ps1` and `Generate-RsdSqlMiMapping.ps1`: SQL MI capability and source-to-target mapping inputs.
- `EC2.csv`, `RDS.csv`, `SQLMI.csv`, `EC2_SQLMI_MAPPING.csv`, and `RDS_SQLMI_MAPPING.csv`: frozen fixtures for parity tests, not the production live-price source.

Useful engineering practices reviewed in `C:\Repos\gaia-robot` and adopted here are listed in section 20.

## 4. Scope

### 4.1 Included in MVP

- AWS EC2 running Microsoft SQL Server.
- AWS RDS for SQL Server.
- On-premises SQL Server with user-entered infrastructure, licensing, and electricity assumptions.
- Separate `EC2`, `RDS`, and `On-prem` project types; one project contains resources of exactly one source type.
- EC2 EBS `gp3`, `io2`, and ephemeral volumes.
- RDS Single-AZ and Multi-AZ deployments.
- SQL Server Standard and Enterprise source editions.
- Source license basis of `License included` or `BYOL`.
- Azure SQL Managed Instance Next Generation General Purpose and Business Critical targets.
- Eight Azure purchase options listed in section 10.6.
- Independent source and Azure component discounts using the six workbook fields.
- Portfolio parity calculation.
- Entra sign-in and user-private project persistence.
- Anonymous guest calculations with browser-local drafts and no server-side project persistence.
- Deterministic per-resource calculation explanations.
- Live public-list pricing with cached fallback and price provenance.
- A bottom-right assistant for Entra-authenticated users, implemented as an application-owned bounded tool loop backed by an approved Azure AI Foundry Model Router deployment.
- JPEG and PNG upload as the primary v1 assisted-input path. The assistant produces a validated, user-reviewable project draft or patch and MUST NOT save it automatically.
- Allowlisted assistant actions that reuse the same server-side identity, owner scoping, validation, decimal calculation, ETag, persistence, and confirmation boundaries as the normal UI.

### 4.2 Assistant Boundaries

- The Rust application owns the autonomous loop, capability registry, authorization, deadlines, confirmation state, and side effects. Foundry supplies inference only.
- Model-generated output is untrusted input. It MUST NOT calculate or supply authoritative money, rates, savings, adjustments, target SKUs, eligibility, totals, revisions, or deterministic explanations.
- Every financial result and target selection MUST come from the existing server-side decimal calculation and target-selection modules.
- Assistant model calls, image upload, and application actions require an Entra-authenticated principal. Guest users MAY receive deterministic local help but MUST NOT cause model or file egress.
- The v1 image path accepts one bounded JPEG or PNG, removes metadata, constrains decoded dimensions, sends only the minimum normalized image and context to the approved Foundry deployment, and discards raw and normalized bytes when the request ends.
- Image extraction produces a typed candidate project draft or patch. The normal domain validator MUST reject unsupported, ambiguous, or invalid fields, and the UI MUST show mappings, omissions, uncertainty, and a field-level preview before applying changes to a draft.
- Persisted, destructive, sharing, and other high-impact actions require a separate host-validated confirmation. A natural-language request alone is not confirmation.
- Tool schemas MUST be closed and typed. Identity, owner, partition, ETag, confirmation, endpoint, credential, and authorization data MUST come from immutable host context and MUST NOT be model-authored.
- The loop MUST enforce model-call, tool-call, batch, token, output, concurrency, cancellation, and whole-turn time budgets. It MUST stop on stale state, authorization failure, malformed output, guardrail response, unavailable dependency, or exhausted budget.
- Conversation and upload content MUST remain request-scoped in v1. Do not add server-side transcripts, browser-durable chat history, embeddings, vector indexes, or retrieval stores.
- Treat project text, uploaded content, model output, and tool results as untrusted data rather than instructions. Do not expose arbitrary HTTP, DOM selectors, script, SQL, shell, provider-console, credential, or Azure control-plane capabilities.
- Use the exact data-flow, threat, egress, testing, rollout, and approval controls in `docs/FOUNDRY-ASSISTANT-APPROVAL-PROPOSAL.md` as normative implementation requirements where they do not conflict with this specification.

### 4.3 Explicitly Excluded from MVP

- AWS report-server workloads that were removed from the workbook.
- Azure SQL Database, SQL Server on Azure VM, PostgreSQL, MySQL, Oracle, or non-SQL workloads.
- Automated database feature-compatibility assessment.
- Actual migration execution.
- Network, backup, snapshot, support, migration-labor, or operational cost modeling.
- RDS provisioned IOPS and throughput charges. IOPS affects Azure tier selection but is not added to AWS RDS cost in MVP.
- Taxes and foreign-exchange conversion. Currency is USD.
- Model-generated financial calculations, sizing decisions, target selections, prices, or licensing advice. Explanations remain deterministic calculation traces.
- Application administrator UI, privileged project access, and global price-cache controls.
- Collaboration, organization workspaces, invitations, role assignment, and ownership transfer. Authenticated capability-link sharing is included only as specified in section 6.5.
- CSV, Excel, and PDF import, native Excel workbook export, and project duplication. Client-side CSV result export is included only as specified in section 7.6. JPEG and PNG assisted input is included only as specified in section 4.2.
- AWS or Azure write access.
- Provider-console login in MVP. Public price feeds are sufficient for the modeled retail prices.

## 5. Users and Access Modes

### 5.1 Guest Mode

A guest is an unauthenticated user.

A guest MUST be able to:

- Create one temporary project.
- Edit project settings.
- Add, edit, and delete resources.
- Fetch public AWS and Azure prices.
- Calculate all totals and inspect explanations.

A guest MUST NOT be able to:

- Save a project to the backend.
- Open a saved project.
- Delete a saved project.
- Access another user's data.

Guest project state MUST be stored only in browser-local durable storage and MUST survive reloads and browser restarts on the same browser profile. Prefer IndexedDB for the structured project draft. The draft MUST NOT be synchronized to the backend or represented as a saved account project. The UI MUST state that the draft exists only on the current device and provide an explicit action, with confirmation, to clear it.

### 5.2 Authenticated Mode

Authentication MUST use Microsoft Entra ID through Azure Container Apps built-in authentication.

- Unauthenticated access MUST remain allowed for guest endpoints.
- Sign-in URL: `/.auth/login/aad?post_login_redirect_uri=/`.
- Sign-out URL: `/.auth/logout?post_logout_redirect_uri=/`.
- MVP authentication is multi-tenant for work or school accounts from any Microsoft Entra tenant. The app registration MUST use the `AzureADMultipleOrgs` sign-in audience; personal Microsoft accounts are not in scope. Container Apps authentication MUST validate the application audience and use the Microsoft identity platform's multi-tenant issuer. The application MUST NOT maintain a tenant allow-list.
- The frontend MUST use the Container Apps platform login and logout routes. It MUST NOT add MSAL or store, refresh, or forward Entra access tokens. The Rust backend MUST rely only on platform-validated principal headers at the protected ingress boundary and MUST NOT implement a second OAuth flow.
- The backend MUST derive the owner identifier from the platform-injected `X-MS-CLIENT-PRINCIPAL` claims and require both the stable Entra tenant ID (`tid`) and object ID (`oid`). The persisted owner identifier MUST be an opaque composite of both values, such as `entra:{tid}:{oid}`, because object IDs are unique only within a tenant. Malformed principals or principals missing either claim MUST be rejected.
- Direct access that bypasses Container Apps authentication MUST be blocked in Azure. Externally supplied identity headers MUST never be trusted.
- The backend MUST ignore any client-supplied owner identifier.
- Project routes MUST return `401` when no authenticated principal exists.
- Every successfully authenticated Entra user MUST be able to create, open, update, and delete their own saved projects. MVP MUST NOT require a paid entitlement, app role, group membership, or administrator assignment for save capability.
- Saved projects MUST be private to the authenticated principal. The only cross-principal disclosure is an explicit 30-day capability link created by the owner; it returns an editable snapshot without owner metadata and never grants access to mutate the source project.

For local development only, a clearly marked mock principal MAY be enabled with `APP_ENV=local`, `LOCAL_AUTH_TENANT_ID`, `LOCAL_AUTH_OWNER_ID`, and `LOCAL_AUTH_DISPLAY_NAME`. The mock path bypasses header parsing but MUST construct the same tenant-plus-object owner identifier and preserve owner-scoped persistence tests. Startup MUST fail if any local-auth setting is present while `APP_ENV` is not `local`.

Before a newly authenticated user can use any application API other than session and privacy-consent endpoints, the user MUST review and accept the current version of the app-specific privacy and data-use notice. A changed notice version requires acceptance again. The acceptance control and the optional `Microsoft may contact me about my interest in Azure SQL` control MUST be separate; contact permission defaults to false and MUST NOT be required to continue. Guests can display the notice but MUST NOT be prompted to accept it.

The consent record MUST persist in Cosmos under the server-derived owner ID. It contains the notice version, acceptance timestamp, contact choice, optional Entra display name, and an email address only when contact permission is true. Prefer an email-like Entra claim for form prefill; if unavailable, permit the user to enter the contact address. Email is contact metadata, never an authorization key. The app MUST discard rather than persist an email when contact permission is false. No application endpoint may list or export contact opt-ins in v1; any operator retrieval requires a separately reviewed, authorized, and audited process.

The notice is an internal-pilot, app-specific disclosure that supplements the Microsoft Privacy Statement. It MUST accurately disclose user-created project shares, Azure/service-provider processing, and legally required disclosures, and MUST NOT promise that data is never disclosed to any third party. Privacy and Legal approval of the controller identity, notice wording, privacy contact, and retention terms is required before external or production use.

### 5.3 Provider Credentials

The MVP MUST NOT request an AWS console login, Azure portal login, AWS access key, Azure subscription credential, or Azure Calculator login from an end user.

The production price adapters use public read-only endpoints. A future credentialed-price adapter MAY be added behind an interface if negotiated account pricing is later required. It must not alter the MVP's public-price behavior.

## 6. Primary User Flows

### 6.1 Main Screen

The main screen MUST show:

- Product name `Azure SQL TCO` with a neutral text wordmark, plus build version.
- Current identity state: `Guest` or the Entra display name.
- `Sign in with Microsoft` or `Sign out` action.
- Privacy/data-use icon that opens the current notice without requiring guest acceptance.
- Repository icon linking to `https://github.com/threadkeeper/tco-calculator`.
- Primary `New project` action.
- For authenticated users, a project table with name, project type, modified time, source region where applicable, Azure region, resource count, source annual total, Azure annual total, and actions.
- Open action for each saved project.
- Delete icon action for each saved project with confirmation.
- Empty state when no projects exist.
- For guests, a concise notice that temporary projects cannot be saved.

The screen MUST NOT use a marketing hero or feature-description cards.

### 6.2 Create Project

`New project` opens a compact dialog requesting:

- Project name, required, 1 to 100 characters.
- Optional description, maximum 500 characters.
- Project type: `EC2`, `RDS`, or `On-prem`.
- AWS source region for EC2 and RDS projects, default `eu-west-1`. The region applies to every resource in the project.
- Azure migration region, default `Sweden Central`. The selected supported region applies to every resource in the project.

Authenticated users create a persisted EC2 or RDS project. Because On-prem License + SA prices are required customer inputs and are not collected in this dialog, an authenticated On-prem project opens as an unsaved workspace; its first explicit save persists it only after both prices are greater than zero. Guests create a temporary browser-local project draft.

### 6.3 Open Project

Opening a project MUST:

1. Render the saved settings, resources, last successful results, and price timestamps immediately.
2. Show whether each provider price snapshot is fresh, cached, stale, or unavailable.
3. Never silently change saved calculation results.
4. Allow the user to select `Refresh prices` to fetch current prices and create a new calculation revision.

Saved projects MUST NOT refresh prices automatically on open. Refresh prices only when the user explicitly selects `Refresh prices`.

### 6.4 Delete Project

- Delete MUST require confirmation containing the project name.
- Delete MUST be owner-scoped and MUST revoke every outstanding share before and after deleting the source project. Share creation MUST recheck the owner-scoped source after persisting the share and remove the share if a concurrent deletion won.
- Successful deletion returns to the main screen.
- MVP uses immediate hard delete with no recovery tombstone or retained application audit copy.

### 6.5 Share Project

- Only the source-project owner may create or revoke a share.
- A share link is reusable until it expires 30 days after creation or the owner revokes it.
- Any authenticated Entra user who possesses the complete link may open the shared editable snapshot. Guests MUST be directed to sign in before the snapshot is disclosed.
- The share secret MUST be generated independently from the share ID, stored only as a SHA-256 digest, placed in the browser URL fragment so it is not sent in HTTP request targets, and exchanged with the API only in a bounded JSON request body. It MUST NOT appear in logs.
- Opening a share MUST NOT disclose the source owner ID, source project ID, ETag, or saved calculation revision and MUST NOT grant update or delete access to the source project.
- The opened snapshot is an unsaved copy. Edits remain local to that workspace until the recipient selects `Save project`, which creates a new project under the recipient's server-derived owner ID. Source prices are referenced only by server-issued snapshot IDs and all persisted calculations remain server-authoritative.
- Expiry MUST be checked server-side before disclosure. Expired links return `410 Gone`; invalid, revoked, or incorrectly keyed links return `404 Not Found`. Because v1 does not change the existing `projects` container TTL policy, an expired share record is removed when encountered rather than relying on automatic Cosmos TTL.

### 6.6 Add Resource

`Add resource` MUST open a modal or right-side drawer with stable dimensions and these stages:

1. Use the source type fixed by the project: `AWS EC2 SQL Server`, `AWS RDS for SQL Server`, or `On-premises SQL Server`.
2. Enter source compute and licensing details.
3. Enter source details: repeatable EBS volumes for EC2, storage class/data size/source max IOPS for RDS, or hardware CAPEX/depreciation/power assumptions for On-prem.
4. Review derived source metadata and submit.

Source RAM is an AWS workload input. MI RAM, MI vCores, MI hardware/storage architecture, and MI service tier are outputs only and MUST NOT appear as editable controls.

Numeric edit controls use ungrouped invariant values while focused and locale-aware grouping when rendered read-only or blurred. `SQL data GB / instance` MUST never render a stray leading comma; for example, one thousand is `1,000`, not `,1000`.

Submitting MUST:

- Validate input client-side and server-side.
- Resolve current AWS and Azure pricing if a usable snapshot is not already selected.
- Calculate the result in Rust.
- Add one row to the resource table.
- Show a deterministic target or `NO MAPPING`.

### 6.7 Edit and Delete Resource

Every row MUST have icon buttons with accessible labels and tooltips:

- Edit.
- Delete.
- Information / calculation explanation.

Delete requires confirmation. Edit reopens the same form and recalculates after save.

### 6.8 Inspect Mapping Explanation

The information icon MUST open a drawer that shows:

- Source inputs used.
- Source vCPU and RAM resolved from the selected AWS SKU.
- Source IOPS derivation.
- Candidate Azure tier decision.
- IOPS threshold and comparison.
- Candidate shapes considered and rejection reasons.
- Selected Azure region, tier, hardware, vCores, included RAM, selected RAM, and storage architecture.
- Price source URL, source version/effective date, retrieval time, currency, and cache status for every cost component.
- Formula inputs and results for AWS compute, license, storage, Azure compute, additional RAM, license, storage, savings, and parity.
- Warnings, exclusions, and `NO MAPPING` reason where applicable.

The explanation MUST be generated from structured calculation steps, not from an LLM.

## 7. Project Screen UX

### 7.1 Layout

The project screen MUST use an operational, dense layout:

1. Top app bar: back, project name, guest/saved status, identity action.
2. Project settings band.
3. Price status and refresh band.
4. Resource toolbar and filters.
5. Resource table.
6. Portfolio totals and parity band.

Use full-width bands and a table. Do not nest cards inside cards.

### 7.2 Project Settings

The settings area MUST contain:

| Setting | Default | Validation | Notes |
|---|---:|---:|---|
| Project name | User supplied | 1-100 chars | Persisted for authenticated users |
| Project type | User selected | `ec2`, `rds`, or `on_prem` | Immutable after the first resource is added |
| AWS region | `eu-west-1` | Supported live catalog region | Required for EC2/RDS; absent for On-prem; one region per project |
| Azure migration region | `Sweden Central` | One of the 28 reviewed SQL MI public pricing regions | One selected region per project; internal ARM region name |

The Azure migration-region catalog MUST contain the 28 public USD regions that, as reviewed on 2026-08-11, are listed by Microsoft for Premium-series hardware with 16-TB storage and are represented by both the global Azure Retail Prices API and Azure SQL calculator: Australia East, Australia Southeast, Brazil South, Canada Central, Canada East, Central India, Central US, East Asia, East US, East US 2, France Central, Germany West Central, Italy North, Japan East, Japan West, North Central US, North Europe, Poland Central, Qatar Central, South Central US, Southeast Asia, Sweden Central, Switzerland North, UK South, West Central US, West Europe, West US, and West US 2.

China North 3 MUST remain excluded while the application uses the approved global USD pricing endpoints because those endpoints do not expose a compatible regional price set for that sovereign-cloud region. East Asia catalog availability does not promise deployment capacity; Microsoft documents that creation or modification can be temporarily disabled there because of limited Premium-series hardware capacity. Region availability MUST be re-reviewed against the official SQL Managed Instance region-availability page before adding, removing, or materially changing a region capability.
| Currency | `USD` | Read-only | Tax excluded |
| AWS/source compute discount | `10%` | 0-100% | AWS projects only; fixed at 0 and not editable for On-prem |
| AWS/source SQL license discount | `5%` | 0-100% | Applies to AWS licensing or On-prem License + SA pack prices |
| AWS/source storage discount | `5%` | 0-100% | AWS projects only; fixed at 0 and not editable for On-prem |
| Azure compute/MACC discount | `0%` | 0-100% | Applies once to compute plus additional RAM |
| Azure SQL license discount | `0%` | 0-100% | Independent |
| Azure storage discount | `0%` | 0-100% | Independent |
| Selected parity adjustment | `0%` | 0-100% | Applied after Azure component discounts |
| Default annual hours | `8760` | 0-8784 | Copied to new resources |
| Default MI purchase option | `PAYG + Azure Hybrid Benefit` | Enum | Copied to new resources |
| Enterprise License + SA price / 2-core pack | User supplied | USD decimal >0 | On-prem only; quoted for selected remaining coverage |
| Standard License + SA price / 2-core pack | User supplied | USD decimal >0 | On-prem only; quoted for selected remaining coverage |
| Remaining EA/SA coverage | `36 months` | `12`, `24`, or `36` | On-prem only; annualizes the entered pack prices |
| Electricity rate | User supplied | USD/kWh decimal >=0 | On-prem only; user performs any currency conversion |

Changing a pricing-relevant setting MUST mark results dirty and require recalculation. For authenticated projects, settings changes use optimistic concurrency.

Authenticated edits and recalculations remain unsaved until the user selects `Save project`. The frontend MUST show dirty state, and the backend MUST persist the complete validated project with ETag concurrency only on explicit save. Do not auto-save individual mutations.

### 7.3 Price Loading State

The UI MUST show two independent status controls:

- Spinner icon plus `Fetching AWS prices...`.
- Spinner icon plus `Fetching Azure prices...`.

States for each provider:

- `idle`
- `fetching`
- `fresh`
- `cached`
- `stale`
- `error`

The controls MUST not shift layout when labels change. When both calls finish, calculations run and the table updates atomically. If one provider fails and a cached snapshot exists, calculation MAY proceed with a prominent stale-price warning. If no usable snapshot exists, affected rows MUST show `PRICE UNAVAILABLE`, not zero cost.

When a snapshot is older than 24 hours but no older than 7 days, calculation proceeds automatically with a persistent stale warning and source provenance in every affected row. No separate stale-price consent is required.

### 7.4 Resource View

Each project displays one resource table specialized for its immutable project type: EC2, RDS, or On-prem. Do not show cross-source tabs. The table MUST support horizontal scrolling on narrow screens and a compact mobile row summary that expands to details. Do not hide financial values without an explicit expansion control.

Show core inputs, selected target, source total, Azure total, savings, and parity by default. Keep component cost groups collapsed behind explicit expansion controls. The explanation drawer shows the selected candidate and decision threshold first, followed by a collapsible ordered candidate/rejection list.

### 7.5 Visual Direction

- Quiet, work-focused, high-density operational UI.
- Use the workbook's semantic grouping rather than its literal spreadsheet styling:
  - AWS source: restrained orange accent.
  - Azure target/cost: blue/teal accent.
  - Savings: green.
  - Parity: plum.
  - Editable inputs: pale yellow only where useful.
- Use an expressive but professional non-default font loaded as a local web asset.
- Use `lucide-svelte` icons.
- Buttons with familiar actions should use icons and tooltips.
- Maximum card radius: 8px.
- No decorative orbs, bokeh, oversized hero, or one-color palette.
- No text or control overlap at 360px, 768px, 1280px, and 1920px widths.

### 7.6 CSV Result Export

After a successful calculation, the user MAY download an Excel-compatible UTF-8 CSV containing the current project settings, source inventory inputs, derived MI target, exact server-returned component costs, savings, parity values, formula version, and pricing snapshot IDs. The export MUST preserve server decimal text and one logical line per resource; it MUST NOT recalculate financial values in the browser.

The browser MUST create the CSV locally from the project already visible to the current user and the latest calculation response. Do not add an export API, server-side export storage, upload, analytics event, or third-party egress. The export MUST exclude owner identifiers, identity claims, display names, email/contact consent, ETags, capability secrets, and other authorization metadata. Prefix text that spreadsheet software could interpret as a formula, use a sanitized filename, and make clear that the downloaded file contains confidential business data governed by the user's managed-device and storage controls.

### 7.7 Accessibility

The frontend MUST meet WCAG 2.2 AA for MVP:

- Complete keyboard operation.
- Visible focus indicators.
- Correct labels and descriptions.
- `aria-live=polite` for provider fetch completion and errors.
- `role=status` on loading text.
- Icons have accessible names.
- Dialog focus is trapped and restored.
- Color is not the only status indicator.
- Currency and percentages use locale-aware formatting while retaining USD semantics.

Support the current and previous major versions of Edge, Chrome, Firefox, and Safari.

The v1 UI is English only. Keep user-facing strings centralized so later localization does not require rewriting domain or component logic.

## 8. Resource Input Contracts

All IDs MUST be UUIDs. All money and rates MUST use decimal strings at API boundaries and `rust_decimal::Decimal` in Rust. Binary floating-point MUST NOT be used for financial calculations.

### 8.1 Shared Resource Fields

| Field | Type | Validation |
|---|---|---|
| `id` | UUID | Server generated |
| `source_type` | `ec2`, `rds`, or `on_prem` | Must equal the immutable project type |
| `workload_name` | string | 1-160 chars |
| `quantity` | integer | 1-10,000 |
| `sql_edition` | `standard` or `enterprise` | Required |
| `license_basis` | `license_included` or `byol` | Required |
| `sql_data_gb_per_instance` | decimal | 0-1,000,000,000 |
| `source_ram_gb_per_instance` | decimal | >0 and <=1,000,000 |
| `annual_hours_per_instance` | decimal | 0-8,784 |
| `mi_purchase_option` | enum in section 10.6 | Required |

AWS SQL edition and license basis MUST affect only AWS source cost. They MUST NOT select or reprice the Azure target.

Annual hours apply independently to each source instance or logical deployment and are copied unchanged to each priced SQL MI target. The default is 8,760 and the valid range is 0-8,784.

### 8.2 EC2 Fields

| Field | Type | Validation / behavior |
|---|---|---|
| `instance_type` | string | Must exist for project AWS region |
| `tenancy` | fixed `shared` | MVP fixed |
| `operating_system` | fixed `windows` | MVP fixed |
| `source_term` | fixed `on_demand` | MVP fixed |
| `volumes` | array | At least one persistent or ephemeral volume |

Selecting an instance type pre-fills source RAM from AWS metadata, but the user may override it to reflect observed workload requirements. The overridden value is authoritative for Azure target sizing. The row explanation MUST show both catalog RAM and the effective overridden value and identify when they differ.

EC2 quantity is preserved exactly. A quantity of two representing an HA pair prices two source instances and two SQL MI targets; v1 does not consolidate the pair into one target automatically.

#### EC2 Volume Fields

| Field | Type | Validation / behavior |
|---|---|---|
| `id` | UUID | Server generated |
| `label` | string | 1-80 chars, such as drive letter |
| `aws_volume_id` | optional string | Maximum 128 chars |
| `volume_type` | `gp3`, `io2`, or `ephemeral` | Required |
| `capacity_gb` | decimal | >=0 |
| `provisioned_iops` | optional integer | >=0; required for `gp3` and `io2` |
| `throughput_mibps` | optional decimal | >=0; used for `gp3` |

The form pre-fills new `gp3` volumes with 3,000 IOPS and 125 MiB/s. Submitted `gp3` and `io2` volumes without explicit IOPS are invalid; the backend MUST NOT silently substitute a default. The submitted provisioned IOPS, including the 3,000 gp3 baseline, participates in source max IOPS.

EC2 source max IOPS is the maximum provisioned IOPS of any non-ephemeral volume. It is not the sum. Ephemeral volumes appear in the explanation but contribute zero persistent storage cost and zero Azure SQL data-storage quantity.

### 8.3 RDS Fields

| Field | Type | Validation / behavior |
|---|---|---|
| `instance_type` | string | Must exist for region |
| `deployment` | `single_az` or `multi_az` | Required |
| `commercial_term` | normalized AWS term | Dependent on instance and deployment |
| `storage_class` | string | Dependent on region and deployment |
| `source_max_iops` | integer | 0-1,000,000,000 |

RDS source RAM is pre-filled from instance metadata and remains editable. `source_max_iops=0` means unspecified and defaults the requested Azure tier to NGGP.

The RDS commercial-term selector MUST expose every On-Demand and Reserved term/payment-option combination normalized by the workbook-compatible live catalog for the selected region, instance, and deployment. Do not maintain a smaller hard-coded allow-list.

RDS Multi-AZ quantity is the number of logical database deployments. AWS Multi-AZ compute and storage prices already represent source HA. Quantity MUST NOT be doubled automatically for standby infrastructure.

RDS source max IOPS influences target-tier selection and appears in the mapping explanation. Additional AWS charges for provisioned RDS IOPS and throughput are excluded in MVP.

### 8.4 On-Prem Fields

| Field | Type | Validation / behavior |
|---|---|---|
| `source_vcpu` | integer | 1-100,000; used for SQL MI sizing |
| `licensable_cores` | integer | 1-100,000; distinct from vCPU and used for source SQL licensing |
| `source_max_iops` | integer | 0-1,000,000,000 |
| `hardware_capex_usd` | decimal | >=0; one-time physical infrastructure acquisition price excluding SQL licensing |
| `depreciation_years` | decimal | >0 and <=50 |
| `average_power_kw_override` | optional decimal | >0; overrides the disclosed indicative server-power estimate |

On-prem resources reuse the shared SQL data, RAM, edition, quantity, and annual-hours fields. `hardware_capex_usd` is the final net price for the complete physical server, including compute and storage hardware but excluding SQL licensing. Do not apply source compute or storage discounts to it. The project-level Enterprise and Standard two-core-pack prices exclude hardware and MUST represent License plus active Software Assurance for the selected remaining EA/SA coverage period. The source licensing explanation MUST round licensable cores up to complete two-core packs with a minimum of four licensable cores and MUST disclose that actual agreement terms control.

## 9. Live Pricing

### 9.1 General Rules

- Browser code MUST NOT call provider APIs directly.
- Rust price-provider adapters MUST make all upstream calls.
- Every price component MUST retain provenance.
- Provider calls MUST use HTTPS, explicit timeouts, response-size limits, schema validation, and bounded retries.
- Provider errors MUST never be converted to zero price.
- The API MUST distinguish `not_found`, `unsupported`, `temporarily_unavailable`, and `schema_changed`.
- Production MUST send a descriptive `User-Agent` containing product name and version.

Provider error behavior is normative:

- `not_found`: the requested meter or SKU has no matching rate. Do not retry. Return the affected component as unresolved.
- `unsupported`: the requested service, region, or term is outside the adapter's modeled contract. Do not retry. Return the affected component as unresolved with a stable reason code.
- `temporarily_unavailable`: network errors, timeouts, HTTP 408, HTTP 429, or HTTP 5xx. Retry at most three attempts with exponential backoff and jitter, honoring `Retry-After`, within the provider's overall time budget.
- `schema_changed`: a required field, type, or relationship fails schema validation. Do not retry the same response. Log the parser version and source version, then use a valid cached snapshot when available.

Normal upstream exhaustion is represented by a successful resolution response with `status = unavailable`, `snapshot_id = null`, and sanitized reason codes. It is not converted to an application HTTP 500. A valid stale snapshot takes precedence over `unavailable` and is returned with a warning.

### 9.2 AWS Sources

Use public unauthenticated sources:

1. EC2 compute and SQL license dimensions:
   `https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/{currency}/current/ec2-calc`
2. RDS compute, storage, and OCPU license fees:
   `https://pricing.us-east-1.amazonaws.com/offers/v1.0/aws/{offerCode}/current/region_index.json`
   with offer codes `AmazonRDS` and `AmazonRDSOCPULicenseFees`.
3. EBS capacity, IOPS, and throughput dimensions from the public EC2 calculator or AWS Price List Bulk catalog.

The adapter MUST:

- Resolve only the selected project region and required SKU branches.
- Filter EC2 to current-generation x86_64, Shared tenancy, Windows, On-Demand.
- Retain Standard and Enterprise license-inclusive prices where available.
- Derive regional per-core license fallback rates from available licensed shapes using a four-core licensing minimum.
- For RDS OCPU licensing, use the mapped regional edition rate. If absent, preserve the workbook fallback of `$0.12` per source vCPU-hour for Standard and `$0.375` per source vCPU-hour for Enterprise, emit a fallback warning, and do not apply EC2's four-core minimum.
- Normalize Reserved RDS recurring plus amortized upfront prices.
- Parse and stream large AWS payloads rather than holding multiple full catalogs in memory.

### 9.3 Azure Sources

Use public unauthenticated sources:

1. Azure Retail Prices API:
   `https://prices.azure.com/api/retail/prices`
2. Azure SQL calculator composition endpoint currently used by the workbook generator:
   `https://azure.microsoft.com/api/v3/pricing/azure-sql/calculator/?culture=en-us&discount=mca`

The adapter MUST:

- Filter Retail Prices to `serviceName eq 'SQL Managed Instance'` and the project's selected Azure ARM region name.
- Follow `NextPageLink` until exhausted.
- Normalize compute, SQL license, data storage, additional memory, reservation, and savings-plan components.
- Compose all eight purchase options deterministically.
- Treat a calculator endpoint schema change as a provider error and use the last verified cache when available.
- Never scrape HTML.

The Azure calculator endpoint is public but not treated as a stable contract. Keep it behind an `AzurePriceProvider` interface so it can be replaced without changing the calculation engine.

### 9.4 Cache and Freshness

Use a two-level cache:

1. In-process bounded cache for hot reads, maximum age 15 minutes.
2. Cosmos DB `pricing-cache` container for normalized provider snapshots.

Default freshness policy:

- Fresh: <=24 hours old.
- Stale but usable: >24 hours and <=7 days old.
- Expired: >7 days old; do not use for a new calculation unless deployment configuration changes the policy.

A user-requested refresh MUST attempt live retrieval even if a fresh cache exists. Concurrent identical refreshes MUST be coalesced by cache key.

The `Pull AWS Pricing Data` GitHub Actions workflow refreshes all eight supported AWS regions daily at 01:00 UTC and also supports manual dispatch. It MUST call the application refresh API over HTTPS, send only currency and reviewed region identifiers, and MUST NOT access Cosmos DB directly or contain credentials. The workflow is current-data ingestion, not historical collection; it fails when any region does not return a fresh snapshot.

Cache key includes provider, currency, source region, target region, service, normalized filter, and parser schema version.

Within a replica, implement single-flight refreshes with one shared Tokio task per cache key. Across replicas, use a conditional Cosmos refresh-lease document in `pricing-cache`, keyed by the cache-key hash and expiring after 150 seconds. The lease owner publishes the resulting snapshot or terminal failure; waiters use bounded backoff within the 120-second provider budget and may take over only after lease expiry. Lease failure MUST NOT prevent use of an otherwise valid stale snapshot.

### 9.5 Price Snapshot Provenance

Each snapshot MUST include:

- `snapshot_id`
- provider
- retrieval status
- retrieved UTC time
- source publication/effective time when supplied
- currency
- source URL list
- ETag or source version when supplied
- parser schema version
- normalized rate records
- content SHA-256
- warnings

`snapshot_id` MUST be `{provider}-{sha256}`, where `sha256` is the full lowercase SHA-256 of a canonical normalized payload containing provider, currency, scope, parser schema version, and rate records sorted by stable rate key. Retrieval timestamps and cache status are excluded from the hash. Identical normalized price content therefore has the same ID. Golden tests MUST lock canonicalization behavior.

AWS persistence MUST split the aggregate domain snapshot into one current-state document and separate EC2, RDS, and EBS component documents. Each component ID uses a lowercase SHA-256 of its canonical core normalized data, including currency, AWS region, parser schema version, stable keys, dimensions, and rates while excluding retrieval metadata and record provenance. A separate full-record SHA-256 protects the persisted component, including provenance, from corruption. If the core hash is unchanged, retain the existing component rather than rewriting provenance-only changes. Build the state and aggregate snapshot from the components actually retained, then publish the state last.

Only current AWS provider data is retained. After a new state is published, delete superseded service components; do not retain historical AWS component or state records. Consequently, an old AWS snapshot ID is not a durable lookup key after current prices change. Saved projects reference the current snapshot ID, while the latest calculation revision embeds the exact resolved rates and provenance used by each resource. A saved revision therefore remains displayable and auditable, but recalculation requires a current usable snapshot.

## 10. Deterministic Calculation Engine

### 10.1 General Rules

- The calculation engine MUST be a pure Rust domain layer with no HTTP, Cosmos, UI, or environment-variable dependencies.
- Inputs are project settings, source resources, normalized price snapshots, and the versioned SQL MI capability catalog.
- Output is a calculation revision containing resource results, portfolio totals, warnings, and explanation steps.
- Money rounds to cents only for display. Intermediate calculations retain at least 10 decimal places.
- Formula version MUST be stored with every revision.
- A source-price lookup MUST be independent of Azure target lookup. A missing Azure mapping MUST NOT erase AWS cost.

The public domain entry point MUST have the equivalent of:

```rust
pub struct CalculationEngine {
  capabilities: Arc<CapabilityCatalog>,
  formula_version: FormulaVersion,
}

impl CalculationEngine {
  pub fn new(
    capabilities: Arc<CapabilityCatalog>,
    formula_version: FormulaVersion,
  ) -> Result<Self, CalculationError>;

  pub fn calculate(
    &self,
    input: CalculationInput<'_>,
  ) -> Result<CalculationRevision, CalculationError>;
}
```

`CalculationInput` contains borrowed project settings, resources, and optional immutable AWS/Azure snapshots. The engine has no mutable request state, provider adapter, database handle, or hidden global. This interface is the test boundary; exact Rust type names MAY vary only if the same dependency direction is preserved.

### 10.2 SQL MI Capability Catalog

Price APIs do not define all sizing limits. Package a versioned, reviewed capability catalog generated from Microsoft SQL Managed Instance resource-limit documentation.

Each target shape includes:

- Azure region availability.
- Service tier.
- Hardware family.
- vCore count.
- Zone redundancy.
- Included memory.
- Supported flexible-memory options.
- Storage architecture: `Remote LRS` for NGGP or `BC local SSD` for BC.
- Maximum supported storage where known.
- Source URL and reviewed date.

The capability catalog MUST cover every Azure SQL MI region exposed by the project settings selector. Sweden Central remains the default and the frozen workbook-parity region.

### 10.3 Candidate Eligibility

A candidate is sizing-eligible only if:

- Azure region equals the project's selected Azure target region.
- Candidate vCores >= source vCPU.
- At least one supported memory value >= source RAM.
- The candidate's known maximum supported storage is >= source SQL data GB.

Only non-zone-redundant candidates are eligible in v1. For NGGP, only Premium Series candidates are eligible in workbook-parity mode.

For each candidate, selected memory is that candidate's smallest supported memory value >= source RAM.

Storage capacity MUST be enforced during selection. Candidates that fail storage are rejected, allowing the next larger candidate in the requested service tier to be selected. When storage causes selection of a larger SKU than CPU and RAM alone require, the row explanation MUST name the rejected SKU, its storage limit, and the selected larger SKU. Storage alone MUST NOT switch NGGP to Business Critical; if no candidate in the IOPS-requested tier satisfies capacity, return `NO MAPPING`.

Price completeness is evaluated after structural target selection. A usable target price set requires all eight purchase options plus the applicable storage and additional-memory prices, matching the workbook catalog gate. Missing prices produce `PRICE UNAVAILABLE`; they MUST NOT be misreported as a capacity-based `NO MAPPING`.

### 10.4 Candidate Ordering

Among eligible candidates in the requested tier, select the minimum tuple:

1. `(candidate_vcores - source_vcpu) / source_vcpu`
2. `(selected_memory - source_memory) / source_memory`
3. Tier priority: NGGP before BC
4. Candidate vCore count
5. Stable configuration key

The engine MUST include the ordered candidates and rejection reasons in the explanation trace.

### 10.5 Service Tier Selection

Constants:

- NGGP IOPS per vCore: `1,600`.
- NGGP maximum IOPS: `80,000`.

Algorithm:

1. Find the best eligible NGGP candidate for source vCPU and RAM.
2. If an NGGP candidate exists, calculate:
   `nggp_iops_limit = min(80,000, 1,600 * nggp_candidate_vcores)`.
3. If no NGGP candidate exists, calculate the decision threshold using source vCPU:
   `nggp_iops_limit = min(80,000, 1,600 * source_vcpu)`.
4. If `source_max_iops <= nggp_iops_limit`, request NGGP.
5. If `source_max_iops > nggp_iops_limit`, request Business Critical.
6. Select the best eligible candidate in the requested tier.
7. If no candidate exists in the requested tier, return `NO MAPPING`.

Business Critical MUST NOT be selected because the AWS source edition is Enterprise. It MUST NOT be selected solely because NGGP lacks enough RAM. It is selected only when source IOPS exceeds the NGGP limit.

Legacy General Purpose MUST never be offered or selected as a fallback. The v1 target selector maps only Next Generation General Purpose and Business Critical.

For EC2, `source_max_iops` is derived from persistent volumes. For RDS, it is a literal input. Throughput does not select the tier.

### 10.6 Azure Purchase Options

Supported labels and stable keys:

| Label | Key |
|---|---|
| PAYG | `payg` |
| PAYG + Azure Hybrid Benefit | `ahb` |
| 1-Year Reserved | `one-year` |
| 1-Year Reserved + AHB | `ahbone-year` |
| 3-Year Reserved | `three-year` |
| 3-Year Reserved + AHB | `ahbthree-year` |
| 1-Year Savings Plan | `sv-one-year` |
| 1-Year Savings Plan + AHB | `ahbsv-one-year` |

Default: `PAYG + Azure Hybrid Benefit`.

The UI MUST display an eligibility warning for AHB options. It does not decide legal entitlement and MUST NOT require or persist an attestation.

### 10.7 AWS Cost Formulas

Let:

- `q` = quantity.
- `h` = annual hours per instance.
- `d_c`, `d_l`, `d_s` = AWS compute, license, and storage discounts.

#### EC2

- `compute_gross = q * h * ec2_compute_hourly`
- `compute_net = compute_gross * (1 - d_c)`
- If license basis is BYOL: `license_gross = 0`.
- If license included: `license_gross = q * h * sql_license_hourly`.
- `license_net = license_gross * (1 - d_l)`
- `storage_gross = q * 12 * sum(volume_monthly_cost)`
- `storage_net = storage_gross * (1 - d_s)`
- `aws_net_total = compute_net + license_net + storage_net`

For missing small-shape license-inclusive EC2 prices, derive a regional per-core license rate and apply AWS's four-core minimum. This is source-only and must not affect Azure sizing.

#### EBS Volume

For `gp3`:

- `capacity_cost = capacity_gb * capacity_rate`
- `iops_cost = max(0, provisioned_iops - included_iops) * iops_rate`
- `throughput_cost = max(0, throughput_mibps - included_throughput) * throughput_rate`

For `io2`:

- `capacity_cost = capacity_gb * capacity_rate`
- Apply live tiered IOPS prices to 0-32,000, 32,001-64,000, and above 64,000 IOPS.
- Throughput cost is zero unless the live catalog introduces a separately billable supported dimension.

For `ephemeral`, every storage cost component is zero.

#### RDS

- `compute_gross = q * h * rds_effective_compute_hourly`
- `compute_net = compute_gross * (1 - d_c)`
- If BYOL: `license_gross = 0`.
- If license included: `license_gross = q * h * source_vcpu * regional_edition_core_hourly`
- `license_net = license_gross * (1 - d_l)`
- `storage_gross = q * sql_data_gb_per_instance * 12 * rds_storage_monthly_per_gb`
- `storage_net = storage_gross * (1 - d_s)`
- `aws_net_total = compute_net + license_net + storage_net`

Reserved RDS effective hourly compute is recurring hourly price plus upfront price amortized across the reservation term.

#### On-Prem

Let:

- `p` = the edition-specific user-entered License + SA price per two-core pack for the selected remaining coverage period.
- `m` = remaining EA/SA coverage months: 12, 24, or 36.
- `e` = user-entered electricity rate in USD/kWh.
- `d_l` = source SQL license discount.

The versioned indicative server-power estimator is:

`estimated_power_kw = 0.100 + (0.0125 * source_vcpu) + (0.000375 * source_ram_gb_per_instance) + (0.010 * sql_data_gb_per_instance / 1024)`

The terms represent 100 W fixed server overhead, 12.5 W per vCPU, 0.375 W per GB of RAM, and 10 W per TB of SQL data storage. This is a TCO estimate, not a hardware measurement. If `average_power_kw_override` is present, it replaces the estimate. Every row explanation MUST show the formula inputs, coefficients, estimated value, effective value, whether an override was used, and annual kWh.

- `hardware_annual = q * hardware_capex_usd / depreciation_years`
- `effective_power_kw = average_power_kw_override ?? estimated_power_kw`
- `electricity_annual = q * h * effective_power_kw * e`
- `electricity_monthly_average = electricity_annual / 12`
- `license_pack_count = ceil(max(4, licensable_cores) / 2)`
- `license_annual = q * license_pack_count * p * (1 - d_l) * 12 / m`
- `on_prem_source_total = hardware_annual + electricity_annual + license_annual`

On-prem source compute and storage discounts do not apply because hardware CAPEX is entered as one final net server price. The calculator MUST NOT perform foreign-exchange conversion. If the user's electricity tariff or License + SA quote is not in USD, the user converts it before entry.

### 10.8 Azure Cost Formulas

Let `a_c`, `a_l`, and `a_s` be Azure compute, license, and storage discounts.

- `compute_gross = q * h * mi_compute_hourly`
- `additional_ram_gb = max(0, selected_mi_ram_gb - included_mi_ram_gb)`
- `additional_ram_gross = q * h * additional_ram_gb * memory_per_gb_hourly`
- `compute_plus_ram_net = (compute_gross + additional_ram_gross) * (1 - a_c)`
- `license_gross = q * h * mi_license_hourly`
- `license_net = license_gross * (1 - a_l)`
- `storage_gross = q * sql_data_gb_per_instance * 12 * mi_storage_monthly_per_gb`
- `storage_net = storage_gross * (1 - a_s)`
- `mi_net_before_parity = compute_plus_ram_net + license_net + storage_net`

Additional RAM MUST be charged exactly once. The compute discount applies once to the sum of base compute and additional RAM.

`mi_compute_hourly` is the already composed compute rate for the selected purchase option, including its reservation or savings-plan treatment. `mi_license_hourly` is the selected option's license rate, including zero where AHB applies. Additional memory uses the normalized per-GB-hour memory meter independently of purchase option; `a_c` is the workbook's Azure compute/MACC discount and is the only additional discount applied to both base compute and additional memory.

### 10.9 Savings and Parity

For mapped rows only:

- `compute_savings = aws_compute_net - azure_compute_plus_ram_net`
- `license_savings = aws_license_net - azure_license_net`
- `storage_savings = aws_storage_net - azure_storage_net`
- `total_savings = aws_net_total - mi_net_before_parity`
- `row_required_adjustment = if mi_net_before_parity == 0 then 0 else 1 - aws_net_total / mi_net_before_parity`
- `mi_after_selected_parity = mi_net_before_parity * (1 - selected_parity_adjustment)`
- `difference = mi_after_selected_parity - aws_net_total`

Portfolio values:

- `aws_all_rows_total` includes mapped and unmapped rows.
- `aws_mapped_rows_total` includes mapped rows only.
- `azure_mapped_rows_total` includes mapped rows only.
- `required_portfolio_adjustment = if azure_mapped_rows_total == 0 then 0 else 1 - aws_mapped_rows_total / azure_mapped_rows_total`
- `portfolio_after_selected_parity = azure_mapped_rows_total * (1 - selected_parity_adjustment)`
- `portfolio_difference = portfolio_after_selected_parity - aws_mapped_rows_total`

A negative required adjustment means Azure is already cheaper and would need an uplift to reach parity. A value above 100% means no feasible discount can reach parity.

### 10.10 No Mapping

When no target in the requested tier satisfies vCPU, RAM, and required price availability:

- Target status is `no_mapping`.
- User-visible tier, RAM, and hardware/storage fields display `NO MAPPING`.
- AWS costs remain calculated and included in `aws_all_rows_total`.
- Azure cost fields are not applicable and serialize as `null`, not numeric zero.
- Savings and parity fields are `null`.
- The row is excluded from mapped portfolio totals and parity.
- The explanation identifies the requested tier and every failed constraint.

The explanation MUST NOT recommend workload splitting, a different Azure service, cross-region architecture, or any other unmodeled migration design. Recommendations require separately approved sizing and pricing rules.

The API may use zero internally for unavailable Azure rates only inside a sentinel object, but it MUST serialize unavailable financial results as `null` to prevent false interpretation.

### 10.11 Price Unavailable

Price availability is independent of target mapping.

- `mapping_status` is `mapped` or `no_mapping` and reflects only structural vCPU/RAM/IOPS selection.
- `aws_pricing_status` and `azure_pricing_status` are each `fresh`, `cached`, `stale`, `unavailable`, or `not_required`.
- Missing AWS prices leave affected AWS costs, savings, and parity as `null`.
- Missing Azure prices may retain a structurally selected target and valid AWS costs, but Azure costs, savings, and parity are `null`.
- An unmapped target uses `azure_pricing_status = not_required` unless an Azure rate was independently requested for another mapped resource.
- Any row without complete comparable AWS and Azure costs is excluded from mapped portfolio parity and includes a stable unresolved-component reason.

The UI displays `PRICE UNAVAILABLE` in affected cost cells. It MUST NOT replace unavailable values with zero or relabel a provider failure as `NO MAPPING`.

## 11. Resource Table Output

Each row MUST expose these logical groups.

### 11.1 Inputs

- Workload.
- Source type.
- Source instance.
- RDS deployment and commercial term, when applicable.
- Quantity.
- AWS SQL edition.
- AWS license basis.
- Storage summary.
- SQL data GB per instance.
- Source RAM GB per instance.
- Source max IOPS.
- Annual hours per instance.
- MI purchase option.

### 11.2 Automatic Azure Target

- MI RAM GB.
- MI service tier.
- MI hardware and storage architecture.
- MI vCores.
- Source vCPU.

### 11.3 AWS Current State

- Compute gross and net.
- SQL license gross and net.
- Storage gross and net.
- AWS net total.

### 11.4 Azure SQL MI

- Compute gross.
- Additional RAM gross.
- Compute plus RAM net.
- SQL license gross and net.
- Storage gross and net.
- MI net before parity.

### 11.5 Savings and Parity

- Compute savings.
- License savings.
- Storage savings.
- Total savings.
- Required adjustment.
- Selected adjustment.
- MI after parity.
- Difference.

Columns MAY be grouped/collapsed for usability, but all values must remain available.

## 12. API Design

Base path: `/api/v1`.

All JSON uses `snake_case`. Dates are RFC 3339 UTC. Decimal values are JSON strings. Errors use RFC 9457 Problem Details.

### 12.1 Health and Session

- `GET /healthz`: process liveness, no upstream calls.
- `GET /readyz`: verifies configuration and Cosmos access; reports price providers as configured without downloading full catalogs.
- `GET /version`: build version and formula version.
- `GET /api/v1/session`: returns guest or authenticated principal summary.
- `PUT /api/v1/privacy-consent`: authenticated point update for the current notice acceptance and independent Azure SQL contact choice; responses use `Cache-Control: no-store`.

`GET /api/v1/session` includes the current notice version, whether acceptance is required, acceptance time, contact choice, optional stored contact email, optional Entra display name, and optional email-like Entra value for contact-form prefill. Session responses use `Cache-Control: no-store`. When a valid authenticated principal has not accepted the current notice, every other API returns `428 Privacy Consent Required`; guest requests remain available without acceptance.

### 12.2 Catalog APIs

- `GET /api/v1/catalog/aws/regions`
- `GET /api/v1/catalog/aws/ec2/instances?region={region}`
- `GET /api/v1/catalog/aws/rds/instances?region={region}`
- `GET /api/v1/catalog/aws/rds/options?region={region}&instance_type={type}&deployment={deployment}`
- `GET /api/v1/catalog/aws/ebs/types?region={region}`
- `GET /api/v1/catalog/azure/mi/purchase-options`

Catalog responses include freshness and provenance metadata.

### 12.3 Price Resolution APIs

The frontend starts these calls concurrently and shows independent loading states:

- `POST /api/v1/pricing/aws/resolve`
- `POST /api/v1/pricing/azure/resolve`

Request contains project region/settings and only the resource descriptors needed to resolve prices. Response contains a server-issued snapshot ID, status, timestamps, warnings, and normalized rate summaries. The client cannot submit arbitrary price values.

Resolution response `status` is `fresh`, `cached`, `stale`, or `unavailable`. `snapshot_id` is non-null only for the first three states. Provider exhaustion with no usable cache returns HTTP 200 and `status = unavailable` so the two independent provider requests can settle and the application can still calculate any available side.

- `POST /api/v1/pricing/aws/refresh`
- `POST /api/v1/pricing/azure/refresh`

Refresh bypasses fresh cache lookup, coalesces duplicate work, and returns the new snapshot.

### 12.4 Calculation API

`POST /api/v1/calculations`

Request:

- Project settings.
- Resource inputs.
- AWS snapshot ID, nullable only to request unavailable-source output.
- Azure snapshot ID, nullable only to request unavailable-target output.
- Expected formula version, optional.

Response:

- Formula version.
- Snapshot references.
- Per-resource results, each containing `mapping_status`, `aws_pricing_status`, and `azure_pricing_status`.
- Portfolio totals.
- Structured explanation steps.
- Warnings and exclusions.

This endpoint is available to guests and authenticated users. It does not persist by itself.

The backend loads snapshots by ID; clients cannot submit rates. An unknown, expired, provider-mismatched, or scope-mismatched snapshot returns the `snapshot-unavailable` Problem Detail and requires price resolution before recalculation.

### 12.5 Project APIs

Authentication required:

- `GET /api/v1/projects`
- `POST /api/v1/projects`
- `GET /api/v1/projects/{project_id}`
- `PUT /api/v1/projects/{project_id}`
- `DELETE /api/v1/projects/{project_id}`
- `POST /api/v1/projects/{project_id}/shares`
- `DELETE /api/v1/projects/{project_id}/shares/{share_id}`
- `POST /api/v1/project-shares/resolve`

`PUT` requires `If-Match` with the current ETag. A missing precondition returns `428 Precondition Required`; a stale or mismatched ETag returns `412 Precondition Failed` with the latest metadata. Project documents include the latest successful calculation revision.

`POST` and `PUT` accept only client-editable metadata, settings, resources, and selected snapshot IDs. They MUST reject client-supplied totals, resolved rates, explanation steps, owner IDs, or revisions. When pricing-relevant inputs change and usable snapshots are supplied, the backend invokes the same calculation engine and persists its server-owned revision. Metadata-only changes retain the previous revision; inputs may be saved without a successful revision when pricing is unavailable, with that state made explicit on reopen.

### 12.6 API Documentation

- Maintain `openapi/openapi.yaml` in the repository.
- Validate it in CI.
- Serve Swagger UI only when `APP_ENV` is not `production`.
- Generate TypeScript schema types with pinned `openapi-typescript` and use pinned `openapi-fetch` for the typed client.
- `npm run api:generate --prefix web` MUST write the committed `web/src/lib/api/generated.ts` from `openapi/openapi.yaml`.
- CI MUST run generation and fail on a non-empty generated-file diff. Do not hand-maintain duplicate request/response interfaces.

### 12.7 Problem Details

Every non-success response uses `application/problem+json` with `type`, `title`, `status`, `detail`, `instance`, and request ID. Validation problems add `errors`, an array of `{ pointer, code, message }`, where `pointer` is an RFC 6901 JSON Pointer.

Stable problem types and statuses:

| Type suffix | Status | Use |
|---|---:|---|
| `malformed-request` | 400 | Invalid JSON or syntax |
| `unauthorized` | 401 | Missing or invalid principal |
| `forbidden` | 403 | Authenticated principal lacks access |
| `not-found` | 404 | Owner-scoped project or route not found |
| `validation-error` | 422 | Valid JSON with invalid fields or limits |
| `precondition-required` | 428 | Missing `If-Match` |
| `privacy-consent-required` | 428 | Authenticated principal has not accepted the current notice version |
| `precondition-failed` | 412 | Stale ETag |
| `payload-too-large` | 413 | Request or persisted document exceeds limit |
| `rate-limited` | 429 | Guest/principal quota exceeded; include `Retry-After` |
| `snapshot-unavailable` | 409 | Snapshot is unknown, expired, or wrong scope |
| `provider-unavailable` | 503 | The application cannot complete provider resolution and cannot return a normal unavailable result |
| `internal-error` | 500 | Unexpected server failure |

Use absolute URNs such as `urn:azure-sql-tco:problem:validation-error` until a stable product domain exists. `NO MAPPING` and normal provider `unavailable` resolution are successful domain outcomes, not HTTP problems.

## 13. Persistence Model

Use Azure Cosmos DB for NoSQL in serverless capacity mode, with consumption-based Request Unit billing and no provisioned throughput. Production access MUST use the Container App's system-assigned managed identity; local/key authentication MUST be disabled in production.

### 13.1 `projects` Container

- Partition key: `/owner_id`.
- One document per project.
- Maximum 100 resources per MVP project.
- Optimistic concurrency through Cosmos ETag.

Project document fields:

- `id`
- `document_type = "project"`
- `owner_id`
- `name`
- `description`
- `created_at`
- `updated_at`
- `settings`
- `resources`
- `latest_calculation_revision`
- `aws_price_snapshot_id`
- `azure_price_snapshot_id`
- `formula_version`
- schema version

Do not persist guest projects.

Share documents use the same container and `/owner_id` partition definition without changing deployment topology. Their partition value is the fixed `project-shares` value so credential resolution is a point read and all links for a deleted source can be found with a single-partition query. A share document contains `document_type = "project_share"`, source owner and project IDs, a SHA-256 secret digest, the editable project snapshot, and creation/expiry timestamps. It MUST NOT contain an ETag from the source response or a calculation revision. Share API responses use `Cache-Control: no-store`.

Each authenticated owner partition MAY also contain one fixed-ID `privacy_consent` document. Its fields are `id = "privacy-consent"`, `document_type`, `owner_id`, `notice_version`, `accepted_at`, optional `display_name`, optional `email_address`, and `allow_contact`. `email_address` MUST be present if and only if `allow_contact` is true. Project queries MUST continue filtering by `document_type = "project"`, and consent access MUST use an owner-partition point read/upsert rather than a cross-partition query.

### 13.2 `pricing-cache` Container

- Partition key: `/cache_partition`.
- Stores normalized provider data, current-state pointers, refresh locks, and distributed refresh-rate counters.
- TTL enabled; default 30 days.
- AWS EC2, RDS, and EBS use separate persistent component documents with `ttl = -1`; each ID is content-addressed from service core data as defined in section 9.5.
- One deterministic persistent AWS state document per currency and source region references the current three component hashes and reconstructable aggregate snapshot ID.
- Publish all validated components before conditionally replacing state. After confirming the published state is still current, delete only superseded components; an older concurrent writer MUST NOT delete components referenced by newer state.
- Reject a serialized component at or above the Cosmos 2 MiB item limit before sending it. Do not silently truncate or split a service record set further without a reviewed specification change.
- Azure snapshot IDs remain content-addressed exactly as defined in section 9.5.
- Refresh leases and distributed refresh-rate counters use short TTLs and distinct `document_type` values.

### 13.3 Data Limits

- Before `POST` or `PUT` persists a project, serialize the complete server-owned document and reject it at 1,800,000 bytes or larger with the `payload-too-large` Problem Detail. This leaves margin below Cosmos's 2 MB item limit.
- Maximum 100 resources per project.
- Maximum 50 EBS volumes per EC2 resource.
- Maximum request body 1 MB.
- Maximum upstream provider response accepted per request must be configured and streaming parsers used for larger catalogs.

## 14. Backend Architecture

### 14.1 Technology

Use stable Rust with:

- `axum` for HTTP routing.
- `tokio` for bounded concurrent provider I/O.
- `reqwest` with Rustls for HTTPS.
- `serde` and `serde_json` for contracts.
- `rust_decimal` for money and rates.
- `thiserror` for typed errors.
- `tower-http` for request IDs, compression, tracing, and body limits.
- `tracing` and `tracing-subscriber` for structured logs.
- `uuid` and `time` or `chrono` for identifiers and timestamps.
- Official Azure SDK for Rust `azure_identity` and `azure_data_cosmos` crates for managed-identity authentication and Cosmos operations.
- `sha2` for snapshot and fixture hashes and `base64` for trusted principal-header decoding.

Use the latest stable compatible versions at implementation time, pin explicit versions, and commit `Cargo.lock`. Each dependency must be justified in the README or pull request.

Do not hand-roll HTTP parsing. Gaia's blocking server is intentionally not copied because this application performs concurrent, potentially slow provider API calls.

### 14.2 Module Boundaries

Suggested `rust/src` modules:

- `main.rs`: readable top-to-bottom composition and startup.
- `server.rs`: router and static-file fallback.
- `auth.rs`: trusted Entra principal extraction.
- `config.rs`: validated environment configuration.
- `api/`: thin request handlers.
- `domain/project.rs`
- `domain/resource.rs`
- `domain/money.rs`
- `calculation/engine.rs`
- `calculation/target_selector.rs`
- `calculation/costs.rs`
- `calculation/explanation.rs`
- `pricing/provider.rs`
- `pricing/aws.rs`
- `pricing/azure.rs`
- `pricing/cache.rs`
- `persistence/repository.rs`: owner-scoped `ProjectRepository` and `PriceSnapshotRepository` traits.
- `persistence/cosmos.rs`: Azure SDK implementations and ETag/TTL behavior.
- `persistence/memory.rs`: deterministic test and local-development implementation.
- `health.rs`
- `problem.rs`

Handlers MUST not contain financial formulas. Provider adapters MUST not select Azure SKUs. The domain layer MUST not know about HTTP or Cosmos.

Production constructs the Cosmos repositories with a managed-identity credential explicitly configured to use the Container App's system-assigned managed identity. It MUST NOT select a user-assigned identity or fall back to environment service-principal credentials. Local unit/integration tests use the in-memory repository by default; optional Cosmos-emulator key authentication is allowed only with `APP_ENV=local`. Repository methods MUST require `owner_id` in every project read, update, and delete signature so an unpartitioned cross-owner access path is not available accidentally.

### 14.3 Error Handling

- Public functions return `Result<T, E>`.
- No `unwrap` or `expect` outside tests or documented impossible states.
- No `unsafe` unless isolated with a `SAFETY` explanation.
- Errors log internal context but return sanitized Problem Details.
- Provider failure logs MUST exclude tokens, identity headers, and full project payloads.

## 15. Frontend Architecture

### 15.1 Technology

- Svelte 5.
- SvelteKit with `@sveltejs/adapter-static` and `fallback: 'index.html'`.
- TypeScript strict mode; no `any`.
- Vite.
- `lucide-svelte`.
- Vitest for component/unit tests.
- Playwright for end-to-end tests.
- ESLint and Prettier.

The static bundle is served by Rust from the same origin. MVP MUST NOT register a service worker or advertise offline calculation because live prices and server-authoritative calculations are required. A future PWA MAY cache only versioned static assets; API, identity, project, and pricing routes remain network-only.

### 15.2 State

- Route state controls main/project navigation.
- Guest project state is hydrated from and written to browser-local durable storage, preferably IndexedDB; it is never sent to project-persistence endpoints.
- Authenticated project state is loaded from the API.
- A dirty flag tracks unsaved settings/resources.
- AWS and Azure pricing states are independent finite states.
- Calculation results update only when both selected snapshots and the calculation response are valid.

Do not implement financial calculations in TypeScript. Formatting and view-only aggregation are allowed, but server results are authoritative.

### 15.3 Suggested Components

- `AppShell.svelte`
- `IdentityMenu.svelte`
- `ProjectList.svelte`
- `ProjectSettings.svelte`
- `PriceStatus.svelte`
- `ResourceToolbar.svelte`
- `ResourceTable.svelte`
- `ResourceForm.svelte`
- `EbsVolumeEditor.svelte`
- `MappingExplanationDrawer.svelte`
- `PortfolioTotals.svelte`
- `ConfirmDialog.svelte`
- `ProblemBanner.svelte`

## 16. Security and Privacy

- Use HTTPS only in Azure.
- Use Container Apps built-in Entra authentication.
- Trust identity headers only behind Container Apps authentication.
- All source-project operations enforce owner scope in the backend and Cosmos query. Share creation and revocation require that same owner scope; resolution requires both an authenticated principal and the independently generated capability secret.
- Every runtime Azure-to-Azure permission MUST use Azure RBAC granted to the consuming resource's system-assigned managed identity (SAMI). For this MVP, the Container App SAMI MUST be used for Cosmos data access, ACR image pull, and Key Vault secret reads when Key Vault is deployed.
- Do not create or attach a user-assigned managed identity. Do not use a service principal, client secret, access key, account key, or connection string for runtime Azure-to-Azure authorization.
- Disable Cosmos local/key authentication in production.
- Disable the ACR admin account. Grant only `AcrPull` to the Container App SAMI.
- Every deployed Azure environment, including development, MUST use a VNet-integrated Container Apps environment, a Cosmos private endpoint, and private DNS. Cosmos public network access MUST be disabled. Local workstation development MAY use the Cosmos emulator.
- Treat project settings, workload names, server identifiers, and cost assumptions as confidential business data. Do not log workload names or server identifiers. Use Azure encryption at rest with Microsoft-managed keys; customer-managed keys and application field encryption are not required for v1.
- Client-side CSV result export is an explicit user-initiated disclosure to the user's device. Generate it only from the authorized project already loaded in that browser, exclude personal and authorization metadata, harden cells against spreadsheet formula injection, and do not send or retain the file server-side.
- Treat display names, email addresses, notice acceptance, and contact choices as personal data. Do not place them in application logs, telemetry, provider requests, share documents, or project API responses.
- Persist contact email only with an affirmative, separate Azure SQL contact choice. Do not add a contact export endpoint, CRM integration, analytics destination, or other egress without written Privacy, Security, architecture, and service-owner approval.
- Retain the consent profile while the signed-in pilot profile is in use. Delete or correct it through the approved Microsoft privacy-request process, or delete it when pilot data is decommissioned. A fixed production retention period and accountable policy owner require Privacy/Legal approval before external use.
- Store any required Entra provider secret as a versioned Key Vault secret reference; never in GitHub or source.
- Use GitHub OIDC for deployment; no Azure client secret in GitHub.
- Apply body-size, timeouts, concurrency, and per-IP rate limits to guest endpoints.
- Apply stricter per-principal limits to authenticated mutation endpoints as needed.
- Enforce general guest quotas with a bounded in-process token bucket keyed by the trusted client IP from Container Apps ingress. Enforce the costlier live-refresh hourly quota with a TTL counter in `pricing-cache` keyed by a one-way hash of client IP or principal ID so it remains effective across replicas. Quota failures return `429` with `Retry-After`.
- Default guest quotas are 60 API requests per minute per IP, 8 live provider refreshes per hour per IP, and 10 concurrent calculation requests per application replica. Eight refreshes permit exactly one scheduled sweep of the supported AWS regions per requester identity. Expose all three as validated deployment configuration; do not hard-code them in handlers.
- Set CSP, HSTS, `X-Content-Type-Options`, `Referrer-Policy`, and frame restrictions.
- Do not log workload names at info level because they may reveal customer systems.
- Do not send project data to AWS or Azure price APIs; send only SKU/region filters required for prices.
- Document all third-party data egress.

## 17. Observability

Rust writes structured JSON logs to stdout for Azure Log Analytics.

Collect operational telemetry only. Do not add third-party analytics, behavioral tracking, or user-level product analytics in v1.

Every request includes or receives a request ID. Logs include:

- Request ID.
- Route template.
- Status code.
- Duration.
- Auth mode: guest/authenticated, never raw identity.
- Provider.
- Cache outcome.
- Upstream duration and retry count.
- Formula version.
- Mapping status counts.

Metrics SHOULD include:

- Pricing fetch latency and failures by provider.
- Cache hit/stale/fallback counts.
- Calculation duration.
- Mapped/no-mapping counts.
- Project CRUD latency and conflicts.

`/healthz` must be cheap. `/readyz` checks configuration and a lightweight Cosmos operation but MUST NOT download provider catalogs.

## 18. Azure Infrastructure

Use modular Bicep. The design adopts Gaia's idempotent infrastructure principle but uses Bicep for readability.

### 18.1 Required Resources

Provision one development environment only for v1. Production and test deployments are out of scope until separately approved.

- Azure Container Apps managed environment.
- One externally accessible Azure Container App.
- Azure Container Registry Basic.
- Azure Cosmos DB for NoSQL account in serverless capacity mode.
- `projects` and `pricing-cache` containers.
- Log Analytics workspace with 30-day retention.
- System-assigned managed identity on the Container App.
- SAMI role assignments for Cosmos data access, ACR pull, and optional Key Vault secret reads.
- Virtual network, delegated Container Apps subnet, Cosmos private endpoint, and private DNS.
- Azure AI Foundry resource with one approved custom Model Router deployment, private endpoint, and private DNS.
- Least-privilege model-inference role assignment for the Container App system-assigned identity.
- Optional Key Vault reference for Entra provider configuration secret.

All runtime role assignments MUST use the Container App's system-assigned identity principal ID. Bicep MUST NOT deploy a user-assigned managed identity. Native Azure platform integrations that do not expose a workload RBAC principal MUST keep their credentials out of the Container App configuration and image.

Do not deploy embeddings, Speech, search, cards, assistant background jobs, vector indexes, or another application runtime for the assistant. Disable Foundry public data-plane and local/key authentication. The application runtime MUST use only its system-assigned managed identity for model inference and MUST NOT receive model-management control-plane permissions.

### 18.2 Container App Defaults

- Development deployment region: South Africa North (`southafricanorth`). This application-hosting region is independent of each project's selected Azure SQL MI target region.
- External HTTPS ingress.
- Use the generated Container App FQDN; do not provision a custom domain or certificate in v1.
- Target port `8080`.
- Minimum replicas `0` for development.
- Maximum replicas `3` by default.
- 0.5 vCPU and 1 GiB memory initially.
- Scale on HTTP concurrency.
- Immutable image tag is the Git commit SHA.
- Read-only root filesystem where supported.
- Non-root UID `10001`.

No in-memory background job may be required for correctness because replicas can restart or scale to zero.

The development environment is best-effort and single-region. Rely on Cosmos-managed durability and redeployable infrastructure; do not scaffold cross-region failover, a formal SLA, RTO, or RPO in v1.

### 18.3 Cosmos Defaults

Deploy Cosmos DB in serverless capacity mode by enabling the account's `EnableServerless` capability. Request Units MUST be consumed and billed on demand; do not configure manual or autoscale throughput on the account, database, or containers. Do not enable or describe this deployment as Cosmos DB free tier. Bicep validation MUST reject parameters that combine serverless capacity with provisioned throughput settings.

### 18.4 Bicep Layout

- `infra/main.bicep`
- `infra/modules/container-app.bicep`
- `infra/modules/cosmos.bicep`
- `infra/modules/registry.bicep`
- `infra/modules/network.bicep`
- `infra/modules/monitoring.bicep`
- `infra/parameters/dev.bicepparam`
- `infra/README.md`

Do not create production or test parameter sets in v1. Deployment must be idempotent. Documentation MUST require `az deployment group what-if` before manual Azure changes.

## 19. Build and Containerization

Use one multi-stage Dockerfile:

1. Node stage runs `npm ci`, checks, and builds the static Svelte bundle.
2. Rust stage caches dependencies and builds a locked release binary.
3. Debian slim runtime contains only CA certificates, the Rust binary, and web assets.
4. Runtime uses non-root UID `10001`.
5. Rust serves API routes and static assets from the same origin.

Root `VERSION` is the single application-version source. Vite injects it into the frontend. The Rust `/version` endpoint returns the same value plus formula and schema versions.

## 20. Gaia Practices Adopted and Rejected

### 20.1 Adopted from `C:\Repos\gaia-robot`

- Simplicity and clarity over cleverness from `.github/copilot-instructions.md`.
- Stable Rust, `rustfmt`, Clippy warnings as errors, `Result`-based errors, no casual `unsafe`, and doc comments on public APIs.
- Clear top-to-bottom application composition in `main.rs` with domain logic delegated to focused modules.
- Conservative dependency policy, committed lockfiles, `cargo audit`, and `cargo deny`.
- Static SvelteKit shell with same-origin API and no CORS surface.
- Dynamic API paths excluded from service-worker caching.
- One multi-stage container serving both Rust and Svelte.
- Non-root runtime.
- Managed identity and least-privilege Azure roles.
- Idempotent infrastructure and `what-if` deployment checks.
- GitHub Actions OIDC rather than stored Azure deployment credentials.
- Change-aware image builds.
- Fast CI gates plus scheduled coverage and supply-chain assurance.
- Separate cheap liveness and dependency-aware readiness endpoints.
- Structured logs to Azure's native logging path.
- Gaia's application-owned bounded model/tool loop, closed typed tool schemas, host-owned identity scope, all-or-nothing batch preflight, structured tool results, Model Router integration, managed-identity inference, and private Foundry networking, adapted to the stricter TCO boundaries in section 4.2.

### 20.2 Deliberately Not Copied

- Gaia's hand-written blocking HTTP server: use Axum/Tokio because live pricing needs bounded concurrent I/O.
- Google/GitHub OAuth: use Entra plus guest mode.
- Gaia's embeddings, wisdom, cards, voice, web search, MCP, WhatsApp, alternate identity providers, public posting, and conversational persistence.
- Seven-container/vector Cosmos topology: use two simple containers.
- Field-level application encryption in MVP: rely on Azure encryption at rest, private networking, Entra, and managed identity unless data-classification requirements change.
- Automatic per-commit version bumping: use explicit semantic version changes or release automation to avoid unrelated version churn.

## 21. Repository Layout

The implementation repository MUST use:

```text
/
  .github/
    workflows/
      ci.yml
      assurance.yml
      deploy.yml
    copilot-instructions.md
  app/
    catalogs/
      sql-mi-capabilities.json
  infra/
    main.bicep
    modules/
    parameters/
    README.md
  openapi/
    openapi.yaml
  rust/
    Cargo.toml
    Cargo.lock
    deny.toml
    rust-toolchain.toml
    src/
    tests/
  tests/
    fixtures/
      pricing/
      workbook-parity/
    e2e/
  web/
    package.json
    package-lock.json
    svelte.config.js
    vite.config.ts
    src/
  Dockerfile
  VERSION
  README.md
  THIRD-PARTY-DATA-EGRESS.md
```

## 22. CI/CD

### 22.1 Pull Request and Push CI

GitHub Actions MUST run:

Rust:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Frontend:

- `npm ci --prefix web`
- `npm run lint --prefix web`
- `npm run check --prefix web`
- `npm run test --prefix web`
- `npm run build --prefix web`

Contracts and infrastructure:

- Validate OpenAPI.
- Regenerate/check TypeScript API types.
- `az bicep build` for every Bicep entry point.
- Unit-test pricing fixture parsers.
- Build the Docker image without pushing on pull requests.

Use workflow concurrency with cancellation of superseded runs. Pin third-party actions to immutable commit SHAs.

### 22.2 Scheduled Assurance

Nightly or manual:

- `cargo llvm-cov --all-features --fail-under-lines 80`.
- `cargo audit`.
- `cargo deny check`.
- `npm audit --audit-level=high`.
- Playwright smoke suite against a deployed non-production environment.
- Public provider schema probes that validate parsing without asserting volatile prices.

### 22.3 Deployment

On push to `main`, after all quality jobs pass and application-owned files changed:

1. Azure login with GitHub OIDC.
2. Build image with BuildKit cache.
3. Push SHA-tagged image to ACR.
4. Update the Container App to that immutable tag.
5. Wait for revision readiness.
6. Smoke-test `/healthz`, `/readyz`, guest calculation with frozen cache, and authenticated project list.
7. Report the Container App FQDN as the environment URL.

Stable configuration and secrets must be managed separately from the hot image deployment path.

## 23. Testing Requirements

### 23.1 Domain Unit Tests

At minimum test:

- Standard and Enterprise source editions produce identical Azure targets for identical source CPU/RAM/IOPS.
- Source edition changes only AWS license cost.
- BYOL source license cost is zero.
- AHB changes Azure license cost but not target sizing.
- NGGP threshold is `min(80,000, 1,600 * vCores)`.
- IOPS equal to the threshold remains NGGP.
- IOPS one above the threshold requests BC.
- BC is never selected only because NGGP lacks RAM.
- `source_max_iops=0` requests NGGP.
- Smallest supported MI RAM meeting source RAM is selected.
- A candidate that fails SQL data capacity is rejected and the next capacity-valid SKU in the requested tier is selected with an explicit explanation.
- Additional RAM is charged once.
- RDS Multi-AZ quantity is not doubled.
- EC2 source IOPS is max persistent-volume IOPS, not sum.
- Ephemeral EBS cost is zero.
- gp3 included IOPS and throughput are honored per volume.
- io2 tier boundaries are correct.
- No mapping retains AWS cost and returns null Azure/savings/parity values.
- Portfolio parity excludes no-mapping rows from mapped totals.
- Decimal rounding is stable.
- On-prem hardware CAPEX is annualized only by depreciation years and is not discounted again.
- On-prem licensable cores round to two-core packs with a four-core minimum.
- On-prem License + SA cost applies the source license discount and annualizes over 12, 24, or 36 remaining coverage months.
- On-prem power-estimate coefficients, kWh, monthly electricity, annual electricity, and override precedence are exact and disclosed.

### 23.2 Frozen Workbook-Parity Tests

Create frozen fixtures from the current CSV files. Live provider calls MUST NOT be used for these tests.

Preserve source decimal text exactly when creating fixtures: UTF-8 CSV values become decimal strings and are parsed directly into `rust_decimal::Decimal`; missing values remain distinct from explicit zero. Record SHA-256 hashes of the frozen source files and fail fixture setup on hash or required-column mismatch. Do not round-trip fixtures through Excel during Rust tests.

With the current validated fixture set, assert within one cent:

- EC2 AWS total: `$136,345.20`.
- EC2 Azure total before parity: `$186,616.387146`.
- EC2 required adjustment: approximately `26.9382%`.
- RDS AWS all-row total: `$721,245.671934`.
- RDS AWS mapped-row total: `$346,373.10`.
- RDS Azure mapped-row total: `$162,952.224`.
- RDS unmapped row count: `2`.
- RDS required adjustment: approximately `-112.5611%`.
- Parity test difference after applying required adjustment: less than `$1.00` absolute.

Also retain the fixture anchor in which `r6id.8xlarge` maps to Sweden Central NGGP Premium Series, 32 vCores, 256 GB selected RAM, 224 GB included RAM, and 32 GB additional RAM at `$0.011663/GB-hour`.

These values are regression fixtures, not promises about future live prices.

### 23.3 Provider Contract Tests

For recorded provider responses:

- Parse AWS EC2 selector and leaf formats.
- Parse AWS RDS region index and offer files.
- Parse reservation recurring and upfront dimensions.
- Parse Azure Retail Prices pagination.
- Parse Azure calculator component references.
- Detect missing required fields and schema drift.
- Verify source URL, effective date, currency, and content hash persistence.

Live tests assert schema and non-negative prices, never exact current amounts.

### 23.4 API and Persistence Tests

- Guest calculation succeeds.
- Guest project CRUD returns `401`.
- Authenticated user can CRUD own projects.
- Authenticated principals with the same object ID in different Entra tenants resolve to different owners.
- An authenticated user requires no paid entitlement, role, or group to save a project.
- A newly authenticated user cannot use project, share, pricing, catalog, or calculation APIs until accepting the current privacy notice; session and consent endpoints remain available.
- Guests can calculate and display the privacy notice without accepting it.
- Contact permission defaults off, is independent of required notice acceptance, and requires a valid email only when enabled.
- Consent records are owner-scoped, store the current notice version and optional trusted display name, and never retain email when contact permission is false.
- User cannot read/update/delete another owner's project.
- ETag conflict returns `412`.
- Invalid decimals, enum values, and limits return Problem Details.
- Provider failure with valid stale cache calculates with warning.
- Provider failure without cache returns `PRICE UNAVAILABLE`.
- Project reopen retains the exact saved snapshots/results until refresh.
- Only an owner can create or revoke a project share; another owner cannot use those endpoints against the source project.
- A valid share resolves for any authenticated principal without exposing source ownership metadata, and saving the result creates a distinct recipient-owned project.
- Missing, malformed, incorrectly keyed, revoked, and expired shares disclose no project data; expiry is enforced at exactly 30 days.

### 23.5 Frontend and E2E Tests

Playwright MUST cover desktop and mobile:

- Guest creates a temporary project and sees no save action.
- Guest project state survives a reload in the same browser profile and is removed by the confirmed clear action.
- Entra test principal sees project CRUD.
- Independent AWS and Azure loading indicators appear and settle.
- Add EC2 resource with multiple volumes.
- Add RDS Multi-AZ resource.
- Add On-prem resource with distinct vCPU/licensable-core values, CAPEX depreciation, electricity, and License + SA inputs.
- Verify Azure RAM, vCores, hardware, and tier are derived outputs with no editable controls.
- Verify SQL data sizes format with valid grouping and never a leading separator.
- Edit and delete a row.
- Open explanation drawer and inspect IOPS and RAM decisions.
- No-mapping row displays no false Azure savings.
- Delete-project confirmation.
- Keyboard-only operation.
- No overlap at required viewports.

## 24. Performance and Reliability

- Initial static shell target: <250 KB compressed JavaScript excluding optional icon/font assets.
- Warm health response: <100 ms server processing.
- Calculation using cached prices for 100 resources: <500 ms server processing target.
- Project CRUD p95 target: <750 ms within the deployment region.
- Provider refresh may be slow; default upstream timeout is 30 seconds per request and overall provider resolution budget is 120 seconds.
- Retry transient upstream failures at most three times with exponential backoff and jitter.
- Bound provider concurrency to avoid memory spikes and upstream abuse.
- Static assets use immutable cache headers; API and identity endpoints use `no-store` where appropriate.

## 25. Documentation Deliverables

The implementation agent MUST create:

- Root `README.md` with local prerequisites, quickstart, architecture, and deployment.
- `infra/README.md` with parameters, OIDC setup, Entra app setup, Key Vault reference, `what-if`, deploy, and rollback.
- `THIRD-PARTY-DATA-EGRESS.md` covering AWS, Azure pricing, and Entra.
- OpenAPI document.
- Pricing source and formula documentation.
- Data-retention and deletion behavior.
- Troubleshooting for provider schema changes and stale cache.
- A short migration note explaining how frozen workbook fixtures map to Rust tests.

## 26. Implementation Sequence for the Coding Agent

The agent MUST implement in this order and keep every phase executable:

1. Scaffold repository, Rust crate, Svelte app, Dockerfile, CI, and Bicep validation.
2. Import frozen pricing/capability fixtures and implement decimal domain types.
3. Implement the pure target selector and cost engine with workbook-parity tests.
4. Implement AWS and Azure provider parsers against recorded fixtures.
5. Implement cache and Cosmos adapters.
6. Implement HTTP contracts, Problem Details, health, and static-file serving.
7. Implement guest mode and calculation flow.
8. Implement Container Apps Entra identity extraction and owner-scoped project CRUD.
9. Implement main screen, project settings, resource forms, resource table, totals, and explanation drawer.
10. Implement live provider refresh and independent loading states.
11. Implement Bicep deployment and GitHub OIDC deployment workflow.
12. Run all unit, integration, Playwright, security, and save/reopen parity checks.
13. Deploy to a non-production Azure environment and report the URL plus smoke-test results.

Do not postpone the domain tests until after UI work.

## 27. Definition of Done

The MVP is complete only when:

- A guest can calculate EC2, RDS, and On-prem resources but cannot save.
- An Entra-authenticated user can create, open, update, and delete only their projects.
- AWS and Azure prices can be fetched independently from public live sources with visible loading states.
- Cached/stale/error states are explicit and never silently converted to zero.
- Project settings match the workbook defaults and semantics.
- EC2, RDS, and On-prem source forms work.
- Every resource row supports edit, delete, and deterministic explanation.
- AWS edition affects source licensing only.
- NGGP/BC selection follows the exact IOPS rule.
- Unsupported selected-tier capacity produces `NO MAPPING` without false savings.
- Additional RAM is charged once.
- Portfolio parity uses mapped rows only while retaining the all-row AWS total.
- Frozen workbook-parity tests pass.
- CI, scheduled assurance, Docker build, Bicep validation, deployment, health checks, and Playwright smoke tests pass.
- The deployed app runs in Azure Container Apps using managed identity and contains no provider or Azure deployment secrets in source or image layers.
- The deployed Cosmos account has the `EnableServerless` capability and no provisioned throughput configuration.
- Every runtime Azure role assignment targets the Container App's system-assigned managed identity; no user-assigned identity, Azure resource key, or connection string is deployed.
