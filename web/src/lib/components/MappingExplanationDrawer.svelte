<script lang="ts">
  import { X } from 'lucide-svelte';

  let { open, onclose }: { open: boolean; onclose: () => void } = $props();
</script>

{#if open}
  <aside class="drawer" aria-labelledby="explanation-heading">
    <header><h2 id="explanation-heading">Calculation explanation</h2><button type="button" onclick={onclose} aria-label="Close explanation" title="Close"><X size={18} /></button></header>
    <section><h3>Target decision</h3><p>Structured calculation steps appear here after a successful calculation.</p></section>
  </aside>
{/if}

<style>
  .drawer { position: fixed; inset: 0 0 0 auto; z-index: 15; width: min(100%, 560px); overflow-y: auto; background: white; border-left: 1px solid #cad4d7; box-shadow: -12px 0 35px rgb(15 30 35 / 16%); }
  header { min-height: 56px; display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 12px 16px; border-bottom: 1px solid #cad4d7; }
  h2, h3 { margin: 0; font-size: 1rem; }
  button { display: grid; place-items: center; padding: 6px; background: transparent; border: 0; cursor: pointer; }
  section { padding: 18px 16px; }
  p { color: #5c6d72; }
</style>