#pragma once

#include "MainWindow.g.h"

#include "../../include/FfiSession.hpp"
#include "../../include/SwapChainCanvas.hpp"

#include <memory>
#include <string>
#include <vector>

namespace winrt::AadWinui::implementation
{
    struct MainWindow : MainWindowT<MainWindow>
    {
        MainWindow();

        void CommandLine_KeyDown(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& e);
        void CommandLine_PreviewKeyDown(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& e);
        void Window_KeyDown(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& e);
        void Window_PreviewKeyDown(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& e);
        void Window_CharacterReceived(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::CharacterReceivedRoutedEventArgs const& e);
        void CommandLine_CharacterReceived(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::CharacterReceivedRoutedEventArgs const& e);
        void DynInput_CharacterReceived(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::CharacterReceivedRoutedEventArgs const& e);
        void CommandLine_TextChanged(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Controls::TextChangedEventArgs const& e);
        void CompleteList_ItemClick(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const& e);

        void CanvasHost_SizeChanged(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::SizeChangedEventArgs const& e);
        void CanvasHost_CompositionScaleChanged(
            winrt::Microsoft::UI::Xaml::Controls::SwapChainPanel const& sender,
            winrt::Windows::Foundation::IInspectable const& args);
        void CanvasHost_PointerPressed(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& e);
        void CanvasHost_PointerMoved(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& e);
        void CanvasHost_PointerReleased(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& e);
        void CanvasHost_PointerWheelChanged(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const& e);

        void LeftRail_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void LeftRail_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void SideRail_SizeChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::SizeChangedEventArgs const&);
        void LeftPane_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void LeftPane_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void LeftLayersBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftElementsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftLevelsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftFillsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabLayers_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabElements_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabLevels_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabFills_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabMeta_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void DraftBarSync_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void DraftBarMeta_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        winrt::fire_and_forget DrawingProps_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void MetaApply_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void MetaReload_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void LayersList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LayersList_DoubleTapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::DoubleTappedRoutedEventArgs const&);
        void BlocksList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LevelsList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LevelsList_DoubleTapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::DoubleTappedRoutedEventArgs const&);
        void FillsList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LayerOn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerOff_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerFreeze_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerThaw_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerLock_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerUnlock_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerNew_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerCurrent_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerChip_Tapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&);
        void LayerStandards_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerSetItem_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerApplyProps_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerFilter_TextChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::TextChangedEventArgs const&);
        void LayerColorCombo_SelectionChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void LevelNew_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LevelCurrent_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LevelChip_Tapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&);
        void CategoryBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void FloorCombo_SelectionChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void ElementCombo_SelectionChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void TraceCombo_SelectionChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void syncFloorElementTraceCombos();
        void syncActiveCategoryLayer();
        void refreshFirmChip();
        void seedDrawingPropsFromConnect();
        void refreshMetaPane();
        void BlockPlace_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void RightRail_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightRail_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPane_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPane_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPropsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightHistoryBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightPlotBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightLispBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightTabProps_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightTabHistory_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightTabPlot_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void RightTabLisp_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PlotPdf_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PlotSetup_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LispRun_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LispLoad_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LispClear_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LispSample_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void PropsChspace_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PropsErase_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PropsLayerApply_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void HistorySave_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void HistoryRefresh_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void HistoryPurge_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void HistoryRestore_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void HistoryList_SelectionChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::SelectionChangedEventArgs const&);
        void UnitsChip_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void UnitsImperial_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void UnitsMetric_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        winrt::fire_and_forget DimManager_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void QatNew_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatOpen_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatSave_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatUndo_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatRedo_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatZoomE_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ZoomExtents_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ZoomWindow_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ZoomIn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ZoomOut_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ViewLock_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void zoomToExtents();
        void syncViewLockChip();
        bool tryApplyTypedLength(std::string const& utf8);
        void QatReview_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatPlot_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void MenuExit_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void MenuAbout_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ComfortHits_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void ToolBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ToolsPalette_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ToolsList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void BridgeFlush_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcClose_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcClear_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcUnit_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcKey_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void CalcExpr_TextChanged(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::TextChangedEventArgs const&);
        void CalcExpr_KeyDown(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const&);
        void refreshCalcResult();

        void AidOsnap_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void AidOrtho_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void AidPolar_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void AidDyn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void AidLwt_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void DynInput_KeyDown(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const&);
        void DynInput_PreviewKeyDown(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const&);

        void PomodoroToggle_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void Pomodoro_Tapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::TappedRoutedEventArgs const&);
        void Pomodoro_DoubleTapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::DoubleTappedRoutedEventArgs const&);
        void PomodoroCrown_PointerPressed(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void PomodoroCrown_PointerMoved(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void PomodoroCrown_PointerReleased(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);

    private:
        enum class LeftTab { Layers, Elements, Levels, Fills, Meta };
        enum class RightTab { Properties, History, Plot, Lisp };
        enum class XlineMode { Through, Hor, Ver };

        void initCanvas();
        void resizeCanvas();
        void refreshCanvas(bool fitIfEmptyView = false);
        void refreshStatus();
        void refreshProperties();
        void refreshHistory();
        void refreshLayers();
        void refreshBlocks();
        void refreshLevels();
        void refreshFills();
        void refreshManagersIfNeeded();
        void appendHistory(winrt::hstring const& line);
        void feedCommand(std::string const& utf8);
        void applyPendingNav();
        void startTool(std::string const& cmd);
        void startImageTool();
        bool tryTogglePlaceFlip();
        bool tryCycleGripMode();
        /// Space / Enter: finish tool, else repeat last command, else grip cycle.
        bool submitLikeEnter();
        /// AutoCAD peer: Space submits CLI/dyn text, else submitLikeEnter.
        bool acceptSpaceAsEnter();
        /// True when Space should insert a literal character (layer name, LISP, …).
        bool allowsLiteralSpace();
        bool isCliOrDynFocused();
        void focusCommandLineForTyping();
        void showLeftPane(bool show);
        void showRightPane(bool show);
        void showLeftTab(LeftTab tab);
        void showRightTab(RightTab tab);
        void scheduleHideLeft();
        void scheduleHideRight();
        void updateCompletions();
        void hideCompletions();
        void pushCmdHistory(std::string const& line);
        bool handleHotkeys(winrt::Windows::System::VirtualKey key);
        bool handleEscape();
        void cycleFocusRegions();
        void updateTitleBar();
        void applyComfortHits();
        void clearActiveTool();
        std::string selectedLayerName() const;
        std::string selectedBlockName() const;
        void layerCmd(char const* flag);
        std::string selectedPlotPaper();
        void appendLispOutput(winrt::hstring const& line);
        void startClockPomodoro();
        void tickClock();
        void tickPomodoro();
        void updatePomodoroUi();
        void updatePomodoroRing(double frac);
        void togglePomodoro();
        void resetPomodoro();
        void applyPomodoroDrag(winrt::Windows::Foundation::Point const& pt);
        void setRibbonPath(std::string const& path);
        void syncAidChips();
        void syncPanelsChrome();
        void stylePaneTab(winrt::Microsoft::UI::Xaml::Controls::Button const& btn, bool active);
        void initLayerPropEditors();
        void syncLayerPropEditors(std::string const& name);
        void applyLayerColorSwatch(uint8_t aci);
        void seedStandardLayersIfSparse();
        void updateDynOverlay(float screenX, float screenY, double worldX, double worldY);
        void placeXlineAt(double wx, double wy);
        void applyDynOrCommand(std::string const& utf8);
        void applyXlinePreview(std::vector<float>& preview);
        bool tryRectangleDimensionInput(std::string const& utf8);
        void refreshUnitsChip();
        bool parseCommaPair(std::string const& text, double* a, double* b) const;

        std::unique_ptr<FfiSession> session_;
        SwapChainCanvas canvas_;
        bool canvasReady_ = false;
        bool panning_ = false;
        float lastX_ = 0.f;
        float lastY_ = 0.f;
        bool didFit_ = false;
        /// When true, REGEN / refresh never re-fits the paper (keeps pan/zoom).
        bool viewLocked_ = true;
        bool leftPinned_ = false;
        bool rightPinned_ = false;
        bool leftHover_ = false;
        bool rightHover_ = false;
        LeftTab leftTab_ = LeftTab::Layers;
        RightTab rightTab_ = RightTab::Properties;
        std::vector<std::string> historyRestoreIds_;
        uint64_t lastMgrRev_ = 0;

        winrt::Microsoft::UI::Dispatching::DispatcherQueueTimer clockTimer_{ nullptr };
        winrt::Microsoft::UI::Dispatching::DispatcherQueueTimer pomodoroClickTimer_{ nullptr };
        int pomodoroDurationSec_ = 25 * 60;
        int pomodoroLeftSec_ = 25 * 60;
        bool pomodoroRunning_ = false;
        bool pomodoroDragging_ = false;
        bool pomodoroPressActive_ = false;
        winrt::Windows::Foundation::Point pomodoroPressOrigin_{ 0, 0 };
        std::string pendingLayer_;
        std::string pendingBlock_;
        std::string pendingLevel_;
        std::vector<std::string> layerNames_;
        std::vector<uint8_t> layerAci_;
        std::vector<std::string> layerLtypes_;
        std::vector<double> layerLw_;
        bool layerEditorsReady_ = false;
        bool layerPropSyncing_ = false;
        std::string activeLevelName_ = "Ground";
        std::string activeCategoryName_ = "Walls";
        std::string traceMode_ = "OFF";
        bool floorUiSyncing_ = false;
        std::string activeTool_;
        std::vector<std::string> traceComboTags_;
        XlineMode xlineMode_ = XlineMode::Through;
        bool toolHasAnchor_ = false;
        double toolAnchorX_ = 0.0;
        double toolAnchorY_ = 0.0;
        enum class RectDimPhase { None, Length, Width };
        RectDimPhase rectDimPhase_ = RectDimPhase::None;
        double rectDimLength_ = 0.0;
        bool osnapOn_ = true;
        bool dynMode_ = true;
        bool comfortHits_ = false;
        bool paperHovered_ = false;
        std::string unitsToken_ = "ft-in";
        bool calcImperial_ = false;
        int focusCycle_ = 0;
        std::wstring documentLeaf_ = L"Untitled drawing";
        std::vector<std::string> elementPlaceNames_;
        std::vector<std::string> levelNames_;
        std::vector<std::string> fillPatternNames_;
        std::vector<std::string> cmdHistory_;
        int historyIndex_ = -1;
        bool boxDrag_ = false;
        bool boxActive_ = false;
        bool zoomWindowPending_ = false;
        float boxX0_ = 0.f;
        float boxY0_ = 0.f;
        float boxX1_ = 0.f;
        float boxY1_ = 0.f;
        bool gripDrag_ = false;
        uint64_t gripId_ = 0;
        uint64_t gripIdx_ = 0;
    };
}

namespace winrt::AadWinui::factory_implementation
{
    struct MainWindow : MainWindowT<MainWindow, implementation::MainWindow>
    {
    };
}
