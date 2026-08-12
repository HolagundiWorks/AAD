#pragma once

#include <d2d1.h>
#include <d2d1helper.h>
#include <vector>
#include <windows.h>

/// Direct2D canvas: paints FFI line segments in world space with pan/zoom.
class Direct2DCanvas {
public:
  Direct2DCanvas();
  ~Direct2DCanvas();

  bool init(HWND hwnd);
  void resize(UINT width, UINT height);
  void setLines(const std::vector<float>& xy);
  void setPreview(const std::vector<float>& xy);
  void fitWorld(float minX, float minY, float maxX, float maxY);
  void pan(float dxPx, float dyPx);
  void zoomAt(float factor, float pivotXPx, float pivotYPx);
  void render();

  float worldX(float screenX) const;
  float worldY(float screenY) const;

private:
  void discardDeviceResources();
  HRESULT createDeviceResources();

  HWND hwnd_ = nullptr;
  ID2D1Factory* factory_ = nullptr;
  ID2D1HwndRenderTarget* rt_ = nullptr;
  ID2D1SolidColorBrush* brush_ = nullptr;
  ID2D1SolidColorBrush* previewBrush_ = nullptr;

  std::vector<float> lines_;
  std::vector<float> preview_;
  float scale_ = 1.f;
  float originX_ = 0.f;
  float originY_ = 0.f;
  UINT width_ = 1;
  UINT height_ = 1;
};
