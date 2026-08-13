import type { PurchaseOption } from './draft';

export type MiCommitment = 'payg' | 'one-year' | 'three-year' | 'sv-one-year';

export const MI_COMMITMENT_OPTIONS: ReadonlyArray<{
  value: MiCommitment;
  label: string;
}> = [
  { value: 'payg', label: 'Pay as you go' },
  { value: 'one-year', label: '1-year reserved' },
  { value: 'three-year', label: '3-year reserved' },
  { value: 'sv-one-year', label: '1-year savings plan' }
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
