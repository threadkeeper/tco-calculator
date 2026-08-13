import { describe, expect, it } from 'vitest';
import { pricingResolutionLabel } from './pricing-resolution';

const NOW = Date.parse('2026-08-10T12:00:00Z');

describe('pricing resolution label', () => {
  it('shows the age of usable price snapshots', () => {
    expect(
      pricingResolutionLabel({ status: 'stale', retrieved_at: '2026-08-07T08:00:00Z' }, true, NOW)
    ).toBe('stale - 3d 4h old');
    expect(
      pricingResolutionLabel({ status: 'cached', retrieved_at: '2026-08-10T10:30:00Z' }, true, NOW)
    ).toBe('cached - 1h 30m old');
    expect(
      pricingResolutionLabel({ status: 'fresh', retrieved_at: '2026-08-10T12:00:00Z' }, true, NOW)
    ).toBe('fresh - 0m old');
  });

  it('reports the exact seven-day usable boundary', () => {
    expect(
      pricingResolutionLabel({ status: 'stale', retrieved_at: '2026-08-03T12:00:00Z' }, true, NOW)
    ).toBe('stale - 7d old');
  });

  it('preserves existing status fallbacks when age is unavailable', () => {
    expect(pricingResolutionLabel({ status: 'stale', retrieved_at: 'invalid' }, true, NOW)).toBe(
      'stale'
    );
    expect(pricingResolutionLabel({ status: 'unavailable' }, true, NOW)).toBe('unavailable');
    expect(pricingResolutionLabel(null, true, NOW)).toBe('not resolved');
    expect(pricingResolutionLabel(null, false, NOW)).toBe('not required');
  });
});
