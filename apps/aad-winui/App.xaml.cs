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
            // Backdrop / composition races must not kill the shell.
            if (e.Exception is ArgumentException)
            {
                e.Handled = true;
            }
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // The window must stay rooted. A local variable is collected and the window closes.
        // Splash is drawn inside MainWindow (not a second top-level window) so acrylic on the
        // command bar is not torn down by a competing Activate/Close.
        _window = new MainWindow(Program.Launch);
        ShellIcon.Apply(_window);
        _window.Activate();
        // Activate does not show the window when this process cannot take the foreground.
        Native.ShowWindow(WindowNative.GetWindowHandle(_window), 8);
    }
}
