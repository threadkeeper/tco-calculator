# Live Foundry Evaluation Result

- Status: `passed`
- Case: `sql_payg/sql-payg-03`
- Evaluated UTC: `2026-08-19T15:49:05.1131799Z`
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
    "actual": 8,
    "assertion": "/settings/sql_payg/enterprise_licensed_cores",
    "expected": 8,
    "passed": true
  },
  {
    "actual": 24,
    "assertion": "/settings/sql_payg/standard_licensed_cores",
    "expected": 24,
    "passed": true
  },
  {
    "actual": "18500",
    "assertion": "/settings/sql_payg/software_assurance_annual_usd",
    "expected": "18500",
    "passed": true
  }
]
```

## Complete Sanitized Response

```json
{
  "answer": "Staged new project draft: open_project_draft (staged: true)\n\nProject type: sql_payg\n\nsettings.sql_payg:\n- enterprise_licensed_cores: 8\n- software_assurance_annual_usd: \"18500\"\n- standard_licensed_cores: 24\n\nOmissions:\n- PAYG usage hours: '2,920 per year' (visible) — not supported for sql_payg settings\n- Service: 'SQL Server enabled by Azure Arc' (visible) — not mapped to project fields\n- Payment option: 'Pay-as-you-go (PAYG)' (visible) — not mapped to project fields\n- Comparison: 'PAYG versus annual SA' (visible) — not mapped to project fields\n- Data quality: 'Synthetic OCR-like alias' (visible) — not mapped to project fields\n- Top-right identifier: 'sql_payg/sql-payg-03' (visible) — not mapped to project fields\n- Reviewer context column values (e.g., 'Enterprise Edition', 'Standard Edition row; OCR text reads STE', 'Software Assurance input', 'Hourly consumption comparison') are visible but not mappable to sql_payg settings\n\nUncertainties:\n- The abbreviation 'STE' is present; reviewer notes indicate it is intentionally noisy and likely maps to Standard Edition.\n- SA annual spend displayed with currency 'USD 18,500' was normalized to '18500'.\n- '2,920 per year' hours are visible but the sql_payg schema does not include an annual hours field; it was not imported.\n\nEvery staged result requires review; no changes were persisted. Explicit confirmation is required to save or publish this draft.",
  "classification": {
    "ambiguities": [],
    "confidence": "high",
    "evidence": [
      "Azure Arc SQL PAYG - scanned comparison extract",
      "Pay-as-you-go (PAYG)",
      "SQL Server enabled by Azure Arc",
      "EE core count",
      "SA annual spend",
      "PAYG usage hours"
    ],
    "project_type": "sql_payg"
  },
  "omissions": [
    "PAYG usage hours: '2,920 per year' (visible) — not supported for sql_payg settings",
    "Service: 'SQL Server enabled by Azure Arc' (visible) — not mapped to project fields",
    "Payment option: 'Pay-as-you-go (PAYG)' (visible) — not mapped to project fields",
    "Comparison: 'PAYG versus annual SA' (visible) — not mapped to project fields",
    "Data quality: 'Synthetic OCR-like alias' (visible) — not mapped to project fields",
    "Top-right identifier: 'sql_payg/sql-payg-03' (visible) — not mapped to project fields",
    "Reviewer context column values (e.g., 'Enterprise Edition', 'Standard Edition row; OCR text reads STE', 'Software Assurance input', 'Hourly consumption comparison') are visible but not mappable to sql_payg settings"
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
          "enterprise_licensed_cores": 8,
          "software_assurance_annual_usd": "18500",
          "standard_licensed_cores": 24
        },
        "standard_license_sa_usd_per_two_core_pack": null
      }
    },
    "proposal_id": "<generated-proposal-id>"
  },
  "uncertainties": [
    "The abbreviation 'STE' is present; reviewer notes indicate it is intentionally noisy and likely maps to Standard Edition.",
    "SA annual spend displayed with currency 'USD 18,500' was normalized to '18500'.",
    "'2,920 per year' hours are visible but the sql_payg schema does not include an annual hours field; it was not imported."
  ]
}
```
