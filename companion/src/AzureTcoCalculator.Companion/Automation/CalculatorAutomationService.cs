using System.Diagnostics;
using System.IO;
using Microsoft.Playwright;

namespace AzureTcoCalculator.Companion.Automation;

public sealed class CalculatorAutomationService
{
    private const string CalculatorUrl = "https://azure.microsoft.com/en-us/pricing/calculator/";

    public async Task RunAsync(
        CalculatorAutomationPlan plan,
        Action<string> reportStatus,
        Action<bool> reportOrdinaryEdgeRunning,
        CancellationToken cancellationToken)
    {
        string edgeExecutable = FindEdgeExecutable();
        OwnedEdgeProfile profile = OwnedEdgeProfile.Create();
        IBrowserContext? context = null;
        try
        {
            reportStatus("Opening an isolated Microsoft Edge window...");
            using IPlaywright playwright = await Playwright.CreateAsync().ConfigureAwait(true);
            context = await playwright.Chromium.LaunchPersistentContextAsync(
                profile.Path,
                new BrowserTypeLaunchPersistentContextOptions
                {
                    AcceptDownloads = false,
                    Channel = "msedge",
                    Headless = false,
                    Locale = "en-US"
                }).ConfigureAwait(true);
            IPage page = context.Pages.FirstOrDefault() ?? await context.NewPageAsync().ConfigureAwait(true);
            await page.GotoAsync(
                CalculatorUrl,
                new PageGotoOptions { WaitUntil = WaitUntilState.DOMContentLoaded, Timeout = 60_000 })
                .ConfigureAwait(true);
            await AssertCalculatorPageAsync(page).ConfigureAwait(true);

            reportStatus($"Creating {plan.Items.Count} SQL Managed Instance lines...");
            await FillAndVerifyAsync(page.GetByPlaceholder("Your Estimate").First, "Azure TCO Estimate")
                .ConfigureAwait(true);
            await CreateItemsAsync(page, plan.Items).ConfigureAwait(true);
            await VerifyAsync(page, plan.Items).ConfigureAwait(true);

            reportStatus("Reloading the Calculator and verifying every value...");
            await page.ReloadAsync(
                new PageReloadOptions { WaitUntil = WaitUntilState.DOMContentLoaded, Timeout = 60_000 })
                .ConfigureAwait(true);
            await AssertCalculatorPageAsync(page).ConfigureAwait(true);
            await ExpectValueAsync(page.GetByPlaceholder("Your Estimate").First, "Azure TCO Estimate")
                .ConfigureAwait(true);
            await VerifyAsync(page, plan.Items).ConfigureAwait(true);

            await context.CloseAsync().ConfigureAwait(true);
            context = null;

            reportStatus("The estimate is ready. Use the ordinary Edge window to sign in or save it, then close Edge to finish cleanup.");
            using Process edge = StartOrdinaryEdge(edgeExecutable, profile.Path);
            reportOrdinaryEdgeRunning(true);
            try
            {
                await edge.WaitForExitAsync(cancellationToken).ConfigureAwait(true);
                if (edge.ExitCode != 0)
                {
                    throw new InvalidOperationException("Microsoft Edge did not close normally.");
                }
            }
            finally
            {
                reportOrdinaryEdgeRunning(false);
            }
        }
        catch (PlaywrightException)
        {
            throw new InvalidOperationException(
                "The Azure Pricing Calculator changed or could not be configured. No estimate was handed off.");
        }
        finally
        {
            if (context is not null)
            {
                await context.CloseAsync().ConfigureAwait(true);
            }
            await profile.DeleteAsync().ConfigureAwait(true);
        }
    }

    private static async Task CreateItemsAsync(
        IPage page,
        IReadOnlyList<CalculatorAutomationItem> items)
    {
        ILocator search = page.Locator("input[aria-label=\"Search products\"]:visible").First;
        await search.FillAsync("SQL Managed Instance").ConfigureAwait(true);
        ILocator add = page.GetByRole(AriaRole.Button, new() { Name = "Add to estimate", Exact = true }).First;
        await add.WaitForAsync(new() { State = WaitForSelectorState.Visible, Timeout = 30_000 }).ConfigureAwait(true);
        for (int index = 0; index < items.Count; index++)
        {
            await add.ClickAsync().ConfigureAwait(true);
            ILocator module = page.Locator("[data-testid=\"azure-sql-module\"]").Nth(index);
            await module.WaitForAsync(new() { State = WaitForSelectorState.Visible, Timeout = 30_000 }).ConfigureAwait(true);
            await ConfigureAsync(module, items[index]).ConfigureAwait(true);
        }
    }

    private static async Task ConfigureAsync(ILocator module, CalculatorAutomationItem item)
    {
        await FillAndVerifyAsync(module.Locator("input[name=\"displayName\"]"), item.DisplayName).ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"region\"]"), item.Region).ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"vcoreTier\"]"), "next-gen-general-purpose").ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"generation\"]"), "premium-series").ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"instanceSize\"]"), item.Vcores).ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"ramMemory\"]"), item.MemoryGb).ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"recovery\"]"), "primaryinstance").ConfigureAwait(true);
        await SelectAndVerifyAsync(module.Locator("select[name=\"zoneRedundancy\"]"), "local").ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"managedCount\"]"), item.Quantity).ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"hours\"]"), item.HoursPerMonth).ConfigureAwait(true);
        await CheckAndVerifyAsync(module.Locator("input[name$=\"-databaseBillingOption\"][value=\"payg\"]")).ConfigureAwait(true);
        await CheckAndVerifyAsync(module.Locator("input[name$=\"-softwareBillingOption\"][value=\"payg\"]")).ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"managedStorageUnits\"]"), item.StorageUnits).ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"additionalIopsSize\"]"), "0").ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"backupStorageSize\"]"), item.BackupStorageGb).ConfigureAwait(true);
        await FillAndVerifyAsync(module.Locator("input[name=\"ltrDatabaseSize\"]"), "0").ConfigureAwait(true);
    }

    private static async Task VerifyAsync(
        IPage page,
        IReadOnlyList<CalculatorAutomationItem> items)
    {
        ILocator modules = page.Locator("[data-testid=\"azure-sql-module\"]");
        if (await modules.CountAsync().ConfigureAwait(true) != items.Count)
        {
            throw new InvalidOperationException("The Calculator did not retain every SQL Managed Instance line.");
        }
        for (int index = 0; index < items.Count; index++)
        {
            ILocator module = modules.Nth(index);
            CalculatorAutomationItem item = items[index];
            await ExpectValueAsync(module.Locator("input[name=\"displayName\"]"), item.DisplayName).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"region\"]"), item.Region).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"vcoreTier\"]"), "next-gen-general-purpose").ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"generation\"]"), "premium-series").ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"instanceSize\"]"), item.Vcores).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"ramMemory\"]"), item.MemoryGb).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"recovery\"]"), "primaryinstance").ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("select[name=\"zoneRedundancy\"]"), "local").ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"managedCount\"]"), item.Quantity).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"hours\"]"), item.HoursPerMonth).ConfigureAwait(true);
            await ExpectCheckedAsync(module.Locator("input[name$=\"-databaseBillingOption\"][value=\"payg\"]")).ConfigureAwait(true);
            await ExpectCheckedAsync(module.Locator("input[name$=\"-softwareBillingOption\"][value=\"payg\"]")).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"managedStorageUnits\"]"), item.StorageUnits).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"additionalIopsSize\"]"), "0").ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"backupStorageSize\"]"), item.BackupStorageGb).ConfigureAwait(true);
            await ExpectValueAsync(module.Locator("input[name=\"ltrDatabaseSize\"]"), "0").ConfigureAwait(true);
        }
    }

    private static async Task AssertCalculatorPageAsync(IPage page)
    {
        Uri location = new(page.Url);
        if (location.Scheme != Uri.UriSchemeHttps
            || location.Host != "azure.microsoft.com"
            || !location.AbsolutePath.StartsWith("/en-us/pricing/calculator", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Calculator navigation reached an unexpected page.");
        }
        await page.Locator("input[aria-label=\"Search products\"]:visible").First
            .WaitForAsync(new() { State = WaitForSelectorState.Visible, Timeout = 30_000 })
            .ConfigureAwait(true);
    }

    private static async Task FillAndVerifyAsync(ILocator locator, string value)
    {
        await locator.FillAsync(value).ConfigureAwait(true);
        await locator.PressAsync("Tab").ConfigureAwait(true);
        await ExpectValueAsync(locator, value).ConfigureAwait(true);
    }

    private static async Task SelectAndVerifyAsync(ILocator locator, string value)
    {
        await locator.SelectOptionAsync(value).ConfigureAwait(true);
        await ExpectValueAsync(locator, value).ConfigureAwait(true);
    }

    private static async Task CheckAndVerifyAsync(ILocator locator)
    {
        await locator.CheckAsync().ConfigureAwait(true);
        await ExpectCheckedAsync(locator).ConfigureAwait(true);
    }

    private static async Task ExpectValueAsync(ILocator locator, string expected)
    {
        if (await locator.InputValueAsync().ConfigureAwait(true) != expected)
        {
            throw new InvalidOperationException("A Calculator control did not retain its configured value.");
        }
    }

    private static async Task ExpectCheckedAsync(ILocator locator)
    {
        if (!await locator.IsCheckedAsync().ConfigureAwait(true))
        {
            throw new InvalidOperationException("A Calculator billing option did not remain selected.");
        }
    }

    private static string FindEdgeExecutable()
    {
        foreach (string root in new[]
        {
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData)
        })
        {
            string candidate = System.IO.Path.Combine(root, "Microsoft", "Edge", "Application", "msedge.exe");
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }
        throw new InvalidOperationException("Microsoft Edge Stable was not found in an approved installation location.");
    }

    private static Process StartOrdinaryEdge(string executable, string profilePath) =>
        Process.Start(new ProcessStartInfo
        {
            FileName = executable,
            UseShellExecute = false,
            CreateNoWindow = false,
            ArgumentList =
            {
                $"--user-data-dir={profilePath}",
                "--new-window",
                "--no-first-run",
                "--no-default-browser-check",
                CalculatorUrl
            }
        }) ?? throw new InvalidOperationException("Microsoft Edge could not be started.");
}