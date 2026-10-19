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

    public static int ClearLog(IntPtr host) => aad_host_clear_log(host);

    public static bool GripCycle(IntPtr host) => aad_host_grip_cycle(host) != 0;

    public static bool GripPoint(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text);
        try
        {
            return aad_host_grip_point(host, textPtr) != 0;
        }
        finally
        {
            Marshal.FreeCoTaskMem(textPtr);
        }
    }

    public static int SnapTab(IntPtr host) => aad_host_snap_tab(host);

    public static int KeyNudge(IntPtr host, int dx, int dy, bool ctrl, bool shift) =>
        aad_host_key_nudge(host, dx, dy, ctrl ? 1 : 0, shift ? 1 : 0);

    public static int KeyActivate(IntPtr host, bool shift) => aad_host_key_activate(host, shift ? 1 : 0);

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

    /// True when Space should insert a space (assistant `/` request, sentence, text prompt).
    public static bool SpaceLiteral(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text ?? "");
        try
        {
            return aad_host_space_literal(host, textPtr) != 0;
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

    public static int SetUcs(IntPtr host, bool on) => aad_host_ucs(host, on ? 1 : 0);

    public static int SetCanvas(IntPtr host, bool visible) => aad_host_canvas(host, visible ? 1 : 0);

    public static int Assistant(IntPtr host, bool accept) => aad_host_assistant(host, accept ? 1 : 0);

    public static int Listen(IntPtr host, ushort port) => aad_host_listen(host, port);

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

    /// Start reading a drawing on a worker thread; returns at once.
    public static bool OpenStart(IntPtr host, string path)
    {
        var pathPtr = Marshal.StringToCoTaskMemUTF8(path);
        try
        {
            return aad_host_open_start(host, pathPtr) == 0;
        }
        finally
        {
            Marshal.FreeCoTaskMem(pathPtr);
        }
    }

    /// 0 nothing loading, 1 reading, 2 opened and fitted, -1 failed (see LastError).
    public static int OpenPoll(IntPtr host) => aad_host_open_poll(host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_open_start(IntPtr host, IntPtr path);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_open_poll(IntPtr host);

    /// Start writing the drawing on a worker thread: to path, or to its own file when null.
    public static bool SaveStart(IntPtr host, string? path)
    {
        var pathPtr = path == null ? IntPtr.Zero : Marshal.StringToCoTaskMemUTF8(path);
        try
        {
            return aad_host_save_start(host, pathPtr) == 0;
        }
        finally
        {
            if (pathPtr != IntPtr.Zero)
            {
                Marshal.FreeCoTaskMem(pathPtr);
            }
        }
    }

    /// 0 nothing saving, 1 writing, 2 saved, -1 failed (see LastError).
    public static int SavePoll(IntPtr host) => aad_host_save_poll(host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_save_start(IntPtr host, IntPtr path);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_save_poll(IntPtr host);

    /// Start a plot (plot or exportpdf, parameters include path) on a worker thread.
    public static bool PlotStart(IntPtr host, string command, string parameters)
    {
        var commandPtr = Marshal.StringToCoTaskMemUTF8(command);
        var paramsPtr = Marshal.StringToCoTaskMemUTF8(parameters);
        try
        {
            return aad_host_plot_start(host, commandPtr, paramsPtr) == 0;
        }
        finally
        {
            Marshal.FreeCoTaskMem(commandPtr);
            Marshal.FreeCoTaskMem(paramsPtr);
        }
    }

    /// 0 no plot running, 1 building, 2 written, -1 failed (see LastError).
    public static int PlotPoll(IntPtr host) => aad_host_plot_poll(host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_plot_start(IntPtr host, IntPtr command, IntPtr parameters);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_plot_poll(IntPtr host);

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

    public static string ReadResult(IntPtr host) => ReadBuffer(aad_host_result, host);

    public static string ReadPopup(IntPtr host) => ReadBuffer(aad_host_popup, host);

    /// JSON from a finished <c>plot.window</c> prompt, or empty. Cleared when read.
    public static string ReadPlotRequest(IntPtr host) => ReadBuffer(aad_host_plot_request, host);

    public static string ReadClipboard(IntPtr host) => ReadBuffer(aad_host_clipboard, host);

    public static bool LoadClipboard(IntPtr host, string text)
    {
        var textPtr = Marshal.StringToCoTaskMemUTF8(text);
        try
        {
            return aad_host_clipboard_load(host, textPtr) != 0;
        }
        finally
        {
            Marshal.FreeCoTaskMem(textPtr);
        }
    }

    private static string ReadBuffer(Func<IntPtr, IntPtr, uint, int> read, IntPtr host)
    {
        int needed = read(host, IntPtr.Zero, 0);
        if (needed == 0)
        {
            return "";
        }
        int size = needed < 0 ? -needed : needed;
        if (size is < 0 or > 8 * 1024 * 1024)
        {
            return "";
        }
        var buffer = new byte[size];
        var handle = GCHandle.Alloc(buffer, GCHandleType.Pinned);
        try
        {
            int n = read(host, handle.AddrOfPinnedObject(), (uint)buffer.Length);
            if (n <= 0)
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

    public static string LastError(IntPtr host) => ReadBuffer(aad_host_last_error, host);

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
    private static extern int aad_host_clear_log(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_form(IntPtr host, IntPtr id, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_start_form(IntPtr host, IntPtr id, IntPtr preset);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_grip_cycle(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_grip_point(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_snap_tab(IntPtr host);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_key_nudge(IntPtr host, int dx, int dy, int ctrl, int shift);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_key_activate(IntPtr host, int shift);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_draft(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_space_literal(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_dyn_tab(IntPtr host, IntPtr text, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_theme(IntPtr host, int dark);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_ucs(IntPtr host, int on);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_canvas(IntPtr host, int visible);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_assistant(IntPtr host, int accept);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_listen(IntPtr host, ushort port);

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
    private static extern int aad_host_popup(IntPtr host, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_plot_request(IntPtr host, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_clipboard(IntPtr host, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_clipboard_load(IntPtr host, IntPtr text);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_result(IntPtr host, IntPtr buffer, uint length);

    [DllImport("aad_host", CallingConvention = CallingConvention.Cdecl, ExactSpelling = true)]
    private static extern int aad_host_last_error(IntPtr host, IntPtr buffer, uint length);

    /// The character this key would type, using the active keyboard layout. Empty for keys that do not type.
    public static string Character(Windows.System.VirtualKey key)
    {
        var state = new byte[256];
        if (!GetKeyboardState(state))
        {
            return "";
        }
        uint scan = MapVirtualKey((uint)key, 0);
        var buffer = new StringBuilder(8);
        int written = ToUnicode((uint)key, scan, state, buffer, buffer.Capacity, 0);
        if (written <= 0)
        {
            return "";
        }
        var text = buffer.ToString();
        if (text.Length == 0 || char.IsControl(text[0]))
        {
            return "";
        }
        return text;
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct PointI
    {
        public int X;
        public int Y;
    }

    public const int GwlpHwndParent = -8;
    public const int GwlExStyle = -20;
    public const int WsExTransparent = 0x20;
    public const int WsExNoActivate = 0x08000000;

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out PointI point);

    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr hWnd, ref PointI point);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int index);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW")]
    public static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int index, IntPtr value);

    [DllImport("dwmapi.dll")]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int cx, int cy, uint flags);

    public static void Lift(IntPtr hwnd) =>
        _ = SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010 | 0x0040);

    public static void RoundWindow(IntPtr hwnd) => Frame(hwnd, 0x00242424, true);

    public static void Frame(IntPtr hwnd, int borderColor, bool dark)
    {
        const long caption = 0x00C00000;
        const long thick = 0x00040000;
        const long border = 0x00800000;
        const long dialog = 0x00400000;
        var style = GetWindowLongPtr(hwnd, -16).ToInt64();
        var next = style & ~(caption | thick | border | dialog);
        if (next != style)
        {
            _ = SetWindowLongPtr(hwnd, -16, new IntPtr(next));
            _ = SetWindowPos(hwnd, IntPtr.Zero, 0, 0, 0, 0, 0x0020 | 0x0002 | 0x0001 | 0x0004 | 0x0010);
        }
        var preference = 2;
        _ = DwmSetWindowAttribute(hwnd, 33, ref preference, sizeof(int));
        var mode = dark ? 1 : 0;
        _ = DwmSetWindowAttribute(hwnd, 20, ref mode, sizeof(int));
        _ = DwmSetWindowAttribute(hwnd, 34, ref borderColor, sizeof(int));
        _ = DwmSetWindowAttribute(hwnd, 35, ref borderColor, sizeof(int));
    }

    [DllImport("user32.dll")]
    private static extern bool GetKeyboardState(byte[] lpKeyState);

    [DllImport("user32.dll")]
    private static extern uint MapVirtualKey(uint uCode, uint uMapType);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int ToUnicode(uint wVirtKey, uint wScanCode, byte[] lpKeyState, StringBuilder pwszBuff, int cchBuff, uint wFlags);
}
