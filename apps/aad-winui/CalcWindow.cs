using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;

namespace Aad.WinUi;

/// <summary>Architect calculator. Equals runs the engine <c>cal</c> command.</summary>
internal sealed class CalcWindow : Window
{
    private readonly IntPtr _host;
    private readonly TextBox _expr = new() { PlaceholderText = "10' 6\" x 20'", AcceptsReturn = false, MinHeight = Density.MinTarget };
    private readonly TextBlock _result = new() { TextWrapping = TextWrapping.Wrap };

    private CalcWindow(Window owner, IntPtr host)
    {
        _host = host;
        Title = "Calculator";
        Density.Name(_expr, "Expression");
        AutomationProperties.SetLiveSetting(_result, AutomationLiveSetting.Polite);
        Density.Name(_result, "Result");
        var equals = Action("=");
        equals.Click += (_, _) => Evaluate();
        var clear = Action("Clear");
        clear.Click += (_, _) =>
        {
            _expr.Text = "";
            _result.Text = "";
        };
        var keys = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        foreach (var key in new[] { "7", "8", "9", "+", "-", "*", "/", "'", "\"", " ", "x" })
        {
            var token = key;
            var caption = token == " " ? "space" : token;
            var button = new Button { Content = caption, MinWidth = Density.MinTarget, MinHeight = Density.MinTarget };
            Density.Name(button, "Insert " + caption);
            button.Click += (_, _) => _expr.Text += token;
            keys.Children.Add(button);
        }
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, Children = { equals, clear } };
        var root = new StackPanel { Spacing = 8, Margin = new Thickness(12), Children = { _expr, keys, row, _result } };
        if (owner.Content is FrameworkElement shell)
        {
            root.RequestedTheme = shell.RequestedTheme;
        }
        Content = root;
        Density.Apply(root);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(420, 220));
        ShellIcon.Apply(this);
        _expr.KeyDown += (_, e) =>
        {
            if (e.Key == Windows.System.VirtualKey.Enter)
            {
                e.Handled = true;
                Evaluate();
            }
        };
    }

    private static Button Action(string label)
    {
        var button = new Button { Content = label, MinHeight = Density.MinTarget, MinWidth = 48 };
        Density.Name(button, label == "=" ? "Evaluate" : label);
        return button;
    }

    public static CalcWindow Show(Window owner, IntPtr host)
    {
        var window = new CalcWindow(owner, host);
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    private void Evaluate()
    {
        var body = JsonSerializer.Serialize(new Dictionary<string, string> { ["expr"] = _expr.Text });
        if (Native.Execute(_host, "cal", body) != 0)
        {
            _result.Text = Native.LastError(_host);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            _result.Text = doc.RootElement.TryGetProperty("message", out var message) ? message.GetString() ?? "" : Native.ReadResult(_host);
        }
        catch (JsonException)
        {
            _result.Text = Native.ReadResult(_host);
        }
    }
}
