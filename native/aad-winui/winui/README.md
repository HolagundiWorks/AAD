# WinUI 3 + Fluent 2 target shell

Product UI direction ([ADR-0017](../../docs/adr/0017-winui-shell-rust-engine.md)).

Open **[`AadWinui.sln`](AadWinui.sln)** (x64). See [`AadWinui/BUILD.md`](AadWinui/BUILD.md).

| Item | Status |
|------|--------|
| M17.1–3 Chrome + paper + Properties | ✅ |
| Connect licence + Aorms.Bridge firm.db | ✅ `aadt_bridge.dll` · Flush |
| M17.4 MSIX | ▶ optional |

Requires `aadt_ffi.dll` **ABI ≥ 4** beside the exe.

**Licence:** Activate only in **AORMS Connect**. WinUI reads
`%LocalAppData%\AORMS-Connect\session.json` (or `--connect-session`) and shows
Licensed / Unbound on the top strip — no HLP Activate UI.

Interim Win32+Direct2D host: [`../src/main.cpp`](../src/main.cpp).
