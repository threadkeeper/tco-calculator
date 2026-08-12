import { describe, expect, it } from 'vitest';
import { buildCalculationResultRows } from './calculation-results';
import type { ResourceDraft } from './draft';

describe('calculation result rows', () => {
  it('preserves server decimals and resolved source inputs for every workbook group', () => {
    const resource: ResourceDraft = {
      id: '11111111-1111-4111-8111-111111111111',
      source_type: 'ec2',
      workload_name: 'SQL, finance',
      quantity: 2,
      sql_edition: 'enterprise',
      license_basis: 'byol',
      sql_data_gb_per_instance: '2048.50',
      source_ram_gb_per_instance: '256',
      annual_hours_per_instance: '8760',
      mi_purchase_option: 'ahb',
      instance_type: 'r6id.8xlarge',
      volumes: []
    };
    const calculation = {
      resource_results: [
        {
          resource_id: resource.id,
          mapping_status: 'mapped',
          aws_pricing_status: 'fresh',
          azure_pricing_status: 'cached',
          target_selection: {
            selected: {
              selected_memory_gb: '256',
              service_tier: 'next_generation_general_purpose',
              hardware_family: 'premium-series',
              storage_architecture: 'next_generation_general_purpose',
              vcores: 32
            }
          },
          source_costs: {
            compute_gross: '100.01',
            compute_net: '90.01',
            license_gross: '200.02',
            license_net: '180.02',
            storage_gross: '300.03',
            storage_net: '270.03',
            hardware_annual: '0',
            electricity_annual: '0',
            total: '540.060000000000000001'
          },
          azure_costs: {
            compute_gross: '400.04',
            additional_ram_gb: '12.5',
            additional_ram_gross: '50.05',
            compute_plus_ram_net: '405.081',
            license_gross: '500.05',
            license_net: '450.05',
            storage_gross: '600.06',
            storage_net: '540.06',
            total_before_parity: '1395.191'
          },
          savings: {
            compute_savings: '-315.071',
            license_savings: '-270.03',
            storage_savings: '-270.03',
            total_savings: '-855.131',
            required_adjustment: '0.612902455723459',
            selected_adjustment: '0.1',
            azure_after_selected_parity: '1255.6719',
            difference: '715.611899999999999999'
          },
          explanation_steps: [
            { code: 'source_inputs', values: { source_vcpu: '32', source_max_iops: '80000' } }
          ]
        }
      ]
    };

    expect(buildCalculationResultRows(calculation, [resource])).toEqual([
      {
        resourceId: resource.id,
        workloadName: 'SQL, finance',
        sourceType: 'ec2',
        sourceSku: 'r6id.8xlarge',
        quantity: 2,
        sqlEdition: 'enterprise',
        licenseBasis: 'byol',
        sqlDataGbPerInstance: '2048.50',
        sourceRamGbPerInstance: '256',
        annualHoursPerInstance: '8760',
        miPurchaseOption: 'ahb',
        mappingStatus: 'mapped',
        awsPricingStatus: 'fresh',
        azurePricingStatus: 'cached',
        sourceVcpu: '32',
        selectedMemoryGb: '256',
        serviceTier: 'next_generation_general_purpose',
        hardwareFamily: 'premium-series',
        storageArchitecture: 'next_generation_general_purpose',
        vcores: 32,
        sourceComputeGross: '100.01',
        sourceComputeNet: '90.01',
        sourceLicenseGross: '200.02',
        sourceLicenseNet: '180.02',
        sourceStorageGross: '300.03',
        sourceStorageNet: '270.03',
        sourceHardwareAnnual: '0',
        sourceElectricityAnnual: '0',
        sourceTotal: '540.060000000000000001',
        azureComputeGross: '400.04',
        azureAdditionalRamGb: '12.5',
        azureAdditionalRamGross: '50.05',
        azureComputePlusRamNet: '405.081',
        azureLicenseGross: '500.05',
        azureLicenseNet: '450.05',
        azureStorageGross: '600.06',
        azureStorageNet: '540.06',
        azureTotalBeforeParity: '1395.191',
        computeSavings: '-315.071',
        licenseSavings: '-270.03',
        storageSavings: '-270.03',
        totalSavings: '-855.131',
        requiredAdjustment: '0.612902455723459',
        selectedAdjustment: '0.1',
        azureAfterSelectedParity: '1255.6719',
        difference: '715.611899999999999999'
      }
    ]);
  });

  it('keeps unavailable calculation groups distinct from zero', () => {
    const calculation = {
      resource_results: [
        {
          resource_id: 'missing-resource',
          mapping_status: 'no_mapping',
          aws_pricing_status: 'unavailable',
          azure_pricing_status: 'unavailable'
        }
      ]
    };

    const [row] = buildCalculationResultRows(calculation, []);

    expect(row.workloadName).toBe('Workload');
    expect(row.sourceTotal).toBeNull();
    expect(row.azureTotalBeforeParity).toBeNull();
    expect(row.difference).toBeNull();
  });
});
