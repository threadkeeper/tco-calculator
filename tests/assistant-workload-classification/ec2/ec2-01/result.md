# Live Foundry Evaluation Result

- Status: `passed`
- Case: `ec2/ec2-01`
- Evaluated UTC: `2026-08-19T15:41:37.0392282Z`
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
    "actual": "m7i.4xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "m7i.4xlarge",
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
  "answer": "Staged new EC2 project draft (action: open_project_draft; report_recorded: true; staged: true). The draft requires review and has not been saved.\n\nOmissions:\n- Service: Amazon EC2\n- Instance ID: i-0123456789abcdef0\n- Operating system: Windows Server 2022\n- Assessment state: ready\n\nUncertainties:\n- Ambiguity splitting 'vol-0abc1234def567890 / SQL data' into aws_volume_id and label\n- No explicit mapping for 'Service' and 'Assessment state' so they were reported as omissions\n\nPersisting this draft requires explicit confirmation.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Source: Amazon EC2",
      "Service: Amazon EC2",
      "Instance ID i-0123456789abcdef0",
      "Instance type m7i.4xlarge",
      "EBS gp3"
    ],
    "project_type": "ec2"
  },
  "omissions": [
    "Service: Amazon EC2",
    "Instance ID: i-0123456789abcdef0",
    "Operating system: Windows Server 2022",
    "Assessment state: ready"
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
          "instance_type": "m7i.4xlarge",
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_ram_gb_per_instance": "64",
          "source_type": "ec2",
          "sql_data_gb_per_instance": "1536",
          "sql_edition": "standard",
          "volumes": [
            {
              "aws_volume_id": "vol-0abc1234def567890",
              "capacity_gb": "2048",
              "id": "<generated-uuid-2>",
              "label": "SQL data",
              "provisioned_iops": 6000,
              "throughput_mibps": "250",
              "volume_type": "gp3"
            }
          ],
          "workload_name": "Synthetic order processing"
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
    "Ambiguity splitting 'vol-0abc1234def567890 / SQL data' into aws_volume_id and label",
    "No explicit mapping for 'Service' and 'Assessment state' so they were reported as omissions"
  ]
}
```
