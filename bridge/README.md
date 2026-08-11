# AADT · Aorms.Bridge / firm.db

Suite SSO for drafting — **Activate only in AORMS Connect**.

| Artifact | Path |
| --- | --- |
| Connect session | `%LocalAppData%\AORMS-Connect\session.json` |
| AADT firm DB | `%LocalAppData%\AADT\firm.db` |
| Native ABI | `aadt_bridge.dll` (NativeAOT, `Aadt.Bridge.Native`) |

## Build

```bat
git submodule update --init --recursive
bridge\build-bridge.cmd
```

Copies `aadt_bridge.dll` next to WinUI output when present. Smoke:

```bat
dotnet run --project bridge\Aadt.Bridge.Smoke -c Release
```

## Consumers

| Surface | Behaviour |
| --- | --- |
| WinUI (`AormsBridgeHost.cpp`) | Load DLL · import session · licence chip · Flush |
| `aadt app` | Load DLL if found; else session.json status line |
| Smoke | Prints firm.db + syncToken flags |

Do **not** add HLP Activate UI in AADT.
