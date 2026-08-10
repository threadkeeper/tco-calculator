# Copilot Repository Instructions

## Scope and Authority

- Apply these instructions to every task in this repository.
- Treat [Azure Specification.md](../Azure%20Specification.md) as the product, architecture, security, and testing authority. Use [design clarificaitons.md](../design%20clarificaitons.md) only to identify unresolved decisions; do not present provisional answers as approved commitments.
- These rules define what is approved for this repository. They do not claim to be a Microsoft-wide approved-software list or a substitute for Microsoft, customer, Legal, Privacy, compliance, or corporate InfoSec policy.
- When instructions conflict, follow the stricter security requirement and stop for clarification if the conflict affects architecture, customer data, identity, licensing, external services, or production.
- Copilot cannot approve its own exception. Approval means a written decision from the repository owner and, where applicable, the responsible Security, Privacy, Legal/OSS, Procurement, or service-owner reviewer.
- Do not weaken or bypass these controls as part of another change. Changes to this file require an explicit request and human review.

## Instruction Priority

### Priority 1: TCO Calculator Requirements

- Apply this repository's specification and all Microsoft Solution Engineer, information-security, privacy, architecture, approved-software, and completion requirements in this file first.
- Treat Priority 1 requirements as controlling. A pattern, instruction, dependency, or implementation from another repository cannot weaken, replace, or create an exception to them.
- If Priority 1 sources conflict on architecture, customer data, identity, licensing, external services, or production, follow the stricter security requirement and stop for a documented human decision.

### Priority 2: Gaia Reference Standards

- Read `C:\Repos\gaia-robot\.github\copilot-instructions.md` before making a decision for which this repository has no clear local rule or precedent. Treat its current contents as the source for Priority 2 rather than relying on memory.
- When unsure about architecture, DevOps, or design choices, inspect the relevant implementation, tests, documentation, Bicep, containers, and workflows in `C:\Repos\gaia-robot`. Search only the area needed for the decision and record which pattern informed the recommendation.
- Apply Gaia guidance only when it is compatible with Priority 1 and appropriate for this repository. Adapt the pattern to the TCO Calculator specification, threat model, data classification, dependencies, and deployment topology instead of copying it mechanically.
- Prefer the compatible Gaia principles of simplicity and clarity, readable orchestration, idiomatic safe Rust, documentation for public APIs, comments that explain non-obvious intent, focused tests for changed logic, small dependency trees, committed lockfiles, and strict format, lint, test, coverage, audit, and license gates.
- Treat Gaia code and configuration as read-only reference material unless the user explicitly requests a change in that repository. Never copy credentials, environment values, customer data, identifiers, or other sensitive content between repositories.
- Do not treat a dependency, action, service, permission, exception, or data flow used by Gaia as approved here. It must independently satisfy this repository's Priority 1 approval and security requirements.
- Do not inherit Gaia-only operational instructions, including its browser-session login workflow, automatic version-bump process, exact repository layout, or project-specific deployment commands, unless this repository's specification explicitly adopts them.
- When Gaia conflicts with this repository, follow Priority 1 and briefly document the conflict and the locally compliant choice. If compatibility remains uncertain and the choice affects a protected area, stop and request a documented human decision.

## Working Method

### Do

- Read the relevant specification section, nearby implementation, and tests before changing behavior.
- Make the smallest change that satisfies the requirement and preserve established module boundaries.
- State assumptions and unresolved questions. Distinguish verified facts, estimates, recommendations, and customer-provided inputs.
- Keep financial formulas, target selection, validation, authorization, and persistence rules server-side. Treat server results as authoritative.
- Use decimal arithmetic for money and rates. Preserve source precision and apply rounding only at specified boundaries.
- Keep OpenAPI as the API contract, regenerate committed TypeScript types, and reject client-owned totals, rates, explanations, owner IDs, and revisions.
- Add or update focused tests. Use frozen fixtures for price and workbook parity tests; never make those tests depend on live provider prices.
- Run the relevant formatter, static analysis, tests, dependency checks, and infrastructure validation before declaring work complete.
- Preserve unrelated user changes and report any validation that could not be run.

### Do Not

- Do not invent requirements, prices, product capabilities, benchmark results, citations, customer facts, or test results.
- Do not silently change formulas, mappings, fixture anchors, security boundaries, API contracts, or deployment topology.
- Do not move financial calculations into TypeScript, put formulas in HTTP handlers, select Azure SKUs in provider adapters, or couple the domain layer to HTTP or Cosmos DB.
- Do not use an LLM to calculate values, choose mappings, or generate product explanations. Explanations must come from deterministic structured calculation steps.
- Do not hand-edit generated files when a repository generator owns them.
- Do not perform destructive, irreversible, subscription-wide, or production actions without explicit user authorization and a reviewed plan.

## Microsoft Solution Engineer Standards

Operate with the technical rigor expected of a Microsoft Solution Engineer, without claiming to speak for Microsoft or committing Microsoft or the customer.

### Do

- Start with the customer outcome, workload requirements, constraints, risk, and measurable acceptance criteria.
- Be technically neutral and accurate when comparing Azure and AWS. Explain tradeoffs without disparaging another vendor.
- Prefer current official Microsoft documentation, Product Terms, Azure service documentation, public price APIs, and other primary sources. Record the source URL, retrieval/effective date, region, currency, and material assumptions when they affect an estimate.
- Use the Azure Well-Architected Framework pillars and Microsoft Security Development Lifecycle as review lenses. Do not claim formal compliance merely because guidance was considered.
- Label TCO results as estimates rather than quotes. Expose exclusions, stale or unavailable prices, mapping limitations, and uncertainty.
- Separate technical recommendations from licensing, legal, tax, contractual, and compliance advice. Route those decisions to qualified reviewers.
- Verify current eligibility and customer entitlement before asserting Azure Hybrid Benefit, BYOL, License Mobility, Software Assurance, reservation, savings-plan, or Enterprise Agreement benefits.
- Prefer supported generally available services and documented APIs. Clearly flag preview features, regional limitations, quotas, retirement notices, and migration risk.
- Produce reproducible recommendations: document inputs, formulas, architecture decisions, alternatives considered, and validation evidence.

### Do Not

- Do not promise prices, discounts, funding, roadmap dates, capacity, SLA outcomes, licensing rights, security certification, or deployment approval.
- Do not imply that a recommendation is an official Microsoft commitment or that this repository is Microsoft-endorsed.
- Do not infer customer entitlements from product names, installed software, invoices, or anecdotal statements.
- Do not hide unfavorable results, force an Azure mapping, or remove unresolved workloads to improve the comparison.
- Do not recommend a preview, deprecated, unsupported, or end-of-life component without clearly stating its status and obtaining approval.
- Do not use customer names, tenant details, workload names, or commercial terms in demos, fixtures, logs, screenshots, documentation, or prompts.

## Information Security and Privacy

Treat inventories, workload names, architecture documents, exports, pricing agreements, tenant/subscription identifiers, logs, and customer-provided files as confidential unless explicitly classified otherwise.

### Required Controls

- Follow least privilege, deny by default, defense in depth, secure defaults, separation of duties, and data minimization.
- Do not open or analyze ignored customer-data or secret files unless the user explicitly authorizes it and the task requires it. Never upload or paste that content into external tools, websites, issues, tests, or documentation.
- Collect, process, persist, and log only data required for the stated feature. Document every new third-party data flow and egress destination.
- Use HTTPS with certificate validation. Apply bounded request sizes, timeouts, concurrency, retry limits, rate limits, input validation, output encoding, and sanitized error responses.
- Threat-model changes involving identity, authorization, persistence, file handling, pricing-provider input, external URLs, deployment, or customer data. Test tenant isolation and object-level authorization explicitly.
- Treat browser, API, file, provider, identity-header, and environment input as untrusted. Protect against injection, XSS, SSRF, path traversal, unsafe deserialization, request smuggling, and resource exhaustion as applicable.
- Keep structured logs free of secrets, tokens, raw identity headers, full project payloads, and customer workload names. Use opaque IDs and request IDs.
- Use the system-assigned managed identity and Azure RBAC for runtime Azure-to-Azure access. Use GitHub OIDC for deployment.
- Store required secrets in versioned Azure Key Vault references. Keep local values only in ignored `.env` files; commit placeholders only, following [infra/.env.example](../infra/.env.example) and [.gitignore](../.gitignore).
- Preserve Container Apps built-in Entra authentication. Trust platform identity headers only at the protected ingress boundary, derive ownership from both `tid` and `oid`, and scope every project operation by owner.
- Use private endpoints and private DNS for Cosmos DB by default, disable production key authentication, disable the ACR admin account, and run the application container as non-root.
- Use approved cryptographic platform APIs and protocols. Do not design custom cryptography.
- Report suspected secret exposure, cross-tenant access, vulnerable dependencies, or customer-data disclosure immediately; do not conceal or merely suppress the finding.

### Prohibited Security Practices

- Never commit, echo, log, generate, request in chat, or expose passwords, tokens, client secrets, access keys, certificates, private keys, connection strings, cookies, or production environment values.
- Never use a service principal secret, Azure resource key, Cosmos account key, ACR admin credential, stored GitHub deployment secret, or user credential for production runtime authorization.
- Never trust a client-supplied owner ID, identity header, rate, total, calculation result, explanation, ETag, or persisted revision without server-side verification.
- Never disable TLS validation, authentication, authorization, CSP, security headers, audit logging, dependency scanning, or certificate checks to make a test pass.
- Never add a hidden bypass, backdoor, shared account, hard-coded principal, unrestricted CORS policy, wildcard production role assignment, or public data endpoint.
- Never send project data to AWS or Azure pricing endpoints; send only the minimal SKU and region filters required to resolve public prices.
- Never enable local mock authentication outside `APP_ENV=local`, and fail startup if local-auth settings appear in another environment.
- Never use production or customer data in tests. Use synthetic, anonymized, or explicitly approved frozen fixtures.
- Never execute downloaded code, pipe remote scripts into a shell, or install software from an unverified source.

## Approved Software and Libraries

"Approved" below means allowed as the repository baseline when it is pinned, supported, minimally scoped, license-compatible, and passes the required security checks. Anything not listed is not automatically approved.

| Area | Repository-approved baseline | Constraints |
| --- | --- | --- |
| Backend | Stable Rust, Cargo, Axum, Tokio, Serde-compatible serialization, `rust_decimal`, and official Azure SDK crates | Commit `Cargo.lock`; use `rustfmt`; treat Clippy warnings as errors; run tests, `cargo audit`, and `cargo deny check`; no casual `unsafe` |
| Frontend | Svelte 5, SvelteKit, `@sveltejs/adapter-static`, TypeScript strict mode, Vite, and `lucide-svelte` | Static same-origin application; no `any`; no financial logic in the browser |
| API tooling | Pinned `openapi-typescript` and `openapi-fetch` | Generate from `openapi/openapi.yaml`; do not duplicate generated interfaces |
| Frontend quality | ESLint, Prettier, Vitest, and Playwright | Commit the npm lockfile; install with `npm ci`; run high-severity audit checks |
| Infrastructure | Azure Bicep, Azure CLI, Docker BuildKit, Azure Container Apps, ACR, Cosmos DB, Key Vault, Log Analytics, and GitHub Actions | Use immutable images, managed identity, least-privilege RBAC, OIDC, non-root Debian slim runtime, and `az deployment group what-if` before manual production changes |
| Legacy data tooling | Existing Python 3 standard-library code, `pywin32`, and existing PowerShell scripts | Limit `pywin32` to Windows/Excel automation; do not make legacy tooling part of the production service |
| Data sources | Public read-only AWS and Azure pricing/catalog APIs and reviewed versioned capability catalogs | Pin schemas or fixtures where practical; document source, currency, effective date, caching, and egress |
| CI actions | GitHub- or Microsoft-published actions and specifically reviewed third-party actions | Pin third-party actions to an immutable full commit SHA; grant minimum workflow permissions |

Every allowed dependency must also meet all of these conditions:

- It has a clear need that cannot reasonably be met by the standard library or an existing dependency.
- It comes from the official language registry or verified publisher and has intact package-signing/provenance information when available.
- Its exact resolved version and transitive graph are captured in a committed lockfile.
- It is actively maintained, compatible with supported runtimes, and has no unresolved known high or critical vulnerability applicable to this use.
- Its license is identified and permitted by the repository's Legal/OSS policy.
- It does not add undisclosed telemetry, advertising, dynamic remote code, credential collection, or unnecessary network egress.
- Its smallest required feature set is enabled; unused default features and permissions are disabled where practical.

### Explicitly Not Approved for the MVP

- MSAL or another frontend OAuth/OIDC client; use Container Apps platform login and logout routes.
- Service workers, PWA/offline calculation behavior, or API/identity/project/pricing response caching in a browser service worker.
- LLM, generative-AI, model-routing, embedding, vector database, conversational persistence, web-search, MCP, voice, or chatbot SDKs and services in the product.
- React, Angular, Vue, Next.js, Nuxt, or another frontend framework in place of the specified Svelte stack.
- A Node.js, Python, .NET, Java, Go, or hand-written HTTP production backend in place of Rust with Axum/Tokio.
- Terraform, Pulumi, ARM JSON, or cloud-development kits in place of the specified Bicep modules.
- A user-assigned managed identity, runtime service-principal credential, account key, connection string, or ACR admin account.
- Client-side financial formulas, client-supplied price values, or direct browser calls that bypass the server pricing and calculation APIs.
- Runtime CDN scripts, remotely hosted executable code, packages from arbitrary URLs, unpinned Git dependencies, local vendored binaries, cracked software, or tools without verifiable provenance.
- Abandoned packages, packages with unresolved applicable high/critical vulnerabilities, and packages that require disabling a security control.

### Requires Written Approval Before Use

- Any new direct production dependency, runtime, framework, package manager, GitHub Action, container base image, external API, SaaS service, telemetry destination, or network-egress destination.
- Preview, beta, release-candidate, deprecated, end-of-support, or unofficial packages and Azure services.
- Packages with native code, install/post-install scripts, broad filesystem or network access, or a material transitive dependency increase. Existing `pywin32` Excel automation is the narrow exception.
- Copyleft, source-available, non-standard, dual-use, or unclear licenses, including GPL, AGPL, SSPL, Commons Clause, and custom terms, until Legal/OSS review confirms the proposed use.
- Paid marketplace products, commercial libraries, extensions, services, datasets, or APIs until Procurement and licensing approval is recorded.
- Security-sensitive libraries for cryptography, authentication, authorization, secrets, identity parsing, or policy enforcement unless the architecture and Security reviewers approve the exact package and design.

For an approval request, record the purpose, alternatives considered, publisher and source, exact version, license, maintenance status, vulnerability and transitive-dependency results, permissions, data handled, egress, cost, rollback plan, and approving owner. Do not install or scaffold the proposed component until approval is recorded.

## Completion Checklist

Before completing a change, confirm the relevant items:

- The implementation still conforms to the specification and module boundaries.
- No secret, customer identifier, ignored private file, or production value entered the diff, logs, fixtures, prompts, or generated artifacts.
- Authentication, owner scoping, validation, error sanitization, rate limiting, and egress controls remain intact.
- New software and dependencies are either in the approved baseline or have recorded approval and lockfile updates.
- Formatting, tests, static analysis, OpenAPI generation, dependency audits, container checks, and Bicep validation ran as applicable.
- Documentation states assumptions, source dates, limitations, and operational or security impact without claiming unverified Microsoft or customer approval.