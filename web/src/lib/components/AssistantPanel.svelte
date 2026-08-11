<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { MessageCircle, Send, Square, Trash2, X } from 'lucide-svelte';
  import { ApiProblem } from '$lib/api';
  import {
    MAX_ASSISTANT_QUESTION_CHARACTERS,
    requestAssistantHelp,
    validateAssistantQuestion,
    type AssistantHelpReference
  } from '$lib/assistant';

  type ChatMessage = {
    id: number;
    role: 'user' | 'assistant';
    text: string;
    references: AssistantHelpReference[];
  };

  let open = $state(false);
  let question = $state('');
  let messages = $state<ChatMessage[]>([]);
  let problem = $state<string | null>(null);
  let pending = $state(false);
  let launcherButton = $state<HTMLButtonElement>();
  let composer = $state<HTMLTextAreaElement>();
  let transcript = $state<HTMLDivElement>();
  let activeRequest: AbortController | null = null;
  let nextMessageId = 1;

  const characterCount = $derived(Array.from(question).length);
  const canSend = $derived(
    question.trim().length > 0 && characterCount <= MAX_ASSISTANT_QUESTION_CHARACTERS && !pending
  );

  onMount(() => {
    window.addEventListener('keydown', handleWindowKeydown);
    return () => window.removeEventListener('keydown', handleWindowKeydown);
  });

  async function openPanel() {
    open = true;
    problem = null;
    await tick();
    composer?.focus();
  }

  async function closePanel() {
    cancelRequest(false);
    open = false;
    await tick();
    launcherButton?.focus();
  }

  function clearConversation() {
    cancelRequest(false);
    question = '';
    messages = [];
    problem = null;
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
        references: []
      }
    ];
    question = '';
    await scrollToLatest();

    try {
      const response = await requestAssistantHelp(normalizedQuestion, controller.signal);
      if (activeRequest !== controller) return;
      messages = [
        ...messages,
        {
          id: nextMessageId++,
          role: 'assistant',
          text: response.answer,
          references: response.references
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
    return error instanceof Error ? error.message : 'Application help is unavailable.';
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
        <span class="heading-icon" aria-hidden="true"><MessageCircle size={18} /></span>
        <div>
          <h2 id="assistant-title">TCO assistant</h2>
          <p id="assistant-boundary">Application help</p>
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
          <MessageCircle size={26} aria-hidden="true" />
          <p>What would you like to understand?</p>
          <span>Ask about a field, action, result, or workflow.</span>
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
          </article>
        {/each}
      {/if}
      {#if pending}
        <div class="pending" role="status">
          <span></span><span></span><span></span><span class="visually-hidden">Finding help</span>
        </div>
      {/if}
    </div>

    <div class="composer-area">
      {#if problem}<p class="problem" role="status">{problem}</p>{/if}
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
  <MessageCircle size={23} strokeWidth={2.2} />
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
    background: #fff;
    border: 1px solid #aebdc1;
    border-radius: 8px;
    box-shadow: 0 18px 48px rgb(15 30 35 / 24%);
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
    background: #17353d;
    border-bottom: 3px solid var(--azure);
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
    color: #17353d;
    background: #90e0df;
    border-radius: 50%;
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
      linear-gradient(180deg, #f7faf9, #eef4f4),
      repeating-linear-gradient(90deg, transparent 0 39px, rgb(18 75 90 / 3%) 40px);
    scrollbar-color: #9aabad transparent;
  }
  .empty-state {
    min-height: 100%;
    display: grid;
    place-content: center;
    justify-items: center;
    padding: 24px;
    color: #607579;
    text-align: center;
  }
  .empty-state p {
    margin: 10px 0 4px;
    color: #263e43;
    font-weight: 650;
  }
  .empty-state span {
    max-width: 240px;
    font-size: 0.8rem;
    line-height: 1.45;
  }
  article {
    width: fit-content;
    max-width: 90%;
    margin-bottom: 12px;
    padding: 9px 11px;
    color: #20383d;
    background: #fff;
    border: 1px solid #cbd7d9;
    border-radius: 7px;
    box-shadow: 0 1px 3px rgb(26 50 58 / 7%);
  }
  article.user {
    margin-left: auto;
    color: #fff;
    background: #006f86;
    border-color: #005366;
  }
  article p {
    margin: 4px 0 0;
    font-size: 0.84rem;
    line-height: 1.45;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .speaker {
    color: #5b7176;
    font-size: 0.65rem;
    font-weight: 720;
    text-transform: uppercase;
  }
  article.user .speaker {
    color: #ccebf0;
  }
  .references {
    padding-top: 7px;
    color: #50666b;
    border-top: 1px solid #dce5e6;
    font-size: 0.7rem;
  }
  .references span {
    display: block;
    margin-bottom: 2px;
    color: #7b520a;
    font-weight: 720;
    text-transform: uppercase;
  }
  .pending {
    width: 52px;
    display: flex;
    gap: 4px;
    padding: 11px 12px;
    background: #fff;
    border: 1px solid #cbd7d9;
    border-radius: 7px;
  }
  .pending > span:not(.visually-hidden) {
    width: 6px;
    height: 6px;
    background: #39727c;
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
    background: #fff;
    border-top: 1px solid #cbd7d9;
  }
  .problem {
    margin: 0 0 8px;
    padding: 7px 9px;
    color: #7e261f;
    background: #fff0ee;
    border-left: 3px solid #b42318;
    font-size: 0.76rem;
    line-height: 1.35;
  }
  form {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 7px;
    padding: 7px 7px 7px 10px;
    background: #fff;
    border: 1px solid #97a9ad;
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
    color: #193136;
    background: transparent;
    border: 0;
    outline: 0;
    font:
      400 0.86rem/1.4 Aptos,
      sans-serif;
  }
  textarea::placeholder {
    color: #73868a;
  }
  .composer-actions {
    align-self: end;
    gap: 7px;
  }
  .composer-actions > span {
    color: #718388;
    font-size: 0.64rem;
    white-space: nowrap;
  }
  .composer-actions > span.near-limit {
    color: #9a4d00;
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
    color: #819195;
    background: #e4e9ea;
    border-color: #c3ced0;
    cursor: not-allowed;
  }
  .composer-actions button.cancel {
    color: #7e261f;
    background: #fff;
    border-color: #c58f89;
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
    background: #006f86;
    border: 1px solid #005366;
    border-radius: 50%;
    box-shadow: 0 7px 20px rgb(15 30 35 / 24%);
    cursor: pointer;
    transition:
      transform 140ms ease,
      background 140ms ease;
  }
  .launcher:hover {
    background: #005366;
    transform: translateY(-2px);
  }
  .launcher.open {
    color: #d7e5e8;
    background: #29464e;
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
