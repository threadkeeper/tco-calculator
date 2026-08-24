using System.Globalization;
using System.IO;
using System.Text.Json.Serialization;

namespace AzureTcoCalculator.Companion.Api;

public sealed record CalculatorManifest
{
    [JsonPropertyName("schema_version")]
    public required int SchemaVersion { get; init; }

    [JsonPropertyName("calculator_contract_version")]
    public required string CalculatorContractVersion { get; init; }

    [JsonPropertyName("calculator_url")]
    public required string CalculatorUrl { get; init; }

    [JsonPropertyName("generated_at")]
    public required string GeneratedAt { get; init; }

    [JsonPropertyName("currency")]
    public required string Currency { get; init; }

    [JsonPropertyName("locale")]
    public required string Locale { get; init; }

    [JsonPropertyName("items")]
    public required CalculatorManifestItem[] Items { get; init; }

    public void Validate()
    {
        if (SchemaVersion != 1
            || CalculatorContractVersion != "2026-08-23"
            || CalculatorUrl != "https://azure.microsoft.com/en-us/pricing/calculator/"
            || Currency != "USD"
            || Locale != "en-US"
            || Items.Length is < 1 or > 25
            || !DateTimeOffset.TryParse(
                GeneratedAt,
                CultureInfo.InvariantCulture,
                DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal,
                out _))
        {
            throw new InvalidDataException("The Calculator manifest contract is unsupported.");
        }

        for (int index = 0; index < Items.Length; index++)
        {
            Items[index].Validate(index + 1);
        }
    }
}

public sealed record CalculatorManifestItem
{
    [JsonPropertyName("item_key")]
    public required string ItemKey { get; init; }

    [JsonPropertyName("display_name")]
    public required string DisplayName { get; init; }

    [JsonPropertyName("product")]
    public required string Product { get; init; }

    [JsonPropertyName("region")]
    public required string Region { get; init; }

    [JsonPropertyName("deployment_model")]
    public required string DeploymentModel { get; init; }

    [JsonPropertyName("service_tier")]
    public required string ServiceTier { get; init; }

    [JsonPropertyName("hardware_family")]
    public required string HardwareFamily { get; init; }

    [JsonPropertyName("vcores")]
    public required int Vcores { get; init; }

    [JsonPropertyName("selected_memory_gb")]
    public required string SelectedMemoryGb { get; init; }

    [JsonPropertyName("zone_redundant")]
    public required bool ZoneRedundant { get; init; }

    [JsonPropertyName("quantity")]
    public required int Quantity { get; init; }

    [JsonPropertyName("hours_per_month")]
    public required string HoursPerMonth { get; init; }

    [JsonPropertyName("purchase_option")]
    public required string PurchaseOption { get; init; }

    [JsonPropertyName("azure_hybrid_benefit")]
    public required bool AzureHybridBenefit { get; init; }

    [JsonPropertyName("data_storage_gb")]
    public required string DataStorageGb { get; init; }

    [JsonPropertyName("backup_storage_gb")]
    public required string BackupStorageGb { get; init; }

    [JsonPropertyName("expected_public_annual")]
    public required CalculatorExpectedPublicAnnual ExpectedPublicAnnual { get; init; }

    public void Validate(int ordinal)
    {
        string key = ordinal.ToString("000", CultureInfo.InvariantCulture);
        if (ItemKey != key
            || DisplayName != $"Workload {key}"
            || Product != "azure_sql_managed_instance"
            || DeploymentModel != "single_instance"
            || Region.Length is < 1 or > 64
            || Region.Any(character => !char.IsAsciiLetterOrDigit(character) || char.IsAsciiLetterUpper(character))
            || ServiceTier is not ("next_generation_general_purpose" or "business_critical")
            || HardwareFamily is not ("premium_series" or "premium_series_memory_optimized")
            || Vcores < 1
            || Quantity is < 1 or > 10_000
            || !CanonicalDecimal(SelectedMemoryGb)
            || !CanonicalDecimal(HoursPerMonth)
            || !CanonicalDecimal(DataStorageGb)
            || !CanonicalDecimal(BackupStorageGb)
            || PurchaseOption is not ("payg" or "one_year_reservation" or "three_year_reservation" or "one_year_savings_plan"))
        {
            throw new InvalidDataException("A Calculator manifest item is invalid.");
        }
        ExpectedPublicAnnual.Validate();
    }

    private static bool CanonicalDecimal(string value) =>
        decimal.TryParse(value, NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint, CultureInfo.InvariantCulture, out decimal parsed)
        && parsed >= 0
        && value == parsed.ToString(CultureInfo.InvariantCulture);
}

public sealed record CalculatorExpectedPublicAnnual
{
    [JsonPropertyName("compute")]
    public required string Compute { get; init; }

    [JsonPropertyName("additional_memory")]
    public required string AdditionalMemory { get; init; }

    [JsonPropertyName("license")]
    public required string License { get; init; }

    [JsonPropertyName("storage")]
    public required string Storage { get; init; }

    [JsonPropertyName("total_before_parity")]
    public required string TotalBeforeParity { get; init; }

    public void Validate()
    {
        foreach (string value in new[] { Compute, AdditionalMemory, License, Storage, TotalBeforeParity })
        {
            if (!decimal.TryParse(value, NumberStyles.AllowLeadingSign | NumberStyles.AllowDecimalPoint, CultureInfo.InvariantCulture, out decimal amount)
                || amount < 0)
            {
                throw new InvalidDataException("A Calculator public-price expectation is invalid.");
            }
        }
    }
}