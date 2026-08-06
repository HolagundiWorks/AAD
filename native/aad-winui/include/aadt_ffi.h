/* aadt_ffi.h — C ABI for WinUI / Direct2D host (ADR-0017).
 * Link against aadt_ffi.dll built from crates/aadt-ffi.
 * ABI version: aadt_ffi_version() == 2
 */
#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#ifdef _WIN32
#ifdef AADT_FFI_EXPORTS
#define AADT_API __declspec(dllexport)
#elif defined(AADT_FFI_DYNAMIC)
#define AADT_API /* declarations only — LoadLibrary */
#else
#define AADT_API __declspec(dllimport)
#endif
#else
#define AADT_API
#endif

typedef struct AadtSession AadtSession;

AADT_API uint32_t aadt_ffi_version(void);

AADT_API AadtSession* aadt_session_new(void);
AADT_API void aadt_session_free(AadtSession* session);

/* Returns 0 on success/silent, 1 on error. */
AADT_API int32_t aadt_session_feed(AadtSession* session, const char* line);
AADT_API const char* aadt_last_feedback(const AadtSession* session);

AADT_API uint64_t aadt_entity_count(const AadtSession* session);
AADT_API uint64_t aadt_selection_count(const AadtSession* session);
AADT_API uint64_t aadt_document_revision(const AadtSession* session);
AADT_API int32_t aadt_is_dirty(const AadtSession* session);

/* Heap path or NULL — free with aadt_string_free. */
AADT_API char* aadt_session_path(const AadtSession* session);
AADT_API void aadt_string_free(char* s);

AADT_API uint64_t aadt_drawlist_len(AadtSession* session);
AADT_API uint64_t aadt_drawlist_rebuild_lines(AadtSession* session);
AADT_API uint64_t aadt_drawlist_copy_lines(const AadtSession* session, float* out_xy,
                                           uint64_t max_floats);
/* out4 = min_x, min_y, max_x, max_y — returns 1 if non-empty. */
AADT_API int32_t aadt_drawlist_bounds(const AadtSession* session, float* out4);

#ifdef __cplusplus
}
#endif
