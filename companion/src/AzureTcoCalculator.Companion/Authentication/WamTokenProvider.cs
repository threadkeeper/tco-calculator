using Microsoft.Identity.Client;
using Microsoft.Identity.Client.Broker;

namespace AzureTcoCalculator.Companion.Authentication;

public sealed class WamTokenProvider(Guid clientId, string scope, Func<nint> parentWindow)
{
    private readonly string[] _scopes = [scope];
    private readonly IPublicClientApplication _application = PublicClientApplicationBuilder
        .Create(clientId.ToString("D"))
        .WithDefaultRedirectUri()
        .WithParentActivityOrWindow(parentWindow)
        .WithBroker(new BrokerOptions(BrokerOptions.OperatingSystems.Windows)
        {
            Title = "Azure TCO Calculator Companion"
        })
        .Build();

    public async Task<string> AcquireAccessTokenAsync(CancellationToken cancellationToken)
    {
        IAccount account = (await _application.GetAccountsAsync().ConfigureAwait(true)).FirstOrDefault()
            ?? PublicClientApplication.OperatingSystemAccount;
        try
        {
            AuthenticationResult result = await _application
                .AcquireTokenSilent(_scopes, account)
                .ExecuteAsync(cancellationToken)
                .ConfigureAwait(true);
            return result.AccessToken;
        }
        catch (MsalUiRequiredException)
        {
            AuthenticationResult result = await _application
                .AcquireTokenInteractive(_scopes)
                .WithAccount(account)
                .WithParentActivityOrWindow(parentWindow())
                .ExecuteAsync(cancellationToken)
                .ConfigureAwait(true);
            return result.AccessToken;
        }
    }
}