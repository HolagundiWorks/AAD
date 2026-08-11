// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet

using System.Runtime.InteropServices;
using System.Text;
using Aadt.Bridge;
using Aorms.Bridge;

namespace Aadt.Bridge.Native;

/// <summary>
/// C ABI for WinUI / Rust. Activate is never exposed — Connect owns Activate.
/// </summary>
public static class NativeExports
{
    static readonly object Gate = new();
    static AormsBridge? Bridge;

    static AormsBridge Ensure()
    {
        lock (Gate)
        {
            Bridge ??= AormsBridgeHost.CreateFromEnvironment();
            return Bridge;
        }
    }

    /// <summary>Create firm.db + import Connect session.json. Returns 1 if imported/bound.</summary>
    [UnmanagedCallersOnly(EntryPoint = "aadt_bridge_ensure")]
    public static int EnsureBridge()
    {
        try
        {
            var b = Ensure();
            b.TryImportConnectSession(overwrite: true);
            return b.HubConfigured().HasSyncToken ? 1 : 0;
        }
        catch
        {
            return -1;
        }
    }

    /// <summary>
    /// Fill UTF-8 status buffers (hub, firmDb, note). Returns 1 if syncToken present.
    /// </summary>
    [UnmanagedCallersOnly(EntryPoint = "aadt_bridge_status")]
    public static unsafe int Status(
        byte* hubUtf8, int hubCch,
        byte* firmDbUtf8, int firmDbCch,
        byte* noteUtf8, int noteCch,
        int* syncReady)
    {
        try
        {
            var b = Ensure();
            b.TryImportConnectSession(overwrite: true);
            var cfg = b.HubConfigured();
            var snap = b.LicenceSnapshot();
            if (syncReady is not null)
                *syncReady = cfg.SyncReady ? 1 : 0;
            WriteUtf8(cfg.HubUrl, hubUtf8, hubCch);
            WriteUtf8(snap.FirmDbPath, firmDbUtf8, firmDbCch);
            var note = cfg.HasSyncToken
                ? $"Licence in firm.db · Connect session · {snap.LicenceStatus}"
                : "Unbound — Activate in AORMS Connect, then reopen AADT";
            WriteUtf8(note, noteUtf8, noteCch);
            return cfg.HasSyncToken ? 1 : 0;
        }
        catch (Exception ex)
        {
            WriteUtf8(ex.Message, noteUtf8, noteCch);
            if (syncReady is not null) *syncReady = 0;
            return -1;
        }
    }

    /// <summary>Flush meta/artifact outboxes. Returns metaSent + (artSent&lt;&lt;16), or -1 on error; -2 if skipped.</summary>
    [UnmanagedCallersOnly(EntryPoint = "aadt_bridge_flush")]
    public static unsafe int Flush(byte* errUtf8, int errCch)
    {
        try
        {
            var b = Ensure();
            b.TryImportConnectSession(overwrite: true);
            var result = b.FlushAsync().GetAwaiter().GetResult();
            if (result.SkippedReason is not null)
            {
                WriteUtf8(result.SkippedReason, errUtf8, errCch);
                return -2;
            }
            WriteUtf8($"metaSent={result.MetaSent} artSent={result.ArtifactsSent}", errUtf8, errCch);
            return result.MetaSent + (result.ArtifactsSent << 16);
        }
        catch (Exception ex)
        {
            WriteUtf8(ex.Message, errUtf8, errCch);
            return -1;
        }
    }

    /// <summary>Enqueue a test meta row (smoke). Returns outbox id or -1.</summary>
    [UnmanagedCallersOnly(EntryPoint = "aadt_bridge_enqueue_ping")]
    public static long EnqueuePing()
    {
        try
        {
            var b = Ensure();
            var id = Guid.NewGuid().ToString("N")[..12];
            return b.EnqueueMeta(
                "aadt.ping",
                id,
                new { source = "AADT", at = DateTime.UtcNow.ToString("O") });
        }
        catch
        {
            return -1;
        }
    }

    [UnmanagedCallersOnly(EntryPoint = "aadt_bridge_shutdown")]
    public static void Shutdown()
    {
        lock (Gate)
        {
            Bridge?.Dispose();
            Bridge = null;
        }
    }

    static unsafe void WriteUtf8(string value, byte* dest, int cch)
    {
        if (dest is null || cch <= 0) return;
        var bytes = Encoding.UTF8.GetBytes(value ?? "");
        var n = Math.Min(bytes.Length, cch - 1);
        for (var i = 0; i < n; i++)
            dest[i] = bytes[i];
        dest[n] = 0;
    }
}
