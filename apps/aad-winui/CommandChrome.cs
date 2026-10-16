using Microsoft.UI.Composition;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>
/// The floating command bar and the cursor fields. Both are top-level windows so the drawing
/// child does not cover them. Geometry and command text stay in the host.
/// </summary>
internal sealed class CommandChrome
{
    private readonly Window _bar = new() { Title = "Command" };
    private readonly Window _dyn = new() { Title = "Dynamic input" };
    private readonly Border _dynA;
    private readonly Border _dynB;
    private readonly TextBlock _dynALabel;
    private readonly TextBlock _dynAValue;
    private readonly TextBlock _dynBLabel;
    private readonly TextBlock _dynBValue;
    private readonly StackPanel _dynBGroup;
    private readonly StackPanel _dynRoot;
    private bool _barReady;
    private bool _dynReady;
    private bool _dynClickThrough;
    private string _shownA = "";
    private string _shownB = "";
    private string _shownALabel = "";
    private string _shownBLabel = "";
    private int _shownActive = -1;

    public CommandChrome()
    {
        Configure(_bar);
        Configure(_dyn);
        _dynALabel = Label();
        _dynAValue = Value();
        _dynBLabel = Label();
        _dynBValue = Value();
        AutomationProperties.SetName(_dynALabel, "Dynamic input label A");
        AutomationProperties.SetName(_dynBLabel, "Dynamic input label B");
        AutomationProperties.SetName(_dynAValue, "Dynamic input value A");
        AutomationProperties.SetName(_dynBValue, "Dynamic input value B");
        _dynA = Field(_dynAValue);
        _dynB = Field(_dynBValue);
        _dynBGroup = new StackPanel { Spacing = 1, Children = { _dynBLabel, _dynB } };
        var groupA = new StackPanel { Spacing = 1, Children = { _dynALabel, _dynA } };
        _dynRoot = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            Padding = new Thickness(8),
            Children = { groupA, _dynBGroup },
        };
        _dyn.Content = _dynRoot;
        ApplyTheme(ElementTheme.Dark);
    }

    public Windows.Graphics.PointInt32 BarPosition => _bar.AppWindow.Position;

    public void Attach(FrameworkElement body, IntPtr owner)
    {
        body.Visibility = Visibility.Visible;
        _bar.Content = body;
        Own(_bar, owner, ref _barReady);
        Own(_dyn, owner, ref _dynReady);
        PassClicks(_dyn);
    }

    public void ApplyTheme(ElementTheme theme)
    {
        var dark = theme != ElementTheme.Light;
        if (_bar.Content is FrameworkElement body)
        {
            body.RequestedTheme = theme;
            if (body is Border card)
            {
                card.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
                card.BorderThickness = new Thickness(0);
            }
        }
        try
        {
            _bar.SystemBackdrop = new HalfAcrylic(dark);
        }
        catch (Exception)
        {
            if (_bar.Content is Border card)
            {
                card.Background = new SolidColorBrush(dark ? Color(128, 24, 24, 24) : Color(128, 255, 255, 255));
            }
        }
        Native.Frame(WindowNative.GetWindowHandle(_bar), dark ? 0x00242424 : 0x00FFFFFF, dark);
        _dynRoot.RequestedTheme = theme;
        var fill = dark ? Color(255, 45, 45, 45) : Color(255, 249, 249, 249);
        var edge = dark ? Color(255, 92, 92, 92) : Color(255, 229, 229, 229);
        var ink = dark ? Microsoft.UI.Colors.White : Color(255, 26, 26, 26);
        var secondary = dark ? Color(255, 197, 197, 197) : Color(255, 92, 92, 92);
        _dynRoot.Background = new SolidColorBrush(fill);
        foreach (var box in new[] { _dynA, _dynB })
        {
            box.Background = new SolidColorBrush(fill);
            box.BorderBrush = new SolidColorBrush(edge);
            box.CornerRadius = new CornerRadius(4);
        }
        foreach (var block in new[] { _dynALabel, _dynBLabel })
        {
            block.Foreground = new SolidColorBrush(secondary);
        }
        foreach (var block in new[] { _dynAValue, _dynBValue })
        {
            block.Foreground = new SolidColorBrush(ink);
        }
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
            Native.Frame(WindowNative.GetWindowHandle(_bar), dark ? 0x00242424 : 0x00FFFFFF, dark);
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

    public void ShowDyn(bool on, int x, int y, int width, int height, string aLabel, string a, string bLabel, string b, int active)
    {
        if (!on)
        {
            if (_dyn.AppWindow.IsVisible)
            {
                _dyn.AppWindow.Hide();
            }
            return;
        }
        if (_shownALabel != aLabel)
        {
            _dynALabel.Text = aLabel;
            _shownALabel = aLabel;
        }
        if (_shownA != a)
        {
            _dynAValue.Text = a;
            _shownA = a;
        }
        var second = bLabel.Length > 0;
        _dynBGroup.Visibility = second ? Visibility.Visible : Visibility.Collapsed;
        if (_shownBLabel != bLabel)
        {
            _dynBLabel.Text = bLabel;
            _shownBLabel = bLabel;
        }
        if (_shownB != b)
        {
            _dynBValue.Text = b;
            _shownB = b;
        }
        if (_shownActive != active)
        {
            _shownActive = active;
            _dynA.BorderThickness = new Thickness(active == 0 ? 2 : 1);
            _dynB.BorderThickness = new Thickness(active == 1 ? 2 : 1);
            _dynAValue.FontWeight = active == 0 ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal;
            _dynBValue.FontWeight = active == 1 ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal;
        }
        AutomationProperties.SetName(_dynAValue, (active == 0 ? "Active " : "") + (aLabel.Length == 0 ? "Value" : aLabel) + " " + a);
        AutomationProperties.SetName(_dynBValue, (active == 1 ? "Active " : "") + (bLabel.Length == 0 ? "Value" : bLabel) + " " + b);
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

    public void Close()
    {
        try
        {
            _bar.Close();
        }
        catch (Exception)
        {
            // The shell is already closing.
        }
        try
        {
            _dyn.Close();
        }
        catch (Exception)
        {
            // The shell is already closing.
        }
    }

    private void PassClicks(Window window)
    {
        if (_dynClickThrough)
        {
            return;
        }
        var hwnd = WindowNative.GetWindowHandle(window);
        var extra = Native.GetWindowLongPtr(hwnd, Native.GwlExStyle).ToInt64();
        extra &= ~Native.WsExTransparent;
        extra |= Native.WsExNoActivate;
        Native.SetWindowLongPtr(hwnd, Native.GwlExStyle, new IntPtr(extra));
        _dynClickThrough = true;
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

    private static TextBlock Label() => new()
    {
        FontSize = 12,
        IsHitTestVisible = false,
    };

    private static TextBlock Value() => new()
    {
        FontSize = 14,
        FontFamily = new FontFamily("Consolas"),
        IsHitTestVisible = false,
    };

    private static Border Field(TextBlock value) => new()
    {
        BorderThickness = new Thickness(1),
        CornerRadius = new CornerRadius(4),
        MinWidth = 96,
        Height = 32,
        Padding = new Thickness(8, 4, 8, 4),
        Child = value,
    };

    private static Windows.UI.Color Color(byte a, byte r, byte g, byte b) =>
        Windows.UI.Color.FromArgb(a, r, g, b);
}

/// <summary>Frosted fill at half opacity so the drawing stays visible.</summary>
internal sealed class HalfAcrylic : SystemBackdrop
{
    private readonly bool _dark;
    private DesktopAcrylicController? _acrylic;

    public HalfAcrylic(bool dark) => _dark = dark;

    protected override void OnTargetConnected(ICompositionSupportsSystemBackdrop target, XamlRoot root)
    {
        base.OnTargetConnected(target, root);
        _acrylic = new DesktopAcrylicController
        {
            TintColor = _dark ? Color(255, 24, 24, 24) : Color(255, 255, 255, 255),
            TintOpacity = 0.5f,
            LuminosityOpacity = 0.5f,
            FallbackColor = _dark ? Color(128, 24, 24, 24) : Color(128, 255, 255, 255),
        };
        var configuration = GetDefaultSystemBackdropConfiguration(target, root);
        _acrylic.AddSystemBackdropTarget(target);
        _acrylic.SetSystemBackdropConfiguration(configuration);
    }

    protected override void OnTargetDisconnected(ICompositionSupportsSystemBackdrop target)
    {
        base.OnTargetDisconnected(target);
        if (_acrylic == null)
        {
            return;
        }
        _acrylic.RemoveSystemBackdropTarget(target);
        _acrylic.Dispose();
        _acrylic = null;
    }

    private static Windows.UI.Color Color(byte a, byte r, byte g, byte b) =>
        Windows.UI.Color.FromArgb(a, r, g, b);
}
