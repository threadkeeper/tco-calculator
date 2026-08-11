import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { createResource, type Ec2ResourceDraft } from '$lib/draft';
import CalculationResults from './CalculationResults.svelte';
import calculationResultsSource from './CalculationResults.svelte?raw';
import ResourceEditor from './ResourceEditor.svelte';
import resourceEditorSource from './ResourceEditor.svelte?raw';

function ec2Resource(): Ec2ResourceDraft {
  const resource = createResource('ec2');
  if (resource.source_type !== 'ec2') throw new Error('Expected an EC2 resource.');
  return resource;
}

describe('inline error messages', () => {
  it('identifies missing required fields and unavailable EBS pricing beside the inputs', () => {
    const resource = ec2Resource();
    resource.workload_name = '';
    resource.sql_data_gb_per_instance = '';
    resource.volumes = [
      {
        id: crypto.randomUUID(),
        label: '',
        aws_volume_id: null,
        volume_type: 'gp3',
        capacity_gb: '',
        provisioned_iops: null,
        throughput_mibps: ''
      }
    ];

    const { body } = render(ResourceEditor, {
      props: {
        resource,
        ebsTypes: [
          {
            key: 'ephemeral',
            label: 'Instance storage',
            price_required: false,
            pricing_available: true
          },
          {
            key: 'gp3',
            label: 'gp3',
            price_required: true,
            pricing_available: false
          }
        ],
        onremove: () => undefined,
        onchange: () => undefined
      }
    });

    expect(body).toContain('Workload name is required.');
    expect(body).toContain('SQL data is required.');
    expect(body).toContain('Volume label is required.');
    expect(body).toContain('Capacity is required for gp3 volumes.');
    expect(body).toContain('Provisioned IOPS are required for gp3 volumes.');
    expect(body).toContain(
      'gp3 pricing is unavailable in the selected AWS region. This workload cannot be included in the comparison.'
    );
    expect(body).toContain('class="field-error');
    expect(resourceEditorSource).toContain('color: #b42318;');
  });

  it('marks unavailable portfolio and workload price indicators instead of presenting zero', () => {
    const resource = ec2Resource();
    const calculation = {
      formula_version: '1.0.0',
      aws_snapshot_id: 'aws-snapshot',
      azure_snapshot_id: 'azure-snapshot',
      portfolio_totals: {
        aws_all_rows_total: null,
        aws_mapped_rows_total: '0',
        azure_mapped_rows_total: '0',
        required_portfolio_adjustment: '0',
        selected_parity_adjustment: '0',
        portfolio_after_selected_parity: '0',
        portfolio_difference: '0',
        comparable_resource_count: 0,
        no_mapping_resource_count: 0,
        price_unavailable_resource_count: 1
      },
      resource_results: [
        {
          resource_id: resource.id,
          mapping_status: 'mapped',
          aws_pricing_status: 'unavailable',
          azure_pricing_status: 'unavailable',
          target_selection: null,
          source_costs: null,
          azure_costs: null,
          savings: null,
          explanation_steps: [],
          unresolved_components: [
            {
              provider: 'aws',
              code: 'incomplete_rate_set',
              message: 'A required EBS rate is unavailable for gp3.'
            }
          ]
        }
      ],
      warnings: []
    };

    const { body } = render(CalculationResults, {
      props: { calculation, resources: [resource] }
    });

    expect(body).toContain('One or more source prices are unavailable.');
    expect(body).toContain('No workloads have complete source and Azure pricing.');
    expect(body).toContain('Source price is unavailable.');
    expect(body).toContain('Azure price is unavailable.');
    expect(body).toContain('Savings require complete source and Azure prices.');
    expect(body).toMatch(/class="price-status [^"]*unavailable/);
    expect(body.match(/PRICE UNAVAILABLE/g)).toHaveLength(6);
    expect(calculationResultsSource).toContain('color: #b42318;');
  });
});
