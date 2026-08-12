param namePrefix string
param location string
param tags object
param applicationRegion string
param managedEnvironmentId string
param registryServer string
param containerImage string
param cosmosEndpoint string
param foundryEndpoint string
param foundryModelDeployment string
@minLength(36)
@maxLength(36)
param entraClientId string
@secure()
@minLength(1)
param entraClientSecretUri string
@minValue(1)
param guestRequestsPerMinute int = 60
@minValue(1)
param providerRefreshesPerHour int = 40
@minValue(1048576)
@maxValue(268435456)
param providerMaxResponseBytes int = 67108864
@minValue(1)
param calculationConcurrency int = 10
@minValue(1)
@maxValue(8)
param assistantConcurrency int = 2
@minValue(1)
@maxValue(60)
param assistantRequestsPerMinute int = 10
param configureAuthentication bool = true
@minValue(0)
@maxValue(3)
param minimumReplicas int = 0

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
      secrets: configureAuthentication ? [
        {
          identity: 'system'
          keyVaultUrl: entraClientSecretUri
          name: 'entra-client-secret'
        }
      ] : []
    }
    environmentId: managedEnvironmentId
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
              name: 'ASSISTANT_ENABLED'
              value: 'true'
            }
            {
              name: 'FOUNDRY_ENDPOINT'
              value: foundryEndpoint
            }
            {
              name: 'FOUNDRY_MODEL_DEPLOYMENT'
              value: foundryModelDeployment
            }
            {
              name: 'FOUNDRY_API_VERSION'
              value: '2024-10-21'
            }
            {
              name: 'ASSISTANT_CONCURRENCY'
              value: string(assistantConcurrency)
            }
            {
              name: 'ASSISTANT_REQUESTS_PER_MINUTE'
              value: string(assistantRequestsPerMinute)
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
              failureThreshold: 30
              httpGet: {
                path: '/healthz'
                port: 8080
                scheme: 'HTTP'
              }
              periodSeconds: 2
              timeoutSeconds: 2
              type: 'Startup'
            }
            {
              failureThreshold: 3
              httpGet: {
                path: '/healthz'
                port: 8080
                scheme: 'HTTP'
              }
              periodSeconds: 20
              timeoutSeconds: 5
              type: 'Liveness'
            }
            {
              failureThreshold: 3
              httpGet: {
                path: '/readyz'
                port: 8080
                scheme: 'HTTP'
              }
              periodSeconds: 10
              successThreshold: 1
              timeoutSeconds: 5
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
        minReplicas: minimumReplicas
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

resource auth 'Microsoft.App/containerApps/authConfigs@2024-03-01' = if (configureAuthentication) {
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