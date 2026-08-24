using System.Windows;
using AzureTcoCalculator.Companion.Activation;

namespace AzureTcoCalculator.Companion;

public partial class App : Application
{
    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        LaunchActivation? activation = null;
        string? activationError = null;
        if (e.Args.Length > 0 && !LaunchActivationParser.TryParse(e.Args, out activation))
        {
            activationError = "The launch request is invalid or unsupported.";
        }

        MainWindow = new MainWindow(activation, activationError);
        MainWindow.Show();
    }
}