<script lang="ts">
  import { ExternalLink, X } from 'lucide-svelte';

  let {
    open,
    state,
    message,
    onopen,
    onclose
  }: {
    open: boolean;
    state: 'starting' | 'ready' | 'error';
    message: string;
    onopen: () => void;
    onclose: () => void;
  } = $props();
</script>

{#if open}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="calculator-launch-title">
      <div class="heading">
        <div>
          <span>Azure Pricing Calculator</span>
          <h2 id="calculator-launch-title">Create Calculator estimate</h2>
        </div>
        <button
          class="icon-button"
          type="button"
          title="Close"
          aria-label="Close"
          onclick={onclose}
        >
          <X size={18} />
        </button>
      </div>
      <p class:error={state === 'error'} aria-live="polite">{message}</p>
      <div class="actions">
        <button class="open" type="button" onclick={onopen} disabled={state === 'starting'}>
          <ExternalLink size={17} />
          Open companion
        </button>
        <button class="done" type="button" onclick={onclose}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 16px;
    background: rgb(15 30 35 / 55%);
  }
  .dialog {
    width: min(100%, 500px);
    padding: 20px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 18px 48px rgb(15 30 35 / 22%);
  }
  .heading,
  .actions {
    display: flex;
    align-items: center;
  }
  .heading {
    justify-content: space-between;
    gap: 16px;
  }
  .heading span {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
  }
  h2 {
    margin: 3px 0 0;
    color: var(--ink-strong);
    font:
      680 1.2rem/1.25 Bahnschrift,
      sans-serif;
  }
  p {
    margin: 18px 0 22px;
    color: var(--ink-soft);
    line-height: 1.5;
  }
  p.error {
    color: var(--danger-text);
  }
  .actions {
    justify-content: flex-end;
    gap: 8px;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 38px;
    padding: 7px 13px;
    border-radius: 4px;
    font:
      700 0.82rem/1 Aptos,
      sans-serif;
    cursor: pointer;
  }
  button:disabled {
    cursor: wait;
    opacity: 0.62;
  }
  .icon-button {
    width: 34px;
    min-height: 34px;
    padding: 0;
    color: var(--muted);
    background: transparent;
    border: 0;
  }
  .open {
    color: #fff;
    background: var(--azure);
    border: 1px solid var(--azure);
  }
  .done {
    color: var(--ink-soft);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
  }
</style>
