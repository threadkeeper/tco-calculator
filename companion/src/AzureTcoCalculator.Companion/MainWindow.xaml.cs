using System.ComponentModel;
using System.IO;
using System.Windows;
using System.Windows.Interop;
using AzureTcoCalculator.Companion.Activation;
using AzureTcoCalculator.Companion.Api;
using AzureTcoCalculator.Companion.Authentication;
using AzureTcoCalculator.Companion.Automation;
using AzureTcoCalculator.Companion.Configuration;

namespace AzureTcoCalculator.Companion;

/// <summary>
/// Interaction logic for MainWindow.xaml
/// </summary>
public partial class MainWindow : Window
{
    private readonly LaunchActivation? _activation;
    private readonly CancellationTokenSource _shutdown = new();
    private bool _ordinaryEdgeRunning;
    private bool _finished;

    public MainWindow(LaunchActivation? activation = null, string? activationError = null)
    {
        InitializeComponent();
        _activation = activation;
        StatusText.Text = activationError ?? (activation is null
            ? "Open this companion from a saved project in the TCO Calculator."
            : "Launch received. Secure project transfer is starting.");
        if (activation is null)
        {
            Progress.Visibility = Visibility.Collapsed;
            CloseButton.Content = "Close";
            _finished = true;
        }
        Loaded += MainWindow_Loaded;
        Closing += MainWindow_Closing;
    }

    private async void MainWindow_Loaded(object sender, RoutedEventArgs e)
    {
        if (_activation is null)
        {
            return;
        }
        try
        {
            CompanionOptions options = CompanionOptions.Load();
            WamTokenProvider tokens = new(
                options.ClientId,
                options.Scope,
                () => new WindowInteropHelper(this).Handle);
            StatusText.Text = "Signing in securely with your Windows account...";
            string accessToken = await tokens.AcquireAccessTokenAsync(_shutdown.Token);
            using CalculatorLaunchClient client = new(options.ApiBaseUri);
            Guid instanceId = Guid.NewGuid();
            StatusText.Text = "Claiming the one-time Calculator estimate...";
            ClaimedLaunch launch = await client.ClaimAsync(
                _activation.LaunchId,
                instanceId,
                accessToken,
                _shutdown.Token);
            CalculatorAutomationPlan plan = CalculatorAutomationPlan.Create(launch.Manifest);
            await client.AcknowledgeAsync(
                _activation.LaunchId,
                instanceId,
                launch.ETag,
                accessToken,
                _shutdown.Token);
            accessToken = string.Empty;

            CalculatorAutomationService automation = new();
            await automation.RunAsync(
                plan,
                status => Dispatcher.Invoke(() => StatusText.Text = status),
                running => Dispatcher.Invoke(() => _ordinaryEdgeRunning = running),
                _shutdown.Token);
            StatusText.Text = "Estimate handoff complete. The isolated Edge profile was removed.";
            Progress.Visibility = Visibility.Collapsed;
            CloseButton.Content = "Close";
            _finished = true;
        }
        catch (OperationCanceledException) when (_shutdown.IsCancellationRequested)
        {
            StatusText.Text = "Calculator handoff cancelled.";
            Progress.Visibility = Visibility.Collapsed;
            CloseButton.Content = "Close";
            CloseButton.IsEnabled = true;
            _finished = true;
        }
        catch (InvalidDataException exception)
        {
            ShowFailure(exception.Message);
        }
        catch (InvalidOperationException exception)
        {
            ShowFailure(exception.Message);
        }
        catch
        {
            ShowFailure("The secure Calculator handoff could not be completed.");
        }
    }

    private void ShowFailure(string message)
    {
        StatusText.Text = message;
        Progress.Visibility = Visibility.Collapsed;
        CloseButton.Content = "Close";
        CloseButton.IsEnabled = true;
        _finished = true;
    }

    private void CloseButton_Click(object sender, RoutedEventArgs e)
    {
        if (_ordinaryEdgeRunning)
        {
            StatusText.Text = "Close the ordinary Edge window first so its isolated profile can be removed.";
            return;
        }
        if (!_finished)
        {
            _shutdown.Cancel();
            CloseButton.IsEnabled = false;
            return;
        }
        Close();
    }

    private void MainWindow_Closing(object? sender, CancelEventArgs e)
    {
        if (_ordinaryEdgeRunning)
        {
            e.Cancel = true;
            StatusText.Text = "Close the ordinary Edge window first so its isolated profile can be removed.";
            return;
        }
        if (!_finished)
        {
            e.Cancel = true;
            _shutdown.Cancel();
            StatusText.Text = "Cancelling Calculator handoff and cleaning up...";
            CloseButton.IsEnabled = false;
            return;
        }
        _shutdown.Cancel();
    }
}