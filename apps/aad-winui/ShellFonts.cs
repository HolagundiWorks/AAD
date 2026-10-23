using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>
/// Resolves UI/mono faces that are actually present. WinUI <see cref="FontFamily"/> does not
/// honour CSS-style comma fallbacks, so we pick one name at startup and publish it to resources.
/// </summary>
internal static class ShellFonts
{
    public static string Ui { get; private set; } = "Segoe UI";
    public static string Mono { get; private set; } = "Consolas";

    public static void Resolve()
    {
        var fonts = Environment.GetFolderPath(Environment.SpecialFolder.Fonts);
        Ui = Has(fonts, "SegUIVar.ttf", "SegoeUI-VF.ttf") ? "Segoe UI Variable" : "Segoe UI";
        Mono = Has(fonts, "CascadiaMono.ttf", "cascadiamono.ttf", "CascadiaMonoPL.ttf")
            ? "Cascadia Mono"
            : Has(fonts, "CascadiaCode.ttf", "cascadiacode.ttf")
                ? "Cascadia Code"
                : "Consolas";
        Density.FontUiResolved = Ui;
        Density.FontMonoResolved = Mono;
        ApplyResources();
    }

    public static FontFamily UiFamily => new(Ui);
    public static FontFamily MonoFamily => new(Mono);

    private static void ApplyResources()
    {
        var resources = Application.Current?.Resources;
        if (resources == null)
        {
            return;
        }
        resources["FontFamilyUi"] = UiFamily;
        resources["FontFamilyMono"] = MonoFamily;
    }

    private static bool Has(string fontsDir, params string[] files)
    {
        foreach (var file in files)
        {
            try
            {
                if (File.Exists(Path.Combine(fontsDir, file)))
                {
                    return true;
                }
            }
            catch (IOException)
            {
            }
            catch (UnauthorizedAccessException)
            {
            }
        }
        return false;
    }
}
