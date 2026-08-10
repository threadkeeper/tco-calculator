# Production Adapter Readiness

Status date: 2026-08-10.

This record separates implemented source behavior from production dependencies that still require an exact dependency review and written approval. It is not a deployment approval.

## Implemented Boundary

- Non-local startup requires an HTTPS Azure Cosmos DB account endpoint and Azure application region. Local mode rejects those production settings and continues to use deterministic in-memory repositories and frozen price fixtures.
- `CosmosProjectRepository` uses only `ManagedIdentityCredential::new(None)` and the system-assigned identity. It resolves `tco/projects` during startup and applies bounded SDK retry and operation-latency policies.
- Every project point operation includes the server-derived `entra:{tid}:{oid}` partition key. Reads validate the stored owner and identifier, creates use `If-None-Match: *`, and updates use the Cosmos service ETag with `If-Match`.
- Project documents are rejected before writes at the 1.8 MB application limit. The Cosmos `_etag` is read as the public concurrency token and is omitted from request bodies.
- Readiness probes the selected project repository. Non-local readiness remains intentionally blocked until live price providers are configured.

## Live HTTPS Provider Adapter

The shared production HTTPS transport is implemented but is not yet wired to AWS or Azure provider orchestration. It permits only the four disclosed pricing hosts over HTTPS, refuses redirects and environment proxies, uses platform certificate validation, sends `azure-sql-tco/{version}`, applies a 10-second connect timeout and 30-second per-request timeout, streams responses through a configurable 1-256 MiB cap, and limits transient retries to three attempts within a 120-second overall budget. HTTP 404, unsupported 4xx/redirects, transient 408/429/5xx, and response/schema failures remain distinct provider outcomes.

Live provider resolution, snapshot cache fallback, single-flight refresh, Cosmos refresh leases, and distributed refresh quotas are not yet implemented. No production pricing egress occurs until those adapters are constructed in non-local application state.

### Dependency Approval and Pending Acceptance

Status: **Approved for resolution and implementation by the repository owner on 2026-08-10**, with the instruction to use best-effort production dependencies. This is not production acceptance: the resolved graph and identity-sensitive implementation still require the review gates below.

| Direct dependency | Publisher and source | License | Proposed features | Registry checksum |
| --- | --- | --- | --- | --- |
| `azure_identity = "=1.0.0"` | Microsoft OSS Releases; crates.io; Azure SDK for Rust | MIT | `default-features = false` | `32df96b356ca7c51d7590c4925cc36efc3947a5da4468e8e0b25c56ecbb3de5` |
| `azure_data_cosmos = "=0.37.1"` | Microsoft OSS Releases; crates.io; Azure SDK for Rust | MIT | `default-features = false`, `reqwest`, `rustls` | `8f7ec933cc053259153c422d5084a9a7518a8244b600d785afdbd6ddeddd0e5` |
| `reqwest = "=0.13.4"` | Sean McArthur; crates.io; seanmonstar/reqwest | MIT OR Apache-2.0 | `default-features = false`, `json`, `query`, `rustls` | `219c5811de6525e5416c7d5d53bb656d3afdbc6c5af81e0802bcfa42dbdc1c3` |
| `futures = "=0.3.33"` | Rust Project Developers; crates.io; rust-lang/futures-rs | MIT OR Apache-2.0 | `default-features = false`, `std` | `a88cf1f829d945f548cf8fec32c61b1f202b6d93b45848602fc02af4b12ad218` |

Purpose and alternatives:

- `azure_identity` and `azure_data_cosmos` provide the specification-required managed-identity and Cosmos data-plane clients. Hand-written identity, token, and Cosmos protocols are rejected.
- `reqwest` provides bounded asynchronous HTTPS for the public pricing APIs; the Rust standard library has no HTTPS client.
- `futures` provides `TryStreamExt` for the Cosmos SDK's paged query stream. Version `0.3.33` was already resolved transitively, so promoting it to direct use adds no package, native code, egress, or transitive dependency.

Supply-chain and operational review:

- Cargo resolved all four exact versions into `rust/Cargo.lock` with Rust 1.97-compatible transitive versions.
- Cosmos `key_auth` remains disabled. The Cosmos driver nevertheless compiles pure-Rust `azure_core/hmac_rust`, and its Rustls transport enables HTTP/2, streaming, and native `aws-lc-sys 0.44.0` transitively.
- Production code must construct `ManagedIdentityCredential` directly for the system-assigned identity. It must not select a user-assigned identity or fall back to developer tools, environment service-principal credentials, account keys, or connection strings.
- Native AWS-LC, vulnerability, provenance, and license review remain production acceptance gates.

Required approval record:

- Repository owner: approved the exact Azure and Reqwest proposal on 2026-08-10 and directed autonomous best-effort dependency decisions. That direction covers promoting already-resolved `futures 0.3.33` only for Cosmos stream consumption.
- Security review of identity behavior and native/transitive cryptography: pending.
- Legal/OSS review if required by repository policy after resolved-graph inspection: pending.
- Condition: do not accept or deploy the graph if an applicable security, license, provenance, or native-code gate fails.

Before accepting the live HTTP implementation for production, verify:

- The recorded package, publisher, registry source, resolved version, license, maintenance status, and provenance.
- Complete transitive graph, enabled features, lifecycle or build behavior, and applicable vulnerability results.
- Why existing approved dependencies and the standard library are insufficient.
- Certificate-validation behavior, proxy behavior, redirect policy, response-size limits, connect/read/overall timeouts, decompression limits, and cancellation behavior.
- The descriptive product/version `User-Agent`, maximum three-attempt retry policy, `Retry-After` handling, and bounded jitter implementation.
- Allowed destinations from [THIRD-PARTY-DATA-EGRESS.md](../THIRD-PARTY-DATA-EGRESS.md), rollback plan, and approving owner.
The implementation uses the Container App system-assigned managed identity and Azure RBAC. It does not accept account keys, connection strings, service-principal secrets, or client-supplied owner IDs. Remaining production behavior includes:

- Store immutable normalized snapshots in `pricing-cache`; saved projects reference snapshot IDs and revisions embed the exact rates used.

Production readiness remains blocked until all of these are evidenced:

- Security, native-code, vulnerability, provenance, and license acceptance for the committed HTTPS and Cosmos dependency graph.
- Focused adapter tests using frozen synthetic/public fixtures, including schema drift, timeout, retry, response limit, stale-cache, lease takeover, tenant isolation, object authorization, and ETag conflicts.
- Rust format, compile, Clippy with warnings denied, tests, at least 80% measured coverage, audit, and license gates.
- Generated OpenAPI TypeScript types plus frontend lint, typecheck, unit tests, build, and Playwright workflows on the approved build machine.

## Deployment Operator Decisions

Azure mutation is also blocked independently of dependency approval:

- South Africa North (`southafricanorth`) was approved by the repository owner on 2026-08-10. Service availability and the Azure `what-if` still require validation before deployment.
- Container Apps built-in Entra authentication requires an approved application client ID and a versioned Key Vault secret URI. Record only those non-secret identifiers; never place the secret value in source, chat, logs, command output, or deployment parameters.
- A reviewed immutable image digest must exist before deployment. A mutable tag or placeholder image is not acceptable.
- Run and review an Azure resource-group `what-if` only after the dependency, frontend, image, identity, region, and production-readiness gates pass. Deployment requires a separate explicit human authorization after that review.