namespace Aad.WinUi;

/// <summary>
/// Command-line options forwarded from <c>aad.exe</c> or passed when starting Aad.WinUi directly.
/// </summary>
public sealed class LaunchArgs
{
    public bool Sample { get; init; }

    public ushort? ControlPort { get; init; }

    /// <summary>The AORMS project the drawings are expected to belong to (<c>--project</c>).</summary>
    public string? Project { get; init; }

    public IReadOnlyList<string> Files { get; init; } = Array.Empty<string>();

    public static LaunchArgs Parse(IEnumerable<string> args)
    {
        var sample = false;
        ushort? control = null;
        string? project = null;
        var files = new List<string>();
        using var it = args.GetEnumerator();
        while (it.MoveNext())
        {
            var a = it.Current;
            if (a is "--sample")
            {
                sample = true;
                continue;
            }
            if (a is "--control")
            {
                if (!it.MoveNext())
                {
                    continue;
                }
                if (ushort.TryParse(it.Current.Trim(), out var port))
                {
                    control = port;
                }
                continue;
            }
            if (a is "--project")
            {
                if (it.MoveNext())
                {
                    var id = it.Current.Trim();
                    if (id.Length is > 0 and <= 64 && id.All(c => char.IsAsciiLetterOrDigit(c) || c == '-'))
                    {
                        project = id;
                    }
                }
                continue;
            }
            if (a.StartsWith("--", StringComparison.Ordinal))
            {
                continue;
            }
            files.Add(a);
        }
        if (control is null
            && ushort.TryParse(Environment.GetEnvironmentVariable("AAD_CONTROL_PORT"), out var fromEnv))
        {
            control = fromEnv;
        }
        return new LaunchArgs
        {
            Sample = sample,
            ControlPort = control,
            Project = project,
            Files = files,
        };
    }
}
