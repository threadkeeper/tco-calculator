<script lang="ts">
  import { onMount } from 'svelte';
  import { Moon, Sun } from 'lucide-svelte';

  type Theme = 'light' | 'dark';

  const storageKey = 'azure-sql-tco-theme';
  let theme = $state<Theme>('dark');

  function isTheme(value: string | null | undefined): value is Theme {
    return value === 'light' || value === 'dark';
  }

  function applyTheme(nextTheme: Theme, persist = true): void {
    theme = nextTheme;
    document.documentElement.dataset.theme = nextTheme;
    document.documentElement.style.colorScheme = nextTheme;
    document
      .querySelector<HTMLMetaElement>('meta[name="theme-color"]')
      ?.setAttribute('content', nextTheme === 'dark' ? '#0f171c' : '#eef2f3');

    if (persist) {
      try {
        localStorage.setItem(storageKey, nextTheme);
      } catch {
        // The active theme still applies when browser storage is unavailable.
      }
    }
  }

  function toggleTheme(): void {
    applyTheme(theme === 'dark' ? 'light' : 'dark');
  }

  onMount(() => {
    const initialTheme = document.documentElement.dataset.theme;
    applyTheme(isTheme(initialTheme) ? initialTheme : 'dark', false);

    const handleStorage = (event: StorageEvent): void => {
      if (event.key === storageKey && isTheme(event.newValue)) {
        applyTheme(event.newValue, false);
      }
    };

    window.addEventListener('storage', handleStorage);
    return () => window.removeEventListener('storage', handleStorage);
  });
</script>

<button
  class="theme-toggle"
  type="button"
  aria-label={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
  aria-pressed={theme === 'dark'}
  title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
  onclick={toggleTheme}
>
  <span class="theme-icon" aria-hidden="true">
    {#if theme === 'dark'}
      <Sun size={18} strokeWidth={2.1} />
    {:else}
      <Moon size={18} strokeWidth={2.1} />
    {/if}
  </span>
</button>

<style>
  .theme-toggle {
    width: 34px;
    height: 34px;
    display: grid;
    flex: 0 0 34px;
    place-items: center;
    padding: 0;
    color: var(--azure-light);
    background: rgb(0 0 0 / 12%);
    border: 1px solid color-mix(in srgb, var(--azure-light) 38%, transparent);
    border-radius: 50%;
    box-shadow: 0 0 10px rgb(0 116 184 / 12%);
    cursor: pointer;
  }
  .theme-toggle:hover {
    color: #fff;
    background: color-mix(in srgb, var(--azure) 34%, transparent);
    border-color: var(--azure-light);
    box-shadow: 0 0 14px color-mix(in srgb, var(--azure-light) 28%, transparent);
  }
  .theme-icon {
    display: grid;
    transition: transform 180ms ease;
  }
  .theme-toggle:hover .theme-icon {
    transform: rotate(12deg);
  }
  @media (prefers-reduced-motion: reduce) {
    .theme-icon {
      transition: none;
    }
  }
</style>
