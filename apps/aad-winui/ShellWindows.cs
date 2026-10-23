using Microsoft.UI.Xaml;

namespace Aad.WinUi;

/// <summary>
/// Tracks secondary top-level windows so closing the main shell can tear them down and exit the process.
/// The main CAD window is not tracked here.
/// </summary>
internal static class ShellWindows
{
    private static readonly object Gate = new();
    private static readonly HashSet<Window> Owned = new();
    private static Window? _main;

    public static void SetMain(Window window)
    {
        lock (Gate)
        {
            _main = window;
            Owned.Remove(window);
        }
    }

    public static bool IsMain(Window window)
    {
        lock (Gate)
        {
            return ReferenceEquals(window, _main);
        }
    }

    /// <summary>Register a tool/dialog window. No-op for the main shell.</summary>
    public static void Track(Window window)
    {
        lock (Gate)
        {
            if (ReferenceEquals(window, _main) || !Owned.Add(window))
            {
                return;
            }
        }
        window.Closed += OnOwnedClosed;
    }

    public static void CloseAllOwned()
    {
        Window[] copy;
        lock (Gate)
        {
            copy = Owned.ToArray();
            Owned.Clear();
        }
        foreach (var window in copy)
        {
            try
            {
                window.Closed -= OnOwnedClosed;
                window.Close();
            }
            catch (Exception)
            {
                // Window may already be tearing down with the shell.
            }
        }
    }

    private static void OnOwnedClosed(object sender, WindowEventArgs e)
    {
        if (sender is not Window window)
        {
            return;
        }
        window.Closed -= OnOwnedClosed;
        lock (Gate)
        {
            Owned.Remove(window);
        }
    }
}
