[CmdletBinding(SupportsShouldProcess)]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string] $MsixPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Find-WindowsSdkTool {
    param(
        [Parameter(Mandatory)]
        [string] $Name
    )

    $command = Get-Command $Name -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $command) {
        return $command.Source
    }

    $kitsRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $tool = Get-ChildItem -Path $kitsRoot -Filter $Name -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.DirectoryName -match '[\\/]x64$' } |
        Sort-Object -Property FullName -Descending |
        Select-Object -First 1
    if ($null -eq $tool) {
        throw "$Name was not found in the installed Windows SDK."
    }

    $signature = Get-AuthenticodeSignature -FilePath $tool.FullName
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft') {
        throw "$Name is not signed by Microsoft with a valid signature."
    }

    return $tool.FullName
}

$resolvedMsixPath = (Resolve-Path -LiteralPath $MsixPath).Path
if ([IO.Path]::GetExtension($resolvedMsixPath) -cne '.msix') {
    throw 'Development signing accepts only an .msix package.'
}

$subject = 'CN=Azure TCO Calculator Development'
$codeSigningOid = '1.3.6.1.5.5.7.3.3'
$now = Get-Date
$certificate = @(
    Get-ChildItem -Path Cert:\CurrentUser\My |
        Where-Object {
            $_.Subject -eq $subject -and
            $_.FriendlyName -eq 'Azure TCO Calculator Companion Development Signing' -and
            $_.HasPrivateKey -and
            $_.NotBefore -le $now -and
            $_.NotAfter -gt $now -and
            @($_.EnhancedKeyUsageList.ObjectId) -contains $codeSigningOid
        } |
        Sort-Object -Property NotAfter -Descending
) | Select-Object -First 1

if ($null -eq $certificate) {
    throw "No valid development signing certificate was found for '$subject'."
}
$trusted = Get-ChildItem -Path Cert:\LocalMachine\TrustedPeople |
    Where-Object { $_.Thumbprint -eq $certificate.Thumbprint } |
    Select-Object -First 1
if ($null -eq $trusted) {
    throw 'The development signing certificate is not trusted in LocalMachine\TrustedPeople. Run Initialize-DevSigning.ps1 from elevated PowerShell.'
}

$makeAppx = Find-WindowsSdkTool -Name 'MakeAppx.exe'
$signTool = Find-WindowsSdkTool -Name 'SignTool.exe'
$inspectionDirectory = Join-Path ([IO.Path]::GetTempPath()) "tco-msix-$([Guid]::NewGuid().ToString('N'))"

try {
    & $makeAppx unpack /p $resolvedMsixPath /d $inspectionDirectory /o | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'MakeAppx could not validate and unpack the MSIX package.'
    }

    [xml] $manifest = Get-Content -LiteralPath (Join-Path $inspectionDirectory 'AppxManifest.xml') -Raw
    $publisher = [string] $manifest.Package.Identity.Publisher
    if ($publisher -ne $certificate.Subject) {
        throw "The MSIX publisher '$publisher' does not match the development certificate subject '$($certificate.Subject)'."
    }

    if (-not $PSCmdlet.ShouldProcess($resolvedMsixPath, "Sign with $($certificate.Subject)")) {
        return
    }

    & $signTool sign /fd SHA256 /sha1 $certificate.Thumbprint /s My $resolvedMsixPath | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'SignTool failed to sign the MSIX package.'
    }

    & $signTool verify /pa $resolvedMsixPath | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'SignTool could not verify the signed MSIX package.'
    }

    $hash = Get-FileHash -LiteralPath $resolvedMsixPath -Algorithm SHA256
    [pscustomobject]@{
        PackagePath = $resolvedMsixPath
        Publisher = $certificate.Subject
        CertificateThumbprint = $certificate.Thumbprint
        Sha256 = $hash.Hash.ToLowerInvariant()
        TrustScope = 'LocalMachine\TrustedPeople'
        GitHubIdentityVerified = $false
    }
}
finally {
    if (Test-Path -LiteralPath $inspectionDirectory) {
        Remove-Item -LiteralPath $inspectionDirectory -Recurse -Force
    }
}