import { describe, expect, it } from 'vitest';
import { DEFAULT_AWS_REGION, readRegionOptions } from './regions';

describe('region catalog options', () => {
  it('reads labeled values and preserves the current fallback', () => {
    const result = readRegionOptions(
      {
        items: [
          { code: 'eu-west-1', label: 'EU (Ireland)' },
          { code: 'eu-north-1', label: 'EU (Stockholm)' },
          { code: '', label: 'Invalid' }
        ]
      },
      [DEFAULT_AWS_REGION, { value: 'legacy-region', label: 'legacy-region' }]
    );

    expect(result).toEqual([
      DEFAULT_AWS_REGION,
      { value: 'legacy-region', label: 'legacy-region' },
      { value: 'eu-north-1', label: 'EU (Stockholm)' }
    ]);
  });

  it('uses fallback options when a catalog response is unavailable', () => {
    expect(readRegionOptions(null, [DEFAULT_AWS_REGION])).toEqual([DEFAULT_AWS_REGION]);
  });
});
