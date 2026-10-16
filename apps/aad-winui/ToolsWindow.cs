using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>Which commands sit on the tool strip, and which system face the chrome uses.</summary>
internal sealed class ToolsWindow : Window
{
    private readonly List<(string Id, string Label, CheckBox Box)> _rows = new();
    private readonly ComboBox _font = new();
    private readonly Action<IReadOnlyList<string>, string> _apply;

    private ToolsWindow(Window owner, IReadOnlyList<(string Id, string Label)> catalog, IReadOnlyList<string> selected, string font, Action<IReadOnlyList<string>, string> apply)
    {
        _apply = apply;
        Title = "Tool bar";
        var list = new StackPanel { Spacing = 2 };
        foreach (var (id, label) in catalog)
        {
            var box = new CheckBox { Content = label, IsChecked = selected.Contains(id) };
            _rows.Add((id, label, box));
            list.Children.Add(box);
        }
        foreach (var (id, label) in Fonts)
        {
            _font.Items.Add(new FontChoice(id, label));
            if (id == font)
            {
                _font.SelectedIndex = _font.Items.Count - 1;
            }
        }
        if (_font.SelectedIndex < 0)
        {
            _font.SelectedIndex = 0;
        }
        var toolsHeading = Heading("Tool bar");
        var fontHeading = Heading("Interface font");
        Density.Name(_font, "Interface font");
        _font.MinHeight = Density.MinTarget;
        var save = new Button { Content = "Apply", MinHeight = Density.MinTarget };
        Density.Name(save, "Apply tool bar");
        save.Click += (_, _) =>
        {
            var ids = _rows.Where(row => row.Box.IsChecked == true).Select(row => row.Id).ToList();
            var face = _font.SelectedItem is FontChoice choice ? choice.Id : "engineering";
            _apply(ids, face);
            Close();
        };
        var root = new StackPanel
        {
            Spacing = 8,
            Margin = new Thickness(12),
            Children = { toolsHeading, list, fontHeading, _font, save },
        };
        if (owner.Content is FrameworkElement shell)
        {
            root.RequestedTheme = shell.RequestedTheme;
        }
        Content = new ScrollViewer { Content = root };
        Density.Apply(root);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(280, 460));
        ShellIcon.Apply(this);
    }

    public static readonly (string Id, string Label)[] Fonts =
    [
        ("engineering", "Engineering"),
        ("editorial", "Editorial"),
        ("multilingual", "Multilingual"),
        ("compact", "Compact"),
        ("drafting", "Drafting utility"),
    ];

    public static ToolsWindow Show(Window owner, IReadOnlyList<(string Id, string Label)> catalog, IReadOnlyList<string> selected, string font, Action<IReadOnlyList<string>, string> apply)
    {
        var window = new ToolsWindow(owner, catalog, selected, font, apply);
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    private static TextBlock Heading(string text)
    {
        var block = new TextBlock { Text = text, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
        AutomationProperties.SetHeadingLevel(block, AutomationHeadingLevel.Level2);
        Density.Name(block, text);
        return block;
    }

    private sealed class FontChoice(string id, string label)
    {
        public string Id { get; } = id;
        public override string ToString() => label;
    }
}
