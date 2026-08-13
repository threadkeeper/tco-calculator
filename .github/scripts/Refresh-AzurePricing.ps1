[CmdletBinding()]
param(
    [string]$BaseUrl = $env:TCO_CALCULATOR_URL,
    [ValidateRange(1, 1000)]
    [int]$AttemptBudget = 40,
    [ValidateRange(1, 3)]
    [int]$MaxAttemptsPerRegion = 3,
    [ValidateRange(30, 180)]
    [int]$RequestTimeoutSeconds = 150,
    [ValidateRange(0, 30)]
    [int]$InterRegionDelaySeconds = 1
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ValidatedOrigin {
    param([Parameter(Mandatory)][string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw 'GitHub environment dev is missing TCO_CALCULATOR_URL.'
    }
    $origin = $null
    if (-not [Uri]::TryCreate($Value, [UriKind]::Absolute, [ref]$origin) -or
        $origin.Scheme -ne 'https' -or
        -not [string]::IsNullOrEmpty($origin.Query) -or
        -not [string]::IsNullOrEmpty($origin.Fragment)) {
        throw 'TCO_CALCULATOR_URL must be an absolute HTTPS URL without a query or fragment.'
    }
    return $origin
}

function Invoke-DefaultHttpRequest {
    param([Parameter(Mandatory)][pscustomobject]$Request)

    $parameters = @{
        Uri = $Request.Uri
        Method = $Request.Method
        TimeoutSec = $Request.TimeoutSeconds
        SkipHttpErrorCheck = $true
    }
    if ($null -ne $Request.Body) {
        $parameters.ContentType = 'application/json'
        $parameters.Body = $Request.Body
    }
    $response = Invoke-WebRequest @parameters
    return [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        Headers = $response.Headers
        Content = $response.Content
    }
}

function Invoke-DefaultDelay {
    param([Parameter(Mandatory)][int]$Seconds)

    if ($Seconds -gt 0) {
        Start-Sleep -Seconds $Seconds
    }
}

function Get-RetryAfterSeconds {
    param([Parameter(Mandatory)]$Headers)

    $raw = $null
    try {
        $raw = $Headers['Retry-After']
    }
    catch {
        return $null
    }
    if ($null -eq $raw) {
        return $null
    }
    $value = [string]@($raw)[0]
    $seconds = 0
    if ([int]::TryParse($value, [ref]$seconds) -and $seconds -ge 0) {
        return $seconds
    }
    $retryAt = [DateTimeOffset]::MinValue
    if ([DateTimeOffset]::TryParse($value, [ref]$retryAt)) {
        return [Math]::Max(0, [int][Math]::Ceiling(($retryAt - [DateTimeOffset]::UtcNow).TotalSeconds))
    }
    return $null
}

function New-RefreshOutcome {
    param(
        [Parameter(Mandatory)][bool]$Succeeded,
        [Parameter(Mandatory)][string]$Category,
        [bool]$Retryable = $false,
        [bool]$OpenCircuit = $false,
        [AllowNull()][Nullable[int]]$RetryAfterSeconds = $null,
        [AllowNull()][string]$RetrievedAt = $null
    )

    return [pscustomobject]@{
        Succeeded = $Succeeded
        Category = $Category
        Retryable = $Retryable
        OpenCircuit = $OpenCircuit
        RetryAfterSeconds = $RetryAfterSeconds
        RetrievedAt = $RetrievedAt
    }
}

function ConvertTo-RefreshOutcome {
    param([Parameter(Mandatory)]$Response)

    $statusCode = [int]$Response.StatusCode
    if ($statusCode -eq 429) {
        return New-RefreshOutcome -Succeeded $false -Category 'http_429' -OpenCircuit $true `
            -RetryAfterSeconds (Get-RetryAfterSeconds -Headers $Response.Headers)
    }
    if ($statusCode -eq 408 -or $statusCode -ge 500) {
        return New-RefreshOutcome -Succeeded $false -Category "http_$statusCode" -Retryable $true `
            -RetryAfterSeconds (Get-RetryAfterSeconds -Headers $Response.Headers)
    }
    if ($statusCode -lt 200 -or $statusCode -ge 300) {
        return New-RefreshOutcome -Succeeded $false -Category "http_$statusCode"
    }

    try {
        $payload = $Response.Content | ConvertFrom-Json
    }
    catch {
        return New-RefreshOutcome -Succeeded $false -Category 'invalid_response'
    }
    $statusProperty = $payload.PSObject.Properties['status']
    $snapshotProperty = $payload.PSObject.Properties['snapshot_id']
    $retrievedProperty = $payload.PSObject.Properties['retrieved_at']
    if ($null -eq $statusProperty) {
        return New-RefreshOutcome -Succeeded $false -Category 'invalid_response'
    }
    $status = [string]$statusProperty.Value
    $snapshotId = if ($null -eq $snapshotProperty) { $null } else { [string]$snapshotProperty.Value }
    $retrievedAt = if ($null -eq $retrievedProperty) { $null } else { [string]$retrievedProperty.Value }
    if ($status -eq 'fresh' -and -not [string]::IsNullOrWhiteSpace($snapshotId)) {
        return New-RefreshOutcome -Succeeded $true -Category 'fresh' -RetrievedAt $retrievedAt
    }

    $warningProperty = $payload.PSObject.Properties['warnings']
    $warningText = if ($null -eq $warningProperty) { '' } else { @($warningProperty.Value) -join ' ' }
    $reasonCodes = @([regex]::Matches(
        $warningText,
        '\b(?:provider_[a-z_]+|price_not_found|scope_unsupported)\b'
    ) | ForEach-Object { $_.Value } | Sort-Object -Unique)
    $category = if ($reasonCodes.Count -gt 0) { $reasonCodes[0] } else { "status_$status" }
    return New-RefreshOutcome -Succeeded $false -Category $category `
        -Retryable ($category -eq 'provider_temporarily_unavailable')
}

function Get-CatalogRegions {
    param(
        [Parameter(Mandatory)][Uri]$CatalogEndpoint,
        [Parameter(Mandatory)][scriptblock]$RequestInvoker,
        [Parameter(Mandatory)][scriptblock]$DelayInvoker,
        [Parameter(Mandatory)][int]$TimeoutSeconds
    )

    for ($attempt = 1; $attempt -le 3; $attempt++) {
        try {
            $response = & $RequestInvoker ([pscustomobject]@{
                Uri = $CatalogEndpoint
                Method = 'Get'
                Body = $null
                TimeoutSeconds = $TimeoutSeconds
            })
            $statusCode = [int]$response.StatusCode
            if ($statusCode -ge 200 -and $statusCode -lt 300) {
                $catalog = $response.Content | ConvertFrom-Json
                $regions = @($catalog.items | ForEach-Object { $_.code } | Where-Object {
                    -not [string]::IsNullOrWhiteSpace($_)
                } | Sort-Object -Unique)
                if ($regions.Count -eq 0) {
                    throw 'The supported Azure region catalog is empty.'
                }
                return $regions
            }
            $transient = $statusCode -eq 408 -or $statusCode -eq 429 -or $statusCode -ge 500
            if (-not $transient) {
                throw "The supported Azure region catalog returned HTTP $statusCode."
            }
            $retryAfter = Get-RetryAfterSeconds -Headers $response.Headers
        }
        catch {
            if ($attempt -ge 3 -or $_.Exception.Message -like 'The supported Azure region catalog*') {
                throw "Unable to load the supported Azure region catalog: $($_.Exception.Message)"
            }
            $retryAfter = $null
        }
        if ($attempt -lt 3) {
            $delay = if ($null -ne $retryAfter) { $retryAfter } else { 2 * $attempt }
            if ($delay -gt 60) {
                throw 'Unable to load the supported Azure region catalog within the retry budget.'
            }
            & $DelayInvoker $delay
        }
    }
}

function Invoke-AzurePricingRefresh {
    param(
        [Parameter(Mandatory)][Uri]$Origin,
        [int]$AttemptBudget = 40,
        [int]$MaxAttemptsPerRegion = 3,
        [int]$RequestTimeoutSeconds = 150,
        [int]$InterRegionDelaySeconds = 1,
        [scriptblock]$RequestInvoker = ${function:Invoke-DefaultHttpRequest},
        [scriptblock]$DelayInvoker = ${function:Invoke-DefaultDelay}
    )

    $catalogEndpoint = [Uri]::new($Origin, '/api/v1/catalog/azure/regions')
    $refreshEndpoint = [Uri]::new($Origin, '/api/v1/pricing/azure/refresh')
    $regions = @(Get-CatalogRegions -CatalogEndpoint $catalogEndpoint -RequestInvoker $RequestInvoker `
        -DelayInvoker $DelayInvoker -TimeoutSeconds $RequestTimeoutSeconds)
    if ($regions.Count -gt $AttemptBudget) {
        throw "Attempt budget $AttemptBudget is smaller than the supported region count $($regions.Count)."
    }

    $refreshed = [System.Collections.Generic.List[object]]::new()
    $failures = [System.Collections.Generic.List[object]]::new()
    $skipped = [System.Collections.Generic.List[object]]::new()
    $attemptsUsed = 0
    $consecutiveSchemaFailures = 0
    $circuitReason = $null

    foreach ($region in $regions) {
        if ($null -ne $circuitReason) {
            $skipped.Add([pscustomobject]@{ Region = $region; Category = $circuitReason })
            continue
        }
        $body = @{
            aws_region = $null
            azure_region = $region
            currency = 'USD'
            resources = @()
        } | ConvertTo-Json -Compress

        $finalOutcome = $null
        for ($attempt = 1; $attempt -le $MaxAttemptsPerRegion; $attempt++) {
            if ($attemptsUsed -ge $AttemptBudget) {
                $circuitReason = 'attempt_budget_exhausted'
                break
            }
            $attemptsUsed++
            try {
                $response = & $RequestInvoker ([pscustomobject]@{
                    Uri = $refreshEndpoint
                    Method = 'Post'
                    Body = $body
                    TimeoutSeconds = $RequestTimeoutSeconds
                })
                $outcome = ConvertTo-RefreshOutcome -Response $response
            }
            catch {
                $outcome = New-RefreshOutcome -Succeeded $false -Category 'transport_error' -Retryable $true
            }
            $finalOutcome = $outcome
            if ($outcome.Succeeded) {
                $refreshed.Add([pscustomobject]@{
                    Region = $region
                    RetrievedAt = $outcome.RetrievedAt
                    Attempts = $attempt
                })
                Write-Host "Refreshed $region at $($outcome.RetrievedAt) after $attempt attempt(s)."
                break
            }
            if ($outcome.OpenCircuit) {
                $circuitReason = $outcome.Category
                break
            }
            if (-not $outcome.Retryable -or $attempt -ge $MaxAttemptsPerRegion) {
                break
            }
            if ($attemptsUsed -ge $AttemptBudget) {
                $circuitReason = 'attempt_budget_exhausted'
                break
            }
            $delay = if ($null -ne $outcome.RetryAfterSeconds) {
                $outcome.RetryAfterSeconds
            }
            else {
                (5 * [Math]::Pow(2, $attempt - 1)) + (Get-Random -Minimum 0 -Maximum 3)
            }
            if ($delay -gt 120) {
                $circuitReason = 'retry_after_exceeds_job_budget'
                break
            }
            Write-Warning "Transient Azure pricing refresh failure for $region ($($outcome.Category)); retrying attempt $($attempt + 1)."
            & $DelayInvoker ([int]$delay)
        }

        if ($null -eq $finalOutcome -or -not $finalOutcome.Succeeded) {
            $category = if ($null -ne $finalOutcome) { $finalOutcome.Category } else { $circuitReason }
            $failures.Add([pscustomobject]@{ Region = $region; Category = $category })
            Write-Host "::warning title=Azure pricing refresh failed::$region failed ($category)."
            if ($category -eq 'provider_schema_changed') {
                $consecutiveSchemaFailures++
                if ($consecutiveSchemaFailures -ge 3) {
                    $circuitReason = 'repeated_provider_schema_changed'
                }
            }
            else {
                $consecutiveSchemaFailures = 0
            }
        }
        else {
            $consecutiveSchemaFailures = 0
        }
        if ($null -eq $circuitReason -and $InterRegionDelaySeconds -gt 0) {
            & $DelayInvoker $InterRegionDelaySeconds
        }
    }

    return [pscustomobject]@{
        SupportedRegionCount = $regions.Count
        AttemptsUsed = $attemptsUsed
        Refreshed = @($refreshed)
        Failures = @($failures)
        Skipped = @($skipped)
        CircuitReason = $circuitReason
    }
}

function Write-AzurePricingSummary {
    param([Parameter(Mandatory)]$Result)

    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('## Azure pricing refresh')
    $lines.Add('')
    $lines.Add("- Supported regions: $($Result.SupportedRegionCount)")
    $lines.Add("- Refresh attempts: $($Result.AttemptsUsed) of $AttemptBudget maximum")
    $lines.Add("- Refreshed: $($Result.Refreshed.Count)")
    $lines.Add("- Failed: $($Result.Failures.Count)")
    $lines.Add("- Skipped by circuit breaker: $($Result.Skipped.Count)")
    if ($Result.Refreshed.Count -gt 0) {
        $lines.Add("- Successful regions: $($Result.Refreshed.Region -join ', ')")
    }
    if ($Result.Failures.Count -gt 0) {
        $lines.Add('')
        $lines.Add('### Failure categories')
        foreach ($group in @($Result.Failures | Group-Object Category | Sort-Object Name)) {
            $lines.Add("- $($group.Name): $($group.Count) region(s)")
        }
    }
    if ($null -ne $Result.CircuitReason) {
        $lines.Add("- Circuit breaker: $($Result.CircuitReason)")
    }
    $summaryPath = [Environment]::GetEnvironmentVariable('GITHUB_STEP_SUMMARY')
    if (-not [string]::IsNullOrWhiteSpace($summaryPath)) {
        $lines | Add-Content -Path $summaryPath
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    $origin = Get-ValidatedOrigin -Value $BaseUrl
    $result = Invoke-AzurePricingRefresh -Origin $origin -AttemptBudget $AttemptBudget `
        -MaxAttemptsPerRegion $MaxAttemptsPerRegion -RequestTimeoutSeconds $RequestTimeoutSeconds `
        -InterRegionDelaySeconds $InterRegionDelaySeconds
    Write-AzurePricingSummary -Result $result
    if ($result.Failures.Count -gt 0 -or $result.Skipped.Count -gt 0) {
        throw "$($result.Failures.Count) of $($result.SupportedRegionCount) Azure regions failed and $($result.Skipped.Count) were skipped."
    }
}