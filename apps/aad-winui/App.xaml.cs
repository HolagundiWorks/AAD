using Microsoft.UI.Xaml;
using WinRT.Interop;

namespace Aad.WinUi;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, e) =>
        {
            try
            {
                var path = Path.Combine(Path.GetTempPath(), "aad-winui.log");
                File.AppendAllText(path, e.Exception + Environment.NewLine);
            }
            catch (IOException)
            {
            }
            e.Handled = false;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // The window must stay rooted. A local variable is collected and the window closes.
        _window = new MainWindow(Program.Launch);
        ShellIcon.Apply(_window);
        _window.Activate();
        // Activate does not show the window when this process cannot take the foreground.
        Native.ShowWindow(WindowNative.GetWindowHandle(_window), 8);
    }
}
