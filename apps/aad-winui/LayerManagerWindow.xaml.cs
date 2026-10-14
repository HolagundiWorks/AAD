using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.Storage;
using Windows.System;

namespace Aad.WinUi;

/// <summary>
/// Layer properties for the open drawing. The layer name is the identity (the drawing has no
/// separate layer id). The revision is the document revision. Mutations go through engine commands.
/// </summary>
public sealed partial class LayerManagerWindow : Window
{
    private static readonly string[] Lineweights =
    [
        "Default", "0.00 mm", "0.05 mm", "0.09 mm", "0.13 mm", "0.15 mm", "0.18 mm", "0.20 mm",
        "0.25 mm", "0.30 mm", "0.35 mm", "0.40 mm", "0.50 mm", "0.53 mm", "0.60 mm", "0.70 mm",
        "0.80 mm", "0.90 mm", "1.00 mm", "1.06 mm", "1.20 mm", "1.40 mm", "1.58 mm", "2.00 mm", "2.11 mm",
    ];

    private readonly IntPtr _host;
    private readonly Action _changed;
    private readonly List<LayerRecord> _all = new();
    private readonly List<string> _linetypes = new();
    private ulong _uid;
    private ulong _revision;
    private bool _sortDesc;
    private bool _reloading;
    private bool _acceptText;
    private int _editors;
    private bool _refreshWhenIdle;
    private string _focusName = "";

    public LayerManagerWindow(FrameworkElement? themeSource, IntPtr host, Action changed)
    {
        InitializeComponent();
        _host = host;
        _changed = changed;
        if (Content is FrameworkElement root)
        {
            if (themeSource != null)
            {
                root.RequestedTheme = themeSource.RequestedTheme;
            }
            Density.Apply(root);
        }
        _reloading = true;
        FillFilter(VisibilityFilter, "Visibility", "Any", "On", "Off");
        FillFilter(FreezeFilter, "Freeze", "Any", "Frozen", "Thawed");
        FillFilter(LockFilter, "Lock", "Any", "Locked", "Unlocked");
        FillFilter(PlotFilter, "Plot", "Any", "Plot", "No plot");
        FillFilter(UsageFilter, "Usage", "Any", "Used", "Unused");
        LoadPrefs();
        _reloading = false;
        Closed += (_, _) => SavePrefs();
        AppWindow.Resize(new Windows.Graphics.SizeInt32 { Width = 1100, Height = 680 });
    }

    public void ApplyTheme(ElementTheme theme)
    {
        if (Content is FrameworkElement root)
        {
            root.RequestedTheme = theme;
        }
    }

    public void OnDocumentRevision(ulong uid, ulong revision)
    {
        if (_reloading || (uid == _uid && revision == _revision))
        {
            return;
        }
        if (_editors > 0)
        {
            _refreshWhenIdle = true;
            return;
        }
        Status.Text = "The drawing changed. The list was refreshed.";
        Reload("");
    }

    public void Reload(string focusName)
    {
        _focusName = focusName;
        var selected = SelectedNames();
        if (focusName.Length > 0)
        {
            selected.Add(focusName);
        }
        _reloading = true;
        try
        {
            if (!ReadTable())
            {
                return;
            }
            Show(selected);
        }
        finally
        {
            _reloading = false;
        }
    }

    private void OnListLoaded(object sender, RoutedEventArgs e)
    {
        if (FindScrollViewer(Layers) is ScrollViewer viewer)
        {
            viewer.ViewChanged += (_, _) => HeaderShift.X = -viewer.HorizontalOffset;
        }
    }

    private void OnSearch(object sender, TextChangedEventArgs e) => Show(SelectedNames());

    private void OnFilterChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_reloading)
        {
            return;
        }
        SavePrefs();
        Show(SelectedNames());
    }

    private void OnSort(object sender, RoutedEventArgs e)
    {
        _sortDesc = !_sortDesc;
        SortButton.Content = _sortDesc ? "Name Z–A" : "Name A–Z";
        SavePrefs();
        Show(SelectedNames());
    }

    private void OnNew(object sender, RoutedEventArgs e)
    {
        if (!EnsureCurrent())
        {
            return;
        }
        var name = NextName();
        if (name == null)
        {
            Status.Text = "Could not choose a new layer name.";
            return;
        }
        if (!Run("layer.new", new Dictionary<string, object> { ["name"] = name }))
        {
            return;
        }
        Status.Text = "Created " + name + ".";
        Reload(name);
        _changed();
    }

    private async void OnDelete(object sender, RoutedEventArgs e)
    {
        try
        {
            await DeleteSelection();
        }
        catch (Exception ex)
        {
            Status.Text = ex.Message;
        }
    }

    private void OnUndo(object sender, RoutedEventArgs e) => History("u");

    private void OnRedo(object sender, RoutedEventArgs e) => History("redo");

    private void OnMakeCurrent(object sender, RoutedEventArgs e)
    {
        if (RecordOf(sender) is LayerRecord row)
        {
            SetCurrent(row.Name);
        }
    }

    private void OnNameDoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        if (RecordOf(sender) is LayerRecord row)
        {
            e.Handled = true;
            SetCurrent(row.Name);
        }
    }

    private void OnVisibility(object sender, RoutedEventArgs e) => Toggle(sender, "on");

    private void OnFreeze(object sender, RoutedEventArgs e) => Toggle(sender, "frozen");

    private void OnLock(object sender, RoutedEventArgs e) => Toggle(sender, "locked");

    private void OnPlot(object sender, RoutedEventArgs e) => Toggle(sender, "plot");

    private async void OnColor(object sender, RoutedEventArgs e)
    {
        if (RecordOf(sender) is not LayerRecord row)
        {
            return;
        }
        var next = await Ask("Layer color", "Index, name, or #RRGGBB. ByLayer is not a layer color.", row.ColorName, "Apply");
        if (next == null || next.Trim() == row.ColorName)
        {
            return;
        }
        Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["color"] = next.Trim() });
    }

    private void OnLinetype(object sender, RoutedEventArgs e)
    {
        if (sender is not Button button || RecordOf(sender) is not LayerRecord row)
        {
            return;
        }
        ShowChoices(button, _linetypes, row.Linetype, value => Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["linetype"] = value }));
    }

    private void OnLineweight(object sender, RoutedEventArgs e)
    {
        if (sender is not Button button || RecordOf(sender) is not LayerRecord row)
        {
            return;
        }
        ShowChoices(button, Lineweights, row.Lineweight, value => Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["lineweight"] = value }));
    }

    private async void OnTransparency(object sender, RoutedEventArgs e)
    {
        if (RecordOf(sender) is not LayerRecord row)
        {
            return;
        }
        var next = await Ask("Transparency", "Percent from 0 to 90.", row.Transparency.ToString(), "Apply");
        if (next == null)
        {
            return;
        }
        if (!int.TryParse(next.Trim(), out var percent) || percent < 0 || percent > 90)
        {
            Status.Text = "Transparency is a whole number from 0 to 90.";
            return;
        }
        if (percent == row.Transparency)
        {
            return;
        }
        Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["transparency"] = percent });
    }

    private void OnEditorFocus(object sender, RoutedEventArgs e) => _editors++;

    private void OnDescriptionLost(object sender, RoutedEventArgs e)
    {
        _editors = Math.Max(0, _editors - 1);
        if (!_reloading)
        {
            CommitDescription(sender);
        }
        if (_refreshWhenIdle && _editors == 0)
        {
            _refreshWhenIdle = false;
            Status.Text = "The drawing changed. The list was refreshed.";
            Reload("");
        }
    }

    private void OnDescriptionKey(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Enter && sender is TextBox box)
        {
            e.Handled = true;
            box.Focus(FocusState.Programmatic);
            CommitDescription(sender);
        }
        else if (e.Key == VirtualKey.Escape && sender is TextBox editor && RecordOf(sender) is LayerRecord row)
        {
            e.Handled = true;
            editor.Text = row.Description;
            Layers.Focus(FocusState.Programmatic);
        }
    }

    private void OnRowMenu(object sender, RightTappedRoutedEventArgs e)
    {
        if (RecordOf(e.OriginalSource) is not LayerRecord row)
        {
            return;
        }
        if (!Layers.SelectedItems.Contains(row))
        {
            Layers.SelectedItem = row;
        }
        var menu = new MenuFlyout();
        menu.Items.Add(Item("Make current", () => SetCurrent(row.Name), row.CanMakeCurrent));
        menu.Items.Add(Item("Rename", () => _ = Rename(row), true));
        menu.Items.Add(Item(row.On ? "Turn off" : "Turn on", () => Set(row.Name, Flag(row.Name, "on", !row.On)), true));
        menu.Items.Add(Item(row.Frozen ? "Thaw" : "Freeze", () => Set(row.Name, Flag(row.Name, "frozen", !row.Frozen)), true));
        menu.Items.Add(Item(row.Locked ? "Unlock" : "Lock", () => Set(row.Name, Flag(row.Name, "locked", !row.Locked)), true));
        menu.Items.Add(Item("Delete", () => _ = DeleteSelection(), true));
        menu.ShowAt(Layers, e.GetPosition(Layers));
        e.Handled = true;
    }

    private void OnWindowKey(object sender, KeyRoutedEventArgs e)
    {
        var control = (Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control) & Windows.UI.Core.CoreVirtualKeyStates.Down) != 0;
        if (control && e.Key == VirtualKey.F)
        {
            e.Handled = true;
            SearchBox.Focus(FocusState.Keyboard);
            SearchBox.SelectAll();
            return;
        }
        if (e.Key == VirtualKey.F2 && Layers.SelectedItems.Count == 1 && Layers.SelectedItem is LayerRecord row)
        {
            e.Handled = true;
            _ = Rename(row);
            return;
        }
        if (control && e.Key == VirtualKey.A && FocusManager.GetFocusedElement(Content.XamlRoot) is not TextBox)
        {
            e.Handled = true;
            Layers.SelectAll();
            return;
        }
        if (e.Key == VirtualKey.Delete && FocusManager.GetFocusedElement(Content.XamlRoot) is not TextBox)
        {
            e.Handled = true;
            _ = DeleteSelection();
        }
    }

    private void Toggle(object sender, string field)
    {
        if (sender is not CheckBox box || RecordOf(sender) is not LayerRecord row || box.IsChecked is not bool value)
        {
            return;
        }
        Set(row.Name, Flag(row.Name, field, value));
    }

    private void SetCurrent(string name)
    {
        if (!EnsureCurrent())
        {
            return;
        }
        if (!Run("layer.current", new Dictionary<string, object> { ["name"] = name }))
        {
            Reload(name);
            return;
        }
        Status.Text = name + " is the current layer.";
        Reload(name);
        _changed();
    }

    private void Set(string name, Dictionary<string, object> parameters)
    {
        if (!EnsureCurrent())
        {
            return;
        }
        if (!Run("layer.set", parameters))
        {
            Reload(name);
            return;
        }
        Status.Text = "";
        Reload(name);
        _changed();
    }

    private void CommitDescription(object sender)
    {
        if (sender is not TextBox box || RecordOf(sender) is not LayerRecord row || box.Text == row.Description)
        {
            return;
        }
        Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["description"] = box.Text });
    }

    private async Task Rename(LayerRecord row)
    {
        var next = await Ask("Rename layer", "New name for " + row.Name + ".", row.Name, "Rename");
        if (next == null)
        {
            return;
        }
        next = next.Trim();
        if (next.Length == 0 || next.Equals(row.Name, StringComparison.OrdinalIgnoreCase))
        {
            return;
        }
        Set(row.Name, new Dictionary<string, object> { ["name"] = row.Name, ["newName"] = next });
    }

    private async Task DeleteSelection()
    {
        var names = Layers.SelectedItems.OfType<LayerRecord>().Select(row => row.Name).ToList();
        if (names.Count == 0)
        {
            Status.Text = "Select one or more layers.";
            return;
        }
        var prompt = names.Count == 1
            ? "Delete layer " + names[0] + "? The command keeps the layer when it still has objects."
            : "Delete " + names.Count + " layers? Each deletion is its own undo step. A layer that still has objects stays.";
        var dialog = new ContentDialog
        {
            Title = "Delete layers",
            Content = new TextBlock { Text = prompt, TextWrapping = TextWrapping.Wrap },
            PrimaryButtonText = "Delete",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary || !EnsureCurrent())
        {
            return;
        }
        var failed = new List<string>();
        var removed = 0;
        foreach (var name in names)
        {
            if (Native.Execute(_host, "layer.delete", JsonSerializer.Serialize(new Dictionary<string, object> { ["name"] = name })) != 0)
            {
                failed.Add(name + ": " + Native.LastError(_host));
            }
            else
            {
                removed++;
            }
        }
        Status.Text = failed.Count == 0
            ? "Deleted " + removed + "."
            : "Deleted " + removed + ". " + string.Join(" ", failed);
        Reload("");
        if (removed > 0)
        {
            _changed();
        }
    }

    private void History(string command)
    {
        if (!Run(command, new Dictionary<string, object>()))
        {
            return;
        }
        Status.Text = "";
        Reload("");
        _changed();
    }

    private bool EnsureCurrent()
    {
        if (!TryReadRevision(out var uid, out var revision))
        {
            return true;
        }
        if (uid == _uid && revision == _revision)
        {
            return true;
        }
        Status.Text = "The drawing changed. The list was refreshed.";
        Reload("");
        return false;
    }

    private bool Run(string command, Dictionary<string, object> parameters)
    {
        if (_host == IntPtr.Zero)
        {
            return false;
        }
        if (Native.Execute(_host, command, JsonSerializer.Serialize(parameters)) == 0)
        {
            return true;
        }
        Status.Text = Native.LastError(_host);
        return false;
    }

    private bool ReadTable()
    {
        if (!Run("layer", new Dictionary<string, object>()))
        {
            return false;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            var root = doc.RootElement;
            _all.Clear();
            _linetypes.Clear();
            if (root.TryGetProperty("linetypes", out var linetypes))
            {
                foreach (var name in linetypes.EnumerateArray())
                {
                    var text = name.GetString();
                    if (!string.IsNullOrEmpty(text))
                    {
                        _linetypes.Add(text);
                    }
                }
            }
            if (root.TryGetProperty("layers", out var layers))
            {
                foreach (var layer in layers.EnumerateArray())
                {
                    _all.Add(LayerRecord.From(layer));
                }
            }
        }
        catch (JsonException)
        {
            Status.Text = "The layer table could not be read.";
            return false;
        }
        if (TryReadRevision(out var uid, out var revision))
        {
            _uid = uid;
            _revision = revision;
        }
        return true;
    }

    private bool TryReadRevision(out ulong uid, out ulong revision)
    {
        uid = 0;
        revision = 0;
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadText(_host, Native.TextPanels));
            uid = doc.RootElement.GetProperty("uid").GetUInt64();
            revision = doc.RootElement.GetProperty("revision").GetUInt64();
            return true;
        }
        catch (JsonException)
        {
            return false;
        }
    }

    private void Show(HashSet<string> selected)
    {
        var query = SearchBox.Text.Trim();
        var shown = _all.Where(row => Matches(row, query)).ToList();
        shown.Sort((a, b) =>
        {
            var order = string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase);
            return _sortDesc ? -order : order;
        });
        _reloading = true;
        try
        {
            Layers.ItemsSource = shown;
            Layers.SelectedItems.Clear();
            foreach (var row in shown)
            {
                if (selected.Contains(row.Name))
                {
                    Layers.SelectedItems.Add(row);
                }
            }
        }
        finally
        {
            _reloading = false;
        }
        Empty.Visibility = shown.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        var current = _all.FirstOrDefault(row => row.Current)?.Name ?? "";
        var filters = FilterIndexes().Any(index => index > 0) || query.Length > 0;
        Summary.Text = (filters ? "Filters active. " : "")
            + _all.Count + " layers, " + shown.Count + " shown. Current layer: " + current + ". Filters combine with AND.";
        if (_focusName.Length > 0)
        {
            var focus = _focusName;
            _focusName = "";
            DispatcherQueue.TryEnqueue(() =>
            {
                var match = shown.FirstOrDefault(row => row.Name.Equals(focus, StringComparison.OrdinalIgnoreCase));
                if (match == null)
                {
                    return;
                }
                Layers.ScrollIntoView(match, ScrollIntoViewAlignment.Leading);
                if (Layers.ContainerFromItem(match) is ListViewItem item)
                {
                    item.Focus(FocusState.Programmatic);
                }
            });
        }
    }

    private bool Matches(LayerRecord row, string query)
    {
        if (query.Length > 0 && row.Name.IndexOf(query, StringComparison.OrdinalIgnoreCase) < 0)
        {
            return false;
        }
        if (VisibilityFilter.SelectedIndex == 1 && !row.On) return false;
        if (VisibilityFilter.SelectedIndex == 2 && row.On) return false;
        if (FreezeFilter.SelectedIndex == 1 && !row.Frozen) return false;
        if (FreezeFilter.SelectedIndex == 2 && row.Frozen) return false;
        if (LockFilter.SelectedIndex == 1 && !row.Locked) return false;
        if (LockFilter.SelectedIndex == 2 && row.Locked) return false;
        if (PlotFilter.SelectedIndex == 1 && !row.Plot) return false;
        if (PlotFilter.SelectedIndex == 2 && row.Plot) return false;
        if (UsageFilter.SelectedIndex == 1 && !row.Used) return false;
        if (UsageFilter.SelectedIndex == 2 && row.Used) return false;
        return true;
    }

    private string? NextName()
    {
        for (var i = 1; i <= 10_000; i++)
        {
            var candidate = "Layer" + i;
            if (!_all.Any(row => row.Name.Equals(candidate, StringComparison.OrdinalIgnoreCase)))
            {
                return candidate;
            }
        }
        return null;
    }

    private HashSet<string> SelectedNames()
    {
        var names = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var item in Layers.SelectedItems)
        {
            if (item is LayerRecord row)
            {
                names.Add(row.Name);
            }
        }
        return names;
    }

    private void ShowChoices(FrameworkElement anchor, IReadOnlyList<string> values, string current, Action<string> apply)
    {
        var menu = new MenuFlyout();
        if (values.Count == 0)
        {
            menu.Items.Add(new MenuFlyoutItem { Text = "None loaded", IsEnabled = false });
        }
        foreach (var value in values)
        {
            var item = new MenuFlyoutItem { Text = value, IsEnabled = !value.Equals(current, StringComparison.OrdinalIgnoreCase) };
            var chosen = value;
            item.Click += (_, _) => apply(chosen);
            menu.Items.Add(item);
        }
        menu.ShowAt(anchor);
    }

    private async Task<string?> Ask(string title, string label, string value, string primary)
    {
        var box = new TextBox { Text = value };
        AutomationProperties.SetName(box, label);
        var dialog = new ContentDialog
        {
            Title = title,
            Content = new StackPanel
            {
                Spacing = 8,
                Children =
                {
                    new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap },
                    box,
                },
            },
            PrimaryButtonText = primary,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        _acceptText = false;
        box.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                _acceptText = true;
                dialog.Hide();
            }
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary && !_acceptText)
        {
            return null;
        }
        return box.Text;
    }

    private void LoadPrefs()
    {
        try
        {
            var values = ApplicationData.Current.LocalSettings.Values;
            _sortDesc = values["aad.layers.sortDesc"] as bool? ?? false;
            SortButton.Content = _sortDesc ? "Name Z–A" : "Name A–Z";
            Select(VisibilityFilter, values["aad.layers.visibility"]);
            Select(FreezeFilter, values["aad.layers.freeze"]);
            Select(LockFilter, values["aad.layers.lock"]);
            Select(PlotFilter, values["aad.layers.plot"]);
            Select(UsageFilter, values["aad.layers.usage"]);
        }
        catch (Exception)
        {
            SortButton.Content = "Name A–Z";
        }
    }

    private void SavePrefs()
    {
        try
        {
            var values = ApplicationData.Current.LocalSettings.Values;
            values["aad.layers.sortDesc"] = _sortDesc;
            values["aad.layers.visibility"] = VisibilityFilter.SelectedIndex;
            values["aad.layers.freeze"] = FreezeFilter.SelectedIndex;
            values["aad.layers.lock"] = LockFilter.SelectedIndex;
            values["aad.layers.plot"] = PlotFilter.SelectedIndex;
            values["aad.layers.usage"] = UsageFilter.SelectedIndex;
        }
        catch (Exception)
        {
            // Preferences are optional. The drawing is unaffected.
        }
    }

    private int[] FilterIndexes() =>
    [
        VisibilityFilter.SelectedIndex, FreezeFilter.SelectedIndex, LockFilter.SelectedIndex,
        PlotFilter.SelectedIndex, UsageFilter.SelectedIndex,
    ];

    private static void FillFilter(ComboBox box, string group, params string[] items)
    {
        box.Items.Clear();
        foreach (var item in items)
        {
            box.Items.Add(group + ": " + item);
        }
        box.SelectedIndex = 0;
    }

    private static void Select(ComboBox box, object? stored)
    {
        if (stored is int index && index >= 0 && index < box.Items.Count)
        {
            box.SelectedIndex = index;
        }
    }

    private static Dictionary<string, object> Flag(string name, string field, bool value) =>
        new() { ["name"] = name, [field] = value };

    private static LayerRecord? RecordOf(object sender)
    {
        if (sender is FrameworkElement element)
        {
            return element.DataContext as LayerRecord;
        }
        return null;
    }

    private static ScrollViewer? FindScrollViewer(DependencyObject parent)
    {
        var count = VisualTreeHelper.GetChildrenCount(parent);
        for (var i = 0; i < count; i++)
        {
            var child = VisualTreeHelper.GetChild(parent, i);
            if (child is ScrollViewer viewer)
            {
                return viewer;
            }
            var nested = FindScrollViewer(child);
            if (nested != null)
            {
                return nested;
            }
        }
        return null;
    }

    private static MenuFlyoutItem Item(string text, Action action, bool enabled)
    {
        var item = new MenuFlyoutItem { Text = text, IsEnabled = enabled };
        item.Click += (_, _) => action();
        return item;
    }
}

public sealed class LayerRecord
{
    public string Name { get; init; } = "";
    public bool On { get; init; }
    public bool Frozen { get; init; }
    public bool Locked { get; init; }
    public bool Plot { get; init; }
    public bool Current { get; init; }
    public bool Used { get; init; }
    public string ColorName { get; init; } = "";
    public string Linetype { get; init; } = "";
    public string Lineweight { get; init; } = "";
    public int Transparency { get; init; }
    public string Description { get; init; } = "";
    public Brush Swatch { get; init; } = new SolidColorBrush(Microsoft.UI.Colors.Gray);

    public string CurrentLabel => Current ? "Current" : "Make current";
    public bool CanMakeCurrent => !Current && !Frozen && !Locked;
    public string CurrentAccess =>
        Current ? "Current layer " + Name
        : Frozen ? "Layer " + Name + " is frozen. Thaw it before making it current."
        : Locked ? "Layer " + Name + " is locked. Unlock it before making it current."
        : "Make " + Name + " the current layer";
    public string OnAccess => (On ? "Turn off layer " : "Turn on layer ") + Name;
    public string FreezeAccess => (Frozen ? "Thaw layer " : "Freeze layer ") + Name;
    public string LockAccess => (Locked ? "Unlock layer " : "Lock layer ") + Name;
    public string PlotAccess => (Plot ? "Do not plot layer " : "Plot layer ") + Name;
    public string NameAccess => "Layer " + Name;
    public string ColorAccess => "Layer color: " + ColorName;
    public string LinetypeAccess => "Linetype of layer " + Name + ": " + Linetype;
    public string LineweightAccess => "Lineweight of layer " + Name + ": " + Lineweight;
    public string TransparencyLabel => Transparency + "%";
    public string TransparencyAccess => "Transparency of layer " + Name + ": " + Transparency + " percent";
    public string UsageLabel => Used ? "Used" : "Unused";
    public string UsageAccess => Used ? "Layer " + Name + " has objects" : "Layer " + Name + " has no objects";
    public string DescriptionAccess => "Description of layer " + Name;

    public static LayerRecord From(JsonElement layer)
    {
        var color = Text(layer, "colorRgb");
        return new LayerRecord
        {
            Name = Text(layer, "name"),
            On = Bool(layer, "on"),
            Frozen = Bool(layer, "frozen"),
            Locked = Bool(layer, "locked"),
            Plot = Bool(layer, "plot"),
            Current = Bool(layer, "current"),
            Used = Bool(layer, "used"),
            ColorName = Text(layer, "color"),
            Linetype = Text(layer, "linetype"),
            Lineweight = Text(layer, "lineweight"),
            Transparency = layer.TryGetProperty("transparency", out var transparency) && transparency.TryGetInt32(out var percent) ? percent : 0,
            Description = Text(layer, "description"),
            Swatch = SwatchOf(color),
        };
    }

    private static string Text(JsonElement layer, string name) =>
        layer.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() ?? "" : "";

    private static bool Bool(JsonElement layer, string name) =>
        layer.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.True;

    private static Brush SwatchOf(string hex)
    {
        if (hex.Length == 7 && hex[0] == '#' && Convert.FromHexString(hex.AsSpan(1)) is byte[] rgb && rgb.Length == 3)
        {
            return new SolidColorBrush(Windows.UI.Color.FromArgb(255, rgb[0], rgb[1], rgb[2]));
        }
        return new SolidColorBrush(Microsoft.UI.Colors.Gray);
    }
}
