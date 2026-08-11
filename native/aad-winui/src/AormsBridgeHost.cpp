// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet

#include "AormsBridgeHost.hpp"
#include "aadt_bridge.h"

#include <Windows.h>

#include <array>
#include <mutex>

namespace aadt::bridge {
namespace {

using FnEnsure = int (*)();
using FnStatus = int (*)(char*, int, char*, int, char*, int, int*);
using FnFlush = int (*)(char*, int);
using FnEnqueue = long long (*)();
using FnShutdown = void (*)();

std::mutex g_mu;
HMODULE g_mod = nullptr;
FnEnsure g_ensure = nullptr;
FnStatus g_status = nullptr;
FnFlush g_flush = nullptr;
FnEnqueue g_enqueue = nullptr;
FnShutdown g_shutdown = nullptr;
bool g_loadAttempted = false;

std::wstring widenUtf8(char const* s)
{
    if (!s || !*s) return {};
    int n = MultiByteToWideChar(CP_UTF8, 0, s, -1, nullptr, 0);
    if (n <= 1) return {};
    std::wstring out((size_t)n - 1, L'\0');
    MultiByteToWideChar(CP_UTF8, 0, s, -1, out.data(), n);
    return out;
}

bool loadDll()
{
    if (g_mod) return true;
    if (g_loadAttempted) return false;
    g_loadAttempted = true;

    wchar_t modulePath[MAX_PATH]{};
    GetModuleFileNameW(nullptr, modulePath, MAX_PATH);
    std::wstring dir(modulePath);
    auto slash = dir.find_last_of(L"\\/");
    if (slash != std::wstring::npos)
        dir.resize(slash + 1);
    else
        dir.clear();

    std::wstring dll = dir + L"aadt_bridge.dll";
    g_mod = LoadLibraryW(dll.c_str());
    if (!g_mod)
    {
        // Also try cwd
        g_mod = LoadLibraryW(L"aadt_bridge.dll");
    }
    if (!g_mod) return false;

    g_ensure = reinterpret_cast<FnEnsure>(GetProcAddress(g_mod, "aadt_bridge_ensure"));
    g_status = reinterpret_cast<FnStatus>(GetProcAddress(g_mod, "aadt_bridge_status"));
    g_flush = reinterpret_cast<FnFlush>(GetProcAddress(g_mod, "aadt_bridge_flush"));
    g_enqueue = reinterpret_cast<FnEnqueue>(GetProcAddress(g_mod, "aadt_bridge_enqueue_ping"));
    g_shutdown = reinterpret_cast<FnShutdown>(GetProcAddress(g_mod, "aadt_bridge_shutdown"));
    return g_ensure && g_status;
}

} // namespace

Status ensureAndStatus()
{
    Status s;
    std::lock_guard lock(g_mu);
    if (!loadDll())
    {
        // Fallback: session.json-only status (no firm.db).
        auto lic = aadt::connect::tryReadSession();
        s.loaded = false;
        s.bound = lic.bound;
        s.syncReady = lic.bound && !lic.hubUrl.empty();
        s.hubUrl = lic.hubUrl;
        s.note = lic.bound
                    ? (L"Licensed · session.json only — build bridge\\build-bridge.cmd for firm.db")
                    : lic.note;
        return s;
    }

    g_ensure();
    std::array<char, 512> hub{};
    std::array<char, 1024> firm{};
    std::array<char, 512> note{};
    int syncReady = 0;
    int rc = g_status(hub.data(), (int)hub.size(), firm.data(), (int)firm.size(),
                      note.data(), (int)note.size(), &syncReady);
    s.loaded = true;
    s.bound = rc == 1;
    s.syncReady = syncReady != 0;
    s.hubUrl = widenUtf8(hub.data());
    s.firmDbPath = widenUtf8(firm.data());
    s.note = widenUtf8(note.data());
    if (s.note.empty())
        s.note = s.bound ? L"Licence in firm.db (Aorms.Bridge)" : L"Unbound — Activate in AORMS Connect";
    return s;
}

bool flush(std::wstring& note)
{
    std::lock_guard lock(g_mu);
    if (!loadDll() || !g_flush)
    {
        note = L"aadt_bridge.dll not loaded — run bridge\\build-bridge.cmd";
        return false;
    }
    std::array<char, 512> err{};
    int rc = g_flush(err.data(), (int)err.size());
    note = widenUtf8(err.data());
    if (rc == -2)
    {
        if (note.empty()) note = L"Flush skipped";
        note = L"Flush skipped=" + note + L" — Activate in AORMS Connect first";
        return false;
    }
    if (rc < 0)
    {
        if (note.empty()) note = L"Flush failed";
        return false;
    }
    if (note.empty())
        note = L"Flush OK";
    return true;
}

long long enqueuePing()
{
    std::lock_guard lock(g_mu);
    if (!loadDll() || !g_enqueue) return -1;
    return g_enqueue();
}

void shutdown()
{
    std::lock_guard lock(g_mu);
    if (g_shutdown) g_shutdown();
    if (g_mod)
    {
        FreeLibrary(g_mod);
        g_mod = nullptr;
    }
    g_ensure = nullptr;
    g_status = nullptr;
    g_flush = nullptr;
    g_enqueue = nullptr;
    g_shutdown = nullptr;
    g_loadAttempted = false;
}

} // namespace aadt::bridge
