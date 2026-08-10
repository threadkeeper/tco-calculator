<script lang="ts">
  import { LogIn, LogOut } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';

  let {
    mode = 'guest',
    displayName = null
  }: {
    mode?: 'loading' | 'guest' | 'authenticated' | 'offline';
    displayName?: string | null;
  } = $props();
</script>

<div class="identity-actions">
  {#if mode === 'authenticated'}
    <span class="identity-state">{displayName ?? 'Signed in'}</span>
    <a class="button secondary" href="/.auth/logout?post_logout_redirect_uri=/">
      <LogOut size={17} aria-hidden="true" />
      <span>Sign out</span>
    </a>
  {:else if mode === 'loading'}
    <span class="identity-state">Checking session…</span>
  {:else}
    <span class="identity-state">{mode === 'offline' ? 'Offline draft' : text.guest}</span>
    <a class="button secondary" href="/.auth/login/aad?post_login_redirect_uri=/">
      <LogIn size={17} aria-hidden="true" />
      <span>{text.signIn}</span>
    </a>
  {/if}
</div>