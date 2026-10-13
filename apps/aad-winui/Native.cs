using System.Runtime.InteropServices;
using System.Text;

namespace Aad.WinUi;

internal static class Native
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct Stats
    {
        public uint entities;
        public uint pad;
        public double viewHeight;
        public uint width;
        public uint height;
    }

    public static IntPtr Create(bool sample) => aad_host_create(sample ? 1 : 0);

    public static void Destroy(IntPtr host) => aad_host_destroy(host);

    public static int Embed(IntPtr host, IntPtr parent, int x, int y, uint width, uint height) =>
        aad_host_embed(host, parent, x, y, width, height);

    public static int Move(IntPtr host, int x, int y, uint width, uint height) =>
        aad_host_move(host, x, y, width, height);

    public static int Pump(IntPtr host) => aad_host_pump(host);

    public static int Redraw(IntPtr host) => aad_host_pump(host);

    public static bool IsEmbedded(IntPtr host) => aad_host_embedded(host) != 0;

    public const int TextMenu = 0;
    public const int TextChrome = 1;
    public const int TextPanels = 2;

    public static int CommandLine(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text);
        try
        {
            return aad_host_cmdline(host, textPtr);
        }
        finally
        {
            Marshal.FreeCoTaskMem(textPtr);
        }
    }

    public static int Cancel(IntPtr host) => aad_host_cancel(host);

    public static bool GripCycle(IntPtr host) => aad_host_grip_cycle(host) != 0;

    public static int SnapTab(IntPtr host) => aad_host_snap_tab(host);

    public static string? ReadForm(IntPtr host, string id)
    {
        var idPtr = Marshal.StringToCoTaskMemUTF8(id);
        try
        {
            int needed = aad_host_form(host, idPtr, IntPtr.Zero, 0);
            if (needed == 0)
            {
                return null;
            }
            int size = needed < 0 ? -needed : needed;
            var buffer = new byte[size];
            var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
            try
            {
                int n = aad_host_form(host, idPtr, handle.AddrOfPinnedObject(), (uint)buffer.Length);
                if (n <= 0)
                {
                    return null;
                }
                return Encoding.UTF8.GetString(buffer, 0, Math.Min(n, buffer.Length));
            }
            finally
            {
                handle.Free();
            }
        }
        finally
        {
            Marshal.FreeCoTaskMem(idPtr);
        }
    }

    public static int StartForm(IntPtr host, string id, string preset)
    {
        var idPtr = Marshal.StringToCoTaskMemUTF8(id);
        var presetPtr = Marshal.StringToCoTaskMemUTF8(preset);
        try
        {
            return aad_host_start_form(host, idPtr, presetPtr);
        }
        finally
        {
            Marshal.FreeCoTaskMem(idPtr);
            Marshal.FreeCoTaskMem(presetPtr);
        }
    }

    public static int Draft(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text);
        try
        {
            return aad_host_draft(host, textPtr);
        }
        finally
        {
            Marshal.FreeCoTaskMem(textPtr);
        }
    }

    public static string? DynTab(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text);
        try
        {
            int needed = aad_host_dyn_tab(host, textPtr, IntPtr.Zero, 0);
            if (needed == 0)
            {
                return null;
            }
            int size = needed < 0 ? -needed : needed;
            var buffer = new byte[size];
            var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
            try
            {
                int n = aad_host_dyn_tab(host, textPtr, handle.AddrOfPinnedObject(), (uint)buffer.Length);
                if (n <= 0)
                {
                    return null;
                }
                return Encoding.UTF8.GetString(buffer, 0, Math.Min(n, buffer.Length));
            }
            finally
            {
                handle.Free();
            }
        }
        finally
        {
            Marshal.FreeCoTaskMem(textPtr);
        }
    }

    public static int Theme(IntPtr host, bool dark) => aad_host_theme(host, dark ? 1 : 0);

    public static byte[]? ReadPdf(IntPtr host)
    {
        int needed = aad_host_pdf(host, IntPtr.Zero, 0);
        if (needed == 0)
        {
            return null;
        }
        int size = needed < 0 ? -needed : needed;
        var buffer = new byte[size];
        var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
        try
        {
            int n = aad_host_pdf(host, handle.AddrOfPinnedObject(), (uint)buffer.Length);
            if (n <= 0)
            {
                return null;
            }
            if (n < buffer.Length)
            {
                Array.Resize(ref buffer, n);
            }
            return buffer;
        }
        finally
        {
            handle.Free();
        }
    }

    public static string ReadText(IntPtr host, int slot)
    {
        int needed = aad_host_text(host, slot, IntPtr.Zero, 0);
        if (needed == 0)
        {
            return "";
        }
        int size = needed < 0 ? -needed : needed;
        var buffer = new byte[size];
        var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
        try
        {
            int n = aad_host_text(host, slot, handle.AddrOfPinnedObject(), (uint)buffer.Length);
            if (n < 0)
            {
                return "";
            }
            return Encoding.UTF8.GetString(buffer, 0, Math.Min(n, buffer.Length));
        }
        finally
        {
            handle.Free();
        }
    }

    public static int Execute(IntPtr host, string command, string parameters)
    {
        var commandPtr = Marshal.StringToCoTaskMemUTF8(command);
        var paramsPtr = Marshal.StringToCoTaskMemUTF8(parameters);
        try
        {
            return aad_host_execute(host, commandPtr, paramsPtr);
        }
        finally
        {
            Marshal.FreeCoTaskMem(commandPtr);
            Marshal.FreeCoTaskMem(paramsPtr);
        }
    }

    public static Stats ReadStats(IntPtr host)
    {
        aad_host_stats(host, out var stats);
        return stats;
    }

    public static string LastError(IntPtr host)
    {
        int needed = aad_host_last_error(host, IntPtr.Zero, 0);
        if (needed == 0)
        {
            return "";
        }
        int size = needed < 0 ? -needed : needed;
        var buffer = new byte[size];
        var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
        try
        {
            int n = aad_host_last_error(host, handle.AddrOfPinnedObject(), (uint)buffer.Length);
            if (n < 0)
            {
                return "";
            }
            return Encoding.UTF8.GetString(buffer, 0, Math.Min(n, buffer.Length));
        }
        finally
        {
            handle.Free();
        }
    }

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern IntPtr aad_host_create(int sample);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern void aad_host_destroy(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_execute(IntPtr host, IntPtr command, IntPtr parameters);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_cmdline(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_cancel(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_form(IntPtr host, IntPtr id, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_start_form(IntPtr host, IntPtr id, IntPtr preset);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_grip_cycle(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_snap_tab(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_draft(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_dyn_tab(IntPtr host, IntPtr text, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_theme(IntPtr host, int dark);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_pdf(IntPtr host, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_text(IntPtr host, int slot, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_embed(IntPtr host, IntPtr parent, int x, int y, uint width, uint height);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_move(IntPtr host, int x, int y, uint width, uint height);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_pump(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_embedded(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_stats(IntPtr host, out Stats stats);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_last_error(IntPtr host, IntPtr buffer, uint length);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
}
