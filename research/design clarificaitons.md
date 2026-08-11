# Design Clarifications

This file intentionally uses the requested filename `design clarificaitons.md`.

Use this register to resolve product choices that are not fully determined by the workbook or the requested feature list. `Azure Specification.md` contains a provisional default for every question so implementation can begin before all answers are available.

Priority:

- `P0`: Confirm before production launch. The provisional default is sufficient for an MVP build.
- `P1`: Confirm during implementation or pilot.
- `P2`: Can remain a post-MVP decision.

Record decisions in the `Decision` field and update `Azure Specification.md`, acceptance tests, and infrastructure parameters when a provisional default changes.

## 1. Identity, Accounts, and Entitlement

### DC-001: Meaning of "free account"

Priority: P0  
Question: Does a free account mean an anonymous guest with no sign-in, an Entra-authenticated user on a free product tier, or a separate email/password account?  
Provisional default: Free use means anonymous guest mode. It can calculate but cannot persist projects. Any successful Entra sign-in enables project persistence.  
Why it matters: This changes identity architecture, user messaging, and whether a subscription/entitlement service is required.  
Decision: Free use is anonymous guest mode. Guests can calculate and keep a browser-local draft but cannot persist projects to the backend. Any successful Entra work/school sign-in enables owner-private project persistence.

### DC-002: Entra tenant model

Priority: P0  
Question: Must Entra sign-in be single-tenant, multi-tenant for any work/school account, or restricted to selected tenant IDs?  
Provisional default: Single-tenant means one organization selected at deployment through `ENTRA_TENANT_ID`. Container Apps authentication accepts only that tenant's issuer, and the backend requires the same tenant claim in its trusted client-principal header. Multi-tenant or allow-list support is post-MVP.  
Why it matters: It changes the app registration, issuer validation, consent, support, and data-isolation expectations.  
Decision: Accept work or school accounts from any Microsoft Entra tenant through the multi-tenant app registration. Do not accept personal Microsoft accounts or maintain a tenant allow-list in v1.

### DC-003: Save entitlement

Priority: P0  
Question: Does every Entra-authenticated user receive save capability, or is save capability a paid/assigned entitlement?  
Provisional default: Every authenticated user can save; only anonymous guests cannot.  
Why it matters: A paid or assigned tier requires billing or group/role checks not otherwise in MVP scope.  
Decision: Every successfully authenticated Entra user can save owner-private projects. Do not require payment, app roles, groups, or assigned entitlements in v1.

### DC-004: Administrator role

Priority: P1  
Question: Is an administrator role required to inspect health, refresh global price caches, view usage, or delete user projects?  
Provisional default: No application administrator UI. Azure operators use platform logs and deployment tools.  
Why it matters: Admin access adds authorization roles, privileged endpoints, audit requirements, and UI.  
Decision: Do not build an application administrator role, UI, privileged project access, or global cache controls in v1. Azure operators use platform logs and deployment tools.

### DC-005: Guest local persistence

Priority: P1  
Question: May a guest project survive browser reload in local storage or IndexedDB even though it is not saved to the server?  
Provisional default: No. Guest state is memory-only and is lost after a warning.  
Why it matters: Browser persistence can feel like saving and may conflict with the stated free-account restriction.  
Decision: Persist one guest project draft in browser-local durable storage, preferably IndexedDB, so it survives reloads and browser restarts on that profile. Never synchronize it to backend project storage; provide a confirmed clear action.

## 2. Projects and Collaboration

### DC-006: Project sharing

Priority: P1  
Question: Must projects be shareable with other users or teams?  
Provisional default: No. Projects are private to one Entra principal.  
Why it matters: Sharing requires roles, invitations, ownership transfer, and a different Cosmos partition strategy.  
Decision: Updated 2026-08-11 with repository-owner approval. V1 supports reusable capability links that expire after 30 days and may be opened by any authenticated Entra user who possesses the link. Recipients edit an unsaved snapshot and may save it only as a new project under their own principal. The source remains owner-private and cannot be modified through the share. Invitations, teams, roles, organization workspaces, and ownership transfer remain excluded.

### DC-007: Project deletion and retention

Priority: P0  
Question: Should delete be permanent immediately, soft-delete with recovery, or retain an audit copy for a fixed period?  
Provisional default: Immediate hard delete after confirmation.  
Why it matters: Retention requirements affect privacy, Cosmos TTL, support, and audit behavior.  
Decision: Immediately hard-delete the owner-scoped project after confirmation. Do not retain a recovery tombstone or application audit copy in v1.

### DC-008: Project version history

Priority: P1  
Question: Must users restore prior settings, resources, price snapshots, or calculation revisions?  
Provisional default: Store only the latest successful calculation revision and its snapshot references.  
Why it matters: Full history increases storage and requires revision browsing/restoration UX.  
Decision: Persist only the latest successful calculation revision and its immutable snapshot references. Do not scaffold revision browsing or restoration.

### DC-009: Project size limits

Priority: P1  
Question: What is the expected maximum number of resources and EBS volumes per project?  
Provisional default: 100 resources per project and 50 EBS volumes per EC2 resource.  
Why it matters: It determines whether resources can remain embedded in one Cosmos project document.  
Decision: Limit each project to 100 resources and each EC2 resource to 50 EBS volumes. Keep resources embedded in the owner-partitioned project document for v1.

### DC-010: Import, export, and duplication

Priority: P1  
Question: Must users import images, CSV/Excel inventories, or PDFs, export project results, or duplicate projects in v1?
Provisional default: None were included in the first scaffold. Resources were added through the form.
Why it matters: File intake processes confidential workload data and changes model, parser, validation, privacy, security, and user-review requirements.
Decision: Updated 2026-08-11 with repository-owner approval. JPEG and PNG upload through the Foundry-backed assistant is the primary v1 assisted project-input method. It produces a typed, validated draft or patch for explicit user review and never saves automatically. Defer CSV, Excel, and PDF import, result export, and project duplication. Users may continue to add resources through the type-specific form.

## 3. Regions and Currency

### DC-011: AWS region scope

Priority: P0  
Question: Is one AWS source region required per project, or may each resource use a different AWS region?  
Provisional default: One AWS source region per project.  
Why it matters: Mixed regions change settings, cache keys, table columns, price refresh, and totals.  
Decision: Select one AWS source region in the project settings for EC2 and RDS projects. Every resource in that project uses the selected region; v1 does not support multi-region source projects.

### DC-012: Azure target region

Priority: P0  
Question: Must Sweden Central remain fixed, or should users be able to choose another Azure SQL MI region?  
Provisional default: Sweden Central is fixed and read-only.  
Why it matters: Configurable target regions multiply capability validation, prices, caching, and regression fixtures.  
Decision: Select one supported Azure SQL MI target region in the project settings. Every resource in that project uses the selected target region; v1 does not support multi-region target projects. Sweden Central remains the default and frozen workbook-parity region.

### DC-013: Application deployment region

Priority: P1  
Question: Should the web application itself always deploy in Sweden Central, or only the modeled SQL MI target?  
Provisional default: Azure resources deploy in Sweden Central unless an infrastructure parameter overrides it; the modeled target remains Sweden Central.  
Why it matters: Container Apps/Cosmos availability and data residency may differ from the modeled migration region.  
Decision: Deploy the v1 development application infrastructure in South Africa North (`southafricanorth`). This is independent of the Azure SQL MI target region selected in each project.

### DC-014: Currency and tax

Priority: P0  
Question: Is USD/tax-excluded sufficient, or must the application support other currencies, VAT, or exchange rates?  
Provisional default: USD only, tax excluded.  
Why it matters: Multi-currency changes provider requests, formatting, snapshots, totals, and rate provenance.  
Decision: Use USD only and exclude tax. On-prem users manually convert hardware, electricity, and License + SA inputs to USD before entry; the app performs no foreign-exchange conversion.

## 4. Pricing Sources and Refresh

### DC-015: Retail versus negotiated pricing

Priority: P0  
Question: Are public retail/list prices plus user-entered discounts sufficient, or must the app fetch customer-specific AWS/Azure agreement prices?  
Provisional default: Public prices plus independent discount fields are sufficient.  
Why it matters: Negotiated pricing would require credentialed provider integrations, consent, secret handling, and account-level authorization.  
Decision: Use public retail/list prices plus the six project discount fields. Do not fetch customer agreement pricing or request provider credentials in v1.

### DC-016: Azure calculator endpoint policy

Priority: P0  
Question: May the application use Azure's public but undocumented SQL calculator composition endpoint to preserve all workbook purchase options, or must it use only documented APIs?  
Provisional default: Use it behind a replaceable adapter, validate its schema, and fall back to the last verified snapshot. Azure Retail Prices remains the documented primary meter source.  
Why it matters: Official-only policy may require dropping or reconstructing savings-plan/configured SKU options.  
Decision: Permit the public undocumented Azure SQL calculator composition endpoint behind a replaceable adapter. Validate its schema and use the last valid cache when allowed; keep Azure Retail Prices as the documented primary meter source.

### DC-017: Provider login fallback

Priority: P0  
Question: If a future price dimension is gated, should an end user connect AWS/Azure interactively, should an administrator configure a service connection, or should the dimension remain unavailable?  
Provisional default: Do not prompt end users. Mark the dimension unavailable and use a valid cache if present. Add credentialed adapters only as a separately approved feature.  
Why it matters: Cloud-console login creates a substantially larger security and support surface.  
Decision: Never prompt end users for AWS/Azure credentials. Use a valid cache when allowed or mark the dimension unavailable; credentialed adapters require separate approval.

### DC-018: Automatic refresh behavior

Priority: P1  
Question: Should a saved project refresh prices automatically when opened, prompt when stale, or refresh only when the user selects `Refresh prices`?  
Provisional default: Render the saved calculation unchanged and let the user explicitly refresh. Show age and stale status.  
Why it matters: Automatic refresh can silently change a saved business case.  
Decision: Render saved results unchanged when a project opens. Refresh prices only when the user explicitly selects `Refresh prices`; show snapshot age and stale status and never silently alter a saved business case.

### DC-019: Freshness windows

Priority: P1  
Question: Are 24 hours fresh, 7 days stale-but-usable, and 30 days cache retention acceptable?  
Provisional default: Yes. Snapshots older than 7 days cannot be used for a new calculation.  
Why it matters: It balances provider availability, calculation freshness, and reproducibility.  
Decision: Treat snapshots as fresh through 24 hours, stale-but-usable through 7 days, and expired after 7 days for new calculations. Retain cache documents for 30 days.

### DC-020: Stale fallback consent

Priority: P1  
Question: May calculation proceed automatically with a stale snapshot, or must the user explicitly accept it?  
Provisional default: Proceed when the snapshot is at most 7 days old, with a persistent warning and provenance in every affected row.  
Why it matters: Some governance models prohibit decisions based on stale prices without acknowledgement.  
Decision: Automatically calculate with a snapshot no older than 7 days, with a persistent warning and source provenance in every affected row. Do not require separate consent.

### DC-021: Scheduled global price refresh

Priority: P2  
Question: Is an Azure Container Apps Job required to pre-warm/refresh common catalogs on a schedule?  
Provisional default: No background job. User-driven refresh and cache coalescing are sufficient.  
Why it matters: A scheduled job improves first-use latency but adds infrastructure and operational ownership.  
Decision: Do not deploy a scheduled provider-refresh job in v1. Use explicit user refresh and cache coalescing; scheduled CI assurance does not mutate runtime prices.

## 5. Source Resource Scope

### DC-022: EC2 source commercial terms

Priority: P0  
Question: Must EC2 source pricing support Reserved Instances or Savings Plans, or remain Shared Windows On-Demand as in the validated workbook?  
Provisional default: Shared Windows On-Demand only for EC2 source cost.  
Why it matters: Supporting commitments adds term, payment option, offering class, and amortization fields.  
Decision: Preserve workbook parity: EC2 source compute uses current-generation x86_64 Shared Windows On-Demand only in v1.

### DC-023: RDS commercial terms

Priority: P1  
Question: Which RDS terms must be exposed: On-Demand, one-year reservations, three-year reservations, and which payment options?  
Provisional default: Expose every normalized term returned by the live catalog for the selected instance/deployment.  
Why it matters: A smaller approved list would simplify the form and tests.  
Decision: Expose every normalized On-Demand and Reserved term/payment-option combination returned by the workbook-compatible live catalog for the selected region, instance, and deployment. Do not use a smaller hard-coded allow-list.

### DC-024: SQL Server Web edition

Priority: P1  
Question: Must source SQL Server Web edition be supported?  
Provisional default: No. MVP supports Standard and Enterprise only, matching the current converter UI.  
Why it matters: Web edition requires source rates, licensing UX, and compatibility decisions.  
Decision: Support Standard and Enterprise source editions only. Do not include SQL Server Web edition in v1.

### DC-025: EBS volume types

Priority: P1  
Question: Are `gp3`, `io2`, and ephemeral sufficient, or must `gp2`, `io1`, st1, sc1, and standard magnetic be supported?  
Provisional default: `gp3`, `io2`, and ephemeral only.  
Why it matters: Every additional type has distinct pricing and performance rules.  
Decision: Support `gp3`, `io2`, and ephemeral volumes only in v1, matching the workbook-compatible converter.

### DC-026: RDS storage charges

Priority: P0  
Question: Should the web app preserve the workbook exclusion of RDS provisioned IOPS/throughput charges, or add them now that users enter IOPS?  
Provisional default: Preserve the exclusion and show it prominently in the explanation. IOPS selects Azure tier only.  
Why it matters: Including these charges could materially change AWS totals and requires additional live pricing dimensions.  
Decision: Preserve workbook behavior: price the deployment-specific RDS GB-month storage rate, but do not add provisioned IOPS or throughput charges to AWS source cost. Entered IOPS influences Azure tier selection only.

### DC-027: Source RAM override

Priority: P1  
Question: May users override RAM derived from the selected AWS SKU, or should it always equal catalog memory?  
Provisional default: Pre-fill catalog RAM and allow override to model observed workload requirements.  
Why it matters: An override is useful but can create a source configuration that no longer literally matches the AWS SKU.  
Decision: Allow an EC2 RAM override and use it as the authoritative Azure sizing input. Show both catalog RAM and effective overridden RAM in the row explanation when they differ.

### DC-028: Mixed SQL data and EBS capacity

Priority: P1  
Question: For EC2, should Azure storage be based on the explicit SQL data GB field, the sum of persistent EBS capacity, or a selectable method?  
Provisional default: Azure storage uses explicit SQL data GB; EBS volume capacity prices AWS storage.  
Why it matters: The workbook intentionally distinguishes provisioned disk capacity from actual SQL data.  
Decision: Use explicit SQL data GB for Azure storage sizing and pricing. Use each persistent EBS volume's provisioned capacity only for AWS EC2 storage cost.

## 6. Target Selection and Calculations

### DC-029: Strict Business Critical rule

Priority: P0  
Question: Confirm that Business Critical is selected only when source max IOPS exceeds `min(80,000, 1,600 * NGGP vCores)`, never because of Enterprise edition or RAM alone.  
Provisional default: Confirmed by specification. If low-IOPS NGGP cannot fit RAM/vCPU, result is `NO MAPPING` even if BC could fit.  
Why it matters: This is the most important target-selection rule and can create intentional no-fit rows.  
Decision: Select Business Critical only when source max IOPS exceeds `min(80,000, 1,600 * NGGP vCores)`. Enterprise edition, RAM, and storage alone never select BC; a no-fit candidate in the IOPS-requested tier returns `NO MAPPING`.

### DC-030: Zero IOPS semantics

Priority: P0  
Question: Does `source_max_iops=0` mean unknown/unspecified and therefore request NGGP, or should the resource be blocked until IOPS is known?  
Provisional default: Zero means unspecified and requests NGGP.  
Why it matters: RDS catalogs do not contain workload provisioned IOPS.  
Decision: Treat zero IOPS as unknown or unspecified and request NGGP. Do not block the resource.

### DC-031: Storage capacity as a sizing constraint

Priority: P0  
Question: Must Azure target selection reject or upscale candidates whose supported storage capacity is below SQL data GB?  
Provisional default: Validate known storage limits and return a warning or `NO MAPPING`; do not select BC solely because of storage architecture.  
Why it matters: The workbook primarily selects on vCPU/RAM/IOPS and does not fully encode every storage-capacity limit.  
Decision: Enforce known SQL MI storage limits during candidate selection. Reject undersized candidates and select the next larger SKU in the IOPS-requested service tier, explaining the capacity-driven bump in the row information view. Storage alone does not switch NGGP to Business Critical; return `NO MAPPING` if the requested tier has no capacity-valid candidate.

### DC-032: General Purpose tier

Priority: P1  
Question: Should legacy General Purpose ever be selectable/mappable, or should all default-tier mappings use only Next Generation General Purpose?  
Provisional default: NGGP and BC only. General Purpose may exist in source data but is not selected.  
Why it matters: It affects availability in regions/shapes where NGGP is missing.  
Decision: Map only Next Generation General Purpose and Business Critical in v1. Never offer or select legacy General Purpose as a fallback.

### DC-033: Zone redundancy

Priority: P1  
Question: Should users be able to request zone-redundant SQL MI targets?  
Provisional default: No. Workbook-parity NGGP candidates are non-zone-redundant; the selected architecture is shown in the explanation.  
Why it matters: Zone redundancy changes price, capability, and candidate ordering.  
Decision: Do not offer or map zone-redundant SQL MI targets in v1. Show the selected non-zone-redundant architecture in the explanation.

### DC-034: RDS Multi-AZ quantity

Priority: P0  
Question: Confirm that quantity one means one logical Multi-AZ deployment whose AWS price already includes HA, and it maps to one SQL MI target.  
Provisional default: Confirmed by specification.  
Why it matters: Doubling quantity would double both source and target incorrectly.  
Decision: Quantity one is one logical Multi-AZ deployment. Price one SQL MI in the project's selected Azure region; do not double quantity or model a cross-region replica in v1.

### DC-035: EC2 HA pairs

Priority: P1  
Question: When an EC2 workload has quantity two for an HA pair, should it always map to two SQL MIs, or can one managed SQL MI replace the pair?  
Provisional default: Quantity is preserved exactly; quantity two maps and prices two MI instances, matching the current workbook row.  
Why it matters: This can materially change target cost and depends on migration architecture.  
Decision: Preserve EC2 quantity exactly. A quantity of two prices two source instances and two SQL MI targets; do not consolidate an HA pair automatically.

### DC-036: Annual hours

Priority: P1  
Question: Is annual hours always per source instance/logical deployment and copied unchanged to each MI target?  
Provisional default: Yes. Default 8,760; valid range 0-8,784.  
Why it matters: Scheduled non-production workloads and Multi-AZ interpretation depend on it.  
Decision: Annual hours apply per source instance or logical deployment and copy unchanged to each target. Default to 8,760 with a valid range of 0-8,784.

### DC-037: AHB eligibility enforcement

Priority: P0  
Question: Should the app merely warn when an AHB option is selected, or require an explicit eligibility attestation?  
Provisional default: Show a warning only; the user remains responsible for licensing eligibility.  
Why it matters: An attestation creates audit and persistence requirements.  
Decision: Show a prominent eligibility warning when an AHB option is selected. Do not require or persist an eligibility attestation in v1; the user remains responsible for confirming active Software Assurance or qualifying subscription rights.

### DC-038: License fallback policy

Priority: P1  
Question: Confirm the regional per-core fallback with a four-core minimum when AWS lacks a small-shape edition price.  
Provisional default: Preserve the workbook behavior and disclose it in the explanation.  
Why it matters: Alternative fallback methods change source license cost.  
Decision: Preserve and disclose the workbook's regional per-core source-license fallback with a four-core minimum when AWS lacks the small-shape edition price.

### DC-039: Negative parity adjustment display

Priority: P1  
Question: Should a negative required parity adjustment be displayed as a negative percentage, as `Azure already lower`, or both?  
Provisional default: Display both the signed percentage and the plain-language state. The selected adjustment input remains restricted to 0-100%.  
Why it matters: Negative discount language can confuse users.  
Decision: Display both the signed required adjustment and the plain-language `Azure already lower` state. Keep the user-selected adjustment input constrained to 0-100%.

### DC-040: No-mapping Azure values

Priority: P0  
Question: Should unavailable Azure financial fields be blank/null, zero with a warning, or hidden?  
Provisional default: API returns null; UI displays an em dash and `NO MAPPING`. AWS cost remains visible.  
Why it matters: Zero can be mistaken for a real free target and create false savings.  
Decision: Return unavailable Azure financial fields as `null`; display an em dash and `NO MAPPING` while keeping valid source cost visible. Never serialize a misleading numeric zero.

## 7. User Experience

### DC-041: Product name and branding

Priority: P1  
Question: What product name, logo, and organization branding should be used?  
Provisional default: `Azure SQL TCO` with a neutral wordmark and no external brand assets.  
Why it matters: It affects page titles, PWA metadata, Entra app name, and deployment resources.  
Decision: Use `Azure SQL TCO` with a neutral text wordmark and no external brand assets in v1.

### DC-042: EC2 and RDS table organization

Priority: P1  
Question: Should EC2 and RDS resources appear in one table, separate tabs, or separate project types?  
Provisional default: One project with `All`, `EC2`, and `RDS` tabs.  
Why it matters: A unified portfolio enables one parity total but has many conditional columns.  
Decision: Use separate, immutable `EC2`, `RDS`, and `On-prem` project types. A project contains resources of one type only and shows one type-specific resource table without cross-source tabs.

### DC-043: Default visible columns

Priority: P1  
Question: Must every workbook column be visible by default, or may detailed cost columns be collapsed into groups?  
Provisional default: Show core inputs, target, AWS total, Azure total, savings, and parity; allow users to expand component groups.  
Why it matters: A literal spreadsheet width is difficult on laptops and mobile screens.  
Decision: Show core inputs, selected target, source total, Azure total, savings, and parity by default. Put component cost groups behind explicit expansion controls.

### DC-044: Explanation depth

Priority: P1  
Question: Should the info drawer show every candidate SKU or only the selected candidate and rejected-constraint summary?  
Provisional default: Show selected candidate, decision threshold, and a collapsible ordered candidate/rejection list.  
Why it matters: Full candidate lists can be large but are valuable for auditability.  
Decision: Show the selected candidate and decision threshold first, then a collapsible ordered candidate/rejection list.

### DC-045: Mobile support level

Priority: P1  
Question: Must users be able to complete full project editing on mobile, or is mobile read-only acceptable?  
Provisional default: Full add/edit/delete and explanation flows work at 360px, using expanded row summaries and horizontal table scrolling.  
Why it matters: Full mobile forms increase interaction and testing effort.  
Decision: Support full project and resource add/edit/delete/explanation flows at 360px, using compact expandable row summaries and horizontal table scrolling.

### DC-046: Unsaved-change behavior

Priority: P1  
Question: Should authenticated changes auto-save, save explicitly, or save each resource mutation immediately?  
Provisional default: Explicit `Save project` with dirty state; resource calculations can run before save.  
Why it matters: Auto-save interacts with ETags, stale pricing, and accidental edits.  
Decision: Use explicit `Save project` with visible dirty state. Calculations may run before save, but settings/resources/results persist only when the user saves the complete validated project with ETag concurrency. Do not auto-save mutations.

## 8. Azure Operations and Governance

### DC-047: Cosmos capacity mode

Priority: P0  
Question: Should Cosmos use serverless, free-tier provisioned throughput, or autoscale provisioned throughput?  
Provisional default: Serverless for MVP, configurable in Bicep.  
Why it matters: Subscription constraints, private networking, throughput, and cost differ.  
Decision: Deploy Cosmos DB for NoSQL with the `EnableServerless` capability and consumption-based Request Units. Do not configure free-tier, manual, or autoscale provisioned throughput.

### DC-048: Container App minimum replicas

Priority: P1  
Question: Is scale-to-zero acceptable in production, or is always-on response latency required?  
Provisional default: Dev min replicas 0; production min replicas 1.  
Why it matters: It trades fixed cost for cold-start latency.  
Decision: The development Container App may scale to zero with minimum replicas set to 0.

### DC-049: Private networking

Priority: P0  
Question: Is a VNet-integrated Container Apps environment with private Cosmos endpoint mandatory for every environment or production only?  
Provisional default: Enabled by default for all shared Azure environments; local development uses the Cosmos emulator or an explicitly approved public dev endpoint.  
Why it matters: Private networking affects cost, deployment time, and hosted-runner integration tests.  
Decision: Require a VNet-integrated Container Apps environment, Cosmos private endpoint, private DNS, and disabled Cosmos public network access in every deployed Azure environment, including development. Local workstation development is not an Azure environment and may use an emulator.

### DC-050: Data classification

Priority: P0  
Question: Are workload names, server identifiers, and cost assumptions classified as confidential, and are there residency or encryption requirements beyond Azure defaults?  
Provisional default: Treat project data as confidential business data; do not log names; use encryption at rest, private networking, and managed identity. No application field encryption in MVP.  
Why it matters: Higher classification may require customer-managed keys, field encryption, audit logs, and restricted operators.  
Decision: Treat project data as confidential business data. Do not log workload names or server identifiers; use private networking, managed identity, and Azure encryption at rest with Microsoft-managed keys. Customer-managed keys and application field encryption are not required for v1.

### DC-051: Telemetry and analytics

Priority: P1  
Question: Is product analytics allowed, and if so must users consent?  
Provisional default: Operational telemetry only; no third-party analytics and no behavioral tracking.  
Why it matters: Analytics changes privacy notices, CSP, egress, and consent.  
Decision: Collect operational telemetry only. Do not add third-party analytics, behavioral tracking, or user-level product analytics in v1.

### DC-052: Availability and recovery targets

Priority: P1  
Question: What SLA, RTO, and RPO are required?  
Provisional default: MVP best-effort single-region deployment, Cosmos-managed durability, redeployable infrastructure, no cross-region failover.  
Why it matters: High availability may require multi-region Cosmos, Front Door, multiple Container Apps environments, and tested failover.  
Decision: Use a best-effort single-region development deployment with Cosmos-managed durability and redeployable infrastructure. Do not scaffold cross-region failover or claim an SLA, RTO, or RPO.

### DC-053: Custom domain

Priority: P2  
Question: Is a custom domain and managed certificate required at launch?  
Provisional default: Use the Container App FQDN for MVP; Bicep leaves a documented extension point.  
Why it matters: It adds DNS ownership and certificate deployment steps.  
Decision: Use the generated Container App FQDN in v1. Do not provision a custom domain or managed certificate.

### DC-054: Guest abuse limits

Priority: P0  
Question: What public guest request and pricing-refresh limits are acceptable?  
Provisional default: Per-IP limits of 60 API requests/minute, 6 live provider refreshes/hour, and 10 concurrent calculation requests/application instance, all configurable.  
Why it matters: Live public-price retrieval can be expensive in bandwidth and vulnerable to abuse.  
Decision: Default to 60 API requests per minute per IP, 6 live provider refreshes per hour per IP, and 10 concurrent calculation requests per application replica. Keep all limits configurable through validated deployment settings.

### DC-055: Supported browsers

Priority: P1  
Question: Which browser versions must be supported?  
Provisional default: Current and previous major versions of Edge, Chrome, Firefox, and Safari.  
Why it matters: It defines polyfills, PWA behavior, and test matrix.  
Decision: Support the current and previous major versions of Edge, Chrome, Firefox, and Safari.

### DC-056: Localization

Priority: P2  
Question: Is English-only UI acceptable for MVP?  
Provisional default: English only, with locale-aware numeric formatting.  
Why it matters: Localization affects every UI string and validation message.  
Decision: Use an English-only v1 UI with locale-aware numeric formatting and centralized user-facing strings.

## 9. Remaining Product and Delivery Choices

### DC-057: No-mapping recommendations

Priority: P1  
Question: When a resource has `NO MAPPING`, should the app recommend splitting the workload, another Azure service/tier, or only explain the failed constraints?  
Provisional default: Explain the requested tier, candidate limits, and failed vCPU/RAM/IOPS constraints only. Do not recommend an unmodeled architecture or divide a workload automatically.  
Why it matters: Recommendations would need separate sizing rules, costs, and validation to avoid presenting speculation as a supported target.  
Decision: Explain the requested tier, candidate limits, and failed constraints only. Do not recommend workload splitting or an unmodeled service or architecture.

### DC-058: Azure environment topology

Priority: P0  
Question: How many Azure environments, subscriptions, and resource groups are required for implementation and launch?  
Provisional default: Separate `dev` and `prod` resource groups in one subscription, each deployed from the same Bicep modules with separate Container Apps, Cosmos accounts, registries, identities, and Entra configuration. A test environment is created on demand.  
Why it matters: Subscription boundaries, names, OIDC federated credentials, DNS, budgets, and data isolation depend on this choice.  
Decision: Provision only one development environment for v1 in the existing subscription and development resource group. Do not scaffold or deploy production or test environment parameter sets until separately requested.

### DC-059: PWA and offline behavior

Priority: P2  
Question: Is installable PWA or offline project viewing/calculation required?  
Provisional default: No service worker or offline mode in MVP. The app requires the server for authoritative calculations and current price state.  
Why it matters: Offline behavior creates cache invalidation, identity, stale-price, and unsaved-project semantics that are not part of the calculator today.  
Decision: Do not register a service worker or support installable PWA/offline project behavior in v1.

### DC-060: Entra authentication implementation

Priority: P0  
Question: Is Azure Container Apps built-in authentication an approved platform constraint, or must the Svelte/Rust application implement MSAL and token validation itself?  
Provisional default: Use Container Apps built-in authentication, platform login/logout routes, and trusted injected principal headers exactly as specified. Do not add MSAL to the frontend.  
Why it matters: Application-managed OAuth adds token storage, refresh, validation, and a larger security surface.  
Decision: Use Azure Container Apps built-in Entra authentication, platform login/logout routes, and trusted injected principal headers. Do not add frontend MSAL or application-managed token validation in v1.

### DC-061: Additional accessibility or compliance targets

Priority: P1  
Question: Are certifications or standards beyond WCAG 2.2 AA required, such as EN 301 549, Section 508, SOC 2 controls, or an internal design system?  
Provisional default: Meet and test WCAG 2.2 AA; document operational security controls without claiming an unperformed certification.  
Why it matters: Additional targets may add evidence, audit, design-system, and procurement requirements.  
Decision: Meet and test WCAG 2.2 AA. Do not claim additional certification without approved requirements and completed evidence.

### DC-062: SQL feature compatibility assessment

Priority: P2  
Question: Should the app eventually assess SQL Server features that can block or alter a SQL Managed Instance migration?  
Provisional default: Excluded from MVP. The calculator sizes cost and capacity only and must not imply migration compatibility.  
Why it matters: Compatibility assessment requires schema/workload collection, sensitive uploads, and a separate rules engine.  
Decision: Exclude SQL feature-compatibility assessment from v1. The product estimates cost and capacity and must not imply migration compatibility.

### DC-063: On-prem source cost model

Priority: P0  
Question: How should one-time server hardware, SQL License plus Software Assurance, electricity, discounts, and power consumption be normalized into an annual On-prem source cost?  
Provisional default: None; this project type was added after the original register.  
Why it matters: These inputs define the On-prem API contract, annual source total, AHB context, and parity comparison.  
Decision: The user enters one final net server CAPEX amount in USD, including compute and storage hardware but excluding SQL licensing, plus depreciation years. Do not apply source compute or storage discounts. The user enters separate vCPU and licensable-core counts, Enterprise and Standard License + SA prices per two-core pack, source SQL license discount, and 12/24/36 remaining coverage months. The user enters electricity directly in USD/kWh. Estimate server kW with a versioned formula based on vCPU, RAM, and SQL data storage, allow a per-resource kW override, and disclose all coefficients and values. Do not perform foreign-exchange conversion.

### DC-064: Signed-in privacy acceptance and Azure SQL contact

Priority: P0
Question: What notice, acceptance gate, profile fields, contact choice, retrieval path, and lifecycle apply to authenticated pilot users?
Provisional default: Do not collect additional profile or marketing-consent data until the controller, notice, retention, and authorized use are approved.
Why it matters: This adds personal data to Cosmos, gates authenticated use, and creates a potential marketing purpose and operator-access path.
Decision: Updated 2026-08-11 with repository-owner direction. Use an app-specific internal-pilot notice that supplements the Microsoft Privacy Statement and requires Privacy/Legal approval before external or production use. Require versioned acceptance after Entra sign-in and before other authenticated application use; guests only display the notice. Keep the Azure SQL contact checkbox separate, optional, and off by default. Save one owner-partitioned Cosmos profile containing acceptance version/time, contact choice, optional Entra display name, and email only when contact is allowed. Prefer an email-like Entra value for prefill and otherwise request a user-entered email only when opted in. Do not build an app retrieval/export endpoint or CRM egress. Fulfill correction, deletion, and withdrawal through the approved Microsoft privacy-request process and remove remaining records when pilot data is decommissioned. Use scoped disclosure language; do not promise that data is never disclosed to third parties because explicit project sharing, Azure processors, and legal disclosures exist.

## 10. Recommended Decision Order

Resolve these first because they can change architecture or financial behavior:

1. DC-001 free-account meaning.
2. DC-002 Entra tenant model.
3. DC-003 save entitlement.
4. DC-011 AWS region scope.
5. DC-012 Azure target region.
6. DC-015 retail versus negotiated pricing.
7. DC-016 Azure calculator endpoint policy.
8. DC-022 EC2 source commercial terms.
9. DC-026 RDS IOPS/throughput source charges.
10. DC-029 strict Business Critical rule.
11. DC-031 storage-capacity constraint.
12. DC-034 Multi-AZ quantity semantics.
13. DC-037 AHB eligibility behavior.
14. DC-047 Cosmos capacity mode.
15. DC-050 data classification.
16. DC-054 guest abuse limits.
17. DC-058 Azure environment topology.
18. DC-060 Entra authentication implementation.
