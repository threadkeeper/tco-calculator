# Live Foundry Evaluation Result

- Status: `passed`
- Case: `ec2/ec2-04`
- Evaluated UTC: `2026-08-21T13:10:00Z`
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
    "actual": "r6i.8xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "r6i.8xlarge",
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
  "answer": "Staged a new unsaved EC2 project draft for VM6. Review is required before saving.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "EC2",
      "r6i.8xlarge",
      "VM6",
      "Windows with SQL Server Standard",
      "1 TB EBS Storage",
      "Licence Included"
    ],
    "project_type": "ec2"
  },
  "omissions": [
    "Windows is visible but is not a supported EC2 resource field.",
    "EBS is visible without a supported gp3, io2, or ephemeral volume type.",
    "Standard RAM is visible without a numeric RAM value.",
    "Quantity and annual hours are not visible."
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
          "id": "<generated-uuid-1>",
          "instance_type": "r6i.8xlarge",
          "license_basis": "license_included",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_ram_gb_per_instance": "256",
          "source_type": "ec2",
          "sql_data_gb_per_instance": "1024",
          "sql_edition": "standard",
          "volumes": [
            {
              "aws_volume_id": null,
              "capacity_gb": "1",
              "id": "<generated-uuid-2>",
              "label": "EBS Storage",
              "provisioned_iops": null,
              "throughput_mibps": null,
              "volume_type": "ephemeral"
            }
          ],
          "workload_name": "VM6"
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
  "uncertainties": [
    "The visible 1 TB value was extracted as 1 without converting units.",
    "Standard RAM does not provide a numeric source RAM value."
  ]
}
```
