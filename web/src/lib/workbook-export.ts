import { buildCalculationResultRows, type CalculationResultRow } from './calculation-results';
import { asRecord, readString, type JsonRecord } from './api';
import { projectRequestPayload, type ProjectDraft } from './draft';

type ExportValue = string | number | boolean | null;
type ResultTone = 'workload' | 'target' | 'source' | 'azure' | 'savings' | 'parity';

type ExportColumn = {
  header: string;
  value: (row: CalculationResultRow) => ExportValue;
  kind: 'text' | 'decimal';
};

type ResultGroup = {
  label: string;
  tone: ResultTone;
  columns: ExportColumn[];
};

type ResultColumn = ExportColumn & { tone: ResultTone };

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

type SheetSpan = {
  start: number;
  end: number;
  value: ExportValue;
  styleId: number;
};

const STYLE = {
  inputHeader: 1,
  inputBody: 2,
  inputBodyAlternate: 3,
  eyebrow: 4,
  title: 5,
  metadataLabel: 6,
  metadataValue: 7,
  groupWorkload: 8,
  groupTarget: 9,
  groupSource: 10,
  groupAzure: 11,
  groupSavings: 12,
  groupParity: 13,
  columnWorkloadFirst: 14,
  columnWorkload: 15,
  columnTarget: 16,
  columnSource: 17,
  columnAzure: 18,
  columnSavings: 19,
  columnParity: 20,
  bodyWorkload: 21,
  bodyTarget: 22,
  bodySource: 23,
  bodyAzure: 24,
  bodySavings: 25,
  bodyParity: 26,
  bodyWorkloadName: 27,
  differenceHigher: 28,
  differenceLower: 29
} as const;

const GROUP_STYLE_IDS: Record<ResultTone, number> = {
  workload: STYLE.groupWorkload,
  target: STYLE.groupTarget,
  source: STYLE.groupSource,
  azure: STYLE.groupAzure,
  savings: STYLE.groupSavings,
  parity: STYLE.groupParity
};

const COLUMN_STYLE_IDS: Record<ResultTone, number> = {
  workload: STYLE.columnWorkload,
  target: STYLE.columnTarget,
  source: STYLE.columnSource,
  azure: STYLE.columnAzure,
  savings: STYLE.columnSavings,
  parity: STYLE.columnParity
};

const BODY_STYLE_IDS: Record<ResultTone, number> = {
  workload: STYLE.bodyWorkload,
  target: STYLE.bodyTarget,
  source: STYLE.bodySource,
  azure: STYLE.bodyAzure,
  savings: STYLE.bodySavings,
  parity: STYLE.bodyParity
};

const XLSX_MIME_TYPE = 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';
const UTF8_ENCODER = new TextEncoder();
const CRC32_TABLE = createCrc32Table();

const RESULT_GROUPS: ResultGroup[] = [
  {
    label: 'Workload',
    tone: 'workload',
    columns: [
      textColumn('Name', (row) => row.workloadName),
      textColumn('Server name', (row) => row.serverName),
      textColumn('Source SKU', (row) => row.sourceSku),
      decimalColumn('Qty', (row) => row.quantity),
      textColumn('SQL edition', (row) => row.sqlEdition),
      textColumn('License', (row) => row.licenseBasis),
      decimalColumn('SQL data GB', (row) => row.sqlDataGbPerInstance),
      decimalColumn('Persistent EBS GB', (row) => row.persistentEbsGbPerInstance),
      decimalColumn('Source RAM GB', (row) => row.sourceRamGbPerInstance),
      decimalColumn('Annual hours', (row) => row.annualHoursPerInstance),
      textColumn('MI purchase', (row) => row.miPurchaseOption)
    ]
  },
  {
    label: 'Derived MI SKU',
    tone: 'target',
    columns: [
      decimalColumn('MI storage GB', (row) => row.azureStorageGbPerInstance),
      decimalColumn('MI RAM GB', (row) => row.selectedMemoryGb),
      textColumn('Service tier', (row) => row.serviceTier),
      textColumn('Hardware', (row) => row.hardwareFamily),
      textColumn('Storage architecture', (row) => row.storageArchitecture),
      decimalColumn('vCores', (row) => row.vcores)
    ]
  },
  {
    label: 'Source cost',
    tone: 'source',
    columns: [
      decimalColumn('Compute gross', (row) => row.sourceComputeGross),
      decimalColumn('Compute net', (row) => row.sourceComputeNet),
      decimalColumn('License gross', (row) => row.sourceLicenseGross),
      decimalColumn('License net', (row) => row.sourceLicenseNet),
      decimalColumn('Storage gross', (row) => row.sourceStorageGross),
      decimalColumn('Storage net', (row) => row.sourceStorageNet),
      decimalColumn('Hardware annual', (row) => row.sourceHardwareAnnual),
      decimalColumn('Electricity annual', (row) => row.sourceElectricityAnnual),
      decimalColumn('Net total', (row) => row.sourceTotal)
    ]
  },
  {
    label: 'Azure SQL MI cost',
    tone: 'azure',
    columns: [
      decimalColumn('Compute gross', (row) => row.azureComputeGross),
      decimalColumn('Additional RAM GB', (row) => row.azureAdditionalRamGb),
      decimalColumn('Additional RAM gross', (row) => row.azureAdditionalRamGross),
      decimalColumn('Compute + RAM net', (row) => row.azureComputePlusRamNet),
      decimalColumn('License gross', (row) => row.azureLicenseGross),
      decimalColumn('License net', (row) => row.azureLicenseNet),
      decimalColumn('Storage gross', (row) => row.azureStorageGross),
      decimalColumn('Storage net', (row) => row.azureStorageNet),
      decimalColumn('MI net before parity', (row) => row.azureTotalBeforeParity)
    ]
  },
  {
    label: 'Savings before parity',
    tone: 'savings',
    columns: [
      decimalColumn('Compute', (row) => row.computeSavings),
      decimalColumn('License', (row) => row.licenseSavings),
      decimalColumn('Storage', (row) => row.storageSavings),
      decimalColumn('Total', (row) => row.totalSavings)
    ]
  },
  {
    label: 'Parity',
    tone: 'parity',
    columns: [
      decimalColumn('Required adjustment', (row) => row.requiredAdjustment),
      decimalColumn('Selected adjustment', (row) => row.selectedAdjustment),
      decimalColumn('MI after parity', (row) => row.azureAfterSelectedParity),
      decimalColumn('Difference (Azure - source)', (row) => row.difference)
    ]
  }
];

const RESULT_COLUMNS: ResultColumn[] = RESULT_GROUPS.flatMap((group) =>
  group.columns.map((column) => ({ ...column, tone: group.tone }))
);

export function createProjectExportXlsx(
  project: ProjectDraft,
  calculation: unknown
): Uint8Array<ArrayBuffer> {
  const rows = buildCalculationResultRows(calculation, project.resources);
  const calculationRecord = asRecord(calculation);

  return createStoredZip([
    ['[Content_Types].xml', contentTypesXml()],
    ['_rels/.rels', packageRelationshipsXml()],
    ['docProps/app.xml', appPropertiesXml()],
    ['docProps/core.xml', corePropertiesXml()],
    ['xl/workbook.xml', workbookXml()],
    ['xl/_rels/workbook.xml.rels', workbookRelationshipsXml()],
    ['xl/styles.xml', stylesXml()],
    ['xl/worksheets/sheet1.xml', worksheetXml(project, calculationRecord, rows)],
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

function textColumn(
  header: string,
  value: (row: CalculationResultRow) => ExportValue
): ExportColumn {
  return { header, value, kind: 'text' };
}

function decimalColumn(
  header: string,
  value: (row: CalculationResultRow) => ExportValue
): ExportColumn {
  return { header, value, kind: 'decimal' };
}

function worksheetXml(
  project: ProjectDraft,
  calculation: JsonRecord | null,
  rows: CalculationResultRow[]
): string {
  const lastColumn = columnName(RESULT_COLUMNS.length);
  const lastRow = Math.max(7, rows.length + 7);
  const columns = RESULT_COLUMNS.map(
    (_, index) =>
      `<col min="${index + 1}" max="${index + 1}" width="${resultColumnWidth(index)}" customWidth="1"/>`
  ).join('');
  const metadataRows = resultMetadataRows(project, calculation);
  const groupSpans = resultGroupSpans();
  const metadata = metadataRows
    .map(({ row, height, spans }) => sheetRowXml(row, height, spans))
    .join('');
  const groups = sheetRowXml(6, 24, groupSpans);
  const header = RESULT_COLUMNS.map((column, index) =>
    inlineStringCellXml(
      `${columnName(index + 1)}7`,
      column.header.toUpperCase(),
      index === 0 ? STYLE.columnWorkloadFirst : COLUMN_STYLE_IDS[column.tone]
    )
  ).join('');
  const body = rows
    .map((row, rowIndex) => {
      const cells = RESULT_COLUMNS.map((column, columnIndex) => {
        const value = column.value(row);
        return inlineStringCellXml(
          `${columnName(columnIndex + 1)}${rowIndex + 8}`,
          value,
          resultBodyStyleId(column, columnIndex, value)
        );
      }).join('');
      return `<row r="${rowIndex + 8}" ht="26" customHeight="1">${cells}</row>`;
    })
    .join('');
  const mergeRefs = [
    ...metadataRows.flatMap(({ row, spans }) => mergedCellRefs(row, spans)),
    ...mergedCellRefs(6, groupSpans)
  ];
  const mergedCells = mergeRefs.map((reference) => `<mergeCell ref="${reference}"/>`).join('');

  return xmlDocument(`<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><tabColor rgb="FF86C8ED"/><pageSetUpPr fitToPage="1"/></sheetPr>
  <dimension ref="A1:${lastColumn}${lastRow}"/>
  <sheetViews><sheetView tabSelected="1" workbookViewId="0" showGridLines="0" zoomScale="85" zoomScaleNormal="85"/></sheetViews>
  <sheetFormatPr defaultRowHeight="26"/>
  <cols>${columns}</cols>
  <sheetData>${metadata}${groups}<row r="7" ht="44" customHeight="1">${header}</row>${body}</sheetData>
  <autoFilter ref="A7:${lastColumn}${lastRow}"/>
  <mergeCells count="${mergeRefs.length}">${mergedCells}</mergeCells>
  <printOptions horizontalCentered="0" verticalCentered="0"/>
  <pageMargins left="0.25" right="0.25" top="0.5" bottom="0.5" header="0.2" footer="0.2"/>
  <pageSetup orientation="landscape" fitToWidth="1" fitToHeight="0" paperSize="9"/>
</worksheet>`);
}

function resultMetadataRows(
  project: ProjectDraft,
  calculation: JsonRecord | null
): Array<{ row: number; height: number; spans: SheetSpan[] }> {
  return [
    {
      row: 1,
      height: 18,
      spans: [{ start: 1, end: 43, value: 'RESOURCE LINE ITEMS', styleId: STYLE.eyebrow }]
    },
    {
      row: 2,
      height: 26,
      spans: [{ start: 1, end: 43, value: 'Workbook-level detail', styleId: STYLE.title }]
    },
    {
      row: 3,
      height: 30,
      spans: [
        { start: 1, end: 2, value: 'PROJECT', styleId: STYLE.metadataLabel },
        { start: 3, end: 11, value: project.name, styleId: STYLE.metadataValue },
        { start: 12, end: 13, value: 'DESCRIPTION', styleId: STYLE.metadataLabel },
        { start: 14, end: 43, value: project.description, styleId: STYLE.metadataValue }
      ]
    },
    {
      row: 4,
      height: 28,
      spans: [
        { start: 1, end: 2, value: 'SOURCE TYPE', styleId: STYLE.metadataLabel },
        {
          start: 3,
          end: 6,
          value: project.settings.project_type,
          styleId: STYLE.metadataValue
        },
        { start: 7, end: 8, value: 'SOURCE REGION', styleId: STYLE.metadataLabel },
        {
          start: 9,
          end: 13,
          value: project.settings.aws_region,
          styleId: STYLE.metadataValue
        },
        { start: 14, end: 15, value: 'AZURE REGION', styleId: STYLE.metadataLabel },
        {
          start: 16,
          end: 20,
          value: project.settings.azure_region,
          styleId: STYLE.metadataValue
        },
        { start: 21, end: 22, value: 'CURRENCY', styleId: STYLE.metadataLabel },
        {
          start: 23,
          end: 24,
          value: project.settings.currency,
          styleId: STYLE.metadataValue
        },
        { start: 25, end: 27, value: 'FORMULA', styleId: STYLE.metadataLabel },
        {
          start: 28,
          end: 43,
          value: readString(calculation, 'formula_version'),
          styleId: STYLE.metadataValue
        }
      ]
    },
    {
      row: 5,
      height: 28,
      spans: [
        { start: 1, end: 4, value: 'AWS SNAPSHOT', styleId: STYLE.metadataLabel },
        {
          start: 5,
          end: 21,
          value: readString(calculation, 'aws_snapshot_id'),
          styleId: STYLE.metadataValue
        },
        { start: 22, end: 25, value: 'AZURE SNAPSHOT', styleId: STYLE.metadataLabel },
        {
          start: 26,
          end: 43,
          value: readString(calculation, 'azure_snapshot_id'),
          styleId: STYLE.metadataValue
        }
      ]
    }
  ];
}

function resultGroupSpans(): SheetSpan[] {
  let start = 1;
  return RESULT_GROUPS.map((group) => {
    const span = {
      start,
      end: start + group.columns.length - 1,
      value: group.label.toUpperCase(),
      styleId: GROUP_STYLE_IDS[group.tone]
    };
    start = span.end + 1;
    return span;
  });
}

function sheetRowXml(row: number, height: number, spans: SheetSpan[]): string {
  const cells = spans
    .flatMap((span) =>
      Array.from({ length: span.end - span.start + 1 }, (_, offset) =>
        inlineStringCellXml(
          `${columnName(span.start + offset)}${row}`,
          offset === 0 ? span.value : null,
          span.styleId
        )
      )
    )
    .join('');
  return `<row r="${row}" ht="${height}" customHeight="1">${cells}</row>`;
}

function mergedCellRefs(row: number, spans: SheetSpan[]): string[] {
  return spans
    .filter((span) => span.end > span.start)
    .map((span) => `${columnName(span.start)}${row}:${columnName(span.end)}${row}`);
}

function resultBodyStyleId(column: ResultColumn, columnIndex: number, value: ExportValue): number {
  if (columnIndex === 0) return STYLE.bodyWorkloadName;
  if (column.header === 'Difference (Azure - source)') {
    const number = Number(value);
    if (Number.isFinite(number) && number > 0) return STYLE.differenceHigher;
    if (Number.isFinite(number) && number < 0) return STYLE.differenceLower;
  }
  return BODY_STYLE_IDS[column.tone];
}

function resultColumnWidth(columnIndex: number): string {
  return columnIndex === 0 ? '27' : '17';
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
    .map((value, index) =>
      inlineStringCellXml(`${columnName(index + 1)}1`, value, STYLE.inputHeader)
    )
    .join('');
  const body = rows
    .map((row, rowIndex) => {
      const values: ExportValue[] = [row.section, row.workload, row.input, row.value];
      const cells = values
        .map((value, columnIndex) =>
          inlineStringCellXml(
            `${columnName(columnIndex + 1)}${rowIndex + 2}`,
            value,
            inputBodyStyleId(rowIndex)
          )
        )
        .join('');
      return `<row r="${rowIndex + 2}">${cells}</row>`;
    })
    .join('');

  return xmlDocument(`<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>
  <dimension ref="A1:D${lastRow}"/>
  <sheetViews><sheetView workbookViewId="0"/></sheetViews>
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

function inputBodyStyleId(rowIndex: number): number {
  return rowIndex % 2 === 1 ? STYLE.inputBodyAlternate : STYLE.inputBody;
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
  <fonts count="20">
    <font><sz val="9"/><color rgb="FFEEF5F7"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><sz val="11"/><color rgb="FF242424"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="11"/><color rgb="FFFFFFFF"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="7.5"/><color rgb="FFA8B7BD"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="14"/><color rgb="FFF5F9FA"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="7.5"/><color rgb="FFC3D0D5"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><sz val="9"/><color rgb="FFEEF5F7"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="9"/><color rgb="FFF5F9FA"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="9"/><color rgb="FF86C8ED"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="9"/><color rgb="FFEFAD52"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="9"/><color rgb="FF64C994"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="9"/><color rgb="FFC898FD"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="8"/><color rgb="FFC3D0D5"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="8"/><color rgb="FF86C8ED"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="8"/><color rgb="FFEFAD52"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="8"/><color rgb="FF64C994"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="8"/><color rgb="FFC898FD"/><name val="Bahnschrift"/><family val="2"/></font>
    <font><b/><sz val="9.5"/><color rgb="FFF5F9FA"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="9"/><color rgb="FFEF8A80"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
    <font><b/><sz val="9"/><color rgb="FF64C994"/><name val="Aptos"/><family val="2"/><scheme val="minor"/></font>
  </fonts>
  <fills count="11">
    <fill><patternFill patternType="none"/></fill>
    <fill><patternFill patternType="gray125"/></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF4472C4"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FFF2F2F2"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF1D292F"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF202A2E"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF182D3D"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF38291F"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF183239"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF1B3425"/><bgColor indexed="64"/></patternFill></fill>
    <fill><patternFill patternType="solid"><fgColor rgb="FF30243A"/><bgColor indexed="64"/></patternFill></fill>
  </fills>
  <borders count="3">
    <border><left/><right/><top/><bottom/><diagonal/></border>
    <border><left style="thin"><color rgb="FFD9E2F3"/></left><right style="thin"><color rgb="FFD9E2F3"/></right><top style="thin"><color rgb="FFD9E2F3"/></top><bottom style="thin"><color rgb="FFD9E2F3"/></bottom><diagonal/></border>
    <border><left style="thin"><color rgb="FF2D3E47"/></left><right style="thin"><color rgb="FF2D3E47"/></right><top style="thin"><color rgb="FF2D3E47"/></top><bottom style="thin"><color rgb="FF2D3E47"/></bottom><diagonal/></border>
  </borders>
  <cellStyleXfs count="1"><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellStyleXfs>
  <cellXfs count="30">
    <xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0"/>
    <xf numFmtId="0" fontId="2" fillId="2" borderId="1" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="center" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="1" fillId="0" borderId="1" xfId="0" applyNumberFormat="1" applyFont="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="1" fillId="3" borderId="1" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment vertical="top" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="3" fillId="4" borderId="0" xfId="0" applyFont="1" applyFill="1" applyAlignment="1"><alignment horizontal="left" vertical="bottom"/></xf>
    <xf numFmtId="0" fontId="4" fillId="4" borderId="0" xfId="0" applyFont="1" applyFill="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="5" fillId="4" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="6" fillId="4" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="7" fillId="5" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="8" fillId="6" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="9" fillId="7" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="8" fillId="8" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="10" fillId="9" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="11" fillId="10" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center"/></xf>
    <xf numFmtId="0" fontId="12" fillId="5" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="12" fillId="5" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="13" fillId="6" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="14" fillId="7" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="13" fillId="8" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="15" fillId="9" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="0" fontId="16" fillId="10" borderId="2" xfId="0" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="5" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="6" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="7" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="8" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="9" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="0" fillId="10" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="17" fillId="5" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="left" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="18" fillId="10" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
    <xf numFmtId="49" fontId="19" fillId="10" borderId="2" xfId="0" applyNumberFormat="1" applyFont="1" applyFill="1" applyBorder="1" applyAlignment="1"><alignment horizontal="right" vertical="center" wrapText="1"/></xf>
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
