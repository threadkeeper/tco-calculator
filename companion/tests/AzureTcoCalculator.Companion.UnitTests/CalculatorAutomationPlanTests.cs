using AzureTcoCalculator.Companion.Api;
using AzureTcoCalculator.Companion.Automation;

namespace AzureTcoCalculator.Companion.UnitTests;

[TestClass]
public sealed class CalculatorAutomationPlanTests
{
    [TestMethod]
    public void Create_ConvertsExactStorageAndCalculatorMinimumBackup()
    {
        CalculatorManifest manifest = Manifest(Item());

        CalculatorAutomationItem item = CalculatorAutomationPlan.Create(manifest).Items.Single();

        Assert.AreEqual("32", item.StorageUnits);
        Assert.AreEqual("1", item.BackupStorageGb);
        Assert.AreEqual("sweden-central", item.Region);
    }

    [TestMethod]
    [DataRow("one_year_reservation", false)]
    [DataRow("payg", true)]
    public void Create_RejectsUnverifiedBillingSelections(string purchaseOption, bool hybridBenefit)
    {
        CalculatorManifest manifest = Manifest(Item() with
        {
            PurchaseOption = purchaseOption,
            AzureHybridBenefit = hybridBenefit
        });

        Assert.ThrowsExactly<InvalidDataException>(() => CalculatorAutomationPlan.Create(manifest));
    }

    [TestMethod]
    public void Create_RejectsStorageThatIsNotAnExactCalculatorUnit()
    {
        CalculatorManifest manifest = Manifest(Item() with { DataStorageGb = "33" });

        Assert.ThrowsExactly<InvalidDataException>(() => CalculatorAutomationPlan.Create(manifest));
    }

    private static CalculatorManifest Manifest(CalculatorManifestItem item) => new()
    {
        SchemaVersion = 1,
        CalculatorContractVersion = "2026-08-23",
        CalculatorUrl = "https://azure.microsoft.com/en-us/pricing/calculator/",
        GeneratedAt = "2026-08-23T19:00:00Z",
        Currency = "USD",
        Locale = "en-US",
        Items = [item]
    };

    private static CalculatorManifestItem Item() => new()
    {
        ItemKey = "001",
        DisplayName = "Workload 001",
        Product = "azure_sql_managed_instance",
        Region = "swedencentral",
        DeploymentModel = "single_instance",
        ServiceTier = "next_generation_general_purpose",
        HardwareFamily = "premium_series",
        Vcores = 4,
        SelectedMemoryGb = "28",
        ZoneRedundant = false,
        Quantity = 1,
        HoursPerMonth = "730",
        PurchaseOption = "payg",
        AzureHybridBenefit = false,
        DataStorageGb = "1024",
        BackupStorageGb = "0",
        ExpectedPublicAnnual = new CalculatorExpectedPublicAnnual
        {
            Compute = "1",
            AdditionalMemory = "0",
            License = "1",
            Storage = "1",
            TotalBeforeParity = "3"
        }
    };
}