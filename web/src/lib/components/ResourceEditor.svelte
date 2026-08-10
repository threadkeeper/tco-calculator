<script lang="ts">
  import { Database, Plus, Trash2 } from 'lucide-svelte';
  import { readBoolean, readString, type JsonRecord } from '$lib/api';
  import type { EbsVolumeDraft, ResourceDraft } from '$lib/draft';

  let {
    resource,
    sourceInstances = [],
    ebsTypes = [],
    rdsOptions = [],
    onremove,
    onchange,
    oncatalogchange = () => {}
  }: {
    resource: ResourceDraft;
    sourceInstances?: JsonRecord[];
    ebsTypes?: JsonRecord[];
    rdsOptions?: JsonRecord[];
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
      volume.throughput_mibps = volume.volume_type === 'gp3' ? (volume.throughput_mibps ?? '125') : null;
    }
    onchange();
  }
</script>

<article class="resource-editor">
  <header>
    <div class="resource-title">
      <span class="resource-icon"><Database size={18} aria-hidden="true" /></span>
      <div>
        <span class="eyebrow">{resource.source_type.replace('_', ' ')}</span>
        <input class="name-input" aria-label="Workload name" bind:value={resource.workload_name} oninput={onchange} />
      </div>
    </div>
    <button class="icon danger" type="button" onclick={onremove} aria-label="Remove workload" title="Remove workload">
      <Trash2 size={18} />
    </button>
  </header>

  <div class="field-grid shared-fields">
    <label>
      <span>Quantity</span>
      <input type="number" min="1" max="10000" bind:value={resource.quantity} oninput={onchange} />
    </label>
    <label>
      <span>SQL edition</span>
      <select bind:value={resource.sql_edition} onchange={onchange}>
        <option value="standard">Standard</option>
        <option value="enterprise">Enterprise</option>
      </select>
    </label>
    <label>
      <span>License basis</span>
      <select bind:value={resource.license_basis} onchange={onchange}>
        <option value="byol">Bring your own license</option>
        <option value="license_included">License included</option>
      </select>
    </label>
    <label>
      <span>Annual hours / instance</span>
      <input type="number" min="0" max="8784" step="1" bind:value={resource.annual_hours_per_instance} oninput={onchange} />
    </label>
    <label>
      <span>Source RAM / instance (GiB)</span>
      <input type="number" min="0.01" step="0.01" bind:value={resource.source_ram_gb_per_instance} oninput={onchange} />
    </label>
    <label>
      <span>SQL data / instance (GB)</span>
      <input type="number" min="0.01" step="0.01" bind:value={resource.sql_data_gb_per_instance} oninput={onchange} />
    </label>
    <label>
      <span>Azure purchase option</span>
      <select bind:value={resource.mi_purchase_option} onchange={onchange}>
        <option value="payg">PAYG, license included</option>
        <option value="ahb">PAYG, Azure Hybrid Benefit</option>
        <option value="one-year">1-year reserved, license included</option>
        <option value="ahbone-year">1-year reserved, AHB</option>
        <option value="three-year">3-year reserved, license included</option>
        <option value="ahbthree-year">3-year reserved, AHB</option>
        <option value="sv-one-year">1-year savings plan, license included</option>
        <option value="ahbsv-one-year">1-year savings plan, AHB</option>
      </select>
    </label>
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
            <select value={resource.instance_type} onchange={(event) => selectInstance(event.currentTarget.value)}>
              {#if !sourceInstances.some((item) => readString(item, 'instance_type') === resource.instance_type)}<option value={resource.instance_type}>{resource.instance_type}</option>{/if}
              {#each sourceInstances as item}<option value={readString(item, 'instance_type') ?? ''}>{readString(item, 'instance_type')}</option>{/each}
            </select>
          {:else}
            <input bind:value={resource.instance_type} oninput={onchange} placeholder="r6id.8xlarge" />
          {/if}
        </label>
      </div>
    </section>

    <section class="source-section">
      <div class="section-heading">
        <div>
          <span class="eyebrow">Storage</span>
          <h3>EBS volumes</h3>
        </div>
        <button class="compact-button" type="button" onclick={addVolume}><Plus size={16} /> Add volume</button>
      </div>
      {#if resource.volumes.length === 0}
        <p class="empty-note">No volumes configured.</p>
      {:else}
        <div class="volume-list">
          {#each resource.volumes as volume (volume.id)}
            <div class="volume-row">
              <label>
                <span>Label</span>
                <input bind:value={volume.label} oninput={onchange} />
              </label>
              <label>
                <span>Type</span>
                <select bind:value={volume.volume_type} onchange={() => normalizeVolume(volume)}>
                  {#if ebsTypes.length > 0}
                    {#each ebsTypes as item}
                      {@const priceUnavailable = readBoolean(item, 'price_required') === true && readBoolean(item, 'pricing_available') !== true}
                      <option value={readString(item, 'key') ?? ''}>{readString(item, 'label')}{priceUnavailable ? ' (price unavailable)' : ''}</option>
                    {/each}
                  {:else}
                    <option value="gp3">gp3</option>
                    <option value="io2">io2</option>
                    <option value="ephemeral">Instance storage</option>
                  {/if}
                </select>
              </label>
              <label>
                <span>Capacity (GB)</span>
                <input type="number" min="0" step="1" disabled={volume.volume_type === 'ephemeral'} bind:value={volume.capacity_gb} oninput={onchange} />
              </label>
              <label>
                <span>Provisioned IOPS</span>
                <input type="number" min="0" disabled={volume.volume_type === 'ephemeral'} bind:value={volume.provisioned_iops} oninput={onchange} />
              </label>
              <label>
                <span>Throughput (MiB/s)</span>
                <input type="number" min="0" disabled={volume.volume_type !== 'gp3'} bind:value={volume.throughput_mibps} oninput={onchange} />
              </label>
              <button class="icon" type="button" onclick={() => removeVolume(volume.id)} aria-label={`Remove ${volume.label}`} title="Remove volume">
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
        <div><span class="eyebrow">Managed source</span><h3>RDS configuration</h3></div>
      </div>
      <div class="field-grid">
        <label>
          <span>Instance type</span>
          {#if sourceInstances.length > 0}
            <select value={resource.instance_type} onchange={(event) => selectInstance(event.currentTarget.value)}>
              {#if !sourceInstances.some((item) => readString(item, 'instance_type') === resource.instance_type)}<option value={resource.instance_type}>{resource.instance_type}</option>{/if}
              {#each sourceInstances as item}<option value={readString(item, 'instance_type') ?? ''}>{readString(item, 'instance_type')}</option>{/each}
            </select>
          {:else}
            <input bind:value={resource.instance_type} oninput={onchange} placeholder="db.m6i.8xlarge" />
          {/if}
        </label>
        <label>
          <span>Deployment</span>
          <select bind:value={resource.deployment} onchange={() => { onchange(); oncatalogchange(); }}>
            <option value="single_az">Single-AZ</option>
            <option value="multi_az">Multi-AZ</option>
          </select>
        </label>
        {#if rdsOptions.length > 0}
          <label>
            <span>Commercial option</span>
            <select value={selectedRdsOption()} onchange={(event) => selectRdsOption(event.currentTarget.value)}>
              {#if selectedRdsOption() === ''}<option value="">{resource.commercial_term} · {resource.storage_class}</option>{/if}
              {#each rdsOptions as option, index}<option value={index}>{readString(option, 'commercial_term')} · {readString(option, 'storage_class')}</option>{/each}
            </select>
          </label>
        {:else}
          <label>
            <span>Commercial term</span>
            <input bind:value={resource.commercial_term} oninput={onchange} placeholder="on-demand" />
          </label>
          <label>
            <span>Storage class</span>
            <input bind:value={resource.storage_class} oninput={onchange} placeholder="gp3" />
          </label>
        {/if}
        <label>
          <span>Maximum source IOPS</span>
          <input type="number" min="0" bind:value={resource.source_max_iops} oninput={onchange} />
        </label>
      </div>
    </section>
  {:else}
    <section class="source-section">
      <div class="section-heading">
        <div><span class="eyebrow">Datacenter source</span><h3>On-premises configuration</h3></div>
      </div>
      <div class="field-grid">
        <label>
          <span>Source vCPU</span>
          <input type="number" min="1" bind:value={resource.source_vcpu} oninput={onchange} />
        </label>
        <label>
          <span>Licensable cores</span>
          <input type="number" min="1" bind:value={resource.licensable_cores} oninput={onchange} />
        </label>
        <label>
          <span>Maximum source IOPS</span>
          <input type="number" min="0" bind:value={resource.source_max_iops} oninput={onchange} />
        </label>
        <label>
          <span>Hardware capex (USD)</span>
          <input type="number" min="0" step="0.01" bind:value={resource.hardware_capex_usd} oninput={onchange} />
        </label>
        <label>
          <span>Depreciation years</span>
          <input type="number" min="0.01" step="0.01" bind:value={resource.depreciation_years} oninput={onchange} />
        </label>
        <label>
          <span>Average power override (kW)</span>
          <input type="number" min="0" step="0.01" bind:value={resource.average_power_kw_override} oninput={onchange} placeholder="Use server estimate" />
        </label>
      </div>
    </section>
  {/if}
</article>

<style>
  .resource-editor { overflow: hidden; background: #fff; border: 1px solid var(--line); border-radius: 6px; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 16px 18px; background: #f5f8f7; border-bottom: 1px solid var(--line); }
  .resource-title { display: flex; align-items: center; gap: 11px; min-width: 0; }
  .resource-icon { display: grid; flex: 0 0 36px; width: 36px; height: 36px; place-items: center; color: #075e54; background: #dceee9; }
  .eyebrow { display: block; margin-bottom: 3px; color: #617276; font: 700 0.68rem/1.2 Bahnschrift, sans-serif; letter-spacing: 0; text-transform: uppercase; }
  .name-input { width: min(440px, 60vw); padding: 0; color: #162c31; background: transparent; border: 0; border-bottom: 1px solid transparent; border-radius: 0; font: 650 1rem/1.3 Bahnschrift, sans-serif; }
  .name-input:focus { border-color: #087f73; outline: none; }
  .shared-fields { padding: 18px; }
  .field-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 14px; }
  .field-grid.compact { grid-template-columns: minmax(220px, 1fr) minmax(0, 2fr); }
  label { display: grid; align-content: start; gap: 6px; min-width: 0; color: #34494e; font-size: 0.79rem; font-weight: 700; }
  input, select { width: 100%; min-width: 0; min-height: 38px; box-sizing: border-box; padding: 7px 9px; color: #172d32; background: #fff; border: 1px solid #9caaad; border-radius: 4px; font: 400 0.9rem/1.3 Aptos, 'Trebuchet MS', sans-serif; }
  input:focus, select:focus { border-color: #087f73; outline: 2px solid #b9e1d9; outline-offset: 0; }
  input:disabled { color: #7d898b; background: #edf1f1; }
  .source-section { padding: 17px 18px 19px; border-top: 1px solid var(--line); }
  .section-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 13px; }
  h3 { margin: 0; color: #1a3338; font: 650 0.95rem/1.2 Bahnschrift, sans-serif; }
  button { font: inherit; cursor: pointer; }
  .icon { display: grid; flex: 0 0 34px; width: 34px; height: 34px; place-items: center; color: #43565a; background: transparent; border: 1px solid transparent; border-radius: 4px; }
  .icon:hover { background: #e9efee; border-color: #bcc8ca; }
  .icon.danger { color: #a72920; }
  .compact-button { display: inline-flex; align-items: center; gap: 6px; min-height: 34px; padding: 6px 10px; color: #075e54; background: #fff; border: 1px solid #6d9e96; border-radius: 4px; font-size: 0.82rem; font-weight: 700; }
  .volume-list { display: grid; gap: 8px; }
  .volume-row { display: grid; grid-template-columns: 1.2fr 0.8fr 0.8fr 0.9fr 0.9fr 34px; align-items: end; gap: 8px; padding: 11px; background: #f8faf9; border: 1px solid #d9e1e1; }
  .empty-note { margin: 0; color: #637478; font-size: 0.85rem; }
  @media (max-width: 900px) {
    .field-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .volume-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .volume-row .icon { align-self: end; }
  }
  @media (max-width: 580px) {
    .field-grid, .field-grid.compact, .volume-row { grid-template-columns: 1fr; }
    .shared-fields, .source-section { padding-inline: 14px; }
    .name-input { width: min(300px, 60vw); }
  }
</style>