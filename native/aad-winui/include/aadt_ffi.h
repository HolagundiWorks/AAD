/* aadt_ffi.h — C ABI for WinUI / Direct2D host (ADR-0017 / M16–M24).
 * Link against aadt_ffi.dll built from crates/aadt-ffi.
 * ABI version: aadt_ffi_version() == 16
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
AADT_API void aadt_session_reset(AadtSession* session);

/* Returns 0 on success/silent, 1 on error. */
AADT_API int32_t aadt_session_feed(AadtSession* session, const char* line);
AADT_API const char* aadt_last_feedback(const AadtSession* session);

AADT_API uint64_t aadt_entity_count(const AadtSession* session);
AADT_API uint64_t aadt_selection_count(const AadtSession* session);
AADT_API uint64_t aadt_document_revision(const AadtSession* session);
AADT_API int32_t aadt_is_dirty(const AadtSession* session);

/* Copy up to max_ids selected entity ids. Returns number written. */
AADT_API uint64_t aadt_selection_copy_ids(const AadtSession* session, uint64_t* out_ids,
                                          uint64_t max_ids);
/* Heap UTF-8 Properties: newline-separated "Label\tValue" rows (Type, Layer,
 * Color, Linetype, Lineweight, geometry/text fields…). Free with aadt_string_free. */
AADT_API char* aadt_entity_prop_summary(const AadtSession* session, uint64_t entity_id);
/* Pick nearest entity or dimension; SELECT id / SELECT NONE / dim highlight.
 * Returns 1 on hit (entity id in *out_id, or 0 for dimension), 0 on miss.
 * out_id may be null. */
AADT_API int32_t aadt_pick_at(AadtSession* session, double x, double y, double tol,
                              uint64_t* out_id);

/* Heap path or NULL — free with aadt_string_free. */
AADT_API char* aadt_session_path(const AadtSession* session);
AADT_API void aadt_string_free(char* s);

AADT_API uint64_t aadt_drawlist_len(AadtSession* session);
AADT_API uint64_t aadt_drawlist_rebuild_lines(AadtSession* session);
/* Stroke pack: x0,y0,x1,y1,lw_mm,aci per segment (stride 6). */
AADT_API uint64_t aadt_drawlist_copy_lines(const AadtSession* session, float* out_xy,
                                           uint64_t max_floats);
/* Hatch fills: n_pts, alpha, x0,y0… per ring. Call rebuild_lines first. */
AADT_API uint64_t aadt_drawlist_copy_fills(const AadtSession* session, float* out,
                                           uint64_t max_floats);
/* out4 = min_x, min_y, max_x, max_y — returns 1 if non-empty. */
AADT_API int32_t aadt_drawlist_bounds(const AadtSession* session, float* out4);
/* Heap nav hint after feed ('ZOOM / ZOOM / PAN). Free with aadt_string_free. */
AADT_API char* aadt_nav_take(AadtSession* session);

/* --- M16/M18 interactive tools --- */
AADT_API int32_t aadt_tool_start(AadtSession* session, const char* cmd);
AADT_API int32_t aadt_tool_start_insert(AadtSession* session, const char* block_name);
AADT_API int32_t aadt_tool_accept_point(AadtSession* session, double x, double y);
AADT_API int32_t aadt_tool_finish(AadtSession* session);
AADT_API int32_t aadt_tool_set_text(AadtSession* session, const char* value);
AADT_API int32_t aadt_tool_needs_text(const AadtSession* session);
AADT_API void aadt_tool_set_cursor(AadtSession* session, double x, double y);
AADT_API void aadt_tool_cancel(AadtSession* session);
AADT_API const char* aadt_tool_prompt(const AadtSession* session);
AADT_API int32_t aadt_tool_active(const AadtSession* session);
AADT_API uint64_t aadt_tool_preview_rebuild_lines(AadtSession* session);
AADT_API uint64_t aadt_tool_preview_copy_lines(const AadtSession* session, float* out_xy,
                                               uint64_t max_floats);

/* --- M19 osnap + box select --- */
AADT_API void aadt_osnap_set(AadtSession* session, int32_t on);
/* Running snap kinds bitmask: END=1 INT=2 MID=4 CEN=8 QUA=16 NEA=32 PER=64 TAN=128 */
AADT_API uint32_t aadt_osnap_modes_get(const AadtSession* session);
AADT_API void aadt_osnap_modes_set(AadtSession* session, uint32_t modes);
AADT_API int32_t aadt_osnap_marker(const AadtSession* session, double* out2);
AADT_API uint64_t aadt_select_box(AadtSession* session, double x0, double y0, double x1,
                                  double y1);
AADT_API uint64_t aadt_selection_dim_count(const AadtSession* session);
AADT_API void aadt_selection_clear(AadtSession* session);

/* --- M18 Layers / Blocks / catalog --- */
/* Heap TSV: name\taci\ton\tfrozen\tlocked\tcurrent\tlinetype\tlw_mm. Free with aadt_string_free. */
AADT_API char* aadt_layers_list(const AadtSession* session);
AADT_API int32_t aadt_layer_set_current(AadtSession* session, const char* name);
AADT_API const char* aadt_layer_current(const AadtSession* session);
/* Heap newline-separated block names. Free with aadt_string_free. */
AADT_API char* aadt_blocks_list(const AadtSession* session);
/* Heap TOKEN\talias1,alias2 lines (all catalog + INSERT). Free with aadt_string_free. */
AADT_API char* aadt_catalog_tools(void);

/* --- M20 command line / Elements / Fills / OTRACK / grips --- */
/* Heap newline-separated completion candidates for prefix. Free with aadt_string_free. */
AADT_API char* aadt_complete(const AadtSession* session, const char* prefix);
/* 1 if token resolves to an interactive tool (bare alias). */
AADT_API int32_t aadt_is_interactive(const char* cmd);
/* Heap elements catalog: name\tkind\tcategory\tdisplay\tpath. Free with aadt_string_free. */
AADT_API char* aadt_elements_catalog(void);
/* Seed place blocks / ensure named block exists. Returns 0 ok. */
AADT_API int32_t aadt_element_ensure(AadtSession* session, const char* name);
/* Heap hatch patterns: name\tkind\tdescription. Free with aadt_string_free. */
AADT_API char* aadt_hatch_patterns_list(void);
AADT_API void aadt_hatch_set_pattern(AadtSession* session, const char* name);
AADT_API const char* aadt_hatch_pattern(const AadtSession* session);

AADT_API void aadt_otrack_set(AadtSession* session, int32_t on);
AADT_API void aadt_otrack_clear(AadtSession* session);
/* Ray segments for overlay: ox,oy,dx,dy floats (dir unit). Returns float count. */
AADT_API uint64_t aadt_otrack_rays_rebuild(AadtSession* session);
AADT_API uint64_t aadt_otrack_rays_copy(const AadtSession* session, float* out, uint64_t max_floats);

/* Grips for current selection: id,idx,x,y as float quads. */
AADT_API uint64_t aadt_grips_rebuild(AadtSession* session);
AADT_API uint64_t aadt_grips_copy(const AadtSession* session, float* out, uint64_t max_floats);
/* Pick grip within tol; returns 1 and writes *out_id / *out_idx (sets hot grip). */
AADT_API int32_t aadt_grip_pick(AadtSession* session, double x, double y, double tol,
                                uint64_t* out_id, uint64_t* out_idx);
/* Hot grip world x,y — returns 1 if set. */
AADT_API int32_t aadt_hot_grip(const AadtSession* session, double* out2);
/* Commit GRIPEDIT. Returns 0 ok. */
AADT_API int32_t aadt_grip_move(AadtSession* session, uint64_t id, uint64_t grip_idx, double nx,
                                double ny);

/* --- M21 selection / ortho / polar / tool numbers --- */
AADT_API uint64_t aadt_selection_rebuild_lines(AadtSession* session);
AADT_API uint64_t aadt_selection_copy_lines(const AadtSession* session, float* out,
                                            uint64_t max_floats);
AADT_API void aadt_ortho_set(AadtSession* session, int32_t on);
AADT_API int32_t aadt_ortho_get(const AadtSession* session);
AADT_API void aadt_polar_set(AadtSession* session, int32_t on);
AADT_API int32_t aadt_polar_get(const AadtSession* session);
AADT_API int32_t aadt_tool_set_number(AadtSession* session, double value);

/* --- M22 remaining gaps --- */
AADT_API int32_t aadt_tool_start_image(AadtSession* session, const char* path);
AADT_API void aadt_place_flip_set(AadtSession* session, int32_t on);
AADT_API int32_t aadt_place_flip_get(const AadtSession* session);
AADT_API void aadt_pick_add_set(AadtSession* session, int32_t on);
AADT_API void aadt_grip_mode_cycle(AadtSession* session);
AADT_API const char* aadt_grip_mode(const AadtSession* session);

/* --- M24 plot / PDF --- */
/* Compose sheet (extents) → write PDF. paper null/"" → A3. Returns 0 ok. */
AADT_API int32_t aadt_plot_pdf_to_path(AadtSession* session, const char* path,
                                       const char* paper);
/* Place sheet border on live document (PLOTSETUP peer). Returns 0 ok. */
AADT_API int32_t aadt_plot_setup(AadtSession* session, const char* paper);

/* --- Drawing properties (ABI 14) --- */
/* Heap UTF-8 key\tvalue rows. Free with aadt_string_free. */
AADT_API char* aadt_drawing_props_get(const AadtSession* session);
/* Apply key\tvalue rows. Returns 0 ok. */
AADT_API int32_t aadt_drawing_props_set(AadtSession* session, const char* pack);

/* --- Local ShilpiDB work DB (ABI 15) --- */
/* Force tip → %LocalAppData%\AADT\work\active.vdb (or OPEN .vdb). 0 = ok. */
AADT_API int32_t aadt_workdb_flush(AadtSession* session);
/* Realtime sync OFF — no-op. Use flush / COMMIT / SAVE for disk writes. */
AADT_API int32_t aadt_workdb_tick(AadtSession* session);
/* Heap UTF-8 work .vdb path. Free with aadt_string_free. */
AADT_API char* aadt_workdb_path(const AadtSession* session);

#ifdef __cplusplus
}
#endif
