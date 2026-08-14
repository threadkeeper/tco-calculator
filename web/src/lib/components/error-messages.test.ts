import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import { createResource, type Ec2ResourceDraft } from '$lib/draft';
import CalculationResults from './CalculationResults.svelte';
import calculationResultsSource from './CalculationResults.svelte?raw';
import ResourceEditor from './ResourceEditor.svelte';
import resourceEditorSource from './ResourceEditor.svelte?raw';

function ec2Resource(): Ec2ResourceDraft {
  const resource = createResource('ec2', {
    default_annual_hours: '8760',
    default_mi_purchase_option: 'ahb'
  });
  if (resource.source_type !== 'ec2') throw new Error('Expected an EC2 resource.');
  return resource;
}

describe('inline error messages', () => {
  it('groups workload inputs with an optional server name and source input tint', () => {
    const resource = ec2Resource();
    resource.server_name = 'sql-prod-01';

    const { body } = render(ResourceEditor, {
      props: {
        resource,
        purchaseOptionDiscounts: {
          payg: '0',
          one_year_reserved: '0.25',
          three_year_reserved: '0.375',
          one_year_savings_plan: '0.125',
          azure_hybrid_benefit: '1'
        },
        onremove: () => undefined,
        onchange: () => undefined
      }
    });

    expect(resourceEditorSource).toContain('<details class="resource-editor" open>');
    expect(body).toContain('Server name');
    expect(body).toContain('sql-prod-01');
    expect(body).toContain('Azure Hybrid Benefit · 100% discount');
    expect(resourceEditorSource).toContain('color-mix(in srgb, #e98b22 11%');
  });

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
    expect(resourceEditorSource).toContain('color: var(--danger);');
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
    expect(calculationResultsSource).toContain('color: var(--danger);');
  });

  it('shows applied purchase discounts in the blue-tinted calculated output', () => {
    const resource = ec2Resource();
    resource.mi_purchase_option = 'ahbone-year';
    const calculation = {
      portfolio_totals: {
        aws_all_rows_total: '100',
        portfolio_after_selected_parity: '75',
        portfolio_difference: '25',
        comparable_resource_count: 1,
        no_mapping_resource_count: 0,
        price_unavailable_resource_count: 0
      },
      resource_results: [
        {
          resource_id: resource.id,
          mapping_status: 'mapped',
          aws_pricing_status: 'fresh',
          azure_pricing_status: 'fresh',
          purchase_option_discounts: {
            payg: '0',
            one_year_reserved: '0.25',
            three_year_reserved: '0.375',
            one_year_savings_plan: '0.125',
            azure_hybrid_benefit: '1'
          },
          source_costs: { total: '100' },
          azure_costs: { total_before_parity: '75' },
          savings: { total_savings: '25' },
          target_selection: null,
          unresolved_components: [],
          explanation_steps: []
        }
      ],
      warnings: []
    };

    const { body } = render(CalculationResults, {
      props: { calculation, resources: [resource] }
    });

    expect(body).toContain('25% compute discount · 100% AHB license discount');
    expect(calculationResultsSource).toContain('color-mix(in srgb, var(--azure) 9%');
  });
});
