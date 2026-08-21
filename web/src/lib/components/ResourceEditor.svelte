<script lang="ts">
  import { ChevronDown, Database, Plus, Trash2 } from 'lucide-svelte';
  import { readBoolean, readString, type JsonRecord } from '$lib/api';
  import type { EbsVolumeDraft, ResourceDraft } from '$lib/draft';
  import type { PurchaseOptionDiscounts } from '$lib/mi-purchase-options';
  import MiPurchasePlanSelector from './MiPurchasePlanSelector.svelte';

  let {
    resource,
    sourceInstances = [],
    ebsTypes = [],
    rdsOptions = [],
    purchaseOptionDiscounts = null,
    onremove,
    onchange,
    oncatalogchange = () => {}
  }: {
    resource: ResourceDraft;
    sourceInstances?: JsonRecord[];
    ebsTypes?: JsonRecord[];
    rdsOptions?: JsonRecord[];
    purchaseOptionDiscounts?: PurchaseOptionDiscounts | null;
    onremove: () => void;
    onchange: () => void;
    oncatalogchange?: () => void;
  } = $props();

  function selectInstance(instanceType: string) {
    if (resource.source_type === 'ec2' || resource.source_type === 'rds') {
      resource.instance_type = instanceType;
      const selected = sourceInstances.find(
        (item) => readString(item, 'instance_type') === instanceType
      );
      resource.source_ram_gb_per_instance =
        readString(selected ?? null, 'memory_gib') ?? resource.source_ram_gb_per_instance;
      onchange();
      oncatalogchange();
    }
  }

  function selectRdsOption(index: string) {
    if (resource.source_type !== 'rds') return;
    const selected = rdsOptions[Number(index)];
    if (selected) {
      resource.commercial_term =
        readString(selected, 'commercial_term') ?? resource.commercial_term;
      resource.storage_class = readString(selected, 'storage_class') ?? resource.storage_class;
      onchange();
    }
  }

  function selectedRdsOption(): string {
    if (resource.source_type !== 'rds') return '';
    const index = rdsOptions.findIndex(
      (option) =>
        readString(option, 'commercial_term') === resource.commercial_term &&
        readString(option, 'storage_class') === resource.storage_class
    );
    return index >= 0 ? String(index) : '';
  }

  let selectedRdsOptionIndex = $derived(selectedRdsOption());

  function addVolume() {
    if (resource.source_type !== 'ec2') return;
    const volume: EbsVolumeDraft = {
      id: crypto.randomUUID(),
      label: 'SQL data',
      aws_volume_id: null,
      volume_type: 'gp3',
      capacity_gb: '1024',
      provisioned_iops: 3000,
      throughput_mibps: '125'
    };
    resource.volumes = [...resource.volumes, volume];
    onchange();
  }

  function removeVolume(volumeId: string) {
    if (resource.source_type !== 'ec2') return;
    resource.volumes = resource.volumes.filter((volume) => volume.id !== volumeId);
    onchange();
  }

  function normalizeVolume(volume: EbsVolumeDraft) {
    if (volume.volume_type === 'ephemeral') {
      volume.capacity_gb = '0';
      volume.provisioned_iops = null;
      volume.throughput_mibps = null;
    } else {
      volume.provisioned_iops ??= volume.volume_type === 'gp3' ? 3000 : 1000;
      volume.throughput_mibps =
        volume.volume_type === 'gp3' ? (volume.throughput_mibps ?? '125') : null;
    }
    onchange();
  }

  function isMissing(value: unknown): boolean {
    return (
      value === null || value === undefined || (typeof value === 'string' && value.trim() === '')
    );
  }

  function ebsPriceError(volume: EbsVolumeDraft): string | null {
    const selected = ebsTypes.find((item) => readString(item, 'key') === volume.volume_type);
    const unavailable =
      readBoolean(selected ?? null, 'price_required') === true &&
      readBoolean(selected ?? null, 'pricing_available') !== true;
    if (!unavailable) return null;
    return `${readString(selected ?? null, 'label') ?? volume.volume_type} pricing is unavailable in the selected AWS region. This workload cannot be included in the comparison.`;
  }
</script>

<details class="resource-editor" open>
  <summary>
    <div class="resource-title">
      <span class="resource-icon"><Database size={18} aria-hidden="true" /></span>
      <div>
        <span class="eyebrow">{resource.source_type.replace('_', ' ')}</span>
        <strong>{resource.workload_name || 'Unnamed workload'}</strong>
        {#if resource.server_name}<span class="server-summary">{resource.server_name}</span>{/if}
      </div>
    </div>
    <span class="collapse-indicator" aria-hidden="true"><ChevronDown size={19} /></span>
  </summary>
  <button
    class="icon danger remove-workload"
    type="button"
    onclick={onremove}
    aria-label="Remove workload"
    title="Remove workload"
  >
    <Trash2 size={18} />
  </button>

  <div class="field-grid shared-fields">
    <label>
      <span>Workload name</span>
      <input
        maxlength="160"
        aria-invalid={isMissing(resource.workload_name)}
        aria-describedby={isMissing(resource.workload_name)
          ? `${resource.id}-workload-name-error`
          : undefined}
        bind:value={resource.workload_name}
        oninput={onchange}
      />
      {#if isMissing(resource.workload_name)}
        <small class="field-error" id={`${resource.id}-workload-name-error`}
          >Workload name is required.</small
        >
      {/if}
    </label>
    <label>
      <span>Server name</span>
      <input
        maxlength="160"
        bind:value={resource.server_name}
        oninput={onchange}
        placeholder="Optional server identifier"
      />
    </label>
    <label>
      <span>Quantity</span>
      <input
        type="number"
        min="1"
        max="10000"
        aria-invalid={isMissing(resource.quantity)}
        bind:value={resource.quantity}
        oninput={onchange}
      />
      {#if isMissing(resource.quantity)}<small class="field-error">Quantity is required.</small
        >{/if}
    </label>
    <label>
      <span>SQL edition</span>
      <select bind:value={resource.sql_edition} {onchange}>
        <option value="standard">Standard</option>
        <option value="enterprise">Enterprise</option>
      </select>
    </label>
    <label>
      <span>License basis</span>
      <select bind:value={resource.license_basis} {onchange}>
        <option value="byol">Bring your own license</option>
        <option value="license_included">License included</option>
      </select>
    </label>
    <label>
      <span>Annual hours / instance</span>
      <input
        type="number"
        min="0"
        max="8784"
        step="1"
        aria-invalid={isMissing(resource.annual_hours_per_instance)}
        bind:value={resource.annual_hours_per_instance}
        oninput={onchange}
      />
      {#if isMissing(resource.annual_hours_per_instance)}<small class="field-error"
          >Annual hours are required.</small
        >{/if}
    </label>
    <label>
      <span>Source RAM / instance (GiB)</span>
      <input
        type="number"
        min="0.01"
        step="0.01"
        aria-invalid={isMissing(resource.source_ram_gb_per_instance)}
        bind:value={resource.source_ram_gb_per_instance}
        oninput={onchange}
      />
      {#if isMissing(resource.source_ram_gb_per_instance)}<small class="field-error"
          >Source RAM is required.</small
        >{/if}
    </label>
    <label>
      <span>SQL data / instance (GB)</span>
      <input
        type="number"
        min="0.01"
        step="0.01"
        aria-invalid={isMissing(resource.sql_data_gb_per_instance)}
        bind:value={resource.sql_data_gb_per_instance}
        oninput={onchange}
      />
      {#if isMissing(resource.sql_data_gb_per_instance)}<small class="field-error"
          >SQL data is required.</small
        >{/if}
    </label>
    <MiPurchasePlanSelector
      id={`${resource.id}-mi-purchase-plan`}
      legend="Azure SQL MI pricing override"
      bind:value={resource.mi_purchase_option}
      discounts={purchaseOptionDiscounts}
      {onchange}
    />
  </div>

  {#if resource.source_type === 'ec2'}
    <section class="source-section">
      <div class="section-heading">
        <div>
          <span class="eyebrow">Compute</span>
          <h3>EC2 instance</h3>
        </div>
      </div>
      <div class="field-grid compact">
        <label>
          <span>Instance type</span>
          {#if sourceInstances.length > 0}
            <select
              value={resource.instance_type}
              aria-invalid={isMissing(resource.instance_type)}
              onchange={(event) => selectInstance(event.currentTarget.value)}
            >
              {#if !sourceInstances.some((item) => readString(item, 'instance_type') === resource.instance_type)}<option
                  value={resource.instance_type}>{resource.instance_type}</option
                >{/if}
              {#each sourceInstances as item, index (index)}<option
                  value={readString(item, 'instance_type') ?? ''}
                  >{readString(item, 'instance_type')}</option
                >{/each}
            </select>
          {:else}
            <input
              aria-invalid={isMissing(resource.instance_type)}
              bind:value={resource.instance_type}
              oninput={onchange}
              placeholder="r6id.8xlarge"
            />
          {/if}
          {#if isMissing(resource.instance_type)}<small class="field-error"
              >Instance type is required.</small
            >{/if}
        </label>
      </div>
    </section>

    <section class="source-section">
      <div class="section-heading">
        <div>
          <span class="eyebrow">Storage</span>
          <h3>EBS volumes</h3>
        </div>
        <button class="compact-button" type="button" onclick={addVolume}
          ><Plus size={16} /> Add volume</button
        >
      </div>
      {#if resource.volumes.length === 0}
        <p class="empty-note">No volumes configured.</p>
      {:else}
        <div class="volume-list">
          {#each resource.volumes as volume (volume.id)}
            {@const priceError = ebsPriceError(volume)}
            <div class="volume-row">
              <label>
                <span>Label</span>
                <input
                  aria-invalid={isMissing(volume.label)}
                  bind:value={volume.label}
                  oninput={onchange}
                />
                {#if isMissing(volume.label)}<small class="field-error"
                    >Volume label is required.</small
                  >{/if}
              </label>
              <label>
                <span>Type</span>
                <select
                  aria-invalid={isMissing(volume.volume_type) || priceError !== null}
                  bind:value={volume.volume_type}
                  onchange={() => normalizeVolume(volume)}
                >
                  {#if ebsTypes.length > 0}
                    {#each ebsTypes as item, index (index)}
                      {@const priceUnavailable =
                        readBoolean(item, 'price_required') === true &&
                        readBoolean(item, 'pricing_available') !== true}
                      <option value={readString(item, 'key') ?? ''}
                        >{readString(item, 'label')}{priceUnavailable
                          ? ' (price unavailable)'
                          : ''}</option
                      >
                    {/each}
                  {:else}
                    <option value="gp3">gp3</option>
                    <option value="io2">io2</option>
                    <option value="ephemeral">Instance storage</option>
                  {/if}
                </select>
                {#if isMissing(volume.volume_type)}
                  <small class="field-error">Volume type is required.</small>
                {:else if priceError}
                  <small class="field-error">{priceError}</small>
                {/if}
              </label>
              <label>
                <span>Capacity (GB)</span>
                <input
                  type="number"
                  min="0"
                  step="1"
                  disabled={volume.volume_type === 'ephemeral'}
                  aria-invalid={volume.volume_type !== 'ephemeral' && isMissing(volume.capacity_gb)}
                  bind:value={volume.capacity_gb}
                  oninput={onchange}
                />
                {#if volume.volume_type !== 'ephemeral' && isMissing(volume.capacity_gb)}<small
                    class="field-error"
                    >Capacity is required for {volume.volume_type} volumes.</small
                  >{/if}
              </label>
              <label>
                <span>Provisioned IOPS</span>
                <input
                  type="number"
                  min="0"
                  disabled={volume.volume_type === 'ephemeral'}
                  aria-invalid={volume.volume_type !== 'ephemeral' &&
                    isMissing(volume.provisioned_iops)}
                  bind:value={volume.provisioned_iops}
                  oninput={onchange}
                />
                {#if volume.volume_type !== 'ephemeral' && isMissing(volume.provisioned_iops)}<small
                    class="field-error"
                    >Provisioned IOPS are required for {volume.volume_type} volumes.</small
                  >{/if}
              </label>
              <label>
                <span>Throughput (MiB/s)</span>
                <input
                  type="number"
                  min="0"
                  disabled={volume.volume_type !== 'gp3'}
                  bind:value={volume.throughput_mibps}
                  oninput={onchange}
                />
              </label>
              <button
                class="icon"
                type="button"
                onclick={() => removeVolume(volume.id)}
                aria-label={`Remove ${volume.label}`}
                title="Remove volume"
              >
                <Trash2 size={17} />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {:else if resource.source_type === 'rds'}
    <section class="source-section">
      <div class="section-heading">
        <div>
          <span class="eyebrow">Managed source</span>
          <h3>RDS configuration</h3>
        </div>
      </div>
      <div class="field-grid">
        <label>
          <span>Instance type</span>
          {#if sourceInstances.length > 0}
            <select
              value={resource.instance_type}
              aria-invalid={isMissing(resource.instance_type)}
              onchange={(event) => selectInstance(event.currentTarget.value)}
            >
              {#if !sourceInstances.some((item) => readString(item, 'instance_type') === resource.instance_type)}<option
                  value={resource.instance_type}>{resource.instance_type}</option
                >{/if}
              {#each sourceInstances as item, index (index)}<option
                  value={readString(item, 'instance_type') ?? ''}
                  >{readString(item, 'instance_type')}</option
                >{/each}
            </select>
          {:else}
            <input
              aria-invalid={isMissing(resource.instance_type)}
              bind:value={resource.instance_type}
              oninput={onchange}
              placeholder="db.m6i.8xlarge"
            />
          {/if}
          {#if isMissing(resource.instance_type)}<small class="field-error"
              >Instance type is required.</small
            >{/if}
        </label>
        <label>
          <span>Deployment</span>
          <select
            bind:value={resource.deployment}
            onchange={() => {
              onchange();
              oncatalogchange();
            }}
          >
            <option value="single_az">Single-AZ</option>
            <option value="multi_az">Multi-AZ</option>
          </select>
        </label>
        {#if rdsOptions.length > 0}
          <label>
            <span>Commercial option</span>
            <select
              bind:value={selectedRdsOptionIndex}
              onchange={(event) => selectRdsOption(event.currentTarget.value)}
            >
              {#if selectedRdsOptionIndex === ''}<option value=""
                  >{resource.commercial_term} · {resource.storage_class}</option
                >{/if}
              {#each rdsOptions as option, index (index)}<option value={String(index)}
                  >{readString(option, 'commercial_term')} · {readString(
                    option,
                    'storage_class'
                  )}</option
                >{/each}
            </select>
          </label>
        {:else}
          <label>
            <span>Commercial term</span>
            <input
              aria-invalid={isMissing(resource.commercial_term)}
              bind:value={resource.commercial_term}
              oninput={onchange}
              placeholder="on-demand"
            />
            {#if isMissing(resource.commercial_term)}<small class="field-error"
                >Commercial term is required.</small
              >{/if}
          </label>
          <label>
            <span>Storage class</span>
            <input
              aria-invalid={isMissing(resource.storage_class)}
              bind:value={resource.storage_class}
              oninput={onchange}
              placeholder="gp3"
            />
            {#if isMissing(resource.storage_class)}<small class="field-error"
                >Storage class is required.</small
              >{/if}
          </label>
        {/if}
        <label>
          <span>Maximum source IOPS</span>
          <input
            type="number"
            min="0"
            aria-invalid={isMissing(resource.source_max_iops)}
            bind:value={resource.source_max_iops}
            oninput={onchange}
          />
          {#if isMissing(resource.source_max_iops)}<small class="field-error"
              >Maximum source IOPS are required.</small
            >{/if}
        </label>
      </div>
    </section>
  {:else}
    <section class="source-section">
      <div class="section-heading">
        <div>
          <span class="eyebrow">Datacenter source</span>
          <h3>On-premises configuration</h3>
        </div>
      </div>
      <div class="field-grid">
        <label>
          <span>Source vCPU</span>
          <input
            type="number"
            min="1"
            aria-invalid={isMissing(resource.source_vcpu)}
            bind:value={resource.source_vcpu}
            oninput={onchange}
          />
          {#if isMissing(resource.source_vcpu)}<small class="field-error"
              >Source vCPU is required.</small
            >{/if}
        </label>
        <label>
          <span>Licensable cores</span>
          <input
            type="number"
            min="1"
            aria-invalid={isMissing(resource.licensable_cores)}
            bind:value={resource.licensable_cores}
            oninput={onchange}
          />
          {#if isMissing(resource.licensable_cores)}<small class="field-error"
              >Licensable cores are required.</small
            >{/if}
        </label>
        <label>
          <span>Maximum source IOPS</span>
          <input
            type="number"
            min="0"
            aria-invalid={isMissing(resource.source_max_iops)}
            bind:value={resource.source_max_iops}
            oninput={onchange}
          />
          {#if isMissing(resource.source_max_iops)}<small class="field-error"
              >Maximum source IOPS are required.</small
            >{/if}
        </label>
        <label>
          <span>Hardware capex (USD)</span>
          <input
            type="number"
            min="0"
            step="0.01"
            aria-invalid={isMissing(resource.hardware_capex_usd)}
            bind:value={resource.hardware_capex_usd}
            oninput={onchange}
          />
          {#if isMissing(resource.hardware_capex_usd)}<small class="field-error"
              >Hardware capex is required.</small
            >{/if}
        </label>
        <label>
          <span>Depreciation years</span>
          <input
            type="number"
            min="0.01"
            step="0.01"
            aria-invalid={isMissing(resource.depreciation_years)}
            bind:value={resource.depreciation_years}
            oninput={onchange}
          />
          {#if isMissing(resource.depreciation_years)}<small class="field-error"
              >Depreciation years are required.</small
            >{/if}
        </label>
        <label>
          <span>Average power override (kW)</span>
          <input
            type="number"
            min="0"
            step="0.01"
            bind:value={resource.average_power_kw_override}
            oninput={onchange}
            placeholder="Use server estimate"
          />
        </label>
      </div>
    </section>
  {/if}
</details>

<style>
  .resource-editor {
    position: relative;
    overflow: hidden;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 62px 16px 18px;
    background: var(--surface-subtle);
    border-bottom: 1px solid var(--line);
    cursor: pointer;
    list-style: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .collapse-indicator {
    display: grid;
    place-items: center;
    color: var(--muted);
    transition: transform 160ms ease;
  }
  .resource-editor:not([open]) .collapse-indicator {
    transform: rotate(-90deg);
  }
  .resource-editor:not([open]) summary {
    border-bottom-color: transparent;
  }
  .resource-title {
    display: flex;
    align-items: center;
    gap: 11px;
    min-width: 0;
  }
  .resource-icon {
    display: grid;
    flex: 0 0 36px;
    width: 36px;
    height: 36px;
    place-items: center;
    color: var(--azure-text);
    background: var(--azure-soft);
  }
  .eyebrow {
    display: block;
    margin-bottom: 3px;
    color: var(--muted);
    font:
      700 0.68rem/1.2 Bahnschrift,
      sans-serif;
    letter-spacing: 0;
    text-transform: uppercase;
  }
  .resource-title strong {
    display: block;
    overflow-wrap: anywhere;
    color: var(--ink-strong);
    font:
      650 1rem/1.3 Bahnschrift,
      sans-serif;
  }
  .server-summary {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: 0.72rem;
  }
  .shared-fields {
    padding: 18px;
  }
  .field-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px;
  }
  .field-grid.compact {
    grid-template-columns: minmax(220px, 1fr) minmax(0, 2fr);
  }
  label {
    display: grid;
    align-content: start;
    gap: 6px;
    min-width: 0;
    color: var(--ink-soft);
    font-size: 0.79rem;
    font-weight: 700;
  }
  input,
  select {
    width: 100%;
    min-width: 0;
    min-height: 38px;
    box-sizing: border-box;
    padding: 7px 9px;
    color: var(--ink);
    background: color-mix(in srgb, #e98b22 11%, var(--surface-input));
    border: 1px solid var(--border-input);
    border-radius: 4px;
    font:
      400 0.9rem/1.3 Aptos,
      'Trebuchet MS',
      sans-serif;
  }
  input:focus,
  select:focus {
    border-color: var(--azure);
    outline: 2px solid var(--azure-focus);
    outline-offset: 0;
  }
  input:disabled {
    color: var(--muted);
    background: var(--surface-muted);
  }
  input[aria-invalid='true'],
  select[aria-invalid='true'] {
    border-color: var(--danger);
  }
  .field-error {
    color: var(--danger);
    font-size: 0.75rem;
    font-weight: 650;
    line-height: 1.3;
  }
  .source-section {
    padding: 17px 18px 19px;
    border-top: 1px solid var(--line);
  }
  .section-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 13px;
  }
  h3 {
    margin: 0;
    color: var(--ink-strong);
    font:
      650 0.95rem/1.2 Bahnschrift,
      sans-serif;
  }
  button {
    font: inherit;
    cursor: pointer;
  }
  .icon {
    display: grid;
    flex: 0 0 34px;
    width: 34px;
    height: 34px;
    place-items: center;
    color: var(--ink-soft);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
  }
  .icon:hover {
    background: var(--surface-muted);
    border-color: var(--border);
  }
  .icon.danger {
    color: var(--danger);
  }
  .remove-workload {
    position: absolute;
    z-index: 2;
    top: 17px;
    right: 16px;
  }
  .compact-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    padding: 6px 10px;
    color: var(--azure-text);
    background: var(--surface-input);
    border: 1px solid color-mix(in srgb, var(--azure) 55%, var(--border-input));
    border-radius: 4px;
    font-size: 0.82rem;
    font-weight: 700;
  }
  .volume-list {
    display: grid;
    gap: 8px;
  }
  .volume-row {
    display: grid;
    grid-template-columns: 1.2fr 0.8fr 0.8fr 0.9fr 0.9fr 34px;
    align-items: end;
    gap: 8px;
    padding: 11px;
    background: var(--surface-subtle);
    border: 1px solid var(--border-subtle);
  }
  .empty-note {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
  }
  @media (max-width: 900px) {
    .field-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .volume-row {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .volume-row .icon {
      align-self: end;
    }
  }
  @media (max-width: 580px) {
    .field-grid,
    .field-grid.compact,
    .volume-row {
      grid-template-columns: 1fr;
    }
    .shared-fields,
    .source-section {
      padding-inline: 14px;
    }
  }
</style>
