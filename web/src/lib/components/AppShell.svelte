<script lang="ts">
  import type { Snippet } from 'svelte';
  import { Database } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';
  import IdentityMenu from './IdentityMenu.svelte';

  let { children }: { children: Snippet } = $props();
</script>

<header class="app-bar">
  <div class="wordmark">
    <span class="brand-mark" aria-hidden="true"><Database size={20} strokeWidth={2.2} /></span>
    <span>{text.productName}</span>
    <span class="version">v{__APP_VERSION__}</span>
  </div>
  <IdentityMenu />
</header>

<main>{@render children()}</main>