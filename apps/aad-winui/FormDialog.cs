using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Settings dialog for an engine form. A separate window, because the drawing child covers XAML.</summary>
internal static class FormDialog
{
    public static void Show(MainWindow owner, IntPtr host, string json, Action done)
    {
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;
        var id = root.GetProperty("id").GetString() ?? "";
        var title = root.GetProperty("title").GetString() ?? id;
        var fields = root.GetProperty("fields");
        var rooms = root.TryGetProperty("rooms", out var roomList) ? roomList : default;

        var stack = new StackPanel { Spacing = 8 };
        var scalars = new Dictionary<string, TextBox>();
        var checks = new Dictionary<string, CheckBox>();
        var choices = new Dictionary<string, ComboBox>();
        var tables = new Dictionary<string, (JsonElement Columns, StackPanel Rows)>();
        var picks = false;

        foreach (var field in fields.EnumerateArray())
        {
            var key = field.GetProperty("key").GetString() ?? "";
            var label = field.GetProperty("label").GetString() ?? key;
            var kind = field.GetProperty("kind").GetString() ?? "text";
            if (field.TryGetProperty("pick", out var pick) && pick.ValueKind == JsonValueKind.True)
            {
                picks = true;
                continue;
            }
            stack.Children.Add(Density.Secondary(label, TextWrapping.Wrap));
            switch (kind)
            {
                case "bool":
                    var box = new CheckBox { IsChecked = BoolOf(field) };
                    Density.Name(box, label);
                    checks[key] = box;
                    stack.Children.Add(box);
                    break;
                case "choice":
                case "room":
                    var combo = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch, MinHeight = Density.MinTarget };
                    Density.Name(combo, label);
                    if (kind == "room" && rooms.ValueKind == JsonValueKind.Array)
                    {
                        foreach (var room in rooms.EnumerateArray())
                        {
                            var code = room.GetProperty("code").GetString() ?? "";
                            var name = room.GetProperty("name").GetString() ?? code;
                            combo.Items.Add(new ComboBoxItem { Content = code + "  " + name, Tag = code });
                        }
                    }
                    else if (field.TryGetProperty("choices", out var list))
                    {
                        foreach (var choice in list.EnumerateArray())
                        {
                            var text = choice.GetString() ?? "";
                            combo.Items.Add(new ComboBoxItem { Content = text.Length == 0 ? "(default)" : text, Tag = text });
                        }
                    }
                    Select(combo, StringOf(field));
                    choices[key] = combo;
                    stack.Children.Add(combo);
                    break;
                case "table":
                    var columns = field.GetProperty("columns");
                    var rows = new StackPanel { Spacing = 4 };
                    if (field.TryGetProperty("value", out var value) && value.ValueKind == JsonValueKind.Array)
                    {
                        foreach (var row in value.EnumerateArray())
                        {
                            rows.Children.Add(TableRow(columns, row));
                        }
                    }
                    if (rows.Children.Count == 0)
                    {
                        rows.Children.Add(TableRow(columns, default));
                    }
                    var add = new Button { Content = "Add row", HorizontalAlignment = HorizontalAlignment.Left, MinHeight = Density.MinTarget };
                    Density.Name(add, "Add row to " + label);
                    add.Click += (_, _) =>
                    {
                        if (rows.Children.Count < 24)
                        {
                            rows.Children.Add(TableRow(columns, default));
                        }
                    };
                    tables[key] = (columns.Clone(), rows);
                    stack.Children.Add(rows);
                    stack.Children.Add(add);
                    break;
                default:
                    var input = new TextBox { Text = StringOf(field), MinHeight = Density.MinTarget };
                    Density.Name(input, label);
                    scalars[key] = input;
                    stack.Children.Add(input);
                    break;
            }
        }
        if (picks)
        {
            stack.Children.Add(Density.Secondary("Points are picked on the drawing after OK.", TextWrapping.Wrap));
        }
        var error = new TextBlock { TextWrapping = TextWrapping.Wrap };
        AutomationProperties.SetLiveSetting(error, AutomationLiveSetting.Assertive);
        Density.Name(error, "Form error");
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var ok = new Button { Content = "OK", MinHeight = Density.MinTarget };
        var cancel = new Button { Content = "Cancel", MinHeight = Density.MinTarget };
        Density.Name(ok, "OK");
        Density.Name(cancel, "Cancel");
        buttons.Children.Add(ok);
        buttons.Children.Add(cancel);
        stack.Children.Add(error);
        stack.Children.Add(buttons);

        var page = new Grid { Padding = new Thickness(12), Children = { new ScrollViewer { Content = stack } } };
        if (owner.Content is FrameworkElement shell)
        {
            page.RequestedTheme = shell.RequestedTheme;
        }
        var window = new Window { Title = title, Content = page };
        ShellIcon.Apply(window);
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32 { Width = 560, Height = 640 });
        ok.Click += (_, _) =>
        {
            var preset = Collect(scalars, checks, choices, tables);
            if (Native.StartForm(host, id, preset) != 0)
            {
                error.Text = Native.LastError(host);
                return;
            }
            window.Close();
            done();
        };
        cancel.Click += (_, _) => window.Close();
        window.Closed += (_, _) => owner.HoldForm(null);
        owner.HoldForm(window);
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
    }

    private static string Collect(
        Dictionary<string, TextBox> scalars,
        Dictionary<string, CheckBox> checks,
        Dictionary<string, ComboBox> choices,
        Dictionary<string, (JsonElement Columns, StackPanel Rows)> tables)
    {
        var body = new Dictionary<string, object?>();
        foreach (var (key, box) in scalars)
        {
            body[key] = box.Text;
        }
        foreach (var (key, box) in checks)
        {
            body[key] = box.IsChecked == true;
        }
        foreach (var (key, box) in choices)
        {
            body[key] = box.SelectedItem is ComboBoxItem item ? item.Tag as string ?? "" : "";
        }
        foreach (var (key, table) in tables)
        {
            var rows = new List<List<string>>();
            foreach (var child in table.Rows.Children)
            {
                if (child is not StackPanel row)
                {
                    continue;
                }
                var cells = new List<string>();
                foreach (var cell in row.Children)
                {
                    if (cell is TextBox text)
                    {
                        cells.Add(text.Text);
                    }
                }
                rows.Add(cells);
            }
            body[key] = rows;
        }
        return JsonSerializer.Serialize(body);
    }

    private static StackPanel TableRow(JsonElement columns, JsonElement row)
    {
        var line = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        var i = 0;
        foreach (var column in columns.EnumerateArray())
        {
            var text = "";
            if (row.ValueKind == JsonValueKind.Array && i < row.GetArrayLength())
            {
                text = Cell(row[i]);
            }
            else if (row.ValueKind == JsonValueKind.Object && column.TryGetProperty("key", out var key))
            {
                var name = key.GetString() ?? "";
                if (row.TryGetProperty(name, out var cell))
                {
                    text = Cell(cell);
                }
            }
            var label = column.GetProperty("label").GetString() ?? "Cell";
            var box = new TextBox { Text = text, Width = 88, MinHeight = Density.MinTarget, PlaceholderText = label };
            Density.Name(box, label);
            line.Children.Add(box);
            i++;
        }
        return line;
    }

    private static void Select(ComboBox combo, string value)
    {
        foreach (var item in combo.Items)
        {
            if (item is ComboBoxItem choice && choice.Tag as string == value)
            {
                combo.SelectedItem = choice;
                return;
            }
        }
        if (combo.Items.Count > 0)
        {
            combo.SelectedIndex = 0;
        }
    }

    private static bool BoolOf(JsonElement field)
    {
        if (!field.TryGetProperty("value", out var value))
        {
            return false;
        }
        if (value.ValueKind == JsonValueKind.True)
        {
            return true;
        }
        return value.ValueKind == JsonValueKind.String && value.GetString() == "true";
    }

    private static string StringOf(JsonElement field)
    {
        if (!field.TryGetProperty("value", out var value) || value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined)
        {
            return field.TryGetProperty("default", out var fallback) ? fallback.GetString() ?? "" : "";
        }
        return Cell(value);
    }

    private static string Cell(JsonElement value) => value.ValueKind switch
    {
        JsonValueKind.String => value.GetString() ?? "",
        JsonValueKind.Number => value.ToString(),
        JsonValueKind.True => "true",
        JsonValueKind.False => "false",
        _ => "",
    };
}
