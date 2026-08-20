# Live Foundry Evaluation Result

- Status: `passed`
- Case: `on_prem/on-prem-01`
- Evaluated UTC: `2026-08-19T15:45:49.4418318Z`
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
    "assertion": "/resources/0/source_vcpu",
    "expected": 24,
    "passed": true
  },
  {
    "actual": 24,
    "assertion": "/resources/0/licensable_cores",
    "expected": 24,
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged a new on_prem project draft (action recorded: open_project_draft, staged: true). Review required; nothing has been saved permanently.\n\nStaged resource(s):\n- workload_name: \"Synthetic manufacturing database\"\n- source_type: \"on_prem\"\n- quantity: 1\n- source_vcpu: 24\n- licensable_cores: 24\n- source_ram_gb_per_instance: \"192\"\n- annual_hours_per_instance: \"8760\"\n- sql_data_gb_per_instance: \"2400\"\n- source_max_iops: 18000\n- average_power_kw_override: \"0.72\"\n- hardware_capex_usd: \"48000\"\n- depreciation_years: \"5\"\n- sql_edition: \"standard\"\n- license_basis: \"byol\"\n\nReported omissions:\n- \"Operating system: Windows Server 2019\"\n- \"Disk capacity: 4,096 GiB\"\n- \"Server type: Physical\"\n- \"Location: datacenter\"\n- \"Cloud provider: none\"\n\nUncertainties:\n- []\n\nNext steps: review the staged draft and explicitly confirm any changes to persist them (natural-language intent is not a confirmation).",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Server type: Physical",
      "Location: datacenter",
      "Cloud provider: none",
      "Operating system Windows Server 2019",
      "Processor cores 24",
      "No AWS, RDS, Azure Arc, or PAYG identifier is present"
    ],
    "project_type": "on_prem"
  },
  "omissions": [
    "Operating system: Windows Server 2019",
    "Disk capacity: 4,096 GiB",
    "Server type: Physical",
    "Location: datacenter",
    "Cloud provider: none"
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
          "average_power_kw_override": "0.72",
          "depreciation_years": "5",
          "hardware_capex_usd": "48000",
          "id": "<generated-uuid-1>",
          "licensable_cores": 24,
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 18000,
          "source_ram_gb_per_instance": "192",
          "source_type": "on_prem",
          "source_vcpu": 24,
          "sql_data_gb_per_instance": "2400",
          "sql_edition": "standard",
          "workload_name": "Synthetic manufacturing database"
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
    "Missing License + SA pack prices use the reviewed SQL Server 2022 public-book reference verified 2026-08-07."
  ]
}
```
