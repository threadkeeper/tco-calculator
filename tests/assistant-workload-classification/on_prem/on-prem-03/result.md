# Live Foundry Evaluation Result

- Status: `passed`
- Case: `on_prem/on-prem-03`
- Evaluated UTC: `2026-08-19T15:47:15.7043189Z`
- Evaluation identity: `system_assigned_managed_identity`
- Classifier prompt: `tco-assistant-image-classifier/1.2.0`
- Draft prompt: `tco-assistant-system/1.3.3`
- Expected family: `on_prem`
- Expected minimum confidence: `medium`
- Observed family: `on_prem`
- Observed confidence: `high`

## Assertions

```json
[
  {
    "actual": "on_prem",
    "assertion": "classification.project_type",
    "expected": "on_prem",
    "passed": true
  },
  {
    "actual": "high",
    "assertion": "classification.minimum_confidence",
    "expected": "medium",
    "passed": true
  },
  {
    "actual": "on_prem",
    "assertion": "/settings/project_type",
    "expected": "on_prem",
    "passed": true
  },
  {
    "actual": "on_prem",
    "assertion": "/resources/0/source_type",
    "expected": "on_prem",
    "passed": true
  },
  {
    "actual": 24,
    "assertion": "/resources/0/licensable_cores",
    "expected": 24,
    "passed": true
  },
  {
    "actual": "enterprise",
    "assertion": "/resources/0/sql_edition",
    "expected": "enterprise",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new on_prem project draft (action: open_project_draft, staged: true, report_recorded: true). This is an unsaved browser draft and requires your review; no changes have been persisted.\n\nStaged resources:\n- resource 1:\n  - source_type: on_prem\n  - workload_name: SQL Server 2019\n  - sql_edition: enterprise\n  - license_basis: byol\n  - annual_hours_per_instance: 8760\n  - source_vcpu: 24\n  - licensable_cores: 24\n  - source_ram_gb_per_instance: 256\n  - sql_data_gb_per_instance: 4800\n  - source_max_iops: 22000\n\nOmissions:\n- \"CPU sockets (2) - visible but no direct on_prem field\"\n- \"Cores per socket (12) - visible but no direct on_prem field\"\n- \"Local storage - Disk role: 'Data and log array' - visible but no direct on_prem field\"\n- \"Local storage - Capacity: 6,144 GiB - visible but no direct on_prem field\"\n\nUncertainties:\n- \"source_vcpu (24) inferred by multiplying CPU sockets (2) and Cores per socket (12).\"\n\nEvery staged result requires review. To persist this draft you must explicitly confirm saving or further actions.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "on_prem/on-prem-03",
      "Physical host",
      "CPU sockets 2",
      "Licensable cores 24",
      "Local storage 6,144 GiB",
      "This inventory has no Amazon EC2, Amazon RDS, Azure Arc, PAYG"
    ],
    "project_type": "on_prem"
  },
  "omissions": [
    "CPU sockets (2) - visible but no direct on_prem field",
    "Cores per socket (12) - visible but no direct on_prem field",
    "Local storage - Disk role: 'Data and log array' - visible but no direct on_prem field",
    "Local storage - Capacity: 6,144 GiB - visible but no direct on_prem field"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": null,
      "name": "SQL TCO estimate",
      "resources": [
        {
          "annual_hours_per_instance": "8760",
          "average_power_kw_override": null,
          "depreciation_years": "5",
          "hardware_capex_usd": "0",
          "id": "<generated-uuid-1>",
          "licensable_cores": 24,
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 22000,
          "source_ram_gb_per_instance": "256",
          "source_type": "on_prem",
          "source_vcpu": 24,
          "sql_data_gb_per_instance": "4800",
          "sql_edition": "enterprise",
          "workload_name": "SQL Server 2019"
        }
      ],
      "settings": {
        "aws_region": null,
        "azure_compute_discount": "0",
        "azure_license_discount": "0",
        "azure_region": "swedencentral",
        "azure_storage_discount": "0",
        "currency": "USD",
        "default_annual_hours": "8760",
        "default_mi_purchase_option": "ahb",
        "electricity_rate_usd_per_kwh": "0",
        "enterprise_license_sa_usd_per_two_core_pack": "20557",
        "project_type": "on_prem",
        "remaining_coverage_months": 12,
        "selected_parity_adjustment": "0",
        "source_compute_discount": "0",
        "source_license_discount": "0",
        "source_storage_discount": "0",
        "sql_payg": null,
        "standard_license_sa_usd_per_two_core_pack": "5363"
      }
    },
    "proposal_id": "<generated-proposal-id>"
  },
  "uncertainties": [
    "source_vcpu (24) inferred by multiplying CPU sockets (2) and Cores per socket (12).",
    "Missing License + SA pack prices use the reviewed SQL Server 2022 public-book reference verified 2026-08-07."
  ]
}
```
