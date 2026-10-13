using System.Runtime.InteropServices.WindowsRuntime;
using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Data.Pdf;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;
using WinRT.Interop;

namespace Aad.WinUi;

public sealed partial class MainWindow : Window
{
    private static readonly JsonSerializerOptions JsonOpts = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };

    private IntPtr _host;
    private DispatcherQueueTimer? _timer;
    private int _ticks;
    private string _panelKey = "";
    private bool _classic = true;
    private bool _dark = true;
    private Window? _pdfWindow;
    private Window? _formWindow;

    internal void HoldForm(Window? window) => _formWindow = window;

    public MainWindow()
    {
        InitializeComponent();
        _host = Native.Create(sample: true);
        Closed += (_, _) =>
        {
            _timer?.Stop();
            if (_host != IntPtr.Zero)
            {
                Native.Destroy(_host);
                _host = IntPtr.Zero;
            }
        };
        Activated += (_, _) => PlaceChild();
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        BuildMenus();
        RefreshChrome();
        RefreshPanels();
        ApplyShellTheme();
    }

    private void OnCanvasSize(object sender, SizeChangedEventArgs e)
    {
        PlaceChild();
        EnsureTimer();
    }

    private void OnCommandKey(object sender, KeyRoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (e.Key == Windows.System.VirtualKey.Enter)
        {
            e.Handled = true;
            var text = CommandInput.Text;
            if (string.IsNullOrWhiteSpace(text) && Native.GripCycle(_host))
            {
                RefreshChrome();
                return;
            }
            CommandInput.Text = "";
            Native.CommandLine(_host, text);
            RefreshChrome();
            RefreshPanels();
            BuildMenus();
        }
        else if (e.Key == Windows.System.VirtualKey.Escape)
        {
            e.Handled = true;
            CommandInput.Text = "";
            Native.Cancel(_host);
            RefreshChrome();
            RefreshPanels();
            BuildMenus();
        }
        else if (e.Key == Windows.System.VirtualKey.Tab)
        {
            var next = Native.DynTab(_host, CommandInput.Text);
            if (next != null)
            {
                e.Handled = true;
                CommandInput.Text = next;
                CommandInput.SelectionStart = next.Length;
            }
            else if (string.IsNullOrEmpty(CommandInput.Text))
            {
                e.Handled = true;
                Native.SnapTab(_host);
            }
        }
        else if (e.Key == Windows.System.VirtualKey.Space && string.IsNullOrEmpty(CommandInput.Text) && Native.GripCycle(_host))
        {
            e.Handled = true;
            RefreshChrome();
        }
    }

    private void OnDraft(object sender, TextChangedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        Native.Draft(_host, CommandInput.Text);
    }

    private void OnMenuCommand(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero || sender is not MenuFlyoutItem item || item.Tag is not string id)
        {
            return;
        }
        if (id is "open" or "qsave" or "saveas")
        {
            _ = PickFile(id);
            return;
        }
        var form = Native.ReadForm(_host, id);
        if (form != null)
        {
            FormDialog.Show(this, _host, form, AfterCommand);
            return;
        }
        Native.CommandLine(_host, id);
        AfterCommand();
    }

    private async Task PickFile(string id)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        try
        {
            await PickFileCore(id);
        }
        catch (Exception ex)
        {
            History.Text = ex.Message;
        }
    }

    private async Task PickFileCore(string id)
    {
        var hwnd = WindowNative.GetWindowHandle(this);
        if (id == "open")
        {
            var picker = new FileOpenPicker();
            InitializeWithWindow.Initialize(picker, hwnd);
            picker.FileTypeFilter.Add(".dxf");
            picker.FileTypeFilter.Add(".dwg");
            var file = await picker.PickSingleFileAsync();
            if (file == null)
            {
                return;
            }
            if (Native.Execute(_host, "open", PathJson(file.Path)) != 0)
            {
                History.Text = Native.LastError(_host);
            }
            else
            {
                Native.Execute(_host, "zoom.extents", "{}");
            }
            AfterCommand();
            return;
        }
        if (id == "qsave")
        {
            if (Native.Execute(_host, "qsave", "{}") == 0)
            {
                AfterCommand();
                return;
            }
            var err = Native.LastError(_host);
            if (!err.Contains("no file name", StringComparison.OrdinalIgnoreCase))
            {
                History.Text = err;
                AfterCommand();
                return;
            }
        }
        var save = new FileSavePicker();
        InitializeWithWindow.Initialize(save, hwnd);
        save.FileTypeChoices.Add("DXF", new List<string> { ".dxf" });
        save.FileTypeChoices.Add("DWG", new List<string> { ".dwg" });
        save.SuggestedFileName = "drawing";
        var chosen = await save.PickSaveFileAsync();
        if (chosen == null)
        {
            return;
        }
        if (Native.Execute(_host, "saveas", PathJson(chosen.Path)) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        AfterCommand();
    }

    private static string PathJson(string path) =>
        JsonSerializer.Serialize(new Dictionary<string, string> { ["path"] = path });

    private void OnWorkspace(object sender, RoutedEventArgs e)
    {
        _classic = !_classic;
        PropertiesColumn.Width = new GridLength(_classic ? 220 : 0);
        BlocksColumn.Width = new GridLength(_classic ? 260 : 0);
        LeftSplitterColumn.Width = _classic ? GridLength.Auto : new GridLength(0);
        RightSplitterColumn.Width = _classic ? GridLength.Auto : new GridLength(0);
        WorkspaceButton.Content = _classic ? "Classic" : "Pro";
        PlaceChild();
    }

    private Border? _dragSplit;
    private double _dragX;
    private double _dragWidth;

    private void OnSplitPressed(object sender, PointerRoutedEventArgs e)
    {
        if (sender is not Border splitter)
        {
            return;
        }
        _dragSplit = splitter;
        _dragX = e.GetCurrentPoint(Content).Position.X;
        _dragWidth = splitter == LeftSplitter ? PropertiesColumn.ActualWidth : BlocksColumn.ActualWidth;
        splitter.CapturePointer(e.Pointer);
    }

    private void OnSplitMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_dragSplit == null || !ReferenceEquals(sender, _dragSplit))
        {
            return;
        }
        var delta = e.GetCurrentPoint(Content).Position.X - _dragX;
        if (_dragSplit == LeftSplitter)
        {
            PropertiesColumn.Width = new GridLength(Math.Clamp(_dragWidth + delta, 140, 480));
        }
        else
        {
            BlocksColumn.Width = new GridLength(Math.Clamp(_dragWidth - delta, 180, 560));
        }
        PlaceChild();
    }

    private void OnSplitReleased(object sender, PointerRoutedEventArgs e)
    {
        if (sender is Border splitter)
        {
            splitter.ReleasePointerCapture(e.Pointer);
        }
        _dragSplit = null;
    }

    private void OnTheme(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        _dark = !_dark;
        Native.Theme(_host, _dark);
        ThemeButton.Content = _dark ? "Dark" : "Light";
        ApplyShellTheme();
    }

    private void ApplyShellTheme()
    {
        if (Content is FrameworkElement root)
        {
            root.RequestedTheme = _dark ? ElementTheme.Dark : ElementTheme.Light;
        }
        CanvasHost.Background = new SolidColorBrush(_dark ? Microsoft.UI.Colors.Black : Microsoft.UI.Colors.White);
        var title = AppWindow.TitleBar;
        var bg = _dark ? Color(0x20, 0x20, 0x20) : Color(0xF3, 0xF3, 0xF3);
        var fg = _dark ? Microsoft.UI.Colors.White : Color(0x1A, 0x1A, 0x1A);
        var inactive = _dark ? Color(0x9A, 0x9A, 0x9A) : Color(0x5C, 0x5C, 0x5C);
        var hover = _dark ? Color(0x2D, 0x2D, 0x2D) : Color(0xE5, 0xE5, 0xE5);
        var pressed = _dark ? Color(0x3A, 0x3A, 0x3A) : Color(0xD6, 0xD6, 0xD6);
        title.BackgroundColor = bg;
        title.InactiveBackgroundColor = bg;
        title.ForegroundColor = fg;
        title.InactiveForegroundColor = inactive;
        title.ButtonBackgroundColor = bg;
        title.ButtonInactiveBackgroundColor = bg;
        title.ButtonForegroundColor = fg;
        title.ButtonInactiveForegroundColor = inactive;
        title.ButtonHoverBackgroundColor = hover;
        title.ButtonHoverForegroundColor = fg;
        title.ButtonPressedBackgroundColor = pressed;
        title.ButtonPressedForegroundColor = fg;
    }

    private static Windows.UI.Color Color(byte r, byte g, byte b) =>
        Windows.UI.Color.FromArgb(255, r, g, b);

    private async void OnPdf(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var bytes = Native.ReadPdf(_host);
        if (bytes == null)
        {
            History.Text = Native.LastError(_host);
            return;
        }
        try
        {
            await ShowPdfAsync(bytes, null);
        }
        catch (Exception ex)
        {
            History.Text = ex.Message;
        }
    }

    private async Task ShowPdfAsync(byte[]? bytes, Windows.Storage.StorageFile? file)
    {
        PdfDocument doc;
        IRandomAccessStream? owned = null;
        if (file != null)
        {
            doc = await PdfDocument.LoadFromFileAsync(file);
        }
        else
        {
            owned = new InMemoryRandomAccessStream();
            await owned.WriteAsync((bytes ?? Array.Empty<byte>()).AsBuffer());
            owned.Seek(0);
            doc = await PdfDocument.LoadFromStreamAsync(owned);
        }
        if (doc.PageCount == 0)
        {
            History.Text = "The PDF has no pages.";
            owned?.Dispose();
            return;
        }
        var image = new Image { Stretch = Stretch.Uniform, MaxWidth = 860 };
        var caption = new TextBlock();
        var open = new Button { Content = "Open PDF…", HorizontalAlignment = HorizontalAlignment.Left };
        var pageBox = new StackPanel { Spacing = 8 };
        pageBox.Children.Add(open);
        pageBox.Children.Add(caption);
        pageBox.Children.Add(new ScrollViewer
        {
            Content = image,
            MaxWidth = 880,
            MaxHeight = 640,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        });
        async Task ShowPage(uint index)
        {
            using var page = doc.GetPage(index);
            var rendered = new InMemoryRandomAccessStream();
            var options = new PdfPageRenderOptions
            {
                DestinationWidth = (uint)Math.Clamp(page.Size.Width * 2.0, 1, 4096),
                DestinationHeight = (uint)Math.Clamp(page.Size.Height * 2.0, 1, 4096),
            };
            await page.RenderToStreamAsync(rendered, options);
            rendered.Seek(0);
            var bitmap = new BitmapImage();
            await bitmap.SetSourceAsync(rendered);
            image.Source = bitmap;
            caption.Text = $"Page {index + 1} of {doc.PageCount}";
        }
        await ShowPage(0);
        open.Click += async (_, _) =>
        {
            try
            {
                var owner = _pdfWindow ?? this;
                var picker = new FileOpenPicker();
                InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(owner));
                picker.FileTypeFilter.Add(".pdf");
                var picked = await picker.PickSingleFileAsync();
                if (picked == null)
                {
                    return;
                }
                doc = await PdfDocument.LoadFromFileAsync(picked);
                if (doc.PageCount == 0)
                {
                    caption.Text = "The PDF has no pages.";
                    return;
                }
                await ShowPage(0);
            }
            catch (Exception ex)
            {
                caption.Text = ex.Message;
            }
        };
        var root = new Grid { Padding = new Thickness(12), Children = { pageBox } };
        if (Content is FrameworkElement shell)
        {
            root.RequestedTheme = shell.RequestedTheme;
        }
        if (_pdfWindow == null)
        {
            _pdfWindow = new Window { Title = "PDF" };
            _pdfWindow.Closed += (_, _) =>
            {
                owned?.Dispose();
                _pdfWindow = null;
            };
        }
        _pdfWindow.Content = root;
        _pdfWindow.AppWindow.Resize(new Windows.Graphics.SizeInt32 { Width = 980, Height = 760 });
        _pdfWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_pdfWindow), 8);
    }

    private void AfterCommand()
    {
        RefreshChrome();
        RefreshPanels();
        BuildMenus();
        CommandInput.Focus(FocusState.Programmatic);
    }

    private void BuildMenus()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        List<MenuNode>? menus;
        try
        {
            menus = JsonSerializer.Deserialize<List<MenuNode>>(Native.ReadText(_host, Native.TextMenu), JsonOpts);
        }
        catch (JsonException)
        {
            History.Text = "The command menu could not be read.";
            return;
        }
        Bar.Items.Clear();
        if (menus == null)
        {
            return;
        }
        foreach (var menu in menus)
        {
            var top = new MenuBarItem { Title = menu.Title };
            foreach (var child in menu.Items)
            {
                top.Items.Add(MakeItem(child, 0));
            }
            Bar.Items.Add(top);
        }
    }

    private MenuFlyoutItemBase MakeItem(MenuNode node, int depth)
    {
        if (depth > 8 || string.IsNullOrEmpty(node.Id))
        {
            var sub = new MenuFlyoutSubItem { Text = node.Title };
            if (depth <= 8)
            {
                foreach (var child in node.Items)
                {
                    sub.Items.Add(MakeItem(child, depth + 1));
                }
            }
            return sub;
        }
        var item = new MenuFlyoutItem
        {
            Text = node.Title,
            Tag = node.Id,
            IsEnabled = node.Enabled,
        };
        if (!string.IsNullOrEmpty(node.Shortcut))
        {
            item.KeyboardAcceleratorTextOverride = node.Shortcut;
        }
        item.Click += OnMenuCommand;
        return item;
    }

    private void RefreshChrome()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        ChromeState? state = null;
        try
        {
            state = JsonSerializer.Deserialize<ChromeState>(Native.ReadText(_host, Native.TextChrome), JsonOpts);
        }
        catch (JsonException)
        {
            History.Text = Native.LastError(_host);
        }
        if (state != null)
        {
            History.Text = string.Join(Environment.NewLine, state.History);
            CommandInput.PlaceholderText = state.Prompt;
            if (HistoryScroll.IsLoaded)
            {
                HistoryScroll.ChangeView(null, HistoryScroll.ScrollableHeight, null);
            }
        }
        var stats = Native.ReadStats(_host);
        Status.Text = stats.entities + " entities";
    }

    private void RefreshPanels()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        PanelSnapshot? snapshot;
        try
        {
            snapshot = JsonSerializer.Deserialize<PanelSnapshot>(Native.ReadText(_host, Native.TextPanels), JsonOpts);
        }
        catch (JsonException)
        {
            return;
        }
        if (snapshot == null)
        {
            return;
        }
        var key = snapshot.Uid + ":" + snapshot.Revision + ":" + snapshot.Selection;
        if (key == _panelKey)
        {
            return;
        }
        _panelKey = key;
        FillProperties(snapshot);
        FillLayers(snapshot);
        FillBlocks(snapshot);
    }

    private void FillProperties(PanelSnapshot snapshot)
    {
        PropertiesPanel.Children.Clear();
        PropertiesPanel.Children.Add(Heading("Properties"));
        foreach (var row in snapshot.Properties)
        {
            var line = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            line.Children.Add(new TextBlock { Text = row.Label, Width = 96, Opacity = 0.7 });
            line.Children.Add(new TextBlock { Text = row.Value });
            PropertiesPanel.Children.Add(line);
        }
    }

    private void FillLayers(PanelSnapshot snapshot)
    {
        LayersPanel.Children.Clear();
        LayersPanel.Children.Add(Heading("Layers"));
        foreach (var layer in snapshot.Layers)
        {
            var block = new StackPanel { Spacing = 2, Margin = new Thickness(0, 0, 0, 6) };
            var title = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
            title.Children.Add(new Border
            {
                Width = 12,
                Height = 12,
                Margin = new Thickness(0, 4, 0, 0),
                Background = Swatch(layer.Color),
            });
            var name = layer.Name;
            var current = new Button
            {
                Content = name,
                Tag = name,
                FontWeight = layer.Current ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
            };
            current.Click += (_, _) => RunLayer("layer.current", new Dictionary<string, object> { ["name"] = name });
            title.Children.Add(current);
            block.Children.Add(title);
            var switches = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
            var on = layer.On;
            switches.Children.Add(Toggle(name, on, "on"));
            switches.Children.Add(Toggle(name, layer.Frozen, "frozen"));
            switches.Children.Add(Toggle(name, layer.Locked, "locked"));
            block.Children.Add(switches);
            LayersPanel.Children.Add(block);
        }
        if (snapshot.LayersTruncated)
        {
            LayersPanel.Children.Add(new TextBlock { Text = "Further layers are not listed.", Opacity = 0.7 });
        }
    }

    private void FillBlocks(PanelSnapshot snapshot)
    {
        BlocksPanel.Children.Clear();
        BlocksPanel.Children.Add(Heading("Blocks"));
        if (snapshot.Blocks.Count == 0)
        {
            BlocksPanel.Children.Add(new TextBlock { Text = "This drawing defines no blocks.", Opacity = 0.7 });
        }
        foreach (var block in snapshot.Blocks)
        {
            var name = block.Name;
            var line = new StackPanel { Spacing = 0 };
            line.Children.Add(new TextBlock { Text = name });
            line.Children.Add(new TextBlock
            {
                Text = block.Entities + " objects · " + block.References + " references",
                Opacity = 0.7,
                FontSize = 12,
            });
            var insert = new Button { Content = "Insert", Tag = name, HorizontalAlignment = HorizontalAlignment.Left };
            insert.Click += (_, _) =>
            {
                if (_host == IntPtr.Zero || Native.CommandLine(_host, "insert") != 0)
                {
                    History.Text = Native.LastError(_host);
                    return;
                }
                Native.CommandLine(_host, name);
                _panelKey = "";
                RefreshChrome();
                RefreshPanels();
            };
            line.Children.Add(insert);
            BlocksPanel.Children.Add(line);
        }
        if (snapshot.BlocksTruncated)
        {
            BlocksPanel.Children.Add(new TextBlock { Text = "Further blocks are not listed.", Opacity = 0.7 });
        }
    }

    private Button Toggle(string layer, bool value, string field)
    {
        var shown = field switch
        {
            "on" => value ? "On" : "Off",
            "frozen" => value ? "Frozen" : "Thaw",
            _ => value ? "Locked" : "Unlocked",
        };
        var button = new Button { Content = shown, Tag = layer, MinWidth = 28 };
        button.Click += (_, _) => RunLayer("layer.set", new Dictionary<string, object> { ["name"] = layer, [field] = !value });
        return button;
    }

    private void RunLayer(string command, Dictionary<string, object> parameters)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var json = JsonSerializer.Serialize(parameters);
        if (Native.Execute(_host, command, json) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        _panelKey = "";
        RefreshChrome();
        RefreshPanels();
    }

    private static TextBlock Heading(string text) => new() { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };

    private static Brush Swatch(string hex)
    {
        if (hex.Length == 7 && hex[0] == '#' && Convert.FromHexString(hex.AsSpan(1)) is byte[] rgb && rgb.Length == 3)
        {
            return new SolidColorBrush(Windows.UI.Color.FromArgb(255, rgb[0], rgb[1], rgb[2]));
        }
        return new SolidColorBrush(Microsoft.UI.Colors.Gray);
    }

    private void PlaceChild()
    {
        if (_host == IntPtr.Zero || CanvasHost.XamlRoot == null || Content is not FrameworkElement root)
        {
            return;
        }
        var scale = CanvasHost.XamlRoot.RasterizationScale;
        if (scale <= 0 || CanvasHost.ActualWidth < 1 || CanvasHost.ActualHeight < 1)
        {
            return;
        }
        var point = CanvasHost.TransformToVisual(root).TransformPoint(new Windows.Foundation.Point(0, 0));
        int x = (int)Math.Round(point.X * scale);
        int y = (int)Math.Round(point.Y * scale);
        int w = Math.Max(1, (int)Math.Round(CanvasHost.ActualWidth * scale));
        int h = Math.Max(1, (int)Math.Round(CanvasHost.ActualHeight * scale));
        var parent = WindowNative.GetWindowHandle(this);
        var rc = Native.IsEmbedded(_host)
            ? Native.Move(_host, x, y, (uint)w, (uint)h)
            : Native.Embed(_host, parent, x, y, (uint)w, (uint)h);
        if (rc != 0)
        {
            History.Text = Native.LastError(_host);
        }
    }

    private void EnsureTimer()
    {
        if (_timer != null)
        {
            return;
        }
        var queue = DispatcherQueue.GetForCurrentThread();
        _timer = queue.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(32);
        _timer.IsRepeating = true;
        _timer.Tick += (_, _) =>
        {
            if (_host == IntPtr.Zero)
            {
                return;
            }
            if (Native.Pump(_host) != 0)
            {
                Status.Text = Native.LastError(_host);
            }
            _ticks++;
            if (_ticks % 8 == 0)
            {
                RefreshPanels();
            }
        };
        _timer.Start();
    }

    private sealed class MenuNode
    {
        public string Title { get; set; } = "";
        public List<MenuNode> Items { get; set; } = new();
        public string? Id { get; set; }
        public string? Shortcut { get; set; }
        public bool Enabled { get; set; } = true;
    }

    private sealed class ChromeState
    {
        public string Prompt { get; set; } = "Command:";
        public List<string> History { get; set; } = new();
    }

    private sealed class PanelSnapshot
    {
        public ulong Uid { get; set; }
        public ulong Revision { get; set; }
        public int Selection { get; set; }
        public List<PropertyRow> Properties { get; set; } = new();
        public List<LayerRow> Layers { get; set; } = new();
        public bool LayersTruncated { get; set; }
        public List<BlockRow> Blocks { get; set; } = new();
        public bool BlocksTruncated { get; set; }
    }

    private sealed class PropertyRow
    {
        public string Label { get; set; } = "";
        public string Value { get; set; } = "";
    }

    private sealed class LayerRow
    {
        public string Name { get; set; } = "";
        public bool On { get; set; }
        public bool Frozen { get; set; }
        public bool Locked { get; set; }
        public string Color { get; set; } = "";
        public bool Current { get; set; }
    }

    private sealed class BlockRow
    {
        public string Name { get; set; } = "";
        public int Entities { get; set; }
        public int References { get; set; }
    }
}
