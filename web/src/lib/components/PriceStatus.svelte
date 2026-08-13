<script lang="ts">
  type PriceState = 'idle' | 'fetching' | 'fresh' | 'cached' | 'stale' | 'error';

  let {
    provider,
    state,
    detail = ''
  }: { provider: 'AWS' | 'Azure'; state: PriceState; detail?: string } = $props();

  function priceLabel(priceState: PriceState, priceProvider: 'AWS' | 'Azure'): string {
    if (priceState === 'fetching') return `Fetching ${priceProvider} prices...`;

    return {
      idle: 'Not fetched',
      fresh: 'Fresh',
      cached: 'Cached',
      stale: 'Stale',
      error: 'Unavailable'
    }[priceState];
  }
</script>

<div
  class="price-status"
  data-state={state}
  role={state === 'fetching' ? 'status' : undefined}
  aria-live="polite"
>
  <strong>{provider}</strong>
  <span>{priceLabel(state, provider)}</span>
  {#if detail}<small>{detail}</small>{/if}
</div>

<style>
  .price-status {
    min-height: 48px;
    display: grid;
    grid-template-columns: 58px 120px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    padding: 8px 12px;
    color: var(--ink);
    background: var(--surface-subtle);
    border-inline-start: 4px solid var(--border-input);
  }
  .price-status[data-state='fresh'] {
    border-inline-start-color: var(--success);
  }
  .price-status[data-state='stale'],
  .price-status[data-state='cached'] {
    border-inline-start-color: var(--warning);
  }
  .price-status[data-state='error'] {
    border-inline-start-color: var(--danger);
  }
  small {
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
