#include "SwapChainCanvas.hpp"

#ifndef NOMINMAX
#define NOMINMAX
#endif

#include <microsoft.ui.xaml.media.dxinterop.h>

#include <algorithm>
#include <cmath>
#include <cstdint>

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

} // namespace

SwapChainCanvas::SwapChainCanvas() = default;

SwapChainCanvas::~SwapChainCanvas() { discardAll(); }

void SwapChainCanvas::discardTarget() {
  safeRelease(cursorBrush_);
  safeRelease(gripBrush_);
  safeRelease(previewBrush_);
  safeRelease(selectionBrush_);
  safeRelease(fillBrush_);
  safeRelease(brush_);
  if (dc_) {
    dc_->SetTarget(nullptr);
  }
  safeRelease(target_);
}

void SwapChainCanvas::discardAll() {
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
  // AutoCAD grip fill (white) — edge drawn with selectionBrush_
  hr = dc_->CreateSolidColorBrush(D2D1::ColorF(1.f, 1.f, 1.f), &gripBrush_);
  if (FAILED(hr)) {
    return hr;
  }
  return dc_->CreateSolidColorBrush(D2D1::ColorF(0.15f, 0.15f, 0.18f), &cursorBrush_);
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

  // WinUI SwapChainPanel: inverse composition scale so the buffer fills the panel.
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

void SwapChainCanvas::setLines(const std::vector<float>& xy) { lines_ = xy; }

void SwapChainCanvas::setFills(const std::vector<float>& pack) { fills_ = pack; }

void SwapChainCanvas::setSelection(const std::vector<float>& xy) { selection_ = xy; }

void SwapChainCanvas::setPreview(const std::vector<float>& xy) { preview_ = xy; }

void SwapChainCanvas::setGrips(const std::vector<float>& xy) { grips_ = xy; }

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

void SwapChainCanvas::render() {
  if (!dc_ || !target_) {
    if (FAILED(createTarget())) {
      return;
    }
  }
  if (!dc_) {
    return;
  }

  dc_->BeginDraw();
  // Pure white paper (model space).
  dc_->Clear(D2D1::ColorF(1.f, 1.f, 1.f));

  // Solid hatch fills under strokes (pack: n_pts, alpha, xy…).
  if (fillBrush_ && factory_ && !fills_.empty()) {
    size_t i = 0;
    while (i + 2 < fills_.size()) {
      const int nPts = static_cast<int>(fills_[i]);
      const float alpha = fills_[i + 1];
      i += 2;
      if (nPts < 3 || i + static_cast<size_t>(nPts) * 2 > fills_.size()) {
        break;
      }
      ID2D1PathGeometry* path = nullptr;
      if (SUCCEEDED(factory_->CreatePathGeometry(&path)) && path) {
        ID2D1GeometrySink* sink = nullptr;
        if (SUCCEEDED(path->Open(&sink)) && sink) {
          const float x0 = originX_ + fills_[i] * scale_;
          const float y0 = originY_ - fills_[i + 1] * scale_;
          sink->BeginFigure(D2D1::Point2F(x0, y0), D2D1_FIGURE_BEGIN_FILLED);
          for (int p = 1; p < nPts; ++p) {
            const float x = originX_ + fills_[i + static_cast<size_t>(p) * 2] * scale_;
            const float y = originY_ - fills_[i + static_cast<size_t>(p) * 2 + 1] * scale_;
            sink->AddLine(D2D1::Point2F(x, y));
          }
          sink->EndFigure(D2D1_FIGURE_END_CLOSED);
          sink->Close();
          sink->Release();
          fillBrush_->SetColor(D2D1::ColorF(0.45f, 0.52f, 0.62f, alpha > 0.f ? alpha : 0.35f));
          dc_->FillGeometry(path, fillBrush_);
        }
        path->Release();
      }
      i += static_cast<size_t>(nPts) * 2;
    }
  }

  // Entity strokes — stride 6: x0,y0,x1,y1,lw_mm,aci (legacy stride-4 also accepted).
  if (brush_ && !lines_.empty()) {
    const bool styled = (lines_.size() % 6) == 0;
    const size_t stride = styled ? 6 : 4;
    for (size_t i = 0; i + stride - 1 < lines_.size(); i += stride) {
      const float x0 = originX_ + lines_[i] * scale_;
      const float y0 = originY_ - lines_[i + 1] * scale_;
      const float x1 = originX_ + lines_[i + 2] * scale_;
      const float y1 = originY_ - lines_[i + 3] * scale_;
      float lwMm = 0.25f;
      uint8_t aci = 7;
      if (styled) {
        lwMm = lines_[i + 4];
        aci = static_cast<uint8_t>(
            (std::clamp)(static_cast<int>(lines_[i + 5] + 0.5f), 0, 255));
      }
      float r = 0.1f, g = 0.1f, b = 0.1f;
      aciBrushColor(aci, r, g, b);
      brush_->SetColor(D2D1::ColorF(r, g, b));
      // ISO mm → screen px (egui peer: scale * 25.4/plot * 0.15); plot≈100.
      float stroke = 1.25f;
      if (showLineweights_ && styled) {
        stroke = (lwMm * scale_ * (25.4f / 100.f) * 0.15f);
        stroke = (std::clamp)(stroke, 0.8f, 10.f);
      }
      dc_->DrawLine(D2D1::Point2F(x0, y0), D2D1::Point2F(x1, y1), brush_, stroke);
    }
  }
  if (selectionBrush_) {
    for (size_t i = 0; i + 3 < selection_.size(); i += 4) {
      const float x0 = originX_ + selection_[i] * scale_;
      const float y0 = originY_ - selection_[i + 1] * scale_;
      const float x1 = originX_ + selection_[i + 2] * scale_;
      const float y1 = originY_ - selection_[i + 3] * scale_;
      dc_->DrawLine(D2D1::Point2F(x0, y0), D2D1::Point2F(x1, y1), selectionBrush_, 2.5f);
    }
  }
  if (previewBrush_) {
    previewBrush_->SetColor(D2D1::ColorF(0.45f, 0.45f, 0.5f));
    for (size_t i = 0; i + 3 < preview_.size(); i += 4) {
      const float x0 = originX_ + preview_[i] * scale_;
      const float y0 = originY_ - preview_[i + 1] * scale_;
      const float x1 = originX_ + preview_[i + 2] * scale_;
      const float y1 = originY_ - preview_[i + 3] * scale_;
      dc_->DrawLine(D2D1::Point2F(x0, y0), D2D1::Point2F(x1, y1), previewBrush_, 1.0f);
    }
  }

  // AutoCAD grips — white fill + blue edge, fixed screen size.
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

  // AutoCAD crosshair — fixed size matching suite clock Viewbox (72 DIP).
  if (cursorVisible_ && cursorBrush_) {
    constexpr float kCross = 72.f; // same as AnalogueClock Viewbox
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
