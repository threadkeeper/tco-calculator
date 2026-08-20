# Live Foundry Evaluation Result

- Status: `passed`
- Case: `rds/rds-01`
- Evaluated UTC: `2026-08-19T15:44:00.5210803Z`
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
    "actual": "db.m6i.2xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "db.m6i.2xlarge",
    "passed": true
  },
  {
    "actual": "multi_az",
    "assertion": "/resources/0/deployment",
    "expected": "multi_az",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new RDS project draft: action=open_project_draft, report_recorded=true, staged=true, status=ok.\n\nOmissions:\n- Source: Amazon RDS (visible but no corresponding project field)\n- Engine: SQL Server (visible but no corresponding project field)\n- Allocated storage: 1,024 GiB (visible but no corresponding field; sql_data_gb_per_instance exists for SQL data only)\n\nUncertainties: []\n\nThis is an unsaved draft that requires your review; no changes have been persisted.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "RDS for SQL Server - deployment summary",
      "Source: Amazon RDS",
      "DB instance class: db.m6i.2xlarge",
      "DB identifier: synthetic-orders-rds",
      "Deployment: Multi-AZ",
      "Storage class: gp3"
    ],
    "project_type": "rds"
  },
  "omissions": [
    "Source: Amazon RDS (visible but no corresponding project field)",
    "Engine: SQL Server (visible but no corresponding project field)",
    "Allocated storage: 1,024 GiB (visible but no corresponding field; sql_data_gb_per_instance exists for SQL data only)"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": null,
      "name": "rds/rds-01",
      "resources": [
        {
          "annual_hours_per_instance": "8760",
          "commercial_term": "On-Demand",
          "deployment": "multi_az",
          "id": "<generated-uuid-1>",
          "instance_type": "db.m6i.2xlarge",
          "license_basis": "license_included",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 12000,
          "source_ram_gb_per_instance": "32",
          "source_type": "rds",
          "sql_data_gb_per_instance": "760",
          "sql_edition": "standard",
          "storage_class": "gp3",
          "workload_name": "synthetic-orders-rds"
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
  "uncertainties": []
}
```
