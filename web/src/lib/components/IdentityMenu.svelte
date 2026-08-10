<script lang="ts">
  import { LogIn } from 'lucide-svelte';
  import { text } from '$lib/i18n/en';
</script>

<div class="identity-actions">
  <span class="identity-state">{text.guest}</span>
  <a class="button secondary" href="/.auth/login/aad?post_login_redirect_uri=/">
    <LogIn size={17} aria-hidden="true" />
    <span>{text.signIn}</span>
  </a>
</div>