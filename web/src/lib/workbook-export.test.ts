import { afterEach, describe, expect, it, vi } from 'vitest';
import { createProjectDraft } from './draft';
import {
  createProjectExportCsv,
  downloadProjectExport,
  projectExportFileName
} from './workbook-export';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('project result export', () => {
  it('creates an Excel-compatible UTF-8 CSV with exact server decimal text', () => {
    const project = createProjectDraft('ec2', '=Finance, estate', null);
    project.description = '+Confidential model';
    project.settings.source_compute_discount = '0.123456789';
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

    const csv = createProjectExportCsv(project, calculation);

    expect(csv.startsWith('\uFEFF"Project | Name"')).toBe(true);
    expect(csv).toContain('"Project | Description"');
    expect(csv).toContain('"Settings | Source compute discount"');
    expect(csv).toContain('"Calculation | Formula version"');
    expect(csv).toContain('"Workload | Server name"');
    expect(csv).toContain('"Source details | EC2 EBS volumes"');
    expect(csv).not.toContain('"Derived MI | Source vCPU"');
    expect(csv).toContain('"Derived MI | vCores"');
    expect(csv).toContain('"Source cost | Compute gross"');
    expect(csv).toContain('"Savings | Total before parity"');
    expect(csv).toContain('"Parity | Difference (Azure - source)"');
    expect(csv).toContain('"\'=Finance, estate"');
    expect(csv).toContain('"\'+Confidential model"');
    expect(csv).toContain('"\'@Quarterly ""SQL"""');
    expect(csv).toContain('"sql-prod-01"');
    expect(csv).toContain('"0.123456789"');
    expect(csv).toContain('"1.0.0"');
    expect(csv).toContain('"aws-aabbcc"');
    expect(csv).toContain('"azure-ddeeff"');
    expect(csv).toContain('"[{""id"":""22222222-2222-4222-8222-222222222222""');
    expect(csv).toContain('""capacity_gb"":""2048.125""');
    expect(csv).toContain('"23546.880000000000000001"');
    expect(csv).toContain('"-30740.4249600000"');
    expect(csv).toContain('"0.5662543937786223823625964725"');
    expect(csv).toContain('"\'=1+1"');
    expect(csv.endsWith('\r\n')).toBe(true);
  });

  it('creates a stable safe filename', () => {
    expect(projectExportFileName(' Finance / SQL ')).toBe('finance-sql-results.csv');
    expect(projectExportFileName('日本語')).toBe('tco-project-results.csv');
  });

  it('clicks an attached download link before releasing its Blob URL', () => {
    vi.useFakeTimers();
    const project = createProjectDraft('ec2', 'Finance SQL', null);
    const click = vi.fn();
    const remove = vi.fn();
    const link = { href: '', download: '', click, remove };
    const append = vi.fn();
    const createObjectURL = vi.fn(() => 'blob:project-results');
    const revokeObjectURL = vi.fn();
    vi.stubGlobal('document', {
      createElement: vi.fn(() => link),
      body: { append }
    });
    vi.stubGlobal('URL', { createObjectURL, revokeObjectURL });

    downloadProjectExport(project, { resource_results: [] });

    expect(createObjectURL).toHaveBeenCalledOnce();
    expect(append).toHaveBeenCalledWith(link);
    expect(link.href).toBe('blob:project-results');
    expect(link.download).toBe('finance-sql-results.csv');
    expect(click).toHaveBeenCalledOnce();
    expect(remove).toHaveBeenCalledOnce();
    expect(revokeObjectURL).not.toHaveBeenCalled();
    vi.runAllTimers();
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:project-results');
  });
});
