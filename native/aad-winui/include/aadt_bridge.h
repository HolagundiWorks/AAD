// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Human Centric Works, Hospet
//
// C ABI for aadt_bridge.dll (Aorms.Bridge NativeAOT). Activate is not exposed.
#pragma once

#ifdef __cplusplus
extern "C" {
#endif

/** Create firm.db + import Connect session. 1=bound, 0=unbound, -1=error. */
int aadt_bridge_ensure(void);

/**
 * Refresh status into UTF-8 buffers. Returns 1 if syncToken present, 0 unbound, -1 error.
 * sync_ready may be NULL.
 */
int aadt_bridge_status(
    char* hub_utf8, int hub_cch,
    char* firm_db_utf8, int firm_db_cch,
    char* note_utf8, int note_cch,
    int* sync_ready);

/** Flush outboxes. >=0 packed counts, -2 skipped (reason in err), -1 error. */
int aadt_bridge_flush(char* err_utf8, int err_cch);

long long aadt_bridge_enqueue_ping(void);

void aadt_bridge_shutdown(void);

#ifdef __cplusplus
}
#endif
