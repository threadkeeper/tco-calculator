targetScope = 'resourceGroup'

@minLength(3)
@maxLength(20)
param namePrefix string
param location string = 'southafricanorth'
param monthlyBudgetUsd string = '100'

var tags = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
  monthlyBudgetUsd: monthlyBudgetUsd
}

var foundryLocation = 'swedencentral'

resource virtualNetwork 'Microsoft.Network/virtualNetworks@2024-05-01' existing = {
  name: '${namePrefix}-vnet'
}

module foundry 'modules/foundry.bicep' = {
  name: 'foundry'
  params: {
    location: foundryLocation
    namePrefix: namePrefix
    tags: tags
  }
}

module foundryPrivateEndpoint 'modules/foundry-private-endpoint.bicep' = {
  name: 'foundry-private-endpoint'
  params: {
    foundryAccountId: foundry.outputs.accountId
    location: location
    namePrefix: namePrefix
    privateEndpointSubnetId: resourceId('Microsoft.Network/virtualNetworks/subnets', virtualNetwork.name, 'private-endpoints')
    tags: tags
    virtualNetworkId: virtualNetwork.id
  }
}

output foundryAccountId string = foundry.outputs.accountId
output foundryAccountName string = foundry.outputs.accountName
output foundryEndpoint string = foundry.outputs.endpoint
output foundryModelDeployment string = foundry.outputs.deploymentName
output foundryPrivateEndpointId string = foundryPrivateEndpoint.outputs.privateEndpointId
output foundryPrivateDnsZoneId string = foundryPrivateEndpoint.outputs.privateDnsZoneId