<script lang="ts">
  import { CircleAlert, X } from 'lucide-svelte';

  let { message, ondismiss }: { message: string; ondismiss?: () => void } = $props();
</script>

<div class="problem" role="alert">
  <CircleAlert size={18} aria-hidden="true" />
  <span>{message}</span>
  {#if ondismiss}
    <button type="button" onclick={ondismiss} aria-label="Dismiss error" title="Dismiss error"
      ><X size={17} /></button
    >
  {/if}
</div>

<style>
  .problem {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    color: var(--danger-text);
    background: var(--danger-surface);
    border: 1px solid var(--danger-border);
  }
  button {
    display: grid;
    place-items: center;
    padding: 4px;
    color: inherit;
    background: transparent;
    border: 0;
    cursor: pointer;
  }
</style>
