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
        void Window_KeyDown(
            winrt::Windows::Foundation::IInspectable const& sender,
            winrt::Microsoft::UI::Xaml::Input::KeyRoutedEventArgs const& e);
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
        void LeftPane_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void LeftPane_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void LeftLayersBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftElementsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftFillsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabLayers_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabElements_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LeftTabFills_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void LayersList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LayersList_DoubleTapped(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::DoubleTappedRoutedEventArgs const&);
        void BlocksList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void FillsList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void LayerOn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerOff_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerFreeze_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void LayerThaw_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void BlockPlace_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void RightRail_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightRail_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPane_PointerEntered(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPane_PointerExited(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Input::PointerRoutedEventArgs const&);
        void RightPropsBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void PropsChspace_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PropsErase_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void PropsLayerApply_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void QatNew_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatOpen_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatSave_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatUndo_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatRedo_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatZoomE_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatReview_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void QatPlot_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

        void ToolBtn_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ToolsPalette_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);
        void ToolsList_ItemClick(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::Controls::ItemClickEventArgs const&);
        void BridgeFlush_Click(winrt::Windows::Foundation::IInspectable const&, winrt::Microsoft::UI::Xaml::RoutedEventArgs const&);

    private:
        enum class LeftTab { Layers, Elements, Fills };

        void initCanvas();
        void resizeCanvas();
        void refreshCanvas(bool fitIfEmptyView = false);
        void refreshStatus();
        void refreshProperties();
        void refreshLayers();
        void refreshBlocks();
        void refreshFills();
        void refreshManagersIfNeeded();
        void appendHistory(winrt::hstring const& line);
        void feedCommand(std::string const& utf8);
        void applyPendingNav();
        void startTool(std::string const& cmd);
        void startImageTool();
        bool tryTogglePlaceFlip();
        bool tryCycleGripMode();
        void showLeftPane(bool show);
        void showRightPane(bool show);
        void showLeftTab(LeftTab tab);
        void scheduleHideLeft();
        void scheduleHideRight();
        void updateCompletions();
        void hideCompletions();
        void pushCmdHistory(std::string const& line);
        bool handleHotkeys(winrt::Windows::System::VirtualKey key);
        void clearActiveTool();
        std::string selectedLayerName() const;
        std::string selectedBlockName() const;
        void layerCmd(char const* flag);

        std::unique_ptr<FfiSession> session_;
        SwapChainCanvas canvas_;
        bool canvasReady_ = false;
        bool panning_ = false;
        float lastX_ = 0.f;
        float lastY_ = 0.f;
        bool didFit_ = false;
        bool leftPinned_ = false;
        bool rightPinned_ = false;
        bool leftHover_ = false;
        bool rightHover_ = false;
        LeftTab leftTab_ = LeftTab::Layers;
        uint64_t lastMgrRev_ = 0;
        std::string pendingLayer_;
        std::string pendingBlock_;
        std::string activeTool_;
        bool osnapOn_ = true;
        std::vector<std::string> elementPlaceNames_;
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
