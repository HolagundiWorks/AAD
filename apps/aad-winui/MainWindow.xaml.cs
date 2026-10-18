using System.Runtime.InteropServices.WindowsRuntime;
using System.Text.Json;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
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

    private readonly LaunchArgs _launch;
    private IntPtr _host;
    private DispatcherQueueTimer? _timer;
    private int _ticks;
    private string _panelKey = "";
    private string _layoutKey = "";
    private string _docKey = "";
    private bool _buildingTabs;
    private bool _filling;
    private bool _historyOpen;
    private bool _asking;
    private bool _allowClose;
    private string _ownClipboard = "";
    private static readonly (string Id, string Label)[] ToolCatalog =
    [
        ("line", "Line"), ("pline", "Polyline"), ("circle", "Circle"), ("arc", "Arc"), ("rectang", "Rectangle"),
        ("move", "Move"), ("copy", "Copy"), ("rotate", "Rotate"), ("offset", "Offset"), ("trim", "Trim"),
        ("erase", "Erase"), ("dimlinear", "Dimension"), ("text", "Text"), ("hatch", "Hatch"),
    ];
    private List<string> _toolIds = ToolCatalog.Select(tool => tool.Id).ToList();
    private string _font = "engineering";
    private bool _showMenu = true;
    private bool _showTools = true;
    private bool _showTabs = true;
    private bool _showStatus = true;
    private bool _showCommand = true;
    private bool _pinLeft = true;
    private bool _pinRight = true;
    private bool _mergeLayers;
    private bool _hoverLeft;
    private bool _leftCollapsed;
    private DateTime _leftIdleSince = DateTime.UtcNow;
    private double _leftWidth = 280;
    private bool _start;
    private bool _ucs = true;
    private Window? _calc;
    private Window? _pomodoro;
    private Window? _toolsWindow;
    private AormsWindow? _aorms;
    private Window? _leftFloat;
    private bool _classic = true;
    private readonly Dictionary<string, ShellPrefs.ModeSlot> _modeSlots = new();
    private bool _dark = true;
    private Window? _pdfWindow;
    private Window? _formWindow;
    private LayerManagerWindow? _layersWindow;
    private Window? _unitsWindow;
    private Window? _textStyleWindow;
    private Window? _pageSetupWindow;
    private Window? _plotStyleWindow;
    private Window? _batchPlotWindow;
    private Window? _dimStyleWindow;
    private Window? _linetypeWindow;
    private Window? _pointStyleWindow;
    private Window? _mleaderStyleWindow;
    private Window? _roomSettingsWindow;
    private string _space = "Model";
    private bool _plotBusy;
    private DraftingWindow? _draftingWindow;
    private QuickWindow? _quickWindow;
    private QuickSelectWindow? _quickSelectWindow;
    private string? _quickJson;
    private string _command = "";
    private bool _qpmode;
    private bool _literalSpace;
    private CommandChrome? _chrome;
    private bool _shellClosing;
    private bool _armSpace;
    private bool _barDrag;
    private int _dragCursorX;
    private int _dragCursorY;
    private int _dragWinX;
    private int _dragWinY;
    private readonly List<string> _typed = new();
    private int _recall = -1;
    private string _liveDraft = "";
    private string _historyShown = "\u0001";
    private double _barDx;
    private double _barDy;
    /// <summary>Shortcut → command id, built from the engine menu registry (same strings as the menu).</summary>
    private readonly Dictionary<(bool Ctrl, bool Shift, Windows.System.VirtualKey Key), string> _menuKeys = new();
    private long _accelAt;
    private string _accelLast = "";

    internal void HoldForm(Window? window) => _formWindow = window;

    public MainWindow(LaunchArgs? launch = null)
    {
        _launch = launch ?? LaunchArgs.Parse(Environment.GetCommandLineArgs().Skip(1));
        InitializeComponent();
        AppWindow.Closing += OnAppClosing;
        // Sample only when asked and no drawing path was given (paths replace the empty session).
        _host = Native.Create(sample: _launch.Sample && _launch.Files.Count == 0);
        AppWindow.Changed += (_, args) =>
        {
            if (!_shellClosing && (args.DidPositionChange || args.DidSizeChange))
            {
                PlaceCommandBar();
            }
        };
        Closed += (_, _) =>
        {
            _shellClosing = true;
            _timer?.Stop();
            _chrome?.Close();
            _chrome = null;
            if (_layersWindow != null)
            {
                try
                {
                    _layersWindow.AttachBody();
                    _layersWindow.Close();
                }
                catch (Exception)
                {
                    // The view is already going away with the shell.
                }
                _layersWindow = null;
            }
            if (_host != IntPtr.Zero)
            {
                Native.Destroy(_host);
                _host = IntPtr.Zero;
            }
        };
        Activated += (_, _) =>
        {
            PlaceChild();
            PlaceCommandBar();
        };
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        OpenCommandChrome();
        LoadShell();
        BuildMenus();
        RefreshChrome();
        RefreshPanels();
        ApplyShellTheme();
        Listen();
        OpenLaunchFiles();
        StartLogo.Source = ShellIcon.Logo();
        ApplyAccess();
        PlaceCommandBar();
        CommandInput.Focus(FocusState.Programmatic);
    }

    private void OpenLaunchFiles()
    {
        foreach (var path in _launch.Files)
        {
            if (string.IsNullOrWhiteSpace(path))
            {
                continue;
            }
            if (Native.Execute(_host, "open", PathJson(path)) != 0)
            {
                History.Text = Native.LastError(_host);
                continue;
            }
            Native.Execute(_host, "zoom.extents", "{}");
            _docKey = "";
        }
        if (_launch.Project is { } project && _launch.Files.Count > 0)
        {
            // Says on the command line when the drawing belongs to a different project (or none).
            Native.Execute(_host, "aorms.expect", JsonSerializer.Serialize(new Dictionary<string, string> { ["projectOfficeId"] = project }));
        }
        if (_launch.Files.Count > 0)
        {
            AfterCommand();
        }
    }

    private void ApplyAccess()
    {
        AutomationProperties.SetLiveSetting(PromptLive, AutomationLiveSetting.Assertive);
        AutomationProperties.SetLiveSetting(PromptLabel, AutomationLiveSetting.Assertive);
        AutomationProperties.SetLiveSetting(Status, AutomationLiveSetting.Polite);
        AutomationProperties.SetLandmarkType(Bar, AutomationLandmarkType.Navigation);
        AutomationProperties.SetLandmarkType(FileTabs, AutomationLandmarkType.Navigation);
        AutomationProperties.SetLandmarkType(ToolStripHost, AutomationLandmarkType.Navigation);
        AutomationProperties.SetLandmarkType(LeftColumn, AutomationLandmarkType.Form);
        AutomationProperties.SetLocalizedLandmarkType(LeftColumn, "complementary");
        AutomationProperties.SetLandmarkType(CanvasHost, AutomationLandmarkType.Main);
        AutomationProperties.SetLandmarkType(StartPage, AutomationLandmarkType.Main);
        AutomationProperties.SetLocalizedLandmarkType(StatusRow, "status");
        AutomationProperties.SetHeadingLevel(StartHeading, AutomationHeadingLevel.Level1);
        AnnouncePrompt(PromptLabel.Text ?? "Command:", "");
        AutomationProperties.SetName(LeftSplitter, "Resize side panel, width " + (int)_leftWidth);
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
        if (ApplyUndoRedo(e))
        {
            return;
        }
        if (KeyDown(Windows.System.VirtualKey.Control) && e.Key is Windows.System.VirtualKey.Number9 or Windows.System.VirtualKey.NumberPad9)
        {
            e.Handled = true;
            _showCommand = false;
            ApplyChrome();
            SaveShell();
            return;
        }
        var ctrl = KeyDown(Windows.System.VirtualKey.Control);
        var shift = KeyDown(Windows.System.VirtualKey.Shift);
        if (ApplyFunctionKey(e, ctrl, shift))
        {
            return;
        }
        if (ApplyDraftKeys(e, ctrl, shift, true))
        {
            return;
        }
        if (ctrl && e.Key is not Windows.System.VirtualKey.Number9 and not Windows.System.VirtualKey.NumberPad9)
        {
            _armSpace = false;
            return;
        }
        if (e.Key == Windows.System.VirtualKey.Enter || e.Key == Windows.System.VirtualKey.Space)
        {
            if (SubmitKey(e.Key))
            {
                e.Handled = true;
            }
            return;
        }
        _armSpace = false;
        if (e.Key == Windows.System.VirtualKey.Escape)
        {
            e.Handled = true;
            if (!CanvasMenu.Dismiss())
            {
                CancelCommand();
            }
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
    }

    private void OnDraft(object sender, TextChangedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        Native.Draft(_host, CommandInput.Text);
    }

    private void CancelCommand()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        CommandInput.Text = "";
        if (Native.Cancel(_host) != 0)
        {
            Status.Text = Native.LastError(_host);
        }
        RefreshChrome();
        RefreshPanels();
        BuildMenus();
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
        if (id is "layer" or "la" or "layers" or "ui.dialog.layers")
        {
            OpenLayers();
            return;
        }
        if (id == "ui.panel.layers")
        {
            ShowSidePanel(SideTab.Layers);
            return;
        }
        if (id is "ui.panel.properties" or "properties")
        {
            ShowSidePanel(SideTab.Properties);
            return;
        }
        if (id is "ui.dialog.blocks" or "blocks")
        {
            ShowSidePanel(SideTab.Blocks);
            return;
        }
        if (id is "units" or "un")
        {
            OpenUnits();
            return;
        }
        if (id is "ui.dialog.about")
        {
            AboutWindow.Show(this, VersionText());
            return;
        }
        if (id is "ui.dialog.commands")
        {
            CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
            return;
        }
        if (id is "ui.dialog.customizetop")
        {
            OpenTools();
            return;
        }
        if (id is "ui.dialog.parameters")
        {
            History.Text = "Parameters Manager is not available yet.";
            return;
        }
        if (id == "ui.workspace.classic")
        {
            SwitchWorkspace(classic: true);
            return;
        }
        if (id == "ui.workspace.pro")
        {
            SwitchWorkspace(classic: false);
            return;
        }
        if (id is "ui.workspace.toggle")
        {
            SwitchWorkspace(!_classic);
            return;
        }
        if (id is "ui.workspace.reset" or "resetui" or "ui.dock.reset")
        {
            ResetCurrentWorkspace();
            return;
        }
        if (id == "ui.workspace")
        {
            History.Text = _classic ? "Workspace: Classic" : "Workspace: Pro";
            return;
        }
        if (id is "style" or "st")
        {
            OpenTextStyle();
            return;
        }
        if (id is "plot" or "print" or "exportpdf")
        {
            _ = BeginPlot(id);
            return;
        }
        if (id == "pagesetup")
        {
            OpenPageSetup();
            return;
        }
        if (id == "stylesmanager")
        {
            OpenPlotStyles();
            return;
        }
        if (id == "plot.batch")
        {
            OpenBatchPlot();
            return;
        }
        if (id is "dimstyle" or "dimstyle.dimension")
        {
            OpenDimStyle();
            return;
        }
        if (id is "linetype" or "lt" or "ltype")
        {
            OpenLinetype();
            return;
        }
        if (id == "ddptype")
        {
            OpenPointStyle();
            return;
        }
        if (id is "mleaderstyle" or "mls")
        {
            OpenMLeaderStyle();
            return;
        }
        if (id == "room.settings")
        {
            OpenRoomSettings();
            return;
        }
        if (id is "dsettings" or "ds" or "se" or "ui.dialog.dsettings")
        {
            OpenDrafting();
            return;
        }
        if (id is "qselect" or "qs" or "ui.dialog.qselect")
        {
            OpenQuickSelect();
            return;
        }
        if (id.StartsWith("aorms.", StringComparison.Ordinal))
        {
            OpenAorms();
            return;
        }
        if (id is "cal" or "quickcalc" or "qc" or "calc")
        {
            OpenCalculator();
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
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (e.Key == Windows.System.VirtualKey.Escape && (CanvasMenu.IsOpen || _command.Length > 0 || !Typing()))
        {
            e.Handled = true;
            if (!CanvasMenu.Dismiss())
            {
                CancelCommand();
            }
            return;
        }
        if (ApplyUndoRedo(e))
        {
            return;
        }
        var ctrl = KeyDown(Windows.System.VirtualKey.Control);
        var shift = KeyDown(Windows.System.VirtualKey.Shift);
        if (e.Key == Windows.System.VirtualKey.F6)
        {
            e.Handled = true;
            _showCommand = true;
            ApplyChrome();
            CommandInput.Focus(FocusState.Programmatic);
            return;
        }
        if (ApplyFunctionKey(e, ctrl, shift))
        {
            return;
        }
        if (Typing())
        {
            return;
        }
        if (ApplyDraftKeys(e, ctrl, shift, false))
        {
            return;
        }
        if (e.Key == Windows.System.VirtualKey.F1)
        {
            e.Handled = true;
            CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
            return;
        }
        if (e.Key == Windows.System.VirtualKey.F2)
        {
            e.Handled = true;
            SetHistoryOpen(!_historyOpen);
            return;
        }
        if (ctrl && e.Key == Windows.System.VirtualKey.F11)
        {
            e.Handled = true;
            if (shift)
            {
                ResetCurrentWorkspace();
            }
            else
            {
                SwitchWorkspace(!_classic);
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
        if (e.Key == Windows.System.VirtualKey.Menu && !_showMenu)
        {
            e.Handled = true;
            _showMenu = true;
            ApplyChrome();
            SaveShell();
            return;
        }
        if (ctrl && e.Key is Windows.System.VirtualKey.Number9 or Windows.System.VirtualKey.NumberPad9)
        {
            e.Handled = true;
            _showCommand = !_showCommand;
            ApplyChrome();
            SaveShell();
            if (_showCommand)
            {
                CommandInput.Focus(FocusState.Programmatic);
            }
            return;
        }
        if (TryMenuKey(ctrl, shift, e.Key, out var id))
        {
            e.Handled = true;
            InvokeAccelId(id);
            return;
        }
        if (ctrl || e.Key == Windows.System.VirtualKey.Menu)
        {
            return;
        }
        if (e.Key == Windows.System.VirtualKey.Enter || e.Key == Windows.System.VirtualKey.Space)
        {
            if (SubmitKey(e.Key))
            {
                e.Handled = true;
            }
            return;
        }
        _armSpace = false;
        if (e.Key == Windows.System.VirtualKey.Back && CommandInput.Text.Length > 0)
        {
            e.Handled = true;
            CommandInput.Text = CommandInput.Text[..^1];
            CommandInput.SelectionStart = CommandInput.Text.Length;
            CommandInput.Focus(FocusState.Programmatic);
            return;
        }
        var typed = Native.Character(e.Key);
        if (string.IsNullOrEmpty(typed))
        {
            return;
        }
        e.Handled = true;
        CommandInput.Text += typed;
        CommandInput.SelectionStart = CommandInput.Text.Length;
        CommandInput.Focus(FocusState.Programmatic);
    }

    /// <summary>
    /// Function keys work wherever the focus is, including the command line (as in other CAD
    /// programs): F1 command reference, F2 history, Ctrl+F11 workspace, and the registry's own
    /// F-key shortcuts (F3 object snap, F8 ortho, F10 polar, F11 object snap tracking, F12 dynamic
    /// input). F6 focuses the command line and is handled by the caller.
    /// </summary>
    private bool ApplyFunctionKey(KeyRoutedEventArgs e, bool ctrl, bool shift)
    {
        if (e.Key is < Windows.System.VirtualKey.F1 or > Windows.System.VirtualKey.F12 || e.Key == Windows.System.VirtualKey.F6)
        {
            return false;
        }
        if (e.Key == Windows.System.VirtualKey.F1 && !ctrl && !shift)
        {
            e.Handled = true;
            CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
            return true;
        }
        if (e.Key == Windows.System.VirtualKey.F2 && !ctrl && !shift)
        {
            e.Handled = true;
            SetHistoryOpen(!_historyOpen);
            return true;
        }
        if (ctrl && e.Key == Windows.System.VirtualKey.F11)
        {
            e.Handled = true;
            if (shift)
            {
                ResetCurrentWorkspace();
            }
            else
            {
                SwitchWorkspace(!_classic);
            }
            return true;
        }
        if (TryMenuKey(ctrl, shift, e.Key, out var id))
        {
            e.Handled = true;
            InvokeAccelId(id);
            return true;
        }
        return false;
    }

    private bool TryMenuKey(bool ctrl, bool shift, Windows.System.VirtualKey key, out string id) =>
        _menuKeys.TryGetValue((ctrl, shift, key), out id!);

    /// <summary>Run a menu/registry shortcut the same way as a menu click or accelerator.</summary>
    private void InvokeAccelId(string id)
    {
        if (id.Length == 0)
        {
            return;
        }
        var now = Environment.TickCount64;
        if (id == _accelLast && now - _accelAt < 80)
        {
            return;
        }
        _accelAt = now;
        _accelLast = id;
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
        if (id is "copyclip" or "cutclip" or "copybase" or "u" or "redo" or "selectall" or "erase" or "erase.selection"
            or "new" or "osnap" or "ortho" or "polar" or "otrack" or "dynmode")
        {
            RunShortcut(id == "erase.selection" ? "erase" : id);
            return;
        }
        StartCommand(id);
    }

    /// <summary>Enter, or Space when it confirms. One path so the two keys cannot run a line twice.</summary>
    private bool SubmitKey(Windows.System.VirtualKey key)
    {
        if (key == Windows.System.VirtualKey.Space && _literalSpace)
        {
            _armSpace = false;
            return false;
        }
        if (key == Windows.System.VirtualKey.Space && string.IsNullOrEmpty(CommandInput.Text))
        {
            if (!_armSpace)
            {
                _armSpace = true;
                return true;
            }
            _armSpace = false;
            SubmitLine();
            return true;
        }
        _armSpace = false;
        SubmitLine();
        return true;
    }

    private void Remember(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return;
        }
        if (_typed.Count == 0 || _typed[^1] != text)
        {
            _typed.Add(text);
            if (_typed.Count > 200)
            {
                _typed.RemoveAt(0);
            }
        }
        _recall = _typed.Count;
        _liveDraft = "";
    }

    private void Recall(int direction)
    {
        if (_typed.Count == 0)
        {
            return;
        }
        if (_recall < 0 || _recall > _typed.Count)
        {
            _recall = _typed.Count;
        }
        if (_recall == _typed.Count)
        {
            _liveDraft = CommandInput.Text;
        }
        var next = Math.Clamp(_recall + direction, 0, _typed.Count);
        _recall = next;
        CommandInput.Text = next == _typed.Count ? _liveDraft : _typed[next];
        CommandInput.SelectionStart = CommandInput.Text.Length;
    }

    private void SubmitLine()
    {
        var text = CommandInput.Text;
        _armSpace = false;
        if (!string.IsNullOrWhiteSpace(text))
        {
            Remember(text);
        }
        else
        {
            _recall = _typed.Count;
        }
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
        if (IsBarePlotCommand(text, out var plotId))
        {
            _ = BeginPlot(plotId);
            return;
        }
        if (IsBareDialogCommand(text, out var dialogId))
        {
            StartCommand(dialogId);
            CommandInput.Focus(FocusState.Programmatic);
            return;
        }
        if (Native.CommandLine(_host, text) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        RefreshChrome();
        RefreshPanels();
        BuildMenus();
        CommandInput.Focus(FocusState.Programmatic);
    }

    private static bool IsBareToken(string text, out string id)
    {
        id = "";
        var trimmed = text.Trim();
        if (trimmed.Length == 0)
        {
            return false;
        }
        var space = trimmed.IndexOfAny([' ', '\t']);
        var token = space < 0 ? trimmed : trimmed[..space];
        if (space >= 0 && trimmed[(space + 1)..].Trim().Length > 0)
        {
            return false;
        }
        id = token.ToLowerInvariant();
        return true;
    }

    private static bool IsBarePlotCommand(string text, out string id) =>
        IsBareToken(text, out id) && id is "plot" or "print" or "exportpdf";

    /// <summary>Bare typed names that open a WinUI dialog (same as the menu), not the JSON command defaults.</summary>
    private static bool IsBareDialogCommand(string text, out string id)
    {
        if (!IsBareToken(text, out id))
        {
            return false;
        }
        return id is "units" or "un"
            or "layer" or "la" or "layers"
            or "style" or "st"
            or "pagesetup"
            or "stylesmanager"
            or "plot.batch"
            or "dimstyle"
            or "linetype" or "lt" or "ltype"
            or "ddptype"
            or "mleaderstyle" or "mls"
            or "room.settings"
            or "dsettings" or "ds" or "se"
            or "qselect" or "qs"
            or "cal" or "quickcalc" or "qc" or "calc";
    }

    private async Task BeginPlot(string id)
    {
        if (_host == IntPtr.Zero || _plotBusy)
        {
            return;
        }
        // Model space: ask what to plot (window / extents / display), then finish with a save picker.
        if (_space.Equals("Model", StringComparison.OrdinalIgnoreCase))
        {
            if (Native.CommandLine(_host, "plot.window") != 0)
            {
                History.Text = Native.LastError(_host);
            }
            AfterCommand();
            return;
        }
        await FinishPlot(id == "exportpdf" ? "exportpdf" : "plot", window: null);
    }

    private async Task FinishPlot(string command, double[]? window)
    {
        if (_host == IntPtr.Zero || _plotBusy)
        {
            return;
        }
        _plotBusy = true;
        try
        {
            var hwnd = WindowNative.GetWindowHandle(this);
            var save = new FileSavePicker();
            InitializeWithWindow.Initialize(save, hwnd);
            save.FileTypeChoices.Add("PDF", new List<string> { ".pdf" });
            var stem = _documents.FirstOrDefault(doc => doc.Active)?.Title ?? "";
            if (stem.Length == 0)
            {
                stem = "Drawing";
            }
            else
            {
                stem = Path.GetFileNameWithoutExtension(stem);
                if (stem.Length == 0)
                {
                    stem = "Drawing";
                }
            }
            save.SuggestedFileName = stem + ".pdf";
            var chosen = await save.PickSaveFileAsync();
            if (chosen == null)
            {
                History.Text = "*Cancel*";
                AfterCommand();
                return;
            }
            var body = new Dictionary<string, object?> { ["path"] = chosen.Path };
            if (window is { Length: 4 })
            {
                body["window"] = window;
            }
            if (Native.Execute(_host, command, JsonSerializer.Serialize(body)) != 0)
            {
                History.Text = Native.LastError(_host);
                AfterCommand();
                return;
            }
            History.Text = window == null
                ? "Plotted to " + chosen.Path
                : "Plotted the window to " + chosen.Path;
            AfterCommand();
        }
        finally
        {
            _plotBusy = false;
        }
    }

    private void DrainPlotRequest()
    {
        if (_host == IntPtr.Zero || _plotBusy)
        {
            return;
        }
        var json = Native.ReadPlotRequest(_host);
        if (json.Length == 0)
        {
            return;
        }
        double[]? window = null;
        try
        {
            using var doc = JsonDocument.Parse(json);
            if (doc.RootElement.TryGetProperty("window", out var win) && win.ValueKind == JsonValueKind.Array && win.GetArrayLength() == 4)
            {
                window = new double[4];
                for (var i = 0; i < 4; i++)
                {
                    window[i] = win[i].GetDouble();
                }
            }
        }
        catch (JsonException)
        {
            History.Text = "The plot window could not be read.";
            return;
        }
        // Model-space finish always uses plot (same as the egui shell).
        _ = FinishPlot("plot", window);
    }

    /// Drawing undo and redo. These win over a text field's own undo, including the command line.
    private bool ApplyUndoRedo(KeyRoutedEventArgs e)
    {
        if (!KeyDown(Windows.System.VirtualKey.Control))
        {
            return false;
        }
        var shift = KeyDown(Windows.System.VirtualKey.Shift);
        var id = (shift, e.Key) switch
        {
            (false, Windows.System.VirtualKey.Z) => "u",
            (true, Windows.System.VirtualKey.Z) => "redo",
            (false, Windows.System.VirtualKey.Y) => "redo",
            (false, Windows.System.VirtualKey.U) => "redo",
            _ => "",
        };
        if (id.Length == 0)
        {
            return false;
        }
        e.Handled = true;
        RunShortcut(id);
        return true;
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

    private void OnWorkspace(object sender, RoutedEventArgs e) => SwitchWorkspace(!_classic);

    private string WorkspaceKey() => _classic ? "classic" : "pro";

    private static ShellPrefs.ModeSlot DefaultSlot(bool classic) => new()
    {
        PinLeft = classic,
        ToolsVisible = true,
        LeftWidth = 280,
        LeftCollapsed = !classic,
    };

    private ShellPrefs.ModeSlot CaptureSlot() => new()
    {
        PinLeft = _pinLeft,
        ToolsVisible = _showTools,
        LeftWidth = _leftWidth,
        LeftCollapsed = _leftCollapsed,
    };

    private void ApplySlot(ShellPrefs.ModeSlot slot)
    {
        _pinLeft = slot.PinLeft;
        _showTools = slot.ToolsVisible;
        _leftWidth = Math.Clamp(slot.LeftWidth > 0 ? slot.LeftWidth : 280, 180, 640);
        _leftCollapsed = slot.LeftCollapsed;
        _leftIdleSince = DateTime.UtcNow;
        UpdatePinButton();
    }

    private void SwitchWorkspace(bool classic)
    {
        if (_classic == classic)
        {
            return;
        }
        _modeSlots[WorkspaceKey()] = CaptureSlot();
        _classic = classic;
        if (!_modeSlots.TryGetValue(WorkspaceKey(), out var slot))
        {
            slot = DefaultSlot(classic);
        }
        ApplySlot(slot);
        ToolTipService.SetToolTip(WorkspaceButton, _classic ? "Classic" : "Pro");
        AutomationProperties.SetName(WorkspaceButton, _classic ? "Classic workspace" : "Pro workspace");
        ApplyChrome();
        PlaceChild();
        SaveShell();
    }

    /// <summary>Resets the current workspace only; the other mode's slot is kept.</summary>
    private void ResetCurrentWorkspace()
    {
        ApplySlot(DefaultSlot(_classic));
        _showMenu = true;
        _showTabs = true;
        _showStatus = true;
        _showCommand = true;
        _start = false;
        _modeSlots[WorkspaceKey()] = CaptureSlot();
        ApplyChrome();
        PlaceChild();
        SaveShell();
    }

    private UIElement? _dragSplit;
    private double _dragX;
    private double _dragWidth;
    private string _promptShown = "";
    private string _dynAccess = "";

    private void OnSplitPressed(object sender, PointerRoutedEventArgs e)
    {
        if (sender is not UIElement splitter)
        {
            return;
        }
        _dragSplit = splitter;
        _dragX = e.GetCurrentPoint(Content).Position.X;
        _dragWidth = _leftWidth;
        _leftCollapsed = false;
        _leftIdleSince = DateTime.UtcNow;
        splitter.CapturePointer(e.Pointer);
        e.Handled = true;
    }

    private void OnSplitMoved(object sender, PointerRoutedEventArgs e)
    {
        if (_dragSplit == null || !ReferenceEquals(sender, _dragSplit))
        {
            return;
        }
        var delta = e.GetCurrentPoint(Content).Position.X - _dragX;
        SetSideWidth(_dragWidth + delta);
        PlaceChild();
        e.Handled = true;
    }

    private void OnSplitReleased(object sender, PointerRoutedEventArgs e)
    {
        if (sender is UIElement splitter)
        {
            splitter.ReleasePointerCapture(e.Pointer);
        }
        EndSplitDrag();
        e.Handled = true;
    }

    private void OnSplitCaptureLost(object sender, PointerRoutedEventArgs e) => EndSplitDrag();

    private void EndSplitDrag()
    {
        if (_dragSplit == null)
        {
            return;
        }
        _dragSplit = null;
        _leftIdleSince = DateTime.UtcNow;
        SaveShell();
    }

    private void OnSplitKey(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key is not (Windows.System.VirtualKey.Left or Windows.System.VirtualKey.Right))
        {
            return;
        }
        var step = KeyDown(Windows.System.VirtualKey.Shift) ? 40 : 16;
        if (e.Key == Windows.System.VirtualKey.Left)
        {
            step = -step;
        }
        SetSideWidth(_leftWidth + step);
        PlaceChild();
        SaveShell();
        e.Handled = true;
    }

    private void SetSideWidth(double width)
    {
        _leftWidth = Math.Clamp(width, 180, 640);
        PropertiesColumn.Width = new GridLength(_leftWidth);
        AutomationProperties.SetName(LeftSplitter, "Resize side panel, width " + (int)_leftWidth);
    }

    private void AnnouncePrompt(string prompt, string dynAccess = "")
    {
        var text = prompt;
        if (dynAccess.Length > 0)
        {
            text = prompt + " " + dynAccess;
        }
        if (text == _promptShown)
        {
            return;
        }
        _promptShown = text;
        PromptLive.Text = text;
        AutomationProperties.SetName(PromptLive, text);
        AutomationProperties.SetName(PromptLabel, text);
        AutomationProperties.SetName(
            CanvasHost,
            _start
                ? "Start"
                : "Drawing. Arrows move the cursor, Insert picks or places, Ctrl+arrows pan. " + text);
        AutomationProperties.SetFullDescription(
            CanvasHost,
            "Keyboard drafting: type coordinates on the command line, Tab cycles snaps, Enter or Space cycles a hot grip.");
    }

    /// <summary>Arrow nudge / pan, Insert activate, and history recall when the line has text.</summary>
    private bool ApplyDraftKeys(KeyRoutedEventArgs e, bool ctrl, bool shift, bool fromCommand)
    {
        if (e.Key == Windows.System.VirtualKey.Insert)
        {
            e.Handled = true;
            _armSpace = false;
            if (Native.KeyActivate(_host, shift) != 0)
            {
                Status.Text = Native.LastError(_host);
            }
            else
            {
                RefreshChrome();
                RefreshPanels();
            }
            return true;
        }
        if (e.Key is not (Windows.System.VirtualKey.Left or Windows.System.VirtualKey.Right
            or Windows.System.VirtualKey.Up or Windows.System.VirtualKey.Down))
        {
            return false;
        }
        var empty = string.IsNullOrEmpty(CommandInput.Text);
        if (!ctrl && !empty && e.Key is Windows.System.VirtualKey.Up or Windows.System.VirtualKey.Down)
        {
            e.Handled = true;
            _armSpace = false;
            Recall(e.Key == Windows.System.VirtualKey.Up ? -1 : 1);
            if (!fromCommand)
            {
                CommandInput.Focus(FocusState.Programmatic);
            }
            return true;
        }
        if (!ctrl && !empty && e.Key is Windows.System.VirtualKey.Left or Windows.System.VirtualKey.Right)
        {
            return false;
        }
        if (!ctrl && !empty)
        {
            return false;
        }
        e.Handled = true;
        _armSpace = false;
        var dx = e.Key == Windows.System.VirtualKey.Left ? -1 : e.Key == Windows.System.VirtualKey.Right ? 1 : 0;
        var dy = e.Key == Windows.System.VirtualKey.Up ? -1 : e.Key == Windows.System.VirtualKey.Down ? 1 : 0;
        if (Native.KeyNudge(_host, dx, dy, ctrl, shift) != 0)
        {
            Status.Text = Native.LastError(_host);
        }
        else
        {
            RefreshChrome();
        }
        return true;
    }

    private void OnTheme(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        _dark = !_dark;
        Native.Theme(_host, _dark);
        if (ThemeButton.Content is DrawnIcon theme)
        {
            theme.Kind = _dark ? "moon" : "sun";
        }
        ToolTipService.SetToolTip(ThemeButton, _dark ? "Dark" : "Light");
        ApplyShellTheme();
    }

    private void ApplyShellTheme()
    {
        if (Content is FrameworkElement root)
        {
            root.RequestedTheme = _dark ? ElementTheme.Dark : ElementTheme.Light;
        }
        _chrome?.ApplyTheme(_dark ? ElementTheme.Dark : ElementTheme.Light);
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
        BuildTools();
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
        var open = new Button { Content = "Open PDF…", HorizontalAlignment = HorizontalAlignment.Left, MinHeight = Density.MinTarget };
        Density.Name(open, "Open PDF");
        Density.Name(caption, "PDF page");
        Density.Name(image, "PDF page preview");
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
            ShellIcon.Apply(_pdfWindow);
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
        _menuKeys.Clear();
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
        // Shell-only aliases that match common Windows habits and stay on the same command ids.
        RememberKey(true, false, Windows.System.VirtualKey.Y, "redo");
        RememberKey(true, false, Windows.System.VirtualKey.U, "redo");
        RememberKey(true, false, Windows.System.VirtualKey.Number1, "ui.panel.properties");
        RememberKey(true, false, Windows.System.VirtualKey.NumberPad1, "ui.panel.properties");
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
        ClearShellItems(help);
        var commands = new MenuFlyoutItem { Text = "Command list", Tag = "shell.commands" };
        void ShowCommands() => CommandList.Show(this, Native.ReadText(_host, Native.TextMenu));
        commands.Click += (_, _) => ShowCommands();
        BindAccel(commands, "F1", ShowCommands, mapId: "");
        var about = new MenuFlyoutItem { Text = "About AAD", Tag = "shell.about" };
        about.Click += (_, _) => AboutWindow.Show(this, VersionText());
        help.Items.Add(commands);
        help.Items.Add(about);
        EnsureShellMenus();
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
        if (!string.IsNullOrEmpty(node.Shortcut) && node.Id != null)
        {
            BindAccel(item, node.Shortcut, () => InvokeAccelId(node.Id!));
        }
        item.Click += OnMenuCommand;
        return item;
    }

    private void BindAccel(MenuFlyoutItem item, string shortcut, Action run, string? mapId = null)
    {
        if (!TryParseShortcut(shortcut, out var key, out var mods))
        {
            item.KeyboardAcceleratorTextOverride = shortcut;
            return;
        }
        item.KeyboardAccelerators.Clear();
        var accel = new KeyboardAccelerator { Key = key, Modifiers = mods };
        accel.Invoked += (_, args) =>
        {
            args.Handled = true;
            run();
        };
        item.KeyboardAccelerators.Add(accel);
        RememberKey(
            mods.HasFlag(Windows.System.VirtualKeyModifiers.Control),
            mods.HasFlag(Windows.System.VirtualKeyModifiers.Shift),
            key,
            mapId ?? item.Tag as string ?? "");
    }

    private void RememberKey(bool ctrl, bool shift, Windows.System.VirtualKey key, string id)
    {
        if (id.Length == 0 || key == Windows.System.VirtualKey.None)
        {
            return;
        }
        _menuKeys[(ctrl, shift, key)] = id;
    }

    private static bool TryParseShortcut(string text, out Windows.System.VirtualKey key, out Windows.System.VirtualKeyModifiers mods)
    {
        key = Windows.System.VirtualKey.None;
        mods = Windows.System.VirtualKeyModifiers.None;
        if (string.IsNullOrWhiteSpace(text))
        {
            return false;
        }
        var parts = text.Split('+', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length == 0)
        {
            return false;
        }
        for (var i = 0; i < parts.Length - 1; i++)
        {
            switch (parts[i].ToUpperInvariant())
            {
                case "CTRL":
                case "CONTROL":
                case "CMD":
                case "COMMAND":
                    mods |= Windows.System.VirtualKeyModifiers.Control;
                    break;
                case "SHIFT":
                    mods |= Windows.System.VirtualKeyModifiers.Shift;
                    break;
                case "ALT":
                case "MENU":
                    mods |= Windows.System.VirtualKeyModifiers.Menu;
                    break;
                default:
                    return false;
            }
        }
        var last = parts[^1].ToUpperInvariant();
        key = last switch
        {
            "DELETE" or "DEL" => Windows.System.VirtualKey.Delete,
            "ESCAPE" or "ESC" => Windows.System.VirtualKey.Escape,
            "ENTER" or "RETURN" => Windows.System.VirtualKey.Enter,
            "SPACE" => Windows.System.VirtualKey.Space,
            "TAB" => Windows.System.VirtualKey.Tab,
            "F1" => Windows.System.VirtualKey.F1,
            "F2" => Windows.System.VirtualKey.F2,
            "F3" => Windows.System.VirtualKey.F3,
            "F4" => Windows.System.VirtualKey.F4,
            "F5" => Windows.System.VirtualKey.F5,
            "F6" => Windows.System.VirtualKey.F6,
            "F7" => Windows.System.VirtualKey.F7,
            "F8" => Windows.System.VirtualKey.F8,
            "F9" => Windows.System.VirtualKey.F9,
            "F10" => Windows.System.VirtualKey.F10,
            "F11" => Windows.System.VirtualKey.F11,
            "F12" => Windows.System.VirtualKey.F12,
            { Length: 1 } s when s[0] is >= 'A' and <= 'Z' => (Windows.System.VirtualKey)s[0],
            { Length: 1 } s when s[0] is >= '0' and <= '9' => Windows.System.VirtualKey.Number0 + (s[0] - '0'),
            _ => Windows.System.VirtualKey.None,
        };
        return key != Windows.System.VirtualKey.None;
    }

    private static void ClearShellItems(MenuBarItem menu)
    {
        for (var i = menu.Items.Count - 1; i >= 0; i--)
        {
            if (menu.Items[i] is MenuFlyoutItem item && item.Tag is string tag && tag.StartsWith("shell.", StringComparison.Ordinal))
            {
                menu.Items.RemoveAt(i);
            }
        }
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
            }
            SyncHistory();
            var prompt = state.Prompt;
            if (state.Assistant is "thinking" or "loading")
            {
                prompt = string.IsNullOrEmpty(state.AssistantPartial) ? "Assistant…" : state.AssistantPartial;
            }
            else if (state.Assistant == "confirm" && !string.IsNullOrEmpty(state.AssistantPrompt))
            {
                prompt = state.AssistantPrompt;
            }
            PromptLabel.Text = prompt;
            AnnouncePrompt(prompt, _dynAccess);
            AssistantButtons.Visibility = state.Assistant == "confirm" ? Visibility.Visible : Visibility.Collapsed;
            CommandInput.PlaceholderText = "";
            if (!string.IsNullOrEmpty(state.Space))
            {
                _space = state.Space;
            }
            if (state.ControlPort > 0)
            {
                ControlPort.Text = "127.0.0.1:" + state.ControlPort;
                ToolTipService.SetToolTip(ControlPort, state.AssistantModel);
            }
            if (state.Quit && !_allowClose)
            {
                // Programmatic quit: no unsaved-changes prompt (same as egui app.quit).
                _allowClose = true;
                Close();
                return;
            }
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
            _literalSpace = state.LiteralSpace;
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

    private void OnUnits(object sender, RoutedEventArgs e) => OpenUnits();

    private void OpenUnits()
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

    private void OpenTextStyle()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_textStyleWindow == null)
        {
            _textStyleWindow = TextStyleWindow.Open(this, _host, AfterCommand);
            _textStyleWindow.Closed += (_, _) => _textStyleWindow = null;
        }
        _textStyleWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_textStyleWindow), 8);
    }

    private void OpenPageSetup()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_pageSetupWindow == null)
        {
            _pageSetupWindow = PageSetupWindow.Open(this, _host, AfterCommand);
            _pageSetupWindow.Closed += (_, _) => _pageSetupWindow = null;
        }
        _pageSetupWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_pageSetupWindow), 8);
    }

    private void OpenPlotStyles()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_plotStyleWindow == null)
        {
            _plotStyleWindow = PlotStyleWindow.Open(this, _host, _space, AfterCommand);
            _plotStyleWindow.Closed += (_, _) => _plotStyleWindow = null;
        }
        _plotStyleWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_plotStyleWindow), 8);
    }

    private void OpenBatchPlot()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_batchPlotWindow == null)
        {
            _batchPlotWindow = BatchPlotWindow.Open(this, _host, AfterCommand);
            _batchPlotWindow.Closed += (_, _) => _batchPlotWindow = null;
        }
        _batchPlotWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_batchPlotWindow), 8);
    }

    private void OpenDimStyle()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_dimStyleWindow == null)
        {
            _dimStyleWindow = DimStyleWindow.Open(this, _host, AfterCommand);
            _dimStyleWindow.Closed += (_, _) => _dimStyleWindow = null;
        }
        _dimStyleWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_dimStyleWindow), 8);
    }

    private void OpenLinetype()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_linetypeWindow == null)
        {
            _linetypeWindow = LinetypeWindow.Open(this, _host, AfterCommand);
            _linetypeWindow.Closed += (_, _) => _linetypeWindow = null;
        }
        _linetypeWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_linetypeWindow), 8);
    }

    private void OpenPointStyle()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_pointStyleWindow == null)
        {
            _pointStyleWindow = PointStyleWindow.Open(this, _host, AfterCommand);
            _pointStyleWindow.Closed += (_, _) => _pointStyleWindow = null;
        }
        _pointStyleWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_pointStyleWindow), 8);
    }

    private void OpenMLeaderStyle()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_mleaderStyleWindow == null)
        {
            _mleaderStyleWindow = MLeaderStyleWindow.Open(this, _host, AfterCommand);
            _mleaderStyleWindow.Closed += (_, _) => _mleaderStyleWindow = null;
        }
        _mleaderStyleWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_mleaderStyleWindow), 8);
    }

    private void OpenRoomSettings()
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (_roomSettingsWindow == null)
        {
            _roomSettingsWindow = RoomSettingsWindow.Open(this, _host, AfterCommand);
            _roomSettingsWindow.Closed += (_, _) => _roomSettingsWindow = null;
        }
        _roomSettingsWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_roomSettingsWindow), 8);
    }

    private void OpenLayers()
    {
        EnsureLayers();
        if (_layersWindow == null)
        {
            return;
        }
        if (_mergeLayers)
        {
            _leftCollapsed = false;
            ApplyColumns();
            _layersWindow.Reload("");
            return;
        }
        _layersWindow.AttachBody();
        _layersWindow.Reload("");
        _layersWindow.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(_layersWindow), 8);
    }

    private void OnMergeLayers(object sender, RoutedEventArgs e) => SetLayerMerge(!_mergeLayers, showWindow: _mergeLayers);

    private void EnsureLayers()
    {
        if (_host == IntPtr.Zero || _layersWindow != null)
        {
            return;
        }
        _layersWindow = new LayerManagerWindow(Content as FrameworkElement, _host, AfterCommand);
        _layersWindow.MergeRequested += (_, _) => SetLayerMerge(!_mergeLayers, showWindow: _mergeLayers);
        _layersWindow.AppWindow.Closing += (_, args) =>
        {
            if (_shellClosing)
            {
                return;
            }
            args.Cancel = true;
            SetLayerMerge(true, showWindow: false);
        };
        _layersWindow.Closed += (_, _) => _layersWindow = null;
    }

    private void SetLayerMerge(bool merge, bool showWindow)
    {
        _mergeLayers = merge;
        ApplyLayerHost(showWindow);
        SaveShell();
    }

    private void ApplyLayerHost(bool showWindow)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        EnsureLayers();
        if (_layersWindow == null)
        {
            return;
        }
        _layersWindow.ShowMerged(_mergeLayers);
        if (_mergeLayers)
        {
            if (_leftWidth < 480)
            {
                _leftWidth = 520;
            }
            _leftCollapsed = false;
            _layersWindow.DetachBody();
            if (!LayersSlot.Children.Contains(_layersWindow.Body()))
            {
                LayersSlot.Children.Add(_layersWindow.Body());
            }
            _layersWindow.Conceal();
            _layersWindow.Reload("");
            Density.Apply(_layersWindow.Body());
        }
        else
        {
            _layersWindow.AttachBody();
            if (showWindow)
            {
                _layersWindow.Reload("");
                _layersWindow.Activate();
                Native.ShowWindow(WindowNative.GetWindowHandle(_layersWindow), 8);
            }
        }
        ApplyColumns();
    }

    private void FillProperties(PanelSnapshot snapshot)
    {
        PropertiesPanel.Children.Clear();
        if (snapshot.Selection > 0)
        {
            if (_quickJson != null)
            {
                FillSelectionProperties(_quickJson);
            }
            return;
        }
        PropertiesPanel.Children.Add(Heading("Properties"));
        PropertiesPanel.Children.Add(FieldLabel("No selection"));
        var layers = new List<string>();
        var currentLayer = "0";
        foreach (var layer in snapshot.Layers)
        {
            layers.Add(layer.Name);
            if (layer.Current)
            {
                currentLayer = layer.Name;
            }
        }
        AddChoice("Color", new[] { "ByLayer", "ByBlock", "Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "White" }, Row(snapshot, "Color"), value =>
            RunProp("color", new Dictionary<string, object> { ["color"] = value }));
        AddChoice("Layer", layers, currentLayer, value =>
            RunProp("layer.current", new Dictionary<string, object> { ["name"] = value }));
        AddChoice("Linetype", new[] { "ByLayer", "ByBlock", "Continuous" }, Row(snapshot, "Linetype"), value =>
            RunProp("linetype", new Dictionary<string, object> { ["current"] = value }));
        AddChoice("Lineweight", new[] { "ByLayer", "ByBlock", "Default", "0.25", "0.50", "1.00" }, NormalizeWeight(Row(snapshot, "Lineweight")), value =>
        {
            if (WeightValue(value) is object weight)
            {
                RunProp("lweight", new Dictionary<string, object> { ["lineweight"] = weight });
            }
        });
    }

    private static string Row(PanelSnapshot snapshot, string label)
    {
        foreach (var row in snapshot.Properties)
        {
            if (row.Label == label)
            {
                return row.Value;
            }
        }
        return "ByLayer";
    }

    private static string NormalizeWeight(string value)
    {
        if (value.Contains("0.25", StringComparison.Ordinal))
        {
            return "0.25";
        }
        if (value.Contains("0.50", StringComparison.Ordinal) || value.Contains("0.5 ", StringComparison.Ordinal))
        {
            return "0.50";
        }
        if (value.Contains("1.00", StringComparison.Ordinal) || value.Contains("1.0 ", StringComparison.Ordinal))
        {
            return "1.00";
        }
        if (value.Contains("Default", StringComparison.OrdinalIgnoreCase))
        {
            return "Default";
        }
        if (value.Contains("ByBlock", StringComparison.OrdinalIgnoreCase))
        {
            return "ByBlock";
        }
        return value.Contains("ByLayer", StringComparison.OrdinalIgnoreCase) ? "ByLayer" : value;
    }

    private static object? WeightValue(string value)
    {
        if (value is "ByLayer" or "ByBlock" or "Default")
        {
            return value;
        }
        var token = value.Replace("mm", "", StringComparison.OrdinalIgnoreCase).Trim();
        return double.TryParse(token, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var mm) && double.IsFinite(mm)
            ? mm
            : null;
    }

    private void FillSelectionProperties(string json)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(json);
        }
        catch (JsonException)
        {
            return;
        }
        using (doc)
        {
            var root = doc.RootElement;
            var handles = new List<string>();
            if (root.TryGetProperty("handles", out var listed) && listed.ValueKind == JsonValueKind.Array)
            {
                foreach (var handle in listed.EnumerateArray())
                {
                    if (handle.ValueKind == JsonValueKind.String && handle.GetString() is string text && text.Length > 0)
                    {
                        handles.Add(text);
                    }
                }
            }
            var title = root.TryGetProperty("title", out var heading) && heading.ValueKind == JsonValueKind.String ? heading.GetString() ?? "Properties" : "Properties";
            PropertiesPanel.Children.Add(Heading(title));
            var layers = Names(root, "layers");
            var linetypes = Names(root, "linetypes");
            AddChoice("Layer", layers, Text(root, "layer"), value => CommitChoice(handles, "layer", value));
            AddChoice("Color", new[] { "ByLayer", "ByBlock", "Red", "Yellow", "Green", "Cyan", "Blue", "Magenta", "White" }, Text(root, "color"), value => CommitChoice(handles, "color", value));
            AddChoice("Linetype", linetypes.Count > 0 ? linetypes : new List<string> { "ByLayer", "ByBlock", "Continuous" }, Text(root, "linetype"), value => CommitChoice(handles, "linetype", value));
            AddChoice("Lineweight", new[] { "ByLayer", "ByBlock", "Default", "0.25", "0.50", "1.00" }, NormalizeWeight(Text(root, "lineweight")), value =>
            {
                if (WeightValue(value) is object weight)
                {
                    CommitChoice(handles, "lineweight", weight);
                }
            });
            AddChoice("Transparency", new[] { "ByLayer", "ByBlock", "0", "30", "50", "70", "90" }, Text(root, "transparency"), value => CommitChoice(handles, "transparency", value));
            AddNumber(handles, "ltscale", "Linetype scale", Text(root, "ltscale"));
            var fields = new Dictionary<string, TextBox>(StringComparer.Ordinal);
            if (root.TryGetProperty("fields", out var rows) && rows.ValueKind == JsonValueKind.Array)
            {
                foreach (var row in rows.EnumerateArray())
                {
                    var key = row.TryGetProperty("key", out var keyValue) && keyValue.ValueKind == JsonValueKind.String ? keyValue.GetString() ?? "" : "";
                    var label = row.TryGetProperty("label", out var labelValue) && labelValue.ValueKind == JsonValueKind.String ? labelValue.GetString() ?? key : key;
                    var value = row.TryGetProperty("value", out var fieldValue) && fieldValue.ValueKind == JsonValueKind.String ? fieldValue.GetString() ?? "" : "";
                    var text = row.TryGetProperty("text", out var kind) && kind.ValueKind == JsonValueKind.True;
                    if (key.Length == 0)
                    {
                        PropertiesPanel.Children.Add(FieldLabel(label));
                        var read = new TextBlock { Text = value, TextWrapping = TextWrapping.Wrap };
                        AutomationProperties.SetName(read, label + " " + value);
                        PropertiesPanel.Children.Add(read);
                        continue;
                    }
                    var box = new TextBox { Text = value, Tag = value, MinHeight = Density.MinTarget };
                    AutomationProperties.SetName(box, label);
                    fields[key] = box;
                    box.LostFocus += (_, _) => CommitPropField(box, key, text, handles, fields);
                    box.KeyDown += (_, args) =>
                    {
                        if (args.Key == Windows.System.VirtualKey.Enter)
                        {
                            CommitPropField(box, key, text, handles, fields);
                            args.Handled = true;
                        }
                    };
                    PropertiesPanel.Children.Add(FieldLabel(label));
                    PropertiesPanel.Children.Add(box);
                }
            }
        }
    }

    private static List<string> Names(JsonElement root, string name)
    {
        var names = new List<string>();
        if (!root.TryGetProperty(name, out var listed) || listed.ValueKind != JsonValueKind.Array)
        {
            return names;
        }
        foreach (var item in listed.EnumerateArray())
        {
            if (item.ValueKind == JsonValueKind.String && item.GetString() is string text && text.Length > 0)
            {
                names.Add(text);
            }
        }
        return names;
    }

    private static string Text(JsonElement root, string name) =>
        root.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() ?? "" : "";

    private void AddChoice(string label, IEnumerable<string> names, string current, Action<string> commit)
    {
        var box = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch, MinHeight = Density.MinTarget };
        AutomationProperties.SetName(box, label);
        _filling = true;
        foreach (var name in names)
        {
            if (name.Length == 0 || box.Items.OfType<ComboBoxItem>().Any(item => item.Tag as string == name))
            {
                continue;
            }
            box.Items.Add(new ComboBoxItem { Content = name, Tag = name });
        }
        if (current.Length > 0 && box.Items.OfType<ComboBoxItem>().All(item => item.Tag as string != current))
        {
            box.Items.Insert(0, new ComboBoxItem { Content = current, Tag = current });
        }
        for (var i = 0; i < box.Items.Count; i++)
        {
            if (box.Items[i] is ComboBoxItem item && item.Tag as string == current)
            {
                box.SelectedIndex = i;
                break;
            }
        }
        _filling = false;
        box.SelectionChanged += (_, _) =>
        {
            if (_filling || box.SelectedItem is not ComboBoxItem picked || picked.Tag is not string value || value == "*VARIES*")
            {
                return;
            }
            commit(value);
        };
        PropertiesPanel.Children.Add(FieldLabel(label));
        PropertiesPanel.Children.Add(box);
    }

    private void RunProp(string command, Dictionary<string, object> values)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        if (Native.Execute(_host, command, JsonSerializer.Serialize(values)) != 0)
        {
            Status.Text = Native.LastError(_host);
            return;
        }
        _panelKey = "";
        RefreshChrome();
        RefreshPanels();
    }

    private void AddNumber(List<string> handles, string key, string label, string value)
    {
        if (value.Length == 0)
        {
            return;
        }
        var box = new TextBox { Text = value, Tag = value, MinHeight = Density.MinTarget };
        AutomationProperties.SetName(box, label);
        box.LostFocus += (_, _) => CommitPropField(box, key, false, handles, new Dictionary<string, TextBox>());
        box.KeyDown += (_, args) =>
        {
            if (args.Key == Windows.System.VirtualKey.Enter)
            {
                CommitPropField(box, key, false, handles, new Dictionary<string, TextBox>());
                args.Handled = true;
            }
        };
        PropertiesPanel.Children.Add(FieldLabel(label));
        PropertiesPanel.Children.Add(box);
    }

    private void CommitChoice(List<string> handles, string key, object value)
    {
        if (handles.Count == 0 || value is "*VARIES*")
        {
            return;
        }
        RunProp("properties.set", new Dictionary<string, object> { ["handles"] = handles, [key] = value });
    }

    private void CommitPropField(TextBox box, string key, bool text, List<string> handles, Dictionary<string, TextBox> fields)
    {
        var value = box.Text.Trim();
        if (handles.Count == 0 || value.Length == 0 || value == "*VARIES*" || value == box.Tag as string)
        {
            return;
        }
        Dictionary<string, object> values;
        if (key is "centerX" or "centerY")
        {
            if (!fields.TryGetValue("centerX", out var xBox) || !fields.TryGetValue("centerY", out var yBox)
                || !double.TryParse(xBox.Text, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var x)
                || !double.TryParse(yBox.Text, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var y)
                || !double.IsFinite(x) || !double.IsFinite(y))
            {
                Status.Text = "Enter a number.";
                return;
            }
            values = new Dictionary<string, object> { ["handles"] = handles, ["center"] = new[] { x, y } };
        }
        else if (text)
        {
            values = new Dictionary<string, object> { ["handles"] = handles, [key] = value };
        }
        else if (double.TryParse(value, System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var number) && double.IsFinite(number))
        {
            values = new Dictionary<string, object> { ["handles"] = handles, [key] = number };
        }
        else
        {
            Status.Text = "Enter a number.";
            return;
        }
        box.Tag = value;
        RunProp("properties.set", values);
    }

    private void FillLayers(PanelSnapshot snapshot)
    {
        LayersPanel.Children.Clear();
        LayersPanel.Children.Add(Heading("Layers"));
        LayersPanel.Children.Add(LayerHeader());
        for (var i = 0; i < snapshot.Layers.Count; i++)
        {
            LayersPanel.Children.Add(LayerLine(snapshot.Layers[i], i));
        }
        if (snapshot.LayersTruncated)
        {
            LayersPanel.Children.Add(FieldLabel("Further layers are not listed."));
        }
    }

    private static Grid LayerHeader()
    {
        var header = LayerGrid();
        LayerCell(header, 0, "Status");
        LayerCell(header, 1, "Name");
        LayerCell(header, 2, "On");
        LayerCell(header, 3, "Lock");
        return header;
    }

    private Grid LayerLine(LayerRow layer, int index)
    {
        var name = layer.Name;
        var grid = LayerGrid();
        grid.Background = layer.Current
            ? new SolidColorBrush(Windows.UI.Color.FromArgb(64, 0, 120, 215))
            : index % 2 == 1
                ? new SolidColorBrush(Windows.UI.Color.FromArgb(16, 128, 128, 128))
                : new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        var status = layer.Current ? "check" : "layers";
        var statusTip = layer.Current ? "Current layer " + name : "Make " + name + " the current layer";
        Put(grid, 0, LayerIcon(status, statusTip, () => RunLayer("layer.current", new Dictionary<string, object> { ["name"] = name })));
        var label = new Button
        {
            Content = new TextBlock { Text = name, TextTrimming = TextTrimming.CharacterEllipsis, VerticalAlignment = VerticalAlignment.Center },
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            MinHeight = Density.MinTarget,
            Padding = new Thickness(4, 0, 4, 0),
            FontWeight = layer.Current ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
        };
        AutomationProperties.SetName(label, "Layer " + name);
        label.Click += (_, _) => RunLayer("layer.current", new Dictionary<string, object> { ["name"] = name });
        Put(grid, 1, label);
        var onKind = layer.On ? "bulb" : "bulb-off";
        var onTip = (layer.On ? "Turn off layer " : "Turn on layer ") + name;
        Put(grid, 2, LayerIcon(onKind, onTip, () => RunLayer("layer.set", new Dictionary<string, object> { ["name"] = name, ["on"] = !layer.On })));
        var lockKind = layer.Locked ? "lock" : "unlock";
        var lockTip = (layer.Locked ? "Unlock layer " : "Lock layer ") + name;
        Put(grid, 3, LayerIcon(lockKind, lockTip, () => RunLayer("layer.set", new Dictionary<string, object> { ["name"] = name, ["locked"] = !layer.Locked })));
        Density.Apply(grid);
        return grid;
    }

    private static Grid LayerGrid()
    {
        var grid = new Grid { MinHeight = Density.MinTarget };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(44) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(Density.MinTarget) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(Density.MinTarget) });
        return grid;
    }

    private static void LayerCell(Grid grid, int column, string title)
    {
        var text = new TextBlock
        {
            Text = title,
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(4, 0, 0, 0),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        Put(grid, column, text);
    }

    private static Button LayerIcon(string kind, string access, Action click)
    {
        var button = new Button
        {
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            Width = Density.MinTarget,
            Height = Density.MinTarget,
            HorizontalAlignment = HorizontalAlignment.Center,
            Content = new DrawnIcon { Kind = kind },
        };
        AutomationProperties.SetName(button, access);
        button.Click += (_, _) => click();
        return button;
    }

    private static void Put(Grid grid, int column, FrameworkElement child)
    {
        Grid.SetColumn(child, column);
        grid.Children.Add(child);
    }

    private string? _selectedBlock;

    private void FillBlocks(PanelSnapshot snapshot)
    {
        BlocksPanel.Children.Clear();
        BlocksPanel.Children.Add(Heading("Blocks"));
        if (snapshot.Blocks.Count == 0)
        {
            BlocksPanel.Children.Add(FieldLabel("This drawing defines no blocks."));
        }
        if (!string.IsNullOrEmpty(_selectedBlock) && snapshot.Blocks.All(b => !string.Equals(b.Name, _selectedBlock, StringComparison.OrdinalIgnoreCase)))
        {
            _selectedBlock = null;
        }
        foreach (var block in snapshot.Blocks)
        {
            var name = block.Name;
            var selected = string.Equals(_selectedBlock, name, StringComparison.OrdinalIgnoreCase);
            var line = new Border
            {
                Padding = new Thickness(4, 4, 4, 4),
                CornerRadius = new CornerRadius(3),
                Background = selected ? SelectedBlockBrush() : null,
            };
            var stack = new StackPanel { Spacing = 2 };
            stack.Children.Add(new TextBlock { Text = name, FontWeight = selected ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal });
            stack.Children.Add(FieldLabel(block.Entities + " objects · " + block.References + " references"));
            line.Child = stack;
            line.PointerPressed += (_, _) =>
            {
                _selectedBlock = name;
                _panelKey = "";
                RefreshPanels();
            };
            BlocksPanel.Children.Add(line);
        }
        var focus = snapshot.Blocks.FirstOrDefault(b => string.Equals(b.Name, _selectedBlock, StringComparison.OrdinalIgnoreCase));
        if (focus != null)
        {
            BlocksPanel.Children.Add(new Border { Height = 1, Margin = new Thickness(0, 8, 0, 8), Background = StripBrush() });
            var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            var insert = new Button { Content = "Insert", MinHeight = Density.MinTarget };
            Density.Name(insert, "Insert block " + focus.Name);
            insert.Click += (_, _) => InsertBlock(focus.Name);
            actions.Children.Add(insert);
            var select = new Button
            {
                Content = "Select references",
                MinHeight = Density.MinTarget,
                IsEnabled = focus.Handles.Count > 0,
            };
            Density.Name(select, "Select references of " + focus.Name);
            select.Click += (_, _) => SelectBlockReferences(focus);
            actions.Children.Add(select);
            BlocksPanel.Children.Add(actions);
            if (focus.Attributes.Count > 0)
            {
                BlocksPanel.Children.Add(FieldLabel("Attribute definitions"));
                foreach (var attr in focus.Attributes)
                {
                    BlocksPanel.Children.Add(new TextBlock { Text = attr });
                }
                BlocksPanel.Children.Add(FieldLabel("Edit attribute values on a reference with ATTEDIT."));
            }
        }
        if (snapshot.BlocksTruncated)
        {
            BlocksPanel.Children.Add(FieldLabel("Further blocks are not listed."));
        }
        BlocksPanel.Children.Add(FieldLabel("External references are not supported."));
    }

    private static Brush? SelectedBlockBrush()
    {
        if (Application.Current.Resources.TryGetValue("SubtleFillColorSecondaryBrush", out var value) && value is Brush brush)
        {
            return brush;
        }
        return null;
    }

    private void InsertBlock(string name)
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
    }

    private void SelectBlockReferences(BlockRow block)
    {
        if (_host == IntPtr.Zero || block.Handles.Count == 0)
        {
            return;
        }
        var json = JsonSerializer.Serialize(new Dictionary<string, object> { ["handles"] = block.Handles });
        if (Native.Execute(_host, "select", json) != 0)
        {
            History.Text = Native.LastError(_host);
            return;
        }
        _panelKey = "";
        RefreshChrome();
        RefreshPanels();
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

    private static TextBlock Heading(string text)
    {
        var block = new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        AutomationProperties.SetName(block, text);
        return block;
    }

    private static TextBlock FieldLabel(string text)
    {
        Brush? brush = null;
        if (Application.Current.Resources.TryGetValue("TextFillColorSecondaryBrush", out var value) && value is Brush secondary)
        {
            brush = secondary;
        }
        var block = new TextBlock
        {
            Text = text,
            Margin = new Thickness(0, 8, 0, 0),
        };
        if (brush != null)
        {
            block.Foreground = brush;
        }
        return block;
    }

    private void PlaceChild()
    {
        if (_host == IntPtr.Zero || CanvasHost.XamlRoot == null || Content is not FrameworkElement root)
        {
            return;
        }
        if (_start)
        {
            StartPage.Visibility = Visibility.Visible;
            Native.SetCanvas(_host, false);
            return;
        }
        StartPage.Visibility = Visibility.Collapsed;
        Native.SetCanvas(_host, true);
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
        PlaceCommandBar();
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
        if (run == "ucs")
        {
            _ucs = id != "hide";
            Native.SetUcs(_host, _ucs);
            return;
        }
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
            CanvasMenu.DismissOutsideClick();
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
            DrainPlotRequest();
            TickCollapse();
            _ticks++;
            SyncHistory();
            UpdateDyn();
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
        ToolStrip.Children.Clear();
        foreach (var (id, label) in ToolCatalog.Where(tool => _toolIds.Contains(tool.Id)))
        {
            var command = id;
            var button = new Button
            {
                Content = ToolIcons.For(id, _dark),
                Width = 32,
                Height = 32,
                Padding = new Thickness(0),
            };
            AutomationProperties.SetName(button, label);
            ToolTipService.SetToolTip(button, label);
            button.Click += (_, _) => StartCommand(command);
            ToolStrip.Children.Add(button);
        }
        var gear = new Button
        {
            Content = ToolIcons.For("gear", _dark),
            Width = 32,
            Height = 32,
            Padding = new Thickness(0),
        };
        AutomationProperties.SetName(gear, "Customize tool bar");
        ToolTipService.SetToolTip(gear, "Customize tool bar");
        gear.Click += (_, _) => OpenTools();
        ToolStrip.Children.Add(gear);
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
                Content = new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    Spacing = 4,
                    Children =
                    {
                        new DrawnIcon { Kind = layout.Equals("Model", StringComparison.OrdinalIgnoreCase) ? "model" : "paper" },
                        new TextBlock
                        {
                            Text = layout,
                            VerticalAlignment = VerticalAlignment.Center,
                            FontWeight = current ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
                        },
                    },
                },
                MinHeight = Density.MinTarget,
                Padding = new Thickness(8, 4, 8, 4),
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
        var key = (_start ? "start\n" : "draw\n") + string.Join("\n", _documents.Select(doc => doc.Index + ":" + doc.Title + ":" + doc.Dirty + ":" + doc.Active));
        if (key == _docKey)
        {
            return;
        }
        _docKey = key;
        _buildingTabs = true;
        FileTabs.TabItems.Clear();
        var start = new TabViewItem
        {
            Header = TabLabel("Start"),
            IsClosable = false,
            MinWidth = 88,
        };
        AutomationProperties.SetName(start, "Start page");
        FileTabs.TabItems.Add(start);
        var selected = 0;
        foreach (var doc in _documents.ToList())
        {
            var title = doc.Dirty ? doc.Title + " *" : doc.Title;
            var tab = new TabViewItem
            {
                Header = TabLabel(title),
                IsClosable = true,
                MinWidth = 120,
                Tag = doc.Index,
            };
            AutomationProperties.SetName(tab, doc.Active ? "Current drawing " + doc.Title : "Switch to " + doc.Title);
            FileTabs.TabItems.Add(tab);
            if (!_start && doc.Active)
            {
                selected = FileTabs.TabItems.Count - 1;
            }
        }
        FileTabs.SelectedIndex = selected;
        _buildingTabs = false;
    }

    private static TextBlock TabLabel(string text) => new()
    {
        Text = text,
        MaxWidth = 180,
        TextTrimming = TextTrimming.CharacterEllipsis,
        VerticalAlignment = VerticalAlignment.Center,
    };

    private void OnNewDrawingTab(TabView sender, object args)
    {
        ShowStart(false);
        StartCommand("new");
    }

    private void OnDrawingTabSelected(object sender, SelectionChangedEventArgs e)
    {
        if (_buildingTabs || _host == IntPtr.Zero || FileTabs.SelectedItem is not TabViewItem tab)
        {
            return;
        }
        if (tab.Tag is not int index)
        {
            ShowStart(true);
            return;
        }
        var doc = _documents.FirstOrDefault(item => item.Index == index);
        if (!_start && doc is { Active: true })
        {
            return;
        }
        ShowStart(false);
        if (doc is not { Active: true } && Native.Execute(_host, "document.switch", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = index })) != 0)
        {
            History.Text = Native.LastError(_host);
        }
        _docKey = "";
        _layoutKey = "";
        _panelKey = "";
        AfterCommand();
    }

    private void OnDrawingTabClose(TabView sender, TabViewTabCloseRequestedEventArgs args)
    {
        if (args.Item is TabViewItem tab && tab.Tag is int index)
        {
            _ = CloseDocument(index);
        }
    }

    private async Task CloseDocument(int index)
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
            var doc = _documents.FirstOrDefault(item => item.Index == index);
            if (doc == null)
            {
                return;
            }
            if (!doc.Active)
            {
                Native.Execute(_host, "document.switch", JsonSerializer.Serialize(new Dictionary<string, object> { ["index"] = index }));
                _docKey = "";
                RefreshChrome();
            }
            if (doc.Dirty && !await DiscardOrSave(doc.Title))
            {
                return;
            }
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

    private void LoadShell()
    {
        var prefs = ShellPrefs.Load();
        if (prefs.Tools.Count > 0)
        {
            _toolIds = prefs.Tools.Where(id => ToolCatalog.Any(tool => tool.Id == id)).ToList();
        }
        _font = string.IsNullOrEmpty(prefs.Font) ? "engineering" : prefs.Font;
        _showMenu = prefs.Menu;
        _showTabs = prefs.Tabs;
        _showStatus = prefs.Status;
        _showCommand = prefs.Command;
        _pinRight = prefs.PinRight;
        _modeSlots.Clear();
        foreach (var (key, slot) in prefs.ModeSlots)
        {
            if (slot != null && (key == "classic" || key == "pro"))
            {
                _modeSlots[key] = slot;
            }
        }
        if (!_modeSlots.ContainsKey("classic"))
        {
            _modeSlots["classic"] = new ShellPrefs.ModeSlot
            {
                PinLeft = prefs.PinLeft,
                ToolsVisible = prefs.ToolsVisible,
                LeftWidth = 280,
                LeftCollapsed = false,
            };
        }
        if (!_modeSlots.ContainsKey("pro"))
        {
            _modeSlots["pro"] = DefaultSlot(classic: false);
        }
        _classic = !string.Equals(prefs.Workspace, "pro", StringComparison.OrdinalIgnoreCase);
        ApplySlot(_modeSlots.TryGetValue(WorkspaceKey(), out var current) ? current : DefaultSlot(_classic));
        ToolTipService.SetToolTip(WorkspaceButton, _classic ? "Classic" : "Pro");
        AutomationProperties.SetName(WorkspaceButton, _classic ? "Classic workspace" : "Pro workspace");
        _leftIdleSince = DateTime.UtcNow;
        _mergeLayers = true;
        _ucs = prefs.Ucs;
        _barDx = prefs.BarDx;
        _barDy = prefs.BarDy;
        Density.Use(_font);
        Native.SetUcs(_host, _ucs);
        ApplyChrome();
        ApplyLayerHost(showWindow: false);
    }

    private void SaveShell()
    {
        _modeSlots[WorkspaceKey()] = CaptureSlot();
        ShellPrefs.Save(new ShellPrefs.Prefs
        {
            Tools = _toolIds,
            Font = _font,
            Menu = _showMenu,
            ToolsVisible = _showTools,
            Tabs = _showTabs,
            Status = _showStatus,
            Command = _showCommand,
            PinLeft = _pinLeft,
            PinRight = _pinRight,
            MergeLayers = _mergeLayers,
            Ucs = _ucs,
            BarDx = _barDx,
            BarDy = _barDy,
            Workspace = WorkspaceKey(),
            ModeSlots = new Dictionary<string, ShellPrefs.ModeSlot>(_modeSlots),
        });
    }

    private void Listen()
    {
        ushort port = _launch.ControlPort ?? 0;
        if (port == 0
            && ushort.TryParse(Environment.GetEnvironmentVariable("AAD_CONTROL_PORT"), out var parsed))
        {
            port = parsed;
        }
        if (Native.Listen(_host, port) < 0)
        {
            Status.Text = Native.LastError(_host);
        }
    }

    private void ApplyChrome()
    {
        Bar.Visibility = _showMenu ? Visibility.Visible : Visibility.Collapsed;
        FileTabs.Visibility = _showTabs ? Visibility.Visible : Visibility.Collapsed;
        ToolRow.Height = new GridLength(0);
        PlaceCommandBar();
        StatusRow.Visibility = _showStatus ? Visibility.Visible : Visibility.Collapsed;
        StartPage.Visibility = _start ? Visibility.Visible : Visibility.Collapsed;
        ApplyColumns();
    }

    private void ApplyColumns()
    {
        // Pro hides the tool strip; Classic honours the per-mode toolbar switch.
        var tools = _classic && _showTools;
        ToolColumn.Width = tools ? GridLength.Auto : new GridLength(0);
        ToolStripHost.Visibility = tools ? Visibility.Visible : Visibility.Collapsed;
        var leftOpen = _leftFloat != null || _pinLeft || _hoverLeft || !_leftCollapsed;
        // Floated panels leave only the context/pin strip in the dock.
        var dockWidth = _leftFloat != null ? Math.Min(_leftWidth, 200) : _leftWidth;
        PropertiesColumn.Width = new GridLength(leftOpen ? dockWidth : 12);
        LeftSplitterColumn.Width = leftOpen && _leftFloat == null ? new GridLength(8) : new GridLength(0);
        LeftSplitter.Visibility = leftOpen && _leftFloat == null ? Visibility.Visible : Visibility.Collapsed;
        SideTabs.Visibility = leftOpen && _leftFloat == null ? Visibility.Visible : Visibility.Collapsed;
        ContextBar.Visibility = leftOpen ? Visibility.Visible : Visibility.Collapsed;
        PinLeftButton.Visibility = leftOpen ? Visibility.Visible : Visibility.Collapsed;
        FloatLeftButton.Visibility = leftOpen ? Visibility.Visible : Visibility.Collapsed;
        LeftColumn.Background = leftOpen ? null : StripBrush();
        LayersHost.Visibility = !_mergeLayers ? Visibility.Visible : Visibility.Collapsed;
    }

    /// Unpinned panels stay open while the pointer is over them or a field in them has focus,
    /// then collapse to the edge strip 1.5 s after the pointer leaves.
    private void TickCollapse()
    {
        if (_leftFloat != null)
        {
            return;
        }
        var now = DateTime.UtcNow;
        if (ColumnBusy(LeftColumn) || _dragSplit != null)
        {
            _leftIdleSince = now;
        }
        var changed = false;
        if (!_pinLeft && !_hoverLeft && (now - _leftIdleSince).TotalMilliseconds >= 1500 && !_leftCollapsed)
        {
            _leftCollapsed = true;
            changed = true;
        }
        if (changed)
        {
            ApplyColumns();
        }
    }

    private static Brush StripBrush()
    {
        if (Application.Current.Resources.TryGetValue("SubtleFillColorSecondaryBrush", out var value) && value is Brush brush)
        {
            return brush;
        }
        return new SolidColorBrush(Windows.UI.Color.FromArgb(255, 96, 96, 96));
    }

    private static bool ColumnBusy(DependencyObject column)
    {
        if (column is not FrameworkElement element || element.XamlRoot == null)
        {
            return false;
        }
        return FocusManager.GetFocusedElement(element.XamlRoot) is DependencyObject focused && IsDescendant(column, focused);
    }

    private static bool IsDescendant(DependencyObject root, DependencyObject node)
    {
        var current = node;
        while (current != null)
        {
            if (current == root)
            {
                return true;
            }
            current = VisualTreeHelper.GetParent(current);
        }
        return false;
    }

    private void EnsureShellMenus()
    {
        MenuBarItem? window = null;
        foreach (var item in Bar.Items)
        {
            if (item is MenuBarItem top && top.Title?.ToString() == "Window")
            {
                window = top;
            }
        }
        if (window == null)
        {
            window = new MenuBarItem { Title = "Window" };
            Bar.Items.Add(window);
        }
        ClearShellItems(window);
        CheckItem(window, "Menu bar", () => _showMenu, value => _showMenu = value);
        CheckItem(window, "Tool bar", () => _showTools, value => _showTools = value);
        CheckItem(window, "File tabs", () => _showTabs, value => _showTabs = value);
        CheckItem(window, "Status bar", () => _showStatus, value => _showStatus = value);
        CheckItem(window, "Command line", () => _showCommand, value => _showCommand = value);
        CheckItem(window, "Pin side panels", () => _pinLeft, value =>
        {
            _pinLeft = value;
            _leftCollapsed = false;
            _leftIdleSince = DateTime.UtcNow;
            UpdatePinButton();
        });
        CommandItem(window, "Properties", () => ShowSidePanel(SideTab.Properties));
        CommandItem(window, "Layers palette", () => ShowSidePanel(SideTab.Layers));
        CommandItem(window, "Blocks", () => ShowSidePanel(SideTab.Blocks));
        CommandItem(window, "Classic workspace", () => SwitchWorkspace(classic: true));
        CommandItem(window, "Pro workspace", () => SwitchWorkspace(classic: false));
        CommandItem(window, "Reset this workspace", ResetCurrentWorkspace);
        CommandItem(window, "Calculator", OpenCalculator);
        CommandItem(window, "Pomodoro", OpenPomodoro);
        CommandItem(window, "AORMS", OpenAorms);
        CommandItem(window, "Customize tool bar", OpenTools);
        CommandItem(window, "Start page", () => ShowStart(true));
        CommandItem(window, "UCS icon", () =>
        {
            _ucs = !_ucs;
            Native.SetUcs(_host, _ucs);
            SaveShell();
        });
    }

    private enum SideTab
    {
        Properties,
        Layers,
        Blocks,
    }

    /// Expands the side column (and floats it back if needed), then selects a panel tab.
    private void ShowSidePanel(SideTab tab)
    {
        if (_leftFloat != null)
        {
            _leftFloat.Activate();
        }
        else
        {
            _leftCollapsed = false;
            _leftIdleSince = DateTime.UtcNow;
            ApplyColumns();
            PlaceChild();
        }
        var item = tab switch
        {
            SideTab.Layers => LayersTab,
            SideTab.Blocks => BlocksTab,
            _ => PropertiesTab,
        };
        SideTabs.SelectedItem = item;
    }

    private void CheckItem(MenuBarItem menu, string label, Func<bool> read, Action<bool> write)
    {
        var item = new MenuFlyoutItem
        {
            Text = (read() ? "✓ " : "") + label,
            Tag = "shell.toggle." + label,
        };
        item.Click += (_, _) =>
        {
            write(!read());
            item.Text = (read() ? "✓ " : "") + label;
            ApplyChrome();
            SaveShell();
        };
        menu.Items.Add(item);
    }

    private static void CommandItem(MenuBarItem menu, string label, Action run)
    {
        var item = new MenuFlyoutItem { Text = label, Tag = "shell.cmd." + label };
        item.Click += (_, _) => run();
        menu.Items.Add(item);
    }

    private void OnPin(object sender, RoutedEventArgs e)
    {
        _pinLeft = !_pinLeft;
        _leftCollapsed = false;
        _leftIdleSince = DateTime.UtcNow;
        UpdatePinButton();
        ApplyChrome();
        SaveShell();
    }

    private void UpdatePinButton()
    {
        if (PinLeftButton.Content is DrawnIcon icon)
        {
            icon.Kind = _pinLeft ? "pin" : "unpin";
        }
        ToolTipService.SetToolTip(PinLeftButton, _pinLeft ? "Unpin side panel (auto-hide)" : "Pin side panel");
        AutomationProperties.SetName(PinLeftButton, _pinLeft ? "Unpin side panel" : "Pin side panel");
    }

    private void OnFloat(object sender, RoutedEventArgs e)
    {
        if (sender is Button button && button.Tag as string == "layers")
        {
            if (_mergeLayers)
            {
                SetLayerMerge(false, showWindow: true);
            }
            else
            {
                OpenLayers();
            }
            return;
        }
        FloatLeft();
    }

    private void FloatLeft()
    {
        if (_leftFloat != null)
        {
            _leftFloat.Activate();
            return;
        }
        if (SideTabs.Parent is Panel dock)
        {
            dock.Children.Remove(SideTabs);
        }
        var window = new Window { Title = "Panels" };
        var host = new Grid();
        host.Children.Add(SideTabs);
        window.Content = host;
        ShellIcon.Apply(window);
        window.Closed += (_, _) =>
        {
            if (SideTabs.Parent is Panel floated)
            {
                floated.Children.Remove(SideTabs);
            }
            window.Content = null;
            if (LeftColumn is Grid column && !column.Children.Contains(SideTabs))
            {
                Grid.SetRow(SideTabs, 1);
                column.Children.Add(SideTabs);
            }
            _leftFloat = null;
            _leftCollapsed = false;
            _leftIdleSince = DateTime.UtcNow;
            ApplyColumns();
            PlaceChild();
        };
        _leftFloat = window;
        Density.Apply(host);
        ApplyColumns();
        PlaceChild();
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
    }

    private void OnColumnEntered(object sender, PointerRoutedEventArgs e)
    {
        _hoverLeft = true;
        _leftCollapsed = false;
        _leftIdleSince = DateTime.UtcNow;
        ApplyColumns();
    }

    private void OnColumnExited(object sender, PointerRoutedEventArgs e)
    {
        if (_dragSplit != null || sender is not FrameworkElement element)
        {
            return;
        }
        // Leaving a control inside the column bubbles here. That is not leaving the column.
        if (e.OriginalSource is DependencyObject source && source != element && IsDescendant(element, source))
        {
            var point = e.GetCurrentPoint(element).Position;
            if (point.X >= 0 && point.Y >= 0 && point.X < element.ActualWidth && point.Y < element.ActualHeight)
            {
                return;
            }
        }
        _hoverLeft = false;
        _leftIdleSince = DateTime.UtcNow;
    }

    private void ShowStart(bool show)
    {
        _start = show;
        ApplyChrome();
        PlaceChild();
    }

    private void OnStartNew(object sender, RoutedEventArgs e)
    {
        ShowStart(false);
        StartCommand("new");
    }

    private void OnStartOpen(object sender, RoutedEventArgs e)
    {
        ShowStart(false);
        StartCommand("open");
    }

    private void OnStartClose(object sender, RoutedEventArgs e) => ShowStart(false);

    private void OnAorms(object sender, RoutedEventArgs e) => OpenAorms();

    private void OnAssistantAllow(object sender, RoutedEventArgs e)
    {
        Native.Assistant(_host, true);
        RefreshChrome();
    }

    private void OnAssistantDecline(object sender, RoutedEventArgs e)
    {
        Native.Assistant(_host, false);
        RefreshChrome();
    }

    private void OpenAorms()
    {
        if (_aorms != null)
        {
            _aorms.Activate();
            return;
        }
        _aorms = AormsWindow.Show(this, _host, text => AormsButton.Content = text);
        _aorms.Closed += (_, _) => _aorms = null;
    }

    private void OpenCalculator()
    {
        if (_calc != null)
        {
            _calc.Activate();
            return;
        }
        _calc = CalcWindow.Show(this, _host);
        _calc.Closed += (_, _) => _calc = null;
    }

    private void OpenPomodoro()
    {
        if (_pomodoro != null)
        {
            _pomodoro.Activate();
            return;
        }
        _pomodoro = PomodoroWindow.Show(this);
        _pomodoro.Closed += (_, _) => _pomodoro = null;
    }

    private void OpenTools()
    {
        if (_toolsWindow != null)
        {
            _toolsWindow.Activate();
            return;
        }
        _toolsWindow = ToolsWindow.Show(this, ToolCatalog, _toolIds, _font, (ids, font) =>
        {
            if (ids.Count > 0)
            {
                _toolIds = ids.ToList();
            }
            _font = font;
            Density.Use(_font);
            Density.Apply(PropertiesPanel);
            Density.Apply(LayersPanel);
            Density.Apply(BlocksPanel);
            BuildTools();
            SaveShell();
        });
        _toolsWindow.Closed += (_, _) => _toolsWindow = null;
    }

    private void OpenCommandChrome()
    {
        if (_chrome != null)
        {
            return;
        }
        if (CommandBar.Parent is Panel panel)
        {
            panel.Children.Remove(CommandBar);
        }
        _chrome = new CommandChrome();
        _chrome.Attach(CommandBar, WindowNative.GetWindowHandle(this));
        CommandBar.SizeChanged += (_, _) => PlaceCommandBar();
    }

    private void SetHistoryOpen(bool open)
    {
        _historyOpen = open;
        HistoryList.Visibility = open ? Visibility.Visible : Visibility.Collapsed;
        HistoryGlyph.Glyph = open ? "\uE70E" : "\uE70D";
        PlaceCommandBar();
    }

    private void OnHistoryToggle(object sender, RoutedEventArgs e) => SetHistoryOpen(!_historyOpen);

    private void OnHistoryPick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not string line)
        {
            return;
        }
        _armSpace = false;
        CommandInput.Text = line;
        CommandInput.SelectionStart = line.Length;
        CommandInput.Focus(FocusState.Programmatic);
    }

    private void OnClearHistory(object sender, RoutedEventArgs e)
    {
        if (_host == IntPtr.Zero)
        {
            return;
        }
        var clear = new Button { Content = "Clear", MinWidth = 96, MinHeight = Density.MinTarget };
        var cancel = new Button { Content = "Cancel", MinWidth = 96, MinHeight = Density.MinTarget };
        Density.Name(clear, "Clear history");
        Density.Name(cancel, "Cancel");
        var text = new TextBlock
        {
            Text = "Clear the command history? This cannot be undone. The drawing and the current command stay as they are.",
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 420,
        };
        Density.Name(text, text.Text);
        var buttons = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            HorizontalAlignment = HorizontalAlignment.Right,
            Children = { clear, cancel },
        };
        var root = new StackPanel { Spacing = 12, Margin = new Thickness(16), Children = { text, buttons } };
        if (Content is FrameworkElement shell)
        {
            root.RequestedTheme = shell.RequestedTheme;
        }
        var window = new Window { Title = "Clear command history", Content = root };
        ShellIcon.Apply(window);
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32(460, 160));
        var pos = AppWindow.Position;
        window.AppWindow.Move(new Windows.Graphics.PointInt32(pos.X + 80, pos.Y + 120));
        cancel.Click += (_, _) => window.Close();
        clear.Click += (_, _) =>
        {
            window.Close();
            if (Native.ClearLog(_host) != 0)
            {
                History.Text = Native.LastError(_host);
                SyncHistory();
                return;
            }
            _typed.Clear();
            _recall = -1;
            _liveDraft = "";
            History.Text = "";
            SyncHistory();
            RefreshChrome();
        };
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
    }

    private static bool IsCommandControl(DependencyObject source)
    {
        for (DependencyObject? node = source; node != null; node = VisualTreeHelper.GetParent(node))
        {
            if (node is TextBox or Button or ListViewBase)
            {
                return true;
            }
        }
        return false;
    }

    private void OnBarPressed(object sender, PointerRoutedEventArgs e)
    {
        if (_chrome == null || e.OriginalSource is not DependencyObject source || IsCommandControl(source) || !e.GetCurrentPoint(CommandBar).Properties.IsLeftButtonPressed || !Native.GetCursorPos(out var point))
        {
            return;
        }
        CommandBar.CapturePointer(e.Pointer);
        _barDrag = true;
        _dragCursorX = point.X;
        _dragCursorY = point.Y;
        var pos = _chrome.BarPosition;
        _dragWinX = pos.X;
        _dragWinY = pos.Y;
        e.Handled = true;
    }

    private void OnBarMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_barDrag || _chrome == null || !e.GetCurrentPoint(CommandBar).Properties.IsLeftButtonPressed || !Native.GetCursorPos(out var point))
        {
            return;
        }
        var x = _dragWinX + (point.X - _dragCursorX);
        var y = _dragWinY + (point.Y - _dragCursorY);
        var scale = CommandBar.XamlRoot?.RasterizationScale ?? 1;
        var width = Math.Max(1, (int)Math.Round(CommandBar.ActualWidth * scale));
        var height = Math.Max(1, (int)Math.Round((CommandBar.ActualHeight > 1 ? CommandBar.ActualHeight : 100) * scale));
        _chrome.PlaceBar(x, y, width, height);
        RememberBarOffset(x, y);
        e.Handled = true;
    }

    private void OnBarReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_barDrag)
        {
            return;
        }
        _barDrag = false;
        CommandBar.ReleasePointerCaptures();
        SaveShell();
        e.Handled = true;
    }

    private void SyncHistory()
    {
        var text = History.Text ?? "";
        var lines = new List<string>();
        foreach (var part in text.Split('\n'))
        {
            var line = part.TrimEnd('\r');
            if (line.Length > 0)
            {
                lines.Add(line);
            }
        }
        Recent0.Text = lines.Count >= 2 ? lines[^2] : "";
        Recent1.Text = lines.Count >= 1 ? lines[^1] : "";
        if (text == _historyShown)
        {
            return;
        }
        _historyShown = text;
        HistoryList.Items.Clear();
        foreach (var line in lines)
        {
            HistoryList.Items.Add(line);
        }
        if (HistoryList.Items.Count > 0)
        {
            HistoryList.ScrollIntoView(HistoryList.Items[HistoryList.Items.Count - 1]);
        }
    }

    private void UpdateDyn()
    {
        if (_chrome == null || _host == IntPtr.Zero || _start || !_showCommand)
        {
            _chrome?.ShowDyn(false, 0, 0, 0, 0, "", "", "", "", 0);
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadText(_host, Native.TextChrome));
            var root = doc.RootElement;
            if (!root.TryGetProperty("dyn", out var dyn) || !dyn.TryGetProperty("on", out var on) || on.ValueKind != JsonValueKind.True)
            {
                if (_dynAccess.Length > 0)
                {
                    _dynAccess = "";
                    AnnouncePrompt(PromptLabel.Text ?? "Command:", "");
                }
                _chrome.ShowDyn(false, 0, 0, 0, 0, "", "", "", "", 0);
                return;
            }
            if (!TryCanvas(out var origin, out var canvasW, out var canvasH, out var scale))
            {
                return;
            }
            var px = dyn.TryGetProperty("x", out var xNode) && xNode.TryGetDouble(out var x) ? x : 0;
            var py = dyn.TryGetProperty("y", out var yNode) && yNode.TryGetDouble(out var y) ? y : 0;
            var aLabel = dyn.TryGetProperty("aLabel", out var aLabelNode) ? aLabelNode.GetString() ?? "" : "";
            var a = dyn.TryGetProperty("a", out var aNode) ? aNode.GetString() ?? "" : "";
            var bLabel = dyn.TryGetProperty("bLabel", out var bLabelNode) ? bLabelNode.GetString() ?? "" : "";
            var b = dyn.TryGetProperty("b", out var bNode) ? bNode.GetString() ?? "" : "";
            var active = dyn.TryGetProperty("active", out var activeNode) && activeNode.TryGetInt32(out var activeValue) ? activeValue : 0;
            var parts = new List<string>();
            if (aLabel.Length > 0)
            {
                parts.Add((active == 0 ? "active " : "") + aLabel + " " + a);
            }
            if (bLabel.Length > 0)
            {
                parts.Add((active == 1 ? "active " : "") + bLabel + " " + b);
            }
            var dynAccess = string.Join(", ", parts);
            if (dynAccess != _dynAccess)
            {
                _dynAccess = dynAccess;
                AnnouncePrompt(PromptLabel.Text ?? "Command:", _dynAccess);
            }
            var dipW = bLabel.Length > 0 ? 300.0 : 150.0;
            var dipH = 88.0;
            var left = origin.X + px / scale + 18;
            var top = origin.Y + py / scale + 8;
            if (left + dipW > origin.X + canvasW - 8)
            {
                left = origin.X + px / scale - dipW - 10;
            }
            if (top + dipH > origin.Y + canvasH - 8)
            {
                top = origin.Y + py / scale - dipH - 10;
            }
            left = Math.Max(origin.X + 4, left);
            top = Math.Max(origin.Y + 4, top);
            if (!ClientToScreen(left, top, out var sx, out var sy))
            {
                return;
            }
            _chrome.ShowDyn(true, sx, sy, (int)Math.Round(dipW * scale), (int)Math.Round(dipH * scale), aLabel, a, bLabel, b, active);
        }
        catch (JsonException)
        {
            _chrome.ShowDyn(false, 0, 0, 0, 0, "", "", "", "", 0);
        }
    }

    private void PlaceCommandBar()
    {
        if (_barDrag || _chrome == null)
        {
            return;
        }
        var chrome = _chrome;
        if (chrome == null)
        {
            return;
        }
        if (!_showCommand || _start || !TryCanvas(out var origin, out var canvasW, out var canvasH, out var scale))
        {
            chrome.ShowBar(false);
            chrome.ShowDyn(false, 0, 0, 0, 0, "", "", "", "", 0);
            return;
        }
        var available = Math.Max(0, canvasW - 48);
        var barW = Math.Min(600, available);
        if (double.IsNaN(CommandBar.Width) || Math.Abs(CommandBar.Width - barW) > 0.5)
        {
            CommandBar.Width = barW;
        }
        var barH = _historyOpen ? 348.0 : 100.0;
        if (double.IsNaN(CommandBar.Height) || Math.Abs(CommandBar.Height - barH) > 0.5)
        {
            CommandBar.Height = barH;
        }
        var left = origin.X + Math.Max(0, (canvasW - barW) / 2) + _barDx;
        var top = origin.Y + Math.Max(0, (canvasH - barH) / 2) + _barDy;
        left = Math.Clamp(left, origin.X + 8, Math.Max(origin.X + 8, origin.X + canvasW - barW - 8));
        top = Math.Clamp(top, origin.Y + 8, Math.Max(origin.Y + 8, origin.Y + canvasH - barH - 8));
        if (!ClientToScreen(left, top, out var sx, out var sy))
        {
            return;
        }
        chrome.ShowBar(true);
        chrome.PlaceBar(sx, sy, Math.Max(1, (int)Math.Round(barW * scale)), Math.Max(1, (int)Math.Round(barH * scale)));
    }

    private void RememberBarOffset(int screenX, int screenY)
    {
        if (!TryCanvas(out var origin, out var canvasW, out var canvasH, out var scale))
        {
            return;
        }
        var barW = CommandBar.ActualWidth > 1 ? CommandBar.ActualWidth : 600;
        var barH = CommandBar.ActualHeight > 1 ? CommandBar.ActualHeight : 100;
        if (!ClientToScreen(origin.X + Math.Max(0, (canvasW - barW) / 2), origin.Y + Math.Max(0, (canvasH - barH) / 2), out var anchorX, out var anchorY))
        {
            return;
        }
        _barDx = (screenX - anchorX) / scale;
        _barDy = (screenY - anchorY) / scale;
    }

    private bool TryCanvas(out Windows.Foundation.Point origin, out double canvasW, out double canvasH, out double scale)
    {
        origin = default;
        canvasW = 0;
        canvasH = 0;
        scale = 1;
        if (Content is not FrameworkElement root || CanvasHost.XamlRoot == null)
        {
            return false;
        }
        scale = CanvasHost.XamlRoot.RasterizationScale;
        if (scale <= 0 || CanvasHost.ActualWidth < 1 || CanvasHost.ActualHeight < 1)
        {
            return false;
        }
        origin = CanvasHost.TransformToVisual(root).TransformPoint(new Windows.Foundation.Point(0, 0));
        canvasW = CanvasHost.ActualWidth;
        canvasH = CanvasHost.ActualHeight;
        return true;
    }

    private bool ClientToScreen(double dipX, double dipY, out int x, out int y)
    {
        x = 0;
        y = 0;
        if (Content is not FrameworkElement root || root.XamlRoot == null)
        {
            return false;
        }
        var scale = root.XamlRoot.RasterizationScale;
        var point = new Native.PointI { X = (int)Math.Round(dipX * scale), Y = (int)Math.Round(dipY * scale) };
        if (!Native.ClientToScreen(WindowNative.GetWindowHandle(this), ref point))
        {
            return false;
        }
        x = point.X;
        y = point.Y;
        return true;
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
        public bool LiteralSpace { get; set; }
        public string Units { get; set; } = "";
        public uint Osmode { get; set; }
        public double Polarang { get; set; } = 90;
        public bool DynpiCartesian { get; set; }
        public bool DynpiAbsolute { get; set; }
        public string Cursor { get; set; } = "";
        public string Assistant { get; set; } = "idle";
        public string AssistantPrompt { get; set; } = "";
        public string AssistantPartial { get; set; } = "";
        public string AssistantModel { get; set; } = "";
        public int ControlPort { get; set; }
        public bool Quit { get; set; }
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
        public List<string> Handles { get; set; } = new();
        public List<string> Attributes { get; set; } = new();
    }
}

/// <summary>Side-column edge. Focusable so Left/Right can change the width.</summary>
public sealed class ResizeGrip : Control
{
    public ResizeGrip()
    {
        IsTabStop = true;
        UseSystemFocusVisuals = true;
        ProtectedCursor = InputSystemCursor.Create(InputSystemCursorShape.SizeWestEast);
        // Without a template, Control.Background never paints and the grip does not hit-test.
        Template = (ControlTemplate)Microsoft.UI.Xaml.Markup.XamlReader.Load(
            """
            <ControlTemplate xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
              <Border Background="{TemplateBinding Background}" />
            </ControlTemplate>
            """);
    }
}
