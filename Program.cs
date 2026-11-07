namespace SlimBrowser;

internal static class Program
{
    [STAThread]
    static void Main(string[] args)
    {
        ApplicationConfiguration.Initialize();
        if (args.Contains("--self-test")) { SelfTest.Run(); return; }
        if (args.Contains("--smoke-test"))
        {
            AppSettings.TestDirectory = Path.Combine(Path.GetTempPath(), "SlimBrowser-smoke-" + Guid.NewGuid());
            Application.Run(new BrowserForm(true)); return;
        }
        if (args.Contains("--ui-preview"))
        {
            AppSettings.TestDirectory = Path.Combine(Path.GetTempPath(), "SlimBrowser-ui-" + Guid.NewGuid());
            Application.Run(new BrowserApplication()); return;
        }
        using var mutex = new Mutex(true, "Local\\SlimBrowser.SingleInstance", out var first);
        if (!first) { MessageBox.Show("SlimBrowser уже запущен."); return; }
        Application.ThreadException += (_, e) => MessageBox.Show(e.Exception.Message, "Ошибка SlimBrowser");
        try { Application.Run(new BrowserApplication()); }
        catch (Exception e) { MessageBox.Show(e.Message, "Не удалось запустить SlimBrowser"); }
    }
}

internal sealed class BrowserApplication : ApplicationContext
{
    public static BrowserApplication? Current { get; private set; }
    int windows;
    public BrowserApplication(){Current=this;Open();}
    public void Open(bool privateMode=false,bool empty=false)
    {
        var form=new BrowserForm(privateMode:privateMode,empty:empty);windows++;
        form.FormClosed+=(_,_)=>{if(--windows==0)ExitThread();};form.Show();
    }
}
