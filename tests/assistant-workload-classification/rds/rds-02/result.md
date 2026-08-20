# Live Foundry Evaluation Result

- Status: `passed`
- Case: `rds/rds-02`
- Evaluated UTC: `2026-08-19T15:44:36.5805421Z`
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
    "actual": "db.r6i.4xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "db.r6i.4xlarge",
    "passed": true
  },
  {
    "actual": "single_az",
    "assertion": "/resources/0/deployment",
    "expected": "single_az",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new unsaved project draft (action: open_project_draft, report_recorded: true, staged: true, status: ok).\n\nStaged draft (exact submitted fields)\n- name: synthetic-analytics-rds\n- project_type: rds\n- resources:\n  - source_type: rds\n    workload_name: synthetic-analytics-rds\n    instance_type: db.r6i.4xlarge\n    license_basis: license_included\n    deployment: single_az\n    commercial_term: 1-year reserved\n    source_ram_gb_per_instance: \"128\"\n    annual_hours_per_instance: \"7200\"\n    storage_class: io1\n    sql_data_gb_per_instance: \"1600\"\n    source_max_iops: 30000\n    sql_edition: enterprise\n- settings:\n  - aws_region: us-west-2\n\nVisible omissions (reported)\n- DB instance status: available (visible but not supported by the RDS resource schema)\n- Capacity: 2,048 GiB (visible but no corresponding RDS resource field; SQL data in use is present)\n\nUncertainties: []\n\nThis is an unsaved draft that requires your review. No changes have been persisted; explicitly confirm to save or apply any edits.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Amazon RDS SQL Server - source configuration",
      "RDS for SQL Server",
      "DB instance class db.r6i.4xlarge",
      "DB identifier synthetic-analytics-rds",
      "RDS storage class io1",
      "DB instance status: available"
    ],
    "project_type": "rds"
  },
  "omissions": [
    "DB instance status: available (visible but not supported by the RDS resource schema)",
    "Capacity: 2,048 GiB (visible but no corresponding RDS resource field; SQL data in use is present)"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": null,
      "name": "synthetic-analytics-rds",
      "resources": [
        {
          "annual_hours_per_instance": "7200",
          "commercial_term": "1-year reserved",
          "deployment": "single_az",
          "id": "<generated-uuid-1>",
          "instance_type": "db.r6i.4xlarge",
          "license_basis": "license_included",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_max_iops": 30000,
          "source_ram_gb_per_instance": "128",
          "source_type": "rds",
          "sql_data_gb_per_instance": "1600",
          "sql_edition": "enterprise",
          "storage_class": "io1",
          "workload_name": "synthetic-analytics-rds"
        }
      ],
      "settings": {
        "aws_region": "us-west-2",
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
