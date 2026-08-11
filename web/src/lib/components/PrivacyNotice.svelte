<script lang="ts">
  import { onMount } from 'svelte';
  import { ExternalLink, LockKeyhole, ShieldCheck, X } from 'lucide-svelte';

  let {
    required,
    authenticated,
    noticeVersion,
    acceptedAt,
    initialAllowContact,
    initialEmailAddress,
    saving,
    error,
    onaccept,
    onclose
  }: {
    required: boolean;
    authenticated: boolean;
    noticeVersion: string;
    acceptedAt: string | null;
    initialAllowContact: boolean;
    initialEmailAddress: string | null;
    saving: boolean;
    error: string | null;
    onaccept: (allowContact: boolean, emailAddress: string | null) => void;
    onclose: () => void;
  } = $props();

  let dialog = $state<HTMLDivElement>();
  let firstControl = $state<HTMLElement>();
  let accepted = $state(false);
  let allowContact = $state(false);
  let emailAddress = $state('');

  onMount(() => {
    allowContact = initialAllowContact;
    emailAddress = initialEmailAddress ?? '';
    firstControl?.focus();
  });

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!accepted || saving) return;
    onaccept(allowContact, allowContact ? emailAddress.trim() : null);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      if (!required) onclose();
      return;
    }
    if (event.key !== 'Tab') return;
    if (!dialog) return;
    const controls = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )
    );
    if (controls.length === 0) return;
    const first = controls[0];
    const last = controls[controls.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function formatAcceptedAt(value: string): string {
    const date = new Date(value);
    return Number.isNaN(date.getTime())
      ? value
      : new Intl.DateTimeFormat('en', { dateStyle: 'long', timeStyle: 'short' }).format(date);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="backdrop" role="presentation">
  <div
    class="dialog"
    bind:this={dialog}
    role={required ? 'alertdialog' : 'dialog'}
    aria-modal="true"
    aria-labelledby="privacy-title"
    aria-describedby="privacy-summary"
  >
    <header>
      <span class="policy-icon" aria-hidden="true"><ShieldCheck size={24} /></span>
      <div>
        <p>Internal pilot notice</p>
        <h2 id="privacy-title">Privacy and data use</h2>
      </div>
      {#if !required}
        <button
          bind:this={firstControl}
          class="icon-button"
          type="button"
          title="Close privacy notice"
          aria-label="Close privacy notice"
          onclick={onclose}
        >
          <X size={19} />
        </button>
      {/if}
    </header>

    <div class="notice-body">
      <p id="privacy-summary" class="summary">
        Azure SQL TCO keeps signed-in projects private to your Microsoft Entra identity by default.
        This pilot notice supplements the
        <a
          href="https://www.microsoft.com/privacy/privacystatement"
          target="_blank"
          rel="noreferrer"
          >Microsoft Privacy Statement <ExternalLink size={13} aria-hidden="true" /></a
        >. It is not a substitute for an approved production privacy notice.
      </p>

      <section>
        <h3>Data we process</h3>
        <ul>
          <li>Entra tenant and object identifiers used to isolate your saved projects.</li>
          <li>Your display name when Entra supplies it.</li>
          <li>
            Your email address only when you choose Azure SQL contact. If Entra does not supply one,
            you may enter the address you want Microsoft to use.
          </li>
          <li>The notice version, acceptance time, and your separate contact choice.</li>
          <li>Your saved project inputs, estimates, and deterministic calculation results.</li>
          <li>
            Limited operational telemetry such as request IDs, status, duration, and auth mode.
          </li>
        </ul>
      </section>

      <section>
        <h3>How we use and disclose data</h3>
        <ul>
          <li>
            To authenticate you, provide the calculator, save projects, and secure the service.
          </li>
          <li>To contact you about Azure SQL only when you enable the optional contact choice.</li>
          <li>
            No sale of personal data, third-party advertising, behavioral tracking, or product
            analytics.
          </li>
          <li>
            Project data is not sent to AWS or Azure pricing APIs; only nonpersonal public SKU and
            region filters are sent.
          </li>
          <li>
            You can intentionally disclose an editable project snapshot by creating a 30-day share
            link. The source project and owner identity remain private.
          </li>
          <li>
            Microsoft, authorized service providers operating Azure, and personnel with approved
            access may process data to run and protect the pilot, or when disclosure is legally
            required. Data is not promised to be free from every legally required disclosure.
          </li>
        </ul>
      </section>

      <section class="safeguards">
        <h3><LockKeyhole size={16} aria-hidden="true" /> Safeguards</h3>
        <ul>
          <li>
            Azure Container Apps validates Microsoft Entra sign-in before identity reaches the app.
          </li>
          <li>
            The app stores or forwards no Entra access token and uses no browser OAuth library.
          </li>
          <li>
            Ownership is derived server-side from both tenant and object ID; client-provided owner
            IDs are ignored.
          </li>
          <li>Every project and consent record is read and written inside that owner partition.</li>
          <li>
            Cosmos DB is reached through private networking with public access and key
            authentication disabled. Runtime access uses a least-privilege system-managed identity
            and Azure RBAC.
          </li>
          <li>HTTPS protects data in transit and Azure Cosmos DB encrypts data at rest.</li>
          <li>
            Logs exclude tokens, raw identity headers, project payloads, and workload names;
            operational logs are retained for 30 days.
          </li>
        </ul>
      </section>

      <section>
        <h3>Retention and choices</h3>
        <p>
          Deleting a project permanently removes that owner-scoped project. The consent profile
          remains while the pilot profile is in use and is removed through an approved privacy
          request or when pilot data is decommissioned. For access, correction, deletion, or
          contact-withdrawal requests, use
          <a href="https://go.microsoft.com/fwlink/?linkid=2126612" target="_blank" rel="noreferrer"
            >Microsoft privacy support <ExternalLink size={13} aria-hidden="true" /></a
          > and identify the Azure SQL TCO internal pilot.
        </p>
      </section>

      <p class="version">
        Notice version {noticeVersion || 'unavailable'}{#if acceptedAt}
          · Accepted {formatAcceptedAt(acceptedAt)}{/if}
      </p>
    </div>

    {#if required && authenticated}
      <form onsubmit={submit}>
        <label class="choice required-choice">
          <input bind:this={firstControl} type="checkbox" bind:checked={accepted} required />
          <span>I have read and accept this privacy and data-use notice.</span>
        </label>
        <label class="choice">
          <input type="checkbox" bind:checked={allowContact} />
          <span>Microsoft may contact me about my interest in Azure SQL.</span>
        </label>
        {#if allowContact}
          <label class="email-field" for="privacy-contact-email">
            <span>Contact email</span>
            <input
              id="privacy-contact-email"
              type="email"
              maxlength="254"
              autocomplete="email"
              bind:value={emailAddress}
              required
            />
          </label>
        {/if}
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        <div class="actions">
          <a
            class="sign-out"
            href="/.auth/logout?post_logout_redirect_uri=/"
            rel="external"
            data-sveltekit-reload>Sign out</a
          >
          <button class="accept" type="submit" disabled={!accepted || saving}>
            {saving ? 'Saving…' : 'Accept and continue'}
          </button>
        </div>
      </form>
    {:else}
      <footer>
        <button bind:this={firstControl} class="accept" type="button" onclick={onclose}
          >Close</button
        >
      </footer>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 30;
    display: grid;
    place-items: center;
    padding: 16px;
    background: rgb(15 30 35 / 62%);
  }
  .dialog {
    width: min(100%, 780px);
    max-height: min(92vh, 900px);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    overflow: hidden;
    background: #fff;
    border: 1px solid #aebdc0;
    border-radius: 6px;
    box-shadow: 0 22px 60px rgb(10 27 31 / 28%);
  }
  header {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 16px 20px;
    color: #f7fbfc;
    background: #17353d;
    border-bottom: 3px solid #087f73;
  }
  header p,
  header h2 {
    margin: 0;
  }
  header p {
    color: #b9cbd0;
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  header h2 {
    margin-top: 2px;
    font:
      680 1.2rem/1.2 Bahnschrift,
      sans-serif;
  }
  .policy-icon {
    display: grid;
    place-items: center;
    color: #8fe0d2;
  }
  .icon-button {
    width: 36px;
    height: 36px;
    display: grid;
    place-items: center;
    padding: 0;
    color: #e8f1f2;
    background: transparent;
    border: 0;
    border-radius: 4px;
    cursor: pointer;
  }
  .icon-button:hover {
    background: rgb(255 255 255 / 9%);
  }
  .notice-body {
    overflow-y: auto;
    padding: 20px;
    color: #3e5358;
  }
  .summary {
    margin-top: 0;
    padding: 12px 14px;
    color: #274249;
    background: #edf6f4;
    border-left: 4px solid #087f73;
  }
  section {
    margin-top: 18px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 0 0 7px;
    color: #19353b;
    font:
      680 0.94rem/1.25 Bahnschrift,
      sans-serif;
  }
  p,
  li {
    font-size: 0.84rem;
    line-height: 1.48;
  }
  section p {
    margin: 0;
  }
  ul {
    margin: 0;
    padding-left: 21px;
  }
  li + li {
    margin-top: 5px;
  }
  a {
    color: #006f86;
    font-weight: 650;
  }
  .summary a,
  section a {
    display: inline-flex;
    align-items: baseline;
    gap: 3px;
  }
  .safeguards {
    padding: 13px 14px;
    background: #f7f9f9;
    border: 1px solid #d3dddf;
  }
  .version {
    margin: 20px 0 0;
    color: #66797d;
    font-size: 0.72rem;
  }
  form,
  footer {
    padding: 15px 20px;
    background: #f7f9f9;
    border-top: 1px solid #cad4d7;
  }
  .choice {
    display: grid;
    grid-template-columns: 20px 1fr;
    align-items: start;
    gap: 8px;
    color: #2d464c;
    font-size: 0.84rem;
    font-weight: 600;
  }
  .choice + .choice {
    margin-top: 10px;
  }
  .choice input {
    width: 17px;
    height: 17px;
    margin: 1px 0 0;
    accent-color: #087f73;
  }
  .required-choice {
    color: #173b42;
    font-weight: 720;
  }
  .email-field {
    display: grid;
    gap: 5px;
    max-width: 430px;
    margin: 12px 0 0 28px;
    color: #3a5156;
    font-size: 0.76rem;
    font-weight: 700;
  }
  .email-field input {
    width: 100%;
    min-height: 38px;
    padding: 8px 10px;
    color: #182f34;
    background: #fff;
    border: 1px solid #91a5a9;
    border-radius: 4px;
  }
  .error {
    margin: 10px 0 0;
    color: #a62a20;
    font-weight: 650;
  }
  .actions,
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }
  .actions {
    margin-top: 14px;
  }
  .sign-out,
  .accept {
    min-height: 38px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 14px;
    border-radius: 4px;
    font-size: 0.83rem;
    font-weight: 720;
    text-decoration: none;
  }
  .sign-out {
    color: #40575c;
    background: #fff;
    border: 1px solid #9baaad;
  }
  .accept {
    color: #fff;
    background: #087f73;
    border: 1px solid #06685f;
    cursor: pointer;
  }
  .accept:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }
  @media (max-width: 600px) {
    .backdrop {
      padding: 8px;
    }
    .dialog {
      max-height: 96vh;
    }
    header,
    .notice-body,
    form,
    footer {
      padding-right: 14px;
      padding-left: 14px;
    }
    .actions {
      align-items: stretch;
      flex-direction: column-reverse;
    }
    .sign-out,
    .accept {
      width: 100%;
    }
    .email-field {
      margin-left: 0;
    }
  }
</style>
