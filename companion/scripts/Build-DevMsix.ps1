[CmdletBinding()]
param(
    [string] $OutputDirectory,
  [Parameter(Mandatory)]
  [ValidatePattern('^https://[a-z0-9.-]+\.azurecontainerapps\.io$')]
  [string] $ApiOrigin,
  [Parameter(Mandatory)]
  [ValidatePattern('^[0-9a-fA-F-]{36}$')]
  [string] $CompanionClientId,
  [Parameter(Mandatory)]
  [ValidatePattern('^api://\S+$')]
  [string] $ApiScope,
    [switch] $Sign
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Find-WindowsSdkTool {
    param(
        [Parameter(Mandatory)]
        [string] $Name
    )

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

function New-SolidPng {
    param(
        [Parameter(Mandatory)]
        [string] $Path,
        [Parameter(Mandatory)]
        [int] $Size
    )

    Add-Type -AssemblyName PresentationCore
    $stride = $Size * 4
    $pixels = [byte[]]::new($stride * $Size)
    for ($offset = 0; $offset -lt $pixels.Length; $offset += 4) {
        $pixels[$offset] = 0x56
        $pixels[$offset + 1] = 0x45
        $pixels[$offset + 2] = 0x00
        $pixels[$offset + 3] = 0xff
    }

    $bitmap = [Windows.Media.Imaging.WriteableBitmap]::new(
        $Size,
        $Size,
        96,
        96,
        [Windows.Media.PixelFormats]::Bgra32,
        $null
    )
    $bitmap.WritePixels([Windows.Int32Rect]::new(0, 0, $Size, $Size), $pixels, $stride, 0)
    $encoder = [Windows.Media.Imaging.PngBitmapEncoder]::new()
    $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
    $stream = [IO.File]::Open($Path, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $encoder.Save($stream)
    }
    finally {
        $stream.Dispose()
    }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$projectPath = Join-Path $repoRoot 'companion\src\AzureTcoCalculator.Companion\AzureTcoCalculator.Companion.csproj'
$versionText = (Get-Content -LiteralPath (Join-Path $repoRoot 'VERSION') -Raw).Trim()
if ($versionText -notmatch '^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)$') {
    throw "VERSION '$versionText' is not a three-part numeric version."
}

$versionParts = @([int] $Matches.major, [int] $Matches.minor, [int] $Matches.patch)
if (@($versionParts | Where-Object { $_ -gt 65535 }).Count -ne 0) {
    throw 'Each MSIX version component must be at most 65535.'
}

$msixVersion = "$($versionParts[0]).$($versionParts[1]).$($versionParts[2]).0"
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot 'companion\artifacts'
}
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$stagingRoot = Join-Path $OutputDirectory 'staging'
$publishDirectory = Join-Path $stagingRoot 'app'
$assetsDirectory = Join-Path $publishDirectory 'Assets'
$packagePath = Join-Path $OutputDirectory "AzureTcoCalculator.Companion.$msixVersion.win-x64.dev.msix"

if (Test-Path -LiteralPath $stagingRoot) {
    Remove-Item -LiteralPath $stagingRoot -Recurse -Force
}
New-Item -ItemType Directory -Path $assetsDirectory -Force | Out-Null

& dotnet publish $projectPath `
    --configuration Release `
    --self-contained false `
    --no-restore `
    -p:WindowsAppSDKSelfContained=true `
    -p:CalculatorApiOrigin=$ApiOrigin `
    -p:CalculatorCompanionClientId=$CompanionClientId `
    -p:CalculatorApiScope=$ApiScope `
    --output $publishDirectory
if ($LASTEXITCODE -ne 0) {
    throw 'The locked companion publish failed.'
}

Get-ChildItem -LiteralPath $publishDirectory -Filter '*.pdb' -File -Recurse |
    Remove-Item -Force

$playwrightNodeDirectory = Join-Path $publishDirectory '.playwright\node'
if (-not (Test-Path -LiteralPath (Join-Path $playwrightNodeDirectory 'win32_x64\node.exe') -PathType Leaf)) {
  throw 'The published Playwright driver does not contain the required Windows x64 Node executable.'
}
Get-ChildItem -LiteralPath $playwrightNodeDirectory -Directory |
  Where-Object { $_.Name -ne 'win32_x64' } |
  Remove-Item -Recurse -Force

New-SolidPng -Path (Join-Path $assetsDirectory 'StoreLogo.png') -Size 50
New-SolidPng -Path (Join-Path $assetsDirectory 'Square44x44Logo.png') -Size 44
New-SolidPng -Path (Join-Path $assetsDirectory 'Square150x150Logo.png') -Size 150

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<Package
  xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">
  <Identity
    Name="AzureTcoCalculator.Companion.Dev"
    Publisher="CN=Azure TCO Calculator Development"
    Version="$msixVersion"
    ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Azure TCO Calculator Companion (Development)</DisplayName>
    <PublisherDisplayName>Azure TCO Calculator Development</PublisherDisplayName>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" />
  </Dependencies>
  <Resources>
    <Resource Language="en-us" />
  </Resources>
  <Applications>
    <Application Id="App" Executable="AzureTcoCalculator.Companion.exe" EntryPoint="Windows.FullTrustApplication">
      <uap:VisualElements
        DisplayName="Azure TCO Calculator Companion"
        Description="Attended Azure Pricing Calculator transfer companion"
        BackgroundColor="#004556"
        Square44x44Logo="Assets\Square44x44Logo.png"
        Square150x150Logo="Assets\Square150x150Logo.png" />
      <Extensions>
        <uap:Extension Category="windows.protocol" Executable="AzureTcoCalculator.Companion.exe" EntryPoint="Windows.FullTrustApplication">
          <uap:Protocol Name="azure-tco-calculator">
            <uap:DisplayName>Azure TCO Calculator Companion</uap:DisplayName>
          </uap:Protocol>
        </uap:Extension>
      </Extensions>
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
"@

Set-Content -LiteralPath (Join-Path $publishDirectory 'AppxManifest.xml') -Value $manifest -Encoding utf8NoBOM
$makeAppx = Find-WindowsSdkTool -Name 'MakeAppx.exe'
& $makeAppx pack /d $publishDirectory /p $packagePath /o /h SHA256 | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'MakeAppx failed to create the development MSIX package.'
}

if ($Sign) {
    & (Join-Path $PSScriptRoot 'Sign-DevMsix.ps1') -MsixPath $packagePath | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'Development MSIX signing failed.'
    }
}

$hash = Get-FileHash -LiteralPath $packagePath -Algorithm SHA256
$hashPath = "$packagePath.sha256"
Set-Content -LiteralPath $hashPath -Value "$($hash.Hash.ToLowerInvariant())  $([IO.Path]::GetFileName($packagePath))" -Encoding ascii

[pscustomobject]@{
    PackagePath = $packagePath
    Sha256Path = $hashPath
    Version = $msixVersion
    Signed = [bool] $Sign
    Publisher = 'CN=Azure TCO Calculator Development'
    TrustModel = 'Owner-only self-signed development certificate'
}