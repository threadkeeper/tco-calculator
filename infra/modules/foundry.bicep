param namePrefix string
param location string
param tags object

var accountName = '${namePrefix}-foundry'
var deploymentName = '${namePrefix}-model-router'

resource account 'Microsoft.CognitiveServices/accounts@2025-12-01' = {
  name: accountName
  location: location
  tags: tags
  kind: 'OpenAI'
  sku: {
    name: 'S0'
  }
  properties: {
    customSubDomainName: accountName
    disableLocalAuth: true
    networkAcls: {
      bypass: 'None'
      defaultAction: 'Deny'
      ipRules: []
      virtualNetworkRules: []
    }
    publicNetworkAccess: 'Disabled'
    storedCompletionsDisabled: true
  }
}

resource modelRouter 'Microsoft.CognitiveServices/accounts/deployments@2025-12-01' = {
  parent: account
  name: deploymentName
  properties: {
    model: {
      format: 'OpenAI'
      name: 'model-router'
      version: '2025-11-18'
    }
    routing: {
      mode: 'balanced'
      models: [
        {
          format: 'OpenAI'
          name: 'gpt-4.1-mini'
          version: '2025-04-14'
        }
        {
          format: 'OpenAI'
          name: 'gpt-5-mini'
          version: '2025-08-07'
        }
      ]
    }
    versionUpgradeOption: 'NoAutoUpgrade'
  }
  sku: {
    capacity: 10
    name: 'DataZoneStandard'
  }
}

output accountId string = account.id
output accountName string = account.name
output deploymentName string = modelRouter.name
output endpoint string = account.properties.endpoint