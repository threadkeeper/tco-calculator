using AzureTcoCalculator.Companion.Activation;

namespace AzureTcoCalculator.Companion.UnitTests;

[TestClass]
public sealed class LaunchActivationParserTests
{
    private const string ValidActivation =
        "azure-tco-calculator://launch?v=1&id=01234567-89ab-4cde-8f01-23456789abcd";

    [TestMethod]
    public void AcceptsCanonicalActivation()
    {
        bool parsed = LaunchActivationParser.TryParse([ValidActivation], out LaunchActivation? activation);

        Assert.IsTrue(parsed);
        Assert.IsNotNull(activation);
        Assert.AreEqual(Guid.Parse("01234567-89ab-4cde-8f01-23456789abcd"), activation.LaunchId);
        Assert.AreEqual(1, activation.ProtocolVersion);
    }

    [TestMethod]
    public void AcceptsWindowsNormalizedActivation()
    {
        const string normalizedActivation =
            "azure-tco-calculator://launch/?v=1&id=01234567-89ab-4cde-8f01-23456789abcd";

        bool parsed = LaunchActivationParser.TryParse([normalizedActivation], out LaunchActivation? activation);

        Assert.IsTrue(parsed);
        Assert.IsNotNull(activation);
        Assert.AreEqual(Guid.Parse("01234567-89ab-4cde-8f01-23456789abcd"), activation.LaunchId);
        Assert.AreEqual(1, activation.ProtocolVersion);
    }

    [TestMethod]
    [DataRow()]
    [DataRow("azure-tco-calculator://launch?v=1&id=01234567-89ab-4cde-8f01-23456789abcd", "extra")]
    [DataRow("azure-tco-calculator://launch?v=2&id=01234567-89ab-4cde-8f01-23456789abcd")]
    [DataRow("azure-tco-calculator://other?v=1&id=01234567-89ab-4cde-8f01-23456789abcd")]
    [DataRow("azure-tco-calculator://launch?id=01234567-89ab-4cde-8f01-23456789abcd&v=1")]
    [DataRow("azure-tco-calculator://launch?v=1&id=01234567-89AB-4CDE-8F01-23456789ABCD")]
    [DataRow("azure-tco-calculator://launch?v=1&id=%30%31%32%33%34%35%36%37-89ab-4cde-8f01-23456789abcd")]
    [DataRow("azure-tco-calculator://launch?v=1&id=01234567-89ab-4cde-8f01-23456789abcd#fragment")]
    [DataRow("azure-tco-calculator://user@launch?v=1&id=01234567-89ab-4cde-8f01-23456789abcd")]
    [DataRow("azure-tco-calculator://launch:443?v=1&id=01234567-89ab-4cde-8f01-23456789abcd")]
    [DataRow("azure-tco-calculator://launch?v=1&id=01234567-89ab-4cde-8f01-23456789abcd&extra=1")]
    public void RejectsNoncanonicalOrOverscopedActivation(params string[] arguments)
    {
        Assert.IsFalse(LaunchActivationParser.TryParse(arguments, out LaunchActivation? activation));
        Assert.IsNull(activation);
    }

    [TestMethod]
    public void RejectsOversizedActivation()
    {
        string oversized = ValidActivation + new string('x', 128);

        Assert.IsFalse(LaunchActivationParser.TryParse([oversized], out LaunchActivation? activation));
        Assert.IsNull(activation);
    }
}