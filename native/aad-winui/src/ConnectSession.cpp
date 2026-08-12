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

std::wstring localAppData()
{
    wchar_t local[MAX_PATH]{};
    DWORD n = GetEnvironmentVariableW(L"LOCALAPPDATA", local, MAX_PATH);
    if (n == 0 || n >= MAX_PATH) return {};
    return local;
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

bool fileExists(std::wstring const& path)
{
    if (path.empty()) return false;
    DWORD attrs = GetFileAttributesW(path.c_str());
    return attrs != INVALID_FILE_ATTRIBUTES
        && (attrs & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

std::wstring probeLogo()
{
    auto base = localAppData();
    if (base.empty()) return {};
    static wchar_t const* names[] = {
        L"\\AORMS-Connect\\logo.png",
        L"\\AORMS-Connect\\firm-logo.png",
        L"\\AORMS-Connect\\branding\\logo.png",
        L"\\AADT\\logo.png",
        L"\\AADT\\firm-logo.png",
    };
    for (auto rel : names)
    {
        auto p = base + rel;
        if (fileExists(p)) return p;
    }
    return {};
}

} // namespace

std::wstring defaultSessionPath()
{
    auto local = localAppData();
    if (local.empty()) return {};
    return local + L"\\AORMS-Connect\\session.json";
}

std::wstring defaultCatalogPath()
{
    auto local = localAppData();
    if (local.empty()) return {};
    return local + L"\\AORMS-Connect\\catalog.json";
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

std::vector<CatalogProject> listCatalog(std::wstring const& pathIn)
{
    std::vector<CatalogProject> out;
    auto path = pathIn.empty() ? defaultCatalogPath() : pathIn;
    if (path.empty()) return out;
    std::ifstream in(path.c_str(), std::ios::binary);
    if (!in) return out;
    std::ostringstream oss;
    oss << in.rdbuf();
    auto json = oss.str();

    // Naive array walk: find each {"…"} object and pull title/ref/id/status.
    size_t pos = 0;
    while (pos < json.size())
    {
        auto start = json.find('{', pos);
        if (start == std::string::npos) break;
        auto end = json.find('}', start);
        if (end == std::string::npos) break;
        auto obj = json.substr(start, end - start + 1);
        pos = end + 1;
        auto title = jsonStringField(obj, "title");
        if (!title || title->empty()) continue;
        CatalogProject p;
        p.title = widen(*title);
        if (auto id = jsonStringField(obj, "id")) p.id = widen(*id);
        if (auto ref = jsonStringField(obj, "ref")) p.ref = widen(*ref);
        if (auto st = jsonStringField(obj, "status")) p.status = widen(*st);
        out.push_back(std::move(p));
    }
    return out;
}

FirmBranding readFirmBranding()
{
    FirmBranding b;
    auto lic = tryReadSession();
    b.tip = statusChipText(lic);
    if (!lic.hubUrl.empty())
        b.tip += L"\n" + lic.hubUrl;
    if (!lic.note.empty())
        b.tip += L"\n" + lic.note;

    auto projects = listCatalog();
    if (!projects.empty())
    {
        b.firmName = projects.front().title;
        b.projectRef = projects.front().ref;
        if (!b.projectRef.empty())
            b.tip += L"\nCatalog · " + b.projectRef;
    }
    if (b.firmName.empty())
        b.firmName = L"AORMS";

    b.logoPath = probeLogo();
    if (!b.logoPath.empty())
        b.tip += L"\nLogo · " + b.logoPath;
    return b;
}

} // namespace aadt::connect
