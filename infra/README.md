# Azure Development Infrastructure

The Bicep templates provision one development environment in South Africa North. This hosting location is independent of the SQL Managed Instance target region selected inside a TCO project.

## Resources

- VNet-integrated Azure Container Apps managed environment and one externally accessible Container App.
- Azure Container Registry Basic with the admin account disabled.
- Cosmos DB for NoSQL with `EnableServerless`, no provisioned throughput, local/key auth disabled, and public network access disabled.
- `projects` and TTL-enabled `pricing-cache` containers.
- Cosmos private endpoint and private DNS.
- Log Analytics workspace with 30-day retention.
- System-assigned identity on the Container App with only ACR pull and Cosmos data-contributor roles.

No user-assigned identity, resource key, connection string, service-principal secret, production parameter set, or application administrator role is deployed.

## Parameters

Copy [.env.example](.env.example) to the ignored `.env` only for local command preparation. Never commit real values. `ENTRA_CLIENT_SECRET_URI` is a versioned Key Vault secret URI, not the secret itself. `CONTAINER_IMAGE` must be an immutable SHA-tagged ACR image.

The Entra application registration is created and reviewed separately. Configure it with the `AzureADMultipleOrgs` audience and the Container App callback URL. Personal Microsoft accounts are excluded.

## Validate

```powershell
az bicep build --file infra/main.bicep
```

Validation compiles the template only and does not access or mutate Azure resources.

## Preview

Authenticate through the approved Azure CLI flow, select the intended development subscription, and run a what-if before any deployment:

```powershell
az deployment group what-if --resource-group <resource-group> --parameters infra/parameters/dev.bicepparam
```

Review resource replacement, public access, identity, role assignments, secret references, serverless capabilities, regions, tags, and image digest. Do not proceed if what-if differs from the reviewed plan.

## Deploy and Roll Back

Deployment requires explicit repository-owner authorization after what-if review. Use GitHub OIDC or the approved interactive operator identity; do not use an Azure client secret. Roll back application code by redeploying a previously reviewed immutable image tag. Infrastructure rollback is another reviewed Bicep deployment, never an ad hoc destructive command.

The development environment scales to zero and is best-effort. It has no production SLA, RTO, RPO, cross-region failover, or production/test parameter set.