# AWS EC2 Windows VM to Azure VM TCO Specification

Status: Adopted implementation specification  
Evidence reviewed: 2026-08-24  
Product authority: The repository owner adopted this workload on 2026-08-24. The controlling
product requirements now live in section 4.4 of
[`research/Azure Specification.md`](../research/Azure%20Specification.md); this document is the
supporting design record. Where the two disagree, the product specification governs. Nothing in
this document changes the existing `ec2` EC2 SQL workload.

## 1. Outcome

Add a distinct **AWS EC2 Virtual Machines** workload that compares Windows EC2 virtual machines
and their persistent EBS volumes with Windows Azure Virtual Machines and Azure managed disks.
The result is an estimate, not a quote or a migration-readiness guarantee.

The implementation must:

- use the project/resource discriminator `ec2_vm`; retain `ec2` for EC2 SQL;
- create one target Azure VM for each source EC2 VM resource;
- preserve every persistent EBS volume as a separate target managed disk;
- contain no SQL edition, SQL license basis, SQL data, SQL MI tier, or SQL MI purchase-option
  fields;
- reuse the reviewed EC2 Windows Shared On-Demand and EBS pricing behavior without changing the
  existing EC2 SQL acceptance rules;
- select targets deterministically on the server and favor the newest eligible current Azure VM
  generation in the requested region;
- treat CPU bursting, local instance storage, disk roles, regional restrictions, and price
  completeness as explicit constraints rather than inferred equivalence;
- expose sources, retrieval times, assumptions, exclusions, and deterministic selection reasons;
- keep formulas, rates, target selection, validation, and explanations server-side.

## 2. Scope

### 2.1 In scope

- Windows x86-64 EC2 instances using Shared tenancy and On-Demand pricing.
- Source instance types that are still represented in the reviewed AWS catalog, including
  previous-generation inventory when a current public price is available.
- Persistent EBS volumes already supported by the EBS provider, beginning with `gp3`.
- Windows Azure VM pay-as-you-go compute.
- Azure Premium SSD, Standard SSD when it satisfies the requirements, and Premium SSD v2 data
  disks.
- Manual entry, reviewed image-assisted drafts, calculation, deterministic target explanations,
  persistence, and workbook export through the existing application boundaries.
- A reviewed, versioned Azure VM capability/lifecycle catalog and managed-disk capability catalog.

### 2.2 Out of scope for the first release

- Linux, Arm, Dedicated Host, Dedicated Instance, Spot, Savings Plans, Reserved Instances, Azure
  Reservations, Azure Savings Plan, and negotiated discounts.
- Azure Hybrid Benefit, BYOL, License Mobility, and customer entitlement inference.
- Automatic rightsizing below the source vCPU or memory allocation.
- CPU benchmark equivalence, application performance guarantees, or automatic equivalence for a
  high-frequency source.
- Availability sets, availability zones, multi-VM high availability, disaster recovery, backup,
  snapshots, data transfer, support, monitoring, Defender, and operational labor.
- GPUs, accelerators, confidential-computing requirements, nested virtualization, or dedicated
  hosts.
- Converting ephemeral EC2 instance-store data into a persistent managed disk without an explicit
  customer decision.
- Direct browser calls to AWS or Azure pricing/catalog APIs.

## 3. Supplied Inventory Evidence

### 3.1 What can be stated from the supplied images

The reviewed images collectively represent:

- 15 EC2 Windows VM rows;
- 15 persistent `gp3` volumes;
- one 1,024 GiB `gp3` volume associated with each represented VM; and
- eight unique EC2 instance types.

The exact multiplicity of each instance type is not recoverable from the retained readable
artifacts. It must not be reconstructed from the total or invented. A frozen end-to-end fixture
requires the original images to be re-read or a machine-readable inventory with the per-SKU counts
confirmed by the user.

The images do not establish source region, target region, monthly running hours, disk role,
instance-store use, T3 utilization/credit suitability, desired availability design, or licensing
entitlement. Those values remain required inputs or explicit assumptions.

### 3.2 Normalized unique source shapes

The following is shape evidence, not a statement of per-SKU quantity or price.

| EC2 instance type | vCPU | Memory GiB | Source semantic class | Local instance store | Mapping implication |
| --- | ---: | ---: | --- | --- | --- |
| `t3.large` | 2 | 8 | Burstable general purpose | None | A burstable Azure target is eligible only after the burst policy in section 8.5 is satisfied. |
| `t3.xlarge` | 4 | 16 | Burstable general purpose | None | A burstable Azure target is eligible only after the burst policy in section 8.5 is satisfied. |
| `m6i.large` | 2 | 8 | General purpose | None | Prefer a current D-series lineage. |
| `r5.8xlarge` | 32 | 256 | Memory optimized | None | Prefer a current E-series lineage. |
| `r6id.8xlarge` | 32 | 256 | Memory optimized with local storage | `1 x 1900 GB` NVMe SSD | Local-storage use must be confirmed; if used, require a compatible local-disk target. |
| `r6id.12xlarge` | 48 | 384 | Memory optimized with local storage | `2 x 1425 GB` NVMe SSD | Local-storage use must be confirmed; if used, require compatible capacity and semantics. |
| `r7i.large` | 2 | 16 | Memory optimized | None | Prefer a current E-series lineage. |
| `z1d.2xlarge` | 8 | 64 | High-frequency memory optimized | `1 x 300 GB` NVMe SSD | Capacity alone is not performance equivalence; require review under section 8.6. |

Capacity groups represented by the unique source shapes are `2/8`, `4/16`, `2/16`, `8/64`,
`32/256`, and `48/384`, expressed as vCPU/GiB.

AWS documents T3 as a CPU-credit burstable family. AWS also documents the `r6id` and `z1d`
instance-store configurations shown above. Instance store is ephemeral and is not an EBS volume.

### 3.3 Research regions

`Sweden Central` was used only to test the target selection design. It must not become an
application default unless the user selects it. No source region was established by the images;
therefore exact AWS prices have not been claimed. `eu-west-1` may be used for a fixture only after
it is confirmed as the source region.

## 4. Domain and API Contract

### 4.1 Discriminator

Add `ec2_vm` to the project and resource discriminators. Do not rename or reinterpret `ec2`.
Existing persisted EC2 SQL projects and API payloads must deserialize and calculate exactly as
they do before this change.

The OpenAPI union must gain schemas equivalent to:

- `Ec2VmResource`
- `Ec2VmVolume`
- `Ec2VmRequirements`
- `AzureVmTarget`
- `AzureManagedDiskTarget`
- `Ec2VmCalculationResult`

Names may be adapted to established repository conventions, but the separate discriminator and
semantic boundary are required. Generated TypeScript types must be regenerated from OpenAPI and
must not be hand-edited.

### 4.2 Resource-owned inputs

Each `ec2_vm` resource must carry, directly or through established project-level fields:

- opaque resource ID and user-visible name;
- source region;
- target Azure region;
- EC2 instance type;
- operating system, constrained to Windows for the first release;
- tenancy, constrained to Shared;
- source purchase model, constrained to On-Demand;
- quantity or one normalized resource per VM, following the existing project convention;
- monthly powered-on hours as a decimal input;
- persistent volume list;
- CPU behavior input for burstable sources;
- instance-store use state and requirements when the source offers local storage;
- high-frequency/per-core-performance requirement state when the source family has that semantic;
- target licensing assumption, initially Windows license included with Azure Hybrid Benefit off.

The client must not supply prices, totals, selected explanations, owner identifiers, revisions, or
calculated target capabilities. A requested target override may be accepted only as a request; the
server must revalidate it against the same catalog, region, lifecycle, capability, storage, and
price rules as an automatically selected target.

### 4.3 Persistent volume contract

Each persistent source volume must include:

- stable volume ID;
- volume type;
- provisioned size in GiB;
- provisioned IOPS when applicable;
- provisioned throughput in MiB/s when applicable;
- role: `os`, `data`, or `unknown`;
- optional sanitized display label.

`unknown` is valid for an image-assisted draft but blocks final target-disk selection and final
calculation. The UI may suggest `os` for a sole volume, but the user must confirm it. Exactly one OS
volume is required for a complete first-release Windows VM resource. Additional persistent volumes
are data volumes.

Do not aggregate volumes. Preserve source-to-target volume identity and emit one managed-disk
mapping per persistent source volume.

### 4.4 Instance-store contract

Use an explicit state rather than a Boolean inferred from the instance type:

- `unknown`: source offers instance storage but use was not established;
- `not_used`: target need not provide equivalent local storage;
- `used`: target must meet the declared ephemeral capacity and workload constraints.

`unknown` blocks a recommendation for `r6id` and `z1d`. When `used`, capture required capacity and
whether data loss on stop/deallocate/redeploy is acceptable. Do not add instance-store capacity to
the persistent managed-disk total. A target local disk is included with VM compute rather than
priced as a managed disk.

### 4.5 Image-assisted drafts

The approved assistant flow may extract a draft of the fields above. It must not calculate money,
invent omitted regions or quantities, choose a target, or turn uncertainty into a confirmed value.
The review UI must surface unconfirmed volume roles, counts, source/target regions, burst behavior,
instance-store use, hours, and licensing assumptions before calculation.

Workload names and raw project payloads must not be logged. Images and extracted customer content
must remain within the data-flow and retention controls approved for the assistant architecture.

## 5. Catalog and Provider Model

### 5.1 AWS EC2 source provider

Reuse EC2 product normalization for these dimensions:

- instance type and family;
- vCPU and memory;
- processor architecture;
- operating system;
- tenancy;
- capacity status;
- pre-installed software;
- purchase term and unit;
- source region;
- current-generation metadata;
- local instance-store count, type, and size;
- network and EBS limits when present.

The current EC2 SQL path requires `currentGeneration = Yes`. Do not relax that path. Add an
`ec2_vm`-specific acceptance policy that can price a reviewed previous-generation source inventory
when all other Windows Shared On-Demand dimensions match and a current public rate exists. Source
acceptance describes the customer's inventory; it is separate from the target policy that favors a
new current Azure generation.

Reject ambiguous or duplicate price rows. Preserve the provider's source/effective/retrieval date
and normalized filter dimensions.

### 5.2 AWS EBS source provider

Reuse the existing EBS provider and formulas. Initial `gp3` normalization must resolve separate
rates for:

- provisioned GiB-month;
- provisioned IOPS above the included baseline; and
- provisioned throughput above the included baseline.

The provider key must include region, currency, volume type, price dimension, unit, and effective
date. Project data must never be sent to the AWS pricing endpoint; only minimal public catalog
filters are permitted.

### 5.3 Azure VM capability and lifecycle catalog

Price data is not a capability catalog. Add an owned, reviewed, versioned catalog generated from
Microsoft documentation and the Azure Compute Resource SKUs API. Each target size record must
include at least:

- ARM SKU name and display family;
- semantic lineage, such as burstable, general purpose, memory optimized, or reviewed
  high-frequency;
- generation identifier and an explicit generation rank within that lineage;
- lifecycle state: current/recommended, previous generation, preview, restricted, or retired;
- supported architecture and Windows eligibility;
- region and applicable location/zone restrictions;
- vCPU and memory GiB;
- maximum data-disk count;
- supported managed-disk capabilities, including Premium I/O;
- VM-level uncached disk IOPS and throughput limits;
- local temporary-disk presence, type, and usable capacity when documented;
- source URLs, retrieval date, effective date when available, and generator version.

Do not derive lifecycle solely by parsing `v5`, `v6`, or `v7` from a SKU string. A generator may
parse a generation only when the family catalog explicitly validates the result. Do not infer
local-disk absence from the Resource SKUs capability `MaxResourceVolumeMB`; use the documented
family capability and the correct resource/temp-disk fields.

Preview, retired, and undocumented sizes are ineligible. A previous-generation target is eligible
only as an explained fallback when no current target survives the hard constraints.

### 5.4 Azure VM price provider

Normalize Windows pay-as-you-go consumption prices from the Azure Retail Prices API. The provider
must match the requested region, currency, ARM SKU, Windows product, consumption price type, and
billing unit. It must exclude Spot, reservation, savings-plan, dev/test, low-priority, and Linux
meters.

Price selection must not depend on a friendly-name substring alone. Normalize and validate all
material meter dimensions, reject conflicting rows, and retain the source URL, effective date,
retrieval time, currency, unit, and normalized raw price lexeme.

### 5.5 Azure managed-disk catalog and price provider

Add a separate managed-disk capability catalog. It must represent:

- disk offer and redundancy option;
- supported regions;
- allowed role (`os`, `data`, or both);
- capacity tiers or min/max/increment rules;
- included and maximum IOPS;
- included and maximum throughput;
- whether performance is provisioned independently of capacity;
- source and effective/retrieval metadata.

Normalize all price components needed by an eligible disk. Tiered disks require the selected tier's
capacity price. Premium SSD v2 requires capacity, additional IOPS, and additional throughput price
dimensions. A target is price-complete only when every required dimension is available in the same
region and currency.

Premium SSD v2 is available in Sweden Central, uses 1 GiB capacity increments, and includes 3,000
IOPS and 125 MB/s before additional performance charges. It cannot be used as an OS disk. These are
hard rules, not UI hints.

### 5.6 Freshness and fallback

Live public prices may populate a bounded server-side provider cache under the existing pricing
policy. Tests must use reviewed frozen fixtures. A stale or unavailable provider result must be
shown as stale/unavailable; it must not silently become zero, reuse another region, switch currency,
or substitute a SQL MI rate.

The selected target and every cost component must resolve from one coherent calculation snapshot.

## 6. Cost Model

Use `rust_decimal` for all rates, quantities, hours, and totals. Preserve source decimal lexemes.
Round only at the product specification's display/export boundary.

### 6.1 Common terms

For resource `r` and volume `v`:

```text
H(r) = powered-on hours per month
Q(r) = resource quantity
S(v) = provisioned capacity in GiB
I(v) = provisioned IOPS
T(v) = provisioned throughput in MiB/s
```

CPU utilization does not discount provisioned VM hours. It is used only to assess burstable
semantic suitability. Full-time operation may be suggested as 730 hours/month only when it is
shown and persisted as an explicit user-confirmed assumption.

### 6.2 AWS monthly source cost

```text
aws_compute(r) = aws_windows_shared_ondemand_hourly_rate(r) * H(r) * Q(r)

aws_gp3(v) =
    S(v) * gp3_capacity_rate
  + max(I(v) - 3000, 0) * gp3_additional_iops_rate
  + max(T(v) - 125, 0) * gp3_additional_throughput_rate

aws_monthly(r) = aws_compute(r) + Q(r) * sum(aws_gp3(v))
```

The 3,000 IOPS and 125 MiB/s baselines apply to `gp3`; do not generalize them to another EBS type.
The exact provider billing units must be normalized before using the formula.

### 6.3 Azure monthly target cost

```text
azure_compute(r) = azure_windows_payg_hourly_rate(selected_vm) * H(r) * Q(r)

azure_monthly(r) =
    azure_compute(r)
  + Q(r) * sum(azure_managed_disk_cost(mapped_volume))
```

For a tiered managed disk, `azure_managed_disk_cost` is the normalized prorated cost of the smallest
eligible tier. For Premium SSD v2 it is the sum of normalized capacity, IOPS above 3,000, and
throughput above 125 MB/s price components. The implementation must resolve the provider's units
and hourly/monthly proration rules rather than assume every retail meter is monthly.

Windows license-included PAYG is the initial target assumption. Azure Hybrid Benefit is `false` and
must not be inferred. The source and target totals must use the same currency; currency conversion
is out of scope.

### 6.4 Project totals and exclusions

Project totals are the decimal sum of complete resource results. A resource with an unresolved
region, volume role, target, or required price dimension is incomplete and must not contribute a
fabricated partial total. The API must return structured missing-data reasons.

The result must list the exclusions in section 2.2 and identify any resource whose target is only a
capacity-fit scenario rather than a recommended equivalent.

## 7. Managed-Disk Mapping

### 7.1 Hard rules

For each persistent EBS volume independently:

1. Preserve at least its provisioned capacity, IOPS, and throughput.
2. Require a disk offer available in the target region and supported by the selected VM.
3. Require an OS-capable offer for role `os`.
4. Count every non-OS persistent disk against the VM's data-disk limit.
5. Check each disk's limits and the selected VM's aggregate uncached IOPS and throughput limits.
6. Select the newest VM only from candidates that can support the complete disk set.
7. Prefer the smallest price-complete disk configuration that meets the requirements; price is not
   allowed to override a capability constraint.
8. Emit a one-to-one source-volume/target-disk explanation.

### 7.2 Initial `gp3` mapping

A 1,024 GiB `gp3` data volume provisioned at its 3,000 IOPS and 125 MiB/s baseline is capability-fit
for a 1,024 GiB Premium SSD v2 data disk configured at those values, subject to target-region,
selected-VM, price, and aggregate-limit checks.

A 1,024 GiB `gp3` OS volume cannot use Premium SSD v2. The selector must evaluate OS-capable tiers;
Premium SSD `P30` is the expected capability candidate because its documented capacity and
performance meet or exceed the `gp3` baseline. This is not a final price commitment until the
target-region meter is normalized. A lower-cost disk may be selected only if it satisfies all three
capacity, IOPS, and throughput requirements.

The selector must never combine the OS and data volumes or treat the total capacity as a SQL data
allocation.

## 8. Azure VM Target Selection

### 8.1 Requirement derivation

For the first release, required vCPU and memory equal the source allocation. The selector may round
up to an available Azure shape but may not automatically round down. It derives:

- minimum vCPU and memory;
- source semantic lineage;
- Windows/x86-64 requirement;
- burst-policy state;
- high-frequency review state;
- local temporary-storage requirements;
- persistent OS/data disk count and per-disk requirements;
- aggregate disk IOPS and throughput requirements;
- target region, currency, and price completeness.

### 8.2 Hard candidate filters

Apply these filters before ranking:

1. The family is GA and current/recommended, or is an explicitly explained previous-generation
   fallback.
2. The exact ARM SKU is available in the target region and has no applicable subscription,
   location, or zone restriction for the requested scenario.
3. The SKU supports Windows x86-64 and all requested VM features.
4. vCPU and memory meet or exceed the requirements.
5. The SKU has enough data-disk slots and supports every selected disk offer.
6. Every disk meets its own limits, and aggregate disk requirements fit the VM-level uncached IOPS
   and throughput limits.
7. When instance-store use is `used`, the SKU provides reviewed local ephemeral storage with enough
   usable capacity and compatible semantics.
8. A complete Windows VM rate and every required managed-disk price dimension are available for the
   target region and currency.

If a newer family fails a hard filter, retain a machine-readable rejection reason. Do not silently
drop to an older family.

### 8.3 Semantic lineages

Initial lineage policy:

| AWS source class | Preferred Azure lineage | Required behavior |
| --- | --- | --- |
| T3 burstable general purpose | B-series current burstable lineage | Eligible only under section 8.5; otherwise use the non-burst fallback policy. |
| M general purpose | D-series current general-purpose lineage | Preserve vCPU/memory and storage constraints. |
| R memory optimized | E-series current memory-optimized lineage | Preserve vCPU/memory and storage constraints. |
| R with declared local-store use | E-series memory-optimized lineage with documented local disk | Require local capacity and ephemeral semantics; otherwise no recommended mapping. |
| Z high-frequency memory optimized | Reviewed high-frequency memory-capable lineage | Do not claim equivalence from vCPU/memory alone; apply section 8.6. |

Family names are policy data in the reviewed catalog, not conditionals scattered through HTTP
handlers or the frontend.

### 8.4 Ranking and newest-generation preference

After hard filters, rank candidates in this order:

1. exact semantic-lineage match before fallback lineage;
2. current/recommended lifecycle before previous generation;
3. highest reviewed generation rank within that lineage;
4. smallest vCPU surplus;
5. smallest memory surplus;
6. smallest eligible aggregate disk-capability surplus;
7. lowest complete pay-as-you-go monthly target cost;
8. lexical ARM SKU name as the final stable tie-breaker.

Generation is deliberately ranked before resource surplus and price. A v7 candidate therefore
outranks v6 or v5 when both satisfy the same semantic and hard constraints. Do not encode a fixed
rule such as "prefer v5 over v4"; the catalog must allow later current generations to win without a
code change.

An older target is allowed only when every newer target in the preferred lineage is rejected for a
recorded hard reason. The result explanation must name the newer rejected generation and reason.

### 8.5 Burstable T3 policy

T3 cannot be mapped to B-series solely from vCPU and memory. Use these states:

- `confirmed_burst_compatible`: the user accepts credit-based behavior and provides a reviewed
  utilization profile that fits the selected B-series baseline; B-series may enter the preferred
  candidate set.
- `requires_sustained_cpu`: exclude burstable targets and evaluate the newest eligible D-series
  non-burstable fallback.
- `unknown`: calculate a clearly labeled D-series conservative capacity-fit scenario, but mark the
  burst decision `review_required`; do not present B-series as the recommendation.

The selector must retain the source T3 credit-mode assumption and disclose that AWS and Azure
credit models are not claimed to be identical.

### 8.6 High-frequency `z1d` policy

`z1d.2xlarge` is not automatically equivalent to an `8 vCPU / 64 GiB` Azure VM. When a per-core or
clock-sensitive requirement is `required` or `unknown`, return a capacity-fit scenario only and
mark it `review_required`. A recommended mapping requires a reviewed target-family
high-frequency capability and customer acceptance of benchmark evidence outside the TCO engine.

The TCO engine must never invent a benchmark score or turn a capacity-fit E-series candidate into a
performance guarantee.

### 8.7 Sweden Central evidence snapshot

Read-only Azure Resource SKU queries on 2026-08-24 found the following capacity candidates in
Sweden Central without restrictions in the returned SKU records:

| Required vCPU/GiB | Capacity candidate | Intended lineage use |
| --- | --- | --- |
| `2/8` | `Standard_D2s_v7` | M-series mapping or conservative T3 fallback |
| `4/16` | `Standard_D4s_v7` | Conservative T3 fallback |
| `2/16` | `Standard_E2s_v7` | R-series mapping |
| `8/64` | `Standard_E8s_v7` | Capacity-only candidate for `z1d.2xlarge` |
| `32/256` | `Standard_E32s_v7` | R-series mapping when local storage is not required |
| `48/384` | `Standard_E48s_v7` | R-series mapping when local storage is not required |

These are evidence that a hard-coded v5 preference is already stale. They are not final mappings or
price commitments. The production selector still has to apply lifecycle, Windows price,
local-storage, Premium I/O, disk count, aggregate IOPS/throughput, and all other hard filters.

Bsv2 sizes were also exposed in Sweden Central and remain candidate targets for confirmed
burst-compatible T3 workloads. Local-disk-enabled D/E variants must be verified from their family
documentation and complete Resource SKU capabilities before use; `MaxResourceVolumeMB = 0` is not
evidence that a `d` variant lacks a local disk.

## 9. Deterministic Results and Explanations

The server must return structured calculation steps suitable for UI and workbook rendering. At a
minimum, each resource explanation must identify:

- normalized source shape and source-generation status;
- source compute and each source-volume price component;
- semantic-lineage decision;
- burst, high-frequency, and local-store decisions;
- target-region availability and restriction result;
- required and selected vCPU/memory;
- every newer-generation rejection before an older fallback;
- one-to-one disk mappings and VM-level disk-limit checks;
- selected Windows VM and managed-disk price provenance;
- assumptions, exclusions, stale/missing data, and recommendation status.

Use stable explanation codes, for example:

```text
vm.source.normalized
vm.target.semantic_lineage
vm.target.generation_preference
vm.target.capacity_fit
vm.target.burst_policy
vm.target.high_frequency_review
vm.target.local_store
vm.target.disk_limits
disk.target.role_constraint
price.source.provenance
price.target.provenance
```

Explanations come from the deterministic workflow. An LLM must not select a target or generate the
authoritative explanation.

## 10. Persistence, Authorization, and Privacy

- Persist `ec2_vm` as a separate resource variant without rewriting existing `ec2` documents.
- Preserve optimistic concurrency and reject client-owned revisions or ETags according to the
  existing API contract.
- Derive ownership from the protected ingress identity and scope every project operation by both
  tenant and object ID. Never accept an owner from the payload.
- Keep local mock authentication restricted to `APP_ENV=local`.
- Apply existing request-size, resource-count, volume-count, timeout, rate-limit, and sanitized
  error controls to the new variant. Add explicit bounds where the existing limits do not cover the
  new volume list.
- Do not log raw identity headers, full project payloads, customer workload names, source images,
  prices from customer agreements, or provider response bodies.
- Send only public SKU, region, currency, and meter filters to pricing/catalog providers.
- Store no AWS or Azure account credentials for public-price retrieval.

## 11. User Experience

Add **AWS EC2 Virtual Machines** as a distinct workload choice. Its editor reuses the repeatable EBS
volume interaction but never displays SQL fields.

The editor must support:

- source and target region selection;
- EC2 instance-type selection and reviewed shape display;
- powered-on hours;
- Windows Shared On-Demand assumptions;
- repeatable volume rows with role, type, size, IOPS, and throughput;
- burst-policy review for T3;
- instance-store review for source families that offer it;
- high-frequency review for Z-family sources;
- target override requests with server revalidation;
- visible incomplete, stale-price, fallback-generation, capacity-fit-only, and no-mapping states.

The result must distinguish `recommended`, `capacity_fit_review_required`, `incomplete`, and
`no_eligible_target`. It must not hide an unfavorable Azure result or silently remove an unresolved
resource from the project total.

## 12. Verified Repository Gaps

The repository does not currently contain the complete data or behavior needed for this workload.

| Area | Current repository state | Required addition |
| --- | --- | --- |
| Workload discriminator | `ec2` means EC2 SQL. | Add separate `ec2_vm` variants through domain, OpenAPI, persistence, workflow, and web draft/editor/result unions. |
| Source VM fixture | `app/catalogs/local-price-fixture.json` contains only `r6id.8xlarge` from the eight represented source SKUs. | Add reviewed shape and Windows Shared On-Demand price rows for all eight SKUs in a confirmed source region. |
| Source EBS fixture | The local fixture has no EBS price rows. | Add `gp3` capacity, additional IOPS, and additional throughput dimensions. |
| Azure VM provider | Loader/provider support is SQL MI-specific on the Azure side. | Add Windows Azure VM Retail Prices normalization. |
| Azure VM capabilities | No reviewed non-SQL VM target catalog or lifecycle rank exists. | Add generated, versioned region/capability/lifecycle records for approved D, E, B, and any reviewed high-frequency lineages. |
| Azure managed disks | No managed-disk provider or capability catalog exists. | Add OS/data role, tier/configuration, IOPS, throughput, region, and price dimensions. |
| Selector | `target_selector` selects SQL MI. | Add an independent Azure VM selector implementing section 8; do not add VM branches to SQL MI formulas. |
| Calculation workflow | EC2 SQL aggregates persistent EBS and SQL data for SQL MI. | Preserve and price VM volumes independently and map one-to-one. |
| Frontend | EC2 draft/editor requires SQL fields. | Add a separate VM draft/editor while reusing only neutral volume controls. |
| Frozen end-to-end inventory | Exact per-SKU image multiplicities are unavailable. | Re-read the source images or accept a user-confirmed machine-readable inventory before freezing expected totals. |

Exact public prices are intentionally not recorded in this specification. The source region is
unconfirmed, final Azure targets are conditional on semantic/storage rules, and rates change. Price
fixtures must be generated only after those dimensions are locked, with source URL, region,
currency, effective date, retrieval date, and raw decimal values recorded.

## 13. Implementation Surfaces

Expected changes, adapted to the final local design, include:

- `research/Azure Specification.md`: adopt the workload and resolve the current non-SQL exclusion.
- `openapi/openapi.yaml`: add the new discriminator, request/result schemas, and generated contract.
- `rust/src/domain/resource.rs`: add a non-SQL VM resource and neutral VM/volume requirements.
- `rust/src/pricing/aws_ec2.rs`: add a VM-specific source acceptance policy without changing EC2
  SQL behavior.
- `rust/src/pricing/aws_ebs.rs`: reuse the provider and extend only where fixture/metadata coverage
  requires it.
- `rust/src/pricing/loader.rs`: load Azure VM and managed-disk normalized records.
- `rust/src/calculation/cost.rs`: add VM and managed-disk formulas outside SQL formulas.
- `rust/src/calculation/workflow.rs`: orchestrate one-to-one VM and disk mapping.
- a dedicated Azure VM selector module rather than overloading the SQL MI selector.
- owned catalog generators and reviewed catalog artifacts under `research/` and `app/catalogs/`.
- `web/src/lib/draft.ts` and resource editor/result components: add the separate union variant and
  review states.
- generated TypeScript API types and focused Rust, Vitest, and Playwright fixtures/tests.

Do not add a new production runtime, external API, frontend framework, or direct dependency for this
feature.

## 14. Validation Strategy

### 14.1 Unit and property tests

- Decimal source and target formulas, including `gp3` and Premium SSD v2 included baselines.
- One-to-one volume identity and quantity multiplication.
- OS role rejects Premium SSD v2.
- Unknown volume role blocks a complete result.
- Per-disk and aggregate VM IOPS/throughput limits.
- No vCPU or memory undersizing.
- Stable tie-breaking independent of input/catalog ordering.
- Missing, duplicate, conflicting, stale, wrong-region, wrong-currency, Linux, Spot, and reservation
  price rows fail closed.

### 14.2 Selector fixtures

Freeze catalog fixtures that prove:

- an eligible v7 D/E size wins over equivalent v6 and v5 sizes;
- a region restriction on v7 causes an explained v6 fallback;
- an unpriced v7 candidate is rejected rather than assigned a zero rate;
- `m6i.large` follows the current D-series lineage;
- R-family instances follow the current E-series lineage;
- `r6id` with `not_used` local storage can select a non-local E-series candidate;
- `r6id` with `used` local storage selects only a compatible local-disk variant or returns no
  eligible target;
- T3 `confirmed_burst_compatible` may select B-series;
- T3 `requires_sustained_cpu` excludes B-series and evaluates current D-series;
- T3 `unknown` produces a conservative capacity-fit result with review required;
- `z1d.2xlarge` cannot become a recommended equivalent from capacity alone;
- a 1,024 GiB baseline `gp3` data volume can map to Premium SSD v2;
- a 1,024 GiB baseline `gp3` OS volume evaluates an OS-capable tier such as P30;
- VM data-disk count or aggregate performance can disqualify an otherwise correct compute shape.

### 14.3 Regression and contract tests

- Existing `ec2` EC2 SQL payloads, source acceptance, target selection, formulas, persistence, and
  generated frontend types remain unchanged except for additive union handling.
- OpenAPI generation is clean and committed output matches the schema.
- API rejects SQL fields on `ec2_vm` and VM-only fields on existing SQL resources where the union
  requires it.
- Tenant/object ownership tests cover create, read, update, calculate, export, and delete for the
  new resource.
- Image-assisted drafts retain uncertainty and require confirmation for omitted fields.
- Workbook output reproduces server totals and structured explanations without client-side
  formulas.

### 14.4 End-to-end acceptance fixture

After the image counts and regions are confirmed, create a synthetic frozen fixture with the 15 VM
rows and 15 one-TiB `gp3` volumes. It passes when:

- all eight unique source SKUs normalize with the confirmed multiplicities;
- all 15 source VMs and 15 volumes appear exactly once;
- every complete resource has one Azure VM and one target disk per persistent source volume;
- newest-generation selection and all fallback reasons match the frozen target catalog;
- source and target totals reproduce from frozen decimal rates;
- no live provider call is needed;
- unresolved T3, local-store, high-frequency, or disk-role inputs prevent a false recommendation;
  and
- changing only catalog generation metadata can make a newer eligible generation win without a
  code change.

## 15. Implementation Order

1. Resolve the inputs in section 16 and amend the product specification to authorize `ec2_vm`.
2. Add reviewed AWS fixture coverage and the Azure VM/managed-disk capability catalogs and
   generators.
3. Add Azure VM and managed-disk price normalization with frozen provider tests.
4. Add the domain variant, disk requirements, formulas, and independent selector.
5. Extend OpenAPI, persistence, authorization tests, and generated TypeScript types.
6. Add the Svelte draft/editor/result flow and image-draft review states.
7. Freeze the confirmed 15-row inventory and run formula, selector, API, frontend, workbook,
   security, dependency, and regression gates.

Each step must keep the existing EC2 SQL workload passing. Catalog/provider work should precede UI
target promises so the interface cannot offer targets the server cannot verify or price.

## 16. Resolved Defaults and Remaining Inputs

### 16.1 Defaults resolved by the repository owner on 2026-08-24

The owner directed that every input in the original section 16.1 default to the ordinary value
that covers the common case rather than remaining a blocking question. Section 4.4 of the product
specification is the controlling record; the table below repeats it for implementation reference.

| Input | Default | Basis |
| --- | --- | --- |
| Powered-on hours | 730 per month, persisted as 8,760 annual | 24/7 production, matching the AWS and Azure calculators |
| Resource quantity | 1 per inventory row | Each row is one named machine |
| AWS source region | Project `aws_region`, currently `eu-west-1` | Existing project setting |
| Azure target region | Project `azure_region`, currently Sweden Central | Existing project setting |
| Operating system | Windows | First release scope |
| Tenancy and purchase | Shared tenancy, On-Demand | First release scope |
| Target licensing | Windows license included, Azure Hybrid Benefit off | Entitlement is never inferred |
| Volume role | First listed volume `os`, every other volume `data` | Deterministic; the calculator never invents a volume |
| Instance-store NVMe (`r6id`, `z1d`) | `not_used` | Ephemeral scratch in the common case |
| T3 burstable mapping | B-series | The source is already burstable; credit models are not claimed identical |
| `z1d` high-frequency mapping | E-series on capacity | Per-core equivalence is not claimed and must be disclosed |

Defaulting an input does not suppress its disclosure. Every defaulted value must appear in the
result assumptions, and the T3 credit-model and `z1d` per-core caveats must remain visible in the
result.

### 16.2 Standing explicit assumptions

- Windows license-included pay-as-you-go on both clouds.
- Shared EC2 tenancy and On-Demand source prices.
- Azure Hybrid Benefit off; no entitlement inferred.
- No reservations, savings plans, negotiated discounts, Spot, or dev/test pricing.
- No automatic downsize, consolidation, high availability, backup, network egress, support, or
  operational-labor cost.
- One target VM per source VM and one target managed disk per persistent EBS volume.

### 16.3 Still customer-specific

These remain per-project inputs a user may override. They do not block implementation because each
has a default above, but a frozen customer estimate should confirm them.

- Per-SKU quantity when one inventory row represents more than one machine.
- Source and target region when the project defaults are not correct for the customer.
- Actual powered-on schedule for non-production machines.
- Whether the `r6id` and `z1d` local NVMe stores are used, and the required capacity when they are.
- Reviewed benchmark evidence for `z1d` per-core performance, which is out of scope for the
  calculator.

## 17. First-Party Evidence

Sources were retrieved or rechecked on 2026-08-24. Product catalogs and prices must record their
own later retrieval/effective dates when generated.

- AWS EC2 general-purpose instance specifications, including T3 shape and burst behavior:
  https://docs.aws.amazon.com/ec2/latest/instancetypes/gp.html
- AWS EC2 memory-optimized instance specifications, including R, Z, EBS, and instance-store data:
  https://docs.aws.amazon.com/ec2/latest/instancetypes/mo.html
- AWS EBS `gp3` behavior:
  https://docs.aws.amazon.com/ebs/latest/userguide/general-purpose.html#gp3-ebs-volume-type
- AWS Price List API:
  https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/price-changes.html
- Azure VM size overview and naming:
  https://learn.microsoft.com/azure/virtual-machines/sizes/overview
- Azure Dsv7 family:
  https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/dsv7-series
- Azure Ddsv7 family:
  https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/ddsv7-series
- Azure Esv7 family:
  https://learn.microsoft.com/azure/virtual-machines/sizes/memory-optimized/esv7-series
- Azure Edsv7 family:
  https://learn.microsoft.com/azure/virtual-machines/sizes/memory-optimized/edsv7-series
- Azure Bsv2 family:
  https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/bsv2-series
- Azure previous-generation sizes:
  https://learn.microsoft.com/azure/virtual-machines/sizes/previous-gen-sizes-list
- Azure managed-disk types and Premium SSD v2 constraints:
  https://learn.microsoft.com/azure/virtual-machines/disks-types#premium-ssd-v2
- Azure Compute Resource SKUs API:
  https://learn.microsoft.com/rest/api/compute/resource-skus/list
- Azure Retail Prices API:
  https://learn.microsoft.com/rest/api/cost-management/retail-prices/azure-retail-prices

Read-only regional evidence was obtained with Azure CLI SKU listing; no deployment, `what-if`,
resource mutation, credential change, or customer-data egress was performed.