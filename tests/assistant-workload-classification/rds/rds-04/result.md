# Live Foundry Evaluation Result

- Status: `passed`
- Case: `rds/rds-04`
- Evaluated UTC: `2026-08-21T13:14:00Z`
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
    "actual": "db.r5.xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "db.r5.xlarge",
    "passed": true
  },
  {
    "actual": "standard",
    "assertion": "/resources/0/sql_edition",
    "expected": "standard",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged a new unsaved RDS project draft for Instance1. Review is required before saving.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "RDS",
      "db.r5.xlarge",
      "SQL Server Standard Edition",
      "32 GB RAM",
      "2 TB gp3 Storage",
      "Licence Included"
    ],
    "project_type": "rds"
  },
  "omissions": [],
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
          "commercial_term": "on-demand",
          "deployment": "single_az",
          "id": "<generated-uuid-1>",
          "instance_type": "db.r5.xlarge",
          "license_basis": "license_included",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 0,
          "source_ram_gb_per_instance": "32",
          "source_type": "rds",
          "sql_data_gb_per_instance": "2",
          "sql_edition": "standard",
          "storage_class": "gp3",
          "workload_name": "Instance1"
        }
      ],
      "settings": {
        "aws_region": "eu-west-1",
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
  "uncertainties": [
    "The visible 2 TB value was extracted as 2 without converting units.",
    "The visible 32 GB RAM value was extracted as 32 without converting units."
  ]
}
```
