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
  // FARPROC is an incomplete prototype on MinGW; cast is intentional for GetProcAddress.
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wcast-function-type"
  *out = reinterpret_cast<T>(p);
#pragma GCC diagnostic pop
  return true;
}

} // namespace

bool FfiSession::loadDll() {
  // Prefer DLL beside the exe, then cwd.
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
      !loadFn(module_, "aadt_session_feed", &fn_feed_, &loadError_) ||
      !loadFn(module_, "aadt_last_feedback", &fn_feedback_, &loadError_) ||
      !loadFn(module_, "aadt_entity_count", &fn_entities_, &loadError_) ||
      !loadFn(module_, "aadt_document_revision", &fn_revision_, &loadError_) ||
      !loadFn(module_, "aadt_is_dirty", &fn_dirty_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_rebuild_lines", &fn_rebuild_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_copy_lines", &fn_copy_, &loadError_) ||
      !loadFn(module_, "aadt_drawlist_bounds", &fn_bounds_, &loadError_)) {
    unload();
    return false;
  }

  if (fn_version_() < 2) {
    loadError_ = "aadt_ffi.dll ABI too old (need version >= 2)";
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
  fn_feed_ = nullptr;
  fn_feedback_ = nullptr;
  fn_entities_ = nullptr;
  fn_revision_ = nullptr;
  fn_dirty_ = nullptr;
  fn_rebuild_ = nullptr;
  fn_copy_ = nullptr;
  fn_bounds_ = nullptr;
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

uint64_t FfiSession::entityCount() const {
  return (session_ && fn_entities_) ? fn_entities_(session_) : 0;
}

uint64_t FfiSession::revision() const {
  return (session_ && fn_revision_) ? fn_revision_(session_) : 0;
}

bool FfiSession::dirty() const {
  return session_ && fn_dirty_ && fn_dirty_(session_) != 0;
}

const std::vector<float>& FfiSession::rebuildLines() {
  lines_.clear();
  if (!session_ || !fn_rebuild_ || !fn_copy_) {
    return lines_;
  }
  const uint64_t n = fn_rebuild_(session_);
  if (n == 0) {
    return lines_;
  }
  lines_.resize(static_cast<size_t>(n));
  fn_copy_(session_, lines_.data(), n);
  return lines_;
}

bool FfiSession::bounds(float out4[4]) const {
  if (!session_ || !fn_bounds_) {
    return false;
  }
  return fn_bounds_(session_, out4) != 0;
}
