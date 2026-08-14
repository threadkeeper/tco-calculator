import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  applyOnPremPublicBookReference,
  createGuestWorkspace,
  createProjectDraft,
  createResource,
  editableProject,
  projectRequestPayload
} from './draft';
import projectWorkspaceSource from './components/ProjectWorkspace.svelte?raw';

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-08-10T12:00:00.000Z'));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('project drafts', () => {
  it('keeps sourced on-premises License + SA inputs visible in project settings', () => {
    expect(projectWorkspaceSource).toContain(
      "untrack(() => workspace.project.settings.project_type === 'on_prem')"
    );
    expect(projectWorkspaceSource).toContain(
      '<details class="settings-panel" class:hidden={isSqlPayg} bind:open={settingsOpen}>'
    );
    expect(projectWorkspaceSource).toContain('On-premises SQL licensing');
    expect(projectWorkspaceSource).toContain('ON_PREM_PUBLIC_BOOK_REFERENCE');
    expect(projectWorkspaceSource).toContain('Use public reference');
    expect(projectWorkspaceSource).toContain('Replace with the applicable EA or customer quote.');
    expect(projectWorkspaceSource).toContain('Enterprise License + SA quote (USD / 2-core pack)');
    expect(projectWorkspaceSource).toContain('Standard License + SA quote (USD / 2-core pack)');
    expect(
      projectWorkspaceSource.match(/project_type !== 'on_prem'/g)?.length
    ).toBeGreaterThanOrEqual(3);
  });

  it('creates a cloud project with explicit regional scope', () => {
    const project = createProjectDraft(
      'ec2',
      'Production migration',
      'Synthetic test estate',
      'af-south-1',
      'southafricanorth'
    );

    expect(project).toMatchObject({
      name: 'Production migration',
      description: 'Synthetic test estate',
      settings: {
        project_type: 'ec2',
        aws_region: 'af-south-1',
        azure_region: 'southafricanorth',
        currency: 'USD',
        default_annual_hours: '8760',
        default_mi_purchase_option: 'ahb',
        enterprise_license_sa_usd_per_two_core_pack: null
      },
      resources: []
    });
  });

  it('leaves customer-supplied on-premises licensing inputs blank', () => {
    const project = createProjectDraft('on_prem', 'Datacenter', null);

    expect(project.settings).toMatchObject({
      project_type: 'on_prem',
      aws_region: null,
      enterprise_license_sa_usd_per_two_core_pack: null,
      standard_license_sa_usd_per_two_core_pack: null,
      remaining_coverage_months: 36,
      electricity_rate_usd_per_kwh: '0'
    });
  });

  it('creates a licensing-only SQL PAYG project with exactly three baseline inputs', () => {
    const project = createProjectDraft('sql_payg', 'PAYG comparison', null);

    expect(project).toMatchObject({
      settings: {
        project_type: 'sql_payg',
        aws_region: null,
        azure_region: 'global',
        sql_payg: {
          enterprise_licensed_cores: 0,
          standard_licensed_cores: 0,
          software_assurance_annual_usd: '0'
        }
      },
      resources: [],
      aws_price_snapshot_id: null,
      azure_price_snapshot_id: null
    });
    expect(Object.keys(project.settings.sql_payg ?? {})).toEqual([
      'enterprise_licensed_cores',
      'standard_licensed_cores',
      'software_assurance_annual_usd'
    ]);
  });

  it('normalizes SQL PAYG browser values without adding workload resources', () => {
    const project = createProjectDraft('sql_payg', 'PAYG comparison', null);
    const settings = project.settings.sql_payg;
    if (!settings) throw new Error('SQL PAYG settings should exist.');
    Reflect.set(settings, 'enterprise_licensed_cores', '8');
    Reflect.set(settings, 'standard_licensed_cores', '16');
    Reflect.set(settings, 'software_assurance_annual_usd', 20000.25);

    const payload = projectRequestPayload(project);

    expect(payload.settings.sql_payg).toEqual({
      enterprise_licensed_cores: 8,
      standard_licensed_cores: 16,
      software_assurance_annual_usd: '20000.25'
    });
    expect(payload.resources).toEqual([]);
  });

  it('applies the sourced first-year public book reference only when requested', () => {
    const project = createProjectDraft('on_prem', 'Datacenter', null);

    applyOnPremPublicBookReference(project.settings);

    expect(project.settings).toMatchObject({
      enterprise_license_sa_usd_per_two_core_pack: '20557',
      standard_license_sa_usd_per_two_core_pack: '5363',
      remaining_coverage_months: 12
    });
  });

  it('wraps a project in a timestamped guest workspace', () => {
    const project = createProjectDraft('rds', 'RDS estate', null);

    expect(createGuestWorkspace(project)).toEqual({
      project,
      calculation: null,
      aws_resolution: null,
      azure_resolution: null,
      updated_at: '2026-08-10T12:00:00.000Z'
    });
  });

  it('accepts editable projects as detached copies and rejects malformed values', () => {
    const original = createProjectDraft('ec2', 'Editable', null);
    original.resources.push(createResource('ec2', original.settings));

    const editable = editableProject(original);

    expect(editable).toEqual(original);
    expect(editable).not.toBe(original);
    expect(editable?.settings).not.toBe(original.settings);
    expect(editable?.resources).not.toBe(original.resources);
    expect(editableProject({ name: 'Missing settings', resources: [] })).toBeNull();
    expect(
      editableProject({ name: 'Unknown type', settings: { project_type: 'vmware' }, resources: [] })
    ).toBeNull();
  });

  it('hydrates legacy resources without a server name as null', () => {
    const original = createProjectDraft('ec2', 'Legacy', null);
    const resource = createResource('ec2', original.settings);
    Reflect.deleteProperty(resource, 'server_name');
    original.resources.push(resource);

    expect(editableProject(original)?.resources[0].server_name).toBeNull();
  });

  it.each(['ec2', 'rds', 'on_prem'] as const)(
    'normalizes browser-coerced %s values to API contract types',
    (projectType) => {
      const project = createProjectDraft(projectType, 'Contract test', null);
      const resource = createResource(projectType, project.settings);
      project.resources.push(resource);
      const settingDecimals = {
        source_compute_discount: 0.1,
        source_license_discount: 0.2,
        source_storage_discount: 0.3,
        azure_compute_discount: 0.4,
        azure_license_discount: 0.5,
        azure_storage_discount: 0.6,
        selected_parity_adjustment: 0.7,
        default_annual_hours: 8000.5
      };
      const sharedDecimals = {
        sql_data_gb_per_instance: 2048.5,
        source_ram_gb_per_instance: 512.25,
        annual_hours_per_instance: 8000.5
      };
      for (const [field, value] of Object.entries(settingDecimals)) {
        Reflect.set(project.settings, field, value);
      }
      for (const [field, value] of Object.entries(sharedDecimals)) {
        Reflect.set(resource, field, value);
      }
      resource.server_name = '  sql-prod-01  ';
      Reflect.set(resource, 'quantity', '2');

      if (resource.source_type === 'ec2') {
        const volume = resource.volumes[0];
        volume.volume_type = 'gp3';
        Reflect.set(volume, 'capacity_gb', 2048.5);
        Reflect.set(volume, 'provisioned_iops', '6000');
        Reflect.set(volume, 'throughput_mibps', 250.25);
      } else if (resource.source_type === 'rds') {
        Reflect.set(resource, 'source_max_iops', '12000');
      } else {
        Reflect.set(project.settings, 'enterprise_license_sa_usd_per_two_core_pack', 7123.45);
        Reflect.set(project.settings, 'standard_license_sa_usd_per_two_core_pack', 2345.67);
        Reflect.set(project.settings, 'remaining_coverage_months', '24');
        Reflect.set(project.settings, 'electricity_rate_usd_per_kwh', 0.1234);
        Reflect.set(resource, 'source_vcpu', '64');
        Reflect.set(resource, 'licensable_cores', '48');
        Reflect.set(resource, 'source_max_iops', '24000');
        Reflect.set(resource, 'hardware_capex_usd', 125000.25);
        Reflect.set(resource, 'depreciation_years', 4.5);
        Reflect.set(resource, 'average_power_kw_override', 0.75);
      }

      const payload = projectRequestPayload(project);
      const payloadResource = payload.resources[0];

      for (const [field, value] of Object.entries(settingDecimals)) {
        expect(Reflect.get(payload.settings, field)).toBe(String(value));
      }
      for (const [field, value] of Object.entries(sharedDecimals)) {
        expect(Reflect.get(payloadResource, field)).toBe(String(value));
      }
      expect(payloadResource.server_name).toBe('sql-prod-01');
      expect(payloadResource.quantity).toBe(2);

      if (payloadResource.source_type === 'ec2') {
        expect(payloadResource.volumes[0]).toMatchObject({
          capacity_gb: '2048.5',
          provisioned_iops: 6000,
          throughput_mibps: '250.25'
        });
      } else if (payloadResource.source_type === 'rds') {
        expect(payloadResource.source_max_iops).toBe(12000);
      } else {
        expect(payload.settings).toMatchObject({
          enterprise_license_sa_usd_per_two_core_pack: '7123.45',
          standard_license_sa_usd_per_two_core_pack: '2345.67',
          remaining_coverage_months: 24,
          electricity_rate_usd_per_kwh: '0.1234'
        });
        expect(payloadResource).toMatchObject({
          source_vcpu: 64,
          licensable_cores: 48,
          source_max_iops: 24000,
          hardware_capex_usd: '125000.25',
          depreciation_years: '4.5',
          average_power_kw_override: '0.75'
        });
      }
    }
  );
});

describe('resource drafts', () => {
  it.each([
    ['ec2', 'r6id.8xlarge', '256'],
    ['rds', 'db.m6i.8xlarge', '128'],
    ['on_prem', null, '256']
  ] as const)(
    'creates %s resources with source-specific defaults',
    (projectType, instanceType, ram) => {
      const project = createProjectDraft(projectType, 'Resource defaults', null);
      const resource = createResource(projectType, project.settings);

      expect(resource.source_type).toBe(projectType);
      expect(resource.id).toMatch(/^[0-9a-f-]{36}$/);
      expect(resource.source_ram_gb_per_instance).toBe(ram);
      if (resource.source_type === 'ec2') {
        expect(resource.instance_type).toBe(instanceType);
        expect(resource.volumes).toHaveLength(1);
        expect(resource.volumes[0]).toMatchObject({ volume_type: 'ephemeral', capacity_gb: '0' });
      } else if (resource.source_type === 'rds') {
        expect(resource.instance_type).toBe(instanceType);
        expect(resource.deployment).toBe('single_az');
      } else {
        expect(instanceType).toBeNull();
        expect(resource).toMatchObject({
          source_vcpu: 32,
          licensable_cores: 32,
          depreciation_years: '5'
        });
      }
    }
  );

  it('copies the project pricing defaults into each new resource', () => {
    const project = createProjectDraft('ec2', 'Custom defaults', null);
    project.settings.default_annual_hours = '7300';
    project.settings.default_mi_purchase_option = 'ahbthree-year';

    const resource = createResource('ec2', project.settings);

    expect(resource).toMatchObject({
      annual_hours_per_instance: '7300',
      mi_purchase_option: 'ahbthree-year'
    });
  });
});
