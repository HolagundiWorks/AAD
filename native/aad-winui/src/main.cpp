// Interim Win32 + Direct2D host that LoadLibrary's aadt_ffi.dll (ADR-0017).
//
// Controls:
//   Type command + Enter  ·  LINE (no args) then two LMB clicks
//   LMB (idle): append world x,y into the command edit
//   MMB / RMB drag: pan · Wheel: zoom · F: fit · Esc: cancel pick

#include <windows.h>
#include <windowsx.h>
#include <commctrl.h>

#include "Direct2DCanvas.hpp"
#include "FfiSession.hpp"

#include <algorithm>
#include <cctype>
#include <cstdio>
#include <cstring>
#include <optional>
#include <string>
#include <vector>

#ifdef _MSC_VER
#pragma comment(linker, "\"/manifestdependency:type='win32' \
name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'\"")
#endif

static constexpr wchar_t kClassName[] = L"AadWinuiHost";
static constexpr wchar_t kCanvasClass[] = L"AadCanvasHost";
static constexpr int kCliH = 112;   // Chrome 2.0 CLI hero
static constexpr int kTopH = 32;
static constexpr int kStatusH = 22;
static constexpr COLORREF kBrandOrange = RGB(232, 93, 4);
static constexpr COLORREF kInk = RGB(36, 36, 36);
static constexpr COLORREF kHairline = RGB(232, 232, 232);
static constexpr COLORREF kPaper = RGB(250, 250, 250);

enum class PickTool { None, Line };

struct AppState {
  FfiSession session;
  Direct2DCanvas canvas;
  HWND hwnd = nullptr;
  HWND hwndCanvas = nullptr;
  HWND hwndCmd = nullptr;
  HWND hwndLog = nullptr;
  HWND hwndStatus = nullptr;
  bool panning = false;
  int lastX = 0;
  int lastY = 0;
  float cursorWx = 0.f;
  float cursorWy = 0.f;
  PickTool pick = PickTool::None;
  std::optional<std::pair<float, float>> pickP1;
  std::wstring log;
};

static AppState* g_app = nullptr;

static std::string formatCoord(float x, float y) {
  char buf[64];
  std::snprintf(buf, sizeof(buf), "%.4g,%.4g", static_cast<double>(x), static_cast<double>(y));
  return buf;
}

static void appendLog(AppState* app, const std::string& line) {
  if (line.empty()) {
    return;
  }
  app->log.append(line.begin(), line.end());
  app->log.push_back(L'\r');
  app->log.push_back(L'\n');
  // Keep roughly last 3 “screens” of history readable.
  if (app->log.size() > 4096) {
    app->log.erase(0, app->log.size() - 4096);
  }
  if (app->hwndLog) {
    SetWindowTextW(app->hwndLog, app->log.c_str());
    SendMessageW(app->hwndLog, EM_SETSEL, (WPARAM)-1, (LPARAM)-1);
    SendMessageW(app->hwndLog, EM_SCROLLCARET, 0, 0);
  }
}

static void updateStatus(AppState* app) {
  if (!app->hwndStatus) {
    return;
  }
  wchar_t buf[256];
  const wchar_t* tool = L"";
  if (app->pick == PickTool::Line) {
    tool = app->pickP1 ? L"  ·  LINE: click end point" : L"  ·  LINE: click start point";
  }
  std::swprintf(buf, 256, L"ents %llu  rev %llu%s  ·  %.3f, %.3f%s",
                static_cast<unsigned long long>(app->session.entityCount()),
                static_cast<unsigned long long>(app->session.revision()),
                app->session.dirty() ? L"  ·  dirty" : L"",
                static_cast<double>(app->cursorWx), static_cast<double>(app->cursorWy), tool);
  SetWindowTextW(app->hwndStatus, buf);
}

static void refreshCanvas(AppState* app) {
  const auto& lines = app->session.rebuildLines();
  app->canvas.setLines(lines);
  if (app->hwndCanvas) {
    InvalidateRect(app->hwndCanvas, nullptr, FALSE);
  }
  updateStatus(app);
}

static void fitExtents(AppState* app) {
  float b[4]{};
  if (app->session.bounds(b)) {
    app->canvas.fitWorld(b[0], b[1], b[2], b[3]);
  }
  if (app->hwndCanvas) {
    InvalidateRect(app->hwndCanvas, nullptr, FALSE);
  }
}

static void cancelPick(AppState* app) {
  if (app->pick == PickTool::None) {
    return;
  }
  app->pick = PickTool::None;
  app->pickP1.reset();
  appendLog(app, "*Cancel*");
  updateStatus(app);
}

static void feedLine(AppState* app, const std::string& utf8) {
  if (utf8.empty()) {
    return;
  }
  appendLog(app, std::string("Command: ") + utf8);
  const bool ok = app->session.feed(utf8);
  const std::string fb = app->session.lastFeedback();
  if (!fb.empty()) {
    appendLog(app, fb);
  } else if (!ok) {
    appendLog(app, "error");
  }
  refreshCanvas(app);
}

static bool iequals(const std::string& a, const char* b) {
  if (a.size() != std::strlen(b)) {
    return false;
  }
  for (size_t i = 0; i < a.size(); ++i) {
    if (std::tolower(static_cast<unsigned char>(a[i])) !=
        std::tolower(static_cast<unsigned char>(b[i]))) {
      return false;
    }
  }
  return true;
}

static void runCommand(AppState* app) {
  wchar_t buf[512]{};
  GetWindowTextW(app->hwndCmd, buf, 512);
  SetWindowTextW(app->hwndCmd, L"");
  char utf8[1024]{};
  WideCharToMultiByte(CP_UTF8, 0, buf, -1, utf8, sizeof(utf8), nullptr, nullptr);
  if (utf8[0] == '\0') {
    return;
  }
  std::string line(utf8);
  // Trim
  while (!line.empty() && (line.back() == ' ' || line.back() == '\t')) {
    line.pop_back();
  }
  size_t start = 0;
  while (start < line.size() && (line[start] == ' ' || line[start] == '\t')) {
    ++start;
  }
  line = line.substr(start);

  if (iequals(line, "LINE") || iequals(line, "L")) {
    app->pick = PickTool::Line;
    app->pickP1.reset();
    appendLog(app, "Command: LINE");
    appendLog(app, "Specify first point:");
    updateStatus(app);
    SetFocus(app->hwndCanvas);
    return;
  }

  feedLine(app, line);
}

static void appendCoordToCmd(AppState* app, float wx, float wy) {
  const std::string coord = formatCoord(wx, wy);
  wchar_t existing[480]{};
  GetWindowTextW(app->hwndCmd, existing, 480);
  std::wstring next(existing);
  if (!next.empty() && next.back() != L' ') {
    next.push_back(L' ');
  }
  next.append(coord.begin(), coord.end());
  SetWindowTextW(app->hwndCmd, next.c_str());
  SendMessageW(app->hwndCmd, EM_SETSEL, (WPARAM)-1, (LPARAM)-1);
}

static void onCanvasClick(AppState* app, float wx, float wy) {
  if (app->pick == PickTool::Line) {
    if (!app->pickP1) {
      app->pickP1 = std::make_pair(wx, wy);
      appendLog(app, std::string("First point: ") + formatCoord(wx, wy));
      appendLog(app, "Specify second point:");
      updateStatus(app);
      return;
    }
    const auto p1 = *app->pickP1;
    app->pick = PickTool::None;
    app->pickP1.reset();
    char cmd[128];
    std::snprintf(cmd, sizeof(cmd), "LINE %.6g %.6g %.6g %.6g", static_cast<double>(p1.first),
                  static_cast<double>(p1.second), static_cast<double>(wx),
                  static_cast<double>(wy));
    feedLine(app, cmd);
    SetFocus(app->hwndCmd);
    return;
  }
  appendCoordToCmd(app, wx, wy);
  SetFocus(app->hwndCmd);
}

static LRESULT CALLBACK CanvasProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
  AppState* app = g_app;
  switch (msg) {
  case WM_PAINT: {
    PAINTSTRUCT ps{};
    BeginPaint(hwnd, &ps);
    if (app) {
      app->canvas.render();
    }
    EndPaint(hwnd, &ps);
    return 0;
  }
  case WM_LBUTTONDOWN: {
    if (!app) {
      return 0;
    }
    const float wx = app->canvas.worldX(static_cast<float>(GET_X_LPARAM(lParam)));
    const float wy = app->canvas.worldY(static_cast<float>(GET_Y_LPARAM(lParam)));
    onCanvasClick(app, wx, wy);
    return 0;
  }
  case WM_MBUTTONDOWN:
  case WM_RBUTTONDOWN:
    if (app) {
      app->panning = true;
      app->lastX = GET_X_LPARAM(lParam);
      app->lastY = GET_Y_LPARAM(lParam);
      SetCapture(hwnd);
    }
    return 0;
  case WM_MOUSEMOVE:
    if (app) {
      const int x = GET_X_LPARAM(lParam);
      const int y = GET_Y_LPARAM(lParam);
      app->cursorWx = app->canvas.worldX(static_cast<float>(x));
      app->cursorWy = app->canvas.worldY(static_cast<float>(y));
      updateStatus(app);
      if (app->panning) {
        app->canvas.pan(static_cast<float>(x - app->lastX), static_cast<float>(y - app->lastY));
        app->lastX = x;
        app->lastY = y;
        InvalidateRect(hwnd, nullptr, FALSE);
      }
    }
    return 0;
  case WM_MBUTTONUP:
  case WM_RBUTTONUP:
    if (app) {
      app->panning = false;
    }
    ReleaseCapture();
    return 0;
  case WM_MOUSEWHEEL: {
    if (!app) {
      return 0;
    }
    POINT pt{GET_X_LPARAM(lParam), GET_Y_LPARAM(lParam)};
    ScreenToClient(hwnd, &pt);
    const float factor = GET_WHEEL_DELTA_WPARAM(wParam) > 0 ? 1.1f : (1.f / 1.1f);
    app->canvas.zoomAt(factor, static_cast<float>(pt.x), static_cast<float>(pt.y));
    InvalidateRect(hwnd, nullptr, FALSE);
    return 0;
  }
  case WM_KEYDOWN:
    if (!app) {
      break;
    }
    if (wParam == 'F') {
      fitExtents(app);
      return 0;
    }
    if (wParam == VK_ESCAPE) {
      cancelPick(app);
      return 0;
    }
    break;
  }
  return DefWindowProcW(hwnd, msg, wParam, lParam);
}

static void paintTopStrip(HDC hdc, const RECT& rc) {
  RECT top{0, 0, rc.right, kTopH};
  HBRUSH bar = CreateSolidBrush(RGB(255, 255, 255));
  FillRect(hdc, &top, bar);
  DeleteObject(bar);

  // Brand mark — 4px orange focus bar (Chrome 2.0: orange chrome-only).
  RECT brand{0, 0, 4, kTopH};
  HBRUSH orange = CreateSolidBrush(kBrandOrange);
  FillRect(hdc, &brand, orange);
  DeleteObject(orange);

  HPEN pen = CreatePen(PS_SOLID, 1, kHairline);
  HGDIOBJ old = SelectObject(hdc, pen);
  MoveToEx(hdc, 0, kTopH - 1, nullptr);
  LineTo(hdc, rc.right, kTopH - 1);
  SelectObject(hdc, old);
  DeleteObject(pen);

  SetBkMode(hdc, TRANSPARENT);
  SetTextColor(hdc, kInk);
  RECT textRc{14, 0, rc.right - 8, kTopH};
  DrawTextW(hdc, L"AAD", -1, &textRc, DT_LEFT | DT_VCENTER | DT_SINGLELINE);
  // Secondary caption after brand.
  SIZE sz{};
  GetTextExtentPoint32W(hdc, L"AAD", 3, &sz);
  RECT sub{14 + sz.cx + 10, 0, rc.right - 8, kTopH};
  SetTextColor(hdc, RGB(110, 110, 110));
  DrawTextW(hdc, L"Direct2D host  ·  Rust engine (aadt_ffi)", -1, &sub,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS);
}

static LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
  AppState* app = g_app;
  switch (msg) {
  case WM_CREATE: {
    CREATESTRUCTW* cs = reinterpret_cast<CREATESTRUCTW*>(lParam);
    app = reinterpret_cast<AppState*>(cs->lpCreateParams);
    g_app = app;
    app->hwnd = hwnd;

    WNDCLASSEXW cc{};
    cc.cbSize = sizeof(cc);
    cc.lpfnWndProc = CanvasProc;
    cc.hInstance = cs->hInstance;
    cc.hCursor = LoadCursor(nullptr, IDC_CROSS);
    cc.hbrBackground = reinterpret_cast<HBRUSH>(GetStockObject(WHITE_BRUSH));
    cc.lpszClassName = kCanvasClass;
    RegisterClassExW(&cc);

    app->hwndCanvas = CreateWindowExW(
        0, kCanvasClass, L"", WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS, 0, kTopH, 100, 100, hwnd,
        nullptr, cs->hInstance, nullptr);
    app->hwndLog = CreateWindowExW(
        WS_EX_CLIENTEDGE, L"EDIT", L"",
        WS_CHILD | WS_VISIBLE | ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL | WS_VSCROLL,
        0, 0, 100, 40, hwnd, nullptr, cs->hInstance, nullptr);
    app->hwndCmd = CreateWindowExW(WS_EX_CLIENTEDGE, L"EDIT", L"",
                                   WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL, 0, 0, 100, 24, hwnd,
                                   reinterpret_cast<HMENU>(1), cs->hInstance, nullptr);
    app->hwndStatus = CreateWindowExW(0, L"STATIC", L"", WS_CHILD | WS_VISIBLE | SS_LEFT,
                                      0, 0, 100, kStatusH, hwnd, nullptr, cs->hInstance, nullptr);
    SendMessageW(app->hwndCmd, EM_SETCUEBANNER, TRUE,
                 reinterpret_cast<LPARAM>(
                     L"Command — Enter runs · LINE then two clicks · F fit · Esc cancel"));

    if (!app->canvas.init(app->hwndCanvas)) {
      MessageBoxW(hwnd, L"Direct2D init failed", L"AAD", MB_ICONERROR);
    }
    appendLog(app, "AAD host (Win32+Direct2D) — ADR-0017 hybrid");
    appendLog(app, "Engine: aadt_ffi.dll  |  Try: LINE  (then click two points)");
    updateStatus(app);
    SetFocus(app->hwndCmd);
    return 0;
  }
  case WM_SIZE: {
    const int w = LOWORD(lParam);
    const int h = HIWORD(lParam);
    const int statusTop = h - kStatusH;
    const int cliTop = statusTop - kCliH;
    const int canvasH = std::max(1, cliTop - kTopH);
    MoveWindow(app->hwndCanvas, 0, kTopH, w, canvasH, TRUE);
    MoveWindow(app->hwndLog, 8, cliTop + 4, w - 16, kCliH - 40, TRUE);
    MoveWindow(app->hwndCmd, 8, statusTop - 30, w - 16, 24, TRUE);
    MoveWindow(app->hwndStatus, 8, statusTop + 2, w - 16, kStatusH - 4, TRUE);
    app->canvas.resize(static_cast<UINT>(w), static_cast<UINT>(canvasH));
    return 0;
  }
  case WM_ERASEBKGND: {
    HDC hdc = reinterpret_cast<HDC>(wParam);
    RECT rc{};
    GetClientRect(hwnd, &rc);
    HBRUSH paper = CreateSolidBrush(kPaper);
    FillRect(hdc, &rc, paper);
    DeleteObject(paper);
    return 1;
  }
  case WM_PAINT: {
    PAINTSTRUCT ps{};
    BeginPaint(hwnd, &ps);
    RECT rc{};
    GetClientRect(hwnd, &rc);
    paintTopStrip(ps.hdc, rc);
    EndPaint(hwnd, &ps);
    return 0;
  }
  case WM_CTLCOLORSTATIC: {
    HDC hdc = reinterpret_cast<HDC>(wParam);
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, RGB(90, 90, 90));
    return reinterpret_cast<LRESULT>(GetStockObject(NULL_BRUSH));
  }
  case WM_DESTROY:
    PostQuitMessage(0);
    return 0;
  }
  return DefWindowProcW(hwnd, msg, wParam, lParam);
}

int APIENTRY wWinMain(HINSTANCE hInst, HINSTANCE, LPWSTR, int nShow) {
  InitCommonControls();
  AppState app;
  if (!app.session.ok()) {
    std::wstring msg = L"Failed to load aadt_ffi.dll / create session.\n\n";
    msg.append(app.session.loadError().begin(), app.session.loadError().end());
    msg.append(L"\n\nBuild: cargo build -p aadt-ffi --release\n"
               L"Place aadt_ffi.dll next to this exe.");
    MessageBoxW(nullptr, msg.c_str(), L"AAD", MB_ICONERROR);
    return 1;
  }

  WNDCLASSEXW wc{};
  wc.cbSize = sizeof(wc);
  wc.style = CS_HREDRAW | CS_VREDRAW;
  wc.lpfnWndProc = WndProc;
  wc.hInstance = hInst;
  wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
  wc.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
  wc.lpszClassName = kClassName;
  RegisterClassExW(&wc);

  HWND hwnd =
      CreateWindowExW(0, kClassName, L"AAD", WS_OVERLAPPEDWINDOW, CW_USEDEFAULT, CW_USEDEFAULT,
                      1280, 800, nullptr, nullptr, hInst, &app);
  if (!hwnd) {
    return 1;
  }
  ShowWindow(hwnd, nShow);

  app.session.feed("LINE 0 0 10 0");
  app.session.feed("LINE 10 0 10 6");
  app.session.feed("LINE 10 6 0 6");
  app.session.feed("LINE 0 6 0 0");
  refreshCanvas(&app);
  fitExtents(&app);

  MSG msg{};
  while (GetMessageW(&msg, nullptr, 0, 0)) {
    if (msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE) {
      cancelPick(&app);
    }
    if (msg.message == WM_KEYDOWN && msg.wParam == VK_RETURN && GetFocus() == app.hwndCmd) {
      runCommand(&app);
      continue;
    }
    TranslateMessage(&msg);
    DispatchMessageW(&msg);
  }
  return static_cast<int>(msg.wParam);
}
