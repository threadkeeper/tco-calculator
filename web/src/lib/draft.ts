export type ProjectType = 'ec2' | 'rds' | 'on_prem';
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
};

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
  return {
    name,
    description,
    settings: {
      project_type: projectType,
      aws_region: onPrem ? null : awsRegion,
      azure_region: azureRegion,
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
      enterprise_license_sa_usd_per_two_core_pack: onPrem ? '0' : null,
      standard_license_sa_usd_per_two_core_pack: onPrem ? '0' : null,
      remaining_coverage_months: onPrem ? 12 : null,
      electricity_rate_usd_per_kwh: onPrem ? '0' : null
    },
    resources: [],
    aws_price_snapshot_id: null,
    azure_price_snapshot_id: null
  };
}

export function createResource(projectType: ProjectType): ResourceDraft {
  const shared: SharedResourceDraft = {
    id: crypto.randomUUID(),
    workload_name: 'SQL workload',
    quantity: 1,
    sql_edition: 'enterprise',
    license_basis: 'byol',
    sql_data_gb_per_instance: '1024',
    source_ram_gb_per_instance: projectType === 'rds' ? '128' : '256',
    annual_hours_per_instance: '8760',
    mi_purchase_option: 'ahb'
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

export function editableProject(value: unknown): ProjectDraft | null {
  if (!isRecord(value) || !isRecord(value.settings) || !Array.isArray(value.resources)) {
    return null;
  }
  if (typeof value.name !== 'string') return null;
  const projectType = value.settings.project_type;
  if (projectType !== 'ec2' && projectType !== 'rds' && projectType !== 'on_prem') return null;

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
    return isGuestWorkspace(value) ? value : null;
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

function isGuestWorkspace(value: unknown): value is GuestWorkspace {
  return (
    isRecord(value) &&
    editableProject(value.project) !== null &&
    typeof value.updated_at === 'string'
  );
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
