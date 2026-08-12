# AWS Pricing Deployment Handoff

Date: 2026-08-11

## Goal

Finish validation and deploy commit `e6c46ac6a63334b2aa38820843c69ca4d5afe4e5` to the approved `dev` environment, then run `Pull AWS Pricing Data` to prime all eight supported AWS regions.

## Current Repository State

- Branch: `main`
- `HEAD` and `origin/main`: `e6c46ac6a63334b2aa38820843c69ca4d5afe4e5`
- Commit: `Add consent, assistant help, and pricing refresh`
- Main worktree was clean when this handoff was written.
- The commit contains pricing persistence, the refresh workflow, privacy consent, and assistant UI/help changes together. Do not assume it is pricing-only.
- Published workflow: `.github/workflows/pull-aws-pricing-data.yml`
- The workflow is scheduled at `0 1 * * *` and supports `workflow_dispatch`.

Temporary worktrees still registered:

- `C:/Repos/tco-calculator-pricing-validation`, detached at `b32a8ed80345d8a747da8120af186420cb6a06a6`. This is an older staged-tree validation snapshot, not current `main`.
- `C:/Repos/tco-calculator-coverage-ca822e6`, branch `coverage/ca822e6`.

Do not delete either worktree unless its owner confirms it is no longer needed.

## Implemented AWS Pricing Design

- The aggregate API/domain contract remains `AwsPriceSnapshot`.
- Cosmos persistence uses one deterministic current-state document per currency/region plus content-addressed EC2, RDS, and EBS component documents.
- Component IDs hash canonical core data. A separate full-record hash protects provenance and record integrity.
- Components are written and validated before state publication.
- `DurableSnapshotRepository::put_aws` returns the exact reconstructable aggregate persisted from retained components.
- Superseded service components are deleted only after confirming the published state is still current.
- Historical AWS provider records are intentionally not retained.
- Serialized component documents at or above the Cosmos 2 MiB item limit are rejected before the Cosmos request.
- Default live refresh quota changed from 6 to 8, allowing one complete supported-region sweep per requester identity each hour.

Primary files:

- `rust/src/pricing/snapshot.rs`
- `rust/src/persistence/pricing_cache.rs`
- `rust/src/persistence/cosmos.rs`
- `rust/src/pricing/repository.rs`
- `rust/src/pricing/coordinator.rs`
- `rust/src/config.rs`
- `infra/main.bicep`
- `infra/modules/container-app.bicep`
- `.github/workflows/pull-aws-pricing-data.yml`
- `research/Azure Specification.md`

## Local Toolchain

Visual C++ is installed and working:

- Visual Studio Build Tools 2022 `17.14.37`
- MSVC tools `14.44.35207`
- Linker: `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64\link.exe`

A newly opened ordinary PowerShell may not have the MSVC environment. Initialize it before Cargo commands:

```powershell
$install = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools'
Import-Module (Join-Path $install 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $install -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64'
```

## CI Blocker

CI run: <https://github.com/threadkeeper/tco-calculator/actions/runs/31509961080>

Successful jobs:

- Frontend quality
- Contracts and infrastructure
- Rust formatting

Failed job:

- Rust quality, `Run Clippy`

Actionable compile errors:

1. `rust/src/persistence/cosmos.rs:220`

   `self.get(owner_id, project_id)` is ambiguous because `CosmosProjectRepository` now implements both `ProjectRepository` and `PrivacyConsentRepository`. This call is in project repository behavior and should be explicitly dispatched to `ProjectRepository::get`.

2. `rust/src/api/privacy.rs` tests at approximately lines 214, 235, 245, 256, 282, and 308

   Tests call `.expect(...)` on `Result<_, Problem>`, but `Problem` does not implement `Debug`. Prefer assertions/matches that do not require changing the public Problem type solely for tests, unless deriving `Debug` is independently appropriate and reviewed.

After fixing, run:

```powershell
cargo +1.97.1 fmt --manifest-path rust/Cargo.toml --all -- --check
cargo +1.97.1 clippy --locked --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo +1.97.1 test --locked --manifest-path rust/Cargo.toml --all-features
az bicep build --file infra/main.bicep --stdout | Out-Null
git diff --check
```

Commit and push the CI fix only after all applicable checks pass. Then wait for successful `CI` on the new `main` SHA.

## Azure Deployment Blocker

The most recent deployment failures were for the previous commit `360f2aa`, not current `main`:

- <https://github.com/threadkeeper/tco-calculator/actions/runs/31508901411>
- <https://github.com/threadkeeper/tco-calculator/actions/runs/31508162665>

Both failed during `Preview application deployment` because OIDC principal object ID `e0f0bd17-58ab-4d93-bd93-0a413bc618d6` lacked:

```text
Microsoft.Authorization/roleAssignments/write
```

The denied scope was the ACR `AcrPull` role assignment under resource group `rg-tco`.

Before retrying deployment, verify the principal has an approved least-privilege role that permits the required role assignments at the reviewed scope. Do not broaden permissions or create role assignments without authorization. Useful read-only check:

```powershell
az role assignment list `
  --assignee-object-id e0f0bd17-58ab-4d93-bd93-0a413bc618d6 `
  --scope /subscriptions/168b5d01-88b5-489c-8246-5f346c834ca5/resourceGroups/rg-tco `
  --include-inherited `
  --query "[].{roleDefinitionName:roleDefinitionName,scope:scope}" `
  --output table
```

## Direct Development Deployment

After CI succeeds for the current `main` commit, any agent may dispatch or retry the development application workflow. A separate preview run is not required:

1. Start the serialized build, validation, and deployment run:

   ```powershell
   gh workflow run deploy-app.yml --ref main `
     -f confirmation='DEPLOY APP rg-tco'
   ```

2. Find and monitor its run ID:

   ```powershell
   gh run list --workflow deploy-app.yml --limit 5
   gh run watch <deploy-run-id> --exit-status
   ```

3. Review the run summary. The application `what-if` must contain no deletions or unexpected foundation changes, and the run must reject a commit superseded before Azure mutation.

4. Verify `/healthz`, `/readyz`, `/version`, and that the deployed immutable image digest corresponds to the selected commit.

## Configure the Refresh Workflow

The GitHub `dev` environment currently does not contain `TCO_CALCULATOR_URL` (the API returned HTTP 404 when queried).

After successful deployment, set it to the deployed HTTPS origin, with no path, query, or fragment:

```powershell
gh variable set TCO_CALCULATOR_URL `
  --repo threadkeeper/tco-calculator `
  --env dev `
  --body 'https://tcocalculator-app.niceforest-9cb86d58.southafricanorth.azurecontainerapps.io'
```

This is a non-secret application URL. Confirm it exists without printing secrets:

```powershell
gh variable list --repo threadkeeper/tco-calculator --env dev
```

## Prime AWS Pricing Data

After the new application is deployed and `TCO_CALCULATOR_URL` is configured:

```powershell
gh workflow run pull-aws-pricing-data.yml --ref main
```

Then monitor it:

```powershell
gh run list --workflow pull-aws-pricing-data.yml --limit 5
gh run watch <run-id> --exit-status
```

GitHub UI equivalent:

1. Open **Actions**.
2. Select **Pull AWS Pricing Data**.
3. Choose **Run workflow** on `main`.
4. Open the run and confirm all eight regions report `Refreshed <region> at <timestamp>`.

Regions refreshed sequentially:

- `eu-central-1`
- `eu-central-2`
- `eu-north-1`
- `eu-south-1`
- `eu-south-2`
- `eu-west-1`
- `eu-west-2`
- `eu-west-3`

The workflow calls only the application HTTPS API. It has no Cosmos credential or direct Cosmos access. It fails closed unless each response is `fresh` with a snapshot ID.

## Post-Prime Verification

- `GET /api/v1/catalog/aws/regions` should return all eight regions with available data.
- Check the workflow log for a fresh result from every region.
- Check Container App logs only for opaque request IDs/statuses; do not expose provider payloads or customer data.
- If a component exceeds Cosmos limits, the application should return a failure rather than truncate records.
- Do not manually write pricing documents into Cosmos.

## Safety Notes

- Do not deploy until CI passes for the exact commit selected from `main`.
- Do not dispatch an intentional duplicate when the same workflow and commit already has a queued or in-progress run.
- Do not bypass the same-run image-lock, deployment-validation, or `what-if` checks.
- Do not bypass the role-assignment authorization failure.
- Do not place credentials, tokens, identity headers, or Cosmos keys in the workflow.
- Do not send project/customer data to AWS pricing endpoints.
- Preserve current-only provider-data semantics; old aggregate snapshot IDs may stop resolving after a price change, while saved calculation revisions remain auditable because they embed the resolved rates.
