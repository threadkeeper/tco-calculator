targetScope = 'resourceGroup'

@minLength(3)
@maxLength(20)
param namePrefix string
param location string = 'southafricanorth'
@description('Existing Container Apps managed environment resource ID from the foundation deployment.')
param managedEnvironmentId string
@description('Existing Azure Container Registry resource ID from the foundation deployment.')
param registryId string
@description('Existing Azure Container Registry login server from the foundation deployment.')
param registryServer string
@description('Existing Cosmos DB account resource ID from the foundation deployment.')
param cosmosAccountId string
@description('Existing HTTPS Cosmos DB account endpoint from the foundation deployment.')
param cosmosEndpoint string
@description('Existing private-endpoint subnet resource ID from the foundation deployment.')
param privateEndpointSubnetId string
@description('Existing virtual network resource ID from the foundation deployment.')
param virtualNetworkId string
param containerImage string
@minLength(36)
@maxLength(36)
param entraClientId string
@secure()
@minLength(1)
param entraClientSecretUri string
@description('Resource group containing the Key Vault referenced by entraClientSecretUri.')
param authKeyVaultResourceGroup string
@description('Key Vault name referenced by entraClientSecretUri.')
param authKeyVaultName string
@description('Key Vault resource ID referenced by entraClientSecretUri.')
param authKeyVaultId string
@description('Apply the Key Vault-backed Container Apps Entra authentication configuration.')
param configureAuthentication bool = true
@minValue(0)
@maxValue(3)
@description('Minimum application replicas. Deployment bootstrap uses one; steady state uses zero.')
param minimumReplicas int = 0
@minValue(1)
param guestRequestsPerMinute int = 60
@minValue(1)
param providerRefreshesPerHour int = 40
@minValue(1048576)
@maxValue(268435456)
param providerMaxResponseBytes int = 67108864
@minValue(1)
param calculationConcurrency int = 10
param tags object = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
}

var containerAppName = '${namePrefix}-app'
var registryName = last(split(registryId, '/'))
var cosmosAccountName = last(split(cosmosAccountId, '/'))

module keyVaultPrivateEndpoint 'modules/key-vault-private-endpoint.bicep' = {
  name: 'key-vault-private-endpoint'
  params: {
    keyVaultId: authKeyVaultId
    location: location
    namePrefix: namePrefix
    privateEndpointSubnetId: privateEndpointSubnetId
    tags: tags
    virtualNetworkId: virtualNetworkId
  }
}

module containerApp 'modules/container-app.bicep' = {
  name: 'container-app'
  dependsOn: [
    keyVaultPrivateEndpoint
  ]
  params: {
    applicationRegion: location
    containerImage: containerImage
    calculationConcurrency: calculationConcurrency
    configureAuthentication: configureAuthentication
    cosmosEndpoint: cosmosEndpoint
    entraClientId: entraClientId
    entraClientSecretUri: entraClientSecretUri
    guestRequestsPerMinute: guestRequestsPerMinute
    location: location
    managedEnvironmentId: managedEnvironmentId
    minimumReplicas: minimumReplicas
    namePrefix: namePrefix
    providerRefreshesPerHour: providerRefreshesPerHour
    providerMaxResponseBytes: providerMaxResponseBytes
    registryServer: registryServer
    tags: tags
  }
}

resource registryResource 'Microsoft.ContainerRegistry/registries@2023-07-01' existing = {
  name: registryName
}

resource cosmosResource 'Microsoft.DocumentDB/databaseAccounts@2024-05-15' existing = {
  name: cosmosAccountName
}

resource acrPull 'Microsoft.Authorization/roleAssignments@2022-04-01' = {
  name: guid(registryResource.id, containerAppName, 'AcrPull')
  scope: registryResource
  properties: {
    principalId: containerApp.outputs.principalId
    principalType: 'ServicePrincipal'
    roleDefinitionId: subscriptionResourceId('Microsoft.Authorization/roleDefinitions', '7f951dda-4ed3-4680-a7ca-43fe172d538d')
  }
}

resource cosmosDataContributor 'Microsoft.DocumentDB/databaseAccounts/sqlRoleAssignments@2024-05-15' = {
  parent: cosmosResource
  name: guid(cosmosResource.id, containerAppName, 'CosmosDataContributor')
  properties: {
    principalId: containerApp.outputs.principalId
    roleDefinitionId: '${cosmosResource.id}/sqlRoleDefinitions/00000000-0000-0000-0000-000000000002'
    scope: '${cosmosResource.id}/dbs/tco'
  }
}

module keyVaultAccess 'modules/key-vault-access.bicep' = {
  name: 'key-vault-access'
  scope: resourceGroup(authKeyVaultResourceGroup)
  params: {
    keyVaultName: authKeyVaultName
    principalId: containerApp.outputs.principalId
  }
}

output containerAppId string = containerApp.outputs.id
output containerAppName string = containerApp.outputs.name
output containerAppFqdn string = containerApp.outputs.fqdn
output containerAppPrincipalId string = containerApp.outputs.principalId
output containerImage string = containerImage
output keyVaultPrivateEndpointId string = keyVaultPrivateEndpoint.outputs.privateEndpointId
output keyVaultPrivateDnsZoneId string = keyVaultPrivateEndpoint.outputs.privateDnsZoneId