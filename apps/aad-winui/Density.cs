using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Design tokens and helpers for code-built UI. XAML counterparts live in <c>Themes/*.xaml</c>.
/// Values match a compact desktop developer-app density (IDE-like), not a tablet layout.
/// </summary>
internal static class Density
{
    // --- Spacing (4 px scale; SpaceXs for compact technical rows) ---
    public const double SpaceXs = 2;
    public const double SpaceSm = 4;
    public const double SpaceMd = 8;
    public const double SpaceLg = 12;
    public const double SpaceXl = 16;
    public const double SpaceXxl = 24;

    public static Thickness DialogMargin { get; } = new Thickness(SpaceLg);
    public static Thickness PanelPadding { get; } = new Thickness(SpaceMd);
    public static Thickness PanelPaddingTight { get; } = new Thickness { Left = SpaceMd, Top = SpaceMd, Right = SpaceMd, Bottom = SpaceSm };
    public static Thickness CommandBarPadding { get; } = new Thickness { Left = SpaceLg, Top = SpaceMd, Right = SpaceLg, Bottom = SpaceMd };
    public static Thickness StatusPadding { get; } = new Thickness { Left = SpaceLg, Top = SpaceSm, Right = SpaceLg, Bottom = SpaceMd };
    public static Thickness LabelFieldGap { get; } = new Thickness { Left = 0, Top = 0, Right = 0, Bottom = SpaceXs };

    // --- Control sizing ---
    /// <summary>WCAG 2.2 target-size floor for custom buttons (DIP).</summary>
    public const double MinTarget = 32;
    public const double ControlHeight = 32;
    public const double ControlHeightCompact = 28;
    public const double IconButton = 32;
    public const double IconSize = 20;
    public const double IconSizeSm = 16;
    public const double SplitterWidth = 8;
    public const double CornerChrome = 3;
    public const double CornerControl = 4;

    // --- Typography ---
    public static double Font = FontBody;
    public static bool OverrideFace;
    public static string Face = "Segoe UI";
    public const double FontCaption = 11;
    public const double FontBody = 12;
    public const double FontCommand = 13;
    public const double FontSection = 14;
    public const double FontHeading = 16;
    public const double FontPageTitle = 24;
    /// <summary>Preferred UI face name; use <see cref="FontUiResolved"/> after <see cref="ShellFonts.Resolve"/>.</summary>
    public const string FontUi = "Segoe UI";
    /// <summary>Preferred mono face name; use <see cref="FontMonoResolved"/> after resolve.</summary>
    public const string FontMono = "Consolas";
    public static string FontUiResolved { get; set; } = FontUi;
    public static string FontMonoResolved { get; set; } = FontMono;

    public static void Use(string preset)
    {
        OverrideFace = true;
        Font = FontBody;
        Face = preset switch
        {
            "editorial" => "Georgia",
            "drafting" => "Consolas",
            _ => "Segoe UI",
        };
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
            if (OverrideFace && text.FontFamily?.Source != FontMonoResolved)
            {
                text.FontFamily = new FontFamily(Face);
            }
            if (OverrideFace && Face == FontMono)
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

    /// <summary>The theme the shell is showing. Brushes are looked up in this theme's dictionary.</summary>
    public static ElementTheme Theme { get; set; } = ElementTheme.Default;

    public static Brush? ThemeBrush(string key)
    {
        var name = Theme == ElementTheme.Light ? "Light" : "Default";
        if (FindThemed(Application.Current.Resources, name, key, 0) is { } brush)
        {
            return brush;
        }
        if (Theme != ElementTheme.Light
            && FindThemed(Application.Current.Resources, "Dark", key, 0) is { } darkBrush)
        {
            return darkBrush;
        }
        return Application.Current.Resources.TryGetValue(key, out var fallback) && fallback is Brush any ? any : null;
    }

    public static Windows.UI.Color? ThemeColor(string key)
    {
        var name = Theme == ElementTheme.Light ? "Light" : "Default";
        if (FindThemedObject(Application.Current.Resources, name, key, 0) is Windows.UI.Color color)
        {
            return color;
        }
        if (Theme != ElementTheme.Light
            && FindThemedObject(Application.Current.Resources, "Dark", key, 0) is Windows.UI.Color dark)
        {
            return dark;
        }
        return null;
    }

    /// <summary>App shell / page fill for the active theme.</summary>
    public static Brush AppBackground() =>
        ThemeBrush("AppBackgroundBrush")
        ?? new SolidColorBrush(Theme == ElementTheme.Light
            ? Windows.UI.Color.FromArgb(255, 0xF3, 0xF3, 0xF3)
            : Windows.UI.Color.FromArgb(255, 0x1B, 0x1B, 0x1B));

    public static Brush CanvasBackground() =>
        ThemeBrush("CanvasBackgroundBrush")
        ?? new SolidColorBrush(Theme == ElementTheme.Light
            ? Microsoft.UI.Colors.White
            : Microsoft.UI.Colors.Black);

    /// <summary>Secondary body text at full opacity (avoids Opacity &lt; 1 contrast failures).</summary>
    public static TextBlock Secondary(string text, TextWrapping wrap = TextWrapping.NoWrap)
    {
        var block = new TextBlock { Text = text, TextWrapping = wrap, FontSize = FontBody };
        block.Foreground = ThemeBrush("TextSecondaryBrush")
            ?? ThemeBrush("TextFillColorSecondaryBrush");
        return block;
    }

    public static TextBlock SectionHeading(string text)
    {
        var block = new TextBlock
        {
            Text = text,
            FontSize = FontSection,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = ThemeBrush("TextSecondaryBrush")
                ?? ThemeBrush("TextFillColorSecondaryBrush"),
        };
        Name(block, text);
        return block;
    }

    public static StackPanel DialogStack(params UIElement[] children)
    {
        var stack = new StackPanel { Spacing = SpaceMd, Margin = DialogMargin };
        foreach (var child in children)
        {
            stack.Children.Add(child);
        }
        return stack;
    }

    public static StackPanel FieldRow(string label, FrameworkElement field)
    {
        var row = new StackPanel { Spacing = SpaceXs };
        var text = new TextBlock { Text = label, FontSize = FontBody };
        Name(text, label);
        row.Children.Add(text);
        row.Children.Add(field);
        return row;
    }

    public static void Name(DependencyObject target, string name)
    {
        if (name.Length > 0)
        {
            AutomationProperties.SetName(target, name);
        }
    }

    private static Brush? FindThemed(ResourceDictionary dictionary, string theme, string key, int depth)
    {
        if (FindThemedObject(dictionary, theme, key, depth) is Brush brush)
        {
            return brush;
        }
        return null;
    }

    private static object? FindThemedObject(ResourceDictionary dictionary, string theme, string key, int depth)
    {
        if (depth > 8)
        {
            return null;
        }
        if (dictionary.ThemeDictionaries.TryGetValue(theme, out var themed)
            && themed is ResourceDictionary inner
            && inner.TryGetValue(key, out var value))
        {
            return value;
        }
        foreach (var merged in dictionary.MergedDictionaries)
        {
            if (FindThemedObject(merged, theme, key, depth + 1) is { } found)
            {
                return found;
            }
        }
        return null;
    }
}
