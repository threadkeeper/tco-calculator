<script lang="ts">
  import { BadgeDollarSign, ExternalLink, Gauge, Scale } from 'lucide-svelte';
  import { asRecord, formatMoney, readRecord, readString } from '$lib/api';
  import type { SqlPaygSettingsDraft } from '$lib/draft';

  let {
    settings,
    annualHours,
    appliedDiscount,
    calculation,
    onchange,
    onannualhourschange,
    onapplieddiscountchange
  }: {
    settings: SqlPaygSettingsDraft;
    annualHours: string;
    appliedDiscount: string;
    calculation: unknown | null;
    onchange: () => void;
    onannualhourschange: (value: string) => void;
    onapplieddiscountchange: (value: string) => void;
  } = $props();

  let utilizationUnit = $state<'monthly' | 'annual'>('annual');
  const analysis = $derived(readRecord(asRecord(calculation), 'sql_payg_analysis'));
  const utilizationValue = $derived(
    utilizationUnit === 'monthly' ? scaleInput(annualHours, 1 / 12) : annualHours
  );
  const discountPercent = $derived(scaleInput(appliedDiscount, 100));

  function scaleInput(value: string, factor: number): string {
    if (value.trim() === '') return '';
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) return value;
    return (parsed * factor).toFixed(6).replace(/\.?0+$/, '');
  }

  function updateUtilization(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    onannualhourschange(utilizationUnit === 'monthly' ? scaleInput(input.value, 12) : input.value);
  }

  function updateDiscount(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    onapplieddiscountchange(scaleInput(input.value, 1 / 100));
  }

  function formatPercent(value: string | null): string {
    if (value === null) return 'Unavailable';
    const rate = Number(value);
    if (!Number.isFinite(rate)) return value;
    return new Intl.NumberFormat('en-US', {
      style: 'percent',
      minimumFractionDigits: 1,
      maximumFractionDigits: 2
    }).format(rate);
  }

  function formatRate(value: string | null): string {
    if (value === null) return 'Unavailable';
    const rate = Number(value);
    if (!Number.isFinite(rate)) return value;
    return `${formatMoney(rate.toFixed(3))} / core-hour`;
  }

  function formatHours(value: string | null): string {
    if (value === null) return 'Unavailable';
    const hours = Number(value);
    if (!Number.isFinite(hours)) return value;
    return new Intl.NumberFormat('en-US', { maximumFractionDigits: 2 }).format(hours);
  }

  function comparisonLabel(value: string | null): string {
    if (value === null) return 'Annual comparison';
    const amount = value === null ? Number.NaN : Number(value);
    if (amount > 0) return 'Annual savings';
    if (amount < 0) return 'Annual overage';
    return 'Annual break-even';
  }

  function comparisonCopy(value: string | null): string {
    const amount = value === null ? Number.NaN : Number(value);
    if (amount > 0) return 'Net PAYG is below the entered annual SA spend.';
    if (amount < 0) return 'Net PAYG exceeds the entered annual SA spend.';
    if (amount === 0) return 'Net PAYG matches the entered annual SA spend.';
    return 'The comparison is unavailable.';
  }

  function comparisonTone(value: string | null): 'savings' | 'overage' | 'neutral' {
    const amount = value === null ? Number.NaN : Number(value);
    if (amount > 0) return 'savings';
    if (amount < 0) return 'overage';
    return 'neutral';
  }

  function formatComparisonMoney(value: string | null): string {
    if (value === null) return formatMoney(null);
    const amount = Number(value);
    if (!Number.isFinite(amount)) return value;
    const formatted = formatMoney(String(Math.abs(amount)));
    return amount > 0 ? `+${formatted}` : formatted;
  }

  function analysisString(key: string): string | null {
    return readString(analysis, key);
  }
</script>

<section class="licensing-intro" aria-labelledby="sql-payg-heading">
  <div>
    <span class="eyebrow">Azure Arc licensing comparison</span>
    <h2 id="sql-payg-heading">SQL Pay As You Go</h2>
    <p>
      Compare annual Software Assurance spend with an Azure Arc-enabled SQL Server PAYG run rate at
      the entered utilization and discount. The estimate is not a quote and does not determine
      licensing entitlement.
    </p>
  </div>
  <BadgeDollarSign class="intro-icon" size={34} aria-hidden="true" />
</section>

<section class="input-panel" aria-labelledby="license-inputs-heading">
  <div class="section-heading">
    <span class="eyebrow">License baseline</span>
    <h2 id="license-inputs-heading">Current estate</h2>
  </div>
  <div class="input-grid">
    <label>
      <span>SQL Enterprise licensed cores</span>
      <input
        type="number"
        min="0"
        max="100000"
        step="1"
        required
        bind:value={settings.enterprise_licensed_cores}
        oninput={onchange}
      />
    </label>
    <label>
      <span>SQL Standard licensed cores</span>
      <input
        type="number"
        min="0"
        max="100000"
        step="1"
        required
        bind:value={settings.standard_licensed_cores}
        oninput={onchange}
      />
    </label>
    <label>
      <span>Annual Software Assurance spend (USD)</span>
      <input
        type="number"
        min="0"
        step="0.01"
        required
        bind:value={settings.software_assurance_annual_usd}
        oninput={onchange}
      />
    </label>
    <fieldset class="utilization-field">
      <legend>PAYG utilization</legend>
      <div class="utilization-control">
        <div class="segmented" aria-label="Utilization period">
          <button
            type="button"
            class:active={utilizationUnit === 'monthly'}
            aria-pressed={utilizationUnit === 'monthly'}
            onclick={() => (utilizationUnit = 'monthly')}>Monthly</button
          >
          <button
            type="button"
            class:active={utilizationUnit === 'annual'}
            aria-pressed={utilizationUnit === 'annual'}
            onclick={() => (utilizationUnit = 'annual')}>Annual</button
          >
        </div>
        <label>
          <span>{utilizationUnit === 'monthly' ? 'Hours per month' : 'Hours per year'}</span>
          <input
            type="number"
            min="0"
            max={utilizationUnit === 'monthly' ? '732' : '8784'}
            step="0.01"
            required
            value={utilizationValue}
            oninput={updateUtilization}
          />
        </label>
      </div>
    </fieldset>
    <label>
      <span>Applied PAYG discount (%)</span>
      <input
        type="number"
        min="0"
        max="100"
        step="0.01"
        required
        value={discountPercent}
        oninput={updateDiscount}
      />
    </label>
  </div>
  <p class="input-note">
    Enter licensable cores after confirming edition, OSE scope, passive replicas, and applicable
    four-core minimums. Perpetual acquisition cost is excluded from this annual run-rate baseline.
  </p>
</section>

{#if analysis}
  <section class="result-panel" aria-labelledby="discount-result-heading">
    <div class="result-lead" data-tone={comparisonTone(analysisString('annual_savings_usd'))}>
      <div>
        <span class="eyebrow">Server-calculated comparison</span>
        <h2 id="discount-result-heading">
          {comparisonLabel(analysisString('annual_savings_usd'))}
        </h2>
      </div>
      <strong>{formatComparisonMoney(analysisString('annual_savings_usd'))}</strong>
      <p>{comparisonCopy(analysisString('annual_savings_usd'))}</p>
    </div>
    <dl class="result-grid">
      <div>
        <dt>Annual SA baseline</dt>
        <dd>{formatMoney(analysisString('software_assurance_annual_usd'))}</dd>
      </div>
      <div>
        <dt>Gross annual PAYG</dt>
        <dd>{formatMoney(analysisString('payg_gross_annual_usd'))}</dd>
      </div>
      <div>
        <dt>Applied PAYG discount</dt>
        <dd>{formatPercent(analysisString('applied_payg_discount'))}</dd>
      </div>
      <div>
        <dt>Net annual PAYG</dt>
        <dd>{formatMoney(analysisString('payg_net_annual_usd'))}</dd>
      </div>
      <div>
        <dt>Required breakeven discount</dt>
        <dd>{formatPercent(analysisString('required_payg_discount'))}</dd>
      </div>
      <div>
        <dt>PAYG at breakeven</dt>
        <dd>{formatMoney(analysisString('payg_at_breakeven_usd'))}</dd>
      </div>
      <div>
        <dt>Annual run hours</dt>
        <dd>{formatHours(analysisString('annual_hours'))}</dd>
      </div>
      <div>
        <dt>Enterprise PAYG rate</dt>
        <dd>{formatRate(analysisString('enterprise_payg_usd_per_core_hour'))}</dd>
      </div>
      <div>
        <dt>Standard PAYG rate</dt>
        <dd>{formatRate(analysisString('standard_payg_usd_per_core_hour'))}</dd>
      </div>
    </dl>
    {#if analysisString('rate_source_url')}
      <a
        class="source-link"
        href={analysisString('rate_source_url') ?? '#'}
        target="_blank"
        rel="external noreferrer"
      >
        Azure Retail Prices source, verified {analysisString('rate_verified_on') ??
          'date unavailable'}
        <ExternalLink size={14} aria-hidden="true" />
      </a>
    {/if}
  </section>
{/if}

<section class="decision-panel" aria-labelledby="licensing-checks-heading">
  <div class="section-heading">
    <span class="eyebrow">Decision checks</span>
    <h2 id="licensing-checks-heading">Validate before changing license type</h2>
  </div>
  <div class="check-grid">
    <article>
      <Scale class="check-icon" size={20} aria-hidden="true" />
      <h3>Agreement position</h3>
      <p>
        Reconcile EA true-up or EAS anniversary orders, renewal timing, and any EAS buyout. Confirm
        perpetual rights and SA end dates from the customer agreement; MCA-E is administered
        separately from classic volume licensing.
      </p>
      <a
        href="https://learn.microsoft.com/volume-licensing-central/learning/contracting/coverage-periods-and-usage-dates"
        target="_blank"
        rel="external noreferrer">Microsoft ordering guidance <ExternalLink size={13} /></a
      >
    </article>
    <article>
      <Gauge class="check-icon" size={20} aria-hidden="true" />
      <h3>Metered core scope</h3>
      <p>
        PAYG meters cores available to each operating system environment, with a four-core minimum.
        Multiple instances share the OSE meter and the highest installed SQL edition determines the
        meter.
      </p>
      <a
        href="https://learn.microsoft.com/sql/sql-server/azure-arc/manage-license-billing?view=sql-server-ver17#metering-and-reporting-software-usage"
        target="_blank"
        rel="external noreferrer">Licensing and billing rules <ExternalLink size={13} /></a
      >
    </article>
    <article>
      <BadgeDollarSign class="check-icon" size={20} aria-hidden="true" />
      <h3>Usage and benefits</h3>
      <p>
        Confirm actual running hours, Azure Arc connectivity, eligible passive HA/DR replicas, and
        whether retained licenses with active SA are better used through Azure Hybrid Benefit or
        other agreement rights.
      </p>
      <a
        href="https://learn.microsoft.com/sql/sql-server/azure-arc/manage-license-billing?view=sql-server-ver17#license-sql-server-instances-by-virtual-cores"
        target="_blank"
        rel="external noreferrer">PAYG operating requirements <ExternalLink size={13} /></a
      >
    </article>
  </div>
  <p class="legal-note">
    Contract interpretation, entitlement, taxes, negotiated discounts, and transition timing require
    confirmation by the customer, licensing specialist, and applicable legal or procurement
    reviewer.
  </p>
</section>

<style>
  .licensing-intro,
  .input-panel,
  .result-panel,
  .decision-panel {
    max-width: 1240px;
    margin-inline: auto;
  }
  .licensing-intro {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 28px 0 22px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .licensing-intro h2,
  .section-heading h2,
  .result-lead h2 {
    margin: 3px 0 0;
    color: var(--ink-strong);
    font:
      700 1.35rem/1.2 Bahnschrift,
      sans-serif;
  }
  .licensing-intro p {
    max-width: 760px;
    margin: 8px 0 0;
    color: var(--ink-soft);
    line-height: 1.55;
  }
  :global(.intro-icon) {
    flex: 0 0 auto;
    color: var(--azure-text);
  }
  .eyebrow {
    color: var(--muted);
    font-size: 0.7rem;
    font-weight: 750;
    letter-spacing: 0;
    text-transform: uppercase;
  }
  .input-panel,
  .result-panel,
  .decision-panel {
    padding: 24px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .input-grid,
  .result-grid,
  .check-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
    margin-top: 16px;
  }
  label {
    display: grid;
    gap: 7px;
    color: var(--ink-soft);
    font-size: 0.8rem;
    font-weight: 700;
  }
  .utilization-field {
    grid-column: span 2;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .utilization-field legend {
    margin-bottom: 7px;
    padding: 0;
    color: var(--ink-soft);
    font-size: 0.8rem;
    font-weight: 700;
  }
  .utilization-control {
    display: grid;
    grid-template-columns: auto minmax(150px, 1fr);
    align-items: end;
    gap: 10px;
  }
  .segmented {
    display: grid;
    grid-template-columns: repeat(2, minmax(72px, 1fr));
  }
  .segmented button {
    min-height: 44px;
    padding: 9px 12px;
    color: var(--ink-soft);
    background: var(--surface-subtle);
    border: 1px solid var(--border-input);
    border-radius: 4px 0 0 4px;
    font-weight: 700;
    cursor: pointer;
  }
  .segmented button + button {
    margin-left: -1px;
    border-radius: 0 4px 4px 0;
  }
  .segmented button.active {
    position: relative;
    color: #fff;
    background: var(--azure);
    border-color: var(--azure);
  }
  input {
    width: 100%;
    min-height: 44px;
    padding: 9px 11px;
    color: var(--ink);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 4px;
    font:
      500 1rem/1.3 Aptos,
      'Trebuchet MS',
      sans-serif;
  }
  input:focus {
    border-color: var(--focus);
    outline: 2px solid color-mix(in srgb, var(--focus) 22%, transparent);
    outline-offset: 1px;
  }
  .input-note,
  .legal-note {
    margin: 13px 0 0;
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }
  .result-panel {
    display: grid;
    grid-template-columns: minmax(240px, 0.75fr) minmax(0, 1.7fr);
    gap: 28px;
    align-items: start;
  }
  .result-lead {
    padding-left: 16px;
    border-left: 4px solid var(--azure);
  }
  .result-lead[data-tone='savings'] {
    border-left-color: var(--success);
  }
  .result-lead[data-tone='savings'] strong {
    color: var(--success);
  }
  .result-lead[data-tone='overage'] {
    border-left-color: var(--danger);
  }
  .result-lead[data-tone='overage'] strong {
    color: var(--danger-text);
  }
  .result-lead strong {
    display: block;
    margin-top: 18px;
    color: var(--azure-text);
    font:
      750 2.65rem/1 Bahnschrift,
      sans-serif;
  }
  .result-lead p {
    margin: 10px 0 0;
    color: var(--ink-soft);
    line-height: 1.5;
  }
  .result-grid {
    margin-top: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .result-grid div {
    min-height: 76px;
    padding: 13px;
    background: var(--surface-subtle);
    border-left: 2px solid var(--border);
  }
  dt {
    color: var(--muted);
    font-size: 0.72rem;
    font-weight: 700;
  }
  dd {
    margin: 7px 0 0;
    color: var(--ink-strong);
    font:
      700 1rem/1.3 Bahnschrift,
      sans-serif;
    overflow-wrap: anywhere;
  }
  .source-link {
    grid-column: 2;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    width: fit-content;
    color: var(--azure-text);
    font-size: 0.76rem;
    font-weight: 700;
  }
  .check-grid article {
    padding: 16px 0 2px;
    border-top: 3px solid var(--border);
  }
  :global(.check-icon) {
    color: var(--azure-text);
  }
  .check-grid h3 {
    margin: 9px 0 6px;
    color: var(--ink-strong);
    font:
      700 0.95rem/1.25 Bahnschrift,
      sans-serif;
  }
  .check-grid p {
    margin: 0 0 12px;
    color: var(--ink-soft);
    font-size: 0.82rem;
    line-height: 1.55;
  }
  .check-grid a {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--azure-text);
    font-size: 0.75rem;
    font-weight: 700;
  }
  @media (max-width: 760px) {
    .licensing-intro {
      padding-top: 20px;
    }
    :global(.intro-icon) {
      display: none;
    }
    .input-grid,
    .check-grid,
    .result-grid {
      grid-template-columns: 1fr;
    }
    .utilization-field {
      grid-column: 1;
    }
    .utilization-control {
      grid-template-columns: 1fr;
    }
    .result-panel {
      grid-template-columns: 1fr;
      gap: 20px;
    }
    .source-link {
      grid-column: 1;
    }
  }
</style>
