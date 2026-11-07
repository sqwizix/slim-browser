use super::*;

pub struct Smoke {
    pub directory: PathBuf,
    pub phase: u8,
    pub started: Instant,
    pub checks: Vec<String>,
    pub private_view: Option<WebView>,
    pub download_speed_seen: bool,
}
impl Smoke {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            phase: 0,
            started: Instant::now(),
            checks: vec![],
            private_view: None,
            download_speed_seen: false,
        }
    }
}
pub fn map_fixture(view: &WebView, directory: &std::path::Path) -> Result<()> {
    let folder = directory.join("fixture");
    std::fs::create_dir_all(&folder)?;
    std::fs::write(
        folder.join("index.html"),
        "<!doctype html><html><head><title>Rust fixture</title><link rel='icon' href='/favicon.png'></head><body><form><input autocomplete='username'><input type='password'></form></body></html>",
    )?;
    std::fs::write(
        folder.join("slim-youtube-test.html"),
        r#"<!doctype html><script>
var ytInitialPlayerResponse = {
  adPlacements:[{ad:true}],adSlots:[{ad:true}],playerAds:[{ad:true}],
  videoDetails:{videoId:'content',lengthSeconds:'360'},
  streamingData:{formats:[{url:'https://rr1.googlevideo.com/videoplayback?content=1'}]},
  captions:{playerCaptionsTracklistRenderer:{captionTracks:[{languageCode:'ru'}]}}
};
globalThis.__slimYoutubeFixture = {
  scrubbed: !('adPlacements' in ytInitialPlayerResponse) && !('adSlots' in ytInitialPlayerResponse) && !('playerAds' in ytInitialPlayerResponse),
  content: ytInitialPlayerResponse.videoDetails.videoId === 'content' && ytInitialPlayerResponse.videoDetails.lengthSeconds === '360',
  media: ytInitialPlayerResponse.streamingData.formats[0].url === 'https://rr1.googlevideo.com/videoplayback?content=1',
  captions: ytInitialPlayerResponse.captions.playerCaptionsTracklistRenderer.captionTracks[0].languageCode === 'ru'
};
</script><title>YouTube protection fixture</title><body><h1>Normal content survives</h1></body>"#,
    )?;
    use base64::Engine;
    std::fs::write(folder.join("favicon.png"), base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAG0lEQVR4nGMwb3nznxLMMGrAqAGjBvwfJgYAACSwph9prlB2AAAAAElFTkSuQmCC")?)?;
    unsafe {
        let core = view.webview().cast::<ICoreWebView2_3>()?;
        for host in ["slim.test", "other.test", "www.youtube.com"] {
            core.SetVirtualHostNameToFolderMapping(
                &HSTRING::from(host),
                &HSTRING::from(folder.as_path()),
                COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
            )?;
        }
    }
    Ok(())
}
impl App {
    pub(crate) fn check(&mut self, value: bool, name: &str) -> Result<()> {
        if !value {
            bail!("Smoke-test: {name}")
        };
        if let Some(s) = &mut self.smoke {
            s.checks.push(format!("PASS {name}"));
        }
        Ok(())
    }
    pub fn begin_smoke(&mut self) -> Result<()> {
        self.smoke.as_mut().unwrap().phase = 50;
        self.check(
            self.windows.len() == 1,
            "Rust window and trusted UI IPC initialized",
        )?;
        self.check(
            self.windows[0].tabs.len() == 1 && self.windows[0].tabs[0].view.is_none(),
            "Home tab creates no site WebView",
        )?;
        let id = self.windows[0].active;
        self.store.borrow_mut().settings.sidebar_visible = false;
        self.layout(0)?;
        self.check(
            self.windows[0].active == id,
            "Hiding sidebar preserves active tab",
        )?;
        self.store.borrow_mut().settings.sidebar_visible = true;
        self.layout(0)?;
        self.toggle_menu(0)?;
        Ok(())
    }
    pub fn smoke_menu_ready(&self, wi: usize) -> Result<()> {
        let p = self.proxy.clone();
        self.windows[wi].popup.as_ref().context("Popup disappeared")?.evaluate_script_with_callback("(()=>{const menu=document.getElementById('popup-menu');const buttons=[...menu.querySelectorAll('button')];return getComputedStyle(menu).display==='block'&&buttons.length===12&&buttons.at(-1).getBoundingClientRect().bottom<=innerHeight})()",move|value|{let _=p.send_event(Event::SmokeResult("Menu visible and settings item fits popup".into(),value=="true"));})?;
        Ok(())
    }
    pub fn smoke_page(&mut self, wi: usize, ti: usize) -> Result<()> {
        let Some(smoke) = &self.smoke else {
            return Ok(());
        };
        let phase = smoke.phase;
        let url = self.windows[wi].tabs[ti].url.as_str();
        match phase {
            1 if url.starts_with("https://slim.test/") => {
                let mut enabled = BOOL::default();
                unsafe {
                    self.windows[wi].tabs[ti]
                        .view
                        .as_ref()
                        .unwrap()
                        .webview()
                        .Settings()?
                        .IsWebMessageEnabled(&mut enabled)?;
                }
                self.check(!enabled.as_bool(), "Remote page has no host IPC bridge")?;
                self.smoke.as_mut().unwrap().phase = 11;
                self.smoke_eval(
                    wi,
                    ti,
                    "Autofill on matching HTTPS origin",
                    "document.querySelector('input[type=password]').value==='fixture-secret'",
                )?;
            }
            2 if url.starts_with("https://other.test/") => {
                self.smoke.as_mut().unwrap().phase = 12;
                self.smoke_eval(
                    wi,
                    ti,
                    "No autofill on another origin",
                    "document.querySelector('input[type=password]').value===''",
                )?;
            }
            3 if url.starts_with("https://other.test/") => {
                self.smoke.as_mut().unwrap().phase = 13;
                self.check(
                    self.windows[wi].tabs[ti].view.is_some(),
                    "Discarded tab restores a real WebView",
                )?;
                let core = self.windows[wi].tabs[ti].view.as_ref().unwrap().webview();
                unsafe {
                    let manager = core.cast::<ICoreWebView2_2>()?.CookieManager()?;
                    let cookie = manager.CreateCookie(
                        &HSTRING::from("rust_smoke"),
                        &HSTRING::from("value"),
                        &HSTRING::from("other.test"),
                        &HSTRING::from("/"),
                    )?;
                    manager.AddOrUpdateCookie(&cookie)?;
                }
                self.smoke_eval(
                    wi,
                    ti,
                    "Cookie created in isolated profile",
                    "document.cookie.includes('rust_smoke=value')",
                )?;
            }
            90 if url.contains("privacy=1") => {
                let empty = self.store.borrow().history.is_empty();
                self.check(empty, "Never remember history suppresses real page visits")?;
                // Isolate our filter from Edge's earlier tracking-prevention layer.
                let core = self.windows[wi].tabs[ti].view.as_ref().unwrap().webview();
                unsafe {
                    engine::profile(&core)?
                        .cast::<ICoreWebView2Profile3>()?
                        .SetPreferredTrackingPreventionLevel(
                            COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
                        )?;
                }
                self.smoke_eval(wi,ti,"Site data fixture contains cookies and DOM storage","(()=>{document.cookie='privacy_fixture=yes;path=/;Secure';localStorage.setItem('privacy_fixture','yes');const ad=document.createElement('script');ad.src='https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js';document.head.append(ad);let guarded=false;try{document.createElement('canvas').toDataURL()}catch(e){guarded=e.name==='NotAllowedError'}return {ok:guarded&&document.cookie.includes('privacy_fixture')&&localStorage.getItem('privacy_fixture')==='yes',guarded,cookies:document.cookie,storage:localStorage.getItem('privacy_fixture'),canvas:String(HTMLCanvasElement.prototype.toDataURL)}})()")?;
            }
            91 if url.contains("child=1") => {
                let folder = self
                    .store
                    .borrow()
                    .folders
                    .iter()
                    .find(|f| f.name == "Popup inheritance")
                    .unwrap()
                    .id;
                self.check(
                    self.windows[wi].tabs[ti].folder_id == Some(folder),
                    "Popup follows its actual opener folder",
                )?;
                let _ = self.proxy.send_event(Event::SmokeResult(
                    "Popup follows its actual opener folder".into(),
                    true,
                ));
            }
            98 if url.starts_with("chrome-extension://") && url.ends_with("workshop.html") => {
                self.check(
                    self.windows[wi].tabs[ti].view.is_some(),
                    "Workshop has a native browser view",
                )?;
                let _ = self.proxy.send_event(Event::SmokeResult(
                    "Extension workshop opens as a managed browser tab".into(),
                    true,
                ));
            }
            99 if url.contains("external=1") => {
                let _ = self.proxy.send_event(Event::SmokeResult(
                    "External URL reaches an existing browser window".into(),
                    true,
                ));
            }
            100 if url.starts_with("file:") => {
                let _ = self.proxy.send_event(Event::SmokeResult(
                    "HTML file association opens a real local document".into(),
                    true,
                ));
            }
            101 if url.starts_with("https://www.youtube.com/slim-youtube-test.html") => {
                self.smoke_eval(
                    wi,
                    ti,
                    "Native YouTube guard scrubs ads before inline page scripts and preserves content",
                    "(()=>{const fixture=globalThis.__slimYoutubeFixture;return {ok:!!fixture&&fixture.scrubbed&&fixture.content&&fixture.media&&fixture.captions&&!!globalThis.__slimYouTubeProtection,fixture,guard:!!globalThis.__slimYouTubeProtection}})()",
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    fn smoke_eval(&self, wi: usize, ti: usize, name: &str, script: &str) -> Result<()> {
        let p = self.proxy.clone();
        let name = name.to_owned();
        self.windows[wi].tabs[ti]
            .view
            .as_ref()
            .context("Нет WebView в тесте")?
            .evaluate_script_with_callback(script, move |value| {
                let parsed = serde_json::from_str::<Value>(&value).unwrap_or_default();
                let artifact = match name.as_str() {
                    "Site data fixture contains cookies and DOM storage" => Some("privacy-fixture.json"),
                    "Native YouTube guard scrubs ads before inline page scripts and preserves content" => Some("youtube-fixture.json"),
                    _ => None,
                };
                if let Some(artifact) = artifact {
                    let _ = std::fs::write(
                        std::env::current_exe()
                            .unwrap_or_default()
                            .with_file_name(artifact),
                        &value,
                    );
                }
                let ok = value == "true" || parsed["ok"] == true;
                let _ = p.send_event(Event::SmokeResult(name.clone(), ok));
            })?;
        Ok(())
    }
    pub fn smoke_result(
        &mut self,
        name: &str,
        value: bool,
        target: &EventLoopWindowTarget<Event>,
    ) -> Result<()> {
        self.check(value, name)?;
        match name {
            "Menu visible and settings item fits popup" => {
                self.check(
                    self.windows[0].popup.is_some(),
                    "Menu survives initial focus handoff",
                )?;
                self.set_view(0, Some("appearance".into()))?;
                let p = self.proxy.clone();
                self.windows[0].shell.evaluate_script_with_callback("(()=>{const nav=document.querySelector('.settings-nav');return !!document.querySelector('#glass-transparency')&&!!document.querySelector('[data-setting=Glass]')&&!document.querySelector('[data-theme]')&&document.querySelectorAll('[data-card-setting=SiteColorScheme]').length===3&&Array.from(nav.querySelectorAll('button')).some(b=>b.textContent==='Поисковая система')})()",move|value|{let _=p.send_event(Event::SmokeResult("Settings render appearance and search choices".into(),value=="true"));})?;
            }
            "Settings render appearance and search choices" => {
                self.set_view(0, Some("languages".into()))?;
                self.change_setting("Language", &json!("en-US"), true)?;
                self.smoke_shell_eval("English interface applies immediately with two supported languages", "document.documentElement.lang==='en'&&document.querySelector('h1').textContent==='Settings'&&document.querySelector('[data-setting=Language]').options.length===2&&document.querySelector('[data-setting=Language]').value==='en-US'&&document.getElementById('home-input').placeholder==='Search Google or enter an address'")?;
            }
            "English interface applies immediately with two supported languages" => {
                self.set_view(0, Some("permissions".into()))?;
                self.smoke_shell_eval("English permission categories contain no Russian shell copy", "document.querySelector('[data-permission-kind=geolocation]').textContent.includes('Location')&&!/[А-Яа-яЁё]/.test(document.getElementById('view').textContent)")?;
            }
            "English permission categories contain no Russian shell copy" => {
                self.change_setting("Language", &json!("ru-RU"), true)?;
                self.set_view(0, Some("appearance".into()))?;
                self.smoke_shell_eval("Zoom lives in appearance without redundant browsing section", "!!document.querySelector('[data-setting=Zoom]')&&![...document.querySelectorAll('.settings-nav button')].some(b=>b.textContent==='Вкладки и просмотр')")?;
            }
            "Zoom lives in appearance without redundant browsing section" => {
                self.set_view(0, Some("privacy".into()))?;
                self.smoke_shell_eval("Privacy combines protection data history and resources", "document.querySelectorAll('[data-privacy=TrackingMode]').length===3&&document.querySelectorAll('[data-privacy=HistoryMode]').length===3&&!!document.querySelector('[data-setting=DiscardMinutes]')&&[...document.querySelectorAll('.settings-nav button')].some(b=>b.textContent==='Приватность и ресурсы')&&![...document.querySelectorAll('.settings-nav button')].some(b=>b.textContent==='Управление вкладками')")?;
            }
            "Privacy combines protection data history and resources" => {
                self.set_view(0, Some("permissions".into()))?;
                self.smoke_shell_eval("Permissions offer per-site searchable editor", "(()=>{permissionDialog('geolocation','Местоположение',true);return document.querySelectorAll('[data-permission-kind]').length>=9&&!!document.querySelector('dialog[open] input[type=search]')&&document.querySelector('dialog[open]').textContent.includes('Сохранить изменения')})()")?;
            }
            "Permissions offer per-site searchable editor" => {
                self.windows[0]
                    .shell
                    .evaluate_script("document.querySelector('dialog[open]').close()")?;
                self.set_view(0, Some("network".into()))?;
                self.smoke_shell_eval("Network settings expose IPv4 IPv6 and per-server DoH", "document.querySelectorAll('.dns-server').length===4&&document.querySelectorAll('.dns-server select').length===4&&document.querySelector('.dns-form').textContent.includes('IPv6')")?;
            }
            "Network settings expose IPv4 IPv6 and per-server DoH" => {
                self.set_view(0, Some("downloads".into()))?;
                self.change_setting("DownloadUnits", &json!("bits"), true)?;
                self.smoke_shell_eval("Bit units convert size and speed by eight", "bytes(1000000)==='8 Мбит'&&document.querySelector('[data-setting=DownloadUnits]').value==='bits'")?;
            }
            "Bit units convert size and speed by eight" => {
                self.change_setting("DownloadUnits", &json!("bytes"), true)?;
                self.set_view(0, Some("search".into()))?;
                self.smoke_shell_eval(
                    "Search settings offer all engines",
                    "document.querySelector('[data-setting=SearchEngine]').options.length===3",
                )?;
            }
            "Search settings offer all engines" => {
                self.change_setting("SearchEngine", &json!("bing"), true)?;
                self.smoke_shell_eval("Search preference updates both input prompts", "document.getElementById('address').placeholder.includes('Bing')&&document.getElementById('home-input').placeholder.includes('Bing')&&document.querySelector('[data-setting=SearchEngine]').value==='bing'")?;
            }
            "Search preference updates both input prompts" => {
                self.change_setting("SearchEngine", &json!("google"), true)?;
                self.smoke_shell_eval("Quick extensions hide built-ins while full settings retain them", "(()=>{const saved=state;const extensions=[{id:'fixture-clipboard',name:'Microsoft Clipboard Extension',enabled:true},{id:'fixture-pdf',name:'Microsoft Edge PDF Viewer',enabled:true},{id:'fixture-user',name:'My extension',enabled:true}];state={...saved,extensions};renderPageActions();const names=[...document.querySelectorAll('.popup-extension .extension-name')].map(n=>n.textContent);window.renderState({...saved,view:'extensions',extensions});const full=document.getElementById('extensions-list').textContent;const ok=names.length===1&&names[0]==='My extension'&&full.includes('Microsoft Clipboard Extension')&&full.includes('Microsoft Edge PDF Viewer');window.renderState(saved);return ok})()")?;
            }
            "Quick extensions hide built-ins while full settings retain them" => {
                self.set_view(0, Some("extensions".into()))?;
                self.smoke_shell_eval("Extension management uses switches and themed removal", "(()=>{const saved=state;window.renderState({...saved,extensions:[{id:'test',name:'Test extension',enabled:true,icon:''}]});const switches=document.querySelectorAll('#extensions-list input[role=switch]').length===1;confirmExtensionRemove({id:'test',name:'Test extension'});const themed=!!document.querySelector('dialog.settings-dialog[open]');document.querySelector('dialog[open]').close();window.renderState(saved);return switches&&themed})()")?;
            }
            "Extension management uses switches and themed removal" => {
                self.set_view(0, Some("privacy".into()))?;
                let mut policy = self.store.borrow().settings.privacy.clone();
                policy.tracking_mode = "custom".into();
                self.change_setting("Privacy", &serde_json::to_value(policy)?, true)?;
                self.smoke_shell_eval("Custom tracking exposes functional independent categories","['BasicSiteFixes','MinorSiteFixes','Cookies','TrackingContent','Cryptominers','KnownFingerprints','SuspectedFingerprints','AdblockEnabled'].every(k=>!!document.querySelector('[data-privacy='+k+']'))&&document.querySelectorAll('[data-privacy-value]').length===3")?;
            }
            "Custom tracking exposes functional independent categories" => {
                let mut policy = self.store.borrow().settings.privacy.clone();
                policy.tracking_mode = "standard".into();
                self.change_setting("Privacy", &serde_json::to_value(policy)?, true)?;
                self.folder_command(0, "folder_create", &json!({"name":"Работа 🗂"}))?;
                self.folder_command(0, "folder_move", &json!({"folder":1}))?;
                self.folder_command(0, "folder_rename", &json!({"id":1,"name":"Мои вкладки"}))?;
                let loaded = Store::open(self.store.borrow().directory.clone(), false)?;
                self.check(
                    loaded.folders.len() == 1
                        && loaded.folders[0].name == "Мои вкладки"
                        && loaded.folders[0].pages.len() == 1,
                    "Named tab folders persist with their pages",
                )?;
                self.smoke_shell_eval("Folder appears above loose tabs", "document.querySelector('#folders .folder-name').textContent==='Мои вкладки'&&document.querySelectorAll('#folders .tab').length===1&&!!(document.getElementById('folders').compareDocumentPosition(document.getElementById('tabs'))&Node.DOCUMENT_POSITION_FOLLOWING)")?;
            }
            "Folder appears above loose tabs" => {
                self.set_view(0, None)?;
                self.smoke.as_mut().unwrap().phase = 1;
                self.store.borrow().save_credential(Credential {
                    origin: "https://slim.test".into(),
                    username: "fixture-user".into(),
                    password: "fixture-secret".into(),
                })?;
                self.navigate(0, "https://slim.test/index.html")?;
            }
            "Autofill on matching HTTPS origin" => {
                self.smoke.as_mut().unwrap().phase = 2;
                self.navigate(0, "https://other.test/index.html")?;
            }
            "No autofill on another origin" => {
                let id = self.windows[0].active.unwrap();
                self.add_tab(0, HOME.into())?;
                self.windows[0].tabs[0].background_since =
                    Instant::now() - Duration::from_secs(6 * 60);
                self.tick()?;
                self.check(
                    self.windows[0].tabs[0].view.is_none(),
                    "Timer destroys inactive WebView",
                )?;
                self.check(
                    self.windows[0].tabs.len() == 2,
                    "Discard keeps both tab models",
                )?;
                self.smoke.as_mut().unwrap().phase = 3;
                self.select_tab(0, id)?;
            }
            "Cookie created in isolated profile" => {
                self.smoke.as_mut().unwrap().phase = 4;
                self.start_cleanup(0, false, true, CleanupPurpose::Smoke)?;
            }
            "Cache-only cleanup preserves cookies" => {
                self.smoke.as_mut().unwrap().phase = 5;
                self.start_cleanup(0, true, false, CleanupPurpose::Smoke)?;
            }
            "Cookies-only cleanup removes cookies" => {
                let core = self.service(0)?;
                unsafe {
                    let manager = core.cast::<ICoreWebView2_2>()?.CookieManager()?;
                    let cookie = manager.CreateCookie(
                        &HSTRING::from("isolation"),
                        &HSTRING::from("normal"),
                        &HSTRING::from("other.test"),
                        &HSTRING::from("/"),
                    )?;
                    manager.AddOrUpdateCookie(&cookie)?;
                }
                let private = WebViewBuilder::new()
                    .with_environment(self.environment.clone())
                    .with_profile_name("PrivateSmoke")
                    .with_incognito(true)
                    .with_visible(false)
                    .with_url("about:blank")
                    .with_bounds(rect(0., 0., 1., 1.))
                    .build_as_child(&self.windows[0].window)?;
                let profile = engine::profile(&private.webview())?;
                let mut flag = BOOL::default();
                unsafe {
                    profile.IsInPrivateModeEnabled(&mut flag)?;
                }
                self.check(flag.as_bool(), "Private controller uses InPrivate profile")?;
                let p = self.proxy.clone();
                unsafe {
                    private
                        .webview()
                        .cast::<ICoreWebView2_2>()?
                        .CookieManager()?
                        .GetCookies(
                            &HSTRING::from("https://other.test"),
                            &GetCookiesCompletedHandler::create(Box::new(move |result, list| {
                                result?;
                                let mut isolated = true;
                                if let Some(list) = list {
                                    let mut count = 0;
                                    list.Count(&mut count)?;
                                    for i in 0..count {
                                        let cookie = list.GetValueAtIndex(i)?;
                                        if engine::string(|v| cookie.Name(v))? == "isolation" {
                                            isolated = false;
                                        }
                                    }
                                }
                                let _ = p.send_event(Event::SmokeResult(
                                    "Private cookies isolated from normal profile".into(),
                                    isolated,
                                ));
                                Ok(())
                            })),
                        )?;
                }
                self.smoke.as_mut().unwrap().private_view = Some(private);
            }
            "Private cookies isolated from normal profile" => {
                let core = self.service(0)?;
                let p = self.proxy.clone();
                unsafe {
                    engine::profile(&core)?
                        .cast::<ICoreWebView2Profile7>()?
                        .GetBrowserExtensions(
                            &ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                                move |result, list| {
                                    let _ = p.send_event(Event::SmokeResult(
                                        "Extension management API available".into(),
                                        result.is_ok() && list.is_some(),
                                    ));
                                    Ok(())
                                },
                            )),
                        )?;
                }
            }
            "Extension management API available" => {
                self.store.borrow_mut().add_entry(
                    true,
                    "Fixture",
                    "https://slim.test/index.html",
                )?;
                self.store.borrow_mut().add_entry(
                    true,
                    "Updated",
                    "https://slim.test/index.html",
                )?;
                let count = self.store.borrow().bookmarks.len();
                self.check(count == 1, "Bookmarks persist without duplicates")?;
                self.store.borrow_mut().settings.clear_cookies_on_exit = false;
                self.store.borrow_mut().settings.clear_cache_on_exit = true;
                self.store.borrow_mut().settings.glass_transparency = 31;
                self.store.borrow().save_settings()?;
                let loaded = Store::open(self.store.borrow().directory.clone(), false)?;
                self.check(
                    !loaded.settings.clear_cookies_on_exit
                        && loaded.settings.clear_cache_on_exit
                        && loaded.settings.glass_transparency == 31,
                    "Independent cleanup and transparency settings persist",
                )?;
                self.change_setting("SiteColorScheme", &json!("dark"), true)?;
                let ti = self.active_index(0).unwrap();
                self.smoke_eval(
                    0,
                    ti,
                    "Dark scheme applies to sites and browser",
                    "matchMedia('(prefers-color-scheme: dark)').matches",
                )?;
            }
            "Dark scheme applies to sites and browser" => {
                let shell_is_light = self.store.borrow().settings.dark_mode;
                self.check(
                    shell_is_light,
                    "Browser theme follows the selected dark scheme",
                )?;
                self.change_setting("SiteColorScheme", &json!("light"), true)?;
                let ti = self.active_index(0).unwrap();
                self.smoke_eval(
                    0,
                    ti,
                    "Site light scheme reaches the live page",
                    "matchMedia('(prefers-color-scheme: light)').matches",
                )?;
            }
            "Site light scheme reaches the live page" => {
                self.change_setting("SiteColorScheme", &json!("system"), true)?;
                let core = self.service(0)?;
                let mut scheme = COREWEBVIEW2_PREFERRED_COLOR_SCHEME::default();
                unsafe {
                    engine::profile(&core)?.PreferredColorScheme(&mut scheme)?;
                }
                self.check(
                    scheme == COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO,
                    "System website scheme follows Windows",
                )?;
                self.smoke_layout("compact")?;
            }
            "Native page and shell match compact layout" => {
                self.smoke_shell_eval("Compact sidebar has no horizontal overflow and stacked footer", "(()=>{const s=document.getElementById('sidebar-scroll'),a=document.getElementById('sidebar-downloads').getBoundingClientRect(),b=document.getElementById('sidebar-add').getBoundingClientRect();return s.scrollWidth===s.clientWidth&&Math.abs(a.left-b.left)<1&&a.bottom<=b.top&&[...document.querySelectorAll('.tab-close')].every(c=>{const r=c.getBoundingClientRect(),t=c.parentElement.getBoundingClientRect();return r.left>=t.left&&r.right<=t.right&&r.top>=t.top&&r.bottom<=t.bottom})})()")?;
            }
            "Compact sidebar has no horizontal overflow and stacked footer" => {
                self.smoke_layout("unified")?
            }
            "Native page and shell match unified layout" => {
                self.toggle_menu(0)?;
                self.smoke.as_mut().unwrap().phase = 51;
            }
            "Unified menu opens beside the sidebar and fits" => self.smoke_layout("split")?,
            "Native page and shell match split layout" => {
                self.change_setting("SidebarVisible", &json!(false), true)?;
                self.smoke_layout("unified")?;
            }
            "Native page and shell match unified layout with sidebar hidden" => {
                self.smoke_shell_eval("Unified sidebar hides its address bar without creating a top toolbar", "getComputedStyle(document.getElementById('toolbar')).display==='none'&&getComputedStyle(document.getElementById('sidebar-reveal')).display!=='none'")?;
            }
            "Unified sidebar hides its address bar without creating a top toolbar" => {
                self.change_setting("SidebarVisible", &json!(true), true)?;
                self.change_setting("BrowserLayout", &json!("split"), true)?;
                self.smoke.as_mut().unwrap().phase = 60;
                let path = self.smoke.as_ref().unwrap().directory.join("page.png");
                let ti = self.active_index(0).unwrap();
                let core = self.windows[0].tabs[ti].view.as_ref().unwrap().webview();
                engine::capture_png(&core, path, self.proxy.clone(), self.windows[0].window.id())?;
            }
            "Downloads panel shows real completed transfer" => {
                let popup = self.windows[0]
                    .popup
                    .as_ref()
                    .context("No downloads popup")?;
                let bounds = popup.bounds()?;
                let scale = self.windows[0].window.scale_factor();
                let size = bounds.size.to_logical::<f64>(scale);
                let position = bounds.position.to_logical::<f64>(scale);
                let window_height = self.windows[0]
                    .window
                    .inner_size()
                    .to_logical::<f64>(scale)
                    .height;
                self.check(
                    size.width <= 350.
                        && size.height <= 348.
                        && position.y + size.height <= window_height - 50.,
                    "Downloads popup is compact and above the footer",
                )?;
                self.smoke_popup_eval("Downloads controls fit without system list", "(()=>{const buttons=[...document.querySelectorAll('button')].filter(b=>b.getClientRects().length);return !document.body.textContent.includes('Системный список загрузок')&&buttons.every(b=>b.getBoundingClientRect().bottom<=innerHeight)})()")?;
            }
            "Downloads controls fit without system list" => {
                self.windows[0]
                    .popup
                    .as_ref()
                    .unwrap()
                    .set_bounds(self.popup_bounds(0, 350., 348.))?;
                self.smoke_popup_eval("Three downloads fit with icon actions on the title row", "(()=>{const saved=state;document.getElementById('transfer-list').replaceChildren();window.renderState({...state,transfers:[1,2,3].map((id,i)=>({id:id+100,name:'example-file-'+i+'.zip',received:100000,total:1000000,speed:50000,state:i===1?'interrupted':'progress',can_resume:i===1}))});const list=document.getElementById('transfer-list');const rows=[...list.querySelectorAll('.transfer')].filter(r=>r.dataset.download>=100);const ok=rows.length===3&&rows.every(r=>r.getBoundingClientRect().bottom<=list.getBoundingClientRect().bottom)&&rows.every(r=>[...r.querySelectorAll('.transfer-actions button')].every(b=>b.querySelector('svg')&&!b.textContent.trim()));document.getElementById('transfer-list').replaceChildren();window.renderState(saved);return ok})()")?;
            }
            "Three downloads fit with icon actions on the title row" => {
                self.check(
                    self.windows[0]
                        .tabs
                        .iter()
                        .any(|t| t.favicon.starts_with("data:image/png;base64,")),
                    "Real WebView2 favicon reaches tab state",
                )?;
                self.command(0, json!({"cmd":"close_menu"}), target)?;
                self.smoke.as_mut().unwrap().phase = 80;
                let tab = self.windows[0].active.unwrap();
                self.command(0, json!({"cmd":"tab_menu","id":tab,"x":30,"y":220}), target)?;
            }
            "Tab context menu has all actions and fits" => {
                self.command(0, json!({"cmd":"close_menu"}), target)?;
                self.smoke.as_mut().unwrap().phase = 81;
                self.command(
                    0,
                    json!({"cmd":"folder_menu","id":1,"x":24,"y":160}),
                    target,
                )?;
            }
            "Folder menu offers reorder rename and delete" => {
                self.command(0, json!({"cmd":"close_menu"}), target)?;
                self.command(0, json!({"cmd":"folder_edit_inline","id":1}), target)?;
                self.smoke_shell_eval("Folder rename uses focused inline input", "!!document.querySelector('.folder-header .folder-inline-name')&&document.activeElement===document.querySelector('.folder-inline-name')")?;
            }
            "Folder rename uses focused inline input" => {
                self.command(
                    0,
                    json!({"cmd":"folder_rename","id":1,"name":"Переименовано"}),
                    target,
                )?;
                let count = self.windows[0].tabs.len();
                self.command(0, json!({"cmd":"folder_delete","id":1}), target)?;
                self.check(
                    self.store.borrow().folders.is_empty()
                        && self.windows[0].tabs.len() == count
                        && self.windows[0].tabs.iter().all(|t| t.folder_id.is_none()),
                    "Deleting folder preserves open tabs",
                )?;
                self.store
                    .borrow_mut()
                    .add_entry(true, "Keep", "https://keep.test/")?;
                self.command(
                    0,
                    json!({"cmd":"delete_bookmark","url":"https://slim.test/index.html"}),
                    target,
                )?;
                self.check(
                    self.store.borrow().bookmarks.len() == 1
                        && self.store.borrow().bookmarks[0].url == "https://keep.test/",
                    "Deleting one bookmark preserves other entries",
                )?;
                self.set_view(0, Some("bookmarks".into()))?;
                self.smoke_shell_eval("Bookmark confirmation uses themed dialog and cancels safely", "(()=>{confirmClearLibrary('bookmarks');const d=document.querySelector('dialog[open]');const ok=!!d&&d.classList.contains('confirm-dialog')&&d.contains(document.activeElement)&&!!document.querySelector('[aria-label^=\"Удалить закладку\"]');d.close();return ok})()")?;
            }
            "Bookmark confirmation uses themed dialog and cancels safely" => {
                self.command(
                    0,
                    json!({"cmd":"clear_library","value":"bookmarks"}),
                    target,
                )?;
                let untouched = self.store.borrow().bookmarks.len() == 1;
                self.check(untouched, "Unconfirmed clear leaves bookmarks untouched")?;
                self.command(
                    0,
                    json!({"cmd":"clear_library","value":"bookmarks","confirmed":true}),
                    target,
                )?;
                let cleared = self.store.borrow().bookmarks.is_empty();
                self.check(cleared, "Confirmed clear persists empty bookmarks")?;
                self.set_view(0, None)?;
                self.add_tab(0, HOME.into())?;
                let tab = self.windows[0].active.unwrap();
                self.tab_action(0, &json!({"id":tab,"action":"pin"}), target)?;
                self.check(
                    self.windows[0].tabs[0].id == tab && self.windows[0].tabs[0].pinned,
                    "Pinning moves tab to start",
                )?;
                self.tab_action(0, &json!({"id":tab,"action":"duplicate"}), target)?;
                let duplicate = self.windows[0].active.unwrap();
                self.check(
                    duplicate != tab
                        && self.windows[0]
                            .tabs
                            .iter()
                            .find(|t| t.id == duplicate)
                            .unwrap()
                            .url
                            == HOME,
                    "Duplicate creates independent tab",
                )?;
                self.tab_action(0, &json!({"id":tab,"action":"close_right"}), target)?;
                self.check(
                    self.windows[0].tabs.len() == 1 && self.windows[0].tabs[0].id == tab,
                    "Close tabs to right preserves target",
                )?;
                self.tab_action(0, &json!({"id":tab,"action":"window"}), target)?;
                self.check(
                    self.windows.len() == 2
                        && self.windows[1].tabs.len() == 1
                        && self.windows[1].tabs[0].url == HOME,
                    "Open tab in new window preserves URL",
                )?;
                self.request_close(1)?;
                self.smoke.as_mut().unwrap().phase = 82;
                self.command(0, json!({"cmd":"organize"}), target)?;
            }
            "Organize panel only creates tabs and folders" => {
                self.command(0, json!({"cmd":"close_menu"}), target)?;
                let folder = self.smoke.as_ref().unwrap().directory.join("store-fixture");
                std::fs::create_dir_all(&folder)?;
                std::fs::write(
                    folder.join("manifest.json"),
                    r#"{"manifest_version":3,"name":"Store fixture","version":"1.0","description":"Isolated installation test","permissions":["storage","tabs"],"action":{"default_popup":"popup.html"}}"#,
                )?;
                std::fs::write(
                    folder.join("slim-package.json"),
                    r#"{"name":"Store fixture","popup":"popup.html"}"#,
                )?;
                std::fs::write(
                    folder.join("popup.html"),
                    "<!doctype html><html><head><style>body{margin:0;width:100vw;height:100vh;background:#202124}</style></head><body><button id='launch'>Workshop</button><script src='popup.js'></script></body></html>",
                )?;
                std::fs::write(
                    folder.join("popup.js"),
                    "document.getElementById('launch').onclick=()=>chrome.tabs.create({url:chrome.runtime.getURL('workshop.html')})",
                )?;
                std::fs::write(
                    folder.join("workshop.html"),
                    "<!doctype html><title>Managed workshop</title><h1>Workshop works</h1>",
                )?;
                self.windows[0].pending_install = Some(extension_store::PendingInstall {
                    id: "fixture".into(),
                    name: "Store fixture".into(),
                    version: "1.0".into(),
                    permissions: vec!["storage".into()],
                    folder,
                });
                self.set_view(0, Some("extension-store".into()))?;
                self.smoke_shell_eval("Extension store shows permissions before installation", "document.querySelector('dialog[open] .permission-list').textContent.includes('storage')&&document.querySelector('dialog[open]').textContent.includes('Установить Store fixture?')")?;
            }
            "Extension store shows permissions before installation" => {
                self.smoke.as_mut().unwrap().phase = 83;
                self.confirm_extension(0)?;
            }
            "Confirmed store package installs through WebView2" => {
                self.begin_privacy_smoke(target)?;
            }
            "Empty tab is represented by a single sidebar action" => {
                self.command(0, json!({"cmd":"view","value":""}), target)?;
                self.change_setting("SidebarWidth", &json!(342), true)?;
                let settings = Store::open(self.store.borrow().directory.clone(), false)?.settings;
                self.check(
                    settings.sidebar_width == 342,
                    "Sidebar drag width survives profile reload",
                )?;
                self.smoke_shell_eval("Sidebar resize updates real shell bounds", "document.getElementById('sidebar').getBoundingClientRect().width===342&&document.getElementById('workspace').getBoundingClientRect().left===342")?;
            }
            "Sidebar resize updates real shell bounds" => {
                self.change_setting("SidebarWidth", &json!(222), true)?;
                self.smoke.as_mut().unwrap().phase = 92;
                self.command(0, json!({"cmd":"maximize"}), target)?;
            }
            "Maximized window keeps Windows work area" => {
                self.check(
                    crate::platform::matches_monitor_area(&self.windows[0].window, false)?,
                    "Yellow maximize button leaves taskbar space",
                )?;
                self.smoke.as_mut().unwrap().phase = 93;
                self.command(0, json!({"cmd":"fullscreen"}), target)?;
            }
            "Fullscreen remains separate from maximization" => {
                self.check(
                    crate::platform::matches_monitor_area(&self.windows[0].window, true)?,
                    "F11 uses the full monitor",
                )?;
                self.command(0, json!({"cmd":"fullscreen"}), target)?;
                self.windows[0].window.set_maximized(false);
                self.command(0, json!({"cmd":"permission_save","kind":"geolocation","blocked":true,"rules":[{"origin":"https://slim.test","kind":"geolocation","state":"allow"}]}), target)?;
                let loaded = Store::open(self.store.borrow().directory.clone(), false)?;
                self.check(
                    loaded.settings.privacy.decision(
                        &loaded.settings.privacy.permissions,
                        "https://slim.test",
                        "geolocation",
                    ) == Some(true)
                        && loaded.settings.privacy.decision(
                            &loaded.settings.privacy.permissions,
                            "https://other.test",
                            "geolocation",
                        ) == Some(false),
                    "Per-site permissions and block-new setting persist",
                )?;
                let mut policy = self.store.borrow().settings.privacy.clone();
                policy.tracking_mode = "strict".into();
                policy.history_mode = "never".into();
                self.change_setting("Privacy", &serde_json::to_value(policy)?, true)?;
                let core = self.service(0)?;
                let mut level = COREWEBVIEW2_TRACKING_PREVENTION_LEVEL::default();
                unsafe {
                    engine::profile(&core)?
                        .cast::<ICoreWebView2Profile3>()?
                        .PreferredTrackingPreventionLevel(&mut level)?;
                }
                self.check(
                    level == COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
                    "Strict protection reaches the actual engine profile",
                )?;
                self.store.borrow_mut().history.clear();
                self.store.borrow().save_library(false)?;
                self.smoke.as_mut().unwrap().phase = 90;
                self.navigate(0, "https://slim.test/index.html?privacy=1")?;
            }
            "Site data fixture contains cookies and DOM storage" => {
                let core = self.service(0)?;
                privacy::clear_origin(
                    &core,
                    "https://slim.test",
                    self.proxy.clone(),
                    self.windows[0].window.id(),
                )?;
            }
            "CDP removes selected site data without clearing other sites" => {
                let ti = self.active_index(0).unwrap();
                self.smoke_eval(0,ti,"Selected origin cookies and localStorage are gone","!document.cookie.includes('privacy_fixture')&&localStorage.getItem('privacy_fixture')===null")?;
            }
            "Selected origin cookies and localStorage are gone" => {
                self.smoke.as_mut().unwrap().phase = 91;
                let mut policy = self.store.borrow().settings.privacy.clone();
                policy.popup_exceptions.push("https://slim.test".into());
                self.change_setting("Privacy", &serde_json::to_value(policy)?, true)?;
                self.folder_command(0, "folder_create", &json!({"name":"Popup inheritance"}))?;
                let folder = self.store.borrow().folders.last().unwrap().id;
                let ti = self.active_index(0).unwrap();
                self.windows[0].tabs[ti].folder_id = Some(folder);
                self.windows[0].tabs[ti]
                    .view
                    .as_ref()
                    .unwrap()
                    .evaluate_script(
                        "window.open('https://slim.test/index.html?child=1','_blank')",
                    )?;
            }
            "Popup follows its actual opener folder" => {
                self.smoke.as_mut().unwrap().phase = 96;
                self.load_extensions(0)?;
            }
            "Extension popup document loaded" => {
                self.smoke.as_mut().unwrap().phase = 98;
                let p = self.proxy.clone();
                self.windows[0].popup.as_ref().unwrap().evaluate_script_with_callback("(()=>{const r={width:innerWidth,body:document.body.getBoundingClientRect().width,ipc:!!globalThis.ipc,ready:!!document.getElementById('launch')};r.ok=r.width===400&&r.body===400&&r.ipc;return r})()",move|value|{
                    let _=std::fs::write(std::env::current_exe().unwrap_or_default().with_file_name("extension-popup-fixture.json"),&value);
                    let parsed=serde_json::from_str::<Value>(&value).unwrap_or_default();let _=p.send_event(Event::SmokeResult("Extension popup uses fixed viewport and opens its workshop".into(),parsed["ok"]==true));
                })?;
            }
            "Extension popup uses fixed viewport and opens its workshop" => {
                self.windows[0]
                    .popup
                    .as_ref()
                    .unwrap()
                    .evaluate_script("document.getElementById('launch').click()")?;
            }
            "Extension workshop opens as a managed browser tab" => {
                self.check(
                    self.protection.blocked.get() > 0,
                    "Built-in blocker intercepts real advertising requests",
                )?;
                self.smoke.as_mut().unwrap().phase = 99;
                let directory = self.smoke.as_ref().unwrap().directory.clone();
                os_integration::start(&directory, self.proxy.clone())?;
                os_integration::forward_at(
                    &directory,
                    &["https://slim.test/index.html?external=1".into()],
                )?;
            }
            "External URL reaches an existing browser window" => {
                self.smoke.as_mut().unwrap().phase = 100;
                let file = self.smoke.as_ref().unwrap().directory.join("external.html");
                std::fs::write(
                    &file,
                    "<!doctype html><title>Local HTML fixture</title><h1>Local page</h1>",
                )?;
                os_integration::forward_at(
                    &self.smoke.as_ref().unwrap().directory,
                    &[url::Url::from_file_path(file).unwrap().into()],
                )?;
            }
            "HTML file association opens a real local document" => {
                self.smoke.as_mut().unwrap().phase = 101;
                self.navigate(0, "https://www.youtube.com/slim-youtube-test.html")?;
            }
            "Native YouTube guard scrubs ads before inline page scripts and preserves content" => {
                self.request_close(0)?;
                self.check(
                    self.windows.len() == 1 && self.windows[0].busy,
                    "Window waits for selected cleanup before exiting",
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    fn begin_privacy_smoke(&mut self, target: &EventLoopWindowTarget<Event>) -> Result<()> {
        self.set_view(0, None)?;
        self.command(0, json!({"cmd":"new_tab"}), target)?;
        let count = self.windows[0].tabs.len();
        let active = self.windows[0].active;
        self.command(0, json!({"cmd":"new_tab"}), target)?;
        self.command(0, json!({"cmd":"new_tab"}), target)?;
        self.check(
            self.windows[0].tabs.len() == count && self.windows[0].active == active,
            "Repeated new-tab requests reuse the existing empty tab",
        )?;
        self.smoke_shell_eval("Empty tab is represented by a single sidebar action","document.querySelectorAll('#tabs .tab').length===state.tabs.filter(t=>!t.folder_id&&t.url!=='slim://newtab').length&&document.querySelectorAll('#new-tab').length===1&&document.getElementById('new-tab').classList.contains('active')")
    }
    fn smoke_popup_eval(&self, name: &str, script: &str) -> Result<()> {
        let proxy = self.proxy.clone();
        let name = name.to_owned();
        self.windows[0]
            .popup
            .as_ref()
            .context("Popup missing")?
            .evaluate_script_with_callback(script, move |value| {
                let _ = proxy.send_event(Event::SmokeResult(name.clone(), value == "true"));
            })?;
        Ok(())
    }
    pub fn smoke_refinement_menu_ready(&self, _wi: usize) -> Result<()> {
        match self.smoke.as_ref().unwrap().phase {
            80 => self.smoke_popup_eval("Tab context menu has all actions and fits", "(()=>{const b=[...document.querySelectorAll('#popup-menu button')];return b.length===11&&b.some(x=>x.textContent==='Переместить вкладку выше')&&b.some(x=>x.textContent==='Закрыть вкладки справа')&&b.some(x=>x.textContent==='Отключить звук на сайте')&&b.every(x=>x.getBoundingClientRect().bottom<=innerHeight)})()"),
            81 => self.smoke_popup_eval("Folder menu offers reorder rename and delete", "(()=>{const b=[...document.querySelectorAll('#popup-menu button')];return b.length===4&&b[0].textContent==='Переместить папку выше'&&b[2].textContent==='Изменить название папки'&&b[3].textContent==='Удалить папку'&&b.every(x=>x.getBoundingClientRect().bottom<=innerHeight)})()"),
            82 => self.smoke_popup_eval("Organize panel only creates tabs and folders", "document.querySelectorAll('#popup-menu button').length===2&&!document.querySelector('#popup-menu select')&&!document.body.textContent.includes('Переместить текущую вкладку')"),
            _ => Ok(()),
        }
    }
    fn smoke_shell_eval(&self, name: &str, script: &str) -> Result<()> {
        let proxy = self.proxy.clone();
        let name = name.to_owned();
        self.windows[0]
            .shell
            .evaluate_script_with_callback(script, move |value| {
                let _ = proxy.send_event(Event::SmokeResult(name.clone(), value == "true"));
            })?;
        Ok(())
    }
    fn smoke_layout(&mut self, layout: &str) -> Result<()> {
        let active = self.windows[0].active;
        self.set_view(0, None)?;
        self.change_setting("BrowserLayout", &json!(layout), true)?;
        let (x, y) = self.store.borrow().settings.page_origin();
        let ti = self.active_index(0).unwrap();
        let scale = self.windows[0].window.scale_factor();
        let position = self.windows[0].tabs[ti]
            .view
            .as_ref()
            .unwrap()
            .bounds()?
            .position
            .to_logical::<f64>(scale);
        anyhow::ensure!(
            (position.x - x).abs() < 1.
                && (position.y - y).abs() < 1.
                && self.windows[0].active == active,
            "Native layout bounds or active tab changed unexpectedly"
        );
        let suffix = if self.store.borrow().settings.sidebar_visible {
            ""
        } else {
            " with sidebar hidden"
        };
        self.smoke_shell_eval(&format!("Native page and shell match {layout} layout{suffix}"), &format!("(()=>{{const r=document.getElementById('workspace').getBoundingClientRect();return Math.abs(r.left-{x})<1&&Math.abs(r.top-{y})<1}})()"))
    }
    pub fn smoke_cleanup_done(&mut self, wi: usize) -> Result<()> {
        let phase = self.smoke.as_ref().context("Нет smoke-test")?.phase;
        let ti = self.active_index(wi).context("Нет активной вкладки")?;
        match phase {
            4 => self.smoke_eval(
                wi,
                ti,
                "Cache-only cleanup preserves cookies",
                "document.cookie.includes('rust_smoke=value')",
            )?,
            5 => self.smoke_eval(
                wi,
                ti,
                "Cookies-only cleanup removes cookies",
                "!document.cookie.includes('rust_smoke=value')",
            )?,
            _ => bail!("Неожиданный этап очистки"),
        };
        Ok(())
    }
    pub fn smoke_screenshot_done(&mut self, path: &std::path::Path) -> Result<()> {
        let bytes = std::fs::read(path)?;
        self.check(
            bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() > 24,
            "Page screenshot saves a real PNG",
        )?;
        let width = u32::from_be_bytes(bytes[16..20].try_into()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into()?);
        self.check(
            width > 100 && height > 100,
            "Page screenshot includes rendered viewport",
        )?;
        self.begin_download_smoke()?;
        Ok(())
    }
    pub fn smoke_wide_menu_ready(&self, wi: usize) -> Result<()> {
        let popup = self.windows[wi]
            .popup
            .as_ref()
            .context("Menu disappeared")?;
        let position = popup
            .bounds()?
            .position
            .to_logical::<f64>(self.windows[wi].window.scale_factor());
        anyhow::ensure!(position.x >= 280., "Unified menu still overlaps sidebar");
        let proxy = self.proxy.clone();
        popup.evaluate_script_with_callback("(()=>{const m=document.getElementById('popup-menu');return getComputedStyle(m).display==='grid'&&[...m.querySelectorAll('button')].every(b=>b.getBoundingClientRect().bottom<=innerHeight)})()",move|value|{let _=proxy.send_event(Event::SmokeResult("Unified menu opens beside the sidebar and fits".into(),value=="true"));})?;
        Ok(())
    }
    fn begin_download_smoke(&mut self) -> Result<()> {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            if stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=transfer-fixture.bin\r\nContent-Length: 2097152\r\nConnection: close\r\n\r\n").is_err() {return;}
            for _ in 0..32 {
                if stream.write_all(&[42; 65536]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let directory = self.smoke.as_ref().unwrap().directory.join("downloads");
        std::fs::create_dir_all(&directory)?;
        self.store.borrow_mut().settings.download_folder = directory;
        let ti = self.active_index(0).unwrap();
        let view = self.windows[0].tabs[ti].view.as_ref().unwrap();
        engine::configure(view, &self.store.borrow().settings, false)?;
        self.smoke.as_mut().unwrap().phase = 70;
        view.load_url(&format!("http://127.0.0.1:{port}/file"))?;
        Ok(())
    }
    pub fn smoke_download_tick(&mut self) -> Result<()> {
        if !self.smoke.as_ref().is_some_and(|s| s.phase == 70) {
            return Ok(());
        }
        let Some(transfer) = self.windows[0].transfers.last() else {
            return Ok(());
        };
        if transfer.info.speed > 0. {
            self.smoke.as_mut().unwrap().download_speed_seen = true;
        }
        if transfer.info.state != "completed" {
            return Ok(());
        }
        let valid = transfer.info.received == 2097152
            && transfer.info.total == Some(2097152)
            && std::fs::metadata(&transfer.path)?.len() == 2097152;
        self.check(
            valid,
            "Native download reports received bytes and file size",
        )?;
        self.check(
            self.smoke.as_ref().unwrap().download_speed_seen,
            "Download speed is measured during transfer",
        )?;
        self.smoke.as_mut().unwrap().phase = 71;
        let proxy = self.proxy.clone();
        self.windows[0].popup.as_ref().context("Downloads popup disappeared")?.evaluate_script_with_callback("document.querySelector('.transfer-name').textContent==='transfer-fixture.bin'&&document.querySelector('.transfer-status').textContent==='Загружено'&&document.querySelector('progress').value===100",move|value|{let _=proxy.send_event(Event::SmokeResult("Downloads panel shows real completed transfer".into(),value=="true"));})?;
        Ok(())
    }
    pub fn finish_smoke(&mut self, error: Option<String>) {
        if let Some(mut smoke) = self.smoke.take() {
            if let Some(error) = &error {
                smoke.checks.push(format!("FAIL {error}"));
            }
            smoke
                .checks
                .push(format!("Profile: {}", smoke.directory.display()));
            let path = std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("smoke-test.txt");
            let _ = std::fs::write(path, smoke.checks.join("\n"));
            smoke.private_view.take();
        }
        self.exit_code = if error.is_some() { 1 } else { 0 };
        self.windows.clear();
    }
}
