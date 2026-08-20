# Live Foundry Evaluation Result

- Status: `passed`
- Case: `sql_payg/sql-payg-02`
- Evaluated UTC: `2026-08-19T15:48:31.3223467Z`
- Evaluation identity: `system_assigned_managed_identity`
- Classifier prompt: `tco-assistant-image-classifier/1.2.0`
- Draft prompt: `tco-assistant-system/1.3.3`
- Expected family: `sql_payg`
- Expected minimum confidence: `medium`
- Observed family: `sql_payg`
- Observed confidence: `high`

## Assertions

```json
[
  {
    "actual": "sql_payg",
    "assertion": "classification.project_type",
    "expected": "sql_payg",
    "passed": true
  },
  {
    "actual": "high",
    "assertion": "classification.minimum_confidence",
    "expected": "medium",
    "passed": true
  },
  {
    "actual": "sql_payg",
    "assertion": "/settings/project_type",
    "expected": "sql_payg",
    "passed": true
  },
  {
    "actual": 16,
    "assertion": "/settings/sql_payg/enterprise_licensed_cores",
    "expected": 16,
    "passed": true
  },
  {
    "actual": 64,
    "assertion": "/settings/sql_payg/standard_licensed_cores",
    "expected": 64,
    "passed": true
  },
  {
    "actual": "50000",
    "assertion": "/settings/sql_payg/software_assurance_annual_usd",
    "expected": "50000",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged a new unsaved project draft (project_type: sql_payg).\n\nStaging result: {\"action\":\"open_project_draft\",\"report_recorded\":true,\"staged\":true,\"status\":\"ok\"}\n\nStaged settings.sql_payg:\n- enterprise_licensed_cores: 16\n- standard_licensed_cores: 64\n- software_assurance_annual_usd: \"50000\"\n\nOmissions:\n- \"Average active hours: 4,380 per year (visible but not a settings.sql_payg field)\"\n- \"Meter: Pay-as-you-go hourly per core (visible but not a settings.sql_payg field)\"\n- \"EE means: Enterprise Edition (visible explanatory label, unsupported)\"\n- \"SE means: Standard Edition (visible explanatory label, unsupported)\"\n- \"SA means: Software Assurance (visible explanatory label, unsupported)\"\n\nUncertainties: []\n\nThis is an unsaved draft and requires your review. Persisting changes requires explicit confirmation.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Azure Arc SQL PAYG",
      "EE licensed cores 16",
      "SE licensed cores 64",
      "Average active hours 4,380 per year",
      "SA annual renewal USD 50,000",
      "Pay-as-you-go hourly per core"
    ],
    "project_type": "sql_payg"
  },
  "omissions": [
    "Average active hours: 4,380 per year (visible but not a settings.sql_payg field)",
    "Meter: Pay-as-you-go hourly per core (visible but not a settings.sql_payg field)",
    "EE means: Enterprise Edition (visible explanatory label, unsupported)",
    "SE means: Standard Edition (visible explanatory label, unsupported)",
    "SA means: Software Assurance (visible explanatory label, unsupported)"
  ],
  "proposal": {
    "action": "open_project_draft",
    "project": {
      "aws_price_snapshot_id": null,
      "azure_price_snapshot_id": null,
      "description": null,
      "name": "SQL TCO estimate",
      "resources": [],
      "settings": {
        "aws_region": null,
        "azure_compute_discount": "0",
        "azure_license_discount": "0",
        "azure_region": "swedencentral",
        "azure_storage_discount": "0",
        "currency": "USD",
        "default_annual_hours": "8760",
        "default_mi_purchase_option": "ahb",
        "electricity_rate_usd_per_kwh": null,
        "enterprise_license_sa_usd_per_two_core_pack": null,
        "project_type": "sql_payg",
        "remaining_coverage_months": null,
        "selected_parity_adjustment": "0",
        "source_compute_discount": "0",
        "source_license_discount": "0",
        "source_storage_discount": "0",
        "sql_payg": {
          "enterprise_licensed_cores": 16,
          "software_assurance_annual_usd": "50000",
          "standard_licensed_cores": 64
        },
        "standard_license_sa_usd_per_two_core_pack": null
      }
    },
    "proposal_id": "<generated-proposal-id>"
  },
  "uncertainties": []
}
```
