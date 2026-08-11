<script lang="ts">
  import { Check, Copy, Link2Off, X } from 'lucide-svelte';

  let {
    open,
    link,
    expiresAt,
    copied,
    revoking,
    oncopy,
    onrevoke,
    onclose
  }: {
    open: boolean;
    link: string;
    expiresAt: string;
    copied: boolean;
    revoking: boolean;
    oncopy: () => void;
    onrevoke: () => void;
    onclose: () => void;
  } = $props();

  function formatExpiry(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? value
      : new Intl.DateTimeFormat('en', { dateStyle: 'long', timeStyle: 'short' }).format(date);
  }
</script>

{#if open}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="share-title">
      <div class="heading">
        <div>
          <span>Expires {formatExpiry(expiresAt)}</span>
          <h2 id="share-title">Share project</h2>
        </div>
        <button
          class="icon-button"
          type="button"
          title="Close"
          aria-label="Close"
          onclick={onclose}
        >
          <X size={18} />
        </button>
      </div>
      <label for="project-share-link">Link</label>
      <div class="link-row">
        <input
          id="project-share-link"
          value={link}
          readonly
          onclick={(event) => event.currentTarget.select()}
        />
        <button class="copy" type="button" onclick={oncopy}>
          {#if copied}<Check size={17} /> Copied{:else}<Copy size={17} /> Copy{/if}
        </button>
      </div>
      <div class="actions">
        <button class="revoke" type="button" onclick={onrevoke} disabled={revoking}>
          <Link2Off size={17} />
          {revoking ? 'Revoking…' : 'Revoke link'}
        </button>
        <button class="done" type="button" onclick={onclose}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 16px;
    background: rgb(15 30 35 / 55%);
  }
  .dialog {
    width: min(100%, 560px);
    padding: 20px;
    background: #fff;
    border: 1px solid #cad4d7;
    border-radius: 6px;
    box-shadow: 0 18px 48px rgb(15 30 35 / 22%);
  }
  .heading,
  .link-row,
  .actions {
    display: flex;
    align-items: center;
  }
  .heading {
    justify-content: space-between;
    gap: 16px;
  }
  .heading span,
  label {
    color: #5d7473;
    font-size: 0.72rem;
    font-weight: 700;
  }
  h2 {
    margin: 3px 0 0;
    color: #173338;
    font:
      680 1.2rem/1.25 Bahnschrift,
      sans-serif;
  }
  label {
    display: block;
    margin: 20px 0 6px;
  }
  .link-row {
    gap: 8px;
  }
  input {
    min-width: 0;
    flex: 1;
    min-height: 38px;
    padding: 8px 10px;
    color: #26383d;
    background: #f6f8f7;
    border: 1px solid #9baaad;
    border-radius: 4px;
    font:
      0.8rem/1.2 Consolas,
      monospace;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 38px;
    padding: 7px 13px;
    border-radius: 4px;
    font:
      700 0.82rem/1 Aptos,
      sans-serif;
    cursor: pointer;
  }
  button:disabled {
    cursor: wait;
    opacity: 0.62;
  }
  .icon-button {
    width: 36px;
    padding: 0;
    color: #43565b;
    background: transparent;
    border: 0;
  }
  .copy,
  .done {
    color: #fff;
    background: #087f73;
    border: 1px solid #087f73;
  }
  .actions {
    justify-content: space-between;
    gap: 8px;
    margin-top: 20px;
  }
  .revoke {
    color: #a62a20;
    background: #fff;
    border: 1px solid #ce8f88;
  }
  @media (max-width: 480px) {
    .link-row {
      align-items: stretch;
      flex-direction: column;
    }
    .copy {
      width: 100%;
    }
  }
</style>
