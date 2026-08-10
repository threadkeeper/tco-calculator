[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$ResourceGroup,
    [string]$DeploymentEnvironment,
    [string]$DeploymentClientId,
    [string]$GitHubRepository,
    [string]$OutputPath = (Join-Path $PSScriptRoot '.env'),
    [switch]$PublishGitHub
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$knownKeys = @(
    'AZURE_TENANT_ID',
    'AZURE_SUBSCRIPTION_ID',
    'AZURE_CLIENT_ID',
    'AZURE_APP_OBJECT_ID',
    'AZURE_SERVICE_PRINCIPAL_OBJECT_ID',
    'AZURE_LOCATION',
    'AZURE_DEPLOYMENT_ENVIRONMENT',
    'AZURE_RESOURCE_GROUP',
    'AZURE_NAME_PREFIX',
    'AZURE_MONTHLY_BUDGET_USD',
    'AZURE_BUDGET_NAME',
    'GITHUB_REPOSITORY',
    'ENTRA_CLIENT_ID',
    'ENTRA_CLIENT_SECRET_URI',
    'CONTAINER_IMAGE',
    'AZURE_CONTAINER_APPS_ENVIRONMENT_ID',
    'AZURE_CONTAINER_APPS_ENVIRONMENT_NAME',
    'AZURE_CONTAINER_APP_ID',
    'AZURE_CONTAINER_APP_NAME',
    'AZURE_CONTAINER_APP_FQDN',
    'AZURE_CONTAINER_APP_PRINCIPAL_ID',
    'AZURE_CONTAINER_REGISTRY',
    'AZURE_CONTAINER_REGISTRY_ID',
    'AZURE_CONTAINER_REGISTRY_NAME',
    'AZURE_COSMOS_ACCOUNT_ID',
    'AZURE_COSMOS_ACCOUNT_NAME',
    'AZURE_COSMOS_PRIVATE_ENDPOINT_ID',
    'AZURE_COSMOS_PRIVATE_DNS_ZONE_ID',
    'AZURE_VIRTUAL_NETWORK_ID',
    'AZURE_CONTAINER_APPS_SUBNET_ID',
    'AZURE_PRIVATE_ENDPOINT_SUBNET_ID',
    'AZURE_LOG_ANALYTICS_WORKSPACE_ID',
    'COSMOSDB_ENDPOINT'
)

function Read-KnownEnvironmentFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )

    $values = @{}
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $values
    }

    foreach ($line in Get-Content -LiteralPath $Path) {
        $trimmedLine = $line.Trim()
        if (-not $trimmedLine -or $trimmedLine.StartsWith('#')) {
            continue
        }

        $separatorIndex = $trimmedLine.IndexOf('=')
        if ($separatorIndex -lt 1) {
            continue
        }

        $name = $trimmedLine.Substring(0, $separatorIndex).Trim()
        if ($knownKeys -notcontains $name) {
            continue
        }

        $values[$name] = $trimmedLine.Substring($separatorIndex + 1).Trim()
    }

    return $values
}

function Get-FirstValue {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [AllowNull()]
        [object[]]$Candidates
    )

    foreach ($candidate in $Candidates) {
        if ($null -ne $candidate -and -not [string]::IsNullOrWhiteSpace([string]$candidate)) {
            return ([string]$candidate).Trim()
        }
    }

    return $null
}

function Invoke-AzureJson {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $commandArguments = @($Arguments) + @('--only-show-errors', '--output', 'json')
    $output = @(& az @commandArguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Azure CLI command failed: $($output -join [Environment]::NewLine)"
    }

    $json = ($output -join [Environment]::NewLine).Trim()
    if (-not $json) {
        return $null
    }

    return $json | ConvertFrom-Json
}

function Invoke-AzureText {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $commandArguments = @($Arguments) + @('--only-show-errors', '--output', 'tsv')
    $output = @(& az @commandArguments 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Azure CLI command failed: $($output -join [Environment]::NewLine)"
    }

    return ($output -join [Environment]::NewLine).Trim()
}

function Get-GitHubVariable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [Parameter(Mandatory = $true)]
        [string]$Repository,
        [Parameter(Mandatory = $true)]
        [string]$Environment
    )

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        return $null
    }

    $output = @(& gh variable get $Name --repo $Repository --env $Environment 2>$null)
    if ($LASTEXITCODE -ne 0) {
        return $null
    }

    return ($output -join [Environment]::NewLine).Trim()
}

function Resolve-GitHubRepository {
    param(
        [string]$ExplicitRepository,
        [hashtable]$ExistingValues
    )

    $repository = Get-FirstValue @(
        $ExplicitRepository,
        $env:GITHUB_REPOSITORY,
        $ExistingValues['GITHUB_REPOSITORY']
    )
    if ($repository) {
        return $repository
    }

    $repositoryRoot = Split-Path -Parent $PSScriptRoot
    $remoteUrl = @(& git -C $repositoryRoot remote get-url origin 2>$null)
    if ($LASTEXITCODE -eq 0) {
        $remote = ($remoteUrl -join [Environment]::NewLine).Trim()
        if ($remote -match 'github\.com[/:](?<owner>[^/]+)/(?<name>[^/]+?)(?:\.git)?$') {
            return "$($Matches.owner)/$($Matches.name)"
        }
    }

    return $null
}

function Get-SingleTaggedResource {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ResourceGroupName,
        [Parameter(Mandatory = $true)]
        [string]$ResourceType,
        [Parameter(Mandatory = $true)]
        [string]$Environment,
        [switch]$AllowMissing
    )

    $resourceResult = Invoke-AzureJson @(
        'resource', 'list',
        '--resource-group', $ResourceGroupName,
        '--resource-type', $ResourceType,
        '--query', '[].{id:id,name:name,location:location,tags:tags}'
    )
    $resources = @($resourceResult)
    $matches = @(
        $resources | Where-Object {
            $null -ne $_ -and
            $null -ne $_.tags -and
            $_.tags.application -eq 'tco-calculator' -and
            $_.tags.environment -eq $Environment
        }
    )

    if ($AllowMissing -and $matches.Count -eq 0) {
        return $null
    }
    if ($matches.Count -ne 1) {
        throw "Expected exactly one $ResourceType resource tagged for environment '$Environment' in resource group '$ResourceGroupName'; found $($matches.Count)."
    }

    return $matches[0]
}

function Assert-SingleLineValue {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,
        [AllowEmptyString()]
        [string]$Value
    )

    if ($Value -match "[`r`n]") {
        throw "Value for $Name contains a line break and cannot be written to an environment file."
    }
}

if (-not (Get-Command az -ErrorAction SilentlyContinue)) {
    throw 'Azure CLI is required. Install it through the approved workstation software process and authenticate before running this script.'
}

$existingValues = Read-KnownEnvironmentFile -Path $OutputPath
$DeploymentEnvironment = Get-FirstValue @(
    $DeploymentEnvironment,
    $env:AZURE_DEPLOYMENT_ENVIRONMENT,
    $existingValues['AZURE_DEPLOYMENT_ENVIRONMENT'],
    'dev'
)
$ResourceGroup = Get-FirstValue @(
    $ResourceGroup,
    $env:AZURE_RESOURCE_GROUP,
    $existingValues['AZURE_RESOURCE_GROUP'],
    'rg-tco'
)
$GitHubRepository = Resolve-GitHubRepository -ExplicitRepository $GitHubRepository -ExistingValues $existingValues

$account = Invoke-AzureJson @(
    'account', 'show',
    '--query', '{tenantId:tenantId,subscriptionId:id,userName:user.name,userType:user.type}'
)
Invoke-AzureJson @('group', 'show', '--name', $ResourceGroup, '--query', '{name:name}') | Out-Null

$DeploymentClientId = Get-FirstValue @(
    $DeploymentClientId,
    $env:AZURE_CLIENT_ID,
    $existingValues['AZURE_CLIENT_ID']
)
if (-not $DeploymentClientId -and $GitHubRepository) {
    $DeploymentClientId = Get-GitHubVariable `
        -Name 'AZURE_CLIENT_ID' `
        -Repository $GitHubRepository `
        -Environment $DeploymentEnvironment
}
if (-not $DeploymentClientId -and $account.userType -eq 'servicePrincipal') {
    $DeploymentClientId = $account.userName
}
if (-not $DeploymentClientId) {
    throw 'The GitHub OIDC deployment client ID could not be discovered. Pass -DeploymentClientId or configure AZURE_CLIENT_ID in the target GitHub environment.'
}

$applicationObjectId = $existingValues['AZURE_APP_OBJECT_ID']
$servicePrincipalObjectId = $existingValues['AZURE_SERVICE_PRINCIPAL_OBJECT_ID']
try {
    $applicationObjectId = Invoke-AzureText @('ad', 'app', 'show', '--id', $DeploymentClientId, '--query', 'id')
    $servicePrincipalObjectId = Invoke-AzureText @('ad', 'sp', 'show', '--id', $DeploymentClientId, '--query', 'id')
}
catch {
    Write-Warning 'Microsoft Graph did not return deployment application object IDs; leaving unavailable values blank.'
}

$managedEnvironment = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.App/managedEnvironments' `
    -Environment $DeploymentEnvironment
$registry = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.ContainerRegistry/registries' `
    -Environment $DeploymentEnvironment
$registryDetails = Invoke-AzureJson @(
    'resource', 'show',
    '--ids', $registry.id,
    '--api-version', '2023-07-01',
    '--query', '{loginServer:properties.loginServer}'
)
$cosmos = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.DocumentDB/databaseAccounts' `
    -Environment $DeploymentEnvironment
$cosmosDetails = Invoke-AzureJson @(
    'resource', 'show',
    '--ids', $cosmos.id,
    '--api-version', '2024-05-15',
    '--query', '{endpoint:properties.documentEndpoint}'
)
$virtualNetwork = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.Network/virtualNetworks' `
    -Environment $DeploymentEnvironment
$containerAppsSubnet = Invoke-AzureJson @(
    'network', 'vnet', 'subnet', 'show',
    '--resource-group', $ResourceGroup,
    '--vnet-name', $virtualNetwork.name,
    '--name', 'container-apps-environment',
    '--query', '{id:id}'
)
$privateEndpointSubnet = Invoke-AzureJson @(
    'network', 'vnet', 'subnet', 'show',
    '--resource-group', $ResourceGroup,
    '--vnet-name', $virtualNetwork.name,
    '--name', 'private-endpoints',
    '--query', '{id:id}'
)
$cosmosPrivateEndpoint = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.Network/privateEndpoints' `
    -Environment $DeploymentEnvironment
$cosmosPrivateDnsZone = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.Network/privateDnsZones' `
    -Environment $DeploymentEnvironment
$logAnalyticsWorkspace = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.OperationalInsights/workspaces' `
    -Environment $DeploymentEnvironment

$entraClientId = Get-FirstValue @($env:ENTRA_CLIENT_ID, $existingValues['ENTRA_CLIENT_ID'], '')
$entraClientSecretUri = Get-FirstValue @($env:ENTRA_CLIENT_SECRET_URI, $existingValues['ENTRA_CLIENT_SECRET_URI'], '')
$containerImage = Get-FirstValue @($env:CONTAINER_IMAGE, $existingValues['CONTAINER_IMAGE'], '')
$containerAppId = ''
$containerAppName = ''
$containerAppFqdn = ''
$containerAppPrincipalId = ''
$containerApp = Get-SingleTaggedResource `
    -ResourceGroupName $ResourceGroup `
    -ResourceType 'Microsoft.App/containerApps' `
    -Environment $DeploymentEnvironment `
    -AllowMissing
if ($null -ne $containerApp) {
    $containerAppDetails = Invoke-AzureJson @(
        'resource', 'show',
        '--ids', $containerApp.id,
        '--api-version', '2024-03-01',
        '--query', '{principalId:identity.principalId,fqdn:properties.configuration.ingress.fqdn,image:properties.template.containers[0].image,secretUri:properties.configuration.secrets[?name==`entra-client-secret`]|[0].keyVaultUrl}'
    )
    $authConfig = Invoke-AzureJson @(
        'resource', 'show',
        '--ids', "$($containerApp.id)/authConfigs/current",
        '--api-version', '2024-03-01',
        '--query', '{clientId:properties.identityProviders.azureActiveDirectory.registration.clientId}'
    )

    $containerAppId = [string]$containerApp.id
    $containerAppName = [string]$containerApp.name
    $containerAppFqdn = [string]$containerAppDetails.fqdn
    $containerAppPrincipalId = [string]$containerAppDetails.principalId
    $containerImage = [string]$containerAppDetails.image
    $entraClientId = [string]$authConfig.clientId
    $entraClientSecretUri = [string]$containerAppDetails.secretUri

    if ($containerImage -notmatch '(@sha256:[0-9a-fA-F]{64}|:[0-9a-fA-F]{40,64})$') {
        throw 'The deployed Container App image is not pinned to a commit SHA tag or image digest.'
    }
    if ($entraClientSecretUri -notmatch '^https://[^/]+/secrets/[^/]+/[^/]+/?$') {
        throw 'The deployed Entra client-secret reference is not a versioned HTTPS Key Vault secret URI.'
    }
}

$namePrefix = $managedEnvironment.name -replace '-cae$', ''
$values = [ordered]@{
    AZURE_TENANT_ID                   = [string]$account.tenantId
    AZURE_SUBSCRIPTION_ID             = [string]$account.subscriptionId
    AZURE_CLIENT_ID                   = $DeploymentClientId
    AZURE_APP_OBJECT_ID               = $applicationObjectId
    AZURE_SERVICE_PRINCIPAL_OBJECT_ID = $servicePrincipalObjectId
    AZURE_LOCATION                    = [string]$managedEnvironment.location
    AZURE_DEPLOYMENT_ENVIRONMENT      = $DeploymentEnvironment
    AZURE_RESOURCE_GROUP              = $ResourceGroup
    AZURE_NAME_PREFIX                 = $namePrefix
    AZURE_MONTHLY_BUDGET_USD          = Get-FirstValue @($existingValues['AZURE_MONTHLY_BUDGET_USD'], '100')
    AZURE_BUDGET_NAME                 = Get-FirstValue @($existingValues['AZURE_BUDGET_NAME'], "$ResourceGroup-monthly")
    GITHUB_REPOSITORY                 = Get-FirstValue @($GitHubRepository, '')
    ENTRA_CLIENT_ID                   = $entraClientId
    ENTRA_CLIENT_SECRET_URI           = $entraClientSecretUri
    CONTAINER_IMAGE                   = $containerImage
    AZURE_CONTAINER_APPS_ENVIRONMENT_ID = [string]$managedEnvironment.id
    AZURE_CONTAINER_APPS_ENVIRONMENT_NAME = [string]$managedEnvironment.name
    AZURE_CONTAINER_APP_ID            = $containerAppId
    AZURE_CONTAINER_APP_NAME          = $containerAppName
    AZURE_CONTAINER_APP_FQDN          = $containerAppFqdn
    AZURE_CONTAINER_APP_PRINCIPAL_ID  = $containerAppPrincipalId
    AZURE_CONTAINER_REGISTRY          = [string]$registryDetails.loginServer
    AZURE_CONTAINER_REGISTRY_ID       = [string]$registry.id
    AZURE_CONTAINER_REGISTRY_NAME     = [string]$registry.name
    AZURE_COSMOS_ACCOUNT_ID           = [string]$cosmos.id
    AZURE_COSMOS_ACCOUNT_NAME         = [string]$cosmos.name
    AZURE_COSMOS_PRIVATE_ENDPOINT_ID  = [string]$cosmosPrivateEndpoint.id
    AZURE_COSMOS_PRIVATE_DNS_ZONE_ID  = [string]$cosmosPrivateDnsZone.id
    AZURE_VIRTUAL_NETWORK_ID          = [string]$virtualNetwork.id
    AZURE_CONTAINER_APPS_SUBNET_ID    = [string]$containerAppsSubnet.id
    AZURE_PRIVATE_ENDPOINT_SUBNET_ID  = [string]$privateEndpointSubnet.id
    AZURE_LOG_ANALYTICS_WORKSPACE_ID  = [string]$logAnalyticsWorkspace.id
    COSMOSDB_ENDPOINT                 = [string]$cosmosDetails.endpoint
}

foreach ($entry in $values.GetEnumerator()) {
    Assert-SingleLineValue -Name $entry.Key -Value ([string]$entry.Value)
}

$outputDirectory = Split-Path -Parent $OutputPath
if (-not $outputDirectory) {
    $outputDirectory = (Get-Location).Path
}
if (-not (Test-Path -LiteralPath $outputDirectory -PathType Container)) {
    throw "Output directory does not exist: $outputDirectory"
}

if ($PSCmdlet.ShouldProcess($OutputPath, 'Write deployment identifiers')) {
    $lines = @(
        '# Generated by infra/getkeys.ps1. Do not commit this file.'
        '# ENTRA_CLIENT_SECRET_URI is a secret reference; no secret value is retrieved.'
    )
    $lines += $values.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }
    $temporaryPath = "$OutputPath.$PID.tmp"
    [System.IO.File]::WriteAllLines(
        $temporaryPath,
        [string[]]$lines,
        [System.Text.UTF8Encoding]::new($false)
    )
    Move-Item -LiteralPath $temporaryPath -Destination $OutputPath -Force
    Write-Host "Wrote deployment identifiers to $OutputPath."
}

if ($PublishGitHub) {
    if (-not $GitHubRepository) {
        throw 'GitHub repository could not be discovered. Pass -GitHubRepository owner/repository.'
    }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw 'GitHub CLI is required when -PublishGitHub is specified.'
    }

    $variableNames = @(
        'AZURE_TENANT_ID',
        'AZURE_SUBSCRIPTION_ID',
        'AZURE_CLIENT_ID',
        'AZURE_APP_OBJECT_ID',
        'AZURE_SERVICE_PRINCIPAL_OBJECT_ID',
        'AZURE_LOCATION',
        'AZURE_DEPLOYMENT_ENVIRONMENT',
        'AZURE_RESOURCE_GROUP',
        'AZURE_NAME_PREFIX',
        'AZURE_MONTHLY_BUDGET_USD',
        'AZURE_BUDGET_NAME',
        'ENTRA_CLIENT_ID',
        'CONTAINER_IMAGE',
        'AZURE_CONTAINER_APPS_ENVIRONMENT_ID',
        'AZURE_CONTAINER_APPS_ENVIRONMENT_NAME',
        'AZURE_CONTAINER_APP_ID',
        'AZURE_CONTAINER_APP_NAME',
        'AZURE_CONTAINER_APP_FQDN',
        'AZURE_CONTAINER_APP_PRINCIPAL_ID',
        'AZURE_CONTAINER_REGISTRY',
        'AZURE_CONTAINER_REGISTRY_ID',
        'AZURE_CONTAINER_REGISTRY_NAME',
        'AZURE_COSMOS_ACCOUNT_ID',
        'AZURE_COSMOS_ACCOUNT_NAME',
        'AZURE_COSMOS_PRIVATE_ENDPOINT_ID',
        'AZURE_COSMOS_PRIVATE_DNS_ZONE_ID',
        'AZURE_VIRTUAL_NETWORK_ID',
        'AZURE_CONTAINER_APPS_SUBNET_ID',
        'AZURE_PRIVATE_ENDPOINT_SUBNET_ID',
        'AZURE_LOG_ANALYTICS_WORKSPACE_ID',
        'COSMOSDB_ENDPOINT'
    )

    foreach ($name in $variableNames) {
        if ([string]::IsNullOrWhiteSpace([string]$values[$name])) {
            continue
        }
        if ($PSCmdlet.ShouldProcess("$GitHubRepository environment $DeploymentEnvironment", "Set GitHub Actions variable $name")) {
            & gh variable set $name `
                --repo $GitHubRepository `
                --env $DeploymentEnvironment `
                --body ([string]$values[$name]) *> $null
            if ($LASTEXITCODE -ne 0) {
                throw "Failed to set GitHub Actions variable $name."
            }
        }
    }

    if (-not [string]::IsNullOrWhiteSpace([string]$values['ENTRA_CLIENT_SECRET_URI']) -and $PSCmdlet.ShouldProcess("$GitHubRepository environment $DeploymentEnvironment", 'Set GitHub Actions secret reference ENTRA_CLIENT_SECRET_URI')) {
        [string]$values['ENTRA_CLIENT_SECRET_URI'] |
            & gh secret set ENTRA_CLIENT_SECRET_URI `
                --repo $GitHubRepository `
                --env $DeploymentEnvironment *> $null
        if ($LASTEXITCODE -ne 0) {
            throw 'Failed to set GitHub Actions secret reference ENTRA_CLIENT_SECRET_URI.'
        }
    }

    Write-Host "Published GitHub Actions configuration to environment '$DeploymentEnvironment' without retrieving a secret value."
}