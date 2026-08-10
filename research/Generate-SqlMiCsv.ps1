param(
    [string]$OutputPath = (Join-Path $PSScriptRoot 'SQLMI.csv'),
    [string]$CurrencyCode = 'USD'
)

$ErrorActionPreference = 'Stop'

function Get-ServiceTier {
    param([string]$Text)

    if ($Text -match 'Next Generation General Purpose|next-gen-general-purpose') {
        return 'Next Generation General Purpose'
    }
    if ($Text -match 'Business Critical|business-critical') {
        return 'Business Critical'
    }
    if ($Text -match 'Instance Pool|instance-pools') {
        return 'General Purpose - Instance Pool'
    }
    if ($Text -match 'General Purpose|general-purpose') {
        return 'General Purpose'
    }
    return ''
}

function Get-HardwareFamily {
    param([string]$Text)

    if ($Text -match 'Premium Series Memory Optimized|premium-series-memory-optimized') {
        return 'Premium Series Memory Optimized'
    }
    if ($Text -match 'Premium Series|premium-series') {
        return 'Premium Series'
    }
    if ($Text -match 'Gen5|gen5') {
        return 'Gen5'
    }
    if ($Text -match 'Gen4|gen4') {
        return 'Gen4'
    }
    return ''
}

function Get-PricingComponent {
    param($Item)

    if ($Item.productName -match 'PITR Backup') {
        return 'PITR Backup Storage'
    }
    if ($Item.productName -match 'LTR Backup') {
        return 'LTR Backup Storage'
    }
    if ($Item.meterName -match 'IOPS') {
        return 'Additional IOPS'
    }
    if ($Item.productName -match 'Storage') {
        return 'Data Storage'
    }
    if ($Item.meterName -match 'Memory') {
        return 'Additional Memory'
    }
    if ($Item.productName -match 'Compute') {
        return 'Compute'
    }
    return 'Other'
}

$serviceName = 'SQL Managed Instance'
$generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
$calculatorApiUrl = 'https://azure.microsoft.com/api/v3/pricing/azure-sql/calculator/?culture=en-us&discount=mca'
$regionMap = [ordered]@{
    austriaeast = [pscustomobject]@{ Location = 'AT East'; CalculatorSlug = 'austria-east'; AwsRegion = 'eu-central-1 (Frankfurt)' }
    belgiumcentral = [pscustomobject]@{ Location = 'BE Central'; CalculatorSlug = 'belgium-central'; AwsRegion = 'eu-west-3 (Paris)' }
    denmarkeast = [pscustomobject]@{ Location = 'DK East'; CalculatorSlug = 'denmark-east'; AwsRegion = 'eu-north-1 (Stockholm)' }
    francecentral = [pscustomobject]@{ Location = 'FR Central'; CalculatorSlug = 'france-central'; AwsRegion = 'eu-west-3 (Paris)' }
    francesouth = [pscustomobject]@{ Location = 'FR South'; CalculatorSlug = 'france-south'; AwsRegion = 'eu-west-3 (Paris)' }
    germanynorth = [pscustomobject]@{ Location = 'DE North'; CalculatorSlug = 'germany-north'; AwsRegion = 'eu-central-1 (Frankfurt)' }
    germanywestcentral = [pscustomobject]@{ Location = 'DE West Central'; CalculatorSlug = 'germany-west-central'; AwsRegion = 'eu-central-1 (Frankfurt)' }
    italynorth = [pscustomobject]@{ Location = 'IT North'; CalculatorSlug = 'italy-north'; AwsRegion = 'eu-south-1 (Milan)' }
    northeurope = [pscustomobject]@{ Location = 'EU North'; CalculatorSlug = 'europe-north'; AwsRegion = 'eu-west-1 (Ireland)' }
    norwayeast = [pscustomobject]@{ Location = 'NO East'; CalculatorSlug = 'norway-east'; AwsRegion = 'eu-north-1 (Stockholm)' }
    norwaywest = [pscustomobject]@{ Location = 'NO West'; CalculatorSlug = 'norway-west'; AwsRegion = 'eu-north-1 (Stockholm)' }
    polandcentral = [pscustomobject]@{ Location = 'PL Central'; CalculatorSlug = 'poland-central'; AwsRegion = 'eu-central-1 (Frankfurt)' }
    spaincentral = [pscustomobject]@{ Location = 'ES Central'; CalculatorSlug = 'spain-central'; AwsRegion = 'eu-south-2 (Spain)' }
    swedencentral = [pscustomobject]@{ Location = 'SE Central'; CalculatorSlug = 'sweden-central'; AwsRegion = 'eu-north-1 (Stockholm)' }
    swedensouth = [pscustomobject]@{ Location = 'SE South'; CalculatorSlug = 'sweden-south'; AwsRegion = 'eu-north-1 (Stockholm)' }
    switzerlandnorth = [pscustomobject]@{ Location = 'CH North'; CalculatorSlug = 'switzerland-north'; AwsRegion = 'eu-central-2 (Zurich)' }
    switzerlandwest = [pscustomobject]@{ Location = 'CH West'; CalculatorSlug = 'switzerland-west'; AwsRegion = 'eu-central-2 (Zurich)' }
    uksouth = [pscustomobject]@{ Location = 'UK South'; CalculatorSlug = 'united-kingdom-south'; AwsRegion = 'eu-west-2 (London)' }
    ukwest = [pscustomobject]@{ Location = 'UK West'; CalculatorSlug = 'united-kingdom-west'; AwsRegion = 'eu-west-2 (London)' }
    westeurope = [pscustomobject]@{ Location = 'EU West'; CalculatorSlug = 'europe-west'; AwsRegion = 'eu-central-1 (Frankfurt)' }
}
$optionMap = @{
    'payg' = [pscustomobject]@{ Compute = 'Pay as you go'; License = 'License included - Pay as you go'; Term = ''; Months = $null }
    'ahb' = [pscustomobject]@{ Compute = 'Pay as you go'; License = 'Azure Hybrid Benefit (BYOL)'; Term = ''; Months = $null }
    'one-year' = [pscustomobject]@{ Compute = '1 year reserved'; License = 'License included - Pay as you go'; Term = '1 Year'; Months = 12 }
    'ahbone-year' = [pscustomobject]@{ Compute = '1 year reserved'; License = 'Azure Hybrid Benefit (BYOL)'; Term = '1 Year'; Months = 12 }
    'three-year' = [pscustomobject]@{ Compute = '3 year reserved'; License = 'License included - Pay as you go'; Term = '3 Years'; Months = 36 }
    'ahbthree-year' = [pscustomobject]@{ Compute = '3 year reserved'; License = 'Azure Hybrid Benefit (BYOL)'; Term = '3 Years'; Months = 36 }
    'sv-one-year' = [pscustomobject]@{ Compute = '1 year savings plan'; License = 'License included - 1 year savings plan'; Term = '1 Year'; Months = 12 }
    'ahbsv-one-year' = [pscustomobject]@{ Compute = '1 year savings plan'; License = 'Azure Hybrid Benefit (BYOL)'; Term = '1 Year'; Months = 12 }
}
$rows = New-Object 'System.Collections.Generic.List[object]'

foreach ($azureRegion in $regionMap.Keys) {
    $comparison = $regionMap[$azureRegion]
    $filter = [uri]::EscapeDataString("serviceName eq '$serviceName' and armRegionName eq '$azureRegion'")
    $sourceApiUrl = "https://prices.azure.com/api/retail/prices?currencyCode='$CurrencyCode'&`$filter=$filter"
    $nextPageUrl = $sourceApiUrl

    Write-Host "Loading retail prices for $azureRegion..."
    do {
        $response = Invoke-RestMethod $nextPageUrl
        foreach ($item in $response.Items) {
            $serviceTier = Get-ServiceTier $item.productName
            $hardwareFamily = Get-HardwareFamily $item.productName
            $pricingComponent = Get-PricingComponent $item
            $vCoreCount = $null
            if ($item.skuName -match '^(\d+) vCore') {
                $vCoreCount = [int]$Matches[1]
            }

            $isZoneRedundant = $item.skuName -match 'Zone Redundan(?:cy|t)|\bZR\b' -or
                $item.meterName -match 'Zone Redundan(?:cy|t)'
            $pricingBasis = if ($null -ne $vCoreCount) {
                'Full compute SKU component; SQL license is separate'
            } elseif ($item.skuName -match '^vCore(?:\s|$)') {
                'Per vCore compute component; SQL license is separate'
            } else {
                "Per $($item.unitOfMeasure)"
            }
            $reservationMonths = switch ($item.reservationTerm) {
                '1 Year' { 12 }
                '3 Years' { 36 }
                default { $null }
            }
            $retailPrice = [decimal]$item.retailPrice
            $effectiveHourlyPrice = $null
            $effectiveMonthlyPrice = $null
            $effectiveAnnualPrice = $null
            $reservationTotalPrice = $null

            if ($item.type -eq 'Reservation' -and $reservationMonths) {
                $reservationTotalPrice = $retailPrice
                $effectiveMonthlyPrice = $retailPrice / $reservationMonths
                $effectiveAnnualPrice = $effectiveMonthlyPrice * 12
                $effectiveHourlyPrice = $retailPrice / (($reservationMonths / 12) * 8760)
            } elseif ($item.unitOfMeasure -in @('1 Hour', '1 GB/Hour')) {
                $effectiveHourlyPrice = $retailPrice
                $effectiveMonthlyPrice = $retailPrice * 730
                $effectiveAnnualPrice = $retailPrice * 8760
            } elseif ($item.unitOfMeasure -match 'Month') {
                $effectiveMonthlyPrice = $retailPrice
                $effectiveAnnualPrice = $retailPrice * 12
            }

            $computePurchaseOption = if ($item.type -eq 'Reservation') {
                "$($item.reservationTerm) reserved"
            } else {
                'Pay as you go component'
            }
            $computeHourlyPrice = if ($pricingComponent -eq 'Compute') {
                $effectiveHourlyPrice
            } else {
                $null
            }

            $rows.Add([pscustomobject][ordered]@{
                GeneratedAtUtc = $generatedAtUtc
                RecordKind = 'Retail Price Dimension'
                SourcePriceKind = 'Azure Retail Prices API'
                SourceApiUrl = $sourceApiUrl
                ComparableAwsRegion = $comparison.AwsRegion
                IsClosestAwsIrelandMatch = $item.armRegionName -eq 'northeurope'
                AzureRegion = $item.armRegionName
                AzureLocation = $item.location
                CalculatorRegionSlug = $comparison.CalculatorSlug
                PricingComponent = $pricingComponent
                ServiceTier = $serviceTier
                InstanceType = 'Single Instance or component-level meter'
                HardwareFamily = $hardwareFamily
                VCoreCount = $vCoreCount
                IsZoneRedundant = $isZoneRedundant
                DeploymentRole = 'Primary or unspecified component'
                ComputePurchaseOption = $computePurchaseOption
                LicenseOption = 'Component price only; see Configured SKU Total rows'
                PricingBasis = $pricingBasis
                ConfigurationKey = ''
                CalculatorOptionKey = ''
                ComponentReferences = ''
                CurrencyCode = $item.currencyCode
                RetailPrice = $item.retailPrice
                UnitPrice = $item.unitPrice
                UnitOfMeasure = $item.unitOfMeasure
                ComputeHourlyPrice = $computeHourlyPrice
                SqlLicenseHourlyPrice = $null
                ReservationTotalPrice = $reservationTotalPrice
                ReservationTerm = $item.reservationTerm
                ReservationTermMonths = $reservationMonths
                EffectiveHourlyPrice = $effectiveHourlyPrice
                EffectiveMonthlyPrice = $effectiveMonthlyPrice
                EffectiveAnnualPrice = $effectiveAnnualPrice
                TierMinimumUnits = $item.tierMinimumUnits
                PriceType = $item.type
                EffectiveStartDate = $item.effectiveStartDate
                MeterId = $item.meterId
                MeterName = $item.meterName
                ProductId = $item.productId
                SkuId = $item.skuId
                ProductName = $item.productName
                SkuName = $item.skuName
                ArmSkuName = $item.armSkuName
                ServiceName = $item.serviceName
                ServiceId = $item.serviceId
                ServiceFamily = $item.serviceFamily
                IsPrimaryMeterRegion = $item.isPrimaryMeterRegion
            })
        }
        $nextPageUrl = $response.NextPageLink
    } while ($nextPageUrl)
}

Write-Host 'Loading configured SQL Managed Instance totals...'
$calculatorPricing = Invoke-RestMethod $calculatorApiUrl
$configurations = @(
    $calculatorPricing.skus.PSObject.Properties |
        Where-Object {
            $_.Name -match '^managed-vcore-.*-\d+$' -and
            $_.Name -notmatch '-(?:software|reserved)(?:-|$)' -and
            $_.Value.PSObject.Properties.Name -contains 'payg' -and
            $_.Value.PSObject.Properties.Name -contains 'ahb'
        }
)

foreach ($azureRegion in $regionMap.Keys) {
    $comparison = $regionMap[$azureRegion]

    foreach ($configuration in $configurations) {
        $configurationKey = $configuration.Name
        $serviceTier = Get-ServiceTier $configurationKey
        $hardwareFamily = Get-HardwareFamily $configurationKey
        $instanceType = if ($configurationKey -match 'instance-pools') {
            'Instance Pools'
        } else {
            'Single Instance'
        }
        $isZoneRedundant = $configurationKey -match '-zone-'
        $vCoreCount = $null
        if ($configurationKey -match '-(\d+)$') {
            $vCoreCount = [int]$Matches[1]
        }

        foreach ($option in $configuration.Value.PSObject.Properties) {
            if (-not $optionMap.ContainsKey($option.Name)) {
                continue
            }

            $computeHourlyPrice = [decimal]0
            $sqlLicenseHourlyPrice = [decimal]0
            $componentReferences = New-Object 'System.Collections.Generic.List[string]'
            $missingPrice = $false

            foreach ($reference in @($option.Value)) {
                $parts = [string]$reference -split '--', 2
                if ($parts.Count -ne 2) {
                    $missingPrice = $true
                    break
                }

                $offerProperty = $calculatorPricing.offers.PSObject.Properties[$parts[0]]
                if ($null -eq $offerProperty) {
                    $missingPrice = $true
                    break
                }

                $offer = $offerProperty.Value
                $dimensionProperty = $offer.prices.PSObject.Properties[$parts[1]]
                if ($null -eq $dimensionProperty) {
                    $missingPrice = $true
                    break
                }

                $regionPriceProperty = $dimensionProperty.Value.PSObject.Properties[$comparison.CalculatorSlug]
                if ($null -eq $regionPriceProperty) {
                    $missingPrice = $true
                    break
                }

                $componentPrice = [decimal]$regionPriceProperty.Value.value
                if ($offer.offerType -eq 'software') {
                    $sqlLicenseHourlyPrice += $componentPrice
                } else {
                    $computeHourlyPrice += $componentPrice
                }
                $componentReferences.Add([string]$reference)
            }

            if ($missingPrice) {
                continue
            }

            $optionMetadata = $optionMap[$option.Name]
            $totalHourlyPrice = $computeHourlyPrice + $sqlLicenseHourlyPrice
            $skuName = "$vCoreCount vCore"
            if ($isZoneRedundant) {
                $skuName += ' Zone Redundant'
            }

            $rows.Add([pscustomobject][ordered]@{
                GeneratedAtUtc = $generatedAtUtc
                RecordKind = 'Configured SKU Total'
                SourcePriceKind = 'Azure Pricing Calculator component sum'
                SourceApiUrl = $calculatorApiUrl
                ComparableAwsRegion = $comparison.AwsRegion
                IsClosestAwsIrelandMatch = $azureRegion -eq 'northeurope'
                AzureRegion = $azureRegion
                AzureLocation = $comparison.Location
                CalculatorRegionSlug = $comparison.CalculatorSlug
                PricingComponent = 'Configured Compute + SQL License'
                ServiceTier = $serviceTier
                InstanceType = $instanceType
                HardwareFamily = $hardwareFamily
                VCoreCount = $vCoreCount
                IsZoneRedundant = $isZoneRedundant
                DeploymentRole = 'Primary Instance'
                ComputePurchaseOption = $optionMetadata.Compute
                LicenseOption = $optionMetadata.License
                PricingBasis = 'Full configured SKU (compute plus selected SQL license)'
                ConfigurationKey = $configurationKey
                CalculatorOptionKey = $option.Name
                ComponentReferences = $componentReferences -join ';'
                CurrencyCode = $CurrencyCode
                RetailPrice = $totalHourlyPrice
                UnitPrice = $totalHourlyPrice
                UnitOfMeasure = '1 Hour'
                ComputeHourlyPrice = $computeHourlyPrice
                SqlLicenseHourlyPrice = $sqlLicenseHourlyPrice
                ReservationTotalPrice = $null
                ReservationTerm = $optionMetadata.Term
                ReservationTermMonths = $optionMetadata.Months
                EffectiveHourlyPrice = $totalHourlyPrice
                EffectiveMonthlyPrice = $totalHourlyPrice * 730
                EffectiveAnnualPrice = $totalHourlyPrice * 8760
                TierMinimumUnits = 0
                PriceType = 'Calculator Configuration'
                EffectiveStartDate = ''
                MeterId = ''
                MeterName = 'Configured compute and SQL license total'
                ProductId = ''
                SkuId = ''
                ProductName = "Azure SQL Managed Instance $serviceTier - $hardwareFamily"
                SkuName = $skuName
                ArmSkuName = ''
                ServiceName = $serviceName
                ServiceId = ''
                ServiceFamily = 'Databases'
                IsPrimaryMeterRegion = ''
            })
        }
    }
}

$sortedRows = @(
    $rows |
        Sort-Object AzureRegion, RecordKind, ServiceTier, InstanceType, HardwareFamily, VCoreCount, IsZoneRedundant, ComputePurchaseOption, LicenseOption, PricingComponent, ProductName, SkuName, MeterName, PriceType, ReservationTerm
)
$identifiedRows = for ($index = 0; $index -lt $sortedRows.Count; $index++) {
    $identifiedRow = [ordered]@{
        SqlMiPriceId = 'SQLMI-{0:D6}' -f ($index + 1)
    }
    foreach ($property in $sortedRows[$index].PSObject.Properties) {
        $identifiedRow[$property.Name] = $property.Value
    }
    [pscustomobject]$identifiedRow
}

$identifiedRows | Export-Csv -Path $OutputPath -NoTypeInformation -Encoding UTF8

Write-Host "Wrote $($identifiedRows.Count) rows to $OutputPath"