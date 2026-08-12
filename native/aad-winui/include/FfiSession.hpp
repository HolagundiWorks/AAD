#pragma once

#include <windows.h>

#include "aadt_ffi.h"

#include <string>
#include <vector>

/// RAII wrapper: LoadLibrary("aadt_ffi.dll") + Session (ADR-0017 / M16–M23).
class FfiSession {
public:
  FfiSession();
  ~FfiSession();

  FfiSession(const FfiSession&) = delete;
  FfiSession& operator=(const FfiSession&) = delete;

  bool ok() const { return session_ != nullptr; }
  std::string loadError() const { return loadError_; }

  bool feed(const std::string& line);
  std::string lastFeedback() const;
  void reset();

  uint64_t entityCount() const;
  uint64_t selectionCount() const;
  uint64_t revision() const;
  bool dirty() const;

  std::vector<uint64_t> selectionIds() const;
  /// Selection overlay segments (x0,y0,x1,y1 floats).
  const std::vector<float>& selectionRebuildLines();
  /// "KIND\\tlayer\\tspace" or empty.
  std::string entityPropSummary(uint64_t id) const;
  /// World-space pick → SELECT. Returns true on hit.
  bool pickAt(double x, double y, double tol, uint64_t* outId = nullptr);

  bool toolStart(const std::string& cmd);
  bool toolStartInsert(const std::string& blockName);
  /// IMAGE place tool with filesystem path (ABI v9).
  bool toolStartImage(const std::string& path);
  bool toolAcceptPoint(double x, double y);
  bool toolFinish();
  bool toolSetText(const std::string& value);
  bool toolSetNumber(double value);
  bool toolNeedsText() const;
  void toolSetCursor(double x, double y);
  void toolCancel();
  bool toolActive() const;
  std::string toolPrompt() const;

  /// Door/window swing flip (Tab) — ABI v9.
  void placeFlipSet(bool on);
  bool placeFlipGet() const;
  /// Shift+pick additive select — ABI v9.
  void pickAddSet(bool on);
  /// Cycle grip edit mode (Stretch/Move/…) — ABI v9.
  void gripModeCycle();
  std::string gripMode() const;

  /// Compose extents sheet → write PDF (`paper` empty → A3).
  bool plotPdfToPath(const std::string& path, const std::string& paper);
  /// Place sheet border (PLOTSETUP peer).
  bool plotSetup(const std::string& paper);

  void osnapSet(bool on);
  bool osnapMarker(double out2[2]) const;
  void orthoSet(bool on);
  bool orthoGet() const;
  void polarSet(bool on);
  bool polarGet() const;
  uint64_t selectBox(double x0, double y0, double x1, double y1);

  /// TSV lines: name\taci\ton\tfrozen\tlocked\tcurrent
  std::string layersList() const;
  bool layerSetCurrent(const std::string& name);
  std::string layerCurrent() const;
  /// Newline-separated block names.
  std::string blocksList() const;
  std::string catalogTools() const;

  /// Completion candidates for prefix (newline-separated).
  std::string complete(const std::string& prefix) const;
  /// True if bare token is an interactive tool.
  bool isInteractive(const std::string& cmd) const;
  /// Elements catalog TSV: name\tkind\tcategory\tdisplay\tpath
  std::string elementsCatalog() const;
  bool elementEnsure(const std::string& name);
  /// Hatch patterns TSV: name\tkind\tdescription
  std::string hatchPatternsList() const;
  void hatchSetPattern(const std::string& name);
  std::string hatchPattern() const;

  void otrackSet(bool on);
  void otrackClear();
  const std::vector<float>& rebuildOtrackRays();
  const std::vector<float>& rebuildGrips();
  bool gripPick(double x, double y, double tol, uint64_t* outId, uint64_t* outIdx);
  bool gripMove(uint64_t id, uint64_t gripIdx, double nx, double ny);

  std::string sessionPath() const;

  /// Drawing properties pack: key\\tvalue lines (ABI 14).
  std::string drawingPropsGet() const;
  bool drawingPropsSet(const std::string& pack);

  const std::vector<float>& rebuildLines();
  /// Hatch fill pack: n_pts, alpha, x0,y0… (rebuilt with lines).
  const std::vector<float>& rebuildFills();
  const std::vector<float>& rebuildPreview();
  bool bounds(float out4[4]) const;
  /// Consume pending nav hint after feed (`extents`, `zoom_window`, …).
  std::string takeNav();

private:
  bool loadDll();
  void unload();
  std::string takeHeapString(char* s) const;

  HMODULE module_ = nullptr;
  AadtSession* session_ = nullptr;
  std::string loadError_;
  std::vector<float> lines_;
  std::vector<float> fills_;
  std::vector<float> preview_;
  std::vector<float> selection_;
  std::vector<float> otrackRays_;
  std::vector<float> grips_;

  decltype(&aadt_ffi_version) fn_version_ = nullptr;
  decltype(&aadt_session_new) fn_new_ = nullptr;
  decltype(&aadt_session_free) fn_free_ = nullptr;
  decltype(&aadt_session_reset) fn_reset_ = nullptr;
  decltype(&aadt_session_feed) fn_feed_ = nullptr;
  decltype(&aadt_last_feedback) fn_feedback_ = nullptr;
  decltype(&aadt_entity_count) fn_entities_ = nullptr;
  decltype(&aadt_selection_count) fn_sel_count_ = nullptr;
  decltype(&aadt_selection_copy_ids) fn_sel_copy_ = nullptr;
  decltype(&aadt_entity_prop_summary) fn_prop_summary_ = nullptr;
  decltype(&aadt_pick_at) fn_pick_ = nullptr;
  decltype(&aadt_string_free) fn_string_free_ = nullptr;
  decltype(&aadt_document_revision) fn_revision_ = nullptr;
  decltype(&aadt_is_dirty) fn_dirty_ = nullptr;
  decltype(&aadt_session_path) fn_path_ = nullptr;
  decltype(&aadt_drawlist_rebuild_lines) fn_rebuild_ = nullptr;
  decltype(&aadt_drawlist_copy_lines) fn_copy_ = nullptr;
  decltype(&aadt_drawlist_copy_fills) fn_copy_fills_ = nullptr;
  decltype(&aadt_drawlist_bounds) fn_bounds_ = nullptr;
  decltype(&aadt_nav_take) fn_nav_take_ = nullptr;
  decltype(&aadt_tool_start) fn_tool_start_ = nullptr;
  decltype(&aadt_tool_start_insert) fn_tool_start_insert_ = nullptr;
  decltype(&aadt_tool_accept_point) fn_tool_accept_ = nullptr;
  decltype(&aadt_tool_finish) fn_tool_finish_ = nullptr;
  decltype(&aadt_tool_set_text) fn_tool_set_text_ = nullptr;
  decltype(&aadt_tool_needs_text) fn_tool_needs_text_ = nullptr;
  decltype(&aadt_tool_set_cursor) fn_tool_cursor_ = nullptr;
  decltype(&aadt_tool_cancel) fn_tool_cancel_ = nullptr;
  decltype(&aadt_tool_prompt) fn_tool_prompt_ = nullptr;
  decltype(&aadt_tool_active) fn_tool_active_ = nullptr;
  decltype(&aadt_tool_preview_rebuild_lines) fn_preview_rebuild_ = nullptr;
  decltype(&aadt_tool_preview_copy_lines) fn_preview_copy_ = nullptr;
  decltype(&aadt_osnap_set) fn_osnap_set_ = nullptr;
  decltype(&aadt_osnap_marker) fn_osnap_marker_ = nullptr;
  decltype(&aadt_select_box) fn_select_box_ = nullptr;
  decltype(&aadt_layers_list) fn_layers_list_ = nullptr;
  decltype(&aadt_layer_set_current) fn_layer_set_current_ = nullptr;
  decltype(&aadt_layer_current) fn_layer_current_ = nullptr;
  decltype(&aadt_blocks_list) fn_blocks_list_ = nullptr;
  decltype(&aadt_catalog_tools) fn_catalog_tools_ = nullptr;
  decltype(&aadt_complete) fn_complete_ = nullptr;
  decltype(&aadt_is_interactive) fn_is_interactive_ = nullptr;
  decltype(&aadt_elements_catalog) fn_elements_catalog_ = nullptr;
  decltype(&aadt_element_ensure) fn_element_ensure_ = nullptr;
  decltype(&aadt_hatch_patterns_list) fn_hatch_patterns_list_ = nullptr;
  decltype(&aadt_hatch_set_pattern) fn_hatch_set_pattern_ = nullptr;
  decltype(&aadt_hatch_pattern) fn_hatch_pattern_ = nullptr;
  decltype(&aadt_otrack_set) fn_otrack_set_ = nullptr;
  decltype(&aadt_otrack_clear) fn_otrack_clear_ = nullptr;
  decltype(&aadt_otrack_rays_rebuild) fn_otrack_rays_rebuild_ = nullptr;
  decltype(&aadt_otrack_rays_copy) fn_otrack_rays_copy_ = nullptr;
  decltype(&aadt_grips_rebuild) fn_grips_rebuild_ = nullptr;
  decltype(&aadt_grips_copy) fn_grips_copy_ = nullptr;
  decltype(&aadt_grip_pick) fn_grip_pick_ = nullptr;
  decltype(&aadt_grip_move) fn_grip_move_ = nullptr;
  decltype(&aadt_selection_rebuild_lines) fn_sel_rebuild_ = nullptr;
  decltype(&aadt_selection_copy_lines) fn_sel_copy_lines_ = nullptr;
  decltype(&aadt_ortho_set) fn_ortho_set_ = nullptr;
  decltype(&aadt_ortho_get) fn_ortho_get_ = nullptr;
  decltype(&aadt_polar_set) fn_polar_set_ = nullptr;
  decltype(&aadt_polar_get) fn_polar_get_ = nullptr;
  decltype(&aadt_tool_set_number) fn_tool_set_number_ = nullptr;
  decltype(&aadt_tool_start_image) fn_tool_start_image_ = nullptr;
  decltype(&aadt_place_flip_set) fn_place_flip_set_ = nullptr;
  decltype(&aadt_place_flip_get) fn_place_flip_get_ = nullptr;
  decltype(&aadt_pick_add_set) fn_pick_add_set_ = nullptr;
  decltype(&aadt_grip_mode_cycle) fn_grip_mode_cycle_ = nullptr;
  decltype(&aadt_grip_mode) fn_grip_mode_ = nullptr;
  decltype(&aadt_plot_pdf_to_path) fn_plot_pdf_ = nullptr;
  decltype(&aadt_plot_setup) fn_plot_setup_ = nullptr;
  decltype(&aadt_drawing_props_get) fn_drawing_props_get_ = nullptr;
  decltype(&aadt_drawing_props_set) fn_drawing_props_set_ = nullptr;
};
