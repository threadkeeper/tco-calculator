import { describe, expect, it } from 'vitest';
import { calculationTargetOutcome } from './calculation-outcome';

describe('calculation target outcome', () => {
  it('reports no mapping when every source workload is structurally ineligible', () => {
    expect(calculationTargetOutcome(0, 1, 0)).toBe('no_mapping');
  });

  it('keeps provider price failures distinct from no mapping', () => {
    expect(calculationTargetOutcome(0, 0, 1)).toBe('price_unavailable');
    expect(calculationTargetOutcome(0, 1, 1)).toBe('price_unavailable');
  });

  it('shows mapped portfolio totals when at least one workload is comparable', () => {
    expect(calculationTargetOutcome(1, 1, 0)).toBe('available');
  });
});
