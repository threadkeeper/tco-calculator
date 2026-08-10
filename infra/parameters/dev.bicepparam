using '../main.bicep'

param namePrefix = 'tcocalculator-dev'
param location = 'southafricanorth'
param containerImage = readEnvironmentVariable('CONTAINER_IMAGE')
param entraClientId = readEnvironmentVariable('ENTRA_CLIENT_ID')
param entraClientSecretUri = readEnvironmentVariable('ENTRA_CLIENT_SECRET_URI')
param tags = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
  monthlyBudgetUsd: '100'
}