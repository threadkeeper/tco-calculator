[CmdletBinding()]
param(
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\app\catalogs\sql-mi-capabilities.json')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$resourceLimitsUrl = 'https://learn.microsoft.com/en-us/azure/azure-sql/managed-instance/resource-limits?view=azuresql'
$regionAvailabilityUrl = 'https://learn.microsoft.com/en-us/azure/azure-sql/managed-instance/region-availability?view=azuresql'
$reviewedDate = '2026-07-31'
$vcores = @(4, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 56, 64, 80, 96, 128)
$memoryOptions = [ordered]@{
    '4' = @('28', '32', '40', '48')
    '6' = @('42', '48', '60', '72')
    '8' = @('56', '64', '80', '96')
    '10' = @('70', '80', '100', '120')
    '12' = @('84', '96', '120', '144')
    '16' = @('112', '128', '160', '192')
    '20' = @('140', '160', '200', '240')
    '24' = @('168', '192', '240', '288')
    '32' = @('224', '256', '320', '384')
    '40' = @('280', '320', '400', '480')
    '48' = @('336', '384', '480')
    '56' = @('392', '448')
    '64' = @('448')
    '80' = @('560')
    '96' = @('560')
    '128' = @('560')
}

function Get-NggpMaximumStorageGb {
    param([int]$Vcores)

    if ($Vcores -le 6) { return '2048' }
    if ($Vcores -le 12) { return '8192' }
    if ($Vcores -le 24) { return '16384' }
    return '32768'
}

function Get-BcMaximumStorageGb {
    param([int]$Vcores)

    if ($Vcores -le 6) { return '1024' }
    if ($Vcores -le 12) { return '2048' }
    if ($Vcores -le 20) { return '4096' }
    if ($Vcores -le 56) { return '5632' }
    return '16384'
}

$candidates = [System.Collections.Generic.List[object]]::new()
foreach ($vcore in $vcores) {
    $supportedMemory = @($memoryOptions[[string]$vcore])
    $candidates.Add([ordered]@{
        configuration_key = "managed-vcore-next-gen-general-purpose-premium-series-$vcore"
        azure_region = 'swedencentral'
        service_tier = 'next_generation_general_purpose'
        hardware_family = 'Premium Series'
        vcores = $vcore
        zone_redundant = $false
        included_memory_gb = $supportedMemory[0]
        supported_memory_gb = $supportedMemory
        storage_architecture = 'Remote LRS'
        maximum_storage_gb = Get-NggpMaximumStorageGb -Vcores $vcore
        source_url = $resourceLimitsUrl
        reviewed_date = $reviewedDate
    })

    $includedMemory = $supportedMemory[0]
    $candidates.Add([ordered]@{
        configuration_key = "managed-vcore-business-critical-premium-series-$vcore"
        azure_region = 'swedencentral'
        service_tier = 'business_critical'
        hardware_family = 'Premium Series'
        vcores = $vcore
        zone_redundant = $false
        included_memory_gb = $includedMemory
        supported_memory_gb = @($includedMemory)
        storage_architecture = 'BC local SSD'
        maximum_storage_gb = Get-BcMaximumStorageGb -Vcores $vcore
        source_url = $resourceLimitsUrl
        reviewed_date = $reviewedDate
    })
}

$anchor = $candidates | Where-Object {
    $_.configuration_key -eq 'managed-vcore-next-gen-general-purpose-premium-series-32'
}
if ($null -eq $anchor -or $anchor.included_memory_gb -ne '224' -or '256' -notin $anchor.supported_memory_gb) {
    throw 'The required Sweden Central 32-vCore, 224/256-GB parity anchor is missing.'
}
if ($candidates.Count -ne 32) {
    throw "Expected 32 reviewed capability records, found $($candidates.Count)."
}

$catalog = [ordered]@{
    schema_version = '1.0.0'
    status = 'reviewed'
    reviewed_date = $reviewedDate
    source_urls = @($resourceLimitsUrl, $regionAvailabilityUrl)
    assumptions = @(
        'The v1 Azure region selector exposes Sweden Central only.'
        'Sweden Central is listed for Premium-series hardware with 16-TB storage.'
        'Business Critical flexible memory is excluded because the reviewed source marks it preview.'
        'Storage values convert documented TiB-scale limits to GB using 1 TiB = 1024 GB.'
    )
    candidates = $candidates
}

$json = ($catalog | ConvertTo-Json -Depth 10).Replace("`r`n", "`n").Replace("`r", "`n")
$fullOutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$outputDirectory = [System.IO.Path]::GetDirectoryName($fullOutputPath)
[System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null
$utf8WithoutBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText($fullOutputPath, "$json`n", $utf8WithoutBom)

Write-Output "Wrote $($candidates.Count) reviewed SQL MI capabilities to $fullOutputPath"