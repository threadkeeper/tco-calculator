import { asRecord, readNumber, readRecord, readRecords, readString, type JsonRecord } from './api';
import type { ResourceDraft } from './draft';

export type CalculationResultRow = {
  resourceId: string;
  workloadName: string;
  serverName: string | null;
  sourceType: ResourceDraft['source_type'] | null;
  sourceSku: string | null;
  quantity: number | null;
  sqlEdition: string | null;
  licenseBasis: string | null;
  sqlDataGbPerInstance: string | null;
  persistentEbsGbPerInstance: string | null;
  azureStorageGbPerInstance: string | null;
  sourceRamGbPerInstance: string | null;
  annualHoursPerInstance: string | null;
  miPurchaseOption: string | null;
  burstPolicy: string | null;
  instanceStoreUse: string | null;
  requiredLocalTempDiskGb: string | null;
  ephemeralDataLossAcceptable: boolean | null;
  highFrequencyRequirement: string | null;
  requestedTargetArmSku: string | null;
  mappingStatus: string | null;
  recommendationStatus: string | null;
  awsPricingStatus: string | null;
  azurePricingStatus: string | null;
  sourceVcpu: string | null;
  selectedMemoryGb: string | null;
  serviceTier: string | null;
  hardwareFamily: string | null;
  storageArchitecture: string | null;
  vcores: number | null;
  sourceComputeGross: string | null;
  sourceComputeNet: string | null;
  sourceLicenseGross: string | null;
  sourceLicenseNet: string | null;
  sourceStorageGross: string | null;
  sourceStorageNet: string | null;
  sourceHardwareAnnual: string | null;
  sourceElectricityAnnual: string | null;
  sourceTotal: string | null;
  azureComputeGross: string | null;
  azureAdditionalRamGb: string | null;
  azureAdditionalRamGross: string | null;
  azureComputePlusRamNet: string | null;
  azureLicenseGross: string | null;
  azureLicenseNet: string | null;
  azureStorageGross: string | null;
  azureStorageNet: string | null;
  azureTotalBeforeParity: string | null;
  computeSavings: string | null;
  licenseSavings: string | null;
  storageSavings: string | null;
  totalSavings: string | null;
  requiredAdjustment: string | null;
  selectedAdjustment: string | null;
  azureAfterSelectedParity: string | null;
  difference: string | null;
};

export function buildCalculationResultRows(
  calculation: unknown,
  resources: ResourceDraft[]
): CalculationResultRow[] {
  const revision = asRecord(calculation);
  return readRecords(revision, 'resource_results').map((result) => {
    const resourceId = readString(result, 'resource_id') ?? '';
    const resource = resources.find((item) => item.id === resourceId);
    const targetSelection =
      readRecord(result, 'vm_target_selection') ?? readRecord(result, 'target_selection');
    const selected = readRecord(targetSelection, 'selected');
    const sourceCosts = readRecord(result, 'source_costs');
    const azureCosts = readRecord(result, 'azure_costs');
    const savings = readRecord(result, 'savings');
    const storageInputs = readRecord(result, 'storage_inputs');
    const sourceInputs = explanationValues(result, 'source_inputs');

    return {
      resourceId,
      workloadName: resource?.workload_name ?? 'Workload',
      serverName: resource?.server_name ?? null,
      sourceType: resource?.source_type ?? null,
      sourceSku: resource ? resourceSku(resource) : null,
      quantity: resource?.quantity ?? null,
      sqlEdition: resource && resource.source_type !== 'ec2_vm' ? resource.sql_edition : null,
      licenseBasis: resource && resource.source_type !== 'ec2_vm' ? resource.license_basis : null,
      sqlDataGbPerInstance:
        readString(storageInputs, 'sql_data_gb_per_instance') ??
        (resource && resource.source_type !== 'ec2_vm'
          ? resource.sql_data_gb_per_instance
          : null) ??
        null,
      persistentEbsGbPerInstance: readString(storageInputs, 'persistent_ebs_gb_per_instance'),
      azureStorageGbPerInstance: readString(storageInputs, 'azure_storage_gb_per_instance'),
      sourceRamGbPerInstance: resource?.source_ram_gb_per_instance ?? null,
      annualHoursPerInstance: resource?.annual_hours_per_instance ?? null,
      miPurchaseOption:
        resource && resource.source_type !== 'ec2_vm' ? resource.mi_purchase_option : null,
      burstPolicy: resource?.source_type === 'ec2_vm' ? resource.requirements.burst_policy : null,
      instanceStoreUse:
        resource?.source_type === 'ec2_vm' ? resource.requirements.instance_store_use : null,
      requiredLocalTempDiskGb:
        resource?.source_type === 'ec2_vm'
          ? resource.requirements.required_local_temp_disk_gb
          : null,
      ephemeralDataLossAcceptable:
        resource?.source_type === 'ec2_vm'
          ? resource.requirements.ephemeral_data_loss_acceptable
          : null,
      highFrequencyRequirement:
        resource?.source_type === 'ec2_vm'
          ? resource.requirements.high_frequency_requirement
          : null,
      requestedTargetArmSku:
        resource?.source_type === 'ec2_vm' ? resource.requirements.requested_target_arm_sku : null,
      mappingStatus: readString(result, 'mapping_status'),
      recommendationStatus: readString(targetSelection, 'recommendation_status'),
      awsPricingStatus: readString(result, 'aws_pricing_status'),
      azurePricingStatus: readString(result, 'azure_pricing_status'),
      sourceVcpu:
        readString(sourceInputs, 'source_vcpu') ??
        (resource?.source_type === 'on_prem' ? String(resource.source_vcpu) : null),
      selectedMemoryGb:
        readString(selected, 'selected_memory_gb') ?? readString(selected, 'memory_gb'),
      serviceTier: readString(selected, 'service_tier') ?? readString(selected, 'arm_sku_name'),
      hardwareFamily:
        readString(selected, 'hardware_family') ?? readString(selected, 'display_family'),
      storageArchitecture:
        readString(selected, 'storage_architecture') ?? selectedDiskSummary(selected),
      vcores: readNumber(selected, 'vcores') ?? readNumber(selected, 'vcpus'),
      sourceComputeGross: readString(sourceCosts, 'compute_gross'),
      sourceComputeNet: readString(sourceCosts, 'compute_net'),
      sourceLicenseGross: readString(sourceCosts, 'license_gross'),
      sourceLicenseNet: readString(sourceCosts, 'license_net'),
      sourceStorageGross: readString(sourceCosts, 'storage_gross'),
      sourceStorageNet: readString(sourceCosts, 'storage_net'),
      sourceHardwareAnnual: readString(sourceCosts, 'hardware_annual'),
      sourceElectricityAnnual: readString(sourceCosts, 'electricity_annual'),
      sourceTotal: readString(sourceCosts, 'total'),
      azureComputeGross: readString(azureCosts, 'compute_gross'),
      azureAdditionalRamGb: readString(azureCosts, 'additional_ram_gb'),
      azureAdditionalRamGross: readString(azureCosts, 'additional_ram_gross'),
      azureComputePlusRamNet: readString(azureCosts, 'compute_plus_ram_net'),
      azureLicenseGross: readString(azureCosts, 'license_gross'),
      azureLicenseNet: readString(azureCosts, 'license_net'),
      azureStorageGross: readString(azureCosts, 'storage_gross'),
      azureStorageNet: readString(azureCosts, 'storage_net'),
      azureTotalBeforeParity: readString(azureCosts, 'total_before_parity'),
      computeSavings: readString(savings, 'compute_savings'),
      licenseSavings: readString(savings, 'license_savings'),
      storageSavings: readString(savings, 'storage_savings'),
      totalSavings: readString(savings, 'total_savings'),
      requiredAdjustment: readString(savings, 'required_adjustment'),
      selectedAdjustment: readString(savings, 'selected_adjustment'),
      azureAfterSelectedParity: readString(savings, 'azure_after_selected_parity'),
      difference: readString(savings, 'difference')
    };
  });
}

function explanationValues(result: JsonRecord, code: string): JsonRecord | null {
  const step = readRecords(result, 'explanation_steps').find(
    (item) => readString(item, 'code') === code
  );
  return readRecord(step ?? null, 'values');
}

function resourceSku(resource: ResourceDraft): string {
  switch (resource.source_type) {
    case 'ec2':
    case 'ec2_vm':
    case 'rds':
      return resource.instance_type;
    case 'on_prem':
      return `${resource.source_vcpu} vCPU`;
  }
}

function selectedDiskSummary(selected: JsonRecord | null): string | null {
  const disks = readRecords(selected, 'disks');
  if (disks.length === 0) return null;
  return disks
    .map((disk) => readString(disk, 'tier_key') ?? readString(disk, 'display_name'))
    .filter((value): value is string => value !== null)
    .join(', ');
}
