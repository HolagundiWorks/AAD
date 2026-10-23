using System.Runtime.InteropServices;
using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Right-click menu in its own window, so the drawing child does not cover it.</summary>
internal static class CanvasMenu
{
    private static Window? _window;
    private static long _openedAt;

    public static bool IsOpen => _window != null;

    /// <summary>Close the menu if it is open. Returns whether it was open.</summary>
    public static bool Dismiss()
    {
        if (_window == null)
        {
            return false;
        }
        try
        {
            _window.Close();
        }
        catch (Exception)
        {
            _window = null;
        }
        return true;
    }

    /// <summary>Close on Escape or a click outside the menu (called from the shell timer).</summary>
    public static void DismissOutsideClick()
    {
        if (_window == null || Environment.TickCount64 - _openedAt < 250)
        {
            return;
        }
        if ((GetAsyncKeyState(0x1B) & 0x8000) != 0)
        {
            Dismiss();
            return;
        }
        var left = (GetAsyncKeyState(0x01) & 0x8000) != 0;
        var right = (GetAsyncKeyState(0x02) & 0x8000) != 0;
        if (!left && !right)
        {
            return;
        }
        if (!Native.GetCursorPos(out var point))
        {
            return;
        }
        var hwnd = WindowNative.GetWindowHandle(_window);
        if (!GetWindowRect(hwnd, out var rect))
        {
            return;
        }
        if (point.X < rect.Left || point.X >= rect.Right || point.Y < rect.Top || point.Y >= rect.Bottom)
        {
            Dismiss();
        }
    }

    public static void Show(MainWindow owner, string json, Action<string, string> choose)
    {
        using var doc = JsonDocument.Parse(json);
        var root = doc.RootElement;
        var x = root.TryGetProperty("x", out var px) ? px.GetInt32() : 0;
        var y = root.TryGetProperty("y", out var py) ? py.GetInt32() : 0;
        var stack = new StackPanel { Spacing = Density.SpaceXs, Margin = new Thickness(Density.SpaceSm) };
        if (root.TryGetProperty("groups", out var groups))
        {
            var first = true;
            foreach (var group in groups.EnumerateArray())
            {
                if (!first)
                {
                    stack.Children.Add(new Border
                    {
                        Height = 1,
                        Margin = new Thickness(0, 2, 0, 2),
                        Background = ThemeBrush("DividerStrokeColorDefaultBrush", Microsoft.UI.Colors.Gray),
                    });
                }
                first = false;
                foreach (var item in group.EnumerateArray())
                {
                    AddItem(stack, item, choose, 0);
                }
            }
        }
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var dark = theme != ElementTheme.Light;
        var body = new ScrollViewer
        {
            Content = stack,
            MaxHeight = 360,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            BorderThickness = new Thickness(0),
        };
        var frame = new Border
        {
            RequestedTheme = theme,
            BorderThickness = new Thickness(0),
            Background = ThemeBrush("SolidBackgroundFillColorBaseBrush", dark ? Microsoft.UI.Colors.Black : Microsoft.UI.Colors.White),
            Child = body,
        };
        frame.PreviewKeyDown += OnMenuKey;
        _window?.Close();
        var window = new Window { Title = "Menu", Content = frame };
        _window = window;
        _openedAt = Environment.TickCount64;
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsAlwaysOnTop = true;
            presenter.IsResizable = false;
            presenter.IsMinimizable = false;
            presenter.IsMaximizable = false;
        }
        window.Activated += (_, args) =>
        {
            if (args.WindowActivationState == WindowActivationState.Deactivated)
            {
                Dismiss();
            }
        };
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
        };
        Density.Apply(frame);
        ShellWindows.Track(window);
        ShellChrome.Apply(window, theme);
        ShellFit.ToContent(window, minWidthDip: 180, minHeightDip: 120, maxWidthDip: 320, maxHeightDip: 420);
        window.AppWindow.Move(new Windows.Graphics.PointInt32(x, y));
        window.Activate();
        var hwnd = WindowNative.GetWindowHandle(window);
        var ownerHwnd = WindowNative.GetWindowHandle(owner);
        if (ownerHwnd != IntPtr.Zero)
        {
            Native.SetWindowLongPtr(hwnd, Native.GwlpHwndParent, ownerHwnd);
        }
        Native.ShowWindow(hwnd, 8);
        Native.Frame(hwnd, Native.DwmColorNone, dark);
        frame.Focus(FocusState.Programmatic);
    }

    private static void OnMenuKey(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != Windows.System.VirtualKey.Escape)
        {
            return;
        }
        e.Handled = true;
        Dismiss();
    }

    private static void AddItem(StackPanel stack, JsonElement item, Action<string, string> choose, int indent)
    {
        if (item.TryGetProperty("items", out var children))
        {
            stack.Children.Add(new TextBlock
            {
                Text = item.TryGetProperty("label", out var title) ? title.GetString() : "",
                Margin = new Thickness(8 + indent, 4, 8, 0),
                Foreground = ThemeBrush("TextFillColorSecondaryBrush", Microsoft.UI.Colors.Gray),
            });
            foreach (var child in children.EnumerateArray())
            {
                AddItem(stack, child, choose, indent + 12);
            }
            return;
        }
        var label = item.TryGetProperty("label", out var text) ? text.GetString() ?? "" : "";
        var id = item.TryGetProperty("id", out var ident) ? ident.GetString() ?? "" : "";
        var run = item.TryGetProperty("run", out var mode) ? mode.GetString() ?? "command" : "command";
        var enabled = !item.TryGetProperty("enabled", out var flag) || flag.ValueKind != JsonValueKind.False;
        var button = new Button
        {
            Content = label,
            IsEnabled = enabled && id.Length > 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Margin = new Thickness(indent, 0, 0, 0),
            MinHeight = Density.ControlHeight,
            Padding = new Thickness(12, 4, 12, 4),
            FontSize = Density.Font,
            BorderThickness = new Thickness(0),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            CornerRadius = new CornerRadius(4),
        };
        Density.Name(button, label);
        button.Click += (_, _) =>
        {
            Dismiss();
            choose(id, run);
        };
        stack.Children.Add(button);
    }

    private static Brush ThemeBrush(string key, Windows.UI.Color fallback) =>
        Density.ThemeBrush(key) is { } brush
            ? brush
            : new SolidColorBrush(fallback);

    [DllImport("user32.dll")]
    private static extern short GetAsyncKeyState(int vKey);

    [DllImport("user32.dll")]
    private static extern bool GetWindowRect(IntPtr hWnd, out RectI rect);

    [StructLayout(LayoutKind.Sequential)]
    private struct RectI
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }
}
