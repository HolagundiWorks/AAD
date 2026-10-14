using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Right-click menu in its own window, so the drawing child does not cover it.</summary>
internal static class CanvasMenu
{
    private static Window? _window;

    public static void Show(MainWindow owner, string json, Action<string, string> choose)
    {
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;
        var x = root.TryGetProperty("x", out var px) ? px.GetInt32() : 0;
        var y = root.TryGetProperty("y", out var py) ? py.GetInt32() : 0;
        var stack = new StackPanel { Spacing = 1, Margin = new Thickness(4) };
        if (root.TryGetProperty("groups", out var groups))
        {
            var first = true;
            foreach (var group in groups.EnumerateArray())
            {
                if (!first)
                {
                    stack.Children.Add(new Border
                    {
                        Height = 1,
                        Margin = new Thickness(0, 2, 0, 2),
                        Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Gray),
                        Opacity = 0.4,
                    });
                }
                first = false;
                foreach (var item in group.EnumerateArray())
                {
                    AddItem(stack, item, choose, 0);
                }
            }
        }
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var body = new ScrollViewer
        {
            Content = stack,
            MaxHeight = 360,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        var frame = new Grid { RequestedTheme = theme, Children = { body } };
        _window?.Close();
        var window = new Window { Title = "Menu", Content = frame };
        _window = window;
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsAlwaysOnTop = true;
            presenter.IsResizable = false;
        }
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
        };
        Density.Apply(frame);
        window.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32(220, 280));
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
    }

    private static void AddItem(StackPanel stack, JsonElement item, Action<string, string> choose, int indent)
    {
        if (item.TryGetProperty("items", out var children))
        {
            stack.Children.Add(new TextBlock
            {
                Text = item.TryGetProperty("label", out var title) ? title.GetString() : "",
                Margin = new Thickness(8 + indent, 4, 8, 0),
                Opacity = 0.7,
            });
            foreach (var child in children.EnumerateArray())
            {
                AddItem(stack, child, choose, indent + 12);
            }
            return;
        }
        var label = item.TryGetProperty("label", out var text) ? text.GetString() ?? "" : "";
        var id = item.TryGetProperty("id", out var ident) ? ident.GetString() ?? "" : "";
        var run = item.TryGetProperty("run", out var mode) ? mode.GetString() ?? "command" : "command";
        var enabled = !item.TryGetProperty("enabled", out var flag) || flag.ValueKind != JsonValueKind.False;
        var button = new Button
        {
            Content = label,
            IsEnabled = enabled && id.Length > 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Margin = new Thickness(indent, 0, 0, 0),
            MinHeight = 21,
            FontSize = Density.Font,
        };
        button.Click += (_, _) =>
        {
            _window?.Close();
            choose(id, run);
        };
        stack.Children.Add(button);
    }
}
