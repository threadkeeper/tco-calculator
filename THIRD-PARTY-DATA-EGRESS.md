# Third-Party Data Egress

Azure SQL TCO sends only public catalog selectors required to resolve prices. It never sends project names, workload names, quantities, customer inventories, totals, tenant identifiers, subscription identifiers, or commercial agreements to pricing providers.

The separately installed attended Azure Pricing Calculator companion is the narrow exception for project-derived target configuration. After explicit user activation, it enters the approved saved project name, workload names, and Azure target fields below into Microsoft's public Calculator. It does not send source infrastructure identifiers, owner identity, calculated totals, credentials, or commercial terms. The target configuration flow was approved by the authorized Privacy, Security, Legal/Terms, Product, and Architecture owner on 2026-08-23; transfer of the saved project and workload names was approved on 2026-08-24.

## Attended Azure Pricing Calculator Companion

- Destination: `https://azure.microsoft.com/en-us/pricing/calculator/` and Microsoft-operated requests initiated by that page.
- Data sent: saved project name as the Calculator estimate name; saved workload name as each Calculator line label; Azure region; SQL Managed Instance product/deployment choice; service tier; hardware family; vCores; selected memory; zone-redundancy setting; quantity; monthly usage hours; purchase plan/term; Azure Hybrid Benefit assumption; data storage; and approved neutral backup-storage values.
- Data never sent by the companion: project descriptions; server names; source cloud SKUs, capacity, costs, or infrastructure identifiers; project discounts; parity adjustments; expected or calculated amounts; tenant, subscription, billing, owner, or contact identifiers; commercial agreements; credentials; tokens; cookies; browser storage; screenshots; traces; page captures; or resulting Save/Share URLs.
- Credentials: the companion authenticates only to the TCO API through WAM and one delegated API scope. Calculator sign-in happens later in ordinary Edge after Playwright and controlled Edge are closed. The companion does not receive or inspect Calculator credentials, cookies, or tokens.
- Retention: the server manifest is purged when the companion acknowledges strict receipt. A minimal consumed idempotency tombstone may remain for 24 hours. The isolated Edge profile is deleted after ordinary Edge exits when possible, with app-root-only startup recovery for abandoned profiles. The TCO application stores no Calculator estimate or URL.
- User control: the attended user initiates transfer of the saved project and workload names plus target configuration, and performs Microsoft sign-in, agreement selection, Save, Share, and Export directly on Microsoft's origin. The feature stops on Calculator drift, challenge, value mismatch, or failure to preserve the unauthenticated estimate state through handoff/sign-in.

The Calculator page was observed making Microsoft-operated analytics and experience-configuration requests. Do not claim that entered target values remain solely in local browser storage or that Microsoft retains nothing. Microsoft's applicable service terms and privacy commitments govern processing after the user and companion enter data on that site.

## Companion GitHub Release Download

- Destination: this repository's fixed HTTPS GitHub Releases page or a server-selected immutable versioned MSIX asset under that release boundary.
- Data sent by the application: no project, launch, workload, owner, tenant, subscription, calculation, or customer data. Download URLs contain no application context.
- Data observed by GitHub: ordinary web and download metadata such as source IP address, user agent, requested asset, timestamp, and applicable GitHub account/session information under GitHub's terms and privacy statement.
- Credentials and retention: the companion sends no TCO or Microsoft access token to GitHub. GitHub controls retention of its service metadata. The TCO application does not persist download telemetry.
- User control: an internal pilot user explicitly opens the release page, downloads the self-signed development MSIX, verifies its SHA-256 and exact development publisher, independently trusts the matching public certificate with local administrator rights, and chooses whether to install the package. GitHub release authorship or attestation establishes provenance only; it does not make the development certificate publicly trusted or verify a GitHub identity in Windows. There is no `ms-appinstaller:` invocation, silent or automated install, background updater, automated certificate deployment, or endpoint-policy bypass.

## Client-Side CSV Export

The user can explicitly download the currently visible project and latest calculation as an Excel-compatible CSV. The browser constructs the file locally; no export API, server-side export copy, telemetry event, upload, or third-party destination is involved. The CSV contains confidential project settings, inventory inputs, server-returned results, and non-secret pricing provenance, but excludes owner and identity claims, display names, contact/consent data, ETags, capability secrets, and authorization metadata. Cells are quoted and text is hardened against spreadsheet formula injection. After download, the file is governed by the user's managed-device and storage controls.

As of 2026-08-10, local mode performs no pricing-provider egress. It loads a frozen reviewed public-price fixture. The source contains pure provider-response normalizers and a host-allowlisted bounded HTTPS transport, but live provider orchestration is not yet constructed in application state. See [docs/PRODUCTION-ADAPTER-READINESS.md](docs/PRODUCTION-ADAPTER-READINESS.md).

## AWS Public Pricing

- Destinations: AWS EC2 Calculator metered-unit maps and AWS Price List Bulk API endpoints listed in the specification.
- Data sent: currency, AWS region, service, SKU, operating system, tenancy, SQL edition, deployment, commercial term, and storage meter filters.
- Data received: public catalog metadata and public USD price dimensions.
- Credentials: none.
- Runtime usability: normalized snapshots are fresh through 24 hours, stale but usable through 7 days, and expired afterward for new calculations. The Cosmos document TTL may retain expired records for cleanup/audit policy, but it must not make them usable. Saved revisions embed only the exact resolved rates and provenance used.

## Azure Public Pricing

- Destinations: Azure Retail Prices API and the Azure SQL calculator composition endpoint listed in the specification.
- Data sent: currency, Azure ARM region, SQL Managed Instance service, tier, hardware, vCores, and purchase-option filters.
- Data received: public catalog metadata and public USD price dimensions.
- Credentials: none.
- Retention: same as AWS normalized pricing snapshots.

The Azure calculator endpoint is public but not a stable contract. Schema drift is a provider error and must fall back only to a still-valid cached snapshot.

## Microsoft Entra ID

Azure Container Apps built-in authentication performs sign-in and token validation. The browser uses platform login and logout routes. The Rust application receives only platform-validated principal claims at the protected ingress boundary and derives ownership from both `tid` and `oid`. It does not store or forward access tokens.

For signed-in privacy acceptance, the application stores the current notice version and acceptance time, optional Entra display name, and the independent Azure SQL contact choice. An email-like Entra value may prefill the contact field. The email is persisted only when contact permission is enabled; otherwise it is discarded. These fields stay in the user's owner-partitioned Cosmos record and are not sent to Entra, pricing providers, analytics services, or a CRM by this application.

The application has no endpoint to list or export contact opt-ins. Any future operator retrieval, CRM integration, or campaign flow is a new personal-data egress that requires documented purpose, authorized recipients, retention, access controls, audit evidence, and written Privacy, Security, architecture, and service-owner approval before implementation.

## Operational Telemetry

Structured application logs go to the environment's Azure Log Analytics workspace. Logs contain request IDs, route templates, status, duration, auth mode, provider/cache outcomes, formula version, and aggregate mapping counts. They must not contain workload names, raw identity headers, tokens, credentials, or full project payloads. No third-party analytics or behavioral tracking is used.

## Microsoft Foundry Model Router

- Purpose: signed-in assistant reasoning, closed host-tool selection, and bounded JPEG/PNG extraction. Foundry proposes project changes; the Rust host alone authorizes, validates, calculates, persists, and verifies them.
- Destination: the private data plane of the repository-owned Azure OpenAI account in Sweden Central, using a `DataZoneStandard` `model-router` deployment version `2025-11-18` over stable Chat Completions API `2024-10-21`.
- Eligible models: OpenAI `gpt-4.1-mini` version `2025-04-14` and `gpt-5-mini` version `2025-08-07` only. Routing mode is `balanced`; automatic model upgrades are disabled.
- Data sent: the signed-in user's current question; bounded tool schemas and results; only the redacted owner-scoped project fields needed for the turn; and, for an explicit image request, one metadata-free normalized JPEG attached to the first model request only. Project and workload names are replaced or excluded before model egress.
- Image intake: one declared JPEG or PNG up to 10 MiB, 25 megapixels, and 16,384 pixels per dimension. The server checks the signature, decodes with bounded limits, removes metadata, converts to RGB, and re-encodes as JPEG. Raw and normalized bytes are request-scoped and are not persisted or logged.
- Data never sent: email address, contact choice, display name, tenant/object IDs, owner IDs, identity headers, tokens, credentials, endpoints, ETags, share credentials, provider payloads, raw logs, unrelated projects, client-authored rates/totals, or customer commercial agreements.
- Authentication and network: Container App system-assigned managed identity with `Cognitive Services OpenAI User` at account scope; public access and local/key authentication disabled; private endpoint and `privatelink.openai.azure.com` private DNS.
- Retention: the application keeps no server-side conversation transcript, upload, normalized image, or model input/output. Browser transcript and selected-file state are memory-only. Azure service retention, abuse-monitoring, human-access, and incident terms remain governed by the applicable Microsoft service terms for the deployed account.
- Processing geography: `DataZoneStandard` constrains processing to the EU Data Zone for the Sweden Central deployment. Private networking controls transport but does not independently establish residency.
- Quotas and limits: 10 assistant turns per principal per minute, two concurrent model turns per application replica, at most eight model requests and twelve tool calls per turn, 4,000 output tokens per model call, and a 120-second whole-turn deadline. Azure quota must be checked before deployment.
- Logging: only request ID, prompt version, actual routed model when returned, model/tool counts, timings, and sanitized status codes. Prompts, project fields, tool arguments/results, files, and model text are prohibited from logs.
- Disable and rollback: omit or set `ASSISTANT_ENABLED=false` in a replacement application revision. No fallback service, public OpenAI endpoint, API key, queued assistant work, or conversation store exists.
- Accountable owner: repository owner acting as application and service owner under `docs/FOUNDRY-ASSISTANT-APPROVAL-PROPOSAL.md`.
- Effective review date: 2026-08-12. Re-review is required for any model, version, API, SKU, region, processing boundary, retention term, eligible data, dependency, or network change.