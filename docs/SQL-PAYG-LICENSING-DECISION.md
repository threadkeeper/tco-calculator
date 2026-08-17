# SQL Pay As You Go Licensing Decision

Status: Approved implementation baseline for the SQL Pay As You Go project type
Research reviewed: 2026-08-14
Currency: USD
Product modeled: Azure Arc-enabled SQL Server pay-as-you-go licensing

## Decision

The calculator compares the customer's annual SQL Server Software Assurance or renewal spend with an Azure Arc-enabled SQL Server PAYG license run rate at a selected utilization and applied discount. It answers two bounded questions:

> At the selected utilization and applied discount, what are the annual savings or overage compared with the entered SA or renewal spend, and what PAYG discount would reach breakeven?

This is a licensing run-rate comparison, not an Azure SQL Managed Instance sizing project and not a license-entitlement determination. It has five customer inputs:

1. SQL Server Enterprise licensed cores.
2. SQL Server Standard licensed cores.
3. Annual Software Assurance or renewal spend in USD.
4. PAYG utilization, entered as hours per month or hours per year and normalized to one annual-hours value.
5. Applied PAYG discount.

No resource inventory, AWS region, Azure migration region, price snapshot, customer agreement identifier, or client-calculated total participates in this project type.

## Calculation Contract

The server uses these reviewed retail rates:

| Edition | PAYG rate |
| --- | ---: |
| Enterprise | $0.375 per core-hour |
| Standard | $0.100 per core-hour |

The comparison defaults to 8,760 hours per year and permits 0 through 8,784 annual hours. Let $E$ be Enterprise cores, $S$ Standard cores, $A$ annual SA spend, $H$ annual hours, and $C$ the applied PAYG discount:

$$
P = H \times (0.375E + 0.100S)
$$

$$
D =
\begin{cases}
0, & P = 0 \\
\max\left(0, 1 - \frac{A}{P}\right), & P > 0
\end{cases}
$$

$$
N = P(1-C)
$$

$$
V = A-N
$$

$P$ is gross annual PAYG, $D$ is the required breakeven discount, $N$ is net annual PAYG after the applied discount, and $V$ is signed annual savings. Positive $V$ is savings and negative $V$ is overage. At least one licensed core is required, annual SA spend cannot be negative, annual hours must be between 0 and 8,784, and both discounts must be between 0 and 1. Money and rates use decimal arithmetic on the server. The frontend displays the returned financial result and does not reproduce the formula.

Perpetual license acquisition cost is excluded. It is normally a sunk cost in this annual run-rate decision and must not be silently annualized as avoidable spend. A separate approved model would be required to compare a future buyout, new perpetual purchase, or residual asset value.

## Licensing Levers Outside the Formula

| Lever | Required review | Calculator treatment |
| --- | --- | --- |
| EA true-up | Reconcile deployed quantities and required year-one, year-two, and final true-up or zero-usage submissions before renewal. | Disclosed; not inferred or priced. |
| EAS anniversary and buyout | Confirm all annual orders, enrollment end date, buyout eligibility, order timing, and quoted buyout cost. Microsoft partner guidance says an EAS buyout order is submitted before expiration and uses the enrollment end date. | Disclosed; a quoted buyout is not part of annual SA unless the customer intentionally includes it. |
| Perpetual rights | Verify which licenses remain perpetual after agreement expiration and which are subscription rights. Download access or product keys do not prove entitlement. | Excluded from arithmetic; requires agreement evidence. |
| Software Assurance | Verify active coverage, renewal date, covered cores, edition, version rights, failover rights, and virtualization benefits. | Customer enters the annual avoidable SA or renewal amount. |
| MCA-E | MCA-E licenses are invoiced and administered separately from classic volume licensing records. | No entitlement is inferred from agreement family. |
| Azure Hybrid Benefit | Evaluate retained eligible licenses with active SA as an alternative when moving workloads to eligible Azure services. Confirm eligibility and allocation before claiming savings. | Alternative decision path; not subtracted from Azure Arc PAYG. |
| Core and OSE scope | Azure Arc meters virtual or physical cores available to each operating system environment. The minimum is four cores per OSE; multiple instances share an OSE meter and the highest installed edition determines it. | User must enter already-validated licensable cores. |
| Standard edition limit | Microsoft documents a maximum of 24 virtual or physical cores for Standard edition in this licensing path. | Disclosed; estate design must be reviewed before aggregation. |
| Runtime utilization | PAYG applies for any part of an hour in which SQL Server is running and the connected machine is online. Stopped or intermittent estates can differ materially from the 8,760-hour default. | User enters monthly or annual hours; the calculator persists and submits one normalized annual-hours value. |
| Passive replicas | SA and PAYG can provide free passive HA/DR instances when all passivity conditions are met. Azure Arc automatic detection has documented technology, monitoring, testing, and Linux limitations. | User excludes only replicas confirmed eligible under current terms and technical conditions. |
| Connectivity | PAYG requires usage reporting to Microsoft. Built-in resilience tolerates intermittent disruption, but a machine disconnected for more than 30 consecutive days is no longer authorized under PAYG until it reconnects. | Operational prerequisite; no cost adjustment. |
| Outsourcing and hosting | Azure Arc connection and SQL license mobility or outsourcing rights depend on the applicable Product Terms and environment. | Must be confirmed before recommendation. |
| Taxes, currency, and negotiated price | Tax, exchange rates, reseller terms, and negotiated discounts vary by customer and agreement. | USD, tax-exclusive estimate; applies the user-entered discount and separately reports the breakeven discount without asserting either is available. |

## Interpretation

- `annual_savings > 0`: net PAYG at the entered utilization and applied discount is below annual SA by the returned amount.
- `annual_savings = 0`: net PAYG matches annual SA.
- `annual_savings < 0`: net PAYG exceeds annual SA; the absolute amount is shown as annual overage.
- `required_payg_discount = 0`: gross PAYG at the entered utilization is no more expensive than the entered annual SA baseline.
- `0 < required_payg_discount < 1`: PAYG must be discounted by the returned percentage to match annual SA.
- `required_payg_discount = 1`: the annual SA baseline is zero, so only a 100% PAYG discount reaches breakeven.

The result is an estimate, not a quote, licensing statement, legal opinion, or promise that a discount is available. Customer licensing records, current Product Terms, and written commercial quotes control.

## Official Sources

- [Manage licensing and billing of SQL Server enabled by Azure Arc](https://learn.microsoft.com/sql/sql-server/azure-arc/manage-license-billing?view=sql-server-ver17), reviewed 2026-08-14. Controls PAYG license types, hourly usage, OSE/core metering, edition selection, passive-instance behavior, and connectivity.
- [Frequently asked questions: SQL Server enabled by Azure Arc](https://learn.microsoft.com/sql/sql-server/azure-arc/faq?view=sql-server-ver17), reviewed 2026-08-14. Confirms the four-core minimum and full accessible-core basis.
- [SQL Server enabled by Azure Arc overview](https://learn.microsoft.com/sql/sql-server/azure-arc/overview?view=sql-server-ver17), reviewed 2026-08-14. Compares license-only, SA/subscription, and PAYG capabilities and use rights.
- [Coverage periods and usage dates in Microsoft License and Software Assurance](https://learn.microsoft.com/volume-licensing-central/learning/contracting/coverage-periods-and-usage-dates), reviewed 2026-08-14. Covers EA true-up, EAS anniversary orders, renewal prerequisites, and buyout ordering scenarios. This is partner operational guidance; the customer's executed agreement controls.
- [Manage volume licensing agreements](https://learn.microsoft.com/microsoft-365/commerce/licenses/manage-volume-licensing?view=o365-worldwide), reviewed 2026-08-14. Distinguishes classic volume licensing administration from MCA-E.
- [SQL Server Product Terms](https://www.microsoft.com/licensing/terms/productoffering/SQLServer/EAEAS), current terms must be checked at decision time.
- [SQL Server licensing resources and documents](https://www.microsoft.com/licensing/docs/view/SQL-Server), current licensing guide must be checked at decision time.
- [Azure Retail Prices API](https://prices.azure.com/api/retail/prices), rate provenance verified 2026-08-07. Public retail rates are estimates and exclude customer-specific commercial terms.
