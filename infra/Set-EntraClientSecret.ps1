[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string]$EnvironmentFile = (Join-Path $PSScriptRoot '.env'),
    [string]$VaultName = 'tcocalculator-auth-kv',
    [string]$SecretName = 'tcologinssecret',
    [switch]$PublishGitHub
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Read-EnvironmentFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Environment file does not exist: $Path"
    }

    $values = @{}
    foreach ($line in Get-Content -LiteralPath $Path) {
        if ($line -match '^(?<name>[A-Z][A-Z0-9_]*)=(?<value>.*)$') {
            if ($values.ContainsKey($Matches.name)) {
                throw "Environment file contains duplicate field $($Matches.name)."
            }
            $values[$Matches.name] = $Matches.value
        }
    }
    return $values
}

function Get-RequiredValue {
    param(
        [Parameter(Mandatory = $true)][hashtable]$Values,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $value = [string]$Values[$Name]
    if ([string]::IsNullOrWhiteSpace($value)) {
        throw "Environment field $Name is required."
    }
    if ($value -match "[`r`n]") {
        throw "Environment field $Name must be a single-line value."
    }
    return $value
}

function Invoke-AzureJson {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    $output = @(& az @Arguments --only-show-errors --output json 2>&1)
    if ($LASTEXITCODE -ne 0) {
        throw "Azure CLI validation failed: $($output -join [Environment]::NewLine)"
    }
    return (($output -join [Environment]::NewLine) | ConvertFrom-Json)
}

function Assert-IgnoredEnvironmentFile {
    param([Parameter(Mandatory = $true)][string]$Path)

    $repositoryRoot = (Resolve-Path -LiteralPath (Split-Path -Parent $PSScriptRoot)).Path
    $resolvedPath = (Resolve-Path -LiteralPath $Path).Path
    $repositoryPrefix = $repositoryRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $resolvedPath.StartsWith($repositoryPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing to process a plaintext credential outside the repository.'
    }
    $relativePath = $resolvedPath.Substring($repositoryPrefix.Length)
    & git -C $repositoryRoot check-ignore --quiet -- $relativePath
    if ($LASTEXITCODE -ne 0) {
        throw 'Refusing to process a plaintext credential from a file that is not ignored by Git.'
    }
}

function Update-EnvironmentFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$SecretUri
    )

    $uriWritten = $false
    $updated = foreach ($line in Get-Content -LiteralPath $Path) {
        if ($line -match '^(APP_SECRET_VALUE|APP_SECRET_ID)=') {
            continue
        }
        if ($line -match '^ENTRA_CLIENT_SECRET_URI=') {
            if ($uriWritten) {
                throw 'Environment file contains duplicate ENTRA_CLIENT_SECRET_URI fields.'
            }
            $uriWritten = $true
            "ENTRA_CLIENT_SECRET_URI=$SecretUri"
        }
        else {
            $line
        }
    }
    if (-not $uriWritten) {
        $updated += "ENTRA_CLIENT_SECRET_URI=$SecretUri"
    }

    $temporaryPath = "$Path.$PID.tmp"
    try {
        [IO.File]::WriteAllLines(
            $temporaryPath,
            [string[]]$updated,
            [Text.UTF8Encoding]::new($false)
        )
        Move-Item -LiteralPath $temporaryPath -Destination $Path -Force
    }
    finally {
        if (Test-Path -LiteralPath $temporaryPath) {
            Remove-Item -LiteralPath $temporaryPath -Force
        }
    }
}

function Publish-GitHubConfiguration {
    param(
        [Parameter(Mandatory = $true)][string]$Repository,
        [Parameter(Mandatory = $true)][string]$Environment,
        [Parameter(Mandatory = $true)][string]$ClientId,
        [Parameter(Mandatory = $true)][string]$SecretUri
    )

    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw 'GitHub CLI is required when -PublishGitHub is specified.'
    }
    & gh variable set ENTRA_CLIENT_ID --repo $Repository --env $Environment --body $ClientId *> $null
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to publish ENTRA_CLIENT_ID to the GitHub environment.'
    }
    $SecretUri | & gh secret set ENTRA_CLIENT_SECRET_URI --repo $Repository --env $Environment *> $null
    if ($LASTEXITCODE -ne 0) {
        throw 'Failed to publish the versioned Key Vault URI reference to the GitHub environment.'
    }
}

if (-not (Get-Command az -ErrorAction SilentlyContinue)) {
    throw 'Azure CLI is required.'
}

Assert-IgnoredEnvironmentFile -Path $EnvironmentFile
$values = Read-EnvironmentFile -Path $EnvironmentFile
$subscriptionId = Get-RequiredValue -Values $values -Name 'AZURE_SUBSCRIPTION_ID'
$resourceGroup = Get-RequiredValue -Values $values -Name 'AZURE_RESOURCE_GROUP'
$clientId = Get-RequiredValue -Values $values -Name 'ENTRA_CLIENT_ID'
$credentialId = Get-RequiredValue -Values $values -Name 'APP_SECRET_ID'
$secretValue = Get-RequiredValue -Values $values -Name 'APP_SECRET_VALUE'

if ($subscriptionId -notmatch '^[0-9a-fA-F-]{36}$' -or
    $clientId -notmatch '^[0-9a-fA-F-]{36}$' -or
    $clientId -eq '00000000-0000-0000-0000-000000000000' -or
    $credentialId -notmatch '^[0-9a-fA-F-]{36}$') {
    throw 'Subscription, Entra client, or credential identifiers are invalid.'
}
if ($secretValue -match '(?i)placeholder|changeme|example') {
    throw 'APP_SECRET_VALUE must contain the newly generated Entra credential, not a placeholder.'
}
if ($VaultName -notmatch '^[a-zA-Z0-9-]{3,24}$' -or $SecretName -notmatch '^[a-zA-Z0-9-]{1,127}$') {
    throw 'VaultName or SecretName is invalid.'
}

$account = Invoke-AzureJson -Arguments @('account', 'show', '--query', '{subscriptionId:id,tenantId:tenantId}')
if ($account.subscriptionId -ne $subscriptionId) {
    throw 'Azure CLI is authenticated to a different subscription than infra/.env.'
}
$vault = Invoke-AzureJson -Arguments @(
    'keyvault', 'show',
    '--name', $VaultName,
    '--resource-group', $resourceGroup,
    '--query', '{id:id,vaultUri:properties.vaultUri,rbac:properties.enableRbacAuthorization,publicNetworkAccess:properties.publicNetworkAccess}'
)
if ($vault.rbac -ne $true -or $vault.publicNetworkAccess -ne 'Disabled') {
    throw 'The target vault must use RBAC and keep public network access disabled.'
}

$credentials = @(Invoke-AzureJson -Arguments @('ad', 'app', 'credential', 'list', '--id', $clientId))
$credential = $credentials | Where-Object { $_.keyId -eq $credentialId } | Select-Object -First 1
if ($null -eq $credential) {
    throw 'APP_SECRET_ID does not identify an active credential on ENTRA_CLIENT_ID.'
}
$expiresAt = [DateTimeOffset]::Parse([string]$credential.endDateTime)
if ($expiresAt -le [DateTimeOffset]::UtcNow) {
    throw 'The selected Entra credential is expired.'
}

$repository = Get-RequiredValue -Values $values -Name 'GITHUB_REPOSITORY'
$deploymentEnvironment = Get-RequiredValue -Values $values -Name 'AZURE_DEPLOYMENT_ENVIRONMENT'
if (-not $PSCmdlet.ShouldProcess(
    "$VaultName/$SecretName",
    'Write the Entra credential through Azure Resource Manager and remove its plaintext .env fields'
)) {
    return
}

$token = (& az account get-access-token --resource 'https://management.azure.com/' --query accessToken --output tsv --only-show-errors)
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($token)) {
    throw 'Azure Resource Manager access token acquisition failed.'
}
$requestUri = 'https://management.azure.com{0}/secrets/{1}?api-version=2024-11-01' -f @(
    [string]$vault.id,
    [Uri]::EscapeDataString($SecretName)
)
$payload = @{
    properties = @{
        value = $secretValue
        contentType = 'Entra client secret for Container Apps built-in authentication'
        attributes = @{
            enabled = $true
            exp = $expiresAt.ToUnixTimeSeconds()
        }
    }
    tags = @{
        application = 'tco-calculator'
        environment = $deploymentEnvironment
        managedBy = 'infra/Set-EntraClientSecret.ps1'
    }
}
$body = $payload | ConvertTo-Json -Depth 6 -Compress
$secretValue = $null
$payload = $null

try {
    $response = Invoke-RestMethod `
        -Method Put `
        -Uri $requestUri `
        -Headers @{ Authorization = "Bearer $token" } `
        -ContentType 'application/json' `
        -Body $body
}
catch {
    throw 'Azure Resource Manager rejected the Key Vault secret write. Plaintext fields were retained.'
}
finally {
    $token = $null
    $body = $null
}

$secretUri = [string]$response.properties.secretUriWithVersion
$expectedPrefix = ([string]$vault.vaultUri).TrimEnd('/') + "/secrets/$SecretName/"
$responseIncludesValue = @($response.properties.PSObject.Properties.Name) -contains 'value'
if (-not $secretUri.StartsWith($expectedPrefix, [StringComparison]::OrdinalIgnoreCase) -or
    $secretUri.Length -le $expectedPrefix.Length -or
    $responseIncludesValue) {
    throw 'Azure did not return the expected write-only, versioned Key Vault secret metadata. Plaintext fields were retained.'
}

Update-EnvironmentFile -Path $EnvironmentFile -SecretUri $secretUri
Write-Host 'Stored the Entra credential in Key Vault and removed APP_SECRET_VALUE and APP_SECRET_ID from the ignored environment file.'

if ($PublishGitHub) {
    Publish-GitHubConfiguration `
        -Repository $repository `
        -Environment $deploymentEnvironment `
        -ClientId $clientId `
        -SecretUri $secretUri
    Write-Host "Published ENTRA_CLIENT_ID and the versioned Key Vault URI reference to GitHub environment '$deploymentEnvironment'."
}