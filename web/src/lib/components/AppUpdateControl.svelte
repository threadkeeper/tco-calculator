<script lang="ts">
  import { onMount } from 'svelte';
  import { RefreshCw, X } from 'lucide-svelte';
  import { availableUpdateVersion, clearCachedAppFiles } from '$lib/app-update';

  const UPDATE_CHECK_INTERVAL_MS = 60_000;
  const DISMISSED_VERSION_KEY = 'tco.dismissed-update-version';
  const REFRESH_QUERY_KEY = 'app_refresh';

  let availableVersion = $state<string | null>(null);
  let refreshInProgress = $state(false);
  let dismissedVersion: string | null = null;
  let checkInProgress = false;

  onMount(() => {
    removeRefreshQuery();

    try {
      dismissedVersion = localStorage.getItem(DISMISSED_VERSION_KEY);
    } catch {
      // In-memory dismissal still works when browser storage is unavailable.
    }

    const abortController = new AbortController();
    const check = () => void checkForUpdate(abortController.signal);
    const checkWhenVisible = () => {
      if (!document.hidden) check();
    };

    check();
    const interval = window.setInterval(check, UPDATE_CHECK_INTERVAL_MS);
    window.addEventListener('focus', check);
    window.addEventListener('online', check);
    document.addEventListener('visibilitychange', checkWhenVisible);

    return () => {
      abortController.abort();
      window.clearInterval(interval);
      window.removeEventListener('focus', check);
      window.removeEventListener('online', check);
      document.removeEventListener('visibilitychange', checkWhenVisible);
    };
  });

  async function checkForUpdate(signal: AbortSignal) {
    if (checkInProgress) return;
    checkInProgress = true;

    try {
      const response = await fetch('/version', {
        cache: 'no-store',
        headers: { Accept: 'application/json' },
        signal
      });
      if (!response.ok) return;

      availableVersion = availableUpdateVersion(
        await response.json(),
        __APP_VERSION__,
        dismissedVersion
      );
    } catch {
      // Update checks must not interrupt work when the app is offline.
    } finally {
      checkInProgress = false;
    }
  }

  function dismissUpdate() {
    if (!availableVersion) return;

    dismissedVersion = availableVersion;
    try {
      localStorage.setItem(DISMISSED_VERSION_KEY, availableVersion);
    } catch {
      // Keep the dismissal for this tab when browser storage is unavailable.
    }
    availableVersion = null;
  }

  async function refreshApp() {
    if (refreshInProgress) return;
    refreshInProgress = true;

    try {
      await clearCachedAppFiles({
        cacheStorage: 'caches' in window ? window.caches : undefined,
        serviceWorkers: 'serviceWorker' in navigator ? navigator.serviceWorker : undefined
      });
    } finally {
      const url = new URL(window.location.href);
      url.searchParams.set(REFRESH_QUERY_KEY, Date.now().toString());
      window.location.replace(url.toString());
    }
  }

  function removeRefreshQuery() {
    const url = new URL(window.location.href);
    if (!url.searchParams.has(REFRESH_QUERY_KEY)) return;

    url.searchParams.delete(REFRESH_QUERY_KEY);
    window.history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`);
  }
</script>

<div class="update-control">
  <button
    class:update-ready={availableVersion !== null}
    class="refresh-button"
    type="button"
    title={availableVersion
      ? `Refresh for Azure SQL TCO ${availableVersion}`
      : 'Clear cached app files and reload'}
    aria-label={availableVersion
      ? `Refresh to use Azure SQL TCO ${availableVersion}`
      : 'Clear cached app files and reload'}
    disabled={refreshInProgress}
    onclick={() => void refreshApp()}
  >
    <RefreshCw size={17} class={refreshInProgress ? 'spinning' : undefined} />
  </button>

  {#if availableVersion}
    <aside class="update-bubble" role="status" aria-live="polite">
      <button
        class="dismiss-button"
        type="button"
        title="Dismiss update notification"
        aria-label="Dismiss update notification"
        onclick={dismissUpdate}
      >
        <X size={15} />
      </button>
      <strong>Update available</strong>
      <span>Azure SQL TCO {availableVersion} is ready.</span>
    </aside>
  {/if}
</div>

<style>
  .update-control {
    position: relative;
    flex: 0 0 auto;
  }

  .refresh-button,
  .dismiss-button {
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }

  .refresh-button {
    width: 34px;
    height: 34px;
    color: #d8e7e9;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
  }

  .refresh-button:hover {
    color: #fff;
    background: rgb(255 255 255 / 8%);
    border-color: #668087;
  }

  .refresh-button.update-ready {
    color: #2f2700;
    background: #f5d84f;
    border-color: #ffe77d;
    box-shadow: 0 0 0 3px rgb(245 216 79 / 18%);
  }

  .refresh-button:disabled {
    cursor: wait;
    opacity: 0.65;
  }

  :global(.spinning) {
    animation: spin 700ms linear infinite;
  }

  .update-bubble {
    position: fixed;
    top: 68px;
    right: clamp(12px, 3vw, 44px);
    z-index: 20;
    width: min(260px, calc(100vw - 24px));
    min-width: 220px;
    padding: 11px 34px 10px 13px;
    color: #2f2700;
    background: #fff4ad;
    border: 2px solid #554600;
    border-radius: 7px;
    box-shadow: 5px 6px 0 rgb(8 20 24 / 28%);
    transform-origin: top right;
    animation: update-pop 180ms ease-out both;
  }

  .update-bubble::before,
  .update-bubble::after {
    position: absolute;
    right: 9px;
    content: '';
    border-right: 8px solid transparent;
    border-left: 8px solid transparent;
  }

  .update-bubble::before {
    top: -12px;
    border-bottom: 12px solid #554600;
  }

  .update-bubble::after {
    top: -8px;
    right: 11px;
    border-right-width: 6px;
    border-bottom: 9px solid #fff4ad;
    border-left-width: 6px;
  }

  .update-bubble strong,
  .update-bubble span {
    display: block;
    letter-spacing: 0;
  }

  .update-bubble strong {
    font-size: 0.86rem;
    line-height: 1.1;
  }

  .update-bubble span {
    margin-top: 4px;
    color: #5c4d09;
    font-size: 0.76rem;
    line-height: 1.25;
  }

  .dismiss-button {
    position: absolute;
    top: 5px;
    right: 5px;
    width: 24px;
    height: 24px;
    color: #554600;
    background: transparent;
    border: 0;
    border-radius: 3px;
  }

  .dismiss-button:hover {
    color: #171300;
    background: rgb(85 70 0 / 10%);
  }

  @keyframes update-pop {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .update-bubble,
    :global(.spinning) {
      animation: none;
    }
  }
</style>
