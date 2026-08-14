<script lang="ts">
  import { AlertTriangle, CheckCircle2, CircleHelp, DatabaseZap, Info } from 'lucide-svelte';
  import {
    asRecord,
    formatMoney,
    readNumber,
    readRecord,
    readRecords,
    readString,
    type JsonRecord
  } from '$lib/api';
  import { calculationTargetOutcome } from '$lib/calculation-outcome';
  import { relevantCalculationWarnings } from '$lib/calculation-warnings';
  import type { ResourceDraft } from '$lib/draft';
  import {
    commitmentDiscount,
    formatAppliedDiscount,
    hasMiCommitment,
    miPurchaseOptionLabel,
    miPurchaseOptionParts,
    type PurchaseOptionDiscounts
  } from '$lib/mi-purchase-options';

  let { calculation, resources }: { calculation: unknown; resources: ResourceDraft[] } = $props();

  const revision = $derived(asRecord(calculation));
  const portfolio = $derived(readRecord(revision, 'portfolio_totals'));
  const resourceResults = $derived(readRecords(revision, 'resource_results'));
  const sourcePortfolioTotal = $derived(readString(portfolio, 'aws_all_rows_total'));
  const azurePortfolioTotal = $derived(readString(portfolio, 'portfolio_after_selected_parity'));
  const portfolioDifference = $derived(readString(portfolio, 'portfolio_difference'));
  const comparableResourceCount = $derived(readNumber(portfolio, 'comparable_resource_count') ?? 0);
  const noMappingResourceCount = $derived(readNumber(portfolio, 'no_mapping_resource_count') ?? 0);
  const priceUnavailableResourceCount = $derived(
    readNumber(portfolio, 'price_unavailable_resource_count') ?? 0
  );
  const targetOutcome = $derived(
    calculationTargetOutcome(
      comparableResourceCount,
      noMappingResourceCount,
      priceUnavailableResourceCount
    )
  );
  const warnings = $derived(relevantCalculationWarnings(revision?.warnings));
  const hasCommittedPlans = $derived(
    resources.some((resource) => hasMiCommitment(resource.mi_purchase_option))
  );

  function sourceResource(result: JsonRecord): ResourceDraft | undefined {
    const id = readString(result, 'resource_id');
    return resources.find((resource) => resource.id === id);
  }

  function resourceName(result: JsonRecord): string {
    return sourceResource(result)?.workload_name ?? 'Workload';
  }

  function resourceIdentifier(result: JsonRecord): string | null {
    const serverName = sourceResource(result)?.server_name?.trim();
    return serverName || readString(result, 'resource_id');
  }

  function purchaseOptionDiscounts(result: JsonRecord): PurchaseOptionDiscounts | null {
    const discounts = readRecord(result, 'purchase_option_discounts');
    const payg = readString(discounts, 'payg');
    const oneYearReserved = readString(discounts, 'one_year_reserved');
    const threeYearReserved = readString(discounts, 'three_year_reserved');
    const oneYearSavingsPlan = readString(discounts, 'one_year_savings_plan');
    const azureHybridBenefit = readString(discounts, 'azure_hybrid_benefit');
    if (
      payg === null ||
      oneYearReserved === null ||
      threeYearReserved === null ||
      oneYearSavingsPlan === null ||
      azureHybridBenefit === null
    ) {
      return null;
    }
    return {
      payg,
      one_year_reserved: oneYearReserved,
      three_year_reserved: threeYearReserved,
      one_year_savings_plan: oneYearSavingsPlan,
      azure_hybrid_benefit: azureHybridBenefit
    };
  }

  function appliedDiscountLabel(
    resource: ResourceDraft,
    discounts: PurchaseOptionDiscounts
  ): string {
    const option = miPurchaseOptionParts(resource.mi_purchase_option);
    const labels = [
      `${formatAppliedDiscount(commitmentDiscount(option.commitment, discounts))} compute discount`
    ];
    if (option.usesAzureHybridBenefit) {
      labels.push(`${formatAppliedDiscount(discounts.azure_hybrid_benefit)} AHB license discount`);
    }
    return labels.join(' · ');
  }

  function label(value: string | null): string {
    return value ? value.replaceAll('_', ' ') : 'unavailable';
  }

  function excludedPriceMessage(): string {
    if (targetOutcome === 'price_unavailable')
      return 'No workloads have complete source and Azure pricing.';
    const noun = priceUnavailableResourceCount === 1 ? 'workload' : 'workloads';
    return `${priceUnavailableResourceCount} ${noun} excluded because pricing is unavailable.`;
  }

  function targetTotal(value: string | null): string {
    if (targetOutcome === 'no_mapping') return 'NO MAPPING';
    if (targetOutcome === 'price_unavailable') return 'PRICE UNAVAILABLE';
    return formatMoney(value);
  }
</script>

<section class="results" aria-labelledby="results-heading">
  <div class="results-heading">
    <div>
      <span class="eyebrow">Server-calculated estimate</span>
      <h2 id="results-heading">Annual comparison</h2>
    </div>
    {#if readString(revision, 'formula_version')}
      <span class="formula">Formula {readString(revision, 'formula_version')}</span>
    {/if}
  </div>

  <div class="totals-grid">
    <div class="total-block source">
      <span>Source estate</span>
      <strong class:unavailable={sourcePortfolioTotal === null}
        >{formatMoney(sourcePortfolioTotal)}</strong
      >
      {#if sourcePortfolioTotal === null}
        <small class="metric-error">One or more source prices are unavailable.</small>
      {:else}
        <small>All price-resolved rows</small>
      {/if}
    </div>
    <div class="total-block azure">
      <span>Azure SQL MI</span>
      <strong class:unavailable={targetOutcome !== 'available'}
        >{targetTotal(azurePortfolioTotal)}</strong
      >
      {#if targetOutcome === 'no_mapping'}
        <small class="metric-error">No workload fits an approved Azure SQL MI target.</small>
      {:else if priceUnavailableResourceCount > 0}
        <small class="metric-error">{excludedPriceMessage()}</small>
      {:else}
        <small>Mapped rows after selected parity</small>
      {/if}
    </div>
    <div class="total-block difference">
      <span>Annual difference</span>
      <strong class:unavailable={targetOutcome !== 'available'}
        >{targetTotal(portfolioDifference)}</strong
      >
      {#if targetOutcome === 'no_mapping'}
        <small class="metric-error">A comparison requires at least one mapped workload.</small>
      {:else if priceUnavailableResourceCount > 0}
        <small class="metric-error">{excludedPriceMessage()}</small>
      {:else}
        <small>Source minus Azure</small>
      {/if}
    </div>
  </div>

  <div class="comparison-meta">
    <span><b>{comparableResourceCount}</b> comparable</span>
    <span><b>{noMappingResourceCount}</b> no mapping</span>
    <span class:unavailable={priceUnavailableResourceCount > 0}
      ><b>{priceUnavailableResourceCount}</b> price unavailable</span
    >
  </div>

  {#if hasCommittedPlans}
    <div class="pricing-assumption" role="note">
      <Info size={18} aria-hidden="true" />
      <p>
        <strong>Commitment assumption.</strong> These estimates apply effective hourly rates only to the
        workload hours entered. Reservations and savings plans are commitments, so actual charges can
        be higher when committed capacity is unused.
      </p>
    </div>
  {/if}

  {#if warnings.length > 0}
    <div class="warnings" role="status">
      <AlertTriangle size={18} aria-hidden="true" />
      <div>
        {#each warnings as warning, index (index)}<p>{warning}</p>{/each}
      </div>
    </div>
  {/if}

  <div class="resource-results">
    {#each resourceResults as result (readString(result, 'resource_id'))}
      {@const sourceCosts = readRecord(result, 'source_costs')}
      {@const resource = sourceResource(result)}
      {@const discounts = purchaseOptionDiscounts(result)}
      {@const azureCosts = readRecord(result, 'azure_costs')}
      {@const savings = readRecord(result, 'savings')}
      {@const targetSelection = readRecord(result, 'target_selection')}
      {@const selected = readRecord(targetSelection, 'selected')}
      {@const unresolved = readRecords(result, 'unresolved_components')}
      {@const reasons = readRecords(targetSelection, 'outcome_reasons')}
      {@const steps = readRecords(result, 'explanation_steps')}
      {@const awsUnavailable = readString(result, 'aws_pricing_status') === 'unavailable'}
      {@const azureUnavailable = readString(result, 'azure_pricing_status') === 'unavailable'}
      {@const noMapping = readString(result, 'mapping_status') === 'no_mapping'}
      {@const pricingIncomplete = awsUnavailable || azureUnavailable}
      {@const sourceAnnual = readString(sourceCosts, 'total')}
      {@const azureAnnual = readString(azureCosts, 'total_before_parity')}
      {@const totalSavings = readString(savings, 'total_savings')}
      <article class="result-row">
        <header>
          <div class="result-name">
            <DatabaseZap size={19} aria-hidden="true" />
            <div>
              <h3>{resourceName(result)}</h3>
              <span>{resourceIdentifier(result)}</span>
            </div>
          </div>
          <div class="status-line">
            <span class:ok={readString(result, 'mapping_status') === 'mapped'} class="status">
              {#if readString(result, 'mapping_status') === 'mapped'}<CheckCircle2
                  size={14}
                />{:else}<CircleHelp size={14} />{/if}
              {label(readString(result, 'mapping_status'))}
            </span>
            <span class="price-status" class:unavailable={awsUnavailable}
              >AWS {label(readString(result, 'aws_pricing_status'))}</span
            >
            <span class="price-status" class:unavailable={azureUnavailable}
              >Azure {label(readString(result, 'azure_pricing_status'))}</span
            >
          </div>
        </header>

        <div class="result-costs">
          <div>
            <span>Source annual</span><strong class:unavailable={awsUnavailable}
              >{formatMoney(sourceAnnual)}</strong
            >
            {#if awsUnavailable}<small class="metric-error">Source price is unavailable.</small
              >{/if}
          </div>
          <div>
            <span>Azure annual</span><strong class:unavailable={azureUnavailable}
              >{noMapping ? 'NO MAPPING' : formatMoney(azureAnnual)}</strong
            >
            {#if resource}<small class="pricing-plan"
                >{miPurchaseOptionLabel(resource.mi_purchase_option)}</small
              >{/if}
            {#if resource && discounts}<small class="applied-discount"
                >{appliedDiscountLabel(resource, discounts)}</small
              >{/if}
            {#if azureUnavailable}<small class="metric-error">Azure price is unavailable.</small
              >{/if}
          </div>
          <div>
            <span>Estimated savings</span><strong class:unavailable={pricingIncomplete}
              >{noMapping ? 'NO MAPPING' : formatMoney(totalSavings)}</strong
            >
            {#if pricingIncomplete}<small class="metric-error"
                >Savings require complete source and Azure prices.</small
              >{/if}
          </div>
        </div>

        {#if selected}
          <div class="target-strip">
            <div>
              <span>Recommended target</span><strong
                >{label(readString(selected, 'service_tier'))}</strong
              >
            </div>
            <div><span>vCores</span><strong>{readNumber(selected, 'vcores')}</strong></div>
            <div>
              <span>Selected memory</span><strong
                >{readString(selected, 'selected_memory_gb')} GiB</strong
              >
            </div>
            <div>
              <span>Additional memory</span><strong
                >{readString(selected, 'additional_memory_gb')} GiB</strong
              >
            </div>
            <div>
              <span>Hardware</span><strong>{readString(selected, 'hardware_family')}</strong>
            </div>
          </div>
        {:else if noMapping}
          <div class="target-strip">
            <div><span>Recommended target</span><strong>NO MAPPING</strong></div>
            <div><span>vCores</span><strong>NO MAPPING</strong></div>
            <div><span>Selected memory</span><strong>NO MAPPING</strong></div>
            <div><span>Additional memory</span><strong>NO MAPPING</strong></div>
            <div><span>Hardware</span><strong>NO MAPPING</strong></div>
          </div>
        {/if}

        {#if unresolved.length > 0 || reasons.length > 0}
          <div class="issues" role="alert">
            {#each unresolved as item, index (index)}<p>
                <AlertTriangle size={15} />
                {readString(item, 'message')}
              </p>{/each}
            {#each reasons as reason, index (index)}<p>
                <CircleHelp size={15} />
                {readString(reason, 'detail')}
              </p>{/each}
          </div>
        {/if}

        {#if steps.length > 0}
          <details>
            <summary>Calculation explanation</summary>
            <ol>
              {#each steps as step, index (index)}
                <li>
                  <b>{label(readString(step, 'code'))}</b><span>{readString(step, 'message')}</span>
                </li>
              {/each}
            </ol>
          </details>
        {/if}
      </article>
    {/each}
  </div>
</section>

<style>
  .results {
    padding: 22px;
    background: color-mix(in srgb, var(--azure) 9%, var(--surface));
    border-top: 3px solid var(--azure);
  }
  .results-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }
  .eyebrow {
    display: block;
    margin-bottom: 4px;
    color: var(--muted);
    font:
      700 0.7rem/1.2 Bahnschrift,
      sans-serif;
    text-transform: uppercase;
  }
  h2 {
    margin: 0;
    color: var(--ink-strong);
    font:
      700 1.35rem/1.2 Bahnschrift,
      sans-serif;
  }
  .formula {
    color: var(--muted);
    font-size: 0.78rem;
  }
  .totals-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border: 1px solid var(--border);
  }
  .total-block {
    display: grid;
    gap: 4px;
    min-height: 112px;
    align-content: center;
    padding: 17px 19px;
    background: var(--surface);
  }
  .total-block + .total-block {
    border-left: 1px solid var(--border);
  }
  .total-block span {
    color: var(--muted);
    font-size: 0.78rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .total-block strong {
    overflow-wrap: anywhere;
    color: var(--ink-strong);
    font:
      700 clamp(1.15rem, 3vw, 1.75rem)/1.2 Bahnschrift,
      sans-serif;
  }
  .total-block strong.unavailable,
  .result-costs strong.unavailable {
    color: var(--danger);
  }
  .total-block small {
    color: var(--muted);
  }
  .metric-error {
    color: var(--danger) !important;
    font-size: 0.75rem;
    font-weight: 700;
    line-height: 1.3;
  }
  .total-block.azure {
    background: var(--azure-surface);
  }
  .total-block.difference {
    background: var(--warning-surface);
  }
  .comparison-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
    padding: 10px 12px;
    color: var(--ink-soft);
    background: var(--comparison-surface);
    font-size: 0.8rem;
  }
  .comparison-meta .unavailable {
    color: var(--danger);
    font-weight: 700;
  }
  .warnings {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 9px;
    margin-top: 14px;
    padding: 11px 13px;
    color: var(--warning-text);
    background: var(--warning-surface);
    border: 1px solid var(--warning-border);
  }
  .pricing-assumption {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 9px;
    margin-top: 14px;
    padding: 11px 13px;
    color: #44595d;
    background: #f8faf9;
    border: 1px solid #c8d5d3;
  }
  .pricing-assumption p {
    margin: 0;
    font-size: 0.8rem;
    line-height: 1.4;
  }
  .warnings p {
    margin: 0 0 3px;
  }
  .resource-results {
    display: grid;
    gap: 12px;
    margin-top: 16px;
  }
  .result-row {
    overflow: hidden;
    background: color-mix(in srgb, var(--azure) 4%, var(--surface));
    border: 1px solid var(--border);
  }
  .result-row > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 15px;
    background: var(--surface-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }
  .result-name {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    color: var(--azure);
  }
  h3 {
    margin: 0;
    color: var(--ink-strong);
    font:
      650 0.95rem/1.25 Bahnschrift,
      sans-serif;
  }
  .result-name span {
    display: block;
    overflow: hidden;
    max-width: 280px;
    color: var(--muted);
    font-size: 0.66rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-line {
    display: flex;
    flex-wrap: wrap;
    justify-content: end;
    gap: 6px;
  }
  .status,
  .price-status {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 7px;
    color: var(--danger-text);
    background: var(--danger-surface);
    border: 1px solid var(--danger-border);
    border-radius: 3px;
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .status.ok {
    color: var(--success);
    background: var(--success-surface);
    border-color: var(--success-border);
  }
  .price-status {
    color: var(--ink-soft);
    background: var(--surface-muted);
    border-color: var(--border);
  }
  .price-status.unavailable {
    color: var(--danger-text);
    background: var(--danger-surface);
    border-color: var(--danger-border);
  }
  .result-costs {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    padding: 15px;
  }
  .result-costs > div {
    display: grid;
    gap: 3px;
    padding-right: 16px;
  }
  .result-costs span,
  .target-strip span {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .result-costs strong {
    overflow-wrap: anywhere;
    color: var(--ink-strong);
    font:
      650 1.05rem/1.25 Bahnschrift,
      sans-serif;
  }
  .result-costs .pricing-plan {
    color: #3f6661;
    font-size: 0.74rem;
    font-weight: 650;
  }
  .result-costs .applied-discount {
    color: var(--azure-text);
    font-size: 0.72rem;
    font-weight: 700;
  }
  .target-strip {
    display: grid;
    grid-template-columns: 2fr repeat(4, 1fr);
    gap: 1px;
    background: var(--border);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  .target-strip > div {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    background: var(--surface-subtle);
  }
  .target-strip strong {
    color: var(--ink-strong);
    font-size: 0.82rem;
    text-transform: capitalize;
  }
  .issues {
    padding: 11px 15px;
    color: var(--danger-text);
    background: var(--danger-surface);
    border-bottom: 1px solid var(--danger-border);
  }
  .issues p {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 3px 0;
    font-size: 0.82rem;
  }
  details {
    padding: 10px 15px 13px;
    color: var(--ink-soft);
    font-size: 0.82rem;
  }
  summary {
    color: var(--azure-text);
    font-weight: 700;
    cursor: pointer;
  }
  ol {
    display: grid;
    gap: 8px;
    margin: 11px 0 0;
    padding-left: 20px;
  }
  li b,
  li span {
    display: block;
  }
  li b {
    color: var(--ink-strong);
    text-transform: capitalize;
  }
  @media (max-width: 760px) {
    .totals-grid,
    .result-costs {
      grid-template-columns: 1fr;
    }
    .total-block + .total-block {
      border-top: 1px solid var(--border);
      border-left: 0;
    }
    .result-costs {
      gap: 13px;
    }
    .target-strip {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .result-row > header,
    .results-heading {
      align-items: flex-start;
      flex-direction: column;
    }
    .status-line {
      justify-content: start;
    }
  }
</style>
