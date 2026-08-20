# Live Foundry Evaluation Result

- Status: `passed`
- Case: `rds/rds-03`
- Evaluated UTC: `2026-08-19T15:45:12.713433Z`
- Evaluation identity: `system_assigned_managed_identity`
- Classifier prompt: `tco-assistant-image-classifier/1.2.0`
- Draft prompt: `tco-assistant-system/1.3.3`
- Expected family: `rds`
- Expected minimum confidence: `medium`
- Observed family: `rds`
- Observed confidence: `high`

## Assertions

```json
[
  {
    "actual": "rds",
    "assertion": "classification.project_type",
    "expected": "rds",
    "passed": true
  },
  {
    "actual": "high",
    "assertion": "classification.minimum_confidence",
    "expected": "medium",
    "passed": true
  },
  {
    "actual": "rds",
    "assertion": "/settings/project_type",
    "expected": "rds",
    "passed": true
  },
  {
    "actual": "rds",
    "assertion": "/resources/0/source_type",
    "expected": "rds",
    "passed": true
  },
  {
    "actual": "db.m5.xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "db.m5.xlarge",
    "passed": true
  },
  {
    "actual": 2,
    "assertion": "/resources/0/quantity",
    "expected": 2,
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new RDS project draft (unsaved).\n\nAction result:\n- action: open_project_draft\n- report_recorded: true\n- staged: true\n\nStaged draft (as recorded):\n- project_type: rds\n- resources:\n  - {\n    source_type: \"rds\",\n    instance_type: \"db.m5.xlarge\",\n    quantity: 2,\n    deployment: \"multi_az\",\n    annual_hours_per_instance: \"8760\",\n    storage_class: \"gp3\",\n    source_max_iops: 3000,\n    source_ram_gb_per_instance: \"16\",\n    sql_data_gb_per_instance: \"420\",\n    license_basis: \"license_included\",\n    commercial_term: \"On-Demand\",\n    sql_edition: \"standard\"\n    }\n- settings:\n  - aws_region: \"ap-southeast-2\"\n\nReported omissions:\n- [\"Capacity: 512 GiB per DB instance (visible in image but there is no corresponding RDS resource field for total capacity in the project schema)\"]\n\nReported uncertainties:\n- []\n\nNote: This is an unsaved draft and requires your review. Persisting changes requires explicit confirmation.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "RDS database estate - repeated deployment",
      "Amazon RDS inventory",
      "DB instance class db.m5.xlarge",
      "Multi-AZ enabled",
      "RDS storage type gp3",
      "License model License included"
    ],
    "project_type": "rds"
  },
  "omissions": [
    "Capacity: 512 GiB per DB instance (visible in image but there is no corresponding RDS resource field for total capacity in the project schema)"
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
          "commercial_term": "On-Demand",
          "deployment": "multi_az",
          "id": "<generated-uuid-1>",
          "instance_type": "db.m5.xlarge",
          "license_basis": "license_included",
          "mi_purchase_option": "ahb",
          "quantity": 2,
          "server_name": null,
          "source_max_iops": 3000,
          "source_ram_gb_per_instance": "16",
          "source_type": "rds",
          "sql_data_gb_per_instance": "420",
          "sql_edition": "standard",
          "storage_class": "gp3",
          "workload_name": "SQL workload"
        }
      ],
      "settings": {
        "aws_region": "ap-southeast-2",
        "azure_compute_discount": "0",
        "azure_license_discount": "0",
        "azure_region": "swedencentral",
        "azure_storage_discount": "0",
        "currency": "USD",
        "default_annual_hours": "8760",
        "default_mi_purchase_option": "ahb",
        "electricity_rate_usd_per_kwh": null,
        "enterprise_license_sa_usd_per_two_core_pack": null,
        "project_type": "rds",
        "remaining_coverage_months": null,
        "selected_parity_adjustment": "0",
        "source_compute_discount": "0",
        "source_license_discount": "0",
        "source_storage_discount": "0",
        "sql_payg": null,
        "standard_license_sa_usd_per_two_core_pack": null
      }
    },
    "proposal_id": "<generated-proposal-id>"
  },
  "uncertainties": []
}
```
