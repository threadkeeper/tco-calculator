<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Database } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';
  import IdentityMenu from './IdentityMenu.svelte';

  let {
    children,
    mode = 'guest',
    displayName = null,
    currentProject = null
  }: {
    children: Snippet;
    mode?: 'loading' | 'guest' | 'authenticated' | 'offline';
    displayName?: string | null;
    currentProject?: string | null;
  } = $props();
</script>

<header class="app-bar">
  <div class="wordmark">
    <span class="brand-mark" aria-hidden="true"><Database size={20} strokeWidth={2.2} /></span>
    <span>{text.productName}</span>
    <span class="version">v{__APP_VERSION__}</span>
    {#if currentProject}<span class="current-project">{currentProject}</span>{/if}
  </div>
  <IdentityMenu {mode} {displayName} />
</header>

<div class="app-content">{@render children()}</div>
