<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { Check, FileImage, FolderOpen, ImagePlus, Send, Square, Trash2, X } from 'lucide-svelte';
  import CopilotIcon from './CopilotIcon.svelte';
  import { ApiProblem } from '$lib/api';
  import {
    executeAssistantAction,
    MAX_ASSISTANT_QUESTION_CHARACTERS,
    requestAssistantImage,
    requestAssistantTurn,
    validateAssistantImage,
    validateAssistantQuestion,
    type AssistantHelpReference,
    type AssistantProposal
  } from '$lib/assistant';

  let {
    authenticated = false,
    projectId = null,
    projectEtag = null,
    projectDirty = false,
    projectOpen = false,
    onprojectupdated = () => {},
    onprojectdrafted = () => {}
  }: {
    authenticated?: boolean;
    projectId?: string | null;
    projectEtag?: string | null;
    projectDirty?: boolean;
    projectOpen?: boolean;
    onprojectupdated?: (document: unknown, etag: string) => void;
    onprojectdrafted?: (project: unknown) => void;
  } = $props();

  type ChatMessage = {
    id: number;
    role: 'user' | 'assistant';
    text: string;
    references: AssistantHelpReference[];
    proposal: AssistantProposal | null;
    proposalStatus: 'pending' | 'applied' | 'opened' | 'dismissed' | null;
    omissions: string[];
    uncertainties: string[];
  };

  let open = $state(false);
  let question = $state('');
  let messages = $state<ChatMessage[]>([]);
  let problem = $state<string | null>(null);
  let pending = $state(false);
  let pendingLabel = $state('Working');
  let selectedImage = $state<File | null>(null);
  let launcherButton = $state<HTMLButtonElement>();
  let composer = $state<HTMLTextAreaElement>();
  let transcript = $state<HTMLDivElement>();
  let imageInput = $state<HTMLInputElement>();
  let activeRequest: AbortController | null = null;
  let nextMessageId = 1;

  const characterCount = $derived(Array.from(question).length);
  const canSend = $derived(
    authenticated &&
      question.trim().length > 0 &&
      characterCount <= MAX_ASSISTANT_QUESTION_CHARACTERS &&
      !pending
  );
  const canAnalyzeImage = $derived(
    authenticated &&
      selectedImage !== null &&
      !pending &&
      (projectId === null ? !projectOpen : !projectDirty)
  );

  onMount(() => {
    window.addEventListener('keydown', handleWindowKeydown);
    return () => {
      window.removeEventListener('keydown', handleWindowKeydown);
      cancelRequest(false);
      removeImage();
    };
  });

  async function openPanel() {
    open = true;
    problem = null;
    await tick();
    composer?.focus();
  }

  async function closePanel() {
    cancelRequest(false);
    removeImage();
    open = false;
    await tick();
    launcherButton?.focus();
  }

  function clearConversation() {
    cancelRequest(false);
    question = '';
    messages = [];
    problem = null;
    removeImage();
    void focusComposer();
  }

  function cancelRequest(showStatus = true) {
    if (!activeRequest) return;
    activeRequest.abort();
    activeRequest = null;
    pending = false;
    if (showStatus) problem = 'The response was cancelled.';
  }

  async function focusComposer() {
    await tick();
    composer?.focus();
  }

  async function scrollToLatest() {
    await tick();
    if (transcript) transcript.scrollTop = transcript.scrollHeight;
  }

  async function sendQuestion() {
    if (!authenticated) return;
    let normalizedQuestion: string;
    try {
      normalizedQuestion = validateAssistantQuestion(question);
    } catch (error) {
      problem = messageFromError(error);
      return;
    }

    const controller = new AbortController();
    activeRequest = controller;
    pending = true;
    problem = null;
    messages = [
      ...messages,
      {
        id: nextMessageId++,
        role: 'user',
        text: normalizedQuestion,
        references: [],
        proposal: null,
        proposalStatus: null,
        omissions: [],
        uncertainties: []
      }
    ];
    question = '';
    await scrollToLatest();

    try {
      const response = await requestAssistantTurn(normalizedQuestion, projectId, controller.signal);
      if (activeRequest !== controller) return;
      messages = [
        ...messages,
        {
          id: nextMessageId++,
          role: 'assistant',
          text: response.answer,
          references: response.references,
          proposal: response.proposal,
          proposalStatus: response.proposal ? 'pending' : null,
          omissions: [],
          uncertainties: []
        }
      ];
      await scrollToLatest();
    } catch (error) {
      if (!controller.signal.aborted && activeRequest === controller) {
        problem = messageFromError(error);
      }
    } finally {
      if (activeRequest === controller) {
        activeRequest = null;
        pending = false;
        await focusComposer();
      }
    }
  }

  function chooseImage() {
    if (!authenticated) {
      problem = 'Please log in to use the TCO agent.';
      return;
    }
    if (!projectId && projectOpen) {
      problem = 'Save this draft before analyzing an image.';
      return;
    }
    if (projectId && projectDirty) {
      problem = 'Save the current project changes before analyzing an image.';
      return;
    }
    imageInput?.click();
  }

  function selectImage(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const image = input.files?.[0] ?? null;
    if (!image) return;
    try {
      selectedImage = validateAssistantImage(image);
      problem = null;
    } catch (error) {
      selectedImage = null;
      input.value = '';
      problem = messageFromError(error);
    }
  }

  function removeImage() {
    selectedImage = null;
    if (imageInput) imageInput.value = '';
  }

  async function analyzeImage() {
    if (
      !authenticated ||
      !selectedImage ||
      pending ||
      (projectId === null ? projectOpen : projectDirty)
    )
      return;
    const image = selectedImage;
    const controller = new AbortController();
    activeRequest = controller;
    pending = true;
    pendingLabel = 'Analyzing image';
    problem = null;
    messages = [
      ...messages,
      {
        id: nextMessageId++,
        role: 'user',
        text: 'Analyze the selected image for project inputs.',
        references: [],
        proposal: null,
        proposalStatus: null,
        omissions: [],
        uncertainties: []
      }
    ];
    await scrollToLatest();

    try {
      const response = await requestAssistantImage(image, projectId, controller.signal);
      if (activeRequest !== controller) return;
      messages = [
        ...messages,
        {
          id: nextMessageId++,
          role: 'assistant',
          text: response.answer,
          references: [],
          proposal: response.proposal,
          proposalStatus: response.proposal ? 'pending' : null,
          omissions: response.omissions,
          uncertainties: response.uncertainties
        }
      ];
      removeImage();
      await scrollToLatest();
    } catch (error) {
      if (!controller.signal.aborted && activeRequest === controller) {
        problem = messageFromError(error);
      }
    } finally {
      if (activeRequest === controller) {
        activeRequest = null;
        pending = false;
        await focusComposer();
      }
    }
  }

  async function applyProposal(messageId: number, proposal: AssistantProposal) {
    if (proposal.action !== 'apply_project_patch' || !canApplyProposal(proposal)) return;
    const controller = new AbortController();
    activeRequest = controller;
    pending = true;
    pendingLabel = 'Applying confirmed changes';
    problem = null;
    try {
      const result = await executeAssistantAction(proposal, controller.signal);
      if (activeRequest !== controller) return;
      onprojectupdated(result.document, result.etag);
      messages = messages.map((message) =>
        message.id === messageId ? { ...message, proposalStatus: 'applied' } : message
      );
      await scrollToLatest();
    } catch (error) {
      if (!controller.signal.aborted && activeRequest === controller) {
        problem = messageFromError(error);
      }
    } finally {
      if (activeRequest === controller) {
        activeRequest = null;
        pending = false;
        await focusComposer();
      }
    }
  }

  function dismissProposal(messageId: number) {
    messages = messages.map((message) =>
      message.id === messageId ? { ...message, proposalStatus: 'dismissed' } : message
    );
  }

  function openProjectDraft(messageId: number, proposal: AssistantProposal) {
    if (proposal.action !== 'open_project_draft' || !canOpenProjectDraft(proposal)) return;
    onprojectdrafted(proposal.project);
    messages = messages.map((message) =>
      message.id === messageId ? { ...message, proposalStatus: 'opened' } : message
    );
  }

  function canApplyProposal(proposal: AssistantProposal): boolean {
    return (
      proposal.action === 'apply_project_patch' &&
      authenticated &&
      !pending &&
      !projectDirty &&
      projectId === proposal.project_id &&
      projectEtag === proposal.expected_etag
    );
  }

  function canOpenProjectDraft(proposal: AssistantProposal): boolean {
    return proposal.action === 'open_project_draft' && authenticated && !pending && !projectOpen;
  }

  function proposalBlockedReason(proposal: AssistantProposal): string | null {
    if (proposal.action === 'open_project_draft') {
      return projectOpen ? 'Close the open project before opening this draft.' : null;
    }
    if (projectDirty) return 'Save current edits before applying this proposal.';
    if (projectId !== proposal.project_id || projectEtag !== proposal.expected_etag) {
      return 'The open project changed after this proposal was prepared.';
    }
    return null;
  }

  function formatPointer(pointer: string): string {
    return pointer
      .split('/')
      .filter(Boolean)
      .map((part) => part.replaceAll('~1', '/').replaceAll('~0', '~').replaceAll('_', ' '))
      .join(' / ');
  }

  function formatValue(value: unknown): string {
    if (value === undefined || value === null) return 'Not set';
    if (typeof value === 'string') return value;
    const encoded = JSON.stringify(value);
    return encoded ?? String(value);
  }

  function performProposalAction(messageId: number, proposal: AssistantProposal) {
    if (proposal.action === 'apply_project_patch') void applyProposal(messageId, proposal);
    else openProjectDraft(messageId, proposal);
  }

  function canConfirmProposal(proposal: AssistantProposal): boolean {
    return proposal.action === 'apply_project_patch'
      ? canApplyProposal(proposal)
      : canOpenProjectDraft(proposal);
  }

  function handleComposerKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      if (canSend) void sendQuestion();
    }
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      event.preventDefault();
      void closePanel();
    }
  }

  function messageFromError(error: unknown): string {
    if (error instanceof ApiProblem && error.requestId) {
      return `${error.message} Request ${error.requestId}`;
    }
    return error instanceof Error ? error.message : 'The TCO assistant is unavailable.';
  }
</script>

{#if open}
  <div
    id="assistant-panel"
    class="panel"
    role="dialog"
    aria-labelledby="assistant-title"
    aria-describedby="assistant-boundary"
  >
    <header>
      <div class="heading">
        <span class="heading-icon" aria-hidden="true"><CopilotIcon size={20} /></span>
        <div>
          <h2 id="assistant-title">TCO assistant</h2>
          <p id="assistant-boundary">
            {authenticated ? 'Tool-enabled agent · reviewed actions' : 'Sign-in required'}
          </p>
        </div>
      </div>
      <div class="header-actions">
        {#if messages.length > 0}
          <button
            type="button"
            aria-label="Clear conversation"
            title="Clear conversation"
            onclick={clearConversation}
          >
            <Trash2 size={17} />
          </button>
        {/if}
        <button type="button" aria-label="Close assistant" title="Close" onclick={closePanel}>
          <X size={19} />
        </button>
      </div>
    </header>

    <div
      class="transcript"
      role="log"
      aria-live="polite"
      aria-busy={pending}
      bind:this={transcript}
    >
      {#if messages.length === 0}
        <div class="empty-state">
          <span class="empty-icon" aria-hidden="true"><CopilotIcon size={28} /></span>
          {#if authenticated}
            <p>What would you like to understand?</p>
            <span>Ask about a field, action, result, or workflow.</span>
          {:else}
            <p>Please log in to use the TCO agent.</p>
          {/if}
        </div>
      {:else}
        {#each messages as message (message.id)}
          <article
            class:assistant={message.role === 'assistant'}
            class:user={message.role === 'user'}
          >
            <span class="speaker">{message.role === 'assistant' ? 'Assistant' : 'You'}</span>
            <p>{message.text}</p>
            {#if message.references.length > 0}
              <p class="references">
                <span>Related controls</span>
                {message.references.map((reference) => reference.label).join(', ')}
              </p>
            {/if}
            {#if message.omissions.length > 0}
              <div class="report omissions">
                <strong>Not mapped</strong>
                <ul>
                  {#each message.omissions as item, itemIndex (itemIndex)}<li>{item}</li>{/each}
                </ul>
              </div>
            {/if}
            {#if message.uncertainties.length > 0}
              <div class="report uncertainties">
                <strong>Needs review</strong>
                <ul>
                  {#each message.uncertainties as item, itemIndex (itemIndex)}<li>{item}</li>{/each}
                </ul>
              </div>
            {/if}
          </article>
          {#if message.proposal}
            <section
              class="proposal"
              aria-label={message.proposal.action === 'apply_project_patch'
                ? 'Proposed project update'
                : 'Proposed new project draft'}
            >
              <div class="proposal-heading">
                <div>
                  {#if message.proposal.action === 'apply_project_patch'}
                    <strong>Proposed project update</strong>
                    <span
                      >{message.proposal.changes.length}
                      {message.proposal.changes.length === 1 ? 'change' : 'changes'}</span
                    >
                  {:else}
                    <strong>New project draft</strong>
                    <span>Unsaved · review before saving</span>
                  {/if}
                </div>
                {#if message.proposalStatus === 'applied'}
                  <span class="proposal-state applied"><Check size={14} /> Applied</span>
                {:else if message.proposalStatus === 'opened'}
                  <span class="proposal-state applied"><Check size={14} /> Opened</span>
                {:else if message.proposalStatus === 'dismissed'}
                  <span class="proposal-state">Dismissed</span>
                {/if}
              </div>
              {#if message.proposal.action === 'apply_project_patch'}
                <dl class="change-list">
                  {#each message.proposal.changes as change (change.pointer)}
                    <div>
                      <dt>{formatPointer(change.pointer)}</dt>
                      <dd>
                        <span>{formatValue(change.before)}</span><b aria-hidden="true">→</b><span
                          >{formatValue(change.after)}</span
                        >
                      </dd>
                    </div>
                  {/each}
                </dl>
              {:else}
                <div class="draft-summary">
                  <strong>{message.proposal.project.name}</strong>
                  <span
                    >{message.proposal.project.settings.project_type
                      .replace('_', ' ')
                      .toUpperCase()} · {message.proposal.project.resources.length}
                    {message.proposal.project.resources.length === 1
                      ? 'workload'
                      : 'workloads'}</span
                  >
                </div>
              {/if}
              {#if message.proposalStatus === 'pending'}
                {#if proposalBlockedReason(message.proposal)}
                  <p class="proposal-blocked">{proposalBlockedReason(message.proposal)}</p>
                {/if}
                <div class="proposal-actions">
                  <button type="button" class="dismiss" onclick={() => dismissProposal(message.id)}
                    >Cancel</button
                  >
                  <button
                    type="button"
                    class="apply"
                    disabled={!canConfirmProposal(message.proposal)}
                    onclick={() => performProposalAction(message.id, message.proposal!)}
                  >
                    {#if message.proposal.action === 'apply_project_patch'}
                      <Check size={16} /> Apply changes
                    {:else}
                      <FolderOpen size={16} /> Open draft
                    {/if}
                  </button>
                </div>
              {/if}
            </section>
          {/if}
        {/each}
      {/if}
      {#if pending}
        <div class="pending" role="status">
          <span></span><span></span><span></span><span class="visually-hidden">{pendingLabel}</span>
        </div>
      {/if}
    </div>

    <div class="composer-area">
      {#if problem}<p class="problem" role="status">{problem}</p>{/if}
      {#if authenticated}
        <input
          class="visually-hidden"
          type="file"
          accept=".jpg,.jpeg,.png,image/jpeg,image/png"
          bind:this={imageInput}
          onchange={selectImage}
          aria-label="Choose project image"
        />
        {#if selectedImage}
          <div class="attachment">
            <FileImage size={18} aria-hidden="true" />
            <div>
              <strong>{selectedImage.name}</strong><span
                >{(selectedImage.size / 1024).toFixed(1)} KiB</span
              >
            </div>
            <button
              type="button"
              class="analyze"
              disabled={!canAnalyzeImage}
              onclick={() => void analyzeImage()}>Analyze</button
            >
            <button
              type="button"
              class="remove"
              aria-label="Remove image"
              title="Remove image"
              onclick={removeImage}><X size={16} /></button
            >
          </div>
        {/if}
        <form
          onsubmit={(event) => {
            event.preventDefault();
            if (canSend) void sendQuestion();
          }}
        >
          <label class="visually-hidden" for="assistant-question">Ask the TCO assistant</label>
          <textarea
            id="assistant-question"
            rows="2"
            maxlength={MAX_ASSISTANT_QUESTION_CHARACTERS}
            placeholder="Ask about this estimate"
            bind:this={composer}
            bind:value={question}
            onkeydown={handleComposerKeydown}></textarea>
          <div class="composer-actions">
            <span class:near-limit={characterCount > 900}
              >{characterCount.toLocaleString('en-US')} / {MAX_ASSISTANT_QUESTION_CHARACTERS.toLocaleString(
                'en-US'
              )}</span
            >
            <button
              class="image"
              type="button"
              disabled={pending}
              aria-label="Add JPEG or PNG"
              title={projectId
                ? projectDirty
                  ? 'Save project changes first'
                  : 'Add JPEG or PNG'
                : projectOpen
                  ? 'Save this draft first'
                  : 'Create a project draft from JPEG or PNG'}
              onclick={chooseImage}><ImagePlus size={17} /></button
            >
            {#if pending}
              <button
                class="cancel"
                type="button"
                aria-label="Cancel response"
                title="Cancel response"
                onclick={() => cancelRequest()}
              >
                <Square size={15} fill="currentColor" />
              </button>
            {:else}
              <button
                class="send"
                type="submit"
                disabled={!canSend}
                aria-label="Send question"
                title="Send question"
              >
                <Send size={17} />
              </button>
            {/if}
          </div>
        </form>
      {:else}
        <p class="login-required">Please log in to use the TCO agent.</p>
      {/if}
    </div>
  </div>
{/if}

<button
  class="launcher"
  class:open
  type="button"
  bind:this={launcherButton}
  aria-label={open ? 'Assistant open' : 'Open TCO assistant'}
  aria-expanded={open}
  aria-controls="assistant-panel"
  title={open ? 'Assistant open' : 'Open TCO assistant'}
  onclick={() => {
    if (!open) void openPanel();
  }}
>
  <CopilotIcon size={26} />
</button>

<style>
  .panel {
    position: fixed;
    right: 22px;
    bottom: 82px;
    z-index: 18;
    width: min(390px, calc(100vw - 32px));
    height: min(560px, calc(100dvh - 112px));
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow:
      0 18px 48px rgb(5 13 18 / 38%),
      0 0 24px rgb(133 52 243 / 10%);
    animation: panel-enter 160ms ease-out;
  }
  header {
    min-height: 58px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 11px 10px 13px;
    color: #f8fbfc;
    background: var(--azure-dark);
    border-bottom: 3px solid var(--copilot-purple);
  }
  .heading,
  .header-actions,
  .composer-actions {
    display: flex;
    align-items: center;
  }
  .heading {
    min-width: 0;
    gap: 9px;
  }
  .heading-icon {
    display: grid;
    width: 32px;
    height: 32px;
    flex: 0 0 32px;
    place-items: center;
    color: var(--copilot-purple-light);
    background: var(--copilot-surface);
    border: 1px solid color-mix(in srgb, var(--copilot-purple-light) 46%, transparent);
    border-radius: 50%;
    box-shadow: 0 0 15px rgb(200 152 253 / 38%);
  }
  h2,
  .heading p {
    margin: 0;
    letter-spacing: 0;
  }
  h2 {
    font:
      650 0.94rem/1.2 Bahnschrift,
      sans-serif;
  }
  .heading p {
    margin-top: 2px;
    color: #c4d5d9;
    font-size: 0.7rem;
  }
  .header-actions {
    gap: 2px;
  }
  .header-actions button,
  .composer-actions button {
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }
  .header-actions button {
    width: 32px;
    height: 32px;
    color: #deeaec;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 4px;
  }
  .header-actions button:hover {
    color: #fff;
    background: rgb(255 255 255 / 9%);
    border-color: #668087;
  }
  .transcript {
    min-height: 0;
    overflow-y: auto;
    padding: 15px 13px;
    background:
      linear-gradient(180deg, var(--surface-subtle), var(--page)),
      repeating-linear-gradient(90deg, transparent 0 39px, rgb(134 200 237 / 3%) 40px);
    scrollbar-color: var(--border-input) transparent;
  }
  .empty-state {
    min-height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    padding: 24px;
    color: var(--muted);
    text-align: center;
  }
  .empty-state p {
    margin: 10px 0 4px;
    color: var(--ink-strong);
    font-weight: 650;
  }
  .empty-state span {
    max-width: 240px;
    font-size: 0.8rem;
    line-height: 1.45;
  }
  .empty-state .empty-icon {
    display: grid;
    place-items: center;
    color: var(--copilot-purple-light);
    filter: drop-shadow(0 0 7px rgb(133 52 243 / 46%));
  }
  article {
    width: fit-content;
    max-width: 90%;
    margin-bottom: 12px;
    padding: 9px 11px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 7px;
    box-shadow: 0 1px 3px rgb(26 50 58 / 7%);
  }
  article.user {
    margin-left: auto;
    color: #fff;
    background: var(--azure);
    border-color: var(--azure-dark);
  }
  article p {
    margin: 4px 0 0;
    font-size: 0.84rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .speaker {
    color: var(--muted);
    font-size: 0.65rem;
    font-weight: 720;
    text-transform: uppercase;
  }
  article.user .speaker {
    color: #ccebf0;
  }
  .references {
    padding-top: 7px;
    color: var(--ink-soft);
    border-top: 1px solid var(--border-subtle);
    font-size: 0.7rem;
  }
  .references span {
    display: block;
    margin-bottom: 2px;
    color: var(--warning-text);
    font-weight: 720;
    text-transform: uppercase;
  }
  .report {
    margin-top: 9px;
    padding-top: 8px;
    color: var(--ink-soft);
    border-top: 1px solid var(--border-subtle);
    font-size: 0.72rem;
    line-height: 1.4;
  }
  .report strong {
    display: block;
    margin-bottom: 3px;
    color: var(--warning-text);
  }
  .report ul {
    margin: 0;
    padding-left: 17px;
  }
  .report li + li {
    margin-top: 3px;
  }
  .proposal {
    margin: -3px 0 14px;
    overflow: hidden;
    background: var(--surface);
    border: 1px solid var(--border-input);
    border-left: 4px solid var(--copilot-purple);
    border-radius: 6px;
    box-shadow: 0 3px 14px rgb(133 52 243 / 12%);
  }
  .proposal-heading {
    min-height: 48px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 9px 11px;
    color: var(--ink-strong);
    background: var(--copilot-surface);
    border-bottom: 1px solid color-mix(in srgb, var(--copilot-purple) 34%, var(--border));
  }
  .proposal-heading > div {
    min-width: 0;
  }
  .proposal-heading strong,
  .proposal-heading span {
    display: block;
  }
  .proposal-heading strong {
    font-size: 0.8rem;
  }
  .proposal-heading div span {
    margin-top: 1px;
    color: var(--muted);
    font-size: 0.68rem;
  }
  .proposal-state {
    flex: 0 0 auto;
    color: var(--muted);
    font-size: 0.7rem;
    font-weight: 700;
  }
  .proposal-state.applied {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--success);
  }
  .change-list {
    margin: 0;
  }
  .draft-summary {
    padding: 11px;
  }
  .draft-summary strong,
  .draft-summary span {
    display: block;
    overflow-wrap: anywhere;
  }
  .draft-summary strong {
    color: var(--ink-strong);
    font-size: 0.78rem;
  }
  .draft-summary span {
    margin-top: 3px;
    color: var(--muted);
    font-size: 0.7rem;
  }
  .change-list > div {
    padding: 8px 10px;
  }
  .change-list > div + div {
    border-top: 1px solid var(--border-subtle);
  }
  .change-list dt {
    margin-bottom: 5px;
    color: var(--ink-soft);
    font-size: 0.67rem;
    font-weight: 700;
    text-transform: capitalize;
  }
  .change-list dd {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 14px minmax(0, 1fr);
    align-items: start;
    gap: 5px;
    margin: 0;
    font-size: 0.72rem;
    line-height: 1.35;
  }
  .change-list dd span {
    min-width: 0;
    padding: 5px 6px;
    overflow-wrap: anywhere;
    background: var(--surface-subtle);
    border-radius: 3px;
  }
  .change-list dd span:last-child {
    color: var(--success);
    background: var(--success-surface);
  }
  .change-list dd b {
    padding-top: 5px;
    color: var(--muted);
    text-align: center;
  }
  .proposal-blocked {
    margin: 0;
    padding: 7px 10px;
    color: var(--danger-text);
    background: var(--danger-surface);
    border-top: 1px solid var(--danger-border);
    font-size: 0.7rem;
    line-height: 1.35;
  }
  .proposal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 7px;
    padding: 9px 10px;
    border-top: 1px solid var(--border-subtle);
  }
  .proposal-actions button,
  .attachment button {
    min-height: 32px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 0 11px;
    border-radius: 4px;
    cursor: pointer;
    font-size: 0.73rem;
    font-weight: 700;
  }
  .proposal-actions .dismiss {
    color: var(--ink-soft);
    background: var(--surface-input);
    border: 1px solid var(--border-input);
  }
  .proposal-actions .apply,
  .attachment .analyze {
    color: #fff;
    background: var(--azure);
    border: 1px solid var(--azure-dark);
  }
  .proposal-actions button:disabled,
  .attachment button:disabled {
    color: var(--muted);
    background: var(--surface-muted);
    border-color: var(--border);
    cursor: not-allowed;
  }
  .pending {
    width: 52px;
    display: flex;
    gap: 4px;
    padding: 11px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 7px;
  }
  .pending > span:not(.visually-hidden) {
    width: 6px;
    height: 6px;
    background: var(--copilot-purple-light);
    border-radius: 50%;
    animation: pulse 900ms ease-in-out infinite;
  }
  .pending > span:nth-child(2) {
    animation-delay: 120ms;
  }
  .pending > span:nth-child(3) {
    animation-delay: 240ms;
  }
  .composer-area {
    padding: 10px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .problem {
    margin: 0 0 8px;
    padding: 7px 9px;
    color: var(--danger-text);
    background: var(--danger-surface);
    border-left: 3px solid var(--danger);
    font-size: 0.76rem;
    line-height: 1.35;
  }
  .login-required {
    margin: 0;
    padding: 9px 10px;
    color: var(--ink-soft);
    background: var(--surface-subtle);
    border-left: 3px solid var(--border-input);
    font-size: 0.78rem;
  }
  .attachment {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
    padding: 8px;
    color: var(--ink-soft);
    background: var(--azure-soft);
    border: 1px solid var(--border-input);
    border-radius: 5px;
  }
  .attachment > div {
    min-width: 0;
  }
  .attachment strong,
  .attachment span {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .attachment strong {
    font-size: 0.73rem;
  }
  .attachment span {
    margin-top: 1px;
    color: var(--muted);
    font-size: 0.65rem;
  }
  .attachment button.remove {
    width: 30px;
    min-height: 30px;
    padding: 0;
    color: var(--ink-soft);
    background: transparent;
    border: 1px solid transparent;
  }
  form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 7px;
    padding: 7px 7px 7px 10px;
    background: var(--surface-input);
    border: 1px solid var(--border-input);
    border-radius: 6px;
  }
  form:focus-within {
    border-color: var(--focus);
    outline: 2px solid color-mix(in srgb, var(--focus) 22%, transparent);
  }
  textarea {
    width: 100%;
    min-height: 43px;
    max-height: 112px;
    resize: vertical;
    padding: 2px 0;
    color: var(--ink);
    background: transparent;
    border: 0;
    outline: 0;
    font:
      400 0.86rem/1.4 Aptos,
      sans-serif;
  }
  textarea::placeholder {
    color: var(--muted);
  }
  .composer-actions {
    align-self: end;
    gap: 7px;
  }
  .composer-actions > span {
    color: var(--muted);
    font-size: 0.64rem;
    white-space: nowrap;
  }
  .composer-actions > span.near-limit {
    color: var(--warning-text);
    font-weight: 700;
  }
  .composer-actions button {
    width: 32px;
    height: 32px;
    color: #fff;
    background: var(--azure);
    border: 1px solid var(--azure-dark);
    border-radius: 5px;
  }
  .composer-actions button:hover {
    background: var(--azure-dark);
  }
  .composer-actions button:disabled {
    color: var(--muted);
    background: var(--surface-muted);
    border-color: var(--border);
    cursor: not-allowed;
  }
  .composer-actions button.cancel {
    color: var(--danger-text);
    background: var(--surface-input);
    border-color: var(--danger-border);
  }
  .composer-actions button.image {
    color: var(--azure-text);
    background: var(--azure-soft);
    border-color: var(--border-input);
  }
  .composer-actions button.image:hover {
    background: var(--azure-surface);
  }
  .launcher {
    position: fixed;
    right: 22px;
    bottom: 22px;
    z-index: 14;
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
    padding: 0;
    color: #fff;
    background: var(--copilot-purple);
    border: 1px solid var(--copilot-purple-light);
    border-radius: 50%;
    box-shadow:
      0 7px 20px rgb(5 13 18 / 34%),
      0 0 20px rgb(133 52 243 / 62%),
      inset 0 0 10px rgb(255 255 255 / 12%);
    cursor: pointer;
    transition:
      transform 140ms ease,
      background 140ms ease;
  }
  .launcher:hover {
    background: var(--copilot-purple-dark);
    box-shadow:
      0 9px 24px rgb(5 13 18 / 38%),
      0 0 26px rgb(200 152 253 / 70%);
    transform: translateY(-2px);
  }
  .launcher.open {
    color: var(--copilot-purple-light);
    background: var(--surface-muted);
  }
  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
  }
  @keyframes panel-enter {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @keyframes pulse {
    0%,
    60%,
    100% {
      opacity: 0.35;
      transform: translateY(0);
    }
    30% {
      opacity: 1;
      transform: translateY(-2px);
    }
  }
  @media (max-width: 560px) {
    .panel {
      inset: auto 0 0;
      width: 100%;
      height: min(78dvh, 620px);
      border-right: 0;
      border-bottom: 0;
      border-left: 0;
      border-radius: 8px 8px 0 0;
    }
    .change-list dd {
      grid-template-columns: minmax(0, 1fr);
    }
    .change-list dd b {
      display: none;
    }
    .attachment {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }
    .attachment .analyze {
      grid-column: 1 / -1;
      grid-row: 2;
    }
    .attachment .remove {
      grid-column: 3;
      grid-row: 1;
    }
    .launcher {
      right: 16px;
      bottom: 16px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel,
    .pending > span:not(.visually-hidden) {
      animation: none;
    }
    .launcher {
      transition: none;
    }
  }
</style>
