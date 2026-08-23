import { afterEach, describe, expect, it, vi } from 'vitest';
import { createProjectDraft } from './draft';
import {
  createProjectExportXlsx,
  downloadProjectExport,
  projectExportFileName
} from './workbook-export';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('project result export', () => {
  it('creates a formatted XLSX workbook with exact server decimal text', () => {
    const project = createProjectDraft('ec2', '=Finance, estate', null);
    project.description = '+Confidential model';
    project.settings.source_compute_discount = '0.123456789';
    project.aws_price_snapshot_id = 'aws-input-snapshot';
    project.azure_price_snapshot_id = 'azure-input-snapshot';
    project.resources = [
      {
        id: '11111111-1111-4111-8111-111111111111',
        source_type: 'ec2',
        workload_name: '@Quarterly "SQL"',
        server_name: 'sql-prod-01',
        quantity: 1,
        sql_edition: 'enterprise',
        license_basis: 'byol',
        sql_data_gb_per_instance: '1024',
        source_ram_gb_per_instance: '256',
        annual_hours_per_instance: '8760',
        mi_purchase_option: 'ahb',
        instance_type: 'r6id.8xlarge',
        volumes: [
          {
            id: '22222222-2222-4222-8222-222222222222',
            label: '=Primary data',
            aws_volume_id: null,
            volume_type: 'gp3',
            capacity_gb: '2048.125',
            provisioned_iops: 3000,
            throughput_mibps: '125'
          }
        ]
      }
    ];
    const calculation = {
      formula_version: '1.0.0',
      aws_snapshot_id: 'aws-aabbcc',
      azure_snapshot_id: 'azure-ddeeff',
      resource_results: [
        {
          resource_id: project.resources[0].id,
          storage_inputs: {
            sql_data_gb_per_instance: '1024',
            persistent_ebs_gb_per_instance: '2048.125',
            azure_storage_gb_per_instance: '3072.125'
          },
          mapping_status: 'mapped',
          aws_pricing_status: 'fresh',
          azure_pricing_status: 'fresh',
          source_costs: {
            compute_gross: '1',
            compute_net: '1',
            license_gross: '2',
            license_net: '2',
            storage_gross: '3',
            storage_net: '3',
            hardware_annual: '0',
            electricity_annual: '0',
            total: '23546.880000000000000001'
          },
          azure_costs: {
            compute_gross: '4',
            additional_ram_gb: '0',
            additional_ram_gross: '0',
            compute_plus_ram_net: '4',
            license_gross: '5',
            license_net: '5',
            storage_gross: '6',
            storage_net: '6',
            total_before_parity: '=1+1'
          },
          savings: {
            compute_savings: '-3',
            license_savings: '-3',
            storage_savings: '-3',
            total_savings: '-30740.4249600000',
            required_adjustment: '0.5662543937786223823625964725',
            selected_adjustment: '0',
            azure_after_selected_parity: '54287.304960',
            difference: '30740.4249600000'
          },
          explanation_steps: [{ code: 'source_inputs', values: { source_vcpu: '32' } }]
        }
      ]
    };

    const workbook = createProjectExportXlsx(project, calculation);
    const entries = readStoredZipEntries(workbook);
    const worksheet = entries.get('xl/worksheets/sheet1.xml') ?? '';
    const inputsWorksheet = entries.get('xl/worksheets/sheet2.xml') ?? '';
    const workbookDefinition = entries.get('xl/workbook.xml') ?? '';
    const styles = entries.get('xl/styles.xml') ?? '';

    expect(Array.from(workbook.slice(0, 4))).toEqual([0x50, 0x4b, 0x03, 0x04]);
    expect(entries.has('[Content_Types].xml')).toBe(true);
    expect(entries.has('_rels/.rels')).toBe(true);
    expect(entries.has('docProps/core.xml')).toBe(true);
    expect(entries.has('xl/workbook.xml')).toBe(true);
    expect(entries.has('xl/_rels/workbook.xml.rels')).toBe(true);
    expect(entries.has('xl/worksheets/sheet2.xml')).toBe(true);
    expect(workbookDefinition).toContain('<sheet name="TCO Results" sheetId="1" r:id="rId1"/>');
    expect(workbookDefinition).toContain('<sheet name="Inputs" sheetId="2" r:id="rId2"/>');
    expect(worksheet).toContain('<pane xSplit="3" ySplit="1"');
    expect(worksheet).toMatch(/<autoFilter ref="A1:[A-Z]+2"\/>/);
    expect(worksheet).toContain('Project | Description');
    expect(worksheet).toContain('Settings | Source compute discount');
    expect(worksheet).toContain('Calculation | Formula version');
    expect(worksheet).toContain('Workload | Server name');
    expect(worksheet).toContain('Source details | EC2 EBS volumes');
    expect(worksheet).toContain('Source details | Persistent EBS GB / instance');
    expect(worksheet).not.toContain('Derived MI | Source vCPU');
    expect(worksheet).toContain('Derived MI | Storage GB / instance');
    expect(worksheet).toContain('Derived MI | vCores');
    expect(worksheet).toContain('Source cost | Compute gross');
    expect(worksheet).toContain('Savings | Total before parity');
    expect(worksheet).toContain('Parity | Difference (Azure - source)');
    expect(worksheet).toContain('=Finance, estate');
    expect(worksheet).toContain('+Confidential model');
    expect(worksheet).toContain('@Quarterly "SQL"');
    expect(worksheet).toContain('sql-prod-01');
    expect(worksheet).toContain('0.123456789');
    expect(worksheet).toContain('1.0.0');
    expect(worksheet).toContain('aws-aabbcc');
    expect(worksheet).toContain('azure-ddeeff');
    expect(worksheet).toContain('22222222-2222-4222-8222-222222222222');
    expect(worksheet).toContain('2048.125');
    expect(worksheet).toContain('3072.125');
    expect(worksheet).toContain('23546.880000000000000001');
    expect(worksheet).toContain('-30740.4249600000');
    expect(worksheet).toContain('0.5662543937786223823625964725');
    expect(worksheet).toContain('=1+1');
    expect(worksheet).toContain('t="inlineStr"');
    expect(worksheet).not.toContain('<f>');
    expect(styles).toContain('4472C4');
    expect(styles).toContain('70AD47');
    expect(styles).toContain('7030A0');
    expect(inputsWorksheet).toContain('<pane xSplit="2" ySplit="1"');
    expect(inputsWorksheet).toMatch(/<autoFilter ref="A1:D\d+"\/>/);
    expect(inputsWorksheet).toContain('Section');
    expect(inputsWorksheet).toContain('Workload');
    expect(inputsWorksheet).toContain('Input');
    expect(inputsWorksheet).toContain('Value');
    expect(inputsWorksheet).toContain('Project settings');
    expect(inputsWorksheet).toContain('settings.project_type');
    expect(inputsWorksheet).toContain('settings.source_compute_discount');
    expect(inputsWorksheet).toContain('0.123456789');
    expect(inputsWorksheet).toContain('aws_price_snapshot_id');
    expect(inputsWorksheet).toContain('aws-input-snapshot');
    expect(inputsWorksheet).toContain('azure_price_snapshot_id');
    expect(inputsWorksheet).toContain('azure-input-snapshot');
    expect(inputsWorksheet).toContain('Resource 1');
    expect(inputsWorksheet).toContain('resources[1].instance_type');
    expect(inputsWorksheet).toContain('r6id.8xlarge');
    expect(inputsWorksheet).toContain('resources[1].volumes[1].capacity_gb');
    expect(inputsWorksheet).toContain('2048.125');
    expect(inputsWorksheet).toContain('resources[1].volumes[1].provisioned_iops');
    expect(inputsWorksheet).toContain('3000');
    expect(inputsWorksheet).toContain('=Primary data');
    expect(inputsWorksheet).toContain('t="inlineStr"');
    expect(inputsWorksheet).not.toContain('<f>');
  });

  it('creates a stable safe filename', () => {
    expect(projectExportFileName(' Finance / SQL ')).toBe('finance-sql-results.xlsx');
    expect(projectExportFileName('日本語')).toBe('tco-project-results.xlsx');
  });

  it('clicks an attached download link before releasing its Blob URL', () => {
    vi.useFakeTimers();
    const project = createProjectDraft('ec2', 'Finance SQL', null);
    const click = vi.fn();
    const remove = vi.fn();
    const link = { href: '', download: '', click, remove };
    const append = vi.fn();
    const createObjectURL = vi.fn((blob: Blob) => {
      void blob;
      return 'blob:project-results';
    });
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('document', {
      createElement: vi.fn(() => link),
      body: { append }
    });
    vi.stubGlobal('URL', { createObjectURL, revokeObjectURL });

    downloadProjectExport(project, { resource_results: [] });

    expect(createObjectURL).toHaveBeenCalledOnce();
    const blob = createObjectURL.mock.calls[0]?.[0];
    expect(blob?.type).toBe('application/vnd.openxmlformats-officedocument.spreadsheetml.sheet');
    expect(append).toHaveBeenCalledWith(link);
    expect(link.href).toBe('blob:project-results');
    expect(link.download).toBe('finance-sql-results.xlsx');
    expect(click).toHaveBeenCalledOnce();
    expect(remove).toHaveBeenCalledOnce();
    expect(revokeObjectURL).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:project-results');
  });
});

function readStoredZipEntries(bytes: Uint8Array): Map<string, string> {
  const entries = new Map<string, string>();
  const decoder = new TextDecoder();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let offset = 0;

  while (offset + 30 <= bytes.length && view.getUint32(offset, true) === 0x04034b50) {
    const method = view.getUint16(offset + 8, true);
    const size = view.getUint32(offset + 18, true);
    const nameLength = view.getUint16(offset + 26, true);
    const extraLength = view.getUint16(offset + 28, true);
    const nameStart = offset + 30;
    const dataStart = nameStart + nameLength + extraLength;
    const name = decoder.decode(bytes.subarray(nameStart, nameStart + nameLength));

    expect(method).toBe(0);
    entries.set(name, decoder.decode(bytes.subarray(dataStart, dataStart + size)));
    offset = dataStart + size;
  }

  return entries;
}
