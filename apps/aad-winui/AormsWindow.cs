using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;

namespace Aad.WinUi;

/// <summary>
/// Sign-in, project list and drawings. The password is sent once to <c>aorms.signin</c> and is not stored.
/// The session is single-threaded, so these commands run on the window thread.
/// </summary>
internal sealed class AormsWindow : Window
{
    private readonly IntPtr _host;
    private readonly Action<string> _chip;
    private readonly TextBox _email = new() { PlaceholderText = "Email" };
    private readonly PasswordBox _password = new() { PlaceholderText = "Password" };
    private readonly TextBlock _status = new() { TextWrapping = TextWrapping.Wrap };
    private readonly ListView _projects = new();
    private readonly TextBlock _drawings = new() { TextWrapping = TextWrapping.Wrap };

    private AormsWindow(Window owner, IntPtr host, Action<string> chip)
    {
        _host = host;
        _chip = chip;
        Title = "AORMS";
        Density.Name(_email, "Email");
        Density.Name(_password, "Password");
        _email.MinHeight = Density.MinTarget;
        AutomationProperties.SetLiveSetting(_status, AutomationLiveSetting.Polite);
        Density.Name(_status, "AORMS status");
        Density.Name(_projects, "Projects");
        Density.Name(_drawings, "Drawings");
        var signIn = Action("Sign in");
        signIn.Click += (_, _) => SignIn();
        var signOut = Action("Sign out");
        signOut.Click += (_, _) => Run("aorms.signout", "{}");
        var refresh = Action("Projects");
        refresh.Click += (_, _) => LoadProjects();
        var open = Action("Pull project info");
        open.Click += (_, _) => OpenSelected();
        var drawings = Action("Drawings");
        drawings.Click += (_, _) => LoadDrawings();
        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        buttons.Children.Add(signIn);
        buttons.Children.Add(signOut);
        buttons.Children.Add(refresh);
        buttons.Children.Add(open);
        buttons.Children.Add(drawings);
        var root = new StackPanel { Spacing = 8, Margin = new Thickness(12), Children = { _email, _password, buttons, _status, _projects, _drawings } };
        if (owner.Content is FrameworkElement shell)
        {
            root.RequestedTheme = shell.RequestedTheme;
        }
        Content = new ScrollViewer { Content = root };
        Density.Apply(root);
        AppWindow.Resize(new Windows.Graphics.SizeInt32(420, 480));
        ShellIcon.Apply(this);
        Activated += (_, _) => Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(this), 8);
        Restore();
    }

    public static AormsWindow Show(Window owner, IntPtr host, Action<string> chip)
    {
        var window = new AormsWindow(owner, host, chip);
        window.Activate();
        Native.ShowWindow(WinRT.Interop.WindowNative.GetWindowHandle(window), 8);
        return window;
    }

    private void Restore()
    {
        Run("aorms.restore", "{}");
        Run("aorms.status", "{}");
    }

    private void SignIn()
    {
        var body = JsonSerializer.Serialize(new Dictionary<string, string>
        {
            ["email"] = _email.Text.Trim(),
            ["password"] = _password.Password,
        });
        _password.Password = "";
        Run("aorms.signin", body);
        LoadProjects();
    }

    private void LoadProjects()
    {
        if (!Run("aorms.projects", "{}"))
        {
            return;
        }
        _projects.Items.Clear();
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            if (!doc.RootElement.TryGetProperty("projects", out var projects))
            {
                return;
            }
            foreach (var project in projects.EnumerateArray())
            {
                var title = project.TryGetProperty("title", out var name) ? name.GetString() ?? "" : "";
                var id = project.TryGetProperty("projectOfficeId", out var office) ? office.ToString() : "";
                _projects.Items.Add(new ProjectItem(id, title));
            }
        }
        catch (JsonException)
        {
            _status.Text = "The project list could not be read.";
        }
    }

    private void OpenSelected()
    {
        if (_projects.SelectedItem is not ProjectItem item || item.Id.Length == 0)
        {
            _status.Text = "Select a project first.";
            return;
        }
        Run("aorms.open", JsonSerializer.Serialize(new Dictionary<string, string> { ["projectOfficeId"] = item.Id }));
    }

    private void LoadDrawings()
    {
        var id = _projects.SelectedItem is ProjectItem item ? item.Id : "";
        var body = id.Length == 0 ? "{}" : JsonSerializer.Serialize(new Dictionary<string, string> { ["projectOfficeId"] = id });
        if (!Run("aorms.drawings", body))
        {
            return;
        }
        try
        {
            using var doc = JsonDocument.Parse(Native.ReadResult(_host));
            if (!doc.RootElement.TryGetProperty("drawings", out var drawings))
            {
                _drawings.Text = "";
                return;
            }
            var lines = new List<string>();
            foreach (var drawing in drawings.EnumerateArray())
            {
                var title = drawing.TryGetProperty("title", out var name) ? name.GetString() ?? "" : "";
                var file = drawing.TryGetProperty("fileName", out var fileName) ? fileName.GetString() ?? "" : "";
                lines.Add(string.IsNullOrEmpty(file) ? title : title + "  " + file);
            }
            _drawings.Text = lines.Count == 0 ? "No drawings." : string.Join("\n", lines);
        }
        catch (JsonException)
        {
            _drawings.Text = "The drawing list could not be read.";
        }
    }

    private bool Run(string command, string body)
    {
        if (Native.Execute(_host, command, body) != 0)
        {
            _status.Text = Native.LastError(_host);
            return false;
        }
        var result = Native.ReadResult(_host);
        var summary = Summary(result);
        _status.Text = summary;
        if (command is "aorms.status" or "aorms.signin" or "aorms.signout" or "aorms.restore")
        {
            _chip(summary);
        }
        return true;
    }

    internal static string Chip(IntPtr host)
    {
        if (Native.Execute(host, "aorms.status", "{}") != 0)
        {
            return "AORMS";
        }
        return Summary(Native.ReadResult(host));
    }

    private static string Summary(string json)
    {
        try
        {
            using var doc = JsonDocument.Parse(json);
            var root = doc.RootElement;
            if (root.TryGetProperty("signedIn", out var signed) && signed.ValueKind == JsonValueKind.False)
            {
                return "Signed out";
            }
            if (root.TryGetProperty("studio", out var studio) && studio.GetString() is string name && name.Length > 0)
            {
                var role = root.TryGetProperty("role", out var roleValue) ? roleValue.GetString() ?? "" : "";
                return role.Length == 0 ? name : name + " · " + role;
            }
            if (root.TryGetProperty("outcome", out var outcome) && outcome.GetString() is string text)
            {
                return text;
            }
            if (root.TryGetProperty("message", out var message) && message.GetString() is string note)
            {
                return note;
            }
        }
        catch (JsonException)
        {
        }
        return json.Length > 180 ? json[..180] : json;
    }

    private static Button Action(string label)
    {
        var button = new Button { Content = label, MinHeight = Density.MinTarget };
        Density.Name(button, label);
        return button;
    }

    private sealed class ProjectItem
    {
        public ProjectItem(string id, string title)
        {
            Id = id;
            Title = title;
        }

        public string Id { get; }
        private string Title { get; }
        public override string ToString() => Title.Length == 0 ? Id : Title;
    }
}
