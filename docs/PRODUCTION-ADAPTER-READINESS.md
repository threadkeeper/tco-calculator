# Production Adapter Readiness

Status date: 2026-08-10.

This record separates implemented source behavior from production dependencies that still require an exact dependency review and written approval. It is not a deployment approval.

## Implemented Boundary

- The Rust calculation engine, target selector, validation, and explanations are deterministic and do not perform network or persistence operations.
- Immutable content-addressed pricing snapshots reject invalid AWS rates, incomplete EBS dimensions, incomplete Azure eight-option matrices, and missing rate provenance URLs.
- Transport-neutral normalizers cover AWS EC2 calculator leaves, selected AWS RDS and EBS bulk-catalog dimensions, the Azure SQL calculator composition graph, and paged Azure Retail Price records.
- Local mode uses only the reviewed SQL MI capability catalog, a frozen public-price fixture, and owner-scoped in-memory project persistence.
- Non-local readiness fails closed with `503 Service Unavailable`; it does not claim Cosmos or provider availability.

## Live HTTPS Provider Adapter

No production HTTPS transport is implemented or approved. Rust's standard library does not provide the required HTTPS client, certificate validation, bounded redirects, decompression limits, timeouts, retry policy, or connection pooling.

Before adding a direct HTTP/TLS dependency, record and approve:

- Exact package, publisher, registry source, resolved version, license, maintenance status, and provenance.
- Complete transitive graph, enabled features, lifecycle or build behavior, and applicable vulnerability results.
- Why existing approved dependencies and the standard library are insufficient.
- Certificate-validation behavior, proxy behavior, redirect policy, response-size limits, connect/read/overall timeouts, decompression limits, and cancellation behavior.
- The descriptive product/version `User-Agent`, maximum three-attempt retry policy, `Retry-After` handling, and bounded jitter implementation.
- Allowed destinations from [THIRD-PARTY-DATA-EGRESS.md](../THIRD-PARTY-DATA-EGRESS.md), rollback plan, and approving owner.

The adapter must stream/filter AWS regional bulk catalogs, follow Azure Retail Prices `NextPageLink`, request only selected SKU branches, and map terminal outcomes to `not_found`, `unsupported`, `temporarily_unavailable`, or `schema_changed`. It must never turn an upstream failure into a zero price.

## Cosmos Persistence Adapter

No production Cosmos implementation or Azure SDK dependency is present. Before adding official Azure SDK crates, record the exact crates, versions, features, licenses, transitive graph, vulnerability results, and Security approval for identity-sensitive behavior.

The implementation must use the Container App system-assigned managed identity and Azure RBAC. It must not accept account keys, connection strings, service-principal secrets, or client-supplied owner IDs. Required behavior includes:

- Partition every project operation by the server-derived `tid` plus `oid` owner ID.
- Preserve Cosmos ETags for optimistic concurrency and enforce the 1.8 MB project limit before writes.
- Store immutable normalized snapshots in `pricing-cache`; saved projects reference snapshot IDs and revisions embed the exact rates used.
- Treat snapshots as fresh through 24 hours, stale but usable through 7 days, and expired afterward for new calculations.
- Implement 150-second conditional refresh leases and bounded waiter takeover for cross-replica coalescing.
- Keep refresh counters and lease updates atomic and bounded; lease failure must not hide a valid stale snapshot.
- Perform the lightweight Cosmos readiness operation without downloading provider catalogs.

## Production Acceptance Gates

Production readiness remains blocked until all of these are evidenced:

- Written dependency approvals and committed lockfile updates for the selected HTTPS and Cosmos clients.
- Focused adapter tests using frozen synthetic/public fixtures, including schema drift, timeout, retry, response limit, stale-cache, lease takeover, tenant isolation, object authorization, and ETag conflicts.
- Rust format, compile, Clippy with warnings denied, tests, at least 80% measured coverage, audit, and license gates.
- Generated OpenAPI TypeScript types plus frontend lint, typecheck, unit tests, build, and Playwright workflows on the approved build machine.
- Digest-pinned multi-stage image build, non-root/runtime-content inspection, vulnerability/configuration/secret scans, and local same-origin browser smoke tests.
- Reviewed Bicep build and Azure `what-if`; deployment still requires separate explicit authorization.

Until then, use `APP_ENV=local` only. Do not override the non-local readiness failure.