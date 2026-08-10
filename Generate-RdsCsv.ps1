param(
    [string]$OutputPath = (Join-Path $PSScriptRoot 'RDS.csv')
)

$ErrorActionPreference = 'Stop'

$pricingBaseUrl = 'https://pricing.us-east-1.amazonaws.com'
$sourceOfferCodes = @(
    'AmazonRDS',
    'AmazonRDSOCPULicenseFees'
)
$generatedAtUtc = (Get-Date).ToUniversalTime().ToString('o')
$attributeNames = @(
    'clockSpeed',
    'currentGeneration',
    'databaseEdition',
    'databaseEngine',
    'dedicatedEbsThroughput',
    'deploymentModel',
    'deploymentOption',
    'engineCode',
    'engineMediaType',
    'enhancedNetworkingSupported',
    'group',
    'groupDescription',
    'instanceFamily',
    'instanceType',
    'instanceTypeFamily',
    'licenseModel',
    'licenseType',
    'location',
    'locationType',
    'maxVolumeSize',
    'memory',
    'minVolumeSize',
    'networkPerformance',
    'normalizationSizeFactor',
    'operation',
    'physicalProcessor',
    'processorArchitecture',
    'processorFeatures',
    'regionCode',
    'servicecode',
    'servicename',
    'storage',
    'storageMedia',
    'unbundledLicensing',
    'usagetype',
    'vcpu',
    'volumeName',
    'volumeType',
    'windowslicensemultiplier'
)

$rows = New-Object 'System.Collections.Generic.List[object]'

foreach ($sourceOfferCode in $sourceOfferCodes) {
    $regionIndexUrl = "$pricingBaseUrl/offers/v1.0/aws/$sourceOfferCode/current/region_index.json"
    $regionIndex = Invoke-RestMethod $regionIndexUrl
    $regions = @(
        $regionIndex.regions.PSObject.Properties |
            Where-Object { $_.Name -like 'eu-*' } |
            Sort-Object Name
    )

    foreach ($regionProperty in $regions) {
        $regionCode = $regionProperty.Name
        $priceListUrl = "$pricingBaseUrl$($regionProperty.Value.currentVersionUrl)"
        Write-Host "Loading $sourceOfferCode $regionCode..."
        $offer = Invoke-RestMethod $priceListUrl
        $productsBySku = @{}

        foreach ($productProperty in $offer.products.PSObject.Properties) {
            $product = $productProperty.Value
            if ($sourceOfferCode -eq 'AmazonRDSOCPULicenseFees' -or $product.attributes.databaseEngine -eq 'SQL Server') {
                $productsBySku[$productProperty.Name] = $product
            }
        }

        foreach ($termTypeProperty in $offer.terms.PSObject.Properties) {
            $termType = $termTypeProperty.Name

            foreach ($skuTermProperty in $termTypeProperty.Value.PSObject.Properties) {
                $sku = $skuTermProperty.Name
                if (-not $productsBySku.ContainsKey($sku)) {
                    continue
                }

                $product = $productsBySku[$sku]
                foreach ($offerTermProperty in $skuTermProperty.Value.PSObject.Properties) {
                    $offerTerm = $offerTermProperty.Value
                    $termAttributes = $offerTerm.termAttributes

                    foreach ($dimensionProperty in $offerTerm.priceDimensions.PSObject.Properties) {
                        $dimension = $dimensionProperty.Value

                        foreach ($currencyProperty in $dimension.pricePerUnit.PSObject.Properties) {
                            $row = [ordered]@{
                                GeneratedAtUtc = $generatedAtUtc
                                SourceOfferCode = $sourceOfferCode
                                OfferVersion = $offer.version
                                PublicationDate = $offer.publicationDate
                                SourcePriceListUrl = $priceListUrl
                                Region = $regionCode
                                SKU = $sku
                                ProductFamily = $product.productFamily
                            }

                            foreach ($attributeName in $attributeNames) {
                                $row[$attributeName] = $product.attributes.$attributeName
                            }

                            $row['TermType'] = $termType
                            $row['OfferTermCode'] = $offerTerm.offerTermCode
                            $row['EffectiveDate'] = $offerTerm.effectiveDate
                            $row['LeaseContractLength'] = $termAttributes.LeaseContractLength
                            $row['PurchaseOption'] = $termAttributes.PurchaseOption
                            $row['OfferingClass'] = $termAttributes.OfferingClass
                            $row['RateCode'] = $dimension.rateCode
                            $row['Description'] = $dimension.description
                            $row['BeginRange'] = $dimension.beginRange
                            $row['EndRange'] = $dimension.endRange
                            $row['Unit'] = $dimension.unit
                            $row['AppliesTo'] = @($dimension.appliesTo) -join ';'
                            $row['Currency'] = $currencyProperty.Name
                            $row['PricePerUnit'] = $currencyProperty.Value
                            $rows.Add([pscustomobject]$row)
                        }
                    }
                }
            }
        }
    }
}

$sortedRows = @(
    $rows |
        Sort-Object Region, SourceOfferCode, ProductFamily, databaseEdition, licenseModel, deploymentOption, instanceType, TermType, LeaseContractLength, PurchaseOption, RateCode
)
$identifiedRows = for ($index = 0; $index -lt $sortedRows.Count; $index++) {
    $identifiedRow = [ordered]@{
        RdsPriceId = 'RDS-{0:D6}' -f ($index + 1)
    }
    foreach ($property in $sortedRows[$index].PSObject.Properties) {
        $identifiedRow[$property.Name] = $property.Value
    }
    [pscustomobject]$identifiedRow
}

$identifiedRows | Export-Csv -Path $OutputPath -NoTypeInformation -Encoding UTF8

Write-Host "Wrote $($identifiedRows.Count) rows to $OutputPath"