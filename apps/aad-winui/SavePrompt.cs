using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using WinRT.Interop;

namespace Aad.WinUi;

/// <summary>Save / Don't save / Cancel in its own window, so the drawing child does not cover it.</summary>
internal static class SavePrompt
{
    private static Window? _window;

    public static Task<int> Ask(Window owner, string title)
    {
        var done = new TaskCompletionSource<int>();
        var theme = owner.Content is FrameworkElement shell ? shell.RequestedTheme : ElementTheme.Default;
        var stack = new StackPanel { Spacing = 8, Margin = new Thickness(12) };
        stack.Children.Add(new TextBlock { Text = "Save changes to " + title + "?", TextWrapping = TextWrapping.Wrap });
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6, HorizontalAlignment = HorizontalAlignment.Right };
        var save = new Button { Content = "Save", MinWidth = 72, FontSize = Density.Font };
        var discard = new Button { Content = "Don't save", MinWidth = 72, FontSize = Density.Font };
        var cancel = new Button { Content = "Cancel", MinWidth = 72, FontSize = Density.Font };
        row.Children.Add(save);
        row.Children.Add(discard);
        row.Children.Add(cancel);
        stack.Children.Add(row);
        var window = new Window
        {
            Title = "AAD",
            Content = new Grid { RequestedTheme = theme, Children = { stack } },
        };
        var answered = false;
        void Finish(int answer)
        {
            if (answered)
            {
                return;
            }
            answered = true;
            done.TrySetResult(answer);
            window.Close();
        }
        save.Click += (_, _) => Finish(1);
        discard.Click += (_, _) => Finish(2);
        cancel.Click += (_, _) => Finish(0);
        window.AppWindow.Closing += (_, args) =>
        {
            if (!answered)
            {
                answered = true;
                done.TrySetResult(0);
            }
        };
        if (window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsAlwaysOnTop = true;
        }
        window.Closed += (_, _) =>
        {
            if (ReferenceEquals(_window, window))
            {
                _window = null;
            }
            if (!answered)
            {
                answered = true;
                done.TrySetResult(0);
            }
        };
        _window = window;
        window.AppWindow.Resize(new Windows.Graphics.SizeInt32(360, 140));
        window.Activate();
        Native.ShowWindow(WindowNative.GetWindowHandle(window), 8);
        return done.Task;
    }
}
