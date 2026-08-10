param namePrefix string
param location string
param tags object
param containerAppsSubnetId string
param logAnalyticsCustomerId string
@secure()
param logAnalyticsSharedKey string

resource environment 'Microsoft.App/managedEnvironments@2024-03-01' = {
  name: '${namePrefix}-cae'
  location: location
  tags: tags
  properties: {
    appLogsConfiguration: {
      destination: 'log-analytics'
      logAnalyticsConfiguration: {
        customerId: logAnalyticsCustomerId
        sharedKey: logAnalyticsSharedKey
      }
    }
    vnetConfiguration: {
      infrastructureSubnetId: containerAppsSubnetId
      internal: false
    }
    zoneRedundant: false
  }
}

output id string = environment.id
output name string = environment.name