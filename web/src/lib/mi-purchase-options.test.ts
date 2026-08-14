import { describe, expect, it } from 'vitest';
import {
  commitmentDiscount,
  formatAppliedDiscount,
  hasMiCommitment,
  MI_COMMITMENT_OPTIONS,
  miPurchaseOption,
  miPurchaseOptionLabel,
  miPurchaseOptionParts,
  type MiCommitment
} from './mi-purchase-options';
import type { PurchaseOption } from './draft';
import selectorSource from './components/MiPurchasePlanSelector.svelte?raw';

const OPTIONS: ReadonlyArray<{
  commitment: MiCommitment;
  usesAzureHybridBenefit: boolean;
  option: PurchaseOption;
  label: string;
}> = [
  {
    commitment: 'payg',
    usesAzureHybridBenefit: false,
    option: 'payg',
    label: 'Pay as you go, license included'
  },
  {
    commitment: 'payg',
    usesAzureHybridBenefit: true,
    option: 'ahb',
    label: 'Pay as you go, Azure Hybrid Benefit'
  },
  {
    commitment: 'one-year',
    usesAzureHybridBenefit: false,
    option: 'one-year',
    label: '1-year reserved, license included'
  },
  {
    commitment: 'one-year',
    usesAzureHybridBenefit: true,
    option: 'ahbone-year',
    label: '1-year reserved, Azure Hybrid Benefit'
  },
  {
    commitment: 'three-year',
    usesAzureHybridBenefit: false,
    option: 'three-year',
    label: '3-year reserved, license included'
  },
  {
    commitment: 'three-year',
    usesAzureHybridBenefit: true,
    option: 'ahbthree-year',
    label: '3-year reserved, Azure Hybrid Benefit'
  },
  {
    commitment: 'sv-one-year',
    usesAzureHybridBenefit: false,
    option: 'sv-one-year',
    label: '1-year savings plan, license included'
  },
  {
    commitment: 'sv-one-year',
    usesAzureHybridBenefit: true,
    option: 'ahbsv-one-year',
    label: '1-year savings plan, Azure Hybrid Benefit'
  }
];

describe('SQL MI purchase options', () => {
  it.each(OPTIONS)('round-trips $option through its two pricing choices', (expected) => {
    expect(miPurchaseOptionParts(expected.option)).toEqual({
      commitment: expected.commitment,
      usesAzureHybridBenefit: expected.usesAzureHybridBenefit
    });
    expect(miPurchaseOption(expected.commitment, expected.usesAzureHybridBenefit)).toBe(
      expected.option
    );
    expect(miPurchaseOptionLabel(expected.option)).toBe(expected.label);
    expect(hasMiCommitment(expected.option)).toBe(expected.commitment !== 'payg');
  });

  it('presents commitment and licensing as separate choices with eligibility guidance', () => {
    expect(selectorSource).toContain('Compute commitment');
    expect(selectorSource).toContain('Azure Hybrid Benefit');
    expect(selectorSource).toContain('SQL license included');
    expect(selectorSource).toContain('active Software Assurance or qualifying');
    expect(selectorSource).toContain('Verify the customer licensing entitlement.');
  });

  it('provides plain-language details and discount guidance for every commitment option', () => {
    expect(MI_COMMITMENT_OPTIONS).toHaveLength(4);
    for (const option of MI_COMMITMENT_OPTIONS) {
      expect(option.summary.length).toBeGreaterThan(40);
      expect(option.discount.toLocaleLowerCase()).toMatch(/discount|off|rate/);
      expect(option.details.length).toBeGreaterThan(40);
    }

    expect(selectorSource).toContain('aria-label={`About ${option.label}`}');
    expect(selectorSource).toContain('About Azure Hybrid Benefit');
    expect(selectorSource).toContain('can save up to 55%');
    expect(selectorSource).toContain('total savings can reach up to 82%');
    expect(selectorSource).toContain('role="dialog"');
    expect(selectorSource).toContain('aria-modal="true"');
  });

  it('formats the applied server-calculated discount for every purchase choice', () => {
    const discounts = {
      payg: '0',
      one_year_reserved: '0.25',
      three_year_reserved: '0.375',
      one_year_savings_plan: '0.125',
      azure_hybrid_benefit: '1'
    };

    expect(formatAppliedDiscount(commitmentDiscount('payg', discounts))).toBe('0%');
    expect(formatAppliedDiscount(commitmentDiscount('one-year', discounts))).toBe('25%');
    expect(formatAppliedDiscount(commitmentDiscount('three-year', discounts))).toBe('37.5%');
    expect(formatAppliedDiscount(commitmentDiscount('sv-one-year', discounts))).toBe('12.5%');
    expect(formatAppliedDiscount(discounts.azure_hybrid_benefit)).toBe('100%');
  });
});
