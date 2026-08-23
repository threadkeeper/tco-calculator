[CmdletBinding(SupportsShouldProcess)]
param(
    [string] $CertificateOutputPath = (Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'AzureTcoCalculator\dev-signing.cer')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$subject = 'CN=Azure TCO Calculator Development'
$friendlyName = 'Azure TCO Calculator Companion Development Signing'
$codeSigningOid = '1.3.6.1.5.5.7.3.3'
$now = Get-Date
$isAdministrator = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdministrator) {
    throw 'Run this script from an elevated PowerShell session to trust the public certificate in LocalMachine\TrustedPeople.'
}

$certificate = @(
    Get-ChildItem -Path Cert:\CurrentUser\My |
        Where-Object {
            $_.Subject -eq $subject -and
            $_.FriendlyName -eq $friendlyName -and
            $_.HasPrivateKey -and
            $_.NotBefore -le $now -and
            $_.NotAfter -gt $now.AddDays(30) -and
            @($_.EnhancedKeyUsageList.ObjectId) -contains $codeSigningOid
        } |
        Sort-Object -Property NotAfter -Descending
) | Select-Object -First 1

if ($null -eq $certificate) {
    if (-not $PSCmdlet.ShouldProcess($subject, 'Create a non-exportable development code-signing certificate')) {
        return
    }

    $certificate = New-SelfSignedCertificate `
        -Type Custom `
        -Subject $subject `
        -FriendlyName $friendlyName `
        -CertStoreLocation 'Cert:\CurrentUser\My' `
        -KeyAlgorithm RSA `
        -KeyLength 3072 `
        -HashAlgorithm SHA256 `
        -KeyExportPolicy NonExportable `
        -KeyUsage DigitalSignature `
        -TextExtension @(
            '2.5.29.37={text}1.3.6.1.5.5.7.3.3',
            '2.5.29.19={text}'
        ) `
        -NotAfter $now.AddYears(1)
}

$certificateDirectory = Split-Path -Parent $CertificateOutputPath
if (-not (Test-Path -LiteralPath $certificateDirectory -PathType Container)) {
    New-Item -ItemType Directory -Path $certificateDirectory -Force | Out-Null
}

Export-Certificate -Cert $certificate -FilePath $CertificateOutputPath -Force | Out-Null
Import-Certificate -FilePath $CertificateOutputPath -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople' | Out-Null

$trustedCertificate = Get-ChildItem -Path Cert:\LocalMachine\TrustedPeople |
    Where-Object { $_.Thumbprint -eq $certificate.Thumbprint } |
    Select-Object -First 1

if ($null -eq $trustedCertificate) {
    throw 'The development signing certificate was not found in LocalMachine\TrustedPeople after import.'
}

[pscustomobject]@{
    Subject = $certificate.Subject
    Thumbprint = $certificate.Thumbprint
    NotAfter = $certificate.NotAfter
    PublicCertificatePath = $CertificateOutputPath
    PrivateKeyExportable = $false
    TrustedStore = 'LocalMachine\TrustedPeople'
}