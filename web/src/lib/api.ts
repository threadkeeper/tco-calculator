export type JsonRecord = Record<string, unknown>;

export class ApiProblem extends Error {
  readonly status: number;
  readonly requestId: string | null;

  constructor(status: number, message: string, requestId: string | null) {
    super(message);
    this.name = 'ApiProblem';
    this.status = status;
    this.requestId = requestId;
  }
}

export async function requestJson(path: string, init?: RequestInit): Promise<unknown> {
  return (await requestJsonResponse(path, init)).payload;
}

export function requestPriceResolution(
  provider: 'aws' | 'azure',
  operation: 'resolve' | 'refresh',
  payload: JsonRecord
): Promise<unknown> {
  return requestJson(`/api/v1/pricing/${provider}/${operation}`, {
    method: 'POST',
    body: JSON.stringify(payload)
  });
}

export async function requestJsonResponse(
  path: string,
  init?: RequestInit
): Promise<{ payload: unknown; etag: string | null }> {
  const headers = new Headers(init?.headers);
  if (init?.body !== undefined) headers.set('content-type', 'application/json');
  headers.set('accept', 'application/json, application/problem+json');
  const response = await fetch(path, { ...init, headers });
  const payload: unknown = response.status === 204 ? null : await response.json();
  if (!response.ok) throw problemFromResponse(response.status, payload);
  return { payload, etag: response.headers.get('etag') };
}

export function asRecord(value: unknown): JsonRecord | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? (value as JsonRecord)
    : null;
}

export function asRecords(value: unknown): JsonRecord[] {
  return Array.isArray(value)
    ? value.map(asRecord).filter((record): record is JsonRecord => record !== null)
    : [];
}

export function readString(record: JsonRecord | null, key: string): string | null {
  const value = record?.[key];
  return typeof value === 'string' ? value : null;
}

export function readNumber(record: JsonRecord | null, key: string): number | null {
  const value = record?.[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

export function readRecord(record: JsonRecord | null, key: string): JsonRecord | null {
  return asRecord(record?.[key]);
}

export function readRecords(record: JsonRecord | null, key: string): JsonRecord[] {
  return asRecords(record?.[key]);
}

export function formatMoney(value: string | null): string {
  if (value === null) return 'PRICE UNAVAILABLE';
  const number = Number(value);
  if (!Number.isFinite(number)) return value;
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2
  }).format(number);
}

function problemFromResponse(status: number, payload: unknown): ApiProblem {
  const problem = asRecord(payload);
  const detail = readString(problem, 'detail') ?? `Request failed with status ${status}.`;
  const errors = asRecords(problem?.errors)
    .map((error) => readString(error, 'message'))
    .filter((message): message is string => message !== null);
  const message = errors.length > 0 ? `${detail} ${errors.join(' ')}` : detail;
  return new ApiProblem(status, message, readString(problem, 'request_id'));
}

export function readBoolean(record: JsonRecord | null, key: string): boolean | null {
  const value = record?.[key];
  return typeof value === 'boolean' ? value : null;
}
