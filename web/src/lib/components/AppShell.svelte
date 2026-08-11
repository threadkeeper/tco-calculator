<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Database, GitBranch, ShieldCheck } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';
  import IdentityMenu from './IdentityMenu.svelte';

  let {
    children,
    mode = 'guest',
    displayName = null,
    currentProject = null,
    onprivacy
  }: {
    children: Snippet;
    mode?: 'loading' | 'guest' | 'authenticated' | 'offline';
    displayName?: string | null;
    currentProject?: string | null;
    onprivacy: () => void;
  } = $props();
</script>

<header class="app-bar">
  <div class="wordmark">
    <span class="brand-mark" aria-hidden="true"><Database size={20} strokeWidth={2.2} /></span>
    <span>{text.productName}</span>
    <span class="version">v{__APP_VERSION__}</span>
    {#if currentProject}<span class="current-project">{currentProject}</span>{/if}
  </div>
  <div class="header-actions">
    <button
      class="header-icon"
      type="button"
      title="Privacy and data use"
      aria-label="Open privacy and data-use notice"
      onclick={onprivacy}
    >
      <ShieldCheck size={19} />
    </button>
    <a
      class="header-icon"
      href="https://github.com/threadkeeper/tco-calculator"
      target="_blank"
      rel="noreferrer"
      title="View repository on GitHub"
      aria-label="View Azure SQL TCO repository on GitHub"
    >
      <GitBranch size={19} />
    </a>
    <IdentityMenu {mode} {displayName} />
  </div>
</header>

<div class="app-content">{@render children()}</div>

<style>
  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .header-icon {
    width: 34px;
    height: 34px;
    display: grid;
    flex: 0 0 34px;
    place-items: center;
    padding: 0;
    color: #d8e7e9;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
    cursor: pointer;
  }
  .header-icon:hover {
    color: #fff;
    background: rgb(255 255 255 / 8%);
    border-color: #668087;
  }
  @media (max-width: 520px) {
    .header-actions {
      gap: 3px;
    }
  }
</style>
