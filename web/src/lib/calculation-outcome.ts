export type CalculationTargetOutcome = 'available' | 'no_mapping' | 'price_unavailable';

export function calculationTargetOutcome(
  comparableResourceCount: number,
  noMappingResourceCount: number,
  priceUnavailableResourceCount: number
): CalculationTargetOutcome {
  if (comparableResourceCount > 0) return 'available';
  if (priceUnavailableResourceCount > 0) return 'price_unavailable';
  if (noMappingResourceCount > 0) return 'no_mapping';
  return 'available';
}
