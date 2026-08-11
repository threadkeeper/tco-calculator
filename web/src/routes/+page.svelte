<script lang="ts">
  import { onMount } from 'svelte';
  import { Building2, Cloud, Database, FileClock, Plus, Trash2 } from 'lucide-svelte';
  import {
    ApiProblem,
    asRecord,
    asRecords,
    readBoolean,
    readRecord,
    readString,
    requestJson,
    requestJsonResponse,
    type JsonRecord
  } from '$lib/api';
  import AppShell from '$lib/components/AppShell.svelte';
  import AssistantPanel from '$lib/components/AssistantPanel.svelte';
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import ProblemBanner from '$lib/components/ProblemBanner.svelte';
  import PrivacyNotice from '$lib/components/PrivacyNotice.svelte';
  import ProjectList from '$lib/components/ProjectList.svelte';
  import ProjectWorkspace from '$lib/components/ProjectWorkspace.svelte';
  import SearchSelect from '$lib/components/SearchSelect.svelte';
  import {
    clearGuestWorkspace,
    createGuestWorkspace,
    createProjectDraft,
    createResource,
    editableProject,
    loadGuestWorkspace,
    saveGuestWorkspace,
    type GuestWorkspace,
    type ProjectType
  } from '$lib/draft';
  import { projectShareFromFragment, type ProjectShareCredentials } from '$lib/project-share';
  import {
    DEFAULT_AWS_REGION,
    DEFAULT_AZURE_REGION,
    readRegionOptions,
    type RegionOption
  } from '$lib/regions';

  type SessionMode = 'loading' | 'guest' | 'authenticated' | 'offline';

  let mode = $state<SessionMode>('loading');
  let displayName = $state<string | null>(null);
  let projects = $state<JsonRecord[]>([]);
  let loadingProjects = $state(false);
  let availableGuestWorkspace = $state<GuestWorkspace | null>(null);
  let activeWorkspace = $state<GuestWorkspace | null>(null);
  let activeProjectId = $state<string | null>(null);
  let activeEtag = $state<string | null>(null);
  let showSetup = $state(false);
  let setupName = $state('SQL TCO estimate');
  let setupDescription = $state('');
  let setupType = $state<ProjectType>('ec2');
  let setupAwsRegion = $state('eu-west-1');
  let setupAzureRegion = $state('swedencentral');
  let awsRegions = $state<RegionOption[]>([DEFAULT_AWS_REGION]);
  let azureRegions = $state<RegionOption[]>([DEFAULT_AZURE_REGION]);
  let creating = $state(false);
  let problem = $state<string | null>(null);
  let deleteTarget = $state<{ id: string; name: string } | null>(null);
  let clearLocalConfirm = $state(false);
  let privacyOpen = $state(false);
  let privacyRequired = $state(false);
  let privacyNoticeVersion = $state('');
  let privacyAcceptedAt = $state<string | null>(null);
  let privacyAllowContact = $state(false);
  let privacyEmailAddress = $state<string | null>(null);
  let identityEmailAddress = $state<string | null>(null);
  let privacySaving = $state(false);
  let privacyProblem = $state<string | null>(null);
  let pendingSharedCredentials = $state<ProjectShareCredentials | null>(null);

  onMount(() => {
    void initialize();
    void loadRegionCatalogs();
  });

  async function loadRegionCatalogs() {
    const [awsCatalog, azureCatalog] = await Promise.allSettled([
      requestJson('/api/v1/catalog/aws/regions'),
      requestJson('/api/v1/catalog/azure/regions')
    ]);
    if (awsCatalog.status === 'fulfilled')
      awsRegions = readRegionOptions(awsCatalog.value, awsRegions);
    if (azureCatalog.status === 'fulfilled')
      azureRegions = readRegionOptions(azureCatalog.value, azureRegions);
  }

  async function initialize() {
    const hasShareFragment = new URLSearchParams(window.location.hash.slice(1)).has('share');
    const sharedCredentials = projectShareFromFragment(window.location.hash);
    if (hasShareFragment) history.replaceState(null, '', `${location.pathname}${location.search}`);
    try {
      availableGuestWorkspace = await loadGuestWorkspace();
    } catch {
      problem = 'The browser-local draft store could not be opened.';
    }

    try {
      const session = asRecord(await requestJson('/api/v1/session'));
      const sessionMode = readString(session, 'mode');
      const consent = readRecord(session, 'privacy_consent');
      mode = sessionMode === 'authenticated' ? 'authenticated' : 'guest';
      displayName = readString(session, 'display_name');
      privacyNoticeVersion = readString(consent, 'notice_version') ?? '';
      if (mode === 'authenticated') {
        privacyRequired = readBoolean(consent, 'required') !== false;
        privacyAcceptedAt = readString(consent, 'accepted_at');
        privacyAllowContact = readBoolean(consent, 'allow_contact') === true;
        identityEmailAddress = readString(session, 'email_address');
        privacyEmailAddress = readString(consent, 'email_address') ?? identityEmailAddress;
        pendingSharedCredentials = sharedCredentials;
        if (privacyRequired) {
          privacyOpen = true;
        } else {
          await loadProjects();
          if (sharedCredentials) {
            pendingSharedCredentials = null;
            await openSharedProject(sharedCredentials);
          }
        }
      } else if (sharedCredentials) {
        problem = 'Sign in with Microsoft, then open the shared project link again.';
      }
      if (hasShareFragment && !sharedCredentials) problem = 'The shared project link is not valid.';
    } catch (error) {
      mode = 'offline';
      if (!(error instanceof TypeError))
        problem = messageFromError(error, 'The application session is unavailable.');
    }
  }

  async function acceptPrivacy(allowContact: boolean, emailAddress: string | null) {
    privacySaving = true;
    privacyProblem = null;
    try {
      const consent = asRecord(
        await requestJson('/api/v1/privacy-consent', {
          method: 'PUT',
          body: JSON.stringify({
            notice_version: privacyNoticeVersion,
            accepted: true,
            allow_contact: allowContact,
            email_address: emailAddress
          })
        })
      );
      privacyNoticeVersion = readString(consent, 'notice_version') ?? privacyNoticeVersion;
      privacyRequired = readBoolean(consent, 'required') !== false;
      privacyAcceptedAt = readString(consent, 'accepted_at');
      privacyAllowContact = readBoolean(consent, 'allow_contact') === true;
      privacyEmailAddress = readString(consent, 'email_address') ?? identityEmailAddress;
      if (privacyRequired) throw new Error('The current privacy notice was not accepted.');
      privacyOpen = false;
      await loadProjects();
      if (pendingSharedCredentials) {
        const credentials = pendingSharedCredentials;
        pendingSharedCredentials = null;
        await openSharedProject(credentials);
      }
    } catch (error) {
      privacyProblem = messageFromError(error, 'Privacy acceptance could not be saved.');
    } finally {
      privacySaving = false;
    }
  }

  async function openSharedProject(credentials: ProjectShareCredentials) {
    try {
      const payload = await requestJson('/api/v1/project-shares/resolve', {
        method: 'POST',
        body: JSON.stringify(credentials)
      });
      const project = editableProject(payload);
      if (!project) throw new Error('The shared project response was not recognized.');
      activeWorkspace = createGuestWorkspace(project);
      activeProjectId = null;
      activeEtag = null;
      showSetup = false;
    } catch (error) {
      problem = messageFromError(error, 'The shared project could not be opened.');
    }
  }

  async function loadProjects() {
    loadingProjects = true;
    try {
      projects = asRecords(await requestJson('/api/v1/projects'));
    } catch (error) {
      problem = messageFromError(error, 'Saved projects could not be loaded.');
    } finally {
      loadingProjects = false;
    }
  }

  async function createProject() {
    if (!setupName.trim()) {
      problem = 'Project name is required.';
      return;
    }
    creating = true;
    problem = null;
    try {
      const project = createProjectDraft(
        setupType,
        setupName.trim(),
        setupDescription.trim() || null,
        setupAwsRegion.trim(),
        setupAzureRegion.trim()
      );
      project.resources = [createResource(setupType)];
      const workspace = createGuestWorkspace(project);

      if (mode === 'authenticated' && setupType !== 'on_prem') {
        const response = await requestJsonResponse('/api/v1/projects', {
          method: 'POST',
          body: JSON.stringify(project)
        });
        openDocument(response.payload, response.etag);
      } else if (mode === 'authenticated') {
        activeWorkspace = workspace;
        activeProjectId = null;
        activeEtag = null;
      } else {
        await saveGuestWorkspace(workspace);
        availableGuestWorkspace = workspace;
        activeWorkspace = workspace;
        activeProjectId = null;
        activeEtag = null;
      }
      showSetup = false;
    } catch (error) {
      problem = messageFromError(error, 'The project could not be created.');
    } finally {
      creating = false;
    }
  }

  async function openProject(id: string) {
    problem = null;
    try {
      const response = await requestJsonResponse(`/api/v1/projects/${encodeURIComponent(id)}`);
      openDocument(response.payload, response.etag);
    } catch (error) {
      problem = messageFromError(error, 'The project could not be opened.');
    }
  }

  function openDocument(payload: unknown, etag: string | null) {
    const document = asRecord(payload);
    const project = editableProject(document);
    const id = readString(document, 'id');
    if (!document || !project || !id) throw new Error('The project response was not recognized.');
    activeWorkspace = {
      project,
      calculation: document.latest_calculation_revision ?? null,
      aws_resolution: null,
      azure_resolution: null,
      updated_at: readString(document, 'updated_at') ?? new Date().toISOString()
    };
    activeProjectId = id;
    activeEtag = etag;
    showSetup = false;
  }

  async function deleteProject() {
    if (!deleteTarget) return;
    try {
      await requestJson(`/api/v1/projects/${encodeURIComponent(deleteTarget.id)}`, {
        method: 'DELETE'
      });
      deleteTarget = null;
      await loadProjects();
    } catch (error) {
      problem = messageFromError(error, 'The project could not be deleted.');
    }
  }

  async function clearLocalDraft() {
    try {
      await clearGuestWorkspace();
      availableGuestWorkspace = null;
      activeWorkspace = null;
      clearLocalConfirm = false;
    } catch (error) {
      problem = messageFromError(error, 'The browser-local draft could not be cleared.');
    }
  }

  function closeWorkspace() {
    if (mode === 'authenticated') void loadProjects();
    else if (activeWorkspace) availableGuestWorkspace = activeWorkspace;
    activeWorkspace = null;
    activeProjectId = null;
    activeEtag = null;
  }

  function projectSaved(id: string, etag: string | null) {
    activeProjectId = id;
    activeEtag = etag;
  }

  function formatDate(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? value
      : new Intl.DateTimeFormat('en', { dateStyle: 'medium', timeStyle: 'short' }).format(date);
  }

  function messageFromError(error: unknown, fallback: string): string {
    if (error instanceof ApiProblem && error.requestId)
      return `${error.message} Request ${error.requestId}`;
    return error instanceof Error ? error.message : fallback;
  }
</script>

<svelte:head>
  <title>Azure SQL TCO</title>
  <meta
    name="description"
    content="Compare current SQL Server costs with an Azure SQL Managed Instance estimate."
  />
</svelte:head>

<AppShell
  {mode}
  {displayName}
  currentProject={activeWorkspace?.project.name ?? null}
  onprivacy={() => {
    privacyProblem = null;
    privacyOpen = true;
  }}
>
  {#if activeWorkspace}
    <ProjectWorkspace
      workspace={activeWorkspace}
      mode={mode === 'authenticated' ? 'authenticated' : 'guest'}
      projectId={activeProjectId}
      etag={activeEtag}
      onclose={closeWorkspace}
      oncleared={() => {
        availableGuestWorkspace = null;
        activeWorkspace = null;
      }}
      onprojectsaved={projectSaved}
    />
  {:else}
    <main class="home">
      {#if problem}<ProblemBanner message={problem} ondismiss={() => (problem = null)} />{/if}
      {#if mode === 'offline'}
        <aside class="offline-notice">
          <strong>API offline</strong><span
            >Draft inputs can still be stored in this browser. Price resolution and calculations
            require the local API.</span
          >
        </aside>
      {/if}

      {#if showSetup}
        <section class="setup" aria-labelledby="setup-heading">
          <div class="setup-heading">
            <div>
              <p class="eyebrow">New estimate</p>
              <h1 id="setup-heading">Set the comparison scope</h1>
            </div>
            <button class="text-button" type="button" onclick={() => (showSetup = false)}
              >Cancel</button
            >
          </div>
          <form
            onsubmit={(event) => {
              event.preventDefault();
              void createProject();
            }}
          >
            <fieldset>
              <legend>Source estate</legend>
              <div class="source-options">
                <button
                  type="button"
                  class:selected={setupType === 'ec2'}
                  onclick={() => (setupType = 'ec2')}
                  ><Cloud size={22} /><b>Amazon EC2</b><span
                    >Windows SQL Server instances and EBS</span
                  ></button
                >
                <button
                  type="button"
                  class:selected={setupType === 'rds'}
                  onclick={() => (setupType = 'rds')}
                  ><Database size={22} /><b>Amazon RDS</b><span
                    >Managed SQL Server instances and storage</span
                  ></button
                >
                <button
                  type="button"
                  class:selected={setupType === 'on_prem'}
                  onclick={() => (setupType = 'on_prem')}
                  ><Building2 size={22} /><b>On premises</b><span
                    >Hardware, licensing, and electricity</span
                  ></button
                >
              </div>
            </fieldset>
            <div class="setup-fields">
              <label
                ><span>Project name</span><input
                  required
                  maxlength="120"
                  bind:value={setupName}
                /></label
              >
              <label
                ><span>Description</span><input
                  maxlength="500"
                  bind:value={setupDescription}
                  placeholder="Optional context"
                /></label
              >
              {#if setupType !== 'on_prem'}
                <div class="region-field">
                  <SearchSelect
                    id="setup-aws-region"
                    label="AWS region"
                    options={awsRegions}
                    bind:value={setupAwsRegion}
                    required
                  />
                </div>
              {/if}
              <div class="region-field">
                <SearchSelect
                  id="setup-azure-region"
                  label="Azure region"
                  options={azureRegions}
                  bind:value={setupAzureRegion}
                  required
                />
              </div>
            </div>
            <div class="setup-actions">
              <span
                >{mode === 'authenticated'
                  ? 'This project will be saved to your account.'
                  : 'This draft will stay in this browser.'}</span
              >
              <button class="button primary" type="submit" disabled={creating}
                ><Plus size={18} /> {creating ? 'Creating…' : 'Create project'}</button
              >
            </div>
          </form>
        </section>
      {:else if mode === 'authenticated'}
        <ProjectList
          {projects}
          loading={loadingProjects}
          onnew={() => (showSetup = true)}
          onopen={(id) => void openProject(id)}
          ondelete={(id, name) => (deleteTarget = { id, name })}
        />
      {:else}
        <section class="title-band" aria-labelledby="draft-heading">
          <div>
            <p class="eyebrow">Browser workspace</p>
            <h1 id="draft-heading">Local estimate</h1>
          </div>
          <button class="button primary" type="button" onclick={() => (showSetup = true)}
            ><Plus size={18} /> New local estimate</button
          >
        </section>
        <aside class="guest-notice">
          <strong>Guest draft</strong><span
            >Inputs and the latest result stay in IndexedDB on this device until you clear them.</span
          >
        </aside>
        {#if availableGuestWorkspace}
          <section class="draft-row">
            <span class="draft-icon"><FileClock size={22} /></span>
            <div>
              <h2>{availableGuestWorkspace.project.name}</h2>
              <p>
                {availableGuestWorkspace.project.settings.project_type
                  .replace('_', ' ')
                  .toUpperCase()} · Updated {formatDate(availableGuestWorkspace.updated_at)}
              </p>
            </div>
            <button
              class="button open"
              type="button"
              onclick={() => (activeWorkspace = availableGuestWorkspace)}>Continue draft</button
            >
            <button
              class="icon-delete"
              type="button"
              onclick={() => (clearLocalConfirm = true)}
              aria-label="Clear local draft"
              title="Clear local draft"><Trash2 size={17} /></button
            >
          </section>
        {:else}
          <section class="new-draft">
            <Database size={28} aria-hidden="true" />
            <h2>No local estimate</h2>
            <p>Create a source inventory to begin.</p>
            <button class="button primary" type="button" onclick={() => (showSetup = true)}
              ><Plus size={18} /> Create estimate</button
            >
          </section>
        {/if}
      {/if}
    </main>
  {/if}
</AppShell>

{#if mode === 'guest' || (mode === 'authenticated' && !privacyRequired)}
  <AssistantPanel />
{/if}

{#if privacyOpen}
  <PrivacyNotice
    required={privacyRequired}
    authenticated={mode === 'authenticated'}
    noticeVersion={privacyNoticeVersion}
    acceptedAt={privacyAcceptedAt}
    initialAllowContact={privacyAllowContact}
    initialEmailAddress={privacyEmailAddress ?? identityEmailAddress}
    saving={privacySaving}
    error={privacyProblem}
    onaccept={(allowContact, emailAddress) => void acceptPrivacy(allowContact, emailAddress)}
    onclose={() => {
      if (!privacyRequired) privacyOpen = false;
    }}
  />
{/if}

<ConfirmDialog
  open={deleteTarget !== null}
  title="Delete saved project?"
  message={`This permanently deletes ${deleteTarget?.name ?? 'the project'} and its latest calculation revision.`}
  confirmLabel="Delete project"
  onconfirm={deleteProject}
  oncancel={() => (deleteTarget = null)}
/>
<ConfirmDialog
  open={clearLocalConfirm}
  title="Clear local draft?"
  message="This permanently deletes the project inputs and latest estimate stored in this browser."
  confirmLabel="Clear local data"
  onconfirm={clearLocalDraft}
  oncancel={() => (clearLocalConfirm = false)}
/>

<style>
  .home {
    width: min(100% - 32px, 1540px);
    margin: 0 auto;
    padding: 26px 0 48px;
  }
  .offline-notice {
    display: flex;
    gap: 12px;
    margin-bottom: 14px;
    padding: 11px 14px;
    color: #634800;
    background: #fff5d5;
    border: 1px solid #e2c15e;
    font-size: 0.86rem;
  }
  .setup {
    max-width: 980px;
    margin: 0 auto;
    background: #fff;
    border: 1px solid var(--border);
    border-top: 4px solid var(--source);
    box-shadow: var(--shadow);
  }
  .setup-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 19px 22px;
    border-bottom: 1px solid var(--border);
  }
  .text-button {
    padding: 6px 8px;
    color: #496167;
    background: transparent;
    border: 0;
    font-weight: 700;
    cursor: pointer;
  }
  form {
    padding: 22px;
  }
  fieldset {
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
  legend {
    margin-bottom: 10px;
    color: #40565a;
    font-size: 0.8rem;
    font-weight: 720;
  }
  .source-options {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 9px;
  }
  .source-options button {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 3px 9px;
    min-height: 90px;
    padding: 13px;
    color: #456068;
    background: #f8faf9;
    border: 1px solid #bac7c9;
    border-radius: 5px;
    text-align: left;
    cursor: pointer;
  }
  .source-options button b {
    color: #223b40;
    font:
      650 0.93rem/1.2 Bahnschrift,
      sans-serif;
  }
  .source-options button span {
    grid-column: 2;
    color: #687a7e;
    font-size: 0.76rem;
  }
  .source-options button.selected {
    color: #087f73;
    background: #e7f3f0;
    border: 2px solid #087f73;
    padding: 12px;
  }
  .setup-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-top: 20px;
  }
  .region-field {
    min-width: 0;
  }
  label {
    display: grid;
    gap: 6px;
    color: #374e53;
    font-size: 0.78rem;
    font-weight: 700;
  }
  input {
    width: 100%;
    min-height: 40px;
    padding: 8px 10px;
    color: #162e33;
    background: #fff;
    border: 1px solid #96a7aa;
    border-radius: 4px;
    font:
      400 0.92rem/1.3 Aptos,
      'Trebuchet MS',
      sans-serif;
  }
  input:focus {
    border-color: #087f73;
    outline: 2px solid #bae0d9;
  }
  .setup-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin-top: 22px;
    padding-top: 16px;
    border-top: 1px solid #dbe2e2;
  }
  .setup-actions span {
    color: #627579;
    font-size: 0.8rem;
  }
  .draft-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 14px;
    min-height: 82px;
    padding: 13px 15px;
    background: #fff;
    border: 1px solid var(--border);
    border-left: 4px solid #087f73;
    box-shadow: var(--shadow);
  }
  .draft-icon {
    display: grid;
    width: 40px;
    height: 40px;
    place-items: center;
    color: #087f73;
    background: #e3f1ed;
  }
  .draft-row h2,
  .new-draft h2 {
    margin: 0;
    color: #20383d;
    font:
      650 1rem/1.2 Bahnschrift,
      sans-serif;
  }
  .draft-row p,
  .new-draft p {
    margin: 4px 0 0;
    color: #687b7f;
    font-size: 0.8rem;
  }
  .button.open {
    color: #075e54;
    background: #fff;
    border-color: #6d9d95;
  }
  .icon-delete {
    display: grid;
    width: 34px;
    height: 34px;
    place-items: center;
    padding: 0;
    color: #a62a20;
    background: #fff;
    border: 1px solid #cda09b;
    border-radius: 4px;
    cursor: pointer;
  }
  .new-draft {
    display: grid;
    min-height: 280px;
    place-content: center;
    justify-items: center;
    gap: 8px;
    color: #6d817e;
    background: #fff;
    border: 1px dashed #a9b9b7;
  }
  @media (max-width: 700px) {
    .home {
      width: min(100% - 24px, 1540px);
      padding-top: 20px;
    }
    .source-options,
    .setup-fields {
      grid-template-columns: 1fr;
    }
    .setup-actions {
      align-items: stretch;
      flex-direction: column;
    }
    .draft-row {
      grid-template-columns: auto 1fr auto;
    }
    .draft-row .button.open {
      grid-column: 1 / -1;
    }
  }
</style>
