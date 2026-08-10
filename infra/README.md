# Azure Development Infrastructure

The Bicep templates define one development environment in South Africa North. This hosting location is independent of the SQL Managed Instance target region selected inside a TCO project. Foundation provisioning and application delivery are deliberately separate.

## Foundation Boundary

- VNet and delegated Container Apps and private-endpoint subnets.
- Log Analytics workspace with 30-day retention.
- Azure Container Registry Basic with the admin account disabled.
- Cosmos DB for NoSQL with `EnableServerless`, no provisioned throughput, local/key auth disabled, and public network access disabled.
- `projects` and TTL-enabled `pricing-cache` containers.
- Cosmos private endpoint and private DNS.
- VNet-integrated Azure Container Apps managed environment.

The README button and `infra/foundation.bicep` stop at this boundary. They do not build or push an image, create a Container App, configure Container Apps authentication, read an Entra secret, or create runtime role assignments.

The separate application workflow owns the OCI image, `infra/main.bicep` application layer, externally accessible Container App, built-in Entra authentication, system-assigned identity, ACR pull and Cosmos data-contributor assignments, readiness checks, and application updates. No user-assigned identity, resource key, connection string, service-principal secret, production parameter set, or application administrator role is approved.

## Parameters

Copy [.env.example](.env.example) to the ignored `.env` only for local command preparation. Never commit real values. The foundation workflow needs only the `AZURE_*` deployment identifiers and settings already configured as GitHub `dev` environment variables. It uses GitHub OIDC and no Azure client secret.

Application deployment later requires an independently reviewed Entra registration and a versioned `ENTRA_CLIENT_SECRET_URI`; the URI is a Key Vault secret reference, not the secret itself. `CONTAINER_IMAGE` must be immutable. The Entra registration uses the `AzureADMultipleOrgs` audience and the Container App callback URL; personal Microsoft accounts are excluded.

## Validate

```powershell
az bicep build --file infra/foundation.bicep
az bicep build --file infra/main.bicep
```

Validation compiles the template only and does not access or mutate Azure resources.

## Preview

Authenticate through the approved Azure CLI flow, select the intended development subscription, and run a what-if before any deployment:

```powershell
$env:AZURE_NAME_PREFIX = 'tcocalculator'
$env:AZURE_LOCATION = 'southafricanorth'
$env:AZURE_MONTHLY_BUDGET_USD = '100'
az deployment group what-if --resource-group <resource-group> --parameters infra/parameters/foundation-dev.bicepparam
```

Review resource replacement, public access, serverless capabilities, private connectivity, regions, and tags. The foundation plan must not contain `Microsoft.App/containerApps`, an image, an auth config, an application secret reference, or a runtime role assignment. Do not proceed if what-if differs from the reviewed plan.

## Deploy and Roll Back

Foundation deployment requires explicit repository-owner authorization after what-if review. Use GitHub OIDC or the approved interactive operator identity; do not use an Azure client secret. Infrastructure rollback is another reviewed Bicep deployment, never an ad hoc destructive command.

Open the manual **Deploy Azure foundation** workflow from the root README. Run `preview`, review the Azure resource changes, then run `deploy` for the same commit with the successful preview run ID and the exact confirmation requested by the workflow. The foundation OIDC principal uses its existing resource-group-scoped `Contributor` assignment; the foundation workflow creates no role assignments.

After deployment, collect the discovered identifiers into the ignored `infra/.env`:

```powershell
./infra/getkeys.ps1
```

To write the same non-secret identifiers to GitHub environment variables and store only the versioned Key Vault URI as an environment secret reference:

```powershell
./infra/getkeys.ps1 -PublishGitHub
```

Immediately after foundation deployment, the script records the managed-environment, ACR, Cosmos, private endpoint/DNS, VNet/subnet, and Log Analytics identifiers. Container App, image, and auth fields remain blank until the separate application workflow creates them; rerunning the script then enriches the same file.

The script does not retrieve an Entra secret value, Azure resource key, connection string, token, or certificate. `-PublishGitHub` publishes non-empty identifiers as `dev` environment variables and publishes only an already-known versioned `ENTRA_CLIENT_SECRET_URI` as a secret reference. Review the generated `.env` before publishing it. Run publication under a GitHub CLI session allowed to manage the repository's `dev` environment.

The development environment scales to zero and is best-effort. It has no production SLA, RTO, RPO, cross-region failover, or production/test parameter set.