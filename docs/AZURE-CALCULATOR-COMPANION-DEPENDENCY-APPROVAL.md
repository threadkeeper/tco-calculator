# Azure Calculator Companion Dependency Approval

Status: **APPROVED FOR OWNER-ONLY SELF-SIGNED DEVELOPMENT DEMO**

Decision date: 2026-08-23

The authorized repository owner stated in the repository conversation that they hold or have delegated authority for Product, Architecture, Identity, Security, Privacy, Legal/Terms, Legal/OSS, Endpoint/Signing, and Operations and approved all companion decisions and dependencies below. This approval does not waive locked restore, vulnerability review, package signing, phase gates, exact-commit CI, or deployment controls.

The authorized owner expanded the development-device scope on 2026-08-23 to x64 Windows 10 version 1809 (build 17763) or later and Windows Server 2022. Microsoft documents Windows App SDK support back to Windows 10 version 1809 and support for Windows Server 2022, and documents .NET 10 x64 support for Windows Server 2022. This remains an owner-only development exception and does not authorize production or third-person distribution.

## Approved toolchain

| Tool | Exact version | Publisher and source | License/provenance | Approved use |
| --- | --- | --- | --- | --- |
| .NET SDK | `10.0.300` | Installed Microsoft SDK under `C:\Program Files\dotnet`; official source is Microsoft | Microsoft .NET, MIT; `dotnet --list-sdks` verified the installed version | Build pinned `net10.0-windows` WPF projects; do not silently roll forward to another feature band |
| .NET Windows Desktop Runtime | `10.0.8` | Installed Microsoft runtime under `C:\Program Files\dotnet` | Microsoft .NET runtime | Local execution and packaging validation; release prerequisites are declared by the signed MSIX |
| Microsoft Edge Stable | `151.0.4129.101` at review | Installed Microsoft-signed executable | `CompanyName: Microsoft Corporation`; runtime version is compatibility-checked rather than bundled | Headed anonymous automation and ordinary isolated-profile handoff |
| NuGet | Bundled with .NET SDK `10.0.300` | Microsoft .NET SDK | Official NuGet client | Locked restore only from `https://api.nuget.org/v3/index.json` |
| Azure CLI Artifact Signing extension | `1.0.0` preview | Microsoft Corporation; official Azure CLI extension index; `https://azcliprod.blob.core.windows.net/cli-extensions/artifact_signing-1.0.0-py3-none-any.whl` | MIT; minimum Azure CLI `2.75.0`; SHA-256 `bbd04ad52426e69e9a24192f2fe0e2ed2db55ae61fb91b6b8b3a0303bdfd0c7f` | Previously installed for the superseded Public Trust design; not used to build, sign, or publish the owner-only demo |
| Azure Artifact Signing | Superseded; no certificate profile | Microsoft Azure service, stable ARM API `2025-10-13` | Microsoft managed signing service | Not used by the owner-only development package; resource cleanup remains a separate explicitly authorized Azure operation |
| Windows PKI cmdlets, MakeAppx, and SignTool | Installed Windows SDK `10.0.26100` tools | Microsoft Corporation; existing Windows/PowerShell platform tools | Valid Microsoft Authenticode signatures verified locally | Create a non-exportable development key, export/import only its public certificate, package MSIX, sign by certificate thumbprint, and verify signature/hash |
| GitHub Releases | Versioned development assets in this repository | GitHub repository release service | Existing repository hosting boundary | Host the signed development MSIX, public `.cer`, hash, dependency/license/vulnerability evidence, and release notes; GitHub identity is provenance, not Windows publisher trust |

The currently available WinGet `.NET 10` SDK is `10.0.400`, published by Microsoft Corporation under MIT with installer SHA-256 `ea44e5caf1e135623dd98c6652d44ee3a9922ce3b0d1bcc2db9e28a2349b318c`. It is **not required or approved for installation by this change** because SDK `10.0.300` is already installed and sufficient. Any SDK upgrade requires a separate lock/toolchain update and validation.

## Approved direct NuGet dependencies

| Package | Exact version | Publisher/source | License | Purpose and constrained features |
| --- | --- | --- | --- | --- |
| `Microsoft.WindowsAppSDK` | `2.4.0` | Microsoft / MicrosoftReunionESTeam, NuGet.org, [source](https://github.com/microsoft/windowsappsdk) | Microsoft Windows App SDK software license terms, explicitly approved by the authorized Legal/OSS owner | Packaged URI activation and `AppInstance` single-instance routing only; no WinUI application framework |
| `Microsoft.Identity.Client` | `4.88.0` | Microsoft / AzureAD, NuGet.org, [source](https://github.com/AzureAD/microsoft-authentication-library-for-dotnet) | MIT | Public native-client token acquisition only; no confidential-client credentials, Graph permissions, embedded web view, or home-grown OAuth |
| `Microsoft.Identity.Client.Broker` | `4.88.0` | Microsoft / AzureAD, NuGet.org, same source as MSAL.NET | MIT | WAM broker, operating-system account picker, Conditional Access, and broker-owned token cache; parent every prompt to the WPF HWND |
| `Microsoft.Playwright` | `1.62.0` | Microsoft / playwright, NuGet.org, [source](https://github.com/microsoft/playwright-dotnet) | MIT | Visible-control automation using installed Edge Stable and one app-owned persistent context; do not download or bundle Playwright browsers |
| `Microsoft.NET.Test.Sdk` | `18.9.0` | Microsoft / vstest, NuGet.org, [source](https://github.com/microsoft/vstest) | MIT | Test discovery/execution only; private asset in test projects |
| `MSTest.TestAdapter` | `4.3.3` | Microsoft / MSTestFramework, NuGet.org, [source](https://github.com/microsoft/testfx) | MIT | Test adapter only; private asset in test projects |
| `MSTest.TestFramework` | `4.3.3` | Microsoft / MSTestFramework, NuGet.org, same TestFX source | MIT | Focused unit and security tests only |

All packages use reserved Microsoft-owned NuGet prefixes and current stable versions observed on 2026-08-23. Exact transitive versions and package content hashes MUST be captured in committed `packages.lock.json` files before implementation proceeds beyond restore. `dotnet package list --include-transitive --vulnerable` and license inventory MUST pass before release.

## Security, privacy, and operational review

- `Microsoft.Identity.Client` and its broker are security-sensitive. The companion uses one delegated TCO API scope, an exact API origin, redirects disabled for API calls, WAM, and no client secret. Access tokens are never logged, persisted by application code, passed to Playwright, or placed in activation URIs. MSAL/WAM may maintain its supported broker-managed cache.
- MSAL and Windows App SDK documentation disclose possible Microsoft telemetry/data collection. The application does not add custom MSAL telemetry or PII logging. Required identity exchanges remain governed by Microsoft identity service terms and the approved privacy notice.
- `Microsoft.Playwright` includes a large native/Node driver payload and can launch processes, access files, and make network requests. Release code constrains it to installed Microsoft Edge, an app-owned profile root, fixed Calculator origins, no proxy/certificate override, and no screenshot, trace, video, download, storage-state, request/response capture, or post-handoff connection. Browser download/install scripts are not run.
- Windows App SDK includes native/runtime packaging assets under custom Microsoft terms. The authorized Legal/OSS and Endpoint/Signing owner approved owner-only development packaging under those terms. Package identity, exact development publisher, non-exportable key, local trust, signature verification, GitHub Release integrity, and rollback evidence remain required before an artifact is published.
- The development certificate subject is `CN=Azure TCO Calculator Development`. It authenticates only possession of the locally generated private key; it does not verify a legal person, organization, Microsoft affiliation, or GitHub account. The private key is non-exportable in `CurrentUser\My`; only the public `.cer` may leave the certificate store.
- Test packages do not ship in the application package. No package is added to the Azure application image.

## Alternatives considered

- WPF without Windows App SDK would require custom protocol/single-instance plumbing or registry manipulation and would not follow the selected supported lifecycle API.
- WinUI 3 adds another UI/runtime surface without improving the required URI, WAM, Playwright, or process-lifecycle boundaries.
- TypeScript/Node packaging adds a second application runtime and weaker native WAM/MSIX integration.
- Browser extensions, localhost listeners, server-side browsers, normal-profile automation, and private Calculator APIs remain prohibited.

## Egress and permissions

- NuGet restore contacts only the official NuGet.org v3 endpoint and retrieves the exact approved package graph.
- Runtime API traffic is limited to the compile-time allowlisted TCO HTTPS origin and the public Azure Pricing Calculator origin. Microsoft identity traffic is owned by MSAL/WAM.
- The install link targets only this repository's fixed HTTPS GitHub Releases page or a server-selected versioned release asset. It carries no project, launch, owner, tenant, or customer data. GitHub receives ordinary download metadata such as IP address and user agent under its service terms.
- One-time certificate trust setup requires an explicitly elevated PowerShell session to import the public `.cer` into `LocalMachine\TrustedPeople`. The package and companion request no elevation, service, scheduled task, local listener, broad filesystem root, browser extension, or normal-profile access.

## Rollback

The companion remains a separate repository project and does not enter the production container. Server and web integration default disabled. Rollback disables new ticket creation, marks the affected GitHub release unavailable, removes the public certificate from `LocalMachine\TrustedPeople`, deletes the matching private key from `CurrentUser\My`, lets bounded tickets expire, and reverts focused application changes without touching saved projects, calculations, or unrelated Azure resources. The companion has no self-updater or downgrade bypass.

## Official references

- [.NET target frameworks and `net10.0-windows`](https://learn.microsoft.com/dotnet/standard/frameworks)
- [WPF desktop SDK project properties](https://learn.microsoft.com/dotnet/core/project-sdk/msbuild-props-desktop)
- [URI protocol activation and single-instance WPF handling](https://learn.microsoft.com/windows/apps/develop/launch/handle-uri-activation-dotnet)
- [MSAL.NET with WAM](https://learn.microsoft.com/entra/msal/dotnet/acquiring-tokens/desktop-mobile/wam)
- [Playwright with Microsoft Edge](https://learn.microsoft.com/microsoft-edge/playwright/)
- [Create a certificate for package signing](https://learn.microsoft.com/windows/msix/package/create-certificate-package-signing)
- [Sign an app package using SignTool](https://learn.microsoft.com/windows/msix/package/sign-app-package-using-signtool)
- [NuGet package records](https://www.nuget.org/)