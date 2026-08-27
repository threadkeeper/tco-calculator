import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import VmPurchasePlanSelector from './VmPurchasePlanSelector.svelte';

describe('VM purchase plan selector', () => {
  it('shows all commitment terms, exact unavailability, and the AHB eligibility warning', () => {
    const { body } = render(VmPurchasePlanSelector, {
      props: {
        id: 'vm-plan',
        value: 'ahbsv-three-year',
        pricing: [
          {
            purchase_option: 'ahbsv-three-year',
            available: false,
            compute_discount: null,
            license_discount: null
          }
        ]
      }
    });

    expect(body).toContain('Pay as you go');
    expect(body).toContain('1-year reservation');
    expect(body).toContain('3-year reservation');
    expect(body).toContain('1-year savings plan');
    expect(body).toContain('3-year savings plan (unavailable)');
    expect(body).toContain('This exact pricing option is unavailable');
    expect(body).toContain('Requires eligible Windows Server licenses');
    expect(body).toContain(
      'href="https://learn.microsoft.com/windows-server/get-started/azure-hybrid-benefit"'
    );
  });
});
