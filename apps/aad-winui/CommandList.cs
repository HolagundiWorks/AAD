using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Command reference (F1): the menu commands, in their own window.</summary>
internal static class CommandList
{
    private static Window? _window;

    public static void Show(Window owner, string menuJson)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var lines = new TextBlock { FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Consolas"), TextWrapping = TextWrapping.Wrap, Margin = new Thickness(12) };
        var text = new System.Text.StringBuilder();
        try
        {
            using var doc = JsonDocument.Parse(string.IsNullOrEmpty(menuJson) ? "[]" : menuJson);
            foreach (var menu in doc.RootElement.EnumerateArray())
            {
                Walk(text, menu, "");
            }
        }
        catch (JsonException)
        {
            text.Append("The command list could not be read.");
        }
        lines.Text = text.ToString();
        lines.FontSize = Density.Font;
        Density.Name(lines, "Command list");
        AutomationProperties.SetHeadingLevel(lines, Microsoft.UI.Xaml.Automation.Peers.AutomationHeadingLevel.Level1);
        _window?.Close();
        var window = new Window
        {
            Title = "Commands",
            Content = new ScrollViewer
            {
                Content = new Grid { RequestedTheme = theme, Children = { lines } },
            },
        };
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
        };
        _window = window;
        ShellIcon.Apply(window);
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32(420, 520));
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
    }

    private static void Walk(System.Text.StringBuilder text, JsonElement node, string prefix)
    {
        var title = node.TryGetProperty("title", out var name) ? name.GetString() ?? "" : "";
        var id = node.TryGetProperty("id", out var ident) && ident.ValueKind == JsonValueKind.String ? ident.GetString() : null;
        if (!string.IsNullOrEmpty(id))
        {
            var shortcut = node.TryGetProperty("shortcut", out var key) && key.ValueKind == JsonValueKind.String ? key.GetString() : null;
            text.Append(prefix).Append(title);
            if (!string.IsNullOrEmpty(shortcut))
            {
                text.Append("  ").Append(shortcut);
            }
            text.Append("  (").Append(id).Append(')').AppendLine();
            return;
        }
        if (!node.TryGetProperty("items", out var items))
        {
            return;
        }
        var next = prefix + title + " > ";
        foreach (var child in items.EnumerateArray())
        {
            Walk(text, child, next);
        }
    }
}
