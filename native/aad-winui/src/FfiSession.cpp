#include "FfiSession.hpp"

#include <windows.h>

namespace {

template <typename T>
bool loadFn(HMODULE mod, const char* name, T* out, std::string* err) {
  FARPROC p = GetProcAddress(mod, name);
  if (!p) {
    *err = std::string("GetProcAddress failed: ") + name;
    return false;
  }
#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wcast-function-type"
#endif
  *out = reinterpret_cast<T>(p);
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif
  return true;
}

} // namespace

bool FfiSession::loadDll() {
  wchar_t path[MAX_PATH]{};
  GetModuleFileNameW(nullptr, path, MAX_PATH);
  std::wstring dir(path);
  const auto slash = dir.find_last_of(L"\\/");
  if (slash != std::wstring::npos) {
    dir.resize(slash + 1);
  }
  const std::wstring candidate = dir + L"aadt_ffi.dll";
  module_ = LoadLibraryW(candidate.c_str());
  if (!module_) {
    module_ = LoadLibraryW(L"aadt_ffi.dll");
  }
  if (!module_) {
    loadError_ = "LoadLibrary(aadt_ffi.dll) failed — copy DLL next to the exe";
    return false;
  }

  if (!loadFn(module_, "aadt_ffi_version", &fn_version_, &loadError_) ||
      !loadFn(module_, "aadt_session_new", &fn_new_, &loadError_) ||
      !loadFn(module_, "aadt_session_free", &fn_free_, &loadError_) ||
      !loadFn(module_, "aadt_session_reset", &fn_reset_, &loadError_) ||
      !loadFn(module_, "aadt_session_feed", &fn_feed_, &loadError_) ||
      !loadFn(module_, "aadt_last_feedback", &fn_feedback_, &loadError_) ||
      !loadFn(module_, "aadt_entity_count", &fn_entities_, &loadError_) ||
      !loadFn(module_, "aadt_selection_count", &fn_sel_count_, &loadError_) ||
      !loadFn(module_, "aadt_selection_copy_ids", &fn_sel_copy_, &loadError_) ||
      !loadFn(module_, "aadt_entity_prop_summary", &fn_prop_summary_, &loadError_) ||
      !loadFn(module_, "aadt_pick_at", &fn_pick_, &loadError_) ||
      !loadFn(module_, "aadt_string_free", &fn_string_free_, &loadError_) ||
      !loadFn(module_, "aadt_document_revision", &fn_revision_, &loadError_) ||
      !loadFn(module_, "aadt_is_dirty", &fn_dirty_, &loadError_) ||
      !loadFn(module_, "aadt_session_path", &fn_path_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_rebuild_lines", &fn_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_copy_lines", &fn_copy_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_copy_fills", &fn_copy_fills_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_bounds", &fn_bounds_, &loadError_) ||
      !loadFn(module_, "aadt_nav_take", &fn_nav_take_, &loadError_) ||
      !loadFn(module_, "aadt_tool_start", &fn_tool_start_, &loadError_) ||
      !loadFn(module_, "aadt_tool_start_insert", &fn_tool_start_insert_, &loadError_) ||
      !loadFn(module_, "aadt_tool_accept_point", &fn_tool_accept_, &loadError_) ||
      !loadFn(module_, "aadt_tool_finish", &fn_tool_finish_, &loadError_) ||
      !loadFn(module_, "aadt_tool_set_text", &fn_tool_set_text_, &loadError_) ||
      !loadFn(module_, "aadt_tool_needs_text", &fn_tool_needs_text_, &loadError_) ||
      !loadFn(module_, "aadt_tool_set_cursor", &fn_tool_cursor_, &loadError_) ||
      !loadFn(module_, "aadt_tool_cancel", &fn_tool_cancel_, &loadError_) ||
      !loadFn(module_, "aadt_tool_prompt", &fn_tool_prompt_, &loadError_) ||
      !loadFn(module_, "aadt_tool_active", &fn_tool_active_, &loadError_) ||
      !loadFn(module_, "aadt_tool_preview_rebuild_lines", &fn_preview_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_tool_preview_copy_lines", &fn_preview_copy_, &loadError_) ||
      !loadFn(module_, "aadt_osnap_set", &fn_osnap_set_, &loadError_) ||
      !loadFn(module_, "aadt_osnap_marker", &fn_osnap_marker_, &loadError_) ||
      !loadFn(module_, "aadt_select_box", &fn_select_box_, &loadError_) ||
      !loadFn(module_, "aadt_layers_list", &fn_layers_list_, &loadError_) ||
      !loadFn(module_, "aadt_layer_set_current", &fn_layer_set_current_, &loadError_) ||
      !loadFn(module_, "aadt_layer_current", &fn_layer_current_, &loadError_) ||
      !loadFn(module_, "aadt_blocks_list", &fn_blocks_list_, &loadError_) ||
      !loadFn(module_, "aadt_catalog_tools", &fn_catalog_tools_, &loadError_) ||
      !loadFn(module_, "aadt_complete", &fn_complete_, &loadError_) ||
      !loadFn(module_, "aadt_is_interactive", &fn_is_interactive_, &loadError_) ||
      !loadFn(module_, "aadt_elements_catalog", &fn_elements_catalog_, &loadError_) ||
      !loadFn(module_, "aadt_element_ensure", &fn_element_ensure_, &loadError_) ||
      !loadFn(module_, "aadt_hatch_patterns_list", &fn_hatch_patterns_list_, &loadError_) ||
      !loadFn(module_, "aadt_hatch_set_pattern", &fn_hatch_set_pattern_, &loadError_) ||
      !loadFn(module_, "aadt_hatch_pattern", &fn_hatch_pattern_, &loadError_) ||
      !loadFn(module_, "aadt_otrack_set", &fn_otrack_set_, &loadError_) ||
      !loadFn(module_, "aadt_otrack_clear", &fn_otrack_clear_, &loadError_) ||
      !loadFn(module_, "aadt_otrack_rays_rebuild", &fn_otrack_rays_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_otrack_rays_copy", &fn_otrack_rays_copy_, &loadError_) ||
      !loadFn(module_, "aadt_grips_rebuild", &fn_grips_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_grips_copy", &fn_grips_copy_, &loadError_) ||
      !loadFn(module_, "aadt_grip_pick", &fn_grip_pick_, &loadError_) ||
      !loadFn(module_, "aadt_grip_move", &fn_grip_move_, &loadError_) ||
      !loadFn(module_, "aadt_selection_rebuild_lines", &fn_sel_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_selection_copy_lines", &fn_sel_copy_lines_, &loadError_) ||
      !loadFn(module_, "aadt_ortho_set", &fn_ortho_set_, &loadError_) ||
      !loadFn(module_, "aadt_ortho_get", &fn_ortho_get_, &loadError_) ||
      !loadFn(module_, "aadt_polar_set", &fn_polar_set_, &loadError_) ||
      !loadFn(module_, "aadt_polar_get", &fn_polar_get_, &loadError_) ||
      !loadFn(module_, "aadt_tool_set_number", &fn_tool_set_number_, &loadError_) ||
      !loadFn(module_, "aadt_tool_start_image", &fn_tool_start_image_, &loadError_) ||
      !loadFn(module_, "aadt_place_flip_set", &fn_place_flip_set_, &loadError_) ||
      !loadFn(module_, "aadt_place_flip_get", &fn_place_flip_get_, &loadError_) ||
      !loadFn(module_, "aadt_pick_add_set", &fn_pick_add_set_, &loadError_) ||
      !loadFn(module_, "aadt_grip_mode_cycle", &fn_grip_mode_cycle_, &loadError_) ||
      !loadFn(module_, "aadt_grip_mode", &fn_grip_mode_, &loadError_) ||
      !loadFn(module_, "aadt_plot_pdf_to_path", &fn_plot_pdf_, &loadError_) ||
      !loadFn(module_, "aadt_plot_setup", &fn_plot_setup_, &loadError_) ||
      !loadFn(module_, "aadt_drawing_props_get", &fn_drawing_props_get_, &loadError_) ||
      !loadFn(module_, "aadt_drawing_props_set", &fn_drawing_props_set_, &loadError_)) {
    unload();
    return false;
  }

  if (fn_version_() < 14) {
    loadError_ = "aadt_ffi.dll ABI too old (need version >= 14 for drawing properties)";
    unload();
    return false;
  }
  return true;
}

void FfiSession::unload() {
  if (session_ && fn_free_) {
    fn_free_(session_);
    session_ = nullptr;
  }
  if (module_) {
    FreeLibrary(module_);
    module_ = nullptr;
  }
  fn_version_ = nullptr;
  fn_new_ = nullptr;
  fn_free_ = nullptr;
  fn_reset_ = nullptr;
  fn_feed_ = nullptr;
  fn_feedback_ = nullptr;
  fn_entities_ = nullptr;
  fn_sel_count_ = nullptr;
  fn_sel_copy_ = nullptr;
  fn_prop_summary_ = nullptr;
  fn_pick_ = nullptr;
  fn_string_free_ = nullptr;
  fn_revision_ = nullptr;
  fn_dirty_ = nullptr;
  fn_path_ = nullptr;
  fn_rebuild_ = nullptr;
  fn_copy_ = nullptr;
  fn_bounds_ = nullptr;
  fn_tool_start_ = nullptr;
  fn_tool_start_insert_ = nullptr;
  fn_tool_accept_ = nullptr;
  fn_tool_finish_ = nullptr;
  fn_tool_set_text_ = nullptr;
  fn_tool_needs_text_ = nullptr;
  fn_tool_cursor_ = nullptr;
  fn_tool_cancel_ = nullptr;
  fn_tool_prompt_ = nullptr;
  fn_tool_active_ = nullptr;
  fn_preview_rebuild_ = nullptr;
  fn_preview_copy_ = nullptr;
  fn_osnap_set_ = nullptr;
  fn_osnap_marker_ = nullptr;
  fn_select_box_ = nullptr;
  fn_layers_list_ = nullptr;
  fn_layer_set_current_ = nullptr;
  fn_layer_current_ = nullptr;
  fn_blocks_list_ = nullptr;
  fn_catalog_tools_ = nullptr;
  fn_complete_ = nullptr;
  fn_is_interactive_ = nullptr;
  fn_elements_catalog_ = nullptr;
  fn_element_ensure_ = nullptr;
  fn_hatch_patterns_list_ = nullptr;
  fn_hatch_set_pattern_ = nullptr;
  fn_hatch_pattern_ = nullptr;
  fn_otrack_set_ = nullptr;
  fn_otrack_clear_ = nullptr;
  fn_otrack_rays_rebuild_ = nullptr;
  fn_otrack_rays_copy_ = nullptr;
  fn_grips_rebuild_ = nullptr;
  fn_grips_copy_ = nullptr;
  fn_grip_pick_ = nullptr;
  fn_grip_move_ = nullptr;
  fn_sel_rebuild_ = nullptr;
  fn_sel_copy_lines_ = nullptr;
  fn_ortho_set_ = nullptr;
  fn_ortho_get_ = nullptr;
  fn_polar_set_ = nullptr;
  fn_polar_get_ = nullptr;
  fn_tool_set_number_ = nullptr;
  fn_tool_start_image_ = nullptr;
  fn_place_flip_set_ = nullptr;
  fn_place_flip_get_ = nullptr;
  fn_pick_add_set_ = nullptr;
  fn_grip_mode_cycle_ = nullptr;
  fn_grip_mode_ = nullptr;
  fn_plot_pdf_ = nullptr;
  fn_plot_setup_ = nullptr;
  fn_drawing_props_get_ = nullptr;
  fn_drawing_props_set_ = nullptr;
}

FfiSession::FfiSession() {
  if (!loadDll()) {
    return;
  }
  session_ = fn_new_();
  if (!session_) {
    loadError_ = "aadt_session_new returned null";
    unload();
  }
}

FfiSession::~FfiSession() { unload(); }

std::string FfiSession::takeHeapString(char* s) const {
  if (!s || !fn_string_free_) {
    return {};
  }
  std::string out(s);
  fn_string_free_(s);
  return out;
}

bool FfiSession::feed(const std::string& line) {
  if (!session_ || !fn_feed_) {
    return false;
  }
  return fn_feed_(session_, line.c_str()) == 0;
}

std::string FfiSession::lastFeedback() const {
  if (!session_ || !fn_feedback_) {
    return {};
  }
  const char* s = fn_feedback_(session_);
  return s ? std::string(s) : std::string();
}

void FfiSession::reset() {
  if (session_ && fn_reset_) {
    fn_reset_(session_);
  }
}

uint64_t FfiSession::entityCount() const {
  return (session_ && fn_entities_) ? fn_entities_(session_) : 0;
}

uint64_t FfiSession::selectionCount() const {
  return (session_ && fn_sel_count_) ? fn_sel_count_(session_) : 0;
}

uint64_t FfiSession::revision() const {
  return (session_ && fn_revision_) ? fn_revision_(session_) : 0;
}

bool FfiSession::dirty() const {
  return session_ && fn_dirty_ && fn_dirty_(session_) != 0;
}

std::vector<uint64_t> FfiSession::selectionIds() const {
  std::vector<uint64_t> ids;
  if (!session_ || !fn_sel_count_ || !fn_sel_copy_) {
    return ids;
  }
  const uint64_t n = fn_sel_count_(session_);
  if (n == 0) {
    return ids;
  }
  ids.resize(static_cast<size_t>(n));
  const uint64_t copied = fn_sel_copy_(session_, ids.data(), n);
  ids.resize(static_cast<size_t>(copied));
  return ids;
}

const std::vector<float>& FfiSession::selectionRebuildLines() {
  selection_.clear();
  if (!session_ || !fn_sel_rebuild_ || !fn_sel_copy_lines_) {
    return selection_;
  }
  const uint64_t n = fn_sel_rebuild_(session_);
  if (n == 0) {
    return selection_;
  }
  selection_.resize(static_cast<size_t>(n));
  fn_sel_copy_lines_(session_, selection_.data(), n);
  return selection_;
}

std::string FfiSession::entityPropSummary(uint64_t id) const {
  if (!session_ || !fn_prop_summary_) {
    return {};
  }
  return takeHeapString(fn_prop_summary_(session_, id));
}

bool FfiSession::pickAt(double x, double y, double tol, uint64_t* outId) {
  if (!session_ || !fn_pick_) {
    return false;
  }
  uint64_t id = 0;
  const bool hit = fn_pick_(session_, x, y, tol, &id) != 0;
  if (outId) {
    *outId = id;
  }
  return hit;
}

bool FfiSession::toolStart(const std::string& cmd) {
  if (!session_ || !fn_tool_start_) {
    return false;
  }
  return fn_tool_start_(session_, cmd.c_str()) == 0;
}

bool FfiSession::toolStartInsert(const std::string& blockName) {
  if (!session_ || !fn_tool_start_insert_) {
    return false;
  }
  return fn_tool_start_insert_(session_, blockName.c_str()) == 0;
}

bool FfiSession::toolStartImage(const std::string& path) {
  if (!session_ || !fn_tool_start_image_) {
    return false;
  }
  return fn_tool_start_image_(session_, path.c_str()) == 0;
}

bool FfiSession::toolAcceptPoint(double x, double y) {
  if (!session_ || !fn_tool_accept_) {
    return false;
  }
  return fn_tool_accept_(session_, x, y) == 0;
}

bool FfiSession::toolFinish() {
  if (!session_ || !fn_tool_finish_) {
    return false;
  }
  return fn_tool_finish_(session_) == 0;
}

bool FfiSession::toolSetText(const std::string& value) {
  if (!session_ || !fn_tool_set_text_) {
    return false;
  }
  return fn_tool_set_text_(session_, value.c_str()) == 0;
}

bool FfiSession::toolSetNumber(double value) {
  if (!session_ || !fn_tool_set_number_) {
    return false;
  }
  return fn_tool_set_number_(session_, value) == 0;
}

void FfiSession::placeFlipSet(bool on) {
  if (session_ && fn_place_flip_set_) {
    fn_place_flip_set_(session_, on ? 1 : 0);
  }
}

bool FfiSession::placeFlipGet() const {
  return session_ && fn_place_flip_get_ && fn_place_flip_get_(session_) != 0;
}

void FfiSession::pickAddSet(bool on) {
  if (session_ && fn_pick_add_set_) {
    fn_pick_add_set_(session_, on ? 1 : 0);
  }
}

void FfiSession::gripModeCycle() {
  if (session_ && fn_grip_mode_cycle_) {
    fn_grip_mode_cycle_(session_);
  }
}

std::string FfiSession::gripMode() const {
  if (!session_ || !fn_grip_mode_) {
    return {};
  }
  const char* s = fn_grip_mode_(session_);
  return s ? std::string(s) : std::string();
}

bool FfiSession::plotPdfToPath(const std::string& path, const std::string& paper) {
  if (!session_ || !fn_plot_pdf_ || path.empty()) {
    return false;
  }
  const char* paperArg = paper.empty() ? nullptr : paper.c_str();
  return fn_plot_pdf_(session_, path.c_str(), paperArg) == 0;
}

bool FfiSession::plotSetup(const std::string& paper) {
  if (!session_ || !fn_plot_setup_) {
    return false;
  }
  const char* paperArg = paper.empty() ? nullptr : paper.c_str();
  return fn_plot_setup_(session_, paperArg) == 0;
}

bool FfiSession::toolNeedsText() const {
  return session_ && fn_tool_needs_text_ && fn_tool_needs_text_(session_) != 0;
}

void FfiSession::toolSetCursor(double x, double y) {
  if (session_ && fn_tool_cursor_) {
    fn_tool_cursor_(session_, x, y);
  }
}

void FfiSession::toolCancel() {
  if (session_ && fn_tool_cancel_) {
    fn_tool_cancel_(session_);
  }
}

bool FfiSession::toolActive() const {
  return session_ && fn_tool_active_ && fn_tool_active_(session_) != 0;
}

std::string FfiSession::toolPrompt() const {
  if (!session_ || !fn_tool_prompt_) {
    return {};
  }
  const char* s = fn_tool_prompt_(session_);
  return s ? std::string(s) : std::string();
}

void FfiSession::osnapSet(bool on) {
  if (session_ && fn_osnap_set_) {
    fn_osnap_set_(session_, on ? 1 : 0);
  }
}

bool FfiSession::osnapMarker(double out2[2]) const {
  if (!session_ || !fn_osnap_marker_ || !out2) {
    return false;
  }
  return fn_osnap_marker_(session_, out2) != 0;
}

void FfiSession::orthoSet(bool on) {
  if (session_ && fn_ortho_set_) {
    fn_ortho_set_(session_, on ? 1 : 0);
  }
}

bool FfiSession::orthoGet() const {
  return session_ && fn_ortho_get_ && fn_ortho_get_(session_) != 0;
}

void FfiSession::polarSet(bool on) {
  if (session_ && fn_polar_set_) {
    fn_polar_set_(session_, on ? 1 : 0);
  }
}

bool FfiSession::polarGet() const {
  return session_ && fn_polar_get_ && fn_polar_get_(session_) != 0;
}

uint64_t FfiSession::selectBox(double x0, double y0, double x1, double y1) {
  if (!session_ || !fn_select_box_) {
    return 0;
  }
  return fn_select_box_(session_, x0, y0, x1, y1);
}

std::string FfiSession::layersList() const {
  if (!session_ || !fn_layers_list_) {
    return {};
  }
  return takeHeapString(fn_layers_list_(session_));
}

bool FfiSession::layerSetCurrent(const std::string& name) {
  if (!session_ || !fn_layer_set_current_) {
    return false;
  }
  return fn_layer_set_current_(session_, name.c_str()) == 0;
}

std::string FfiSession::layerCurrent() const {
  if (!session_ || !fn_layer_current_) {
    return {};
  }
  const char* s = fn_layer_current_(session_);
  return s ? std::string(s) : std::string();
}

std::string FfiSession::drawingPropsGet() const {
  if (!session_ || !fn_drawing_props_get_) {
    return {};
  }
  return takeHeapString(fn_drawing_props_get_(session_));
}

bool FfiSession::drawingPropsSet(const std::string& pack) {
  if (!session_ || !fn_drawing_props_set_) {
    return false;
  }
  return fn_drawing_props_set_(session_, pack.c_str()) == 0;
}

std::string FfiSession::blocksList() const {
  if (!session_ || !fn_blocks_list_) {
    return {};
  }
  return takeHeapString(fn_blocks_list_(session_));
}

std::string FfiSession::catalogTools() const {
  if (!fn_catalog_tools_) {
    return {};
  }
  return takeHeapString(fn_catalog_tools_());
}

std::string FfiSession::complete(const std::string& prefix) const {
  if (!session_ || !fn_complete_) {
    return {};
  }
  return takeHeapString(fn_complete_(session_, prefix.c_str()));
}

bool FfiSession::isInteractive(const std::string& cmd) const {
  if (!fn_is_interactive_) {
    return false;
  }
  return fn_is_interactive_(cmd.c_str()) != 0;
}

std::string FfiSession::elementsCatalog() const {
  if (!fn_elements_catalog_) {
    return {};
  }
  return takeHeapString(fn_elements_catalog_());
}

bool FfiSession::elementEnsure(const std::string& name) {
  if (!session_ || !fn_element_ensure_) {
    return false;
  }
  return fn_element_ensure_(session_, name.c_str()) == 0;
}

std::string FfiSession::hatchPatternsList() const {
  if (!fn_hatch_patterns_list_) {
    return {};
  }
  return takeHeapString(fn_hatch_patterns_list_());
}

void FfiSession::hatchSetPattern(const std::string& name) {
  if (session_ && fn_hatch_set_pattern_) {
    fn_hatch_set_pattern_(session_, name.c_str());
  }
}

std::string FfiSession::hatchPattern() const {
  if (!session_ || !fn_hatch_pattern_) {
    return {};
  }
  const char* s = fn_hatch_pattern_(session_);
  return s ? std::string(s) : std::string();
}

void FfiSession::otrackSet(bool on) {
  if (session_ && fn_otrack_set_) {
    fn_otrack_set_(session_, on ? 1 : 0);
  }
}

void FfiSession::otrackClear() {
  if (session_ && fn_otrack_clear_) {
    fn_otrack_clear_(session_);
  }
}

const std::vector<float>& FfiSession::rebuildOtrackRays() {
  otrackRays_.clear();
  if (!session_ || !fn_otrack_rays_rebuild_ || !fn_otrack_rays_copy_) {
    return otrackRays_;
  }
  const uint64_t n = fn_otrack_rays_rebuild_(session_);
  if (n == 0) {
    return otrackRays_;
  }
  otrackRays_.resize(static_cast<size_t>(n));
  fn_otrack_rays_copy_(session_, otrackRays_.data(), n);
  return otrackRays_;
}

const std::vector<float>& FfiSession::rebuildGrips() {
  grips_.clear();
  if (!session_ || !fn_grips_rebuild_ || !fn_grips_copy_) {
    return grips_;
  }
  const uint64_t n = fn_grips_rebuild_(session_);
  if (n == 0) {
    return grips_;
  }
  grips_.resize(static_cast<size_t>(n));
  fn_grips_copy_(session_, grips_.data(), n);
  return grips_;
}

bool FfiSession::gripPick(double x, double y, double tol, uint64_t* outId, uint64_t* outIdx) {
  if (!session_ || !fn_grip_pick_) {
    return false;
  }
  uint64_t id = 0;
  uint64_t idx = 0;
  const bool hit = fn_grip_pick_(session_, x, y, tol, &id, &idx) != 0;
  if (outId) {
    *outId = id;
  }
  if (outIdx) {
    *outIdx = idx;
  }
  return hit;
}

bool FfiSession::gripMove(uint64_t id, uint64_t gripIdx, double nx, double ny) {
  if (!session_ || !fn_grip_move_) {
    return false;
  }
  return fn_grip_move_(session_, id, gripIdx, nx, ny) == 0;
}

std::string FfiSession::sessionPath() const {
  if (!session_ || !fn_path_) {
    return {};
  }
  return takeHeapString(fn_path_(session_));
}

const std::vector<float>& FfiSession::rebuildLines() {
  lines_.clear();
  fills_.clear();
  if (!session_ || !fn_rebuild_ || !fn_copy_) {
    return lines_;
  }
  const uint64_t n = fn_rebuild_(session_);
  if (n > 0) {
    lines_.resize(static_cast<size_t>(n));
    fn_copy_(session_, lines_.data(), n);
  }
  if (fn_copy_fills_) {
    const uint64_t nf = fn_copy_fills_(session_, nullptr, 0);
    if (nf > 0) {
      fills_.resize(static_cast<size_t>(nf));
      fn_copy_fills_(session_, fills_.data(), nf);
    }
  }
  return lines_;
}

const std::vector<float>& FfiSession::rebuildFills() {
  // Fills are refreshed alongside lines in rebuildLines().
  if (fills_.empty() && session_ && fn_rebuild_ && fn_copy_fills_) {
    rebuildLines();
  }
  return fills_;
}

std::string FfiSession::takeNav() {
  if (!session_ || !fn_nav_take_) {
    return {};
  }
  return takeHeapString(fn_nav_take_(session_));
}

const std::vector<float>& FfiSession::rebuildPreview() {
  preview_.clear();
  if (!session_ || !fn_preview_rebuild_ || !fn_preview_copy_) {
    return preview_;
  }
  const uint64_t n = fn_preview_rebuild_(session_);
  if (n == 0) {
    return preview_;
  }
  preview_.resize(static_cast<size_t>(n));
  fn_preview_copy_(session_, preview_.data(), n);
  return preview_;
}

bool FfiSession::bounds(float out4[4]) const {
  if (!session_ || !fn_bounds_) {
    return false;
  }
  return fn_bounds_(session_, out4) != 0;
}
