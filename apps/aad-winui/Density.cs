using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>Chrome is three quarters of the WinUI default (14px) type size.</summary>
internal static class Density
{
    public const double Font = 10.5;

    public static void Apply(DependencyObject node)
    {
        if (node is Control control)
        {
            control.FontSize = Font;
        }
        else if (node is TextBlock text)
        {
            text.FontSize = Font;
        }
        var count = VisualTreeHelper.GetChildrenCount(node);
        for (var i = 0; i < count; i++)
        {
            Apply(VisualTreeHelper.GetChild(node, i));
        }
    }
}
