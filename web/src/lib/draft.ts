import type { components } from './api/generated';

export type ProjectType = 'ec2' | 'ec2_vm' | 'rds' | 'on_prem' | 'sql_payg';
export type SqlEdition = 'standard' | 'enterprise';
export type LicenseBasis = 'license_included' | 'byol';
export type PurchaseOption =
  | 'payg'
  | 'ahb'
  | 'one-year'
  | 'ahbone-year'
  | 'three-year'
  | 'ahbthree-year'
  | 'sv-one-year'
  | 'ahbsv-one-year';
export type VmPurchaseOption = components['schemas']['VmPurchaseOptionKey'];

const VM_PURCHASE_OPTIONS: ReadonlySet<string> = new Set([
  'payg',
  'ahb',
  'one-year',
  'ahbone-year',
  'three-year',
  'ahbthree-year',
  'sv-one-year',
  'ahbsv-one-year',
  'sv-three-year',
  'ahbsv-three-year'
]);

export function isVmPurchaseOption(value: unknown): value is VmPurchaseOption {
  return typeof value === 'string' && VM_PURCHASE_OPTIONS.has(value);
}

export type ProjectSettingsDraft = {
  project_type: ProjectType;
  aws_region: string | null;
  azure_region: string;
  currency: 'USD';
  source_compute_discount: string;
  source_license_discount: string;
  source_storage_discount: string;
  azure_compute_discount: string;
  azure_license_discount: string;
  azure_storage_discount: string;
  selected_parity_adjustment: string;
  default_annual_hours: string;
  default_mi_purchase_option: PurchaseOption;
  enterprise_license_sa_usd_per_two_core_pack: string | null;
  standard_license_sa_usd_per_two_core_pack: string | null;
  remaining_coverage_months: 12 | 24 | 36 | null;
  electricity_rate_usd_per_kwh: string | null;
  sql_payg: SqlPaygSettingsDraft | null;
};

export type SqlPaygSettingsDraft = {
  enterprise_licensed_cores: number;
  standard_licensed_cores: number;
  software_assurance_annual_usd: string;
};

export type ResourceDefaults = Pick<
  ProjectSettingsDraft,
  'default_annual_hours' | 'default_mi_purchase_option'
>;

export const ON_PREM_PUBLIC_BOOK_REFERENCE = {
  enterprise_license_sa_usd_per_two_core_pack: '20557',
  standard_license_sa_usd_per_two_core_pack: '5363',
  remaining_coverage_months: 12,
  source_url: 'https://www.microsoft.com/en-us/sql-server/sql-server-2022-pricing',
  verified_on: '2026-08-07'
} as const;

export function applyOnPremPublicBookReference(settings: ProjectSettingsDraft): void {
  settings.enterprise_license_sa_usd_per_two_core_pack =
    ON_PREM_PUBLIC_BOOK_REFERENCE.enterprise_license_sa_usd_per_two_core_pack;
  settings.standard_license_sa_usd_per_two_core_pack =
    ON_PREM_PUBLIC_BOOK_REFERENCE.standard_license_sa_usd_per_two_core_pack;
  settings.remaining_coverage_months = ON_PREM_PUBLIC_BOOK_REFERENCE.remaining_coverage_months;
}

type SharedResourceDraft = {
  id: string;
  workload_name: string;
  server_name: string | null;
  quantity: number;
  source_ram_gb_per_instance: string;
  annual_hours_per_instance: string;
};

type SharedSqlResourceDraft = SharedResourceDraft & {
  sql_edition: SqlEdition;
  license_basis: LicenseBasis;
  sql_data_gb_per_instance: string;
  mi_purchase_option: PurchaseOption;
};

export type EbsVolumeDraft = {
  id: string;
  label: string;
  aws_volume_id: string | null;
  volume_type: 'gp3' | 'io2' | 'ephemeral';
  capacity_gb: string;
  provisioned_iops: number | null;
  throughput_mibps: string | null;
};

export type Ec2ResourceDraft = SharedSqlResourceDraft & {
  source_type: 'ec2';
  instance_type: string;
  volumes: EbsVolumeDraft[];
};

export type VmVolumeDraft = Omit<EbsVolumeDraft, 'volume_type' | 'provisioned_iops'> & {
  role: 'os' | 'data' | 'unknown';
  volume_type: 'gp3' | 'io2';
  provisioned_iops: number;
};

export type VmBurstPolicy =
  'confirmed_burst_compatible' | 'requires_sustained_cpu' | 'unknown' | 'not_applicable';
export type VmInstanceStoreUse = 'unknown' | 'not_used' | 'used';
export type VmHighFrequencyRequirement =
  'required' | 'unknown' | 'capacity_fit_accepted' | 'not_applicable';

export type Ec2VmRequirementsDraft = {
  burst_policy: VmBurstPolicy;
  instance_store_use: VmInstanceStoreUse;
  required_local_temp_disk_gb: string | null;
  ephemeral_data_loss_acceptable: boolean | null;
  high_frequency_requirement: VmHighFrequencyRequirement;
  requested_target_arm_sku: string | null;
};

export type Ec2VmResourceDraft = SharedResourceDraft & {
  source_type: 'ec2_vm';
  instance_type: string;
  vm_purchase_option: VmPurchaseOption;
  requirements: Ec2VmRequirementsDraft;
  volumes: VmVolumeDraft[];
};

export type RdsResourceDraft = SharedSqlResourceDraft & {
  source_type: 'rds';
  instance_type: string;
  deployment: 'single_az' | 'multi_az';
  commercial_term: string;
  storage_class: string;
  source_max_iops: number;
};

export type OnPremResourceDraft = SharedSqlResourceDraft & {
  source_type: 'on_prem';
  source_vcpu: number;
  licensable_cores: number;
  source_max_iops: number;
  hardware_capex_usd: string;
  depreciation_years: string;
  average_power_kw_override: string | null;
};

export type ResourceDraft =
  Ec2ResourceDraft | Ec2VmResourceDraft | RdsResourceDraft | OnPremResourceDraft;

export type ProjectDraft = {
  name: string;
  description: string | null;
  settings: ProjectSettingsDraft;
  resources: ResourceDraft[];
  aws_price_snapshot_id: string | null;
  azure_price_snapshot_id: string | null;
};

export type GuestWorkspace = {
  project: ProjectDraft;
  calculation: unknown | null;
  aws_resolution: unknown | null;
  azure_resolution: unknown | null;
  updated_at: string;
};

const DATABASE_NAME = 'azure-sql-tco';
const DATABASE_VERSION = 1;
const STORE_NAME = 'guest-workspace';
const ACTIVE_KEY = 'active';

export function createProjectDraft(
  projectType: ProjectType,
  name: string,
  description: string | null,
  awsRegion = 'eu-west-1',
  azureRegion = 'swedencentral'
): ProjectDraft {
  const onPrem = projectType === 'on_prem';
  const sqlPayg = projectType === 'sql_payg';
  return {
    name,
    description,
    settings: {
      project_type: projectType,
      aws_region: onPrem || sqlPayg ? null : awsRegion,
      azure_region: sqlPayg ? 'global' : azureRegion,
      currency: 'USD',
      source_compute_discount: '0',
      source_license_discount: '0',
      source_storage_discount: '0',
      azure_compute_discount: '0',
      azure_license_discount: '0',
      azure_storage_discount: '0',
      selected_parity_adjustment: '0',
      default_annual_hours: '8760',
      default_mi_purchase_option: 'ahb',
      enterprise_license_sa_usd_per_two_core_pack: null,
      standard_license_sa_usd_per_two_core_pack: null,
      remaining_coverage_months: onPrem ? 36 : null,
      electricity_rate_usd_per_kwh: onPrem ? '0' : null,
      sql_payg: sqlPayg
        ? {
            enterprise_licensed_cores: 0,
            standard_licensed_cores: 0,
            software_assurance_annual_usd: '0'
          }
        : null
    },
    resources: [],
    aws_price_snapshot_id: null,
    azure_price_snapshot_id: null
  };
}

export function createResource(
  projectType: Exclude<ProjectType, 'sql_payg'>,
  defaults: ResourceDefaults
): ResourceDraft {
  const shared: SharedResourceDraft = {
    id: crypto.randomUUID(),
    workload_name: projectType === 'ec2_vm' ? 'Windows VM' : 'SQL workload',
    server_name: null,
    quantity: 1,
    source_ram_gb_per_instance: projectType === 'rds' ? '128' : '256',
    annual_hours_per_instance: defaults.default_annual_hours
  };

  if (projectType === 'ec2_vm') {
    return {
      ...shared,
      source_type: 'ec2_vm',
      instance_type: 'r6id.8xlarge',
      vm_purchase_option: 'payg',
      requirements: createVmRequirements('r6id.8xlarge'),
      volumes: [
        {
          id: crypto.randomUUID(),
          label: 'OS',
          aws_volume_id: null,
          volume_type: 'gp3',
          role: 'os',
          capacity_gb: '1024',
          provisioned_iops: 3000,
          throughput_mibps: '125'
        }
      ]
    };
  }

  const sqlShared: SharedSqlResourceDraft = {
    ...shared,
    sql_edition: 'enterprise',
    license_basis: 'byol',
    sql_data_gb_per_instance: '1024',
    mi_purchase_option: defaults.default_mi_purchase_option
  };

  if (projectType === 'ec2') {
    return {
      ...sqlShared,
      source_type: 'ec2',
      instance_type: 'r6id.8xlarge',
      volumes: [
        {
          id: crypto.randomUUID(),
          label: 'Instance storage',
          aws_volume_id: null,
          volume_type: 'ephemeral',
          capacity_gb: '0',
          provisioned_iops: null,
          throughput_mibps: null
        }
      ]
    };
  }
  if (projectType === 'rds') {
    return {
      ...sqlShared,
      source_type: 'rds',
      instance_type: 'db.m6i.8xlarge',
      deployment: 'single_az',
      commercial_term: 'on-demand',
      storage_class: 'gp3',
      source_max_iops: 0
    };
  }
  return {
    ...sqlShared,
    source_type: 'on_prem',
    source_vcpu: 32,
    licensable_cores: 32,
    source_max_iops: 0,
    hardware_capex_usd: '0',
    depreciation_years: '5',
    average_power_kw_override: null
  };
}

export function createVmRequirements(instanceType: string): Ec2VmRequirementsDraft {
  return {
    burst_policy: instanceType.startsWith('t3.') ? 'confirmed_burst_compatible' : 'not_applicable',
    instance_store_use: 'not_used',
    required_local_temp_disk_gb: null,
    ephemeral_data_loss_acceptable: null,
    high_frequency_requirement: instanceType.startsWith('z1d.')
      ? 'capacity_fit_accepted'
      : 'not_applicable',
    requested_target_arm_sku: null
  };
}

export function createGuestWorkspace(project: ProjectDraft): GuestWorkspace {
  return {
    project,
    calculation: null,
    aws_resolution: null,
    azure_resolution: null,
    updated_at: new Date().toISOString()
  };
}

export function projectRequestPayload(project: ProjectDraft): ProjectDraft {
  const settings = {
    ...project.settings,
    source_compute_discount: requiredDecimal(
      project.settings.source_compute_discount,
      'Source compute discount'
    ),
    source_license_discount: requiredDecimal(
      project.settings.source_license_discount,
      'Source license discount'
    ),
    source_storage_discount: requiredDecimal(
      project.settings.source_storage_discount,
      'Source storage discount'
    ),
    azure_compute_discount: requiredDecimal(
      project.settings.azure_compute_discount,
      'Azure compute discount'
    ),
    azure_license_discount: requiredDecimal(
      project.settings.azure_license_discount,
      'Azure license discount'
    ),
    azure_storage_discount: requiredDecimal(
      project.settings.azure_storage_discount,
      'Azure storage discount'
    ),
    selected_parity_adjustment: requiredDecimal(
      project.settings.selected_parity_adjustment,
      'Selected parity adjustment'
    ),
    default_annual_hours: requiredDecimal(
      project.settings.default_annual_hours,
      'Default annual hours'
    ),
    enterprise_license_sa_usd_per_two_core_pack: optionalDecimal(
      project.settings.enterprise_license_sa_usd_per_two_core_pack
    ),
    standard_license_sa_usd_per_two_core_pack: optionalDecimal(
      project.settings.standard_license_sa_usd_per_two_core_pack
    ),
    remaining_coverage_months: optionalCoverageMonths(project.settings.remaining_coverage_months),
    electricity_rate_usd_per_kwh: optionalDecimal(project.settings.electricity_rate_usd_per_kwh),
    sql_payg: project.settings.sql_payg
      ? {
          enterprise_licensed_cores: requiredInteger(
            project.settings.sql_payg.enterprise_licensed_cores,
            'Enterprise licensed cores'
          ),
          standard_licensed_cores: requiredInteger(
            project.settings.sql_payg.standard_licensed_cores,
            'Standard licensed cores'
          ),
          software_assurance_annual_usd: requiredDecimal(
            project.settings.sql_payg.software_assurance_annual_usd,
            'Annual Software Assurance spend'
          )
        }
      : null
  };
  const resources = project.resources.map((resource, index): ResourceDraft => {
    const shared = {
      id: resource.id,
      workload_name: resource.workload_name,
      server_name: optionalText(resource.server_name),
      quantity: requiredInteger(resource.quantity, `Workload ${index + 1} quantity`),
      source_ram_gb_per_instance: requiredDecimal(
        resource.source_ram_gb_per_instance,
        `Workload ${index + 1} source RAM`
      ),
      annual_hours_per_instance: requiredDecimal(
        resource.annual_hours_per_instance,
        `Workload ${index + 1} annual hours`
      )
    };

    if (resource.source_type === 'ec2_vm') {
      return {
        ...shared,
        source_type: 'ec2_vm',
        instance_type: resource.instance_type,
        vm_purchase_option: resource.vm_purchase_option,
        requirements: {
          ...resource.requirements,
          required_local_temp_disk_gb: optionalDecimal(
            resource.requirements.required_local_temp_disk_gb
          ),
          requested_target_arm_sku: optionalText(resource.requirements.requested_target_arm_sku)
        },
        volumes: resource.volumes.map((volume, volumeIndex) => ({
          ...volume,
          capacity_gb: requiredDecimal(
            volume.capacity_gb,
            `Workload ${index + 1} volume ${volumeIndex + 1} capacity`
          ),
          provisioned_iops: requiredInteger(
            volume.provisioned_iops,
            `Workload ${index + 1} volume ${volumeIndex + 1} provisioned IOPS`
          ),
          throughput_mibps: optionalDecimal(volume.throughput_mibps)
        }))
      };
    }

    const sqlShared = {
      ...shared,
      sql_edition: resource.sql_edition,
      license_basis: resource.license_basis,
      sql_data_gb_per_instance: requiredDecimal(
        resource.sql_data_gb_per_instance,
        `Workload ${index + 1} SQL data`
      ),
      mi_purchase_option: resource.mi_purchase_option
    };

    if (resource.source_type === 'ec2') {
      return {
        ...sqlShared,
        source_type: 'ec2',
        instance_type: resource.instance_type,
        volumes: resource.volumes.map((volume, volumeIndex) => ({
          ...volume,
          capacity_gb: requiredDecimal(
            volume.capacity_gb,
            `Workload ${index + 1} volume ${volumeIndex + 1} capacity`
          ),
          provisioned_iops: optionalInteger(volume.provisioned_iops),
          throughput_mibps: optionalDecimal(volume.throughput_mibps)
        }))
      };
    }
    if (resource.source_type === 'rds') {
      return {
        ...sqlShared,
        source_type: 'rds',
        instance_type: resource.instance_type,
        deployment: resource.deployment,
        commercial_term: resource.commercial_term,
        storage_class: resource.storage_class,
        source_max_iops: requiredInteger(
          resource.source_max_iops,
          `Workload ${index + 1} maximum source IOPS`
        )
      };
    }
    return {
      ...sqlShared,
      source_type: 'on_prem',
      source_vcpu: requiredInteger(resource.source_vcpu, `Workload ${index + 1} source vCPU`),
      licensable_cores: requiredInteger(
        resource.licensable_cores,
        `Workload ${index + 1} licensable cores`
      ),
      source_max_iops: requiredInteger(
        resource.source_max_iops,
        `Workload ${index + 1} maximum source IOPS`
      ),
      hardware_capex_usd: requiredDecimal(
        resource.hardware_capex_usd,
        `Workload ${index + 1} hardware capex`
      ),
      depreciation_years: requiredDecimal(
        resource.depreciation_years,
        `Workload ${index + 1} depreciation years`
      ),
      average_power_kw_override: optionalDecimal(resource.average_power_kw_override)
    };
  });

  return { ...project, settings, resources };
}

function requiredDecimal(value: unknown, label: string): string {
  if (typeof value === 'string' && value.trim() !== '') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  throw new Error(`${label} is required.`);
}

function optionalDecimal(value: unknown): string | null {
  if (value === null || value === undefined || value === '') return null;
  if (typeof value === 'string') return value;
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  return null;
}

function optionalText(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const normalized = value.trim();
  return normalized === '' ? null : normalized;
}

function requiredInteger(value: unknown, label: string): number {
  if (typeof value === 'number' && Number.isSafeInteger(value)) return value;
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value);
    if (Number.isSafeInteger(parsed)) return parsed;
  }
  throw new Error(`${label} is required.`);
}

function optionalInteger(value: unknown): number | null {
  if (value === null || value === undefined || value === '') return null;
  return requiredInteger(value, 'Integer value');
}

function optionalCoverageMonths(value: unknown): 12 | 24 | 36 | null {
  const months = optionalInteger(value);
  if (months === null || months === 12 || months === 24 || months === 36) return months;
  throw new Error('Remaining coverage months must be 12, 24, or 36.');
}

export function editableProject(value: unknown): ProjectDraft | null {
  if (!isRecord(value) || !isRecord(value.settings) || !Array.isArray(value.resources)) {
    return null;
  }
  if (typeof value.name !== 'string') return null;
  const projectType = value.settings.project_type;
  if (
    projectType !== 'ec2' &&
    projectType !== 'ec2_vm' &&
    projectType !== 'rds' &&
    projectType !== 'on_prem' &&
    projectType !== 'sql_payg'
  )
    return null;

  const resources = structuredClone(value.resources) as ResourceDraft[];
  for (const resource of resources) {
    resource.server_name = typeof resource.server_name === 'string' ? resource.server_name : null;
    if (resource.source_type === 'ec2_vm') {
      resource.vm_purchase_option = isVmPurchaseOption(Reflect.get(resource, 'vm_purchase_option'))
        ? resource.vm_purchase_option
        : 'payg';
      resource.requirements = editableVmRequirements(
        Reflect.get(resource, 'requirements'),
        resource.instance_type
      );
    }
  }

  return {
    name: value.name,
    description: typeof value.description === 'string' ? value.description : null,
    settings: structuredClone(value.settings) as ProjectSettingsDraft,
    resources,
    aws_price_snapshot_id:
      typeof value.aws_price_snapshot_id === 'string' ? value.aws_price_snapshot_id : null,
    azure_price_snapshot_id:
      typeof value.azure_price_snapshot_id === 'string' ? value.azure_price_snapshot_id : null
  };
}

function editableVmRequirements(value: unknown, instanceType: string): Ec2VmRequirementsDraft {
  const defaults = createVmRequirements(instanceType);
  if (!isRecord(value)) return defaults;
  return {
    burst_policy:
      value.burst_policy === 'confirmed_burst_compatible' ||
      value.burst_policy === 'requires_sustained_cpu' ||
      value.burst_policy === 'unknown' ||
      value.burst_policy === 'not_applicable'
        ? value.burst_policy
        : defaults.burst_policy,
    instance_store_use:
      value.instance_store_use === 'unknown' ||
      value.instance_store_use === 'not_used' ||
      value.instance_store_use === 'used'
        ? value.instance_store_use
        : defaults.instance_store_use,
    required_local_temp_disk_gb: optionalDecimal(value.required_local_temp_disk_gb),
    ephemeral_data_loss_acceptable:
      typeof value.ephemeral_data_loss_acceptable === 'boolean'
        ? value.ephemeral_data_loss_acceptable
        : null,
    high_frequency_requirement:
      value.high_frequency_requirement === 'required' ||
      value.high_frequency_requirement === 'unknown' ||
      value.high_frequency_requirement === 'capacity_fit_accepted' ||
      value.high_frequency_requirement === 'not_applicable'
        ? value.high_frequency_requirement
        : defaults.high_frequency_requirement,
    requested_target_arm_sku: optionalText(value.requested_target_arm_sku)
  };
}

export async function loadGuestWorkspace(): Promise<GuestWorkspace | null> {
  const database = await openDatabase();
  try {
    const value = await requestToPromise(
      database.transaction(STORE_NAME, 'readonly').objectStore(STORE_NAME).get(ACTIVE_KEY)
    );
    if (!isRecord(value) || typeof value.updated_at !== 'string') return null;
    const project = editableProject(value.project);
    if (!project) return null;
    return {
      project,
      calculation: value.calculation ?? null,
      aws_resolution: value.aws_resolution ?? null,
      azure_resolution: value.azure_resolution ?? null,
      updated_at: value.updated_at
    };
  } finally {
    database.close();
  }
}

export async function saveGuestWorkspace(workspace: GuestWorkspace): Promise<void> {
  const database = await openDatabase();
  try {
    workspace.updated_at = new Date().toISOString();
    await requestToPromise(
      database
        .transaction(STORE_NAME, 'readwrite')
        .objectStore(STORE_NAME)
        .put(structuredClone(workspace), ACTIVE_KEY)
    );
  } finally {
    database.close();
  }
}

export async function clearGuestWorkspace(): Promise<void> {
  const database = await openDatabase();
  try {
    await requestToPromise(
      database.transaction(STORE_NAME, 'readwrite').objectStore(STORE_NAME).delete(ACTIVE_KEY)
    );
  } finally {
    database.close();
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    request.onupgradeneeded = () => {
      if (!request.result.objectStoreNames.contains(STORE_NAME)) {
        request.result.createObjectStore(STORE_NAME);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Unable to open the guest draft.'));
  });
}

function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error ?? new Error('Guest draft operation failed.'));
  });
}
