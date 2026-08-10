param namePrefix string
param location string
param tags object
param applicationRegion string
param containerAppsSubnetId string
param logAnalyticsCustomerId string
@secure()
param logAnalyticsSharedKey string
param registryServer string
param containerImage string
param cosmosEndpoint string
@minLength(36)
@maxLength(36)
param entraClientId string
@secure()
@minLength(1)
param entraClientSecretUri string
@minValue(1)
param guestRequestsPerMinute int = 60
@minValue(1)
param providerRefreshesPerHour int = 6
@minValue(1048576)
@maxValue(268435456)
param providerMaxResponseBytes int = 67108864
@minValue(1)
param calculationConcurrency int = 10

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

resource app 'Microsoft.App/containerApps@2024-03-01' = {
  name: '${namePrefix}-app'
  location: location
  tags: tags
  identity: {
    type: 'SystemAssigned'
  }
  properties: {
    configuration: {
      activeRevisionsMode: 'Single'
      ingress: {
        allowInsecure: false
        external: true
        targetPort: 8080
        traffic: [
          {
            latestRevision: true
            weight: 100
          }
        ]
        transport: 'auto'
      }
      registries: [
        {
          identity: 'system'
          server: registryServer
        }
      ]
      secrets: [
        {
          identity: 'system'
          keyVaultUrl: entraClientSecretUri
          name: 'entra-client-secret'
        }
      ]
    }
    environmentId: environment.id
    template: {
      containers: [
        {
          env: [
            {
              name: 'APP_ENV'
              value: 'development'
            }
            {
              name: 'HTTP_BIND'
              value: '0.0.0.0:8080'
            }
            {
              name: 'AZURE_REGION'
              value: applicationRegion
            }
            {
              name: 'COSMOSDB_ENDPOINT'
              value: cosmosEndpoint
            }
            {
              name: 'GUEST_REQUESTS_PER_MINUTE'
              value: string(guestRequestsPerMinute)
            }
            {
              name: 'PROVIDER_REFRESHES_PER_HOUR'
              value: string(providerRefreshesPerHour)
            }
            {
              name: 'PROVIDER_MAX_RESPONSE_BYTES'
              value: string(providerMaxResponseBytes)
            }
            {
              name: 'CALCULATION_CONCURRENCY'
              value: string(calculationConcurrency)
            }
          ]
          image: containerImage
          name: 'azure-sql-tco'
          probes: [
            {
              httpGet: {
                path: '/healthz'
                port: 8080
                scheme: 'HTTP'
              }
              initialDelaySeconds: 2
              periodSeconds: 10
              type: 'Liveness'
            }
            {
              httpGet: {
                path: '/readyz'
                port: 8080
                scheme: 'HTTP'
              }
              initialDelaySeconds: 3
              periodSeconds: 10
              type: 'Readiness'
            }
          ]
          resources: {
            cpu: json('0.5')
            memory: '1Gi'
          }
        }
      ]
      scale: {
        maxReplicas: 3
        minReplicas: 0
        rules: [
          {
            http: {
              metadata: {
                concurrentRequests: '50'
              }
            }
            name: 'http-concurrency'
          }
        ]
      }
    }
  }
}

resource auth 'Microsoft.App/containerApps/authConfigs@2024-03-01' = {
  parent: app
  name: 'current'
  properties: {
    globalValidation: {
      unauthenticatedClientAction: 'AllowAnonymous'
    }
    httpSettings: {
      forwardProxy: {
        convention: 'NoProxy'
      }
      requireHttps: true
    }
    identityProviders: {
      azureActiveDirectory: {
        enabled: true
        registration: {
          clientId: entraClientId
          clientSecretSettingName: 'entra-client-secret'
          openIdIssuer: uri(az.environment().authentication.loginEndpoint, 'common/v2.0')
        }
        validation: {
          allowedAudiences: [
            entraClientId
          ]
        }
      }
    }
    login: {
      allowedExternalRedirectUrls: []
      nonce: {
        validateNonce: true
      }
      preserveUrlFragmentsForLogins: false
      tokenStore: {
        enabled: false
      }
    }
    platform: {
      enabled: true
      runtimeVersion: '~1'
    }
  }
}

output id string = app.id
output name string = app.name
output principalId string = app.identity.principalId
output fqdn string = app.properties.configuration.ingress.fqdn