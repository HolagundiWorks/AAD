using Microsoft.UI.Xaml;

namespace Aad.WinUi;

/// <summary>Sanity-check that semantic theme tokens resolve in both light and dark.</summary>
internal static class ShellTheme
{
    private static readonly string[] RequiredBrushes =
    [
        "AppBackgroundBrush",
        "SurfaceBrush",
        "SurfaceSecondaryBrush",
        "TextPrimaryBrush",
        "TextSecondaryBrush",
        "AccentBrush",
        "CommandChromeBrush",
        "CommandChromeCadBrush",
        "CommandChromeAccentBrush",
        "CadChannelBrush",
        "CanvasBackgroundBrush",
        "BorderSubtleBrush",
    ];

    public static void VerifyTokens()
    {
        var prior = Density.Theme;
        var missing = new List<string>();
        foreach (var theme in new[] { ElementTheme.Dark, ElementTheme.Light })
        {
            Density.Theme = theme;
            foreach (var key in RequiredBrushes)
            {
                if (Density.ThemeBrush(key) == null)
                {
                    missing.Add(theme + ":" + key);
                }
            }
            if (Density.ThemeColor("TitleBarBackgroundColor") == null)
            {
                missing.Add(theme + ":TitleBarBackgroundColor");
            }
        }
        Density.Theme = prior == ElementTheme.Default ? ElementTheme.Dark : prior;
        if (missing.Count == 0)
        {
            return;
        }
        try
        {
            var path = Path.Combine(Path.GetTempPath(), "aad-winui.log");
            File.AppendAllText(path, "Missing UI tokens: " + string.Join(", ", missing) + Environment.NewLine);
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }
}
