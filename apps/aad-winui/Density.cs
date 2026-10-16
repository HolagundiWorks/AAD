using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>Type size for panel content. The shell, tool bar and command bar keep the WinUI defaults.</summary>
internal static class Density
{
    public static double Font = 12;
    public static bool OverrideFace;
    public static string Face = "Segoe UI";
    private const double MinFont = 12;
    /// <summary>WCAG 2.2 target-size floor for custom buttons (DIP).</summary>
    public const double MinTarget = 32;

    public static void Use(string preset)
    {
        OverrideFace = true;
        switch (preset)
        {
            case "editorial":
                Font = MinFont;
                Face = "Georgia";
                break;
            case "multilingual":
                Font = MinFont;
                Face = "Segoe UI";
                break;
            case "compact":
                Font = MinFont;
                Face = "Segoe UI";
                break;
            case "drafting":
                Font = MinFont;
                Face = "Consolas";
                break;
            default:
                Font = MinFont;
                Face = "Segoe UI";
                break;
        }
    }

    public static void Apply(DependencyObject node)
    {
        if (node is Control control)
        {
            control.FontSize = Font;
            if (OverrideFace)
            {
                control.FontFamily = new FontFamily(Face);
            }
        }
        else if (node is TextBlock text)
        {
            text.FontSize = Font;
            if (OverrideFace && text.FontFamily?.Source != "Consolas")
            {
                text.FontFamily = new FontFamily(Face);
            }
            if (OverrideFace && Face == "Consolas")
            {
                text.FontFamily = new FontFamily(Face);
            }
        }
        var count = VisualTreeHelper.GetChildrenCount(node);
        for (var i = 0; i < count; i++)
        {
            Apply(VisualTreeHelper.GetChild(node, i));
        }
    }

    /// <summary>Secondary body text at full opacity (avoids Opacity &lt; 1 contrast failures).</summary>
    public static TextBlock Secondary(string text, TextWrapping wrap = TextWrapping.NoWrap)
    {
        var block = new TextBlock { Text = text, TextWrapping = wrap };
        if (Application.Current.Resources.TryGetValue("TextFillColorSecondaryBrush", out var value) && value is Brush brush)
        {
            block.Foreground = brush;
        }
        return block;
    }

    public static void Name(DependencyObject target, string name)
    {
        if (name.Length > 0)
        {
            AutomationProperties.SetName(target, name);
        }
    }
}
