import { asRecord, readString } from './api';

const MINUTE_MS = 60 * 1000;
const MINUTES_PER_HOUR = 60;
const HOURS_PER_DAY = 24;

export function pricingResolutionLabel(
  value: unknown,
  required: boolean,
  nowMs = Date.now()
): string {
  const record = asRecord(value);
  const status = readString(record, 'status');
  if (!status) return required ? 'not resolved' : 'not required';

  const label = status.replaceAll('_', ' ');
  if (status !== 'fresh' && status !== 'cached' && status !== 'stale') return label;

  const age = snapshotAge(readString(record, 'retrieved_at'), nowMs);
  return age ? `${label} - ${age} old` : label;
}

function snapshotAge(retrievedAt: string | null, nowMs: number): string | null {
  if (!retrievedAt || !Number.isFinite(nowMs)) return null;
  const retrievedAtMs = Date.parse(retrievedAt);
  if (!Number.isFinite(retrievedAtMs) || retrievedAtMs > nowMs) return null;

  const totalMinutes = Math.floor((nowMs - retrievedAtMs) / MINUTE_MS);
  if (totalMinutes < MINUTES_PER_HOUR) return `${totalMinutes}m`;

  const totalHours = Math.floor(totalMinutes / MINUTES_PER_HOUR);
  const remainingMinutes = totalMinutes % MINUTES_PER_HOUR;
  if (totalHours < HOURS_PER_DAY) {
    return remainingMinutes === 0 ? `${totalHours}h` : `${totalHours}h ${remainingMinutes}m`;
  }

  const days = Math.floor(totalHours / HOURS_PER_DAY);
  const remainingHours = totalHours % HOURS_PER_DAY;
  return remainingHours === 0 ? `${days}d` : `${days}d ${remainingHours}h`;
}
