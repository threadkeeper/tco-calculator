# Third-Party Data Egress

Azure SQL TCO sends only public catalog selectors required to resolve prices. It never sends project names, workload names, quantities, customer inventories, totals, tenant identifiers, subscription identifiers, or commercial agreements to pricing providers.

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

- Purpose: signed-in assistant text responses and selection of closed, host-owned read-only tools. Foundry supplies inference only and cannot authorize, calculate, persist, confirm, or execute application actions.
- Destination: the private data plane of the repository-owned Azure OpenAI account in Sweden Central, using a `DataZoneStandard` `model-router` deployment version `2025-11-18` over stable Chat Completions API `2024-10-21`.
- Eligible models: OpenAI `gpt-4.1-mini` version `2025-04-14` and `gpt-5-mini` version `2025-08-07` only. Routing mode is `balanced`; automatic model upgrades are disabled.
- Data sent: the signed-in user's current question and only the bounded help or owner-scoped project fields required for that turn. Project and workload names are excluded. Image bytes are not sent until the separately recorded normalization path is implemented and validated.
- Data never sent: tenant/object IDs, identity headers, tokens, credentials, endpoints, ETags, share credentials, provider payloads, raw logs, unrelated projects, client-authored rates/totals, or customer commercial agreements.
- Authentication and network: Container App system-assigned managed identity with `Cognitive Services OpenAI User` at account scope; public access and local/key authentication disabled; private endpoint and `privatelink.openai.azure.com` private DNS.
- Retention: the application keeps no server-side conversation transcript and persists no model input or output. Browser transcript state is memory-only. Azure service retention, abuse-monitoring, human-access, and incident terms remain governed by the applicable Microsoft service terms for the deployed account.
- Processing geography: `DataZoneStandard` constrains processing to the EU Data Zone for the Sweden Central deployment. Private networking controls transport but does not independently establish residency.
- Quotas and limits: 10 assistant turns per principal per minute, two concurrent model turns per application replica, at most eight model requests and twelve tool calls per turn, 4,000 output tokens per model call, and a 120-second whole-turn deadline. Azure quota must be checked before deployment.
- Logging: only request ID, prompt version, actual routed model when returned, model/tool counts, timings, and sanitized status codes. Prompts, project fields, tool arguments/results, files, and model text are prohibited from logs.
- Disable and rollback: omit or set `ASSISTANT_ENABLED=false` in a replacement application revision. No fallback service, public OpenAI endpoint, API key, queued assistant work, or conversation store exists.
- Accountable owner: repository owner acting as application and service owner under `docs/FOUNDRY-ASSISTANT-APPROVAL-PROPOSAL.md`.
- Effective review date: 2026-08-12. Re-review is required for any model, version, API, SKU, region, processing boundary, retention term, eligible data, dependency, or network change.