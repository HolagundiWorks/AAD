// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet
//
// Read-only AORMS Connect session.json consumer (licence SSO).
// Activate stays in AORMS Connect — AADT never writes tokens.
#pragma once

#include <string>

namespace aadt::connect {

struct LicenceStatus
{
    bool bound = false;
    std::wstring path;
    std::wstring hubUrl;
    std::wstring syncTokenPreview; // first 8 chars only for status UI
    std::wstring note;
};

/// Default: %LocalAppData%\AORMS-Connect\session.json
std::wstring defaultSessionPath();

/// Prefer --connect-session <path> from process command line, else default.
std::wstring resolveSessionPath();

/// Parse session.json; bound=true only when syncToken present and not expired.
LicenceStatus tryReadSession(std::wstring const& path = {});

/// Short status line for chrome: "Licensed · Connect" / "Unbound · Connect".
std::wstring statusChipText(LicenceStatus const& s);

} // namespace aadt::connect
