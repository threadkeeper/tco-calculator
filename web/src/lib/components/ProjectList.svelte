<script lang="ts">
  import { ArrowRight, Database, Plus, Trash2 } from 'lucide-svelte';
  import { formatMoney, readNumber, readString, type JsonRecord } from '$lib/api';
  import { text } from '$lib/i18n/en';

  let {
    projects = [],
    loading = false,
    onnew,
    onopen,
    ondelete
  }: {
    projects?: JsonRecord[];
    loading?: boolean;
    onnew: () => void;
    onopen: (id: string) => void;
    ondelete: (id: string, name: string) => void;
  } = $props();

  function formatDate(value: string | null): string {
    if (!value) return 'Unknown';
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? value
      : new Intl.DateTimeFormat('en', { dateStyle: 'medium', timeStyle: 'short' }).format(date);
  }

  function typeLabel(value: string | null): string {
    if (value === 'on_prem') return 'On-prem';
    if (value === 'sql_payg') return 'SQL PAYG';
    if (value === 'ec2_vm') return 'EC2 VM';
    return value?.toUpperCase() ?? 'Unknown';
  }

  function signedMoney(value: string | null): string {
    if (value === null) return formatMoney(null);
    const amount = Number(value);
    if (!Number.isFinite(amount)) return value;
    const formatted = formatMoney(value);
    return amount > 0 ? `+${formatted}` : formatted;
  }

  function savingsTone(value: string | null): 'positive' | 'negative' | 'neutral' | 'unavailable' {
    if (value === null) return 'unavailable';
    const amount = Number(value);
    if (!Number.isFinite(amount) || amount === 0) return 'neutral';
    return amount > 0 ? 'positive' : 'negative';
  }
</script>

<section class="title-band" aria-labelledby="projects-heading">
  <div>
    <p class="eyebrow">Current estimates</p>
    <h1 id="projects-heading">{text.projects}</h1>
  </div>
  <button class="button primary" type="button" onclick={onnew}>
    <Plus size={18} aria-hidden="true" />
    <span>{text.newProject}</span>
  </button>
</section>

<section class="project-table" aria-label={text.projects}>
  <div class="table-header" aria-hidden="true">
    <span>Name</span>
    <span>Type</span>
    <span>Modified</span>
    <span>Source region</span>
    <span>Azure region</span>
    <span>Resources</span>
    <span>Source annual</span>
    <span>Azure annual</span>
    <span>Azure Savings</span>
    <span>Actions</span>
  </div>
  {#if loading}
    <div class="empty-state" role="status"><span>Loading saved projects…</span></div>
  {:else if projects.length === 0}
    <div class="empty-state">
      <Database size={28} aria-hidden="true" />
      <p>{text.noProject}</p>
      <span>{text.noProjectDetail}</span>
    </div>
  {:else}
    <div class="table-body">
      {#each projects as project (readString(project, 'id'))}
        {@const id = readString(project, 'id') ?? ''}
        {@const name = readString(project, 'name') ?? 'Untitled project'}
        {@const azureSavings = readString(project, 'azure_savings')}
        <div class="table-row">
          <button class="project-name" type="button" onclick={() => onopen(id)}>{name}</button>
          <span>{typeLabel(readString(project, 'project_type'))}</span>
          <span>{formatDate(readString(project, 'modified_at'))}</span>
          <span>{readString(project, 'source_region') ?? 'Not applicable'}</span>
          <span>{readString(project, 'azure_region') ?? 'Unknown'}</span>
          <span>{readNumber(project, 'resource_count') ?? 0}</span>
          <strong>{formatMoney(readString(project, 'source_annual_total'))}</strong>
          <strong>{formatMoney(readString(project, 'azure_annual_total'))}</strong>
          <strong class="azure-savings" data-tone={savingsTone(azureSavings)}
            >{signedMoney(azureSavings)}</strong
          >
          <span class="row-actions">
            <button
              type="button"
              onclick={() => onopen(id)}
              aria-label={`Open ${name}`}
              title="Open project"><ArrowRight size={17} /></button
            >
            <button
              class="delete"
              type="button"
              onclick={() => ondelete(id, name)}
              aria-label={`Delete ${name}`}
              title="Delete project"><Trash2 size={16} /></button
            >
          </span>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .table-body {
    min-width: 1280px;
  }
  .table-row {
    display: grid;
    grid-template-columns: 1.8fr 0.75fr 1fr 1fr 1fr 0.65fr 1fr 1fr 1fr 0.7fr;
    align-items: center;
    gap: 14px;
    min-height: 58px;
    padding: 8px 14px;
    color: var(--ink-soft);
    border-bottom: 1px solid var(--border-subtle);
    font-size: 0.8rem;
  }
  .table-row:last-child {
    border-bottom: 0;
  }
  .table-row:hover {
    background: var(--surface-hover);
  }
  .project-name {
    overflow: hidden;
    padding: 4px 0;
    color: var(--azure-text);
    background: transparent;
    border: 0;
    font:
      700 0.88rem/1.3 Bahnschrift,
      sans-serif;
    text-align: left;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  .table-row strong {
    overflow-wrap: anywhere;
    color: var(--ink-strong);
    font-weight: 650;
  }
  .azure-savings {
    width: max-content;
    max-width: 100%;
    padding: 4px 7px;
    border: 1px solid transparent;
    border-radius: 4px;
  }
  .azure-savings[data-tone='positive'] {
    color: var(--success);
    background: var(--success-surface);
    border-color: var(--success-border);
  }
  .azure-savings[data-tone='negative'] {
    color: var(--danger-text);
    background: var(--danger-surface);
    border-color: var(--danger-border);
  }
  .azure-savings[data-tone='unavailable'] {
    color: var(--muted);
  }
  .row-actions {
    display: flex;
    gap: 4px;
  }
  .row-actions button {
    display: grid;
    width: 31px;
    height: 31px;
    place-items: center;
    padding: 0;
    color: var(--ink-soft);
    background: var(--surface-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    cursor: pointer;
  }
  .row-actions .delete {
    color: var(--danger);
  }
</style>
