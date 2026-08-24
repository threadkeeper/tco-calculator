using System.Globalization;
using System.IO;
using AzureTcoCalculator.Companion.Api;

namespace AzureTcoCalculator.Companion.Automation;

public sealed record CalculatorAutomationItem(
    string DisplayName,
    string Region,
    string Vcores,
    string MemoryGb,
    string Quantity,
    string HoursPerMonth,
    string StorageUnits,
    string BackupStorageGb);

public sealed record CalculatorAutomationPlan(
    string EstimateName,
    IReadOnlyList<CalculatorAutomationItem> Items)
{
    public static CalculatorAutomationPlan Create(CalculatorManifest manifest)
    {
        List<CalculatorAutomationItem> items = new(manifest.Items.Length);
        foreach (CalculatorManifestItem item in manifest.Items)
        {
            if (item.Region != "swedencentral"
                || item.ServiceTier != "next_generation_general_purpose"
                || item.HardwareFamily != "premium_series"
                || item.ZoneRedundant
                || item.PurchaseOption != "payg"
                || item.AzureHybridBenefit)
            {
                throw new InvalidDataException(
                    "This estimate uses a Calculator option that the MVP companion does not yet support.");
            }

            decimal storageGb = ParseDecimal(item.DataStorageGb);
            decimal storageUnits = storageGb / 32m;
            if (storageUnits != decimal.Truncate(storageUnits) || storageUnits < 1)
            {
                throw new InvalidDataException(
                    "This estimate's storage cannot be represented exactly in the Calculator.");
            }

            decimal backupGb = ParseDecimal(item.BackupStorageGb);
            if (backupGb != 0m)
            {
                throw new InvalidDataException(
                    "This estimate's backup storage is not supported by the MVP companion.");
            }

            items.Add(new CalculatorAutomationItem(
                item.DisplayName,
                "sweden-central",
                item.Vcores.ToString(CultureInfo.InvariantCulture),
                item.SelectedMemoryGb,
                item.Quantity.ToString(CultureInfo.InvariantCulture),
                item.HoursPerMonth,
                storageUnits.ToString(CultureInfo.InvariantCulture),
                "1"));
        }
        return new CalculatorAutomationPlan(manifest.EstimateName, items);
    }

    private static decimal ParseDecimal(string value) => decimal.Parse(
        value,
        NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint,
        CultureInfo.InvariantCulture);
}