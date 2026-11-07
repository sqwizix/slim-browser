using System.Diagnostics;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

namespace SlimBrowser;

internal sealed class BrowserTab
{
    public TabPage Page { get; } = new MaterialTabPage("Новая вкладка");
    public string Url { get; set; } = Navigation.Home;
    public string Title { get; set; } = "Новая вкладка";
    public WebView2? View { get; set; }
    public StartPanel? Start { get; set; }
    public long BackgroundSince { get; set; } = Environment.TickCount64;
    public bool Loading { get; set; }
    public bool Navigating { get; set; }
    public bool Downloading => Downloads.Any(d => d.State == CoreWebView2DownloadState.InProgress);
    public List<CoreWebView2DownloadOperation> Downloads { get; } = [];
}

internal sealed class BrowserForm : MacWindow
{
    readonly AppSettings settings = AppSettings.Load();
    readonly PasswordManager vault = new();
    readonly BrowserEngine engine = new();
    readonly TabControl tabs = new ContentTabs() { Dock = DockStyle.Fill };
    readonly List<BrowserTab> models = [];
    readonly BrowserLibrary library = new();
    readonly bool privateMode;
    readonly List<SiteEntry> sessionHistory = [];
    readonly FlowLayoutPanel tabStrip = new MaterialFlowPanel() { Dock = DockStyle.Fill, AutoScroll = true, WrapContents = false, FlowDirection = FlowDirection.TopDown, Padding = new Padding(0,4,0,0) };
    readonly Panel sidebar = new MaterialPanel { Dock = DockStyle.Left, Width = 222, Padding = new Padding(12, 8, 10, 12) };
    readonly ChromeButton sidebarToggle = new("Боковая панель");
    readonly TextBox address = new GlassTextBox() { Dock = DockStyle.Fill, PlaceholderText = "Адрес сайта или поиск Google", AccessibleName = "Адрес сайта или поиск Google" };
    readonly Label status = new() { Dock = DockStyle.Bottom, Height = 25, TextAlign = ContentAlignment.MiddleLeft, Padding = new Padding(8, 0, 0, 0) };
    readonly Button back = MakeButton("Назад"), forward = MakeButton("Вперед"), refresh = MakeButton("Обновить"), clean = MakeButton("Очистить RAM");
    readonly System.Windows.Forms.Timer timer = new() { Interval = 15000 };
    BrowserTab? previous;
    bool closing, allowClose;
    BrowserTab? Active => models.FirstOrDefault(t => t.Page == tabs.SelectedTab);
    public BrowserForm(bool smokeTest = false,bool privateMode=false,bool empty=false)
    {
        this.privateMode=privateMode;
        Text = privateMode?"SlimBrowser · Приватное окно":"SlimBrowser"; Font = new Font("Segoe UI", 10); Size = new Size(1280, 850); MinimumSize = new Size(900, 550);
        ChromeTheme.Dark = settings.DarkMode;
        var toolbar = new MaterialTablePanel { Dock = DockStyle.Top, Height = 52, Padding = new Padding(12,6,12,6), ColumnCount = 6, RowCount = 1 };
        toolbar.RowStyles.Add(new RowStyle(SizeType.Percent,100));
        for(int i=0;i<6;i++) toolbar.ColumnStyles.Add(new(i==4?SizeType.Percent:SizeType.Absolute,i==4?100:42));
        var options=MakeButton("Меню");
        var addressShell=new RoundedPanel { Dock=DockStyle.Fill,Padding=new Padding(10,7,12,5),Margin=new Padding(12,0,12,0) };
        address.BorderStyle=BorderStyle.None; address.Font=new Font("Segoe UI",10);addressShell.Controls.Add(address);
        Control[] controls=[sidebarToggle,back,forward,refresh,addressShell,options];
        for(int i=0;i<controls.Length;i++)toolbar.Controls.Add(controls[i],i,0);
        tabs.Appearance=TabAppearance.FlatButtons;tabs.ItemSize=new Size(0,1);tabs.SizeMode=TabSizeMode.Fixed;
        var add=new TabSelectButton{Text="+   Новая вкладка",AccessibleName="Новая вкладка",Dock=DockStyle.Top,Height=36,BackColor=ChromeTheme.Surface};
        sidebar.Controls.Add(tabStrip);sidebar.Controls.Add(add);sidebar.Visible=settings.SidebarVisible;
        sidebarToggle.Selected=settings.SidebarVisible;
        sidebarToggle.Click+=(_,_)=>ToggleSidebar();
        status.Visible=false;
        status.TextChanged+=(_,_)=>status.Visible=!string.IsNullOrEmpty(status.Text)&&status.Text!="Готово"&&status.Text!="Загрузка…";
        Controls.Add(tabs);Controls.Add(status);Controls.Add(sidebar);Controls.Add(toolbar);Controls.Add(MakeTitleBar(privateMode?"Приватное окно":""));
        add.Click+=(_,_)=>AddTab();
        options.Click+=(_,_)=>ShowMenu(options);
        ApplyTheme();
        back.Click += (_, _) => { if (Active?.View?.CoreWebView2 is { CanGoBack: true } c) c.GoBack(); };
        forward.Click += (_, _) => { if (Active?.View?.CoreWebView2 is { CanGoForward: true } c) c.GoForward(); };
        refresh.Click += async (_, _) => await Safe(async () => { if (Active is { } t) { if (t.View?.CoreWebView2 is { } c) { if(t.Navigating)c.Stop();else c.Reload(); } else await EnsureAsync(t); } });
        address.Enter+=(_,_)=>{if(Active is {} t)address.Text=Navigation.IsHome(t.Url)?"":t.Url;};
        address.Leave+=(_,_)=>UpdateAddress();
        address.KeyDown += async (_, e) =>
        {
            if (e.KeyCode != Keys.Enter) return;
            e.SuppressKeyPress = true;
            await Safe(async () =>
            {
                if(Active==null)AddTab();
                if (Active is not { } t) return;
                t.Url = Navigation.Resolve(address.Text); await EnsureAsync(t);
                t.View?.CoreWebView2?.Navigate(t.Url); t.View?.Focus();
            });
        };
        tabs.SelectedIndexChanged += async (_, _) => await Safe(SelectAsync);
        timer.Tick += (_, _) => Tick();
        Shown += async (_, _) => { ApplyMaterial(settings.Glass); if(!empty)AddTab(); timer.Start(); if (smokeTest) await SmokeAsync(); };
        FormClosing += OnClosing;
        FormClosed += (_, _) => { timer.Dispose(); foreach (var t in models) t.View?.Dispose(); };
    }
    void ToggleSidebar()
    {
        settings.SidebarVisible=!settings.SidebarVisible;sidebar.Visible=settings.SidebarVisible;
        sidebarToggle.Selected=settings.SidebarVisible;sidebarToggle.AccessibleDescription=settings.SidebarVisible?"Боковая панель открыта":"Боковая панель скрыта";
        sidebarToggle.Invalidate();settings.Save();
    }
    void UpdateAddress()
    {
        if(Active is not {} tab){address.Clear();return;}
        if(Navigation.IsHome(tab.Url)){address.Clear();return;}
        address.Text=address.Focused?tab.Url:new Uri(tab.Url).Authority;
    }
    async void ShowSettings(string section="Внешний вид")
    {
        if(Active==null)AddTab();
        await Safe(async()=>{if(Active is {} tab)await EnsureAsync(tab,true);});
        using var dialog=new SettingsForm(settings,vault,Active?.Url??Navigation.Home,ApplyTheme,()=>_ = Safe(CleanAsync),Active?.View?.CoreWebView2,section);
        dialog.ShowDialog(this);
    }
    void ShowLibrary(string name,string title)
    {
        using var dialog=new LibraryForm(title,privateMode&&name=="history"?sessionHistory:library.Read(name),url=>AddTab(url),()=>{if(privateMode&&name=="history")sessionHistory.Clear();else library.Clear(name);});
        dialog.ShowDialog(this);
    }
    void ShowMenu(Control anchor)
    {
        var menu=new BrowserMenu { ShowImageMargin=true,ShowCheckMargin=false,ShowItemToolTips=false,Font=Font,BackColor=ChromeTheme.SolidSurface,ForeColor=ChromeTheme.Ink,Padding=new Padding(8),Renderer=new BrowserMenuRenderer() };
        void Item(string label,Action action,string shortcut=""){var item=new ToolStripMenuItem(label){Padding=new Padding(8,8,16,8),Image=MenuIcons.Create(label),ShortcutKeyDisplayString=shortcut};item.Click+=(_,_)=>action();menu.Items.Add(item);}
        Item("Новая вкладка",()=>AddTab(),"Ctrl+T");
        Item("Новое окно",()=>BrowserApplication.Current?.Open(),"Ctrl+N");
        Item("Новое пустое окно",()=>BrowserApplication.Current?.Open(empty:true));
        Item("Новое приватное окно",()=>BrowserApplication.Current?.Open(privateMode:true),"Ctrl+Shift+P");
        Item(settings.SidebarVisible?"Скрыть боковую панель":"Показать боковую панель",ToggleSidebar);
        menu.Items.Add(new ToolStripSeparator());
        Item("Журнал",()=>ShowLibrary("history","Журнал"));
        Item("Закладки",()=>ShowLibrary("bookmarks","Закладки"));
        Item("Добавить страницу в закладки",()=>{if(Active is {} tab)library.Add("bookmarks",tab.Title,tab.Url);});
        Item("Загрузки",()=>_ = Safe(async()=>{if(Active==null)AddTab();if(Active is {} tab){await EnsureAsync(tab,true);tab.View?.CoreWebView2?.OpenDefaultDownloadDialog();}}));
        Item("Пароли",()=>ShowSettings("Пароли и автозаполнение"));
        menu.Items.Add(new ToolStripSeparator());
        Item("Расширения и темы",()=>ShowSettings("Расширения и темы"));
        Item("Настройки",()=>ShowSettings());
        menu.Closed+=(_,_)=>BeginInvoke(()=>{foreach(ToolStripItem item in menu.Items)item.Image?.Dispose();menu.Dispose();});menu.Show(anchor,new Point(anchor.Width-menu.PreferredSize.Width,anchor.Height+6));
    }
    void ApplyTheme()
    {
        ChromeTheme.Dark=settings.DarkMode;ChromeTheme.Glass=settings.Glass;ChromeTheme.Transparency=settings.GlassTransparency;ChromeTheme.Apply(this);ApplyMaterial(settings.Glass);
        foreach(var tab in models) if(tab.View?.CoreWebView2 is {} core)
        {
            core.Profile.PreferredColorScheme=settings.DarkMode?CoreWebView2PreferredColorScheme.Dark:CoreWebView2PreferredColorScheme.Light;
            core.Profile.DefaultDownloadFolderPath=settings.DownloadFolder;tab.View.ZoomFactor=settings.Zoom;
            if(Navigation.IsHome(tab.Url)) _ = core.ExecuteScriptAsync("document.documentElement.dataset.glass='"+(settings.Glass?"true":"false")+"'");
        }
        StyleTabStrip();
    }
    void StyleTabStrip()
    {
        foreach(Control chip in tabStrip.Controls)
        {
            chip.BackColor=chip.Tag==Active?ChromeTheme.Field:ChromeTheme.Surface;
            foreach(Control child in chip.Controls){child.BackColor=chip.BackColor;child.ForeColor=ChromeTheme.Ink;child.Invalidate();}
            chip.Invalidate();
        }
    }
    void RefreshTabStrip()
    {
        tabStrip.SuspendLayout();
        foreach(Control old in tabStrip.Controls.Cast<Control>().ToArray()){tabStrip.Controls.Remove(old);old.Dispose();}
        foreach(var tab in models)
        {
            var chip=new RoundedPanel { Tag=tab,Width=190,Height=40,Margin=new Padding(0,0,0,6),Padding=new Padding(8,4,6,4),BackColor=tab==Active?ChromeTheme.Field:ChromeTheme.Surface };
            var select=new TabSelectButton { Text=tab.Page.Text,Dock=DockStyle.Fill,FlatStyle=FlatStyle.Flat,TextAlign=ContentAlignment.MiddleLeft,Padding=new Padding(8,0,0,0),ForeColor=ChromeTheme.Ink,BackColor=chip.BackColor,AccessibleName="Вкладка: "+tab.Title,AutoEllipsis=false };
            select.FlatAppearance.BorderSize=0;select.Click+=(_,_)=>tabs.SelectedTab=tab.Page;
            var close=MakeButton("Закрыть");close.AccessibleName="Закрыть вкладку: "+tab.Title;close.Dock=DockStyle.Right;close.Width=30;close.Click+=(_,_)=>CloseTab(tab);
            chip.Controls.Add(select);chip.Controls.Add(close);tabStrip.Controls.Add(chip);
        }
        tabStrip.ResumeLayout();
    }
    void ShowStart(BrowserTab tab)
    {
        if(Navigation.IsHome(tab.Url))
        {
            if(tab.Start==null || tab.Start.IsDisposed)
            {
                tab.Start=new StartPanel(value=>_ = Safe(async()=>{tab.Url=Navigation.Resolve(value);await EnsureAsync(tab);tab.View?.CoreWebView2?.Navigate(tab.Url);ShowStart(tab);}));
                tab.Page.Controls.Add(tab.Start);ChromeTheme.Apply(tab.Start);
            }
            tab.Start.Visible=true;tab.Start.BringToFront();if(tab.View!=null)tab.View.Visible=false;
        }
        else {if(tab.Start!=null)tab.Start.Visible=false;if(tab.View!=null)tab.View.Visible=true;}
    }
    static Button MakeButton(string text) => new ChromeButton(text);
    async Task Safe(Func<Task> action)
    {
        try { if (!closing) await action(); }
        catch (Exception e) { if (!closing) { status.Text = e.Message; MessageBox.Show(this, e.Message, "SlimBrowser", MessageBoxButtons.OK, MessageBoxIcon.Warning); } }
    }
    void AddTab(string? url = null)
    {
        if (closing) return;
        var tab = new BrowserTab { Url = url ?? Navigation.Home };
        models.Add(tab); tabs.TabPages.Add(tab.Page); tabs.SelectedTab = tab.Page;
        _ = Safe(SelectAsync);
    }
    async Task SelectAsync()
    {
        var active = Active;
        if (previous != active && previous is { } old)
        {
            old.BackgroundSince = Environment.TickCount64;
            if (old.View?.CoreWebView2 is { } c) c.MemoryUsageTargetLevel = CoreWebView2MemoryUsageTargetLevel.Low;
        }
        previous = active;
        RefreshTabStrip();
        if (active == null) return;
        UpdateAddress();
        await EnsureAsync(active);
        if (Active == active && active.View?.CoreWebView2 is { } core) core.MemoryUsageTargetLevel = CoreWebView2MemoryUsageTargetLevel.Normal;
        UpdateNavigation();
    }
    async Task EnsureAsync(BrowserTab tab,bool force=false)
    {
        if (tab.View != null || tab.Loading || closing || !models.Contains(tab)) return;
        if(Navigation.IsHome(tab.Url)&&!force){ShowStart(tab);return;}
        tab.Loading = true;
        WebView2? view = null;
        try
        {
            var environment = await engine.Environment;
            if (closing || !models.Contains(tab)) return;
            tab.Start?.Dispose();tab.Start=null;
            view = new WebView2 { Dock = DockStyle.Fill, DefaultBackgroundColor = Color.Transparent, CreationProperties = new CoreWebView2CreationProperties { IsInPrivateModeEnabled=privateMode, ProfileName=privateMode?"Private":"Default" } };
            tab.View = view; tab.Page.Controls.Clear(); tab.Page.Controls.Add(view);
            await view.EnsureCoreWebView2Async(environment);
            if (closing || tab.View != view || view.IsDisposed) return;
            var core = view.CoreWebView2;
            BrowserEngine.Configure(core);
            core.Profile.DefaultDownloadFolderPath=settings.DownloadFolder;view.ZoomFactor=settings.Zoom;
            core.Profile.PreferredColorScheme = settings.DarkMode ? CoreWebView2PreferredColorScheme.Dark : CoreWebView2PreferredColorScheme.Light;
            core.SetVirtualHostNameToFolderMapping("slim-browser.invalid", Path.Combine(AppContext.BaseDirectory, "Home"), CoreWebView2HostResourceAccessKind.DenyCors);
            core.MemoryUsageTargetLevel = Active == tab ? CoreWebView2MemoryUsageTargetLevel.Normal : CoreWebView2MemoryUsageTargetLevel.Low;
            core.NavigationStarting += (_, e) =>
            {
                if (Uri.TryCreate(e.Uri, UriKind.Absolute, out var requested) && requested.Host == "slim-browser.invalid" && requested.AbsolutePath == "/navigate")
                {
                    e.Cancel = true;
                    var query = requested.Query.TrimStart('?').Split('&').FirstOrDefault(x => x.StartsWith("q="))?[2..] ?? "";
                    var target = Navigation.Resolve(Uri.UnescapeDataString(query.Replace("+", " ")));
                    BeginInvoke(() => { if (!closing && tab.View == view) core.Navigate(target); });
                    return;
                }
                if (!Navigation.IsWeb(e.Uri)) { e.Cancel = true; status.Text = "Разрешены только адреса HTTP и HTTPS."; }
                else {tab.Navigating=true;if (Active == tab) status.Text = "Загрузка…";UpdateNavigation();}
            };
            core.SourceChanged += (_, _) => { tab.Url = core.Source; ShowStart(tab); if (Active == tab) UpdateAddress(); };
            core.DocumentTitleChanged += (_, _) => { tab.Title = core.DocumentTitle; UpdateTitle(tab); };
            core.HistoryChanged += (_, _) => UpdateNavigation();
            core.NavigationCompleted += async (_, e) =>
            {
                if (closing || tab.View != view) return;
                tab.Navigating=false;UpdateNavigation();
                if (Active == tab) status.Text = e.IsSuccess ? "Готово" : "Ошибка загрузки: " + e.WebErrorStatus;
                if(e.IsSuccess && !Navigation.IsHome(tab.Url))
                {
                    if(!privateMode)library.Add("history",tab.Title,tab.Url);
                    else { sessionHistory.RemoveAll(x=>x.Url==tab.Url);sessionHistory.Insert(0,new(tab.Title,tab.Url,DateTime.Now)); }
                }
                if(e.IsSuccess && Navigation.IsHome(tab.Url)) await core.ExecuteScriptAsync("document.documentElement.dataset.glass='"+(settings.Glass?"true":"false")+"'");
                if (e.IsSuccess && settings.SavePasswords && !privateMode)
                {
                    try { await vault.FillAsync(core); }
                    catch (Exception ex) { if (!closing) status.Text = "Автозаполнение недоступно: " + ex.Message; }
                }
            };
            core.NewWindowRequested += (_, e) => { e.Handled = true; if (e.IsUserInitiated && Navigation.IsWeb(e.Uri)) AddTab(e.Uri); };
            core.PermissionRequested += (_, e) =>
            {
                e.State = Active == tab && e.IsUserInitiated && MessageBox.Show(this, $"{e.Uri}\nРазрешить: {e.PermissionKind}?", "Разрешение сайта", MessageBoxButtons.YesNo, MessageBoxIcon.Question) == DialogResult.Yes
                    ? CoreWebView2PermissionState.Allow : CoreWebView2PermissionState.Deny;
            };
            core.DownloadStarting += (_, e) =>
            {
                var operation = e.DownloadOperation; tab.Downloads.Add(operation);
                operation.StateChanged += (_, _) => { if (operation.State != CoreWebView2DownloadState.InProgress) tab.Downloads.Remove(operation); };
            };
            core.ProcessFailed += (_, e) => { if (!closing) status.Text = $"Процесс страницы завершился ({e.ProcessFailedKind}). Закройте вкладку и откройте снова."; };
            core.Navigate(tab.Url); ShowStart(tab); UpdateTitle(tab);
        }
        catch
        {
            if (tab.View == view) { view?.Dispose(); tab.View = null; }
            throw;
        }
        finally { tab.Loading = false; }
    }
    void UpdateTitle(BrowserTab tab) { tab.Page.Text = (tab.View == null ? "[Сон] " : "") + (tab.Title.Length > 28 ? tab.Title[..28] + "…" : tab.Title); RefreshTabStrip(); }
    void UpdateNavigation()
    {
        back.Enabled = Active?.View?.CoreWebView2?.CanGoBack == true; forward.Enabled = Active?.View?.CoreWebView2?.CanGoForward == true;
        ((ChromeButton)refresh).Loading=Active?.Navigating==true;
        refresh.AccessibleName=((ChromeButton)refresh).Loading?"Остановить загрузку":"Обновить страницу";refresh.Invalidate();
    }
    void Discard(BrowserTab tab)
    {
        if (tab == Active || tab.Loading || tab.Downloading || tab.View == null) return;
        tab.View.Dispose(); tab.View = null; tab.Navigating=false; tab.Downloads.Clear(); tab.Start?.Dispose();tab.Start=null;tab.Page.Controls.Clear(); UpdateTitle(tab);
    }
    void Tick()
    {
        if (closing) return;
        if (settings.DiscardMinutes > 0)
            foreach (var tab in models.ToArray()) if (Environment.TickCount64 - tab.BackgroundSince >= settings.DiscardMinutes * 60000L) Discard(tab);
    }
    async Task CleanAsync()
    {
        clean.Enabled = false;
        try
        {
            foreach (var tab in models.ToArray()) Discard(tab);
            if (Active?.View?.CoreWebView2 is { } core)
            {
                await BrowserEngine.ClearAsync(core, false);
                try { await core.CallDevToolsProtocolMethodAsync("HeapProfiler.collectGarbage", "{}"); }
                catch (System.Runtime.InteropServices.COMException) { status.Text = "Кэш очищен; сборка JS-мусора недоступна."; }
            }
            GC.Collect(GC.MaxGeneration, GCCollectionMode.Aggressive, true, true);
            status.Text = "Кэш очищен, фоновые вкладки выгружены (кроме загрузок файлов).";
        }
        finally { if (!closing) clean.Enabled = true; }
    }
    void CloseTab(BrowserTab? tab = null)
    {
        tab ??= Active;
        if (tab == null) return;
        if (tab.Downloading && MessageBox.Show(this, "Закрытие прервет загрузку файла. Продолжить?", "Загрузка", MessageBoxButtons.YesNo) != DialogResult.Yes) return;
        models.Remove(tab); tab.View?.Dispose(); tab.View = null; tabs.TabPages.Remove(tab.Page); tab.Page.Dispose();
        if (models.Count == 0) AddTab();
        RefreshTabStrip();
    }
    async void OnClosing(object? sender, FormClosingEventArgs e)
    {
        if (allowClose) return;
        e.Cancel = true;
        if (closing) return;
        if (models.Any(t => t.Downloading) && MessageBox.Show(this, "Выход прервет загрузки файлов. Продолжить?", "Загрузки", MessageBoxButtons.YesNo) != DialogResult.Yes) return;
        closing = true; timer.Stop(); Enabled = false;
        try
        {
            if ((settings.ClearCookiesOnExit || settings.ClearCacheOnExit) && !privateMode && !Application.OpenForms.OfType<BrowserForm>().Any(f=>f!=this&&!f.privateMode))
            {
                foreach (var tab in models) tab.View?.CoreWebView2?.Stop();
                var environment = await engine.Environment;
                using var cleaner = new WebView2 { Visible = false, Size = new Size(1, 1) };
                Controls.Add(cleaner); await cleaner.EnsureCoreWebView2Async(environment);
                foreach (var tab in models) { tab.View?.Dispose(); tab.View = null; }
                await BrowserEngine.ClearAsync(cleaner.CoreWebView2, settings.ClearCookiesOnExit, settings.ClearCacheOnExit);
            }
            allowClose = true; BeginInvoke(Close);
        }
        catch (Exception ex)
        {
            closing = false; Enabled = true; timer.Start();
            MessageBox.Show(this, "Не удалось очистить данные. Браузер оставлен открытым. Повторите выход или отключите очистку.\n" + ex.Message, "Ошибка очистки");
        }
    }
    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        switch (keyData)
        {
            case Keys.Control | Keys.L: address.Focus(); address.SelectAll(); return true;
            case Keys.Control | Keys.N: BrowserApplication.Current?.Open(); return true;
            case Keys.Control | Keys.Shift | Keys.P: BrowserApplication.Current?.Open(true); return true;
            case Keys.Control | Keys.T: AddTab(); return true;
            case Keys.Control | Keys.W: CloseTab(); return true;
            case Keys.F5: refresh.PerformClick(); return true;
            case Keys.Alt | Keys.Left: back.PerformClick(); return true;
            case Keys.Alt | Keys.Right: forward.PerformClick(); return true;
            default: return base.ProcessCmdKey(ref msg, keyData);
        }
    }
    async Task SmokeAsync()
    {
        var log = new List<string>();
        void Check(bool value, string name) { if (!value) throw new Exception(name); log.Add("PASS " + name); }
        async Task Ready(BrowserTab tab)
        {
            await EnsureAsync(tab,true);
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
            while (tab.Loading || tab.View?.CoreWebView2 == null) await Task.Delay(50, timeout.Token);
        }
        try
        {
            var first = Active!; await Ready(first);
            int originalCount=models.Count;var originalTab=Active;
            ToggleSidebar();Check(!sidebar.Visible&&models.Count==originalCount&&Active==originalTab,"Sidebar hides without changing tabs");
            ToggleSidebar();Check(sidebar.Visible&&sidebarToggle.Selected,"Sidebar reopens with same button");
            Check(first.View!.CoreWebView2 != null, "WebView2 initialization");
            AddTab(); var second = Active!; await Ready(second);
            Check(models.Count == 2 && second.Url == Navigation.Home, "New tab opens local start page");
            first.BackgroundSince = Environment.TickCount64 - 6 * 60000L; Tick();
            Check(first.View == null && first.Page.Text.StartsWith("[Сон]"), "Timed discard destroys background WebView");
            Check(second.View != null, "Active tab preserved");
            tabs.SelectedTab = first.Page; await Ready(first);
            Check(first.View != null, "Discarded tab restored");
            await CleanAsync(); Check(second.View == null, "Manual cleanup discards background tab");
            var core = first.View!.CoreWebView2;
            var cookie = core!.CookieManager.CreateCookie("smoke", "value", "example.com", "/");
            core.CookieManager.AddOrUpdateCookie(cookie);
            Check((await core.CookieManager.GetCookiesAsync("https://example.com")).Any(c => c.Name == "smoke"), "Cookie created in isolated profile");
            await BrowserEngine.ClearAsync(core, false, true);
            Check((await core.CookieManager.GetCookiesAsync("https://example.com")).Any(c => c.Name == "smoke"), "Cache-only cleanup preserves cookies");
            await BrowserEngine.ClearAsync(core, true, false);
            Check(!(await core.CookieManager.GetCookiesAsync("https://example.com")).Any(c => c.Name == "smoke"), "Cookies-only cleanup removes cookies");
            var fixture = Path.Combine(AppSettings.DataDirectory, "fixture"); Directory.CreateDirectory(fixture);
            File.WriteAllText(Path.Combine(fixture, "index.html"), "<title>Login fixture</title><form><input autocomplete='username'><input type='password'></form>");
            core.SetVirtualHostNameToFolderMapping("slim.test", fixture, CoreWebView2HostResourceAccessKind.DenyCors);
            core.SetVirtualHostNameToFolderMapping("other.test", fixture, CoreWebView2HostResourceAccessKind.DenyCors);
            async Task NavigateFixture(string url)
            {
                var done = new TaskCompletionSource<bool>();
                ulong? navigationId = null;
                void Starting(object? sender, CoreWebView2NavigationStartingEventArgs e) { if (e.Uri == url) navigationId = e.NavigationId; }
                void Completed(object? sender, CoreWebView2NavigationCompletedEventArgs e) { if (e.NavigationId == navigationId) done.TrySetResult(e.IsSuccess); }
                core.NavigationStarting += Starting;
                core.NavigationCompleted += Completed;
                try { core.Navigate(url); Check(await done.Task.WaitAsync(TimeSpan.FromSeconds(20)), "Fixture loaded " + url); }
                finally { core.NavigationStarting -= Starting; core.NavigationCompleted -= Completed; }
            }
            settings.SavePasswords = false;
            vault.Save(new("https://slim.test", "fixture-user", "fixture-secret"));
            await NavigateFixture("https://slim.test/index.html");
            async Task<bool> EvaluateBool(string expression)
            {
                var json=await core.CallDevToolsProtocolMethodAsync("Runtime.evaluate",System.Text.Json.JsonSerializer.Serialize(new{expression,awaitPromise=true,returnByValue=true}));
                using var result=System.Text.Json.JsonDocument.Parse(json);
                return result.RootElement.GetProperty("result").GetProperty("value").GetBoolean();
            }
            Check(await EvaluateBool("(async()=>{const c=await caches.open('cleanup-fixture');await c.put('/cache-probe',new Response('keep'));return true})()"),"CacheStorage fixture created");
            await BrowserEngine.ClearAsync(core,true,false);
            Check(await EvaluateBool("(async()=>!!(await caches.match('/cache-probe')))()"),"Cookies-only cleanup preserves CacheStorage");
            await BrowserEngine.ClearAsync(core,false,true);
            Check(!await EvaluateBool("(async()=>!!(await caches.match('/cache-probe')))()"),"Cache-only cleanup removes CacheStorage");
            await vault.FillAsync(core);
            Check(await core.ExecuteScriptAsync("document.querySelector('input[type=password]').value") == "\"fixture-secret\"", "Password autofill on matching HTTPS origin");
            await NavigateFixture("https://other.test/index.html"); await vault.FillAsync(core);
            Check(await core.ExecuteScriptAsync("document.querySelector('input[type=password]').value") == "\"\"", "Password never filled on another origin");
            library.Add("bookmarks","Fixture","https://slim.test/index.html");
            Check(library.Read("bookmarks").Count==1,"Bookmarks persist in isolated profile");
            library.Add("bookmarks","Updated","https://slim.test/index.html");
            Check(library.Read("bookmarks").Count==1,"Duplicate bookmarks replaced");
            using var privateView=new WebView2 { Size=new Size(1,1),Visible=false,CreationProperties=new CoreWebView2CreationProperties{IsInPrivateModeEnabled=true,ProfileName="PrivateSmoke"} };
            Controls.Add(privateView);await privateView.EnsureCoreWebView2Async(await engine.Environment);
            Check(privateView.CoreWebView2.Profile.IsInPrivateModeEnabled,"Private window uses InPrivate profile");
            core.CookieManager.AddOrUpdateCookie(core.CookieManager.CreateCookie("isolation","normal","example.com","/"));
            Check(!(await privateView.CoreWebView2.CookieManager.GetCookiesAsync("https://example.com")).Any(c=>c.Name=="isolation"),"Private cookies isolated from normal profile");
            Check((await core.Profile.GetBrowserExtensionsAsync()) != null,"Extension management API available");
            settings.DarkMode=true;settings.Glass=false;settings.Save();
            Check(AppSettings.Load().DarkMode&&!AppSettings.Load().Glass,"Theme and material settings persist");
            settings.ClearCookiesOnExit = false; settings.ClearCacheOnExit = true; settings.Save();
            Check(!AppSettings.Load().ClearCookiesOnExit && AppSettings.Load().ClearCacheOnExit, "Independent exit cleanup settings persisted");
            Environment.ExitCode = 0;
        }
        catch (Exception e) { log.Add("FAIL " + e); Environment.ExitCode = 1; }
        finally { File.WriteAllLines(Path.Combine(AppContext.BaseDirectory, "smoke-test.txt"), log); Close(); }
    }
}
