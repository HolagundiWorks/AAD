using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Windows.Foundation;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>
/// Fixed AI bar and cursor-anchored CAD prompt. Both are top-level windows so the drawing
/// child does not cover them. Geometry and command text stay in the host.
/// </summary>
internal sealed class CommandChrome
{
    private readonly Window _bar = new() { Title = "Ask" };
    private readonly Window _dyn = new() { Title = "CAD input" };
    private readonly Border _cadCard;
    private readonly StackPanel _dynRoot;
    private bool _barReady;
    private bool _dynReady;

    public TextBox CadInput { get; }
    public ListView SuggestList { get; }
    public TextBlock CadPrompt { get; }

    public CommandChrome()
    {
        Configure(_bar);
        Configure(_dyn);
        // Cursor CAD: typed text only — no prompt, units hint, or Length/Angle labels.
        CadPrompt = new TextBlock
        {
            Visibility = Visibility.Collapsed,
            Text = "",
            IsHitTestVisible = false,
        };
        AutomationProperties.SetName(CadPrompt, "CAD prompt");
        CadInput = new TextBox
        {
            FontFamily = ShellFonts.MonoFamily,
            FontSize = Density.FontCaption,
            MinHeight = 22,
            MinWidth = 96,
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Padding = new Thickness(0, 2, 4, 2),
            VerticalAlignment = VerticalAlignment.Center,
            PlaceholderText = "",
        };
        CadInput.Resources["TextControlBorderThemeThickness"] = new Thickness(0);
        CadInput.Resources["TextControlBorderThemeThicknessFocused"] = new Thickness(0);
        AutomationProperties.SetName(CadInput, "CAD command line");
        SuggestList = new ListView
        {
            MaxHeight = 72,
            Visibility = Visibility.Collapsed,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            FontFamily = ShellFonts.MonoFamily,
            FontSize = Density.FontCaption,
            SelectionMode = ListViewSelectionMode.Single,
            IsItemClickEnabled = true,
        };
        AutomationProperties.SetName(SuggestList, "Command suggestions");
        SuggestList.ItemContainerStyle = SuggestItemStyle();
        var cadColumn = new StackPanel
        {
            Spacing = 1,
            Children = { CadInput, SuggestList },
        };
        _cadCard = new Border
        {
            Padding = new Thickness(4, 2, 4, 2),
            CornerRadius = new CornerRadius(3),
            BorderThickness = new Thickness(0),
            Child = cadColumn,
        };
        _dynRoot = new StackPanel
        {
            Orientation = Orientation.Vertical,
            Spacing = Density.SpaceXs,
            Padding = new Thickness(Density.SpaceSm),
            Children = { _cadCard },
        };
        _dyn.Content = _dynRoot;
        ApplyTheme(ElementTheme.Dark);
    }

    public Windows.Graphics.PointInt32 BarPosition => _bar.AppWindow.Position;

    public void Attach(FrameworkElement body, IntPtr owner)
    {
        body.Visibility = Visibility.Visible;
        if (body is Border border)
        {
            // AI channel: solid chrome with a thin accent edge (distinct from CAD cursor field).
            border.BorderThickness = new Thickness(0, 2, 0, 0);
            border.CornerRadius = new CornerRadius(Density.CornerControl);
            border.BorderBrush = Density.ThemeBrush("CommandChromeAccentBrush")
                ?? new SolidColorBrush(Windows.UI.Color.FromArgb(255, 0x60, 0xCD, 0xFF));
        }
        _bar.Content = body;
        Own(_bar, owner, ref _barReady);
        Own(_dyn, owner, ref _dynReady);
        AllowActivate(_dyn);
    }

    public void WireCad(
        KeyEventHandler previewKeyDown,
        TextChangedEventHandler textChanged,
        TypedEventHandler<UIElement, CharacterReceivedRoutedEventArgs> characterReceived,
        ItemClickEventHandler suggestClick)
    {
        CadInput.PreviewKeyDown += previewKeyDown;
        CadInput.TextChanged += textChanged;
        CadInput.CharacterReceived += characterReceived;
        SuggestList.ItemClick += suggestClick;
    }

    public void FocusCad() => CadInput.Focus(FocusState.Programmatic);

    public void ApplyTheme(ElementTheme theme)
    {
        var dark = theme != ElementTheme.Light;
        Density.Theme = theme == ElementTheme.Default ? Density.Theme : theme;
        _bar.SystemBackdrop = null;
        _dyn.SystemBackdrop = null;
        if (_bar.Content is FrameworkElement body)
        {
            body.RequestedTheme = theme;
        }
        if (_bar.Content is Border barCard)
        {
            barCard.Background = Density.ThemeBrush("CommandChromeBrush")
                ?? new SolidColorBrush(dark ? Color(242, 31, 31, 31) : Color(250, 255, 255, 255));
            barCard.BorderThickness = new Thickness(0, 2, 0, 0);
            barCard.CornerRadius = new CornerRadius(Density.CornerControl);
            barCard.BorderBrush = Density.ThemeBrush("CommandChromeAccentBrush")
                ?? new SolidColorBrush(dark ? Color(255, 0x60, 0xCD, 0xFF) : Color(255, 0x00, 0x78, 0xD4));
        }
        Native.Frame(WindowNative.GetWindowHandle(_bar), Native.DwmColorNone, dark);
        Native.Frame(WindowNative.GetWindowHandle(_dyn), Native.DwmColorNone, dark);
        _dynRoot.RequestedTheme = theme;
        var fill = Density.ThemeBrush("CommandChromeCadBrush")
            ?? new SolidColorBrush(dark ? Color(242, 40, 40, 40) : Color(250, 247, 247, 247));
        var ink = Density.ThemeBrush("TextPrimaryBrush")
            ?? new SolidColorBrush(dark ? Microsoft.UI.Colors.White : Color(255, 26, 26, 26));
        var cadEdge = Density.ThemeBrush("CadChannelBrush")
            ?? new SolidColorBrush(dark ? Color(255, 0x6C, 0xCB, 0x8A) : Color(255, 0x0F, 0x7B, 0x3A));
        _dynRoot.Background = fill;
        _cadCard.Background = fill;
        _cadCard.BorderThickness = new Thickness(2, 0, 0, 0);
        _cadCard.BorderBrush = cadEdge;
        _cadCard.CornerRadius = new CornerRadius(Density.CornerChrome);
        CadInput.Foreground = ink;
        CadInput.FontFamily = ShellFonts.MonoFamily;
        CadInput.PlaceholderText = "";
    }

    public void ShowBar(bool on)
    {
        if (on)
        {
            if (!_bar.AppWindow.IsVisible)
            {
                _bar.AppWindow.Show();
                Native.ShowWindow(WindowNative.GetWindowHandle(_bar), 8);
            }
            var dark = _bar.Content is FrameworkElement body && body.RequestedTheme != ElementTheme.Light;
            Native.Frame(WindowNative.GetWindowHandle(_bar), Native.DwmColorNone, dark);
            return;
        }
        if (_bar.AppWindow.IsVisible)
        {
            _bar.AppWindow.Hide();
        }
    }

    public void PlaceBar(int x, int y, int width, int height)
    {
        if (width < 1 || height < 1)
        {
            return;
        }
        var size = _bar.AppWindow.Size;
        if (size.Width != width || size.Height != height)
        {
            _bar.AppWindow.Resize(new Windows.Graphics.SizeInt32(width, height));
        }
        var pos = _bar.AppWindow.Position;
        if (pos.X != x || pos.Y != y)
        {
            _bar.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        }
    }

    /// <summary>Cursor-anchored CAD field. Prompt/dims args are ignored (typed text only).</summary>
    public void ShowCad(
        bool on,
        int x,
        int y,
        int width,
        int height,
        string prompt,
        bool dims,
        string aLabel,
        string a,
        string bLabel,
        string b,
        int active)
    {
        _ = (prompt, dims, aLabel, a, bLabel, b, active);
        if (!on)
        {
            if (_dyn.AppWindow.IsVisible)
            {
                _dyn.AppWindow.Hide();
            }
            return;
        }
        CadPrompt.Visibility = Visibility.Collapsed;
        CadInput.PlaceholderText = "";
        var pixelsWide = Math.Max(1, width);
        var pixelsHigh = Math.Max(1, height);
        var size = _dyn.AppWindow.Size;
        if (size.Width != pixelsWide || size.Height != pixelsHigh)
        {
            _dyn.AppWindow.Resize(new Windows.Graphics.SizeInt32(pixelsWide, pixelsHigh));
        }
        var pos = _dyn.AppWindow.Position;
        if (pos.X != x || pos.Y != y)
        {
            _dyn.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        }
        if (!_dyn.AppWindow.IsVisible)
        {
            _dyn.AppWindow.Show();
            var hwnd = WindowNative.GetWindowHandle(_dyn);
            Native.ShowWindow(hwnd, 8);
            Native.Lift(hwnd);
        }
    }

    public void ShowDyn(bool on, int x, int y, int width, int height, string aLabel, string a, string bLabel, string b, int active) =>
        ShowCad(on, x, y, width, height, "", false, aLabel, a, bLabel, b, active);

    public void Close()
    {
        try
        {
            _bar.Close();
        }
        catch (Exception)
        {
        }
        try
        {
            _dyn.Close();
        }
        catch (Exception)
        {
        }
    }

    private static void AllowActivate(Window window)
    {
        var hwnd = WindowNative.GetWindowHandle(window);
        var extra = Native.GetWindowLongPtr(hwnd, Native.GwlExStyle).ToInt64();
        extra &= ~Native.WsExTransparent;
        extra &= ~Native.WsExNoActivate;
        Native.SetWindowLongPtr(hwnd, Native.GwlExStyle, new IntPtr(extra));
    }

    private static void Own(Window window, IntPtr owner, ref bool ready)
    {
        if (ready || owner == IntPtr.Zero)
        {
            return;
        }
        Native.SetWindowLongPtr(WindowNative.GetWindowHandle(window), Native.GwlpHwndParent, owner);
        ready = true;
    }

    private static void Configure(Window window)
    {
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsResizable = false;
            presenter.IsMinimizable = false;
            presenter.IsMaximizable = false;
        }
        Native.RoundWindow(WindowNative.GetWindowHandle(window));
    }

    private static Style SuggestItemStyle()
    {
        var style = new Style(typeof(ListViewItem));
        style.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, 20.0));
        style.Setters.Add(new Setter(Control.PaddingProperty, new Thickness(4, 1, 4, 1)));
        style.Setters.Add(new Setter(Control.FontFamilyProperty, ShellFonts.MonoFamily));
        style.Setters.Add(new Setter(Control.FontSizeProperty, 10.0));
        style.Setters.Add(new Setter(Control.HorizontalContentAlignmentProperty, HorizontalAlignment.Left));
        return style;
    }

    private static Windows.UI.Color Color(byte a, byte r, byte g, byte b) =>
        Windows.UI.Color.FromArgb(a, r, g, b);
}
