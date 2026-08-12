<script lang="ts">
  import { AlertTriangle, CheckCircle2, CircleHelp, DatabaseZap } from 'lucide-svelte';
  import {
    asRecord,
    formatMoney,
    readNumber,
    readRecord,
    readRecords,
    readString,
    type JsonRecord
  } from '$lib/api';
  import { relevantCalculationWarnings } from '$lib/calculation-warnings';
  import type { ResourceDraft } from '$lib/draft';

  let { calculation, resources }: { calculation: unknown; resources: ResourceDraft[] } = $props();

  const revision = $derived(asRecord(calculation));
  const portfolio = $derived(readRecord(revision, 'portfolio_totals'));
  const resourceResults = $derived(readRecords(revision, 'resource_results'));
  const sourcePortfolioTotal = $derived(readString(portfolio, 'aws_all_rows_total'));
  const azurePortfolioTotal = $derived(readString(portfolio, 'portfolio_after_selected_parity'));
  const portfolioDifference = $derived(readString(portfolio, 'portfolio_difference'));
  const comparableResourceCount = $derived(readNumber(portfolio, 'comparable_resource_count') ?? 0);
  const priceUnavailableResourceCount = $derived(
    readNumber(portfolio, 'price_unavailable_resource_count') ?? 0
  );
  const noComparablePrices = $derived(
    comparableResourceCount === 0 && priceUnavailableResourceCount > 0
  );
  const warnings = $derived(relevantCalculationWarnings(revision?.warnings));

  function resourceName(result: JsonRecord): string {
    const id = readString(result, 'resource_id');
    return resources.find((resource) => resource.id === id)?.workload_name ?? 'Workload';
  }

  function label(value: string | null): string {
    return value ? value.replaceAll('_', ' ') : 'unavailable';
  }

  function excludedPriceMessage(): string {
    if (noComparablePrices) return 'No workloads have complete source and Azure pricing.';
    const noun = priceUnavailableResourceCount === 1 ? 'workload' : 'workloads';
    return `${priceUnavailableResourceCount} ${noun} excluded because pricing is unavailable.`;
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
      <strong class:unavailable={noComparablePrices}
        >{noComparablePrices ? 'PRICE UNAVAILABLE' : formatMoney(azurePortfolioTotal)}</strong
      >
      {#if priceUnavailableResourceCount > 0}
        <small class="metric-error">{excludedPriceMessage()}</small>
      {:else}
        <small>Mapped rows after selected parity</small>
      {/if}
    </div>
    <div class="total-block difference">
      <span>Annual difference</span>
      <strong class:unavailable={noComparablePrices}
        >{noComparablePrices ? 'PRICE UNAVAILABLE' : formatMoney(portfolioDifference)}</strong
      >
      {#if priceUnavailableResourceCount > 0}
        <small class="metric-error">{excludedPriceMessage()}</small>
      {:else}
        <small>Source minus Azure</small>
      {/if}
    </div>
  </div>

  <div class="comparison-meta">
    <span><b>{comparableResourceCount}</b> comparable</span>
    <span><b>{readNumber(portfolio, 'no_mapping_resource_count') ?? 0}</b> no mapping</span>
    <span class:unavailable={priceUnavailableResourceCount > 0}
      ><b>{priceUnavailableResourceCount}</b> price unavailable</span
    >
  </div>

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
      {@const azureCosts = readRecord(result, 'azure_costs')}
      {@const savings = readRecord(result, 'savings')}
      {@const targetSelection = readRecord(result, 'target_selection')}
      {@const selected = readRecord(targetSelection, 'selected')}
      {@const unresolved = readRecords(result, 'unresolved_components')}
      {@const reasons = readRecords(targetSelection, 'outcome_reasons')}
      {@const steps = readRecords(result, 'explanation_steps')}
      {@const awsUnavailable = readString(result, 'aws_pricing_status') === 'unavailable'}
      {@const azureUnavailable = readString(result, 'azure_pricing_status') === 'unavailable'}
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
              <span>{readString(result, 'resource_id')}</span>
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
              >{formatMoney(azureAnnual)}</strong
            >
            {#if azureUnavailable}<small class="metric-error">Azure price is unavailable.</small
              >{/if}
          </div>
          <div>
            <span>Estimated savings</span><strong class:unavailable={pricingIncomplete}
              >{formatMoney(totalSavings)}</strong
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
    background: #edf3f1;
    border-top: 3px solid #087f73;
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
    color: #50716c;
    font:
      700 0.7rem/1.2 Bahnschrift,
      sans-serif;
    text-transform: uppercase;
  }
  h2 {
    margin: 0;
    color: #153236;
    font:
      700 1.35rem/1.2 Bahnschrift,
      sans-serif;
  }
  .formula {
    color: #587075;
    font-size: 0.78rem;
  }
  .totals-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border: 1px solid #b9cac7;
  }
  .total-block {
    display: grid;
    gap: 4px;
    min-height: 112px;
    align-content: center;
    padding: 17px 19px;
    background: #fff;
  }
  .total-block + .total-block {
    border-left: 1px solid #b9cac7;
  }
  .total-block span {
    color: #596d71;
    font-size: 0.78rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .total-block strong {
    overflow-wrap: anywhere;
    color: #172e33;
    font:
      700 clamp(1.15rem, 3vw, 1.75rem)/1.2 Bahnschrift,
      sans-serif;
  }
  .total-block strong.unavailable,
  .result-costs strong.unavailable {
    color: #b42318;
  }
  .total-block small {
    color: #6b7d80;
  }
  .metric-error {
    color: #b42318 !important;
    font-size: 0.75rem;
    font-weight: 700;
    line-height: 1.3;
  }
  .total-block.azure {
    background: #e1f1ed;
  }
  .total-block.difference {
    background: #fff8e4;
  }
  .comparison-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
    padding: 10px 12px;
    color: #506367;
    background: #dce8e5;
    font-size: 0.8rem;
  }
  .comparison-meta .unavailable {
    color: #b42318;
    font-weight: 700;
  }
  .warnings {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 9px;
    margin-top: 14px;
    padding: 11px 13px;
    color: #6b4c00;
    background: #fff5d5;
    border: 1px solid #e5c66b;
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
    background: #fff;
    border: 1px solid #becdca;
  }
  .result-row > header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 15px;
    background: #f8faf9;
    border-bottom: 1px solid #d6e0de;
  }
  .result-name {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    color: #087f73;
  }
  h3 {
    margin: 0;
    color: #172f34;
    font:
      650 0.95rem/1.25 Bahnschrift,
      sans-serif;
  }
  .result-name span {
    display: block;
    overflow: hidden;
    max-width: 280px;
    color: #778689;
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
    color: #5c4540;
    background: #f8e7e2;
    border: 1px solid #e7c1b8;
    border-radius: 3px;
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .status.ok {
    color: #166249;
    background: #dff2e9;
    border-color: #a4d6c4;
  }
  .price-status {
    color: #44595d;
    background: #edf1f1;
    border-color: #d1dada;
  }
  .price-status.unavailable {
    color: #8f1d16;
    background: #fff0ee;
    border-color: #efb0aa;
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
    color: #687a7e;
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .result-costs strong {
    overflow-wrap: anywhere;
    color: #20383c;
    font:
      650 1.05rem/1.25 Bahnschrift,
      sans-serif;
  }
  .target-strip {
    display: grid;
    grid-template-columns: 2fr repeat(4, 1fr);
    gap: 1px;
    background: #d8e1df;
    border-top: 1px solid #d8e1df;
    border-bottom: 1px solid #d8e1df;
  }
  .target-strip > div {
    display: grid;
    gap: 4px;
    padding: 10px 12px;
    background: #f1f6f4;
  }
  .target-strip strong {
    color: #20383c;
    font-size: 0.82rem;
    text-transform: capitalize;
  }
  .issues {
    padding: 11px 15px;
    color: #8f1d16;
    background: #fff0ee;
    border-bottom: 1px solid #efb0aa;
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
    color: #42565a;
    font-size: 0.82rem;
  }
  summary {
    color: #245951;
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
    color: #233d41;
    text-transform: capitalize;
  }
  @media (max-width: 760px) {
    .totals-grid,
    .result-costs {
      grid-template-columns: 1fr;
    }
    .total-block + .total-block {
      border-top: 1px solid #b9cac7;
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
