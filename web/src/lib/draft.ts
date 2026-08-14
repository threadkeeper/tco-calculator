export type ProjectType = 'ec2' | 'rds' | 'on_prem' | 'sql_payg';
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
  quantity: number;
  sql_edition: SqlEdition;
  license_basis: LicenseBasis;
  sql_data_gb_per_instance: string;
  source_ram_gb_per_instance: string;
  annual_hours_per_instance: string;
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

export type Ec2ResourceDraft = SharedResourceDraft & {
  source_type: 'ec2';
  instance_type: string;
  volumes: EbsVolumeDraft[];
};

export type RdsResourceDraft = SharedResourceDraft & {
  source_type: 'rds';
  instance_type: string;
  deployment: 'single_az' | 'multi_az';
  commercial_term: string;
  storage_class: string;
  source_max_iops: number;
};

export type OnPremResourceDraft = SharedResourceDraft & {
  source_type: 'on_prem';
  source_vcpu: number;
  licensable_cores: number;
  source_max_iops: number;
  hardware_capex_usd: string;
  depreciation_years: string;
  average_power_kw_override: string | null;
};

export type ResourceDraft = Ec2ResourceDraft | RdsResourceDraft | OnPremResourceDraft;

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
    workload_name: 'SQL workload',
    quantity: 1,
    sql_edition: 'enterprise',
    license_basis: 'byol',
    sql_data_gb_per_instance: '1024',
    source_ram_gb_per_instance: projectType === 'rds' ? '128' : '256',
    annual_hours_per_instance: defaults.default_annual_hours,
    mi_purchase_option: defaults.default_mi_purchase_option
  };

  if (projectType === 'ec2') {
    return {
      ...shared,
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
      ...shared,
      source_type: 'rds',
      instance_type: 'db.m6i.8xlarge',
      deployment: 'single_az',
      commercial_term: 'on-demand',
      storage_class: 'gp3',
      source_max_iops: 0
    };
  }
  return {
    ...shared,
    source_type: 'on_prem',
    source_vcpu: 32,
    licensable_cores: 32,
    source_max_iops: 0,
    hardware_capex_usd: '0',
    depreciation_years: '5',
    average_power_kw_override: null
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
      quantity: requiredInteger(resource.quantity, `Workload ${index + 1} quantity`),
      sql_edition: resource.sql_edition,
      license_basis: resource.license_basis,
      sql_data_gb_per_instance: requiredDecimal(
        resource.sql_data_gb_per_instance,
        `Workload ${index + 1} SQL data`
      ),
      source_ram_gb_per_instance: requiredDecimal(
        resource.source_ram_gb_per_instance,
        `Workload ${index + 1} source RAM`
      ),
      annual_hours_per_instance: requiredDecimal(
        resource.annual_hours_per_instance,
        `Workload ${index + 1} annual hours`
      ),
      mi_purchase_option: resource.mi_purchase_option
    };

    if (resource.source_type === 'ec2') {
      return {
        ...shared,
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
        ...shared,
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
      ...shared,
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
    projectType !== 'rds' &&
    projectType !== 'on_prem' &&
    projectType !== 'sql_payg'
  )
    return null;

  return {
    name: value.name,
    description: typeof value.description === 'string' ? value.description : null,
    settings: structuredClone(value.settings) as ProjectSettingsDraft,
    resources: structuredClone(value.resources) as ResourceDraft[],
    aws_price_snapshot_id:
      typeof value.aws_price_snapshot_id === 'string' ? value.aws_price_snapshot_id : null,
    azure_price_snapshot_id:
      typeof value.azure_price_snapshot_id === 'string' ? value.azure_price_snapshot_id : null
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
