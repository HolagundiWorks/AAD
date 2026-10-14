using System.Runtime.InteropServices.WindowsRuntime;
using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.ApplicationModel.DataTransfer;
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
    private string _layoutKey = "";
    private string _docKey = "";
    private bool _filling;
    private bool _historyOpen;
    private bool _asking;
    private bool _allowClose;
    private string _ownClipboard = "";
    private static readonly (string Id, string Label)[] Tools =
    [
        ("line", "Line"), ("pline", "Polyline"), ("circle", "Circle"), ("arc", "Arc"), ("rectang", "Rectangle"),
        ("move", "Move"), ("copy", "Copy"), ("rotate", "Rotate"), ("offset", "Offset"), ("trim", "Trim"),
        ("erase", "Erase"), ("dimlinear", "Dimension"), ("text", "Text"), ("hatch", "Hatch"),
    ];
    private bool _classic = true;
    private bool _dark = true;
    private Window? _pdfWindow;
    private Window? _formWindow;
    private LayerManagerWindow? _layersWindow;
    private Window? _unitsWindow;
    private DraftingWindow? _draftingWindow;
    private QuickWindow? _quickWindow;
    private QuickSelectWindow? _quickSelectWindow;
    private string? _quickJson;
    private string _command = "";
    private bool _qpmode;

    internal void HoldForm(Window? window) => _formWindow = window;

    public MainWindow()
    {
        InitializeComponent();
        AppWindow.Closing += OnAppClosing;
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
        BuildTools();
        RefreshChrome();
        RefreshPanels();
        ApplyShellTheme();
        if (Content is DependencyObject root)
        {
            Density.Apply(root);
        }
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
            if (Native.GripPoint(_host, text))
            {
                CommandInput.Text = "";
                RefreshChrome();
                RefreshPanels();
                return;
            }
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
        else if (e.Key == Windows.System.VirtualKey.Space && (Native.GripPoint(_host, CommandInput.Text) || (string.IsNullOrWhiteSpace(CommandInput.Text) && Native.GripCycle(_host))))
        {
            e.Handled = true;
            CommandInput.Text = "";
            RefreshChrome();
            RefreshPanels();
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
        StartCommand(id);
    }

    private void StartCommand(string id)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (id is "open" or "qsave" or "saveas")
        {
            _ = PickFile(id);
            return;
        }
        if (id == "close")
        {
            _ = CloseActive();
            return;
        }
        if (id == "layer")
        {
            OpenLayers();
            return;
        }
        if (id == "dsettings")
        {
            OpenDrafting();
            return;
        }
        if (id == "qselect")
        {
            OpenQuickSelect();
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

    private void OnShellKey(object sender, KeyRoutedEventArgs e)
    {
        if (_host == IntPtr.Zero || Typing())
        {
            return;
        }
        var ctrl = KeyDown(Windows.System.VirtualKey.Control);
        var shift = KeyDown(Windows.System.VirtualKey.Shift);
        if (e.Key == Windows.System.VirtualKey.F1)
        {
            e.Handled = true;
            CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
            return;
        }
        if (e.Key == Windows.System.VirtualKey.F2)
        {
            e.Handled = true;
            _historyOpen = !_historyOpen;
            HistoryScroll.MaxHeight = _historyOpen ? 240 : 72;
            return;
        }
        if (ctrl && e.Key == Windows.System.VirtualKey.F11)
        {
            e.Handled = true;
            if (shift)
            {
                PropertiesColumn.Width = new GridLength(165);
                BlocksColumn.Width = new GridLength(195);
            }
            else
            {
                OnWorkspace(this, e);
            }
            return;
        }
        if (ctrl && e.Key == Windows.System.VirtualKey.W)
        {
            e.Handled = true;
            _ = CloseActive();
            return;
        }
        if (ctrl && e.Key == Windows.System.VirtualKey.Q)
        {
            e.Handled = true;
            _ = CloseAll(quit: true);
            return;
        }
        if (ctrl && e.Key is Windows.System.VirtualKey.Number9 or Windows.System.VirtualKey.NumberPad9)
        {
            e.Handled = true;
            CommandInput.Focus(FocusState.Programmatic);
            return;
        }
        if (e.Key == Windows.System.VirtualKey.Delete && !ctrl)
        {
            e.Handled = true;
            RunShortcut("erase");
            return;
        }
        var id = (ctrl, shift, e.Key) switch
        {
            (true, true, Windows.System.VirtualKey.Z) => "redo",
            (true, false, Windows.System.VirtualKey.Z) => "u",
            (true, false, Windows.System.VirtualKey.Y) => "redo",
            (true, false, Windows.System.VirtualKey.N) => "new",
            (true, false, Windows.System.VirtualKey.O) => "open",
            (true, true, Windows.System.VirtualKey.S) => "saveas",
            (true, false, Windows.System.VirtualKey.S) => "qsave",
            (true, false, Windows.System.VirtualKey.P) => "plot",
            (true, false, Windows.System.VirtualKey.A) => "selectall",
            (true, true, Windows.System.VirtualKey.C) => "copybase",
            (true, false, Windows.System.VirtualKey.C) => "copyclip",
            (true, false, Windows.System.VirtualKey.X) => "cutclip",
            (true, true, Windows.System.VirtualKey.V) => "pasteblock",
            (true, false, Windows.System.VirtualKey.V) => "pasteclip",
            (false, false, Windows.System.VirtualKey.F3) => "osnap",
            (false, false, Windows.System.VirtualKey.F8) => "ortho",
            (false, false, Windows.System.VirtualKey.F10) => "polar",
            (false, false, Windows.System.VirtualKey.F11) => "otrack",
            (false, false, Windows.System.VirtualKey.F12) => "dynmode",
            _ => "",
        };
        if (id.Length == 0)
        {
            return;
        }
        e.Handled = true;
        if (id is "open" or "qsave" or "saveas")
        {
            _ = PickFile(id);
            return;
        }
        if (id is "pasteclip" or "pasteblock")
        {
            _ = Paste(id);
            return;
        }
        RunShortcut(id);
    }

    private static bool KeyDown(Windows.System.VirtualKey key) =>
        (InputKeyboardSource.GetKeyStateForCurrentThread(key) & Windows.UI.Core.CoreVirtualKeyStates.Down) == Windows.UI.Core.CoreVirtualKeyStates.Down;

    private bool Typing()
    {
        if (Content is not FrameworkElement root || root.XamlRoot == null)
        {
            return false;
        }
        return FocusManager.GetFocusedElement(root.XamlRoot) is TextBox or ComboBox;
    }

    private void RunShortcut(string id)
    {
        if (id is "copyclip" or "cutclip" or "copybase")
        {
            if (Native.Execute(_host, id, "{}") != 0)
            {
                History.Text = Native.LastError(_host);
            }
            else
            {
                PublishClipboard();
            }
            AfterCommand();
            return;
        }
        if (id is "u" or "redo" or "selectall" or "erase" or "new" or "osnap" or "ortho" or "polar" or "otrack" or "dynmode")
        {
            if (Native.Execute(_host, id, "{}") != 0)
            {
                History.Text = Native.LastError(_host);
            }
            _docKey = "";
            AfterCommand();
            return;
        }
        StartCommand(id);
    }

    private void PublishClipboard()
    {
        var text = Native.ReadClipboard(_host);
        if (text.Length == 0)
        {
            return;
        }
        _ownClipboard = text;
        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
    }

    private async Task Paste(string id)
    {
        try
        {
            var view = Clipboard.GetContent();
            if (view.Contains(StandardDataFormats.Text))
            {
                var text = await view.GetTextAsync();
                if (text.Length > 0 && text != _ownClipboard)
                {
                    Native.LoadClipboard(_host, text);
                }
            }
        }
        catch (Exception ex)
        {
            History.Text = ex.Message;
        }
        if (Native.Execute(_host, id, "{}") != 0)
        {
            History.Text = Native.LastError(_host);
        }
        AfterCommand();
    }

    private void OnMode(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero || sender is not ToggleButton button || button.Tag is not string id || button.IsChecked is not bool on)
        {
            return;
        }
        var json = JsonSerializer.Serialize(new Dictionary<string, object> { ["on"] = on });
        if (Native.Execute(_host, id, json) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        RefreshChrome();
    }

    private void OnNewLayout(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (Native.Execute(_host, "layout.new", "{}") != 0)
        {
            History.Text = Native.LastError(_host);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            var name = doc.RootElement.TryGetProperty("name", out var value) ? value.GetString() : null;
            if (!string.IsNullOrEmpty(name))
            {
                Native.Execute(_host, "layout.set", JsonSerializer.Serialize(new Dictionary<string, object> { ["name"] = name }));
            }
        }
        catch (JsonException)
        {
            History.Text = "The new layout could not be read.";
        }
        _layoutKey = "";
        AfterCommand();
    }

    private async Task<bool> PickFile(string id)
    {
        if (_host == IntPtr.Zero)
        {
            return false;
        }
        try
        {
            return await PickFileCore(id);
        }
        catch (Exception ex)
        {
            History.Text = ex.Message;
            return false;
        }
    }

    private async Task<bool> PickFileCore(string id)
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
                return false;
            }
            if (Native.Execute(_host, "open", PathJson(file.Path)) != 0)
            {
                History.Text = Native.LastError(_host);
                AfterCommand();
                return false;
            }
            Native.Execute(_host, "zoom.extents", "{}");
            _docKey = "";
            AfterCommand();
            return true;
        }
        if (id == "qsave")
        {
            if (Native.Execute(_host, "qsave", "{}") == 0)
            {
                _docKey = "";
                AfterCommand();
                return true;
            }
            var err = Native.LastError(_host);
            if (!err.Contains("no file name", StringComparison.OrdinalIgnoreCase))
            {
                History.Text = err;
                AfterCommand();
                return false;
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
            return false;
        }
        if (Native.Execute(_host, "saveas", PathJson(chosen.Path)) != 0)
        {
            History.Text = Native.LastError(_host);
            AfterCommand();
            return false;
        }
        _docKey = "";
        AfterCommand();
        return true;
    }

    private void OnAppClosing(Microsoft.UI.Windowing.AppWindow sender, Microsoft.UI.Windowing.AppWindowClosingEventArgs args)
    {
        if (_allowClose)
        {
            return;
        }
        args.Cancel = true;
        _ = CloseAll(quit: true);
    }

    private async Task CloseActive()
    {
        if (_asking || _host == IntPtr.Zero)
        {
            return;
        }
        _asking = true;
        try
        {
            _docKey = "";
            RefreshChrome();
            var active = _documents.FirstOrDefault(doc => doc.Active) ?? _documents.FirstOrDefault();
            if (active != null && active.Dirty && !await DiscardOrSave(active.Title))
            {
                return;
            }
            var index = active?.Index ?? 0;
            if (Native.Execute(_host, "close", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = index })) != 0)
            {
                History.Text = Native.LastError(_host);
                return;
            }
            _docKey = "";
            RefreshChrome();
            if (_documents.Count == 0)
            {
                Native.Execute(_host, "new", "{}");
            }
            _panelKey = "";
            _layoutKey = "";
            AfterCommand();
        }
        finally
        {
            _asking = false;
        }
    }

    private async Task CloseAll(bool quit)
    {
        if (_asking || _host == IntPtr.Zero)
        {
            return;
        }
        _asking = true;
        try
        {
            while (true)
            {
                _docKey = "";
                RefreshChrome();
                var dirty = _documents.FirstOrDefault(doc => doc.Dirty);
                if (dirty == null)
                {
                    break;
                }
                if (!dirty.Active)
                {
                    Native.Execute(_host, "document.switch", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = dirty.Index }));
                    _docKey = "";
                    RefreshChrome();
                }
                if (!await DiscardOrSave(dirty.Title))
                {
                    return;
                }
                if (Native.Execute(_host, "close", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = dirty.Index })) != 0)
                {
                    History.Text = Native.LastError(_host);
                    return;
                }
            }
            if (quit)
            {
                _allowClose = true;
                Close();
                return;
            }
        }
        finally
        {
            _asking = false;
        }
    }

    private async Task<bool> DiscardOrSave(string title)
    {
        var answer = await SavePrompt.Ask(this, title.Length == 0 ? "drawing" : title);
        if (answer == 0)
        {
            return false;
        }
        if (answer == 2)
        {
            return true;
        }
        return await PickFile("qsave");
    }

    private static string PathJson(string path) =>
        JsonSerializer.Serialize(new Dictionary<string, string> { ["path"] = path });

    private void OnWorkspace(object sender, RoutedEventArgs e)
    {
        _classic = !_classic;
        PropertiesColumn.Width = new GridLength(_classic ? 165 : 0);
        BlocksColumn.Width = new GridLength(_classic ? 195 : 0);
        LeftSplitterColumn.Width = _classic ? GridLength.Auto : new GridLength(0);
        RightSplitterColumn.Width = _classic ? GridLength.Auto : new GridLength(0);
        WorkspaceButton.Content = _classic ? "Classic" : "Pro";
        ToolRow.Height = _classic ? GridLength.Auto : new GridLength(0);
        ToolStripHost.Visibility = _classic ? Visibility.Visible : Visibility.Collapsed;
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
            PropertiesColumn.Width = new GridLength(Math.Clamp(_dragWidth + delta, 105, 360));
        }
        else
        {
            BlocksColumn.Width = new GridLength(Math.Clamp(_dragWidth - delta, 135, 420));
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
        _layersWindow?.ApplyTheme(_dark ? ElementTheme.Dark : ElementTheme.Light);
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
        EnsureHelp();
    }

    private void EnsureHelp()
    {
        MenuBarItem? help = null;
        foreach (var item in Bar.Items)
        {
            if (item is MenuBarItem top && top.Title?.ToString() == "Help")
            {
                help = top;
            }
        }
        if (help == null)
        {
            help = new MenuBarItem { Title = "Help" };
            Bar.Items.Add(help);
        }
        var commands = new MenuFlyoutItem { Text = "Command list", KeyboardAcceleratorTextOverride = "F1" };
        commands.Click += (_, _) => CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
        var about = new MenuFlyoutItem { Text = "About AAD" };
        about.Click += (_, _) => AboutWindow.Show(this, VersionText());
        help.Items.Add(commands);
        help.Items.Add(about);
    }

    private string VersionText()
    {
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadText(_host, Native.TextChrome));
            if (doc.RootElement.TryGetProperty("version", out var version) && version.GetString() is string text)
            {
                return text;
            }
        }
        catch (JsonException)
        {
        }
        return "";
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
            var history = string.Join(Environment.NewLine, state.History);
            if (History.Text != history)
            {
                History.Text = history;
                if (HistoryScroll.IsLoaded)
                {
                    HistoryScroll.ChangeView(null, HistoryScroll.ScrollableHeight, null);
                }
            }
            PromptLabel.Text = state.Prompt;
            CommandInput.PlaceholderText = state.Prompt;
            Coords.Text = state.Cursor;
            SnapButton.IsChecked = state.Osnap;
            OrthoButton.IsChecked = state.Ortho;
            PolarButton.IsChecked = state.Polar;
            TrackButton.IsChecked = state.Otrack;
            DynButton.IsChecked = state.Dynmode;
            WeightButton.IsChecked = state.Lwdisplay;
            TransparencyButton.IsChecked = state.Transparency;
            QuickButton.IsChecked = state.Qpmode;
            _qpmode = state.Qpmode;
            _command = state.Command;
            if (!string.IsNullOrEmpty(state.Units))
            {
                UnitsButton.Content = state.Units;
            }
            ShowLayouts(state);
            ShowDocuments(state);
            _draftingWindow?.Reload(state.Polar, state.Osnap, state.Dynmode, state.Ortho, state.Osmode, state.Polarang, state.DynpiCartesian, state.DynpiAbsolute);
            UpdateQuick();
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
            var text = Native.ReadText(_host, Native.TextPanels);
            using var document = JsonDocument.Parse(text);
            _quickJson = document.RootElement.TryGetProperty("quick", out var quick) && quick.ValueKind == JsonValueKind.Object ? quick.GetRawText() : null;
            snapshot = JsonSerializer.Deserialize<PanelSnapshot>(text, JsonOpts);
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
        FillContext(snapshot);
        Density.Apply(PropertiesPanel);
        Density.Apply(LayersPanel);
        Density.Apply(BlocksPanel);
        Density.Apply(ContextBar);
        _layersWindow?.OnDocumentRevision(snapshot.Uid, snapshot.Revision);
        UpdateQuick();
    }

    private void OpenQuickSelect()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_quickSelectWindow == null)
        {
            _quickSelectWindow = new QuickSelectWindow(Content as FrameworkElement, _host, AfterCommand);
            _quickSelectWindow.Closed += (_, _) => _quickSelectWindow = null;
        }
        _quickSelectWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_quickSelectWindow), 8);
    }

    private void UpdateQuick()
    {
        var wanted = _qpmode && _command.Length == 0 && _quickJson != null;
        if (!wanted)
        {
            _quickWindow?.Close();
            return;
        }
        if (_quickWindow == null)
        {
            _quickWindow = new QuickWindow(Content as FrameworkElement, (command, json) =>
            {
                if (_host == IntPtr.Zero)
                {
                    return;
                }
                if (Native.Execute(_host, command, json) != 0)
                {
                    History.Text = Native.LastError(_host);
                }
                _panelKey = "";
                AfterCommand();
            });
            _quickWindow.Closed += (_, _) => _quickWindow = null;
            _quickWindow.Place(this);
            _quickWindow.Activate();
            Native.ShowWindow(WindowNative.GetWindowHandle(_quickWindow), 8);
        }
        if (_quickJson != null)
        {
            _quickWindow.Apply(_quickJson);
        }
    }

    private void OnLayers(object sender, RoutedEventArgs e) => OpenLayers();

    private void OpenDrafting()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_draftingWindow == null)
        {
            _draftingWindow = new DraftingWindow(Content as FrameworkElement, _host, () =>
            {
                RefreshChrome();
            });
            _draftingWindow.Closed += (_, _) => _draftingWindow = null;
            RefreshChrome();
        }
        _draftingWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_draftingWindow), 8);
    }

    private void OnUnits(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_unitsWindow == null)
        {
            _unitsWindow = UnitsWindow.Open(this, _host, AfterCommand);
            _unitsWindow.Closed += (_, _) => _unitsWindow = null;
        }
        _unitsWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_unitsWindow), 8);
    }

    private void OpenLayers()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_layersWindow == null)
        {
            _layersWindow = new LayerManagerWindow(Content as FrameworkElement, _host, AfterCommand);
            _layersWindow.Closed += (_, _) => _layersWindow = null;
        }
        _layersWindow.Reload("");
        _layersWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_layersWindow), 8);
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
        var manage = new Button { Content = "Layer properties", HorizontalAlignment = HorizontalAlignment.Left };
        manage.Click += (_, _) => OpenLayers();
        LayersPanel.Children.Add(manage);
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
                FontSize = 9,
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
        var button = new Button { Content = shown, Tag = layer, MinWidth = 21, FontSize = Density.Font };
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

    private void FillContext(PanelSnapshot snapshot)
    {
        _filling = true;
        try
        {
            var current = snapshot.Layers.FirstOrDefault(layer => layer.Current)?.Name;
            FillCombo(LayerBox, snapshot.Layers.Select(layer => (layer.Name, layer.Name)), current);
            FillFloor();
            FillSpaces();
        }
        finally
        {
            _filling = false;
        }
    }

    private void FillFloor()
    {
        if (Native.Execute(_host, "floor.current", "{}") != 0)
        {
            FloorBox.IsEnabled = false;
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            var current = doc.RootElement.TryGetProperty("floor", out var floor) && floor.ValueKind == JsonValueKind.String ? floor.GetString() : null;
            var names = new List<(string, string)>();
            if (doc.RootElement.TryGetProperty("floors", out var floors) && floors.ValueKind == JsonValueKind.Array)
            {
                foreach (var item in floors.EnumerateArray())
                {
                    if (item.ValueKind == JsonValueKind.String && item.GetString() is string name && name.Length > 0)
                    {
                        names.Add((name, name));
                    }
                }
            }
            FillCombo(FloorBox, names, current);
        }
        catch (JsonException)
        {
            FloorBox.IsEnabled = false;
        }
    }

    private void FillSpaces()
    {
        if (Native.Execute(_host, "space.list", "{}") != 0)
        {
            SpaceBox.IsEnabled = false;
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            var active = doc.RootElement.TryGetProperty("active", out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;
            var names = new List<(string, string)>();
            if (doc.RootElement.TryGetProperty("spaces", out var spaces))
            {
                foreach (var space in spaces.EnumerateArray())
                {
                    var id = space.TryGetProperty("id", out var ident) ? ident.GetString() ?? "" : "";
                    var name = space.TryGetProperty("name", out var label) ? label.GetString() ?? id : id;
                    if (id.Length > 0)
                    {
                        names.Add((id, name));
                    }
                }
            }
            FillCombo(SpaceBox, names, active);
        }
        catch (JsonException)
        {
            SpaceBox.IsEnabled = false;
        }
    }

    private static void FillCombo(ComboBox box, IEnumerable<(string Tag, string Label)> items, string? selected)
    {
        box.Items.Clear();
        foreach (var (tag, label) in items)
        {
            box.Items.Add(new ComboBoxItem { Content = label, Tag = tag });
        }
        box.IsEnabled = box.Items.Count > 0;
        for (var i = 0; i < box.Items.Count; i++)
        {
            if (box.Items[i] is ComboBoxItem item && item.Tag is string tag && tag.Equals(selected, StringComparison.OrdinalIgnoreCase))
            {
                box.SelectedIndex = i;
                return;
            }
        }
    }

    private void OnLayerPicked(object sender, SelectionChangedEventArgs e) => PickContext(LayerBox, "layer.current", "name");

    private void OnFloorPicked(object sender, SelectionChangedEventArgs e) => PickContext(FloorBox, "floor.current", "name");

    private void OnSpacePicked(object sender, SelectionChangedEventArgs e) => PickContext(SpaceBox, "space.activate", "id");

    private void PickContext(ComboBox box, string command, string key)
    {
        if (_filling || box.SelectedItem is not ComboBoxItem item || item.Tag is not string value || _host == IntPtr.Zero)
        {
            return;
        }
        var json = JsonSerializer.Serialize(new Dictionary<string, object> { [key] = value });
        if (Native.Execute(_host, command, json) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        _panelKey = "";
        AfterCommand();
    }

    private void ChoosePopup(string id, string run)
    {
        if (run == "type")
        {
            if (Native.CommandLine(_host, id) != 0)
            {
                History.Text = Native.LastError(_host);
            }
            RefreshChrome();
            return;
        }
        StartCommand(id);
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
            var popup = Native.ReadPopup(_host);
            if (popup.Length > 0)
            {
                try
                {
                    CanvasMenu.Show(this, popup, ChoosePopup);
                }
                catch (JsonException)
                {
                    Status.Text = "The shortcut menu could not be read.";
                }
            }
            _ticks++;
            if (_ticks % 4 == 0)
            {
                RefreshChrome();
            }
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

    private void BuildTools()
    {
        foreach (var (id, label) in Tools)
        {
            var command = id;
            var button = new Button { Content = label, MinHeight = 21, FontSize = Density.Font };
            AutomationProperties.SetName(button, label);
            button.Click += (_, _) => StartCommand(command);
            ToolStrip.Children.Add(button);
        }
    }

    private void ShowLayouts(ChromeState state)
    {
        var key = state.Space + "\n" + string.Join("\n", state.Layouts);
        if (key == _layoutKey)
        {
            return;
        }
        _layoutKey = key;
        LayoutTabs.Children.Clear();
        foreach (var name in state.Layouts)
        {
            var layout = name;
            var current = layout.Equals(state.Space, StringComparison.OrdinalIgnoreCase);
            var tab = new Button
            {
                Content = layout,
                MinHeight = 21,
                FontSize = Density.Font,
                FontWeight = current ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
            };
            AutomationProperties.SetName(tab, current ? "Current layout " + layout : "Switch to " + layout);
            tab.Click += (_, _) =>
            {
                var json = JsonSerializer.Serialize(new Dictionary<string, object> { ["name"] = layout });
                if (Native.Execute(_host, "layout.set", json) != 0)
                {
                    History.Text = Native.LastError(_host);
                }
                _layoutKey = "";
                AfterCommand();
            };
            LayoutTabs.Children.Add(tab);
        }
    }

    private void ShowDocuments(ChromeState state)
    {
        _documents.Clear();
        _documents.AddRange(state.Documents);
        var active = _documents.FirstOrDefault(doc => doc.Active);
        Title = active == null || active.Title.Length == 0 ? "AAD" : "AAD — " + active.Title;
        var key = string.Join("\n", _documents.Select(doc => doc.Index + ":" + doc.Title + ":" + doc.Dirty + ":" + doc.Active));
        if (key == _docKey)
        {
            return;
        }
        _docKey = key;
        FileTabs.Children.Clear();
        foreach (var doc in _documents.ToList())
        {
            var index = doc.Index;
            var tab = new Button
            {
                Content = doc.Dirty ? doc.Title + " *" : doc.Title,
                MinHeight = 21,
                FontSize = Density.Font,
                FontWeight = doc.Active ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
            };
            AutomationProperties.SetName(tab, doc.Active ? "Current drawing " + doc.Title : "Switch to " + doc.Title);
            tab.Click += (_, _) =>
            {
                if (Native.Execute(_host, "document.switch", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = index })) != 0)
                {
                    History.Text = Native.LastError(_host);
                }
                _docKey = "";
                _layoutKey = "";
                _panelKey = "";
                AfterCommand();
            };
            FileTabs.Children.Add(tab);
        }
    }

    private sealed class ChromeState
    {
        public string Prompt { get; set; } = "Command:";
        public List<string> History { get; set; } = new();
        public string Space { get; set; } = "Model";
        public List<string> Layouts { get; set; } = new();
        public string Layer { get; set; } = "0";
        public bool Ortho { get; set; }
        public bool Polar { get; set; }
        public bool Otrack { get; set; }
        public bool Dynmode { get; set; }
        public bool Osnap { get; set; }
        public bool Lwdisplay { get; set; }
        public bool Transparency { get; set; }
        public bool Qpmode { get; set; }
        public string Command { get; set; } = "";
        public string Units { get; set; } = "";
        public uint Osmode { get; set; }
        public double Polarang { get; set; } = 90;
        public bool DynpiCartesian { get; set; }
        public bool DynpiAbsolute { get; set; }
        public string Cursor { get; set; } = "";
        public List<DocTab> Documents { get; set; } = new();
    }

    private sealed class DocTab
    {
        public int Index { get; set; }
        public string Title { get; set; } = "";
        public bool Dirty { get; set; }
        public bool Active { get; set; }
    }

    private readonly List<DocTab> _documents = new();

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
