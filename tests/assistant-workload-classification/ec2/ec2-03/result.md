# Live Foundry Evaluation Result

- Status: `passed`
- Case: `ec2/ec2-03`
- Evaluated UTC: `2026-08-19T15:43:22.9047288Z`
- Evaluation identity: `system_assigned_managed_identity`
- Classifier prompt: `tco-assistant-image-classifier/1.2.0`
- Draft prompt: `tco-assistant-system/1.3.3`
- Expected family: `ec2`
- Expected minimum confidence: `medium`
- Observed family: `ec2`
- Observed confidence: `high`

## Assertions

```json
[
  {
    "actual": "ec2",
    "assertion": "classification.project_type",
    "expected": "ec2",
    "passed": true
  },
  {
    "actual": "high",
    "assertion": "classification.minimum_confidence",
    "expected": "medium",
    "passed": true
  },
  {
    "actual": "ec2",
    "assertion": "/settings/project_type",
    "expected": "ec2",
    "passed": true
  },
  {
    "actual": "ec2",
    "assertion": "/resources/0/source_type",
    "expected": "ec2",
    "passed": true
  },
  {
    "actual": "r6id.8xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "r6id.8xlarge",
    "passed": true
  },
  {
    "actual": 3,
    "assertion": "/resources/0/quantity",
    "expected": 3,
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new unsaved EC2 project draft (action recorded; draft opened).\n\nProject draft summary:\n- name: EC2 SQL fleet - consolidated workload row\n- description: Draft from uploaded EC2 fleet image: consolidated workload row\n- project_type: ec2\n- resources:\n  - workload_name: Synthetic transaction cluster\n    - source_type: ec2\n    - instance_type: r6id.8xlarge\n    - quantity: 3\n    - source_ram_gb_per_instance: \"256\"\n    - annual_hours_per_instance: \"8760\"\n    - license_basis: byol\n    - sql_edition: enterprise\n    - sql_data_gb_per_instance: \"3072\"\n    - volumes:\n      - label: SQL data\n        - volume_type: gp3\n        - capacity_gb: \"3072\"\n        - provisioned_iops: 12000\n        - throughput_mibps: \"500\"\n      - label: SQL log\n        - volume_type: io2\n        - capacity_gb: \"512\"\n        - provisioned_iops: 24000\n        - throughput_mibps: \"750\"\n- settings:\n  - aws_region: eu-central-1\n\nOmissions (visible values not supported or not mapped):\n- \"Service: Amazon EC2 (visible badge not a supported project field)\"\n- \"Inventory: 3 instances (visible badge not a direct supported field name)\"\n\nUncertainties:\n- []\n\nThis is an unsaved draft and requires your review. Persisting changes requires explicit confirmation.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Service: Amazon EC2",
      "EC2 instance type r6id.8xlarge",
      "Inventory: 3 instances",
      "Amazon EBS",
      "gp3",
      "io2"
    ],
    "project_type": "ec2"
  },
  "omissions": [
    "Service: Amazon EC2 (visible badge not a supported project field)",
    "Inventory: 3 instances (visible badge not a direct supported field name)"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": "Draft from uploaded EC2 fleet image: consolidated workload row",
      "name": "EC2 SQL fleet - consolidated workload row",
      "resources": [
        {
          "annual_hours_per_instance": "8760",
          "id": "<generated-uuid-1>",
          "instance_type": "r6id.8xlarge",
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 3,
          "server_name": null,
          "source_ram_gb_per_instance": "256",
          "source_type": "ec2",
          "sql_data_gb_per_instance": "3072",
          "sql_edition": "enterprise",
          "volumes": [
            {
              "aws_volume_id": null,
              "capacity_gb": "3072",
              "id": "<generated-uuid-2>",
              "label": "SQL data",
              "provisioned_iops": 12000,
              "throughput_mibps": "500",
              "volume_type": "gp3"
            },
            {
              "aws_volume_id": null,
              "capacity_gb": "512",
              "id": "<generated-uuid-3>",
              "label": "SQL log",
              "provisioned_iops": 24000,
              "throughput_mibps": "750",
              "volume_type": "io2"
            }
          ],
          "workload_name": "Synthetic transaction cluster"
        }
      ],
      "settings": {
        "aws_region": "eu-central-1",
        "azure_compute_discount": "0",
        "azure_license_discount": "0",
        "azure_region": "swedencentral",
        "azure_storage_discount": "0",
        "currency": "USD",
        "default_annual_hours": "8760",
        "default_mi_purchase_option": "ahb",
        "electricity_rate_usd_per_kwh": null,
        "enterprise_license_sa_usd_per_two_core_pack": null,
        "project_type": "ec2",
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
