#pragma once

#include <windows.h>

#include "aadt_ffi.h"

#include <string>
#include <vector>

/// RAII wrapper: LoadLibrary("aadt_ffi.dll") + Session (ADR-0017).
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

  uint64_t entityCount() const;
  uint64_t revision() const;
  bool dirty() const;

  const std::vector<float>& rebuildLines();
  bool bounds(float out4[4]) const;

private:
  bool loadDll();
  void unload();

  HMODULE module_ = nullptr;
  AadtSession* session_ = nullptr;
  std::string loadError_;
  std::vector<float> lines_;

  decltype(&aadt_ffi_version) fn_version_ = nullptr;
  decltype(&aadt_session_new) fn_new_ = nullptr;
  decltype(&aadt_session_free) fn_free_ = nullptr;
  decltype(&aadt_session_feed) fn_feed_ = nullptr;
  decltype(&aadt_last_feedback) fn_feedback_ = nullptr;
  decltype(&aadt_entity_count) fn_entities_ = nullptr;
  decltype(&aadt_document_revision) fn_revision_ = nullptr;
  decltype(&aadt_is_dirty) fn_dirty_ = nullptr;
  decltype(&aadt_drawlist_rebuild_lines) fn_rebuild_ = nullptr;
  decltype(&aadt_drawlist_copy_lines) fn_copy_ = nullptr;
  decltype(&aadt_drawlist_bounds) fn_bounds_ = nullptr;
};
