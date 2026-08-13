const VERSION_PATTERN = /^\d+\.\d+\.\d+$/;

type CacheStorageApi = {
  keys: () => Promise<string[]>;
  delete: (cacheName: string) => Promise<boolean>;
};

type ServiceWorkerRegistrationApi = {
  unregister: () => Promise<boolean>;
};

type ServiceWorkerApi = {
  getRegistrations: () => Promise<readonly ServiceWorkerRegistrationApi[]>;
};

export type AppCacheApis = {
  cacheStorage?: CacheStorageApi;
  serviceWorkers?: ServiceWorkerApi;
};

export function availableUpdateVersion(
  payload: unknown,
  currentVersion: string,
  dismissedVersion: string | null
): string | null {
  if (!payload || typeof payload !== 'object') return null;

  const version = normalizeVersion((payload as Record<string, unknown>).version);
  const current = normalizeVersion(currentVersion);
  const dismissed = normalizeVersion(dismissedVersion);

  if (!version || !current || compareVersions(version, current) <= 0 || version === dismissed)
    return null;
  return version;
}

export async function clearCachedAppFiles({
  cacheStorage,
  serviceWorkers
}: AppCacheApis): Promise<void> {
  const clearingTasks: Promise<unknown>[] = [];

  if (cacheStorage) {
    clearingTasks.push(
      cacheStorage
        .keys()
        .then((cacheNames) =>
          Promise.allSettled(cacheNames.map((cacheName) => cacheStorage.delete(cacheName)))
        )
    );
  }

  if (serviceWorkers) {
    clearingTasks.push(
      serviceWorkers
        .getRegistrations()
        .then((registrations) =>
          Promise.allSettled(registrations.map((registration) => registration.unregister()))
        )
    );
  }

  await Promise.allSettled(clearingTasks);
}

function normalizeVersion(value: unknown): string | null {
  if (typeof value !== 'string') return null;

  const normalized = value.trim().replace(/^v/i, '');
  return VERSION_PATTERN.test(normalized) ? normalized : null;
}

function compareVersions(left: string, right: string): number {
  const leftParts = left.split('.').map(Number);
  const rightParts = right.split('.').map(Number);

  for (let index = 0; index < leftParts.length; index += 1) {
    const difference = leftParts[index] - rightParts[index];
    if (difference !== 0) return difference;
  }
  return 0;
}
