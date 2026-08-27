<script lang="ts">
  import { AlertTriangle } from 'lucide-svelte';
  import { formatAppliedDiscount } from '$lib/mi-purchase-options';
  import type { VmPurchaseOption } from '$lib/draft';
  import {
    VM_COMMITMENT_OPTIONS,
    vmPricingForOption,
    vmPurchaseOption,
    vmPurchaseOptionParts,
    type VmCommitment,
    type VmPurchaseOptionPricing
  } from '$lib/vm-purchase-options';

  let {
    id,
    value = $bindable(),
    pricing = null,
    onchange = () => {}
  }: {
    id: string;
    value: VmPurchaseOption;
    pricing?: VmPurchaseOptionPricing[] | null;
    onchange?: () => void;
  } = $props();

  const selected = $derived(vmPurchaseOptionParts(value));
  const selectedPricing = $derived(vmPricingForOption(pricing, value));

  function optionPricing(commitment: VmCommitment): VmPurchaseOptionPricing | null {
    return vmPricingForOption(
      pricing,
      vmPurchaseOption(commitment, selected.usesAzureHybridBenefit)
    );
  }

  function selectCommitment(event: Event) {
    const commitment = (event.currentTarget as HTMLSelectElement).value as VmCommitment;
    value = vmPurchaseOption(commitment, selected.usesAzureHybridBenefit);
    onchange();
  }

  function selectLicense(event: Event) {
    const usesAzureHybridBenefit = (event.currentTarget as HTMLInputElement).checked;
    value = vmPurchaseOption(selected.commitment, usesAzureHybridBenefit);
    onchange();
  }
</script>

<fieldset class="vm-purchase-plan">
  <legend>Azure VM pricing</legend>
  <label class="commitment" for={`${id}-commitment`}>
    <span>Compute commitment</span>
    <select id={`${id}-commitment`} value={selected.commitment} onchange={selectCommitment}>
      {#each VM_COMMITMENT_OPTIONS as option (option.value)}
        {@const optionRate = optionPricing(option.value)}
        <option value={option.value} disabled={pricing !== null && optionRate?.available !== true}
          >{option.label}{pricing !== null
            ? optionRate?.available
              ? ` · ${formatAppliedDiscount(optionRate.compute_discount ?? '0')} lower compute`
              : ' (unavailable)'
            : ''}</option
        >
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
    <span>
      <strong>Azure Hybrid Benefit</strong>
      <small
        >{selected.usesAzureHybridBenefit
          ? 'Eligible Windows Server licenses applied'
          : 'Windows Server license included'}</small
      >
    </span>
  </label>
  {#if pricing !== null && selectedPricing?.available !== true}
    <p class="availability-warning" role="status">
      <AlertTriangle size={16} aria-hidden="true" />
      <span>This exact pricing option is unavailable for the selected Azure VM SKU and region.</span
      >
    </p>
  {/if}
  {#if selected.usesAzureHybridBenefit}
    <p class="eligibility-warning" id={`${id}-ahb-warning`}>
      <AlertTriangle size={16} aria-hidden="true" />
      <span
        >Requires eligible Windows Server licenses with active Software Assurance or qualifying
        subscriptions. Verify the customer licensing entitlement in the
        <a
          href="https://learn.microsoft.com/windows-server/get-started/azure-hybrid-benefit"
          target="_blank"
          rel="noreferrer">Azure Hybrid Benefit guidance</a
        >.</span
      >
    </p>
  {/if}
</fieldset>

<style>
  .vm-purchase-plan {
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
    color: var(--azure-text);
    font:
      700 0.8rem/1.2 Bahnschrift,
      sans-serif;
  }
  .commitment {
    display: grid;
    gap: 6px;
    min-width: 0;
    color: var(--ink-soft);
    font-size: 0.76rem;
    font-weight: 700;
  }
  select,
  .license-choice {
    min-width: 0;
    min-height: 38px;
    box-sizing: border-box;
    color: var(--ink);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 4px;
  }
  select {
    width: 100%;
    padding: 7px 9px;
  }
  .license-choice {
    display: flex;
    align-items: center;
    gap: 10px;
    align-self: end;
    padding: 7px 10px;
    cursor: pointer;
  }
  .license-choice:focus-within,
  select:focus {
    border-color: var(--azure);
    outline: 3px solid var(--azure-focus);
  }
  .license-choice input {
    flex: 0 0 auto;
    width: 17px;
    height: 17px;
    margin: 0;
    accent-color: var(--azure);
  }
  .license-choice span {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .license-choice strong {
    font-size: 0.78rem;
  }
  .license-choice small {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 400;
  }
  .availability-warning,
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
  .eligibility-warning a {
    color: inherit;
    font-weight: 700;
  }
  @media (max-width: 620px) {
    .vm-purchase-plan {
      grid-template-columns: 1fr;
    }
    .availability-warning,
    .eligibility-warning {
      grid-column: 1;
    }
  }
</style>
