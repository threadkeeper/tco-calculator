import { describe, expect, it } from 'vitest';
import { relevantCalculationWarnings } from './calculation-warnings';

describe('calculation warnings', () => {
  it('removes legacy per-candidate Azure storage fallback diagnostics', () => {
    expect(
      relevantCalculationWarnings([
        'Azure SQL MI managed-vcore-next-gen-general-purpose-premium-series-10 uses the General Purpose data-storage meter fallback for Next Generation General Purpose.',
        'Azure SQL MI managed-vcore-next-gen-general-purpose-premium-series-12 uses the General Purpose data-storage meter fallback for Next Generation General Purpose.'
      ])
    ).toEqual([]);
  });

  it('preserves actionable warnings and ignores malformed values', () => {
    const warnings = [
      'Azure pricing snapshot is stale but still within the usable window.',
      'Azure SQL MI managed-vcore-business-critical-premium-series-8 has no applicable data-storage meter.'
    ];

    expect(relevantCalculationWarnings([warnings[0], null, 42, warnings[1]])).toEqual(warnings);
    expect(relevantCalculationWarnings(null)).toEqual([]);
  });
});
