<script lang="ts">
  import { AlertTriangle } from 'lucide-svelte';
  import type { PurchaseOption } from '$lib/draft';
  import {
    MI_COMMITMENT_OPTIONS,
    miPurchaseOption,
    miPurchaseOptionParts,
    type MiCommitment
  } from '$lib/mi-purchase-options';

  let {
    id,
    legend,
    value = $bindable(),
    onchange = () => {}
  }: {
    id: string;
    legend: string;
    value: PurchaseOption;
    onchange?: () => void;
  } = $props();

  const selected = $derived(miPurchaseOptionParts(value));

  function selectCommitment(event: Event) {
    const commitment = (event.currentTarget as HTMLSelectElement).value as MiCommitment;
    value = miPurchaseOption(commitment, selected.usesAzureHybridBenefit);
    onchange();
  }

  function selectLicense(event: Event) {
    const usesAzureHybridBenefit = (event.currentTarget as HTMLInputElement).checked;
    value = miPurchaseOption(selected.commitment, usesAzureHybridBenefit);
    onchange();
  }
</script>

<fieldset class="purchase-plan">
  <legend>{legend}</legend>
  <label class="commitment" for={`${id}-commitment`}>
    <span>Compute commitment</span>
    <select id={`${id}-commitment`} value={selected.commitment} onchange={selectCommitment}>
      {#each MI_COMMITMENT_OPTIONS as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
  <label class="license-choice" for={`${id}-ahb`}>
    <input
      id={`${id}-ahb`}
      type="checkbox"
      checked={selected.usesAzureHybridBenefit}
      aria-describedby={selected.usesAzureHybridBenefit ? `${id}-ahb-warning` : undefined}
      onchange={selectLicense}
    />
    <span class="license-copy">
      <strong>Azure Hybrid Benefit</strong>
      <span
        >{selected.usesAzureHybridBenefit
          ? 'Eligible licenses applied'
          : 'SQL license included'}</span
      >
    </span>
  </label>
  {#if selected.usesAzureHybridBenefit}
    <p class="eligibility-warning" id={`${id}-ahb-warning`}>
      <AlertTriangle size={16} aria-hidden="true" />
      <span
        >Requires eligible SQL Server licenses with active Software Assurance or qualifying
        subscriptions. Verify the customer licensing entitlement.</span
      >
    </p>
  {/if}
</fieldset>

<style>
  .purchase-plan {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 14px;
    min-width: 0;
    margin: 0;
    padding: 11px 12px 12px;
    background: var(--surface-subtle);
    border: 1px solid var(--border);
  }
  legend {
    padding: 0 5px;
    color: var(--copilot-ink);
    font:
      700 0.8rem/1.2 Bahnschrift,
      sans-serif;
    text-shadow: 0 0 10px rgb(133 52 243 / 18%);
  }
  .commitment {
    display: grid;
    gap: 6px;
    min-width: 0;
    color: var(--ink-soft);
    font-size: 0.76rem;
    font-weight: 700;
  }
  select {
    width: 100%;
    min-width: 0;
    min-height: 38px;
    box-sizing: border-box;
    padding: 7px 9px;
    color: var(--copilot-ink);
    background: var(--copilot-surface);
    border: 1px solid color-mix(in srgb, var(--copilot-purple) 62%, var(--border-input));
    border-radius: 4px;
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(133 52 243 / 10%),
      0 0 14px rgb(133 52 243 / 18%);
    font:
      650 0.9rem/1.3 Aptos,
      'Trebuchet MS',
      sans-serif;
  }
  select:hover {
    background: color-mix(in srgb, var(--copilot-purple-light) 12%, var(--surface-input));
    border-color: var(--copilot-purple);
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(200 152 253 / 18%),
      0 0 18px rgb(133 52 243 / 26%);
  }
  select:focus {
    border-color: var(--copilot-purple);
    outline: 3px solid rgb(200 152 253 / 32%);
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(200 152 253 / 24%),
      0 0 20px rgb(133 52 243 / 30%);
  }
  .license-choice {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    min-height: 38px;
    box-sizing: border-box;
    padding: 7px 10px;
    color: var(--ink-soft);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 4px;
    cursor: pointer;
  }
  .license-choice:focus-within {
    border-color: var(--azure);
    outline: 2px solid var(--azure-focus);
  }
  .license-choice input {
    flex: 0 0 auto;
    width: 17px;
    height: 17px;
    margin: 0;
    accent-color: var(--azure);
  }
  .license-copy {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .license-copy strong {
    color: var(--ink);
    font-size: 0.78rem;
  }
  .license-copy span {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 400;
  }
  .eligibility-warning {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: auto 1fr;
    gap: 7px;
    margin: 0;
    padding: 8px 10px;
    color: var(--warning-text);
    background: var(--warning-surface);
    border-left: 3px solid var(--warning-border);
    font-size: 0.75rem;
    font-weight: 500;
    line-height: 1.35;
  }
  @media (max-width: 620px) {
    .purchase-plan {
      grid-template-columns: 1fr;
    }
    .eligibility-warning {
      grid-column: 1;
    }
  }
</style>
