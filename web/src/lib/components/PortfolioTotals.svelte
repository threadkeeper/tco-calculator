<script lang="ts">
  let {
    sourceTotal = null,
    azureTotal = null,
    difference = null
  }: {
    sourceTotal?: string | null;
    azureTotal?: string | null;
    difference?: string | null;
  } = $props();

  const show = (value: string | null): string =>
    value === null ? 'PRICE UNAVAILABLE' : `$${value}`;
</script>

<section class="totals" aria-label="Portfolio totals">
  <div><span>Source annual total</span><strong>{show(sourceTotal)}</strong></div>
  <div class="azure"><span>Azure annual total</span><strong>{show(azureTotal)}</strong></div>
  <div class="difference"><span>Difference</span><strong>{show(difference)}</strong></div>
</section>

<style>
  .totals {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-block: 1px solid #cad4d7;
    background: white;
  }
  .totals div {
    display: grid;
    gap: 5px;
    min-height: 74px;
    padding: 13px 16px;
    border-inline-start: 4px solid #b45f06;
  }
  .totals .azure {
    border-inline-start-color: #006f86;
  }
  .totals .difference {
    border-inline-start-color: #7b3f82;
  }
  span {
    color: #5c6d72;
    font-size: 0.78rem;
  }
  strong {
    overflow-wrap: anywhere;
  }
  @media (max-width: 680px) {
    .totals {
      grid-template-columns: 1fr;
    }
  }
</style>
