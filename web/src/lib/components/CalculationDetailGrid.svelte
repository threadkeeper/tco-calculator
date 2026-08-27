<script lang="ts">
  import { ArrowRight, FileSpreadsheet } from 'lucide-svelte';
  import { formatMoney } from '$lib/api';
  import { buildCalculationResultRows, type CalculationResultRow } from '$lib/calculation-results';
  import type { ProjectDraft } from '$lib/draft';
  import { downloadProjectExport } from '$lib/workbook-export';

  type CellValue = string | number | null;
  type CellKind = 'text' | 'number' | 'money' | 'rate' | 'signed-money';
  type ResultColumn = {
    label: string;
    value: (row: CalculationResultRow) => CellValue;
    kind: CellKind;
  };
  type ResultGroup = {
    label: string;
    tone: 'workload' | 'target' | 'source' | 'azure' | 'savings' | 'parity';
    columns: ResultColumn[];
  };

  let {
    calculation,
    project
  }: {
    calculation: unknown;
    project: ProjectDraft;
  } = $props();

  const rows = $derived(buildCalculationResultRows(calculation, project.resources));
  const isVmProject = $derived(project.settings.project_type === 'ec2_vm');

  const sqlGroups: ResultGroup[] = [
    {
      label: 'Workload',
      tone: 'workload',
      columns: [
        column('Name', 'text', (row) => row.workloadName),
        column('Server name', 'text', (row) => row.serverName),
        column('Source SKU', 'text', (row) => row.sourceSku),
        column('Qty', 'number', (row) => row.quantity),
        column('SQL edition', 'text', (row) => row.sqlEdition),
        column('License', 'text', (row) => row.licenseBasis),
        column('SQL data GB', 'number', (row) => row.sqlDataGbPerInstance),
        column('Persistent EBS GB', 'number', (row) => row.persistentEbsGbPerInstance),
        column('Source RAM GB', 'number', (row) => row.sourceRamGbPerInstance),
        column('Annual hours', 'number', (row) => row.annualHoursPerInstance),
        column('MI purchase', 'text', (row) => row.miPurchaseOption)
      ]
    },
    {
      label: 'Derived MI SKU',
      tone: 'target',
      columns: [
        column('MI storage GB', 'number', (row) => row.azureStorageGbPerInstance),
        column('MI RAM GB', 'number', (row) => row.selectedMemoryGb),
        column('Service tier', 'text', (row) => row.serviceTier),
        column('Hardware', 'text', (row) => row.hardwareFamily),
        column('Storage architecture', 'text', (row) => row.storageArchitecture),
        column('vCores', 'number', (row) => row.vcores)
      ]
    },
    {
      label: 'Source cost',
      tone: 'source',
      columns: [
        column('Compute gross', 'money', (row) => row.sourceComputeGross),
        column('Compute net', 'money', (row) => row.sourceComputeNet),
        column('License gross', 'money', (row) => row.sourceLicenseGross),
        column('License net', 'money', (row) => row.sourceLicenseNet),
        column('Storage gross', 'money', (row) => row.sourceStorageGross),
        column('Storage net', 'money', (row) => row.sourceStorageNet),
        column('Hardware annual', 'money', (row) => row.sourceHardwareAnnual),
        column('Electricity annual', 'money', (row) => row.sourceElectricityAnnual),
        column('Net total', 'money', (row) => row.sourceTotal)
      ]
    },
    {
      label: 'Azure SQL MI cost',
      tone: 'azure',
      columns: [
        column('Compute gross', 'money', (row) => row.azureComputeGross),
        column('Additional RAM GB', 'number', (row) => row.azureAdditionalRamGb),
        column('Additional RAM gross', 'money', (row) => row.azureAdditionalRamGross),
        column('Compute + RAM net', 'money', (row) => row.azureComputePlusRamNet),
        column('License gross', 'money', (row) => row.azureLicenseGross),
        column('License net', 'money', (row) => row.azureLicenseNet),
        column('Storage gross', 'money', (row) => row.azureStorageGross),
        column('Storage net', 'money', (row) => row.azureStorageNet),
        column('MI net before parity', 'money', (row) => row.azureTotalBeforeParity)
      ]
    },
    {
      label: 'Savings before parity',
      tone: 'savings',
      columns: [
        column('Compute', 'money', (row) => row.computeSavings),
        column('License', 'money', (row) => row.licenseSavings),
        column('Storage', 'money', (row) => row.storageSavings),
        column('Total', 'money', (row) => row.totalSavings)
      ]
    },
    {
      label: 'Parity',
      tone: 'parity',
      columns: [
        column('Required adjustment', 'rate', (row) => row.requiredAdjustment),
        column('Selected adjustment', 'rate', (row) => row.selectedAdjustment),
        column('MI after parity', 'money', (row) => row.azureAfterSelectedParity),
        column('Difference (Azure - source)', 'signed-money', (row) => row.difference)
      ]
    }
  ];

  const vmGroups: ResultGroup[] = [
    {
      label: 'Workload',
      tone: 'workload',
      columns: [
        column('Name', 'text', (row) => row.workloadName),
        column('Server name', 'text', (row) => row.serverName),
        column('Source SKU', 'text', (row) => row.sourceSku),
        column('Qty', 'number', (row) => row.quantity),
        column('Burst policy', 'text', (row) => row.burstPolicy),
        column('Instance store', 'text', (row) => row.instanceStoreUse),
        column('Local temp GB', 'number', (row) => row.requiredLocalTempDiskGb),
        column('Ephemeral loss accepted', 'text', (row) =>
          row.ephemeralDataLossAcceptable === null
            ? null
            : row.ephemeralDataLossAcceptable
              ? 'yes'
              : 'no'
        ),
        column('High-frequency', 'text', (row) => row.highFrequencyRequirement),
        column('Target override', 'text', (row) => row.requestedTargetArmSku),
        column('Persistent EBS GB', 'number', (row) => row.persistentEbsGbPerInstance),
        column('Source RAM GB', 'number', (row) => row.sourceRamGbPerInstance),
        column('Annual hours', 'number', (row) => row.annualHoursPerInstance),
        column('VM purchase', 'text', (row) => row.vmPurchaseOption)
      ]
    },
    {
      label: 'Derived Azure VM',
      tone: 'target',
      columns: [
        column('Managed disk GB', 'number', (row) => row.azureStorageGbPerInstance),
        column('VM RAM GB', 'number', (row) => row.selectedMemoryGb),
        column('VM SKU', 'text', (row) => row.serviceTier),
        column('Family', 'text', (row) => row.hardwareFamily),
        column('Managed disks', 'text', (row) => row.storageArchitecture),
        column('vCPU', 'number', (row) => row.vcores),
        column('Recommendation', 'text', (row) => row.recommendationStatus)
      ]
    },
    {
      label: 'Source cost',
      tone: 'source',
      columns: [
        column('Compute gross', 'money', (row) => row.sourceComputeGross),
        column('Compute net', 'money', (row) => row.sourceComputeNet),
        column('Storage gross', 'money', (row) => row.sourceStorageGross),
        column('Storage net', 'money', (row) => row.sourceStorageNet),
        column('Net total', 'money', (row) => row.sourceTotal)
      ]
    },
    {
      label: 'Azure VM cost',
      tone: 'azure',
      columns: [
        column('Compute gross', 'money', (row) => row.azureComputeGross),
        column('Compute net', 'money', (row) => row.azureComputePlusRamNet),
        column('License gross', 'money', (row) => row.azureLicenseGross),
        column('License net', 'money', (row) => row.azureLicenseNet),
        column('Storage gross', 'money', (row) => row.azureStorageGross),
        column('Storage net', 'money', (row) => row.azureStorageNet),
        column('VM net before parity', 'money', (row) => row.azureTotalBeforeParity)
      ]
    },
    {
      label: 'Savings before parity',
      tone: 'savings',
      columns: [
        column('Compute', 'money', (row) => row.computeSavings),
        column('License', 'money', (row) => row.licenseSavings),
        column('Storage', 'money', (row) => row.storageSavings),
        column('Total', 'money', (row) => row.totalSavings)
      ]
    },
    {
      label: 'Parity',
      tone: 'parity',
      columns: [
        column('Required adjustment', 'rate', (row) => row.requiredAdjustment),
        column('Selected adjustment', 'rate', (row) => row.selectedAdjustment),
        column('Azure after parity', 'money', (row) => row.azureAfterSelectedParity),
        column('Difference (Azure - source)', 'signed-money', (row) => row.difference)
      ]
    }
  ];

  const groups = $derived(isVmProject ? vmGroups : sqlGroups);

  function column(
    label: string,
    kind: CellKind,
    value: (row: CalculationResultRow) => CellValue
  ): ResultColumn {
    return { label, kind, value };
  }

  function display(value: CellValue, kind: CellKind): string {
    if (value === null)
      return kind === 'money' || kind === 'signed-money' ? 'PRICE UNAVAILABLE' : 'Unavailable';
    if (kind === 'money') return formatMoney(String(value));
    if (kind === 'signed-money') return signedMoney(String(value));
    if (kind === 'rate') return formatRate(String(value));
    return kind === 'text' ? label(String(value)) : String(value);
  }

  function label(value: string): string {
    return value.replaceAll('_', ' ');
  }

  function formatRate(value: string): string {
    const number = Number(value);
    if (!Number.isFinite(number)) return value;
    return new Intl.NumberFormat('en-US', {
      style: 'percent',
      minimumFractionDigits: 0,
      maximumFractionDigits: 4
    }).format(number);
  }

  function signedMoney(value: string): string {
    const number = Number(value);
    if (!Number.isFinite(number)) return value;
    const formatted = formatMoney(value);
    return number > 0 ? `+${formatted}` : formatted;
  }

  function differenceDirection(value: string | null): 'higher' | 'lower' | 'even' | 'unknown' {
    if (value === null) return 'unknown';
    const number = Number(value);
    if (!Number.isFinite(number)) return 'unknown';
    if (number > 0) return 'higher';
    if (number < 0) return 'lower';
    return 'even';
  }
</script>

<section class="detail-results" aria-labelledby="detail-results-heading">
  <header class="detail-heading">
    <div>
      <span class="eyebrow">Resource line items</span>
      <h2 id="detail-results-heading">Workbook-level detail</h2>
    </div>
    <button
      class="export-button"
      type="button"
      title="Export formatted Excel workbook"
      onclick={() => downloadProjectExport(project, calculation)}
    >
      <FileSpreadsheet size={17} aria-hidden="true" />
      Export XLSX
    </button>
  </header>

  <div class="table-shell" aria-label="Scrollable resource cost comparison">
    <table>
      <caption
        >Annual source and {isVmProject ? 'Azure Virtual Machine' : 'Azure SQL Managed Instance'} cost
        details by resource</caption
      >
      <thead>
        <tr class="group-row">
          {#each groups as group (group.label)}
            <th class={group.tone} colspan={group.columns.length} scope="colgroup">{group.label}</th
            >
          {/each}
        </tr>
        <tr class="column-row">
          {#each groups as group (group.label)}
            {#each group.columns as resultColumn (resultColumn.label)}
              <th class={group.tone} scope="col">{resultColumn.label}</th>
            {/each}
          {/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as row (row.resourceId)}
          <tr>
            {#each groups as group, groupIndex (group.label)}
              {#each group.columns as resultColumn, columnIndex (resultColumn.label)}
                {@const value = resultColumn.value(row)}
                <td
                  class:sticky-cell={groupIndex === 0 && columnIndex === 0}
                  class:difference-higher={resultColumn.kind === 'signed-money' &&
                    differenceDirection(row.difference) === 'higher'}
                  class:difference-lower={resultColumn.kind === 'signed-money' &&
                    differenceDirection(row.difference) === 'lower'}
                  data-tone={group.tone}
                >
                  {#if groupIndex === 0 && columnIndex === 0}
                    <strong>{display(value, resultColumn.kind)}</strong>
                    <span class:ok={row.mappingStatus === 'mapped'} class="mapping-status"
                      >{label(row.mappingStatus ?? 'not mapped')}</span
                    >
                  {:else}
                    {display(value, resultColumn.kind)}
                  {/if}
                </td>
              {/each}
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  <div class="mobile-rows">
    {#each rows as row (row.resourceId)}
      <details class="mobile-row">
        <summary>
          <div>
            <strong>{row.workloadName}</strong>
            <span>{label(row.serviceTier ?? row.mappingStatus ?? 'not mapped')}</span>
          </div>
          <div class="mobile-comparison">
            <span
              >{formatMoney(row.sourceTotal)}
              <ArrowRight size={13} />
              {formatMoney(row.azureAfterSelectedParity)}</span
            >
            <b
              class:higher={differenceDirection(row.difference) === 'higher'}
              class:lower={differenceDirection(row.difference) === 'lower'}
              >{display(row.difference, 'signed-money')}</b
            >
          </div>
        </summary>
        <div class="mobile-groups">
          {#each groups as group (group.label)}
            <section class={`mobile-group ${group.tone}`}>
              <h3>{group.label}</h3>
              <dl>
                {#each group.columns as resultColumn (resultColumn.label)}
                  <div>
                    <dt>{resultColumn.label}</dt>
                    <dd>{display(resultColumn.value(row), resultColumn.kind)}</dd>
                  </div>
                {/each}
              </dl>
            </section>
          {/each}
        </div>
      </details>
    {/each}
  </div>
</section>

<style>
  .detail-results {
    padding: 22px;
    background: var(--surface-subtle);
    border-top: 1px solid var(--border);
  }
  .detail-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
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
      700 1.25rem/1.2 Bahnschrift,
      sans-serif;
  }
  .export-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 38px;
    padding: 8px 12px;
    color: var(--azure-text);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 4px;
    font-weight: 700;
    cursor: pointer;
  }
  .export-button:hover {
    background: var(--azure-soft);
  }
  .table-shell {
    overflow-x: auto;
    max-width: 100%;
    border: 1px solid var(--border);
    background: var(--surface);
    scrollbar-color: var(--border-input) var(--surface-muted);
  }
  table {
    width: 100%;
    min-width: 5100px;
    border-collapse: separate;
    border-spacing: 0;
    table-layout: fixed;
    color: var(--ink);
    font-size: 0.75rem;
  }
  caption {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  th,
  td {
    min-width: 120px;
    padding: 9px 10px;
    border-right: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
    text-align: right;
    vertical-align: middle;
  }
  th:first-child,
  td:first-child {
    min-width: 190px;
    text-align: left;
  }
  .group-row th {
    padding: 9px 12px;
    border-bottom-color: var(--border-input);
    color: var(--ink-strong);
    font:
      700 0.75rem/1.2 Bahnschrift,
      sans-serif;
    text-align: left;
    text-transform: uppercase;
  }
  .column-row th {
    color: var(--ink-soft);
    font-size: 0.68rem;
    line-height: 1.25;
    text-transform: uppercase;
  }
  th.workload,
  td[data-tone='workload'] {
    background: var(--tone-workload);
  }
  th.target,
  td[data-tone='target'] {
    background: var(--tone-target);
  }
  th.source,
  td[data-tone='source'] {
    background: var(--tone-source);
  }
  th.azure,
  td[data-tone='azure'] {
    background: var(--tone-azure);
  }
  th.savings,
  td[data-tone='savings'] {
    background: var(--tone-savings);
  }
  th.parity,
  td[data-tone='parity'] {
    background: var(--tone-parity);
  }
  th.target,
  th.azure {
    color: var(--azure-text);
  }
  th.source {
    color: var(--source);
  }
  th.savings {
    color: var(--success);
  }
  th.parity {
    color: var(--copilot-ink);
  }
  tbody tr:last-child td {
    border-bottom: 0;
  }
  tbody tr:hover td {
    filter: brightness(0.975);
  }
  .sticky-cell {
    position: sticky;
    left: 0;
    z-index: 1;
    box-shadow: 2px 0 0 var(--border);
  }
  .sticky-cell strong {
    display: block;
    overflow: hidden;
    color: var(--ink-strong);
    font-size: 0.8rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mapping-status {
    display: inline-block;
    margin-top: 4px;
    padding: 2px 5px;
    color: var(--issue-text);
    background: var(--issue-surface);
    border-radius: 3px;
    font-size: 0.62rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .mapping-status.ok {
    color: var(--success);
    background: var(--success-surface);
  }
  .difference-higher,
  .higher {
    color: var(--danger);
    font-weight: 800;
  }
  .difference-lower,
  .lower {
    color: var(--success);
    font-weight: 800;
  }
  .mobile-rows {
    display: none;
  }
  @media (max-width: 800px) {
    .detail-results {
      padding: 18px 14px;
    }
    .detail-heading {
      align-items: stretch;
      flex-direction: column;
    }
    .export-button {
      align-self: flex-start;
    }
    .table-shell {
      display: none;
    }
    .mobile-rows {
      display: grid;
      gap: 10px;
    }
    .mobile-row {
      overflow: hidden;
      background: var(--surface);
      border: 1px solid var(--border);
      border-radius: 4px;
    }
    .mobile-row summary {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
      gap: 12px;
      padding: 13px;
      cursor: pointer;
      list-style: none;
    }
    .mobile-row summary::-webkit-details-marker {
      display: none;
    }
    .mobile-row summary strong,
    .mobile-row summary span {
      display: block;
    }
    .mobile-row summary strong {
      overflow-wrap: anywhere;
      color: var(--ink-strong);
    }
    .mobile-row summary span {
      margin-top: 3px;
      color: var(--muted);
      font-size: 0.72rem;
      text-transform: capitalize;
    }
    .mobile-comparison {
      text-align: right;
    }
    .mobile-comparison span {
      display: flex;
      align-items: center;
      justify-content: end;
      gap: 4px;
      white-space: nowrap;
    }
    .mobile-comparison b {
      display: block;
      margin-top: 4px;
      font-size: 0.85rem;
    }
    .mobile-groups {
      border-top: 1px solid var(--border-subtle);
    }
    .mobile-group {
      padding: 13px;
    }
    .mobile-group + .mobile-group {
      border-top: 1px solid var(--border-subtle);
    }
    .mobile-group.workload {
      background: var(--tone-workload);
    }
    .mobile-group.target {
      background: var(--tone-target);
    }
    .mobile-group.source {
      background: var(--tone-source);
    }
    .mobile-group.azure {
      background: var(--tone-azure);
    }
    .mobile-group.savings {
      background: var(--tone-savings);
    }
    .mobile-group.parity {
      background: var(--tone-parity);
    }
    .mobile-group h3 {
      margin: 0 0 8px;
      color: var(--ink-strong);
      font:
        700 0.72rem/1.2 Bahnschrift,
        sans-serif;
      text-transform: uppercase;
    }
    dl {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 9px 14px;
      margin: 0;
    }
    dl div {
      min-width: 0;
    }
    dt {
      color: var(--muted);
      font-size: 0.65rem;
      font-weight: 700;
      text-transform: uppercase;
    }
    dd {
      overflow-wrap: anywhere;
      margin: 2px 0 0;
      color: var(--ink);
      font-size: 0.78rem;
      text-transform: capitalize;
    }
  }
  @media (max-width: 430px) {
    .mobile-row summary {
      grid-template-columns: 1fr;
    }
    .mobile-comparison {
      text-align: left;
    }
    .mobile-comparison span {
      justify-content: start;
    }
    dl {
      grid-template-columns: 1fr;
    }
  }
</style>
