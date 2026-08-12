# Family Dinner

This file is the repository's transient informational board for concurrent agents and chat sessions. It contains active work only. It never reserves source code, branches, refs, CI capacity, or permission to proceed; agents use it to avoid accidental operational collisions while continuing locally scoped work.

## Flight Controller

Snapshot UTC: `not set`. Durations, CI ownership, and mutex ownership are point-in-time values at this snapshot, not live counters.

Pipeline glow: **✨ `workflow-running` ✨** means a CI/CD run is queued or in progress. While `.family-dinner.lock/` exists, its `owner` file is the live mutex authority. Priority: 🔴 P0 urgent | 🟠 P1 high | 🟡 P2 normal | 🟢 P3 opportunistic.

| Agent / chat name | Status | Status reason | Status change time (UTC) | Status duration | Blocked by agent | CI owner | Mutex owner | Priority |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

_No active tasks._

## Coordination Rules

1. Read this board at task start and make a best-effort attempt to add one Flight Controller row and task entry. If the board is busy, missing, or malformed, continue isolated source work, validation, commits, and authorized pushes; retry the informational update later without waiting or starting a watcher solely for the board.
2. Give each task a unique ID and record its local worktree or branch, expected files, commands, ports, processes, cloud resources, and CI activity. These fields describe intent and current activity; they never grant exclusive ownership or permission and never make another task wait.
3. Use isolated worktrees or branches for concurrent source changes. Overlapping file paths, branches, target refs, CI runs, or advisory entries are not blockers. Pause only the exact operation that has a concrete collision on a running process, bound port, mutable generated output, deployment/environment mutation, cloud resource mutation, or unresolved Git conflict; continue every unrelated part of the task.
4. Keep the row and task entry current on a best-effort basis. Every agent normally edits only its own row and block; a protocol-maintenance task may repair the panel or remove board-created blockers while preserving factual task metadata. A busy board never delays implementation or delivery.
5. Use these statuses: `planning`, `active`, `waiting`, `workflow-running`, or `handoff`. An agent accepting a handoff MUST update the owner and status before continuing.
6. Do not place secrets, credentials, tokens, customer data, tenant or subscription identifiers, private URLs, or sensitive logs in this file. Use only the minimum coordination metadata.

## Flight Controller Rules

1. Keep **Flight Controller** as the first operational section and maintain exactly one row per active task. The row's agent/chat name MUST equal its task ID so blockers are unambiguous.
2. On every board write, set `Snapshot UTC` to the write time and refresh all status durations from each row's status-change time. A duration is a compact rounded-down value such as `8m`, `2h 14m`, or `3d 2h`; it is accurate only at the displayed snapshot.
3. Change a row's status-change time only when its status or status reason changes. Use `none`, an exact task ID, or `external: <short reason>` for a concrete blocker; never list the board, a source-path overlap, a target ref, an integration queue, or another task's CI ownership as a blocker.
4. In `CI owner`, identify each task's reserved, queued, or in-progress workflow with its workflow, ref, and short commit SHA; use `none` otherwise and clear terminal ownership promptly. When a run is queued or in progress, also use status `workflow-running` and render it as **✨ `workflow-running` ✨**. The cell is informational and does not reserve CI capacity or block another commit or push.
5. In `Mutex owner`, use `held at snapshot` only on the row whose task ID matches `.family-dinner.lock/owner` while writing the snapshot; use `none` elsewhere. The table is historical after its snapshot, so the owner file is authoritative while the lock exists and an absent lock directory means no current mutex owner.
6. Render priority as 🔴 P0 for urgent coordination, security, or release blockers; 🟠 P1 for high-priority correctness or release work; 🟡 P2 for normal work; and 🟢 P3 for opportunistic work.
7. A task owner controls its row's operational values. A protocol-maintenance task may bootstrap or repair the panel from existing task metadata but MUST NOT otherwise reprioritize or change another owner's reported state.

## Board Write Mutex

1. The mutex serializes edits to this board only. To write, atomically create `.family-dinner.lock/`, then immediately write the task ID to `.family-dinner.lock/owner`. If the directory exists, do not alter or delete it; skip or defer the board update and continue all source edits, tests, commits, pushes, and other locally scoped work without waiting.
2. After acquiring the mutex, re-read the board, apply the narrowest update, validate it, and release immediately. Never hold the mutex while doing task work, waiting for a decision, watching a process, or monitoring CI; never create a watcher merely to wait for this mutex.
3. Release the mutex in a cleanup step immediately after the board update is validated by deleting only the `.family-dinner.lock/` directory and owner marker created by that writer. Never commit the mutex directory, and never infer that another writer's lock is stale from elapsed time alone.

## GitHub Actions Coordination

1. Record a workflow operation, ref, exact commit SHA when known, and reason on a best-effort basis before manually changing a run. A busy or stale board never blocks an otherwise authorized workflow operation or an automatic run caused by a push.
2. Reuse an observed run for the exact same workflow/ref/commit instead of manually duplicating it. Distinct commits and their automatic CI runs may proceed independently; CI ownership in this board is informational rather than exclusive.
3. Add the run ID and non-sensitive URL when practical, monitor runs required by the task, and clear terminal ownership promptly. Do not wait for another task's unrelated run merely because it appears here.
4. Coordinate a genuinely shared deployment or environment mutation at that operational boundary. Local checks, commits, and pushes continue concurrently; substantive deployment authorization and safety rules remain controlling.
5. This board coordinates operations but never authorizes them. All approval, security, preview-deployment, environment, and retry restrictions in `.github/copilot-instructions.md` still apply.

## Integration Batches

1. Keep changes in focused, independently validated commits. Agents may push those commits independently as soon as they are ready and authorized; never wait solely to form a batch or for another board entry, target-ref owner, or CI owner to clear.
2. When multiple compatible commits are already ready, agents MAY combine them into one bounded fast-forward push while preserving commit boundaries and authorship. Record exact SHAs, ordering, expected base, and validation when convenient, but the board does not own or lock the target ref.
3. Before pushing, fetch the latest target and use normal Git integration. If a fast-forward push loses a race, fetch, rebase or merge according to repository policy, resolve only understood conflicts, rerun affected checks, and retry; do not wait for Family Dinner serialization.
4. Preserve constituent commits so failures remain attributable through test output, commit inspection, bisect, and focused revert. A successful exact-SHA CI run may be reused where substantive workflow policy allows.
5. Batching and board metadata never authorize preview or production deployment and never weaken review, security, rollback, exact-SHA, or environment-mutation controls.

## Completion And Cleanup

1. Task completion depends on the requested implementation, validation, and substantive workflow requirements, never on Family Dinner availability or metadata freshness.
2. Before the final response, make one best-effort attempt to remove the task's row and block. If the mutex is busy, do not wait and do not withhold completed work or the final response; a later successful board write may remove an entry whose owner explicitly reported completion.
3. For a handoff, keep only useful receiving context when the board is available. Failure to update the board does not block the receiver from proceeding with the actual branch or commit.
4. Do not remove another task merely because it looks old. Protocol maintenance may remove it only from explicit completion evidence; otherwise leave informationally stale data in place without blocking anyone.
5. When the final active entry is removed, restore `_No active tasks._` under **Active Tasks**.

## Active Tasks

_No active tasks._

<!--
When claiming the first active task, remove both empty-state lines above, set the Flight Controller snapshot, add one table row in the form below, and append one task block. Remove the row and entire block together when the task is complete:

| `<task-id>` | `<status>` or **✨ `workflow-running` ✨** | <short reason> | `<ISO 8601 UTC>` | <duration> | `<task-id>` / `external: <reason>` / `none` | `<workflow> / <ref> / <short-sha>` / `none` | `held at snapshot` / `none` | 🔴 P0 / 🟠 P1 / 🟡 P2 / 🟢 P3 |

### <task-id>

- Agent/session: <owner>
- Status: `planning` | `active` | `waiting` | `workflow-running` | `handoff`
- Scope: <brief task description>
- Informational scope: <local worktree/branch, expected paths, processes, ports, or shared resources>
- Shared resources: <commands, outputs, processes, ports, or none>
- GitHub Actions: <workflow, ref, commit, operation, run ID/URL/status, or none>
- Started UTC: <ISO 8601 timestamp>
- Updated UTC: <ISO 8601 timestamp>
- Next coordination point: <next action or handoff condition>
-->
