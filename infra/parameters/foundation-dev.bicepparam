using '../foundation.bicep'

param namePrefix = readEnvironmentVariable('AZURE_NAME_PREFIX')
param location = readEnvironmentVariable('AZURE_LOCATION')
param tags = {
  application: 'tco-calculator'
  environment: 'dev'
  managedBy: 'bicep'
  monthlyBudgetUsd: readEnvironmentVariable('AZURE_MONTHLY_BUDGET_USD')
}