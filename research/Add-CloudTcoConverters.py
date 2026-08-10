from __future__ import annotations

import argparse
import csv
import math
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

import pywintypes
import win32com.client as win32


ROOT = Path(__file__).resolve().parent
RDS_MAPPING_PATH = ROOT / "RDS_SQLMI_MAPPING.csv"
EC2_MAPPING_PATH = ROOT / "EC2_SQLMI_MAPPING.csv"
EC2_PATH = ROOT / "EC2.csv"
SQLMI_PATH = ROOT / "SQLMI.csv"
EXHIBIT_A_PATH = ROOT / "ExhibitA.png"
EXHIBIT_B_PATH = ROOT / "ExhibitB.png"
WORKBOOK_PATH = ROOT / "SQL TCO Calculator.xlsx"

MONTHLY_HOURS = 730.0
ANNUAL_HOURS = 8_760.0
EC2_AZURE_MIGRATION_REGION = "swedencentral"
EC2_AZURE_MIGRATION_REGION_LABEL = "Sweden Central"
RDS_AZURE_MIGRATION_REGION = EC2_AZURE_MIGRATION_REGION
RDS_AZURE_MIGRATION_REGION_LABEL = EC2_AZURE_MIGRATION_REGION_LABEL
EC2_DEFAULT_MI_SERVICE_TIER = "Next Generation General Purpose"
EC2_MI_SERVICE_TIERS = (
    EC2_DEFAULT_MI_SERVICE_TIER,
    "Business Critical",
)
NGGP_IOPS_PER_VCORE = 1_600
NGGP_MAX_IOPS = 80_000
NO_MI_MAPPING = "NO MAPPING"

NGGP_PREMIUM_MEMORY_OPTIONS = {
    4: (28, 32, 40, 48),
    6: (42, 48, 60, 72),
    8: (56, 64, 80, 96),
    10: (70, 80, 100, 120),
    12: (84, 96, 120, 144),
    16: (112, 128, 160, 192),
    20: (140, 160, 200, 240),
    24: (168, 192, 240, 288),
    32: (224, 256, 320, 384),
    40: (280, 320, 400, 480),
    48: (336, 384, 480),
    56: (392, 448),
    64: (448,),
    80: (560,),
    96: (560,),
    128: (560,),
}

MI_TIER_PRIORITY = {
    "Next Generation General Purpose": 0,
    "General Purpose": 1,
    "Business Critical": 2,
}
REQUIRED_MI_OPTIONS = {
    "payg",
    "ahb",
    "one-year",
    "ahbone-year",
    "three-year",
    "ahbthree-year",
    "sv-one-year",
    "ahbsv-one-year",
}
MI_PURCHASE_OPTIONS = (
    ("PAYG", "payg"),
    ("PAYG + Azure Hybrid Benefit", "ahb"),
    ("1-Year Reserved", "one-year"),
    ("1-Year Reserved + AHB", "ahbone-year"),
    ("3-Year Reserved", "three-year"),
    ("3-Year Reserved + AHB", "ahbthree-year"),
    ("1-Year Savings Plan", "sv-one-year"),
    ("1-Year Savings Plan + AHB", "ahbsv-one-year"),
)
DEFAULT_AZURE_COMPONENT_DISCOUNT = 0.0
DEFAULT_MI_PURCHASE_OPTION = "PAYG + Azure Hybrid Benefit"
EC2_PRIMED_WORKLOADS = (
    ("AWOMDSQLV101 (DEV, 77 DBs, no HA)", "r6id.2xlarge", 1, 3_227.22),
    ("AWOMPSQLV201 (PRD-STANDARD, 11 DBs, no HA)", "r6id.2xlarge", 1, 125.58),
    (
        "AWOMPSQLV101 + AWOMPSQLV104 (PROD-ENTERPRISE, 57 DBs, Multi-AZ)",
        "r6id.8xlarge",
        2,
        5_431.60,
    ),
    ("AWOMQSQLV101 (QA, 54 DBs, no HA)", "r6id.4xlarge", 1, 3_321.03),
)
EC2_PRIMED_EDITIONS = (
    "Standard",
    "Standard",
    "Enterprise",
    "Standard",
)
EC2_PRIMED_LICENSE_BASES = ("BYOL",) * len(EC2_PRIMED_WORKLOADS)
EC2_PRIMED_MEMORY_GB = (
    64.0,
    64.0,
    256.0,
    128.0,
)
EC2_PRIMED_EBS_PROFILES = (
    "AWOMDSQLV101",
    "AWOMPSQLV201",
    "AWOMPSQLV101",
    "AWOMQSQLV101",
)
EBS_VOLUME_DETAILS = (
    ("AWOMPSQLV101", "I", "vol-0f1e8ddf2c95dc6f2", "gp3", 600, 3_000, 300),
    ("AWOMPSQLV101", "H", "vol-0f82305cfdc606562", "gp3", 4_800, 6_000, 1_000),
    ("AWOMPSQLV101", "G", "vol-01de54a5e62197d35", "gp3", 200, 3_000, 125),
    ("AWOMPSQLV101", "F", "vol-09e47f0d27a3341f3", "gp3", 1_000, 4_000, 1_000),
    ("AWOMPSQLV101", "E", "vol-0da6717115b4de636", "gp3", 200, 3_000, 125),
    ("AWOMPSQLV101", "C", "vol-0f3f9377079cf91cb", "gp3", 100, 3_000, 125),
    ("AWOMPSQLV101", "T", "Ephemeral", "Ephemeral", 1_900, None, None),
    ("AWOMPSQLV201", "E", "vol-05e62b2e89eb2fa26", "gp3", 100, 3_000, 125),
    ("AWOMPSQLV201", "C", "vol-0ab304b0391c538da", "gp3", 100, 3_000, 125),
    ("AWOMPSQLV201", "G", "vol-0441475ea923b0549", "gp3", 120, 3_000, 125),
    ("AWOMPSQLV201", "F", "vol-089627343888e3a39", "gp3", 500, 3_000, 125),
    ("AWOMPSQLV201", "T", "Ephemeral", "Ephemeral", 474, None, None),
    ("AWOMQSQLV101", "G", "vol-0508270fe06cf1fa8", "gp3", 200, 3_000, 125),
    ("AWOMQSQLV101", "E", "vol-05c43aa2bc4aec394", "gp3", 200, 3_000, 125),
    ("AWOMQSQLV101", "F", "vol-05b1ba255eff7884b", "gp3", 3_700, 3_000, 125),
    ("AWOMQSQLV101", "C", "vol-0f94f6dd122675440", "gp3", 100, 3_000, 125),
    ("AWOMQSQLV101", "T", "Ephemeral", "Ephemeral", 900, None, None),
    ("AWOMDSQLV101", "F", "vol-01ca0d85da87a4f92", "gp3", 3_650, 3_000, 250),
    ("AWOMDSQLV101", "G", "vol-0ada69f0668e02d55", "gp3", 330, 3_000, 125),
    ("AWOMDSQLV101", "C", "vol-053d4634d4b03879d", "gp3", 100, 3_000, 125),
    ("AWOMDSQLV101", "E", "vol-02e7f1b633efd267c", "gp3", 200, 3_000, 125),
    ("AWOMDSQLV101", "T", "Ephemeral", "Ephemeral", 450, None, None),
)
EBS_DETAIL_FIRST_ROW = 7
EBS_DETAIL_LAST_ROW = EBS_DETAIL_FIRST_ROW + len(EBS_VOLUME_DETAILS) - 1
EBS_STORAGE_OPTIONS = (
    ("gp3", "gp3 - General Purpose SSD"),
    ("io2", "io2 - Provisioned IOPS SSD"),
)
EBS_REGIONAL_RATES = {
    "eu-central-1": {"gp3": (0.0952, 3_000, 0.0060, 0.0, 0.0, 125, 0.0476), "io2": (0.1490, 0, 0.0780, 0.0546, 0.03822, 0, 0.0)},
    "eu-central-2": {"gp3": (0.1142, 3_000, 0.0070, 0.0, 0.0, 125, 0.0571), "io2": (0.1790, 0, 0.0940, 0.0658, 0.04610, 0, 0.0)},
    "eu-north-1": {"gp3": (0.0836, 3_000, 0.0052, 0.0, 0.0, 125, 0.0418), "io2": (0.1311, 0, 0.0684, 0.04788, 0.033516, 0, 0.0)},
    "eu-south-1": {"gp3": (0.0924, 3_000, 0.0058, 0.0, 0.0, 125, 0.0462), "io2": (0.1449, 0, 0.0756, 0.0529, 0.0370, 0, 0.0)},
    "eu-south-2": {"gp3": (0.0880, 3_000, 0.0055, 0.0, 0.0, 125, 0.0440), "io2": (0.1380, 0, 0.0720, 0.0504, 0.0353, 0, 0.0)},
    "eu-west-1": {"gp3": (0.0880, 3_000, 0.0055, 0.0, 0.0, 125, 0.0440), "io2": (0.1380, 0, 0.0720, 0.0504, 0.03528, 0, 0.0)},
    "eu-west-2": {"gp3": (0.0928, 3_000, 0.0058, 0.0, 0.0, 125, 0.0464), "io2": (0.1450, 0, 0.0760, 0.0532, 0.03724, 0, 0.0)},
    "eu-west-3": {"gp3": (0.0928, 3_000, 0.0058, 0.0, 0.0, 125, 0.0464), "io2": (0.1450, 0, 0.0760, 0.0532, 0.0372, 0, 0.0)},
}

XL_AUTOMATIC = -4105
XL_CENTER = -4108
XL_LEFT = -4131
XL_LANDSCAPE = 2
XL_CONTINUOUS = 1
XL_THIN = 2
XL_VALIDATE_WHOLE_NUMBER = 1
XL_VALIDATE_DECIMAL = 2
XL_VALIDATE_LIST = 3
XL_VALID_ALERT_STOP = 1
XL_BETWEEN = 1
XL_CELL_TYPE_ALL_VALIDATION = -4174


def ole_color(hex_color: str) -> int:
    value = hex_color.lstrip("#")
    red = int(value[0:2], 16)
    green = int(value[2:4], 16)
    blue = int(value[4:6], 16)
    return red + green * 256 + blue * 65_536


NAVY = ole_color("17324D")
BLUE = ole_color("28666E")
AWS_ORANGE = ole_color("C45A18")
GREEN = ole_color("477A54")
PLUM = ole_color("76526E")
DARK_GRAY = ole_color("4F5963")
LIGHT_BLUE = ole_color("E7F0F5")
LIGHT_ORANGE = ole_color("FCE8D5")
LIGHT_GREEN = ole_color("E6F0E8")
LIGHT_PLUM = ole_color("F0E8EE")
LIGHT_GRAY = ole_color("F2F4F5")
YELLOW = ole_color("FFF2CC")
RED = ole_color("FCE4D6")
WHITE = ole_color("FFFFFF")
BORDER_COLOR = ole_color("B8C0C5")


def as_float(value: str | None, default: float = 0.0) -> float:
    if value is None or not value.strip():
        return default
    return float(value)


def as_bool(value: str | None) -> bool:
    return str(value).strip().lower() == "true"


def source_key(
    region: str,
    instance_type: str,
    deployment: str,
    term: str,
    lease: str,
    purchase: str,
) -> tuple[str, ...]:
    return region, instance_type, deployment, term, lease, purchase


def display_source_key(key: tuple[str, ...]) -> str:
    region, instance_type, deployment, term, lease, purchase = key
    commercial = " ".join(part for part in (term, lease, purchase) if part)
    return " | ".join((region, instance_type, deployment, commercial))


@dataclass(frozen=True)
class MiOption:
    compute_hourly: float
    license_hourly: float


def load_mi_options() -> dict[tuple[str, str], dict[str, MiOption]]:
    options: dict[tuple[str, str], dict[str, MiOption]] = defaultdict(dict)
    with SQLMI_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["RecordKind"] != "Configured SKU Total":
                continue
            if row["InstanceType"] != "Single Instance":
                continue
            option_key = row["CalculatorOptionKey"]
            if option_key not in REQUIRED_MI_OPTIONS:
                continue
            options[(row["AzureRegion"], row["ConfigurationKey"])][option_key] = MiOption(
                compute_hourly=as_float(row["ComputeHourlyPrice"]),
                license_hourly=as_float(row["SqlLicenseHourlyPrice"]),
            )
    return dict(options)


def load_mi_storage_rates() -> dict[tuple[str, str, bool], float]:
    candidates: dict[tuple[str, str, bool], list[float]] = defaultdict(list)
    with SQLMI_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["RecordKind"] != "Retail Price Dimension":
                continue
            if row["PricingComponent"] != "Data Storage":
                continue
            if row["PriceType"] != "Consumption" or "Month" not in row["UnitOfMeasure"]:
                continue
            price = as_float(row["EffectiveMonthlyPrice"], as_float(row["RetailPrice"]))
            if price <= 0:
                continue
            key = row["AzureRegion"], row["ServiceTier"], as_bool(row["IsZoneRedundant"])
            candidates[key].append(price)
    rates = {key: min(prices) for key, prices in candidates.items()}
    for (region, tier, zone_redundant), price in list(rates.items()):
        if tier == "General Purpose":
            rates.setdefault(
                (region, "Next Generation General Purpose", zone_redundant),
                price,
            )
    return rates


def load_mi_memory_rates() -> dict[tuple[str, bool], float]:
    rates: dict[tuple[str, bool], tuple[str, float]] = {}
    with SQLMI_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["RecordKind"] != "Retail Price Dimension":
                continue
            if row["PricingComponent"] != "Additional Memory":
                continue
            if row["ServiceTier"] != "General Purpose":
                continue
            if row["HardwareFamily"] != "Premium Series":
                continue
            if row["PriceType"] != "Consumption" or row["UnitOfMeasure"] != "1 GB/Hour":
                continue
            price = as_float(row["RetailPrice"])
            if price <= 0:
                continue
            key = row["AzureRegion"], as_bool(row["IsZoneRedundant"])
            effective_date = row["EffectiveStartDate"]
            current = rates.get(key)
            if current is None or effective_date > current[0]:
                rates[key] = effective_date, price
    return {key: value[1] for key, value in rates.items()}


def load_rds_license_rates() -> dict[tuple[str, str], float]:
    rates: dict[tuple[str, str], float] = {}
    with RDS_MAPPING_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["MappingStatus"] != "Mapped to SQL MI license component":
                continue
            edition = row["RdsDatabaseEdition"]
            if edition not in {"Standard", "Enterprise", "Web"}:
                continue
            rates[(row["RdsRegion"], edition)] = as_float(row["RdsPricePerUnit"])

    regional_editions = {(region, edition) for region, edition in rates}
    regions = {region for region, _ in regional_editions}
    for region in regions | {"eu-central-2"}:
        for edition, fallback in (("Standard", 0.12), ("Enterprise", 0.375), ("Web", 0.017)):
            rates.setdefault((region, edition), fallback)
    return rates


def build_rds_storage_catalog() -> list[dict[str, object]]:
    candidates: dict[tuple[str, str, str], list[dict[str, str]]] = defaultdict(list)
    with RDS_MAPPING_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["RdsProductFamily"] != "Database Storage":
                continue
            if not row["MappingStatus"].startswith("Mapped"):
                continue
            if row["RdsUnit"] != "GB-Mo" or not row["RdsVolumeType"]:
                continue
            key = row["RdsRegion"], row["RdsDeploymentOption"], row["RdsVolumeType"]
            candidates[key].append(row)

    catalog: list[dict[str, object]] = []
    for key, rows in candidates.items():
        source_rate = min(as_float(row["RdsPricePerUnit"]) for row in rows)
        catalog.append(
            {
                "storage_key": "|".join(key),
                "region": key[0],
                "deployment": key[1],
                "volume_type": key[2],
                "source_monthly_per_gb": source_rate,
            }
        )
    catalog.sort(key=lambda row: str(row["storage_key"]))
    return catalog


def choose_mi_target(rows: list[dict[str, str]]) -> dict[str, str]:
    return min(
        rows,
        key=lambda row: (
            as_float(row.get("FitScore") or row.get("PerformanceFitScore"), math.inf),
            MI_TIER_PRIORITY.get(row.get("SqlMiServiceTier", ""), 99),
            as_float(row.get("SqlMiVCoreCount"), math.inf),
            row.get("MappingId", ""),
        ),
    )


def choose_edition_target(
    rows: list[dict[str, str]], edition: str
) -> dict[str, str] | None:
    allowed_tiers = (
        {"General Purpose", "Next Generation General Purpose"}
        if edition == "Standard"
        else {"Business Critical"}
    )
    candidates = [row for row in rows if row["SqlMiServiceTier"] in allowed_tiers]
    return choose_mi_target(candidates) if candidates else None


def build_rds_catalog(
    mi_options: dict[tuple[str, str], dict[str, MiOption]],
    storage_rates: dict[tuple[str, str, bool], float],
    memory_rates: dict[tuple[str, bool], float],
    rds_license_rates: dict[tuple[str, str], float],
) -> list[dict[str, object]]:
    grouped: dict[tuple[str, ...], list[dict[str, str]]] = defaultdict(list)
    mi_candidates_by_configuration: dict[str, dict[str, str]] = {}
    with RDS_MAPPING_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["MappingStatus"] != "Mapped to configured SQL MI SKU":
                continue
            if row["RdsProductFamily"] != "Database Instance":
                continue
            if not row["RdsInstanceType"] or not row["RdsEffectiveHourlyPrice"]:
                continue
            if row["AzureRegion"] == RDS_AZURE_MIGRATION_REGION:
                mi_candidates_by_configuration.setdefault(
                    row["SqlMiConfigurationKey"],
                    row,
                )
            key = source_key(
                row["RdsRegion"],
                row["RdsInstanceType"],
                row["RdsDeploymentOption"],
                row["RdsTermType"],
                row["RdsLeaseContractLength"],
                row["RdsPurchaseOption"],
            )
            grouped[key].append(row)

    mi_candidates = list(mi_candidates_by_configuration.values())
    unavailable_options = {
        option_key: MiOption(compute_hourly=0.0, license_hourly=0.0)
        for _, option_key in MI_PURCHASE_OPTIONS
    }
    catalog: list[dict[str, object]] = []
    for key, rows in grouped.items():
        display_key = display_source_key(key)
        compute_hourly = min(as_float(row["RdsEffectiveHourlyPrice"]) for row in rows)
        source_vcpu = as_float(rows[0]["RdsVcpu"])
        source_memory_gib = as_float(rows[0]["RdsMemoryGiB"])
        for edition in ("Standard", "Enterprise"):
            target_added = False
            for service_tier in EC2_MI_SERVICE_TIERS:
                target = choose_mi_target_for_ec2(
                    mi_candidates,
                    service_tier,
                    source_vcpu,
                    source_memory_gib,
                )
                if target is None:
                    continue
                target_region = str(target["AzureRegion"])
                target_configuration = str(target["SqlMiConfigurationKey"])
                option_set = mi_options.get((target_region, target_configuration), {})
                if not REQUIRED_MI_OPTIONS.issubset(option_set):
                    continue
                zone_redundant = as_bool(str(target["SqlMiIsZoneRedundant"]))
                storage_key = target_region, str(target["SqlMiServiceTier"]), zone_redundant
                selected_memory = float(target["SelectedMemoryGiB"])
                included_memory = float(target["IncludedMemoryGiB"])
                additional_memory = max(0.0, selected_memory - included_memory)
                memory_rate = memory_rates.get((target_region, zone_redundant), 0.0)
                if additional_memory > 0 and memory_rate <= 0:
                    continue
                catalog.append(
                    {
                        "lookup_key": f"{display_key}|{edition}|{service_tier}",
                        "display_key": display_key,
                        "edition": edition,
                        "mi_service_tier_selection": service_tier,
                        "region": key[0],
                        "instance_type": key[1],
                        "deployment": key[2],
                        "term": key[3],
                        "lease": key[4],
                        "purchase": key[5],
                        "vcpu": source_vcpu,
                        "memory_gib": source_memory_gib,
                        "source_compute_hourly": compute_hourly,
                        "source_standard_license_core_hourly": rds_license_rates.get((key[0], "Standard"), 0.12),
                        "source_enterprise_license_core_hourly": rds_license_rates.get((key[0], "Enterprise"), 0.375),
                        "mi_region": target_region,
                        "mi_configuration": target_configuration,
                        "mi_tier": target["SqlMiServiceTier"],
                        "mi_hardware": target["SqlMiHardwareFamily"],
                        "mi_vcores": as_float(str(target["SqlMiVCoreCount"])),
                        "mi_memory_gb": selected_memory,
                        "mi_included_memory_gb": included_memory,
                        "mi_additional_memory_gb": additional_memory,
                        "mi_memory_hourly_rate": memory_rate,
                        "mi_memory_options": target["MemoryOptions"],
                        "mi_zone_redundant": zone_redundant,
                        "mi_storage_monthly_per_gb": storage_rates.get(storage_key, 0.0),
                        "mi_options": option_set,
                    }
                )
                target_added = True
            if not target_added:
                catalog.append(
                    {
                        "lookup_key": f"{display_key}|{edition}|{NO_MI_MAPPING}",
                        "display_key": display_key,
                        "edition": edition,
                        "mi_service_tier_selection": NO_MI_MAPPING,
                        "region": key[0],
                        "instance_type": key[1],
                        "deployment": key[2],
                        "term": key[3],
                        "lease": key[4],
                        "purchase": key[5],
                        "vcpu": source_vcpu,
                        "memory_gib": source_memory_gib,
                        "source_compute_hourly": compute_hourly,
                        "source_standard_license_core_hourly": rds_license_rates.get(
                            (key[0], "Standard"), 0.12
                        ),
                        "source_enterprise_license_core_hourly": rds_license_rates.get(
                            (key[0], "Enterprise"), 0.375
                        ),
                        "mi_region": RDS_AZURE_MIGRATION_REGION,
                        "mi_configuration": NO_MI_MAPPING,
                        "mi_tier": NO_MI_MAPPING,
                        "mi_hardware": "No single MI fit",
                        "mi_vcores": 0.0,
                        "mi_memory_gb": 0.0,
                        "mi_included_memory_gb": 0.0,
                        "mi_additional_memory_gb": 0.0,
                        "mi_memory_hourly_rate": 0.0,
                        "mi_memory_options": (),
                        "mi_zone_redundant": False,
                        "mi_storage_monthly_per_gb": 0.0,
                        "mi_options": unavailable_options,
                    }
                )
    catalog.sort(key=lambda row: str(row["display_key"]))
    return catalog


def load_ec2_rates() -> dict[tuple[str, str], dict[str, float]]:
    rates: dict[tuple[str, str], dict[str, float]] = defaultdict(dict)
    with EC2_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if row["TermType"] != "OnDemand" or row["Tenancy"] != "Shared":
                continue
            if row["CurrentGeneration"] != "Yes" or row["OperatingSystem"] != "Windows":
                continue
            if row["WindowsLicenseModel"] != "No License required":
                continue
            software = row["PreInstalledSoftware"]
            component = {"NA": "compute", "SQL Std": "standard", "SQL Ent": "enterprise"}.get(software)
            if component is None:
                continue
            key = row["AWSRegionCode"], row["InstanceType"]
            rates[key][component] = as_float(row["EffectiveHourlyPrice"])
            rates[key]["vcpu"] = as_float(row["vCPU"])
            rates[key]["memory_gib"] = as_float(row["MemoryGiB"])

    regional_license_core_rates: dict[tuple[str, str], list[float]] = defaultdict(list)
    for (region, _), rate_set in rates.items():
        compute_rate = rate_set.get("compute", 0.0)
        billable_cores = max(4.0, rate_set.get("vcpu", 0.0))
        if compute_rate <= 0 or billable_cores <= 0:
            continue
        for rate_key in ("standard", "enterprise"):
            licensed_rate = rate_set.get(rate_key, 0.0)
            if licensed_rate > compute_rate:
                regional_license_core_rates[(region, rate_key)].append(
                    (licensed_rate - compute_rate) / billable_cores
                )

    minimum_core_rates = {
        key: min(values)
        for key, values in regional_license_core_rates.items()
        if values
    }
    for (region, _), rate_set in rates.items():
        compute_rate = rate_set.get("compute", 0.0)
        billable_cores = max(4.0, rate_set.get("vcpu", 0.0))
        if compute_rate <= 0:
            continue
        for rate_key in ("standard", "enterprise"):
            if rate_key in rate_set:
                continue
            core_rate = minimum_core_rates.get((region, rate_key))
            if core_rate is not None:
                rate_set[rate_key] = compute_rate + billable_cores * core_rate
    return dict(rates)


def mi_memory_options(row: dict[str, str]) -> tuple[float, ...]:
    vcores = int(as_float(row["SqlMiVCoreCount"]))
    if (
        row["SqlMiServiceTier"] == "Next Generation General Purpose"
        and row["SqlMiHardwareFamily"] == "Premium Series"
        and not as_bool(row["SqlMiIsZoneRedundant"])
        and vcores in NGGP_PREMIUM_MEMORY_OPTIONS
    ):
        return tuple(float(value) for value in NGGP_PREMIUM_MEMORY_OPTIONS[vcores])
    return (as_float(row["SqlMiEstimatedMemoryGiB"]),)


def choose_mi_target_for_ec2(
    rows: list[dict[str, str]],
    service_tier: str,
    source_vcpu: float,
    source_memory_gib: float,
) -> dict[str, object] | None:
    candidates: list[tuple[dict[str, str], float, tuple[float, ...]]] = []
    for row in rows:
        if row["SqlMiServiceTier"] != service_tier:
            continue
        if service_tier == EC2_DEFAULT_MI_SERVICE_TIER and (
            row["SqlMiHardwareFamily"] != "Premium Series"
            or as_bool(row["SqlMiIsZoneRedundant"])
        ):
            continue
        if as_float(row["SqlMiVCoreCount"]) < source_vcpu:
            continue
        memory_options = mi_memory_options(row)
        selected_memory = next(
            (memory for memory in memory_options if memory >= source_memory_gib),
            None,
        )
        if selected_memory is None:
            continue
        candidates.append((row, selected_memory, memory_options))
    if not candidates:
        return None
    row, selected_memory, memory_options = min(
        candidates,
        key=lambda candidate: (
            (as_float(candidate[0]["SqlMiVCoreCount"]) - source_vcpu) / source_vcpu,
            (candidate[1] - source_memory_gib)
            / source_memory_gib,
            MI_TIER_PRIORITY.get(candidate[0]["SqlMiServiceTier"], 99),
            as_bool(candidate[0]["SqlMiIsZoneRedundant"]),
            as_float(candidate[0]["SqlMiVCoreCount"]),
            candidate[0]["SqlMiConfigurationKey"],
        ),
    )
    selected = dict(row)
    selected["SelectedMemoryGiB"] = selected_memory
    selected["IncludedMemoryGiB"] = memory_options[0]
    selected["MemoryOptions"] = memory_options
    return selected


def build_ec2_catalog(
    mi_options: dict[tuple[str, str], dict[str, MiOption]],
    storage_rates: dict[tuple[str, str, bool], float],
    memory_rates: dict[tuple[str, bool], float],
) -> list[dict[str, object]]:
    mi_candidates: list[dict[str, str]] = []
    with EC2_MAPPING_PATH.open(newline="", encoding="utf-8-sig") as handle:
        for row in csv.DictReader(handle):
            if not row["MappingStatus"].startswith("Mapped"):
                continue
            if row["AzureRegion"] == EC2_AZURE_MIGRATION_REGION:
                mi_candidates.append(row)

    ec2_rates = load_ec2_rates()
    catalog: list[dict[str, object]] = []
    for key, rate_set in ec2_rates.items():
        if "compute" not in rate_set:
            continue
        display_key = f"{key[0]} | {key[1]}"
        source_vcpu = rate_set["vcpu"]
        source_memory_gib = rate_set["memory_gib"]
        for edition in ("Standard", "Enterprise"):
            for service_tier in EC2_MI_SERVICE_TIERS:
                target = choose_mi_target_for_ec2(
                    mi_candidates,
                    service_tier,
                    source_vcpu,
                    source_memory_gib,
                )
                if target is None:
                    continue
                target_region = str(target["AzureRegion"])
                target_configuration = str(target["SqlMiConfigurationKey"])
                option_set = mi_options.get((target_region, target_configuration), {})
                if not REQUIRED_MI_OPTIONS.issubset(option_set):
                    continue
                zone_redundant = as_bool(str(target["SqlMiIsZoneRedundant"]))
                storage_key = target_region, str(target["SqlMiServiceTier"]), zone_redundant
                selected_memory = float(target["SelectedMemoryGiB"])
                included_memory = float(target["IncludedMemoryGiB"])
                additional_memory = max(0.0, selected_memory - included_memory)
                memory_rate = memory_rates.get((target_region, zone_redundant), 0.0)
                if additional_memory > 0 and memory_rate <= 0:
                    continue
                catalog.append(
                    {
                        "lookup_key": f"{display_key}|{edition}|{service_tier}",
                        "display_key": display_key,
                        "edition": edition,
                        "mi_service_tier_selection": service_tier,
                        "region": key[0],
                        "instance_type": key[1],
                        "vcpu": source_vcpu,
                        "memory_gib": source_memory_gib,
                        "source_compute_hourly": rate_set["compute"],
                        "source_standard_license_hourly": rate_set.get("standard", rate_set["compute"]) - rate_set["compute"],
                        "source_enterprise_license_hourly": rate_set.get("enterprise", rate_set["compute"]) - rate_set["compute"],
                        "mi_region": target_region,
                        "mi_configuration": target_configuration,
                        "mi_tier": target["SqlMiServiceTier"],
                        "mi_hardware": target["SqlMiHardwareFamily"],
                        "mi_vcores": as_float(str(target["SqlMiVCoreCount"])),
                        "mi_memory_gb": selected_memory,
                        "mi_included_memory_gb": included_memory,
                        "mi_additional_memory_gb": additional_memory,
                        "mi_memory_hourly_rate": memory_rate,
                        "mi_memory_options": target["MemoryOptions"],
                        "mi_zone_redundant": zone_redundant,
                        "mi_storage_monthly_per_gb": storage_rates.get(storage_key, 0.0),
                        "mi_options": option_set,
                    }
                )
    catalog.sort(key=lambda row: str(row["display_key"]))
    return catalog


def validate_catalogs(
    rds_catalog: list[dict[str, object]],
    ec2_catalog: list[dict[str, object]],
    rds_license_rates: dict[tuple[str, str], float],
    rds_storage_catalog: list[dict[str, object]],
) -> None:
    if len(rds_catalog) < 1_000:
        raise RuntimeError(f"RDS catalog unexpectedly small: {len(rds_catalog)}")
    if len(ec2_catalog) < 100:
        raise RuntimeError(f"EC2 catalog unexpectedly small: {len(ec2_catalog)}")
    if len(rds_storage_catalog) < 100:
        raise RuntimeError(f"RDS storage catalog unexpectedly small: {len(rds_storage_catalog)}")
    if rds_license_rates.get(("eu-west-1", "Standard")) != 0.12:
        raise RuntimeError("RDS Standard license rate did not resolve.")
    if rds_license_rates.get(("eu-west-1", "Enterprise")) != 0.375:
        raise RuntimeError("RDS Enterprise license rate did not resolve.")

    rds_sample_display_key = (
        "eu-west-1 | db.m6i.2xlarge | Single-AZ | Reserved 1yr No Upfront"
    )
    for service_tier in EC2_MI_SERVICE_TIERS:
        rds_edition_rows = {
            edition: next(
                (
                    row
                    for row in rds_catalog
                    if row["display_key"] == rds_sample_display_key
                    and row["edition"] == edition
                    and row["mi_service_tier_selection"] == service_tier
                ),
                None,
            )
            for edition in ("Standard", "Enterprise")
        }
        if any(row is None for row in rds_edition_rows.values()):
            raise RuntimeError(
                f"RDS validation selection did not resolve for {service_tier}."
            )
        azure_fields = (
            "mi_region",
            "mi_configuration",
            "mi_tier",
            "mi_hardware",
            "mi_vcores",
            "mi_memory_gb",
            "mi_included_memory_gb",
            "mi_memory_hourly_rate",
            "mi_storage_monthly_per_gb",
            "mi_options",
        )
        standard_target = tuple(
            rds_edition_rows["Standard"][field] for field in azure_fields
        )
        enterprise_target = tuple(
            rds_edition_rows["Enterprise"][field] for field in azure_fields
        )
        if standard_target != enterprise_target:
            raise RuntimeError(
                f"RDS edition changed the Azure target for {service_tier}."
            )
        if rds_edition_rows["Standard"]["mi_region"] != RDS_AZURE_MIGRATION_REGION:
            raise RuntimeError(
                f"RDS {service_tier} target is not in Sweden Central."
            )

    ec2_sample = next(
        row
        for row in ec2_catalog
        if row["region"] == "eu-west-1"
        and row["instance_type"] == "r8i.xlarge"
        and row["edition"] == "Standard"
        and row["mi_service_tier_selection"] == EC2_DEFAULT_MI_SERVICE_TIER
    )
    if not math.isclose(float(ec2_sample["source_compute_hourly"]), 0.4949, abs_tol=1e-9):
        raise RuntimeError("EC2 compute baseline is not the canonical Windows rate.")
    if not math.isclose(float(ec2_sample["source_standard_license_hourly"]), 0.48, abs_tol=1e-9):
        raise RuntimeError("EC2 Standard SQL premium did not resolve.")
    if not math.isclose(float(ec2_sample["source_enterprise_license_hourly"]), 1.50, abs_tol=1e-9):
        raise RuntimeError("EC2 Enterprise SQL premium did not resolve.")
    if float(ec2_sample["mi_storage_monthly_per_gb"]) <= 0:
        raise RuntimeError("EC2 sample did not resolve an MI storage rate.")
    options = ec2_sample["mi_options"]
    if not isinstance(options, dict) or not REQUIRED_MI_OPTIONS.issubset(options):
        raise RuntimeError("EC2 sample did not resolve required MI purchase options.")
    for instance_type in {item[1] for item in EC2_PRIMED_WORKLOADS}:
        for service_tier in EC2_MI_SERVICE_TIERS:
            edition_rows: dict[str, dict[str, object]] = {}
            for edition in ("Standard", "Enterprise"):
                inventory_rows = [
                    row
                    for row in ec2_catalog
                    if row["region"] == "eu-west-1"
                    and row["instance_type"] == instance_type
                    and row["edition"] == edition
                    and row["mi_service_tier_selection"] == service_tier
                ]
                if len(inventory_rows) != 1:
                    raise RuntimeError(
                        f"EC2 inventory type {instance_type} did not resolve exactly "
                        f"once for {edition} / {service_tier}."
                    )
                edition_rows[edition] = inventory_rows[0]
                if inventory_rows[0]["mi_vcores"] < inventory_rows[0]["vcpu"]:
                    raise RuntimeError(
                        f"EC2 inventory type {instance_type} has an undersized "
                        f"{service_tier} target."
                    )
            azure_fields = (
                "mi_region",
                "mi_configuration",
                "mi_tier",
                "mi_hardware",
                "mi_vcores",
                "mi_memory_gb",
                "mi_included_memory_gb",
                "mi_memory_hourly_rate",
                "mi_storage_monthly_per_gb",
                "mi_options",
            )
            standard_target = tuple(
                edition_rows["Standard"][field] for field in azure_fields
            )
            enterprise_target = tuple(
                edition_rows["Enterprise"][field] for field in azure_fields
            )
            if standard_target != enterprise_target:
                raise RuntimeError(
                    f"AWS edition changed the Azure target for {instance_type} / "
                    f"{service_tier}."
                )
    flexible_sample = next(
        row
        for row in ec2_catalog
        if row["region"] == "eu-west-1"
        and row["instance_type"] == "r6id.8xlarge"
        and row["edition"] == "Standard"
        and row["mi_service_tier_selection"] == EC2_DEFAULT_MI_SERVICE_TIER
    )
    expected_flexible_target = (
        EC2_AZURE_MIGRATION_REGION,
        "Next Generation General Purpose",
        "Premium Series",
        32.0,
        256.0,
        224.0,
        32.0,
    )
    actual_flexible_target = tuple(
        flexible_sample[field]
        for field in (
            "mi_region",
            "mi_tier",
            "mi_hardware",
            "mi_vcores",
            "mi_memory_gb",
            "mi_included_memory_gb",
            "mi_additional_memory_gb",
        )
    )
    if actual_flexible_target != expected_flexible_target:
        raise RuntimeError(
            "r6id.8xlarge did not resolve to Sweden Central Premium 32/256: "
            f"{actual_flexible_target}"
        )
    if not math.isclose(
        numeric(flexible_sample["mi_memory_hourly_rate"]),
        0.011663,
        abs_tol=1e-9,
    ):
        raise RuntimeError("Sweden Central additional-memory rate did not resolve.")


def numeric(value: object, fallback: float = 0.0) -> float:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return float(value)
    return fallback


def normalize_ec2_mi_service_tier(value: object) -> str:
    text = str(value or "")
    if not text:
        return ""
    for service_tier in EC2_MI_SERVICE_TIERS:
        if service_tier in text:
            return service_tier
    return EC2_DEFAULT_MI_SERVICE_TIER


def ebs_monthly_cost_components(
    region: str,
    volume_type: str,
    storage_gb: float,
    provisioned_iops: float | None,
    provisioned_throughput: float | None = None,
) -> tuple[float, float, float]:
    if volume_type == "Ephemeral" or storage_gb <= 0:
        return 0.0, 0.0, 0.0
    option_to_sku = {
        option: sku
        for sku, label in EBS_STORAGE_OPTIONS
        for option in (sku, label)
    }
    if volume_type not in option_to_sku:
        raise RuntimeError(f"Unknown EBS volume type: {volume_type}")
    if region not in EBS_REGIONAL_RATES:
        raise RuntimeError(f"EBS pricing is unavailable for {region}.")
    (
        capacity_rate,
        included_iops,
        tier_1_rate,
        tier_2_rate,
        tier_3_rate,
        included_throughput,
        throughput_rate,
    ) = (
        EBS_REGIONAL_RATES[region][option_to_sku[volume_type]]
    )
    provisioned_iops = max(0.0, float(provisioned_iops or 0))
    provisioned_throughput = max(0.0, float(provisioned_throughput or 0))
    if included_iops:
        iops_cost = max(0.0, provisioned_iops - included_iops) * tier_1_rate
    else:
        iops_cost = (
            min(provisioned_iops, 32_000) * tier_1_rate
            + max(0.0, min(provisioned_iops - 32_000, 32_000)) * tier_2_rate
            + max(0.0, provisioned_iops - 64_000) * tier_3_rate
        )
    throughput_cost = (
        max(0.0, provisioned_throughput - included_throughput) * throughput_rate
    )
    return storage_gb * capacity_rate, iops_cost, throughput_cost


def ebs_monthly_cost(
    region: str,
    volume_type: str,
    storage_gb: float,
    provisioned_iops: float | None,
    provisioned_throughput: float | None = None,
) -> float:
    return sum(
        ebs_monthly_cost_components(
            region,
            volume_type,
            storage_gb,
            provisioned_iops,
            provisioned_throughput,
        )
    )


def ebs_profile_monthly_cost(region: str, profile: str) -> float:
    return sum(
        ebs_monthly_cost(region, volume_type, storage_gb, iops, throughput)
        for server, _, _, volume_type, storage_gb, iops, throughput in EBS_VOLUME_DETAILS
        if server == profile
    )


def ebs_profile_max_iops(profile: str) -> float:
    return max(
        (
            float(iops or 0)
            for server, _, _, volume_type, _, iops, _ in EBS_VOLUME_DETAILS
            if server == profile and volume_type != "Ephemeral"
        ),
        default=0.0,
    )


def nggp_iops_cap(vcores: float) -> float:
    return min(NGGP_MAX_IOPS, NGGP_IOPS_PER_VCORE * vcores)


def style_range(
    cell_range,
    *,
    fill: int | None = None,
    font_color: int | None = None,
    bold: bool | None = None,
    font_size: int | None = None,
    horizontal: int | None = None,
    wrap: bool | None = None,
    borders: bool = False,
) -> None:
    if fill is not None:
        cell_range.Interior.Color = fill
    if font_color is not None:
        cell_range.Font.Color = font_color
    if bold is not None:
        cell_range.Font.Bold = bold
    if font_size is not None:
        cell_range.Font.Size = font_size
    if horizontal is not None:
        cell_range.HorizontalAlignment = horizontal
    if wrap is not None:
        cell_range.WrapText = wrap
    cell_range.VerticalAlignment = XL_CENTER
    if borders:
        cell_range.Borders.LineStyle = XL_CONTINUOUS
        cell_range.Borders.Color = BORDER_COLOR
        cell_range.Borders.Weight = XL_THIN


def merge_with_value(worksheet, address: str, value: object) -> None:
    cell_range = worksheet.Range(address)
    cell_range.Merge()
    cell_range.Cells(1, 1).Value2 = value


def remove_name(workbook, name: str) -> None:
    try:
        workbook.Names.Item(name).Delete()
    except pywintypes.com_error:
        pass


def set_name(workbook, name: str, refers_to: str) -> None:
    remove_name(workbook, name)
    workbook.Names.Add(name, refers_to)


def remove_sheet(workbook, name: str) -> None:
    for worksheet in list(workbook.Worksheets):
        if worksheet.Name == name:
            worksheet.Delete()
            return


def write_matrix(worksheet, top_left: str, rows: list[tuple[object, ...]]) -> None:
    if not rows:
        return
    start = worksheet.Range(top_left)
    column_count = len(rows[0])
    for offset in range(0, len(rows), 500):
        chunk = rows[offset : offset + 500]
        chunk_start = worksheet.Cells(start.Row + offset, start.Column)
        chunk_end = worksheet.Cells(
            start.Row + offset + len(chunk) - 1,
            start.Column + column_count - 1,
        )
        target = worksheet.Range(chunk_start, chunk_end)
        target.Value2 = tuple(chunk)


def excel_column_name(column_number: int) -> str:
    name = ""
    while column_number:
        column_number, remainder = divmod(column_number - 1, 26)
        name = chr(65 + remainder) + name
    return name


def catalog_helper_rows(catalog: list[dict[str, object]]) -> list[tuple[object, ...]]:
    headers = (
        "LookupKey",
        "DisplayConfiguration",
        "Edition",
        "SourceRegion",
        "SourceInstance",
        "SourceDeployment",
        "SourceVcpu",
        "SourceMemoryGiB",
        "SourceComputeHourly",
        "SourceStandardLicenseHourly",
        "SourceEnterpriseLicenseHourly",
        "MiRegion",
        "MiConfiguration",
        "MiTier",
        "MiHardware",
        "MiVcores",
        "MiZoneRedundant",
        "MiStorageMonthlyPerGiB",
        *(label for label, _ in MI_PURCHASE_OPTIONS),
        *(label for label, _ in MI_PURCHASE_OPTIONS),
    )
    output: list[tuple[object, ...]] = [headers]
    for row in catalog:
        options = row["mi_options"]
        if not isinstance(options, dict):
            raise TypeError("MI option set is not a dictionary.")
        compute_rates = tuple(options[key].compute_hourly for _, key in MI_PURCHASE_OPTIONS)
        license_rates = tuple(options[key].license_hourly for _, key in MI_PURCHASE_OPTIONS)
        output.append(
            (
                row["lookup_key"],
                row["display_key"],
                row["edition"],
                row["region"],
                row["instance_type"],
                row.get("deployment", ""),
                row["vcpu"],
                row["memory_gib"],
                row["source_compute_hourly"],
                row.get("source_standard_license_core_hourly", row.get("source_standard_license_hourly", 0.0)),
                row.get("source_enterprise_license_core_hourly", row.get("source_enterprise_license_hourly", 0.0)),
                row["mi_region"],
                row["mi_configuration"],
                row["mi_tier"],
                row["mi_hardware"],
                row["mi_vcores"],
                row["mi_zone_redundant"],
                row["mi_storage_monthly_per_gb"],
                *compute_rates,
                *license_rates,
            )
        )
    return output


def available_sample_configurations(
    catalog: list[dict[str, object]], count: int
) -> list[str]:
    editions_by_config: dict[str, set[str]] = defaultdict(set)
    metadata: dict[str, dict[str, object]] = {}
    for row in catalog:
        display_key = str(row["display_key"])
        editions_by_config[display_key].add(str(row["edition"]))
        metadata.setdefault(display_key, row)
    configurations = [
        key
        for key, editions in editions_by_config.items()
        if {"Standard", "Enterprise"}.issubset(editions)
    ]
    region_priority = {"eu-west-1": 0, "us-east-1": 1, "eu-central-1": 2}

    def score(configuration: str) -> tuple[object, ...]:
        row = metadata[configuration]
        instance_type = str(row["instance_type"])
        modern_family = 0 if any(token in instance_type for token in ("m6i", "r6i", "r8i")) else 1
        size = numeric(row["vcpu"], 999.0)
        preferred_size = abs(size - 8.0)
        on_demand = 0 if "OnDemand" in configuration else 1
        return (
            region_priority.get(str(row["region"]), 9),
            modern_family,
            on_demand,
            preferred_size,
            configuration,
        )

    configurations.sort(key=score)
    if len(configurations) < count:
        raise RuntimeError(f"Only {len(configurations)} configurations support both editions.")
    step = max(1, len(configurations) // count)
    selected = [configurations[index * step] for index in range(count)]
    return selected


def available_rds_sample_selections(
    catalog: list[dict[str, object]],
    region: str,
    count: int,
) -> list[tuple[str, str, str]]:
    editions_by_selection: dict[tuple[str, str, str], set[str]] = defaultdict(set)
    for row in catalog:
        if row["region"] != region:
            continue
        if row["mi_service_tier_selection"] == NO_MI_MAPPING:
            continue
        commercial = " ".join(
            str(row[field])
            for field in ("term", "lease", "purchase")
            if row[field]
        )
        selection = (
            str(row["instance_type"]),
            str(row["deployment"]),
            commercial,
        )
        editions_by_selection[selection].add(str(row["edition"]))

    selections = [
        selection
        for selection, editions in editions_by_selection.items()
        if {"Standard", "Enterprise"}.issubset(editions)
    ]
    requested = (
        "db.m6i.2xlarge",
        "Single-AZ",
        "Reserved 1yr No Upfront",
    )

    def score(selection: tuple[str, str, str]) -> tuple[object, ...]:
        instance_type, deployment, commercial = selection
        modern_family = 0 if any(token in instance_type for token in ("m6i", "r6i")) else 1
        on_demand = 0 if commercial == "OnDemand" else 1
        single_az = 0 if deployment == "Single-AZ" else 1
        return modern_family, on_demand, single_az, instance_type, commercial

    selections.sort(key=score)
    selected = [requested] if requested in selections else []
    remaining = [selection for selection in selections if selection not in selected]
    slots = count - len(selected)
    if len(remaining) < slots:
        raise RuntimeError(
            f"Only {len(selections)} RDS selections support both editions in {region}."
        )
    step = max(1, len(remaining) // max(slots, 1))
    selected.extend(remaining[index] for index in range(0, len(remaining), step) if len(selected) < count)
    if len(selected) < count:
        selected.extend(selection for selection in remaining if selection not in selected)
    return selected[:count]


def available_ec2_sample_instances(
    catalog: list[dict[str, object]],
    region: str,
    count: int,
) -> list[str]:
    editions_by_instance: dict[str, set[str]] = defaultdict(set)
    for row in catalog:
        if row["region"] == region:
            editions_by_instance[str(row["instance_type"])].add(str(row["edition"]))
    instances = [
        instance_type
        for instance_type, editions in editions_by_instance.items()
        if {"Standard", "Enterprise"}.issubset(editions)
    ]

    def score(instance_type: str) -> tuple[object, ...]:
        preferred = 0 if instance_type == "r8i.xlarge" else 1
        family = 0 if instance_type.startswith("r8i.") else 1
        return preferred, family, len(instance_type), instance_type

    instances.sort(key=score)
    if len(instances) < count:
        raise RuntimeError(
            f"Only {len(instances)} EC2 instances support both editions in {region}."
        )
    return instances[:count]


def add_list_validation(cell_range, formula: str, title: str) -> None:
    if cell_range.MergeCells:
        cell_range = cell_range.MergeArea
    cell_range.Validation.Delete()
    cell_range.Validation.Add(
        XL_VALIDATE_LIST,
        XL_VALID_ALERT_STOP,
        XL_BETWEEN,
        formula,
    )
    cell_range.Validation.IgnoreBlank = False
    cell_range.Validation.InCellDropdown = True
    cell_range.Validation.InputTitle = ""
    cell_range.Validation.InputMessage = ""
    cell_range.Validation.ErrorTitle = "Invalid selection"
    cell_range.Validation.ErrorMessage = "Choose a value from the dropdown list."
    cell_range.Validation.ShowInput = False
    cell_range.Validation.ShowError = True


def add_number_validation(
    cell_range,
    validation_type: int,
    minimum: float,
    maximum: float,
    title: str,
) -> None:
    cell_range.Validation.Delete()
    cell_range.Validation.Add(
        validation_type,
        XL_VALID_ALERT_STOP,
        XL_BETWEEN,
        minimum,
        maximum,
    )
    cell_range.Validation.IgnoreBlank = False
    cell_range.Validation.InputTitle = ""
    cell_range.Validation.InputMessage = ""
    cell_range.Validation.ErrorTitle = "Invalid value"
    cell_range.Validation.ErrorMessage = f"Use a value from {minimum:g} to {maximum:g}."
    cell_range.Validation.ShowInput = False
    cell_range.Validation.ShowError = True


def remove_workbook_tooltips(workbook) -> None:
    for worksheet in workbook.Worksheets:
        try:
            validation_cells = worksheet.UsedRange.SpecialCells(
                XL_CELL_TYPE_ALL_VALIDATION
            )
        except pywintypes.com_error:
            validation_cells = None
        if validation_cells is not None:
            for cell in validation_cells.Cells:
                if cell.MergeCells and (
                    cell.Row != cell.MergeArea.Row
                    or cell.Column != cell.MergeArea.Column
                ):
                    continue
                try:
                    cell.Validation.InputTitle = ""
                    cell.Validation.InputMessage = ""
                    cell.Validation.ShowInput = False
                except pywintypes.com_error:
                    pass
        for hyperlink_index in range(1, worksheet.Hyperlinks.Count + 1):
            worksheet.Hyperlinks.Item(hyperlink_index).ScreenTip = ""


def find_workbook_tooltips(workbook) -> list[str]:
    tooltips: list[str] = []
    for worksheet in workbook.Worksheets:
        try:
            validation_cells = worksheet.UsedRange.SpecialCells(
                XL_CELL_TYPE_ALL_VALIDATION
            )
        except pywintypes.com_error:
            validation_cells = None
        if validation_cells is not None:
            for cell in validation_cells.Cells:
                if cell.MergeCells and (
                    cell.Row != cell.MergeArea.Row
                    or cell.Column != cell.MergeArea.Column
                ):
                    continue
                if (
                    bool(cell.Validation.ShowInput)
                    or cell.Validation.InputTitle
                    or cell.Validation.InputMessage
                ):
                    tooltips.append(
                        f"{worksheet.Name}!{cell.Address} validation"
                    )
        for hyperlink_index in range(1, worksheet.Hyperlinks.Count + 1):
            hyperlink = worksheet.Hyperlinks.Item(hyperlink_index)
            if hyperlink.ScreenTip:
                tooltips.append(
                    f"{worksheet.Name}!{hyperlink.Range.Address} hyperlink"
                )
    return tooltips


def write_hidden_catalog(
    workbook,
    worksheet,
    catalog: list[dict[str, object]],
    prefix: str,
    rds_storage_catalog: list[dict[str, object]] | None = None,
) -> dict[str, int]:
    helper_rows = catalog_helper_rows(catalog)
    write_matrix(worksheet, "AQ1", helper_rows)
    catalog_end = len(helper_rows)
    write_matrix(
        worksheet,
        "DB1",
        [
            (
                "MiMemoryGb",
                "MiIncludedMemoryGb",
                "MiAdditionalMemoryGb",
                "MiMemoryHourlyRate",
            ),
            *(
                (
                    row.get("mi_memory_gb", 0.0),
                    row.get("mi_included_memory_gb", 0.0),
                    row.get("mi_additional_memory_gb", 0.0),
                    row.get("mi_memory_hourly_rate", 0.0),
                )
                for row in catalog
            ),
        ],
    )

    configurations = sorted({str(row["display_key"]) for row in catalog})
    write_matrix(
        worksheet,
        "CE1",
        [("ConfigurationDropdown",), *((configuration,) for configuration in configurations)],
    )
    write_matrix(
        worksheet,
        "CG1",
        [("EditionDropdown",), ("Standard",), ("Enterprise",)],
    )
    write_matrix(
        worksheet,
        "CH1",
        [("LicenseDropdown",), ("License included",), ("BYOL",)],
    )
    write_matrix(
        worksheet,
        "CI1",
        [("PurchaseOptionDropdown",), *((label,) for label, _ in MI_PURCHASE_OPTIONS)],
    )

    storage_end = 1
    storage_types: list[str] = []
    regions = sorted({str(row["region"]) for row in catalog})
    instance_pairs = sorted(
        {(str(row["region"]), str(row["instance_type"])) for row in catalog}
    )
    write_matrix(
        worksheet,
        "CJ1",
        [("RegionDropdown",), *((region,) for region in regions)],
    )
    write_matrix(
        worksheet,
        "CK1",
        [("InstanceRegionKey", "InstanceDropdown"), *instance_pairs],
    )
    region_count = len(regions)
    instance_end = len(instance_pairs) + 1
    deployment_end = 1
    commercial_end = 1
    memory_end = 1
    if rds_storage_catalog is not None:
        storage_rows = [
            ("StorageKey", "Region", "Deployment", "VolumeType", "MonthlyPerGiB"),
            *(
                (
                    row["storage_key"],
                    row["region"],
                    row["deployment"],
                    row["volume_type"],
                    row["source_monthly_per_gb"],
                )
                for row in rds_storage_catalog
            ),
        ]
        write_matrix(worksheet, "BZ1", storage_rows)
        storage_end = len(storage_rows)
        storage_types = sorted({str(row["volume_type"]) for row in rds_storage_catalog})
        write_matrix(
            worksheet,
            "CF1",
            [("StorageDropdown",), *((storage_type,) for storage_type in storage_types)],
        )

        deployment_pairs = sorted(
            {
                (
                    f'{row["region"]}|{row["instance_type"]}',
                    str(row["deployment"]),
                )
                for row in catalog
            }
        )
        commercial_pairs = sorted(
            {
                (
                    f'{row["region"]}|{row["instance_type"]}|{row["deployment"]}',
                    " ".join(
                        str(row[field])
                        for field in ("term", "lease", "purchase")
                        if row[field]
                    ),
                )
                for row in catalog
            }
        )
        write_matrix(
            worksheet,
            "CM1",
            [("DeploymentKey", "DeploymentDropdown"), *deployment_pairs],
        )
        write_matrix(
            worksheet,
            "CO1",
            [("CommercialKey", "CommercialDropdown"), *commercial_pairs],
        )
        deployment_end = len(deployment_pairs) + 1
        commercial_end = len(commercial_pairs) + 1
    else:
        missing_regions = sorted(set(regions) - EBS_REGIONAL_RATES.keys())
        if missing_regions:
            raise RuntimeError(
                f"EBS pricing is unavailable for: {', '.join(missing_regions)}"
            )
        storage_types = [sku for sku, _ in EBS_STORAGE_OPTIONS] + ["Ephemeral"]
        write_matrix(
            worksheet,
            "CF1",
            [("StorageDropdown",), *((storage_type,) for storage_type in storage_types)],
        )
        ebs_rows = [
            (
                "EbsKey",
                "Region",
                "StorageSku",
                "CapacityPerGbMonth",
                "IncludedIops",
                "IopsTier1",
                "IopsTier2",
                "IopsTier3",
                "IncludedThroughput",
                "ThroughputRate",
            )
        ]
        for region in regions:
            for sku, _ in EBS_STORAGE_OPTIONS:
                ebs_rows.append(
                    (
                        f"{region}|{sku}",
                        region,
                        sku,
                        *EBS_REGIONAL_RATES[region][sku],
                    )
                )
        write_matrix(worksheet, "CQ1", ebs_rows)
        storage_end = len(ebs_rows)
        profile_options = list(dict.fromkeys(EC2_PRIMED_EBS_PROFILES))
        write_matrix(
            worksheet,
            "DA1",
            [("EbsProfileDropdown",), *((profile,) for profile in profile_options)],
        )

    memory_pairs = sorted(
        {
            (str(row["mi_configuration"]), float(memory))
            for row in catalog
            for memory in row.get("mi_memory_options", ())
        }
    )
    write_matrix(
        worksheet,
        "DF1",
        [("MemoryConfigKey", "MemoryDropdown"), *memory_pairs],
    )
    memory_end = len(memory_pairs) + 1

    quoted_sheet = worksheet.Name.replace("'", "''")
    set_name(
        workbook,
        f"{prefix}_Config_List",
        f"='{quoted_sheet}'!$CE$2:$CE${len(configurations) + 1}",
    )
    set_name(workbook, f"{prefix}_Edition_List", f"='{quoted_sheet}'!$CG$2:$CG$3")
    set_name(workbook, f"{prefix}_License_List", f"='{quoted_sheet}'!$CH$2:$CH$3")
    set_name(workbook, f"{prefix}_Purchase_List", f"='{quoted_sheet}'!$CI$2:$CI$9")
    set_name(
        workbook,
        f"{prefix}_Region_List",
        f"='{quoted_sheet}'!$CJ$2:$CJ${region_count + 1}",
    )
    if storage_types:
        set_name(
            workbook,
            f"{prefix}_Storage_List",
            f"='{quoted_sheet}'!$CF$2:$CF${len(storage_types) + 1}",
        )
    if rds_storage_catalog is None:
        set_name(
            workbook,
            f"{prefix}_EBS_Profile_List",
            f"='{quoted_sheet}'!$DA$2:$DA${len(profile_options) + 1}",
        )
        write_matrix(
            worksheet,
            "DI1",
            [
                ("MiServiceTierDropdown",),
                *((service_tier,) for service_tier in EC2_MI_SERVICE_TIERS),
            ],
        )
        set_name(
            workbook,
            f"{prefix}_MI_Tier_List",
            f"='{quoted_sheet}'!$DI$2:$DI${len(EC2_MI_SERVICE_TIERS) + 1}",
        )

    worksheet.Columns("AQ:DI").EntireColumn.Hidden = True
    return {
        "catalog_end": catalog_end,
        "storage_end": storage_end,
        "instance_end": instance_end,
        "deployment_end": deployment_end,
        "commercial_end": commercial_end,
        "memory_end": memory_end,
    }


def set_rds_workload_formulas(
    worksheet,
    row: int,
    catalog_end: int,
    storage_end: int,
    memory_end: int,
) -> None:
    lookup_match = (
        f'MATCH($K$9&" | "&$B{row}&" | "&$C{row}&" | "&$D{row}&"|"&$F{row}&"|"&$O{row},'
        f'$AQ$2:$AQ${catalog_end},0)'
    )
    default_nggp_match = (
        f'MATCH($K$9&" | "&$B{row}&" | "&$C{row}&" | "&$D{row}&"|"&$F{row}&"|"&'
        f'"{EC2_DEFAULT_MI_SERVICE_TIER}",$AQ$2:$AQ${catalog_end},0)'
    )
    source_lookup_match = (
        f'MATCH($K$9&" | "&$B{row}&" | "&$C{row}&" | "&$D{row}&"|"&$F{row}&"|*",'
        f'$AQ$2:$AQ${catalog_end},0)'
    )

    def catalog_value(column: str) -> str:
        return f"INDEX(${column}$2:${column}${catalog_end},{lookup_match})"

    def source_catalog_value(column: str) -> str:
        return f"INDEX(${column}$2:${column}${catalog_end},{source_lookup_match})"

    memory_formula = (
        f'AGGREGATE(15,6,$DG$2:$DG${memory_end}/'
        f'(($DF$2:$DF${memory_end}={catalog_value("BC")})*'
        f'($DG$2:$DG${memory_end}>=$J{row})),1)'
    )
    worksheet.Range(f"N{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({memory_formula},"NO MAPPING"))'
    )
    nggp_vcores = f'INDEX($BF$2:$BF${catalog_end},{default_nggp_match})'
    base_lookup_key = (
        f'$K$9&" | "&$B{row}&" | "&$C{row}&" | "&$D{row}&"|"&$F{row}&"|"'
    )
    nggp_available = (
        f'COUNTIF($AQ$2:$AQ${catalog_end},{base_lookup_key}&'
        f'"{EC2_DEFAULT_MI_SERVICE_TIER}")>0'
    )
    bc_available = (
        f'COUNTIF($AQ$2:$AQ${catalog_end},{base_lookup_key}&'
        '"Business Critical")>0'
    )
    source_vcores = (
        f'MAXIFS($AW$2:$AW${catalog_end},$AR$2:$AR${catalog_end},'
        f'$K$9&" | "&$B{row}&" | "&$C{row}&" | "&$D{row},'
        f'$AS$2:$AS${catalog_end},$F{row})'
    )
    worksheet.Range(f"O{row}").Formula = (
        f'=IF($B{row}="","",IF({nggp_available},IF($K{row}>MIN('
        f'{NGGP_MAX_IOPS},{NGGP_IOPS_PER_VCORE}*{nggp_vcores}),'
        f'IF({bc_available},"Business Critical","NO MAPPING"),'
        f'"{EC2_DEFAULT_MI_SERVICE_TIER}"),IF(AND({bc_available},$K{row}>MIN('
        f'{NGGP_MAX_IOPS},{NGGP_IOPS_PER_VCORE}*{source_vcores})),'
        '"Business Critical","NO MAPPING")))'
    )
    worksheet.Range(f"P{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","{NO_MI_MAPPING}",IFERROR({catalog_value("BE")}&IF('
        f'{catalog_value("BD")}="Business Critical"," / BC local SSD",'
        f'" / Remote LRS"),"{NO_MI_MAPPING}")))'
    )
    worksheet.Range(f"Q{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({catalog_value("BF")},""))'
    )
    worksheet.Range(f"R{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({source_catalog_value("AW")},""))'
    )

    worksheet.Range(f"S{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$L{row}*{source_catalog_value("AY")},0))'
    )
    worksheet.Range(f"T{row}").Formula = f'=IF($B{row}="","",S{row}*(1-$B$5))'
    license_rate = (
        f'IF($F{row}="Standard",{source_catalog_value("AZ")},{source_catalog_value("BA")})'
    )
    worksheet.Range(f"U{row}").Formula = (
        f'=IF($B{row}="","",IFERROR(IF($G{row}="License included",'
        f'$E{row}*$L{row}*$R{row}*{license_rate},0),0))'
    )
    worksheet.Range(f"V{row}").Formula = f'=IF($B{row}="","",U{row}*(1-$B$6))'
    storage_rate = (
        f'INDEX($CD$2:$CD${storage_end},MATCH('
        f'{source_catalog_value("AT")}&"|"&{source_catalog_value("AV")}&"|"&$H{row},'
        f'$BZ$2:$BZ${storage_end},0))'
    )
    worksheet.Range(f"W{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$I{row}*12*{storage_rate},0))'
    )
    worksheet.Range(f"X{row}").Formula = f'=IF($B{row}="","",W{row}*(1-$B$7))'
    worksheet.Range(f"Y{row}").Formula = f'=IF($B{row}="","",SUM(T{row},V{row},X{row}))'

    mi_compute_rate = (
        f'INDEX($BI$2:$BP${catalog_end},{lookup_match},'
        f'MATCH($M{row},$BI$1:$BP$1,0))'
    )
    mi_license_rate = (
        f'INDEX($BQ$2:$BX${catalog_end},{lookup_match},'
        f'MATCH($M{row},$BQ$1:$BX$1,0))'
    )
    worksheet.Range(f"Z{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$L{row}*{mi_compute_rate},0))'
    )
    worksheet.Range(f"AA{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$L{row}*MAX(0,$N{row}-'
        f'{catalog_value("DC")})*{catalog_value("DE")},0))'
    )
    worksheet.Range(f"AB{row}").Formula = (
        f'=IF($B{row}="","",SUM(Z{row}:AA{row})*(1-$E$5))'
    )
    worksheet.Range(f"AC{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$L{row}*{mi_license_rate},0))'
    )
    worksheet.Range(f"AD{row}").Formula = f'=IF($B{row}="","",AC{row}*(1-$E$6))'
    worksheet.Range(f"AE{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($E{row}*$I{row}*12*{catalog_value("BH")},0))'
    )
    worksheet.Range(f"AF{row}").Formula = f'=IF($B{row}="","",AE{row}*(1-$E$7))'
    worksheet.Range(f"AG{row}").Formula = f'=IF($B{row}="","",SUM(AB{row},AD{row},AF{row}))'
    worksheet.Range(f"AH{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","",T{row}-AB{row}))'
    )
    worksheet.Range(f"AI{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","",V{row}-AD{row}))'
    )
    worksheet.Range(f"AJ{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","",X{row}-AF{row}))'
    )
    worksheet.Range(f"AK{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","{NO_MI_MAPPING}",Y{row}-AG{row}))'
    )
    worksheet.Range(f"AL{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","{NO_MI_MAPPING}",IF(AG{row}=0,0,1-Y{row}/AG{row})))'
    )
    worksheet.Range(f"AM{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","",$H$5))'
    )
    worksheet.Range(f"AN{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","{NO_MI_MAPPING}",AG{row}*(1-AM{row})))'
    )
    worksheet.Range(f"AO{row}").Formula = (
        f'=IF($B{row}="","",IF($O{row}="{NO_MI_MAPPING}","{NO_MI_MAPPING}",AN{row}-Y{row}))'
    )


def set_workload_formulas(
    worksheet,
    row: int,
    catalog_end: int,
    source_kind: str,
    storage_end: int,
    memory_end: int,
) -> None:
    if source_kind == "RDS":
        set_rds_workload_formulas(
            worksheet,
            row,
            catalog_end,
            storage_end,
            memory_end,
        )
        return

    lookup_match = (
        f'MATCH($K$9&" | "&$B{row}&"|"&$D{row}&"|"&$L{row},'
        f'$AQ$2:$AQ${catalog_end},0)'
    )
    default_nggp_match = (
        f'MATCH($K$9&" | "&$B{row}&"|"&$D{row}&"|"&'
        f'"{EC2_DEFAULT_MI_SERVICE_TIER}",$AQ$2:$AQ${catalog_end},0)'
    )

    def catalog_value(column: str) -> str:
        return f"INDEX(${column}$2:${column}${catalog_end},{lookup_match})"

    memory_formula = (
        f'AGGREGATE(15,6,$DG$2:$DG${memory_end}/'
        f'(($DF$2:$DF${memory_end}={catalog_value("BC")})*'
        f'($DG$2:$DG${memory_end}>=$H{row})),1)'
    )
    worksheet.Range(f"K{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({memory_formula},"NO MAPPING"))'
    )
    nggp_vcores = f'INDEX($BF$2:$BF${catalog_end},{default_nggp_match})'
    worksheet.Range(f"L{row}").Formula = (
        f'=IF($B{row}="","",IFERROR(IF($P{row}>MIN({NGGP_MAX_IOPS},'
        f'{NGGP_IOPS_PER_VCORE}*{nggp_vcores}),"Business Critical",'
        f'"{EC2_DEFAULT_MI_SERVICE_TIER}"),"NO MAPPING"))'
    )
    worksheet.Range(f"M{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({catalog_value("BE")}&IF('
        f'{catalog_value("BD")}="Business Critical"," / BC local SSD",'
        '" / Remote LRS"),"NO MAPPING"))'
    )
    worksheet.Range(f"N{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({catalog_value("BF")},""))'
    )
    worksheet.Range(f"O{row}").Formula = (
        f'=IF($B{row}="","",IFERROR({catalog_value("AW")},""))'
    )
    worksheet.Range(f"P{row}").Formula = (
        f'=IF($B{row}="","",IFERROR(MAXIFS('
        f"'EC2 EBS Detail'!$F${EBS_DETAIL_FIRST_ROW}:$F${EBS_DETAIL_LAST_ROW},"
        f"'EC2 EBS Detail'!$A${EBS_DETAIL_FIRST_ROW}:$A${EBS_DETAIL_LAST_ROW},$F{row},"
        f"'EC2 EBS Detail'!$D${EBS_DETAIL_FIRST_ROW}:$D${EBS_DETAIL_LAST_ROW},"
        '"<>Ephemeral"),0))'
    )

    worksheet.Range(f"Q{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($C{row}*$I{row}*{catalog_value("AY")},0))'
    )
    worksheet.Range(f"R{row}").Formula = f'=IF($B{row}="","",Q{row}*(1-$B$5))'
    license_rate = (
        f'IF($D{row}="Standard",{catalog_value("AZ")},{catalog_value("BA")})'
    )
    source_license_multiplier = f"*$J{row}" if source_kind == "RDS" else ""
    worksheet.Range(f"S{row}").Formula = (
        f'=IF($B{row}="","",IFERROR(IF($E{row}="License included",'
        f'$C{row}*$I{row}{source_license_multiplier}*{license_rate},0),0))'
    )
    worksheet.Range(f"T{row}").Formula = f'=IF($B{row}="","",S{row}*(1-$B$6))'

    if source_kind == "RDS":
        storage_rate = (
            f'INDEX($CD$2:$CD${storage_end},MATCH('
            f'{catalog_value("AT")}&"|"&{catalog_value("AV")}&"|"&$F{row},'
            f'$BZ$2:$BZ${storage_end},0))'
        )
        worksheet.Range(f"U{row}").Formula = (
            f'=IF($B{row}="","",IFERROR($C{row}*$G{row}*12*{storage_rate},0))'
        )
    else:
        worksheet.Range(f"U{row}").Formula = (
            f'=IF($B{row}="","",IFERROR($C{row}*12*SUMIF('
            f"'EC2 EBS Detail'!$A${EBS_DETAIL_FIRST_ROW}:$A${EBS_DETAIL_LAST_ROW},"
            f"$F{row},'EC2 EBS Detail'!$M${EBS_DETAIL_FIRST_ROW}:$M${EBS_DETAIL_LAST_ROW}),0))"
        )
    worksheet.Range(f"V{row}").Formula = f'=IF($B{row}="","",U{row}*(1-$B$7))'
    worksheet.Range(f"W{row}").Formula = f'=IF($B{row}="","",SUM(R{row},T{row},V{row}))'

    mi_compute_rate = (
        f'INDEX($BI$2:$BP${catalog_end},{lookup_match},'
        f'MATCH($J{row},$BI$1:$BP$1,0))'
    )
    mi_license_rate = (
        f'INDEX($BQ$2:$BX${catalog_end},{lookup_match},'
        f'MATCH($J{row},$BQ$1:$BX$1,0))'
    )
    worksheet.Range(f"X{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($C{row}*$I{row}*{mi_compute_rate},0))'
    )
    worksheet.Range(f"Y{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($C{row}*$I{row}*MAX(0,$K{row}-'
        f'{catalog_value("DC")})*{catalog_value("DE")},0))'
    )
    worksheet.Range(f"Z{row}").Formula = (
        f'=IF($B{row}="","",SUM(X{row}:Y{row})*(1-$E$5))'
    )
    worksheet.Range(f"AA{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($C{row}*$I{row}*{mi_license_rate},0))'
    )
    worksheet.Range(f"AB{row}").Formula = f'=IF($B{row}="","",AA{row}*(1-$E$6))'
    worksheet.Range(f"AC{row}").Formula = (
        f'=IF($B{row}="","",IFERROR($C{row}*$G{row}*12*{catalog_value("BH")},0))'
    )
    worksheet.Range(f"AD{row}").Formula = f'=IF($B{row}="","",AC{row}*(1-$E$7))'
    worksheet.Range(f"AE{row}").Formula = f'=IF($B{row}="","",SUM(Z{row},AB{row},AD{row}))'
    worksheet.Range(f"AF{row}").Formula = f'=IF($B{row}="","",R{row}-Z{row})'
    worksheet.Range(f"AG{row}").Formula = f'=IF($B{row}="","",T{row}-AB{row})'
    worksheet.Range(f"AH{row}").Formula = f'=IF($B{row}="","",V{row}-AD{row})'
    worksheet.Range(f"AI{row}").Formula = f'=IF($B{row}="","",W{row}-AE{row})'
    worksheet.Range(f"AJ{row}").Formula = (
        f'=IF($B{row}="","",IF(AE{row}=0,0,1-W{row}/AE{row}))'
    )
    worksheet.Range(f"AK{row}").Formula = f'=IF($B{row}="","",$H$5)'
    worksheet.Range(f"AL{row}").Formula = f'=IF($B{row}="","",AE{row}*(1-AK{row}))'
    worksheet.Range(f"AM{row}").Formula = f'=IF($B{row}="","",AL{row}-W{row})'


def set_hyperlink_formula(cell, address: str, display_text: str) -> None:
    escaped_address = address.replace('"', '""')
    escaped_display_text = display_text.replace('"', '""')
    cell.Formula = (
        f'=HYPERLINK("{escaped_address}","{escaped_display_text}")'
    )


def add_local_source_link(worksheet, row: int, label: str, path: Path) -> None:
    worksheet.Cells(row, 1).Value2 = label
    set_hyperlink_formula(
        worksheet.Cells(row, 2),
        str(path),
        path.name,
    )


def build_ec2_ebs_detail_sheet(
    workbook,
    converter_sheet,
    storage_end: int,
):
    sheet_name = "EC2 EBS Detail"
    remove_sheet(workbook, sheet_name)
    worksheet = workbook.Worksheets.Add(
        None,
        workbook.Worksheets.Item(workbook.Worksheets.Count),
    )
    worksheet.Name = sheet_name
    worksheet.Tab.Color = AWS_ORANGE
    active_window = worksheet.Application.ActiveWindow
    if active_window is not None:
        active_window.DisplayGridlines = False

    merge_with_value(worksheet, "A1:M1", "EC2 EBS Per-Volume Cost Detail")
    style_range(
        worksheet.Range("A1:M1"),
        fill=NAVY,
        font_color=WHITE,
        bold=True,
        font_size=16,
        horizontal=XL_LEFT,
    )
    merge_with_value(
        worksheet,
        "A2:M2",
        "Persistent EBS capacity, provisioned IOPS, and gp3 throughput are priced per volume. Ephemeral drives are retained for inventory traceability and excluded from EBS cost.",
    )
    style_range(worksheet.Range("A2:M2"), fill=LIGHT_BLUE, wrap=True)
    worksheet.Rows(2).RowHeight = 30

    worksheet.Range("A4").Value2 = "Selected AWS region"
    worksheet.Range("B4:D4").Merge()
    worksheet.Range("B4").Formula = "='EC2 TCO Converter'!$K$9"
    worksheet.Range("E4").Value2 = "Pricing basis"
    worksheet.Range("F4:M4").Merge()
    worksheet.Range("F4").Value2 = "Regional capacity + provisioned IOPS + gp3 throughput"
    style_range(worksheet.Range("A4"), fill=LIGHT_ORANGE, bold=True, borders=True)
    style_range(
        worksheet.Range("B4:D4"),
        fill=LIGHT_ORANGE,
        bold=True,
        horizontal=XL_CENTER,
        borders=True,
    )
    style_range(worksheet.Range("E4"), fill=LIGHT_GRAY, bold=True, borders=True)
    style_range(worksheet.Range("F4:M4"), fill=LIGHT_GRAY, borders=True)

    headers = (
        "Server",
        "Drive",
        "Volume ID",
        "Type",
        "Size (GiB)",
        "Provisioned IOPS",
        "Throughput (MiB/s)",
        "Billable IOPS",
        "Billable MiB/s",
        "Capacity $/mo",
        "IOPS $/mo",
        "Throughput $/mo",
        "Total EBS $/mo",
    )
    write_matrix(worksheet, "A6", [headers])
    style_range(
        worksheet.Range("A6:M6"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )
    worksheet.Rows(6).RowHeight = 42
    source_rows = [
        (
            server,
            drive,
            volume_id,
            volume_type,
            storage_gb,
            "" if iops is None else iops,
            "" if throughput is None else throughput,
        )
        for server, drive, volume_id, volume_type, storage_gb, iops, throughput in EBS_VOLUME_DETAILS
    ]
    write_matrix(worksheet, f"A{EBS_DETAIL_FIRST_ROW}", source_rows)

    converter_name = converter_sheet.Name.replace("'", "''")
    for row in range(EBS_DETAIL_FIRST_ROW, EBS_DETAIL_LAST_ROW + 1):
        rate_match = (
            f'MATCH($B$4&"|"&$D{row},'
            f"'{converter_name}'!$CQ$2:$CQ${storage_end},0)"
        )

        def rate_value(column: str) -> str:
            return (
                f"INDEX('{converter_name}'!${column}$2:${column}${storage_end},"
                f"{rate_match})"
            )

        included_iops = rate_value("CU")
        tier_1_rate = rate_value("CV")
        tier_2_rate = rate_value("CW")
        tier_3_rate = rate_value("CX")
        included_throughput = rate_value("CY")
        throughput_rate = rate_value("CZ")
        worksheet.Range(f"H{row}").Formula = (
            f'=IF(OR($A{row}="",$D{row}="Ephemeral"),0,'
            f'IFERROR(IF({included_iops}>0,MAX(0,$F{row}-{included_iops}),$F{row}),0))'
        )
        worksheet.Range(f"I{row}").Formula = (
            f'=IF(OR($A{row}="",$D{row}="Ephemeral"),0,'
            f'IFERROR(IF({throughput_rate}=0,0,'
            f'MAX(0,$G{row}-{included_throughput})),0))'
        )
        worksheet.Range(f"J{row}").Formula = (
            f'=IF(OR($A{row}="",$D{row}="Ephemeral"),0,'
            f'IFERROR($E{row}*{rate_value("CT")},0))'
        )
        worksheet.Range(f"K{row}").Formula = (
            f'=IF(OR($A{row}="",$D{row}="Ephemeral"),0,IFERROR('
            f'IF({included_iops}>0,$H{row}*{tier_1_rate},'
            f'MIN($F{row},32000)*{tier_1_rate}+'
            f'MAX(0,MIN($F{row}-32000,32000))*{tier_2_rate}+'
            f'MAX(0,$F{row}-64000)*{tier_3_rate}),0))'
        )
        worksheet.Range(f"L{row}").Formula = (
            f'=IF(OR($A{row}="",$D{row}="Ephemeral"),0,'
            f'IFERROR($I{row}*{throughput_rate},0))'
        )
        worksheet.Range(f"M{row}").Formula = f"=SUM(J{row}:L{row})"

    style_range(
        worksheet.Range(f"A{EBS_DETAIL_FIRST_ROW}:G{EBS_DETAIL_LAST_ROW}"),
        fill=YELLOW,
        borders=True,
    )
    style_range(
        worksheet.Range(f"H{EBS_DETAIL_FIRST_ROW}:M{EBS_DETAIL_LAST_ROW}"),
        fill=LIGHT_ORANGE,
        borders=True,
    )
    worksheet.Range(f"E{EBS_DETAIL_FIRST_ROW}:I{EBS_DETAIL_LAST_ROW}").NumberFormat = "#,##0"
    worksheet.Range(f"J{EBS_DETAIL_FIRST_ROW}:M{EBS_DETAIL_LAST_ROW}").NumberFormat = "$#,##0.00"
    add_list_validation(
        worksheet.Range(f"D{EBS_DETAIL_FIRST_ROW}:D{EBS_DETAIL_LAST_ROW}"),
        "=EC2_TCO_Storage_List",
        "EBS volume type",
    )
    add_number_validation(
        worksheet.Range(f"E{EBS_DETAIL_FIRST_ROW}:E{EBS_DETAIL_LAST_ROW}"),
        XL_VALIDATE_DECIMAL,
        0,
        1_000_000_000,
        "Volume size",
    )
    add_number_validation(
        worksheet.Range(f"F{EBS_DETAIL_FIRST_ROW}:F{EBS_DETAIL_LAST_ROW}"),
        XL_VALIDATE_WHOLE_NUMBER,
        0,
        256_000,
        "Provisioned IOPS",
    )
    add_number_validation(
        worksheet.Range(f"G{EBS_DETAIL_FIRST_ROW}:G{EBS_DETAIL_LAST_ROW}"),
        XL_VALIDATE_DECIMAL,
        0,
        4_000,
        "Provisioned throughput",
    )

    summary_profiles = list(dict.fromkeys(row[0] for row in EBS_VOLUME_DETAILS))
    summary_header_row = EBS_DETAIL_LAST_ROW + 3
    summary_first_row = summary_header_row + 2
    merge_with_value(
        worksheet,
        f"A{summary_header_row}:I{summary_header_row}",
        "SERVER PROFILE SUMMARY",
    )
    style_range(
        worksheet.Range(f"A{summary_header_row}:I{summary_header_row}"),
        fill=AWS_ORANGE,
        font_color=WHITE,
        bold=True,
        borders=True,
    )
    summary_headers = (
        "Server",
        "EBS volumes",
        "EBS GiB",
        "Ephemeral GiB",
        "Provisioned IOPS",
        "Provisioned MiB/s",
        "Billable IOPS",
        "Billable MiB/s",
        "EBS $/month",
    )
    write_matrix(worksheet, f"A{summary_header_row + 1}", [summary_headers])
    style_range(
        worksheet.Range(f"A{summary_header_row + 1}:I{summary_header_row + 1}"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )
    detail_server_range = f"$A${EBS_DETAIL_FIRST_ROW}:$A${EBS_DETAIL_LAST_ROW}"
    detail_type_range = f"$D${EBS_DETAIL_FIRST_ROW}:$D${EBS_DETAIL_LAST_ROW}"
    for offset, profile in enumerate(summary_profiles):
        row = summary_first_row + offset
        worksheet.Range(f"A{row}").Value2 = profile
        worksheet.Range(f"B{row}").Formula = (
            f'=COUNTIFS({detail_server_range},$A{row},{detail_type_range},"<>Ephemeral")'
        )
        for column, source_column, criterion in (
            ("C", "E", "<>Ephemeral"),
            ("D", "E", "Ephemeral"),
            ("E", "F", "<>Ephemeral"),
            ("F", "G", "<>Ephemeral"),
        ):
            worksheet.Range(f"{column}{row}").Formula = (
                f'=SUMIFS(${source_column}${EBS_DETAIL_FIRST_ROW}:'
                f'${source_column}${EBS_DETAIL_LAST_ROW},'
                f'{detail_server_range},$A{row},{detail_type_range},"{criterion}")'
            )
        worksheet.Range(f"G{row}").Formula = (
            f'=SUMIFS($H${EBS_DETAIL_FIRST_ROW}:$H${EBS_DETAIL_LAST_ROW},'
            f'{detail_server_range},$A{row})'
        )
        worksheet.Range(f"H{row}").Formula = (
            f'=SUMIFS($I${EBS_DETAIL_FIRST_ROW}:$I${EBS_DETAIL_LAST_ROW},'
            f'{detail_server_range},$A{row})'
        )
        worksheet.Range(f"I{row}").Formula = (
            f'=SUMIFS($M${EBS_DETAIL_FIRST_ROW}:$M${EBS_DETAIL_LAST_ROW},'
            f'{detail_server_range},$A{row})'
        )
    summary_last_row = summary_first_row + len(summary_profiles) - 1
    total_row = summary_last_row + 1
    worksheet.Range(f"A{total_row}").Value2 = "TOTAL / DISTINCT PROFILES"
    for column in "BCDEFGHI":
        worksheet.Range(f"{column}{total_row}").Formula = (
            f"=SUM({column}{summary_first_row}:{column}{summary_last_row})"
        )
    style_range(
        worksheet.Range(f"A{summary_first_row}:I{summary_last_row}"),
        fill=LIGHT_ORANGE,
        borders=True,
    )
    style_range(
        worksheet.Range(f"A{total_row}:I{total_row}"),
        fill=NAVY,
        font_color=WHITE,
        bold=True,
        borders=True,
    )
    worksheet.Range(f"B{summary_first_row}:H{total_row}").NumberFormat = "#,##0"
    worksheet.Range(f"I{summary_first_row}:I{total_row}").NumberFormat = "$#,##0.00"

    note_row = total_row + 3
    notes = (
        "Source: per-volume EC2 storage inventory supplied in the pasted image.",
        "gp3 includes 3,000 IOPS and 125 MiB/s per volume; only provisioned amounts above those baselines are billed as add-ons.",
        "Ephemeral drives are shown for completeness but are excluded from EBS capacity and cost.",
        "The EC2 converter multiplies each selected server profile by workload quantity; AWOMPSQLV104 inherits the AWOMPSQLV101 profile because no secondary-node detail was supplied.",
    )
    for offset, note in enumerate(notes):
        row = note_row + offset
        worksheet.Range(f"A{row}").Value2 = f"Note {offset + 1}"
        worksheet.Range(f"B{row}:M{row}").Merge()
        worksheet.Range(f"B{row}").Value2 = note
    style_range(
        worksheet.Range(f"A{note_row}:M{note_row + len(notes) - 1}"),
        wrap=True,
        borders=True,
    )
    style_range(
        worksheet.Range(f"A{note_row}:A{note_row + len(notes) - 1}"),
        fill=LIGHT_GRAY,
        bold=True,
    )
    worksheet.Rows(f"{note_row}:{note_row + len(notes) - 1}").RowHeight = 30

    widths = {
        "A": 19,
        "B": 8,
        "C": 24,
        "D": 12,
        "E": 12,
        "F": 16,
        "G": 18,
        "H": 14,
        "I": 15,
        "J": 14,
        "K": 13,
        "L": 17,
        "M": 16,
    }
    for column, width in widths.items():
        worksheet.Columns(column).ColumnWidth = width
    worksheet.Range(f"A1:M{note_row + len(notes) - 1}").Font.Name = "Aptos"
    worksheet.Range(f"A6:M{EBS_DETAIL_LAST_ROW}").VerticalAlignment = XL_CENTER
    worksheet.Range(f"A6:M{EBS_DETAIL_LAST_ROW}").AutoFilter()
    worksheet.Activate()
    active_window = worksheet.Application.ActiveWindow
    if active_window is not None:
        active_window.FreezePanes = False
        active_window.SplitColumn = 0
        active_window.SplitRow = 6
        active_window.FreezePanes = True
        active_window.ScrollColumn = 1
        active_window.ScrollRow = EBS_DETAIL_FIRST_ROW
        active_window.Zoom = 80
    try:
        worksheet.PageSetup.PrintArea = f"$A$1:$M${note_row + len(notes) - 1}"
        worksheet.PageSetup.Orientation = XL_LANDSCAPE
        worksheet.PageSetup.Zoom = False
        worksheet.PageSetup.FitToPagesWide = 1
        worksheet.PageSetup.FitToPagesTall = 2
    except pywintypes.com_error:
        pass

    set_hyperlink_formula(
        converter_sheet.Range("K8"),
        f"#'{sheet_name}'!A1",
        "Open per-volume detail",
    )
    converter_sheet.Activate()
    return worksheet


def build_converter_sheet(
    workbook,
    catalog: list[dict[str, object]],
    source_kind: str,
    rds_storage_catalog: list[dict[str, object]] | None = None,
    preserved_inputs: dict[str, object] | None = None,
):
    is_rds = source_kind == "RDS"
    sheet_name = "RDS SQL MI TCO Converter" if is_rds else "EC2 TCO Converter"
    prefix = "RDS_TCO" if is_rds else "EC2_TCO"
    last_column = "AO" if is_rds else "AM"
    for suffix in (
        "Config_List",
        "Edition_List",
        "License_List",
        "Purchase_List",
        "Storage_List",
        "Region_List",
        "EBS_Profile_List",
        "MI_Tier_List",
    ):
        remove_name(workbook, f"{prefix}_{suffix}")
    remove_sheet(workbook, sheet_name)
    worksheet = workbook.Worksheets.Add(
        None,
        workbook.Worksheets.Item(workbook.Worksheets.Count),
    )
    worksheet.Name = sheet_name
    worksheet.Tab.Color = AWS_ORANGE if is_rds else BLUE
    worksheet.Activate()
    active_window = worksheet.Application.ActiveWindow
    if active_window is not None:
        active_window.DisplayGridlines = False

    title = (
        "RDS to Azure SQL Managed Instance TCO Converter"
        if is_rds
        else "EC2 SQL Server to Azure SQL Managed Instance TCO Converter"
    )
    subtitle = (
        "Choose the AWS source configuration, then enter source RAM and provisioned IOPS. Azure MI RAM and service tier are derived automatically in Sweden Central."
        if is_rds
        else "Enter the AWS source RAM and EBS profile. Azure MI RAM is sized automatically, and source peak provisioned IOPS determines Next Generation General Purpose or Business Critical."
    )
    merge_with_value(worksheet, f"A1:{last_column}1", title)
    style_range(
        worksheet.Range(f"A1:{last_column}1"),
        fill=NAVY,
        font_color=WHITE,
        bold=True,
        font_size=16,
        horizontal=XL_LEFT,
    )
    worksheet.Rows(1).RowHeight = 30
    merge_with_value(worksheet, f"A2:{last_column}2", subtitle)
    style_range(worksheet.Range(f"A2:{last_column}2"), fill=LIGHT_BLUE, wrap=True)
    worksheet.Rows(2).RowHeight = 28

    for address, label, color in (
        ("A4:B4", "AWS component discounts", AWS_ORANGE),
        ("D4:E4", "Azure / MACC component discounts", BLUE),
        ("G4:H4", "Additional parity adjustment", PLUM),
        ("J4:M4", "Model basis", DARK_GRAY),
    ):
        merge_with_value(worksheet, address, label)
        style_range(
            worksheet.Range(address),
            fill=color,
            font_color=WHITE,
            bold=True,
            horizontal=XL_CENTER,
            borders=True,
        )

    assumptions = (
        (5, "Compute", 0.10, "Compute", DEFAULT_AZURE_COMPONENT_DISCOUNT),
        (6, "SQL license", 0.05, "SQL license", DEFAULT_AZURE_COMPONENT_DISCOUNT),
        (7, "Storage", 0.05, "Storage", DEFAULT_AZURE_COMPONENT_DISCOUNT),
    )
    for row, aws_label, aws_value, azure_label, azure_value in assumptions:
        worksheet.Cells(row, 1).Value2 = aws_label
        worksheet.Cells(row, 2).Value2 = aws_value
        worksheet.Cells(row, 4).Value2 = azure_label
        worksheet.Cells(row, 5).Value2 = azure_value
    worksheet.Range("G5").Value2 = "Selected adjustment"
    worksheet.Range("H5").Value2 = 0.0
    worksheet.Range("G6").Value2 = "Required for parity"
    worksheet.Range("H6").Formula = "=$AL$23" if is_rds else "=$AJ$23"
    worksheet.Range("G7").Value2 = "Difference at selected"
    worksheet.Range("H7").Formula = "=$AO$23" if is_rds else "=$AM$23"
    worksheet.Range("G8").Value2 = "Result"
    worksheet.Range("H8").Formula = (
        '=IF(ABS(H7)<1,"PARITY",IF(H6<0,"AZURE ALREADY LOWER",'
        'IF(H6>1,"NO FEASIBLE DISCOUNT",IF(H7>0,"AZURE HIGHER","AZURE LOWER"))))'
    )
    basis = (
        (5, "Currency", "USD, tax excluded"),
        (6, "Output period", "Annual run-rate"),
        (7, "Storage basis", "GB-month" if is_rds else "Per-volume EBS + SQL data"),
        (
            8,
            "Hours" if is_rds else "EBS cost detail",
            "Editable by workload" if is_rds else "Open per-volume detail",
        ),
    )
    for row, label, value in basis:
        worksheet.Cells(row, 10).Value2 = label
        worksheet.Range(f"K{row}:M{row}").Merge()
        worksheet.Cells(row, 11).Value2 = value
    worksheet.Range("J9").Value2 = "AWS region"
    worksheet.Range("K9:M9").Merge()
    worksheet.Range("K9").Value2 = "eu-west-1"
    worksheet.Range("J10").Value2 = "Azure migration region"
    worksheet.Range("K10:M10").Merge()
    worksheet.Range("K10").Value2 = (
        RDS_AZURE_MIGRATION_REGION_LABEL
        if is_rds
        else EC2_AZURE_MIGRATION_REGION_LABEL
    )

    style_range(worksheet.Range("A5:A7"), fill=LIGHT_ORANGE, bold=True, borders=True)
    style_range(worksheet.Range("B5:B7"), fill=YELLOW, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("D5:D7"), fill=LIGHT_BLUE, bold=True, borders=True)
    style_range(worksheet.Range("E5:E7"), fill=YELLOW, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("G5:G8"), fill=LIGHT_PLUM, bold=True, borders=True)
    style_range(worksheet.Range("H5"), fill=YELLOW, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("H6:H8"), fill=LIGHT_PLUM, bold=True, horizontal=XL_CENTER, borders=True)
    style_range(worksheet.Range("J5:J8"), fill=LIGHT_GRAY, bold=True, borders=True)
    style_range(worksheet.Range("K5:M8"), fill=LIGHT_GRAY, borders=True)
    style_range(worksheet.Range("J9"), fill=LIGHT_ORANGE, bold=True, borders=True)
    style_range(
        worksheet.Range("K9:M9"),
        fill=YELLOW,
        bold=True,
        horizontal=XL_CENTER,
        borders=True,
    )
    style_range(worksheet.Range("J10"), fill=LIGHT_BLUE, bold=True, borders=True)
    style_range(
        worksheet.Range("K10:M10"),
        fill=LIGHT_BLUE,
        bold=True,
        horizontal=XL_CENTER,
        borders=True,
    )
    worksheet.Range("B5:B7").NumberFormat = "0.0%"
    worksheet.Range("E5:E7").NumberFormat = "0.0%"
    worksheet.Range("H5:H6").NumberFormat = "0.00%"
    worksheet.Range("H7").NumberFormat = "$#,##0"

    group_headers = (
        (
            ("A11:M11", "WORKLOAD INPUTS", AWS_ORANGE),
            ("N11:Q11", "AUTOMATIC AZURE MI TARGET", BLUE),
            ("R11:Y11", "AWS CURRENT STATE", AWS_ORANGE),
            ("Z11:AG11", "AZURE SQL MI", BLUE),
            ("AH11:AK11", "SAVINGS BEFORE PARITY", GREEN),
            ("AL11:AO11", "PARITY", PLUM),
        )
        if is_rds
        else (
            ("A11:J11", "WORKLOAD INPUTS", AWS_ORANGE),
            ("K11:N11", "AUTOMATIC AZURE MI TARGET", BLUE),
            ("O11:W11", "AWS CURRENT STATE", AWS_ORANGE),
            ("X11:AE11", "AZURE SQL MI", BLUE),
            ("AF11:AI11", "SAVINGS BEFORE PARITY", GREEN),
            ("AJ11:AM11", "PARITY", PLUM),
        )
    )
    for address, label, color in group_headers:
        merge_with_value(worksheet, address, label)
        style_range(
            worksheet.Range(address),
            fill=color,
            font_color=WHITE,
            bold=True,
            horizontal=XL_CENTER,
            borders=True,
        )

    input_headers = (
        (
            "Workload",
            "RDS instance type",
            "Deployment",
            "Commercial term",
            "Deployment qty",
            "AWS SQL edition",
            "AWS licence basis",
            "RDS storage class",
            "SQL data GB / instance",
            "Source RAM GB / instance",
            "Source max IOPS",
            "Annual hours / instance",
            "MI purchase option",
        )
        if is_rds
        else (
            "Workload",
            "EC2 instance type",
            "Qty",
            "AWS SQL edition",
            "AWS licence basis",
            "EBS server profile",
            "SQL data GB / instance",
            "Source RAM GB / instance",
            "Annual hours / instance",
            "MI purchase option",
        )
    )
    output_headers = (
        (
            "MI RAM GB",
            "MI service tier",
            "MI hardware / data storage",
            "MI vCores",
            "Source vCPU",
            "Compute gross",
            "Compute net",
            "SQL license gross",
            "SQL license net",
            "Storage gross",
            "Storage net",
            "AWS net total",
            "Compute gross",
            "Additional RAM gross",
            "Compute + RAM net",
            "SQL license gross",
            "SQL license net",
            "Storage gross",
            "Storage net",
            "MI net before parity",
            "Compute",
            "SQL license",
            "Storage",
            "Total",
            "Required adj.",
            "Selected adj.",
            "MI after parity",
            "Difference",
        )
        if is_rds
        else (
            "MI RAM GB",
            "MI service tier",
            "MI hardware / data storage",
            "MI vCores",
            "Source vCPU",
            "Source max IOPS",
            "Compute gross",
            "Compute net",
            "SQL license gross",
            "SQL license net",
            "Storage gross",
            "Storage net",
            "AWS net total",
            "Compute gross",
            "Additional RAM gross",
            "Compute + RAM net",
            "SQL license gross",
            "SQL license net",
            "Storage gross",
            "Storage net",
            "MI net before parity",
            "Compute",
            "SQL license",
            "Storage",
            "Total",
            "Required adj.",
            "Selected adj.",
            "MI after parity",
            "Difference",
        )
    )
    headers = input_headers + output_headers
    write_matrix(worksheet, "A12", [headers])
    style_range(
        worksheet.Range(f"A12:{last_column}12"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        horizontal=XL_CENTER,
        wrap=True,
        borders=True,
    )
    worksheet.Rows(12).RowHeight = 52

    helper_bounds = write_hidden_catalog(
        workbook,
        worksheet,
        catalog,
        prefix,
        rds_storage_catalog,
    )
    if not is_rds:
        build_ec2_ebs_detail_sheet(
            workbook,
            worksheet,
            helper_bounds["storage_end"],
        )
    for row in range(13, 23):
        set_workload_formulas(
            worksheet,
            row,
            helper_bounds["catalog_end"],
            source_kind,
            helper_bounds["storage_end"],
            helper_bounds["memory_end"],
        )

    sample_labels = ("Customer portal", "ERP primary", "Reporting", "Line of business", "Development")
    sample_editions = ("Standard", "Enterprise", "Standard", "Enterprise", "Standard")
    sample_licenses = ("License included", "License included", "BYOL", "BYOL", "License included")
    sample_purchases = (DEFAULT_MI_PURCHASE_OPTION,) * len(sample_labels)
    if is_rds:
        selected_region = str(worksheet.Range("K9").Value2)
        sample_selections = available_rds_sample_selections(catalog, selected_region, 5)
        for index, (instance_type, deployment, commercial) in enumerate(sample_selections):
            row = 13 + index
            worksheet.Cells(row, 1).Value2 = sample_labels[index]
            worksheet.Cells(row, 2).Value2 = instance_type
            worksheet.Cells(row, 3).Value2 = deployment
            worksheet.Cells(row, 4).Value2 = commercial
            worksheet.Cells(row, 5).Value2 = 1 if index != 4 else 2
            worksheet.Cells(row, 6).Value2 = sample_editions[index]
            worksheet.Cells(row, 7).Value2 = sample_licenses[index]
            compatible_storage = [
                storage
                for storage in (rds_storage_catalog or [])
                if storage["region"] == selected_region
                and storage["deployment"] == deployment
            ]
            if not compatible_storage:
                raise RuntimeError(
                    f"No RDS storage rate for {selected_region} | {deployment}"
                )
            compatible_storage.sort(key=lambda item: numeric(item["source_monthly_per_gb"]))
            worksheet.Cells(row, 8).Value2 = compatible_storage[0]["volume_type"]
            worksheet.Cells(row, 9).Value2 = (512, 2048, 1024, 4096, 256)[index]
            configuration = " | ".join(
                (selected_region, instance_type, deployment, commercial)
            )
            source_catalog_row = next(
                item
                for item in catalog
                if item["display_key"] == configuration
                and item["mi_service_tier_selection"]
                == EC2_DEFAULT_MI_SERVICE_TIER
            )
            worksheet.Cells(row, 10).Value2 = source_catalog_row["memory_gib"]
            worksheet.Cells(row, 11).Value2 = 0
            worksheet.Cells(row, 12).Value2 = (8_760, 8_760, 4_380, 8_760, 2_080)[index]
            worksheet.Cells(row, 13).Value2 = sample_purchases[index]
    else:
        selected_region = str(worksheet.Range("K9").Value2)
        available_selections = {
            (str(item["instance_type"]), str(item["edition"]))
            for item in catalog
            if item["region"] == selected_region
            and item["mi_service_tier_selection"] == EC2_DEFAULT_MI_SERVICE_TIER
        }
        missing_selections = sorted(
            {
                (instance_type, EC2_PRIMED_EDITIONS[index])
                for index, (_, instance_type, _, _) in enumerate(EC2_PRIMED_WORKLOADS)
                if (instance_type, EC2_PRIMED_EDITIONS[index])
                not in available_selections
            }
        )
        if missing_selections:
            raise RuntimeError(
                f"EC2 inventory selections are unavailable in {selected_region}: "
                + ", ".join(
                    f"{instance_type} / {edition}"
                    for instance_type, edition in missing_selections
                )
            )
        for index, (label, instance_type, quantity, storage_gb) in enumerate(
            EC2_PRIMED_WORKLOADS
        ):
            row = 13 + index
            worksheet.Cells(row, 1).Value2 = label
            worksheet.Cells(row, 2).Value2 = instance_type
            worksheet.Cells(row, 3).Value2 = quantity
            worksheet.Cells(row, 4).Value2 = EC2_PRIMED_EDITIONS[index]
            worksheet.Cells(row, 5).Value2 = EC2_PRIMED_LICENSE_BASES[index]
            worksheet.Cells(row, 6).Value2 = EC2_PRIMED_EBS_PROFILES[index]
            worksheet.Cells(row, 7).Value2 = storage_gb
            worksheet.Cells(row, 8).Value2 = EC2_PRIMED_MEMORY_GB[index]
            worksheet.Cells(row, 9).Value2 = 8_760
            worksheet.Cells(row, 10).Value2 = DEFAULT_MI_PURCHASE_OPTION

    if preserved_inputs:
        for address, value in preserved_inputs["assumptions"].items():
            worksheet.Range(address).Value2 = value
        for row, values in preserved_inputs["rows"].items():
            if is_rds:
                worksheet.Range(f"A{row}:I{row}").Value2 = (values,)
        if is_rds:
            source_memory_by_configuration = {
                str(item["display_key"]): numeric(item["memory_gib"])
                for item in catalog
            }
            selected_region = str(worksheet.Range("K9").Value2)
            for row in range(13, 23):
                if not worksheet.Cells(row, 2).Value2:
                    continue
                configuration = " | ".join(
                    (
                        selected_region,
                        str(worksheet.Cells(row, 2).Value2 or ""),
                        str(worksheet.Cells(row, 3).Value2 or ""),
                        str(worksheet.Cells(row, 4).Value2 or ""),
                    )
                )
                worksheet.Cells(row, 10).Value2 = preserved_inputs.get(
                    "source_memory", {}
                ).get(
                    row,
                    source_memory_by_configuration.get(configuration, 0.0),
                )
                worksheet.Cells(row, 11).Value2 = preserved_inputs.get(
                    "source_iops", {}
                ).get(row, 0.0)
                worksheet.Cells(row, 12).Value2 = preserved_inputs.get(
                    "annual_hours", {}
                ).get(row, 8_760)
                worksheet.Cells(row, 13).Value2 = preserved_inputs.get(
                    "purchase_options", {}
                ).get(row, DEFAULT_MI_PURCHASE_OPTION)
        if not is_rds:
            for row, memory_gb in preserved_inputs.get("source_memory", {}).items():
                if worksheet.Cells(row, 2).Value2:
                    worksheet.Cells(row, 8).Value2 = memory_gb
            for row, annual_hours in preserved_inputs.get("annual_hours", {}).items():
                if worksheet.Cells(row, 2).Value2:
                    worksheet.Cells(row, 9).Value2 = annual_hours
            for row, purchase_option in preserved_inputs.get("purchase_options", {}).items():
                if worksheet.Cells(row, 2).Value2:
                    worksheet.Cells(row, 10).Value2 = purchase_option

    worksheet.Range("A23").Value2 = "TOTAL / PORTFOLIO"
    if is_rds:
        worksheet.Range("A23:R23").Merge()
        for column in range(19, 38):
            column_letter = excel_column_name(column)
            worksheet.Cells(23, column).Formula = f"=SUM({column_letter}13:{column_letter}22)"
        mapped_aws_total = (
            f'SUMIFS(Y13:Y22,O13:O22,"<>{NO_MI_MAPPING}")'
        )
        worksheet.Range("AL23").Formula = (
            f"=IF(AG23=0,0,1-{mapped_aws_total}/AG23)"
        )
        worksheet.Range("AM23").Formula = "=$H$5"
        worksheet.Range("AN23").Formula = "=AG23*(1-AM23)"
        worksheet.Range("AO23").Formula = f"=AN23-{mapped_aws_total}"
    else:
        worksheet.Range("A23:P23").Merge()
        for column in range(17, 36):
            column_letter = excel_column_name(column)
            worksheet.Cells(23, column).Formula = f"=SUM({column_letter}13:{column_letter}22)"
        worksheet.Range("AJ23").Formula = "=IF(AE23=0,0,1-W23/AE23)"
        worksheet.Range("AK23").Formula = "=$H$5"
        worksheet.Range("AL23").Formula = "=AE23*(1-AK23)"
        worksheet.Range("AM23").Formula = "=AL23-W23"
    style_range(
        worksheet.Range(f"A23:{last_column}23"),
        fill=NAVY,
        font_color=WHITE,
        bold=True,
        borders=True,
    )

    if is_rds:
        style_range(worksheet.Range("A13:M22"), fill=YELLOW, borders=True, wrap=True)
        style_range(worksheet.Range("N13:Q22"), fill=LIGHT_BLUE, borders=True, wrap=True)
        style_range(worksheet.Range("R13:Y22"), fill=LIGHT_ORANGE, borders=True)
        style_range(worksheet.Range("Z13:AG22"), fill=LIGHT_BLUE, borders=True)
        style_range(worksheet.Range("AH13:AK22"), fill=LIGHT_GREEN, borders=True)
        style_range(worksheet.Range("AL13:AO22"), fill=LIGHT_PLUM, borders=True)
        worksheet.Range("S13:AK23").NumberFormat = "$#,##0"
        worksheet.Range("AN13:AO23").NumberFormat = "$#,##0"
        worksheet.Range("AL13:AM23").NumberFormat = "0.00%"
        worksheet.Range("H13:H22").NumberFormat = "General"
        worksheet.Range("I13:I22").NumberFormat = "0.00"
        worksheet.Range("J13:J22").NumberFormat = "0"
        worksheet.Range("K13:L22").NumberFormat = "#,##0"
        worksheet.Range("N13:N22").NumberFormat = "0"
        worksheet.Range("Q13:R22").NumberFormat = "0"

        add_list_validation(worksheet.Range("K9"), f"={prefix}_Region_List", "AWS region")
        for row in range(13, 23):
            instance_formula = (
                f'=OFFSET($CL$2,MATCH($K$9,$CK$2:$CK${helper_bounds["instance_end"]},0)-1,0,'
                f'COUNTIF($CK$2:$CK${helper_bounds["instance_end"]},$K$9),1)'
            )
            deployment_key = f'$K$9&"|"&$B{row}'
            deployment_formula = (
                f'=OFFSET($CN$2,IFERROR(MATCH({deployment_key},$CM$2:$CM${helper_bounds["deployment_end"]},0)-1,0),0,'
                f'MAX(1,COUNTIF($CM$2:$CM${helper_bounds["deployment_end"]},{deployment_key})),1)'
            )
            commercial_key = f'$K$9&"|"&$B{row}&"|"&$C{row}'
            commercial_formula = (
                f'=OFFSET($CP$2,IFERROR(MATCH({commercial_key},$CO$2:$CO${helper_bounds["commercial_end"]},0)-1,0),0,'
                f'MAX(1,COUNTIF($CO$2:$CO${helper_bounds["commercial_end"]},{commercial_key})),1)'
            )
            add_list_validation(worksheet.Cells(row, 2), instance_formula, "RDS instance type")
            add_list_validation(worksheet.Cells(row, 3), deployment_formula, "Deployment")
            add_list_validation(worksheet.Cells(row, 4), commercial_formula, "Commercial term")
        add_number_validation(worksheet.Range("E13:E22"), XL_VALIDATE_WHOLE_NUMBER, 1, 10_000, "Quantity")
        add_list_validation(worksheet.Range("F13:F22"), f"={prefix}_Edition_List", "SQL edition")
        add_list_validation(worksheet.Range("G13:G22"), f"={prefix}_License_List", "Source license")
        add_list_validation(worksheet.Range("H13:H22"), f"={prefix}_Storage_List", "RDS storage class")
        add_number_validation(worksheet.Range("I13:I22"), XL_VALIDATE_DECIMAL, 0, 1_000_000_000, "Storage GB")
        add_number_validation(worksheet.Range("J13:J22"), XL_VALIDATE_DECIMAL, 1, 1_000_000, "Source RAM GB")
        add_number_validation(worksheet.Range("K13:K22"), XL_VALIDATE_WHOLE_NUMBER, 0, 1_000_000_000, "Source max IOPS")
        add_number_validation(worksheet.Range("L13:L22"), XL_VALIDATE_DECIMAL, 0, 8_784, "Annual hours")
        add_list_validation(worksheet.Range("M13:M22"), f"={prefix}_Purchase_List", "MI purchase option")
    else:
        style_range(worksheet.Range("A13:J22"), fill=YELLOW, borders=True, wrap=True)
        style_range(worksheet.Range("K13:N22"), fill=LIGHT_BLUE, borders=True, wrap=True)
        style_range(worksheet.Range("O13:W22"), fill=LIGHT_ORANGE, borders=True)
        style_range(worksheet.Range("X13:AE22"), fill=LIGHT_BLUE, borders=True)
        style_range(worksheet.Range("AF13:AI22"), fill=LIGHT_GREEN, borders=True)
        style_range(worksheet.Range("AJ13:AM22"), fill=LIGHT_PLUM, borders=True)
        worksheet.Range("Q13:AI23").NumberFormat = "$#,##0"
        worksheet.Range("AL13:AM23").NumberFormat = "$#,##0"
        worksheet.Range("AJ13:AK23").NumberFormat = "0.00%"
        worksheet.Range("F13:F22").NumberFormat = "General"
        worksheet.Range("G13:G22").NumberFormat = "0.00"
        worksheet.Range("H13:H22").NumberFormat = "0"
        worksheet.Range("I13:I22").NumberFormat = "#,##0"
        worksheet.Range("K13:K22").NumberFormat = "0"
        worksheet.Range("N13:O22").NumberFormat = "0"
        worksheet.Range("P13:P22").NumberFormat = "#,##0"

        add_list_validation(worksheet.Range("K9"), f"={prefix}_Region_List", "AWS region")
        instance_formula = (
            f'=OFFSET($CL$2,MATCH($K$9,$CK$2:$CK${helper_bounds["instance_end"]},0)-1,0,'
            f'COUNTIF($CK$2:$CK${helper_bounds["instance_end"]},$K$9),1)'
        )
        for row in range(13, 23):
            add_list_validation(
                worksheet.Cells(row, 2),
                instance_formula,
                "EC2 instance type",
            )
        add_number_validation(worksheet.Range("C13:C22"), XL_VALIDATE_WHOLE_NUMBER, 1, 10_000, "Quantity")
        add_list_validation(worksheet.Range("D13:D22"), f"={prefix}_Edition_List", "SQL edition")
        add_list_validation(worksheet.Range("E13:E22"), f"={prefix}_License_List", "Source license")
        add_list_validation(worksheet.Range("F13:F22"), f"={prefix}_EBS_Profile_List", "EBS server profile")
        add_number_validation(worksheet.Range("G13:G22"), XL_VALIDATE_DECIMAL, 0, 1_000_000_000, "SQL data GB")
        add_number_validation(worksheet.Range("H13:H22"), XL_VALIDATE_DECIMAL, 1, 1_000_000, "Source RAM GB")
        add_number_validation(worksheet.Range("I13:I22"), XL_VALIDATE_DECIMAL, 0, 8_784, "Annual hours")
        add_list_validation(worksheet.Range("J13:J22"), f"={prefix}_Purchase_List", "MI purchase option")
    for address in ("B5:B7", "E5:E7", "H5"):
        add_number_validation(worksheet.Range(address), XL_VALIDATE_DECIMAL, 0, 1, "Discount")

    merge_with_value(
        worksheet,
        f"A26:{last_column}26",
        "Sources, Scope, and Interpretation",
    )
    style_range(
        worksheet.Range(f"A26:{last_column}26"),
        fill=DARK_GRAY,
        font_color=WHITE,
        bold=True,
        borders=True,
    )
    if is_rds:
        add_local_source_link(worksheet, 27, "AWS RDS prices and mappings", RDS_MAPPING_PATH)
    else:
        add_local_source_link(worksheet, 27, "AWS EC2 prices", EC2_PATH)
        add_local_source_link(worksheet, 28, "EC2 to SQL MI mappings", EC2_MAPPING_PATH)
    add_local_source_link(worksheet, 29, "Azure SQL MI prices", SQLMI_PATH)
    if not is_rds:
        worksheet.Cells(30, 1).Value2 = "AWS EBS pricing"
        set_hyperlink_formula(
            worksheet.Cells(30, 2),
            "https://aws.amazon.com/ebs/pricing/",
            "Amazon EBS pricing",
        )
        worksheet.Cells(30, 4).Value2 = "Workload inventory"
        set_hyperlink_formula(
            worksheet.Cells(30, 5),
            str(EXHIBIT_A_PATH),
            EXHIBIT_A_PATH.name,
        )
        worksheet.Cells(30, 7).Value2 = "Drive inventory"
        set_hyperlink_formula(
            worksheet.Cells(30, 8),
            str(EXHIBIT_B_PATH),
            EXHIBIT_B_PATH.name,
        )
    notes = (
        (
            "AWS SQL edition and AWS licence basis affect only AWS source cost; neither selects nor reprices the Azure SQL Managed Instance target."
        ),
        "AWS, MACC, and parity discounts are independent. Component savings compare net AWS with net Azure before the additional parity adjustment.",
        "The required parity adjustment is applied uniformly to the Azure net total after MACC discounts. A negative value means Azure is already lower; above 100% is infeasible.",
        "BYOL sets the modeled AWS SQL license component to zero. Choose an AHB purchase option only when eligible licenses with Software Assurance or subscription rights exist.",
    )
    if is_rds:
        notes += (
            "Azure migration region is fixed to Sweden Central. The service tier is automatic: Next Generation General Purpose is used when source max IOPS is within its vCore-specific limit; Business Critical is used only when that IOPS limit is exceeded. If the selected tier cannot satisfy source vCPU/RAM, the row shows NO MAPPING. Throughput does not select the tier.",
            "Source RAM and source max IOPS are AWS workload inputs because the normalized RDS price catalog does not include workload-level provisioned IOPS. MI RAM is the smallest supported value meeting source RAM. Enter the deployed IOPS requirement; zero means unspecified and defaults the target to Next Generation General Purpose.",
            "Additional RAM above included memory is billed at the Sweden Central per-GB-hour rate. Additional RAM gross shows the annual charge separately; Compute + RAM net applies the Azure compute discount once to base compute plus additional RAM.",
            "RDS Multi-AZ is one database deployment, not two worksheet quantities. The selected Multi-AZ compute and storage price dimensions already represent source HA; one Azure SQL Managed Instance target provides built-in platform HA. Use a quantity above one only for multiple independent RDS deployments and MI targets.",
            "SQL Managed Instance is a newer managed architecture and does not map to AWS Standard or Enterprise edition labels. Validate feature compatibility separately from this CPU, RAM, storage, and IOPS cost comparison.",
            "RDS source storage cost models the selected deployment-specific GB-month rate. Provisioned IOPS and throughput can drive the Azure tier input but their additional AWS charges remain excluded because those quantities are absent from the normalized source catalog.",
            "A source that exceeds every single-MI vCPU/RAM limit shows NO MAPPING. Its AWS cost remains in the AWS total, but its row savings and parity are not calculated; Azure totals and portfolio parity include mapped rows only.",
        )
    else:
        notes += (
            "Azure migration region is fixed to Sweden Central. The service tier is automatic: Next Generation General Purpose is used when the source profile IOPS fits its vCore-specific limit; Business Critical is used when that limit is exceeded. AWS SQL edition never selects the Azure tier.",
            "Source RAM is an AWS workload input. MI RAM is the smallest supported value on the automatically selected Azure configuration that meets the source requirement. Additional RAM above included memory is billed at the Sweden Central per-GB-hour rate; Compute + RAM net applies the compute discount once.",
            "Where AWS omits an edition-specific price for a small instance, source SQL licence cost uses the regional per-core rate with AWS's four-core licensing minimum. This source-only fallback does not alter Azure sizing or price.",
            "Only the database-server rows from Exhibit A are loaded. Reporting-server rows are excluded from the sheet and calculations. Exhibit A supplies server names, environments, database counts, HA pairs, instance types, quantities, and SQL data totals. PROD-ENTERPRISE is Enterprise BYOL; every other loaded workload is Standard BYOL.",
            "Exhibit B supplies drive-level capacity, IOPS, and throughput in the EBS detail sheet. Source max IOPS is the highest provisioned IOPS among a profile's persistent volumes, matching Exhibit B; it is not a sum. Throughput does not select the MI tier.",
            "Defaults for fields absent from Exhibits A and B: eu-west-1, 8,760 annual hours, and Azure SQL MI PAYG with Azure Hybrid Benefit. Confirm the yellow inputs before using the result.",
            "Rows with quantity 2 preserve the supplied secondary node. The AWOMPSQLV101 + AWOMPSQLV104 row duplicates both SQL data and the AWOMPSQLV101 EBS profile across two instances because no AWOMPSQLV104 drive detail was supplied.",
            "gp3 capacity, provisioned IOPS above 3,000 per volume, and throughput above 125 MiB/s per volume are priced regionally. Ephemeral drives are listed but excluded from persistent EBS and Azure SQL storage.",
        )
    notes += (
        "Network, backups and snapshots, support, migration, and operational costs are excluded. RDS provisioned IOPS/throughput and EBS features other than capacity, provisioned IOPS, and gp3 throughput are excluded.",
    )
    notes_end = 30 + len(notes)
    for offset, note in enumerate(notes, start=31):
        worksheet.Cells(offset, 1).Value2 = f"Note {offset - 30}"
        worksheet.Range(f"B{offset}:{last_column}{offset}").Merge()
        worksheet.Cells(offset, 2).Value2 = note
    style_range(
        worksheet.Range(f"A27:{last_column}{notes_end}"),
        wrap=True,
        borders=True,
    )
    style_range(
        worksheet.Range(f"A27:A{notes_end}"),
        fill=LIGHT_GRAY,
        bold=True,
    )
    worksheet.Rows(f"31:{notes_end}").RowHeight = 34

    widths = (
        {
            "A": 24,
            "B": 20,
            "C": 12,
            "D": 27,
            "E": 12,
            "F": 14,
            "G": 16,
            "H": 20,
            "I": 18,
            "J": 16,
            "K": 16,
            "L": 13,
            "M": 27,
            "N": 13,
            "O": 28,
            "P": 38,
            "Q": 10,
            "R": 10,
        }
        if is_rds
        else {
            "A": 48,
            "B": 20,
            "C": 7,
            "D": 12,
            "E": 16,
            "F": 20,
            "G": 18,
            "H": 16,
            "I": 13,
            "J": 27,
            "K": 13,
            "L": 28,
            "M": 38,
            "N": 10,
            "O": 11,
            "P": 17,
        }
    )
    for column, width in widths.items():
        worksheet.Columns(column).ColumnWidth = width
    worksheet.Columns("S:AO" if is_rds else "Q:AM").ColumnWidth = 14
    worksheet.Range(f"A1:{last_column}{notes_end}").Font.Name = "Aptos"
    worksheet.Range(f"A12:{last_column}23").VerticalAlignment = XL_CENTER
    worksheet.Range(f"A12:{last_column}22").AutoFilter()
    if not is_rds:
        worksheet.Rows("13:19").RowHeight = 34
    worksheet.Activate()
    active_window = worksheet.Application.ActiveWindow
    if active_window is not None:
        active_window.FreezePanes = False
        active_window.SplitColumn = 0
        active_window.SplitRow = 12
        active_window.FreezePanes = True
        active_window.ScrollColumn = 1
        active_window.ScrollRow = 13
        active_window.Zoom = 65
    try:
        worksheet.PageSetup.PrintArea = f"$A$1:${last_column}${notes_end}"
        worksheet.PageSetup.Orientation = XL_LANDSCAPE
        worksheet.PageSetup.Zoom = False
        worksheet.PageSetup.FitToPagesWide = 1
        worksheet.PageSetup.FitToPagesTall = 2
    except pywintypes.com_error:
        pass
    return worksheet


def scan_visible_errors(worksheet) -> list[str]:
    values = worksheet.Range("A1:AO50").Value2
    errors: list[str] = []
    if not isinstance(values, tuple):
        return errors
    for row_number, row_values in enumerate(values, start=1):
        if not isinstance(row_values, tuple):
            continue
        for column_number, value in enumerate(row_values, start=1):
            if isinstance(value, str) and value.startswith("#"):
                address = worksheet.Cells(row_number, column_number).Address
                errors.append(f"{address}={value}")
    return errors


def validate_ec2_ebs_detail_sheet(
    excel,
    worksheet,
    region: str,
    *,
    exercise_io2: bool,
) -> None:
    excel.CalculateFullRebuild()
    if str(worksheet.Range("B4").Value2 or "") != region:
        raise RuntimeError("EC2 EBS detail region did not follow the converter.")
    for index, expected_row in enumerate(EBS_VOLUME_DETAILS):
        row_number = EBS_DETAIL_FIRST_ROW + index
        server, drive, volume_id, volume_type, storage_gb, iops, throughput = expected_row
        actual_row = (
            str(worksheet.Cells(row_number, 1).Value2 or ""),
            str(worksheet.Cells(row_number, 2).Value2 or ""),
            str(worksheet.Cells(row_number, 3).Value2 or ""),
            str(worksheet.Cells(row_number, 4).Value2 or ""),
            numeric(worksheet.Cells(row_number, 5).Value2),
            None
            if worksheet.Cells(row_number, 6).Value2 in (None, "")
            else numeric(worksheet.Cells(row_number, 6).Value2),
            None
            if worksheet.Cells(row_number, 7).Value2 in (None, "")
            else numeric(worksheet.Cells(row_number, 7).Value2),
        )
        expected_values = (
            server,
            drive,
            volume_id,
            volume_type,
            float(storage_gb),
            None if iops is None else float(iops),
            None if throughput is None else float(throughput),
        )
        if actual_row != expected_values:
            raise RuntimeError(
                f"EC2 EBS detail row {row_number} differs: "
                f"{actual_row} vs {expected_values}"
            )
        expected_components = ebs_monthly_cost_components(
            region,
            volume_type,
            storage_gb,
            iops,
            throughput,
        )
        actual_components = tuple(
            numeric(worksheet.Cells(row_number, column).Value2, math.nan)
            for column in range(10, 13)
        )
        for label, actual, expected in zip(
            ("capacity", "IOPS", "throughput"),
            actual_components,
            expected_components,
        ):
            if not math.isclose(actual, expected, abs_tol=0.005):
                raise RuntimeError(
                    f"EC2 EBS {label} mismatch in row {row_number}: "
                    f"{actual} vs {expected}"
                )
        actual_total = numeric(worksheet.Cells(row_number, 13).Value2, math.nan)
        expected_total = sum(expected_components)
        if not math.isclose(actual_total, expected_total, abs_tol=0.005):
            raise RuntimeError(
                f"EC2 EBS total mismatch in row {row_number}: "
                f"{actual_total} vs {expected_total}"
            )

    summary_profiles = list(dict.fromkeys(row[0] for row in EBS_VOLUME_DETAILS))
    summary_first_row = EBS_DETAIL_LAST_ROW + 5
    for offset, profile in enumerate(summary_profiles):
        row_number = summary_first_row + offset
        actual_profile = str(worksheet.Cells(row_number, 1).Value2 or "")
        if actual_profile != profile:
            raise RuntimeError(
                f"EC2 EBS summary profile differs: {actual_profile} vs {profile}"
            )
        actual_monthly = numeric(worksheet.Cells(row_number, 9).Value2, math.nan)
        expected_monthly = ebs_profile_monthly_cost(region, profile)
        if not math.isclose(actual_monthly, expected_monthly, abs_tol=0.005):
            raise RuntimeError(
                f"EC2 EBS profile mismatch for {profile}: "
                f"{actual_monthly} vs {expected_monthly}"
            )

    if exercise_io2:
        test_row = EBS_DETAIL_FIRST_ROW
        original_type = worksheet.Cells(test_row, 4).Value2
        original_iops = worksheet.Cells(test_row, 6).Value2
        original_throughput = worksheet.Cells(test_row, 7).Value2
        worksheet.Cells(test_row, 4).Value2 = "io2"
        worksheet.Cells(test_row, 6).Value2 = 70_000
        worksheet.Cells(test_row, 7).Value2 = 0
        excel.CalculateFullRebuild()
        expected_components = ebs_monthly_cost_components(
            region,
            "io2",
            numeric(worksheet.Cells(test_row, 5).Value2),
            70_000,
            0,
        )
        actual_components = tuple(
            numeric(worksheet.Cells(test_row, column).Value2, math.nan)
            for column in range(10, 13)
        )
        if any(
            not math.isclose(actual, expected, abs_tol=0.005)
            for actual, expected in zip(actual_components, expected_components)
        ):
            raise RuntimeError(
                f"EC2 io2 tier test mismatch: "
                f"{actual_components} vs {expected_components}"
            )
        worksheet.Cells(test_row, 4).Value2 = original_type
        worksheet.Cells(test_row, 6).Value2 = original_iops
        worksheet.Cells(test_row, 7).Value2 = original_throughput
        excel.CalculateFullRebuild()


def validate_converter_sheet(
    excel,
    worksheet,
    catalog: list[dict[str, object]],
    source_kind: str,
) -> dict[str, float]:
    excel.CalculateFullRebuild()
    is_rds = source_kind == "RDS"
    catalog_index = {
        (
            str(row["display_key"]),
            str(row["edition"]),
            str(row["mi_service_tier_selection"]),
        ): row
        for row in catalog
    }
    option_keys = dict(MI_PURCHASE_OPTIONS)
    validation_end = 18 if is_rds else 13 + len(EC2_PRIMED_WORKLOADS)
    for row_number in range(13, validation_end):
        if is_rds:
            configuration = " | ".join(
                (
                    str(worksheet.Range("K9").Value2 or ""),
                    str(worksheet.Cells(row_number, 2).Value2 or ""),
                    str(worksheet.Cells(row_number, 3).Value2 or ""),
                    str(worksheet.Cells(row_number, 4).Value2 or ""),
                )
            )
            edition = str(worksheet.Cells(row_number, 6).Value2 or "")
            service_tier = str(worksheet.Cells(row_number, 15).Value2 or "")
            catalog_key = (configuration, edition, service_tier)
            target_column = 15
            aws_total_column = 25
            mi_total_column = 33
        else:
            configuration = (
                f'{worksheet.Range("K9").Value2} | '
                f'{worksheet.Cells(row_number, 2).Value2 or ""}'
            )
            edition = str(worksheet.Cells(row_number, 4).Value2 or "")
            service_tier = str(worksheet.Cells(row_number, 12).Value2 or "")
            catalog_key = (configuration, edition, service_tier)
            target_column = 12
            aws_total_column = 23
            mi_total_column = 31
        catalog_row = catalog_index.get(catalog_key)
        if catalog_row is None and is_rds and service_tier == NO_MI_MAPPING:
            catalog_row = next(
                (
                    candidate
                    for key, candidate in catalog_index.items()
                    if key[0] == configuration and key[1] == edition
                ),
                None,
            )
        if catalog_row is None:
            raise RuntimeError(
                f"{worksheet.Name} sample row {row_number} has no selected MI-tier mapping."
            )
        target = str(worksheet.Cells(row_number, target_column).Value2 or "")
        if service_tier != NO_MI_MAPPING and target != catalog_row["mi_tier"]:
            raise RuntimeError(
                f"{source_kind} sample row {row_number} did not derive its mapped MI tier."
            )
        if numeric(worksheet.Cells(row_number, aws_total_column).Value2) <= 0:
            raise RuntimeError(f"{worksheet.Name} AWS total is not positive in row {row_number}.")
        mi_total = numeric(worksheet.Cells(row_number, mi_total_column).Value2)
        if service_tier == NO_MI_MAPPING:
            if not math.isclose(mi_total, 0.0, abs_tol=0.01):
                raise RuntimeError(
                    f"{worksheet.Name} unmapped row {row_number} has Azure cost."
                )
        elif mi_total <= 0:
            raise RuntimeError(f"{worksheet.Name} MI total is not positive in row {row_number}.")

    if is_rds:
        selected_region = str(worksheet.Range("K9").Value2)
        for row_number in range(13, 18):
            configuration = " | ".join(
                (
                    selected_region,
                    str(worksheet.Cells(row_number, 2).Value2 or ""),
                    str(worksheet.Cells(row_number, 3).Value2 or ""),
                    str(worksheet.Cells(row_number, 4).Value2 or ""),
                )
            )
            edition = str(worksheet.Cells(row_number, 6).Value2 or "")
            source_memory = numeric(
                worksheet.Cells(row_number, 10).Value2,
                math.nan,
            )
            source_iops = numeric(
                worksheet.Cells(row_number, 11).Value2,
                math.nan,
            )
            if not math.isfinite(source_memory) or source_memory <= 0:
                raise RuntimeError(
                    f"RDS source RAM is invalid in row {row_number}: "
                    f"{worksheet.Cells(row_number, 10).Value2!r}"
                )
            if not math.isfinite(source_iops) or source_iops < 0:
                raise RuntimeError(
                    f"RDS source IOPS is invalid in row {row_number}: "
                    f"{worksheet.Cells(row_number, 11).Value2!r}"
                )
            if bool(worksheet.Cells(row_number, 10).HasFormula) or bool(
                worksheet.Cells(row_number, 11).HasFormula
            ):
                raise RuntimeError(
                    f"RDS source RAM or IOPS is not a literal input in row {row_number}."
                )
            nggp_catalog_row = catalog_index.get(
                (configuration, edition, EC2_DEFAULT_MI_SERVICE_TIER)
            )
            bc_catalog_row = catalog_index.get(
                (configuration, edition, "Business Critical")
            )
            if nggp_catalog_row is None:
                source_catalog_row = bc_catalog_row or catalog_index.get(
                    (configuration, edition, NO_MI_MAPPING)
                )
                if source_catalog_row is None:
                    raise RuntimeError(
                        f"RDS source metadata is unavailable in row {row_number}."
                    )
                expected_service_tier = (
                    "Business Critical"
                    if bc_catalog_row is not None
                    and source_iops
                    > nggp_iops_cap(numeric(source_catalog_row["vcpu"]))
                    else NO_MI_MAPPING
                )
            elif source_iops > nggp_iops_cap(
                numeric(nggp_catalog_row["mi_vcores"])
            ):
                expected_service_tier = (
                    "Business Critical"
                    if bc_catalog_row is not None
                    else NO_MI_MAPPING
                )
            else:
                expected_service_tier = EC2_DEFAULT_MI_SERVICE_TIER
            service_tier = str(worksheet.Cells(row_number, 15).Value2 or "")
            if service_tier != expected_service_tier:
                raise RuntimeError(
                    f"RDS automatic MI tier mismatch in row {row_number}: "
                    f"{service_tier} vs {expected_service_tier}"
                )
            if service_tier == NO_MI_MAPPING:
                if worksheet.Cells(row_number, 14).Value2 != NO_MI_MAPPING:
                    raise RuntimeError(
                        f"RDS unmapped MI RAM is unclear in row {row_number}."
                    )
                if worksheet.Cells(row_number, 16).Value2 != NO_MI_MAPPING:
                    raise RuntimeError(
                        f"RDS unmapped hardware/storage is unclear in row {row_number}."
                    )
                if not math.isclose(
                    numeric(worksheet.Cells(row_number, 33).Value2),
                    0.0,
                    abs_tol=0.01,
                ):
                    raise RuntimeError(
                        f"RDS unmapped row {row_number} has Azure cost."
                    )
                if any(
                    worksheet.Cells(row_number, column).Value2 not in (None, "")
                    for column in range(34, 37)
                ):
                    raise RuntimeError(
                        f"RDS unmapped row {row_number} has component savings."
                    )
                for column in (37, 38, 40, 41):
                    if worksheet.Cells(row_number, column).Value2 != NO_MI_MAPPING:
                        raise RuntimeError(
                            f"RDS unmapped row {row_number} has misleading parity output."
                        )
                continue
            catalog_row = catalog_index.get(
                (configuration, edition, service_tier)
            )
            if catalog_row is None:
                raise RuntimeError(
                    f"RDS mapped target metadata is unavailable in row {row_number}."
                )
            if catalog_row["mi_region"] != RDS_AZURE_MIGRATION_REGION:
                raise RuntimeError(
                    f"RDS row {row_number} did not target Sweden Central."
                )
            actual_memory = numeric(
                worksheet.Cells(row_number, 14).Value2,
                math.nan,
            )
            expected_memory = next(
                (
                    numeric(memory)
                    for memory in catalog_row["mi_memory_options"]
                    if numeric(memory) >= source_memory
                ),
                math.nan,
            )
            if not math.isclose(actual_memory, expected_memory, abs_tol=1e-9):
                raise RuntimeError(
                    f"RDS MI RAM mismatch in row {row_number}: "
                    f"{actual_memory} vs {expected_memory}"
                )
            expected_hardware_storage = (
                f'{catalog_row["mi_hardware"]} / '
                + (
                    "BC local SSD"
                    if service_tier == "Business Critical"
                    else "Remote LRS"
                )
            )
            actual_hardware_storage = str(
                worksheet.Cells(row_number, 16).Value2 or ""
            )
            if actual_hardware_storage != expected_hardware_storage:
                raise RuntimeError(
                    f"RDS MI hardware/storage mismatch in row {row_number}: "
                    f"{actual_hardware_storage} vs {expected_hardware_storage}"
                )
            quantity = numeric(worksheet.Cells(row_number, 5).Value2)
            hours = numeric(worksheet.Cells(row_number, 12).Value2)
            expected_additional_memory_cost = (
                quantity
                * hours
                * max(
                    0.0,
                    actual_memory
                    - numeric(catalog_row["mi_included_memory_gb"]),
                )
                * numeric(catalog_row["mi_memory_hourly_rate"])
            )
            actual_additional_memory_cost = numeric(
                worksheet.Cells(row_number, 27).Value2,
                math.nan,
            )
            if not math.isclose(
                actual_additional_memory_cost,
                expected_additional_memory_cost,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"RDS additional RAM cost mismatch in row {row_number}: "
                    f"{actual_additional_memory_cost} vs "
                    f"{expected_additional_memory_cost}"
                )
            azure_compute_gross = numeric(
                worksheet.Cells(row_number, 26).Value2,
                math.nan,
            )
            azure_compute_ram_net = numeric(
                worksheet.Cells(row_number, 28).Value2,
                math.nan,
            )
            expected_compute_ram_net = (
                azure_compute_gross + actual_additional_memory_cost
            ) * (1 - numeric(worksheet.Range("E5").Value2))
            if not math.isclose(
                azure_compute_ram_net,
                expected_compute_ram_net,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"RDS Compute + RAM net mismatch in row {row_number}: "
                    f"{azure_compute_ram_net} vs {expected_compute_ram_net}"
                )
            expected_mi_storage = (
                quantity
                * numeric(worksheet.Cells(row_number, 9).Value2)
                * 12
                * numeric(catalog_row["mi_storage_monthly_per_gb"])
            )
            actual_mi_storage = numeric(
                worksheet.Cells(row_number, 31).Value2,
                math.nan,
            )
            if not math.isclose(
                actual_mi_storage,
                expected_mi_storage,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"RDS MI storage mismatch in row {row_number}: "
                    f"{actual_mi_storage} vs {expected_mi_storage}"
                )
        if worksheet.Range("K10").Value2 != RDS_AZURE_MIGRATION_REGION_LABEL:
            raise RuntimeError("RDS Azure migration region is not Sweden Central.")
    else:
        selected_region = str(worksheet.Range("K9").Value2)
        ebs_detail_sheet = worksheet.Parent.Worksheets.Item("EC2 EBS Detail")
        validate_ec2_ebs_detail_sheet(
            excel,
            ebs_detail_sheet,
            selected_region,
            exercise_io2=True,
        )
        for index, (label, instance_type, quantity, storage_gb) in enumerate(
            EC2_PRIMED_WORKLOADS
        ):
            row_number = 13 + index
            actual = (
                worksheet.Cells(row_number, 1).Value2,
                worksheet.Cells(row_number, 2).Value2,
                numeric(worksheet.Cells(row_number, 3).Value2),
                numeric(worksheet.Cells(row_number, 7).Value2),
            )
            expected = (
                label,
                instance_type,
                float(quantity),
                storage_gb,
            )
            if actual != expected:
                raise RuntimeError(
                    f"EC2 inventory row {row_number} differs from source: "
                    f"{actual} vs {expected}"
                )
            source_memory = numeric(
                worksheet.Cells(row_number, 8).Value2,
                math.nan,
            )
            if not math.isfinite(source_memory) or source_memory <= 0:
                raise RuntimeError(
                    f"EC2 source RAM is invalid in row {row_number}: "
                    f"{worksheet.Cells(row_number, 8).Value2!r}"
                )
            if bool(worksheet.Cells(row_number, 8).HasFormula):
                raise RuntimeError(
                    f"EC2 source RAM is not an input in row {row_number}."
                )
            ebs_profile = str(worksheet.Cells(row_number, 6).Value2 or "")
            expected_profile = EC2_PRIMED_EBS_PROFILES[index]
            if ebs_profile != expected_profile:
                raise RuntimeError(
                    f"EC2 EBS profile differs in row {row_number}: "
                    f"{ebs_profile} vs {expected_profile}"
                )
            actual_source_assignment = (
                str(worksheet.Cells(row_number, 4).Value2 or ""),
                str(worksheet.Cells(row_number, 5).Value2 or ""),
            )
            expected_source_assignment = (
                EC2_PRIMED_EDITIONS[index],
                EC2_PRIMED_LICENSE_BASES[index],
            )
            if actual_source_assignment != expected_source_assignment:
                raise RuntimeError(
                    f"EC2 source licence assignment differs in row {row_number}: "
                    f"{actual_source_assignment} vs {expected_source_assignment}"
                )
            expected_storage = quantity * 12 * ebs_profile_monthly_cost(
                selected_region, ebs_profile
            )
            actual_storage = numeric(
                worksheet.Cells(row_number, 21).Value2,
                math.nan,
            )
            if not math.isclose(actual_storage, expected_storage, abs_tol=0.05):
                raise RuntimeError(
                    f"EC2 storage mismatch in row {row_number}: "
                    f"{actual_storage} vs {expected_storage}"
                )
            configuration = f"{selected_region} | {instance_type}"
            edition = str(worksheet.Cells(row_number, 4).Value2 or "")
            nggp_catalog_row = catalog_index[
                (configuration, edition, EC2_DEFAULT_MI_SERVICE_TIER)
            ]
            expected_iops = ebs_profile_max_iops(ebs_profile)
            actual_iops = numeric(
                worksheet.Cells(row_number, 16).Value2,
                math.nan,
            )
            if not math.isclose(actual_iops, expected_iops, abs_tol=1e-9):
                raise RuntimeError(
                    f"EC2 source IOPS mismatch in row {row_number}: "
                    f"{actual_iops} vs {expected_iops}"
                )
            expected_service_tier = (
                "Business Critical"
                if expected_iops
                > nggp_iops_cap(numeric(nggp_catalog_row["mi_vcores"]))
                else EC2_DEFAULT_MI_SERVICE_TIER
            )
            service_tier = str(worksheet.Cells(row_number, 12).Value2 or "")
            if service_tier != expected_service_tier:
                raise RuntimeError(
                    f"EC2 automatic MI tier mismatch in row {row_number}: "
                    f"{service_tier} vs {expected_service_tier}"
                )
            catalog_row = catalog_index[(configuration, edition, service_tier)]
            actual_memory = numeric(
                worksheet.Cells(row_number, 11).Value2,
                math.nan,
            )
            expected_memory = next(
                (
                    numeric(memory)
                    for memory in catalog_row["mi_memory_options"]
                    if numeric(memory) >= source_memory
                ),
                math.nan,
            )
            if not math.isclose(actual_memory, expected_memory, abs_tol=1e-9):
                raise RuntimeError(
                    f"EC2 MI RAM mismatch in row {row_number}: "
                    f"{actual_memory} vs {expected_memory}"
                )
            if str(worksheet.Cells(row_number, 11).Text) != f"{expected_memory:.0f}":
                raise RuntimeError(
                    f"EC2 MI RAM display is malformed in row {row_number}: "
                    f"{worksheet.Cells(row_number, 11).Text!r}"
                )
            expected_hardware_storage = (
                f'{catalog_row["mi_hardware"]} / '
                + (
                    "BC local SSD"
                    if catalog_row["mi_tier"] == "Business Critical"
                    else "Remote LRS"
                )
            )
            actual_hardware_storage = str(
                worksheet.Cells(row_number, 13).Value2 or ""
            )
            if actual_hardware_storage != expected_hardware_storage:
                raise RuntimeError(
                    f"EC2 MI hardware/storage mismatch in row {row_number}: "
                    f"{actual_hardware_storage} vs {expected_hardware_storage}"
                )
            expected_additional_memory_cost = (
                quantity
                * numeric(worksheet.Cells(row_number, 9).Value2)
                * max(
                    0.0,
                    actual_memory
                    - numeric(catalog_row["mi_included_memory_gb"]),
                )
                * numeric(catalog_row["mi_memory_hourly_rate"])
            )
            actual_additional_memory_cost = numeric(
                worksheet.Cells(row_number, 25).Value2,
                math.nan,
            )
            if not math.isclose(
                actual_additional_memory_cost,
                expected_additional_memory_cost,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"EC2 additional RAM cost mismatch in row {row_number}: "
                    f"{actual_additional_memory_cost} vs "
                    f"{expected_additional_memory_cost}"
                )
            azure_compute_gross = numeric(
                worksheet.Cells(row_number, 24).Value2,
                math.nan,
            )
            azure_compute_ram_net = numeric(
                worksheet.Cells(row_number, 26).Value2,
                math.nan,
            )
            expected_compute_ram_net = (
                azure_compute_gross + actual_additional_memory_cost
            ) * (1 - numeric(worksheet.Range("E5").Value2))
            if not math.isclose(
                azure_compute_ram_net,
                expected_compute_ram_net,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"EC2 Compute + RAM net mismatch in row {row_number}: "
                    f"{azure_compute_ram_net} vs {expected_compute_ram_net}"
                )
            expected_mi_storage = (
                quantity
                * storage_gb
                * 12
                * numeric(catalog_row["mi_storage_monthly_per_gb"])
            )
            actual_mi_storage = numeric(
                worksheet.Cells(row_number, 29).Value2,
                math.nan,
            )
            if not math.isclose(
                actual_mi_storage,
                expected_mi_storage,
                abs_tol=0.05,
            ):
                raise RuntimeError(
                    f"EC2 MI storage mismatch in row {row_number}: "
                    f"{actual_mi_storage} vs {expected_mi_storage}"
                )
        for row_number in range(13 + len(EC2_PRIMED_WORKLOADS), 23):
            stale_inputs = tuple(
                worksheet.Cells(row_number, column).Value2
                for column in range(1, 11)
            )
            if any(value not in (None, "") for value in stale_inputs):
                raise RuntimeError(
                    f"EC2 unseeded row {row_number} retained workload inputs: "
                    f"{stale_inputs}"
                )
        if worksheet.Range("K10").Value2 != EC2_AZURE_MIGRATION_REGION_LABEL:
            raise RuntimeError("EC2 Azure migration region is not Sweden Central.")
        if not str(worksheet.Range("K8").Formula or "").upper().startswith(
            "=HYPERLINK("
        ):
            raise RuntimeError(
                "EC2 per-volume detail link is missing."
            )

    first_row = 13
    if is_rds:
        configuration = " | ".join(
            (
                str(worksheet.Range("K9").Value2),
                str(worksheet.Cells(first_row, 2).Value2),
                str(worksheet.Cells(first_row, 3).Value2),
                str(worksheet.Cells(first_row, 4).Value2),
            )
        )
        edition = str(worksheet.Cells(first_row, 6).Value2)
        source_license = str(worksheet.Cells(first_row, 7).Value2)
        purchase_label = str(worksheet.Cells(first_row, 13).Value2)
        quantity = numeric(worksheet.Cells(first_row, 5).Value2)
        hours = numeric(worksheet.Cells(first_row, 12).Value2)
        source_compute_column = 19
        source_license_column = 21
        mi_compute_column = 26
        service_tier = str(worksheet.Cells(first_row, 15).Value2)
    else:
        configuration = (
            f'{worksheet.Range("K9").Value2} | '
            f'{worksheet.Cells(first_row, 2).Value2}'
        )
        edition = str(worksheet.Cells(first_row, 4).Value2)
        source_license = str(worksheet.Cells(first_row, 5).Value2)
        purchase_label = str(worksheet.Cells(first_row, 10).Value2)
        quantity = numeric(worksheet.Cells(first_row, 3).Value2)
        hours = numeric(worksheet.Cells(first_row, 9).Value2)
        source_compute_column = 17
        source_license_column = 19
        mi_compute_column = 24
        service_tier = str(worksheet.Cells(first_row, 12).Value2)
    catalog_row = catalog_index[
        (configuration, edition, service_tier)
    ]
    expected_compute = quantity * hours * numeric(catalog_row["source_compute_hourly"])
    actual_compute = numeric(
        worksheet.Cells(first_row, source_compute_column).Value2,
        math.nan,
    )
    if not math.isclose(actual_compute, expected_compute, abs_tol=0.05):
        raise RuntimeError(
            f"{worksheet.Name} source compute mismatch: {actual_compute} vs {expected_compute}"
        )

    if source_license == "License included":
        license_field = (
            "source_standard_license_core_hourly"
            if edition == "Standard" and source_kind == "RDS"
            else "source_enterprise_license_core_hourly"
            if source_kind == "RDS"
            else "source_standard_license_hourly"
            if edition == "Standard"
            else "source_enterprise_license_hourly"
        )
        expected_license = quantity * hours * numeric(catalog_row[license_field])
        if source_kind == "RDS":
            expected_license *= numeric(catalog_row["vcpu"])
    else:
        expected_license = 0.0
    actual_license = numeric(
        worksheet.Cells(first_row, source_license_column).Value2,
        math.nan,
    )
    if not math.isclose(actual_license, expected_license, abs_tol=0.05):
        raise RuntimeError(
            f"{worksheet.Name} source license mismatch: {actual_license} vs {expected_license}"
        )

    option_set = catalog_row["mi_options"]
    if not isinstance(option_set, dict):
        raise RuntimeError("MI option set is not available for validation.")
    expected_mi_compute = (
        quantity * hours * option_set[option_keys[purchase_label]].compute_hourly
    )
    actual_mi_compute = numeric(
        worksheet.Cells(first_row, mi_compute_column).Value2,
        math.nan,
    )
    if not math.isclose(actual_mi_compute, expected_mi_compute, abs_tol=0.05):
        raise RuntimeError(
            f"{worksheet.Name} MI compute mismatch: {actual_mi_compute} vs {expected_mi_compute}"
        )

    if source_kind == "RDS":
        original_edition = worksheet.Cells(first_row, 6).Value2
        original_license_basis = worksheet.Cells(first_row, 7).Value2
        worksheet.Cells(first_row, 7).Value2 = "License included"
        edition_results: dict[str, tuple[tuple[object, ...], tuple[float, ...], float]] = {}
        for test_edition in ("Standard", "Enterprise"):
            worksheet.Cells(first_row, 6).Value2 = test_edition
            excel.CalculateFullRebuild()
            azure_target = tuple(
                worksheet.Cells(first_row, column).Value2
                for column in range(14, 18)
            )
            azure_costs = tuple(
                numeric(worksheet.Cells(first_row, column).Value2, math.nan)
                for column in range(26, 34)
            )
            source_license_cost = numeric(
                worksheet.Cells(first_row, 21).Value2,
                math.nan,
            )
            edition_results[test_edition] = (
                azure_target,
                azure_costs,
                source_license_cost,
            )
        standard_result = edition_results["Standard"]
        enterprise_result = edition_results["Enterprise"]
        if standard_result[0] != enterprise_result[0] or any(
            not math.isclose(standard, enterprise, abs_tol=0.01)
            for standard, enterprise in zip(standard_result[1], enterprise_result[1])
        ):
            raise RuntimeError(
                "Changing the RDS SQL edition altered the Azure MI target or cost."
            )
        if enterprise_result[2] <= standard_result[2]:
            raise RuntimeError(
                "Changing the RDS SQL edition did not alter the AWS source licence cost."
            )
        worksheet.Cells(first_row, 6).Value2 = original_edition
        worksheet.Cells(first_row, 7).Value2 = original_license_basis
        excel.CalculateFullRebuild()

        configuration = " | ".join(
            (
                str(worksheet.Range("K9").Value2),
                str(worksheet.Cells(first_row, 2).Value2),
                str(worksheet.Cells(first_row, 3).Value2),
                str(worksheet.Cells(first_row, 4).Value2),
            )
        )
        edition = str(worksheet.Cells(first_row, 6).Value2)
        nggp_catalog_row = catalog_index[
            (configuration, edition, EC2_DEFAULT_MI_SERVICE_TIER)
        ]
        original_source_memory = worksheet.Cells(first_row, 10).Value2
        memory_probe = next(
            (
                numeric(memory)
                for memory in nggp_catalog_row["mi_memory_options"]
                if numeric(memory) > numeric(original_source_memory)
            ),
            None,
        )
        if memory_probe is not None:
            worksheet.Cells(first_row, 10).Value2 = memory_probe
            excel.CalculateFullRebuild()
            if not math.isclose(
                numeric(worksheet.Cells(first_row, 14).Value2, math.nan),
                memory_probe,
                abs_tol=1e-9,
            ):
                raise RuntimeError(
                    "Increasing RDS source RAM did not automatically increase MI RAM."
                )
            worksheet.Cells(first_row, 10).Value2 = original_source_memory
            excel.CalculateFullRebuild()

        original_source_iops = worksheet.Cells(first_row, 11).Value2
        worksheet.Cells(first_row, 11).Value2 = (
            nggp_iops_cap(numeric(nggp_catalog_row["mi_vcores"])) + 1
        )
        excel.CalculateFullRebuild()
        if worksheet.Cells(first_row, 15).Value2 != "Business Critical":
            raise RuntimeError(
                "RDS IOPS above the NGGP vCore limit did not select Business Critical."
            )
        worksheet.Cells(first_row, 11).Value2 = original_source_iops
        excel.CalculateFullRebuild()
        expected_restored_tier = (
            "Business Critical"
            if numeric(original_source_iops)
            > nggp_iops_cap(numeric(nggp_catalog_row["mi_vcores"]))
            else EC2_DEFAULT_MI_SERVICE_TIER
        )
        if worksheet.Cells(first_row, 15).Value2 != expected_restored_tier:
            raise RuntimeError(
                "Restoring RDS source IOPS did not restore the automatic MI tier."
            )

    if source_kind == "EC2":
        original_edition = worksheet.Cells(first_row, 4).Value2
        original_license_basis = worksheet.Cells(first_row, 5).Value2
        worksheet.Cells(first_row, 5).Value2 = "License included"
        edition_results: dict[str, tuple[tuple[object, ...], tuple[float, ...], float]] = {}
        for test_edition in ("Standard", "Enterprise"):
            worksheet.Cells(first_row, 4).Value2 = test_edition
            excel.CalculateFullRebuild()
            azure_target = (
                worksheet.Cells(first_row, 11).Value2,
                worksheet.Cells(first_row, 12).Value2,
                worksheet.Cells(first_row, 13).Value2,
                worksheet.Cells(first_row, 14).Value2,
            )
            azure_costs = tuple(
                numeric(worksheet.Cells(first_row, column).Value2, math.nan)
                for column in range(24, 32)
            )
            source_license_cost = numeric(
                worksheet.Cells(first_row, 19).Value2,
                math.nan,
            )
            edition_results[test_edition] = (
                azure_target,
                azure_costs,
                source_license_cost,
            )
        standard_result = edition_results["Standard"]
        enterprise_result = edition_results["Enterprise"]
        if standard_result[0] != enterprise_result[0] or any(
            not math.isclose(standard, enterprise, abs_tol=0.01)
            for standard, enterprise in zip(standard_result[1], enterprise_result[1])
        ):
            raise RuntimeError(
                "Changing the AWS SQL edition altered the Azure MI target or cost."
            )
        if enterprise_result[2] <= standard_result[2]:
            raise RuntimeError(
                "Changing the AWS SQL edition did not alter the AWS source licence cost."
            )
        worksheet.Cells(first_row, 4).Value2 = original_edition
        worksheet.Cells(first_row, 5).Value2 = original_license_basis
        excel.CalculateFullRebuild()

        original_source_memory = worksheet.Cells(first_row, 8).Value2
        memory_probe = next(
            (
                numeric(memory)
                for memory in catalog_row["mi_memory_options"]
                if numeric(memory) > numeric(original_source_memory)
            ),
            None,
        )
        if memory_probe is not None:
            worksheet.Cells(first_row, 8).Value2 = memory_probe
            excel.CalculateFullRebuild()
            if not math.isclose(
                numeric(worksheet.Cells(first_row, 11).Value2, math.nan),
                memory_probe,
                abs_tol=1e-9,
            ):
                raise RuntimeError(
                    "Increasing source RAM did not automatically increase MI RAM."
                )
            worksheet.Cells(first_row, 8).Value2 = original_source_memory
            excel.CalculateFullRebuild()

        ebs_profile = str(worksheet.Cells(first_row, 6).Value2 or "")
        iops_detail_index = next(
            (
                index
                for index, detail in enumerate(EBS_VOLUME_DETAILS)
                if detail[0] == ebs_profile and detail[3] != "Ephemeral"
            ),
            None,
        )
        if iops_detail_index is None:
            raise RuntimeError("EC2 IOPS tier probe has no persistent EBS volume.")
        iops_detail_row = EBS_DETAIL_FIRST_ROW + iops_detail_index
        original_iops = ebs_detail_sheet.Cells(iops_detail_row, 6).Value2
        nggp_catalog_row = catalog_index[
            (configuration, edition, EC2_DEFAULT_MI_SERVICE_TIER)
        ]
        ebs_detail_sheet.Cells(iops_detail_row, 6).Value2 = (
            nggp_iops_cap(numeric(nggp_catalog_row["mi_vcores"])) + 1
        )
        excel.CalculateFullRebuild()
        if worksheet.Cells(first_row, 12).Value2 != "Business Critical":
            raise RuntimeError(
                "IOPS above the NGGP vCore limit did not select Business Critical."
            )
        ebs_detail_sheet.Cells(iops_detail_row, 6).Value2 = original_iops
        excel.CalculateFullRebuild()
        if worksheet.Cells(first_row, 12).Value2 != EC2_DEFAULT_MI_SERVICE_TIER:
            raise RuntimeError(
                "Restoring source IOPS did not restore Next Generation General Purpose."
            )

    errors = scan_visible_errors(worksheet)
    if errors:
        raise RuntimeError(f"{worksheet.Name} contains Excel errors: {', '.join(errors[:10])}")

    selected_adjustment = numeric(worksheet.Range("H5").Value2)
    selected_difference = numeric(worksheet.Range("H7").Value2, math.nan)
    required_adjustment = numeric(worksheet.Range("H6").Value2, math.nan)
    if not math.isfinite(required_adjustment):
        raise RuntimeError(f"{worksheet.Name} required parity adjustment did not calculate.")
    worksheet.Range("H5").Value2 = required_adjustment
    excel.CalculateFullRebuild()
    tested_difference = numeric(worksheet.Range("H7").Value2, math.nan)
    if abs(tested_difference) >= 1.0:
        raise RuntimeError(
            f"{worksheet.Name} parity test missed zero by ${tested_difference:,.6f}."
        )
    worksheet.Range("H5").Value2 = selected_adjustment
    excel.CalculateFullRebuild()

    if is_rds:
        mapped_rows = [
            row_number
            for row_number in range(13, 23)
            if worksheet.Cells(row_number, 2).Value2
            and worksheet.Cells(row_number, 15).Value2 != NO_MI_MAPPING
        ]
        unmapped_count = sum(
            1
            for row_number in range(13, 23)
            if worksheet.Cells(row_number, 2).Value2
            and worksheet.Cells(row_number, 15).Value2 == NO_MI_MAPPING
        )
        mapped_aws_total = sum(
            numeric(worksheet.Cells(row_number, 25).Value2)
            for row_number in mapped_rows
        )
    else:
        unmapped_count = 0
        mapped_aws_total = numeric(worksheet.Range("W23").Value2)

    return {
        "aws_total": numeric(
            worksheet.Range("Y23" if is_rds else "W23").Value2
        ),
        "mi_total": numeric(worksheet.Range("AG23" if is_rds else "AE23").Value2),
        "mapped_aws_total": mapped_aws_total,
        "unmapped_count": float(unmapped_count),
        "selected_difference": selected_difference,
        "required_adjustment": required_adjustment,
        "tested_difference": tested_difference,
    }


def verify_saved_workbook(excel, workbook_path: Path = WORKBOOK_PATH) -> None:
    workbook = None
    try:
        workbook = excel.Workbooks.Open(
            str(workbook_path),
            UpdateLinks=0,
            ReadOnly=True,
            IgnoreReadOnlyRecommended=True,
        )
        sheet_names = [worksheet.Name for worksheet in workbook.Worksheets]
        expected_names = [
            "Business Case",
            "SQL License Book Prices",
            "RDS SQL MI TCO Converter",
            "EC2 TCO Converter",
            "EC2 EBS Detail",
        ]
        if sheet_names != expected_names:
            raise RuntimeError(f"Unexpected saved sheet order: {sheet_names}")
        if not workbook.Windows.Item(1).Visible:
            raise RuntimeError("Workbook saved with its document window hidden.")
        tooltips = find_workbook_tooltips(workbook)
        if tooltips:
            raise RuntimeError(
                f"Workbook saved with tooltips: {', '.join(tooltips[:10])}"
            )
        for name in expected_names[2:4]:
            worksheet = workbook.Worksheets.Item(name)
            worksheet.Activate()
            if not worksheet.Columns("AQ:DI").EntireColumn.Hidden:
                raise RuntimeError(f"{name} helper columns are not hidden.")
            if not worksheet.Range("B13").Value2:
                raise RuntimeError(f"{name} sample rows did not persist.")
            if not math.isfinite(numeric(worksheet.Range("H6").Value2, math.nan)):
                raise RuntimeError(f"{name} parity formula did not persist.")
            active_window = excel.ActiveWindow
            if active_window is not None and int(active_window.ScrollColumn) != 1:
                raise RuntimeError(f"{name} did not persist its column-A opening view.")
            errors = scan_visible_errors(worksheet)
            if errors:
                raise RuntimeError(f"{name} saved with Excel errors: {', '.join(errors[:10])}")
        rds_sheet = workbook.Worksheets.Item("RDS SQL MI TCO Converter")
        if rds_sheet.Range("K9").Value2 != "eu-west-1":
            raise RuntimeError("RDS region selection did not persist.")
        if rds_sheet.Range("K10").Value2 != RDS_AZURE_MIGRATION_REGION_LABEL:
            raise RuntimeError("RDS Sweden Central migration region did not persist.")
        expected_rds_selection = (
            "db.m6i.2xlarge",
            "Single-AZ",
            "Reserved 1yr No Upfront",
        )
        actual_rds_selection = tuple(
            rds_sheet.Cells(13, column).Value2 for column in range(2, 5)
        )
        if actual_rds_selection != expected_rds_selection:
            raise RuntimeError(
                f"RDS split selection did not persist: {actual_rds_selection}"
            )
        for address in ("K9", "B13", "C13", "D13", "M13"):
            if rds_sheet.Range(address).Validation.Type != XL_VALIDATE_LIST:
                raise RuntimeError(f"RDS dropdown validation missing from {address}.")
        for address in ("I13", "J13", "L13"):
            if rds_sheet.Range(address).Validation.Type != XL_VALIDATE_DECIMAL:
                raise RuntimeError(f"RDS numeric validation missing from {address}.")
        if rds_sheet.Range("K13").Validation.Type != XL_VALIDATE_WHOLE_NUMBER:
            raise RuntimeError("RDS source IOPS validation missing from K13.")
        for address in ("N13", "O13"):
            try:
                validation_type = rds_sheet.Range(address).Validation.Type
            except pywintypes.com_error:
                validation_type = None
            if validation_type == XL_VALIDATE_LIST:
                raise RuntimeError(
                    f"RDS automatic output {address} still has dropdown validation."
                )
        expected_rds_headers = {
            "E12": "Deployment qty",
            "I12": "SQL data GB / instance",
            "J12": "Source RAM GB / instance",
            "K12": "Source max IOPS",
            "N12": "MI RAM GB",
            "O12": "MI service tier",
            "P12": "MI hardware / data storage",
            "Q12": "MI vCores",
            "R12": "Source vCPU",
            "Z12": "Compute gross",
            "AA12": "Additional RAM gross",
            "AB12": "Compute + RAM net",
        }
        for address, expected_header in expected_rds_headers.items():
            if rds_sheet.Range(address).Value2 != expected_header:
                raise RuntimeError(
                    f"RDS header {address} is not aligned: "
                    f"{rds_sheet.Range(address).Value2!r} vs {expected_header!r}"
                )
        for address, expected_color in (
            ("N11", BLUE),
            ("R11", AWS_ORANGE),
            ("Z11", BLUE),
            ("J13", YELLOW),
            ("N13", LIGHT_BLUE),
            ("R13", LIGHT_ORANGE),
            ("Z13", LIGHT_BLUE),
            ("AA13", LIGHT_BLUE),
        ):
            if int(rds_sheet.Range(address).Interior.Color) != expected_color:
                raise RuntimeError(f"RDS color band is incorrect at {address}.")
        for row_number in range(13, 18):
            if bool(rds_sheet.Cells(row_number, 10).HasFormula):
                raise RuntimeError(
                    f"RDS source RAM is not a literal input in row {row_number}."
                )
            if bool(rds_sheet.Cells(row_number, 11).HasFormula):
                raise RuntimeError(
                    f"RDS source IOPS is not a literal input in row {row_number}."
                )
            if not bool(rds_sheet.Cells(row_number, 14).HasFormula):
                raise RuntimeError(
                    f"RDS MI RAM is not automatically derived in row {row_number}."
                )
            if not bool(rds_sheet.Cells(row_number, 15).HasFormula):
                raise RuntimeError(
                    f"RDS MI service tier is not automatically derived in row {row_number}."
                )
            service_tier = str(rds_sheet.Cells(row_number, 15).Value2 or "")
            if service_tier not in (*EC2_MI_SERVICE_TIERS, NO_MI_MAPPING):
                raise RuntimeError(
                    f"RDS automatic MI service tier is invalid in row {row_number}."
                )
            if str(rds_sheet.Cells(row_number, 9).Text).startswith(","):
                raise RuntimeError(
                    f"RDS saved SQL data display has a leading comma in row {row_number}."
                )
            hardware_storage = str(
                rds_sheet.Cells(row_number, 16).Value2 or ""
            )
            if service_tier == NO_MI_MAPPING:
                if rds_sheet.Cells(row_number, 14).Value2 != NO_MI_MAPPING:
                    raise RuntimeError(
                        f"RDS saved no-fit RAM output is unclear in row {row_number}."
                    )
                if hardware_storage != NO_MI_MAPPING:
                    raise RuntimeError(
                        f"RDS saved no-fit hardware output is unclear in row {row_number}."
                    )
                if not math.isclose(
                    numeric(rds_sheet.Cells(row_number, 33).Value2),
                    0.0,
                    abs_tol=0.01,
                ):
                    raise RuntimeError(
                        f"RDS saved no-fit row {row_number} has Azure cost."
                    )
                continue
            if not any(
                label in hardware_storage
                for label in ("Remote LRS", "BC local SSD")
            ):
                raise RuntimeError(
                    f"RDS hardware/storage output is missing in row {row_number}."
                )
        ec2_sheet = workbook.Worksheets.Item("EC2 TCO Converter")
        if ec2_sheet.Range("K9").Value2 != "eu-west-1":
            raise RuntimeError("EC2 region selection did not persist.")
        if ec2_sheet.Range("K10").Value2 != EC2_AZURE_MIGRATION_REGION_LABEL:
            raise RuntimeError("EC2 Sweden Central migration region did not persist.")
        if ec2_sheet.Range("B13").Value2 != "r6id.2xlarge":
            raise RuntimeError("EC2 instance-only sample selection did not persist.")
        if ec2_sheet.Range("L13").Value2 not in EC2_MI_SERVICE_TIERS:
            raise RuntimeError("EC2 automatic MI service tier did not persist.")
        for address in ("K9", "B13", "F13", "J13"):
            if ec2_sheet.Range(address).Validation.Type != XL_VALIDATE_LIST:
                raise RuntimeError(f"EC2 input validation missing from {address}.")
        for address in ("G13", "H13", "I13"):
            if ec2_sheet.Range(address).Validation.Type != XL_VALIDATE_DECIMAL:
                raise RuntimeError(f"EC2 numeric validation missing from {address}.")
        for address in ("K13", "L13"):
            try:
                validation_type = ec2_sheet.Range(address).Validation.Type
            except pywintypes.com_error:
                validation_type = None
            if validation_type == XL_VALIDATE_LIST:
                raise RuntimeError(
                    f"EC2 automatic output {address} still has dropdown validation."
                )
        expected_ec2_headers = {
            "H12": "Source RAM GB / instance",
            "K12": "MI RAM GB",
            "L12": "MI service tier",
            "M12": "MI hardware / data storage",
            "N12": "MI vCores",
            "O12": "Source vCPU",
            "P12": "Source max IOPS",
            "X12": "Compute gross",
            "Y12": "Additional RAM gross",
            "Z12": "Compute + RAM net",
        }
        for address, expected_header in expected_ec2_headers.items():
            if ec2_sheet.Range(address).Value2 != expected_header:
                raise RuntimeError(
                    f"EC2 header {address} is not aligned: "
                    f"{ec2_sheet.Range(address).Value2!r} vs {expected_header!r}"
                )
        for address, expected_color in (
            ("K11", BLUE),
            ("O11", AWS_ORANGE),
            ("X11", BLUE),
            ("H13", YELLOW),
            ("K13", LIGHT_BLUE),
            ("O13", LIGHT_ORANGE),
            ("X13", LIGHT_BLUE),
            ("Y13", LIGHT_BLUE),
        ):
            if int(ec2_sheet.Range(address).Interior.Color) != expected_color:
                raise RuntimeError(f"EC2 color band is incorrect at {address}.")
        if not str(ec2_sheet.Range("K8").Formula or "").upper().startswith(
            "=HYPERLINK("
        ):
            raise RuntimeError("EC2 per-volume detail link did not persist.")
        for index, (label, instance_type, quantity, storage_gb) in enumerate(
            EC2_PRIMED_WORKLOADS
        ):
            row_number = 13 + index
            saved_values = (
                ec2_sheet.Cells(row_number, 1).Value2,
                ec2_sheet.Cells(row_number, 2).Value2,
                numeric(ec2_sheet.Cells(row_number, 3).Value2),
                numeric(ec2_sheet.Cells(row_number, 7).Value2),
            )
            expected_values = (
                label,
                instance_type,
                float(quantity),
                storage_gb,
            )
            if saved_values != expected_values:
                raise RuntimeError(
                    f"EC2 saved inventory row {row_number} differs: "
                    f"{saved_values} vs {expected_values}"
                )
            ebs_profile = str(ec2_sheet.Cells(row_number, 6).Value2 or "")
            expected_profile = EC2_PRIMED_EBS_PROFILES[index]
            if ebs_profile != expected_profile:
                raise RuntimeError(
                    f"EC2 saved EBS profile differs in row {row_number}: "
                    f"{ebs_profile} vs {expected_profile}"
                )
            saved_source_assignment = (
                str(ec2_sheet.Cells(row_number, 4).Value2 or ""),
                str(ec2_sheet.Cells(row_number, 5).Value2 or ""),
            )
            expected_source_assignment = (
                EC2_PRIMED_EDITIONS[index],
                EC2_PRIMED_LICENSE_BASES[index],
            )
            if saved_source_assignment != expected_source_assignment:
                raise RuntimeError(
                    f"EC2 saved source licence assignment differs in row "
                    f"{row_number}: {saved_source_assignment} vs "
                    f"{expected_source_assignment}"
                )
            if str(ec2_sheet.Cells(row_number, 7).Text) != f"{storage_gb:.2f}":
                raise RuntimeError(
                    f"EC2 saved SQL data display is malformed in row {row_number}: "
                    f"{ec2_sheet.Cells(row_number, 7).Text!r}"
                )
            source_memory = numeric(
                ec2_sheet.Cells(row_number, 8).Value2,
                math.nan,
            )
            if not math.isfinite(source_memory) or source_memory <= 0:
                raise RuntimeError(
                    f"EC2 saved source RAM is invalid in row {row_number}: "
                    f"{ec2_sheet.Cells(row_number, 8).Value2!r}"
                )
            if bool(ec2_sheet.Cells(row_number, 8).HasFormula):
                raise RuntimeError(
                    f"EC2 source RAM is not a literal input in row {row_number}."
                )
            if not bool(ec2_sheet.Cells(row_number, 11).HasFormula):
                raise RuntimeError(
                    f"EC2 MI RAM is not automatically derived in row {row_number}."
                )
            if not bool(ec2_sheet.Cells(row_number, 12).HasFormula):
                raise RuntimeError(
                    f"EC2 MI service tier is not automatically derived in row {row_number}."
                )
            hardware_storage = str(
                ec2_sheet.Cells(row_number, 13).Value2 or ""
            )
            if not hardware_storage or not any(
                storage_label in hardware_storage
                for storage_label in ("Remote LRS", "BC local SSD")
            ):
                raise RuntimeError(
                    f"EC2 saved hardware/storage output is missing in row {row_number}."
                )
            expected_iops = ebs_profile_max_iops(ebs_profile)
            actual_iops = numeric(
                ec2_sheet.Cells(row_number, 16).Value2,
                math.nan,
            )
            if not math.isclose(actual_iops, expected_iops, abs_tol=1e-9):
                raise RuntimeError(
                    f"EC2 saved source IOPS mismatch in row {row_number}: "
                    f"{actual_iops} vs {expected_iops}"
                )
            expected_storage = quantity * 12 * ebs_profile_monthly_cost(
                "eu-west-1", ebs_profile
            )
            actual_storage = numeric(ec2_sheet.Cells(row_number, 21).Value2, math.nan)
            if not math.isclose(actual_storage, expected_storage, abs_tol=0.05):
                raise RuntimeError(
                    f"EC2 saved storage mismatch in row {row_number}: "
                    f"{actual_storage} vs {expected_storage}"
                )
        for row_number in range(13 + len(EC2_PRIMED_WORKLOADS), 23):
            stale_inputs = tuple(
                ec2_sheet.Cells(row_number, column).Value2
                for column in range(1, 11)
            )
            if any(value not in (None, "") for value in stale_inputs):
                raise RuntimeError(
                    f"EC2 saved unseeded row {row_number} retained workload inputs: "
                    f"{stale_inputs}"
                )
        if numeric(ec2_sheet.Range("Y15").Value2) <= 0:
            raise RuntimeError(
                "EC2 additional RAM cost did not persist for r6id.8xlarge."
            )
        ebs_detail_sheet = workbook.Worksheets.Item("EC2 EBS Detail")
        ebs_detail_sheet.Activate()
        active_window = excel.ActiveWindow
        if active_window is not None and int(active_window.ScrollColumn) != 1:
            raise RuntimeError("EC2 EBS detail did not persist its column-A opening view.")
        for address, validation_type in (
            (f"D{EBS_DETAIL_FIRST_ROW}", XL_VALIDATE_LIST),
            (f"E{EBS_DETAIL_FIRST_ROW}", XL_VALIDATE_DECIMAL),
            (f"F{EBS_DETAIL_FIRST_ROW}", XL_VALIDATE_WHOLE_NUMBER),
            (f"G{EBS_DETAIL_FIRST_ROW}", XL_VALIDATE_DECIMAL),
        ):
            if ebs_detail_sheet.Range(address).Validation.Type != validation_type:
                raise RuntimeError(
                    f"EC2 EBS detail validation missing from {address}."
                )
        validate_ec2_ebs_detail_sheet(
            excel,
            ebs_detail_sheet,
            "eu-west-1",
            exercise_io2=False,
        )
        errors = scan_visible_errors(ebs_detail_sheet)
        if errors:
            raise RuntimeError(
                f"EC2 EBS detail saved with Excel errors: {', '.join(errors[:10])}"
            )
    finally:
        if workbook is not None:
            workbook.Close(False)


def update_workbook(
    rds_catalog: list[dict[str, object]],
    ec2_catalog: list[dict[str, object]],
    rds_storage_catalog: list[dict[str, object]],
    workbook_path: Path = WORKBOOK_PATH,
) -> dict[str, dict[str, float]]:
    if not workbook_path.exists():
        raise FileNotFoundError(f"Workbook not found: {workbook_path}")
    excel = win32.DispatchEx("Excel.Application")
    excel.Visible = False
    excel.DisplayAlerts = False
    excel.ScreenUpdating = False
    excel.AskToUpdateLinks = False
    workbook = None
    summaries: dict[str, dict[str, float]] = {}
    try:
        workbook = excel.Workbooks.Open(
            str(workbook_path),
            UpdateLinks=0,
            ReadOnly=False,
            IgnoreReadOnlyRecommended=True,
        )
        if workbook.ReadOnly:
            raise RuntimeError("Workbook opened read-only. Close it in Excel and rerun the updater.")
        workbook.Activate()
        preserved_inputs: dict[str, dict[str, object]] = {}
        for source_kind, sheet_name, input_columns in (
            ("RDS", "RDS SQL MI TCO Converter", 9),
            ("EC2", "EC2 TCO Converter", 10),
        ):
            try:
                existing_sheet = workbook.Worksheets.Item(sheet_name)
            except Exception:
                continue
            preserved_inputs[source_kind] = {
                "assumptions": {
                    address: existing_sheet.Range(address).Value2
                    for address in ("B5", "B6", "B7", "E5", "E6", "E7", "H5", "K9")
                },
                "rows": {
                    row: tuple(
                        existing_sheet.Cells(row, column).Value2
                        for column in range(1, input_columns + 1)
                    )
                    for row in range(13, 23)
                },
            }
            if source_kind in {"RDS", "EC2"}:
                header_columns = {
                    str(existing_sheet.Cells(12, column).Value2 or ""): column
                    for column in range(1, 42)
                }
                annual_hours_column = header_columns.get("Annual hours / instance")
                purchase_column = header_columns.get("MI purchase option")
                if annual_hours_column is None or purchase_column is None:
                    raise RuntimeError(
                        "Existing EC2 annual-hours or purchase-option header could not be found."
                    )
                preserved_inputs[source_kind]["annual_hours"] = {
                    row: existing_sheet.Cells(row, annual_hours_column).Value2
                    for row in range(13, 23)
                    if existing_sheet.Cells(row, 2).Value2
                }
                preserved_inputs[source_kind]["purchase_options"] = {
                    row: existing_sheet.Cells(row, purchase_column).Value2
                    for row in range(13, 23)
                    if existing_sheet.Cells(row, 2).Value2
                }
                source_memory_column = header_columns.get("Source RAM GB / instance")
                preserved_inputs[source_kind]["source_memory"] = (
                    {
                        row: existing_sheet.Cells(row, source_memory_column).Value2
                        for row in range(13, 23)
                        if existing_sheet.Cells(row, 2).Value2
                        and existing_sheet.Cells(row, source_memory_column).Value2
                        not in (None, "")
                    }
                    if source_memory_column is not None
                    else {}
                )
                if source_kind == "RDS":
                    source_iops_column = header_columns.get("Source max IOPS")
                    preserved_inputs[source_kind]["source_iops"] = (
                        {
                            row: existing_sheet.Cells(row, source_iops_column).Value2
                            for row in range(13, 23)
                            if existing_sheet.Cells(row, 2).Value2
                            and existing_sheet.Cells(row, source_iops_column).Value2
                            not in (None, "")
                        }
                        if source_iops_column is not None
                        else {}
                    )
        rds_sheet = build_converter_sheet(
            workbook,
            rds_catalog,
            "RDS",
            rds_storage_catalog,
            preserved_inputs.get("RDS"),
        )
        summaries["RDS"] = validate_converter_sheet(
            excel,
            rds_sheet,
            rds_catalog,
            "RDS",
        )
        ec2_sheet = build_converter_sheet(
            workbook,
            ec2_catalog,
            "EC2",
            preserved_inputs=preserved_inputs.get("EC2"),
        )
        summaries["EC2"] = validate_converter_sheet(
            excel,
            ec2_sheet,
            ec2_catalog,
            "EC2",
        )
        remove_workbook_tooltips(workbook)
        ec2_sheet.Activate()
        workbook.Windows.Item(1).Visible = True
        workbook.CheckCompatibility = False
        workbook.Save()
        workbook.Close(False)
        workbook = None
        verify_saved_workbook(excel, workbook_path)
    finally:
        if workbook is not None:
            workbook.Close(False)
        excel.Quit()
    return summaries


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Build AWS-to-Azure SQL MI TCO converter sheets."
    )
    parser.add_argument(
        "--catalog-only",
        action="store_true",
        help="Validate normalized source catalogs without changing the workbook.",
    )
    parser.add_argument(
        "--workbook",
        type=Path,
        default=WORKBOOK_PATH,
        help="Workbook to update and validate (defaults to SQL TCO Calculator.xlsx).",
    )
    args = parser.parse_args()

    mi_options = load_mi_options()
    storage_rates = load_mi_storage_rates()
    memory_rates = load_mi_memory_rates()
    rds_license_rates = load_rds_license_rates()
    rds_storage_catalog = build_rds_storage_catalog()
    rds_catalog = build_rds_catalog(
        mi_options,
        storage_rates,
        memory_rates,
        rds_license_rates,
    )
    ec2_catalog = build_ec2_catalog(mi_options, storage_rates, memory_rates)
    validate_catalogs(rds_catalog, ec2_catalog, rds_license_rates, rds_storage_catalog)

    ec2_sample = next(
        row
        for row in ec2_catalog
        if row["region"] == "eu-west-1" and row["instance_type"] == "r8i.xlarge"
    )
    print(f"RDS selectable configurations: {len(rds_catalog):,}")
    print(f"EC2 selectable configurations: {len(ec2_catalog):,}")
    print(f"RDS storage mappings: {len(rds_storage_catalog):,}")
    print(f"SQL MI option sets: {len(mi_options):,}")
    print(f"SQL MI storage rate keys: {len(storage_rates):,}")
    print(f"SQL MI memory rate keys: {len(memory_rates):,}")
    print(
        "EC2 sample components: "
        f"compute=${ec2_sample['source_compute_hourly']:.4f}/hr, "
        f"Standard SQL=${ec2_sample['source_standard_license_hourly']:.4f}/hr, "
        f"Enterprise SQL=${ec2_sample['source_enterprise_license_hourly']:.4f}/hr"
    )
    print(
        "EC2 sample target: "
        f"{ec2_sample['mi_region']} | {ec2_sample['mi_tier']} | "
        f"{ec2_sample['mi_hardware']} | {ec2_sample['mi_vcores']:.0f} vCores"
    )
    print("Catalog validation: OK")
    if args.catalog_only:
        return

    workbook_path = args.workbook.resolve()
    summaries = update_workbook(
        rds_catalog,
        ec2_catalog,
        rds_storage_catalog,
        workbook_path,
    )
    print(f"Workbook: {workbook_path}")
    for source_kind, summary in summaries.items():
        if summary["unmapped_count"]:
            print(
                f"{source_kind}: AWS all rows=${summary['aws_total']:,.2f}; "
                f"AWS mapped rows=${summary['mapped_aws_total']:,.2f}; "
                f"MI mapped rows=${summary['mi_total']:,.2f}; "
                f"unmapped rows={int(summary['unmapped_count'])}; "
                f"required adjustment={summary['required_adjustment']:.4%}; "
                f"parity test difference=${summary['tested_difference']:,.6f}"
            )
        else:
            print(
                f"{source_kind}: AWS=${summary['aws_total']:,.2f}; "
                f"MI before parity=${summary['mi_total']:,.2f}; "
                f"required adjustment={summary['required_adjustment']:.4%}; "
                f"parity test difference=${summary['tested_difference']:,.6f}"
            )
    print("Workbook converter validation: OK")


if __name__ == "__main__":
    main()