import { asRecord, readRecords, readString } from './api';

export type RegionOption = {
  value: string;
  label: string;
};

export const DEFAULT_AWS_REGION: RegionOption = {
  value: 'eu-west-1',
  label: 'EU (Ireland)'
};

export const DEFAULT_AZURE_REGION: RegionOption = {
  value: 'swedencentral',
  label: 'Sweden Central'
};

export function readRegionOptions(payload: unknown, fallback: RegionOption[]): RegionOption[] {
  const options = readRecords(asRecord(payload), 'items')
    .map((item) => ({
      value: readString(item, 'code') ?? '',
      label: readString(item, 'label') ?? ''
    }))
    .filter((option) => option.value !== '' && option.label !== '');

  const merged = new Map(fallback.map((option) => [option.value, option]));
  for (const option of options) merged.set(option.value, option);
  return [...merged.values()];
}
