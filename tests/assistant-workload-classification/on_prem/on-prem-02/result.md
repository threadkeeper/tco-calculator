# Live Foundry Evaluation Result

- Status: `passed`
- Case: `on_prem/on-prem-02`
- Evaluated UTC: `2026-08-19T15:46:29.0454728Z`
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
    "actual": 16,
    "assertion": "/resources/0/source_vcpu",
    "expected": 16,
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
  "answer": "Staged new on_prem project draft: \"VMware SQL workload - assessment row\" (action: open_project_draft, report_recorded: true, staged: true).\n\nStaged resource (on_prem):\n- workload_name: \"Synthetic finance SQL VM\"\n- source_type: on_prem\n- source_vcpu: 16\n- licensable_cores: 16\n- source_ram_gb_per_instance: \"128\"\n- source_max_iops: 8000\n- sql_data_gb_per_instance: \"1180\"\n- sql_edition: \"enterprise\"\n- license_basis: \"byol\"\n- annual_hours_per_instance: \"8000\"\n- quantity: 1\n\nOmissions (visible but not mapped to on_prem fields or unsupported):\n- \"Server type: Virtual (visible but not a supported on_prem resource field)\"\n- \"Hypervisor: VMware (visible but not a supported on_prem resource field)\"\n- \"Source: private datacenter (visible but not a supported on_prem resource field)\"\n- \"Disk size: 1,536 GiB (visible but not a supported on_prem field; sql_data_gb_per_instance is supported)\"\n- \"CPU utilization: 42 percent (visible but not a supported on_prem resource field)\"\n- \"Memory utilization: 68 percent (visible but not a supported on_prem resource field)\"\n- \"Cloud service identifier: Not present (visible but not a supported on_prem resource field)\"\n- \"Top-right identifier 'on_prem/on-prem-02' (visible but not a supported on_prem resource field)\"\n\nUncertainties:\n- \"Both Disk size (1,536 GiB) and SQL data (1,180 GiB) are visible; unclear which value should be used as the canonical SQL data size for the on_prem resource.\"\n\nEvery staged result requires review. Nothing has been persisted; to save or apply changes you must explicitly confirm.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "VMware SQL workload - assessment row",
      "Server type: Virtual",
      "Hypervisor: VMware",
      "Source: private datacenter",
      "on_prem/on-prem-02"
    ],
    "project_type": "on_prem"
  },
  "omissions": [
    "Server type: Virtual (visible but not a supported on_prem resource field)",
    "Hypervisor: VMware (visible but not a supported on_prem resource field)",
    "Source: private datacenter (visible but not a supported on_prem resource field)",
    "Disk size: 1,536 GiB (visible but not a supported on_prem field; sql_data_gb_per_instance is supported)",
    "CPU utilization: 42 percent (visible but not a supported on_prem resource field)",
    "Memory utilization: 68 percent (visible but not a supported on_prem resource field)",
    "Cloud service identifier: Not present (visible but not a supported on_prem resource field)",
    "Top-right identifier 'on_prem/on-prem-02' (visible but not a supported on_prem resource field)"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": null,
      "name": "VMware SQL workload - assessment row",
      "resources": [
        {
          "annual_hours_per_instance": "8000",
          "average_power_kw_override": null,
          "depreciation_years": "5",
          "hardware_capex_usd": "0",
          "id": "<generated-uuid-1>",
          "licensable_cores": 16,
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 8000,
          "source_ram_gb_per_instance": "128",
          "source_type": "on_prem",
          "source_vcpu": 16,
          "sql_data_gb_per_instance": "1180",
          "sql_edition": "enterprise",
          "workload_name": "Synthetic finance SQL VM"
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
    "Both Disk size (1,536 GiB) and SQL data (1,180 GiB) are visible; unclear which value should be used as the canonical SQL data size for the on_prem resource.",
    "Missing License + SA pack prices use the reviewed SQL Server 2022 public-book reference verified 2026-08-07."
  ]
}
```
