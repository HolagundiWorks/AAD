#include "pch.h"
#include "MainWindow.xaml.h"
#if __has_include("MainWindow.g.cpp")
#include "MainWindow.g.cpp"
#endif

#include "../../include/AormsBridgeHost.hpp"
#include "../../include/ConnectSession.hpp"

#include <winrt/Microsoft.UI.Xaml.Input.h>
#include <winrt/Microsoft.UI.Input.h>
#include <winrt/Microsoft.UI.Windowing.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Graphics.h>
#include <winrt/Windows.System.h>

#include <commdlg.h>

#include <cctype>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <sstream>
#include <vector>

using namespace winrt;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
using namespace Microsoft::UI::Xaml::Input;
using namespace Windows::System;

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

    } // namespace

    MainWindow::MainWindow()
    {
        InitializeComponent();

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
            session_->otrackSet(true);
            session_->feed("LINE 0 0 100 0");
            session_->feed("LINE 100 0 100 60");
            session_->feed("LINE 100 60 0 60");
            session_->feed("LINE 0 60 0 0");
            session_->feed("CIRCLE 50 30 18");
            appendHistory(L"Demo rectangle + circle loaded  -  click an edge, or type LINE.");
            appendHistory(
                L"Note: wheel zoom mid-tool is the WinUI transparent-view equivalent.");
        }

        CanvasHost().SizeChanged({ this, &MainWindow::CanvasHost_SizeChanged });
        CanvasHost().CompositionScaleChanged({ this, &MainWindow::CanvasHost_CompositionScaleChanged });
        CanvasHost().PointerPressed({ this, &MainWindow::CanvasHost_PointerPressed });
        CanvasHost().PointerMoved({ this, &MainWindow::CanvasHost_PointerMoved });
        CanvasHost().PointerReleased({ this, &MainWindow::CanvasHost_PointerReleased });
        CanvasHost().PointerWheelChanged({ this, &MainWindow::CanvasHost_PointerWheelChanged });

        CanvasHost().Loaded([this](auto&&, auto&&)
        {
            initCanvas();
            resizeCanvas();
            refreshCanvas(true);
            refreshStatus();
            refreshProperties();
            refreshLayers();
            refreshBlocks();
            refreshFills();
            showRightPane(true);
            rightPinned_ = true;
        });

        CommandLine().Focus(FocusState::Programmatic);
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

            // OTRACK rays are already x0,y0,x1,y1 segments.
            const auto& rays = session_->rebuildOtrackRays();
            preview.insert(preview.end(), rays.begin(), rays.end());

            // Grips: id,idx,x,y → small crosses in world space.
            const auto& grips = session_->rebuildGrips();
            const float hs = static_cast<float>(
                std::abs(canvas_.worldX(5.f) - canvas_.worldX(0.f)));
            const float half = hs > 1e-6f ? hs : 1.f;
            for (size_t i = 0; i + 3 < grips.size(); i += 4)
            {
                const float gx = grips[i + 2];
                const float gy = grips[i + 3];
                appendSeg(preview, gx - half, gy, gx + half, gy);
                appendSeg(preview, gx, gy - half, gx, gy + half);
            }

            canvas_.setPreview(preview);
            float b[4]{};
            if (session_->bounds(b))
            {
                if (fitIfEmptyView || !didFit_)
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
        canvas_.render();
        refreshManagersIfNeeded();
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
        refreshFills();
    }

    void MainWindow::refreshLayers()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        LayersList().Items().Clear();
        std::istringstream in(session_->layersList());
        std::string line;
        while (std::getline(in, line))
        {
            if (line.empty())
            {
                continue;
            }
            // name\taci\ton\tfrozen\tlocked\tcurrent
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
            if (cols.size() > 5 && cols[5] == "1")
            {
                label = "> " + label;
            }
            if (cols.size() > 2 && cols[2] == "0")
            {
                label += " [off]";
            }
            if (cols.size() > 3 && cols[3] == "1")
            {
                label += " [frz]";
            }
            auto item = box_value(to_hstring(label));
            LayersList().Items().Append(item);
        }
    }

    void MainWindow::refreshBlocks()
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        BlocksList().Items().Clear();
        elementPlaceNames_.clear();

        std::istringstream blocksIn(session_->blocksList());
        std::string line;
        while (std::getline(blocksIn, line))
        {
            if (line.empty())
            {
                continue;
            }
            BlocksList().Items().Append(box_value(to_hstring(line)));
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
            const std::string display =
                (cols.size() > 3 && !cols[3].empty()) ? cols[3] : name;
            BlocksList().Items().Append(box_value(to_hstring(display)));
            elementPlaceNames_.push_back(name);
        }

        if (BlocksList().Items().Size() == 0)
        {
            BlocksList().Items().Append(
                box_value(hstring{ L"(no elements  -  OPEN a drawing or place from catalog)" }));
            elementPlaceNames_.clear();
        }
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
        for (const auto id : ids)
        {
            auto summary = session_->entityPropSummary(id);
            std::wstring line = L"#" + std::to_wstring(id);
            if (!summary.empty())
            {
                std::vector<std::string> cols;
                {
                    std::string cell;
                    std::istringstream row(summary);
                    while (std::getline(row, cell, '\t'))
                    {
                        cols.push_back(cell);
                    }
                }
                if (id == ids.front() && cols.size() > 1)
                {
                    PropsLayerBox().Text(to_hstring(cols[1]));
                }
                for (char& c : summary)
                {
                    if (c == '\t')
                    {
                        c = ' ';
                    }
                }
                line += L"  ";
                line += to_hstring(summary);
            }
            PropsList().Items().Append(box_value(hstring{ line }));
        }
    }

    void MainWindow::startTool(std::string const& cmd)
    {
        if (!session_ || !session_->ok())
        {
            return;
        }
        if (session_->toolStart(cmd))
        {
            activeTool_ = upperAscii(trimCopy(cmd));
            appendHistory(to_hstring(cmd + " (click · Space finishes)"));
            if (activeTool_ == "DOOR" || activeTool_ == "WINDOW")
            {
                appendHistory(L"Tab — toggle swing flip while placing");
            }
            refreshCanvas();
            refreshStatus();
        }
        else
        {
            appendHistory(to_hstring(std::string("! cannot start tool ") + cmd));
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

    void MainWindow::clearActiveTool()
    {
        activeTool_.clear();
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
        if (utf8.empty() || !session_ || !session_->ok())
        {
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

        const auto trimmed = trimCopy(utf8);
        double number = 0.0;
        const bool toolActive = session_->toolActive();
        const bool numberTool =
            !toolActive || activeTool_ == "FILLET" || activeTool_ == "WALL";
        if (numberTool && tryParseFloat(trimmed, &number) && session_->toolSetNumber(number))
        {
            appendHistory(to_hstring(std::string("Number → ") + trimmed));
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
        refreshCanvas(true);
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

    bool MainWindow::handleHotkeys(VirtualKey key)
    {
        if (!session_ || !session_->ok())
        {
            return false;
        }
        if (key == VirtualKey::F8)
        {
            const bool on = !session_->orthoGet();
            session_->orthoSet(on);
            appendHistory(on ? L"ORTHO on" : L"ORTHO off");
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::F10)
        {
            const bool on = !session_->polarGet();
            session_->polarSet(on);
            appendHistory(on ? L"POLAR on" : L"POLAR off");
            refreshStatus();
            return true;
        }
        if (key == VirtualKey::F3)
        {
            osnapOn_ = !osnapOn_;
            session_->osnapSet(osnapOn_);
            appendHistory(osnapOn_ ? L"OSNAP on" : L"OSNAP off");
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

    void MainWindow::Window_KeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }
        if (e.Key() == VirtualKey::Tab && tryTogglePlaceFlip())
        {
            e.Handled(true);
            return;
        }
        if (e.Key() == VirtualKey::Space && tryCycleGripMode())
        {
            e.Handled(true);
        }
    }

    void MainWindow::CommandLine_KeyDown(
        Windows::Foundation::IInspectable const&,
        KeyRoutedEventArgs const& e)
    {
        if (handleHotkeys(e.Key()))
        {
            e.Handled(true);
            return;
        }

        if (e.Key() == VirtualKey::Escape)
        {
            e.Handled(true);
            ToolsPane().Visibility(Visibility::Collapsed);
            hideCompletions();
            if (session_ && session_->toolActive())
            {
                session_->toolCancel();
                clearActiveTool();
                appendHistory(L"*Cancel*");
                refreshCanvas();
                refreshStatus();
            }
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

        if (e.Key() == VirtualKey::Space)
        {
            auto text = CommandLine().Text();
            if (text.empty() && session_ && session_->toolActive())
            {
                e.Handled(true);
                if (session_->toolFinish())
                {
                    clearActiveTool();
                    appendHistory(L"(Space) finish");
                    refreshCanvas(true);
                    refreshStatus();
                    refreshProperties();
                }
                return;
            }
            if (text.empty() && tryCycleGripMode())
            {
                e.Handled(true);
                return;
            }
            // Space on bare interactive alias runs like Enter (before space inserts).
            auto bare = trimCopy(to_string(text));
            if (!bare.empty() && bare.find(' ') == std::string::npos && session_ &&
                session_->ok() && session_->isInteractive(bare))
            {
                e.Handled(true);
                CommandLine().Text(L"");
                hideCompletions();
                feedCommand(bare);
                return;
            }
        }

        if (e.Key() != VirtualKey::Enter)
        {
            return;
        }
        e.Handled(true);
        hideCompletions();
        auto text = CommandLine().Text();
        if (text.empty())
        {
            if (session_ && session_->toolActive() && session_->toolFinish())
            {
                clearActiveTool();
                appendHistory(L"(Enter) finish");
                refreshCanvas(true);
                refreshStatus();
                refreshProperties();
            }
            else if (!cmdHistory_.empty())
            {
                feedCommand(cmdHistory_.back());
            }
            return;
        }
        CommandLine().Text(L"");
        feedCommand(to_string(text));
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
            session_->toolAcceptPoint(wx, wy);
            refreshCanvas();
            refreshStatus();
            refreshProperties();
            if (session_->toolNeedsText())
            {
                appendHistory(L"TEXT  -  type value + Enter");
                CommandLine().Focus(FocusState::Programmatic);
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

        if (panning_)
        {
            canvas_.pan(x - lastX_, y - lastY_);
            lastX_ = x;
            lastY_ = y;
            canvas_.render();
            e.Handled(true);
            return;
        }

        if (gripDrag_)
        {
            lastX_ = x;
            lastY_ = y;
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
            session_->toolSetCursor(canvas_.worldX(x), canvas_.worldY(y));
            if (session_->toolActive())
            {
                refreshCanvas();
                refreshStatus();
            }
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
            refreshFills();
        }
    }

    void MainWindow::showRightPane(bool show)
    {
        RightPane().Visibility(show ? Visibility::Visible : Visibility::Collapsed);
        if (show)
        {
            refreshProperties();
        }
    }

    void MainWindow::showLeftTab(LeftTab tab)
    {
        leftTab_ = tab;
        const bool layers = tab == LeftTab::Layers;
        const bool elements = tab == LeftTab::Elements;
        const bool fills = tab == LeftTab::Fills;
        LayersList().Visibility(layers ? Visibility::Visible : Visibility::Collapsed);
        BlocksList().Visibility(elements ? Visibility::Visible : Visibility::Collapsed);
        FillsList().Visibility(fills ? Visibility::Visible : Visibility::Collapsed);
        LayerActions().Visibility(layers ? Visibility::Visible : Visibility::Collapsed);
        BlockPlaceBtn().Visibility(elements ? Visibility::Visible : Visibility::Collapsed);
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

    void MainWindow::LeftTabFills_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        showLeftTab(LeftTab::Fills);
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
        auto s = to_string(hs);
        if (s.rfind("> ", 0) == 0)
        {
            s = s.substr(2);
        }
        auto bracket = s.find(" [");
        if (bracket != std::string::npos)
        {
            s = s.substr(0, bracket);
        }
        pendingLayer_ = s;
    }

    void MainWindow::LayersList_DoubleTapped(Windows::Foundation::IInspectable const&, DoubleTappedRoutedEventArgs const&)
    {
        if (pendingLayer_.empty() || !session_ || !session_->ok())
        {
            return;
        }
        session_->layerSetCurrent(pendingLayer_);
        appendHistory(to_hstring(std::string("Current layer â†’ ") + pendingLayer_));
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
        rightPinned_ = !rightPinned_;
        showRightPane(rightPinned_ || rightHover_);
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
        session_->otrackSet(true);
        didFit_ = false;
        lastMgrRev_ = 0;
        appendHistory(L"New drawing");
        refreshCanvas(true);
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
        didFit_ = false;
        refreshCanvas(true);
    }

    void MainWindow::QatSave_Click(Windows::Foundation::IInspectable const&, RoutedEventArgs const&)
    {
        auto existing = session_ && session_->ok() ? session_->sessionPath() : std::string{};
        if (!existing.empty())
        {
            feedCommand(std::string("SAVE ") + existing);
            return;
        }
        auto path = pickSavePath();
        if (path.empty())
        {
            return;
        }
        feedCommand(std::string("SAVE ") + path);
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
        // Transparent-friendly extents: keeps active tool.
        feedCommand("'ZOOM E");
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
        feedCommand("PLOTSETUP");
    }

    void MainWindow::ToolBtn_Click(Windows::Foundation::IInspectable const& sender, RoutedEventArgs const&)
    {
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
        if (tag == L"IMAGE")
        {
            startImageTool();
            return;
        }
        startTool(to_string(tag));
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
            return;
        }
        std::string prompt = session_->toolPrompt();
        std::wstring wprompt(prompt.begin(), prompt.end());
        auto layer = session_->layerCurrent();
        std::wstring wlayer(layer.begin(), layer.end());
        auto hatch = session_->hatchPattern();
        std::wstring whatch(hatch.begin(), hatch.end());
        std::wstring chips;
        if (session_->orthoGet())
        {
            chips += L"  ORTHO";
        }
        if (session_->polarGet())
        {
            chips += L"  POLAR";
        }
        if (osnapOn_)
        {
            chips += L"  OSNAP";
        }
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
        wchar_t buf[512]{};
        swprintf_s(
            buf,
            L"ents %llu  sel %llu  rev %llu%s  lyr %s%s%s%s%s%s%s  -  WinUI (M22)",
            static_cast<unsigned long long>(session_->entityCount()),
            static_cast<unsigned long long>(session_->selectionCount()),
            static_cast<unsigned long long>(session_->revision()),
            session_->dirty() ? L" *" : L"",
            wlayer.c_str(),
            whatch.empty() ? L"" : L"  hatch ",
            whatch.c_str(),
            wprompt.empty() ? L"" : L" · ",
            wprompt.c_str(),
            chips.c_str(),
            gripChip.c_str());
        StatusBar().Text(buf);
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
}
