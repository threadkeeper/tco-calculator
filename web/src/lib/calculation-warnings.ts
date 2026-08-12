const AZURE_STORAGE_FALLBACK_PREFIX = 'Azure SQL MI ';
const AZURE_STORAGE_FALLBACK_SUFFIX =
  ' uses the General Purpose data-storage meter fallback for Next Generation General Purpose.';

export function relevantCalculationWarnings(value: unknown): string[] {
  if (!Array.isArray(value)) return [];

  return value.filter(
    (warning): warning is string =>
      typeof warning === 'string' &&
      !(
        warning.startsWith(AZURE_STORAGE_FALLBACK_PREFIX) &&
        warning.endsWith(AZURE_STORAGE_FALLBACK_SUFFIX)
      )
  );
}
