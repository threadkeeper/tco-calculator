<script lang="ts">
  let {
    open,
    title,
    message,
    confirmLabel = 'Confirm',
    onconfirm,
    oncancel
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();
</script>

{#if open}
  <div class="backdrop" role="presentation">
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
    >
      <h2 id="confirm-title">{title}</h2>
      <p id="confirm-message">{message}</p>
      <div class="actions">
        <button type="button" class="secondary" onclick={oncancel}>Cancel</button>
        <button type="button" class="danger" onclick={onconfirm}>{confirmLabel}</button>
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
    width: min(100%, 440px);
    padding: 20px;
    background: white;
    border: 1px solid #cad4d7;
    border-radius: 6px;
    box-shadow: 0 18px 48px rgb(15 30 35 / 22%);
  }
  h2 {
    margin: 0;
    font-size: 1.2rem;
  }
  p {
    color: #43565b;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }
  button {
    min-height: 36px;
    padding: 7px 13px;
    border-radius: 5px;
    font: inherit;
    cursor: pointer;
  }
  .secondary {
    color: #26383d;
    background: white;
    border: 1px solid #9baaad;
  }
  .danger {
    color: white;
    background: #b42318;
    border: 1px solid #912018;
  }
</style>
