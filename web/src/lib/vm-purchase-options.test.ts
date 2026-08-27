import { describe, expect, it } from 'vitest';
import {
  VM_COMMITMENT_OPTIONS,
  readVmPurchaseOptionPricing,
  vmPurchaseOption,
  vmPurchaseOptionLabel,
  vmPurchaseOptionParts
} from './vm-purchase-options';

describe('VM purchase options', () => {
  it.each([
    ['payg', false, 'payg'],
    ['payg', true, 'ahb'],
    ['one-year', false, 'one-year'],
    ['one-year', true, 'ahbone-year'],
    ['three-year', false, 'three-year'],
    ['three-year', true, 'ahbthree-year'],
    ['sv-one-year', false, 'sv-one-year'],
    ['sv-one-year', true, 'ahbsv-one-year'],
    ['sv-three-year', false, 'sv-three-year'],
    ['sv-three-year', true, 'ahbsv-three-year']
  ] as const)('maps %s with AHB=%s to %s', (commitment, usesAhb, option) => {
    expect(vmPurchaseOption(commitment, usesAhb)).toBe(option);
    expect(vmPurchaseOptionParts(option)).toEqual({
      commitment,
      usesAzureHybridBenefit: usesAhb
    });
    expect(vmPurchaseOptionLabel(option)).toContain(
      VM_COMMITMENT_OPTIONS.find((item) => item.value === commitment)?.label
    );
  });

  it('accepts only complete server pricing rows', () => {
    expect(
      readVmPurchaseOptionPricing([
        {
          purchase_option: 'ahbsv-three-year',
          available: true,
          compute_discount: '0.4',
          license_discount: '1'
        }
      ])
    ).toEqual([
      {
        purchase_option: 'ahbsv-three-year',
        available: true,
        compute_discount: '0.4',
        license_discount: '1'
      }
    ]);
    expect(
      readVmPurchaseOptionPricing([{ purchase_option: 'unsupported', available: true }])
    ).toBeNull();
  });
});
