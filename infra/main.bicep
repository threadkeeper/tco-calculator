targetScope = 'resourceGroup'

@minLength(3)
@maxLength(20)
param namePrefix string
param location string = 'southafricanorth'
param containerImage string
param entraClientId string
param entraClientSecretUri string
@minValue(1)
param guestRequestsPerMinute int = 60
@minValue(1)
param providerRefreshesPerHour int = 6
@minValue(1)
param calculationConcurrency int = 10
param tags object = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
}

var registryName = toLower(replace('${namePrefix}${uniqueString(resourceGroup().id)}', '-', ''))
var cosmosAccountName = toLower(replace('${namePrefix}-${uniqueString(resourceGroup().id)}', '-', ''))
var containerAppName = '${namePrefix}-app'

module monitoring 'modules/monitoring.bicep' = {
  name: 'monitoring'
  params: {
    location: location
    namePrefix: namePrefix
    tags: tags
  }
}

module network 'modules/network.bicep' = {
  name: 'network'
  params: {
    location: location
    namePrefix: namePrefix
    tags: tags
  }
}

module registry 'modules/registry.bicep' = {
  name: 'registry'
  params: {
    location: location
    namePrefix: namePrefix
    tags: tags
  }
}

module cosmos 'modules/cosmos.bicep' = {
  name: 'cosmos'
  params: {
    location: location
    namePrefix: namePrefix
    tags: tags
  }
}

module cosmosPrivateEndpoint 'modules/cosmos-private-endpoint.bicep' = {
  name: 'cosmos-private-endpoint'
  params: {
    cosmosAccountId: cosmos.outputs.id
    location: location
    namePrefix: namePrefix
    privateEndpointSubnetId: network.outputs.privateEndpointSubnetId
    tags: tags
    virtualNetworkId: network.outputs.virtualNetworkId
  }
}

module containerApp 'modules/container-app.bicep' = {
  name: 'container-app'
  params: {
    containerAppsSubnetId: network.outputs.containerAppsSubnetId
    containerImage: containerImage
    calculationConcurrency: calculationConcurrency
    entraClientId: entraClientId
    entraClientSecretUri: entraClientSecretUri
    guestRequestsPerMinute: guestRequestsPerMinute
    location: location
    logAnalyticsCustomerId: monitoring.outputs.customerId
    logAnalyticsSharedKey: monitoring.outputs.sharedKey
    namePrefix: namePrefix
    providerRefreshesPerHour: providerRefreshesPerHour
    registryServer: registry.outputs.loginServer
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
    scope: cosmosResource.id
  }
}

output containerAppFqdn string = containerApp.outputs.fqdn
output containerAppPrincipalId string = containerApp.outputs.principalId
output cosmosEndpoint string = cosmos.outputs.endpoint
output registryLoginServer string = registry.outputs.loginServer