#include "SwapChainCanvas.hpp"

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <microsoft.ui.xaml.media.dxinterop.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <unordered_map>
#include <utility>
#include <vector>

#pragma comment(lib, "d2d1.lib")
#pragma comment(lib, "d3d11.lib")
#pragma comment(lib, "dxgi.lib")

namespace {

template <typename T>
void safeRelease(T*& p) {
  if (p) {
    p->Release();
    p = nullptr;
  }
}

uint32_t batchKey(uint8_t aci, float lwMm) {
  // Quantize lineweight so near-identical weights share one PathGeometry.
  const int q = static_cast<int>(std::lround(lwMm * 100.f));
  return (static_cast<uint32_t>(aci) << 16) | static_cast<uint32_t>(q & 0xffff);
}

} // namespace

SwapChainCanvas::SwapChainCanvas() = default;

SwapChainCanvas::~SwapChainCanvas() { discardAll(); }

void SwapChainCanvas::discardScene() {
  for (auto& b : strokeBatches_) {
    safeRelease(b.geom);
  }
  strokeBatches_.clear();
  for (auto& f : fillBatches_) {
    safeRelease(f.geom);
  }
  fillBatches_.clear();
  sceneDirty_ = true;
}

void SwapChainCanvas::invalidateScene() { sceneDirty_ = true; }

void SwapChainCanvas::discardTarget() {
  safeRelease(cursorBrush_);
  safeRelease(hotGripBrush_);
  safeRelease(gripBrush_);
  safeRelease(previewBrush_);
  safeRelease(selectionBrush_);
  safeRelease(fillBrush_);
  safeRelease(brush_);
  safeRelease(fixedStroke_);
  if (dc_) {
    dc_->SetTarget(nullptr);
  }
  safeRelease(target_);
}

void SwapChainCanvas::discardAll() {
  discardScene();
  discardTarget();
  safeRelease(dc_);
  safeRelease(d2dDevice_);
  safeRelease(swapChain_);
  safeRelease(d3dCtx_);
  safeRelease(d3d_);
  safeRelease(factory_);
  panel_ = nullptr;
}

HRESULT SwapChainCanvas::ensureDevice() {
  if (d3d_ && factory_ && d2dDevice_ && dc_) {
    return S_OK;
  }

  D2D1_FACTORY_OPTIONS opts{};
#if defined(_DEBUG)
  opts.debugLevel = D2D1_DEBUG_LEVEL_INFORMATION;
#endif
  HRESULT hr = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, __uuidof(ID2D1Factory1),
                                 &opts, reinterpret_cast<void**>(&factory_));
  if (FAILED(hr)) {
    return hr;
  }

  UINT flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
  D3D_FEATURE_LEVEL level{};
  const D3D_FEATURE_LEVEL levels[] = {
      D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_0};
  hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, flags, levels,
                         ARRAYSIZE(levels), D3D11_SDK_VERSION, &d3d_, &level, &d3dCtx_);
  if (FAILED(hr)) {
    hr = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr, flags, levels,
                           ARRAYSIZE(levels), D3D11_SDK_VERSION, &d3d_, &level, &d3dCtx_);
  }
  if (FAILED(hr)) {
    return hr;
  }

  IDXGIDevice* dxgiDevice = nullptr;
  hr = d3d_->QueryInterface(IID_PPV_ARGS(&dxgiDevice));
  if (FAILED(hr)) {
    return hr;
  }
  hr = factory_->CreateDevice(dxgiDevice, &d2dDevice_);
  dxgiDevice->Release();
  if (FAILED(hr)) {
    return hr;
  }
  return d2dDevice_->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, &dc_);
}

HRESULT SwapChainCanvas::createTarget() {
  if (!swapChain_ || !dc_) {
    return E_FAIL;
  }
  discardTarget();

  IDXGISurface* surface = nullptr;
  HRESULT hr = swapChain_->GetBuffer(0, IID_PPV_ARGS(&surface));
  if (FAILED(hr)) {
    return hr;
  }

  const D2D1_BITMAP_PROPERTIES1 props = D2D1::BitmapProperties1(
      D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
      D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
      96.f * scaleX_, 96.f * scaleY_);
  hr = dc_->CreateBitmapFromDxgiSurface(surface, &props, &target_);
  surface->Release();
  if (FAILED(hr)) {
    return hr;
  }
  dc_->SetTarget(target_);
  dc_->SetDpi(96.f * scaleX_, 96.f * scaleY_);

  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(0.1f, 0.1f, 0.1f), &brush_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(0.55f, 0.62f, 0.72f, 0.35f), &fillBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(0.12f, 0.47f, 1.0f), &selectionBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(0.45f, 0.45f, 0.5f), &previewBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(1.f, 1.f, 1.f), &gripBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  // Radiant Orange accent for the hot grip.
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(1.f, 0.31f, 0.094f), &hotGripBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(0.15f, 0.15f, 0.18f), &cursorBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  // Stroke widths stay in screen DIPs while geometry is in world space.
  const D2D1_STROKE_STYLE_PROPERTIES1 ssp = D2D1::StrokeStyleProperties1(
      D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_FLAT, D2D1_LINE_JOIN_MITER,
      10.0f, D2D1_DASH_STYLE_SOLID, 0.0f, D2D1_STROKE_TRANSFORM_TYPE_FIXED);
  return factory_->CreateStrokeStyle(ssp, nullptr, 0, &fixedStroke_);
}

bool SwapChainCanvas::attach(IUnknown* panel) {
  discardAll();
  panel_ = panel;
  if (FAILED(ensureDevice()) || !panel_) {
    return false;
  }

  IDXGIDevice* dxgiDevice = nullptr;
  if (FAILED(d3d_->QueryInterface(IID_PPV_ARGS(&dxgiDevice)))) {
    return false;
  }
  IDXGIAdapter* adapter = nullptr;
  HRESULT hr = dxgiDevice->GetAdapter(&adapter);
  dxgiDevice->Release();
  if (FAILED(hr)) {
    return false;
  }
  IDXGIFactory2* factory = nullptr;
  hr = adapter->GetParent(IID_PPV_ARGS(&factory));
  adapter->Release();
  if (FAILED(hr)) {
    return false;
  }

  DXGI_SWAP_CHAIN_DESC1 desc{};
  desc.Width = 1;
  desc.Height = 1;
  desc.Format = DXGI_FORMAT_B8G8R8A8_UNORM;
  desc.SampleDesc.Count = 1;
  desc.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
  desc.BufferCount = 2;
  desc.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
  desc.Scaling = DXGI_SCALING_STRETCH;
  desc.AlphaMode = DXGI_ALPHA_MODE_IGNORE;

  hr = factory->CreateSwapChainForComposition(d3d_, &desc, nullptr, &swapChain_);
  factory->Release();
  if (FAILED(hr)) {
    return false;
  }

  ISwapChainPanelNative* native = nullptr;
  hr = panel_->QueryInterface(__uuidof(ISwapChainPanelNative),
                              reinterpret_cast<void**>(&native));
  if (FAILED(hr) || !native) {
    return false;
  }
  hr = native->SetSwapChain(swapChain_);
  native->Release();
  if (FAILED(hr)) {
    return false;
  }
  return SUCCEEDED(createTarget());
}

void SwapChainCanvas::resize(float widthDips, float heightDips, float scaleX, float scaleY) {
  widthDips_ = (std::max)(1.f, widthDips);
  heightDips_ = (std::max)(1.f, heightDips);
  scaleX_ = (std::max)(0.1f, scaleX);
  scaleY_ = (std::max)(0.1f, scaleY);
  if (!swapChain_) {
    return;
  }

  const UINT w = (std::max)(1u, static_cast<UINT>(std::ceil(widthDips_ * scaleX_)));
  const UINT h = (std::max)(1u, static_cast<UINT>(std::ceil(heightDips_ * scaleY_)));

  discardTarget();
  HRESULT hr = swapChain_->ResizeBuffers(2, w, h, DXGI_FORMAT_B8G8R8A8_UNORM, 0);
  if (FAILED(hr)) {
    return;
  }

  IDXGISwapChain2* sc2 = nullptr;
  if (SUCCEEDED(swapChain_->QueryInterface(IID_PPV_ARGS(&sc2))) && sc2) {
    DXGI_MATRIX_3X2_F inverse{};
    inverse._11 = 1.f / scaleX_;
    inverse._22 = 1.f / scaleY_;
    sc2->SetMatrixTransform(&inverse);
    sc2->Release();
  }

  createTarget();
}

void SwapChainCanvas::setLines(const std::vector<float>& xy) {
  linesView_ = nullptr;
  linesViewN_ = 0;
  if (xy.size() == lines_.size() && (xy.empty() || xy.data() == lines_.data())) {
    return;
  }
  lines_ = xy;
  invalidateScene();
}

void SwapChainCanvas::setLinesView(const float* data, size_t count) {
  lines_.clear();
  lines_.shrink_to_fit();
  // Host calls this only when tess pack changed — always REGEN the display list.
  linesView_ = data;
  linesViewN_ = count;
  invalidateScene();
}

void SwapChainCanvas::setFills(const std::vector<float>& pack) {
  fillsView_ = nullptr;
  fillsViewN_ = 0;
  if (pack.size() == fills_.size() && (pack.empty() || pack.data() == fills_.data())) {
    return;
  }
  fills_ = pack;
  invalidateScene();
}

void SwapChainCanvas::setFillsView(const float* data, size_t count) {
  fills_.clear();
  fills_.shrink_to_fit();
  fillsView_ = data;
  fillsViewN_ = count;
  invalidateScene();
}

void SwapChainCanvas::setSelection(const std::vector<float>& xy) {
  if (xy.size() == selection_.size() && (xy.empty() || xy.data() == selection_.data())) {
    return;
  }
  selection_ = xy;
}

void SwapChainCanvas::setPreview(const std::vector<float>& xy) {
  if (xy.size() == preview_.size() && (xy.empty() || xy.data() == preview_.data())) {
    return;
  }
  preview_ = xy;
}

void SwapChainCanvas::setGrips(const std::vector<float>& xy) {
  if (xy.size() == grips_.size() && (xy.empty() || xy.data() == grips_.data())) {
    return;
  }
  grips_ = xy;
}

void SwapChainCanvas::setHotGrip(float wx, float wy, bool visible) {
  hotGripX_ = wx;
  hotGripY_ = wy;
  hotGripVisible_ = visible;
}

void SwapChainCanvas::setCursorDip(float x, float y, bool visible) {
  cursorX_ = x;
  cursorY_ = y;
  cursorVisible_ = visible;
}

void SwapChainCanvas::setShowLineweights(bool on) { showLineweights_ = on; }

void SwapChainCanvas::aciBrushColor(uint8_t aci, float& r, float& g, float& b) const {
  switch (aci) {
  case 0:
  case 7:
  case 250:
    r = g = b = 0.08f;
    break;
  case 1:
    r = 1.f;
    g = 0.f;
    b = 0.f;
    break;
  case 2:
    r = 1.f;
    g = 1.f;
    b = 0.f;
    break;
  case 3:
    r = 0.f;
    g = 1.f;
    b = 0.f;
    break;
  case 4:
    r = 0.f;
    g = 1.f;
    b = 1.f;
    break;
  case 5:
    r = 0.f;
    g = 0.f;
    b = 1.f;
    break;
  case 6:
    r = 1.f;
    g = 0.f;
    b = 1.f;
    break;
  case 8:
    r = g = b = 0.5f;
    break;
  case 9:
    r = g = b = 0.75f;
    break;
  default: {
    const int idx = static_cast<int>(aci) - 10;
    const float hue = static_cast<float>(idx / 10) * 15.f;
    const int rem = idx % 10;
    const float sat = rem < 5 ? 1.f : 0.5f;
    const int light = rem < 5 ? rem : rem - 5;
    const float value = light == 0   ? 0.35f
                        : light == 1 ? 0.50f
                        : light == 2 ? 0.65f
                        : light == 3 ? 0.80f
                                     : 1.0f;
    const float h = hue / 60.f;
    const float c = value * sat;
    const float x = c * (1.f - std::fabs(std::fmod(h, 2.f) - 1.f));
    const float m = value - c;
    float r1 = 0, g1 = 0, b1 = 0;
    switch (static_cast<int>(std::floor(h)) % 6) {
    case 0:
      r1 = c;
      g1 = x;
      break;
    case 1:
      r1 = x;
      g1 = c;
      break;
    case 2:
      g1 = c;
      b1 = x;
      break;
    case 3:
      g1 = x;
      b1 = c;
      break;
    case 4:
      r1 = x;
      b1 = c;
      break;
    default:
      r1 = c;
      b1 = x;
      break;
    }
    r = r1 + m;
    g = g1 + m;
    b = b1 + m;
    break;
  }
  }
}

void SwapChainCanvas::fitWorld(float minX, float minY, float maxX, float maxY) {
  const float w = (std::max)(1e-3f, maxX - minX);
  const float h = (std::max)(1e-3f, maxY - minY);
  const float pad = 0.08f;
  const float sx = widthDips_ * (1.f - 2.f * pad) / w;
  const float sy = heightDips_ * (1.f - 2.f * pad) / h;
  scale_ = (std::min)(sx, sy);
  const float cx = (minX + maxX) * 0.5f;
  const float cy = (minY + maxY) * 0.5f;
  originX_ = widthDips_ * 0.5f - cx * scale_;
  originY_ = heightDips_ * 0.5f + cy * scale_;
}

void SwapChainCanvas::pan(float dxPx, float dyPx) {
  originX_ += dxPx;
  originY_ += dyPx;
}

void SwapChainCanvas::zoomAt(float factor, float pivotXPx, float pivotYPx) {
  const float wx = worldX(pivotXPx);
  const float wy = worldY(pivotYPx);
  scale_ = (std::clamp)(scale_ * factor, 1e-4f, 1e6f);
  originX_ = pivotXPx - wx * scale_;
  originY_ = pivotYPx + wy * scale_;
}

float SwapChainCanvas::worldX(float screenX) const {
  return (screenX - originX_) / scale_;
}

float SwapChainCanvas::worldY(float screenY) const {
  return (originY_ - screenY) / scale_;
}

D2D1_MATRIX_3X2_F SwapChainCanvas::worldToScreen() const {
  // sx = originX + wx * scale ; sy = originY - wy * scale
  return D2D1::Matrix3x2F(scale_, 0.f, 0.f, -scale_, originX_, originY_);
}

void SwapChainCanvas::rebuildSceneIfNeeded() {
  if (!sceneDirty_ || !factory_) {
    return;
  }
  // Clear batches without flipping dirty (we are about to rebuild).
  for (auto& b : strokeBatches_) {
    safeRelease(b.geom);
  }
  strokeBatches_.clear();
  for (auto& f : fillBatches_) {
    safeRelease(f.geom);
  }
  fillBatches_.clear();
  sceneDirty_ = false;

  const float* lines = linesView_ ? linesView_ : lines_.data();
  const size_t linesN = linesView_ ? linesViewN_ : lines_.size();
  if (lines && linesN > 0) {
    const bool styled = (linesN % 6) == 0;
    const size_t stride = styled ? 6 : 4;

    struct Acc {
      uint8_t aci;
      float lwMm;
      std::vector<D2D1_POINT_2F> pts; // pairs: a,b,a,b,…
    };
    std::unordered_map<uint32_t, Acc> map;
    map.reserve(64);

    for (size_t i = 0; i + stride - 1 < linesN; i += stride) {
      float lwMm = 0.25f;
      uint8_t aci = 7;
      if (styled) {
        lwMm = lines[i + 4];
        aci = static_cast<uint8_t>(
            (std::clamp)(static_cast<int>(lines[i + 5] + 0.5f), 0, 255));
      }
      const uint32_t key = batchKey(aci, lwMm);
      Acc& acc = map[key];
      if (acc.pts.empty()) {
        acc.aci = aci;
        acc.lwMm = lwMm;
      }
      // World-space endpoints — transform applied at Present time.
      acc.pts.push_back(D2D1::Point2F(lines[i], lines[i + 1]));
      acc.pts.push_back(D2D1::Point2F(lines[i + 2], lines[i + 3]));
    }

    strokeBatches_.reserve(map.size());
    for (auto& kv : map) {
      Acc& acc = kv.second;
      if (acc.pts.size() < 2) {
        continue;
      }
      ID2D1PathGeometry* geom = nullptr;
      if (FAILED(factory_->CreatePathGeometry(&geom)) || !geom) {
        continue;
      }
      ID2D1GeometrySink* sink = nullptr;
      if (FAILED(geom->Open(&sink)) || !sink) {
        geom->Release();
        continue;
      }
      for (size_t i = 0; i + 1 < acc.pts.size(); i += 2) {
        sink->BeginFigure(acc.pts[i], D2D1_FIGURE_BEGIN_HOLLOW);
        sink->AddLine(acc.pts[i + 1]);
        sink->EndFigure(D2D1_FIGURE_END_OPEN);
      }
      sink->Close();
      sink->Release();
      StrokeBatch b;
      b.aci = acc.aci;
      b.lwMm = acc.lwMm;
      b.geom = geom;
      strokeBatches_.push_back(b);
    }
  }

  const float* fills = fillsView_ ? fillsView_ : fills_.data();
  const size_t fillsN = fillsView_ ? fillsViewN_ : fills_.size();
  if (fills && fillsN > 0) {
    size_t i = 0;
    while (i + 2 < fillsN) {
      const int nPts = static_cast<int>(fills[i]);
      const float alpha = fills[i + 1];
      i += 2;
      if (nPts < 3 || i + static_cast<size_t>(nPts) * 2 > fillsN) {
        break;
      }
      ID2D1PathGeometry* geom = nullptr;
      if (SUCCEEDED(factory_->CreatePathGeometry(&geom)) && geom) {
        ID2D1GeometrySink* sink = nullptr;
        if (SUCCEEDED(geom->Open(&sink)) && sink) {
          sink->BeginFigure(D2D1::Point2F(fills[i], fills[i + 1]), D2D1_FIGURE_BEGIN_FILLED);
          for (int p = 1; p < nPts; ++p) {
            sink->AddLine(D2D1::Point2F(fills[i + static_cast<size_t>(p) * 2],
                                        fills[i + static_cast<size_t>(p) * 2 + 1]));
          }
          sink->EndFigure(D2D1_FIGURE_END_CLOSED);
          sink->Close();
          sink->Release();
          FillBatch fb;
          fb.alpha = alpha > 0.f ? alpha : 0.35f;
          fb.geom = geom;
          fillBatches_.push_back(fb);
        } else {
          geom->Release();
        }
      }
      i += static_cast<size_t>(nPts) * 2;
    }
  }
}

void SwapChainCanvas::render() {
  if (!dc_ || !target_) {
    if (FAILED(createTarget())) {
      return;
    }
  }
  if (!dc_) {
    return;
  }

  rebuildSceneIfNeeded();

  dc_->BeginDraw();
  dc_->Clear(D2D1::ColorF(1.f, 1.f, 1.f));

  // --- Scene (retained display list) under world→screen transform ---
  const D2D1_MATRIX_3X2_F view = worldToScreen();
  dc_->SetTransform(view);

  if (fillBrush_) {
    for (const auto& fb : fillBatches_) {
      if (!fb.geom) {
        continue;
      }
      fillBrush_->SetColor(D2D1::ColorF(0.45f, 0.52f, 0.62f, fb.alpha));
      dc_->FillGeometry(fb.geom, fillBrush_);
    }
  }

  if (brush_) {
    for (const auto& b : strokeBatches_) {
      if (!b.geom) {
        continue;
      }
      float r = 0.1f, g = 0.1f, bl = 0.1f;
      aciBrushColor(b.aci, r, g, bl);
      brush_->SetColor(D2D1::ColorF(r, g, bl));
      float stroke = 1.25f;
      if (showLineweights_) {
        stroke = (b.lwMm * scale_ * (25.4f / 100.f) * 0.15f);
        stroke = (std::clamp)(stroke, 0.8f, 10.f);
      }
      dc_->DrawGeometry(b.geom, brush_, stroke, fixedStroke_);
    }
  }

  // Selection / preview / grips in world space (same transform).
  if (selectionBrush_) {
    selectionBrush_->SetColor(D2D1::ColorF(0.12f, 0.47f, 1.0f));
    for (size_t i = 0; i + 3 < selection_.size(); i += 4) {
      dc_->DrawLine(D2D1::Point2F(selection_[i], selection_[i + 1]),
                    D2D1::Point2F(selection_[i + 2], selection_[i + 3]), selectionBrush_, 2.5f,
                    fixedStroke_);
    }
  }
  if (previewBrush_) {
    previewBrush_->SetColor(D2D1::ColorF(0.45f, 0.45f, 0.5f));
    for (size_t i = 0; i + 3 < preview_.size(); i += 4) {
      dc_->DrawLine(D2D1::Point2F(preview_[i], preview_[i + 1]),
                    D2D1::Point2F(preview_[i + 2], preview_[i + 3]), previewBrush_, 1.0f,
                    fixedStroke_);
    }
  }
  if (gripBrush_ && selectionBrush_ && !grips_.empty()) {
    // Grips are screen-fixed size — draw in screen space below.
  }

  // --- Screen-space overlays (identity transform) ---
  dc_->SetTransform(D2D1::Matrix3x2F::Identity());

  if (gripBrush_ && selectionBrush_ && !grips_.empty()) {
    constexpr float kGripHalf = 4.f;
    selectionBrush_->SetColor(D2D1::ColorF(0.12f, 0.47f, 1.0f));
    for (size_t i = 0; i + 1 < grips_.size(); i += 2) {
      const float sx = originX_ + grips_[i] * scale_;
      const float sy = originY_ - grips_[i + 1] * scale_;
      const D2D1_RECT_F rc =
          D2D1::RectF(sx - kGripHalf, sy - kGripHalf, sx + kGripHalf, sy + kGripHalf);
      dc_->FillRectangle(rc, gripBrush_);
      dc_->DrawRectangle(rc, selectionBrush_, 1.5f);
    }
  }
  if (hotGripVisible_ && hotGripBrush_ && selectionBrush_) {
    constexpr float kHotHalf = 6.f;
    const float sx = originX_ + hotGripX_ * scale_;
    const float sy = originY_ - hotGripY_ * scale_;
    const D2D1_RECT_F rc =
        D2D1::RectF(sx - kHotHalf, sy - kHotHalf, sx + kHotHalf, sy + kHotHalf);
    dc_->FillRectangle(rc, hotGripBrush_);
    selectionBrush_->SetColor(D2D1::ColorF(0.08f, 0.08f, 0.1f));
    dc_->DrawRectangle(rc, selectionBrush_, 2.f);
  }

  if (cursorVisible_ && cursorBrush_) {
    constexpr float kCross = 72.f;
    constexpr float kHalf = kCross * 0.5f;
    constexpr float kPick = 5.f;
    cursorBrush_->SetColor(D2D1::ColorF(0.12f, 0.12f, 0.14f));
    dc_->DrawLine(D2D1::Point2F(cursorX_ - kHalf, cursorY_),
                  D2D1::Point2F(cursorX_ + kHalf, cursorY_), cursorBrush_, 1.f);
    dc_->DrawLine(D2D1::Point2F(cursorX_, cursorY_ - kHalf),
                  D2D1::Point2F(cursorX_, cursorY_ + kHalf), cursorBrush_, 1.f);
    const D2D1_RECT_F pb =
        D2D1::RectF(cursorX_ - kPick, cursorY_ - kPick, cursorX_ + kPick, cursorY_ + kPick);
    dc_->DrawRectangle(pb, cursorBrush_, 1.f);
  }

  const HRESULT hr = dc_->EndDraw();
  if (hr == D2DERR_RECREATE_TARGET) {
    discardTarget();
    createTarget();
    return;
  }
  if (swapChain_) {
    swapChain_->Present(1, 0);
  }
}
