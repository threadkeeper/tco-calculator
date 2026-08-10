param(
    [string]$OutputPath = (Join-Path $PSScriptRoot 'EC2.csv'),
    [string]$CurrencyCode = 'USD',
    [string[]]$RegionCodes = @(
        'eu-central-1',
        'eu-central-2',
        'eu-north-1',
        'eu-south-1',
        'eu-south-2',
        'eu-west-1',
        'eu-west-2',
        'eu-west-3'
    ),
    [ValidateSet('OnDemand', 'Reserved')]
    [string[]]$TermTypes = @('OnDemand', 'Reserved'),
    [int]$MaxBranchesPerRegion = 0
)

$ErrorActionPreference = 'Stop'
$invariantCulture = [System.Globalization.CultureInfo]::InvariantCulture
$generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
$monthlyHours = [decimal]730
$annualHours = [decimal]8760
$regionMap = [ordered]@{
    'eu-central-1' = 'EU (Frankfurt)'
    'eu-central-2' = 'EU (Zurich)'
    'eu-north-1' = 'EU (Stockholm)'
    'eu-south-1' = 'EU (Milan)'
    'eu-south-2' = 'EU (Spain)'
    'eu-west-1' = 'EU (Ireland)'
    'eu-west-2' = 'EU (London)'
    'eu-west-3' = 'EU (Paris)'
}
$catalogBaseUrl = "https://calculator.aws/pricing/2.0/meteredUnitMaps/ec2/$CurrencyCode/current/ec2-calc"
$metadataUrl = "$catalogBaseUrl/metadata.json"

function Invoke-PricingJson {
    param([string]$Uri)

    $lastError = $null
    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            return Invoke-RestMethod -Uri $Uri -UseBasicParsing
        } catch {
            $lastError = $_
        }
    }
    throw $lastError
}

function ConvertTo-UrlPathSegment {
    param([string]$Value)

    return [uri]::EscapeDataString($Value)
}

function ConvertTo-DecimalValue {
    param($Value)

    if ([string]::IsNullOrWhiteSpace([string]$Value)) {
        return [decimal]0
    }
    return [decimal]::Parse([string]$Value, [System.Globalization.NumberStyles]::Any, $invariantCulture)
}

function ConvertTo-DecimalText {
    param($Value)

    if ($null -eq $Value) {
        return ''
    }
    return ([decimal]$Value).ToString('0.0000000000', $invariantCulture)
}

function Get-MemoryGiB {
    param([string]$Memory)

    if ($Memory -match '^([0-9]+(?:\.[0-9]+)?)\s+GiB$') {
        return [decimal]::Parse($Matches[1], $invariantCulture)
    }
    return $null
}

function Get-SqlServerEdition {
    param([string]$PreInstalledSoftware)

    switch ($PreInstalledSoftware) {
        'SQL Web' { return 'Web' }
        'SQL Std' { return 'Standard' }
        'SQL Ent' { return 'Enterprise' }
        default { return 'Customer supplied / BYOL' }
    }
}

function Get-LeafUrl {
    param(
        [string]$Location,
        $Selectors
    )

    $segments = New-Object 'System.Collections.Generic.List[string]'
    @(
        $Location,
        $Selectors.TermType,
        $Selectors.Tenancy,
        $Selectors.'Operating System',
        $Selectors.'Pre Installed S/W',
        $Selectors.'License Model'
    ) | ForEach-Object { $segments.Add((ConvertTo-UrlPathSegment $_)) }

    if ($Selectors.TermType -eq 'Reserved') {
        @(
            $Selectors.LeaseContractLength,
            $Selectors.PurchaseOption,
            $Selectors.OfferingClass
        ) | ForEach-Object { $segments.Add((ConvertTo-UrlPathSegment $_)) }
    }

    $segments.Add((ConvertTo-UrlPathSegment $Selectors.'Current Generation'))
    return "$catalogBaseUrl/$($segments -join '/')/index.json"
}

function Get-BranchSortKey {
    param($Branch)

    $selectors = $Branch.selectors
    return @(
        $selectors.TermType,
        $selectors.Tenancy,
        $selectors.'Pre Installed S/W',
        $selectors.'License Model',
        $selectors.LeaseContractLength,
        $selectors.PurchaseOption,
        $selectors.OfferingClass,
        $selectors.'Current Generation'
    ) -join '|'
}

function Write-CsvBatch {
    param(
        [System.IO.StreamWriter]$Writer,
        [System.Collections.IEnumerable]$Rows
    )

    $csvLines = @($Rows | ConvertTo-Csv -NoTypeInformation)
    if ($csvLines.Count -eq 0) {
        return
    }

    $startIndex = if ($script:csvHeaderWritten) { 1 } else { 0 }
    for ($lineIndex = $startIndex; $lineIndex -lt $csvLines.Count; $lineIndex++) {
        $Writer.WriteLine($csvLines[$lineIndex])
    }
    $script:csvHeaderWritten = $true
}

foreach ($regionCode in $RegionCodes) {
    if (-not $regionMap.Contains($regionCode)) {
        throw "Unsupported region code '$regionCode'."
    }
}

$metadata = Invoke-PricingJson $metadataUrl
$catalogVersion = if ($metadata.manifest.esIndex -match '(\d{14})$') { $Matches[1] } else { $metadata.manifest.esIndex }
$outputDirectory = Split-Path -Parent $OutputPath
if (-not [string]::IsNullOrWhiteSpace($outputDirectory) -and -not (Test-Path $outputDirectory)) {
    New-Item -ItemType Directory -Path $outputDirectory | Out-Null
}
$temporaryOutputPath = "$OutputPath.tmp"
if (Test-Path $temporaryOutputPath) {
    Remove-Item $temporaryOutputPath -Force
}

$utf8WithBom = New-Object System.Text.UTF8Encoding($true)
$writer = New-Object System.IO.StreamWriter($temporaryOutputPath, $false, $utf8WithBom)
$script:csvHeaderWritten = $false
$rowCount = 0
$sourceDimensionCount = 0
$completed = $false

try {
    foreach ($regionCode in $RegionCodes) {
        $location = $regionMap[$regionCode]
        $selectorUrl = "$catalogBaseUrl/$(ConvertTo-UrlPathSegment $location)/primary-selector-aggregations.json"
        Write-Host "Loading EC2 selectors for $regionCode ($location)..."
        $selectorCatalog = Invoke-PricingJson $selectorUrl
        $branches = @(
            $selectorCatalog.aggregations |
                Where-Object {
                    $_.selectors.'Operating System' -eq 'Windows' -and
                    $_.selectors.'Pre Installed S/W' -in @('NA', 'SQL Web', 'SQL Std', 'SQL Ent') -and
                    $_.selectors.TermType -in $TermTypes
                } |
                Sort-Object { Get-BranchSortKey $_ }
        )
        if ($MaxBranchesPerRegion -gt 0) {
            $branches = @($branches | Select-Object -First $MaxBranchesPerRegion)
        }

        $branchNumber = 0
        foreach ($branch in $branches) {
            $branchNumber++
            $selectors = $branch.selectors
            $leafUrl = Get-LeafUrl -Location $location -Selectors $selectors
            Write-Progress -Activity "Loading EC2 prices for $regionCode" -Status "Branch $branchNumber of $($branches.Count)" -PercentComplete (($branchNumber / $branches.Count) * 100)
            $leaf = Invoke-PricingJson $leafUrl
            $regionProperty = $leaf.regions.PSObject.Properties[$location]
            if ($null -eq $regionProperty) {
                throw "The price leaf did not contain location '$location': $leafUrl"
            }

            $dimensionProperties = @($regionProperty.Value.PSObject.Properties)
            if ($dimensionProperties.Count -ne [int]$branch.total_count) {
                throw "Expected $($branch.total_count) dimensions but found $($dimensionProperties.Count): $leafUrl"
            }
            $sourceDimensionCount += $dimensionProperties.Count

            $dimensions = @($dimensionProperties | ForEach-Object { $_.Value })
            $skuGroups = @(
                $dimensions |
                    Group-Object { ([string]$_.rateCode -split '\.')[0] } |
                    Sort-Object @{ Expression = { $_.Group[0].'Instance Type' } }, Name
            )
            $branchRows = New-Object 'System.Collections.Generic.List[object]'

            foreach ($skuGroup in $skuGroups) {
                $priceDimensions = @($skuGroup.Group)
                $product = $priceDimensions[0]
                if ($product.'Physical Processor' -match 'Graviton|ARM') {
                    continue
                }

                $offerTermCodes = @(
                    $priceDimensions |
                        ForEach-Object { ([string]$_.rateCode -split '\.')[1] } |
                        Sort-Object -Unique
                )
                if ($offerTermCodes.Count -ne 1) {
                    throw "Expected one offer term but found $($offerTermCodes.Count) for SKU '$($skuGroup.Name)'."
                }

                $hourlyDimensions = @($priceDimensions | Where-Object { $_.Unit -eq 'Hrs' })
                $upfrontDimensions = @($priceDimensions | Where-Object { $_.Unit -eq 'Quantity' })
                $unknownUnits = @($priceDimensions | Where-Object { $_.Unit -notin @('Hrs', 'Quantity') })
                if ($unknownUnits.Count -gt 0) {
                    throw "Unsupported price unit '$($unknownUnits[0].Unit)' for SKU '$($skuGroup.Name)'."
                }

                $recurringHourlyPrice = [decimal]0
                foreach ($dimension in $hourlyDimensions) {
                    $recurringHourlyPrice += ConvertTo-DecimalValue $dimension.price
                }
                $upfrontPrice = [decimal]0
                foreach ($dimension in $upfrontDimensions) {
                    $upfrontPrice += ConvertTo-DecimalValue $dimension.price
                }

                $reservationYears = switch ($product.LeaseContractLength) {
                    '1yr' { [decimal]1 }
                    '3yr' { [decimal]3 }
                    default { $null }
                }
                $reservationMonths = if ($null -ne $reservationYears) { [int]($reservationYears * 12) } else { $null }
                $amortizedUpfrontHourlyPrice = if ($null -ne $reservationYears) {
                    $upfrontPrice / ($annualHours * $reservationYears)
                } else {
                    [decimal]0
                }
                $effectiveHourlyPrice = $recurringHourlyPrice + $amortizedUpfrontHourlyPrice
                $effectiveMonthlyPrice = if ($null -ne $reservationYears) {
                    ($recurringHourlyPrice * $monthlyHours) + ($upfrontPrice / $reservationMonths)
                } else {
                    $recurringHourlyPrice * $monthlyHours
                }
                $effectiveAnnualPrice = if ($null -ne $reservationYears) {
                    ($recurringHourlyPrice * $annualHours) + ($upfrontPrice / $reservationYears)
                } else {
                    $recurringHourlyPrice * $annualHours
                }
                $reservationTermTotalPrice = if ($null -ne $reservationYears) {
                    ($recurringHourlyPrice * $annualHours * $reservationYears) + $upfrontPrice
                } else {
                    $null
                }

                $offerTermCode = $offerTermCodes[0]
                $memoryGiB = Get-MemoryGiB $product.Memory
                $sqlServerEdition = Get-SqlServerEdition $product.'Pre Installed S/W'
                $sqlLicenseOption = if ($product.'Pre Installed S/W' -eq 'NA') {
                    'Customer-provided SQL Server license and media (BYOL/self-managed)'
                } else {
                    'SQL Server license included in EC2 price'
                }

                $rowCount++
                $branchRows.Add([pscustomobject][ordered]@{
                    Ec2PriceId = 'EC2-{0:D6}' -f $rowCount
                    GeneratedAtUtc = $generatedAtUtc
                    SourceServiceCode = 'AmazonEC2'
                    SourcePriceCatalog = 'AWS Pricing Calculator EC2 catalog'
                    SourceCatalogVersion = $catalogVersion
                    SourcePublicationDate = $metadata.manifest.hawkFilePublicationDate
                    SourceIngestionDate = $metadata.manifest.ETLIngestionTriggerDate
                    SourceMetadataUrl = $metadataUrl
                    SourceSelectorUrl = $selectorUrl
                    SourcePriceUrl = $leafUrl
                    AWSRegionCode = $regionCode
                    AWSLocation = $product.Location
                    Currency = $metadata.manifest.currencyCode
                    InstanceType = $product.'Instance Type'
                    InstanceFamily = $product.'Instance Family'
                    vCPU = $product.vCPU
                    ECU = $product.ECU
                    Memory = $product.Memory
                    MemoryGiB = if ($null -ne $memoryGiB) { ConvertTo-DecimalText $memoryGiB } else { '' }
                    Storage = $product.Storage
                    NetworkPerformance = $product.'Network Performance'
                    PhysicalProcessor = $product.'Physical Processor'
                    ProcessorArchitecture = 'x86_64'
                    CurrentGeneration = $product.'Current Generation'
                    OperatingSystem = $product.'Operating System'
                    WindowsLicenseModel = $product.'License Model'
                    PreInstalledSoftware = $product.'Pre Installed S/W'
                    SqlServerEdition = $sqlServerEdition
                    SqlServerLicenseOption = $sqlLicenseOption
                    TermType = $product.TermType
                    Tenancy = $product.Tenancy
                    LeaseContractLength = $product.LeaseContractLength
                    ReservationTermMonths = $reservationMonths
                    PurchaseOption = $product.PurchaseOption
                    OfferingClass = $product.OfferingClass
                    PricingBasis = if ($product.TermType -eq 'Reserved') { 'Reserved hourly price plus amortized upfront fee' } else { 'On-Demand hourly price' }
                    MonthlyHours = ConvertTo-DecimalText $monthlyHours
                    AnnualHours = ConvertTo-DecimalText $annualHours
                    RecurringHourlyPrice = ConvertTo-DecimalText $recurringHourlyPrice
                    UpfrontPrice = ConvertTo-DecimalText $upfrontPrice
                    AmortizedUpfrontHourlyPrice = ConvertTo-DecimalText $amortizedUpfrontHourlyPrice
                    EffectiveHourlyPrice = ConvertTo-DecimalText $effectiveHourlyPrice
                    EffectiveMonthlyPrice = ConvertTo-DecimalText $effectiveMonthlyPrice
                    EffectiveAnnualPrice = ConvertTo-DecimalText $effectiveAnnualPrice
                    ReservationTermTotalPrice = ConvertTo-DecimalText $reservationTermTotalPrice
                    SKU = $skuGroup.Name
                    OfferTermCode = $offerTermCode
                    HourlyRateCode = @($hourlyDimensions | ForEach-Object { $_.rateCode }) -join ';'
                    UpfrontRateCode = @($upfrontDimensions | ForEach-Object { $_.rateCode }) -join ';'
                    PriceDimensionCount = $priceDimensions.Count
                })
            }

            Write-CsvBatch -Writer $writer -Rows $branchRows
        }
        Write-Progress -Activity "Loading EC2 prices for $regionCode" -Completed
        $writer.Flush()
        Write-Host "Completed ${regionCode}: $($branches.Count) branches."
    }

    if (-not $script:csvHeaderWritten) {
        throw 'No matching EC2 prices were found.'
    }
    $writer.Flush()
    $writer.Dispose()
    $writer = $null
    Move-Item -Path $temporaryOutputPath -Destination $OutputPath -Force
    $completed = $true
} finally {
    if ($null -ne $writer) {
        $writer.Dispose()
    }
    if (-not $completed -and (Test-Path $temporaryOutputPath)) {
        Remove-Item $temporaryOutputPath -Force
    }
}

Write-Host "Wrote $rowCount configurations from $sourceDimensionCount source price dimensions to $OutputPath"