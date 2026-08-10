param(
    [string]$RdsPath = (Join-Path $PSScriptRoot 'RDS.csv'),
    [string]$SqlMiPath = (Join-Path $PSScriptRoot 'SQLMI.csv'),
    [string]$OutputPath = (Join-Path $PSScriptRoot 'RSD_SQLMI_MAPPING.csv')
)

$ErrorActionPreference = 'Stop'
$invariantCulture = [System.Globalization.CultureInfo]::InvariantCulture
$generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
$mappingRuleVersion = '1.0'
$sizingSourceUrl = 'https://learn.microsoft.com/en-us/azure/azure-sql/managed-instance/resource-limits?view=azuresql'

function Get-DecimalValue {
    param($Value)

    if ([string]::IsNullOrWhiteSpace([string]$Value)) {
        return $null
    }
    return [decimal]::Parse([string]$Value, [System.Globalization.NumberStyles]::Any, $invariantCulture)
}

function Get-MemoryGiB {
    param([string]$Memory)

    if ($Memory -match '([0-9]+(?:\.[0-9]+)?)\s*GiB') {
        return [decimal]::Parse($Matches[1], $invariantCulture)
    }
    return $null
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

function Get-DesiredServiceTier {
    param([string]$DatabaseEdition)

    if ($DatabaseEdition -match '^Enterprise' -or $DatabaseEdition -eq 'Developer') {
        return 'Business Critical'
    }
    return 'General Purpose'
}

function Test-AzureHybridBenefitFit {
    param($RdsRow)

    return $RdsRow.licenseModel -match 'Bring your own|^NA$' -or
        $RdsRow.databaseEdition -match 'BYOM|Developer' -or
        $RdsRow.deploymentModel -eq 'Custom'
}

function Get-CalculatorOptionKey {
    param($RdsRow)

    $usesAhb = Test-AzureHybridBenefitFit $RdsRow
    if ($RdsRow.TermType -eq 'OnDemand') {
        if ($usesAhb) { return 'ahb' }
        return 'payg'
    }
    if ($RdsRow.LeaseContractLength -eq '1yr') {
        if ($usesAhb) { return 'ahbone-year' }
        return 'one-year'
    }
    if ($RdsRow.LeaseContractLength -eq '3yr') {
        if ($usesAhb) { return 'ahbthree-year' }
        return 'three-year'
    }
    if ($usesAhb) { return 'ahb' }
    return 'payg'
}

function Get-HardwarePenalty {
    param(
        $RdsRow,
        [string]$HardwareFamily,
        [decimal]$SourceVcpu,
        [decimal]$SourceMemoryGiB
    )

    if ($HardwareFamily -eq 'Gen4') {
        return [decimal]4
    }

    $memoryPerVcpu = $SourceMemoryGiB / $SourceVcpu
    if ($memoryPerVcpu -ge 10) {
        switch ($HardwareFamily) {
            'Premium Series Memory Optimized' { return [decimal]0 }
            'Premium Series' { return [decimal]2 }
            default { return [decimal]4 }
        }
    }
    if ($memoryPerVcpu -ge 6) {
        switch ($HardwareFamily) {
            'Premium Series' { return [decimal]0 }
            'Premium Series Memory Optimized' { return [decimal]0.25 }
            default { return [decimal]1.5 }
        }
    }

    $isModernProcessor = $RdsRow.instanceTypeFamily -match 'M6i|M7i|R6i|R7i|R8i|R8a|Z1D'
    if ($isModernProcessor) {
        switch ($HardwareFamily) {
            'Premium Series' { return [decimal]0 }
            'Premium Series Memory Optimized' { return [decimal]1.25 }
            default { return [decimal]0.75 }
        }
    }

    switch ($HardwareFamily) {
        'Gen5' { return [decimal]0 }
        'Premium Series' { return [decimal]0.35 }
        'Premium Series Memory Optimized' { return [decimal]1.5 }
        default { return [decimal]2 }
    }
}

function Get-ComputeGroupKey {
    param($RdsRow)

    return @(
        $RdsRow.Region,
        $RdsRow.SKU,
        $RdsRow.TermType,
        $RdsRow.OfferTermCode,
        $RdsRow.EffectiveDate,
        $RdsRow.LeaseContractLength,
        $RdsRow.PurchaseOption,
        $RdsRow.OfferingClass
    ) -join '|'
}

function Get-ConfiguredTarget {
    param(
        $RdsRow,
        [string]$AzureRegion
    )

    $sourceVcpu = Get-DecimalValue $RdsRow.vcpu
    $sourceMemoryGiB = Get-MemoryGiB $RdsRow.memory
    $desiredTier = Get-DesiredServiceTier $RdsRow.databaseEdition
    $optionKey = Get-CalculatorOptionKey $RdsRow
    $desiredZoneRedundancy = $RdsRow.deploymentOption -match '^Multi-AZ'
    $cacheKey = @(
        $AzureRegion,
        $desiredTier,
        $optionKey,
        $desiredZoneRedundancy,
        $sourceVcpu,
        $sourceMemoryGiB,
        $RdsRow.instanceTypeFamily
    ) -join '|'
    if ($script:configuredTargetCache.ContainsKey($cacheKey)) {
        return $script:configuredTargetCache[$cacheKey]
    }

    $candidateTiers = if ($desiredTier -eq 'General Purpose') {
        @('Next Generation General Purpose', 'General Purpose')
    } else {
        @($desiredTier)
    }
    $candidateBuffer = New-Object 'System.Collections.Generic.List[object]'
    foreach ($candidateTier in $candidateTiers) {
        $exactKey = "$AzureRegion|$candidateTier|$optionKey|$desiredZoneRedundancy"
        $candidateList = $configuredIndex[$exactKey]
        if ($null -ne $candidateList) {
            foreach ($candidate in $candidateList) {
                $candidateBuffer.Add($candidate)
            }
        }
    }
    $candidates = @($candidateBuffer.ToArray())
    $zoneFallback = $false

    if ($candidates.Count -eq 0) {
        foreach ($candidateTier in $candidateTiers) {
            $fallbackKey = "$AzureRegion|$candidateTier|$optionKey|$(-not $desiredZoneRedundancy)"
            $candidateList = $configuredIndex[$fallbackKey]
            if ($null -ne $candidateList) {
                foreach ($candidate in $candidateList) {
                    $candidateBuffer.Add($candidate)
                }
            }
        }
        $candidates = @($candidateBuffer.ToArray())
        $zoneFallback = $true
    }
    if ($candidates.Count -eq 0) {
        return $null
    }

    $best = $null
    foreach ($candidate in $candidates) {
        $targetVcore = Get-DecimalValue $candidate.VCoreCount
        $targetMemoryGiB = Get-SqlMiMemoryGiB $candidate.HardwareFamily $targetVcore
        $cpuUnder = [math]::Max([decimal]0, ($sourceVcpu - $targetVcore) / $sourceVcpu)
        $cpuOver = [math]::Max([decimal]0, ($targetVcore - $sourceVcpu) / $sourceVcpu)
        $memoryUnder = [math]::Max([decimal]0, ($sourceMemoryGiB - $targetMemoryGiB) / $sourceMemoryGiB)
        $memoryOver = [math]::Max([decimal]0, ($targetMemoryGiB - $sourceMemoryGiB) / $sourceMemoryGiB)
        $hardwarePenalty = Get-HardwarePenalty $RdsRow $candidate.HardwareFamily $sourceVcpu $sourceMemoryGiB
        $tierPenalty = if ($desiredTier -eq 'General Purpose' -and $candidate.ServiceTier -eq 'General Purpose') {
            [decimal]0.1
        } else {
            [decimal]0
        }
        $score = ($cpuUnder * 15) + ($cpuOver * 2) + ($memoryUnder * 10) + $memoryOver + $hardwarePenalty + $tierPenalty
        $evaluated = [pscustomobject]@{
            Row = $candidate
            Score = [math]::Round($score, 6)
            TargetMemoryGiB = $targetMemoryGiB
            ZoneFallback = $zoneFallback
        }

        if ($null -eq $best -or
            $evaluated.Score -lt $best.Score -or
            ($evaluated.Score -eq $best.Score -and $candidate.SqlMiPriceId -lt $best.Row.SqlMiPriceId)) {
            $best = $evaluated
        }
    }
    $script:configuredTargetCache[$cacheKey] = $best
    return $best
}

function Get-RetailTarget {
    param(
        [string]$AzureRegion,
        [string]$PricingComponent,
        [string]$ServiceTier,
        [bool]$IsZoneRedundant,
        [string]$SkuName,
        [string]$MeterName,
        [string]$UnitOfMeasure
    )

    $candidates = @(
        $retailByRegion[$AzureRegion] |
            Where-Object {
                $_.PricingComponent -eq $PricingComponent -and
                ([string]::IsNullOrWhiteSpace($ServiceTier) -or $_.ServiceTier -eq $ServiceTier) -and
                ([string]::IsNullOrWhiteSpace($SkuName) -or $_.SkuName -eq $SkuName) -and
                ([string]::IsNullOrWhiteSpace($MeterName) -or $_.MeterName -eq $MeterName) -and
                ([string]::IsNullOrWhiteSpace($UnitOfMeasure) -or $_.UnitOfMeasure -eq $UnitOfMeasure) -and
                $_.PriceType -eq 'Consumption'
            }
    )
    if ($PricingComponent -in @('Data Storage', 'Additional IOPS')) {
        $zonalCandidates = @(
            $candidates |
                Where-Object { [System.Convert]::ToBoolean($_.IsZoneRedundant) -eq $IsZoneRedundant }
        )
        if ($zonalCandidates.Count -gt 0) {
            $candidates = $zonalCandidates
        }
    }
    return $candidates | Sort-Object SqlMiPriceId | Select-Object -First 1
}

function Format-Number {
    param($Value)

    if ($null -eq $Value) {
        return ''
    }
    return ([decimal]$Value).ToString('0.##', $invariantCulture)
}

$rdsRows = @(Import-Csv $RdsPath)
$sqlMiRows = @(Import-Csv $SqlMiPath)

if ($rdsRows.Count -eq 0 -or [string]::IsNullOrWhiteSpace($rdsRows[0].RdsPriceId)) {
    throw 'RDS.csv does not contain RdsPriceId. Regenerate it with Generate-RdsCsv.ps1.'
}
if ($sqlMiRows.Count -eq 0 -or [string]::IsNullOrWhiteSpace($sqlMiRows[0].SqlMiPriceId)) {
    throw 'SQLMI.csv does not contain SqlMiPriceId. Regenerate it with Generate-SqlMiCsv.ps1.'
}

$regionMap = @{
    'eu-central-1' = [pscustomobject]@{ AzureRegion = 'germanywestcentral'; Reason = 'Germany West Central is the closest in-country Azure region to AWS Frankfurt.' }
    'eu-central-2' = [pscustomobject]@{ AzureRegion = 'switzerlandnorth'; Reason = 'Switzerland North is the closest in-country Azure region to AWS Zurich.' }
    'eu-north-1' = [pscustomobject]@{ AzureRegion = 'swedencentral'; Reason = 'Sweden Central is the closest in-country Azure region to AWS Stockholm.' }
    'eu-south-1' = [pscustomobject]@{ AzureRegion = 'italynorth'; Reason = 'Italy North is the closest in-country Azure region to AWS Milan.' }
    'eu-south-2' = [pscustomobject]@{ AzureRegion = 'spaincentral'; Reason = 'Spain Central is the closest in-country Azure region to AWS Spain.' }
    'eu-west-1' = [pscustomobject]@{ AzureRegion = 'northeurope'; Reason = 'North Europe is Azure''s Ireland region and is the direct geographic match for AWS Ireland.' }
    'eu-west-2' = [pscustomobject]@{ AzureRegion = 'uksouth'; Reason = 'UK South is the closest in-country Azure region to AWS London.' }
    'eu-west-3' = [pscustomobject]@{ AzureRegion = 'francecentral'; Reason = 'France Central is the closest in-country Azure region to AWS Paris.' }
}

$configuredIndex = @{}
$retailByRegion = @{}
$sqlMiById = @{}
$script:configuredTargetCache = @{}
foreach ($sqlMiRow in $sqlMiRows) {
    $sqlMiById[$sqlMiRow.SqlMiPriceId] = $sqlMiRow
    if ($sqlMiRow.RecordKind -eq 'Configured SKU Total' -and
        $sqlMiRow.InstanceType -eq 'Single Instance' -and
        $sqlMiRow.ServiceTier -in @('General Purpose', 'Next Generation General Purpose', 'Business Critical')) {
        $key = "$($sqlMiRow.AzureRegion)|$($sqlMiRow.ServiceTier)|$($sqlMiRow.CalculatorOptionKey)|$($sqlMiRow.IsZoneRedundant)"
        if (-not $configuredIndex.ContainsKey($key)) {
            $configuredIndex[$key] = New-Object 'System.Collections.Generic.List[object]'
        }
        $configuredIndex[$key].Add($sqlMiRow)
    }
    if ($sqlMiRow.RecordKind -eq 'Retail Price Dimension') {
        if (-not $retailByRegion.ContainsKey($sqlMiRow.AzureRegion)) {
            $retailByRegion[$sqlMiRow.AzureRegion] = New-Object 'System.Collections.Generic.List[object]'
        }
        $retailByRegion[$sqlMiRow.AzureRegion].Add($sqlMiRow)
    }
}

$computePriceGroups = @{}
$databaseInstanceGroups = @(
    $rdsRows |
        Where-Object { $_.ProductFamily -eq 'Database Instance' } |
        Group-Object { Get-ComputeGroupKey $_ }
)
foreach ($group in $databaseInstanceGroups) {
    $sample = $group.Group[0]
    $termHours = switch ($sample.LeaseContractLength) {
        '1yr' { [decimal]8760 }
        '3yr' { [decimal]26280 }
        default { [decimal]1 }
    }
    $recurringHourly = [decimal]0
    $upfront = [decimal]0
    foreach ($dimension in $group.Group) {
        $price = Get-DecimalValue $dimension.PricePerUnit
        if ($dimension.Unit -eq 'Hrs') {
            $recurringHourly += $price
        } elseif ($dimension.Unit -eq 'Quantity') {
            $upfront += $price
        }
    }
    $effectiveHourly = if ($sample.TermType -eq 'Reserved') {
        $recurringHourly + ($upfront / $termHours)
    } else {
        $recurringHourly
    }
    $computePriceGroups[$group.Name] = [pscustomobject]@{
        GroupKey = $group.Name
        RecurringHourlyPrice = $recurringHourly
        UpfrontPrice = $upfront
        EffectiveHourlyPrice = $effectiveHourly
        EffectiveMonthlyPrice = $effectiveHourly * 730
    }
}

$mappingRows = New-Object 'System.Collections.Generic.List[object]'
for ($index = 0; $index -lt $rdsRows.Count; $index++) {
    $rdsRow = $rdsRows[$index]
    $region = $regionMap[$rdsRow.Region]
    if ($null -eq $region) {
        throw "No Azure region mapping is defined for $($rdsRow.Region)."
    }

    $target = $null
    $targetMemoryGiB = $null
    $mappingStatus = ''
    $mappingCategory = $rdsRow.ProductFamily
    $fitConfidence = ''
    $fitScore = $null
    $reasonParts = New-Object 'System.Collections.Generic.List[string]'
    $caveatParts = New-Object 'System.Collections.Generic.List[string]'
    $comparisonBasis = ''
    $sqlMiUnitsPerRdsUnit = $null
    $sqlMiComparedPrice = $null
    $rdsEffectiveHourlyPrice = $null
    $rdsEffectiveMonthlyPrice = $null
    $rdsPricingGroupKey = ''
    $sourceVcpu = Get-DecimalValue $rdsRow.vcpu
    $sourceMemoryGiB = Get-MemoryGiB $rdsRow.memory

    $reasonParts.Add($region.Reason)

    switch ($rdsRow.ProductFamily) {
        'Database Instance' {
            $evaluatedTarget = Get-ConfiguredTarget $rdsRow $region.AzureRegion
            if ($null -eq $evaluatedTarget) {
                throw "No configured SQL MI target found for $($rdsRow.RdsPriceId)."
            }
            $target = $evaluatedTarget.Row
            $targetMemoryGiB = $evaluatedTarget.TargetMemoryGiB
            $fitScore = $evaluatedTarget.Score
            $mappingStatus = 'Mapped to configured SQL MI SKU'
            $comparisonBasis = 'Effective hourly compute plus SQL license; AWS reservation upfront and recurring dimensions are amortized over the term.'
            $rdsPricingGroupKey = Get-ComputeGroupKey $rdsRow
            $rdsGroupPrice = $computePriceGroups[$rdsPricingGroupKey]
            $rdsEffectiveHourlyPrice = $rdsGroupPrice.EffectiveHourlyPrice
            $rdsEffectiveMonthlyPrice = $rdsGroupPrice.EffectiveMonthlyPrice
            $sqlMiComparedPrice = Get-DecimalValue $target.EffectiveHourlyPrice
            $sqlMiUnitsPerRdsUnit = [decimal]1

            if ($target.ServiceTier -eq 'Business Critical') {
                $reasonParts.Add("$($rdsRow.databaseEdition) maps to Business Critical to preserve Enterprise/Developer feature headroom, local-SSD latency, and replica-based recovery behavior.")
            } elseif ($target.ServiceTier -eq 'Next Generation General Purpose') {
                $reasonParts.Add("$($rdsRow.databaseEdition) maps to Next-gen General Purpose as the closest balanced managed tier, using its broader vCore range and improved I/O while avoiding Business Critical replica overhead.")
                $caveatParts.Add('Next-gen General Purpose bills only memory or IOPS provisioned above its included allowances; those optional quantities are not part of this compute-and-license row.')
            } else {
                $reasonParts.Add("$($rdsRow.databaseEdition) maps to General Purpose as the closest balanced managed tier without paying for Business Critical replicas and local SSD.")
            }

            $targetVcore = Get-DecimalValue $target.VCoreCount
            $cpuDeltaPercent = (($targetVcore - $sourceVcpu) / $sourceVcpu) * 100
            $memoryDeltaPercent = (($targetMemoryGiB - $sourceMemoryGiB) / $sourceMemoryGiB) * 100
            $reasonParts.Add("$($rdsRow.instanceType) provides $(Format-Number $sourceVcpu) vCPU and $(Format-Number $sourceMemoryGiB) GiB; $($target.HardwareFamily) with $(Format-Number $targetVcore) vCores provides about $(Format-Number $targetMemoryGiB) GiB and minimizes weighted CPU/RAM shortfall before overprovisioning.")

            if ($targetVcore -ge $sourceVcpu -and $targetMemoryGiB -ge $sourceMemoryGiB -and
                $cpuDeltaPercent -le 25 -and $memoryDeltaPercent -le 25) {
                $fitConfidence = 'High'
            } elseif ($targetVcore -ge ($sourceVcpu * [decimal]0.85) -and
                $targetMemoryGiB -ge ($sourceMemoryGiB * [decimal]0.85)) {
                $fitConfidence = 'Medium'
            } else {
                $fitConfidence = 'Constrained'
            }

            if ($sourceVcpu -lt 4 -and $targetVcore -eq 4) {
                $reasonParts.Add('The source is below SQL MI standalone minimum size, so it rounds up to 4 vCores.')
            }
            if ($targetVcore -lt $sourceVcpu) {
                $caveatParts.Add("Target vCores are $(Format-Number ([math]::Abs($cpuDeltaPercent)))% below the AWS vCPU count because the closest available SQL MI shape trades CPU for memory or reaches the service limit.")
            }
            if ($targetMemoryGiB -lt $sourceMemoryGiB) {
                $caveatParts.Add("Target memory is $(Format-Number ([math]::Abs($memoryDeltaPercent)))% below the AWS allocation because no closer SQL MI hardware shape is available.")
            }

            $sourceIsMultiAz = $rdsRow.deploymentOption -match '^Multi-AZ'
            $targetIsZoneRedundant = [System.Convert]::ToBoolean($target.IsZoneRedundant)
            if ($sourceIsMultiAz -and $targetIsZoneRedundant) {
                $reasonParts.Add('AWS Multi-AZ maps to SQL MI zone redundancy to retain availability-zone fault isolation.')
            } elseif ($sourceIsMultiAz) {
                $caveatParts.Add('The paired Azure region/configuration has no matching zone-redundant price, so this falls back to local redundancy.')
            } else {
                $reasonParts.Add('AWS Single-AZ maps to locally redundant SQL MI; managed service failover remains built in without zone redundancy.')
            }

            if ($rdsRow.TermType -eq 'OnDemand') {
                $reasonParts.Add('AWS On-Demand maps to Azure pay-as-you-go.')
            } else {
                $reasonParts.Add("AWS $($rdsRow.LeaseContractLength) $($rdsRow.PurchaseOption) maps to the same-length Azure reservation; Azure exposes one effective reservation rate rather than separate upfront payment choices.")
            }

            if (Test-AzureHybridBenefitFit $rdsRow) {
                $reasonParts.Add('Customer-provided/BYOM or developer licensing maps to Azure Hybrid Benefit as the closest license-porting price.')
                $caveatParts.Add('Azure Hybrid Benefit requires eligible SQL Server licenses with Software Assurance or a qualifying subscription; otherwise use the license-included option.')
            } else {
                $reasonParts.Add('AWS license-included maps to the SQL MI license-included option.')
            }
            if ($rdsRow.deploymentModel -eq 'Custom') {
                $caveatParts.Add('RDS Custom host/OS access and customer-provided media are not available on SQL Managed Instance; validate agent, file-system, and unsupported-feature dependencies.')
            }
            $caveatParts.Add('Sizing is catalog-based, not benchmark equivalence; validate CPU, memory, I/O, tempdb, storage capacity, database count, and SQL feature compatibility before migration.')
        }
        'Database Storage' {
            $sourceIsMultiAz = $rdsRow.deploymentOption -match '^Multi-AZ'
            $usesHighIopsStorage = $rdsRow.volumeType -match 'Provisioned IOPS' -or $rdsRow.volumeName -match '^io[12]$'
            $storageTier = Get-DesiredServiceTier $rdsRow.databaseEdition
            if ($usesHighIopsStorage) {
                $storageTier = 'Business Critical'
            }
            $target = Get-RetailTarget $region.AzureRegion 'Data Storage' $storageTier $sourceIsMultiAz '' '' '1 GB/Month'
            $mappingStatus = 'Mapped to SQL MI component meter'
            $fitConfidence = 'High'
            $sqlMiUnitsPerRdsUnit = [decimal]1
            $sqlMiComparedPrice = Get-DecimalValue $target.RetailPrice
            $comparisonBasis = 'One provisioned GB-month maps to one SQL MI reserved-storage GB-month.'
            if ($usesHighIopsStorage) {
                $reasonParts.Add('RDS io1/io2 storage maps to Business Critical local-SSD storage as the closest low-latency practical replacement.')
            } else {
                $reasonParts.Add("RDS $($rdsRow.volumeName) storage maps to $storageTier reserved data storage with the same GB-month unit.")
            }
            if ($sourceIsMultiAz) {
                $reasonParts.Add('The zone-redundant storage meter preserves the source Multi-AZ storage topology.')
            }
            $caveatParts.Add('SQL MI storage is provisioned in 32-GB increments and maximum capacity depends on service tier, hardware, vCores, and region.')
        }
        'Provisioned IOPS' {
            $sourceIsMultiAz = $rdsRow.deploymentOption -match '^Multi-AZ'
            $target = Get-RetailTarget $region.AzureRegion 'Additional IOPS' 'General Purpose' $sourceIsMultiAz '' '' '1 IOPS/Month'
            $mappingStatus = 'Mapped to conditional SQL MI component meter'
            $fitConfidence = 'Medium'
            $sqlMiUnitsPerRdsUnit = [decimal]1
            $sqlMiComparedPrice = Get-DecimalValue $target.RetailPrice
            $comparisonBasis = 'One provisioned RDS IOPS-month maps to one additional Next-gen General Purpose IOPS-month.'
            $reasonParts.Add('The SQL MI Additional IOPS meter is the direct unit match for provisioned IOPS and supports explicit I/O sizing in Next-gen General Purpose.')
            $caveatParts.Add('Use this charge only with Next-gen General Purpose and above its free IOPS allowance; Business Critical includes IOPS in the service-tier price.')
        }
        'Provisioned Throughput' {
            $sourceIsMultiAz = $rdsRow.deploymentOption -match '^Multi-AZ'
            $target = Get-RetailTarget $region.AzureRegion 'Additional IOPS' 'General Purpose' $sourceIsMultiAz '' '' '1 IOPS/Month'
            $mappingStatus = 'Mapped through SQL MI IOPS conversion'
            $fitConfidence = 'Medium'
            $sqlMiUnitsPerRdsUnit = [decimal]30
            $sqlMiComparedPrice = (Get-DecimalValue $target.RetailPrice) * $sqlMiUnitsPerRdsUnit
            $comparisonBasis = 'Next-gen General Purpose uses throughput MB/s = IOPS / 30, so 1 MB/s-month maps to approximately 30 additional IOPS-months.'
            $reasonParts.Add('SQL MI has no standalone throughput meter; Next-gen General Purpose derives throughput from IOPS at approximately 30 IOPS per MB/s, making Additional IOPS the closest attainable control.')
            $caveatParts.Add('The 30:1 conversion is a service formula, but vCore throughput caps still apply and free IOPS may reduce the billed quantity.')
        }
        'Storage Snapshot' {
            $target = Get-RetailTarget $region.AzureRegion 'PITR Backup Storage' '' $false 'Backup RA-GRS' '' '1 GB/Month'
            if ($null -eq $target) {
                $target = Get-RetailTarget $region.AzureRegion 'PITR Backup Storage' '' $false '' '' '1 GB/Month'
            }
            $mappingStatus = 'Mapped to SQL MI component meter'
            $fitConfidence = 'High'
            $sqlMiUnitsPerRdsUnit = [decimal]1
            $sqlMiComparedPrice = Get-DecimalValue $target.RetailPrice
            $comparisonBasis = 'One RDS backup/snapshot GB-month maps to one RA-GRS PITR backup GB-month.'
            $reasonParts.Add("RDS snapshot or excess backup storage maps to SQL MI PITR backup storage using the available $($target.SkuName) meter.")
            if ($target.SkuName -ne 'Backup RA-GRS') {
                $caveatParts.Add('The paired Azure region does not publish an RA-GRS PITR meter in this catalog, so the nearest available backup redundancy meter is used.')
            }
            $caveatParts.Add('SQL MI includes PITR backup storage up to the configured maximum data size; only excess usage is billed, and long-term retention uses a separate LTR meter.')
        }
        'Optimized License' {
            if ($rdsRow.licenseType -eq 'SQLServer') {
                $licenseTier = Get-DesiredServiceTier $rdsRow.databaseEdition
                $licenseCandidateList = $configuredIndex["$($region.AzureRegion)|$licenseTier|payg|False"]
                $licenseCandidates = if ($null -ne $licenseCandidateList) { @($licenseCandidateList.ToArray()) } else { @() }
                $target = $licenseCandidates |
                    Where-Object { $_.HardwareFamily -eq 'Gen5' -and $_.VCoreCount -eq '4' } |
                    Sort-Object SqlMiPriceId |
                    Select-Object -First 1
                if ($null -eq $target) {
                    $target = $licenseCandidates | Sort-Object {[int]$_.VCoreCount}, SqlMiPriceId | Select-Object -First 1
                }
                $targetVcore = Get-DecimalValue $target.VCoreCount
                $targetMemoryGiB = Get-SqlMiMemoryGiB $target.HardwareFamily $targetVcore
                $mappingStatus = 'Mapped to SQL MI license component'
                $fitConfidence = 'High'
                $sqlMiUnitsPerRdsUnit = [decimal]1
                $sqlMiComparedPrice = (Get-DecimalValue $target.SqlLicenseHourlyPrice) / $targetVcore
                $comparisonBasis = 'The SQL license portion of a license-included SQL MI configuration is divided by vCores for a per-vCore-hour comparison.'
                $reasonParts.Add("The RDS $($rdsRow.databaseEdition) SQL Server license meter maps to the per-vCore license component of a $licenseTier SQL MI configuration.")
                $caveatParts.Add('This row compares license components only; use a configured SKU mapping for the complete compute-plus-license replacement.')
            } else {
                $mappingStatus = 'Included; no separate SQL MI meter'
                $fitConfidence = 'High'
                $sqlMiUnitsPerRdsUnit = [decimal]0
                $sqlMiComparedPrice = [decimal]0
                $comparisonBasis = 'Windows OS licensing is included in the SQL MI PaaS service and has no separately billable replacement meter.'
                $reasonParts.Add('The RDS Windows optimized-license charge has no separate SQL MI equivalent because Microsoft operates and licenses the underlying OS as part of the managed service.')
            }
        }
        'CPU Credits' {
            $mappingStatus = 'Included; no separate SQL MI meter'
            $fitConfidence = 'High'
            $sqlMiUnitsPerRdsUnit = [decimal]0
            $sqlMiComparedPrice = [decimal]0
            $comparisonBasis = 'SQL MI uses continuously provisioned vCores and does not bill burst CPU credits separately.'
            $reasonParts.Add('RDS T3 CPU credits disappear on SQL MI because the replacement uses sustained provisioned vCores rather than a burst-credit model.')
            $caveatParts.Add('This zero applies only to the supplemental credit meter; SQL MI compute is still charged through the configured vCore SKU.')
        }
        'Performance Insights' {
            $mappingStatus = 'Included; no direct SQL MI meter'
            $fitConfidence = 'Medium'
            $sqlMiUnitsPerRdsUnit = [decimal]0
            $sqlMiComparedPrice = [decimal]0
            $comparisonBasis = 'Core SQL MI diagnostics such as Query Store are included and have no vCPU-month Performance Insights meter.'
            $reasonParts.Add('RDS Performance Insights maps operationally to built-in SQL MI Query Store and monitoring capabilities, with no direct SQL MI vCPU-month SKU.')
            $caveatParts.Add('Azure Monitor diagnostic-log ingestion, retention, workbooks, or third-party observability can add charges outside the SQL MI price catalog.')
        }
        'RDSProxy' {
            $mappingStatus = 'Included; no direct SQL MI meter'
            $fitConfidence = 'Medium'
            $sqlMiUnitsPerRdsUnit = [decimal]0
            $sqlMiComparedPrice = [decimal]0
            $comparisonBasis = 'SQL MI connectivity and gateway behavior are part of the managed endpoint, with no per-vCPU proxy SKU.'
            $reasonParts.Add('RDS Proxy has no separately billed SQL MI replacement; managed connectivity is exposed through the SQL MI endpoint and application pooling remains an application concern.')
            $caveatParts.Add('RDS Proxy features are not identical to the SQL MI gateway; validate connection pooling, authentication, and failover behavior in the application stack.')
        }
        default {
            throw "Unhandled RDS product family '$($rdsRow.ProductFamily)' for $($rdsRow.RdsPriceId)."
        }
    }

    $rdsUnitPrice = Get-DecimalValue $rdsRow.PricePerUnit
    $costComparisonSource = if ($rdsRow.ProductFamily -eq 'Database Instance') {
        $rdsEffectiveHourlyPrice
    } else {
        $rdsUnitPrice
    }
    $comparableCostDelta = $null
    $comparableCostDeltaPercent = $null
    if ($null -ne $sqlMiComparedPrice -and $null -ne $costComparisonSource) {
        $comparableCostDelta = $sqlMiComparedPrice - $costComparisonSource
        if ($costComparisonSource -ne 0) {
            $comparableCostDeltaPercent = ($comparableCostDelta / $costComparisonSource) * 100
        }
    }

    $targetVcoreCount = $null
    $vcoreDelta = $null
    $vcoreDeltaPercent = $null
    $memoryDeltaGiB = $null
    $memoryDeltaPercent = $null
    if ($null -ne $target -and -not [string]::IsNullOrWhiteSpace($target.VCoreCount)) {
        $targetVcoreCount = Get-DecimalValue $target.VCoreCount
        if ($null -ne $sourceVcpu) {
            $vcoreDelta = $targetVcoreCount - $sourceVcpu
            $vcoreDeltaPercent = ($vcoreDelta / $sourceVcpu) * 100
        }
        if ($null -eq $targetMemoryGiB) {
            $targetMemoryGiB = Get-SqlMiMemoryGiB $target.HardwareFamily $targetVcoreCount
        }
        if ($null -ne $sourceMemoryGiB) {
            $memoryDeltaGiB = $targetMemoryGiB - $sourceMemoryGiB
            $memoryDeltaPercent = ($memoryDeltaGiB / $sourceMemoryGiB) * 100
        }
    }

    $mappingRows.Add([pscustomobject][ordered]@{
        MappingId = 'MAP-{0:D6}' -f ($index + 1)
        GeneratedAtUtc = $generatedAtUtc
        MappingRuleVersion = $mappingRuleVersion
        SizingMethodSourceUrl = $sizingSourceUrl
        MappingStatus = $mappingStatus
        MappingCategory = $mappingCategory
        FitConfidence = $fitConfidence
        FitScore = $fitScore
        RdsPriceId = $rdsRow.RdsPriceId
        RdsRegion = $rdsRow.Region
        RdsSku = $rdsRow.SKU
        RdsRateCode = $rdsRow.RateCode
        RdsProductFamily = $rdsRow.ProductFamily
        RdsInstanceType = $rdsRow.instanceType
        RdsInstanceFamily = $rdsRow.instanceFamily
        RdsVcpu = $sourceVcpu
        RdsMemoryGiB = $sourceMemoryGiB
        RdsDatabaseEdition = $rdsRow.databaseEdition
        RdsLicenseModel = $rdsRow.licenseModel
        RdsLicenseType = $rdsRow.licenseType
        RdsDeploymentModel = $rdsRow.deploymentModel
        RdsDeploymentOption = $rdsRow.deploymentOption
        RdsVolumeType = $rdsRow.volumeType
        RdsVolumeName = $rdsRow.volumeName
        RdsTermType = $rdsRow.TermType
        RdsLeaseContractLength = $rdsRow.LeaseContractLength
        RdsPurchaseOption = $rdsRow.PurchaseOption
        RdsUnit = $rdsRow.Unit
        RdsPricePerUnit = $rdsUnitPrice
        RdsPricingGroupKey = $rdsPricingGroupKey
        RdsEffectiveHourlyPrice = $rdsEffectiveHourlyPrice
        RdsEffectiveMonthlyPrice = $rdsEffectiveMonthlyPrice
        RdsDescription = $rdsRow.Description
        AzureRegion = $region.AzureRegion
        SqlMiPriceId = if ($null -ne $target) { $target.SqlMiPriceId } else { '' }
        SqlMiRecordKind = if ($null -ne $target) { $target.RecordKind } else { '' }
        SqlMiConfigurationKey = if ($null -ne $target) { $target.ConfigurationKey } else { '' }
        SqlMiProductName = if ($null -ne $target) { $target.ProductName } else { '' }
        SqlMiSkuName = if ($null -ne $target) { $target.SkuName } else { '' }
        SqlMiMeterName = if ($null -ne $target) { $target.MeterName } else { '' }
        SqlMiPricingComponent = if ($null -ne $target) { $target.PricingComponent } else { '' }
        SqlMiServiceTier = if ($null -ne $target) { $target.ServiceTier } else { '' }
        SqlMiInstanceType = if ($null -ne $target) { $target.InstanceType } else { '' }
        SqlMiHardwareFamily = if ($null -ne $target) { $target.HardwareFamily } else { '' }
        SqlMiVCoreCount = $targetVcoreCount
        SqlMiEstimatedMemoryGiB = $targetMemoryGiB
        SqlMiIsZoneRedundant = if ($null -ne $target) { $target.IsZoneRedundant } else { '' }
        SqlMiComputePurchaseOption = if ($null -ne $target) { $target.ComputePurchaseOption } else { '' }
        SqlMiLicenseOption = if ($null -ne $target) { $target.LicenseOption } else { '' }
        SqlMiUnitOfMeasure = if ($null -ne $target) { $target.UnitOfMeasure } else { '' }
        SqlMiRetailPrice = if ($null -ne $target) { $target.RetailPrice } else { '' }
        SqlMiEffectiveHourlyPrice = if ($null -ne $target) { $target.EffectiveHourlyPrice } else { '' }
        SqlMiEffectiveMonthlyPrice = if ($null -ne $target) { $target.EffectiveMonthlyPrice } else { '' }
        SqlMiUnitsPerRdsUnit = $sqlMiUnitsPerRdsUnit
        SqlMiComparedPricePerRdsUnit = $sqlMiComparedPrice
        VCoreDelta = $vcoreDelta
        VCoreDeltaPercent = $vcoreDeltaPercent
        MemoryDeltaGiB = $memoryDeltaGiB
        MemoryDeltaPercent = $memoryDeltaPercent
        ComparableCostDelta = $comparableCostDelta
        ComparableCostDeltaPercent = $comparableCostDeltaPercent
        ComparisonBasis = $comparisonBasis
        ReasonChosen = $reasonParts -join ' '
        Caveats = $caveatParts -join ' '
    })
}

$mappingRows | Export-Csv -Path $OutputPath -NoTypeInformation -Encoding UTF8
Write-Host "Wrote $($mappingRows.Count) rows to $OutputPath"