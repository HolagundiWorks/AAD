using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Solid shell chrome for every top-level window (main, layers, hatch, rooms, forms, …).
/// Solid Fluent shell chrome (opaque fills). Floating command overlays stay non-Mica so the
/// HWND drawing child remains fully opaque. Colours: <c>Themes/ColorTokens.xaml</c>.
/// </summary>
internal static class ShellChrome
{
    public static ElementTheme ThemeOf(Window? owner)
    {
        if (owner?.Content is FrameworkElement shell && shell.RequestedTheme != ElementTheme.Default)
        {
            return shell.RequestedTheme;
        }
        return Resolve(Density.Theme);
    }

    public static ElementTheme Resolve(ElementTheme theme)
    {
        if (theme != ElementTheme.Default)
        {
            return theme;
        }
        if (Density.Theme != ElementTheme.Default)
        {
            return Density.Theme;
        }
        return ElementTheme.Dark;
    }

    /// <summary>Icon, solid backdrop, page theme/fill, and title-bar colours.</summary>
    public static void Apply(Window window, ElementTheme theme = ElementTheme.Default)
    {
        theme = Resolve(theme);
        Density.Theme = theme;
        window.SystemBackdrop = null;
        ShellIcon.SetIcon(window);
        if (window.Content is FrameworkElement root)
        {
            root.RequestedTheme = theme;
            Paint(root);
        }
        PaintTitleBar(window);
    }

    /// <summary>Theme + fill for a content root that is not yet (or no longer) the window Content.</summary>
    public static void Paint(FrameworkElement root, ElementTheme theme)
    {
        theme = Resolve(theme);
        Density.Theme = theme;
        root.RequestedTheme = theme;
        Paint(root);
    }

    private static void Paint(FrameworkElement root)
    {
        var fill = Density.AppBackground();
        switch (root)
        {
            case Panel panel:
                panel.Background = fill;
                break;
            case Border border:
                border.Background = fill;
                break;
            case Control control:
                control.Background = fill;
                break;
        }
    }

    private static void PaintTitleBar(Window window)
    {
        var title = window.AppWindow.TitleBar;
        var bg = Density.ThemeColor("TitleBarBackgroundColor")
            ?? (Density.Theme == ElementTheme.Light
                ? Windows.UI.Color.FromArgb(255, 0xF3, 0xF3, 0xF3)
                : Windows.UI.Color.FromArgb(255, 0x20, 0x20, 0x20));
        var fg = Density.ThemeColor("TitleBarForegroundColor")
            ?? (Density.Theme == ElementTheme.Light
                ? Windows.UI.Color.FromArgb(255, 0x1A, 0x1A, 0x1A)
                : Microsoft.UI.Colors.White);
        var inactive = Density.ThemeColor("TitleBarInactiveForegroundColor")
            ?? (Density.Theme == ElementTheme.Light
                ? Windows.UI.Color.FromArgb(255, 0x5C, 0x5C, 0x5C)
                : Windows.UI.Color.FromArgb(255, 0xAA, 0xAA, 0xAA));
        var hover = Density.ThemeColor("TitleBarHoverColor")
            ?? (Density.Theme == ElementTheme.Light
                ? Windows.UI.Color.FromArgb(255, 0xE5, 0xE5, 0xE5)
                : Windows.UI.Color.FromArgb(255, 0x2D, 0x2D, 0x2D));
        var pressed = Density.ThemeColor("TitleBarPressedColor")
            ?? (Density.Theme == ElementTheme.Light
                ? Windows.UI.Color.FromArgb(255, 0xD6, 0xD6, 0xD6)
                : Windows.UI.Color.FromArgb(255, 0x3A, 0x3A, 0x3A));
        title.BackgroundColor = bg;
        title.InactiveBackgroundColor = bg;
        title.ForegroundColor = fg;
        title.InactiveForegroundColor = inactive;
        title.ButtonBackgroundColor = bg;
        title.ButtonInactiveBackgroundColor = bg;
        title.ButtonForegroundColor = fg;
        title.ButtonInactiveForegroundColor = inactive;
        title.ButtonHoverBackgroundColor = hover;
        title.ButtonHoverForegroundColor = fg;
        title.ButtonPressedBackgroundColor = pressed;
        title.ButtonPressedForegroundColor = fg;
        try
        {
            title.ExtendsContentIntoTitleBar = false;
        }
        catch (Exception)
        {
        }
    }
}
