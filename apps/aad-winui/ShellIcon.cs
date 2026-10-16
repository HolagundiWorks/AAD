using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Aad.WinUi;

/// <summary>The AAD mark from <c>assets/app-icon</c>, copied beside the program.</summary>
internal static class ShellIcon
{
    public static void Apply(Window window)
    {
        var path = Path.Combine(AppContext.BaseDirectory, "aad.ico");
        if (File.Exists(path))
        {
            window.AppWindow.SetIcon(path);
        }
    }

    public static ImageSource? Logo()
    {
        var path = Path.Combine(AppContext.BaseDirectory, "aad-256.png");
        if (!File.Exists(path))
        {
            return null;
        }
        return new BitmapImage(new Uri(path));
    }
}
