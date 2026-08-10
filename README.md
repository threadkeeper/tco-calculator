# Azure SQL TCO

Azure SQL TCO is an operational calculator for comparing estimated annual AWS EC2, AWS RDS, or on-premises SQL Server costs with Azure SQL Managed Instance. Results are planning estimates, not quotes, licensing advice, or migration-readiness assessments.

## Status

Pass 1 establishes the executable Rust service boundary, OpenAPI contract, Svelte source structure, secure Azure development topology, and container layout. Pricing adapters, calculations, persistence, authentication, and resource workflows are intentionally explicit stubs until later passes.

JavaScript dependency operations are blocked on the current Microsoft-managed workstation. The Svelte dependency restore and executable frontend checks must run on the approved non-managed build machine described in [docs/NON-IT-BUILD-HANDOFF.md](docs/NON-IT-BUILD-HANDOFF.md).

## Architecture

- One Rust process built with Axum and Tokio serves `/api/v1`, health endpoints, and static web assets.
- Financial and target-selection behavior belongs only in the pure Rust calculation layer.
- The Svelte 5 frontend is a static same-origin client and never owns rates, totals, formulas, explanations, owner IDs, or revisions.
- Guest drafts remain in browser-local storage. Authenticated projects are owner-scoped by both Entra tenant ID and object ID.
- Azure Cosmos DB stores projects and normalized pricing snapshots. Production access uses only the Container App system-assigned managed identity.
- One OCI image runs as non-root UID `10001` in one Azure Container App.

## Repository Layout

- `app/catalogs/`: reviewed SQL MI capability catalogs.
- `openapi/`: API contract used to generate frontend types.
- `rust/`: backend, domain, pricing, calculation, and persistence code.
- `web/`: SvelteKit static frontend source.
- `infra/`: modular Bicep for the development environment.
- `research/`: legacy workbook-generation tools and ignored frozen source data.

## Prerequisites

- Stable Rust 1.97.1 with `rustfmt` and Clippy.
- A C/C++ linker. On Windows, install the official Visual Studio Build Tools C++ workload; VS Code alone does not include `link.exe`.
- Node.js 24 and pnpm 11.20.0 on an approved build machine for frontend work.
- Azure CLI with Bicep for infrastructure validation.
- Docker BuildKit for image validation.

Do not use `npm` or `npx` in this repository. Follow [.github/copilot-instructions.md](.github/copilot-instructions.md) for workstation and package-source restrictions.

## Local Backend

```powershell
cargo run --locked --manifest-path rust/Cargo.toml
```

The service listens on `http://localhost:8080` by default:

- `GET /healthz`: cheap process liveness.
- `GET /readyz`: dependency readiness.
- `GET /version`: application, formula, and schema versions.
- `GET /api/v1/session`: current guest or authenticated session summary.

Optional local values belong in ignored `.env` files. Local mock identity is permitted only with `APP_ENV=local`; startup must fail if local-auth settings appear in another environment.

## Quality Gates

Backend:

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-features
```

Frontend, on the approved non-managed build machine:

```powershell
pnpm --dir web install --frozen-lockfile
pnpm --dir web run lint
pnpm --dir web run check
pnpm --dir web run test
pnpm --dir web run build
```

Infrastructure:

```powershell
az bicep build --file infra/main.bicep
az deployment group what-if --resource-group <resource-group> --parameters infra/parameters/dev.bicepparam
```

No Azure deployment is performed by local validation.

## Security and Privacy

Project settings, workload names, infrastructure details, and commercial assumptions are confidential business data. The service must not log workload names, server identifiers, raw identity headers, credentials, or complete project payloads. Browser, API, provider, identity-header, file, and environment input is untrusted.

See [THIRD-PARTY-DATA-EGRESS.md](THIRD-PARTY-DATA-EGRESS.md) for external data flows and [research/Azure Specification.md](research/Azure%20Specification.md) for the normative product and security requirements.
