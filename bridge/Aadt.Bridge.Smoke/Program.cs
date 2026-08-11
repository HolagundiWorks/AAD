// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet

using Aadt.Bridge;

Console.WriteLine("AADT Bridge smoke — firm.db + Connect session (no Activate)");
using var bridge = AormsBridgeHost.CreateFromEnvironment();
var cfg = bridge.HubConfigured();
var snap = bridge.LicenceSnapshot();
Console.WriteLine($"firm.db={snap.FirmDbPath}");
Console.WriteLine($"session={snap.SessionPath} present={snap.SessionFilePresent}");
Console.WriteLine($"hasSyncToken={cfg.HasSyncToken} syncReady={cfg.SyncReady} hub={cfg.HubUrl}");
Console.WriteLine(cfg.HasSyncToken
    ? "OK — licence imported from AORMS Connect"
    : "Unbound — Activate in AORMS Connect first");
