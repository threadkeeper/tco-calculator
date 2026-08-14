<script lang="ts">
  import { tick } from 'svelte';
  import { AlertTriangle, Check, ChevronDown, Info, X } from 'lucide-svelte';
  import type { PurchaseOption } from '$lib/draft';
  import {
    commitmentDiscount,
    formatAppliedDiscount,
    MI_COMMITMENT_OPTIONS,
    miPurchaseOption,
    miPurchaseOptionParts,
    type MiCommitment,
    type PurchaseOptionDiscounts
  } from '$lib/mi-purchase-options';

  let {
    id,
    legend,
    value = $bindable(),
    discounts = null,
    onchange = () => {}
  }: {
    id: string;
    legend: string;
    value: PurchaseOption;
    discounts?: PurchaseOptionDiscounts | null;
    onchange?: () => void;
  } = $props();

  type PurchaseHelp = {
    title: string;
    summary: string;
    discount: string;
    details: string;
  };

  const AHB_HELP: PurchaseHelp = {
    title: 'Azure Hybrid Benefit',
    summary:
      'Bring eligible SQL Server licenses you already own instead of paying for the SQL license again in Azure.',
    discount:
      'Microsoft says Azure Hybrid Benefit can save up to 55% on SQL Managed Instance. Combined with a reservation, total savings can reach up to 82%.',
    details:
      'You need qualifying SQL Server licenses with active Software Assurance or an eligible subscription. The calculator does not verify entitlement, so confirm it with the customer licensing team before using this option.'
  };

  let menuOpen = $state(false);
  let help = $state<PurchaseHelp | null>(null);
  let trigger: HTMLButtonElement;
  let closeButton = $state<HTMLButtonElement>();
  let returnFocus: HTMLElement | null = null;

  const selected = $derived(miPurchaseOptionParts(value));
  const selectedOption = $derived(
    MI_COMMITMENT_OPTIONS.find((option) => option.value === selected.commitment) ??
      MI_COMMITMENT_OPTIONS[0]
  );

  function selectCommitment(commitment: MiCommitment) {
    value = miPurchaseOption(commitment, selected.usesAzureHybridBenefit);
    menuOpen = false;
    onchange();
    trigger.focus();
  }

  function selectLicense(event: Event) {
    const usesAzureHybridBenefit = (event.currentTarget as HTMLInputElement).checked;
    value = miPurchaseOption(selected.commitment, usesAzureHybridBenefit);
    onchange();
  }

  async function showHelp(event: MouseEvent, nextHelp: PurchaseHelp) {
    returnFocus = menuOpen ? trigger : (event.currentTarget as HTMLElement);
    menuOpen = false;
    help = nextHelp;
    await tick();
    closeButton?.focus();
  }

  async function hideHelp() {
    help = null;
    await tick();
    returnFocus?.focus();
  }

  function handleFocusOut(event: FocusEvent) {
    const container = event.currentTarget as HTMLElement;
    if (event.relatedTarget instanceof Node && container.contains(event.relatedTarget)) return;
    menuOpen = false;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape') return;
    if (help) {
      event.preventDefault();
      void hideHelp();
    } else if (menuOpen) {
      event.preventDefault();
      menuOpen = false;
      trigger.focus();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<fieldset class="purchase-plan">
  <legend>{legend}</legend>
  <div class="commitment" onfocusout={handleFocusOut}>
    <span id={`${id}-commitment-label`}>Compute commitment</span>
    <div class="plan-select">
      <button
        bind:this={trigger}
        id={`${id}-commitment`}
        type="button"
        class="plan-trigger"
        aria-labelledby={`${id}-commitment-label ${id}-commitment-value`}
        aria-controls={`${id}-commitment-options`}
        aria-expanded={menuOpen}
        onclick={() => (menuOpen = !menuOpen)}
      >
        <span id={`${id}-commitment-value`}
          >{selectedOption.label}{discounts
            ? ` · ${formatAppliedDiscount(commitmentDiscount(selected.commitment, discounts))}`
            : ''}</span
        >
        <ChevronDown size={17} aria-hidden="true" />
      </button>
      {#if menuOpen}
        <ul id={`${id}-commitment-options`} aria-label="Compute commitment options">
          {#each MI_COMMITMENT_OPTIONS as option (option.value)}
            <li>
              <button
                type="button"
                class="option-choice"
                aria-pressed={option.value === selected.commitment}
                onclick={() => selectCommitment(option.value)}
              >
                <span
                  >{option.label}{discounts
                    ? ` · ${formatAppliedDiscount(commitmentDiscount(option.value, discounts))} discount`
                    : ''}</span
                >
                {#if option.value === selected.commitment}<Check
                    size={17}
                    aria-hidden="true"
                  />{/if}
              </button>
              <button
                type="button"
                class="info-button"
                aria-label={`About ${option.label}`}
                title={`About ${option.label}`}
                onclick={(event) =>
                  showHelp(event, {
                    title: option.label,
                    summary: option.summary,
                    discount: option.discount,
                    details: option.details
                  })}
              >
                <Info size={17} aria-hidden="true" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
  <div class="license-row">
    <label class="license-choice" for={`${id}-ahb`}>
      <input
        id={`${id}-ahb`}
        type="checkbox"
        checked={selected.usesAzureHybridBenefit}
        aria-describedby={selected.usesAzureHybridBenefit ? `${id}-ahb-warning` : undefined}
        onchange={selectLicense}
      />
      <span class="license-copy">
        <strong
          >Azure Hybrid Benefit{discounts
            ? ` · ${formatAppliedDiscount(discounts.azure_hybrid_benefit)} discount`
            : ''}</strong
        >
        <span
          >{selected.usesAzureHybridBenefit
            ? 'Eligible licenses applied'
            : 'SQL license included'}</span
        >
      </span>
    </label>
    <button
      type="button"
      class="info-button ahb-info"
      aria-label="About Azure Hybrid Benefit"
      title="About Azure Hybrid Benefit"
      onclick={(event) => showHelp(event, AHB_HELP)}
    >
      <Info size={17} aria-hidden="true" />
    </button>
  </div>
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

{#if help}
  <div class="backdrop" role="presentation">
    <div
      class="help-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby={`${id}-help-title`}
      aria-describedby={`${id}-help-summary ${id}-help-discount ${id}-help-details`}
    >
      <div class="dialog-heading">
        <div>
          <span class="eyebrow">Pricing explained</span>
          <h2 id={`${id}-help-title`}>{help.title}</h2>
        </div>
        <button
          bind:this={closeButton}
          type="button"
          class="dialog-close"
          aria-label="Close pricing explanation"
          title="Close"
          onclick={hideHelp}
        >
          <X size={19} aria-hidden="true" />
        </button>
      </div>
      <p id={`${id}-help-summary`}>{help.summary}</p>
      <p id={`${id}-help-discount`} class="discount"><strong>Discount</strong>{help.discount}</p>
      <p id={`${id}-help-details`} class="details">{help.details}</p>
      <p class="estimate-note">
        This calculator provides an estimate, not a quote. Actual eligibility and billed rates can
        vary.
      </p>
      <div class="dialog-actions">
        <button type="button" class="done" onclick={hideHelp}>Got it</button>
      </div>
    </div>
  </div>
{/if}

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
  .plan-select {
    position: relative;
  }
  .plan-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
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
    text-align: left;
    cursor: pointer;
  }
  .plan-trigger:hover {
    background: color-mix(in srgb, var(--copilot-purple-light) 12%, var(--surface-input));
    border-color: var(--copilot-purple);
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(200 152 253 / 18%),
      0 0 18px rgb(133 52 243 / 26%);
  }
  .plan-trigger:focus {
    border-color: var(--copilot-purple);
    outline: 3px solid rgb(200 152 253 / 32%);
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(200 152 253 / 24%),
      0 0 20px rgb(133 52 243 / 30%);
  }
  ul {
    position: absolute;
    z-index: 30;
    top: calc(100% + 4px);
    right: 0;
    left: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border: 1px solid var(--border-input);
    border-radius: 4px;
    box-shadow: 0 10px 24px rgb(8 20 26 / 24%);
  }
  li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 38px;
    align-items: stretch;
    gap: 2px;
  }
  .option-choice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 0;
    min-height: 42px;
    padding: 7px 9px;
    color: var(--ink);
    background: transparent;
    border: 0;
    border-radius: 3px;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option-choice:hover,
  .option-choice:focus,
  .option-choice[aria-pressed='true'] {
    background: var(--azure-soft);
    outline: none;
  }
  .info-button {
    display: grid;
    width: 36px;
    min-width: 36px;
    min-height: 36px;
    padding: 0;
    place-items: center;
    color: var(--azure-text);
    background: transparent;
    border: 0;
    border-radius: 3px;
    cursor: pointer;
  }
  .info-button:hover,
  .info-button:focus {
    color: var(--ink);
    background: var(--azure-soft);
    outline: 2px solid var(--azure-focus);
  }
  .license-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 38px;
    align-items: stretch;
    min-width: 0;
  }
  .license-choice {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    min-height: 38px;
    box-sizing: border-box;
    padding: 7px 10px;
    color: var(--copilot-ink);
    background: var(--copilot-surface);
    border: 1px solid color-mix(in srgb, var(--copilot-purple) 62%, var(--border-input));
    border-radius: 4px;
    box-shadow:
      inset 3px 0 0 var(--copilot-purple),
      0 0 0 1px rgb(133 52 243 / 10%),
      0 0 14px rgb(133 52 243 / 18%);
    cursor: pointer;
  }
  .license-choice:hover {
    background: color-mix(in srgb, var(--copilot-purple-light) 12%, var(--surface-input));
    border-color: var(--copilot-purple);
  }
  .ahb-info {
    width: 38px;
    min-width: 38px;
    border: 1px solid var(--border-input);
    border-left: 0;
    border-radius: 0 4px 4px 0;
  }
  .license-row .license-choice {
    border-radius: 4px 0 0 4px;
  }
  .license-choice:focus-within {
    border-color: var(--copilot-purple);
    outline: 3px solid rgb(200 152 253 / 32%);
  }
  .license-choice input {
    flex: 0 0 auto;
    width: 17px;
    height: 17px;
    margin: 0;
    accent-color: var(--copilot-purple);
  }
  .license-copy {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .license-copy strong {
    color: var(--copilot-ink);
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
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    padding: 16px;
    place-items: center;
    background: rgb(5 14 18 / 68%);
  }
  .help-dialog {
    width: min(100%, 520px);
    max-height: min(680px, calc(100vh - 32px));
    padding: 20px;
    overflow-y: auto;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 22px 60px rgb(3 10 13 / 38%);
  }
  .dialog-heading {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .eyebrow {
    color: var(--azure-text);
    font-size: 0.7rem;
    font-weight: 750;
    text-transform: uppercase;
  }
  h2 {
    margin: 3px 0 0;
    font:
      700 1.25rem/1.2 Bahnschrift,
      sans-serif;
  }
  .dialog-close {
    display: grid;
    flex: 0 0 36px;
    width: 36px;
    height: 36px;
    padding: 0;
    place-items: center;
    color: var(--ink-soft);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 4px;
    cursor: pointer;
  }
  .help-dialog p {
    color: var(--ink-soft);
    font-size: 0.9rem;
    line-height: 1.5;
  }
  .discount {
    display: grid;
    gap: 3px;
    padding: 10px 12px;
    color: var(--ink) !important;
    background: var(--azure-soft);
    border-left: 3px solid var(--azure);
  }
  .discount strong {
    color: var(--azure-text);
    font-size: 0.72rem;
    text-transform: uppercase;
  }
  .details {
    margin-bottom: 0;
  }
  .estimate-note {
    color: var(--muted) !important;
    font-size: 0.76rem !important;
  }
  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }
  .done {
    min-height: 38px;
    padding: 8px 16px;
    color: white;
    background: var(--azure);
    border: 1px solid var(--azure-dark);
    border-radius: 4px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
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
