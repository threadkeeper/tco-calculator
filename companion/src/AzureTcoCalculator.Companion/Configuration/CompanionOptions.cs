using System.Reflection;

namespace AzureTcoCalculator.Companion.Configuration;

public sealed record CompanionOptions(Uri ApiBaseUri, Guid ClientId, string Scope)
{
    public static CompanionOptions Load()
    {
        IReadOnlyDictionary<string, string> metadata = typeof(CompanionOptions).Assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .Where(attribute => attribute.Value is not null)
            .ToDictionary(attribute => attribute.Key, attribute => attribute.Value!);
        metadata.TryGetValue("CalculatorApiOrigin", out string? endpoint);
        metadata.TryGetValue("CalculatorCompanionClientId", out string? clientId);
        metadata.TryGetValue("CalculatorApiScope", out string? scope);
        endpoint ??= string.Empty;
        clientId ??= string.Empty;
        scope ??= string.Empty;

        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out Uri? origin)
            || origin.Scheme != Uri.UriSchemeHttps
            || origin.UserInfo.Length != 0
            || origin.Query.Length != 0
            || origin.Fragment.Length != 0
            || origin.AbsolutePath != "/"
            || !origin.Host.EndsWith(".azurecontainerapps.io", StringComparison.OrdinalIgnoreCase)
            || !Guid.TryParseExact(clientId, "D", out Guid parsedClientId)
            || parsedClientId == Guid.Empty
            || scope.Length is < 3 or > 200
            || !scope.StartsWith("api://", StringComparison.Ordinal)
            || scope.Any(char.IsWhiteSpace))
        {
            throw new InvalidOperationException("The Calculator companion is not configured for the approved API.");
        }

        return new CompanionOptions(new Uri(origin, "/api/v1/"), parsedClientId, scope);
    }
}