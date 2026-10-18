using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Aad.WinUi;

/// <summary>Format &gt; Point Style… — PDMODE / PDSIZE via <c>ddptype</c>.</summary>
internal static class PointStyleWindow
{
    private static readonly long[] Modes = [0, 1, 2, 3, 4, 32, 33, 34, 35, 36, 64, 65, 66, 67, 68, 96, 97, 98, 99, 100];

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
        Density.Name(status, "Point style status");

        var grid = new Grid { ColumnSpacing = 6, RowSpacing = 6 };
        for (var c = 0; c < 5; c++)
        {
            grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        }
        for (var r = 0; r < 4; r++)
        {
            grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        }

        var size = new TextBox { MinHeight = Density.MinTarget, Width = 80 };
        Density.Name(size, "Point size");
        var relative = new RadioButton { Content = "Set Size Relative to Screen", MinHeight = Density.MinTarget, GroupName = "pdsize" };
        var absolute = new RadioButton { Content = "Set Size in Absolute Units", MinHeight = Density.MinTarget, GroupName = "pdsize" };
        Density.Name(relative, "Relative point size");
        Density.Name(absolute, "Absolute point size");
        var unitLabel = new TextBlock { Text = "%", VerticalAlignment = VerticalAlignment.Center };

        var root = new StackPanel { Spacing = 8, Margin = new Thickness(12), MaxWidth = 360 };
        root.Children.Add(Heading("Point Style"));
        root.Children.Add(grid);
        var sizeRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        sizeRow.Children.Add(new TextBlock { Text = "Point Size", VerticalAlignment = VerticalAlignment.Center });
        sizeRow.Children.Add(size);
        sizeRow.Children.Add(unitLabel);
        root.Children.Add(sizeRow);
        root.Children.Add(relative);
        root.Children.Add(absolute);
        root.Children.Add(status);

        var state = new State();
        var loading = false;
        var modeButtons = new Dictionary<long, Button>();

        void Fail(string message) => status.Text = message;

        bool Run(long mode, double sz, bool rel)
        {
            if (Native.Execute(host, "ddptype", JsonSerializer.Serialize(new
            {
                pdmode = mode,
                pdsize = rel ? -sz : sz,
            }, JsonOpts)) != 0)
            {
                Fail(Native.LastError(host));
                return false;
            }
            status.Text = "";
            state.Mode = mode;
            state.Size = sz;
            state.Relative = rel;
            Highlight();
            unitLabel.Text = rel ? "%" : "units";
            changed();
            return true;
        }

        void Highlight()
        {
            foreach (var (mode, button) in modeButtons)
            {
                button.Background = mode == state.Mode
                    ? Density.ThemeBrush("AccentFillColorDefaultBrush")!
                    : Density.ThemeBrush("ControlFillColorDefaultBrush")!;
            }
        }

        double GetNum(string name, double fallback)
        {
            if (Native.Execute(host, "getvar", JsonSerializer.Serialize(new { name }, JsonOpts)) != 0)
            {
                return fallback;
            }
            try
            {
                using var doc = JsonDocument.Parse(Native.ReadResult(host));
                return doc.RootElement.TryGetProperty("value", out var v) && v.TryGetDouble(out var n) ? n : fallback;
            }
            catch (JsonException)
            {
                return fallback;
            }
        }

        void Load()
        {
            loading = true;
            state.Mode = (long)GetNum("PDMODE", 0);
            var pdsize = GetNum("PDSIZE", 0);
            state.Relative = pdsize <= 0;
            state.Size = pdsize == 0 ? 5 : Math.Abs(pdsize);
            size.Text = state.Size.ToString("0.####", CultureInfo.InvariantCulture);
            relative.IsChecked = state.Relative;
            absolute.IsChecked = !state.Relative;
            unitLabel.Text = state.Relative ? "%" : "units";
            Highlight();
            loading = false;
        }

        void ApplyFromUi(long? mode = null)
        {
            if (loading)
            {
                return;
            }
            if (!double.TryParse((size.Text ?? "").Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out var sz)
                && !double.TryParse((size.Text ?? "").Trim(), NumberStyles.Float, CultureInfo.CurrentCulture, out sz))
            {
                Fail("Point size must be a positive number.");
                return;
            }
            if (sz <= 0)
            {
                Fail("Point size must be a positive number.");
                return;
            }
            Run(mode ?? state.Mode, sz, relative.IsChecked == true);
        }

        for (var i = 0; i < Modes.Length; i++)
        {
            var mode = Modes[i];
            var button = new Button
            {
                Content = mode.ToString(CultureInfo.InvariantCulture),
                Width = 48,
                Height = 36,
                MinHeight = Density.MinTarget,
                Padding = new Thickness(0),
                Tag = mode,
            };
            Density.Name(button, "PDMODE " + mode);
            ToolTipService.SetToolTip(button, "PDMODE " + mode);
            Grid.SetRow(button, i / 5);
            Grid.SetColumn(button, i % 5);
            button.Click += (_, _) => ApplyFromUi(mode);
            modeButtons[mode] = button;
            grid.Children.Add(button);
        }

        size.LostFocus += (_, _) => ApplyFromUi();
        relative.Checked += (_, _) => ApplyFromUi();
        absolute.Checked += (_, _) => ApplyFromUi();

        Density.Apply(root);
        var window = new Window
        {
            Title = "Point Style",
            Content = new ScrollViewer
            {
                Content = new Grid { RequestedTheme = theme, Children = { root } },
            },
        };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
        }
        ShellIcon.Apply(window);
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32(360, 360));
        Load();
        return window;
    }

    private static TextBlock Heading(string text)
    {
        var block = new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        Density.Name(block, text);
        return block;
    }

    private sealed class State
    {
        public long Mode { get; set; }
        public double Size { get; set; } = 5;
        public bool Relative { get; set; } = true;
    }
}
