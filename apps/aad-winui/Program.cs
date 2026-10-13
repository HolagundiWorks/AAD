using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;

namespace Aad.WinUi;

public static class Program
{
    [STAThread]
    public static void Main(string[] _)
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(_ =>
        {
            var context = new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            new App();
        });
    }
}
