#include "pch.h"
#include "App.xaml.h"
#include "MainWindow.xaml.h"

using namespace winrt;
using namespace Microsoft::UI::Xaml;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace winrt::AadWinui::implementation
{
    /// <summary>
    /// Initializes the singleton application object.  This is the first line of authored code
    /// executed, and as such is the logical equivalent of main() or WinMain().
    /// </summary>
    App::App()
    {
        InitializeComponent();

        UnhandledException([](IInspectable const&, UnhandledExceptionEventArgs const& e)
        {
            try
            {
                auto msg = e.Message();
                FILE* f = nullptr;
                if (fopen_s(&f, "aadwinui_crash.txt", "w") == 0 && f)
                {
                    fprintf(f, "%ls\n", msg.c_str());
                    fclose(f);
                }
            }
            catch (...) {}
            if (IsDebuggerPresent())
            {
                __debugbreak();
            }
        });
    }

    /// <summary>
    /// Invoked when the application is launched.
    /// </summary>
    /// <param name="e">Details about the launch request and process.</param>
    void App::OnLaunched([[maybe_unused]] LaunchActivatedEventArgs const& e)
    {
        window = make<MainWindow>();
        window.Activate();
    }
}
