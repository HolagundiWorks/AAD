using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>Format / Modify &gt; Hatch… — pattern library, scale and angle for selected hatches or defaults.</summary>
internal static class HatchWindow
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    public static Window Open(Window owner, IntPtr host, Action changed)
    {
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var status = Density.Secondary("", TextWrapping.Wrap);
        AutomationProperties.SetLiveSetting(status, AutomationLiveSetting.Polite);
        Density.Name(status, "Hatch status");

        var list = new ListBox { MinHeight = 220, MinWidth = 280 };
        Density.Name(list, "Hatch patterns");
        var selectionLabel = Density.Secondary("Selection: —", TextWrapping.Wrap);
        Density.Name(selectionLabel, "Hatch selection");
        var scale = new TextBox { MinHeight = Density.ControlHeight, Width = 100 };
        Density.Name(scale, "Hatch scale");
        var angle = new TextBox { MinHeight = Density.ControlHeight, Width = 100 };
        Density.Name(angle, "Hatch angle");
        var apply = ActionButton("Apply to selection");
        var setDefaults = ActionButton("Set as defaults");

        var scaleRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceSm };
        scaleRow.Children.Add(new TextBlock { Text = "Scale", VerticalAlignment = VerticalAlignment.Center });
        scaleRow.Children.Add(scale);
        scaleRow.Children.Add(new TextBlock { Text = "Angle", VerticalAlignment = VerticalAlignment.Center, Margin = new Thickness(12, 0, 0, 0) });
        scaleRow.Children.Add(angle);

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceSm };
        actions.Children.Add(apply);
        actions.Children.Add(setDefaults);

        var root = new StackPanel { Spacing = Density.SpaceMd, Margin = Density.DialogMargin, MaxWidth = 440 };
        root.Children.Add(Heading("Hatch Manager"));
        root.Children.Add(selectionLabel);
        root.Children.Add(list);
        root.Children.Add(scaleRow);
        root.Children.Add(actions);
        root.Children.Add(status);

        var handles = new List<string>();

        void Fail(string message) => status.Text = message;

        bool Run(string command, object body)
        {
            if (Native.Execute(host, command, JsonSerializer.Serialize(body, JsonOpts)) != 0)
            {
                Fail(Native.LastError(host));
                return false;
            }
            status.Text = "";
            changed();
            return true;
        }

        string GetVar(string name)
        {
            if (Native.Execute(host, "getvar", JsonSerializer.Serialize(new { name }, JsonOpts)) != 0)
            {
                return "";
            }
            try
            {
                using var doc = JsonDocument.Parse(Native.ReadResult(host));
                if (!doc.RootElement.TryGetProperty("value", out var value))
                {
                    return "";
                }
                return value.ValueKind switch
                {
                    JsonValueKind.String => value.GetString() ?? "",
                    JsonValueKind.Number => value.ToString(),
                    _ => value.ToString(),
                };
            }
            catch (JsonException)
            {
                return "";
            }
        }

        bool SetVar(string name, object value) =>
            Run("setvar", new Dictionary<string, object> { ["name"] = name, ["value"] = value });

        string SelectedPattern()
        {
            var item = list.SelectedItem as string ?? "";
            var sep = item.IndexOf(" —", StringComparison.Ordinal);
            return sep < 0 ? item : item[..sep];
        }

        bool ParsePositive(TextBox box, out double value)
        {
            value = 0;
            var text = (box.Text ?? "").Trim();
            if (!double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out value)
                && !double.TryParse(text, NumberStyles.Float, CultureInfo.CurrentCulture, out value))
            {
                return false;
            }
            return value > 0 && double.IsFinite(value);
        }

        bool ParseAngle(out double degrees)
        {
            degrees = 0;
            var text = (angle.Text ?? "").Trim();
            if (!double.TryParse(text, NumberStyles.Float, CultureInfo.InvariantCulture, out degrees)
                && !double.TryParse(text, NumberStyles.Float, CultureInfo.CurrentCulture, out degrees))
            {
                return false;
            }
            return double.IsFinite(degrees);
        }

        void Reload()
        {
            handles.Clear();
            if (!Run("hatch.patterns", new { }))
            {
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(Native.ReadResult(host));
                var pattern = "ANSI31";
                var scaleValue = 1.0;
                var angleValue = 0.0;
                if (doc.RootElement.TryGetProperty("defaults", out var defaults))
                {
                    pattern = defaults.TryGetProperty("pattern", out var p) ? p.GetString() ?? pattern : pattern;
                    if (defaults.TryGetProperty("scale", out var s) && s.TryGetDouble(out var sv) && sv > 0)
                    {
                        scaleValue = sv;
                    }
                    if (defaults.TryGetProperty("angle", out var a) && a.TryGetDouble(out var av))
                    {
                        angleValue = av;
                    }
                }
                scale.Text = scaleValue.ToString("0.####", CultureInfo.InvariantCulture);
                angle.Text = angleValue.ToString("0.####", CultureInfo.InvariantCulture);
                list.Items.Clear();
                if (doc.RootElement.TryGetProperty("patterns", out var rows) && rows.ValueKind == JsonValueKind.Array)
                {
                    foreach (var row in rows.EnumerateArray())
                    {
                        var name = row.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                        if (name.Length == 0)
                        {
                            continue;
                        }
                        var desc = row.TryGetProperty("description", out var d) ? d.GetString() ?? "" : "";
                        list.Items.Add(desc.Length == 0 ? name : name + " — " + desc);
                    }
                }
                var sel = list.Items.Cast<object>().Select(o => o as string ?? "").FirstOrDefault(item =>
                    item.Equals(pattern, StringComparison.OrdinalIgnoreCase)
                    || item.StartsWith(pattern + " —", StringComparison.OrdinalIgnoreCase));
                if (sel != null)
                {
                    list.SelectedItem = sel;
                }
            }
            catch (JsonException)
            {
                Fail("The hatch pattern list could not be read.");
                return;
            }

            // Current selection: how many hatches can Apply change.
            if (Native.Execute(host, "properties", "{}") == 0)
            {
                try
                {
                    using var props = JsonDocument.Parse(Native.ReadResult(host));
                    if (props.RootElement.TryGetProperty("objects", out var objects) && objects.ValueKind == JsonValueKind.Array)
                    {
                        foreach (var row in objects.EnumerateArray())
                        {
                            var type = row.TryGetProperty("type", out var t) ? t.GetString() ?? "" : "";
                            var handle = row.TryGetProperty("handle", out var h) ? h.GetString() ?? "" : "";
                            if (type.Equals("Hatch", StringComparison.OrdinalIgnoreCase) && handle.Length > 0)
                            {
                                handles.Add(handle);
                            }
                        }
                    }
                }
                catch (JsonException)
                {
                    // Keep defaults-only mode.
                }
            }
            selectionLabel.Text = handles.Count == 0
                ? "Selection: none (Apply needs a hatch selected)"
                : handles.Count == 1
                    ? "Selection: 1 hatch"
                    : "Selection: " + handles.Count + " hatches";
            _ = GetVar("HPNAME");
        }

        apply.Click += (_, _) =>
        {
            if (handles.Count == 0)
            {
                Fail("Select one or more hatch objects in the drawing first.");
                return;
            }
            var name = SelectedPattern();
            if (name.Length == 0)
            {
                Fail("Choose a hatch pattern.");
                return;
            }
            if (!ParsePositive(scale, out var scaleValue))
            {
                Fail("Scale must be a positive number.");
                return;
            }
            if (!ParseAngle(out var angleValue))
            {
                Fail("Angle must be a number (degrees).");
                return;
            }
            if (Run("hatchedit", new { handles, pattern = name, scale = scaleValue, angle = angleValue }))
            {
                status.Text = handles.Count == 1 ? "1 hatch updated." : handles.Count + " hatches updated.";
                Reload();
            }
        };

        setDefaults.Click += (_, _) =>
        {
            var name = SelectedPattern();
            if (name.Length == 0)
            {
                Fail("Choose a hatch pattern.");
                return;
            }
            if (!ParsePositive(scale, out var scaleValue))
            {
                Fail("Scale must be a positive number.");
                return;
            }
            if (!ParseAngle(out var angleValue))
            {
                Fail("Angle must be a number (degrees).");
                return;
            }
            // Store radians in HPANG (engine header), degrees in the UI.
            if (SetVar("HPNAME", name)
                && SetVar("HPSCALE", scaleValue)
                && SetVar("HPANG", angleValue * Math.PI / 180.0))
            {
                status.Text = "Defaults updated for new hatches.";
                Reload();
            }
        };

        Density.Apply(root);
        var window = new Window
        {
            Title = "Hatch Manager",
            Content = new ScrollViewer
            {
                Content = new Grid { RequestedTheme = theme, Children = { root } },
            },
        };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = true;
        }
        ShellIcon.Apply(window, theme);
        ShellFit.UserResizable(window, minWidthDip: 400, minHeightDip: 360, maxWidthDip: 720, maxHeightDip: 800, persistKey: "hatch");
        Reload();
        ShellFit.Invalidate(window);
        return window;
    }

    private static Button ActionButton(string label)
    {
        var button = new Button { Content = label, MinHeight = Density.ControlHeight };
        Density.Name(button, label);
        return button;
    }

    private static TextBlock Heading(string text)
    {
        var block = new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        Density.Name(block, text);
        return block;
    }
}
