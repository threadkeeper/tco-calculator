param namePrefix string
param location string
param tags object

var accountName = toLower(replace('${namePrefix}-${uniqueString(resourceGroup().id)}', '-', ''))

resource account 'Microsoft.DocumentDB/databaseAccounts@2024-05-15' = {
  name: accountName
  location: location
  tags: tags
  kind: 'GlobalDocumentDB'
  properties: {
    capabilities: [
      {
        name: 'EnableServerless'
      }
    ]
    consistencyPolicy: {
      defaultConsistencyLevel: 'Session'
    }
    databaseAccountOfferType: 'Standard'
    disableKeyBasedMetadataWriteAccess: true
    disableLocalAuth: true
    enableAutomaticFailover: false
    enableFreeTier: false
    locations: [
      {
        failoverPriority: 0
        isZoneRedundant: false
        locationName: location
      }
    ]
    minimalTlsVersion: 'Tls12'
    networkAclBypass: 'None'
    publicNetworkAccess: 'Disabled'
  }
}

resource database 'Microsoft.DocumentDB/databaseAccounts/sqlDatabases@2024-05-15' = {
  parent: account
  name: 'tco'
  properties: {
    resource: {
      id: 'tco'
    }
  }
}

resource projects 'Microsoft.DocumentDB/databaseAccounts/sqlDatabases/containers@2024-05-15' = {
  parent: database
  name: 'projects'
  properties: {
    resource: {
      id: 'projects'
      partitionKey: {
        kind: 'Hash'
        paths: [
          '/owner_id'
        ]
        version: 2
      }
    }
  }
}

resource pricingCache 'Microsoft.DocumentDB/databaseAccounts/sqlDatabases/containers@2024-05-15' = {
  parent: database
  name: 'pricing-cache'
  properties: {
    resource: {
      defaultTtl: 2592000
      id: 'pricing-cache'
      partitionKey: {
        kind: 'Hash'
        paths: [
          '/cache_partition'
        ]
        version: 2
      }
    }
  }
}

output id string = account.id
output name string = account.name
output endpoint string = account.properties.documentEndpoint