using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>
/// The same countdown as <c>crates/engine/src/pomodoro.rs</c>: 5 to 60 minutes, steps of 5, 25 by default.
/// A tick is one second from the window timer. A running block ignores length changes.
/// </summary>
internal sealed class PomodoroWindow : Window
{
    private const uint DefaultMinutes = 25;
    private const uint MinMinutes = 5;
    private const uint MaxMinutes = 60;
    private const uint StepMinutes = 5;
    private uint _duration = DefaultMinutes * 60;
    private uint _left = DefaultMinutes * 60;
    private bool _running;
    private readonly TextBlock _label = new() { Text = "25:00", FontSize = 28 };
    private readonly DispatcherTimer _timer = new() { Interval = TimeSpan.FromSeconds(1) };

    private PomodoroWindow(Window owner)
    {
        Title = "Pomodoro";
        AutomationProperties.SetLiveSetting(_label, AutomationLiveSetting.Polite);
        Density.Name(_label, "Remaining time");
        var toggle = Action("Start");
        toggle.Click += (_, _) =>
        {
            Toggle();
            toggle.Content = _running ? "Pause" : "Start";
            Density.Name(toggle, _running ? "Pause" : "Start");
            Show();
        };
        var reset = Action("Reset");
        reset.Click += (_, _) =>
        {
            Reset();
            toggle.Content = "Start";
            Density.Name(toggle, "Start");
            Show();
        };
        var shorter = Action("−5");
        Density.Name(shorter, "Shorten by five minutes");
        shorter.Click += (_, _) => SetMinutes(Minutes() - StepMinutes);
        var longer = Action("+5");
        Density.Name(longer, "Lengthen by five minutes");
        longer.Click += (_, _) => SetMinutes(Minutes() + StepMinutes);
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = Density.SpaceSm,
            Children = { toggle, reset, shorter, longer },
        };
        var root = new StackPanel { Spacing = Density.SpaceMd, Margin = Density.DialogMargin, Children = { _label, row } };
        var theme = ShellChrome.ThemeOf(owner);
        root.RequestedTheme = theme;
        Content = root;
        Density.Apply(root);
        _label.FontSize = 22;
        ShellIcon.Apply(this, theme);
        ShellFit.ToContent(this, minWidthDip: 260, minHeightDip: 120, maxWidthDip: 360, maxHeightDip: 220);
        _timer.Tick += (_, _) =>
        {
            Tick(1);
            Show();
            if (!_running)
            {
                toggle.Content = "Start";
                Density.Name(toggle, "Start");
            }
        };
        Closed += (_, _) => _timer.Stop();
        Show();
    }

    private static Button Action(string label)
    {
        var button = new Button { Content = label, MinHeight = Density.ControlHeight, MinWidth = 48 };
        Density.Name(button, label);
        return button;
    }

    public static PomodoroWindow Show(Window owner)
    {
        var window = new PomodoroWindow(owner);
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    private void Toggle()
    {
        if (_left == 0)
        {
            _left = _duration;
            _running = true;
        }
        else
        {
            _running = !_running;
        }
        if (_running)
        {
            _timer.Start();
        }
        else
        {
            _timer.Stop();
        }
    }

    private void Reset()
    {
        _running = false;
        _left = _duration;
        _timer.Stop();
    }

    private void Tick(uint seconds)
    {
        if (!_running)
        {
            return;
        }
        if (_left <= seconds)
        {
            _left = 0;
            _running = false;
            _timer.Stop();
            return;
        }
        _left -= seconds;
    }

    private uint Minutes() => _duration / 60;

    private void SetMinutes(uint minutes)
    {
        var snapped = Snap(minutes);
        if (!_running)
        {
            _duration = snapped * 60;
            _left = _duration;
            Show();
        }
    }

    private static uint Snap(uint minutes)
    {
        var steps = Math.Round(minutes / (double)StepMinutes);
        if (!double.IsFinite(steps))
        {
            return DefaultMinutes;
        }
        var value = steps * StepMinutes;
        if (value < MinMinutes)
        {
            return MinMinutes;
        }
        if (value > MaxMinutes)
        {
            return MaxMinutes;
        }
        return (uint)value;
    }

    private void Show()
    {
        _label.Text = $"{_left / 60:00}:{_left % 60:00}";
        Density.Name(_label, "Remaining time " + _label.Text);
    }
}
