// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet

using Aorms.Bridge;

namespace Aadt.Bridge;

/// <summary>
/// Factory for the AORMS hub bridge (firm.db under LocalAppData\AADT).
/// Imports AORMS Connect session.json when present (C2 SSO).
/// Activate stays in AORMS Connect — never call ActivateAsync from AADT UI.
/// </summary>
public static class AormsBridgeHost
{
    public const string AppFolderName = "AADT";

    public static string FirmDbPath() =>
        Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            AppFolderName,
            "firm.db");

    public static AormsBridge CreateFromEnvironment()
    {
        var deviceId = Environment.GetEnvironmentVariable("INSTALL_ID")
            ?? $"aadt-{Environment.MachineName}".ToLowerInvariant();
        var opt = new BridgeOptions
        {
            LicenseApiUrl = Environment.GetEnvironmentVariable("ESTI_LICENSE_API_URL") ?? "",
            HubUrl = Environment.GetEnvironmentVariable("ESTI_HUB_URL") ?? "http://127.0.0.1:4000",
            ProductApiKey = Environment.GetEnvironmentVariable("ESTI_PRODUCT_API_KEY") ?? "",
            DeviceId = deviceId,
            DeviceName = "AADT",
        };
        var dbPath = FirmDbPath();
        Directory.CreateDirectory(Path.GetDirectoryName(dbPath)!);
        var bridge = new AormsBridge(opt, dbPath);
        bridge.TryImportConnectSession(overwrite: true);
        return bridge;
    }
}
