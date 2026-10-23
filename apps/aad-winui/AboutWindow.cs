using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>About O-Draft. The version comes from the host.</summary>
internal static class AboutWindow
{
    private static Window? _window;

    public static void Show(Window owner, string version)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var logo = new Image { Width = 64, Height = 64, HorizontalAlignment = HorizontalAlignment.Left, Source = ShellIcon.Logo() };
        AutomationProperties.SetName(logo, "O-Draft");
        var stack = new StackPanel { Spacing = Density.SpaceSm, Margin = new Thickness(Density.SpaceXl) };
        stack.Children.Add(logo);
        stack.Children.Add(new TextBlock { Text = "O-Draft", FontSize = 16, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        stack.Children.Add(new TextBlock { Text = "Drafting. Part of OSAS, the Open Source Architecture Suite." });
        if (version.Length > 0)
        {
            stack.Children.Add(new TextBlock { Text = "Version " + version });
        }
        stack.Children.Add(new TextBlock { Text = "2D architectural drafting. Open source, written in Rust.", TextWrapping = TextWrapping.Wrap });
        stack.Children.Add(Density.Secondary("MIT OR Apache-2.0. Not affiliated with Autodesk, Inc.", TextWrapping.Wrap));
        Density.Apply(stack);
        _window?.Close();
        var window = new Window
        {
            Title = "About O-Draft",
            Content = new Grid { RequestedTheme = theme, Children = { stack } },
        };
        ShellIcon.Apply(window, theme);
        ShellFit.ToContent(window, minWidthDip: 320, minHeightDip: 180, maxWidthDip: 480, maxHeightDip: 360);
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
        };
        _window = window;
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
    }
}
