import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createGuestWorkspace, createProjectDraft, createResource, editableProject } from './draft';

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date('2026-08-10T12:00:00.000Z'));
});

afterEach(() => {
  vi.useRealTimers();
});

describe('project drafts', () => {
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

  it('creates an on-premises project with source licensing inputs', () => {
    const project = createProjectDraft('on_prem', 'Datacenter', null);

    expect(project.settings).toMatchObject({
      project_type: 'on_prem',
      aws_region: null,
      enterprise_license_sa_usd_per_two_core_pack: '0',
      standard_license_sa_usd_per_two_core_pack: '0',
      remaining_coverage_months: 12,
      electricity_rate_usd_per_kwh: '0'
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
    original.resources.push(createResource('ec2'));

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
});

describe('resource drafts', () => {
  it.each([
    ['ec2', 'r6id.8xlarge', '256'],
    ['rds', 'db.m6i.8xlarge', '128'],
    ['on_prem', null, '256']
  ] as const)(
    'creates %s resources with source-specific defaults',
    (projectType, instanceType, ram) => {
      const resource = createResource(projectType);

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
});
