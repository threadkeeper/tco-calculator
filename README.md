# Azure SQL TCO

Azure SQL TCO is an operational calculator for comparing estimated annual AWS EC2, AWS RDS, or on-premises SQL Server costs with Azure SQL Managed Instance. Results are planning estimates, not quotes, licensing advice, or migration-readiness assessments.

## Status

[![Deploy Azure foundation](https://aka.ms/deploytoazurebutton)](https://github.com/threadkeeper/tco-calculator/actions/workflows/deploy.yml)

The button opens a guarded manual workflow for the approved development foundation. Run `preview` first, review its Azure `what-if`, then choose `deploy` with that preview run ID and the requested resource-group confirmation. It provisions networking, monitoring, ACR, Cosmos DB, private connectivity, and the Container Apps managed environment. It does **not** build or push an image, deploy application code, create a Container App, configure Entra application authentication, or grant runtime identity roles. Those belong to the separate application CI/CD workflow described in [infra/README.md](infra/README.md).

The manual **Deploy Azure application** workflow is one guarded development deployment run. A deploy request authorizes its required exact-commit CI check, locked dependency and application-image builds, application-only deletion-free Bicep `what-if`, deployment of the resolved immutable digest, and identity, health, readiness, persistence, and version verification. The workflow requires the exact resource-group confirmation and rejects a commit superseded by newer `main` work before Azure mutation. Foundation changes and any future test or production deployment remain separately authorized operations.

The current source implements the pure Rust calculation and target-selection workflows, strict OpenAPI request/response contract, owner-scoped project APIs, immutable pricing snapshots, transport-neutral AWS/Azure price normalization, local frozen-price resolution, in-memory persistence, request quotas, security headers, and the complete Svelte project/resource/results workflow.

Local mode is intentionally self-contained: it uses synthetic/frozen public-price data and in-memory persistence. It does not call live pricing providers or Cosmos DB. Non-local readiness fails closed until the production HTTPS and Cosmos adapters pass the review in [docs/PRODUCTION-ADAPTER-READINESS.md](docs/PRODUCTION-ADAPTER-READINESS.md).

JavaScript dependency operations are blocked on the current Microsoft-managed workstation, and its Rust toolchain has no MSVC `link.exe`. Frontend restore/build/tests, Rust compile/tests/coverage, container E2E, and measured 80% coverage remain unverified here. Run the controlled handoff in [docs/NON-IT-BUILD-HANDOFF.md](docs/NON-IT-BUILD-HANDOFF.md); do not treat source diagnostics as passing executable tests.

## Architecture

- One Rust process built with Axum and Tokio serves `/api/v1`, health endpoints, and static web assets.
- Financial and target-selection behavior belongs only in the pure Rust calculation layer.
- The Svelte 5 frontend is a static same-origin client and never owns rates, totals, formulas, explanations, owner IDs, or revisions.
- Guest drafts remain in browser IndexedDB. Authenticated projects are owner-scoped by both Entra tenant ID and object ID.
- Local authenticated projects use the in-memory repository. The approved production design uses Cosmos DB and the Container App system-assigned managed identity, but that adapter is not implemented yet.
- One OCI image runs as non-root UID `10001` in one Azure Container App.

## Repository Layout

- `app/catalogs/`: reviewed SQL MI capability catalogs.
- `openapi/`: API contract used to generate frontend types.
- `rust/`: backend, domain, pricing, calculation, and persistence code.
- `web/`: SvelteKit static frontend source.
- `infra/`: modular Bicep for the development environment.
- `docs/`: controlled build handoff and production-adapter readiness records.
- `research/`: legacy workbook-generation tools and ignored frozen source data.

## Prerequisites

- Stable Rust 1.97.1 with `rustfmt` and Clippy.
- A C/C++ linker. On Windows, install the official Visual Studio Build Tools C++ workload; VS Code alone does not include `link.exe`.
- Node.js 24 and pnpm 11.20.0 on an approved build machine for frontend work. The first reviewed `web/pnpm-lock.yaml` is still required.
- Azure CLI with Bicep for infrastructure validation.
- Docker BuildKit for image validation.

Do not use `npm` or `npx` in this repository. Follow [.github/copilot-instructions.md](.github/copilot-instructions.md) for workstation and package-source restrictions.

## Local Backend

```powershell
cargo run --locked --manifest-path rust/Cargo.toml
```

Until the Svelte app is built, this serves only the embedded diagnostic fallback from `rust/static`. On the approved build machine, generate and validate `web/build`, then point the Rust process at it:

```powershell
$env:WEB_ASSET_DIR = 'web/build'
cargo run --locked --manifest-path rust/Cargo.toml
```

The service listens on `http://localhost:8080` by default:

- `GET /healthz`: cheap process liveness.
- `GET /readyz`: dependency readiness.
- `GET /version`: application, formula, and schema versions.
- `GET /api/v1/session`: current guest or authenticated session summary.

The default guest session can calculate but cannot save. Optional local mock identity enables the authenticated CRUD flow only under `APP_ENV=local`:

```powershell
$env:APP_ENV = 'local'
$env:LOCAL_AUTH_TENANT_ID = '<synthetic-tenant-uuid>'
$env:LOCAL_AUTH_OWNER_ID = '<synthetic-owner-uuid>'
$env:LOCAL_AUTH_DISPLAY_NAME = 'Local developer'
```

Optional local values belong in ignored `.env` files. Local mock identity is permitted only with `APP_ENV=local`; startup must fail if local-auth settings appear in another environment.

## Quality Gates

Backend:

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-features
cargo audit
cargo deny check
```

Frontend, on the approved non-managed build machine:

```powershell
pnpm --dir web install --frozen-lockfile
pnpm --dir web run lint
pnpm --dir web run check
pnpm --dir web run test
pnpm --dir web run build
```

Generate committed frontend API types with `pnpm --dir web run api:generate` before the frontend gates. Measure the 80% target only with an exact coverage tool approved for the build environment, and retain its report with the build evidence.

Infrastructure:

```powershell
az bicep build --file infra/main.bicep
az deployment group what-if --resource-group <resource-group> --parameters infra/parameters/dev.bicepparam
```

No Azure deployment is performed by local validation. Azure `what-if` also requires an authenticated, reviewed target and is not part of an offline source check.

## Security and Privacy

Project settings, workload names, infrastructure details, and commercial assumptions are confidential business data. The service must not log workload names, server identifiers, raw identity headers, credentials, or complete project payloads. Browser, API, provider, identity-header, file, and environment input is untrusted.

Guests can display the app-specific privacy notice without accepting it. After Entra sign-in, the current notice version must be accepted before the rest of the application is available; the separate Azure SQL contact choice is optional and off by default. Acceptance time/version, contact choice, optional Entra display name, and email only when contact is enabled are stored in one fixed-ID document in the server-derived owner partition. There is no application contact-list export endpoint.

The in-app wording is an internal-pilot notice, not a claim of Microsoft-wide approval or a replacement for the Microsoft Privacy Statement. Privacy and Legal must approve the controller identity, final wording, privacy contact, retention terms, and contact-use process before external or production use.

See [THIRD-PARTY-DATA-EGRESS.md](THIRD-PARTY-DATA-EGRESS.md) for external data flows and [research/Azure Specification.md](research/Azure%20Specification.md) for the normative product and security requirements.
