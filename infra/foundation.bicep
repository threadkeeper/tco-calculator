targetScope = 'resourceGroup'

@minLength(3)
@maxLength(20)
param namePrefix string
param location string = 'southafricanorth'
param tags object = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
}

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

module managedEnvironment 'modules/managed-environment.bicep' = {
  name: 'managed-environment'
  params: {
    containerAppsSubnetId: network.outputs.containerAppsSubnetId
    location: location
    logAnalyticsCustomerId: monitoring.outputs.customerId
    logAnalyticsSharedKey: monitoring.outputs.sharedKey
    namePrefix: namePrefix
    tags: tags
  }
}

output containerAppsEnvironmentId string = managedEnvironment.outputs.id
output containerAppsEnvironmentName string = managedEnvironment.outputs.name
output containerAppsSubnetId string = network.outputs.containerAppsSubnetId
output cosmosAccountId string = cosmos.outputs.id
output cosmosAccountName string = cosmos.outputs.name
output cosmosEndpoint string = cosmos.outputs.endpoint
output cosmosPrivateEndpointId string = cosmosPrivateEndpoint.outputs.privateEndpointId
output cosmosPrivateDnsZoneId string = cosmosPrivateEndpoint.outputs.privateDnsZoneId
output logAnalyticsWorkspaceId string = monitoring.outputs.workspaceId
output privateEndpointSubnetId string = network.outputs.privateEndpointSubnetId
output registryId string = registry.outputs.id
output registryLoginServer string = registry.outputs.loginServer
output registryName string = registry.outputs.name
output virtualNetworkId string = network.outputs.virtualNetworkId