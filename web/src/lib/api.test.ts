import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ApiProblem,
  asRecord,
  asRecords,
  formatMoney,
  readBoolean,
  readNumber,
  readRecord,
  readRecords,
  readString,
  requestJson,
  requestPriceResolution,
  requestJsonResponse
} from './api';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('API requests', () => {
  it('sets JSON headers and returns the response ETag', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ status: 'ready' }), {
        status: 200,
        headers: { 'content-type': 'application/json', etag: '"revision-2"' }
      })
    );
    vi.stubGlobal('fetch', fetchMock);

    const result = await requestJsonResponse('/api/projects', {
      method: 'POST',
      body: JSON.stringify({ name: 'Migration estimate' }),
      headers: { 'x-requested-with': 'tco-web' }
    });

    expect(result).toEqual({ payload: { status: 'ready' }, etag: '"revision-2"' });
    expect(fetchMock).toHaveBeenCalledOnce();
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    const headers = new Headers(init.headers);
    expect(headers.get('accept')).toBe('application/json, application/problem+json');
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('x-requested-with')).toBe('tco-web');
  });

  it('returns null for a successful empty response', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 204 })));

    await expect(requestJson('/api/projects/project-1', { method: 'DELETE' })).resolves.toBeNull();
  });

  it('uses live refresh only when explicitly requested', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(
        async () => new Response(JSON.stringify({ status: 'fresh' }), { status: 200 })
      );
    vi.stubGlobal('fetch', fetchMock);
    const payload = { currency: 'USD', azure_region: 'swedencentral', resources: [] };

    await requestPriceResolution('aws', 'resolve', payload);
    await requestPriceResolution('azure', 'refresh', payload);

    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/v1/pricing/aws/resolve');
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/v1/pricing/azure/refresh');
    expect(fetchMock.mock.calls[1]?.[1]).toMatchObject({
      method: 'POST',
      body: JSON.stringify(payload)
    });
  });

  it('turns problem details and field errors into an ApiProblem', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            detail: 'Invalid project.',
            request_id: 'request-42',
            errors: [{ message: 'Quantity must be positive.' }, { ignored: true }]
          }),
          { status: 422, headers: { 'content-type': 'application/problem+json' } }
        )
      )
    );

    const error = await requestJson('/api/projects').catch((reason: unknown) => reason);

    expect(error).toBeInstanceOf(ApiProblem);
    expect(error).toMatchObject({
      status: 422,
      requestId: 'request-42',
      message: 'Invalid project. Quantity must be positive.'
    });
  });
});

describe('API value readers', () => {
  const record = {
    name: 'estimate',
    count: 2,
    infinite: Number.POSITIVE_INFINITY,
    enabled: true,
    nested: { id: 'nested-1' },
    rows: [{ id: 'row-1' }, null, 'ignored']
  };

  it('accepts records and filters mixed record arrays', () => {
    expect(asRecord(record)).toBe(record);
    expect(asRecord([])).toBeNull();
    expect(asRecords(record.rows)).toEqual([{ id: 'row-1' }]);
    expect(asRecords('not-an-array')).toEqual([]);
  });

  it('reads only values of the requested type', () => {
    expect(readString(record, 'name')).toBe('estimate');
    expect(readString(record, 'count')).toBeNull();
    expect(readNumber(record, 'count')).toBe(2);
    expect(readNumber(record, 'infinite')).toBeNull();
    expect(readBoolean(record, 'enabled')).toBe(true);
    expect(readBoolean(record, 'name')).toBeNull();
    expect(readRecord(record, 'nested')).toEqual({ id: 'nested-1' });
    expect(readRecords(record, 'rows')).toEqual([{ id: 'row-1' }]);
  });

  it('formats valid money values and preserves unavailable values', () => {
    expect(formatMoney('1234.5')).toBe('$1,234.50');
    expect(formatMoney('not-priced')).toBe('not-priced');
    expect(formatMoney(null)).toBe('PRICE UNAVAILABLE');
  });
});
