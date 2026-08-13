import { describe, expect, it, vi } from 'vitest';
import { availableUpdateVersion, clearCachedAppFiles } from './app-update';

describe('app update detection', () => {
  it('returns a different deployed version', () => {
    expect(availableUpdateVersion({ version: 'v1.3.0' }, '1.2.4', null)).toBe('1.3.0');
    expect(availableUpdateVersion({ version: '2.0.0' }, '1.12.9', null)).toBe('2.0.0');
  });

  it('ignores current, older, dismissed, and malformed versions', () => {
    expect(availableUpdateVersion({ version: '1.2.4' }, 'v1.2.4', null)).toBeNull();
    expect(availableUpdateVersion({ version: '1.2.3' }, '1.2.4', null)).toBeNull();
    expect(availableUpdateVersion({ version: '1.3.0' }, '1.2.4', '1.3.0')).toBeNull();
    expect(availableUpdateVersion({ version: 'latest' }, '1.2.4', null)).toBeNull();
    expect(availableUpdateVersion(null, '1.2.4', null)).toBeNull();
  });
});

describe('app cache refresh', () => {
  it('deletes cache storage and unregisters service workers', async () => {
    const deleteCache = vi.fn(async () => true);
    const unregisterFirst = vi.fn(async () => true);
    const unregisterSecond = vi.fn(async () => false);

    await clearCachedAppFiles({
      cacheStorage: {
        keys: async () => ['app-shell', 'static-assets'],
        delete: deleteCache
      },
      serviceWorkers: {
        getRegistrations: async () => [
          { unregister: unregisterFirst },
          { unregister: unregisterSecond }
        ]
      }
    });

    expect(deleteCache).toHaveBeenCalledTimes(2);
    expect(deleteCache).toHaveBeenNthCalledWith(1, 'app-shell');
    expect(deleteCache).toHaveBeenNthCalledWith(2, 'static-assets');
    expect(unregisterFirst).toHaveBeenCalledOnce();
    expect(unregisterSecond).toHaveBeenCalledOnce();
  });

  it('settles unavailable cache APIs so reload can continue', async () => {
    await expect(
      clearCachedAppFiles({
        cacheStorage: {
          keys: async () => {
            throw new Error('cache storage unavailable');
          },
          delete: async () => false
        },
        serviceWorkers: {
          getRegistrations: async () => {
            throw new Error('service workers unavailable');
          }
        }
      })
    ).resolves.toBeUndefined();
  });
});
