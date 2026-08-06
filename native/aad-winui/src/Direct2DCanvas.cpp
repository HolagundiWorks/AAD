#include "Direct2DCanvas.hpp"

#include <algorithm>
#include <cmath>

Direct2DCanvas::Direct2DCanvas() = default;

Direct2DCanvas::~Direct2DCanvas() {
  discardDeviceResources();
  if (factory_) {
    factory_->Release();
    factory_ = nullptr;
  }
}

bool Direct2DCanvas::init(HWND hwnd) {
  hwnd_ = hwnd;
  if (FAILED(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, &factory_))) {
    return false;
  }
  return SUCCEEDED(createDeviceResources());
}

void Direct2DCanvas::discardDeviceResources() {
  if (brush_) {
    brush_->Release();
    brush_ = nullptr;
  }
  if (rt_) {
    rt_->Release();
    rt_ = nullptr;
  }
}

HRESULT Direct2DCanvas::createDeviceResources() {
  if (rt_) {
    return S_OK;
  }
  RECT rc{};
  GetClientRect(hwnd_, &rc);
  width_ = static_cast<UINT>(std::max(1L, rc.right - rc.left));
  height_ = static_cast<UINT>(std::max(1L, rc.bottom - rc.top));
  D2D1_SIZE_U size = D2D1::SizeU(width_, height_);
  HRESULT hr = factory_->CreateHwndRenderTarget(
      D2D1::RenderTargetProperties(),
      D2D1::HwndRenderTargetProperties(hwnd_, size), &rt_);
  if (FAILED(hr)) {
    return hr;
  }
  return rt_->CreateSolidColorBrush(D2D1::ColorF(0.14f, 0.14f, 0.14f), &brush_);
}

void Direct2DCanvas::resize(UINT width, UINT height) {
  width_ = std::max(1u, width);
  height_ = std::max(1u, height);
  if (rt_) {
    rt_->Resize(D2D1::SizeU(width_, height_));
  }
}

void Direct2DCanvas::setLines(const std::vector<float>& xy) { lines_ = xy; }

void Direct2DCanvas::fitWorld(float minX, float minY, float maxX, float maxY) {
  const float w = std::max(1e-3f, maxX - minX);
  const float h = std::max(1e-3f, maxY - minY);
  const float pad = 0.08f;
  const float sx = static_cast<float>(width_) * (1.f - 2.f * pad) / w;
  const float sy = static_cast<float>(height_) * (1.f - 2.f * pad) / h;
  scale_ = std::min(sx, sy);
  // Y grows up in CAD; screen Y grows down.
  const float cx = (minX + maxX) * 0.5f;
  const float cy = (minY + maxY) * 0.5f;
  originX_ = static_cast<float>(width_) * 0.5f - cx * scale_;
  originY_ = static_cast<float>(height_) * 0.5f + cy * scale_;
}

void Direct2DCanvas::pan(float dxPx, float dyPx) {
  originX_ += dxPx;
  originY_ += dyPx;
}

void Direct2DCanvas::zoomAt(float factor, float pivotXPx, float pivotYPx) {
  const float wx = worldX(pivotXPx);
  const float wy = worldY(pivotYPx);
  scale_ = std::clamp(scale_ * factor, 1e-4f, 1e6f);
  originX_ = pivotXPx - wx * scale_;
  originY_ = pivotYPx + wy * scale_;
}

float Direct2DCanvas::worldX(float screenX) const {
  return (screenX - originX_) / scale_;
}

float Direct2DCanvas::worldY(float screenY) const {
  return (originY_ - screenY) / scale_;
}

void Direct2DCanvas::render() {
  if (FAILED(createDeviceResources()) || !rt_) {
    return;
  }
  rt_->BeginDraw();
  rt_->Clear(D2D1::ColorF(1.f, 1.f, 1.f)); // paper

  // Brand-quiet stroke (near-black), not orange — orange is chrome-only.
  if (brush_) {
    brush_->SetColor(D2D1::ColorF(0.1f, 0.1f, 0.1f));
  }
  for (size_t i = 0; i + 3 < lines_.size(); i += 4) {
    const float x0 = originX_ + lines_[i] * scale_;
    const float y0 = originY_ - lines_[i + 1] * scale_;
    const float x1 = originX_ + lines_[i + 2] * scale_;
    const float y1 = originY_ - lines_[i + 3] * scale_;
    rt_->DrawLine(D2D1::Point2F(x0, y0), D2D1::Point2F(x1, y1), brush_, 1.0f);
  }

  const HRESULT hr = rt_->EndDraw();
  if (hr == D2DERR_RECREATE_TARGET) {
    discardDeviceResources();
  }
}
