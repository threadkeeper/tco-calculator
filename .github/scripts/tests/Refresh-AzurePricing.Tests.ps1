Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '..\Refresh-AzurePricing.ps1')

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function New-TestResponse {
    param([int]$StatusCode, $Body, [hashtable]$Headers = @{})

    return [pscustomobject]@{
        StatusCode = $StatusCode
        Headers = $Headers
        Content = if ($Body -is [string]) { $Body } else { $Body | ConvertTo-Json -Depth 8 -Compress }
    }
}

function New-TestHarness {
    param([object[]]$Responses)

    $queue = [System.Collections.Generic.Queue[object]]::new()
    foreach ($response in $Responses) {
        $queue.Enqueue($response)
    }
    $requests = [System.Collections.Generic.List[object]]::new()
    $delays = [System.Collections.Generic.List[int]]::new()
    $requestInvoker = {
        param($request)
        $requests.Add($request)
        if ($queue.Count -eq 0) {
            throw 'The test response queue is empty.'
        }
        return $queue.Dequeue()
    }.GetNewClosure()
    $delayInvoker = {
        param($seconds)
        $delays.Add([int]$seconds)
    }.GetNewClosure()
    return [pscustomobject]@{
        Requests = $requests
        Delays = $delays
        RequestInvoker = $requestInvoker
        DelayInvoker = $delayInvoker
    }
}

$origin = [Uri]'https://calculator.example.test/'

$catalog = New-TestResponse 200 @{ items = @(
    @{ code = 'australiaeast' },
    @{ code = 'brazilsouth' },
    @{ code = 'eastus' },
    @{ code = 'northeurope' },
    @{ code = 'swedencentral' }
) }
$schemaFailure = New-TestResponse 200 @{
    status = 'stale'
    snapshot_id = 'azure-existing'
    retrieved_at = '2026-08-01T00:00:00Z'
    warnings = @('Live Azure price refresh failed (provider_schema_changed); the most recent usable snapshot was returned.')
}
$harness = New-TestHarness @($catalog, $schemaFailure, $schemaFailure, $schemaFailure)
$result = Invoke-AzurePricingRefresh -Origin $origin -AttemptBudget 40 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 3 $result.AttemptsUsed 'Schema circuit attempt count.'
Assert-Equal 3 $result.Failures.Count 'Schema circuit failure count.'
Assert-Equal 2 $result.Skipped.Count 'Schema circuit skipped count.'
Assert-Equal 'repeated_provider_schema_changed' $result.CircuitReason 'Schema circuit reason.'

$catalog = New-TestResponse 200 @{ items = @(@{ code = 'swedencentral' }) }
$temporary = New-TestResponse 503 '' @{ 'Retry-After' = '9' }
$fresh = New-TestResponse 200 @{
    status = 'fresh'
    snapshot_id = 'azure-current'
    retrieved_at = '2026-08-13T15:00:00Z'
    warnings = @()
}
$harness = New-TestHarness @($catalog, $temporary, $fresh)
$result = Invoke-AzurePricingRefresh -Origin $origin -AttemptBudget 40 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 2 $result.AttemptsUsed 'Transient retry attempt count.'
Assert-Equal 1 $result.Refreshed.Count 'Transient retry success count.'
Assert-Equal 0 $result.Failures.Count 'Transient retry failure count.'
Assert-Equal 1 $harness.Delays.Count 'Transient retry delay count.'
Assert-Equal 9 $harness.Delays[0] 'Transient Retry-After delay.'

$catalog = New-TestResponse 200 @{ items = @(
    @{ code = 'australiaeast' },
    @{ code = 'swedencentral' }
) }
$rateLimited = New-TestResponse 429 '' @{ 'Retry-After' = '3599' }
$harness = New-TestHarness @($catalog, $rateLimited)
$result = Invoke-AzurePricingRefresh -Origin $origin -AttemptBudget 40 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 1 $result.AttemptsUsed 'Rate-limit circuit attempt count.'
Assert-Equal 1 $result.Failures.Count 'Rate-limit circuit failure count.'
Assert-Equal 1 $result.Skipped.Count 'Rate-limit circuit skipped count.'
Assert-Equal 'http_429' $result.CircuitReason 'Rate-limit circuit reason.'
Assert-Equal 0 $harness.Delays.Count 'Rate-limit circuit must not sleep and retry.'

$catalog = New-TestResponse 200 @{ items = @(
    @{ code = 'australiaeast' },
    @{ code = 'swedencentral' }
) }
$temporaryHttp = New-TestResponse 503 ''
$harness = New-TestHarness @($catalog, $temporaryHttp, $temporaryHttp)
$result = Invoke-AzurePricingRefresh -Origin $origin -AttemptBudget 2 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 2 $result.AttemptsUsed 'Global attempt budget count.'
Assert-Equal 1 $result.Failures.Count 'Global attempt budget failure count.'
Assert-Equal 1 $result.Skipped.Count 'Global attempt budget skipped count.'
Assert-Equal 'attempt_budget_exhausted' $result.CircuitReason 'Global attempt budget circuit reason.'

Write-Output 'Refresh-AzurePricing tests passed.'