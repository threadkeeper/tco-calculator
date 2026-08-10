param(
    [string]$SqlMiPath = (Join-Path $PSScriptRoot 'SQLMI.csv'),
    [string]$Ec2Path = (Join-Path $PSScriptRoot 'EC2.csv'),
    [string]$OutputPath = (Join-Path $PSScriptRoot 'EC2_SQLMI_MAPPING.csv'),
    [string[]]$AzureRegions = @(),
    [int]$MaxShapes = 0
)

$ErrorActionPreference = 'Stop'
$invariantCulture = [System.Globalization.CultureInfo]::InvariantCulture
$generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
$mappingRuleVersion = '1.0'
$sqlMiSizingSourceUrl = 'https://learn.microsoft.com/en-us/azure/azure-sql/managed-instance/resource-limits?view=azuresql'
$expectedOptionKeys = @(
    'payg',
    'ahb',
    'one-year',
    'ahbone-year',
    'three-year',
    'ahbthree-year',
    'sv-one-year',
    'ahbsv-one-year'
)

function Get-DecimalValue {
    param($Value)

    if ([string]::IsNullOrWhiteSpace([string]$Value)) {
        return $null
    }
    return [decimal]::Parse([string]$Value, [System.Globalization.NumberStyles]::Any, $invariantCulture)
}

function Format-Decimal {
    param($Value)

    if ($null -eq $Value) {
        return ''
    }
    return ([decimal]$Value).ToString('0.0000000000', $invariantCulture)
}

function Format-Number {
    param($Value)

    if ($null -eq $Value) {
        return ''
    }
    return ([decimal]$Value).ToString('0.##', $invariantCulture)
}

function Get-SqlMiMemoryGiB {
    param(
        [string]$HardwareFamily,
        [decimal]$VCoreCount
    )

    switch ($HardwareFamily) {
        'Premium Series Memory Optimized' {
            return [math]::Min([decimal]870.4, $VCoreCount * [decimal]13.6)
        }
        'Premium Series' {
            return [math]::Min([decimal]560, $VCoreCount * [decimal]7)
        }
        default {
            return [math]::Min([decimal]408, $VCoreCount * [decimal]5.1)
        }
    }
}

function Get-AwsRegionCode {
    param([string]$ComparableAwsRegion)

    if ($ComparableAwsRegion -match '^(eu-[a-z]+-\d+)') {
        return $Matches[1]
    }
    throw "Cannot determine an AWS region code from '$ComparableAwsRegion'."
}

function Get-FamilyPenalty {
    param(
        [string]$ServiceTier,
        [string]$HardwareFamily,
        [string]$InstanceFamily
    )

    if ($HardwareFamily -eq 'Premium Series Memory Optimized') {
        switch ($InstanceFamily) {
            'Memory optimized' { return [decimal]0 }
            'Storage optimized' { return [decimal]0.75 }
            'General purpose' { return [decimal]2.5 }
            default { return [decimal]5 }
        }
    }

    if ($ServiceTier -eq 'Business Critical') {
        switch ($InstanceFamily) {
            'Memory optimized' { return [decimal]0 }
            'Storage optimized' { return [decimal]0.4 }
            'General purpose' { return [decimal]1.25 }
            default { return [decimal]4 }
        }
    }

    switch ($InstanceFamily) {
        'Memory optimized' { return [decimal]0 }
        'General purpose' { return [decimal]0.75 }
        'Storage optimized' { return [decimal]1 }
        default { return [decimal]4 }
    }
}

function Get-ProcessorPenalty {
    param([string]$PhysicalProcessor)

    if ($PhysicalProcessor -match 'Granite Rapids') {
        return [decimal]0
    }
    if ($PhysicalProcessor -match 'Emerald Rapids') {
        return [decimal]0.05
    }
    if ($PhysicalProcessor -match 'Sapphire Rapids|EPYC 9R|EPYC 9B|EPYC 9V|EPYC 9575') {
        return [decimal]0.15
    }
    if ($PhysicalProcessor -match 'Ice ?Lake|Icelake|EPYC 7R') {
        return [decimal]0.35
    }
    if ($PhysicalProcessor -match 'Cascade Lake') {
        return [decimal]0.75
    }
    if ($PhysicalProcessor -match 'Skylake|EPYC 7571') {
        return [decimal]1.1
    }
    if ($PhysicalProcessor -eq 'Variable') {
        return [decimal]1.25
    }
    return [decimal]1.5
}

function Get-FitConfidence {
    param(
        [decimal]$CpuHeadroomPercent,
        [decimal]$MemoryHeadroomPercent,
        [decimal]$ProcessorPenalty
    )

    if ($CpuHeadroomPercent -le 25 -and $MemoryHeadroomPercent -le 35 -and $ProcessorPenalty -le [decimal]0.35) {
        return 'High'
    }
    if ($CpuHeadroomPercent -le 50 -and $MemoryHeadroomPercent -le 80) {
        return 'Good'
    }
    return 'Conservative'
}

function Get-StorageRecommendation {
    param([string]$ServiceTier)

    if ($ServiceTier -eq 'Business Critical') {
        return 'Use EBS io2 Block Express for durable SQL data and logs; use instance-local NVMe only for tempdb when the selected type provides it.'
    }
    if ($ServiceTier -eq 'Next Generation General Purpose') {
        return 'Start with EBS io2 or tuned gp3 and provision IOPS/throughput from measured workload demand; EC2 storage is priced separately.'
    }
    return 'Start with EBS gp3 and move latency-sensitive data or logs to io2 when measured IOPS and latency require it; EC2 storage is priced separately.'
}

function Get-AvailabilityRecommendation {
    param(
        [bool]$IsZoneRedundant,
        [string]$ServiceTier
    )

    if ($IsZoneRedundant) {
        return 'The selected EC2 type represents one SQL node. Use SQL Server Always On across separate Availability Zones with quorum/witness and duplicate the node/storage cost to preserve zone fault isolation.'
    }
    if ($ServiceTier -eq 'Business Critical') {
        return 'Business Critical includes replica-based service availability. A single EC2 node does not; use an Always On availability group or failover cluster when equivalent recovery behavior is required.'
    }
    return 'SQL MI includes platform availability and automated failover. A single EC2 node does not; add a second node and SQL Server HA architecture when the service-level behavior must be retained.'
}

function Get-OptionHourlyPrice {
    param(
        [hashtable]$RowsByOption,
        [string]$OptionKey
    )

    if (-not $RowsByOption.ContainsKey($OptionKey)) {
        return $null
    }
    return Get-DecimalValue $RowsByOption[$OptionKey].EffectiveHourlyPrice
}

function Import-Ec2Candidates {
    param([string]$Path)

    Add-Type -AssemblyName Microsoft.VisualBasic
    $parser = New-Object Microsoft.VisualBasic.FileIO.TextFieldParser((Resolve-Path $Path).Path)
    $parser.TextFieldType = [Microsoft.VisualBasic.FileIO.FieldType]::Delimited
    $parser.SetDelimiters(',')
    $parser.HasFieldsEnclosedInQuotes = $true
    $header = $parser.ReadFields()
    $columnIndex = @{}
    for ($column = 0; $column -lt $header.Count; $column++) {
        $columnIndex[$header[$column]] = $column
    }

    $requiredColumns = @(
        'Ec2PriceId',
        'SourceCatalogVersion',
        'SourcePriceUrl',
        'AWSRegionCode',
        'AWSLocation',
        'Currency',
        'InstanceType',
        'InstanceFamily',
        'vCPU',
        'MemoryGiB',
        'Storage',
        'NetworkPerformance',
        'PhysicalProcessor',
        'ProcessorArchitecture',
        'CurrentGeneration',
        'OperatingSystem',
        'WindowsLicenseModel',
        'PreInstalledSoftware',
        'SqlServerEdition',
        'SqlServerLicenseOption',
        'TermType',
        'Tenancy',
        'EffectiveHourlyPrice',
        'EffectiveMonthlyPrice',
        'EffectiveAnnualPrice',
        'SKU',
        'OfferTermCode',
        'HourlyRateCode'
    )
    foreach ($requiredColumn in $requiredColumns) {
        if (-not $columnIndex.ContainsKey($requiredColumn)) {
            $parser.Close()
            throw "EC2.csv is missing required column '$requiredColumn'."
        }
    }

    $candidates = New-Object 'System.Collections.Generic.List[object]'
    $candidateKeys = New-Object 'System.Collections.Generic.HashSet[string]'
    try {
        while (-not $parser.EndOfData) {
            $fields = $parser.ReadFields()
            if ($fields.Count -ne $header.Count) {
                throw 'EC2.csv contains a row with an unexpected column count.'
            }
            if ($fields[$columnIndex.TermType] -ne 'OnDemand' -or
                $fields[$columnIndex.Tenancy] -ne 'Shared' -or
                $fields[$columnIndex.CurrentGeneration] -ne 'Yes' -or
                $fields[$columnIndex.OperatingSystem] -ne 'Windows' -or
                $fields[$columnIndex.WindowsLicenseModel] -ne 'No License required' -or
                $fields[$columnIndex.PreInstalledSoftware] -ne 'SQL Ent' -or
                $fields[$columnIndex.ProcessorArchitecture] -ne 'x86_64' -or
                $fields[$columnIndex.InstanceFamily] -notin @('General purpose', 'Memory optimized', 'Storage optimized', 'Compute optimized') -or
                $fields[$columnIndex.InstanceType] -match '\.metal|-flex\.|^t\d') {
                continue
            }

            $candidateKey = "$($fields[$columnIndex.AWSRegionCode])|$($fields[$columnIndex.InstanceType])"
            if (-not $candidateKeys.Add($candidateKey)) {
                throw "EC2.csv contains duplicate canonical candidate '$candidateKey'."
            }
            $candidates.Add([pscustomobject][ordered]@{
                Ec2PriceId = $fields[$columnIndex.Ec2PriceId]
                SourceCatalogVersion = $fields[$columnIndex.SourceCatalogVersion]
                SourcePriceUrl = $fields[$columnIndex.SourcePriceUrl]
                AWSRegionCode = $fields[$columnIndex.AWSRegionCode]
                AWSLocation = $fields[$columnIndex.AWSLocation]
                Currency = $fields[$columnIndex.Currency]
                InstanceType = $fields[$columnIndex.InstanceType]
                InstanceFamily = $fields[$columnIndex.InstanceFamily]
                vCPU = [int]$fields[$columnIndex.vCPU]
                MemoryGiB = Get-DecimalValue $fields[$columnIndex.MemoryGiB]
                Storage = $fields[$columnIndex.Storage]
                NetworkPerformance = $fields[$columnIndex.NetworkPerformance]
                PhysicalProcessor = $fields[$columnIndex.PhysicalProcessor]
                ProcessorArchitecture = $fields[$columnIndex.ProcessorArchitecture]
                CurrentGeneration = $fields[$columnIndex.CurrentGeneration]
                OperatingSystem = $fields[$columnIndex.OperatingSystem]
                WindowsLicenseModel = $fields[$columnIndex.WindowsLicenseModel]
                PreInstalledSoftware = $fields[$columnIndex.PreInstalledSoftware]
                SqlServerEdition = $fields[$columnIndex.SqlServerEdition]
                SqlServerLicenseOption = $fields[$columnIndex.SqlServerLicenseOption]
                TermType = $fields[$columnIndex.TermType]
                Tenancy = $fields[$columnIndex.Tenancy]
                EffectiveHourlyPrice = Get-DecimalValue $fields[$columnIndex.EffectiveHourlyPrice]
                EffectiveMonthlyPrice = Get-DecimalValue $fields[$columnIndex.EffectiveMonthlyPrice]
                EffectiveAnnualPrice = Get-DecimalValue $fields[$columnIndex.EffectiveAnnualPrice]
                SKU = $fields[$columnIndex.SKU]
                OfferTermCode = $fields[$columnIndex.OfferTermCode]
                HourlyRateCode = $fields[$columnIndex.HourlyRateCode]
            })
        }
    } finally {
        $parser.Close()
    }
    return $candidates.ToArray()
}

if (-not (Test-Path $SqlMiPath)) {
    throw "SQL MI source file not found: $SqlMiPath"
}
if (-not (Test-Path $Ec2Path)) {
    throw "EC2 source file not found: $Ec2Path"
}

Write-Host 'Loading configured SQL MI prices...'
$sqlMiRows = @(
    Import-Csv $SqlMiPath |
        Where-Object {
            $_.RecordKind -eq 'Configured SKU Total' -and
            $_.InstanceType -eq 'Single Instance' -and
            $_.ServiceTier -in @('General Purpose', 'Next Generation General Purpose', 'Business Critical') -and
            ($AzureRegions.Count -eq 0 -or $_.AzureRegion -in $AzureRegions)
        }
)
if ($sqlMiRows.Count -eq 0) {
    throw 'No configured SQL MI rows matched the requested scope.'
}
if ([string]::IsNullOrWhiteSpace($sqlMiRows[0].SqlMiPriceId)) {
    throw 'SQLMI.csv does not contain SqlMiPriceId.'
}

Write-Host 'Streaming canonical EC2 candidates...'
$ec2Candidates = @(Import-Ec2Candidates $Ec2Path)
if ($ec2Candidates.Count -eq 0) {
    throw 'No canonical EC2 candidates were found.'
}
$candidatesByRegion = @{}
foreach ($candidate in $ec2Candidates) {
    if (-not $candidatesByRegion.ContainsKey($candidate.AWSRegionCode)) {
        $candidatesByRegion[$candidate.AWSRegionCode] = New-Object 'System.Collections.Generic.List[object]'
    }
    $candidatesByRegion[$candidate.AWSRegionCode].Add($candidate)
}

$shapeGroups = @(
    $sqlMiRows |
        Group-Object {
            @(
                $_.AzureRegion,
                $_.ComparableAwsRegion,
                $_.ServiceTier,
                $_.HardwareFamily,
                $_.VCoreCount,
                $_.IsZoneRedundant,
                $_.ConfigurationKey
            ) -join '|'
        } |
        Sort-Object {
            $row = $_.Group[0]
            '{0}|{1}|{2}|{3:D4}|{4}|{5}' -f $row.AzureRegion, $row.ServiceTier, $row.HardwareFamily, [int]$row.VCoreCount, $row.IsZoneRedundant, $row.ConfigurationKey
        }
)
if ($MaxShapes -gt 0) {
    $shapeGroups = @($shapeGroups | Select-Object -First $MaxShapes)
}

$selectionCache = @{}
$mappingRows = New-Object 'System.Collections.Generic.List[object]'
for ($shapeIndex = 0; $shapeIndex -lt $shapeGroups.Count; $shapeIndex++) {
    $group = $shapeGroups[$shapeIndex]
    $sourceRows = @($group.Group | Sort-Object SqlMiPriceId)
    $source = $sourceRows[0]
    $awsRegionCode = Get-AwsRegionCode $source.ComparableAwsRegion
    $sourceVCore = [decimal]$source.VCoreCount
    $sourceMemoryGiB = Get-SqlMiMemoryGiB $source.HardwareFamily $sourceVCore
    $sourceMemoryPerVCore = $sourceMemoryGiB / $sourceVCore
    $selectionKey = "$awsRegionCode|$($source.ServiceTier)|$($source.HardwareFamily)|$sourceVCore"

    if ($selectionCache.ContainsKey($selectionKey)) {
        $selected = $selectionCache[$selectionKey]
    } else {
        $regionalCandidates = $candidatesByRegion[$awsRegionCode]
        if ($null -eq $regionalCandidates) {
            throw "No EC2 candidates are available in '$awsRegionCode'."
        }
        $evaluatedCandidates = New-Object 'System.Collections.Generic.List[object]'
        foreach ($candidate in $regionalCandidates) {
            if ([decimal]$candidate.vCPU -lt $sourceVCore -or $candidate.MemoryGiB -lt $sourceMemoryGiB) {
                continue
            }
            $cpuOverRatio = ([decimal]$candidate.vCPU - $sourceVCore) / $sourceVCore
            $memoryOverRatio = ($candidate.MemoryGiB - $sourceMemoryGiB) / $sourceMemoryGiB
            $targetMemoryPerVcpu = $candidate.MemoryGiB / [decimal]$candidate.vCPU
            $ratioPenalty = [decimal][math]::Abs([math]::Log([double]($targetMemoryPerVcpu / $sourceMemoryPerVCore)))
            $familyPenalty = Get-FamilyPenalty $source.ServiceTier $source.HardwareFamily $candidate.InstanceFamily
            $processorPenalty = Get-ProcessorPenalty $candidate.PhysicalProcessor
            $score = ($cpuOverRatio * 5) + ($memoryOverRatio * 2) + $ratioPenalty + $familyPenalty + $processorPenalty
            $evaluatedCandidates.Add([pscustomobject]@{
                Row = $candidate
                Score = [math]::Round($score, 6)
                CpuHeadroomPercent = [math]::Round($cpuOverRatio * 100, 4)
                MemoryHeadroomPercent = [math]::Round($memoryOverRatio * 100, 4)
                TargetMemoryPerVcpu = $targetMemoryPerVcpu
                ProcessorPenalty = $processorPenalty
            })
        }
        if ($evaluatedCandidates.Count -eq 0) {
            throw "No EC2 instance in '$awsRegionCode' satisfies $sourceVCore vCPU and $(Format-Number $sourceMemoryGiB) GiB for '$($source.ConfigurationKey)'."
        }
        $selected = $evaluatedCandidates |
            Sort-Object Score, @{ Expression = { $_.Row.EffectiveHourlyPrice } }, @{ Expression = { $_.Row.InstanceType } } |
            Select-Object -First 1
        $selectionCache[$selectionKey] = $selected
    }

    $target = $selected.Row
    if ([decimal]$target.vCPU -lt $sourceVCore -or $target.MemoryGiB -lt $sourceMemoryGiB) {
        throw "Selected EC2 target '$($target.InstanceType)' does not satisfy '$($source.ConfigurationKey)'."
    }

    $rowsByOption = @{}
    foreach ($sourceRow in $sourceRows) {
        if ($rowsByOption.ContainsKey($sourceRow.CalculatorOptionKey)) {
            throw "Duplicate SQL MI option '$($sourceRow.CalculatorOptionKey)' for shape '$($group.Name)'."
        }
        $rowsByOption[$sourceRow.CalculatorOptionKey] = $sourceRow
    }
    $presentOptionKeys = @($rowsByOption.Keys | Sort-Object)
    $missingOptionKeys = @($expectedOptionKeys | Where-Object { $_ -notin $presentOptionKeys })
    $sqlMiPaygHourlyPrice = Get-OptionHourlyPrice $rowsByOption 'payg'
    $paygHourlyDelta = if ($null -ne $sqlMiPaygHourlyPrice) {
        $target.EffectiveHourlyPrice - $sqlMiPaygHourlyPrice
    } else {
        $null
    }
    $paygHourlyRatio = if ($null -ne $sqlMiPaygHourlyPrice -and $sqlMiPaygHourlyPrice -ne 0) {
        $target.EffectiveHourlyPrice / $sqlMiPaygHourlyPrice
    } else {
        $null
    }
    $isZoneRedundant = [System.Convert]::ToBoolean($source.IsZoneRedundant)
    $fitConfidence = Get-FitConfidence $selected.CpuHeadroomPercent $selected.MemoryHeadroomPercent $selected.ProcessorPenalty
    $selectionRationale = "$($source.ServiceTier) $($source.HardwareFamily) with $(Format-Number $sourceVCore) vCores is estimated at $(Format-Number $sourceMemoryGiB) GiB. $($target.InstanceType) supplies $($target.vCPU) vCPU and $(Format-Number $target.MemoryGiB) GiB with no CPU or memory shortfall; the score then minimizes excess capacity, memory-profile mismatch, SQL-host family penalty, and processor-generation penalty."
    $caveats = 'SQL MI vCores and EC2 vCPUs are treated as logical CPU units, but processor throughput is not identical. Estimated MI memory uses published service ratios. Benchmark CPU, memory, storage latency/IOPS, tempdb, log throughput, and concurrency before migration. The EC2 price is one On-Demand shared Windows plus SQL Server Enterprise node and excludes EBS, backups, data transfer, HA replicas, and operations.'

    $mappingRows.Add([pscustomobject][ordered]@{
        MappingId = 'EC2MI-{0:D6}' -f ($shapeIndex + 1)
        GeneratedAtUtc = $generatedAtUtc
        MappingRuleVersion = $mappingRuleVersion
        MappingStatus = 'Mapped to one performance-satisfying EC2 instance'
        FitConfidence = $fitConfidence
        PerformanceFitScore = Format-Decimal $selected.Score
        MappingBasis = 'One current-generation shared x86 EC2 instance with Windows and license-included SQL Server Enterprise; target vCPU and memory must both meet or exceed the SQL MI shape.'
        SqlMiShapeKey = $group.Name
        SqlMiConfigurationKey = $source.ConfigurationKey
        SqlMiPriceIds = @($sourceRows.SqlMiPriceId) -join ';'
        SqlMiPriceRowCount = $sourceRows.Count
        SqlMiCalculatorOptionKeys = $presentOptionKeys -join ';'
        SqlMiMissingOptionKeys = $missingOptionKeys -join ';'
        AzureRegion = $source.AzureRegion
        AzureLocation = $source.AzureLocation
        ComparableAwsRegion = $source.ComparableAwsRegion
        SqlMiServiceTier = $source.ServiceTier
        SqlMiHardwareFamily = $source.HardwareFamily
        SqlMiVCoreCount = Format-Decimal $sourceVCore
        SqlMiEstimatedMemoryGiB = Format-Decimal $sourceMemoryGiB
        SqlMiMemoryPerVCoreGiB = Format-Decimal $sourceMemoryPerVCore
        SqlMiIsZoneRedundant = $isZoneRedundant
        SqlMiPaygHourlyPrice = Format-Decimal $sqlMiPaygHourlyPrice
        SqlMiAhbHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'ahb')
        SqlMiOneYearHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'one-year')
        SqlMiAhbOneYearHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'ahbone-year')
        SqlMiThreeYearHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'three-year')
        SqlMiAhbThreeYearHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'ahbthree-year')
        SqlMiOneYearSavingsHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'sv-one-year')
        SqlMiAhbOneYearSavingsHourlyPrice = Format-Decimal (Get-OptionHourlyPrice $rowsByOption 'ahbsv-one-year')
        SqlMiSizingSourceUrl = $sqlMiSizingSourceUrl
        Ec2PriceId = $target.Ec2PriceId
        Ec2SourceCatalogVersion = $target.SourceCatalogVersion
        Ec2SourcePriceUrl = $target.SourcePriceUrl
        AwsRegionCode = $target.AWSRegionCode
        AwsLocation = $target.AWSLocation
        Ec2InstanceType = $target.InstanceType
        Ec2InstanceFamily = $target.InstanceFamily
        Ec2Vcpu = $target.vCPU
        Ec2MemoryGiB = Format-Decimal $target.MemoryGiB
        Ec2MemoryPerVcpuGiB = Format-Decimal $selected.TargetMemoryPerVcpu
        Ec2CpuHeadroomPercent = Format-Decimal $selected.CpuHeadroomPercent
        Ec2MemoryHeadroomPercent = Format-Decimal $selected.MemoryHeadroomPercent
        Ec2PhysicalProcessor = $target.PhysicalProcessor
        Ec2Storage = $target.Storage
        Ec2NetworkPerformance = $target.NetworkPerformance
        Ec2CurrentGeneration = $target.CurrentGeneration
        Ec2OperatingSystem = $target.OperatingSystem
        Ec2PreInstalledSoftware = $target.PreInstalledSoftware
        Ec2SqlServerEdition = $target.SqlServerEdition
        Ec2SqlServerLicenseOption = $target.SqlServerLicenseOption
        Ec2CatalogLicenseModel = $target.WindowsLicenseModel
        Ec2TermType = $target.TermType
        Ec2Tenancy = $target.Tenancy
        Currency = $target.Currency
        Ec2EffectiveHourlyPrice = Format-Decimal $target.EffectiveHourlyPrice
        Ec2EffectiveMonthlyPrice = Format-Decimal $target.EffectiveMonthlyPrice
        Ec2EffectiveAnnualPrice = Format-Decimal $target.EffectiveAnnualPrice
        Ec2MinusSqlMiPaygHourlyPrice = Format-Decimal $paygHourlyDelta
        Ec2ToSqlMiPaygHourlyRatio = Format-Decimal $paygHourlyRatio
        Ec2Sku = $target.SKU
        Ec2OfferTermCode = $target.OfferTermCode
        Ec2HourlyRateCode = $target.HourlyRateCode
        SelectionRationale = $selectionRationale
        StorageRecommendation = Get-StorageRecommendation $source.ServiceTier
        AvailabilityRecommendation = Get-AvailabilityRecommendation $isZoneRedundant $source.ServiceTier
        Caveats = $caveats
    })
}

$mappingRows | Export-Csv -Path $OutputPath -NoTypeInformation -Encoding UTF8
Write-Host "Wrote $($mappingRows.Count) SQL MI shape mappings to $OutputPath"