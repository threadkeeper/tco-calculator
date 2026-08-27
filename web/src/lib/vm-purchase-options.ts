import { asRecord, readBoolean, readString } from './api';
import type { components } from './api/generated';
import { isVmPurchaseOption, type VmPurchaseOption } from './draft';

export type VmCommitment = 'payg' | 'one-year' | 'three-year' | 'sv-one-year' | 'sv-three-year';
export type VmPurchaseOptionPricing = components['schemas']['VmPurchaseOptionPricing'];

export const VM_COMMITMENT_OPTIONS: ReadonlyArray<{
  value: VmCommitment;
  label: string;
}> = [
  { value: 'payg', label: 'Pay as you go' },
  { value: 'one-year', label: '1-year reservation' },
  { value: 'three-year', label: '3-year reservation' },
  { value: 'sv-one-year', label: '1-year savings plan' },
  { value: 'sv-three-year', label: '3-year savings plan' }
];

const PURCHASE_OPTIONS = {
  payg: { licenseIncluded: 'payg', azureHybridBenefit: 'ahb' },
  'one-year': { licenseIncluded: 'one-year', azureHybridBenefit: 'ahbone-year' },
  'three-year': { licenseIncluded: 'three-year', azureHybridBenefit: 'ahbthree-year' },
  'sv-one-year': { licenseIncluded: 'sv-one-year', azureHybridBenefit: 'ahbsv-one-year' },
  'sv-three-year': { licenseIncluded: 'sv-three-year', azureHybridBenefit: 'ahbsv-three-year' }
} as const satisfies Record<
  VmCommitment,
  { licenseIncluded: VmPurchaseOption; azureHybridBenefit: VmPurchaseOption }
>;

export function vmPurchaseOption(
  commitment: VmCommitment,
  usesAzureHybridBenefit: boolean
): VmPurchaseOption {
  const options = PURCHASE_OPTIONS[commitment];
  return usesAzureHybridBenefit ? options.azureHybridBenefit : options.licenseIncluded;
}

export function vmPurchaseOptionParts(option: VmPurchaseOption): {
  commitment: VmCommitment;
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
    case 'sv-three-year':
      return { commitment: 'sv-three-year', usesAzureHybridBenefit: false };
    case 'ahbsv-three-year':
      return { commitment: 'sv-three-year', usesAzureHybridBenefit: true };
  }
}

export function vmPurchaseOptionLabel(option: VmPurchaseOption): string {
  const { commitment, usesAzureHybridBenefit } = vmPurchaseOptionParts(option);
  const commitmentLabel = VM_COMMITMENT_OPTIONS.find((item) => item.value === commitment)?.label;
  return `${commitmentLabel}, ${usesAzureHybridBenefit ? 'Azure Hybrid Benefit' : 'Windows license included'}`;
}

export function hasVmCommitment(option: VmPurchaseOption): boolean {
  return vmPurchaseOptionParts(option).commitment !== 'payg';
}

export function vmPricingForOption(
  pricing: readonly VmPurchaseOptionPricing[] | null,
  option: VmPurchaseOption
): VmPurchaseOptionPricing | null {
  return pricing?.find((item) => item.purchase_option === option) ?? null;
}

export function readVmPurchaseOptionPricing(value: unknown): VmPurchaseOptionPricing[] | null {
  if (!Array.isArray(value)) return null;
  const pricing: VmPurchaseOptionPricing[] = [];
  for (const item of value) {
    const record = asRecord(item);
    const purchaseOption = readString(record, 'purchase_option');
    const available = readBoolean(record, 'available');
    const computeDiscount = nullableDecimal(record, 'compute_discount');
    const licenseDiscount = nullableDecimal(record, 'license_discount');
    if (
      !isVmPurchaseOption(purchaseOption) ||
      available === null ||
      computeDiscount === undefined ||
      licenseDiscount === undefined
    ) {
      return null;
    }
    pricing.push({
      purchase_option: purchaseOption,
      available,
      compute_discount: computeDiscount,
      license_discount: licenseDiscount
    });
  }
  return pricing;
}

function nullableDecimal(
  record: ReturnType<typeof asRecord>,
  key: string
): string | null | undefined {
  if (!record || !(key in record)) return undefined;
  const value = Reflect.get(record, key);
  return value === null ? null : typeof value === 'string' ? value : undefined;
}
