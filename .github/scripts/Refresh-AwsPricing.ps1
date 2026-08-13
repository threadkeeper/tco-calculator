[CmdletBinding()]
param(
    [string]$BaseUrl = $env:TCO_CALCULATOR_URL,
    [string[]]$Regions = @(
        'eu-central-1',
        'eu-central-2',
        'eu-north-1',
        'eu-south-1',
        'eu-south-2',
        'eu-west-1',
        'eu-west-2',
        'eu-west-3'
    ),
    [ValidateRange(1, 3)]
    [int]$MaxAttemptsPerRegion = 3,
    [ValidateRange(0, 8)]
    [int]$MaxRetainedSnapshots = 2,
    [ValidateRange(30, 900)]
    [int]$RequestTimeoutSeconds = 900,
    [ValidateRange(0, 120)]
    [int]$InterAttemptDelaySeconds = 5,
    [ValidateRange(0, 30)]
    [int]$InterRegionDelaySeconds = 1
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-ValidatedAwsOrigin {
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

function Invoke-DefaultAwsHttpRequest {
    param([Parameter(Mandatory)][pscustomobject]$Request)

    $response = Invoke-WebRequest `
        -Uri $Request.Uri `
        -Method Post `
        -ContentType 'application/json' `
        -Body $Request.Body `
        -TimeoutSec $Request.TimeoutSeconds `
        -SkipHttpErrorCheck
    return [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        Headers = $response.Headers
        Content = $response.Content
    }
}

function Invoke-DefaultAwsDelay {
    param([Parameter(Mandatory)][int]$Seconds)

    if ($Seconds -gt 0) {
        Start-Sleep -Seconds $Seconds
    }
}

function Get-AwsRetryAfterSeconds {
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
    $seconds = 0
    if ([int]::TryParse([string]@($raw)[0], [ref]$seconds) -and $seconds -ge 0) {
        return $seconds
    }
    return $null
}

function New-AwsRefreshOutcome {
    param(
        [Parameter(Mandatory)][bool]$IsFresh,
        [Parameter(Mandatory)][bool]$IsUsable,
        [Parameter(Mandatory)][string]$Category,
        [AllowNull()][string]$Status = $null,
        [bool]$Retryable = $false,
        [AllowNull()][Nullable[int]]$RetryAfterSeconds = $null,
        [AllowNull()][string]$RetrievedAt = $null
    )

    return [pscustomobject]@{
        IsFresh = $IsFresh
        IsUsable = $IsUsable
        Category = $Category
        Status = $Status
        Retryable = $Retryable
        RetryAfterSeconds = $RetryAfterSeconds
        RetrievedAt = $RetrievedAt
    }
}

function ConvertTo-AwsRefreshOutcome {
    param([Parameter(Mandatory)]$Response)

    $statusCode = [int]$Response.StatusCode
    if ($statusCode -eq 408 -or $statusCode -eq 429 -or $statusCode -ge 500) {
        return New-AwsRefreshOutcome -IsFresh $false -IsUsable $false -Category "http_$statusCode" `
            -Retryable $true -RetryAfterSeconds (Get-AwsRetryAfterSeconds -Headers $Response.Headers)
    }
    if ($statusCode -lt 200 -or $statusCode -ge 300) {
        return New-AwsRefreshOutcome -IsFresh $false -IsUsable $false -Category "http_$statusCode"
    }

    try {
        $payload = $Response.Content | ConvertFrom-Json
    }
    catch {
        return New-AwsRefreshOutcome -IsFresh $false -IsUsable $false -Category 'invalid_response'
    }
    $statusProperty = $payload.PSObject.Properties['status']
    $snapshotProperty = $payload.PSObject.Properties['snapshot_id']
    $retrievedProperty = $payload.PSObject.Properties['retrieved_at']
    if ($null -eq $statusProperty) {
        return New-AwsRefreshOutcome -IsFresh $false -IsUsable $false -Category 'invalid_response'
    }

    $status = [string]$statusProperty.Value
    $snapshotId = if ($null -eq $snapshotProperty) { $null } else { [string]$snapshotProperty.Value }
    $retrievedAt = if ($null -eq $retrievedProperty) { $null } else { [string]$retrievedProperty.Value }
    $hasSnapshot = -not [string]::IsNullOrWhiteSpace($snapshotId)
    if ($status -eq 'fresh' -and $hasSnapshot) {
        return New-AwsRefreshOutcome -IsFresh $true -IsUsable $true -Category 'fresh' `
            -Status $status -RetrievedAt $retrievedAt
    }

    $warningProperty = $payload.PSObject.Properties['warnings']
    $warningText = if ($null -eq $warningProperty) { '' } else { @($warningProperty.Value) -join ' ' }
    $reasonCodes = @([regex]::Matches(
        $warningText,
        '\b(?:provider_[a-z_]+|price_not_found|scope_unsupported)\b'
    ) | ForEach-Object { $_.Value } | Sort-Object -Unique)
    $category = if ($reasonCodes.Count -gt 0) { $reasonCodes[0] } else { "status_$status" }
    if ($status -in @('cached', 'stale') -and $hasSnapshot) {
        return New-AwsRefreshOutcome -IsFresh $false -IsUsable $true -Category $category `
            -Status $status -RetrievedAt $retrievedAt
    }
    return New-AwsRefreshOutcome -IsFresh $false -IsUsable $false -Category $category `
        -Status $status -Retryable ($status -eq 'unavailable')
}

function Invoke-AwsPricingRefresh {
    param(
        [Parameter(Mandatory)][Uri]$Origin,
        [Parameter(Mandatory)][string[]]$Regions,
        [int]$MaxAttemptsPerRegion = 3,
        [int]$RequestTimeoutSeconds = 900,
        [int]$InterAttemptDelaySeconds = 5,
        [int]$InterRegionDelaySeconds = 1,
        [scriptblock]$RequestInvoker = ${function:Invoke-DefaultAwsHttpRequest},
        [scriptblock]$DelayInvoker = ${function:Invoke-DefaultAwsDelay}
    )

    $refreshEndpoint = [Uri]::new($Origin, '/api/v1/pricing/aws/refresh')
    $refreshed = [System.Collections.Generic.List[object]]::new()
    $retained = [System.Collections.Generic.List[object]]::new()
    $failures = [System.Collections.Generic.List[object]]::new()
    $attemptsUsed = 0

    foreach ($region in $Regions) {
        $body = @{
            aws_region = $region
            azure_region = 'swedencentral'
            currency = 'USD'
            resources = @()
        } | ConvertTo-Json -Compress
        $freshOutcome = $null
        $fallbackOutcome = $null
        $finalOutcome = $null
        $attemptsForRegion = 0

        for ($attempt = 1; $attempt -le $MaxAttemptsPerRegion; $attempt++) {
            $attemptsUsed++
            $attemptsForRegion = $attempt
            try {
                $response = & $RequestInvoker ([pscustomobject]@{
                    Uri = $refreshEndpoint
                    Body = $body
                    TimeoutSeconds = $RequestTimeoutSeconds
                })
                $outcome = ConvertTo-AwsRefreshOutcome -Response $response
            }
            catch {
                $outcome = New-AwsRefreshOutcome -IsFresh $false -IsUsable $false `
                    -Category 'transport_error' -Retryable $true
            }
            $finalOutcome = $outcome
            if ($outcome.IsFresh) {
                $freshOutcome = $outcome
                break
            }
            if ($outcome.IsUsable) {
                $fallbackOutcome = $outcome
            }
            elseif (-not $outcome.Retryable) {
                break
            }
            if ($attempt -lt $MaxAttemptsPerRegion) {
                $delay = if ($null -ne $outcome.RetryAfterSeconds) {
                    [Math]::Min(120, $outcome.RetryAfterSeconds)
                }
                else {
                    $InterAttemptDelaySeconds
                }
                Write-Warning "AWS pricing refresh for $region returned $($outcome.Category); retrying attempt $($attempt + 1)."
                & $DelayInvoker ([int]$delay)
            }
        }

        if ($null -ne $freshOutcome) {
            $refreshed.Add([pscustomobject]@{
                Region = $region
                RetrievedAt = $freshOutcome.RetrievedAt
                Attempts = $attemptsForRegion
            })
            Write-Host "Refreshed $region at $($freshOutcome.RetrievedAt) after $attemptsForRegion attempt(s)."
        }
        elseif ($null -ne $fallbackOutcome) {
            $retained.Add([pscustomobject]@{
                Region = $region
                Status = $fallbackOutcome.Status
                Category = $fallbackOutcome.Category
                RetrievedAt = $fallbackOutcome.RetrievedAt
                Attempts = $attemptsForRegion
            })
            Write-Host "::warning title=AWS pricing snapshot retained::$region used its existing $($fallbackOutcome.Status) snapshot after $attemptsForRegion attempt(s) ($($fallbackOutcome.Category))."
        }
        else {
            $category = if ($null -eq $finalOutcome) { 'not_attempted' } else { $finalOutcome.Category }
            $failures.Add([pscustomobject]@{ Region = $region; Category = $category })
            Write-Host "::error title=AWS pricing snapshot unavailable::$region has no usable snapshot ($category)."
        }

        if ($InterRegionDelaySeconds -gt 0) {
            & $DelayInvoker $InterRegionDelaySeconds
        }
    }

    return [pscustomobject]@{
        RegionCount = $Regions.Count
        AttemptsUsed = $attemptsUsed
        Refreshed = @($refreshed)
        Retained = @($retained)
        Failures = @($failures)
    }
}

function Write-AwsPricingSummary {
    param(
        [Parameter(Mandatory)]$Result,
        [Parameter(Mandatory)][int]$AllowedRetainedSnapshots
    )

    $lines = [System.Collections.Generic.List[string]]::new()
    $lines.Add('## AWS pricing refresh')
    $lines.Add('')
    $lines.Add("- Regions: $($Result.RegionCount)")
    $lines.Add("- Attempts: $($Result.AttemptsUsed)")
    $lines.Add("- Refreshed: $($Result.Refreshed.Count)")
    $lines.Add("- Existing usable snapshots retained: $($Result.Retained.Count) of $AllowedRetainedSnapshots allowed")
    $lines.Add("- No usable snapshot: $($Result.Failures.Count)")
    if ($Result.Refreshed.Count -gt 0) {
        $lines.Add("- Successful regions: $($Result.Refreshed.Region -join ', ')")
    }
    if ($Result.Retained.Count -gt 0) {
        $lines.Add('')
        $lines.Add('### Retained snapshot warnings')
        foreach ($item in $Result.Retained) {
            $lines.Add("- $($item.Region): $($item.Status), retrieved $($item.RetrievedAt), $($item.Category)")
        }
    }
    if ($Result.Failures.Count -gt 0) {
        $lines.Add('')
        $lines.Add('### Unavailable regions')
        foreach ($item in $Result.Failures) {
            $lines.Add("- $($item.Region): $($item.Category)")
        }
    }
    $summaryPath = [Environment]::GetEnvironmentVariable('GITHUB_STEP_SUMMARY')
    if (-not [string]::IsNullOrWhiteSpace($summaryPath)) {
        $lines | Add-Content -Path $summaryPath
    }
}

function Assert-AwsPricingRefreshResult {
    param(
        [Parameter(Mandatory)]$Result,
        [Parameter(Mandatory)][int]$MaxRetainedSnapshots
    )

    if ($Result.Failures.Count -gt 0) {
        throw "$($Result.Failures.Count) of $($Result.RegionCount) AWS regions have no usable snapshot."
    }
    if ($Result.Retained.Count -gt $MaxRetainedSnapshots) {
        throw "$($Result.Retained.Count) of $($Result.RegionCount) AWS regions retained existing snapshots; the allowed maximum is $MaxRetainedSnapshots."
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    $origin = Get-ValidatedAwsOrigin -Value $BaseUrl
    $result = Invoke-AwsPricingRefresh -Origin $origin -Regions $Regions `
        -MaxAttemptsPerRegion $MaxAttemptsPerRegion -RequestTimeoutSeconds $RequestTimeoutSeconds `
        -InterAttemptDelaySeconds $InterAttemptDelaySeconds -InterRegionDelaySeconds $InterRegionDelaySeconds
    Write-AwsPricingSummary -Result $result -AllowedRetainedSnapshots $MaxRetainedSnapshots
    Assert-AwsPricingRefreshResult -Result $result -MaxRetainedSnapshots $MaxRetainedSnapshots
}