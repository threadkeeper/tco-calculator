# Live Foundry Evaluation Result

- Status: `passed`
- Case: `sql_payg/sql-payg-01`
- Evaluated UTC: `2026-08-19T15:47:50.9082667Z`
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
    "actual": 32,
    "assertion": "/settings/sql_payg/enterprise_licensed_cores",
    "expected": 32,
    "passed": true
  },
  {
    "actual": 48,
    "assertion": "/settings/sql_payg/standard_licensed_cores",
    "expected": 48,
    "passed": true
  },
  {
    "actual": "72000",
    "assertion": "/settings/sql_payg/software_assurance_annual_usd",
    "expected": "72000",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new unsaved project draft (project_type: sql_payg). Action: open_project_draft, staged: true, status: ok.\n\nExtracted fields:\n- settings.sql_payg.enterprise_licensed_cores: 32\n- settings.sql_payg.software_assurance_annual_usd: \"72000\"\n- settings.sql_payg.standard_licensed_cores: 48\n\nReported omissions:\n- Licensed core inventory — PAYG usage hours: '6,000 per year' (Enterprise Edition row)\n- Licensed core inventory — PAYG usage hours: '6,000 per year' (Standard Edition row)\n- Licensed core inventory — Payment comparison: 'Hourly per-core PAYG' (Enterprise Edition row)\n- Licensed core inventory — Payment comparison: 'Hourly per-core PAYG' (Standard Edition row)\n- Existing agreement input — Comparison currency: 'USD'\n- Existing agreement input — Coverage basis: 'Enterprise and Standard cores'\n- Existing agreement input — Source: 'Synthetic licensing worksheet'\n- Visible UI elements: 'SQL Server enabled by Azure Arc' button, 'License type: PAYG' button, 'Period: annual' button\n- Top-right identifier: 'sql_payg/sql-payg-01'\n\nUncertainties: []\n\nThis is an unsaved draft and requires your review. Persisting (saving) or making further changes requires an explicit confirmation action.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Azure Arc-enabled SQL Server PAYG",
      "License type: PAYG",
      "Enterprise Edition",
      "Standard Edition",
      "Licensed cores: 32, 48",
      "Software Assurance annual spend USD 72,000"
    ],
    "project_type": "sql_payg"
  },
  "omissions": [
    "Licensed core inventory — PAYG usage hours: '6,000 per year' (Enterprise Edition row)",
    "Licensed core inventory — PAYG usage hours: '6,000 per year' (Standard Edition row)",
    "Licensed core inventory — Payment comparison: 'Hourly per-core PAYG' (Enterprise Edition row)",
    "Licensed core inventory — Payment comparison: 'Hourly per-core PAYG' (Standard Edition row)",
    "Existing agreement input — Comparison currency: 'USD'",
    "Existing agreement input — Coverage basis: 'Enterprise and Standard cores'",
    "Existing agreement input — Source: 'Synthetic licensing worksheet'",
    "Visible UI elements: 'SQL Server enabled by Azure Arc' button, 'License type: PAYG' button, 'Period: annual' button",
    "Top-right identifier: 'sql_payg/sql-payg-01'"
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
          "enterprise_licensed_cores": 32,
          "software_assurance_annual_usd": "72000",
          "standard_licensed_cores": 48
        },
        "standard_license_sa_usd_per_two_core_pack": null
      }
    },
    "proposal_id": "<generated-proposal-id>"
  },
  "uncertainties": []
}
```
