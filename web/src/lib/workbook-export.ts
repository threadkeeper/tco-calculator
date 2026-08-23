import { buildCalculationResultRows, type CalculationResultRow } from './calculation-results';
import { asRecord, readString, type JsonRecord } from './api';
import { projectRequestPayload, type ProjectDraft, type ResourceDraft } from './draft';

type ExportContext = {
  project: ProjectDraft;
  calculation: JsonRecord | null;
  resource: ResourceDraft | null;
  row: CalculationResultRow;
};

type ExportValue = string | number | boolean | null;

type ExportColumn = {
  header: string;
  value: (context: ExportContext) => ExportValue;
  kind: 'text' | 'decimal';
};

type ZipEntry = {
  name: string;
  data: Uint8Array<ArrayBuffer>;
  crc32: number;
  offset: number;
};

type InputRow = {
  section: string;
  workload: string;
  input: string;
  value: ExportValue;
};

const XLSX_MIME_TYPE = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
const UTF8_ENCODER = new TextEncoder();
const CRC32_TABLE = createCrc32Table();

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

export function createProjectExportXlsx(
  project: ProjectDraft,
  calculation: unknown
): Uint8Array<ArrayBuffer> {
  const rows = buildCalculationResultRows(calculation, project.resources);
  const calculationRecord = asRecord(calculation);
  const values = rows.map((row) => {
    const context = {
      project,
      calculation: calculationRecord,
      resource: project.resources.find((resource) => resource.id === row.resourceId) ?? null,
      row
    };
    return COLUMNS.map((column) => column.value(context));
  });

  return createStoredZip([
    ['[Content_Types].xml', contentTypesXml()],
    ['_rels/.rels', packageRelationshipsXml()],
    ['docProps/app.xml', appPropertiesXml()],
    ['docProps/core.xml', corePropertiesXml()],
    ['xl/workbook.xml', workbookXml()],
    ['xl/_rels/workbook.xml.rels', workbookRelationshipsXml()],
    ['xl/styles.xml', stylesXml()],
    ['xl/worksheets/sheet1.xml', worksheetXml(values)],
    ['xl/worksheets/sheet2.xml', inputsWorksheetXml(buildInputRows(project))]
  ]);
}

export function projectExportFileName(projectName: string): string {
  const stem = projectName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  return `${stem || 'tco-project'}-results.xlsx`;
}

export function downloadProjectExport(project: ProjectDraft, calculation: unknown): void {
  const blob = new Blob([createProjectExportXlsx(project, calculation)], {
    type: XLSX_MIME_TYPE
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

function worksheetXml(rows: ExportValue[][]): string {
  const lastColumn = columnName(COLUMNS.length);
  const lastRow = rows.length + 1;
  const columns = COLUMNS.map(
    (column, index) =>
      `<col min="${index + 1}" max="${index + 1}" width="${columnWidth(column)}" customWidth="1"/>`
  ).join('');
  const header = COLUMNS.map((column, index) =>
    inlineStringCellXml(`${columnName(index + 1)}1`, column.header, headerStyleId(column.header))
  ).join('');
  const body = rows
    .map((row, rowIndex) => {
      const cells = COLUMNS.map((column, columnIndex) =>
        inlineStringCellXml(
          `${columnName(columnIndex + 1)}${rowIndex + 2}`,
          row[columnIndex] ?? null,
          bodyStyleId(column.kind, rowIndex)
        )
      ).join('');
      return `<row r="${rowIndex + 2}">${cells}</row>`;
    })
    .join('');

  return xmlDocument(`<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>
  <dimension ref="A1:${lastColumn}${lastRow}"/>
  <sheetViews><sheetView tabSelected="1" workbookViewId="0"><pane xSplit="3" ySplit="1" topLeftCell="D2" activePane="bottomRight" state="frozen"/><selection pane="bottomRight" activeCell="D2" sqref="D2"/></sheetView></sheetViews>
  <sheetFormatPr defaultRowHeight="15"/>
  <cols>${columns}</cols>
  <sheetData><row r="1" ht="42" customHeight="1">${header}</row>${body}</sheetData>
  <autoFilter ref="A1:${lastColumn}${lastRow}"/>
  <printOptions horizontalCentered="0" verticalCentered="0"/>
  <pageMargins left="0.25" right="0.25" top="0.5" bottom="0.5" header="0.2" footer="0.2"/>
  <pageSetup orientation="landscape" fitToWidth="1" fitToHeight="0" paperSize="9"/>
</worksheet>`);
}

function inputsWorksheetXml(rows: InputRow[]): string {
  const headers = ['Section', 'Workload', 'Input', 'Value'];
  const widths = [22, 28, 48, 32];
  const lastRow = rows.length + 1;
  const columns = widths
    .map(
      (width, index) =>
        `<col min="${index + 1}" max="${index + 1}" width="${width}" customWidth="1"/>`
    )
    .join('');
  const header = headers
    .map((value, index) => inlineStringCellXml(`${columnName(index + 1)}1`, value, 1))
    .join('');
  const body = rows
    .map((row, rowIndex) => {
      const values: ExportValue[] = [row.section, row.workload, row.input, row.value];
      const cells = values
        .map((value, columnIndex) =>
          inlineStringCellXml(
            `${columnName(columnIndex + 1)}${rowIndex + 2}`,
            value,
            bodyStyleId('text', rowIndex)
          )
        )
        .join('');
      return `<row r="${rowIndex + 2}">${cells}</row>`;
    })
    .join('');

  return xmlDocument(`<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>
  <dimension ref="A1:D${lastRow}"/>
  <sheetViews><sheetView workbookViewId="0"><pane xSplit="2" ySplit="1" topLeftCell="C2" activePane="bottomRight" state="frozen"/><selection pane="bottomRight" activeCell="C2" sqref="C2"/></sheetView></sheetViews>
  <sheetFormatPr defaultRowHeight="15"/>
  <cols>${columns}</cols>
  <sheetData><row r="1" ht="24" customHeight="1">${header}</row>${body}</sheetData>
  <autoFilter ref="A1:D${lastRow}"/>
  <printOptions horizontalCentered="0" verticalCentered="0"/>
  <pageMargins left="0.25" right="0.25" top="0.5" bottom="0.5" header="0.2" footer="0.2"/>
  <pageSetup orientation="landscape" fitToWidth="1" fitToHeight="0" paperSize="9"/>
</worksheet>`);
}

function buildInputRows(project: ProjectDraft): InputRow[] {
  const payload = projectRequestPayload(project);
  const rows: InputRow[] = [
    { section: 'Project', workload: '', input: 'name', value: payload.name },
    { section: 'Project', workload: '', input: 'description', value: payload.description },
    {
      section: 'Project',
      workload: '',
      input: 'aws_price_snapshot_id',
      value: payload.aws_price_snapshot_id
    },
    {
      section: 'Project',
      workload: '',
      input: 'azure_price_snapshot_id',
      value: payload.azure_price_snapshot_id
    }
  ];

  appendInputRows(rows, payload.settings, 'settings', 'Project settings', '');
  if (payload.resources.length === 0) {
    rows.push({ section: 'Resources', workload: '', input: 'resources', value: '[]' });
  } else {
    payload.resources.forEach((resource, index) => {
      appendInputRows(
        rows,
        resource,
        `resources[${index + 1}]`,
        `Resource ${index + 1}`,
        resource.workload_name
      );
    });
  }

  return rows;
}

function appendInputRows(
  rows: InputRow[],
  value: unknown,
  path: string,
  section: string,
  workload: string
): void {
  if (value === null || ['string', 'number', 'boolean'].includes(typeof value)) {
    rows.push({ section, workload, input: path, value: value as ExportValue });
    return;
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      rows.push({ section, workload, input: path, value: '[]' });
      return;
    }
    value.forEach((item, index) => {
      appendInputRows(rows, item, `${path}[${index + 1}]`, section, workload);
    });
    return;
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length === 0) {
      rows.push({ section, workload, input: path, value: '{}' });
      return;
    }
    entries.forEach(([key, child]) => {
      appendInputRows(rows, child, `${path}.${key}`, section, workload);
    });
  }
}

function inlineStringCellXml(reference: string, value: ExportValue, styleId: number): string {
  if (value === null) return `<c r="${reference}" s="${styleId}"/>`;
  return `<c r="${reference}" s="${styleId}" t="inlineStr"><is><t xml:space="preserve">${escapeXml(String(value))}</t></is></c>`;
}

function headerStyleId(header: string): number {
  if (header.startsWith('Source ')) return 2;
  if (header.startsWith('Derived MI') || header.startsWith('Azure cost')) return 3;
  if (header.startsWith('Savings')) return 4;
  if (header.startsWith('Parity')) return 5;
  return 1;
}

function bodyStyleId(kind: ExportColumn['kind'], rowIndex: number): number {
  const alternateRow = rowIndex % 2 === 1;
  if (kind === 'decimal') return alternateRow ? 9 : 7;
  return alternateRow ? 8 : 6;
}

function columnWidth(column: ExportColumn): string {
  if (column.header === 'Source details | EC2 EBS volumes') return '42';
  const minimum = column.kind === 'decimal' ? 16 : 14;
  return String(Math.min(28, Math.max(minimum, Math.ceil(column.header.length * 0.72))));
}

function columnName(index: number): string {
  let value = index;
  let name = '';
  while (value > 0) {
    value -= 1;
    name = String.fromCharCode(65 + (value % 26)) + name;
    value = Math.floor(value / 26);
  }
  return name;
}

function contentTypesXml(): string {
  return xmlDocument(`<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>
  <Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>`);
}

function packageRelationshipsXml(): string {
  return xmlDocument(`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>`);
}

function appPropertiesXml(): string {
  return xmlDocument(`<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
  <Application>TCO Calculator</Application>
  <DocSecurity>0</DocSecurity>
  <ScaleCrop>false</ScaleCrop>
  <HeadingPairs><vt:vector size="2" baseType="variant"><vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant></vt:vector></HeadingPairs>
  <TitlesOfParts><vt:vector size="2" baseType="lpstr"><vt:lpstr>TCO Results</vt:lpstr><vt:lpstr>Inputs</vt:lpstr></vt:vector></TitlesOfParts>
  <Company/>
  <LinksUpToDate>false</LinksUpToDate>
  <SharedDoc>false</SharedDoc>
  <HyperlinksChanged>false</HyperlinksChanged>
  <AppVersion>1.0</AppVersion>
</Properties>`);
}

function corePropertiesXml(): string {
  return xmlDocument(`<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title>TCO calculation results</dc:title>
  <dc:subject>Azure SQL Managed Instance total cost of ownership estimate</dc:subject>
  <dc:creator>TCO Calculator</dc:creator>
  <cp:lastModifiedBy>TCO Calculator</cp:lastModifiedBy>
</cp:coreProperties>`);
}

function workbookXml(): string {
  return xmlDocument(`<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <fileVersion appName="Microsoft Excel"/>
  <workbookPr date1904="0"/>
  <bookViews><workbookView activeTab="0"/></bookViews>
  <sheets><sheet name="TCO Results" sheetId="1" r:id="rId1"/><sheet name="Inputs" sheetId="2" r:id="rId2"/></sheets>
  <calcPr calcId="191029"/>
</workbook>`);
}

function workbookRelationshipsXml(): string {
  return xmlDocument(`<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>`);
}

function stylesXml(): string {
  return xmlDocument(`<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <fonts count="2">
    <font><sz val="11"/><color rgb="FF242424"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Aptos Display"/><family val="2"/><scheme val="minor"/></font>
  </fonts>
  <fills count="8">
    <fill><patternFill patternType="none"/></fill>
    <fill><patternFill patternType="gray125"/></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF4472C4"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FFED7D31"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF008C95"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF70AD47"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF7030A0"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FFF2F2F2"/><bgColor indexed="64"/></patternFill></fill>
  </fills>
  <borders count="2">
    <border><left/><right/><top/><bottom/><diagonal/></border>
    <border><left style="thin"><color rgb="FFD9E2F3"/></left><right style="thin"><color rgb="FFD9E2F3"/></right><top style="thin"><color rgb="FFD9E2F3"/></top><bottom style="thin"><color rgb="FFD9E2F3"/></bottom><diagonal/></border>
  </borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="10">
    <xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>
    <xf numFmtId="0" fontId="1" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="1" fillId="3" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="1" fillId="4" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="1" fillId="5" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="1" fillId="6" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="0" borderId="1" xfId="0" applyNumberFormat="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="0" borderId="1" xfId="0" applyNumberFormat="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="top"/></xf>
    <xf numFmtId="49" fontId="0" fillId="7" borderId="1" xfId="0" applyNumberFormat="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="7" borderId="1" xfId="0" applyNumberFormat="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="top"/></xf>
  </cellXfs>
  <cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>
  <dxfs count="0"/>
  <tableStyles count="0" defaultTableStyle="TableStyleMedium2" defaultPivotStyle="PivotStyleLight16"/>
</styleSheet>`);
}

function xmlDocument(contents: string): string {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n${contents}`;
}

function escapeXml(value: string): string {
  return value
    .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\ufffe\uffff]/g, '\ufffd')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
}

function createStoredZip(files: Array<[name: string, contents: string]>): Uint8Array<ArrayBuffer> {
  const entries: ZipEntry[] = [];
  const localChunks: Uint8Array[] = [];
  let localSize = 0;

  for (const [name, contents] of files) {
    const nameBytes = UTF8_ENCODER.encode(name);
    const data = UTF8_ENCODER.encode(contents);
    const entry = { name, data, crc32: crc32(data), offset: localSize };
    const header = new Uint8Array(30 + nameBytes.length);
    const view = new DataView(header.buffer);
    view.setUint32(0, 0x04034b50, true);
    view.setUint16(4, 20, true);
    view.setUint16(6, 0x0800, true);
    view.setUint16(8, 0, true);
    view.setUint16(10, 0, true);
    view.setUint16(12, 0x0021, true);
    view.setUint32(14, entry.crc32, true);
    view.setUint32(18, data.length, true);
    view.setUint32(22, data.length, true);
    view.setUint16(26, nameBytes.length, true);
    view.setUint16(28, 0, true);
    header.set(nameBytes, 30);
    localChunks.push(header, data);
    entries.push(entry);
    localSize += header.length + data.length;
  }

  const centralChunks = entries.map((entry) => {
    const nameBytes = UTF8_ENCODER.encode(entry.name);
    const header = new Uint8Array(46 + nameBytes.length);
    const view = new DataView(header.buffer);
    view.setUint32(0, 0x02014b50, true);
    view.setUint16(4, 20, true);
    view.setUint16(6, 20, true);
    view.setUint16(8, 0x0800, true);
    view.setUint16(10, 0, true);
    view.setUint16(12, 0, true);
    view.setUint16(14, 0x0021, true);
    view.setUint32(16, entry.crc32, true);
    view.setUint32(20, entry.data.length, true);
    view.setUint32(24, entry.data.length, true);
    view.setUint16(28, nameBytes.length, true);
    view.setUint16(30, 0, true);
    view.setUint16(32, 0, true);
    view.setUint16(34, 0, true);
    view.setUint16(36, 0, true);
    view.setUint32(38, 0, true);
    view.setUint32(42, entry.offset, true);
    header.set(nameBytes, 46);
    return header;
  });
  const centralSize = centralChunks.reduce((total, chunk) => total + chunk.length, 0);
  const end = new Uint8Array(22);
  const endView = new DataView(end.buffer);
  endView.setUint32(0, 0x06054b50, true);
  endView.setUint16(4, 0, true);
  endView.setUint16(6, 0, true);
  endView.setUint16(8, entries.length, true);
  endView.setUint16(10, entries.length, true);
  endView.setUint32(12, centralSize, true);
  endView.setUint32(16, localSize, true);
  endView.setUint16(20, 0, true);

  return concatenateBytes([...localChunks, ...centralChunks, end]);
}

function concatenateBytes(chunks: Uint8Array<ArrayBufferLike>[]): Uint8Array<ArrayBuffer> {
  const result = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

function createCrc32Table(): Uint32Array {
  const table = new Uint32Array(256);
  for (let index = 0; index < table.length; index += 1) {
    let value = index;
    for (let bit = 0; bit < 8; bit += 1) {
      value = (value & 1) === 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
    }
    table[index] = value >>> 0;
  }
  return table;
}

function crc32(data: Uint8Array): number {
  let value = 0xffffffff;
  for (const byte of data) {
    value = CRC32_TABLE[(value ^ byte) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}
