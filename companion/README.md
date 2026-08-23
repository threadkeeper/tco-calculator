# Azure TCO Calculator Companion

This directory contains the separately installed Windows companion for attended transfer of server-authoritative estimates to the public Azure Pricing Calculator.

The solution targets pinned .NET 10 and restores only the locked dependency graph from the repository-owned `NuGet.config`. It uses installed Microsoft Edge and must not download Playwright browsers.

The owner-only x64 package supports Windows 10 version 1809 (build 17763) or later and Windows Server 2022. It is not approved for another person's device or for production distribution.

## Local validation

```powershell
Set-Location companion
dotnet restore AzureTcoCalculator.Companion.sln --locked-mode --configfile NuGet.config
dotnet build AzureTcoCalculator.Companion.sln --configuration Release --no-restore
dotnet test AzureTcoCalculator.Companion.sln --configuration Release --no-build
dotnet package list --project AzureTcoCalculator.Companion.sln --vulnerable --include-transitive --format json --no-restore
```

The independent `Companion CI` GitHub workflow performs these checks on Windows. It does not receive a signing key or sign the application. GitHub identity, OIDC, release authorship, and artifact attestations can establish repository/build provenance, but they do not provide an Authenticode certificate trusted by Windows App Installer.

## Owner-only development package

The approved demo uses a locally generated self-signed certificate. Run the one-time initializer from an elevated PowerShell session:

```powershell
Set-Location companion
.\scripts\Initialize-DevSigning.ps1
```

The initializer creates a non-exportable code-signing private key in `CurrentUser\My`, exports only its public `.cer` under the current user's local application-data directory, and imports that public certificate into `LocalMachine\TrustedPeople`. It never creates a PFX.

After locked restore and validation, build and sign the package from a normal PowerShell session:

```powershell
.\scripts\Build-DevMsix.ps1 -Sign
```

Generated MSIX and SHA-256 files remain under ignored `companion/artifacts`. The package identity ends in `.Dev`, uses publisher `CN=Azure TCO Calculator Development`, runs `asInvoker`, and is trusted only on machines where the matching public certificate was explicitly installed. The certificate label is not a verified GitHub identity. Do not distribute this owner-only package to another person.

To retire the demo, uninstall the app and run the rollback from elevated PowerShell:

```powershell
.\scripts\Remove-DevSigningCertificate.ps1
```

The companion does not invoke `ms-appinstaller:`, alter sideloading policy, install a root CA, or include a self-updater. A device whose Windows or enterprise policy blocks certificate trust or installation is unsupported.
