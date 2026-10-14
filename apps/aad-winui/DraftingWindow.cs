using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;

namespace Aad.WinUi;

/// <summary>Polar tracking, object snaps and dynamic input. Changes go through engine commands.</summary>
internal sealed class DraftingWindow : Window
{
    private static readonly (uint Bit, string Name)[] Modes =
    [
        (1, "Endpoint"), (2, "Midpoint"), (4, "Center"), (1024, "Geometric Center"),
        (8, "Node"), (16, "Quadrant"), (32, "Intersection"), (4096, "Extension"),
        (64, "Insertion"), (128, "Perpendicular"), (256, "Tangent"), (512, "Nearest"),
        (2048, "Apparent Intersection"), (8192, "Parallel"),
    ];

    private static readonly double[] Angles = [90, 45, 30, 22.5, 18, 15, 10, 5];

    private readonly IntPtr _host;
    private readonly Action _changed;
    private readonly ToggleButton _polar = new() { Content = "Polar tracking on (F10)" };
    private readonly ToggleButton _snap = new() { Content = "Object snap on (F3)" };
    private readonly ToggleButton _dyn = new() { Content = "Dynamic input (F12)" };
    private readonly ToggleButton _ortho = new() { Content = "Ortho (F8)" };
    private readonly ComboBox _angle = new() { MinWidth = 80 };
    private readonly RadioButton _polarFormat = new() { Content = "Polar (distance < angle)", GroupName = "dynpi" };
    private readonly RadioButton _cartesian = new() { Content = "Cartesian (x, y)", GroupName = "dynpi" };
    private readonly RadioButton _relative = new() { Content = "Relative", GroupName = "dyncoord" };
    private readonly RadioButton _absolute = new() { Content = "Absolute", GroupName = "dyncoord" };
    private readonly Dictionary<uint, CheckBox> _modes = new();
    private bool _filling;
    private uint _osmode;

    public DraftingWindow(FrameworkElement? themeSource, IntPtr host, Action changed)
    {
        _host = host;
        _changed = changed;
        var stack = new StackPanel { Spacing = 6, Margin = new Thickness(12) };
        stack.Children.Add(Heading("Polar tracking"));
        stack.Children.Add(_polar);
        var angleRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        angleRow.Children.Add(new TextBlock { Text = "Increment angle", VerticalAlignment = VerticalAlignment.Center });
        foreach (var angle in Angles)
        {
            _angle.Items.Add(angle);
        }
        angleRow.Children.Add(_angle);
        stack.Children.Add(angleRow);
        stack.Children.Add(Heading("Object snap"));
        stack.Children.Add(_snap);
        var grid = new Grid { ColumnSpacing = 12, RowSpacing = 2 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (var i = 0; i < Modes.Length; i++)
        {
            if (i % 2 == 0)
            {
                grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            }
            var (bit, name) = Modes[i];
            var box = new CheckBox { Content = name };
            var mode = bit;
            box.Click += (_, _) => SetMode(mode, box.IsChecked == true);
            _modes[bit] = box;
            Grid.SetRow(box, i / 2);
            Grid.SetColumn(box, i % 2);
            grid.Children.Add(box);
        }
        stack.Children.Add(grid);
        stack.Children.Add(Heading("Dynamic input"));
        stack.Children.Add(_dyn);
        stack.Children.Add(_polarFormat);
        stack.Children.Add(_cartesian);
        stack.Children.Add(_relative);
        stack.Children.Add(_absolute);
        stack.Children.Add(_ortho);
        Density.Apply(stack);
        var theme = themeSource?.RequestedTheme ?? ElementTheme.Default;
        Content = new ScrollViewer { Content = new Grid { RequestedTheme = theme, Children = { stack } } };
        Title = "Drafting Settings";
        AppWindow.Resize(new Windows.Graphics.SizeInt32(420, 520));
        _polar.Click += (_, _) => Run("polar", new { on = _polar.IsChecked == true });
        _snap.Click += (_, _) => Run("osnap", new { on = _snap.IsChecked == true });
        _dyn.Click += (_, _) => Run("dynmode", new { on = _dyn.IsChecked == true });
        _ortho.Click += (_, _) => Run("ortho", new { on = _ortho.IsChecked == true });
        _angle.SelectionChanged += (_, _) =>
        {
            if (_filling || _angle.SelectedItem is not double degrees)
            {
                return;
            }
            Run("dsettings", new { polarang = degrees });
        };
        _polarFormat.Click += (_, _) => RunVar("DYNPIFORMAT", false);
        _cartesian.Click += (_, _) => RunVar("DYNPIFORMAT", true);
        _relative.Click += (_, _) => RunVar("DYNPICOORDS", false);
        _absolute.Click += (_, _) => RunVar("DYNPICOORDS", true);
    }

    public void Reload(bool polar, bool snap, bool dyn, bool ortho, uint osmode, double polarang, bool cartesian, bool absolute)
    {
        _filling = true;
        _osmode = osmode;
        _polar.IsChecked = polar;
        _snap.IsChecked = snap;
        _dyn.IsChecked = dyn;
        _ortho.IsChecked = ortho;
        _polarFormat.IsChecked = !cartesian;
        _cartesian.IsChecked = cartesian;
        _relative.IsChecked = !absolute;
        _absolute.IsChecked = absolute;
        _polarFormat.IsEnabled = dyn;
        _cartesian.IsEnabled = dyn;
        _relative.IsEnabled = dyn;
        _absolute.IsEnabled = dyn;
        foreach (var (bit, box) in _modes)
        {
            box.IsChecked = (osmode & bit) != 0;
        }
        var nearest = Angles.OrderBy(angle => Math.Abs(angle - polarang)).First();
        _angle.SelectedItem = nearest;
        _filling = false;
    }

    private void SetMode(uint bit, bool on)
    {
        if (_filling)
        {
            return;
        }
        var next = on ? _osmode | bit : _osmode & ~bit;
        Run("osnap", new { osmode = next });
    }

    private void Run(string command, object parameters)
    {
        if (_filling)
        {
            return;
        }
        if (Native.Execute(_host, command, JsonSerializer.Serialize(parameters)) != 0)
        {
            return;
        }
        _changed();
    }

    private void RunVar(string name, bool value) => Run("setvar", new { name, value });

    private static TextBlock Heading(string text) => new() { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Margin = new Thickness(0, 8, 0, 0) };
}
