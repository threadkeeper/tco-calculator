[CmdletBinding(SupportsShouldProcess, ConfirmImpact = 'High')]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$subject = 'CN=Azure TCO Calculator Development'
$friendlyName = 'Azure TCO Calculator Companion Development Signing'
$isAdministrator = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdministrator) {
    throw 'Run this script from an elevated PowerShell session to remove LocalMachine\TrustedPeople trust.'
}

$personalCertificates = @(
    Get-ChildItem -Path Cert:\CurrentUser\My |
        Where-Object { $_.Subject -eq $subject -and $_.FriendlyName -eq $friendlyName }
)
$thumbprints = @($personalCertificates.Thumbprint)
$trustedCertificates = @(
    Get-ChildItem -Path Cert:\LocalMachine\TrustedPeople |
        Where-Object { $thumbprints -contains $_.Thumbprint }
)

foreach ($certificate in $trustedCertificates) {
    if ($PSCmdlet.ShouldProcess($certificate.Thumbprint, 'Remove development trust from LocalMachine\TrustedPeople')) {
        Remove-Item -LiteralPath $certificate.PSPath -Force
    }
}

foreach ($certificate in $personalCertificates) {
    if ($PSCmdlet.ShouldProcess($certificate.Thumbprint, 'Delete development signing certificate and private key from CurrentUser\My')) {
        Remove-Item -LiteralPath $certificate.PSPath -Force
    }
}