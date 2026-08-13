Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

. (Join-Path $PSScriptRoot '..\Refresh-AwsPricing.ps1')

function Assert-Equal {
    param($Expected, $Actual, [string]$Message)

    if ($Expected -ne $Actual) {
        throw "$Message Expected '$Expected', got '$Actual'."
    }
}

function Assert-Throws {
    param([scriptblock]$Action, [string]$ExpectedMessage, [string]$Message)

    try {
        & $Action
    }
    catch {
        if ($_.Exception.Message -notlike $ExpectedMessage) {
            throw "$Message Expected '$ExpectedMessage', got '$($_.Exception.Message)'."
        }
        return
    }
    throw "$Message Expected an exception."
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
$fresh = New-TestResponse 200 @{
    status = 'fresh'
    snapshot_id = 'aws-current'
    retrieved_at = '2026-08-13T18:00:00Z'
    warnings = @()
}
$cached = New-TestResponse 200 @{
    status = 'cached'
    snapshot_id = 'aws-existing'
    retrieved_at = '2026-08-13T12:00:00Z'
    warnings = @('Live AWS price refresh failed (provider_temporarily_unavailable); the most recent usable snapshot was returned.')
}
$stale = New-TestResponse 200 @{
    status = 'stale'
    snapshot_id = 'aws-existing'
    retrieved_at = '2026-08-10T12:00:00Z'
    warnings = @('Live AWS price refresh failed (provider_schema_changed); the most recent usable snapshot was returned.')
}
$unavailable = New-TestResponse 200 @{
    status = 'unavailable'
    snapshot_id = $null
    retrieved_at = $null
    warnings = @('Live AWS price refresh failed; no usable snapshot is available.')
}

$harness = New-TestHarness @($fresh, $fresh, $fresh)
$result = Invoke-AwsPricingRefresh -Origin $origin -Regions @('region-a', 'region-b', 'region-c') `
    -MaxAttemptsPerRegion 3 -InterAttemptDelaySeconds 0 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 3 $result.AttemptsUsed 'All-fresh attempt count.'
Assert-Equal 3 $result.Refreshed.Count 'All-fresh refreshed count.'
Assert-Equal 0 $result.Retained.Count 'All-fresh retained count.'
Assert-Equal 0 $result.Failures.Count 'All-fresh unavailable count.'
Assert-AwsPricingRefreshResult -Result $result -MaxRetainedSnapshots 2

$harness = New-TestHarness @($cached, $cached, $cached, $stale, $stale, $stale, $fresh)
$result = Invoke-AwsPricingRefresh -Origin $origin -Regions @('region-a', 'region-b', 'region-c') `
    -MaxAttemptsPerRegion 3 -InterAttemptDelaySeconds 0 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 7 $result.AttemptsUsed 'Two-fallback attempt count.'
Assert-Equal 1 $result.Refreshed.Count 'Two-fallback fresh count.'
Assert-Equal 2 $result.Retained.Count 'Two-fallback retained count.'
Assert-Equal 0 $result.Failures.Count 'Two-fallback unavailable count.'
Assert-Equal 'cached' $result.Retained[0].Status 'Cached fallback status.'
Assert-Equal 'stale' $result.Retained[1].Status 'Stale fallback status.'
Assert-Equal 3 $result.Retained[0].Attempts 'Cached fallback attempt count.'
Assert-Equal 3 $result.Retained[1].Attempts 'Stale fallback attempt count.'
Assert-AwsPricingRefreshResult -Result $result -MaxRetainedSnapshots 2

$harness = New-TestHarness @($cached, $cached, $cached)
$result = Invoke-AwsPricingRefresh -Origin $origin -Regions @('region-a', 'region-b', 'region-c') `
    -MaxAttemptsPerRegion 1 -InterAttemptDelaySeconds 0 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Throws -Action { Assert-AwsPricingRefreshResult -Result $result -MaxRetainedSnapshots 2 } `
    -ExpectedMessage '3 of 3 AWS regions retained existing snapshots; the allowed maximum is 2.' `
    -Message 'Three-fallback degradation budget.'

$harness = New-TestHarness @($unavailable, $unavailable, $unavailable)
$result = Invoke-AwsPricingRefresh -Origin $origin -Regions @('region-a') `
    -MaxAttemptsPerRegion 3 -InterAttemptDelaySeconds 0 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 3 $result.AttemptsUsed 'Unavailable snapshot attempt count.'
Assert-Throws -Action { Assert-AwsPricingRefreshResult -Result $result -MaxRetainedSnapshots 2 } `
    -ExpectedMessage '1 of 1 AWS regions have no usable snapshot.' `
    -Message 'Unavailable snapshot failure.'

$harness = New-TestHarness @($cached, $fresh)
$result = Invoke-AwsPricingRefresh -Origin $origin -Regions @('region-a') `
    -MaxAttemptsPerRegion 3 -InterAttemptDelaySeconds 0 -InterRegionDelaySeconds 0 `
    -RequestInvoker $harness.RequestInvoker -DelayInvoker $harness.DelayInvoker
Assert-Equal 2 $result.AttemptsUsed 'Recovered refresh attempt count.'
Assert-Equal 1 $result.Refreshed.Count 'Recovered refresh fresh count.'
Assert-Equal 0 $result.Retained.Count 'Recovered refresh retained count.'
Assert-Equal 0 $result.Failures.Count 'Recovered refresh unavailable count.'

Write-Output 'Refresh-AwsPricing tests passed.'