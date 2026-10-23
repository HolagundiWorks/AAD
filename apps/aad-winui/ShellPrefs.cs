using System.Text.Json;

namespace Aad.WinUi;

/// <summary>Shell choices kept beside the user profile. Passwords are never written here.</summary>
internal static class ShellPrefs
{
    private static readonly JsonSerializerOptions Opts = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };

    public static Prefs Load()
    {
        try
        {
            var path = Path();
            if (!File.Exists(path))
            {
                return new Prefs();
            }
            return JsonSerializer.Deserialize<Prefs>(File.ReadAllText(path), Opts) ?? new Prefs();
        }
        catch (IOException)
        {
            return new Prefs();
        }
        catch (JsonException)
        {
            return new Prefs();
        }
        catch (UnauthorizedAccessException)
        {
            return new Prefs();
        }
    }

    public static void Save(Prefs prefs)
    {
        try
        {
            var path = Path();
            var dir = System.IO.Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(dir))
            {
                Directory.CreateDirectory(dir);
            }
            File.WriteAllText(path, JsonSerializer.Serialize(prefs, Opts));
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private static string Path() =>
        System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "AAD", "shell.json");

    internal sealed class Prefs
    {
        public List<string> Tools { get; set; } = new();
        public string Font { get; set; } = "engineering";
        public bool Menu { get; set; } = true;
        public bool ToolsVisible { get; set; } = true;
        public bool Tabs { get; set; } = true;
        public bool Status { get; set; } = true;
        public bool Command { get; set; } = true;
        public bool PinLeft { get; set; } = true;
        public bool PinRight { get; set; } = true;
        public bool MergeLayers { get; set; }
        public bool Ucs { get; set; } = true;
        public double BarDx { get; set; }
        public double BarDy { get; set; }
        /// <summary><c>classic</c> or <c>pro</c>.</summary>
        public string Workspace { get; set; } = "classic";
        /// <summary>Per-mode pin, width and toolbar; keys are <c>classic</c> and <c>pro</c>.</summary>
        public Dictionary<string, ModeSlot> ModeSlots { get; set; } = new();
        /// <summary>User-resized tool window sizes (pixels), keyed by stable window id.</summary>
        public Dictionary<string, WindowSize> WindowSizes { get; set; } = new();
    }

    internal sealed class ModeSlot
    {
        public bool PinLeft { get; set; } = true;
        public bool ToolsVisible { get; set; } = true;
        public double LeftWidth { get; set; } = 280;
        public bool LeftCollapsed { get; set; }
    }

    internal sealed class WindowSize
    {
        public int Width { get; set; }
        public int Height { get; set; }
    }
}
