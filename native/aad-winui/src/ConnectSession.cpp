// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet

#include "ConnectSession.hpp"

#include <Windows.h>
#include <shellapi.h>

#include <fstream>
#include <optional>
#include <sstream>
#include <vector>

namespace aadt::connect {
namespace {

std::wstring widen(std::string const& s)
{
    if (s.empty()) return {};
    int n = MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), nullptr, 0);
    if (n <= 0) return {};
    std::wstring out((size_t)n, L'\0');
    MultiByteToWideChar(CP_UTF8, 0, s.c_str(), (int)s.size(), out.data(), n);
    return out;
}

std::optional<std::string> jsonStringField(std::string const& json, char const* key)
{
    std::string needle = std::string("\"") + key + "\"";
    auto pos = json.find(needle);
    if (pos == std::string::npos) return std::nullopt;
    pos = json.find(':', pos + needle.size());
    if (pos == std::string::npos) return std::nullopt;
    pos = json.find('"', pos + 1);
    if (pos == std::string::npos) return std::nullopt;
    ++pos;
    auto end = json.find('"', pos);
    if (end == std::string::npos) return std::nullopt;
    return json.substr(pos, end - pos);
}

bool expired(std::string const& expiresAt)
{
    if (expiresAt.empty() || expiresAt.size() < 19) return false;
    SYSTEMTIME st{};
    try
    {
        st.wYear = (WORD)std::stoi(expiresAt.substr(0, 4));
        st.wMonth = (WORD)std::stoi(expiresAt.substr(5, 2));
        st.wDay = (WORD)std::stoi(expiresAt.substr(8, 2));
        st.wHour = (WORD)std::stoi(expiresAt.substr(11, 2));
        st.wMinute = (WORD)std::stoi(expiresAt.substr(14, 2));
        st.wSecond = (WORD)std::stoi(expiresAt.substr(17, 2));
    }
    catch (...)
    {
        return false;
    }
    FILETIME ft{};
    if (!SystemTimeToFileTime(&st, &ft)) return false;
    ULARGE_INTEGER exp{};
    exp.LowPart = ft.dwLowDateTime;
    exp.HighPart = ft.dwHighDateTime;

    FILETIME nowFt{};
    GetSystemTimeAsFileTime(&nowFt);
    ULARGE_INTEGER now{};
    now.LowPart = nowFt.dwLowDateTime;
    now.HighPart = nowFt.dwHighDateTime;
    return now.QuadPart > exp.QuadPart;
}

std::vector<std::wstring> commandLineArgs()
{
    int argc = 0;
    LPWSTR* argv = CommandLineToArgvW(GetCommandLineW(), &argc);
    std::vector<std::wstring> out;
    if (!argv) return out;
    out.reserve((size_t)argc);
    for (int i = 0; i < argc; ++i)
        out.emplace_back(argv[i]);
    LocalFree(argv);
    return out;
}

} // namespace

std::wstring defaultSessionPath()
{
    wchar_t local[MAX_PATH]{};
    DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", local, MAX_PATH);
    if (n == 0 || n >= MAX_PATH) return {};
    return std::wstring(local) + L"\\AORMS-Connect\\session.json";
}

std::wstring resolveSessionPath()
{
    auto args = commandLineArgs();
    for (size_t i = 0; i + 1 < args.size(); ++i)
    {
        if (_wcsicmp(args[i].c_str(), L"--connect-session") == 0)
        {
            auto p = args[i + 1];
            while (!p.empty() && (p.front() == L'"' || p.front() == L'\''))
                p.erase(p.begin());
            while (!p.empty() && (p.back() == L'"' || p.back() == L'\''))
                p.pop_back();
            if (!p.empty()) return p;
        }
    }
    return defaultSessionPath();
}

LicenceStatus tryReadSession(std::wstring const& pathIn)
{
    LicenceStatus s;
    s.path = pathIn.empty() ? resolveSessionPath() : pathIn;
    if (s.path.empty())
    {
        s.note = L"No session path";
        return s;
    }

    // MSVC: ifstream accepts wchar_t paths (LOCALAPPDATA may be non-ASCII).
    std::ifstream in(s.path.c_str(), std::ios::binary);
    if (!in)
    {
        s.note = L"No Connect session.json — Activate in AORMS Connect";
        return s;
    }
    std::ostringstream oss;
    oss << in.rdbuf();
    auto json = oss.str();

    auto sync = jsonStringField(json, "syncToken");
    if (!sync || sync->empty())
    {
        s.note = L"session.json missing syncToken";
        return s;
    }
    auto exp = jsonStringField(json, "expiresAt");
    if (exp && expired(*exp))
    {
        s.note = L"Connect session expired — Activate again in Connect";
        return s;
    }

    s.bound = true;
    if (auto hub = jsonStringField(json, "hubUrl"))
        s.hubUrl = widen(*hub);
    if (sync->size() > 8)
        s.syncTokenPreview = widen(sync->substr(0, 8)) + L"…";
    else
        s.syncTokenPreview = widen(*sync);
    s.note = L"Licence from AORMS Connect";
    return s;
}

std::wstring statusChipText(LicenceStatus const& s)
{
    if (s.bound)
    {
        if (!s.hubUrl.empty())
            return L"Licensed · Connect · " + s.hubUrl;
        return L"Licensed · Connect";
    }
    return L"Unbound · Connect";
}

} // namespace aadt::connect
