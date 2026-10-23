using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>File &gt; Batch Plot — sheets of the current drawing via <c>plot.batch</c>.</summary>
internal static class BatchPlotWindow
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
        Density.Name(status, "Batch plot status");

        var list = new StackPanel { Spacing = Density.SpaceSm };
        Density.Name(list, "Sheets");
        var folder = new TextBox { MinHeight = Density.ControlHeight, PlaceholderText = "Exports beside the drawing (or temp)" };
        Density.Name(folder, "Output folder");
        var browse = ActionButton("Browse…");
        var includeHidden = new CheckBox
        {
            Content = "Also plot hidden, frozen and unmarked project items that are ticked",
            MinHeight = Density.ControlHeight,
        };
        Density.Name(includeHidden, "Include hidden");
        var plot = ActionButton("Plot");
        var selectAll = ActionButton("Select all");
        var selectNone = ActionButton("Select none");
        var checks = new Dictionary<string, CheckBox>(StringComparer.OrdinalIgnoreCase);

        var folderRow = new Grid { ColumnSpacing = Density.SpaceSm };
        folderRow.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        folderRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(folder, 0);
        Grid.SetColumn(browse, 1);
        folderRow.Children.Add(folder);
        folderRow.Children.Add(browse);

        var selectRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = Density.SpaceSm };
        selectRow.Children.Add(selectAll);
        selectRow.Children.Add(selectNone);

        var root = new StackPanel { Spacing = Density.SpaceMd, Margin = Density.DialogMargin, MaxWidth = 480 };
        root.Children.Add(Heading("Sheets of this drawing"));
        root.Children.Add(Density.Secondary("Each ticked layout is written as a PDF into the folder below.", TextWrapping.Wrap));
        root.Children.Add(selectRow);
        root.Children.Add(new ScrollViewer { Content = list, MaxHeight = 280 });
        root.Children.Add(Labeled("Folder", folderRow));
        root.Children.Add(includeHidden);
        root.Children.Add(plot);
        root.Children.Add(status);

        void Fail(string message) => status.Text = message;

        void LoadSheets()
        {
            list.Children.Clear();
            checks.Clear();
            if (Native.Execute(host, "pagesetup", "{}") != 0)
            {
                Fail(Native.LastError(host));
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(Native.ReadResult(host));
                if (!doc.RootElement.TryGetProperty("layouts", out var layouts) || layouts.ValueKind != JsonValueKind.Array)
                {
                    Fail("This drawing has no layouts.");
                    return;
                }
                foreach (var item in layouts.EnumerateArray())
                {
                    var name = item.TryGetProperty("name", out var n) ? n.GetString() ?? "" : "";
                    if (name.Length == 0)
                    {
                        continue;
                    }
                    var box = new CheckBox { Content = name, IsChecked = true, MinHeight = Density.ControlHeight };
                    Density.Name(box, "Sheet " + name);
                    checks[name] = box;
                    list.Children.Add(box);
                }
                if (checks.Count == 0)
                {
                    Fail("This drawing has no layouts.");
                }
            }
            catch (JsonException)
            {
                Fail("The layout list could not be read.");
            }
        }

        selectAll.Click += (_, _) =>
        {
            foreach (var box in checks.Values)
            {
                box.IsChecked = true;
            }
        };
        selectNone.Click += (_, _) =>
        {
            foreach (var box in checks.Values)
            {
                box.IsChecked = false;
            }
        };
        browse.Click += async (_, _) =>
        {
            var picker = new FolderPicker();
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(owner));
            picker.FileTypeFilter.Add("*");
            var picked = await picker.PickSingleFolderAsync();
            if (picked != null)
            {
                folder.Text = picked.Path;
            }
        };
        plot.Click += (_, _) =>
        {
            var sheets = checks.Where(kv => kv.Value.IsChecked == true).Select(kv => kv.Key).ToList();
            if (sheets.Count == 0)
            {
                Fail("Tick at least one sheet.");
                return;
            }
            var body = new Dictionary<string, object?>
            {
                ["sheets"] = sheets,
                ["includeHidden"] = includeHidden.IsChecked == true,
            };
            var dir = (folder.Text ?? "").Trim();
            if (dir.Length > 0)
            {
                body["folder"] = dir;
            }
            if (Native.Execute(host, "plot.batch", JsonSerializer.Serialize(body, JsonOpts)) != 0)
            {
                Fail(Native.LastError(host));
                return;
            }
            try
            {
                using var doc = JsonDocument.Parse(Native.ReadResult(host));
                status.Text = doc.RootElement.TryGetProperty("message", out var msg) ? msg.GetString() ?? "Plotted." : "Plotted.";
            }
            catch (JsonException)
            {
                status.Text = "Plotted.";
            }
            changed();
        };

        Density.Apply(root);
        var window = new Window
        {
            Title = "Batch Plot",
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
        ShellFit.UserResizable(window, minWidthDip: 440, minHeightDip: 360, maxWidthDip: 720, maxHeightDip: 720);
        LoadSheets();
        return window;
    }

    private static Button ActionButton(string label)
    {
        var button = new Button { Content = label, MinHeight = Density.ControlHeight };
        Density.Name(button, label);
        return button;
    }

    private static FrameworkElement Labeled(string label, FrameworkElement field)
    {
        var row = new StackPanel { Spacing = Density.SpaceXs };
        var text = new TextBlock { Text = label };
        Density.Name(text, label);
        row.Children.Add(text);
        row.Children.Add(field);
        return row;
    }

    private static TextBlock Heading(string text)
    {
        var block = new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        Density.Name(block, text);
        return block;
    }
}
