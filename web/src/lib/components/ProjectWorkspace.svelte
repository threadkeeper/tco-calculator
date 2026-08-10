<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import {
    ArrowLeft,
    Calculator,
    CloudCog,
    Database,
    Plus,
    RotateCw,
    Save,
    ShieldCheck,
    Trash2
  } from 'lucide-svelte';
  import {
    ApiProblem,
    asRecord,
    readRecords,
    readString,
    requestJson,
    requestJsonResponse,
    type JsonRecord
  } from '$lib/api';
  import {
    clearGuestWorkspace,
    createResource,
    editableProject,
    saveGuestWorkspace,
    type GuestWorkspace
  } from '$lib/draft';
  import CalculationResults from './CalculationResults.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import ProblemBanner from './ProblemBanner.svelte';
  import ResourceEditor from './ResourceEditor.svelte';

  let {
    workspace,
    mode,
    projectId = null,
    etag = null,
    onclose,
    oncleared,
    onprojectsaved
  }: {
    workspace: GuestWorkspace;
    mode: 'guest' | 'authenticated';
    projectId?: string | null;
    etag?: string | null;
    onclose: () => void;
    oncleared: () => void;
    onprojectsaved: (id: string, etag: string | null, name: string) => void;
  } = $props();

  let currentProjectId = $state(projectId);
  let currentEtag = $state(etag);
  let dirty = $state(false);
  let saving = $state(false);
  let resolving = $state(false);
  let calculating = $state(false);
  let autosaveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  let problem = $state<string | null>(null);
  let catalogWarning = $state<string | null>(null);
  let sourceInstances = $state<JsonRecord[]>([]);
  let ebsTypes = $state<JsonRecord[]>([]);
  let rdsOptions = $state<Record<string, JsonRecord[]>>({});
  let confirmClear = $state(false);
  let autosaveTimer: ReturnType<typeof setTimeout> | null = null;

  const resourceLabel = $derived(
    workspace.project.settings.project_type === 'on_prem'
      ? 'server'
      : workspace.project.settings.project_type.toUpperCase()
  );

  function markDirty() {
    dirty = true;
    workspace.calculation = null;
    problem = null;
    if (mode !== 'guest') return;
    autosaveStatus = 'saving';
    if (autosaveTimer) clearTimeout(autosaveTimer);
    autosaveTimer = setTimeout(() => void persistGuest(), 450);
  }

  async function persistGuest() {
    try {
      await saveGuestWorkspace($state.snapshot(workspace));
      dirty = false;
      autosaveStatus = 'saved';
    } catch (error) {
      autosaveStatus = 'error';
      problem = messageFromError(error, 'The browser draft could not be saved.');
    }
  }

  async function saveProject() {
    if (mode === 'guest') {
      await persistGuest();
      return;
    }
    saving = true;
    problem = null;
    try {
      const path = currentProjectId
        ? `/api/v1/projects/${encodeURIComponent(currentProjectId)}`
        : '/api/v1/projects';
      const headers = new Headers();
      if (currentProjectId) {
        if (!currentEtag) throw new Error('The saved project ETag is unavailable. Reload the project.');
        headers.set('if-match', currentEtag);
      }
      const response = await requestJsonResponse(path, {
        method: currentProjectId ? 'PUT' : 'POST',
        headers,
        body: JSON.stringify(workspace.project)
      });
      const document = asRecord(response.payload);
      const savedProject = editableProject(document);
      if (!document || !savedProject) throw new Error('The project response was not recognized.');
      currentProjectId = readString(document, 'id');
      currentEtag = response.etag;
      workspace.project = savedProject;
      workspace.calculation = document.latest_calculation_revision ?? null;
      dirty = false;
      if (currentProjectId) onprojectsaved(currentProjectId, currentEtag, workspace.project.name);
    } catch (error) {
      problem = messageFromError(error, 'The project could not be saved.');
    } finally {
      saving = false;
    }
  }

  function addResource() {
    const resource = createResource(workspace.project.settings.project_type);
    workspace.project.resources = [
      ...workspace.project.resources,
      resource
    ];
    markDirty();
    if (resource.source_type === 'rds') void loadRdsOptions(resource);
  }

  function removeResource(resourceId: string) {
    workspace.project.resources = workspace.project.resources.filter(
      (resource) => resource.id !== resourceId
    );
    markDirty();
  }

  async function loadSourceCatalogs() {
    const project = workspace.project;
    if (project.settings.project_type === 'on_prem' || !project.settings.aws_region) {
      sourceInstances = [];
      ebsTypes = [];
      rdsOptions = {};
      return;
    }
    const region = encodeURIComponent(project.settings.aws_region);
    try {
      if (project.settings.project_type === 'ec2') {
        const [instances, volumes] = await Promise.all([
          requestJson(`/api/v1/catalog/aws/ec2/instances?region=${region}`),
          requestJson(`/api/v1/catalog/aws/ebs/types?region=${region}`)
        ]);
        sourceInstances = readRecords(asRecord(instances), 'items');
        ebsTypes = readRecords(asRecord(volumes), 'items');
        rdsOptions = {};
        catalogWarning = catalogMessage(instances) ?? catalogMessage(volumes);
      } else {
        const instances = await requestJson(`/api/v1/catalog/aws/rds/instances?region=${region}`);
        sourceInstances = readRecords(asRecord(instances), 'items');
        ebsTypes = [];
        catalogWarning = catalogMessage(instances);
        await Promise.all(
          project.resources
            .filter((resource) => resource.source_type === 'rds')
            .map((resource) => loadRdsOptions(resource))
        );
      }
    } catch (error) {
      sourceInstances = [];
      ebsTypes = [];
      rdsOptions = {};
      catalogWarning = messageFromError(error, 'The source catalog is unavailable.');
    }
  }

  async function loadRdsOptions(resource: Extract<(typeof workspace.project.resources)[number], { source_type: 'rds' }>) {
    const region = workspace.project.settings.aws_region;
    if (!region) return;
    try {
      const query = new URLSearchParams({
        region,
        instance_type: resource.instance_type,
        deployment: resource.deployment
      });
      const response = await requestJson(`/api/v1/catalog/aws/rds/options?${query.toString()}`);
      rdsOptions = { ...rdsOptions, [resource.id]: readRecords(asRecord(response), 'items') };
      catalogWarning ??= catalogMessage(response);
    } catch (error) {
      rdsOptions = { ...rdsOptions, [resource.id]: [] };
      catalogWarning = messageFromError(error, 'RDS commercial options are unavailable.');
    }
  }

  function catalogMessage(value: unknown): string | null {
    const record = asRecord(value);
    const warnings = Array.isArray(record?.warnings)
      ? record.warnings.filter((warning): warning is string => typeof warning === 'string')
      : [];
    return warnings[0] ?? null;
  }

  async function resolvePrices(): Promise<void> {
    resolving = true;
    problem = null;
    try {
      const project = workspace.project;
      const awsRequest =
        project.settings.project_type === 'on_prem'
          ? Promise.resolve<unknown>({ status: 'not_required' })
          : requestJson('/api/v1/pricing/aws/resolve', {
              method: 'POST',
              body: JSON.stringify({
                currency: project.settings.currency,
                aws_region: project.settings.aws_region,
                azure_region: project.settings.azure_region,
                resources: project.resources
              })
            });
      const azureRequest = requestJson('/api/v1/pricing/azure/resolve', {
        method: 'POST',
        body: JSON.stringify({
          currency: project.settings.currency,
          aws_region: project.settings.aws_region,
          azure_region: project.settings.azure_region,
          resources: project.resources
        })
      });
      const [awsOutcome, azureOutcome] = await Promise.allSettled([awsRequest, azureRequest]);
      const awsResolution =
        awsOutcome.status === 'fulfilled'
          ? awsOutcome.value
          : { status: 'unavailable', warnings: [messageFromError(awsOutcome.reason, 'AWS price resolution failed.')] };
      const azureResolution =
        azureOutcome.status === 'fulfilled'
          ? azureOutcome.value
          : { status: 'unavailable', warnings: [messageFromError(azureOutcome.reason, 'Azure price resolution failed.')] };
      workspace.aws_resolution = awsResolution;
      workspace.azure_resolution = azureResolution;
      const awsRecord = asRecord(workspace.aws_resolution);
      const azureRecord = asRecord(workspace.azure_resolution);
      workspace.project.aws_price_snapshot_id = readString(awsRecord, 'snapshot_id');
      workspace.project.azure_price_snapshot_id = readString(azureRecord, 'snapshot_id');
      const resolutionFailures = [awsOutcome, azureOutcome]
        .filter((outcome) => outcome.status === 'rejected')
        .map((outcome) => messageFromError(outcome.reason, 'Price resolution failed.'));
      if (resolutionFailures.length > 0) problem = resolutionFailures.join(' ');
      if (mode === 'guest') await persistGuest();
    } catch (error) {
      problem = messageFromError(error, 'Price resolution failed.');
    } finally {
      resolving = false;
    }
  }

  async function calculate() {
    if (workspace.project.resources.length === 0) {
      problem = `Add at least one ${resourceLabel.toLowerCase()} workload before calculating.`;
      return;
    }
    calculating = true;
    problem = null;
    try {
      await resolvePrices();
      workspace.calculation = await requestJson('/api/v1/calculations', {
        method: 'POST',
        body: JSON.stringify(workspace.project)
      });
      dirty = mode === 'authenticated';
      if (mode === 'guest') await persistGuest();
    } catch (error) {
      if (!problem) problem = messageFromError(error, 'The estimate could not be calculated.');
    } finally {
      calculating = false;
    }
  }

  async function clearLocalData() {
    try {
      await clearGuestWorkspace();
      confirmClear = false;
      oncleared();
    } catch (error) {
      problem = messageFromError(error, 'The local browser draft could not be cleared.');
    }
  }

  function resolutionState(value: unknown, required: boolean): string {
    const status = readString(asRecord(value), 'status');
    if (status) return status.replaceAll('_', ' ');
    return required ? 'not resolved' : 'not required';
  }

  function messageFromError(error: unknown, fallback: string): string {
    if (error instanceof ApiProblem && error.requestId) return `${error.message} Request ${error.requestId}`;
    return error instanceof Error ? error.message : fallback;
  }

  onDestroy(() => {
    if (autosaveTimer) clearTimeout(autosaveTimer);
  });

  onMount(() => {
    void loadSourceCatalogs();
  });
</script>

<div class="workspace">
  <div class="workspace-bar">
    <button class="back" type="button" onclick={onclose}><ArrowLeft size={18} /> Projects</button>
    <div class="title-block">
      <span>{mode === 'guest' ? 'Browser-local draft' : currentProjectId ? 'Saved project' : 'Unsaved project'}</span>
      <h1>{workspace.project.name}</h1>
    </div>
    <div class="bar-actions">
      {#if mode === 'guest'}
        <span class:error-state={autosaveStatus === 'error'} class="save-state">
          {autosaveStatus === 'saving' ? 'Saving locally…' : autosaveStatus === 'saved' ? 'Saved locally' : autosaveStatus === 'error' ? 'Local save failed' : 'Local draft'}
        </span>
      {:else if dirty}
        <span class="save-state">Unsaved changes</span>
      {/if}
      <button class="secondary" type="button" onclick={saveProject} disabled={saving}>
        <Save size={17} /> {saving ? 'Saving…' : mode === 'guest' ? 'Save draft' : 'Save project'}
      </button>
      <button class="primary" type="button" onclick={calculate} disabled={calculating || resolving}>
        <Calculator size={17} /> {calculating ? 'Calculating…' : 'Calculate estimate'}
      </button>
    </div>
  </div>

  <main>
    {#if problem}<ProblemBanner message={problem} ondismiss={() => (problem = null)} />{/if}

    <section class="scope-band" aria-label="Project pricing scope">
      <div class="scope-heading">
        <div>
          <span class="eyebrow">Comparison scope</span>
          <h2>{workspace.project.settings.project_type === 'on_prem' ? 'Datacenter' : 'AWS'} to Azure SQL Managed Instance</h2>
        </div>
        <button class="secondary compact" type="button" onclick={resolvePrices} disabled={resolving}>
          <RotateCw size={16} class:spin={resolving} /> {resolving ? 'Resolving…' : 'Resolve prices'}
        </button>
      </div>
      <div class="scope-grid">
        <div class="scope-cell source-scope">
          <CloudCog size={19} />
          <div><span>Source region</span><strong>{workspace.project.settings.aws_region ?? 'On premises'}</strong></div>
          <span class="resolution">{resolutionState(workspace.aws_resolution, workspace.project.settings.project_type !== 'on_prem')}</span>
        </div>
        <div class="scope-arrow" aria-hidden="true">→</div>
        <div class="scope-cell target-scope">
          <ShieldCheck size={19} />
          <div><span>Azure region</span><strong>{workspace.project.settings.azure_region}</strong></div>
          <span class="resolution">{resolutionState(workspace.azure_resolution, true)}</span>
        </div>
      </div>
      {#if catalogWarning}<p class="catalog-warning">{catalogWarning}</p>{/if}
    </section>

    <details class="settings-panel">
      <summary>Project settings</summary>
      <div class="settings-grid">
        <label><span>Project name</span><input bind:value={workspace.project.name} oninput={markDirty} /></label>
        <label><span>Description</span><input bind:value={workspace.project.description} oninput={markDirty} placeholder="Optional" /></label>
        {#if workspace.project.settings.project_type !== 'on_prem'}
          <label><span>AWS region</span><input bind:value={workspace.project.settings.aws_region} oninput={markDirty} onchange={() => void loadSourceCatalogs()} /></label>
        {/if}
        <label><span>Azure region</span><input bind:value={workspace.project.settings.azure_region} oninput={markDirty} /></label>
        <label><span>Source compute discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.source_compute_discount} oninput={markDirty} /></label>
        <label><span>Source license discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.source_license_discount} oninput={markDirty} /></label>
        <label><span>Source storage discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.source_storage_discount} oninput={markDirty} /></label>
        <label><span>Azure compute discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.azure_compute_discount} oninput={markDirty} /></label>
        <label><span>Azure license discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.azure_license_discount} oninput={markDirty} /></label>
        <label><span>Azure storage discount</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.azure_storage_discount} oninput={markDirty} /></label>
        <label><span>Selected parity adjustment</span><input type="number" min="0" max="1" step="0.01" bind:value={workspace.project.settings.selected_parity_adjustment} oninput={markDirty} /></label>
        {#if workspace.project.settings.project_type === 'on_prem'}
          <label><span>Enterprise License + SA / two-core pack</span><input type="number" min="0" step="0.01" bind:value={workspace.project.settings.enterprise_license_sa_usd_per_two_core_pack} oninput={markDirty} /></label>
          <label><span>Standard License + SA / two-core pack</span><input type="number" min="0" step="0.01" bind:value={workspace.project.settings.standard_license_sa_usd_per_two_core_pack} oninput={markDirty} /></label>
          <label><span>Remaining coverage months</span><select bind:value={workspace.project.settings.remaining_coverage_months} onchange={markDirty}><option value={12}>12</option><option value={24}>24</option><option value={36}>36</option></select></label>
          <label><span>Electricity rate (USD/kWh)</span><input type="number" min="0" step="0.0001" bind:value={workspace.project.settings.electricity_rate_usd_per_kwh} oninput={markDirty} /></label>
        {/if}
      </div>
    </details>

    <section class="resources" aria-labelledby="resources-heading">
      <div class="section-header">
        <div><span class="eyebrow">Inventory</span><h2 id="resources-heading">{resourceLabel} workloads</h2></div>
        <button class="secondary" type="button" onclick={addResource}><Plus size={17} /> Add {resourceLabel}</button>
      </div>

      {#if workspace.project.resources.length === 0}
        <div class="empty-resources">
          <Database size={28} aria-hidden="true" />
          <h3>No workloads yet</h3>
          <button class="primary" type="button" onclick={addResource}><Plus size={17} /> Add first {resourceLabel}</button>
        </div>
      {:else}
        <div class="resource-list">
          {#each workspace.project.resources as resource (resource.id)}
            <ResourceEditor
              {resource}
              {sourceInstances}
              {ebsTypes}
              rdsOptions={rdsOptions[resource.id] ?? []}
              onchange={markDirty}
              oncatalogchange={() => resource.source_type === 'rds' && void loadRdsOptions(resource)}
              onremove={() => removeResource(resource.id)}
            />
          {/each}
        </div>
      {/if}
    </section>

    {#if workspace.calculation}
      <CalculationResults calculation={workspace.calculation} resources={workspace.project.resources} />
    {/if}

    {#if mode === 'guest'}
      <section class="local-data">
        <div><span class="eyebrow">Local privacy control</span><h2>Browser data</h2><p>This draft and its latest estimate are stored only in this browser profile.</p></div>
        <button class="danger-button" type="button" onclick={() => (confirmClear = true)}><Trash2 size={17} /> Clear local data</button>
      </section>
    {/if}
  </main>
</div>

<ConfirmDialog
  open={confirmClear}
  title="Clear local draft?"
  message="This permanently deletes the project inputs and latest estimate stored in this browser."
  confirmLabel="Clear local data"
  onconfirm={clearLocalData}
  oncancel={() => (confirmClear = false)}
/>

<style>
  .workspace { min-height: calc(100vh - 56px); background: #f1f4f3; }
  .workspace-bar { position: sticky; top: 56px; z-index: 8; display: grid; grid-template-columns: auto minmax(180px, 1fr) auto; align-items: center; gap: 16px; min-height: 66px; padding: 8px max(20px, calc((100vw - 1240px) / 2)); color: #eaf5f2; background: #17363a; border-bottom: 1px solid #2f5256; }
  .back { display: inline-flex; align-items: center; gap: 6px; padding: 7px 8px; color: #c9dbd7; background: transparent; border: 0; border-radius: 4px; font: inherit; cursor: pointer; }
  .back:hover { color: #fff; background: #25474b; }
  .title-block { min-width: 0; }
  .title-block span { display: block; color: #9fb9b4; font-size: 0.68rem; font-weight: 700; text-transform: uppercase; }
  .title-block h1 { overflow: hidden; margin: 2px 0 0; font: 650 1rem/1.2 Bahnschrift, sans-serif; text-overflow: ellipsis; white-space: nowrap; }
  .bar-actions { display: flex; align-items: center; justify-content: end; gap: 8px; }
  .save-state { color: #afc4c0; font-size: 0.72rem; }
  .save-state.error-state { color: #ffb4a8; }
  main { width: min(1240px, calc(100% - 32px)); margin: 0 auto; padding: 22px 0 42px; }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 7px; min-height: 37px; padding: 7px 12px; border-radius: 4px; font: 700 0.82rem/1 Aptos, 'Trebuchet MS', sans-serif; cursor: pointer; }
  button:disabled { cursor: wait; opacity: 0.62; }
  .primary { color: #fff; background: #087f73; border: 1px solid #087f73; }
  .primary:hover:not(:disabled) { background: #076c63; }
  .secondary { color: #24454a; background: #fff; border: 1px solid #8ba09f; }
  .workspace-bar .secondary { color: #eaf5f2; background: transparent; border-color: #64827f; }
  .compact { min-height: 34px; padding: 6px 10px; }
  .scope-band { margin-top: 14px; padding: 18px 20px 20px; background: #fff; border: 1px solid var(--line); border-top: 3px solid #d39b21; }
  .scope-heading, .section-header, .local-data { display: flex; align-items: center; justify-content: space-between; gap: 18px; }
  .eyebrow { display: block; margin-bottom: 4px; color: #5d7473; font: 700 0.68rem/1.2 Bahnschrift, sans-serif; text-transform: uppercase; }
  h2 { margin: 0; color: #173338; font: 680 1.12rem/1.25 Bahnschrift, sans-serif; }
  .scope-grid { display: grid; grid-template-columns: 1fr auto 1fr; align-items: stretch; gap: 9px; margin-top: 15px; }
  .scope-cell { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 11px; min-height: 62px; padding: 10px 13px; background: #f6f8f7; border: 1px solid #d6dfde; }
  .source-scope { color: #9b5a13; }
  .target-scope { color: #087f73; }
  .scope-cell span { display: block; color: #667a7e; font-size: 0.69rem; font-weight: 700; text-transform: uppercase; }
  .scope-cell strong { color: #20383d; font-size: 0.9rem; }
  .scope-cell .resolution { padding: 4px 6px; color: #445a5d; background: #e6eceb; border-radius: 3px; font-size: 0.64rem; }
  .scope-arrow { display: grid; place-items: center; color: #738582; font-size: 1.3rem; }
  .catalog-warning { margin: 10px 0 0; color: #725515; font-size: 0.78rem; }
  .settings-panel { margin-top: 12px; background: #fff; border: 1px solid var(--line); }
  .settings-panel summary { padding: 12px 15px; color: #2c4c50; font: 700 0.86rem/1.2 Bahnschrift, sans-serif; cursor: pointer; }
  .settings-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 13px; padding: 15px; border-top: 1px solid var(--line); }
  label { display: grid; gap: 6px; min-width: 0; color: #344b50; font-size: 0.76rem; font-weight: 700; }
  input, select { width: 100%; min-width: 0; min-height: 38px; box-sizing: border-box; padding: 7px 9px; color: #172e33; background: #fff; border: 1px solid #98a8aa; border-radius: 4px; font: 400 0.9rem/1.3 Aptos, 'Trebuchet MS', sans-serif; }
  input:focus, select:focus { border-color: #087f73; outline: 2px solid #bae0d9; }
  .resources { margin-top: 22px; }
  .section-header { margin-bottom: 12px; }
  .resource-list { display: grid; gap: 14px; }
  .empty-resources { display: grid; justify-items: center; gap: 9px; padding: 45px 20px; color: #69807d; background: #fff; border: 1px dashed #a9bab7; }
  .empty-resources h3 { margin: 0; color: #29454a; font: 650 1rem/1.2 Bahnschrift, sans-serif; }
  .local-data { margin-top: 22px; padding: 17px 19px; background: #fff; border: 1px solid var(--line); }
  .local-data p { margin: 5px 0 0; color: #64767a; font-size: 0.82rem; }
  .danger-button { flex: 0 0 auto; color: #a62a20; background: #fff; border: 1px solid #ce8f88; }
  .spin { animation: spin 0.9s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 900px) {
    .workspace-bar { grid-template-columns: auto 1fr; top: 52px; }
    .bar-actions { grid-column: 1 / -1; justify-content: stretch; padding-bottom: 5px; }
    .bar-actions button { flex: 1; }
    .settings-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
  @media (max-width: 620px) {
    main { width: min(100% - 20px, 1240px); padding-top: 12px; }
    .workspace-bar { gap: 9px; padding-inline: 10px; }
    .save-state { display: none; }
    .scope-heading, .section-header, .local-data { align-items: stretch; flex-direction: column; }
    .scope-grid { grid-template-columns: 1fr; }
    .scope-arrow { min-height: 18px; transform: rotate(90deg); }
    .scope-cell { grid-template-columns: auto 1fr; }
    .scope-cell .resolution { grid-column: 2; justify-self: start; }
    .settings-grid { grid-template-columns: 1fr; }
    .danger-button { width: 100%; }
  }
</style>