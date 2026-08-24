[CmdletBinding()]
param(
    [string[]]$Location = @('swedencentral'),
    [string]$OutputPath = (Join-Path $PSScriptRoot '..\app\catalogs\azure-vm-capabilities.json'),
    [string]$ReviewedDate = (Get-Date -Format 'yyyy-MM-dd')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Generates the reviewed Azure Virtual Machine capability catalog used by the ec2_vm workload.
#
# Capability values come from the read-only Azure Compute Resource SKUs API. Lineage, generation
# rank, and lifecycle come from the reviewed family policy below rather than from parsing a SKU
# string, as required by section 5.3 of docs/EC2-VM-TCO-SPEC.md.
#
# The catalog carries capabilities only. It never carries prices.

$generatorVersion = 'azure-vm-capabilities-v1'
$resourceSkusUrl = 'https://learn.microsoft.com/en-us/rest/api/compute/resource-skus/list'

# Reviewed family policy. A family is only eligible when it appears here, so a later generation
# cannot enter the catalog without a documented review.
$familyPolicy = @(
    [pscustomobject]@{
        Family          = 'standardBsv2Family'
        DisplayFamily   = 'Bsv2'
        Lineage         = 'burstable'
        Generation      = 'v2'
        GenerationRank  = 2
        Lifecycle       = 'current'
        HasLocalDisk    = $false
        DocumentationUrl = 'https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/bsv2-series'
    },
    [pscustomobject]@{
        Family          = 'StandardDsv7Family'
        DisplayFamily   = 'Dsv7'
        Lineage         = 'general_purpose'
        Generation      = 'v7'
        GenerationRank  = 7
        Lifecycle       = 'current'
        HasLocalDisk    = $false
        DocumentationUrl = 'https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/dsv7-series'
    },
    [pscustomobject]@{
        Family          = 'StandardDdsv7Family'
        DisplayFamily   = 'Ddsv7'
        Lineage         = 'general_purpose'
        Generation      = 'v7'
        GenerationRank  = 7
        Lifecycle       = 'current'
        HasLocalDisk    = $true
        DocumentationUrl = 'https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/general-purpose/ddsv7-series'
    },
    [pscustomobject]@{
        Family          = 'StandardEsv7Family'
        DisplayFamily   = 'Esv7'
        Lineage         = 'memory_optimized'
        Generation      = 'v7'
        GenerationRank  = 7
        Lifecycle       = 'current'
        HasLocalDisk    = $false
        DocumentationUrl = 'https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/memory-optimized/esv7-series'
    },
    [pscustomobject]@{
        Family          = 'StandardEdsv7Family'
        DisplayFamily   = 'Edsv7'
        Lineage         = 'memory_optimized'
        Generation      = 'v7'
        GenerationRank  = 7
        Lifecycle       = 'current'
        HasLocalDisk    = $true
        DocumentationUrl = 'https://learn.microsoft.com/en-us/azure/virtual-machines/sizes/memory-optimized/edsv7-series'
    }
)

function Get-Capability {
    param(
        [Parameter(Mandatory)] $Sku,
        [Parameter(Mandatory)] [string]$Name
    )

    $capability = $Sku.capabilities | Where-Object { $_.name -eq $Name } | Select-Object -First 1
    if ($null -eq $capability) { return $null }
    return $capability.value
}

$candidates = New-Object System.Collections.Generic.List[object]

foreach ($region in $Location) {
    Write-Verbose "Listing virtual machine SKUs for $region"
    $skus = az vm list-skus --location $region --resource-type virtualMachines --output json |
        ConvertFrom-Json

    foreach ($policy in $familyPolicy) {
        $familySkus = $skus | Where-Object { $_.family -eq $policy.Family }
        foreach ($sku in $familySkus) {
            # A SKU with any returned restriction is ineligible; the catalog never guesses around one.
            if ($sku.restrictions -and $sku.restrictions.Count -gt 0) { continue }

            $architecture = Get-Capability -Sku $sku -Name 'CpuArchitectureType'
            if ($architecture -ne 'x64') { continue }

            $vcpus = Get-Capability -Sku $sku -Name 'vCPUs'
            $memory = Get-Capability -Sku $sku -Name 'MemoryGB'
            $maxDataDisks = Get-Capability -Sku $sku -Name 'MaxDataDiskCount'
            $premiumIo = Get-Capability -Sku $sku -Name 'PremiumIO'
            $uncachedIops = Get-Capability -Sku $sku -Name 'UncachedDiskIOPS'
            $uncachedBytes = Get-Capability -Sku $sku -Name 'UncachedDiskBytesPerSecond'
            $tempDiskMb = Get-Capability -Sku $sku -Name 'MaxResourceVolumeMB'

            if (-not $vcpus -or -not $memory -or -not $maxDataDisks) { continue }
            if ($premiumIo -ne 'True') { continue }
            if (-not $uncachedIops -or -not $uncachedBytes) { continue }

            # The API reports throughput in bytes per second; the catalog stores MB/s to match the
            # unit used by Azure managed-disk documentation and by the disk catalog.
            $uncachedThroughputMbps = [math]::Floor([decimal]$uncachedBytes / 1000000)

            # MaxResourceVolumeMB is not evidence of local-disk absence. Only the reviewed family
            # policy decides whether a local temporary disk exists.
            $localDiskGb = $null
            if ($policy.HasLocalDisk -and [int]$tempDiskMb -gt 0) {
                $localDiskGb = [string][math]::Floor([decimal]$tempDiskMb / 1024)
            }

            $candidates.Add([ordered]@{
                arm_sku_name                 = $sku.name
                display_family               = $policy.DisplayFamily
                lineage                      = $policy.Lineage
                generation                   = $policy.Generation
                generation_rank              = $policy.GenerationRank
                lifecycle                    = $policy.Lifecycle
                azure_region                 = $region
                cpu_architecture             = $architecture
                windows_eligible             = $true
                vcpus                        = [int]$vcpus
                memory_gb                    = [string]$memory
                max_data_disk_count          = [int]$maxDataDisks
                premium_io                   = $true
                uncached_disk_iops           = [int]$uncachedIops
                uncached_disk_throughput_mbps = [int]$uncachedThroughputMbps
                local_temp_disk_gb           = $localDiskGb
                source_url                   = $resourceSkusUrl
                documentation_url            = $policy.DocumentationUrl
                reviewed_date                = $ReviewedDate
            })
        }
    }
}

$catalog = [ordered]@{
    schema_version    = '1.0.0'
    status            = 'reviewed'
    reviewed_date     = $ReviewedDate
    generator_version = $generatorVersion
    source_urls       = @($resourceSkusUrl) + ($familyPolicy.DocumentationUrl | Sort-Object -Unique)
    assumptions       = @(
        'Capability values are read from the Azure Compute Resource SKUs API for each listed region and are not inferred from the SKU name.',
        'Lineage, generation rank, and lifecycle come from the reviewed family policy in research/Generate-AzureVmCapabilityCatalog.ps1.',
        'A SKU that returns any restriction for the queried subscription is excluded rather than assumed available.',
        'Only x86-64 SKUs with Premium I/O support and reported uncached disk limits are eligible.',
        'MaxResourceVolumeMB is not treated as evidence that a family lacks a local temporary disk.',
        'The catalog carries capabilities only. Prices are resolved separately from the Azure Retail Prices API.'
    )
    candidates        = @($candidates | Sort-Object -Property @{ Expression = 'azure_region' }, @{ Expression = 'lineage' }, @{ Expression = 'vcpus' }, @{ Expression = 'arm_sku_name' })
}

$json = $catalog | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText((Resolve-Path -LiteralPath (Split-Path -Parent $OutputPath) | Join-Path -ChildPath (Split-Path -Leaf $OutputPath)), $json + "`n", (New-Object System.Text.UTF8Encoding $false))

Write-Host ("Wrote {0} candidate sizes to {1}" -f $candidates.Count, $OutputPath)
