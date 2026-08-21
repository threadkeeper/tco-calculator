import { buildCalculationResultRows, type CalculationResultRow } from './calculation-results';
import { asRecord, readString, type JsonRecord } from './api';
import type { ProjectDraft, ResourceDraft } from './draft';

type ExportContext = {
  project: ProjectDraft;
  calculation: JsonRecord | null;
  resource: ResourceDraft | null;
  row: CalculationResultRow;
};

type ExportValue = string | number | null;

type ExportColumn = {
  header: string;
  value: (context: ExportContext) => ExportValue;
  kind: 'text' | 'decimal';
};

const COLUMNS: ExportColumn[] = [
  textColumn('Project | Name', ({ project }) => project.name),
  textColumn('Project | Description', ({ project }) => project.description),
  textColumn('Project | Source type', ({ project }) => project.settings.project_type),
  textColumn('Project | Source region', ({ project }) => project.settings.aws_region),
  textColumn('Project | Azure region', ({ project }) => project.settings.azure_region),
  textColumn('Project | Currency', ({ project }) => project.settings.currency),
  decimalColumn(
    'Settings | Source compute discount',
    ({ project }) => project.settings.source_compute_discount
  ),
  decimalColumn(
    'Settings | Source SQL license discount',
    ({ project }) => project.settings.source_license_discount
  ),
  decimalColumn(
    'Settings | Source storage discount',
    ({ project }) => project.settings.source_storage_discount
  ),
  decimalColumn(
    'Settings | Azure compute discount',
    ({ project }) => project.settings.azure_compute_discount
  ),
  decimalColumn(
    'Settings | Azure SQL license discount',
    ({ project }) => project.settings.azure_license_discount
  ),
  decimalColumn(
    'Settings | Azure storage discount',
    ({ project }) => project.settings.azure_storage_discount
  ),
  decimalColumn(
    'Settings | Selected parity adjustment',
    ({ project }) => project.settings.selected_parity_adjustment
  ),
  decimalColumn(
    'Settings | Default annual hours',
    ({ project }) => project.settings.default_annual_hours
  ),
  textColumn(
    'Settings | Default MI purchase option',
    ({ project }) => project.settings.default_mi_purchase_option
  ),
  decimalColumn(
    'Settings | Enterprise License + SA / 2-core pack',
    ({ project }) => project.settings.enterprise_license_sa_usd_per_two_core_pack
  ),
  decimalColumn(
    'Settings | Standard License + SA / 2-core pack',
    ({ project }) => project.settings.standard_license_sa_usd_per_two_core_pack
  ),
  decimalColumn(
    'Settings | Remaining coverage months',
    ({ project }) => project.settings.remaining_coverage_months
  ),
  decimalColumn(
    'Settings | Electricity rate USD/kWh',
    ({ project }) => project.settings.electricity_rate_usd_per_kwh
  ),
  textColumn('Calculation | Formula version', ({ calculation }) =>
    readString(calculation, 'formula_version')
  ),
  textColumn('Calculation | AWS snapshot ID', ({ calculation }) =>
    readString(calculation, 'aws_snapshot_id')
  ),
  textColumn('Calculation | Azure snapshot ID', ({ calculation }) =>
    readString(calculation, 'azure_snapshot_id')
  ),
  textColumn('Workload | Name', ({ row }) => row.workloadName),
  textColumn('Workload | Server name', ({ row }) => row.serverName),
  textColumn('Workload | Resource ID', ({ row }) => row.resourceId),
  textColumn('Workload | Source SKU', ({ row }) => row.sourceSku),
  decimalColumn('Workload | Quantity', ({ row }) => row.quantity),
  textColumn('Workload | SQL edition', ({ row }) => row.sqlEdition),
  textColumn('Workload | License basis', ({ row }) => row.licenseBasis),
  decimalColumn('Workload | SQL data GB / instance', ({ row }) => row.sqlDataGbPerInstance),
  decimalColumn('Workload | Source RAM GB / instance', ({ row }) => row.sourceRamGbPerInstance),
  decimalColumn('Workload | Annual hours / instance', ({ row }) => row.annualHoursPerInstance),
  textColumn('Workload | MI purchase option', ({ row }) => row.miPurchaseOption),
  textColumn('Source details | EC2 EBS volumes', ({ resource }) =>
    resource?.source_type === 'ec2' ? JSON.stringify(resource.volumes) : null
  ),
  decimalColumn(
    'Source details | Persistent EBS GB / instance',
    ({ row }) => row.persistentEbsGbPerInstance
  ),
  textColumn('Source details | RDS deployment', ({ resource }) =>
    resource?.source_type === 'rds' ? resource.deployment : null
  ),
  textColumn('Source details | RDS commercial term', ({ resource }) =>
    resource?.source_type === 'rds' ? resource.commercial_term : null
  ),
  textColumn('Source details | RDS storage class', ({ resource }) =>
    resource?.source_type === 'rds' ? resource.storage_class : null
  ),
  decimalColumn('Source details | Source max IOPS', ({ resource }) =>
    resource?.source_type === 'rds' || resource?.source_type === 'on_prem'
      ? resource.source_max_iops
      : null
  ),
  decimalColumn('Source details | On-prem licensable cores', ({ resource }) =>
    resource?.source_type === 'on_prem' ? resource.licensable_cores : null
  ),
  decimalColumn('Source details | On-prem hardware CAPEX USD', ({ resource }) =>
    resource?.source_type === 'on_prem' ? resource.hardware_capex_usd : null
  ),
  decimalColumn('Source details | On-prem depreciation years', ({ resource }) =>
    resource?.source_type === 'on_prem' ? resource.depreciation_years : null
  ),
  decimalColumn('Source details | On-prem average power kW override', ({ resource }) =>
    resource?.source_type === 'on_prem' ? resource.average_power_kw_override : null
  ),
  textColumn('Status | Mapping', ({ row }) => row.mappingStatus),
  textColumn('Status | Source pricing', ({ row }) => row.awsPricingStatus),
  textColumn('Status | Azure pricing', ({ row }) => row.azurePricingStatus),
  decimalColumn('Derived MI | Storage GB / instance', ({ row }) => row.azureStorageGbPerInstance),
  decimalColumn('Derived MI | MI RAM GB', ({ row }) => row.selectedMemoryGb),
  textColumn('Derived MI | Service tier', ({ row }) => row.serviceTier),
  textColumn('Derived MI | Hardware family', ({ row }) => row.hardwareFamily),
  textColumn('Derived MI | Storage architecture', ({ row }) => row.storageArchitecture),
  decimalColumn('Derived MI | vCores', ({ row }) => row.vcores),
  decimalColumn('Source cost | Compute gross', ({ row }) => row.sourceComputeGross),
  decimalColumn('Source cost | Compute net', ({ row }) => row.sourceComputeNet),
  decimalColumn('Source cost | SQL license gross', ({ row }) => row.sourceLicenseGross),
  decimalColumn('Source cost | SQL license net', ({ row }) => row.sourceLicenseNet),
  decimalColumn('Source cost | Storage gross', ({ row }) => row.sourceStorageGross),
  decimalColumn('Source cost | Storage net', ({ row }) => row.sourceStorageNet),
  decimalColumn('Source cost | Hardware annual', ({ row }) => row.sourceHardwareAnnual),
  decimalColumn('Source cost | Electricity annual', ({ row }) => row.sourceElectricityAnnual),
  decimalColumn('Source cost | Net total', ({ row }) => row.sourceTotal),
  decimalColumn('Azure cost | Compute gross', ({ row }) => row.azureComputeGross),
  decimalColumn('Azure cost | Additional RAM GB', ({ row }) => row.azureAdditionalRamGb),
  decimalColumn('Azure cost | Additional RAM gross', ({ row }) => row.azureAdditionalRamGross),
  decimalColumn('Azure cost | Compute + RAM net', ({ row }) => row.azureComputePlusRamNet),
  decimalColumn('Azure cost | SQL license gross', ({ row }) => row.azureLicenseGross),
  decimalColumn('Azure cost | SQL license net', ({ row }) => row.azureLicenseNet),
  decimalColumn('Azure cost | Storage gross', ({ row }) => row.azureStorageGross),
  decimalColumn('Azure cost | Storage net', ({ row }) => row.azureStorageNet),
  decimalColumn('Azure cost | MI net before parity', ({ row }) => row.azureTotalBeforeParity),
  decimalColumn('Savings | Compute', ({ row }) => row.computeSavings),
  decimalColumn('Savings | License', ({ row }) => row.licenseSavings),
  decimalColumn('Savings | Storage', ({ row }) => row.storageSavings),
  decimalColumn('Savings | Total before parity', ({ row }) => row.totalSavings),
  decimalColumn('Parity | Required adjustment', ({ row }) => row.requiredAdjustment),
  decimalColumn('Parity | Selected adjustment', ({ row }) => row.selectedAdjustment),
  decimalColumn('Parity | MI after parity', ({ row }) => row.azureAfterSelectedParity),
  decimalColumn('Parity | Difference (Azure - source)', ({ row }) => row.difference)
];

export function createProjectExportCsv(project: ProjectDraft, calculation: unknown): string {
  const rows = buildCalculationResultRows(calculation, project.resources);
  const calculationRecord = asRecord(calculation);
  const header = COLUMNS.map((column) => csvCell(column.header, 'text')).join(',');
  const body = rows.map((row) => {
    const context = {
      project,
      calculation: calculationRecord,
      resource: project.resources.find((resource) => resource.id === row.resourceId) ?? null,
      row
    };
    return COLUMNS.map((column) => csvCell(column.value(context), column.kind)).join(',');
  });
  return `\uFEFF${[header, ...body].join('\r\n')}\r\n`;
}

export function projectExportFileName(projectName: string): string {
  const stem = projectName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  return `${stem || 'tco-project'}-results.csv`;
}

export function downloadProjectExport(project: ProjectDraft, calculation: unknown): void {
  const blob = new Blob([createProjectExportCsv(project, calculation)], {
    type: 'text/csv;charset=utf-8'
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = projectExportFileName(project.name);
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function textColumn(header: string, value: (context: ExportContext) => ExportValue): ExportColumn {
  return { header, value, kind: 'text' };
}

function decimalColumn(
  header: string,
  value: (context: ExportContext) => ExportValue
): ExportColumn {
  return { header, value, kind: 'decimal' };
}

function csvCell(value: ExportValue, kind: ExportColumn['kind']): string {
  if (value === null) return '""';
  let text = String(value);
  const validDecimal = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(text);
  if (kind === 'text' || !validDecimal) text = protectSpreadsheetText(text);
  return `"${text.replaceAll('"', '""')}"`;
}

function protectSpreadsheetText(value: string): string {
  return /^[\u0000-\u0020]*[=+\-@]/.test(value) ? `'${value}` : value;
}
