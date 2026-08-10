# Non-IT Build Handoff

Use this checklist on the approved non-managed development machine. It does not authorize bypassing controls on a Microsoft-managed workstation.

## 1. Verify Tools

Confirm stable Rust, a working native linker, Node.js 24, pnpm 11.20.0, Azure CLI with Bicep, and Docker BuildKit. Install tools only through an approved package source and verify publisher, version, license, installer URL, and hash before installation.

## 2. Create the Frontend Lockfile

The managed workstation could not use JavaScript package operations, so the repository does not contain `web/pnpm-lock.yaml`. Do not reuse an npm lockfile or an ignored `node_modules` tree.

```powershell
pnpm --dir web install
pnpm --dir web list --depth Infinity
pnpm --dir web audit --audit-level high
```

Review the resolved graph, licenses, lifecycle scripts, and audit output before committing `web/pnpm-lock.yaml`. Do not approve unexpected native code, telemetry, registry changes, or lifecycle scripts.

After the first reviewed lockfile exists, use only:

```powershell
pnpm --dir web install --frozen-lockfile
```

## 3. Run Source Gates

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-features
pnpm --dir web run api:generate
pnpm --dir web run lint
pnpm --dir web run check
pnpm --dir web run test
pnpm --dir web run build
az bicep build --file infra/main.bicep
```

Review the generated OpenAPI TypeScript diff before continuing. Use an approved Rust coverage tool to measure the specification's 80% target only after its exact package/version and installation method are reviewed; until a report exists, record coverage as not measured.

The source includes transport-neutral AWS/Azure normalizers but no live HTTPS transport or Cosmos adapter. Complete the dependency review and acceptance gates in [PRODUCTION-ADAPTER-READINESS.md](PRODUCTION-ADAPTER-READINESS.md) before enabling non-local readiness.

Record any failure exactly. Do not disable TLS, lint rules, tests, authentication, endpoint protection, or certificate validation to make a gate pass.

## 4. Build the Image

The Dockerfile requires three approved, digest-pinned image arguments:

- `WEB_BUILD_IMAGE`: Node.js 24 image containing pnpm 11.20.0.
- `RUST_BUILD_IMAGE`: stable Rust 1.97.1 build image.
- `RUNTIME_IMAGE`: minimal Debian slim runtime image.

Do not use mutable tags for a release build. Review image publisher, license, vulnerability results, and digest before supplying each argument.

```powershell
docker build --build-arg WEB_BUILD_IMAGE=<approved-image@sha256:digest> --build-arg RUST_BUILD_IMAGE=<approved-image@sha256:digest> --build-arg RUNTIME_IMAGE=<approved-image@sha256:digest> --tag azure-sql-tco:local .
```

Inspect the final image to confirm it contains no Node.js runtime, package manager, compiler, source tree, credentials, or build secrets and runs as UID `10001`.

## 5. Do Not Deploy Yet

Build and local smoke testing do not authorize Azure deployment. Before a manual development deployment, review `az deployment group what-if`, confirm the immutable ACR image reference, validate Entra multi-tenant configuration and the versioned Key Vault reference, and obtain the repository owner's explicit authorization.