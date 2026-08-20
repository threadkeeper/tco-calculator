# Live Foundry Evaluation Result

- Status: `passed`
- Case: `ec2/ec2-02`
- Evaluated UTC: `2026-08-19T15:42:26.4057223Z`
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
    "actual": "r6i.2xlarge",
    "assertion": "/resources/0/instance_type",
    "expected": "r6i.2xlarge",
    "passed": true
  },
  {
    "actual": "io2",
    "assertion": "/resources/0/volumes/0/volume_type",
    "expected": "io2",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new EC2 project draft (unsaved).\nAction result: open_project_draft — staged: true.\n\nStaged project (exactly as recorded):\n- project_type: ec2\n- settings.aws_region: us-east-2\n- resources (1):\n  - workload_name: Synthetic reporting warehouse\n  - source_type: ec2\n  - instance_type: r6i.2xlarge\n  - license_basis: byol\n  - annual_hours_per_instance: \"6240\"\n  - quantity: 1\n  - source_ram_gb_per_instance: \"64\"\n  - sql_data_gb_per_instance: \"880\"\n  - sql_edition: enterprise\n  - volumes (1):\n    - label: SQL data and log\n    - aws_volume_id: vol-02223333444455556\n    - volume_type: io2\n    - capacity_gb: \"1024\"\n    - provisioned_iops: 20000\n    - throughput_mibps: \"500\"\n\nReported omissions:\n- \"AMI (ami-0abc1234def567890) is visible but AMI/image is not a supported EC2 resource field\"\n- \"vCPU (8) is visible but source_vcpu or vCPU is not a supported EC2 resource field\"\n\nReported uncertainties:\n- \"Image shows both 'SQL data in use: 880 GiB' and attached volume capacity '1,024 GiB' — uncertain which value should be treated as the canonical sql_data_gb_per_instance\"\n- \"Observed 'Storage throughput: 500 MiB/s' is listed under Observed values — uncertain whether this throughput applies specifically to the listed EBS volume or to overall storage\"\n\nThis is an unsaved draft and requires your review. Explicit confirmation is required to persist changes.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Platform: Amazon EC2",
      "EC2 instance type: r6i.2xlarge",
      "AMI: ami-0abc1234def567890",
      "Volume ID: vol-02223333444455556",
      "EBS type: io2",
      "Region: us-east-2"
    ],
    "project_type": "ec2"
  },
  "omissions": [
    "AMI (ami-0abc1234def567890) is visible but AMI/image is not a supported EC2 resource field",
    "vCPU (8) is visible but source_vcpu or vCPU is not a supported EC2 resource field"
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
          "annual_hours_per_instance": "6240",
          "id": "<generated-uuid-1>",
          "instance_type": "r6i.2xlarge",
          "license_basis": "byol",
          "mi_purchase_option": "ahb",
          "quantity": 1,
          "server_name": null,
          "source_ram_gb_per_instance": "64",
          "source_type": "ec2",
          "sql_data_gb_per_instance": "880",
          "sql_edition": "enterprise",
          "volumes": [
            {
              "aws_volume_id": "vol-02223333444455556",
              "capacity_gb": "1024",
              "id": "<generated-uuid-2>",
              "label": "SQL data and log",
              "provisioned_iops": 20000,
              "throughput_mibps": "500",
              "volume_type": "io2"
            }
          ],
          "workload_name": "Synthetic reporting warehouse"
        }
      ],
      "settings": {
        "aws_region": "us-east-2",
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
    "Image shows both 'SQL data in use: 880 GiB' and attached volume capacity '1,024 GiB' — uncertain which value should be treated as the canonical sql_data_gb_per_instance",
    "Observed 'Storage throughput: 500 MiB/s' is listed under Observed values — uncertain whether this throughput applies specifically to the listed EBS volume or to overall storage"
  ]
}
```
