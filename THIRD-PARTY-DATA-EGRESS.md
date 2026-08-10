# Third-Party Data Egress

Azure SQL TCO sends only public catalog selectors required to resolve prices. It never sends project names, workload names, quantities, customer inventories, totals, tenant identifiers, subscription identifiers, or commercial agreements to pricing providers.

## AWS Public Pricing

- Destinations: AWS EC2 Calculator metered-unit maps and AWS Price List Bulk API endpoints listed in the specification.
- Data sent: currency, AWS region, service, SKU, operating system, tenancy, SQL edition, deployment, commercial term, and storage meter filters.
- Data received: public catalog metadata and public USD price dimensions.
- Credentials: none.
- Retention: normalized snapshots are cached for up to 30 days; saved revisions embed only the exact resolved rates and provenance used.

## Azure Public Pricing

- Destinations: Azure Retail Prices API and the Azure SQL calculator composition endpoint listed in the specification.
- Data sent: currency, Azure ARM region, SQL Managed Instance service, tier, hardware, vCores, and purchase-option filters.
- Data received: public catalog metadata and public USD price dimensions.
- Credentials: none.
- Retention: same as AWS normalized pricing snapshots.

The Azure calculator endpoint is public but not a stable contract. Schema drift is a provider error and must fall back only to a still-valid cached snapshot.

## Microsoft Entra ID

Azure Container Apps built-in authentication performs sign-in and token validation. The browser uses platform login and logout routes. The Rust application receives only platform-validated principal claims at the protected ingress boundary and derives ownership from both `tid` and `oid`. It does not store or forward access tokens.

## Operational Telemetry

Structured application logs go to the environment's Azure Log Analytics workspace. Logs contain request IDs, route templates, status, duration, auth mode, provider/cache outcomes, formula version, and aggregate mapping counts. They must not contain workload names, raw identity headers, tokens, credentials, or full project payloads. No third-party analytics or behavioral tracking is used.