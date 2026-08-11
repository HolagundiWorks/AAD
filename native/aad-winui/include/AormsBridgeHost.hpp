// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet
//
// Load aadt_bridge.dll (Aorms.Bridge) beside the exe — firm.db under %LocalAppData%\AADT.
#pragma once

#include "ConnectSession.hpp"

#include <string>

namespace aadt::bridge {

struct Status
{
    bool loaded = false;
    bool bound = false;
    bool syncReady = false;
    std::wstring hubUrl;
    std::wstring firmDbPath;
    std::wstring note;
};

/// Load DLL (once), ensure firm.db, import Connect session, return status for chrome.
Status ensureAndStatus();

/// Flush outboxes; note holds result / skip reason.
bool flush(std::wstring& note);

/// Enqueue aadt.ping meta smoke row; returns outbox id or -1.
long long enqueuePing();

void shutdown();

} // namespace aadt::bridge
