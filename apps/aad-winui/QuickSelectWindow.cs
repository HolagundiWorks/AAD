using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>Edit &gt; Quick Select. The filter runs the engine <c>qselect</c> command.</summary>
internal sealed class QuickSelectWindow : Window
{
    private readonly IntPtr _host;
    private readonly Action _changed;
    private readonly ComboBox _apply = new() { MinWidth = 160 };
    private readonly ComboBox _type = new() { MinWidth = 160 };
    private readonly ComboBox _property = new() { MinWidth = 160 };
    private readonly ComboBox _operator = new() { MinWidth = 80 };
    private readonly TextBox _value = new() { MinWidth = 120 };
    private readonly ComboBox _mode = new() { MinWidth = 160 };
    private readonly TextBlock _message = new() { TextWrapping = TextWrapping.Wrap };
    private bool _filling;

    public QuickSelectWindow(FrameworkElement? themeSource, IntPtr host, Action changed)
    {
        _host = host;
        _changed = changed;
        var stack = new StackPanel { Spacing = 6, Margin = new Thickness(12) };
        stack.Children.Add(Label("Apply to"));
        stack.Children.Add(_apply);
        stack.Children.Add(Label("Object type"));
        stack.Children.Add(_type);
        stack.Children.Add(Label("Property"));
        stack.Children.Add(_property);
        stack.Children.Add(Label("Operator"));
        stack.Children.Add(_operator);
        stack.Children.Add(Label("Value"));
        stack.Children.Add(_value);
        stack.Children.Add(Label("How to apply"));
        stack.Children.Add(_mode);
        var ok = new Button { Content = "OK", HorizontalAlignment = HorizontalAlignment.Right, MinWidth = 72 };
        ok.Click += (_, _) => ApplyFilter();
        stack.Children.Add(ok);
        stack.Children.Add(_message);
        Density.Apply(stack);
        var theme = themeSource?.RequestedTheme ?? ElementTheme.Default;
        Content = new ScrollViewer { Content = new Grid { RequestedTheme = theme, Children = { stack } } };
        Title = "Quick Select";
        AppWindow.Resize(new Windows.Graphics.SizeInt32(360, 420));
        _apply.Items.Add(Item("Drawing", "drawing"));
        _apply.Items.Add(Item("Current selection", "selection"));
        _apply.SelectedIndex = 0;
        _mode.Items.Add(Item("New selection", "new"));
        _mode.Items.Add(Item("Add to selection", "append"));
        _mode.Items.Add(Item("Exclude from selection", "exclude"));
        _mode.SelectedIndex = 0;
        _apply.SelectionChanged += (_, _) => LoadInfo();
        _type.SelectionChanged += (_, _) => LoadInfo();
        LoadInfo();
    }

    private void LoadInfo()
    {
        if (_filling)
        {
            return;
        }
        var apply = TagOf(_apply) ?? "drawing";
        var type = TagOf(_type);
        var query = type == null || type == "*"
            ? JsonSerializer.Serialize(new Dictionary<string, string> { ["applyTo"] = apply })
            : JsonSerializer.Serialize(new Dictionary<string, string> { ["applyTo"] = apply, ["type"] = type });
        if (Native.Execute(_host, "qselect.info", query) != 0)
        {
            _message.Text = Native.LastError(_host);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            var root = doc.RootElement;
            _filling = true;
            var keepType = type ?? "*";
            var keepProperty = TagOf(_property) ?? "";
            var keepOp = TagOf(_operator) ?? "=";
            _type.Items.Clear();
            _type.Items.Add(Item("All", "*"));
            if (root.TryGetProperty("types", out var types))
            {
                foreach (var item in types.EnumerateArray())
                {
                    var name = item.TryGetProperty("type", out var ty) ? ty.GetString() ?? "" : "";
                    var count = item.TryGetProperty("count", out var n) && n.TryGetInt32(out var c) ? c : 0;
                    if (name.Length > 0)
                    {
                        _type.Items.Add(Item(name + " (" + count + ")", name));
                    }
                }
            }
            SelectTag(_type, keepType);
            _property.Items.Clear();
            _property.Items.Add(Item("Any", ""));
            if (root.TryGetProperty("properties", out var properties))
            {
                foreach (var item in properties.EnumerateArray())
                {
                    if (item.GetString() is string name && name.Length > 0)
                    {
                        _property.Items.Add(Item(name, name));
                    }
                }
            }
            SelectTag(_property, keepProperty);
            _operator.Items.Clear();
            if (root.TryGetProperty("operators", out var operators))
            {
                foreach (var item in operators.EnumerateArray())
                {
                    if (item.GetString() is string name && name.Length > 0)
                    {
                        _operator.Items.Add(Item(name, name));
                    }
                }
            }
            if (_operator.Items.Count == 0)
            {
                _operator.Items.Add(Item("=", "="));
            }
            SelectTag(_operator, keepOp);
            _message.Text = "";
        }
        catch (JsonException)
        {
            _message.Text = "Quick Select properties could not be read.";
        }
        finally
        {
            _filling = false;
        }
    }

    private void ApplyFilter()
    {
        var payload = new Dictionary<string, string>
        {
            ["applyTo"] = TagOf(_apply) ?? "drawing",
            ["mode"] = TagOf(_mode) ?? "new",
        };
        var type = TagOf(_type);
        if (type != null && type != "*")
        {
            payload["type"] = type;
        }
        var property = TagOf(_property);
        if (!string.IsNullOrEmpty(property))
        {
            payload["property"] = property;
            payload["operator"] = TagOf(_operator) ?? "=";
            payload["value"] = _value.Text;
        }
        if (Native.Execute(_host, "qselect", JsonSerializer.Serialize(payload)) != 0)
        {
            _message.Text = Native.LastError(_host);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            _message.Text = doc.RootElement.TryGetProperty("message", out var message) ? message.GetString() ?? "" : "";
        }
        catch (JsonException)
        {
            _message.Text = "";
        }
        _changed();
        LoadInfo();
    }

    private static ComboBoxItem Item(string label, string tag) => new() { Content = label, Tag = tag };

    private static string? TagOf(ComboBox box) => box.SelectedItem is ComboBoxItem item ? item.Tag as string : null;

    private static void SelectTag(ComboBox box, string tag)
    {
        for (var i = 0; i < box.Items.Count; i++)
        {
            if (box.Items[i] is ComboBoxItem item && item.Tag as string == tag)
            {
                box.SelectedIndex = i;
                return;
            }
        }
        if (box.Items.Count > 0)
        {
            box.SelectedIndex = 0;
        }
    }

    private static TextBlock Label(string text) => new() { Text = text, Opacity = 0.7 };
}
