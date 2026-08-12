# Assistant Implementation Handoff

Snapshot date: 2026-08-11

## Completion Update: 2026-08-12

The 2026-08-11 content below is retained as the historical starting handoff. Its statements that Foundry inference, image intake, proposals, and persisted actions are not implemented are superseded by this update.

Implemented on `feature/autonomous-assistant-actions`:

- a request-bounded Rust reasoning loop with closed host tools, phase-aware schemas, preflight policy, model/tool/time budgets, cancellation, and host-authored awareness of its prompt version, available tools, selected-project state, completed action history, and request-only memory boundary;
- authenticated Foundry Model Router inference through managed identity and the approved private data plane, with project/workload names redacted and no identity or contact fields in model context;
- one-shot JPEG/PNG intake with 10 MiB, 25 megapixel, and 16,384-pixel dimension bounds, signature and decode checks, metadata removal, RGB JPEG normalization, and request-scoped byte disposal;
- typed extraction reports containing project-patch proposals, omissions, and uncertainties;
- a dedicated confirmed-action endpoint that re-reads and updates only the authenticated owner's project through the existing project service and current ETag;
- a Svelte upload, review, Apply/Cancel, stale-project, cancellation, and authoritative-editor-refresh flow; and
- OpenAPI and generated TypeScript contracts for image analysis, proposals, typed notes, and confirmed actions.

The deliberately small application security boundary is:

1. Email address, contact choice, display name, tenant/object claims, and owner identifiers are never included in prompts, image payloads, or model-visible tool data. The email field remains isolated to the privacy/contact workflow.
2. Signed-out users can open the panel but see `Please log in to use the TCO agent.` The browser does not expose the composer, image picker, or action controls and sends no assistant request until authenticated.
3. The server derives owner scope from the authenticated principal. Every project read and write uses that owner plus the project ID; another owner's project is returned as not found. ETags and explicit Apply remain data-integrity controls against stale or unintended updates.

The current persisted-action surface is intentionally one useful operation: apply a reviewed project patch. Additional create, delete, share, price-refresh, and client-navigation actions from the original capability matrix are not silently implied and require their own implementation when product demand justifies them.

## Historical Handoff

## Purpose

This document lets another agent resume the bottom-right TCO assistant work after a workstation shutdown. It records the repository state, completed deterministic implementation, validation evidence, unresolved compile failures, and controls that still block Foundry and image-processing work.

The original product goal is an application assistant that:

1. explains every field, button, result, and supported workflow in natural language;
2. uses JPEG or PNG upload as the primary v1 assisted project-input path;
3. proposes typed, validated project changes for review instead of saving automatically; and
4. performs only allowlisted application actions through existing server-owned authorization, validation, calculation, concurrency, confirmation, and persistence boundaries.

## Repository Snapshot

- Primary workspace: `C:\Repos\tco-calculator`
- Branch at capture: `main`
- Integrated remote base before the handoff commit: `d466ff4` (`Group pricing cache repositories into one constructor argument`)
- Assistant implementation commit: `e6c46ac` (`Add consent, assistant help, and pricing refresh`)
- Parent before that combined change: `360f2aa`
- This document is the only file in the handoff commit immediately above `d466ff4`.

Concurrent work was active during capture. `main` advanced from `e6c46ac` through `406feed`, `16a110f`, `24ed232`, and `d466ff4` while this handoff was being prepared. Always rerun `git status --short --branch`, `git log -3 --oneline --decorate`, and `git worktree list --porcelain` before editing.

Active worktrees at capture:

| Path | State |
| --- | --- |
| `C:\Repos\tco-calculator` | `main` based on `d466ff4` plus this handoff commit |
| `C:\Repos\tco-calculator-coverage-ca822e6` | branch `coverage/ca822e6` at `63e2896` |
| `C:\Repos\tco-calculator-pricing-validation` | detached at `b32a8ed` |

Do not remove, reset, repoint, or reuse either secondary worktree without first determining whether its owner has finished. Commit `406feed` and `docs/AWS-PRICING-DEPLOYMENT-HANDOFF.md` belong to separate AWS pricing work.

The `e6c46ac` commit is not assistant-only. It combines assistant, privacy-consent, pricing-cache, workflow, infrastructure, and documentation changes across 36 files. Do not revert, cherry-pick, or rewrite that whole commit merely to change the assistant. Work from the owning files listed below.

## Controlling Decisions

Read these before changing behavior:

1. `.github/copilot-instructions.md`
2. `research/Azure Specification.md`, especially sections 4.1 through 4.3 and the security, API, identity, and completion requirements
3. `research/design clarificaitons.md`, especially DC-010
4. `docs/FOUNDRY-ASSISTANT-APPROVAL-PROPOSAL.md`
5. `THIRD-PARTY-DATA-EGRESS.md`

Product scope was approved on 2026-08-11 with JPEG/PNG as the primary v1 assisted-input method. CSV, Excel, and PDF are deferred.

That product decision did **not** approve live model calls, model routing, customer/image egress, an image decoder or normalizer, Foundry resources, deployment changes, or new dependencies. The proposal currently records these approvals as pending:

- Architecture
- Security and threat model
- Privacy and data residency
- Legal/OSS and model/service terms
- Procurement/cost owner
- Operations/service owner
- Accessibility
- Approved specification change reference
- Approved dependency/service records
- Approval date and review expiry

`.github/copilot-instructions.md` still lists LLM, generative-AI, model-routing, and related services as not approved for the MVP. It may only be changed after an explicit request and human review. Until the applicable approvals are recorded, do not add Foundry calls, model routing, image decoding, multipart upload dependencies, customer-data egress, Foundry Bicep, model credentials, or assistant deployment wiring.

## Completed Safe Slice

Only deterministic application help has been implemented. It has no model dependency and no third-party egress.

### Backend

`rust/src/api/assistant.rs` contains:

- a reviewed static help catalog with stable `control_id` values, labels, keywords, and natural-language explanations;
- bounded input of 1 through 1,000 Unicode characters after trimming;
- deterministic keyword/phrase scoring with at most three matches;
- a safe unsupported-question response;
- explicit language preserving server authority for financial calculations and target selection;
- `serde(deny_unknown_fields)` on the request;
- `Cache-Control: no-store` on successful responses; and
- five focused unit tests for matching, financial boundaries, fallback behavior, length limits, and unique control IDs.

Routing and middleware:

- `rust/src/api/mod.rs` exports the assistant module.
- `rust/src/server.rs` routes `POST /api/v1/assistant/help` inside the existing consent-gated API router.
- Guest requests are allowed through the existing privacy middleware and produce deterministic help only.
- An authenticated principal that has not accepted the current privacy notice receives the normal `428 Privacy Consent Required` response.
- The endpoint has no Foundry, upload, persistence, calculation, or mutation capability.

### OpenAPI and Generated Types

`openapi/openapi.yaml` now defines:

- the `Assistant` tag;
- `POST /assistant/help` with operation ID `getAssistantHelp`;
- `AssistantHelpRequest`;
- `AssistantHelpResponse`;
- `AssistantHelpReference`; and
- a successful `no-store` response contract.

`web/src/lib/api/generated.ts` was regenerated from OpenAPI. Never edit it by hand. Regenerate it with:

```powershell
npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run api:generate
```

### Typed Frontend Boundary

`web/src/lib/assistant.ts`:

- aliases the generated OpenAPI request, response, and reference types;
- trims and counts Unicode code points for the 1,000-character limit;
- sends a same-origin JSON request with `cache: 'no-store'` and optional `AbortSignal`; and
- runtime-validates untrusted response shape and the maximum of three references.

`web/src/lib/assistant.test.ts` has three focused tests covering:

- trimmed, non-cacheable requests;
- empty, Unicode, and oversized input boundaries; and
- malformed or excessive response references.

### Assistant Panel

`web/src/lib/components/AssistantPanel.svelte` implements:

- a fixed bottom-right icon launcher;
- a compact desktop panel and mobile bottom sheet;
- an in-memory transcript that is neither server-persisted nor browser-durable;
- plain-text rendering only, with no model-authored HTML;
- Enter to send and Shift+Enter for a newline;
- an accessible label, dialog, live transcript, status messages, and icon tooltips;
- focus on the composer after opening;
- close-button and Escape-key support with focus returned to the launcher;
- request cancellation through `AbortController`;
- clear-conversation behavior;
- input count and send-state bounds;
- loading, error, and cancellation states; and
- reduced-motion support.

The Escape behavior was found defective during browser validation when implemented with `<svelte:window onkeydown>`. It was replaced by an explicit `onMount` `window.addEventListener` with cleanup, then revalidated successfully. Preserve that behavior or add an automated regression test before changing it.

`web/src/routes/+page.svelte` mounts the panel only when:

- the session is a guest; or
- the session is authenticated and the current privacy notice has been accepted.

It is absent while session state is loading or offline and while authenticated privacy acceptance is required.

No upload picker is shown. This is intentional: presenting image analysis before the decoder, privacy, residency, egress, model, and service approvals exist would imply a capability and data flow that the application cannot safely provide.

## Validation Evidence

The following frontend gates passed against the implemented slice:

```text
npm run check: 0 errors, 0 warnings
npm test: 5 files passed, 21 tests passed
npm run lint: ESLint and Prettier passed
npm run lockfile:check: 241 entries validated through the Microsoft npm proxy
npm run build: production Svelte static build succeeded
npm audit --audit-level=high: no high or critical findings
```

The audit reported three existing low-severity `cookie` findings. npm proposed a breaking forced change, so no dependency change was made. Reassess through the repository's dependency-approval process; do not run `npm audit fix --force`.

Rust formatting passed:

```powershell
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
```

After integrating the concurrent compile fixes in `24ed232`, the focused backend test passed:

```text
cargo test --manifest-path rust/Cargo.toml assistant: 5 passed, 0 failed
```

Editor diagnostics were clean for the touched assistant, routing, OpenAPI, generated-type, helper, test, panel, and root-page files.

### Browser Validation

The panel was exercised against a local Vite server with synthetic Playwright route responses for session, region catalogs, and assistant help. No real customer data or external model call was used.

Verified behavior:

- application and launcher load in guest mode;
- opening focuses the composer;
- Enter sends a question and clears the composer;
- plain-text answer and related-control labels render;
- cancellation aborts the HTTP request, clears pending state, and shows a cancellation status;
- the close button returns focus to the launcher;
- Escape closes the stable panel and returns focus to the launcher after the explicit listener fix;
- desktop viewport `1440 x 900`: panel settled at `390 x 560` inside the viewport;
- mobile viewport `390 x 844`: bottom sheet settled at approximately `390 x 620`; and
- no horizontal page overflow was present on mobile.

The browser validation used `http://127.0.0.1:5173/`. A production application tab happened to be open, but it was not used to validate or deploy this change. Do not infer that the assistant is deployed from this handoff.

## Rust Compile Blockers Resolved During Handoff

The MSVC linker is available. An initial focused run could not start assistant tests because of two committed errors outside `rust/src/api/assistant.rs`:

1. `rust/src/persistence/cosmos.rs` had an ambiguous `self.get(owner_id, project_id)` call after `CosmosProjectRepository` implemented both project and privacy-consent repository traits (`E0034`).
2. Tests in `rust/src/api/privacy.rs` called `.expect(...)` on `Result<_, Problem>` while `Problem` did not implement `Debug` (`E0277`).

Concurrent commit `24ed232` resolved both owning-slice errors. The handoff commit was rebased onto that fix, and `cargo test --manifest-path rust/Cargo.toml assistant` then completed with all five assistant tests passing.

Only the focused assistant tests were rerun after integration. Do not claim the full Rust suite, Clippy, audit, or deny gates pass until they execute successfully.

## Recommended Resume Sequence

1. Confirm concurrent state:

   ```powershell
   git status --short --branch
   git log -3 --oneline --decorate
   git worktree list --porcelain
   ```

2. Read the controlling files listed above and this handoff. Treat the approval proposal's pending entries as hard gates.
3. Verify that the assistant files still match this snapshot. Other agents were active during handoff creation.
4. Run focused validation to confirm the integrated compile fixes and assistant behavior remain intact:

   ```powershell
   cargo fmt --manifest-path rust/Cargo.toml --all -- --check
   cargo test --manifest-path rust/Cargo.toml assistant
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run check
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ test -- assistant.test.ts
   ```

5. Run the broader applicable gates after focused checks pass:

   ```powershell
   cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
   cargo test --manifest-path rust/Cargo.toml --all-features
   cargo audit
   cargo deny check
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run lint
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ test
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run lockfile:check
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ audit --audit-level=high
   npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run build
   ```

6. The next approval-safe assistant increment is to audit the deterministic help catalog against every currently visible control and add a coverage test for stable control IDs. A component or Playwright regression test for open/send/cancel/Escape/focus/mobile behavior would also convert the manual evidence above into repeatable CI evidence.
7. Stop before implementing image intake, model inference, Foundry infrastructure, or allowlisted mutation tools unless the exact applicable approvals have been recorded in a reviewed change.

## Future Work After Approval

Proceed as separate reviewed increments, not one broad implementation:

1. Record exact model, router subset, region/processing geography, API version, deployment type, content filtering, quotas, costs, data terms, retention, and approvers.
2. Record and approve the exact image decoder/normalizer dependency, license, provenance, native/build behavior, transitive graph, vulnerabilities, and rollback plan.
3. Add off-by-default server feature flags and an operator kill switch.
4. Add bounded JPEG/PNG intake with signature validation, decoded-dimension limits, metadata removal, normalization, cancellation, and request-scoped byte disposal.
5. Add a typed extraction result and deterministic project-draft/patch validator. Show mappings, omissions, uncertainty, and field-level preview. Never auto-save.
6. Add the TCO-owned bounded model/tool loop using system-assigned managed identity and the approved private Foundry data plane. Foundry supplies inference only.
7. Add read-only tools first, then reversible draft actions, then persisted actions with fresh owner/ETag checks and dedicated confirmation.
8. Add Foundry private endpoint/DNS and least-privilege RBAC in reviewed Bicep without changing the one-image, one-Container-App topology.
9. Add deterministic mock-model tests, synthetic image fixtures, tenant/object authorization tests, cancellation/budget tests, prompt-injection tests, privacy/logging tests, accessibility tests, and controlled non-blocking live probes.

## Non-Negotiable Boundaries

- Financial values, rates, target selection, totals, savings, explanations, revisions, authorization, and persistence remain server-authoritative and deterministic.
- Model output, upload content, project text, and tool results are untrusted data.
- Never accept model-authored identity, owner, partition, ETag, endpoint, credential, confirmation, or authorization fields.
- Never expose arbitrary HTTP, DOM selectors, JavaScript, SQL, shell, provider-console, credential, or Azure control-plane capabilities.
- Never persist raw images, normalized images, or chat transcripts in v1.
- Never send customer project data or images to AWS or unapproved services.
- Never add API keys, service-principal secrets, user credentials, or broad credential-chain fallback for production inference.
- Never hand-edit generated OpenAPI TypeScript.
- Use `npm` and the official Microsoft proxy for every npm command that can access a registry.
- Preserve unrelated user and agent changes, especially the active pricing and coverage worktrees.

## Local Restart Commands

After reboot, the local frontend can be started with:

```powershell
npm --prefix web --registry=https://packagefeedproxy.microsoft.io/npm/ run dev -- --host 127.0.0.1
```

The expected default URL is `http://127.0.0.1:5173/`, unless that port is already occupied. The deterministic help endpoint requires the Rust API for an unmocked end-to-end run; frontend-only browser checks may use synthetic same-origin route mocks as described above.

Before continuing, inspect `git status` again. This document intentionally does not authorize deployment, production actions, dependency installation, control-file changes, or removal of another agent's worktree.