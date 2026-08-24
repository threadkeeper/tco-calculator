using System.Text.RegularExpressions;

namespace AzureTcoCalculator.Companion.Activation;

public static partial class LaunchActivationParser
{
    private const int MaximumActivationLength = 128;

    public static bool TryParse(IReadOnlyList<string> arguments, out LaunchActivation? activation)
    {
        activation = null;
        if (arguments.Count != 1)
        {
            return false;
        }

        string value = arguments[0];
        if (value.Length > MaximumActivationLength)
        {
            return false;
        }

        Match match = ActivationPattern().Match(value);
        if (!match.Success || !Guid.TryParseExact(match.Groups["id"].Value, "D", out Guid launchId))
        {
            return false;
        }

        activation = new LaunchActivation(launchId, 1);
        return true;
    }

    [GeneratedRegex(
        "\\Aazure-tco-calculator://launch/?\\?v=1&id=(?<id>[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\\z",
        RegexOptions.CultureInvariant | RegexOptions.ExplicitCapture | RegexOptions.NonBacktracking)]
    private static partial Regex ActivationPattern();
}