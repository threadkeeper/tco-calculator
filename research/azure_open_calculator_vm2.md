# Azure Pricing Calculator EC2 VM Walkthrough 2

Status: complete
Run date: 2026-08-24
Source: synthetic fixture `tests/assistant-workload-classification/ec2_vm/ec2-vm-02/input.png`

## Scope And Boundaries

This attended research run maps the synthetic EC2 VM inventory to Windows Azure Virtual Machines and enters the resulting target-only configuration into a user-opened Azure Pricing Calculator estimate through visible Playwright controls.

- The estimate name is `Workload 2 EC2 VM`, formed from the image header `Workload 2 | EC2 VM` by omitting the visual separator.
- Calculator line labels exactly match source rows `VM1` through `VM10` in source order.
- Browser automation does not enter credentials, operate sign-in, inspect account identifiers, capture cookies or tokens, read browser storage, inspect request or response bodies, or automate Save/Share.
- The run is an estimate, not a quote, migration-readiness guarantee, performance guarantee, capacity promise, licensing determination, or deployment approval.
- No Azure resource is created or changed. Subscription quota and target-image compatibility are not validated and must be checked before deployment.

## Image Analysis

The image is a synthetic spreadsheet headed `Workload 2 | EC2 VM`. It contains ten Windows EC2 rows, each with one `1 TB gp3 Storage` entry:

| Source row | Visible EC2 shape | Source capacity | Visible OS | Visible persistent storage |
| --- | --- | ---: | --- | --- |
| VM1 | `t3.large` | 2 vCPU / 8 GiB | Windows | 1 TB gp3 |
| VM2 | `m6i.large` | 2 vCPU / 8 GiB | Windows | 1 TB gp3 |
| VM3 | `r7i.large` | 2 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM4 | `t3.xlarge` | 4 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM5 | `t3.xlarge` | 4 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM6 | `m6i.large` | 2 vCPU / 8 GiB | Windows | 1 TB gp3 |
| VM7 | `r7i.large` | 2 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM8 | `t3.xlarge` | 4 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM9 | `t3.xlarge` | 4 vCPU / 16 GiB | Windows | 1 TB gp3 |
| VM10 | `t3.large` | 2 vCPU / 8 GiB | Windows | 1 TB gp3 |

The image does not state region, running schedule, availability design, disk role, gp3 provisioned performance, CPU-credit use, or licensing entitlement. Those values are assumptions below, not extracted facts.

## Estimate Assumptions

The run applies the adopted defaults in `research/Azure Specification.md` section 4.4 and the detailed selection rules in `docs/EC2-VM-TCO-SPEC.md`:

| Setting | Applied value |
| --- | --- |
| Azure target region | Sweden Central |
| Quantity | 1 per source row |
| Usage | 730 hours/month |
| Operating system | Windows |
| Purchase option | Pay as you go |
| Azure Hybrid Benefit | Off; Windows license included |
| Availability design | Single VM; no availability zone, set, or multi-VM HA added |
| First and only volume role | OS disk |
| gp3 performance | Baseline 3,000 IOPS and 125 MiB/s because the image shows no override |
| T3 target lineage | Current B-series burstable lineage, as explicitly adopted by the controlling specification |

AWS T3 and Azure B-series both use CPU-credit models, but their credit accrual, baseline, banking, throttling, and workload behavior are not claimed to be equivalent. The B-series targets require workload-profile validation before migration.

## Reviewed Mapping

The deterministic selector preserves or rounds up vCPU and memory, retains the source family class, and retains one persistent disk per source volume. The 1 TB source label is normalized under the repository contract to 1,024 GiB. Because it is the OS volume, Premium SSD v2 is ineligible; Premium SSD LRS P30 is the smallest OS-capable tier meeting 1,024 GiB, 3,000 IOPS, and 125 MiB/s.

| Source row | EC2 source | Azure VM target | Target capacity | Azure OS disk | Mapping qualification |
| --- | --- | --- | ---: | --- | --- |
| VM1 | `t3.large` | `Standard_B2s_v2` | 2 vCPU / 8 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |
| VM2 | `m6i.large` | `Standard_D2ds_v7` | 2 vCPU / 8 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; general-purpose lineage |
| VM3 | `r7i.large` | `Standard_E2ds_v7` | 2 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; memory-optimized lineage |
| VM4 | `t3.xlarge` | `Standard_B4s_v2` | 4 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |
| VM5 | `t3.xlarge` | `Standard_B4s_v2` | 4 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |
| VM6 | `m6i.large` | `Standard_D2ds_v7` | 2 vCPU / 8 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; general-purpose lineage |
| VM7 | `r7i.large` | `Standard_E2ds_v7` | 2 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; memory-optimized lineage |
| VM8 | `t3.xlarge` | `Standard_B4s_v2` | 4 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |
| VM9 | `t3.xlarge` | `Standard_B4s_v2` | 4 vCPU / 16 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |
| VM10 | `t3.large` | `Standard_B2s_v2` | 2 vCPU / 8 GiB | 1 x P30 Premium SSD LRS | Exact capacity fit; burst-credit behavior requires review |

## Pre-Browser Validation

- At 2026-08-24T21:52:54Z, the focused Rust selector test `section_3_2_source_shapes_map_to_the_newest_eligible_current_size` passed: 1 passed, 0 failed, 313 filtered out.
- Microsoft Learn confirms B2s v2 at 2 vCPU/8 GiB, B4s v2 at 4 vCPU/16 GiB, D2ds v7 at 2 vCPU/8 GiB, and E2ds v7 at 2 vCPU/16 GiB. The Bsv2 documentation also confirms that the series uses an Azure CPU-credit model.
- Microsoft Learn confirms Premium Storage support for the selected lineages and P30 base capability of 1,024 GiB, 5,000 IOPS, and 200 MB/s.
- On 2026-08-24, the Azure Retail Prices tool returned current non-Spot Windows Consumption meters in Sweden Central, USD: B2s v2 at $0.0956/hour effective 2023-09-01, B4s v2 at $0.191/hour effective 2023-09-01, D2ds v7 at $0.258/hour effective 2026-04-01, and E2ds v7 at $0.304/hour effective 2026-05-01.
- The current P30 Premium SSD Managed Disks LRS Consumption meter is $148.68/month in Sweden Central, USD, effective 2021-06-08.
- Public-rate arithmetic predicts $1,517.816/month for Windows compute and $1,486.80/month for ten P30 disks, or $3,004.616 before Calculator rounding. The expected displayed total is $3,004.62/month; the visible Calculator result remains authoritative for this attended run.

## Playwright Log

| UTC | Step | Outcome |
| --- | --- | --- |
| 2026-08-24T21:55:06Z | Read the shared Azure Pricing Calculator page through its visible accessibility state. | Confirmed an empty `Your Estimate` at $0.00 and a visible Virtual Machines `Add to estimate` control. Account and session details were excluded from this record. |
| 2026-08-24T21:55:06Z | Clicked the Virtual Machines product card's `Add to estimate` control. | One expanded Virtual Machines item was added with visible defaults: East US, Windows, OS Only, Standard, D2 v3, quantity 1, 730 hours, PAYG, license included, zero managed disks, 5 GB example bandwidth, and $137.24/month. |
| 2026-08-24T21:55:06Z | Renamed the estimate to `Workload 2 EC2 VM`, renamed the first line to `VM1`, changed region to Sweden Central, and selected General purpose. | Title, line name, and region succeeded before a combined locator timed out because the category accessible name was `Category`, not `Category:`. A visible read-back confirmed those successful changes; selecting General purpose with the corrected exact name then succeeded. |
| 2026-08-24T21:55:48Z | Searched the visible Instance Series control for `Bsv2` and selected the sole `Bsv2-series` result, then searched Instance for `B2s v2` and selected the sole result. | Read-back confirmed B2s v2, 2 vCPU, 8 GB RAM, and a displayed $0.096/hour rate. `VM1` showed $69.79/month before disk. |
| 2026-08-24T21:55:48Z | Expanded Managed Disks, selected Premium SSD, retained LRS, selected P30, and changed quantity from 0 to 1. | The visible P30 option stated 1,024 GiB, 5,000 IOPS, 200 MB/sec, and $148.680/month. A locator timed out after the quantity label changed from plural `Disks` to singular `Disk`; subsequent state confirmed one P30 and $218.47/month. |
| 2026-08-24T21:55:48Z | Expanded Bandwidth and changed the generated outbound transfer example from 5 GB to 0 GB. | The visible summary confirmed 0 GB transfer from Sweden Central and remained $218.47/month. `VM1` became the canonical line for cloning. |
| 2026-08-24T21:56:55Z | Cloned `VM1`, renamed the clone `VM2`, selected Ddsv7-series, and selected D2ds v7. | Read-back confirmed D2ds v7, Sweden Central, Windows license included, OS Only, Standard, PAYG, quantity 1, 730 hours, one P30, 0 GB transfer, and $337.02/month. |
| 2026-08-24T21:57:10Z | Cloned `VM2`, renamed the clone `VM3`, selected Memory optimized, selected Edsv7-series, and selected E2ds v7. | Read-back confirmed E2ds v7 with the inherited common settings, one P30, 0 GB transfer, and $370.60/month. |
| 2026-08-24T21:57:32Z | Cloned `VM3`, renamed the clone `VM4`, selected General purpose, selected Bsv2-series, and selected B4s v2. | The B4s v2 compute selection succeeded. A later consolidated read-back found that this series change had reset the inherited managed-disk size to Standard SSD E1. |
| 2026-08-24T21:57:44Z | Cloned `VM4` and renamed the clone `VM5`; no compute change was needed. | B4s v2 was inherited, along with the unintended E1 disk that had resulted from the prior Bsv2 series change. |
| 2026-08-24T21:58:52Z | Performed a five-line visible checkpoint read-back. | Confirmed VM1 B2s v2/P30 at $218.47, VM2 D2ds v7/P30 at $337.02, VM3 E2ds v7/P30 at $370.60, and detected E1 instead of P30 on VM4 and VM5. An initial read helper was unavailable in this Playwright version; a corrected indexed read made no state change and exposed the mismatch. |
| 2026-08-24T21:59:19Z | Reopened Managed Disks for VM4 and VM5 and explicitly selected Premium SSD, LRS, and P30 while retaining quantity 1. | Both repaired controls read back as Premium SSD P30 at 1,024 GiB and quantity 1. |
| 2026-08-24T21:59:33Z | Re-read VM4 and VM5 after repair. | Both summaries confirmed B4s v2, one P30, 0 GB transfer, and $288.11/month. The five-line estimate header showed approximately $1,502/month; an initially ambiguous monthly-total locator was narrowed without changing state. |
| 2026-08-24T22:00:23Z | Cloned `VM5`, renamed the clone `VM6`, selected Ddsv7-series, and selected D2ds v7. | Read-back confirmed six items and D2ds v7 for VM6. Its inherited visible summary later confirmed one P30, 0 GB transfer, and $337.02/month. |
| 2026-08-24T22:00:37Z | Cloned `VM6`, renamed the clone `VM7`, selected Memory optimized, selected Edsv7-series, and selected E2ds v7. | Read-back confirmed seven items and E2ds v7 for VM7. Its inherited visible summary later confirmed one P30, 0 GB transfer, and $370.60/month. |
| 2026-08-24T22:00:53Z | Cloned `VM7`, renamed the clone `VM8`, selected General purpose, selected Bsv2-series, and selected B4s v2. | Because the earlier Bsv2 switch had reset the disk, this step immediately reopened Managed Disks and explicitly selected Premium SSD LRS P30. Control read-back confirmed P30 before proceeding. |
| 2026-08-24T22:01:02Z | Cloned `VM8` and renamed the clone `VM9`; no compute change was needed. | Read-back confirmed nine items and four B4s v2 compute headings. The inherited visible summary later confirmed one P30, 0 GB transfer, and $288.11/month. |
| 2026-08-24T22:01:17Z | Cloned `VM9`, renamed the clone `VM10`, and changed Instance from B4s v2 to B2s v2 within Bsv2-series. | Read-back confirmed ten items and two B2s v2 compute headings. VM10 retained one P30 and 0 GB transfer and showed $218.47/month. |
| 2026-08-24T22:02:11Z | Performed a consolidated visible read-back of estimate title and all ten VM item forms and summaries. | Confirmed title `Workload 2 EC2 VM`; names VM1 through VM10 in order; intended B2s/D2ds/E2ds/B4s/B4s/D2ds/E2ds/B4s/B4s/B2s order; Sweden Central; Windows; OS Only; Standard; quantity 1; 730 hours; PAYG checked; License included checked; one P30; 0 GB transfer; and the intended item prices. The exact detail total was $3,004.62/month while the sticky header rounded it to $3,005. |
| 2026-08-24T22:02:49Z | Expanded and read the Managed Disks controls on every line. | All ten controls independently confirmed Premium SSD, LRS, P30 at 1,024 GiB/5,000 IOPS/200 MB/sec, quantity 1, and $148.68 subtotal. An initial broad button query also matched the managed-disk information tooltip; the corrected query was scoped to the visible submodule button and made no configuration change. |
| 2026-08-24T22:03:10Z | Expanded and read the Bandwidth controls on every line. | All ten independently confirmed 0 GB outbound from Sweden Central, $0.00 subtotal. East Asia remained the generated destination but contributes no transfer cost at 0 GB. |
| 2026-08-24T22:03:26Z | Read the Calculator-wide settings and exact totals and completed the attended run. | Confirmed USD, Basic support included, Microsoft Customer Agreement, Dev/Test off, $0.00 upfront, $3,004.62/month, and $36,055.39/year. Save, Save As, and Share were not invoked; the configured estimate was left open in the user's browser. |

## Final Calculator Estimate

| Item | VM | Common configuration | Monthly | Annual |
| --- | --- | --- | ---: | ---: |
| VM1 | B2s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $218.47 | $2,621.62 |
| VM2 | D2ds v7 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $337.02 | $4,044.24 |
| VM3 | E2ds v7 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $370.60 | $4,447.20 |
| VM4 | B4s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $288.11 | $3,457.32 |
| VM5 | B4s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $288.11 | $3,457.32 |
| VM6 | D2ds v7 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $337.02 | $4,044.24 |
| VM7 | E2ds v7 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $370.60 | $4,447.20 |
| VM8 | B4s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $288.11 | $3,457.32 |
| VM9 | B4s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $288.11 | $3,457.32 |
| VM10 | B2s v2 | Sweden Central; Windows OS Only; Standard; PAYG; license included; 1 x P30 Premium SSD LRS; 0 GB transfer | $218.47 | $2,621.62 |
| **Total** |  |  | **$3,004.62** | **$36,055.39** |

The quote name at completion was `Workload 2 EC2 VM`. Calculator-wide settings were USD, Basic support, Microsoft Customer Agreement (MCA), and Dev/Test pricing off. The $3,004.62/month total reconciles before display rounding to $1,517.816 of VM compute and included Windows licensing plus $1,486.80 for ten P30 disks. Bandwidth contributes $0.00.

The Calculator computes the $36,055.39 annual total from unrounded monthly values: `$3,004.616 x 12 = $36,055.392`. Summing the individually rounded annual line displays produces $36,055.40, a one-cent presentation difference; the Calculator's aggregate total is recorded as authoritative.

This estimate remains subject to the assumptions and qualifications above. The Calculator page was left open and was not saved or shared by automation.

## Sources

- https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/bsv2-series
- https://learn.microsoft.com/azure/virtual-machines/sizes/general-purpose/ddsv7-series
- https://learn.microsoft.com/azure/virtual-machines/sizes/memory-optimized/edsv7-series
- https://learn.microsoft.com/azure/virtual-machines/disks-types#premium-ssds
- https://prices.azure.com/api/retail/prices
- https://azure.microsoft.com/pricing/calculator/
- Azure Retail Prices results retrieved through the Azure Pricing tool on 2026-08-24.