param namePrefix string
param location string
param tags object
param cosmosAccountId string
param privateEndpointSubnetId string
param virtualNetworkId string

resource privateDnsZone 'Microsoft.Network/privateDnsZones@2024-06-01' = {
  name: 'privatelink.documents.azure.com'
  location: 'global'
  tags: tags
}

resource virtualNetworkLink 'Microsoft.Network/privateDnsZones/virtualNetworkLinks@2024-06-01' = {
  parent: privateDnsZone
  name: '${namePrefix}-cosmos-link'
  location: 'global'
  tags: tags
  properties: {
    registrationEnabled: false
    virtualNetwork: {
      id: virtualNetworkId
    }
  }
}

resource privateEndpoint 'Microsoft.Network/privateEndpoints@2024-05-01' = {
  name: '${namePrefix}-cosmos-pe'
  location: location
  tags: tags
  properties: {
    privateLinkServiceConnections: [
      {
        name: 'cosmos-sql'
        properties: {
          groupIds: [
            'Sql'
          ]
          privateLinkServiceId: cosmosAccountId
        }
      }
    ]
    subnet: {
      id: privateEndpointSubnetId
    }
  }
}

resource dnsZoneGroup 'Microsoft.Network/privateEndpoints/privateDnsZoneGroups@2024-05-01' = {
  parent: privateEndpoint
  name: 'default'
  properties: {
    privateDnsZoneConfigs: [
      {
        name: 'cosmos-sql'
        properties: {
          privateDnsZoneId: privateDnsZone.id
        }
      }
    ]
  }
}

output privateEndpointId string = privateEndpoint.id
output privateDnsZoneId string = privateDnsZone.id