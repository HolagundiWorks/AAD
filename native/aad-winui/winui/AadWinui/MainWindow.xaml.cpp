#include "pch.h"
#include "MainWindow.xaml.h"
#if __has_include("MainWindow.g.cpp")
#include "MainWindow.g.cpp"
#endif

#include "../../include/AormsBridgeHost.hpp"
#include "../../include/ConnectSession.hpp"

#include <winrt/Microsoft.UI.Xaml.Input.h>
#include <winrt/Microsoft.UI.Xaml.Controls.Primitives.h>
#include <winrt/Microsoft.UI.Xaml.Automation.h>
#include <winrt/Microsoft.UI.Input.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Graphics.h>
#include <winrt/Windows.System.h>

#include <commdlg.h>

#include <winrt/Windows.UI.h>
#include <winrt/Microsoft.UI.Xaml.Media.h>
#include <winrt/Microsoft.UI.Xaml.Media.Imaging.h>
#include <winrt/Windows.Foundation.h>

#include <cctype>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <optional>
#include <sstream>
#include <string>
#include <vector>

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
using namespace Microsoft::UI::Xaml::Controls::Primitives;
using namespace Microsoft::UI::Xaml::Input;
using namespace Microsoft::UI::Xaml::Media;
using namespace Microsoft::UI::Xaml::Media::Imaging;
using namespace Microsoft::UI::Xaml::Automation;
using namespace Microsoft::UI::Input;
using namespace Windows::Foundation;
using namespace Windows::System;
using namespace Windows::UI;

namespace winrt::AadWinui::implementation
{
    namespace {

    std::string trimCopy(std::string s)
    {
        while (!s.empty() && std::isspace(static_cast<unsigned char>(s.front())))
        {
            s.erase(s.begin());
        }
        while (!s.empty() && std::isspace(static_cast<unsigned char>(s.back())))
        {
            s.pop_back();
        }
        return s;
    }

    void aciToRgb(uint8_t aci, uint8_t& r, uint8_t& g, uint8_t& b)
    {
        switch (aci)
        {
        case 0:
        case 7:
        case 250:
            r = g = b = 0;
            break;
        case 1:
            r = 255;
            g = 0;
            b = 0;
            break;
        case 2:
            r = 255;
            g = 255;
            b = 0;
            break;
        case 3:
            r = 0;
            g = 255;
            b = 0;
            break;
        case 4:
            r = 0;
            g = 255;
            b = 255;
            break;
        case 5:
            r = 0;
            g = 0;
            b = 255;
            break;
        case 6:
            r = 255;
            g = 0;
            b = 255;
            break;
        case 8:
            r = g = b = 128;
            break;
        case 9:
            r = g = b = 192;
            break;
        case 251:
            r = g = b = 51;
            break;
        case 252:
            r = g = b = 102;
            break;
        case 253:
            r = g = b = 153;
            break;
        case 254:
            r = g = b = 204;
            break;
        case 255:
            r = g = b = 255;
            break;
        default:
        {
            const int idx = static_cast<int>(aci) - 10;
            const int hue = idx / 10;
            const int rem = idx % 10;
            const double sat = rem < 5 ? 1.0 : 0.5;
            const int light = rem < 5 ? rem : rem - 5;
            const double value = light == 0   ? 0.35
                                 : light == 1 ? 0.50
                                 : light == 2 ? 0.65
                                 : light == 3 ? 0.80
                                              : 1.0;
            const double h = (hue * 15.0) / 60.0;
            const double c = value * sat;
            const double x = c * (1.0 - std::abs(std::fmod(h, 2.0) - 1.0));
            const double m = value - c;
            double r1 = 0, g1 = 0, b1 = 0;
            switch (static_cast<int>(std::floor(h)) % 6)
            {
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
            r = static_cast<uint8_t>(std::lround((r1 + m) * 255.0));
            g = static_cast<uint8_t>(std::lround((g1 + m) * 255.0));
            b = static_cast<uint8_t>(std::lround((b1 + m) * 255.0));
            break;
        }
        }
    }

    char const* shortLtype(std::string const& lt)
    {
        if (lt == "CONTINUOUS")
            return "Cont";
        if (lt == "DASHED")
            return "Dash";
        if (lt == "CENTER")
            return "Ctr";
        if (lt == "HIDDEN")
            return "Hid";
        if (lt == "PHANTOM")
            return "Phan";
        if (lt == "DOT")
            return "Dot";
        return lt.c_str();
    }

    std::string pickOpenPath()
    {
        wchar_t file[MAX_PATH]{};
        OPENFILENAMEW ofn{};
        ofn.lStructSize = sizeof(ofn);
        ofn.lpstrFile = file;
        ofn.nMaxFile = MAX_PATH;
        ofn.lpstrFilter =
            L"Drawings (*.dxf;*.dwg;*.aadt)\0*.dxf;*.dwg;*.aadt\0All\0*.*\0";
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER;
        if (!GetOpenFileNameW(&ofn))
        {
            return {};
        }
        return to_string(hstring{ file });
    }

    std::string pickImagePath()
    {
        wchar_t file[MAX_PATH]{};
        OPENFILENAMEW ofn{};
        ofn.lStructSize = sizeof(ofn);
        ofn.lpstrFile = file;
        ofn.nMaxFile = MAX_PATH;
        ofn.lpstrFilter =
            L"Images (*.png;*.jpg;*.jpeg;*.webp)\0*.png;*.jpg;*.jpeg;*.webp\0All\0*.*\0";
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER;
        if (!GetOpenFileNameW(&ofn))
        {
            return {};
        }
        return to_string(hstring{ file });
    }

    std::string pickSavePath()
    {
        wchar_t file[MAX_PATH]{};
        wcscpy_s(file, L"drawing.dxf");
        OPENFILENAMEW ofn{};
        ofn.lStructSize = sizeof(ofn);
        ofn.lpstrFile = file;
        ofn.nMaxFile = MAX_PATH;
        ofn.lpstrFilter = L"DXF drawing (*.dxf)\0*.dxf\0All\0*.*\0";
        ofn.lpstrDefExt = L"dxf";
        ofn.Flags = OFN_OVERWRITEPROMPT | OFN_EXPLORER;
        if (!GetSaveFileNameW(&ofn))
        {
            return {};
        }
        return to_string(hstring{ file });
    }

    void appendSeg(std::vector<float>& out, float x0, float y0, float x1, float y1)
    {
        out.push_back(x0);
        out.push_back(y0);
        out.push_back(x1);
        out.push_back(y1);
    }

    std::string upperAscii(std::string s)
    {
        for (char& c : s)
        {
            c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
        }
        return s;
    }

    bool tryParseFloat(std::string const& text, double* out)
    {
        if (!out || text.empty())
        {
            return false;
        }
        char* end = nullptr;
        const double v = std::strtod(text.c_str(), &end);
        if (end == text.c_str() || end == nullptr)
        {
            return false;
        }
        while (*end && std::isspace(static_cast<unsigned char>(*end)))
        {
            ++end;
        }
        if (*end != '\0' || !std::isfinite(v))
        {
            return false;
        }
        *out = v;
        return true;
    }

    /// Normalize feet/inch marks: Unicode ′ ″ and curly quotes → ASCII ' "
    std::string normalizeLengthInput(std::string const& text)
    {
        std::string out;
        out.reserve(text.size());
        for (size_t i = 0; i < text.size();)
        {
            auto c = static_cast<unsigned char>(text[i]);
            if (c == 0xE2 && i + 2 < text.size()
                && static_cast<unsigned char>(text[i + 1]) == 0x80)
            {
                auto t = static_cast<unsigned char>(text[i + 2]);
                // ′ (B2) ‘ (98) ’ (99) → '
                if (t == 0xB2 || t == 0x98 || t == 0x99)
                {
                    out.push_back('\'');
                    i += 3;
                    continue;
                }
                // ″ (B3) “ (9C) ” (9D) → "
                if (t == 0xB3 || t == 0x9C || t == 0x9D)
                {
                    out.push_back('"');
                    i += 3;
                    continue;
                }
            }
            out.push_back(static_cast<char>(c));
            ++i;
        }
        return out;
    }

    /// Imperial length → inches. Bare number = inches; `N'` = feet; `N"` = inches;
    /// `N'M"` / `N' M"` = feet + inches. Returns false if not a length token.
    bool tryParseImperialToInches(std::string const& text, double* outInches)
    {
        if (!outInches) return false;
        auto s = trimCopy(normalizeLengthInput(text));
        if (s.empty()) return false;
        // Strip leading @ (relative coords)
        if (s[0] == '@') s = trimCopy(s.substr(1));
        if (s.empty()) return false;

        auto isDig = [](char c) { return c >= '0' && c <= '9'; };
        size_t i = 0;
        if (s[i] == '+' || s[i] == '-') ++i;
        size_t numStart = i;
        if (i >= s.size() || (!isDig(s[i]) && s[i] != '.')) return false;
        while (i < s.size() && (isDig(s[i]) || s[i] == '.')) ++i;
        if (i == numStart) return false;
        double sign = (s[0] == '-') ? -1.0 : 1.0;
        double first = 0;
        try { first = std::stod(s.substr(numStart, i - numStart)); }
        catch (...) { return false; }

        if (i >= s.size())
        {
            // Bare number → inches
            *outInches = sign * first;
            return std::isfinite(*outInches);
        }
        if (s[i] == '\'')
        {
            ++i;
            double inches = 0;
            while (i < s.size() && std::isspace(static_cast<unsigned char>(s[i]))) ++i;
            if (i < s.size() && (isDig(s[i]) || s[i] == '.'))
            {
                size_t j = i;
                while (j < s.size() && (isDig(s[j]) || s[j] == '.')) ++j;
                try { inches = std::stod(s.substr(i, j - i)); }
                catch (...) { return false; }
                i = j;
                while (i < s.size() && std::isspace(static_cast<unsigned char>(s[i]))) ++i;
                if (i < s.size() && s[i] == '"') ++i;
            }
            while (i < s.size() && std::isspace(static_cast<unsigned char>(s[i]))) ++i;
            if (i != s.size()) return false;
            *outInches = sign * (first * 12.0 + inches);
            return std::isfinite(*outInches);
        }
        if (s[i] == '"')
        {
            ++i;
            while (i < s.size() && std::isspace(static_cast<unsigned char>(s[i]))) ++i;
            if (i != s.size()) return false;
            *outInches = sign * first;
            return std::isfinite(*outInches);
        }
        // ft / in suffixes
        auto rest = s.substr(i);
        auto rl = rest;
        for (auto& c : rl) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        if (rl == "ft" || rl == "feet")
        {
            *outInches = sign * first * 12.0;
            return true;
        }
        if (rl == "in" || rl == "inch" || rl == "inches")
        {
            *outInches = sign * first;
            return true;
        }
        return false;
    }

    /// Explicit metric suffix → metres (`2500mm`, `25cm`, `2.5m`).
    bool tryParseExplicitMetricMeters(std::string const& text, double* outMeters)
    {
        if (!outMeters) return false;
        auto s = trimCopy(normalizeLengthInput(text));
        if (s.empty()) return false;
        if (s[0] == '@') s = trimCopy(s.substr(1));
        if (s.empty()) return false;

        size_t i = s.size();
        while (i > 0 && std::isalpha(static_cast<unsigned char>(s[i - 1]))) --i;
        if (i == 0 || i == s.size()) return false;
        auto num = trimCopy(s.substr(0, i));
        auto unit = trimCopy(s.substr(i));
        for (auto& c : unit) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));

        double factor = 0;
        if (unit == "mm" || unit == "millimeter" || unit == "millimeters"
            || unit == "millimetre" || unit == "millimetres")
            factor = 0.001;
        else if (unit == "cm" || unit == "centimeter" || unit == "centimeters"
                 || unit == "centimetre" || unit == "centimetres")
            factor = 0.01;
        else if (unit == "m" || unit == "meter" || unit == "meters"
                 || unit == "metre" || unit == "metres")
            factor = 1.0;
        else
            return false;

        double v = 0;
        if (!tryParseFloat(num, &v)) return false;
        *outMeters = v * factor;
        return std::isfinite(*outMeters);
    }

    /// Drawing-unit length: imperial (ft-in) → model inches; metric → metres.
    /// Explicit `mm`/`cm`/`m`/`ft`/`in`/`'`/`"` always convert, any unit system.
    bool tryParseDrawingLength(std::string const& text, bool imperial, double* out)
    {
        if (!out) return false;
        auto raw = normalizeLengthInput(text);
        constexpr double kInPerM = 39.37007874015748;
        constexpr double kMPerIn = 0.0254;

        double meters = 0;
        if (tryParseExplicitMetricMeters(raw, &meters))
        {
            *out = imperial ? meters * kInPerM : meters;
            return true;
        }

        double inches = 0;
        const bool hasImperialMark =
            raw.find('\'') != std::string::npos || raw.find('"') != std::string::npos;
        auto low = trimCopy(raw);
        for (auto& c : low) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        const bool hasImperialWord =
            low.size() >= 2
            && (low.compare(low.size() - 2, 2, "ft") == 0
                || low.compare(low.size() - 2, 2, "in") == 0
                || (low.size() >= 4 && low.compare(low.size() - 4, 4, "feet") == 0)
                || (low.size() >= 4 && low.compare(low.size() - 4, 4, "inch") == 0)
                || (low.size() >= 6 && low.compare(low.size() - 6, 6, "inches") == 0));

        if (imperial)
        {
            if (tryParseImperialToInches(raw, out)) return true;
            return false;
        }

        // Metric drawing: imperial markers → metres; bare number → metres.
        if ((hasImperialMark || hasImperialWord) && tryParseImperialToInches(raw, &inches))
        {
            *out = inches * kMPerIn;
            return true;
        }
        auto s = trimCopy(raw);
        return tryParseFloat(s, out);
    }

    bool looksLikeLengthToken(std::string const& text)
    {
        auto s = trimCopy(normalizeLengthInput(text));
        if (s.empty()) return false;
        // Has feet/inch mark or unit suffix
        if (s.find('\'') != std::string::npos || s.find('"') != std::string::npos)
            return true;
        auto low = s;
        for (auto& c : low) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
        if (low.size() >= 2)
        {
            auto ends = [&](char const* suf) {
                size_t n = std::strlen(suf);
                return low.size() >= n && low.compare(low.size() - n, n, suf) == 0;
            };
            if (ends("ft") || ends("in") || ends("mm") || ends("cm")
                || (ends("m") && !ends("mm") && !ends("cm")))
                return true;
        }
        // Bare number
        double v = 0;
        return tryParseFloat(s, &v);
    }

    /// True when CLI/dyn text is a length that may still take a unit (`2500 ` → `mm`).
    bool lengthMayTakeUnitSuffix(std::string const& text)
    {
        auto s = trimCopy(text);
        if (s.empty()) return false;
        if (s[0] == '@') s = trimCopy(s.substr(1));
        if (s.empty()) return false;
        size_t i = 0;
        if (s[i] == '+' || s[i] == '-') ++i;
        if (i >= s.size() || ((s[i] < '0' || s[i] > '9') && s[i] != '.')) return false;
        // Already has a unit letter/mark → complete; Space should submit.
        if (s.find('\'') != std::string::npos || s.find('"') != std::string::npos)
            return false;
        for (char c : s)
        {
            if (std::isalpha(static_cast<unsigned char>(c))) return false;
        }
        return true;
    }

    } // namespace

    MainWindow::MainWindow()
    {
        InitializeComponent();

        // Force light chrome so caption + custom title bar stay ink-on-white
        // (system dark theme otherwise washes title text / buttons to white).
        if (auto root = Content().try_as<FrameworkElement>())
            root.RequestedTheme(ElementTheme::Light);

        // Top chrome is the OS app/title bar (drag · system caption buttons).
        ExtendsContentIntoTitleBar(true);
        SetTitleBar(AppTitleBar());
        try
        {
            auto tb = AppWindow().TitleBar();
            // Ink caption buttons on transparent/white title region (HCW light).
            const auto ink = ColorHelper::FromArgb(255, 0x14, 0x15, 0x17);
            const auto muted = ColorHelper::FromArgb(255, 0x5C, 0x63, 0x70);
            const auto soft = ColorHelper::FromArgb(255, 0xEC, 0xEE, 0xF2);
            const auto white = ColorHelper::FromArgb(255, 0xFF, 0xFF, 0xFF);
            const auto edge = ColorHelper::FromArgb(255, 0xD0, 0xD4, 0xDC);
            tb.BackgroundColor(white);
            tb.InactiveBackgroundColor(white);
            tb.ForegroundColor(ink);
            tb.InactiveForegroundColor(muted);
            tb.ButtonBackgroundColor(Colors::Transparent());
            tb.ButtonInactiveBackgroundColor(Colors::Transparent());
            tb.ButtonForegroundColor(ink);
            tb.ButtonInactiveForegroundColor(muted);
            tb.ButtonHoverBackgroundColor(soft);
            tb.ButtonHoverForegroundColor(ink);
            tb.ButtonPressedBackgroundColor(edge);
            tb.ButtonPressedForegroundColor(ink);
        }
        catch (...)
        {
        }

        // Licence SSO: Aorms.Bridge firm.db + Connect session.json (never Activate here).
        {
            auto st = aadt::bridge::ensureAndStatus();
            auto chip = st.bound
                            ? (st.loaded ? L"Licensed · firm.db" : L"Licensed · Connect")
                            : L"Unbound · Connect";
            if (st.syncReady)
                chip = L"Licensed · sync ready";
            ConnectLicenceChip().Text(hstring{ chip });
            auto tip = st.note;
            if (!st.firmDbPath.empty())
                tip += L"\n" + st.firmDbPath;
            if (!st.hubUrl.empty())
                tip += L"\n" + st.hubUrl;
            ToolTipService::SetToolTip(ConnectLicenceChip(), box_value(hstring{ tip }));
            if (!st.bound)
                appendHistory(hstring{ L"! " + st.note });
            else if (st.loaded)
                appendHistory(hstring{ L"Aorms.Bridge firm.db · " + st.hubUrl });
            else
                appendHistory(hstring{ L"Licence from session.json · " + st.hubUrl });
        }

        session_ = std::make_unique<FfiSession>();
        if (!session_->ok())
        {
            StatusBar().Text(
                L"FFI load failed: " +
                to_hstring(session_->loadError()) +
                L"  -  copy aadt_ffi.dll next to the exe");
            appendHistory(L"! " + to_hstring(session_->loadError()));
        }
        else
        {
            session_->osnapSet(true);
            // OTRACK off by default — infinite H/V rays looked like sticky XY axes.
            session_->otrackSet(false);
            session_->feed("LINE 0 0 100 0");
            session_->feed("LINE 100 0 100 60");
            session_->feed("LINE 100 60 0 60");
            session_->feed("LINE 0 60 0 0");
            session_->feed("CIRCLE 50 30 18");
            appendHistory(L"Demo rectangle + circle loaded  -  click an edge, or type LINE.");
            appendHistory(
                L"Note: wheel zoom mid-tool is the WinUI transparent-view equivalent.");
            seedStandardLayersIfSparse();
        }

        initLayerPropEditors();

        CanvasHost().SizeChanged({ this, &MainWindow::CanvasHost_SizeChanged });
        CanvasHost().CompositionScaleChanged({ this, &MainWindow::CanvasHost_CompositionScaleChanged });
        CanvasHost().PointerPressed({ this, &MainWindow::CanvasHost_PointerPressed });
        CanvasHost().PointerMoved({ this, &MainWindow::CanvasHost_PointerMoved });
        CanvasHost().PointerReleased({ this, &MainWindow::CanvasHost_PointerReleased });
        CanvasHost().PointerWheelChanged({ this, &MainWindow::CanvasHost_PointerWheelChanged });
        CanvasHost().PointerEntered([this](auto&&, auto&&)
        {
            paperHovered_ = true;
        });
        CanvasHost().PointerExited([this](auto&&, auto&&)
        {
            paperHovered_ = false;
            canvas_.setCursorDip(0.f, 0.f, false);
            if (canvasReady_)
            {
                canvas_.render();
            }
        });

        CanvasHost().Loaded([this](auto&&, auto&&)
        {
            initCanvas();
            // Crosshair: engine preview + dyn card (ProtectedCursor needs newer WASDK).
            resizeCanvas();
            refreshCanvas(false);
            zoomToExtents(); // initial fit; view lock keeps it across later REGENs
            refreshStatus();
            refreshProperties();
            refreshLayers();
            refreshBlocks();
            refreshLevels();
            refreshFills();
            syncActiveCategoryLayer();
            refreshFirmChip();
            seedDrawingPropsFromConnect();
            // Paper at rest — managers open from Panels / View / lyr chips
            showLeftTab(LeftTab::Layers);
            leftPinned_ = false;
            showLeftPane(false);
            showRightTab(RightTab::Properties);
            showRightPane(false);
            rightPinned_ = false;
            syncAidChips();
            syncViewLockChip();
            syncPanelsChrome();
            refreshUnitsChip();
        });

        CommandLine().Focus(FocusState::Programmatic);
        startClockPomodoro();
    }

    void MainWindow::initCanvas()
    {
        if (canvasReady_)
        {
            return;
        }
        canvasReady_ = canvas_.attach(winrt::get_unknown(CanvasHost()));
        if (!canvasReady_)
        {
            StatusBar().Text(L"SwapChainPanel / Direct2D attach failed");
        }
    }

    void MainWindow::resizeCanvas()
    {
        if (!canvasReady_)
        {
            initCanvas();
        }
        if (!canvasReady_)
        {
            return;
        }
        const auto& panel = CanvasHost();
        const float w = static_cast<float>(panel.ActualWidth());
        const float h = static_cast<float>(panel.ActualHeight());
        if (w < 1.f || h < 1.f)
        {
            return;
        }
        canvas_.resize(w, h, panel.CompositionScaleX(), panel.CompositionScaleY());
        refreshCanvas();
    }

    void MainWindow::refreshCanvas(bool fitIfEmptyView)
    {
        if (!canvasReady_)
        {
            return;
        }
        if (session_ && session_->ok())
        {
            canvas_.setLines(session_->rebuildLines());
            canvas_.setFills(session_->rebuildFills());
            canvas_.setSelection(session_->selectionRebuildLines());
            auto preview = session_->rebuildPreview();
            if (activeTool_ == "XLINE" && session_->toolActive())
            {
                applyXlinePreview(preview);
            }
            if (boxActive_)
            {
                const double x0 = canvas_.worldX(boxX0_);
                const double y0 = canvas_.worldY(boxY0_);
                const double x1 = canvas_.worldX(boxX1_);
                const double y1 = canvas_.worldY(boxY1_);
                auto push = [&](double ax, double ay, double bx, double by) {
                    preview.push_back(static_cast<float>(ax));
                    preview.push_back(static_cast<float>(ay));
                    preview.push_back(static_cast<float>(bx));
                    preview.push_back(static_cast<float>(by));
                };
                push(x0, y0, x1, y0);
                push(x1, y0, x1, y1);
                push(x1, y1, x0, y1);
                push(x0, y1, x0, y0);
            }

            // OTRACK alignment rays only while a tool is active (never idle sticky XY).
            if (session_->toolActive())
            {
                const auto& rays = session_->rebuildOtrackRays();
                preview.insert(preview.end(), rays.begin(), rays.end());
            }
            canvas_.setPreview(preview);

            // Grips: id,idx,x,y → AutoCAD squares in SwapChainCanvas (screen-fixed).
            const auto& grips = session_->rebuildGrips();
            std::vector<float> gripXy;
            gripXy.reserve(grips.size() / 2);
            for (size_t i = 0; i + 3 < grips.size(); i += 4)
            {
                gripXy.push_back(grips[i + 2]);
                gripXy.push_back(grips[i + 3]);
            }
            canvas_.setGrips(gripXy);
            float b[4]{};
            if (session_->bounds(b))
            {
                // View lock: never re-fit on REGEN / draws. First open still fits
                // when unlocked or when caller forces (New / Open via zoomToExtents).
                if (!viewLocked_ && (fitIfEmptyView || !didFit_))
                {
                    canvas_.fitWorld(b[0], b[1], b[2], b[3]);
                    didFit_ = true;
                }
            }
            if (!session_->toolActive())
            {
                clearActiveTool();
            }
        }
        if (!paperHovered_)
        {
            canvas_.setCursorDip(0.f, 0.f, false);
        }
        canvas_.render();
        refreshManagersIfNeeded();
    }

    void MainWindow::zoomToExtents()
    {
        if (!canvasReady_ || !session_ || !session_->ok())
        {
            return;
        }
        // Rebuild geometry first so bounds match the latest drawing.
        canvas_.setLines(session_->rebuildLines());
        canvas_.setFills(session_->rebuildFills());
        float b[4]{};
        if (session_->bounds(b))
        {
            canvas_.fitWorld(b[0], b[1], b[2], b[3]);
            didFit_ = true;
        }
        canvas_.render();
        appendHistory(L"Zoom → extents");
    }

    void MainWindow::syncViewLockChip()
    {
        ViewLockBtn().IsChecked(viewLocked_);
        ViewLockMenuItem().IsChecked(viewLocked_);
        auto accent = Application::Current().Resources().Lookup(box_value(L"HcwAccentBrush")).as<Brush>();
        auto muted = Application::Current().Resources().Lookup(box_value(L"HcwMutedBrush")).as<Brush>();
        auto clear = SolidColorBrush(Windows::UI::ColorHelper::FromArgb(0, 0, 0, 0));
        ViewLockBtn().Foreground(viewLocked_ ? accent : muted);
        ViewLockBtn().BorderBrush(viewLocked_ ? accent : clear);
        AutomationProperties::SetName(
            ViewLockBtn(), viewLocked_ ? L"View lock on" : L"View lock off");
        ToolTipService::SetToolTip(
            ViewLockBtn(),
            box_value(viewLocked_
                          ? hstring{ L"View locked — REGEN keeps pan/zoom (click to unlock)" }
                          : hstring{ L"View unlocked — refresh may fit extents (click to lock)" }));
    }

    void MainWindow::refreshManagersIfNeeded()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        const auto rev = session_->revision();
        if (rev == lastMgrRev_)
        {
            return;
        }
        lastMgrRev_ = rev;
        refreshLayers();
        refreshBlocks();
        refreshLevels();
        refreshFills();
    }

    void MainWindow::initLayerPropEditors()
    {
        if (layerEditorsReady_)
        {
            return;
        }
        layerEditorsReady_ = true;
        layerPropSyncing_ = true;
        LayerColorCombo().Items().Clear();
        struct AciOpt
        {
            char const* label;
            int aci;
        };
        AciOpt const colors[] = {
            { "1 Red", 1 },         { "2 Yellow", 2 },     { "3 Green", 3 },
            { "4 Cyan", 4 },        { "5 Blue", 5 },       { "6 Magenta", 6 },
            { "7 Black/White", 7 }, { "8 Dark gray", 8 },  { "9 Light gray", 9 },
            { "30 Orange", 30 },    { "40 Brown", 40 },    { "140 Teal", 140 },
            { "210 Purple", 210 },  { "252 Mid gray", 252 }
        };
        for (auto const& c : colors)
        {
            auto item = ComboBoxItem();
            item.Content(box_value(to_hstring(c.label)));
            item.Tag(box_value(c.aci));
            LayerColorCombo().Items().Append(item);
        }
        LayerLtypeCombo().Items().Clear();
        for (auto const* lt : { "CONTINUOUS", "DASHED", "CENTER", "HIDDEN", "PHANTOM", "DOT" })
        {
            LayerLtypeCombo().Items().Append(box_value(to_hstring(lt)));
        }
        LayerLwCombo().Items().Clear();
        // ISO 128 / AutoCAD pen ladder (mm)
        for (auto const* lw : { "0.13", "0.18", "0.25", "0.35", "0.50", "0.70", "1.00" })
        {
            LayerLwCombo().Items().Append(box_value(to_hstring(lw)));
        }
        LayerLtypeCombo().SelectedIndex(0);
        LayerLwCombo().SelectedIndex(2); // 0.25 default
        LayerColorCombo().SelectedIndex(6);
        layerPropSyncing_ = false;
    }

    void MainWindow::applyLayerColorSwatch(uint8_t aci)
    {
        uint8_t r = 0, g = 0, b = 0;
        aciToRgb(aci, r, g, b);
        LayerColorSwatch().Background(
            SolidColorBrush(ColorHelper::FromArgb(255, r, g, b)));
        LayerAciBox().Text(to_hstring(std::to_string(static_cast<unsigned>(aci))));
    }

    void MainWindow::syncLayerPropEditors(std::string const& name)
    {
        if (!layerEditorsReady_ || name.empty())
        {
            return;
        }
        for (size_t i = 0; i < layerNames_.size(); ++i)
        {
            if (layerNames_[i] != name)
            {
                continue;
            }
            layerPropSyncing_ = true;
            applyLayerColorSwatch(layerAci_[i]);
            // Match color combo by ACI tag when possible
            auto const& items = LayerColorCombo().Items();
            int match = -1;
            for (uint32_t j = 0; j < items.Size(); ++j)
            {
                if (auto cbi = items.GetAt(j).try_as<ComboBoxItem>())
                {
                    if (unbox_value_or<int>(cbi.Tag(), -1) == static_cast<int>(layerAci_[i]))
                    {
                        match = static_cast<int>(j);
                        break;
                    }
                }
            }
            if (match >= 0)
            {
                LayerColorCombo().SelectedIndex(match);
            }
            auto const& lt = layerLtypes_[i];
            auto litems = LayerLtypeCombo().Items();
            for (uint32_t j = 0; j < litems.Size(); ++j)
            {
                if (to_string(unbox_value<hstring>(litems.GetAt(j))) == lt)
                {
                    LayerLtypeCombo().SelectedIndex(static_cast<int32_t>(j));
                    break;
                }
            }
            char lwbuf[16]{};
            std::snprintf(lwbuf, sizeof(lwbuf), "%.2f", layerLw_[i]);
            auto witems = LayerLwCombo().Items();
            int lwMatch = -1;
            for (uint32_t j = 0; j < witems.Size(); ++j)
            {
                if (to_string(unbox_value<hstring>(witems.GetAt(j))) == lwbuf)
                {
                    lwMatch = static_cast<int>(j);
                    break;
                }
            }
            if (lwMatch >= 0)
            {
                LayerLwCombo().SelectedIndex(lwMatch);
            }
            else
            {
                LayerLwCombo().Items().Append(box_value(to_hstring(lwbuf)));
                LayerLwCombo().SelectedIndex(static_cast<int32_t>(witems.Size()));
            }
            layerPropSyncing_ = false;
            return;
        }
    }

    void MainWindow::seedStandardLayersIfSparse()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        // Fresh drawings only have layer "0" (+ maybe a few) — seed ISO/AIA catalog.
        auto list = session_->layersList();
        size_t n = 0;
        for (char c : list)
        {
            if (c == '\n')
            {
                ++n;
            }
        }
        if (!list.empty() && list.back() != '\n')
        {
            ++n;
        }
        if (n < 8)
        {
            feedCommand("STANDARDS");
            feedCommand("LAYERSET CONSTRUCTION");
            appendHistory(L"Seeded ISO 13567 / AIA layers · set CONSTRUCTION");
        }
    }

    void MainWindow::refreshLayers()
    {
        LayersList().Items().Clear();
        layerNames_.clear();
        layerAci_.clear();
        layerLtypes_.clear();
        layerLw_.clear();
        if (!session_ || !session_->ok())
        {
            LayersList().Items().Append(box_value(L"(engine offline — check aadt_ffi.dll)"));
            return;
        }
        auto filter = to_string(LayerFilterBox().Text());
        for (auto& c : filter)
        {
            c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
        }
        std::istringstream in(session_->layersList());
        std::string line;
        while (std::getline(in, line))
        {
            if (line.empty())
            {
                continue;
            }
            // name\taci\ton\tfrozen\tlocked\tcurrent\tlinetype\tlw_mm
            std::vector<std::string> cols;
            {
                std::string cell;
                std::istringstream row(line);
                while (std::getline(row, cell, '\t'))
                {
                    cols.push_back(cell);
                }
            }
            if (cols.empty())
            {
                continue;
            }
            auto const& name = cols[0];
            if (!filter.empty())
            {
                std::string up = name;
                for (auto& c : up)
                {
                    c = static_cast<char>(std::toupper(static_cast<unsigned char>(c)));
                }
                if (up.find(filter) == std::string::npos)
                {
                    continue;
                }
            }
            uint8_t aci = 7;
            if (cols.size() > 1)
            {
                aci = static_cast<uint8_t>(std::atoi(cols[1].c_str()));
            }
            std::string ltype = cols.size() > 6 ? cols[6] : "CONTINUOUS";
            double lw = cols.size() > 7 ? std::atof(cols[7].c_str()) : 0.25;
            layerNames_.push_back(name);
            layerAci_.push_back(aci);
            layerLtypes_.push_back(ltype);
            layerLw_.push_back(lw);

            std::string label;
            if (cols.size() > 5 && cols[5] == "1")
            {
                label = "> ";
            }
            else
            {
                label = "  ";
            }
            label += name;
            char meta[48]{};
            std::snprintf(
                meta,
                sizeof(meta),
                "  C%-3u %-4s %.2f",
                static_cast<unsigned>(aci),
                shortLtype(ltype),
                lw);
            label += meta;
            if (cols.size() > 2 && cols[2] == "0")
            {
                label += " [off]";
            }
            if (cols.size() > 3 && cols[3] == "1")
            {
                label += " [frz]";
            }
            if (cols.size() > 4 && cols[4] == "1")
            {
                label += " [lck]";
            }
            LayersList().Items().Append(box_value(to_hstring(label)));
        }
        if (LayersList().Items().Size() == 0)
        {
            LayersList().Items().Append(box_value(L"(no layers — Seed ISO/AIA)"));
        }
        if (!pendingLayer_.empty())
        {
            syncLayerPropEditors(pendingLayer_);
        }
    }

    void MainWindow::refreshBlocks()
    {
        BlocksList().Items().Clear();
        elementPlaceNames_.clear();
        if (!session_ || !session_->ok())
        {
            BlocksList().Items().Append(box_value(L"(engine offline — check aadt_ffi.dll)"));
            return;
        }

        std::istringstream blocksIn(session_->blocksList());
        std::string line;
        while (std::getline(blocksIn, line))
        {
            if (line.empty())
            {
                continue;
            }
            BlocksList().Items().Append(box_value(to_hstring(std::string("blk · ") + line)));
            elementPlaceNames_.push_back(line);
        }

        std::istringstream catIn(session_->elementsCatalog());
        while (std::getline(catIn, line))
        {
            if (line.empty())
            {
                continue;
            }
            // name\tkind\tcategory\tdisplay\tpath
            std::vector<std::string> cols;
            {
                std::string cell;
                std::istringstream row(line);
                while (std::getline(row, cell, '\t'))
                {
                    cols.push_back(cell);
                }
            }
            if (cols.empty())
            {
                continue;
            }
            const std::string& name = cols[0];
            std::string display =
                (cols.size() > 3 && !cols[3].empty()) ? cols[3] : name;
            if (cols.size() > 2 && !cols[2].empty())
            {
                display = cols[2] + " · " + display;
            }
            BlocksList().Items().Append(box_value(to_hstring(display)));
            elementPlaceNames_.push_back(name);
        }

        if (BlocksList().Items().Size() == 0)
        {
            BlocksList().Items().Append(
                box_value(hstring{ L"(no elements — catalog empty or library missing)" }));
            elementPlaceNames_.clear();
        }
    }

    void MainWindow::refreshLevels()
    {
        LevelsList().Items().Clear();
        levelNames_.clear();
        if (!session_ || !session_->ok())
        {
            LevelsList().Items().Append(box_value(L"(engine offline — check aadt_ffi.dll)"));
            return;
        }

        // FLOOR with no args lists floors via feedback (ADR-0009).
        session_->feed("FLOOR");
        const auto fb = session_->lastFeedback();
        std::istringstream in(fb);
        std::string line;
        while (std::getline(in, line))
        {
            if (line.empty())
            {
                continue;
            }
            // "* [0] Ground  elev=0.000 m" or "  [1] Level 2  elev=3.000 m"
            const bool active = !line.empty() && line[0] == '*';
            const auto bracket = line.find('[');
            const auto close = line.find(']');
            if (bracket == std::string::npos || close == std::string::npos || close + 1 >= line.size())
            {
                continue;
            }
            std::string rest = trimCopy(line.substr(close + 1));
            auto elevAt = rest.find("  elev=");
            if (elevAt == std::string::npos)
            {
                elevAt = rest.find(" elev=");
            }
            std::string name = elevAt == std::string::npos ? rest : trimCopy(rest.substr(0, elevAt));
            if (name.empty())
            {
                continue;
            }
            std::string elev;
            if (elevAt != std::string::npos)
            {
                elev = trimCopy(rest.substr(elevAt));
            }
            std::string label = (active ? "* " : "  ") + name;
            if (!elev.empty())
            {
                label += "  " + elev;
            }
            if (active)
            {
                activeLevelName_ = name;
            }
            LevelsList().Items().Append(box_value(to_hstring(label)));
            levelNames_.push_back(name);
        }

        // Active category from CATEGORY (no args).
        session_->feed("CATEGORY");
        const auto catFb = session_->lastFeedback();
        // "active: Ground · Walls  (layer shim A-WALL)"
        auto activePos = catFb.find("active:");
        if (activePos != std::string::npos)
        {
            auto after = trimCopy(catFb.substr(activePos + 7));
            auto paren = after.find("  (");
            if (paren == std::string::npos)
            {
                paren = after.find(" (");
            }
            if (paren != std::string::npos)
            {
                after = trimCopy(after.substr(0, paren));
            }
            // Prefer category segment after last ·
            auto mid = after.rfind("·");
            if (mid == std::string::npos)
            {
                mid = after.rfind("-");
            }
            if (mid != std::string::npos && mid + 1 < after.size())
            {
                activeCategoryName_ = trimCopy(after.substr(mid + 1));
                auto floorPart = trimCopy(after.substr(0, mid));
                if (!floorPart.empty())
                {
                    activeLevelName_ = floorPart;
                }
            }
            else if (!after.empty())
            {
                activeCategoryName_ = after;
            }
        }

        LevelCurrentLabel().Text(
            to_hstring(activeLevelName_ + " · " + activeCategoryName_));

        // TRACE (no args) reports current underlay mode without changing it.
        session_->feed("TRACE");
        const auto trFb = session_->lastFeedback();
        if (trFb.find("trace selected floor") != std::string::npos)
        {
            auto arrow = trFb.find("→");
            if (arrow == std::string::npos)
            {
                arrow = trFb.find("->");
            }
            if (arrow != std::string::npos)
            {
                auto name = trimCopy(trFb.substr(arrow + (trFb[arrow] == '-' ? 2 : 1)));
                // strip UTF-8 continuation if arrow was multi-byte "→"
                while (!name.empty() && static_cast<unsigned char>(name.front()) > 0x7f)
                {
                    name.erase(name.begin());
                }
                name = trimCopy(name);
                if (!name.empty())
                {
                    traceMode_ = std::string("FLOOR:") + name;
                }
            }
        }
        else
        {
            const bool below =
                trFb.find("below=on") != std::string::npos
                || trFb.find("below = on") != std::string::npos;
            const bool above =
                trFb.find("above=on") != std::string::npos
                || trFb.find("above = on") != std::string::npos;
            if (below && above)
            {
                traceMode_ = "BOTH";
            }
            else if (below)
            {
                traceMode_ = "BELOW";
            }
            else if (above)
            {
                traceMode_ = "ABOVE";
            }
            else
            {
                traceMode_ = "OFF";
            }
        }

        syncFloorElementTraceCombos();

        if (LevelsList().Items().Size() == 0)
        {
            LevelsList().Items().Append(box_value(L"(no levels — FLOOR seed missing)"));
        }
    }

    void MainWindow::syncFloorElementTraceCombos()
    {
        floorUiSyncing_ = true;

        FloorCombo().Items().Clear();
        int floorSel = -1;
        for (size_t i = 0; i < levelNames_.size(); ++i)
        {
            FloorCombo().Items().Append(box_value(to_hstring(levelNames_[i])));
            if (levelNames_[i] == activeLevelName_)
            {
                floorSel = static_cast<int>(i);
            }
        }
        if (floorSel >= 0)
        {
            FloorCombo().SelectedIndex(floorSel);
        }

        static constexpr char const* kCats[] = {
            "Walls", "Structural", "Lintel", "BeamLayout", "Furniture"};
        ElementCombo().Items().Clear();
        int catSel = 0;
        for (int i = 0; i < 5; ++i)
        {
            ElementCombo().Items().Append(box_value(to_hstring(kCats[i])));
            if (activeCategoryName_ == kCats[i])
            {
                catSel = i;
            }
        }
        ElementCombo().SelectedIndex(catSel);

        TraceCombo().Items().Clear();
        traceComboTags_.clear();
        auto addTrace = [&](char const* label, std::string tag) {
            TraceCombo().Items().Append(box_value(to_hstring(label)));
            traceComboTags_.push_back(std::move(tag));
        };
        addTrace("Off", "OFF");
        addTrace("Below", "BELOW");
        addTrace("Above", "ABOVE");
        addTrace("Both", "BOTH");
        for (auto const& name : levelNames_)
        {
            std::string label = std::string("Ghost: ") + name;
            addTrace(label.c_str(), std::string("FLOOR:") + name);
        }
        int trSel = 0;
        for (size_t i = 0; i < traceComboTags_.size(); ++i)
        {
            if (traceComboTags_[i] == traceMode_)
            {
                trSel = static_cast<int>(i);
                break;
            }
        }
        // Match FLOOR:<name> case-insensitively if exact miss.
        if (trSel == 0 && traceMode_.rfind("FLOOR:", 0) == 0)
        {
            auto want = traceMode_.substr(6);
            for (size_t i = 0; i < traceComboTags_.size(); ++i)
            {
                if (traceComboTags_[i].rfind("FLOOR:", 0) != 0)
                {
                    continue;
                }
                auto have = traceComboTags_[i].substr(6);
                if (have.size() == want.size()
                    && _stricmp(have.c_str(), want.c_str()) == 0)
                {
                    trSel = static_cast<int>(i);
                    break;
                }
            }
        }
        TraceCombo().SelectedIndex(trSel);

        floorUiSyncing_ = false;
    }

    void MainWindow::FloorCombo_SelectionChanged(
        Windows::Foundation::IInspectable const&, SelectionChangedEventArgs const&)
    {
        if (floorUiSyncing_ || !session_ || !session_->ok())
        {
            return;
        }
        const int idx = FloorCombo().SelectedIndex();
        if (idx < 0 || static_cast<size_t>(idx) >= levelNames_.size())
        {
            return;
        }
        auto const& name = levelNames_[static_cast<size_t>(idx)];
        if (name == activeLevelName_)
        {
            return;
        }
        feedCommand(std::string("FLOOR ") + name);
        activeLevelName_ = name;
        appendHistory(to_hstring(std::string("Floor → ") + name));
        syncActiveCategoryLayer();
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::ElementCombo_SelectionChanged(
        Windows::Foundation::IInspectable const&, SelectionChangedEventArgs const&)
    {
        if (floorUiSyncing_ || !session_ || !session_->ok())
        {
            return;
        }
        const int idx = ElementCombo().SelectedIndex();
        if (idx < 0)
        {
            return;
        }
        auto item = ElementCombo().SelectedItem();
        if (!item)
        {
            return;
        }
        auto name = to_string(unbox_value_or<hstring>(item, L""));
        if (name.empty() || name == activeCategoryName_)
        {
            return;
        }
        feedCommand(std::string("CATEGORY ") + name);
        activeCategoryName_ = name;
        appendHistory(to_hstring(std::string("Element → ") + name));
        syncActiveCategoryLayer();
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::syncActiveCategoryLayer()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        char const* shim = "A-WALL";
        if (activeCategoryName_ == "Structural")
        {
            shim = "S-COLS";
        }
        else if (activeCategoryName_ == "Lintel")
        {
            shim = "A-WALL-LINT";
        }
        else if (activeCategoryName_ == "BeamLayout")
        {
            shim = "S-BEAM";
        }
        else if (activeCategoryName_ == "Furniture")
        {
            shim = "A-FURN";
        }
        else if (activeCategoryName_ == "Custom")
        {
            shim = "A-ANNO";
        }
        session_->layerSetCurrent(shim);
        LayerCurrentLabel().Text(to_hstring(shim));
    }

    void MainWindow::TraceCombo_SelectionChanged(
        Windows::Foundation::IInspectable const&, SelectionChangedEventArgs const&)
    {
        if (floorUiSyncing_ || !session_ || !session_->ok())
        {
            return;
        }
        const int idx = TraceCombo().SelectedIndex();
        if (idx < 0 || static_cast<size_t>(idx) >= traceComboTags_.size())
        {
            return;
        }
        auto const& tag = traceComboTags_[static_cast<size_t>(idx)];
        if (tag == traceMode_)
        {
            return;
        }
        std::string cmd;
        if (tag.rfind("FLOOR:", 0) == 0)
        {
            cmd = std::string("TRACE FLOOR ") + tag.substr(6);
        }
        else
        {
            cmd = std::string("TRACE ") + tag;
        }
        feedCommand(cmd);
        traceMode_ = tag;
        appendHistory(to_hstring(std::string("Trace → ") + tag));
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::refreshFills()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        FillsList().Items().Clear();
        fillPatternNames_.clear();
        const auto current = session_->hatchPattern();
        std::istringstream in(session_->hatchPatternsList());
        std::string line;
        while (std::getline(in, line))
        {
            if (line.empty())
            {
                continue;
            }
            // name\tkind\tdescription
            std::vector<std::string> cols;
            {
                std::string cell;
                std::istringstream row(line);
                while (std::getline(row, cell, '\t'))
                {
                    cols.push_back(cell);
                }
            }
            if (cols.empty())
            {
                continue;
            }
            std::string label = cols[0];
            if (cols.size() > 1 && !cols[1].empty())
            {
                label += " Â· ";
                label += cols[1];
            }
            if (!current.empty() && cols[0] == current)
            {
                label = "> " + label;
            }
            FillsList().Items().Append(box_value(to_hstring(label)));
            fillPatternNames_.push_back(cols[0]);
        }
        if (FillsList().Items().Size() == 0)
        {
            FillsList().Items().Append(box_value(hstring{ L"(no hatch patterns)" }));
        }
    }

    void MainWindow::refreshProperties()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        PropsList().Items().Clear();
        const auto ids = session_->selectionIds();
        if (ids.empty())
        {
            PropsEmpty().Visibility(Visibility::Visible);
            PropsList().Visibility(Visibility::Collapsed);
            return;
        }
        PropsEmpty().Visibility(Visibility::Collapsed);
        PropsList().Visibility(Visibility::Visible);
        bool layerSet = false;
        for (size_t sel = 0; sel < ids.size(); ++sel)
        {
            const auto id = ids[sel];
            if (ids.size() > 1)
            {
                PropsList().Items().Append(box_value(
                    hstring{ L"── #" + std::to_wstring(id) + L" ──" }));
            }
            auto summary = session_->entityPropSummary(id);
            if (summary.empty())
            {
                PropsList().Items().Append(box_value(
                    hstring{ L"#" + std::to_wstring(id) + L"  (no properties)" }));
                continue;
            }
            std::istringstream in(summary);
            std::string row;
            while (std::getline(in, row))
            {
                if (row.empty())
                {
                    continue;
                }
                const auto tab = row.find('\t');
                std::string label;
                std::string value;
                if (tab == std::string::npos)
                {
                    label = row;
                }
                else
                {
                    label = row.substr(0, tab);
                    value = row.substr(tab + 1);
                }
                if (!layerSet && label == "Layer")
                {
                    PropsLayerBox().Text(to_hstring(value));
                    layerSet = true;
                }
                // Pad label for a readable two-column look in the list.
                std::wstring line;
                if (label.size() < 12)
                {
                    line = to_hstring(label + std::string(12 - label.size(), ' '));
                }
                else
                {
                    line = to_hstring(label);
                }
                line += L"  ";
                line += to_hstring(value);
                PropsList().Items().Append(box_value(hstring{ line }));
            }
        }
    }

    void MainWindow::startTool(std::string const& cmd)
    {
        if (!session_ || !session_->ok())
        {
            auto err = session_ ? session_->loadError() : std::string{ "no session" };
            appendHistory(L"! Engine offline — " + to_hstring(err));
            return;
        }
        auto token = upperAscii(trimCopy(cmd));
        if (token == "XLH")
        {
            xlineMode_ = XlineMode::Hor;
            token = "XL";
        }
        else if (token == "XLV")
        {
            xlineMode_ = XlineMode::Ver;
            token = "XL";
        }
        else if (token == "XL" || token == "XLINE")
        {
            xlineMode_ = XlineMode::Through;
            token = "XL";
        }
        else
        {
            xlineMode_ = XlineMode::Through;
        }

        if (session_->toolStart(token))
        {
            activeTool_ = upperAscii(trimCopy(token));
            if (activeTool_ == "XL")
            {
                activeTool_ = "XLINE";
            }
            if (activeTool_ == "REC" || activeTool_ == "RECTANG" || activeTool_ == "RECTANGLE")
            {
                activeTool_ = "RECTANGLE";
            }
            toolHasAnchor_ = false;
            rectDimPhase_ = RectDimPhase::None;
            rectDimLength_ = 0.0;
            if (activeTool_ == "XLINE")
            {
                if (xlineMode_ == XlineMode::Hor)
                {
                    appendHistory(L"XLINE Hor — specify a point (Space finishes)");
                }
                else if (xlineMode_ == XlineMode::Ver)
                {
                    appendHistory(L"XLINE Ver — specify a point (Space finishes)");
                }
                else
                {
                    appendHistory(L"XLINE — first point · through · [H]/[V]er · Space ends");
                }
            }
            else if (activeTool_ == "RECTANGLE")
            {
                appendHistory(L"RECTANGLE — first corner · opposite (@W,H) · or D for dimensions");
            }
            else if (activeTool_ == "CIRCLE")
            {
                appendHistory(L"CIRCLE — center, then radius point (or type radius)");
            }
            else if (activeTool_ == "FILLET")
            {
                appendHistory(L"FILLET — type radius (optional), pick first line, then second");
            }
            else if (activeTool_ == "ERASE")
            {
                appendHistory(L"ERASE — pick objects · Space finishes (or Delete on selection)");
            }
            else if (activeTool_ == "ARRAYRECT")
            {
                appendHistory(
                    L"ARRAYRECT — select objects · type rows then cols · origin · col spacing · row spacing");
            }
            else if (activeTool_ == "ARRAYPOLAR")
            {
                appendHistory(L"ARRAYPOLAR — select objects · type count · pick center");
            }
            else if (activeTool_ == "ARRAYPATH")
            {
                appendHistory(L"ARRAYPATH — select objects · type count · pick path · Space");
            }
            else
            {
                appendHistory(to_hstring(token + " — specify first point (click · Space finishes)"));
            }
            if (activeTool_ == "DOOR" || activeTool_ == "WINDOW")
            {
                appendHistory(L"Tab — toggle swing flip while placing");
            }
            CommandLine().Focus(FocusState::Programmatic);
            refreshCanvas();
            refreshStatus();
            updateDynOverlay(lastX_, lastY_, canvas_.worldX(lastX_), canvas_.worldY(lastY_));
        }
        else
        {
            appendHistory(to_hstring(std::string("! cannot start tool ") + token));
        }
    }

    void MainWindow::startImageTool()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        auto path = pickImagePath();
        if (path.empty())
        {
            return;
        }
        if (session_->toolStartImage(path))
        {
            activeTool_ = "IMAGE";
            appendHistory(to_hstring(std::string("IMAGE ") + path + " (click to place)"));
            refreshCanvas();
            refreshStatus();
        }
        else
        {
            appendHistory(L"! cannot start IMAGE tool");
        }
    }

    bool MainWindow::tryTogglePlaceFlip()
    {
        if (!session_ || !session_->ok() || !session_->toolActive())
        {
            return false;
        }
        if (activeTool_ != "DOOR" && activeTool_ != "WINDOW")
        {
            return false;
        }
        const bool on = !session_->placeFlipGet();
        session_->placeFlipSet(on);
        appendHistory(on ? L"Swing flip on (Tab)" : L"Swing flip off (Tab)");
        refreshStatus();
        return true;
    }

    bool MainWindow::tryCycleGripMode()
    {
        if (!session_ || !session_->ok() || session_->toolActive())
        {
            return false;
        }
        if (session_->selectionCount() == 0)
        {
            return false;
        }
        const auto& grips = session_->rebuildGrips();
        if (grips.empty())
        {
            return false;
        }
        session_->gripModeCycle();
        const auto mode = session_->gripMode();
        appendHistory(to_hstring(std::string("Grip mode → ") + (mode.empty() ? "?" : mode)));
        refreshStatus();
        return true;
    }

    bool MainWindow::submitLikeEnter()
    {
        if (!session_ || !session_->ok())
        {
            return false;
        }
        if (session_->toolActive())
        {
            if (session_->toolFinish())
            {
                clearActiveTool();
                appendHistory(L"(Space/Enter) finish");
                refreshCanvas(true);
                refreshStatus();
                refreshProperties();
                return true;
            }
            return false;
        }
        if (!cmdHistory_.empty())
        {
            feedCommand(cmdHistory_.back());
            return true;
        }
        return tryCycleGripMode();
    }

    bool MainWindow::allowsLiteralSpace()
    {
        auto focused = Microsoft::UI::Xaml::Input::FocusManager::GetFocusedElement(
            Content().XamlRoot());
        auto tb = focused.try_as<TextBox>();
        if (!tb)
        {
            return false;
        }
        // Free-type fields — Space is a character, not Enter (AutoCAD dialog peer).
        auto same = [&](TextBox const& other)
        {
            return get_abi(tb) == get_abi(other);
        };
        return same(LayerNewBox()) || same(LevelNewBox()) || same(LayerFilterBox()) ||
               same(LayerAciBox()) || same(PropsLayerBox()) || same(HistoryLabelBox()) ||
               same(LispExprBox());
    }

    bool MainWindow::acceptSpaceAsEnter()
    {
        hideCompletions();
        // Dyn card value first when open (cursor prompt).
        if (DynInputCard().Visibility() == Visibility::Visible)
        {
            auto dyn = trimCopy(to_string(DynInputBox().Text()));
            if (!dyn.empty())
            {
                DynInputBox().Text(L"");
                applyDynOrCommand(dyn);
                return true;
            }
        }
        auto text = trimCopy(to_string(CommandLine().Text()));
        if (!text.empty())
        {
            CommandLine().Text(L"");
            applyDynOrCommand(text);
            return true;
        }
        return submitLikeEnter();
    }

    void MainWindow::clearActiveTool()
    {
        activeTool_.clear();
        xlineMode_ = XlineMode::Through;
        toolHasAnchor_ = false;
        rectDimPhase_ = RectDimPhase::None;
        rectDimLength_ = 0.0;
        DynInputCard().Visibility(Visibility::Collapsed);
        DynOverlayCanvas().IsHitTestVisible(false);
    }

    void MainWindow::applyXlinePreview(std::vector<float>& preview)
    {
        preview.clear();
        const float w = static_cast<float>(CanvasHost().ActualWidth());
        const float h = static_cast<float>(CanvasHost().ActualHeight());
        if (w < 2.f || h < 2.f)
        {
            return;
        }
        // Pad past the visible paper so the line reads as infinite.
        const double xL = canvas_.worldX(-w);
        const double xR = canvas_.worldX(w * 2.f);
        const double yT = canvas_.worldY(-h);
        const double yB = canvas_.worldY(h * 2.f);
        const double cx = canvas_.worldX(lastX_);
        const double cy = canvas_.worldY(lastY_);
        auto push = [&](double ax, double ay, double bx, double by) {
            preview.push_back(static_cast<float>(ax));
            preview.push_back(static_cast<float>(ay));
            preview.push_back(static_cast<float>(bx));
            preview.push_back(static_cast<float>(by));
        };
        if (xlineMode_ == XlineMode::Hor)
        {
            push(xL, cy, xR, cy);
            return;
        }
        if (xlineMode_ == XlineMode::Ver)
        {
            push(cx, yT, cx, yB);
            return;
        }
        // Through: infinite line from first point through cursor (or H+V guide).
        if (toolHasAnchor_)
        {
            const double dx = cx - toolAnchorX_;
            const double dy = cy - toolAnchorY_;
            const double len = std::hypot(dx, dy);
            if (len > 1e-12)
            {
                const double ux = dx / len;
                const double uy = dy / len;
                const double span = std::hypot(xR - xL, yB - yT) * 2.0;
                push(toolAnchorX_ - ux * span, toolAnchorY_ - uy * span,
                     toolAnchorX_ + ux * span, toolAnchorY_ + uy * span);
                return;
            }
        }
        push(xL, cy, xR, cy);
        push(cx, yT, cx, yB);
    }

    bool MainWindow::parseCommaPair(std::string const& text, double* a, double* b) const
    {
        if (!a || !b)
        {
            return false;
        }
        auto s = trimCopy(text);
        if (!s.empty() && s[0] == '@')
        {
            s.erase(s.begin());
            s = trimCopy(s);
        }
        const auto comma = s.find(',');
        if (comma == std::string::npos)
        {
            // Also accept "W H"
            const auto sp = s.find_first_of(" \t");
            if (sp == std::string::npos)
            {
                return false;
            }
            try
            {
                *a = std::stod(trimCopy(s.substr(0, sp)));
                *b = std::stod(trimCopy(s.substr(sp + 1)));
                return true;
            }
            catch (...)
            {
                return false;
            }
        }
        try
        {
            *a = std::stod(trimCopy(s.substr(0, comma)));
            *b = std::stod(trimCopy(s.substr(comma + 1)));
            return true;
        }
        catch (...)
        {
            return false;
        }
    }

    bool MainWindow::tryRectangleDimensionInput(std::string const& utf8)
    {
        if (activeTool_ != "RECTANGLE" || !session_ || !session_->ok() || !session_->toolActive())
        {
            return false;
        }
        auto up = upperAscii(trimCopy(utf8));
        if (up == "D" || up == "DIM" || up == "DIMENSIONS")
        {
            rectDimPhase_ = RectDimPhase::Length;
            appendHistory(L"RECTANGLE — type length (width) then Enter");
            DynPromptText().Text(L"RECTANGLE — length");
            return true;
        }
        const bool imperial = (unitsToken_ == "ft-in");
        if (rectDimPhase_ == RectDimPhase::Length)
        {
            double len = 0.0;
            if (!tryParseDrawingLength(trimCopy(utf8), imperial, &len) || std::abs(len) < 1e-12)
            {
                return false;
            }
            rectDimLength_ = len;
            rectDimPhase_ = RectDimPhase::Width;
            appendHistory(to_hstring(std::string("RECTANGLE length → ") + trimCopy(utf8)));
            DynPromptText().Text(
                imperial ? L"RECTANGLE — width (bare=in · '=ft · \"=in)"
                         : L"RECTANGLE — width");
            appendHistory(L"RECTANGLE — type width then Enter");
            return true;
        }
        if (rectDimPhase_ == RectDimPhase::Width)
        {
            double wid = 0.0;
            if (!tryParseDrawingLength(trimCopy(utf8), imperial, &wid) || std::abs(wid) < 1e-12)
            {
                return false;
            }
            if (!toolHasAnchor_)
            {
                rectDimPhase_ = RectDimPhase::None;
                appendHistory(L"RECTANGLE — click first corner, then dimensions apply");
                // Keep length/width: store width in rectDimLength_ pair via pending
                // — require corner first. Reset and ask for corner.
                rectDimLength_ = 0.0;
                return true;
            }
            const double x1 = toolAnchorX_ + rectDimLength_;
            const double y1 = toolAnchorY_ + wid;
            char buf[160]{};
            std::snprintf(
                buf,
                sizeof(buf),
                "RECTANGLE %.9g %.9g %.9g %.9g",
                toolAnchorX_,
                toolAnchorY_,
                x1,
                y1);
            session_->toolCancel();
            clearActiveTool();
            feedCommand(buf);
            appendHistory(to_hstring(
                std::string("RECTANGLE ") + std::to_string(rectDimLength_) + " × " +
                trimCopy(utf8)));
            refreshCanvas();
            refreshStatus();
            return true;
        }
        // Opposite corner as @W,H or W,H (relative to first corner).
        if (toolHasAnchor_)
        {
            double dx = 0.0;
            double dy = 0.0;
            if (parseCommaPair(utf8, &dx, &dy))
            {
                const bool relative = !trimCopy(utf8).empty() &&
                                      (trimCopy(utf8)[0] == '@' || utf8.find(',') != std::string::npos ||
                                       utf8.find(' ') != std::string::npos);
                double x1 = dx;
                double y1 = dy;
                if (relative)
                {
                    x1 = toolAnchorX_ + dx;
                    y1 = toolAnchorY_ + dy;
                }
                if (session_->toolAcceptPoint(x1, y1))
                {
                    appendHistory(to_hstring(std::string("RECTANGLE corner → ") + trimCopy(utf8)));
                }
                else
                {
                    char buf[160]{};
                    std::snprintf(
                        buf,
                        sizeof(buf),
                        "RECTANGLE %.9g %.9g %.9g %.9g",
                        toolAnchorX_,
                        toolAnchorY_,
                        x1,
                        y1);
                    session_->toolCancel();
                    clearActiveTool();
                    feedCommand(buf);
                }
                refreshCanvas();
                refreshStatus();
                return true;
            }
        }
        return false;
    }

    void MainWindow::stylePaneTab(Button const& btn, bool active)
    {
        auto res = Application::Current().Resources();
        auto accent = res.Lookup(box_value(L"HcwAccentBrush")).as<Brush>();
        auto muted = res.Lookup(box_value(L"HcwMutedBrush")).as<Brush>();
        auto ink = res.Lookup(box_value(L"HcwInkBrush")).as<Brush>();
        auto clear = SolidColorBrush(Windows::UI::ColorHelper::FromArgb(0, 0, 0, 0));
        btn.BorderBrush(active ? accent : clear);
        btn.Foreground(active ? ink : muted);
        // WCAG 1.3.1 / 4.1.2 — selected state beyond colour underline
        AutomationProperties::SetItemStatus(btn, active ? L"selected" : L"");
    }

    void MainWindow::syncAidChips()
    {
        AidOsnap().IsChecked(osnapOn_);
        bool ortho = false;
        bool polar = false;
        if (session_ && session_->ok())
        {
            ortho = session_->orthoGet();
            polar = session_->polarGet();
        }
        AidOrtho().IsChecked(ortho);
        AidPolar().IsChecked(polar);
        AidDyn().IsChecked(dynMode_);
        AidLwt().IsChecked(canvas_.showLineweights());
        auto accent = Application::Current().Resources().Lookup(box_value(L"HcwAccentBrush")).as<Brush>();
        auto muted = Application::Current().Resources().Lookup(box_value(L"HcwMutedBrush")).as<Brush>();
        auto clear = SolidColorBrush(Windows::UI::ColorHelper::FromArgb(0, 0, 0, 0));
        auto styleChip = [&](ToggleButton const& t, bool on)
        {
            t.Foreground(on ? accent : muted);
            t.BorderBrush(on ? accent : clear);
        };
        styleChip(AidOsnap(), osnapOn_);
        styleChip(AidOrtho(), ortho);
        styleChip(AidPolar(), polar);
        styleChip(AidDyn(), dynMode_);
        styleChip(AidLwt(), canvas_.showLineweights());
        // WCAG 4.1.2 — name includes toggle state (not colour alone)
        AutomationProperties::SetName(
            AidOsnap(), osnapOn_ ? L"Object snap on" : L"Object snap off");
        AutomationProperties::SetName(
            AidOrtho(), ortho ? L"Ortho mode on" : L"Ortho mode off");
        AutomationProperties::SetName(
            AidPolar(), polar ? L"Polar tracking on" : L"Polar tracking off");
        AutomationProperties::SetName(
            AidDyn(), dynMode_ ? L"Dynamic input on" : L"Dynamic input off");
    }

    void MainWindow::placeXlineAt(double wx, double wy)
    {
        char buf[160]{};
        if (xlineMode_ == XlineMode::Hor)
        {
            std::snprintf(buf, sizeof(buf), "XLINE %.9g %.9g 1 0", wx, wy);
        }
        else
        {
            std::snprintf(buf, sizeof(buf), "XLINE %.9g %.9g 0 1", wx, wy);
        }
        const auto mode = xlineMode_;
        feedCommand(buf);
        // Do not fitWorld — XLINE extents are huge; keep the current paper view.
        if (session_ && session_->ok() && session_->toolStart("XL"))
        {
            activeTool_ = "XLINE";
            xlineMode_ = mode;
            toolHasAnchor_ = false;
            appendHistory(
                mode == XlineMode::Hor ? L"XLINE Hor — specify a point (Space finishes)"
                                       : L"XLINE Ver — specify a point (Space finishes)");
            refreshCanvas(false);
            refreshStatus();
        }
    }

    bool MainWindow::tryApplyTypedLength(std::string const& utf8)
    {
        if (!session_ || !session_->ok())
        {
            return false;
        }
        const auto trimmed = trimCopy(normalizeLengthInput(utf8));
        if (trimmed.empty())
        {
            return false;
        }
        // Never treat transparent view cmds ('ZOOM) as lengths.
        if (!trimmed.empty() && trimmed.front() == '\'')
        {
            // Leading apostrophe alone starts transparent zoom — only if more than marks.
            // `25'` has digits before ' — not transparent.
            bool digitBefore = false;
            for (char c : trimmed)
            {
                if (c >= '0' && c <= '9')
                {
                    digitBefore = true;
                    break;
                }
                if (c == '\'') break;
            }
            if (!digitBefore)
            {
                return false;
            }
        }

        const bool imperial = (unitsToken_ == "ft-in");
        double len = 0.0;
        if (!tryParseDrawingLength(trimmed, imperial, &len) || std::abs(len) < 1e-12)
        {
            return false;
        }

        // Scalar tools (before a point anchor): fillet radius, wall thickness, …
        if (session_->toolActive() && !toolHasAnchor_
            && (activeTool_ == "FILLET" || activeTool_ == "WALL" || activeTool_ == "OFFSET"
                || activeTool_ == "IMAGE"))
        {
            if (session_->toolSetNumber(len))
            {
                appendHistory(to_hstring(std::string(activeTool_ + " → ") + trimmed));
                refreshCanvas(false);
                refreshStatus();
                return true;
            }
        }

        if (!session_->toolActive() || !toolHasAnchor_)
        {
            // Length typed with no draw target — don't send unit tokens to the engine as commands.
            auto low = trimmed;
            for (auto& c : low) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            const bool hasUnit =
                trimmed.find('\'') != std::string::npos || trimmed.find('"') != std::string::npos
                || low.find("ft") != std::string::npos || low.find("in") != std::string::npos
                || (low.size() >= 2
                    && (low.compare(low.size() - 2, 2, "mm") == 0
                        || low.compare(low.size() - 2, 2, "cm") == 0
                        || (low.back() == 'm' && low.compare(low.size() - 2, 2, "mm") != 0
                            && low.compare(low.size() - 2, 2, "cm") != 0)));
            if (hasUnit)
            {
                appendHistory(to_hstring(
                    std::string("Length ") + trimmed
                    + " — pick first point, then type distance (2500mm · 25cm · 2.5m · 25' · 6\")"));
                return true;
            }
            return false;
        }

        // CIRCLE: typed radius from center.
        if (activeTool_ == "CIRCLE")
        {
            if (session_->toolAcceptPoint(toolAnchorX_ + len, toolAnchorY_))
            {
                appendHistory(to_hstring(std::string("CIRCLE radius → ") + trimmed));
                toolHasAnchor_ = false;
                refreshCanvas(false);
                refreshStatus();
                return true;
            }
            return false;
        }

        // Distance from last point toward cursor (ORTHO clamps to axis).
        double cx = canvas_.worldX(lastX_);
        double cy = canvas_.worldY(lastY_);
        double dx = cx - toolAnchorX_;
        double dy = cy - toolAnchorY_;
        if (session_->orthoGet())
        {
            if (std::abs(dx) >= std::abs(dy))
                dy = 0;
            else
                dx = 0;
        }
        double mag = std::hypot(dx, dy);
        if (mag < 1e-9)
        {
            dx = 1.0;
            dy = 0.0;
            mag = 1.0;
        }
        const double nx = toolAnchorX_ + dx / mag * len;
        const double ny = toolAnchorY_ + dy / mag * len;
        if (!session_->toolAcceptPoint(nx, ny))
        {
            return false;
        }
        {
            char buf[96]{};
            std::snprintf(buf, sizeof(buf), "Distance %s → %.4g %s", trimmed.c_str(), len,
                          imperial ? "in" : "m");
            appendHistory(to_hstring(buf));
        }
        // Chain tools: next typed distance starts from the new point.
        if (activeTool_ == "LINE" || activeTool_ == "PL" || activeTool_ == "POLYLINE"
            || activeTool_ == "WALL" || activeTool_ == "XLINE")
        {
            toolHasAnchor_ = true;
            toolAnchorX_ = nx;
            toolAnchorY_ = ny;
        }
        else if (activeTool_ == "RECTANGLE")
        {
            toolHasAnchor_ = false;
        }
        refreshCanvas(false);
        refreshStatus();
        refreshProperties();
        return true;
    }

    void MainWindow::applyDynOrCommand(std::string const& utf8)
    {
        if (utf8.empty())
        {
            return;
        }
        const auto normalized = normalizeLengthInput(utf8);
        // Single keyword while XLINE through is active → switch mode.
        auto up = upperAscii(trimCopy(normalized));
        if (activeTool_ == "XLINE" && session_ && session_->toolActive())
        {
            if (up == "H" || up == "HOR")
            {
                xlineMode_ = XlineMode::Hor;
                appendHistory(L"XLINE Hor — specify a point");
                refreshStatus();
                refreshCanvas();
                return;
            }
            if (up == "V" || up == "VER")
            {
                xlineMode_ = XlineMode::Ver;
                appendHistory(L"XLINE Ver — specify a point");
                refreshStatus();
                refreshCanvas();
                return;
            }
            if (up == "T" || up == "THROUGH")
            {
                xlineMode_ = XlineMode::Through;
                appendHistory(L"XLINE Through — origin then through points");
                refreshStatus();
                refreshCanvas();
                return;
            }
        }
        if (tryRectangleDimensionInput(normalized))
        {
            return;
        }
        if (tryApplyTypedLength(normalized))
        {
            return;
        }
        feedCommand(normalized);
    }

    void MainWindow::updateDynOverlay(float screenX, float screenY, double worldX, double worldY)
    {
        if (!dynMode_ || !session_ || !session_->ok() || !session_->toolActive())
        {
            DynInputCard().Visibility(Visibility::Collapsed);
            DynOverlayCanvas().IsHitTestVisible(false);
            return;
        }
        DynOverlayCanvas().IsHitTestVisible(true);
        DynInputCard().Visibility(Visibility::Visible);
        auto prompt = session_->toolPrompt();
        if (prompt.empty())
        {
            prompt = "Specify point";
        }
        if (xlineMode_ == XlineMode::Hor && activeTool_ == "XLINE")
        {
            prompt = "XLINE Hor — specify a point";
        }
        else if (xlineMode_ == XlineMode::Ver && activeTool_ == "XLINE")
        {
            prompt = "XLINE Ver — specify a point";
        }
        else if (activeTool_ == "RECTANGLE")
        {
            if (rectDimPhase_ == RectDimPhase::Length)
            {
                prompt = "RECTANGLE — length";
            }
            else if (rectDimPhase_ == RectDimPhase::Width)
            {
                prompt = "RECTANGLE — width";
            }
            else if (toolHasAnchor_)
            {
                prompt = "RECTANGLE — opposite (@W,H) or D";
            }
            else
            {
                prompt = "RECTANGLE — first corner";
            }
        }
        DynPromptText().Text(to_hstring(prompt));
        wchar_t coords[80]{};
        swprintf_s(coords, L"%.2f, %.2f", worldX, worldY);
        DynCoordsText().Text(coords);
        const float cardW = 200.f;
        const float cardH = 78.f;
        float left = screenX + 16.f;
        float top = screenY + 16.f;
        const float hostW = static_cast<float>(CanvasHost().ActualWidth());
        const float hostH = static_cast<float>(CanvasHost().ActualHeight());
        if (left + cardW > hostW)
        {
            left = screenX - cardW - 12.f;
        }
        if (top + cardH > hostH)
        {
            top = screenY - cardH - 12.f;
        }
        if (left < 4.f)
        {
            left = 4.f;
        }
        if (top < 4.f)
        {
            top = 4.f;
        }
        Canvas::SetLeft(DynInputCard(), left);
        Canvas::SetTop(DynInputCard(), top);
    }

    void MainWindow::pushCmdHistory(std::string const& line)
    {
        if (line.empty())
        {
            return;
        }
        if (!cmdHistory_.empty() && cmdHistory_.back() == line)
        {
            historyIndex_ = -1;
            return;
        }
        cmdHistory_.push_back(line);
        historyIndex_ = -1;
    }

    void MainWindow::feedCommand(std::string const& utf8)
    {
        if (utf8.empty())
        {
            return;
        }
        if (!session_ || !session_->ok())
        {
            auto err = session_ ? session_->loadError() : std::string{ "no session" };
            appendHistory(L"! Engine offline — " + to_hstring(err));
            StatusBar().Text(L"FFI offline — copy aadt_ffi.dll next to AadWinui.exe");
            return;
        }

        pushCmdHistory(utf8);

        if (session_->toolNeedsText())
        {
            if (session_->toolSetText(utf8))
            {
                appendHistory(to_hstring(std::string("TEXT → ") + utf8));
                refreshCanvas(true);
                refreshStatus();
                refreshProperties();
                return;
            }
        }

        const auto trimmed = trimCopy(normalizeLengthInput(utf8));
        // Typed feet/inches (25') while a tool wants a distance — never feed as a command.
        if (tryApplyTypedLength(trimmed))
        {
            return;
        }
        double number = 0.0;
        const bool toolActive = session_->toolActive();
        const bool countTool = activeTool_ == "ARRAYRECT" || activeTool_ == "ARRAYPOLAR"
                               || activeTool_ == "ARRAYPATH";
        const bool lengthTool = activeTool_ == "FILLET" || activeTool_ == "WALL"
                                || activeTool_ == "OFFSET" || activeTool_ == "IMAGE";
        const bool numberTool = !toolActive || countTool || lengthTool;
        const bool imperial = (unitsToken_ == "ft-in");
        const bool parsed = countTool
                                ? tryParseFloat(trimmed, &number)
                                : lengthTool
                                      ? tryParseDrawingLength(trimmed, imperial, &number)
                                      : (!toolActive && tryParseFloat(trimmed, &number));
        if (numberTool && parsed && session_->toolSetNumber(number))
        {
            if (activeTool_ == "ARRAYRECT")
            {
                appendHistory(to_hstring(std::string("Array rows/cols → ") + trimmed));
            }
            else if (activeTool_ == "ARRAYPOLAR" || activeTool_ == "ARRAYPATH")
            {
                appendHistory(to_hstring(std::string("Array count → ") + trimmed));
            }
            else if (activeTool_ == "FILLET")
            {
                appendHistory(to_hstring(std::string("Fillet radius → ") + trimmed));
            }
            else
            {
                appendHistory(to_hstring(std::string("Number → ") + trimmed));
            }
            refreshCanvas();
            refreshStatus();
            return;
        }

        if (session_->isInteractive(utf8))
        {
            const auto up = upperAscii(trimCopy(utf8));
            if (up == "IMAGE" || up == "PLACEIMAGE" || up == "IMAGEATTACH")
            {
                startImageTool();
                return;
            }
            startTool(utf8);
            return;
        }

        appendHistory(to_hstring(std::string("Command: ") + utf8));
        if (!session_->feed(utf8))
        {
            appendHistory(L"! feed failed");
            const auto fb = session_->lastFeedback();
            if (!fb.empty())
            {
                appendHistory(to_hstring(fb));
            }
        }
        else
        {
            const auto fb = session_->lastFeedback();
            if (!fb.empty())
            {
                appendHistory(to_hstring(fb));
            }
        }
        applyPendingNav();
        // Keep current pan/zoom across REGEN (view lock / default).
        refreshCanvas(false);
        refreshStatus();
        refreshProperties();
        refreshManagersIfNeeded();
    }

    void MainWindow::applyPendingNav()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        const auto hint = session_->takeNav();
        if (hint.empty())
        {
            return;
        }
        if (hint == "extents" || hint == "previous")
        {
            float b[4]{};
            if (session_->bounds(b))
            {
                canvas_.fitWorld(b[0], b[1], b[2], b[3]);
                didFit_ = true;
            }
            appendHistory(L"Nav → extents");
            return;
        }
        if (hint == "zoom_window")
        {
            zoomWindowPending_ = true;
            appendHistory(L"'ZOOM — drag a window on the paper");
            return;
        }
        if (hint == "pan")
        {
            appendHistory(L"'PAN — middle-drag to pan (tool stays active)");
            return;
        }
        if (hint.rfind("scale:", 0) == 0)
        {
            try
            {
                const float f = static_cast<float>(std::stod(hint.substr(6)));
                if (f > 1e-6f)
                {
                    const float px =
                        static_cast<float>(CanvasHost().ActualWidth()) * 0.5f;
                    const float py =
                        static_cast<float>(CanvasHost().ActualHeight()) * 0.5f;
                    canvas_.zoomAt(f, px, py);
                }
            }
            catch (...)
            {
            }
            appendHistory(to_hstring(std::string("Nav → ") + hint));
            return;
        }
        if (hint.rfind("window:", 0) == 0)
        {
            const auto rest = hint.substr(7);
            float vals[4]{};
            int n = 0;
            std::stringstream ss(rest);
            std::string part;
            while (std::getline(ss, part, ',') && n < 4)
            {
                try
                {
                    vals[n++] = static_cast<float>(std::stod(part));
                }
                catch (...)
                {
                    break;
                }
            }
            if (n == 4)
            {
                canvas_.fitWorld(
                    (std::min)(vals[0], vals[2]),
                    (std::min)(vals[1], vals[3]),
                    (std::max)(vals[0], vals[2]),
                    (std::max)(vals[1], vals[3]));
                didFit_ = true;
            }
            appendHistory(L"Nav → zoom window");
            return;
        }
        if (hint.rfind("pan:", 0) == 0)
        {
            appendHistory(to_hstring(std::string("Nav → ") + hint));
        }
    }

    void MainWindow::hideCompletions()
    {
        CompleteList().Items().Clear();
        CompleteList().Visibility(Visibility::Collapsed);
    }

    void MainWindow::updateCompletions()
    {
        if (!session_ || !session_->ok())
        {
            hideCompletions();
            return;
        }
        auto text = to_string(CommandLine().Text());
        // Complete bare prefix only (no args yet).
        if (text.empty() || text.find(' ') != std::string::npos)
        {
            hideCompletions();
            return;
        }
        CompleteList().Items().Clear();
        std::istringstream in(session_->complete(text));
        std::string line;
        int count = 0;
        while (std::getline(in, line) && count < 12)
        {
            if (line.empty())
            {
                continue;
            }
            CompleteList().Items().Append(box_value(to_hstring(line)));
            ++count;
        }
        CompleteList().Visibility(
            count > 0 ? Visibility::Visible : Visibility::Collapsed);
    }

    void MainWindow::CommandLine_TextChanged(
        Windows::Foundation::IInspectable const&,
        TextChangedEventArgs const&)
    {
        updateCompletions();
    }

    void MainWindow::CompleteList_ItemClick(
        Windows::Foundation::IInspectable const&,
        ItemClickEventArgs const& e)
    {
        auto hs = unbox_value<hstring>(e.ClickedItem());
        CommandLine().Text(hs);
        hideCompletions();
        CommandLine().Focus(FocusState::Programmatic);
        CommandLine().Select(static_cast<int32_t>(hs.size()), 0);
    }

    bool MainWindow::handleEscape()
    {
        // 1) Cancel active tool  2) close Tools catalog  3) close overlays
        if (session_ && session_->ok() && session_->toolActive())
        {
            session_->toolCancel();
            clearActiveTool();
            appendHistory(L"*Cancel*");
            hideCompletions();
            refreshCanvas();
            refreshStatus();
            return true;
        }
        hideCompletions();
        if (ToolsPane().Visibility() == Visibility::Visible)
        {
            ToolsPane().Visibility(Visibility::Collapsed);
            return true;
        }
        bool closed = false;
        if (LeftPane().Visibility() == Visibility::Visible)
        {
            leftPinned_ = false;
            leftHover_ = false;
            showLeftPane(false);
            closed = true;
        }
        if (RightPane().Visibility() == Visibility::Visible)
        {
            rightPinned_ = false;
            rightHover_ = false;
            showRightPane(false);
            closed = true;
        }
        return closed;
    }

    void MainWindow::cycleFocusRegions()
    {
        // Paper → CLI → open panel (if any) → Paper
        std::vector<FrameworkElement> targets;
        targets.push_back(CanvasHost());
        targets.push_back(CommandLine());
        if (LeftPane().Visibility() == Visibility::Visible)
        {
            targets.push_back(LeftTabLayers());
        }
        else if (RightPane().Visibility() == Visibility::Visible)
        {
            targets.push_back(RightTabProps());
        }
        focusCycle_ = (focusCycle_ + 1) % static_cast<int>(targets.size());
        targets[static_cast<size_t>(focusCycle_)].Focus(FocusState::Programmatic);
    }

    void MainWindow::updateTitleBar()
    {
        const bool dirty = session_ && session_->ok() && session_->dirty();
        hstring leaf{ documentLeaf_ };
        hstring text = dirty ? leaf + L" *" : leaf;
        TitleBarText().Text(text);
        Title(hstring{ L"AADT — " } + text);
    }

    void MainWindow::applyComfortHits()
    {
        const double hit = comfortHits_ ? 35.0 : 24.0;
        const double strip = comfortHits_ ? 36.0 : 28.0;
        const double modelH = comfortHits_ ? 32.0 : 22.0;
        StatusRowDef().Height(GridLength{ strip, GridUnitType::Pixel });
        TaskbarChrome().Height(strip);
        auto setHit = [](auto const& el, double h)
        {
            el.Height(h);
            el.MinHeight(h);
            el.MaxHeight(h);
        };
        setHit(PanelsBtn(), hit);
        setHit(AidOsnap(), hit);
        setHit(AidOrtho(), hit);
        setHit(AidPolar(), hit);
        setHit(AidDyn(), hit);
        setHit(AidLwt(), hit);
        setHit(ModelTabBtn(), modelH);
        ComfortHitsItem().IsChecked(comfortHits_);
    }

    void MainWindow::ComfortHits_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        comfortHits_ = ComfortHitsItem().IsChecked();
        applyComfortHits();
        appendHistory(comfortHits_ ? L"Comfortable hits on (35px)" : L"Comfortable hits off (24px CAD)");
    }

    bool MainWindow::handleHotkeys(VirtualKey key)
    {
        if (key == VirtualKey::F6)
        {
            cycleFocusRegions();
            return true;
        }
        if (!session_ || !session_->ok())
        {
            return false;
        }
        const bool ctrl = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        if (ctrl && (key == VirtualKey::Z || key == VirtualKey::Y))
        {
            // AutoCAD: Ctrl+Z/Y always undo/redo the drawing.
            // Only free-type fields (layer name, LISP, …) keep TextBox edit undo.
            if (allowsLiteralSpace())
            {
                return false;
            }
            if (key == VirtualKey::Z)
            {
                feedCommand("UNDO");
            }
            else
            {
                feedCommand("REDO");
            }
            return true;
        }
        if (key == VirtualKey::F8)
        {
            const bool on = !session_->orthoGet();
            session_->orthoSet(on);
            appendHistory(on ? L"ORTHO on" : L"ORTHO off");
            syncAidChips();
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::F10)
        {
            const bool on = !session_->polarGet();
            session_->polarSet(on);
            appendHistory(on ? L"POLAR on" : L"POLAR off");
            syncAidChips();
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::F3)
        {
            osnapOn_ = !osnapOn_;
            session_->osnapSet(osnapOn_);
            appendHistory(osnapOn_ ? L"OSNAP on" : L"OSNAP off");
            syncAidChips();
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::F12)
        {
            dynMode_ = !dynMode_;
            appendHistory(dynMode_ ? L"DYN on" : L"DYN off");
            syncAidChips();
            if (!dynMode_)
            {
                DynInputCard().Visibility(Visibility::Collapsed);
            }
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::Delete)
        {
            if (!session_->toolActive() && session_->selectionCount() > 0)
            {
                feedCommand("ERASE SELECTION");
                return true;
            }
        }
        return false;
    }

    bool MainWindow::isCliOrDynFocused()
    {
        auto focused = Microsoft::UI::Xaml::Input::FocusManager::GetFocusedElement(
            Content().XamlRoot());
        auto tb = focused.try_as<TextBox>();
        if (!tb)
        {
            return false;
        }
        auto same = [&](TextBox const& other)
        {
            return get_abi(tb) == get_abi(other);
        };
        return same(CommandLine()) || same(DynInputBox());
    }

    void MainWindow::focusCommandLineForTyping()
    {
        CommandLine().Focus(FocusState::Programmatic);
    }

    void MainWindow::Window_PreviewKeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        // Tunnel early: Ctrl+Z/Y, F-keys, then Space=Enter (AutoCAD peers).
        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }
        if (e.Key() != VirtualKey::Space)
        {
            return;
        }
        if (allowsLiteralSpace())
        {
            return;
        }
        // Let Space insert while typing unfinished lengths (`2500 mm`).
        if (isCliOrDynFocused())
        {
            auto focused = Microsoft::UI::Xaml::Input::FocusManager::GetFocusedElement(
                Content().XamlRoot());
            if (auto tb = focused.try_as<TextBox>())
            {
                if (lengthMayTakeUnitSuffix(to_string(tb.Text())))
                {
                    return;
                }
            }
        }
        e.Handled(true);
        acceptSpaceAsEnter();
    }

    void MainWindow::Window_CharacterReceived(
        Windows::Foundation::IInspectable const&,
        CharacterReceivedRoutedEventArgs const& e)
    {
        // AutoCAD: typing anywhere starts / continues the command line.
        const wchar_t ch = e.Character();
        if (ch < 32 || ch == 127)
        {
            return;
        }
        if ((GetKeyState(VK_CONTROL) & 0x8000) != 0 ||
            (GetKeyState(VK_MENU) & 0x8000) != 0)
        {
            return;
        }
        if (allowsLiteralSpace() || isCliOrDynFocused())
        {
            return;
        }
        e.Handled(true);
        focusCommandLineForTyping();
        std::wstring text{ CommandLine().Text().c_str() };
        text.push_back(ch);
        CommandLine().Text(hstring{ text });
        CommandLine().Select(static_cast<int32_t>(text.size()), 0);
        updateCompletions();
    }

    void MainWindow::Window_KeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }
        if (e.Key() == VirtualKey::Escape && handleEscape())
        {
            e.Handled(true);
            return;
        }
        if (e.Key() == VirtualKey::Tab && tryTogglePlaceFlip())
        {
            e.Handled(true);
            return;
        }
        // Enter (not Space — PreviewKeyDown owns Space) when focus is not a TextBox.
        if (e.Key() == VirtualKey::Enter)
        {
            auto focused = Microsoft::UI::Xaml::Input::FocusManager::GetFocusedElement(
                Content().XamlRoot());
            if (focused && focused.try_as<TextBox>())
            {
                return;
            }
            if (acceptSpaceAsEnter())
            {
                e.Handled(true);
            }
        }
    }

    void MainWindow::CommandLine_CharacterReceived(
        Windows::Foundation::IInspectable const&,
        CharacterReceivedRoutedEventArgs const& e)
    {
        // Block literal space in CLI — Space is Enter — unless typing `2500 mm`.
        if (e.Character() == L' ')
        {
            if (!lengthMayTakeUnitSuffix(to_string(CommandLine().Text())))
            {
                e.Handled(true);
            }
        }
    }

    void MainWindow::DynInput_CharacterReceived(
        Windows::Foundation::IInspectable const&,
        CharacterReceivedRoutedEventArgs const& e)
    {
        if (e.Character() == L' ')
        {
            if (!lengthMayTakeUnitSuffix(to_string(DynInputBox().Text())))
            {
                e.Handled(true);
            }
        }
    }

    void MainWindow::CommandLine_PreviewKeyDown(
        Windows::Foundation::IInspectable const& sender,
        KeyRoutedEventArgs const& e)
    {
        // TextBox often swallows Enter / Ctrl+Z before KeyDown — handle early.
        const bool ctrl = (GetKeyState(VK_CONTROL) & 0x8000) != 0;
        if (ctrl && (e.Key() == VirtualKey::Z || e.Key() == VirtualKey::Y))
        {
            if (handleHotkeys(e.Key()))
            {
                e.Handled(true);
            }
            return;
        }
        if (e.Key() == VirtualKey::Enter || e.Key() == VirtualKey::Escape ||
            e.Key() == VirtualKey::Tab || e.Key() == VirtualKey::Up ||
            e.Key() == VirtualKey::Down || e.Key() == VirtualKey::Space ||
            e.Key() == VirtualKey::F6)
        {
            CommandLine_KeyDown(sender, e);
        }
    }

    void MainWindow::CommandLine_KeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        if (e.Handled())
        {
            return;
        }

        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }

        if (e.Key() == VirtualKey::Escape)
        {
            e.Handled(true);
            handleEscape();
            return;
        }

        if (e.Key() == VirtualKey::Tab)
        {
            e.Handled(true);
            if (tryTogglePlaceFlip())
            {
                return;
            }
            updateCompletions();
            if (CompleteList().Items().Size() > 0)
            {
                auto first = unbox_value<hstring>(CompleteList().Items().GetAt(0));
                CommandLine().Text(first);
                hideCompletions();
                CommandLine().Select(static_cast<int32_t>(first.size()), 0);
            }
            return;
        }

        if (e.Key() == VirtualKey::Up)
        {
            if (cmdHistory_.empty())
            {
                return;
            }
            e.Handled(true);
            if (historyIndex_ < 0)
            {
                historyIndex_ = static_cast<int>(cmdHistory_.size()) - 1;
            }
            else if (historyIndex_ > 0)
            {
                --historyIndex_;
            }
            CommandLine().Text(to_hstring(cmdHistory_[static_cast<size_t>(historyIndex_)]));
            hideCompletions();
            return;
        }

        if (e.Key() == VirtualKey::Down)
        {
            if (historyIndex_ < 0)
            {
                return;
            }
            e.Handled(true);
            if (historyIndex_ + 1 < static_cast<int>(cmdHistory_.size()))
            {
                ++historyIndex_;
                CommandLine().Text(to_hstring(cmdHistory_[static_cast<size_t>(historyIndex_)]));
            }
            else
            {
                historyIndex_ = -1;
                CommandLine().Text(L"");
            }
            hideCompletions();
            return;
        }

        // Enter here; Space is usually handled by Window_PreviewKeyDown first.
        if (e.Key() != VirtualKey::Enter && e.Key() != VirtualKey::Space)
        {
            return;
        }
        if (e.Key() == VirtualKey::Space
            && lengthMayTakeUnitSuffix(to_string(CommandLine().Text())))
        {
            return; // insert space for `2500 mm`
        }
        e.Handled(true);
        acceptSpaceAsEnter();
    }

    void MainWindow::CanvasHost_SizeChanged(
        Windows::Foundation::IInspectable const&,
        SizeChangedEventArgs const&)
    {
        resizeCanvas();
    }

    void MainWindow::CanvasHost_CompositionScaleChanged(
        SwapChainPanel const&,
        Windows::Foundation::IInspectable const&)
    {
        resizeCanvas();
    }

    void MainWindow::CanvasHost_PointerPressed(
        Windows::Foundation::IInspectable const&,
        PointerRoutedEventArgs const& e)
    {
        auto pt = e.GetCurrentPoint(CanvasHost());
        const float x = static_cast<float>(pt.Position().X);
        const float y = static_cast<float>(pt.Position().Y);
        lastX_ = x;
        lastY_ = y;

        if (pt.Properties().IsRightButtonPressed())
        {
            if (session_ && session_->ok() && session_->toolActive())
            {
                if (session_->toolFinish())
                {
                    clearActiveTool();
                    appendHistory(L"(Right-click) finish");
                    refreshCanvas(true);
                    refreshStatus();
                    refreshProperties();
                }
                e.Handled(true);
                return;
            }
            panning_ = true;
            CanvasHost().CapturePointer(e.Pointer());
            e.Handled(true);
            return;
        }

        if (pt.Properties().IsMiddleButtonPressed())
        {
            panning_ = true;
            CanvasHost().CapturePointer(e.Pointer());
            e.Handled(true);
            return;
        }

        if (!pt.Properties().IsLeftButtonPressed() || !session_ || !session_->ok())
        {
            return;
        }

        const double wx = canvas_.worldX(x);
        const double wy = canvas_.worldY(y);
        if (session_->toolActive())
        {
            if (activeTool_ == "XLINE" &&
                (xlineMode_ == XlineMode::Hor || xlineMode_ == XlineMode::Ver))
            {
                placeXlineAt(wx, wy);
            }
            else
            {
                const bool hadAnchor = toolHasAnchor_;
                session_->toolAcceptPoint(wx, wy);
                // Track last point so typed lengths (25' / 6") resolve from here.
                const bool chainTool = activeTool_ == "LINE" || activeTool_ == "PL"
                                       || activeTool_ == "POLYLINE" || activeTool_ == "WALL"
                                       || activeTool_ == "ARC";
                if (activeTool_ == "RECTANGLE" || activeTool_ == "CIRCLE" || chainTool
                    || (activeTool_ == "XLINE" && xlineMode_ == XlineMode::Through))
                {
                    if (!hadAnchor)
                    {
                        toolHasAnchor_ = true;
                        toolAnchorX_ = wx;
                        toolAnchorY_ = wy;
                        if (activeTool_ == "RECTANGLE")
                        {
                            appendHistory(
                                L"RECTANGLE — opposite corner · @W,H · or D for dimensions");
                        }
                        else if (chainTool || activeTool_ == "CIRCLE")
                        {
                            appendHistory(
                                L"Type distance (2500mm · 2.5m · 25' · 6\" · bare) then Enter");
                        }
                    }
                    else if (chainTool)
                    {
                        // Next segment starts at the point just placed.
                        toolHasAnchor_ = true;
                        toolAnchorX_ = wx;
                        toolAnchorY_ = wy;
                    }
                    else if (activeTool_ == "XLINE")
                    {
                        // Through places keep going from same origin.
                        toolHasAnchor_ = true;
                    }
                }
            }
            refreshCanvas();
            refreshStatus();
            refreshProperties();
            updateDynOverlay(x, y, wx, wy);
            if (session_->toolNeedsText())
            {
                appendHistory(L"TEXT  -  type value + Enter");
                CommandLine().Focus(FocusState::Programmatic);
                DynInputBox().Focus(FocusState::Programmatic);
            }
            e.Handled(true);
            return;
        }

        // Idle: try grip pick before box select / entity pick.
        const double tol =
            std::abs(static_cast<double>(canvas_.worldX(8.f) - canvas_.worldX(0.f)));
        uint64_t gid = 0;
        uint64_t gidx = 0;
        session_->rebuildGrips();
        if (session_->gripPick(wx, wy, tol > 1e-9 ? tol : 1.0, &gid, &gidx))
        {
            gripDrag_ = true;
            gripId_ = gid;
            gripIdx_ = gidx;
            CanvasHost().CapturePointer(e.Pointer());
            e.Handled(true);
            return;
        }

        boxDrag_ = true;
        boxActive_ = false;
        boxX0_ = boxX1_ = x;
        boxY0_ = boxY1_ = y;
        CanvasHost().CapturePointer(e.Pointer());
        e.Handled(true);
    }

    void MainWindow::CanvasHost_PointerMoved(
        Windows::Foundation::IInspectable const&,
        PointerRoutedEventArgs const& e)
    {
        auto pt = e.GetCurrentPoint(CanvasHost());
        const float x = static_cast<float>(pt.Position().X);
        const float y = static_cast<float>(pt.Position().Y);

        paperHovered_ = true;
        canvas_.setCursorDip(x, y, true);

        if (panning_)
        {
            canvas_.pan(x - lastX_, y - lastY_);
            lastX_ = x;
            lastY_ = y;
            canvas_.render();
            e.Handled(true);
            return;
        }

        lastX_ = x;
        lastY_ = y;

        if (gripDrag_)
        {
            e.Handled(true);
            return;
        }

        if (boxDrag_)
        {
            boxX1_ = x;
            boxY1_ = y;
            const float dx = std::abs(boxX1_ - boxX0_);
            const float dy = std::abs(boxY1_ - boxY0_);
            boxActive_ = dx > 4.f || dy > 4.f;
            refreshCanvas();
            e.Handled(true);
            return;
        }

        if (session_ && session_->ok())
        {
            const double wx = canvas_.worldX(x);
            const double wy = canvas_.worldY(y);
            wchar_t coords[64]{};
            swprintf_s(coords, L"%.2f, %.2f", wx, wy);
            CoordStatus().Text(coords);
            session_->toolSetCursor(wx, wy);
            if (session_->toolActive())
            {
                refreshCanvas();
                refreshStatus();
                updateDynOverlay(x, y, wx, wy);
            }
            else
            {
                if (dynMode_)
                {
                    DynInputCard().Visibility(Visibility::Collapsed);
                }
                // Idle: redraw for crosshair/pickbox without rebuilding geometry.
                if (canvasReady_)
                {
                    canvas_.render();
                }
            }
        }
        else if (canvasReady_)
        {
            canvas_.render();
        }
    }

    void MainWindow::CanvasHost_PointerReleased(
        Windows::Foundation::IInspectable const&,
        PointerRoutedEventArgs const& e)
    {
        if (panning_)
        {
            panning_ = false;
            CanvasHost().ReleasePointerCapture(e.Pointer());
            e.Handled(true);
            return;
        }

        if (gripDrag_ && session_ && session_->ok())
        {
            gripDrag_ = false;
            CanvasHost().ReleasePointerCapture(e.Pointer());
            auto pt = e.GetCurrentPoint(CanvasHost());
            const float x = static_cast<float>(pt.Position().X);
            const float y = static_cast<float>(pt.Position().Y);
            const double wx = canvas_.worldX(x);
            const double wy = canvas_.worldY(y);
            if (session_->gripMove(gripId_, gripIdx_, wx, wy))
            {
                appendHistory(to_hstring(
                    std::string("GRIPEDIT ") + std::to_string(gripId_) + " " +
                    std::to_string(gripIdx_)));
            }
            refreshCanvas(true);
            refreshStatus();
            refreshProperties();
            e.Handled(true);
            return;
        }

        if (boxDrag_ && session_ && session_->ok())
        {
            boxDrag_ = false;
            CanvasHost().ReleasePointerCapture(e.Pointer());
            const float dx = std::abs(boxX1_ - boxX0_);
            const float dy = std::abs(boxY1_ - boxY0_);
            if (boxActive_ && (dx > 4.f || dy > 4.f))
            {
                const double x0 = canvas_.worldX(boxX0_);
                const double y0 = canvas_.worldY(boxY0_);
                const double x1 = canvas_.worldX(boxX1_);
                const double y1 = canvas_.worldY(boxY1_);
                if (zoomWindowPending_)
                {
                    canvas_.fitWorld(
                        static_cast<float>((std::min)(x0, x1)),
                        static_cast<float>((std::min)(y0, y1)),
                        static_cast<float>((std::max)(x0, x1)),
                        static_cast<float>((std::max)(y0, y1)));
                    didFit_ = true;
                    zoomWindowPending_ = false;
                    appendHistory(L"Zoom window applied");
                    boxActive_ = false;
                    refreshCanvas();
                    refreshStatus();
                }
                else
                {
                const auto n = session_->selectBox(x0, y0, x1, y1);
                const bool crossing = x1 < x0;
                appendHistory(to_hstring(
                    std::string(crossing ? "Crossing" : "Window") + " select → " +
                    std::to_string(n)));
                boxActive_ = false;
                refreshCanvas();
                refreshStatus();
                refreshProperties();
                if (n > 0)
                {
                    showRightPane(true);
                    rightPinned_ = true;
                }
                }
            }
            else
            {
                boxActive_ = false;
                const double wx = canvas_.worldX(boxX0_);
                const double wy = canvas_.worldY(boxY0_);
                const double tol =
                    std::abs(static_cast<double>(canvas_.worldX(8.f) - canvas_.worldX(0.f)));
                const bool shiftHeld =
                    (e.KeyModifiers() & VirtualKeyModifiers::Shift) == VirtualKeyModifiers::Shift;
                session_->pickAddSet(shiftHeld);
                session_->pickAt(wx, wy, tol > 1e-9 ? tol : 1.0);
                session_->pickAddSet(false);
                const auto fb = session_->lastFeedback();
                if (!fb.empty())
                {
                    appendHistory(to_hstring(fb));
                }
                refreshCanvas();
                refreshStatus();
                refreshProperties();
                if (session_->selectionCount() > 0)
                {
                    showRightTab(RightTab::Properties);
                    showRightPane(true);
                    rightPinned_ = true;
                }
            }
            e.Handled(true);
        }
    }

    void MainWindow::CanvasHost_PointerWheelChanged(
        Windows::Foundation::IInspectable const&,
        PointerRoutedEventArgs const& e)
    {
        auto pt = e.GetCurrentPoint(CanvasHost());
        const float x = static_cast<float>(pt.Position().X);
        const float y = static_cast<float>(pt.Position().Y);
        const int delta = pt.Properties().MouseWheelDelta();
        const float factor = delta > 0 ? 1.1f : (1.f / 1.1f);
        canvas_.zoomAt(factor, x, y);
        canvas_.render();
        e.Handled(true);
    }

    void MainWindow::showLeftPane(bool show)
    {
        LeftPane().Visibility(show ? Visibility::Visible : Visibility::Collapsed);
        if (show)
        {
            refreshLayers();
            refreshBlocks();
            refreshLevels();
            refreshFills();
        }
        syncPanelsChrome();
    }

    void MainWindow::showRightPane(bool show)
    {
        RightPane().Visibility(show ? Visibility::Visible : Visibility::Collapsed);
        if (show)
        {
            showRightTab(rightTab_);
        }
        syncPanelsChrome();
    }

    void MainWindow::syncPanelsChrome()
    {
        const bool open =
            LeftPane().Visibility() == Visibility::Visible ||
            RightPane().Visibility() == Visibility::Visible;
        stylePaneTab(PanelsBtn(), open);
    }

    void MainWindow::showRightTab(RightTab tab)
    {
        rightTab_ = tab;
        const bool props = tab == RightTab::Properties;
        const bool history = tab == RightTab::History;
        const bool plot = tab == RightTab::Plot;
        const bool lisp = tab == RightTab::Lisp;
        PropsPane().Visibility(props ? Visibility::Visible : Visibility::Collapsed);
        HistoryPane().Visibility(history ? Visibility::Visible : Visibility::Collapsed);
        PlotPane().Visibility(plot ? Visibility::Visible : Visibility::Collapsed);
        LispPane().Visibility(lisp ? Visibility::Visible : Visibility::Collapsed);
        stylePaneTab(RightTabProps(), props);
        stylePaneTab(RightTabHistory(), history);
        stylePaneTab(RightTabPlot(), plot);
        stylePaneTab(RightTabLisp(), lisp);
        if (props)
        {
            refreshProperties();
        }
        else if (history)
        {
            refreshHistory();
        }
    }

    void MainWindow::refreshHistory()
    {
        HistoryList().Items().Clear();
        historyRestoreIds_.clear();
        if (!session_ || !session_->ok())
        {
            HistoryList().Items().Append(box_value(hstring{ L"(no session)" }));
            return;
        }
        // Quiet feed — do not push into the command-line ↑↓ history.
        if (!session_->feed("HISTORY LIST"))
        {
            const auto fb = session_->lastFeedback();
            HistoryList().Items().Append(box_value(
                fb.empty() ? hstring{ L"(HISTORY LIST failed)" } : to_hstring(fb)));
            return;
        }
        const auto fb = session_->lastFeedback();
        if (fb.empty())
        {
            HistoryList().Items().Append(box_value(hstring{ L"(empty response)" }));
            return;
        }
        std::istringstream in(fb);
        std::string line;
        bool any = false;
        while (std::getline(in, line))
        {
            // Checkpoint / commit rows look like: "  #12 [auto] …"
            const auto hash = line.find('#');
            if (hash == std::string::npos || hash + 1 >= line.size())
            {
                continue;
            }
            if (hash > 0 && line[hash - 1] != ' ' && line[hash - 1] != '\t')
            {
                continue;
            }
            size_t i = hash + 1;
            std::string id;
            while (i < line.size() && std::isdigit(static_cast<unsigned char>(line[i])))
            {
                id.push_back(line[i++]);
            }
            if (id.empty())
            {
                continue;
            }
            any = true;
            historyRestoreIds_.push_back(id);
            HistoryList().Items().Append(box_value(to_hstring(trimCopy(line))));
        }
        if (!any)
        {
            // Surface the CLI message (e.g. "no Merkle commits yet…").
            HistoryList().Items().Append(box_value(to_hstring(fb)));
        }
    }

    void MainWindow::showLeftTab(LeftTab tab)
    {
        leftTab_ = tab;
        const bool layers = tab == LeftTab::Layers;
        const bool elements = tab == LeftTab::Elements;
        const bool levels = tab == LeftTab::Levels;
        const bool fills = tab == LeftTab::Fills;
        const bool meta = tab == LeftTab::Meta;
        LayersList().Visibility(layers ? Visibility::Visible : Visibility::Collapsed);
        BlocksList().Visibility(elements ? Visibility::Visible : Visibility::Collapsed);
        LevelsList().Visibility(levels ? Visibility::Visible : Visibility::Collapsed);
        FillsList().Visibility(fills ? Visibility::Visible : Visibility::Collapsed);
        MetaScroll().Visibility(meta ? Visibility::Visible : Visibility::Collapsed);
        LayerCreateRow().Visibility(layers ? Visibility::Visible : Visibility::Collapsed);
        ElementCreateRow().Visibility(elements ? Visibility::Visible : Visibility::Collapsed);
        LevelCreateRow().Visibility(levels ? Visibility::Visible : Visibility::Collapsed);
        LayerActions().Visibility(layers ? Visibility::Visible : Visibility::Collapsed);
        ElementActions().Visibility(elements ? Visibility::Visible : Visibility::Collapsed);
        LevelActions().Visibility(levels ? Visibility::Visible : Visibility::Collapsed);
        stylePaneTab(LeftTabLayers(), layers);
        stylePaneTab(LeftTabElements(), elements);
        stylePaneTab(LeftTabLevels(), levels);
        stylePaneTab(LeftTabFills(), fills);
        stylePaneTab(LeftTabMeta(), meta);
        if (elements)
        {
            refreshBlocks();
        }
        else if (levels)
        {
            refreshLevels();
        }
        else if (layers)
        {
            refreshLayers();
        }
        else if (fills)
        {
            refreshFills();
        }
        else if (meta)
        {
            refreshMetaPane();
        }
    }

    void MainWindow::scheduleHideLeft()
    {
        if (!leftPinned_ && !leftHover_)
        {
            showLeftPane(false);
        }
    }

    void MainWindow::scheduleHideRight()
    {
        if (!rightPinned_ && !rightHover_)
        {
            showRightPane(false);
        }
    }

    void MainWindow::SideRail_SizeChanged(Windows::Foundation::IInspectable const& sender, SizeChangedEventArgs const&)
    {
        // WinUI does not clip by default — rotated tab labels paint into the paper
        // unless the rail (and each tab button) clip to their laid-out bounds.
        auto border = sender.try_as<Border>();
        if (!border)
        {
            return;
        }
        const float w = static_cast<float>(border.ActualWidth());
        const float h = static_cast<float>(border.ActualHeight());
        if (w > 0.f && h > 0.f)
        {
            RectangleGeometry railClip;
            railClip.Rect({ 0.f, 0.f, w, h });
            border.Clip(railClip);
        }
        auto panel = border.Child().try_as<Panel>();
        if (!panel)
        {
            return;
        }
        for (auto const& child : panel.Children())
        {
            auto fe = child.try_as<FrameworkElement>();
            if (!fe)
            {
                continue;
            }
            const float bw = static_cast<float>(fe.ActualWidth());
            const float bh = static_cast<float>(fe.ActualHeight());
            if (bw <= 0.f || bh <= 0.f)
            {
                continue;
            }
            RectangleGeometry btnClip;
            btnClip.Rect({ 0.f, 0.f, bw, bh });
            fe.Clip(btnClip);
        }
    }

    void MainWindow::LeftRail_PointerEntered(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        leftHover_ = true;
        showLeftPane(true);
    }

    void MainWindow::LeftRail_PointerExited(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        leftHover_ = false;
        scheduleHideLeft();
    }

    void MainWindow::LeftPane_PointerEntered(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        leftHover_ = true;
    }

    void MainWindow::LeftPane_PointerExited(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        leftHover_ = false;
        scheduleHideLeft();
    }

    void MainWindow::LeftLayersBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Layers);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::LeftElementsBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Elements);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::LeftLevelsBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Levels);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::LeftFillsBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Fills);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::LeftTabLayers_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Layers);
    }

    void MainWindow::LeftTabElements_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Elements);
    }

    void MainWindow::LeftTabLevels_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Levels);
    }

    void MainWindow::LeftTabFills_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Fills);
    }

    void MainWindow::LeftTabMeta_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Meta);
    }

    void MainWindow::DraftBarMeta_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Meta);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::DraftBarSync_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        BridgeFlush_Click({}, {});
        refreshFirmChip();
        seedDrawingPropsFromConnect();
        appendHistory(L"Sync — Connect flush + firm chip refreshed");
    }

    std::string MainWindow::selectedLayerName() const
    {
        if (!pendingLayer_.empty())
        {
            return pendingLayer_;
        }
        return {};
    }

    std::string MainWindow::selectedBlockName() const
    {
        return pendingBlock_;
    }

    void MainWindow::LayersList_ItemClick(Windows::Foundation::IInspectable const&, ItemClickEventArgs const& e)
    {
        auto hs = unbox_value<hstring>(e.ClickedItem());
        const auto items = LayersList().Items();
        pendingLayer_.clear();
        for (uint32_t i = 0; i < items.Size() && i < layerNames_.size(); ++i)
        {
            if (unbox_value<hstring>(items.GetAt(i)) == hs)
            {
                pendingLayer_ = layerNames_[i];
                syncLayerPropEditors(pendingLayer_);
                return;
            }
        }
    }

    void MainWindow::LayersList_DoubleTapped(Windows::Foundation::IInspectable const&, DoubleTappedRoutedEventArgs const&)
    {
        if (pendingLayer_.empty() || !session_ || !session_->ok())
        {
            return;
        }
        session_->layerSetCurrent(pendingLayer_);
        appendHistory(to_hstring(std::string("Current layer → ") + pendingLayer_));
        refreshLayers();
        refreshStatus();
    }

    void MainWindow::BlocksList_ItemClick(Windows::Foundation::IInspectable const&, ItemClickEventArgs const& e)
    {
        auto hs = unbox_value<hstring>(e.ClickedItem());
        auto s = to_string(hs);
        if (s.rfind("(no elements", 0) == 0 || s.rfind("(no blocks", 0) == 0)
        {
            pendingBlock_.clear();
            return;
        }
        // Resolve place name from parallel list (display may differ for catalog).
        const auto items = BlocksList().Items();
        pendingBlock_.clear();
        for (uint32_t i = 0; i < items.Size() && i < elementPlaceNames_.size(); ++i)
        {
            auto itemHs = unbox_value<hstring>(items.GetAt(i));
            if (itemHs == hs)
            {
                pendingBlock_ = elementPlaceNames_[i];
                break;
            }
        }
        if (pendingBlock_.empty())
        {
            pendingBlock_ = s;
        }
    }

    void MainWindow::FillsList_ItemClick(Windows::Foundation::IInspectable const&, ItemClickEventArgs const& e)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        auto hs = unbox_value<hstring>(e.ClickedItem());
        const auto items = FillsList().Items();
        for (uint32_t i = 0; i < items.Size() && i < fillPatternNames_.size(); ++i)
        {
            auto itemHs = unbox_value<hstring>(items.GetAt(i));
            if (itemHs == hs)
            {
                const auto& name = fillPatternNames_[i];
                session_->hatchSetPattern(name);
                appendHistory(to_hstring(std::string("Hatch pattern â†’ ") + name));
                refreshFills();
                refreshStatus();
                return;
            }
        }
    }

    void MainWindow::layerCmd(char const* flag)
    {
        auto name = selectedLayerName();
        if (name.empty() || !session_ || !session_->ok())
        {
            return;
        }
        feedCommand(std::string("LAYER ") + name + " " + flag);
        refreshLayers();
    }

    void MainWindow::LayerOn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("ON");
    }

    void MainWindow::LayerOff_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("OFF");
    }

    void MainWindow::LayerFreeze_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("FREEZE");
    }

    void MainWindow::LayerThaw_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("THAW");
    }

    void MainWindow::LayerLock_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("LOCK");
    }

    void MainWindow::LayerUnlock_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        layerCmd("UNLOCK");
    }

    void MainWindow::LayerNew_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"Engine offline — cannot create layer");
            return;
        }
        auto name = to_string(LayerNewBox().Text());
        // trim
        while (!name.empty() && (name.front() == ' ' || name.front() == '\t'))
            name.erase(name.begin());
        while (!name.empty() && (name.back() == ' ' || name.back() == '\t'))
            name.pop_back();
        if (name.empty())
        {
            appendHistory(L"Enter a layer name");
            return;
        }
        feedCommand(std::string("LAYER ") + name);
        LayerNewBox().Text(L"");
        pendingLayer_ = name;
        refreshLayers();
        refreshStatus();
        appendHistory(to_hstring(std::string("Layer created → ") + name));
    }

    void MainWindow::LayerCurrent_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (pendingLayer_.empty() || !session_ || !session_->ok())
        {
            appendHistory(L"Select a layer first");
            return;
        }
        session_->layerSetCurrent(pendingLayer_);
        appendHistory(to_hstring(std::string("Current layer → ") + pendingLayer_));
        refreshLayers();
        refreshStatus();
    }

    void MainWindow::LayerChip_Tapped(Windows::Foundation::IInspectable const&, TappedRoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Layers);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::LayerStandards_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        feedCommand("STANDARDS");
        appendHistory(L"STANDARDS — ISO 13567 / AIA layer catalog seeded");
        refreshLayers();
        refreshStatus();
        refreshCanvas();
    }

    void MainWindow::LayerSetItem_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        auto btn = sender.try_as<MenuFlyoutItem>();
        if (!btn)
        {
            return;
        }
        auto tag = unbox_value_or<hstring>(btn.Tag(), L"");
        if (tag.empty())
        {
            return;
        }
        feedCommand(std::string("LAYERSET ") + to_string(tag));
        appendHistory(hstring{ L"LAYERSET " } + tag);
        refreshLayers();
        refreshCanvas();
    }

    void MainWindow::LayerApplyProps_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto name = selectedLayerName();
        if (name.empty() || !session_ || !session_->ok())
        {
            appendHistory(L"Select a layer first");
            return;
        }
        int aci = 7;
        auto aciText = trimCopy(to_string(LayerAciBox().Text()));
        if (!aciText.empty())
        {
            aci = std::atoi(aciText.c_str());
        }
        else if (auto cbi = LayerColorCombo().SelectedItem().try_as<ComboBoxItem>())
        {
            aci = unbox_value_or<int>(cbi.Tag(), 7);
        }
        if (aci < 1)
        {
            aci = 1;
        }
        if (aci > 255)
        {
            aci = 255;
        }
        std::string ltype = "CONTINUOUS";
        if (auto sel = LayerLtypeCombo().SelectedItem())
        {
            ltype = to_string(unbox_value<hstring>(sel));
        }
        std::string lw = "0.25";
        if (auto sel = LayerLwCombo().SelectedItem())
        {
            lw = to_string(unbox_value<hstring>(sel));
        }
        char cmd[256]{};
        std::snprintf(
            cmd,
            sizeof(cmd),
            "LAYER %s COLOR %d LTYPE %s LWEIGHT %s",
            name.c_str(),
            aci,
            ltype.c_str(),
            lw.c_str());
        feedCommand(cmd);
        appendHistory(to_hstring(cmd));
        refreshLayers();
        refreshCanvas();
    }

    void MainWindow::LayerFilter_TextChanged(Windows::Foundation::IInspectable const&, TextChangedEventArgs const&)
    {
        refreshLayers();
    }

    void MainWindow::LayerColorCombo_SelectionChanged(
        Windows::Foundation::IInspectable const&,
        SelectionChangedEventArgs const&)
    {
        if (layerPropSyncing_)
        {
            return;
        }
        if (auto cbi = LayerColorCombo().SelectedItem().try_as<ComboBoxItem>())
        {
            int aci = unbox_value_or<int>(cbi.Tag(), 7);
            applyLayerColorSwatch(static_cast<uint8_t>(aci));
        }
    }

    void MainWindow::LevelsList_ItemClick(Windows::Foundation::IInspectable const&, ItemClickEventArgs const& e)
    {
        auto hs = unbox_value<hstring>(e.ClickedItem());
        const auto items = LevelsList().Items();
        pendingLevel_.clear();
        for (uint32_t i = 0; i < items.Size() && i < levelNames_.size(); ++i)
        {
            if (unbox_value<hstring>(items.GetAt(i)) == hs)
            {
                pendingLevel_ = levelNames_[i];
                break;
            }
        }
    }

    void MainWindow::LevelsList_DoubleTapped(Windows::Foundation::IInspectable const&, DoubleTappedRoutedEventArgs const&)
    {
        if (pendingLevel_.empty() || !session_ || !session_->ok())
        {
            return;
        }
        feedCommand(std::string("FLOOR ") + pendingLevel_);
        appendHistory(to_hstring(std::string("Active level → ") + pendingLevel_));
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::LevelNew_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"Engine offline — cannot add level");
            return;
        }
        auto name = to_string(LevelNewBox().Text());
        while (!name.empty() && (name.front() == ' ' || name.front() == '\t'))
            name.erase(name.begin());
        while (!name.empty() && (name.back() == ' ' || name.back() == '\t'))
            name.pop_back();
        if (name.empty())
        {
            appendHistory(L"Enter a level name");
            return;
        }
        feedCommand(std::string("FLOORADD ") + name);
        LevelNewBox().Text(L"");
        pendingLevel_ = name;
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
        appendHistory(to_hstring(std::string("Level added → ") + name));
    }

    void MainWindow::LevelCurrent_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (pendingLevel_.empty() || !session_ || !session_->ok())
        {
            appendHistory(L"Select a level first");
            return;
        }
        feedCommand(std::string("FLOOR ") + pendingLevel_);
        appendHistory(to_hstring(std::string("Active level → ") + pendingLevel_));
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::LevelChip_Tapped(Windows::Foundation::IInspectable const&, TappedRoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Levels);
        leftPinned_ = true;
        showLeftPane(true);
    }

    void MainWindow::CategoryBtn_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        auto btn = sender.try_as<Button>();
        if (!btn)
        {
            return;
        }
        auto tag = unbox_value_or<hstring>(btn.Tag(), L"");
        if (tag.empty())
        {
            return;
        }
        feedCommand(std::string("CATEGORY ") + to_string(tag));
        activeCategoryName_ = to_string(tag);
        appendHistory(to_hstring(std::string("Category → ") + activeCategoryName_));
        syncActiveCategoryLayer();
        refreshLevels();
        refreshCanvas(false);
        refreshStatus();
    }

    void MainWindow::BlockPlace_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto name = selectedBlockName();
        if (name.empty() || !session_ || !session_->ok())
        {
            appendHistory(L"Select an element first");
            return;
        }
        session_->elementEnsure(name);
        if (session_->toolStartInsert(name))
        {
            appendHistory(to_hstring(std::string("INSERT ") + name + " (click to place)"));
            refreshCanvas();
            refreshStatus();
        }
    }

    void MainWindow::RightRail_PointerEntered(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        rightHover_ = true;
        showRightPane(true);
    }

    void MainWindow::RightRail_PointerExited(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        rightHover_ = false;
        scheduleHideRight();
    }

    void MainWindow::RightPane_PointerEntered(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        rightHover_ = true;
    }

    void MainWindow::RightPane_PointerExited(Windows::Foundation::IInspectable const&, PointerRoutedEventArgs const&)
    {
        rightHover_ = false;
        scheduleHideRight();
    }

    void MainWindow::RightPropsBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Properties);
        rightPinned_ = true;
        showRightPane(true);
    }

    void MainWindow::RightHistoryBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::History);
        rightPinned_ = true;
        showRightPane(true);
    }

    void MainWindow::RightPlotBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Plot);
        rightPinned_ = true;
        showRightPane(true);
    }

    void MainWindow::RightLispBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Lisp);
        rightPinned_ = true;
        showRightPane(true);
    }

    void MainWindow::RightTabProps_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Properties);
    }

    void MainWindow::RightTabHistory_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::History);
    }

    void MainWindow::RightTabPlot_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Plot);
    }

    void MainWindow::RightTabLisp_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Lisp);
    }

    std::string MainWindow::selectedPlotPaper()
    {
        auto box = PlotPaperBox();
        if (auto item = box.SelectedItem().try_as<ComboBoxItem>())
        {
            auto hs = unbox_value_or<hstring>(item.Content(), L"A3");
            auto s = to_string(hs);
            return s.empty() ? "A3" : s;
        }
        return "A3";
    }

    void MainWindow::appendLispOutput(hstring const& line)
    {
        auto cur = to_string(LispOutputText().Text());
        if (!cur.empty())
        {
            cur += "\n";
        }
        cur += to_string(line);
        // Cap output length
        if (cur.size() > 8000)
        {
            cur = cur.substr(cur.size() - 8000);
        }
        LispOutputText().Text(to_hstring(cur));
    }

    void MainWindow::PlotPdf_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"! Engine offline — cannot plot PDF");
            return;
        }
        wchar_t file[MAX_PATH]{};
        wcscpy_s(file, L"drawing.pdf");
        OPENFILENAMEW ofn{};
        ofn.lStructSize = sizeof(ofn);
        ofn.lpstrFile = file;
        ofn.nMaxFile = MAX_PATH;
        ofn.lpstrFilter = L"PDF (*.pdf)\0*.pdf\0All\0*.*\0";
        ofn.lpstrDefExt = L"pdf";
        ofn.Flags = OFN_OVERWRITEPROMPT | OFN_EXPLORER;
        if (!GetSaveFileNameW(&ofn))
        {
            return;
        }
        auto path = to_string(hstring{ file });
        auto paper = selectedPlotPaper();
        const bool ok = session_->plotPdfToPath(path, paper);
        auto fb = session_->lastFeedback();
        auto msg = fb.empty()
                       ? (ok ? std::string("Plotted PDF → ") + path : std::string("Plot PDF failed"))
                       : fb;
        appendHistory(to_hstring(msg));
        PlotStatusText().Text(to_hstring(msg));
        StatusBar().Text(to_hstring(msg));
        if (ok)
        {
            showRightTab(RightTab::Plot);
            rightPinned_ = true;
            showRightPane(true);
        }
    }

    void MainWindow::PlotSetup_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"! Engine offline — cannot place sheet border");
            return;
        }
        auto paper = selectedPlotPaper();
        const bool ok = session_->plotSetup(paper);
        auto fb = session_->lastFeedback();
        auto msg = fb.empty() ? (ok ? "PLOTSETUP done" : "PLOTSETUP failed") : fb;
        appendHistory(to_hstring(msg));
        PlotStatusText().Text(to_hstring(msg));
        refreshCanvas(true);
        refreshStatus();
    }

    void MainWindow::LispRun_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendLispOutput(L"! Engine offline");
            return;
        }
        auto expr = trimCopy(to_string(LispExprBox().Text()));
        if (expr.empty())
        {
            appendLispOutput(L"! Enter a LISP expression");
            return;
        }
        // Bare lists are fine; wrap bare symbols that aren't already LISP/LISPLOAD.
        std::string line = expr;
        if (!expr.empty() && expr.front() != '(' &&
            upperAscii(expr).rfind("LISP", 0) != 0 &&
            upperAscii(expr).rfind("LISPLOAD", 0) != 0)
        {
            line = std::string("LISP ") + expr;
        }
        appendLispOutput(to_hstring(std::string("> ") + line));
        session_->feed(line);
        auto fb = session_->lastFeedback();
        appendLispOutput(fb.empty() ? hstring{ L"(ok)" } : to_hstring(fb));
        appendHistory(to_hstring(std::string("LISP → ") + (fb.empty() ? "ok" : fb)));
        refreshCanvas(true);
        refreshStatus();
        refreshLayers();
    }

    void MainWindow::LispLoad_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendLispOutput(L"! Engine offline");
            return;
        }
        wchar_t file[MAX_PATH]{};
        OPENFILENAMEW ofn{};
        ofn.lStructSize = sizeof(ofn);
        ofn.lpstrFile = file;
        ofn.nMaxFile = MAX_PATH;
        ofn.lpstrFilter =
            L"LISP (*.lsp;*.lisp)\0*.lsp;*.lisp\0All\0*.*\0";
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER;
        if (!GetOpenFileNameW(&ofn))
        {
            return;
        }
        auto path = to_string(hstring{ file });
        auto line = std::string("LISPLOAD ") + path;
        appendLispOutput(to_hstring(std::string("> ") + line));
        session_->feed(line);
        auto fb = session_->lastFeedback();
        appendLispOutput(fb.empty() ? hstring{ L"(loaded)" } : to_hstring(fb));
        appendHistory(to_hstring(std::string("LISPLOAD → ") + path));
        showRightTab(RightTab::Lisp);
        rightPinned_ = true;
        showRightPane(true);
        refreshCanvas(true);
        refreshStatus();
    }

    void MainWindow::LispClear_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        LispOutputText().Text(L"");
    }

    void MainWindow::LispSample_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        auto fe = sender.try_as<FrameworkElement>();
        if (!fe)
        {
            return;
        }
        auto tag = unbox_value_or<hstring>(fe.Tag(), L"");
        if (tag.empty())
        {
            return;
        }
        LispExprBox().Text(tag);
        LispRun_Click(sender, {});
    }

    void MainWindow::HistorySave_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto label = trimCopy(to_string(HistoryLabelBox().Text()));
        if (label.empty())
        {
            label = "Named_version";
        }
        for (char& c : label)
        {
            if (c == ' ')
            {
                c = '_';
            }
        }
        // Creates %LOCALAPPDATA%\AADT\untitled.vdb when no file is open yet.
        feedCommand(std::string("HISTORY SAVE ") + label);
        refreshHistory();
        refreshCanvas();
        HistoryDiffText().Text(L"Commit point saved. Select a version to trace changes vs tip.");
    }

    void MainWindow::HistoryRefresh_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        refreshHistory();
    }

    void MainWindow::HistoryPurge_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("HISTORY PURGE");
        refreshHistory();
    }

    void MainWindow::HistoryRestore_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        const auto idx = HistoryList().SelectedIndex();
        if (idx < 0 || static_cast<size_t>(idx) >= historyRestoreIds_.size())
        {
            appendHistory(L"History: select a version first");
            return;
        }
        feedCommand(std::string("HISTORY RESTORE ") + historyRestoreIds_[static_cast<size_t>(idx)]);
        HistoryDiffText().Text(L"(restored — tip matches this version)");
        refreshHistory();
        refreshCanvas();
    }

    void MainWindow::HistoryList_SelectionChanged(Windows::Foundation::IInspectable const&,
                                                   Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&)
    {
        const auto idx = HistoryList().SelectedIndex();
        if (idx < 0 || static_cast<size_t>(idx) >= historyRestoreIds_.size() || !session_ || !session_->ok())
        {
            return;
        }
        const auto& id = historyRestoreIds_[static_cast<size_t>(idx)];
        if (!session_->feed(std::string("HISTORY DIFF ") + id))
        {
            const auto fb = session_->lastFeedback();
            HistoryDiffText().Text(fb.empty() ? hstring{ L"DIFF failed" } : to_hstring(fb));
            return;
        }
        HistoryDiffText().Text(to_hstring(session_->lastFeedback()));
        refreshCanvas();
    }

    void MainWindow::PropsChspace_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("CHSPACE");
    }

    void MainWindow::PropsErase_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("ERASE SELECTION");
    }

    void MainWindow::PropsLayerApply_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto layer = to_string(PropsLayerBox().Text());
        if (layer.empty())
        {
            return;
        }
        feedCommand(std::string("CHANGE SELECTION LAYER ") + layer);
    }

    void MainWindow::QatNew_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        session_->reset();
        session_->osnapSet(true);
        session_->otrackSet(false);
        didFit_ = false;
        lastMgrRev_ = 0;
        appendHistory(L"New drawing");
        setRibbonPath({});
        feedCommand("STANDARDS");
        feedCommand("LAYERSET CONSTRUCTION");
        appendHistory(L"Seeded ISO 13567 / AIA layers · LAYERSET CONSTRUCTION");
        refreshCanvas(false);
        zoomToExtents();
        refreshStatus();
        refreshProperties();
        refreshLayers();
        refreshBlocks();
        refreshFills();
    }

    void MainWindow::QatOpen_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto path = pickOpenPath();
        if (path.empty())
        {
            return;
        }
        feedCommand(std::string("OPEN ") + path);
        setRibbonPath(path);
        didFit_ = false;
        refreshCanvas(false);
        zoomToExtents();
    }

    void MainWindow::QatSave_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto existing = session_ && session_->ok() ? session_->sessionPath() : std::string{};
        if (!existing.empty())
        {
            feedCommand(std::string("SAVE ") + existing);
            setRibbonPath(existing);
            return;
        }
        auto path = pickSavePath();
        if (path.empty())
        {
            return;
        }
        feedCommand(std::string("SAVE ") + path);
        setRibbonPath(path);
    }

    void MainWindow::QatUndo_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("UNDO");
    }

    void MainWindow::QatRedo_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("REDO");
    }

    void MainWindow::QatZoomE_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        zoomToExtents();
    }

    void MainWindow::ZoomExtents_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        zoomToExtents();
    }

    void MainWindow::ZoomWindow_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        zoomWindowPending_ = true;
        appendHistory(L"Zoom window — drag a rectangle on the paper");
        DynPromptText().Text(L"Zoom window — drag");
    }

    void MainWindow::ZoomIn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!canvasReady_) return;
        const float px = static_cast<float>(CanvasHost().ActualWidth()) * 0.5f;
        const float py = static_cast<float>(CanvasHost().ActualHeight()) * 0.5f;
        canvas_.zoomAt(1.25f, px, py);
        canvas_.render();
        appendHistory(L"Zoom → in");
    }

    void MainWindow::ZoomOut_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!canvasReady_) return;
        const float px = static_cast<float>(CanvasHost().ActualWidth()) * 0.5f;
        const float py = static_cast<float>(CanvasHost().ActualHeight()) * 0.5f;
        canvas_.zoomAt(0.8f, px, py);
        canvas_.render();
        appendHistory(L"Zoom → out");
    }

    void MainWindow::ViewLock_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        // Prefer the control that raised the event (menu vs status chip).
        if (auto tb = sender.try_as<ToggleButton>())
        {
            viewLocked_ = tb.IsChecked().GetBoolean();
        }
        else if (auto mi = sender.try_as<ToggleMenuFlyoutItem>())
        {
            viewLocked_ = mi.IsChecked();
        }
        else
        {
            viewLocked_ = !viewLocked_;
        }
        syncViewLockChip();
        appendHistory(viewLocked_ ? L"View lock ON — REGEN keeps current view"
                                  : L"View lock OFF");
    }

    void MainWindow::QatReview_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        if (session_->selectionCount() > 0)
        {
            feedCommand("LIST");
        }
        else
        {
            appendHistory(to_hstring(
                std::string("Review — ") + std::to_string(session_->entityCount()) +
                " entities · select objects then Review for LIST"));
        }
    }

    void MainWindow::QatPlot_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showRightTab(RightTab::Plot);
        rightPinned_ = true;
        showRightPane(true);
    }

    void MainWindow::ToolBtn_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        // Works for Button and MenuFlyoutItem (both FrameworkElement + Tag).
        auto fe = sender.try_as<FrameworkElement>();
        if (!fe)
        {
            return;
        }
        auto tag = unbox_value_or<hstring>(fe.Tag(), L"");
        if (tag.empty())
        {
            return;
        }
        if (tag == L"IMAGE")
        {
            startImageTool();
            return;
        }
        startTool(to_string(tag));
    }

    namespace {

    // Metric: bare numbers = metres. Imperial: bare = inches; `'` = feet; `"` = inches.
    // Supports + - * / ( ). Result always in metres for formatting.
    std::optional<double> evalCalcExpr(std::string expr, bool imperial)
    {
        // Normalize: drop spaces; map unicode ops. Keep ' and " for length tokens.
        std::string s;
        s.reserve(expr.size());
        for (size_t k = 0; k < expr.size(); ++k)
        {
            unsigned char c = static_cast<unsigned char>(expr[k]);
            if (c == ' ' || c == '\t') continue;
            if (c == 0xC3 && k + 1 < expr.size())
            {
                // UTF-8 × (C3 97) ÷ (C3 B7) — skip multi-byte fancy ops via ASCII tags
            }
            if (c == 'x' || c == 'X') { s.push_back('*'); continue; }
            s.push_back(static_cast<char>(c));
        }
        if (s.empty()) return 0.0;

        // Expand length tokens → metres as decimal literals.
        std::string norm;
        norm.reserve(s.size() * 2);
        size_t i = 0;
        auto isDigit = [](char c) { return c >= '0' && c <= '9'; };
        auto emitMeters = [&](double m) {
            char buf[64];
            std::snprintf(buf, sizeof(buf), "%.10g", m);
            norm += buf;
        };
        while (i < s.size())
        {
            if (isDigit(s[i]) || (s[i] == '.' && i + 1 < s.size() && isDigit(s[i + 1])))
            {
                size_t start = i;
                while (i < s.size() && (isDigit(s[i]) || s[i] == '.')) ++i;
                std::string num = s.substr(start, i - start);
                // feet' [inches"]
                if (i < s.size() && s[i] == '\'')
                {
                    double ft = std::stod(num);
                    ++i;
                    double inches = 0;
                    size_t j = i;
                    while (j < s.size() && (isDigit(s[j]) || s[j] == '.')) ++j;
                    if (j > i)
                    {
                        inches = std::stod(s.substr(i, j - i));
                        i = j;
                        if (i < s.size() && s[i] == '"') ++i;
                    }
                    emitMeters(ft * 0.3048 + inches * 0.0254);
                    continue;
                }
                // Nft / Nin / Nmm / Ncm / Nm  (check mm/cm before bare m)
                if (i + 1 < s.size() && (s[i] == 'f' || s[i] == 'F') && (s[i + 1] == 't' || s[i + 1] == 'T'))
                {
                    emitMeters(std::stod(num) * 0.3048);
                    i += 2;
                    continue;
                }
                if (i + 1 < s.size() && (s[i] == 'i' || s[i] == 'I') && (s[i + 1] == 'n' || s[i + 1] == 'N'))
                {
                    emitMeters(std::stod(num) * 0.0254);
                    i += 2;
                    continue;
                }
                if (i + 1 < s.size()
                    && (s[i] == 'm' || s[i] == 'M')
                    && (s[i + 1] == 'm' || s[i + 1] == 'M')
                    && (i + 2 >= s.size() || (!isDigit(s[i + 2]) && s[i + 2] != '.')))
                {
                    emitMeters(std::stod(num) * 0.001);
                    i += 2;
                    continue;
                }
                if (i + 1 < s.size()
                    && (s[i] == 'c' || s[i] == 'C')
                    && (s[i + 1] == 'm' || s[i + 1] == 'M')
                    && (i + 2 >= s.size() || (!isDigit(s[i + 2]) && s[i + 2] != '.')))
                {
                    emitMeters(std::stod(num) * 0.01);
                    i += 2;
                    continue;
                }
                if (i < s.size() && (s[i] == 'm' || s[i] == 'M')
                    && (i + 1 >= s.size() || (!isDigit(s[i + 1]) && s[i + 1] != '.'
                                             && s[i + 1] != 'm' && s[i + 1] != 'M')))
                {
                    ++i;
                    emitMeters(std::stod(num));
                    continue;
                }
                // Explicit inches via "
                if (i < s.size() && s[i] == '"')
                {
                    ++i;
                    emitMeters(std::stod(num) * 0.0254);
                    continue;
                }
                // Bare number: imperial → inches; metric → metres
                if (imperial)
                    emitMeters(std::stod(num) * 0.0254);
                else
                    norm += num;
                continue;
            }
            char c = s[i++];
            if (c == '+' || c == '-' || c == '*' || c == '/' || c == '(' || c == ')')
            {
                norm.push_back(c);
                continue;
            }
            return std::nullopt;
        }

        // Shunting-yard → RPN for + - * / ( )
        std::vector<std::string> output;
        std::vector<char> ops;
        auto prec = [](char op) { return (op == '+' || op == '-') ? 1 : 2; };
        i = 0;
        while (i < norm.size())
        {
            if (isDigit(norm[i]) || norm[i] == '.')
            {
                size_t start = i;
                while (i < norm.size() && (isDigit(norm[i]) || norm[i] == '.' || norm[i] == 'e'
                                           || norm[i] == 'E' || ((norm[i] == '+' || norm[i] == '-')
                                               && i > start && (norm[i - 1] == 'e' || norm[i - 1] == 'E'))))
                    ++i;
                output.push_back(norm.substr(start, i - start));
                continue;
            }
            char c = norm[i++];
            if (c == '(')
            {
                ops.push_back(c);
            }
            else if (c == ')')
            {
                while (!ops.empty() && ops.back() != '(')
                {
                    output.push_back(std::string(1, ops.back()));
                    ops.pop_back();
                }
                if (ops.empty()) return std::nullopt;
                ops.pop_back();
            }
            else if (c == '+' || c == '-' || c == '*' || c == '/')
            {
                // unary minus
                if (c == '-' && (output.empty() || (!ops.empty() && (ops.back() == '(' || ops.back() == '+'
                        || ops.back() == '-' || ops.back() == '*' || ops.back() == '/'))))
                {
                    // treat as 0 -
                    output.push_back("0");
                }
                while (!ops.empty() && ops.back() != '(' && prec(ops.back()) >= prec(c))
                {
                    output.push_back(std::string(1, ops.back()));
                    ops.pop_back();
                }
                ops.push_back(c);
            }
            else
            {
                return std::nullopt;
            }
        }
        while (!ops.empty())
        {
            if (ops.back() == '(') return std::nullopt;
            output.push_back(std::string(1, ops.back()));
            ops.pop_back();
        }

        std::vector<double> st;
        for (auto const& t : output)
        {
            if (t == "+" || t == "-" || t == "*" || t == "/")
            {
                if (st.size() < 2) return std::nullopt;
                double b = st.back(); st.pop_back();
                double a = st.back(); st.pop_back();
                if (t == "+") st.push_back(a + b);
                else if (t == "-") st.push_back(a - b);
                else if (t == "*") st.push_back(a * b);
                else
                {
                    if (std::fabs(b) < 1e-15) return std::nullopt;
                    st.push_back(a / b);
                }
            }
            else
            {
                try { st.push_back(std::stod(t)); }
                catch (...) { return std::nullopt; }
            }
        }
        if (st.size() != 1 || !std::isfinite(st.front())) return std::nullopt;
        return st.front();
    }

    std::wstring formatCalcMeters(double m, bool imperial)
    {
        wchar_t buf[96]{};
        if (!imperial)
        {
            swprintf_s(buf, L"= %.4g m", m);
            return buf;
        }
        double totalIn = std::fabs(m) / 0.0254;
        int feet = static_cast<int>(totalIn / 12.0);
        double inches = totalIn - feet * 12.0;
        if (inches >= 11.999)
        {
            ++feet;
            inches = 0;
        }
        if (m < 0)
            swprintf_s(buf, L"= -%d'%.2f\"", feet, inches);
        else
            swprintf_s(buf, L"= %d'%.2f\"", feet, inches);
        return buf;
    }

    } // namespace

    void MainWindow::CalcBtn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        const bool show = CalcPanel().Visibility() == Visibility::Collapsed;
        CalcPanel().Visibility(show ? Visibility::Visible : Visibility::Collapsed);
        if (show)
        {
            refreshCalcResult();
            CalcExprBox().Focus(FocusState::Programmatic);
        }
    }

    void MainWindow::CalcClose_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        CalcPanel().Visibility(Visibility::Collapsed);
    }

    void MainWindow::CalcClear_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        CalcExprBox().Text(L"");
        refreshCalcResult();
    }

    void MainWindow::CalcUnit_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        calcImperial_ = CalcImperialToggle().IsChecked().GetBoolean();
        CalcImperialToggle().Content(box_value(calcImperial_ ? L"ft·in" : L"m"));
        CalcExprBox().PlaceholderText(
            calcImperial_
                ? L"bare=inches · 10'=feet · 6\"=inches · 10'6\""
                : L"bare=metres · e.g. 3.2 + 1.5");
        refreshCalcResult();
    }

    void MainWindow::CalcKey_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
        auto btn = sender.try_as<Button>();
        if (!btn) return;
        auto tag = unbox_value_or<hstring>(btn.Tag(), L"");
        if (tag.empty()) return;
        CalcExprBox().Text(CalcExprBox().Text() + tag);
        refreshCalcResult();
    }

    void MainWindow::CalcExpr_TextChanged(Windows::Foundation::IInspectable const&, TextChangedEventArgs const&)
    {
        refreshCalcResult();
    }

    void MainWindow::CalcExpr_KeyDown(Windows::Foundation::IInspectable const&, KeyRoutedEventArgs const& e)
    {
        if (e.Key() == Windows::System::VirtualKey::Escape)
        {
            CalcPanel().Visibility(Visibility::Collapsed);
            e.Handled(true);
        }
    }

    void MainWindow::refreshCalcResult()
    {
        auto expr = to_string(CalcExprBox().Text());
        if (trimCopy(expr).empty())
        {
            CalcResultText().Text(calcImperial_ ? L"= 0'0\"" : L"= 0 m");
            return;
        }
        auto v = evalCalcExpr(expr, calcImperial_);
        if (!v)
        {
            CalcResultText().Text(L"—");
            return;
        }
        CalcResultText().Text(hstring{ formatCalcMeters(*v, calcImperial_) });
    }

    void MainWindow::BridgeFlush_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        std::wstring note;
        bool ok = aadt::bridge::flush(note);
        appendHistory(hstring{ (ok ? L"Bridge: " : L"! Bridge: ") + note });
        StatusBar().Text(hstring{ note });
        auto st = aadt::bridge::ensureAndStatus();
        ConnectLicenceChip().Text(hstring{
            st.bound ? (st.syncReady ? L"Licensed · sync ready" : L"Licensed · firm.db")
                     : L"Unbound · Connect" });
        refreshFirmChip();
    }

    void MainWindow::refreshFirmChip()
    {
        auto brand = aadt::connect::readFirmBranding();
        FirmNameLabel().Text(hstring{ brand.firmName });
        ToolTipService::SetToolTip(FirmNameLabel(), box_value(hstring{ brand.tip }));
        ToolTipService::SetToolTip(FirmLogoFallback(), box_value(hstring{ brand.tip }));

        // Prefer logo_path from drawing properties when set.
        std::wstring logo = brand.logoPath;
        if (session_ && session_->ok())
        {
            auto pack = session_->drawingPropsGet();
            std::istringstream in(pack);
            std::string line;
            while (std::getline(in, line))
            {
                auto tab = line.find('\t');
                if (tab == std::string::npos) continue;
                auto key = line.substr(0, tab);
                auto val = line.substr(tab + 1);
                if (key == "logo_path" && !val.empty())
                {
                    logo = to_hstring(val).c_str();
                }
                if (key == "firm_name" && !val.empty())
                {
                    FirmNameLabel().Text(to_hstring(val));
                }
                if (key == "title" && !val.empty())
                {
                    DraftBarTitleHint().Text(to_hstring(val));
                }
                else if (key == "project" && !val.empty()
                         && DraftBarTitleHint().Text() == L"Untitled drawing")
                {
                    DraftBarTitleHint().Text(to_hstring(val));
                }
            }
        }

        if (!logo.empty())
        {
            try
            {
                BitmapImage bmp;
                bmp.UriSource(Uri(hstring{ logo }));
                FirmLogoImage().Source(bmp);
                FirmLogoImage().Visibility(Visibility::Visible);
                FirmLogoFallback().Visibility(Visibility::Collapsed);
            }
            catch (...)
            {
                FirmLogoImage().Visibility(Visibility::Collapsed);
                FirmLogoFallback().Visibility(Visibility::Visible);
            }
        }
        else
        {
            FirmLogoImage().Visibility(Visibility::Collapsed);
            FirmLogoFallback().Visibility(Visibility::Visible);
        }
    }

    void MainWindow::seedDrawingPropsFromConnect()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        auto brand = aadt::connect::readFirmBranding();
        auto pack = session_->drawingPropsGet();
        std::string firm;
        std::string logo;
        std::string project;
        std::istringstream in(pack);
        std::string line;
        while (std::getline(in, line))
        {
            auto tab = line.find('\t');
            if (tab == std::string::npos) continue;
            auto key = line.substr(0, tab);
            auto val = line.substr(tab + 1);
            if (key == "firm_name") firm = val;
            if (key == "logo_path") logo = val;
            if (key == "project") project = val;
        }
        std::ostringstream out;
        bool changed = false;
        if (firm.empty() && !brand.firmName.empty())
        {
            out << "firm_name\t" << to_string(hstring{ brand.firmName }) << "\n";
            changed = true;
        }
        if (logo.empty() && !brand.logoPath.empty())
        {
            out << "logo_path\t" << to_string(hstring{ brand.logoPath }) << "\n";
            changed = true;
        }
        if (project.empty() && !brand.projectRef.empty())
        {
            out << "project\t" << to_string(hstring{ brand.projectRef }) << "\n";
            changed = true;
        }
        if (changed)
        {
            session_->drawingPropsSet(out.str());
            refreshFirmChip();
        }
    }

    fire_and_forget MainWindow::DrawingProps_Click(
        Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"! Engine offline — drawing properties unavailable");
            co_return;
        }

        auto pack = session_->drawingPropsGet();
        auto getField = [&](char const* key) -> std::string {
            std::istringstream in(pack);
            std::string line;
            while (std::getline(in, line))
            {
                auto tab = line.find('\t');
                if (tab == std::string::npos) continue;
                if (line.substr(0, tab) == key) return line.substr(tab + 1);
            }
            return {};
        };

        auto makeBox = [](std::string const& value, wchar_t const* placeholder) {
            TextBox tb;
            tb.Height(28);
            tb.FontSize(12);
            tb.PlaceholderText(placeholder);
            tb.Text(to_hstring(value));
            tb.Margin({ 0, 0, 0, 6 });
            return tb;
        };

        auto titleBox = makeBox(getField("title"), L"Title");
        auto projectBox = makeBox(getField("project"), L"Project");
        auto clientBox = makeBox(getField("client"), L"Client");
        auto firmBox = makeBox(getField("firm_name"), L"Firm name");
        auto logoBox = makeBox(getField("logo_path"), L"Logo path (PNG/JPEG)");
        auto drawingBox = makeBox(getField("drawing_number"), L"Drawing number");
        auto revBox = makeBox(getField("revision"), L"Revision");
        auto unitsBox = makeBox(getField("units"), L"Units (ft-in | m)");
        auto floorHBox = makeBox(getField("floor_height"), L"Floor height");
        auto lintelBox = makeBox(getField("lintel_height"), L"Lintel height");

        StackPanel body;
        body.Spacing(0);
        auto label = [](wchar_t const* t) {
            TextBlock tb;
            tb.Text(t);
            tb.FontSize(10);
            tb.Foreground(
                Application::Current().Resources().Lookup(box_value(L"HcwMutedBrush")).as<Brush>());
            return tb;
        };
        body.Children().Append(label(L"Title"));
        body.Children().Append(titleBox);
        body.Children().Append(label(L"Project"));
        body.Children().Append(projectBox);
        body.Children().Append(label(L"Client"));
        body.Children().Append(clientBox);
        body.Children().Append(label(L"Firm name"));
        body.Children().Append(firmBox);
        body.Children().Append(label(L"Logo path"));
        body.Children().Append(logoBox);
        body.Children().Append(label(L"Drawing number · Revision · Units"));
        body.Children().Append(drawingBox);
        body.Children().Append(revBox);
        body.Children().Append(unitsBox);
        body.Children().Append(label(L"Floor / lintel height"));
        body.Children().Append(floorHBox);
        body.Children().Append(lintelBox);

        ScrollViewer sc;
        sc.Content(body);
        sc.MaxHeight(420);

        ContentDialog dlg;
        dlg.Title(box_value(L"Drawing properties"));
        dlg.Content(sc);
        dlg.PrimaryButtonText(L"Apply");
        dlg.CloseButtonText(L"Cancel");
        dlg.DefaultButton(ContentDialogButton::Primary);
        dlg.XamlRoot(Content().XamlRoot());

        auto result = co_await dlg.ShowAsync();
        if (result != ContentDialogResult::Primary)
        {
            co_return;
        }

        std::ostringstream out;
        auto put = [&](char const* k, TextBox const& tb) {
            out << k << '\t' << to_string(tb.Text()) << '\n';
        };
        put("title", titleBox);
        put("project", projectBox);
        put("client", clientBox);
        put("firm_name", firmBox);
        put("logo_path", logoBox);
        put("drawing_number", drawingBox);
        put("revision", revBox);
        put("units", unitsBox);
        put("floor_height", floorHBox);
        put("lintel_height", lintelBox);
        if (session_->drawingPropsSet(out.str()))
        {
            appendHistory(L"Drawing properties applied");
            refreshFirmChip();
            refreshStatus();
        }
        else
        {
            appendHistory(L"! Drawing properties failed");
        }
    }

    void MainWindow::refreshMetaPane()
    {
        MetaLevelHint().Text(to_hstring(
            std::string("Level · Element — ") + activeLevelName_ + " · " + activeCategoryName_));
        if (!session_ || !session_->ok() || session_->selectionCount() == 0)
        {
            MetaContextLabel().Text(L"Select objects on the paper, then Apply tags for AQC / BBS.");
            return;
        }
        MetaContextLabel().Text(to_hstring(
            std::string("Selection: ") + std::to_string(session_->selectionCount())
            + " — edit fields and Apply"));
        MetaReload_Click({}, {});
    }

    void MainWindow::MetaReload_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok() || session_->selectionCount() == 0)
        {
            return;
        }
        auto ids = session_->selectionIds();
        if (ids.empty())
        {
            return;
        }
        session_->feed("OBJMETA");
        auto fb = session_->lastFeedback();
        auto fill = [&](TextBox const& box, char const* key) {
            std::string needle = std::string(key) + "=";
            auto pos = fb.find(needle);
            if (pos == std::string::npos)
            {
                box.Text(L"");
                return;
            }
            pos += needle.size();
            auto end = fb.find("  ·  ", pos);
            if (end == std::string::npos) end = fb.find('\n', pos);
            auto val = end == std::string::npos ? fb.substr(pos) : fb.substr(pos, end - pos);
            box.Text(to_hstring(trimCopy(val)));
        };
        fill(MetaMarkBox(), "MARK");
        fill(MetaWallNameBox(), "WALL_NAME");
        fill(MetaNosBox(), "NOS");
        fill(MetaAqcCodeBox(), "AQC_CODE");
        fill(MetaColumnNoBox(), "COLUMN_NO");
        fill(MetaCodeBox(), "CODE");
        fill(MetaTradeBox(), "TRADE");
        fill(MetaMaterialBox(), "MATERIAL");
        MetaLevelHint().Text(to_hstring(
            std::string("Level · Element — ") + activeLevelName_ + " · " + activeCategoryName_));
        (void)ids;
    }

    void MainWindow::MetaApply_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        if (session_->selectionCount() == 0)
        {
            appendHistory(L"Meta — select objects first");
            return;
        }
        auto apply = [&](char const* key, TextBox const& box) {
            auto v = trimCopy(to_string(box.Text()));
            if (v.empty()) return;
            feedCommand(std::string("OBJMETA ") + key + " " + v);
        };
        apply("MARK", MetaMarkBox());
        apply("WALL_NAME", MetaWallNameBox());
        apply("NOS", MetaNosBox());
        apply("AQC_CODE", MetaAqcCodeBox());
        apply("COLUMN_NO", MetaColumnNoBox());
        apply("CODE", MetaCodeBox());
        apply("TRADE", MetaTradeBox());
        apply("MATERIAL", MetaMaterialBox());
        // Keep context stamps current.
        feedCommand(std::string("OBJMETA LEVEL ") + activeLevelName_);
        feedCommand(std::string("OBJMETA ELEMENT ") + activeCategoryName_);
        appendHistory(L"Meta tags applied to selection");
        refreshProperties();
        refreshCanvas(false);
    }

    void MainWindow::ToolsPalette_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        const bool show = ToolsPane().Visibility() == Visibility::Collapsed;
        ToolsPane().Visibility(show ? Visibility::Visible : Visibility::Collapsed);
        if (!show || !session_ || !session_->ok())
        {
            return;
        }
        ToolsList().Items().Clear();
        std::istringstream in(session_->catalogTools());
        std::string line;
        while (std::getline(in, line))
        {
            if (line.empty())
            {
                continue;
            }
            auto tab = line.find('\t');
            std::string token = tab == std::string::npos ? line : line.substr(0, tab);
            ToolsList().Items().Append(box_value(to_hstring(token)));
        }
    }

    void MainWindow::ToolsList_ItemClick(Windows::Foundation::IInspectable const&, ItemClickEventArgs const& e)
    {
        auto hs = unbox_value<hstring>(e.ClickedItem());
        ToolsPane().Visibility(Visibility::Collapsed);
        auto token = to_string(hs);
        if (token == "INSERT")
        {
            appendHistory(L"INSERT  -  pick an element in the Elements panel, then Place");
            showLeftTab(LeftTab::Elements);
            leftPinned_ = true;
            showLeftPane(true);
            return;
        }
        if (token == "IMAGE" || token == "PLACEIMAGE" || token == "IMAGEATTACH")
        {
            startImageTool();
            return;
        }
        startTool(token);
    }

    void MainWindow::refreshStatus()
    {
        if (!session_ || !session_->ok())
        {
            CliPromptText().Text(L"Type a command");
            updateTitleBar();
            return;
        }
        std::string prompt = session_->toolPrompt();
        if (activeTool_ == "XLINE")
        {
            if (xlineMode_ == XlineMode::Hor)
            {
                prompt = "XLINE Hor — specify a point";
            }
            else if (xlineMode_ == XlineMode::Ver)
            {
                prompt = "XLINE Ver — specify a point";
            }
            else if (prompt.empty())
            {
                prompt = "XLINE — specify first point";
            }
        }
        if (prompt.empty() && session_->toolActive())
        {
            prompt = "Specify first point";
        }
        CliPromptText().Text(
            prompt.empty() ? hstring{ L"Type a command" } : to_hstring(prompt));

        auto layer = session_->layerCurrent();
        if (layer.empty())
        {
            layer = "0";
        }
        LayerCurrentLabel().Text(to_hstring(layer));
        LevelCurrentLabel().Text(
            to_hstring(activeLevelName_ + " · " + activeCategoryName_));
        std::wstring gripChip;
        if (session_->selectionCount() > 0)
        {
            auto mode = session_->gripMode();
            if (!mode.empty())
            {
                gripChip = L"  grip ";
                gripChip += std::wstring(mode.begin(), mode.end());
            }
        }
        wchar_t buf[256]{};
        swprintf_s(
            buf,
            L"ents %llu  sel %llu  rev %llu%s%s",
            static_cast<unsigned long long>(session_->entityCount()),
            static_cast<unsigned long long>(session_->selectionCount()),
            static_cast<unsigned long long>(session_->revision()),
            session_->dirty() ? L" *" : L"",
            gripChip.c_str());
        StatusBar().Text(buf);
        refreshUnitsChip();
        updateTitleBar();
    }

    void MainWindow::refreshUnitsChip()
    {
        if (!session_ || !session_->ok())
        {
            UnitsChip().Content(box_value(L"ft-in"));
            return;
        }
        if (!session_->feed("UNITS"))
        {
            return;
        }
        const auto fb = session_->lastFeedback();
        // "units=ft-in (Feet & inches) secondary=..."
        std::string token = "ft-in";
        const auto eq = fb.find("units=");
        if (eq != std::string::npos)
        {
            size_t i = eq + 6;
            size_t j = i;
            while (j < fb.size() && !std::isspace(static_cast<unsigned char>(fb[j])) && fb[j] != '(')
            {
                ++j;
            }
            if (j > i)
            {
                token = fb.substr(i, j - i);
            }
        }
        unitsToken_ = token;
        UnitsChip().Content(box_value(to_hstring(token)));
        AutomationProperties::SetName(
            UnitsChip(),
            token == "m" ? L"Drawing units: metric meters" : L"Drawing units: imperial feet-inches");
    }

    void MainWindow::UnitsChip_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        // Flyout opens from XAML; keep click for keyboard activation.
    }

    void MainWindow::UnitsImperial_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("UNITS ft-in");
        refreshUnitsChip();
        appendHistory(L"UNITS → imperial (ft-in, model inches)");
    }

    void MainWindow::UnitsMetric_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        feedCommand("UNITS m");
        refreshUnitsChip();
        appendHistory(L"UNITS → metric (decimal meters)");
    }

    fire_and_forget MainWindow::DimManager_Click(
        Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            appendHistory(L"! Engine offline — Dimension manager unavailable");
            co_return;
        }

        // MenuFlyout still owns the UI thread — defer ContentDialog or it never shows.
        co_await winrt::resume_after(std::chrono::milliseconds(80));

        if (!session_ || !session_->ok())
        {
            co_return;
        }

        auto pack = session_->drawingPropsGet();
        auto getField = [&](char const* key) -> std::string {
            std::istringstream in(pack);
            std::string line;
            while (std::getline(in, line))
            {
                auto tab = line.find('\t');
                if (tab == std::string::npos) continue;
                if (line.substr(0, tab) == key) return line.substr(tab + 1);
            }
            return {};
        };

        auto muted = Application::Current()
                         .Resources()
                         .Lookup(box_value(L"HcwMutedBrush"))
                         .as<Brush>();
        auto label = [&](wchar_t const* t) {
            TextBlock tb;
            tb.Text(t);
            tb.FontSize(10);
            tb.Foreground(muted);
            tb.Margin({ 0, 8, 0, 2 });
            return tb;
        };

        auto unitsTok = getField("units");
        if (unitsTok.empty()) unitsTok = unitsToken_.empty() ? "ft-in" : unitsToken_;
        const bool isMetric =
            unitsTok == "m" || unitsTok == "meters" || unitsTok == "metric";

        ComboBox systemBox;
        systemBox.Height(32);
        {
            ComboBoxItem imp;
            imp.Content(box_value(L"Imperial — feet & inches (ft-in)"));
            imp.Tag(box_value(L"ft-in"));
            systemBox.Items().Append(imp);
            ComboBoxItem met;
            met.Content(box_value(L"Metric — decimal meters (m)"));
            met.Tag(box_value(L"m"));
            systemBox.Items().Append(met);
            systemBox.SelectedIndex(isMetric ? 1 : 0);
        }

        CheckBox multiBox;
        multiBox.Content(box_value(L"Display in multiple units (secondary in brackets)"));
        {
            auto sec = getField("dim_secondary");
            multiBox.IsChecked(
                sec == "1" || sec == "true" || sec == "on" || sec == "yes");
        }

        ComboBox precBox;
        precBox.Height(32);
        {
            auto cur = getField("dim_precision");
            int sel = 3;
            if (!cur.empty())
            {
                try { sel = std::stoi(cur); }
                catch (...) { sel = 3; }
            }
            if (sel < 0) sel = 0;
            if (sel > 6) sel = 6;
            for (int n = 0; n <= 6; ++n)
            {
                ComboBoxItem it;
                it.Content(box_value(to_hstring(n) + L" decimal places"));
                it.Tag(box_value(to_hstring(n)));
                precBox.Items().Append(it);
            }
            precBox.SelectedIndex(sel);
        }

        ComboBox fracBox;
        fracBox.Height(32);
        {
            auto cur = getField("dim_fraction");
            for (auto& c : cur) c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            struct Opt { wchar_t const* label; char const* tag; };
            Opt opts[] = {
                { L"Decimal inches", "0" },
                { L"1\"", "1" },
                { L"1/2\"", "2" },
                { L"1/4\"", "4" },
                { L"1/8\"", "8" },
                { L"1/16\"", "16" },
            };
            int sel = 0;
            for (int i = 0; i < 6; ++i)
            {
                ComboBoxItem it;
                it.Content(box_value(opts[i].label));
                it.Tag(box_value(to_hstring(opts[i].tag)));
                fracBox.Items().Append(it);
                if (cur == opts[i].tag
                    || (i == 0
                        && (cur.empty() || cur == "dec" || cur == "decimal" || cur == "off")))
                    sel = i;
            }
            fracBox.SelectedIndex(sel);
        }

        TextBox heightBox;
        heightBox.Height(28);
        heightBox.FontSize(12);
        heightBox.PlaceholderText(L"Dimension text height");
        {
            auto h = getField("dim_text_height");
            heightBox.Text(to_hstring(h.empty() ? "2.5" : h));
        }

        TextBlock tip;
        tip.Text(L"Typed lengths: glue the unit — 2500mm · 25cm · 2.5m · 10ft · 6\" (any system).");
        tip.FontSize(11);
        tip.TextWrapping(TextWrapping::Wrap);
        tip.Foreground(muted);
        tip.Margin({ 0, 10, 0, 0 });

        TextBlock sample;
        sample.FontSize(12);
        sample.Margin({ 0, 8, 0, 0 });
        sample.Text(L"Tip: Apply updates dim labels. Typed lengths: 2500mm · 25' · 6\"");

        StackPanel body;
        body.Spacing(0);
        body.Children().Append(label(L"Unit system"));
        body.Children().Append(systemBox);
        body.Children().Append(multiBox);
        body.Children().Append(label(L"Precision"));
        body.Children().Append(precBox);
        body.Children().Append(label(L"Imperial fractions (up to 1/16)"));
        body.Children().Append(fracBox);
        body.Children().Append(label(L"Dimension text height"));
        body.Children().Append(heightBox);
        body.Children().Append(sample);
        body.Children().Append(tip);

        ScrollViewer sc;
        sc.Content(body);
        sc.MaxHeight(480);

        ContentDialog dlg;
        dlg.Title(box_value(L"Dimension manager"));
        dlg.Content(sc);
        dlg.PrimaryButtonText(L"Apply");
        dlg.CloseButtonText(L"Cancel");
        dlg.DefaultButton(ContentDialogButton::Primary);
        dlg.XamlRoot(Content().XamlRoot());

        auto result = co_await dlg.ShowAsync();
        if (result != ContentDialogResult::Primary)
        {
            co_return;
        }

        auto tagOf = [](ComboBox const& box) -> std::string {
            if (auto cbi = box.SelectedItem().try_as<ComboBoxItem>())
            {
                return to_string(unbox_value_or<hstring>(cbi.Tag(), L""));
            }
            return {};
        };

        const auto sysTok = tagOf(systemBox);
        const auto sys = sysTok.empty() ? (isMetric ? "m" : "ft-in") : sysTok;
        auto prec = tagOf(precBox);
        if (prec.empty()) prec = "3";
        auto frac = tagOf(fracBox);
        if (frac.empty()) frac = "0";
        const bool multi = multiBox.IsChecked() && multiBox.IsChecked().Value();
        auto height = trimCopy(to_string(heightBox.Text()));
        if (height.empty()) height = "2.5";

        // Persist all dim prefs via props pack, then UNITS to sync dimstyle name.
        {
            std::ostringstream packOut;
            packOut << "units\t" << sys << '\n';
            packOut << "dim_secondary\t" << (multi ? "1" : "0") << '\n';
            packOut << "dim_precision\t" << prec << '\n';
            packOut << "dim_fraction\t" << frac << '\n';
            packOut << "dim_text_height\t" << height << '\n';
            packOut << "dimstyle\t" << (sys == "m" ? "METRIC" : "ARCHITECTURAL") << '\n';
            if (!session_->drawingPropsSet(packOut.str()))
            {
                appendHistory(L"! Dimension manager — props set failed (update aadt_ffi.dll?)");
                co_return;
            }
        }

        std::ostringstream cmd;
        cmd << "UNITS " << sys
            << " SECONDARY " << (multi ? "on" : "off")
            << " PRECISION " << prec
            << " FRACTION " << frac;
        feedCommand(cmd.str());

        unitsToken_ = sys;
        refreshUnitsChip();
        refreshCanvas(false);
        appendHistory(to_hstring(
            std::string("Dimension manager → ") + sys + " · prec=" + prec + " · frac=" + frac
            + (multi ? " · multi" : " · single")));
        co_return;
    }

    void MainWindow::AidOsnap_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        osnapOn_ = AidOsnap().IsChecked() && AidOsnap().IsChecked().Value();
        session_->osnapSet(osnapOn_);
        appendHistory(osnapOn_ ? L"OSNAP on" : L"OSNAP off");
        syncAidChips();
        refreshStatus();
    }

    void MainWindow::AidOrtho_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        const bool on = AidOrtho().IsChecked() && AidOrtho().IsChecked().Value();
        session_->orthoSet(on);
        appendHistory(on ? L"ORTHO on" : L"ORTHO off");
        syncAidChips();
        refreshStatus();
    }

    void MainWindow::AidPolar_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        const bool on = AidPolar().IsChecked() && AidPolar().IsChecked().Value();
        session_->polarSet(on);
        appendHistory(on ? L"POLAR on" : L"POLAR off");
        syncAidChips();
        refreshStatus();
    }

    void MainWindow::AidDyn_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        dynMode_ = AidDyn().IsChecked() && AidDyn().IsChecked().Value();
        appendHistory(dynMode_ ? L"DYN on" : L"DYN off");
        if (!dynMode_)
        {
            DynInputCard().Visibility(Visibility::Collapsed);
            DynOverlayCanvas().IsHitTestVisible(false);
        }
        syncAidChips();
        refreshStatus();
    }

    void MainWindow::AidLwt_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        const bool on = AidLwt().IsChecked() && AidLwt().IsChecked().Value();
        canvas_.setShowLineweights(on);
        appendHistory(on ? L"LWT on — layer lineweights" : L"LWT off — constant stroke");
        syncAidChips();
        if (canvasReady_)
        {
            canvas_.render();
        }
    }

    void MainWindow::DynInput_PreviewKeyDown(
        Windows::Foundation::IInspectable const& sender,
        KeyRoutedEventArgs const& e)
    {
        if (e.Key() == VirtualKey::Enter || e.Key() == VirtualKey::Space ||
            e.Key() == VirtualKey::Escape)
        {
            DynInput_KeyDown(sender, e);
        }
    }

    void MainWindow::DynInput_KeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        if (e.Handled())
        {
            return;
        }
        if (e.Key() == VirtualKey::Escape)
        {
            e.Handled(true);
            handleEscape();
            return;
        }
        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }
        if (e.Key() != VirtualKey::Enter && e.Key() != VirtualKey::Space)
        {
            return;
        }
        if (e.Key() == VirtualKey::Space
            && lengthMayTakeUnitSuffix(to_string(DynInputBox().Text())))
        {
            return; // insert space for `2500 mm`
        }
        e.Handled(true);
        auto text = to_string(DynInputBox().Text());
        DynInputBox().Text(L"");
        if (text.empty() && session_ && session_->toolActive())
        {
            if (session_->toolFinish())
            {
                clearActiveTool();
                appendHistory(L"(Dyn) finish");
                refreshCanvas(true);
                refreshStatus();
                refreshProperties();
            }
            return;
        }
        applyDynOrCommand(text);
        CommandLine().Focus(FocusState::Programmatic);
    }

    void MainWindow::appendHistory(hstring const& line)
    {
        CliHistory().Items().Append(box_value(line));
        const auto n = CliHistory().Items().Size();
        if (n > 0)
        {
            CliHistory().ScrollIntoView(CliHistory().Items().GetAt(n - 1));
        }
    }

    void MainWindow::setRibbonPath(std::string const& path)
    {
        if (path.empty())
        {
            documentLeaf_ = L"Untitled drawing";
            RibbonPathLabel().Text(hstring{ documentLeaf_ });
            updateTitleBar();
            return;
        }
        const auto pos = path.find_last_of("\\/");
        const auto leaf = (pos == std::string::npos) ? path : path.substr(pos + 1);
        documentLeaf_.assign(leaf.begin(), leaf.end());
        RibbonPathLabel().Text(hstring{ documentLeaf_ });
        updateTitleBar();
    }

    void MainWindow::MenuExit_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        Close();
    }

    void MainWindow::MenuAbout_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        appendHistory(
            L"AADT — Accelerated Architectural Drafting · AORMS suite · HCW chrome · "
            L"Activate licence in AORMS Connect");
    }

    void MainWindow::startClockPomodoro()
    {
        auto dq = DispatcherQueue();
        if (!dq)
        {
            return;
        }
        clockTimer_ = dq.CreateTimer();
        clockTimer_.Interval(std::chrono::milliseconds{ 1000 });
        clockTimer_.Tick([this](auto&&, auto&&)
        {
            tickClock();
            tickPomodoro();
        });
        clockTimer_.Start();
        tickClock();
        updatePomodoroUi();
    }

    static void setHand(Microsoft::UI::Xaml::Shapes::Line const& line, double cx, double cy,
                        double degrees, double length)
    {
        const double rad = (degrees - 90.0) * 3.14159265358979323846 / 180.0;
        line.X1(static_cast<float>(cx));
        line.Y1(static_cast<float>(cy));
        line.X2(static_cast<float>(cx + std::cos(rad) * length));
        line.Y2(static_cast<float>(cy + std::sin(rad) * length));
    }

    void MainWindow::tickClock()
    {
        if (!ClockHour())
        {
            return;
        }
        SYSTEMTIME st{};
        GetLocalTime(&st);
        const double cx = 50.0;
        const double cy = 50.0;
        setHand(ClockHour(), cx, cy, (st.wHour % 12 + st.wMinute / 60.0) * 30.0, 22.0);
        setHand(ClockMinute(), cx, cy, st.wMinute * 6.0, 32.0);
        ClockSecond().Opacity(pomodoroRunning_ ? 0.0 : 1.0);
        if (!pomodoroRunning_)
        {
            setHand(ClockSecond(), cx, cy, st.wSecond * 6.0, 36.0);
        }
    }

    void MainWindow::tickPomodoro()
    {
        if (!pomodoroRunning_ || pomodoroDragging_)
        {
            return;
        }
        if (pomodoroLeftSec_ <= 0)
        {
            pomodoroRunning_ = false;
            pomodoroLeftSec_ = 0;
            updatePomodoroUi();
            appendHistory(L"Pomodoro — focus block complete");
            return;
        }
        --pomodoroLeftSec_;
        updatePomodoroUi();
    }

    void MainWindow::updatePomodoroUi()
    {
        if (!PomodoroLabel())
        {
            return;
        }
        const bool active =
            pomodoroRunning_ || pomodoroLeftSec_ < pomodoroDurationSec_ || pomodoroDragging_;
        const int s = (std::max)(0, pomodoroLeftSec_);
        wchar_t label[64]{};
        swprintf_s(label, L"%02d:%02d", s / 60, s % 60);
        std::wstring text = label;
        if (pomodoroDragging_)
        {
            text += L" · set";
        }
        else if (!pomodoroRunning_ && !active)
        {
            text += L" · Pomodoro";
        }
        else if (pomodoroRunning_)
        {
            text += L" · running";
        }
        else
        {
            text += L" · paused";
        }
        PomodoroLabel().Text(hstring{ text });
        const auto color = active ? Windows::UI::ColorHelper::FromArgb(255, 0xFF, 0x4F, 0x18)
                                  : Windows::UI::ColorHelper::FromArgb(255, 0x5C, 0x63, 0x70);
        PomodoroLabel().Foreground(SolidColorBrush{ color });
        const double frac = (std::clamp)(pomodoroLeftSec_ / 3600.0, 0.0, 1.0);
        updatePomodoroRing(frac);
    }

    static Windows::Foundation::Point pointOnRing(double cx, double cy, double r, double frac)
    {
        const double a = (-90.0 + frac * 360.0) * 3.14159265358979323846 / 180.0;
        return {
            static_cast<float>(cx + r * std::cos(a)),
            static_cast<float>(cy + r * std::sin(a)),
        };
    }

    void MainWindow::updatePomodoroRing(double frac)
    {
        if (!PomodoroArm())
        {
            return;
        }
        constexpr double outer = 127.0;
        constexpr double cx = outer / 2.0;
        constexpr double cy = outer / 2.0;
        constexpr double r = outer * (72.5 / 165.0);
        const auto tip = pointOnRing(cx, cy, r, frac);
        PomodoroArm().X1(static_cast<float>(cx));
        PomodoroArm().Y1(static_cast<float>(cy));
        PomodoroArm().X2(tip.X);
        PomodoroArm().Y2(tip.Y);
        Canvas::SetLeft(PomodoroCrown(), tip.X - 5.f);
        Canvas::SetTop(PomodoroCrown(), tip.Y - 5.f);
        Canvas::SetLeft(PomodoroCrownHit(), tip.X - 14.f);
        Canvas::SetTop(PomodoroCrownHit(), tip.Y - 14.f);

        if (frac <= 0.001)
        {
            PomodoroArc().Data(nullptr);
            return;
        }
        const auto start = pointOnRing(cx, cy, r, 0.0);
        const auto end = pointOnRing(cx, cy, r, (std::min)(frac, 0.9999));
        PathFigure fig;
        fig.StartPoint(start);
        fig.IsClosed(false);
        fig.IsFilled(false);
        ArcSegment arc;
        arc.Point(end);
        arc.Size({ static_cast<float>(r), static_cast<float>(r) });
        arc.IsLargeArc(frac > 0.5);
        arc.SweepDirection(SweepDirection::Clockwise);
        arc.RotationAngle(0);
        fig.Segments().Append(arc);
        PathGeometry geo;
        geo.Figures().Append(fig);
        PomodoroArc().Data(geo);
    }

    void MainWindow::PomodoroToggle_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        togglePomodoro();
    }

    void MainWindow::Pomodoro_Tapped(Windows::Foundation::IInspectable const&, TappedRoutedEventArgs const& e)
    {
        if (pomodoroDragging_ || pomodoroPressActive_)
        {
            return;
        }
        if (pomodoroClickTimer_)
        {
            pomodoroClickTimer_.Stop();
        }
        auto dq = DispatcherQueue();
        if (!dq)
        {
            return;
        }
        pomodoroClickTimer_ = dq.CreateTimer();
        pomodoroClickTimer_.Interval(std::chrono::milliseconds{ 280 });
        pomodoroClickTimer_.IsRepeating(false);
        pomodoroClickTimer_.Tick([this](auto&&, auto&&)
        {
            if (pomodoroClickTimer_)
            {
                pomodoroClickTimer_.Stop();
            }
            togglePomodoro();
        });
        pomodoroClickTimer_.Start();
        e.Handled(true);
    }

    void MainWindow::Pomodoro_DoubleTapped(Windows::Foundation::IInspectable const&,
                                           DoubleTappedRoutedEventArgs const& e)
    {
        if (pomodoroClickTimer_)
        {
            pomodoroClickTimer_.Stop();
        }
        resetPomodoro();
        e.Handled(true);
    }

    void MainWindow::togglePomodoro()
    {
        if (pomodoroLeftSec_ <= 0)
        {
            pomodoroLeftSec_ = pomodoroDurationSec_;
            pomodoroRunning_ = true;
        }
        else
        {
            pomodoroRunning_ = !pomodoroRunning_;
        }
        updatePomodoroUi();
        appendHistory(pomodoroRunning_ ? L"Pomodoro — running" : L"Pomodoro — paused");
    }

    void MainWindow::resetPomodoro()
    {
        pomodoroRunning_ = false;
        pomodoroDragging_ = false;
        pomodoroPressActive_ = false;
        pomodoroLeftSec_ = pomodoroDurationSec_;
        updatePomodoroUi();
        appendHistory(L"Pomodoro — reset to focus block");
    }

    void MainWindow::PomodoroCrown_PointerPressed(Windows::Foundation::IInspectable const&,
                                                  PointerRoutedEventArgs const& e)
    {
        if (pomodoroRunning_)
        {
            return;
        }
        pomodoroPressActive_ = true;
        pomodoroDragging_ = false;
        pomodoroPressOrigin_ = e.GetCurrentPoint(PomodoroCanvas()).Position();
        PomodoroCrownHit().CapturePointer(e.Pointer());
        e.Handled(true);
    }

    void MainWindow::PomodoroCrown_PointerMoved(Windows::Foundation::IInspectable const&,
                                                PointerRoutedEventArgs const& e)
    {
        if (!pomodoroPressActive_ || pomodoroRunning_)
        {
            return;
        }
        const auto pt = e.GetCurrentPoint(PomodoroCanvas()).Position();
        const double dx = pt.X - pomodoroPressOrigin_.X;
        const double dy = pt.Y - pomodoroPressOrigin_.Y;
        if (!pomodoroDragging_ && (dx * dx + dy * dy) < 36.0)
        {
            return;
        }
        pomodoroDragging_ = true;
        applyPomodoroDrag(pt);
        e.Handled(true);
    }

    void MainWindow::PomodoroCrown_PointerReleased(Windows::Foundation::IInspectable const&,
                                                   PointerRoutedEventArgs const& e)
    {
        if (!pomodoroPressActive_)
        {
            return;
        }
        const bool wasDragging = pomodoroDragging_;
        pomodoroPressActive_ = false;
        pomodoroDragging_ = false;
        try
        {
            PomodoroCrownHit().ReleasePointerCapture(e.Pointer());
        }
        catch (...)
        {
        }
        if (wasDragging)
        {
            const int minutes =
                (std::max)(5, (std::min)(60, static_cast<int>(std::lround(pomodoroLeftSec_ / 60.0 / 5.0) * 5)));
            pomodoroDurationSec_ = minutes * 60;
            pomodoroLeftSec_ = pomodoroDurationSec_;
            updatePomodoroUi();
            appendHistory(to_hstring(std::string("Pomodoro — set · ") + std::to_string(minutes) + " min"));
        }
        else
        {
            togglePomodoro();
        }
        e.Handled(true);
    }

    void MainWindow::applyPomodoroDrag(Windows::Foundation::Point const& pt)
    {
        constexpr double cx = 63.5;
        constexpr double cy = 63.5;
        double deg = std::atan2(pt.Y - cy, pt.X - cx) * 180.0 / 3.14159265358979323846;
        double fromTop = std::fmod(deg + 90.0, 360.0);
        if (fromTop < 0)
        {
            fromTop += 360.0;
        }
        const double frac = (std::max)(5.0 / 60.0, (std::min)(1.0, fromTop / 360.0));
        pomodoroLeftSec_ = static_cast<int>(std::lround(frac * 3600.0));
        updatePomodoroUi();
    }
}
