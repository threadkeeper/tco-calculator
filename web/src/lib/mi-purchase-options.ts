import type { PurchaseOption } from './draft';

export type MiCommitment = 'payg' | 'one-year' | 'three-year' | 'sv-one-year';

export const MI_COMMITMENT_OPTIONS: ReadonlyArray<{
  value: MiCommitment;
  label: string;
  summary: string;
  discount: string;
  details: string;
}> = [
  {
    value: 'payg',
    label: 'Pay as you go',
    summary: 'No long-term commitment. Pay for the SQL Managed Instance compute hours you use.',
    discount: 'Commitment discount: none. The estimate uses the current pay-as-you-go rate.',
    details:
      'This is the most flexible choice when usage might change or stop. SQL licensing, storage, and networking are priced separately.'
  },
  {
    value: 'one-year',
    label: '1-year reserved',
    summary:
      'Commit to matching SQL Managed Instance compute for one year. Azure applies the benefit automatically to eligible instances in the reservation scope.',
    discount:
      'Microsoft advertises SQL reservations at up to 33% off compute. The actual saving depends on region, tier, hardware, and utilization; this estimate uses the current catalog rate.',
    details:
      'The benefit is use-it-or-lose-it each hour and covers compute, not SQL licensing, storage, or networking. Azure Hybrid Benefit can reduce eligible SQL licensing costs separately.'
  },
  {
    value: 'three-year',
    label: '3-year reserved',
    summary:
      'Commit to matching SQL Managed Instance compute for three years. The longer term usually provides a lower compute rate than a one-year reservation.',
    discount:
      'Microsoft advertises SQL reservations at up to 33% off compute. The actual saving depends on region, tier, hardware, and utilization; this estimate uses the current catalog rate.',
    details:
      'Choose this for stable workloads you expect to keep. Unused reservation hours do not roll over, and SQL licensing, storage, and networking remain separate charges.'
  },
  {
    value: 'sv-one-year',
    label: '1-year savings plan',
    summary:
      'Commit to a fixed hourly database spend for one year. The benefit can move across eligible database usage, making it more flexible than a reservation.',
    discount:
      'There is no single fixed discount percentage. Azure applies the current savings-plan rate for each eligible meter; this estimate uses the current catalog rate.',
    details:
      'Reservations are applied first when both benefits could cover the same usage. A savings plan can then cover other eligible usage, up to its hourly commitment.'
  }
];

const PURCHASE_OPTIONS = {
  payg: { licenseIncluded: 'payg', azureHybridBenefit: 'ahb' },
  'one-year': { licenseIncluded: 'one-year', azureHybridBenefit: 'ahbone-year' },
  'three-year': { licenseIncluded: 'three-year', azureHybridBenefit: 'ahbthree-year' },
  'sv-one-year': { licenseIncluded: 'sv-one-year', azureHybridBenefit: 'ahbsv-one-year' }
} as const satisfies Record<
  MiCommitment,
  { licenseIncluded: PurchaseOption; azureHybridBenefit: PurchaseOption }
>;

export function miPurchaseOption(
  commitment: MiCommitment,
  usesAzureHybridBenefit: boolean
): PurchaseOption {
  const options = PURCHASE_OPTIONS[commitment];
  return usesAzureHybridBenefit ? options.azureHybridBenefit : options.licenseIncluded;
}

export function miPurchaseOptionParts(option: PurchaseOption): {
  commitment: MiCommitment;
  usesAzureHybridBenefit: boolean;
} {
  switch (option) {
    case 'payg':
      return { commitment: 'payg', usesAzureHybridBenefit: false };
    case 'ahb':
      return { commitment: 'payg', usesAzureHybridBenefit: true };
    case 'one-year':
      return { commitment: 'one-year', usesAzureHybridBenefit: false };
    case 'ahbone-year':
      return { commitment: 'one-year', usesAzureHybridBenefit: true };
    case 'three-year':
      return { commitment: 'three-year', usesAzureHybridBenefit: false };
    case 'ahbthree-year':
      return { commitment: 'three-year', usesAzureHybridBenefit: true };
    case 'sv-one-year':
      return { commitment: 'sv-one-year', usesAzureHybridBenefit: false };
    case 'ahbsv-one-year':
      return { commitment: 'sv-one-year', usesAzureHybridBenefit: true };
  }
}

export function miPurchaseOptionLabel(option: PurchaseOption): string {
  const { commitment, usesAzureHybridBenefit } = miPurchaseOptionParts(option);
  const commitmentLabel = MI_COMMITMENT_OPTIONS.find((item) => item.value === commitment)?.label;
  return `${commitmentLabel}, ${usesAzureHybridBenefit ? 'Azure Hybrid Benefit' : 'license included'}`;
}

export function hasMiCommitment(option: PurchaseOption): boolean {
  return miPurchaseOptionParts(option).commitment !== 'payg';
}
