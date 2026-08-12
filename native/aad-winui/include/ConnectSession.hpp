// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet
//
// Read-only AORMS Connect session.json + catalog.json consumer.
// Activate stays in AORMS Connect — AADT never writes tokens.
#pragma once

#include <string>
#include <vector>

namespace aadt::connect {

struct LicenceStatus
{
    bool bound = false;
    std::wstring path;
    std::wstring hubUrl;
    std::wstring syncTokenPreview; // first 8 chars only for status UI
    std::wstring note;
};

struct CatalogProject
{
    std::wstring id;
    std::wstring ref;
    std::wstring title;
    std::wstring status;
};

struct FirmBranding
{
    std::wstring firmName;   // display name (catalog title or fallback)
    std::wstring projectRef;
    std::wstring logoPath;   // absolute path if a logo file was found
    std::wstring tip;        // tooltip (hub / licence note)
};

/// Default: %LocalAppData%\AORMS-Connect\session.json
std::wstring defaultSessionPath();

/// %LocalAppData%\AORMS-Connect\catalog.json
std::wstring defaultCatalogPath();

/// Prefer --connect-session <path> from process command line, else default.
std::wstring resolveSessionPath();

/// Parse session.json; bound=true only when syncToken present and not expired.
LicenceStatus tryReadSession(std::wstring const& path = {});

/// Short status line for chrome: "Licensed · Connect" / "Unbound · Connect".
std::wstring statusChipText(LicenceStatus const& s);

/// Read Connect catalog.json projects (empty if missing).
std::vector<CatalogProject> listCatalog(std::wstring const& path = {});

/// Firm chip fields: catalog primary title + probed logo under Connect/AADT dirs.
FirmBranding readFirmBranding();

} // namespace aadt::connect
