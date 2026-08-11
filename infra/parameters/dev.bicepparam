using '../main.bicep'

param namePrefix = readEnvironmentVariable('AZURE_NAME_PREFIX')
param location = readEnvironmentVariable('AZURE_LOCATION')
param managedEnvironmentId = readEnvironmentVariable('AZURE_CONTAINER_APPS_ENVIRONMENT_ID')
param registryId = readEnvironmentVariable('AZURE_CONTAINER_REGISTRY_ID')
param registryServer = readEnvironmentVariable('AZURE_CONTAINER_REGISTRY')
param cosmosAccountId = readEnvironmentVariable('AZURE_COSMOS_ACCOUNT_ID')
param cosmosEndpoint = readEnvironmentVariable('COSMOSDB_ENDPOINT')
param privateEndpointSubnetId = readEnvironmentVariable('AZURE_PRIVATE_ENDPOINT_SUBNET_ID')
param virtualNetworkId = readEnvironmentVariable('AZURE_VIRTUAL_NETWORK_ID')
param containerImage = readEnvironmentVariable('CONTAINER_IMAGE')
param entraClientId = readEnvironmentVariable('ENTRA_CLIENT_ID')
param entraClientSecretUri = readEnvironmentVariable('ENTRA_CLIENT_SECRET_URI')
param authKeyVaultResourceGroup = readEnvironmentVariable('ENTRA_CLIENT_SECRET_KEY_VAULT_RESOURCE_GROUP')
param authKeyVaultName = readEnvironmentVariable('ENTRA_CLIENT_SECRET_KEY_VAULT_NAME')
param authKeyVaultId = readEnvironmentVariable('ENTRA_CLIENT_SECRET_KEY_VAULT_ID')
param tags = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
  monthlyBudgetUsd: '100'
}