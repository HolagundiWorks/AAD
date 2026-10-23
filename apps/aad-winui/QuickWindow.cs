using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;

namespace Aad.WinUi;

/// <summary>Quick Properties for the current selection. Edits go through <c>properties.set</c>.</summary>
internal sealed class QuickWindow : Window
{
    private static readonly string[] Colors = ["ByLayer", "ByBlock", "Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "White"];

    private readonly Action<string, string> _run;
    private readonly StackPanel _stack = new() { Spacing = Density.SpaceSm, Margin = Density.DialogMargin };
    private readonly ComboBox _color = new() { MinWidth = 140, MinHeight = Density.ControlHeight };
    private readonly ComboBox _layer = new() { MinWidth = 140, MinHeight = Density.ControlHeight };
    private readonly ComboBox _linetype = new() { MinWidth = 140, MinHeight = Density.ControlHeight };
    private readonly Dictionary<string, TextBox> _fields = new();
    private List<string> _handles = new();
    private string _applied = "";
    private bool _filling;
    private bool _placed;

    public QuickWindow(FrameworkElement? themeSource, Action<string, string> run)
    {
        _run = run;
        var theme = ShellChrome.Resolve(themeSource?.RequestedTheme ?? Density.Theme);
        Content = new ScrollViewer { Content = new Grid { RequestedTheme = theme, Children = { _stack } } };
        Title = "Quick Properties";
        ShellIcon.Apply(this, theme);
        ShellFit.UserResizable(this, minWidthDip: 280, minHeightDip: 240, maxWidthDip: 420, maxHeightDip: 640);
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsAlwaysOnTop = true;
            presenter.IsResizable = false;
        }
        _color.SelectionChanged += (_, _) => CommitChoice("color", _color);
        _layer.SelectionChanged += (_, _) => CommitChoice("layer", _layer);
        _linetype.SelectionChanged += (_, _) => CommitChoice("linetype", _linetype);
    }

    public void Place(Window owner)
    {
        if (_placed)
        {
            return;
        }
        _placed = true;
        var pos = owner.AppWindow.Position;
        AppWindow.Move(new Windows.Graphics.PointInt32(pos.X + 72, pos.Y + 120));
    }

    /// <summary>Rebuilds when the snapshot changes and the user is not typing in a field.</summary>
    public bool Apply(string json)
    {
        if (json == _applied)
        {
            return true;
        }
        if (Content is FrameworkElement root && root.XamlRoot != null && FocusManager.GetFocusedElement(root.XamlRoot) is TextBox)
        {
            return false;
        }
        try
        {
            using var doc = JsonDocument.Parse(json);
            Fill(doc.RootElement);
            _applied = json;
            return true;
        }
        catch (JsonException)
        {
            return true;
        }
    }

    private void Fill(JsonElement root)
    {
        _filling = true;
        _stack.Children.Clear();
        _fields.Clear();
        _handles = new List<string>();
        if (root.TryGetProperty("handles", out var handles))
        {
            foreach (var handle in handles.EnumerateArray())
            {
                if (handle.GetString() is string hex && hex.Length > 0)
                {
                    _handles.Add(hex);
                }
            }
        }
        Title = root.TryGetProperty("title", out var title) ? title.GetString() ?? "Quick Properties" : "Quick Properties";
        _stack.Children.Add(Row("Color", _color));
        FillChoices(_color, Colors, Text(root, "color"));
        _stack.Children.Add(Row("Layer", _layer));
        FillChoices(_layer, Names(root, "layers"), Text(root, "layer"));
        _stack.Children.Add(Row("Linetype", _linetype));
        FillChoices(_linetype, Names(root, "linetypes"), Text(root, "linetype"));
        if (root.TryGetProperty("fields", out var fields))
        {
            foreach (var field in fields.EnumerateArray())
            {
                var label = field.TryGetProperty("label", out var labelValue) ? labelValue.GetString() ?? "" : "";
                var value = field.TryGetProperty("value", out var valueNode) ? valueNode.GetString() ?? "" : "";
                var key = field.TryGetProperty("key", out var keyNode) ? keyNode.GetString() ?? "" : "";
                var text = field.TryGetProperty("text", out var textNode) && textNode.ValueKind == JsonValueKind.True;
                var box = new TextBox { Text = value, MinWidth = 140, MinHeight = Density.ControlHeight, Tag = value };
                if (key.Length > 0)
                {
                    _fields[key] = box;
                    var fieldKey = key;
                    box.LostFocus += (_, _) => CommitField(fieldKey, text);
                    box.KeyDown += (_, args) =>
                    {
                        if (args.Key == Windows.System.VirtualKey.Enter)
                        {
                            CommitField(fieldKey, text);
                            args.Handled = true;
                        }
                    };
                }
                else
                {
                    box.IsReadOnly = true;
                }
                _stack.Children.Add(Row(label, box));
            }
        }
        var close = new Button { Content = "Turn off", HorizontalAlignment = HorizontalAlignment.Right, MinHeight = Density.ControlHeight };
        Density.Name(close, "Turn off quick properties");
        close.Click += (_, _) => _run("qpmode", "{\"on\":false}");
        _stack.Children.Add(close);
        Density.Apply(_stack);
        _filling = false;
    }

    private void CommitChoice(string key, ComboBox box)
    {
        if (_filling || _handles.Count == 0 || box.SelectedItem is not ComboBoxItem item || item.Tag is not string value || value.Length == 0 || value == "*VARIES*")
        {
            return;
        }
        var payload = new Dictionary<string, object> { ["handles"] = _handles, [key] = value };
        _run("properties.set", JsonSerializer.Serialize(payload));
    }

    private void CommitField(string key, bool text)
    {
        if (_filling || _handles.Count == 0 || !_fields.TryGetValue(key, out var box))
        {
            return;
        }
        if (box.Tag as string == box.Text)
        {
            return;
        }
        if (key is "centerX" or "centerY")
        {
            if (!_fields.TryGetValue("centerX", out var xBox) || !_fields.TryGetValue("centerY", out var yBox))
            {
                return;
            }
            if (!double.TryParse(xBox.Text, out var x) || !double.TryParse(yBox.Text, out var y))
            {
                return;
            }
            var payload = new Dictionary<string, object> { ["handles"] = _handles, ["center"] = new[] { x, y } };
            _run("properties.set", JsonSerializer.Serialize(payload));
            xBox.Tag = xBox.Text;
            yBox.Tag = yBox.Text;
            return;
        }
        Dictionary<string, object> body;
        if (text)
        {
            body = new Dictionary<string, object> { ["handles"] = _handles, [key] = box.Text };
        }
        else if (double.TryParse(box.Text, out var number) && double.IsFinite(number))
        {
            body = new Dictionary<string, object> { ["handles"] = _handles, [key] = number };
        }
        else
        {
            return;
        }
        _run("properties.set", JsonSerializer.Serialize(body));
        box.Tag = box.Text;
    }

    private static void FillChoices(ComboBox box, IEnumerable<string> names, string current)
    {
        box.Items.Clear();
        var seen = false;
        foreach (var name in names)
        {
            box.Items.Add(new ComboBoxItem { Content = name, Tag = name });
            if (name == current)
            {
                seen = true;
            }
        }
        if (!seen && current.Length > 0)
        {
            box.Items.Insert(0, new ComboBoxItem { Content = current, Tag = current });
        }
        for (var i = 0; i < box.Items.Count; i++)
        {
            if (box.Items[i] is ComboBoxItem item && item.Tag as string == current)
            {
                box.SelectedIndex = i;
                return;
            }
        }
    }

    private static string Text(JsonElement root, string name) =>
        root.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() ?? "" : "";

    private static List<string> Names(JsonElement root, string name)
    {
        var names = new List<string>();
        if (root.TryGetProperty(name, out var items))
        {
            foreach (var item in items.EnumerateArray())
            {
                if (item.GetString() is string text && text.Length > 0)
                {
                    names.Add(text);
                }
            }
        }
        return names;
    }

    private static StackPanel Row(string label, UIElement editor)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceMd };
        var caption = Density.Secondary(label);
        caption.Width = 88;
        caption.VerticalAlignment = VerticalAlignment.Center;
        row.Children.Add(caption);
        row.Children.Add(editor);
        Density.Name(editor, label);
        return row;
    }
}
