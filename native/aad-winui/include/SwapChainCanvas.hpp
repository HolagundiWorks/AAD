#pragma once

#include <d2d1_1.h>
#include <d2d1helper.h>
#include <d3d11.h>
#include <dxgi1_2.h>
#include <dxgi1_3.h>
#include <unknwn.h>
#include <cstdint>
#include <vector>

/// Direct2D paper on a WinUI 3 SwapChainPanel (composition DXGI swap chain).
///
/// CAD display-list model (AutoCAD-like):
/// - Entity strokes/fills are tessellated once into world-space PathGeometry
///   batches (REGEN) when the host stroke pack changes.
/// - Pan / zoom / crosshair / rubber-band only re-Present with a view transform
///   — they do not rebuild the display list or re-issue N DrawLine calls.
class SwapChainCanvas {
public:
  SwapChainCanvas();
  ~SwapChainCanvas();

  SwapChainCanvas(const SwapChainCanvas&) = delete;
  SwapChainCanvas& operator=(const SwapChainCanvas&) = delete;

  /// `panel` is a Microsoft.UI.Xaml.Controls.SwapChainPanel (IInspectable).
  bool attach(IUnknown* panel);
  void resize(float widthDips, float heightDips, float scaleX, float scaleY);
  /// Entity strokes: x0,y0,x1,y1,lw_mm,aci per segment (stride 6).
  void setLines(const std::vector<float>& xy);
  /// Borrow host stroke buffer (no copy) — pointer must outlive next setLines/render.
  void setLinesView(const float* data, size_t count);
  void setFills(const std::vector<float>& pack);
  void setFillsView(const float* data, size_t count);
  void setSelection(const std::vector<float>& xy);
  void setPreview(const std::vector<float>& xy);
  /// Grip centres in world space: gx,gy pairs.
  void setGrips(const std::vector<float>& xy);
  /// Hot grip (hover / active) in world space — drawn larger / accent.
  void setHotGrip(float wx, float wy, bool visible);
  void setCursorDip(float x, float y, bool visible);
  void setShowLineweights(bool on);
  bool showLineweights() const { return showLineweights_; }
  void fitWorld(float minX, float minY, float maxX, float maxY);
  void pan(float dxPx, float dyPx);
  void zoomAt(float factor, float pivotXPx, float pivotYPx);
  void render();

  float worldX(float screenX) const;
  float worldY(float screenY) const;
  bool ok() const { return swapChain_ != nullptr; }

private:
  struct StrokeBatch {
    uint8_t aci = 7;
    float lwMm = 0.25f;
    ID2D1PathGeometry* geom = nullptr;
  };
  struct FillBatch {
    float alpha = 0.35f;
    ID2D1PathGeometry* geom = nullptr;
  };

  void discardTarget();
  void discardScene();
  void discardAll();
  void invalidateScene();
  void rebuildSceneIfNeeded();
  HRESULT createTarget();
  HRESULT ensureDevice();
  void aciBrushColor(uint8_t aci, float& r, float& g, float& b) const;
  D2D1_MATRIX_3X2_F worldToScreen() const;

  IUnknown* panel_ = nullptr; // not owned
  ID2D1Factory1* factory_ = nullptr;
  ID3D11Device* d3d_ = nullptr;
  ID3D11DeviceContext* d3dCtx_ = nullptr;
  IDXGISwapChain1* swapChain_ = nullptr;
  ID2D1Device* d2dDevice_ = nullptr;
  ID2D1DeviceContext* dc_ = nullptr;
  ID2D1Bitmap1* target_ = nullptr;
  ID2D1SolidColorBrush* brush_ = nullptr;
  ID2D1SolidColorBrush* fillBrush_ = nullptr;
  ID2D1SolidColorBrush* selectionBrush_ = nullptr;
  ID2D1SolidColorBrush* previewBrush_ = nullptr;
  ID2D1SolidColorBrush* gripBrush_ = nullptr;
  ID2D1SolidColorBrush* hotGripBrush_ = nullptr;
  ID2D1SolidColorBrush* cursorBrush_ = nullptr;
  /// FIXED stroke — width stays in DIPs under the world→screen transform.
  ID2D1StrokeStyle1* fixedStroke_ = nullptr;

  std::vector<float> lines_;
  std::vector<float> fills_;
  /// Optional borrow of FfiSession stroke packs (avoids a 3rd RAM copy).
  const float* linesView_ = nullptr;
  size_t linesViewN_ = 0;
  const float* fillsView_ = nullptr;
  size_t fillsViewN_ = 0;
  std::vector<float> selection_;
  std::vector<float> preview_;
  std::vector<float> grips_; // world gx,gy
  float hotGripX_ = 0.f;
  float hotGripY_ = 0.f;
  bool hotGripVisible_ = false;

  /// Retained display list (world space) — rebuilt only when stroke pack changes.
  std::vector<StrokeBatch> strokeBatches_;
  std::vector<FillBatch> fillBatches_;
  bool sceneDirty_ = true;

  float scale_ = 1.f;
  float originX_ = 0.f;
  float originY_ = 0.f;
  float widthDips_ = 1.f;
  float heightDips_ = 1.f;
  float scaleX_ = 1.f;
  float scaleY_ = 1.f;
  float cursorX_ = 0.f;
  float cursorY_ = 0.f;
  bool cursorVisible_ = false;
  bool showLineweights_ = true;
};
