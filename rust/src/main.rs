#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod compat;
mod downloads;
mod engine;
mod extension_store;
mod i18n;
mod model;
mod motion;
mod network;
mod os_integration;
mod platform;
mod privacy;
mod protection;
mod smoke;

use anyhow::{Context, Result, bail};
use model::{Credential, HOME, SiteEntry, Store};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event as TaoEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    platform::windows::WindowExtWindows,
    window::{Fullscreen, ResizeDirection, Window, WindowBuilder, WindowId},
};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::System::WinRT::{RO_INIT_SINGLETHREADED, RoInitialize},
    core::{BOOL, HSTRING, Interface},
};
use wry::{
    MemoryUsageLevel, Rect, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows,
};

#[derive(Clone, Copy)]
enum CleanupPurpose {
    Exit,
    Manual,
    Smoke,
}
enum Event {
    ExternalOpen(Vec<String>),
    ExtensionOpen(WindowId, u64, String),
    Ipc(WindowId, bool, String),
    Key(WindowId, String),
    Title(WindowId, u64, String),
    Favicon(WindowId, u64, String),
    Page(WindowId, u64, bool, String),
    NavigationFailed(WindowId, u64, Option<String>),
    Refresh(WindowId, u64),
    NewTab(WindowId, u64, String),
    BrowsingCleared(WindowId, f64, bool, std::result::Result<(), String>),
    NativePermissions(WindowId, Vec<privacy::PermissionRule>),
    CosmeticNodes(WindowId, u64, String, String),
    StorageSize(WindowId, u64),
    PermissionDecision(WindowId, privacy::PermissionRule),
    SiteData(
        WindowId,
        std::result::Result<Vec<privacy::SiteData>, String>,
    ),
    SiteDataCleared(WindowId, std::result::Result<(), String>),
    Notice(WindowId, String),
    PopupLostFocus(WindowId),
    WarmPopup(WindowId),
    Download(WindowId, u64, bool, String),
    DownloadStarted(WindowId, u64, u64),
    CleanupDone(WindowId, CleanupPurpose, std::result::Result<(), String>),
    Extensions(WindowId, std::result::Result<Vec<ExtensionInfo>, String>),
    ExtensionChanged(WindowId, std::result::Result<(), String>),
    SmokeResult(String, bool),
    ScreenshotDone(WindowId, PathBuf, std::result::Result<(), String>),
    Catalog(Vec<extension_store::Package>),
    PackagePrepared(
        WindowId,
        std::result::Result<extension_store::PendingInstall, String>,
    ),
    StoreInstalled(WindowId, std::result::Result<String, String>),
}
struct Tab {
    id: u64,
    url: String,
    title: String,
    view: Option<WebView>,
    loading: bool,
    background_since: Instant,
    downloads: usize,
    active: Rc<Cell<bool>>,
    folder_id: Option<u64>,
    favicon: String,
    pinned: bool,
    muted: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowMode {
    Normal,
    Maximized,
    Fullscreen,
}
struct WindowMotion {
    spring: motion::Spring,
    mode: WindowMode,
    previous: Instant,
    started: Instant,
}
struct BrowserWindow {
    // WebViews must drop before the native parent HWND.
    shell: WebView,
    popup: Option<WebView>,
    popup_cache: Option<WebView>,
    popup_loaded: bool,
    popup_session: u64,
    service: Option<WebView>,
    tabs: Vec<Tab>,
    window: Window,
    active: Option<u64>,
    private: bool,
    profile_name: String,
    view: Option<String>,
    status: String,
    busy: bool,
    private_history: Vec<SiteEntry>,
    extensions: Vec<ExtensionInfo>,
    popup_kind: String,
    edge_to_edge: Cell<bool>,
    motion: Option<WindowMotion>,
    restore_bounds: Option<[f64; 4]>,
    windowed_mode: WindowMode,
    share_session: Option<platform::ShareSession>,
    private_folders: Vec<model::TabFolder>,
    folder_edit: Option<u64>,
    folder_rename: Option<u64>,
    menu_tab: Option<u64>,
    popup_anchor: Option<(f64, f64)>,
    extension_size: (f64, f64),
    transfers: Vec<downloads::Transfer>,
    pending_install: Option<extension_store::PendingInstall>,
    store_busy: bool,
    private_permissions: Rc<RefCell<Vec<privacy::PermissionRule>>>,
    site_data: Vec<privacy::SiteData>,
    storage_bytes: Option<u64>,
}
#[derive(Clone, Serialize)]
struct ExtensionInfo {
    id: String,
    name: String,
    enabled: bool,
    icon: String,
}
#[derive(Serialize)]
struct TabInfo<'a> {
    id: u64,
    url: &'a str,
    title: &'a str,
    loading: bool,
    sleeping: bool,
    downloads: usize,
    folder_id: Option<u64>,
    favicon: &'a str,
    pinned: bool,
    muted: bool,
}
#[derive(Serialize)]
struct VaultInfo {
    origin: String,
    username: String,
}
struct App {
    windows: Vec<BrowserWindow>,
    store: Rc<RefCell<Store>>,
    proxy: EventLoopProxy<Event>,
    environment: ICoreWebView2Environment,
    ui_environment: ICoreWebView2Environment,
    next_tab: u64,
    next_window: u64,
    next_tick: Instant,
    next_frame: Instant,
    smoke: Option<smoke::Smoke>,
    exit_code: i32,
    folders_restored: bool,
    next_download: u64,
    active_dns: network::DnsSettings,
    network_proxy: Option<std::net::SocketAddr>,
    catalog: Vec<extension_store::Package>,
    catalog_loading: bool,
    protection: Rc<protection::Protection>,
    last_blocked: u64,
}

fn html() -> String {
    include_str!("../ui/index.html")
        .replace("/*__STYLE__*/", include_str!("../ui/style.css"))
        .replace(
            "/*__SCRIPT__*/",
            &format!(
                "{}\n{}\n{}",
                include_str!("../ui/i18n.js").replace(
                    "/*__TRANSLATIONS__*/",
                    include_str!("../ui/locales/en.json")
                ),
                include_str!("../ui/privacy.js"),
                include_str!("../ui/app.js")
            ),
        )
}
const UI_ORIGIN: &str = "https://slim-ui.localhost/";
fn popup_size(kind: &str) -> (f64, f64) {
    match kind {
        "extension" => (440., 600.),
        "page" => (350., 460.),
        "translate" => (350., 260.),
        "organize" => (300., 210.),
        "folder-menu" => (238., 178.),
        "tab-menu" => (290., 458.),
        "tab-folders" => (290., 300.),
        "downloads" => (350., 348.),
        _ => (324., 520.),
    }
}
fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        position: wry::dpi::LogicalPosition::new(x, y).into(),
        size: wry::dpi::LogicalSize::new(width.max(1.), height.max(1.)).into(),
    }
}
impl App {
    fn new(
        store: Store,
        proxy: EventLoopProxy<Event>,
        smoke: Option<smoke::Smoke>,
    ) -> Result<Self> {
        let proxy_address = network::start(&store.settings.dns)?;
        let environment = engine::environment(
            &store.directory.join("WebView2"),
            &store.settings.language,
            proxy_address,
        )?;
        let ui_environment = engine::environment(
            &store.directory.join("Interface"),
            &store.settings.language,
            None,
        )?;
        let active_dns = store.settings.dns.clone();
        Ok(Self {
            windows: vec![],
            store: Rc::new(RefCell::new(store)),
            proxy,
            environment,
            ui_environment,
            next_tab: 1,
            next_window: 1,
            next_tick: Instant::now() + Duration::from_secs(1),
            next_frame: Instant::now(),
            smoke,
            exit_code: 0,
            folders_restored: false,
            next_download: 1,
            active_dns,
            network_proxy: proxy_address,
            catalog: vec![],
            catalog_loading: false,
            protection: Rc::new(protection::Protection::new()?),
            last_blocked: 0,
        })
    }
    fn ui_builder(&self, id: WindowId, popup: bool) -> WebViewBuilder<'static> {
        let proxy = self.proxy.clone();
        WebViewBuilder::new()
            .with_environment(self.ui_environment.clone())
            .with_transparent(true)
            .with_devtools(false)
            .with_general_autofill_enabled(false)
            .with_additional_browser_args(engine::UI_ARGS)
            .with_custom_protocol("slim-ui".into(), |_, _| {
                wry::http::Response::builder()
                    .header("Content-Type", "text/html; charset=utf-8")
                    .header("Cache-Control", "no-store")
                    .body(std::borrow::Cow::Owned(html().into_bytes()))
                    .unwrap()
            })
            .with_https_scheme(true)
            .with_url("slim-ui://localhost/")
            .with_initialization_script(if popup {
                "window.__popup=true;"
            } else {
                "window.__popup=false;"
            })
            .with_navigation_handler(|url| url == UI_ORIGIN)
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                // Only the packaged, non-navigable UI has a host bridge. Sites get none.
                if request.uri() == UI_ORIGIN && request.body().len() <= 1_048_576 {
                    let _ = proxy.send_event(Event::Ipc(id, popup, request.into_body()));
                }
            })
    }
    fn open_window(
        &mut self,
        target: &EventLoopWindowTarget<Event>,
        private: bool,
        empty: bool,
    ) -> Result<()> {
        let window = WindowBuilder::new()
            .with_window_icon(
                tao::window::Icon::from_rgba(
                    include_bytes!("../assets/icon-64.rgba").to_vec(),
                    64,
                    64,
                )
                .ok(),
            )
            .with_title(if private {
                "Slim Browser · Приватное окно"
            } else {
                "Slim Browser"
            })
            .with_inner_size(LogicalSize::new(1280., 850.))
            .with_min_inner_size(LogicalSize::new(900., 550.))
            .with_decorations(false)
            .with_transparent(true)
            .with_background_color((0, 0, 0, 0))
            .with_visible(false)
            .build(target)?;
        let id = window.id();
        let settings = self.store.borrow().settings.clone();
        platform::material(&window, settings.glass, settings.dark_mode);
        let shell = self
            .ui_builder(id, false)
            .with_bounds(rect(0., 0., 1280., 850.))
            .build(&window)?;
        engine::configure(&shell, &settings, true)?;
        engine::shortcuts(&shell, id, self.proxy.clone(), true)?;
        platform::custom_frame(&window)?;
        let profile_name = if private {
            format!("Private-{}-{}", std::process::id(), self.next_window)
        } else {
            "Default".into()
        };
        self.next_window += 1;
        self.windows.push(BrowserWindow {
            shell,
            popup: None,
            popup_cache: None,
            popup_loaded: false,
            popup_session: 0,
            service: None,
            tabs: vec![],
            window,
            active: None,
            private,
            profile_name,
            view: None,
            status: String::new(),
            busy: false,
            private_history: vec![],
            extensions: vec![],
            popup_kind: "menu".into(),
            edge_to_edge: Cell::new(false),
            motion: None,
            restore_bounds: None,
            windowed_mode: WindowMode::Normal,
            share_session: None,
            private_folders: vec![],
            folder_edit: None,
            folder_rename: None,
            menu_tab: None,
            popup_anchor: None,
            extension_size: (400., 560.),
            transfers: vec![],
            pending_install: None,
            store_busy: false,
            private_permissions: Rc::new(RefCell::new(vec![])),
            site_data: vec![],
            storage_bytes: None,
        });
        let wi = self.windows.len() - 1;
        if !empty {
            self.add_tab(wi, HOME.into())?;
        }
        if !private && !self.folders_restored {
            self.folders_restored = true;
            let folders = self.store.borrow().folders.clone();
            for folder in folders {
                for page in folder.pages {
                    let id = self.next_tab;
                    self.next_tab += 1;
                    self.windows[wi].tabs.push(Tab {
                        id,
                        url: page.url,
                        title: page.title,
                        view: None,
                        loading: false,
                        background_since: Instant::now(),
                        downloads: 0,
                        active: Rc::new(Cell::new(false)),
                        folder_id: Some(folder.id),
                        favicon: String::new(),
                        pinned: false,
                        muted: false,
                    });
                }
            }
        }
        self.windows[wi].window.set_visible(true);
        self.layout(wi)?;
        self.sync(wi)?;
        Ok(())
    }
    fn index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|w| w.window.id() == id)
    }
    fn active_index(&self, wi: usize) -> Option<usize> {
        self.windows[wi]
            .tabs
            .iter()
            .position(|t| Some(t.id) == self.windows[wi].active)
    }
    fn finish_window_motion(&mut self, wi: usize) -> Result<()> {
        let Some(motion) = self.windows[wi].motion.take() else {
            return Ok(());
        };
        platform::place_window(&self.windows[wi].window, motion.spring.target)?;
        match motion.mode {
            WindowMode::Maximized => self.windows[wi].window.set_maximized(true),
            WindowMode::Fullscreen => self.windows[wi]
                .window
                .set_fullscreen(Some(Fullscreen::Borderless(None))),
            WindowMode::Normal => {}
        }
        self.layout(wi)
    }
    fn animate_window(&mut self, wi: usize, mode: WindowMode) -> Result<()> {
        let w = &mut self.windows[wi];
        let bounds = platform::window_bounds(&w.window)?;
        let previous_mode = w.motion.as_ref().map(|m| m.mode).unwrap_or_else(|| {
            if w.window.fullscreen().is_some() {
                WindowMode::Fullscreen
            } else if w.window.is_maximized() {
                WindowMode::Maximized
            } else {
                WindowMode::Normal
            }
        });
        if mode == WindowMode::Fullscreen && previous_mode != WindowMode::Fullscreen {
            w.windowed_mode = previous_mode;
        }
        if previous_mode == WindowMode::Normal && mode != WindowMode::Normal && w.motion.is_none() {
            w.restore_bounds = Some(bounds);
        }
        let velocity = w
            .motion
            .take()
            .map(|m| m.spring.velocity)
            .unwrap_or([0.; 4]);
        if w.window.fullscreen().is_some() {
            w.window.set_fullscreen(None);
        }
        if w.window.is_maximized() {
            w.window.set_maximized(false);
        }
        let target = match mode {
            WindowMode::Normal => w
                .restore_bounds
                .unwrap_or(platform::window_bounds(&w.window)?),
            WindowMode::Maximized => platform::monitor_bounds(&w.window, false)?,
            WindowMode::Fullscreen => platform::monitor_bounds(&w.window, true)?,
        };
        platform::place_window(&w.window, bounds)?;
        let mut spring = motion::Spring::new(bounds, target);
        spring.velocity = velocity;
        w.motion = Some(WindowMotion {
            spring,
            mode,
            previous: Instant::now(),
            started: Instant::now(),
        });
        self.next_frame = Instant::now();
        if !platform::motion_enabled() {
            self.finish_window_motion(wi)?;
        }
        Ok(())
    }
    fn animate_frame(&mut self) -> Result<()> {
        let now = Instant::now();
        self.next_frame = now + Duration::from_millis(16);
        for wi in 0..self.windows.len() {
            let w = &mut self.windows[wi];
            let finished = if let Some(motion) = &mut w.motion {
                let settled = motion
                    .spring
                    .step(now.duration_since(motion.previous).as_secs_f64());
                motion.previous = now;
                platform::place_window(&w.window, motion.spring.value)?;
                settled || now.duration_since(motion.started) > Duration::from_millis(350)
            } else {
                false
            };
            if finished {
                self.finish_window_motion(wi)?;
            }
        }
        Ok(())
    }
    fn page_bounds(&self, wi: usize) -> Rect {
        let size = self.windows[wi]
            .window
            .inner_size()
            .to_logical::<f64>(self.windows[wi].window.scale_factor());
        let (x, y) = self.store.borrow().settings.page_origin();
        let inset = if platform::edge_to_edge(&self.windows[wi].window) {
            0.
        } else {
            6.
        };
        rect(x, y, size.width - x - inset, size.height - y - inset)
    }
    fn layout(&self, wi: usize) -> Result<()> {
        let w = &self.windows[wi];
        let edge = platform::edge_to_edge(&w.window);
        if w.edge_to_edge.replace(edge) != edge {
            platform::rounding(&w.window);
            w.shell
                .evaluate_script(&format!("window.setWindowState?.({edge})"))?;
        }
        let size = w
            .window
            .inner_size()
            .to_logical::<f64>(w.window.scale_factor());
        w.shell.set_bounds(rect(0., 0., size.width, size.height))?;
        for t in &w.tabs {
            let visible = Some(t.id) == w.active && w.view.is_none();
            let changed = t.active.replace(visible) != visible;
            if let Some(v) = &t.view {
                if visible {
                    v.set_bounds(self.page_bounds(wi))?;
                }
                if changed {
                    v.set_visible(visible)?;
                    v.set_memory_usage_level(if visible {
                        MemoryUsageLevel::Normal
                    } else {
                        MemoryUsageLevel::Low
                    })?;
                }
            }
        }
        if let Some(p) = &w.popup {
            let (width, height) = self.popup_dimensions(wi, &w.popup_kind);
            p.set_bounds(self.popup_bounds(wi, width, height))?;
        }
        Ok(())
    }
    fn sync(&self, wi: usize) -> Result<()> {
        let w = &self.windows[wi];
        let store = self.store.borrow();
        if w.private {
            w.window.set_title(&i18n::text(
                &store.settings.language,
                "Slim Browser · Приватное окно",
            ));
        }
        let tabs = w
            .tabs
            .iter()
            .map(|t| TabInfo {
                id: t.id,
                url: &t.url,
                title: &t.title,
                loading: t.loading,
                sleeping: t.view.is_none() && t.url != HOME,
                downloads: t.downloads,
                folder_id: t.folder_id,
                favicon: &t.favicon,
                pinned: t.pinned,
                muted: t.muted,
            })
            .collect::<Vec<_>>();
        // Passwords never go into a shell snapshot, including when DevTools is used.
        let credentials = if w.view.as_deref() == Some("passwords") {
            store
                .credentials()?
                .iter()
                .map(|c| VaultInfo {
                    origin: c.origin.clone(),
                    username: c.username.clone(),
                })
                .collect()
        } else {
            Vec::<VaultInfo>::new()
        };
        let (can_back, can_forward) = self
            .active_index(wi)
            .and_then(|ti| w.tabs[ti].view.as_ref())
            .map(|v| unsafe {
                let core = v.webview();
                let mut b = BOOL::default();
                let mut f = BOOL::default();
                let _ = core.CanGoBack(&mut b);
                let _ = core.CanGoForward(&mut f);
                (b.as_bool(), f.as_bool())
            })
            .unwrap_or_default();
        let bookmarks = if w.view.as_deref() == Some("bookmarks") {
            store.bookmarks.as_slice()
        } else {
            &[]
        };
        let history = if w.view.as_deref() == Some("history") {
            if w.private {
                w.private_history.as_slice()
            } else {
                store.history.as_slice()
            }
        } else {
            &[]
        };
        let (page_left, page_top) = store.settings.page_origin();
        let folders = if w.private {
            &w.private_folders
        } else {
            &store.folders
        };
        let transfers = w
            .transfers
            .iter()
            .rev()
            .map(|d| &d.info)
            .collect::<Vec<_>>();
        let wide_menu = store.settings.browser_layout == "unified"
            && store.settings.sidebar_visible
            && w.view.is_none();
        let permissions = if w.private {
            w.private_permissions.borrow().clone()
        } else {
            store.settings.privacy.permissions.clone()
        };
        let state = json!({"blockedRequests":self.protection.blocked.get(),"storageBytes":w.storage_bytes,"permissions":permissions,"permissionCategories":privacy::CATEGORIES,"siteData":w.site_data,"settings":store.settings,"catalog":self.catalog,"catalogLoading":self.catalog_loading,"pendingInstall":w.pending_install,"storeBusy":w.store_busy,"dnsRestartRequired":store.settings.dns!=self.active_dns,"tabs":tabs,"active":w.active,"private":w.private,"view":w.view,"status":w.status,"busy":w.busy,"menuOpen":w.popup.is_some(),"popupMode":w.popup_kind,"popupSession":w.popup_session,"wideMenu":wide_menu,"folders":folders,"folderEdit":w.folder_edit,"folderRename":w.folder_rename,"menuTab":w.menu_tab,"transfers":transfers,"edgeToEdge":platform::edge_to_edge(&w.window),"translationLanguages":model::TRANSLATION_LANGUAGES,"searchEngines":model::SEARCH_ENGINES,"pageLeft":page_left,"pageTop":page_top,"bookmarks":bookmarks,"history":history,"credentials":credentials,"extensions":w.extensions,"canBack":can_back,"canForward":can_forward});
        let script = format!("window.renderState?.({state})");
        w.shell.evaluate_script(&script)?;
        if let Some(p) = &w.popup {
            p.evaluate_script(&script)?;
        }
        Ok(())
    }
    fn sync_all(&self) -> Result<()> {
        for i in 0..self.windows.len() {
            self.layout(i)?;
            self.sync(i)?;
        }
        Ok(())
    }
    fn add_tab(&mut self, wi: usize, url: String) -> Result<()> {
        if self.windows[wi].busy {
            return Ok(());
        }
        if let Some(ti) = self.active_index(wi) {
            self.windows[wi].tabs[ti].background_since = Instant::now();
        }
        let id = self.next_tab;
        self.next_tab += 1;
        self.windows[wi].tabs.push(Tab {
            id,
            url,
            title: "Новая вкладка".into(),
            view: None,
            loading: false,
            background_since: Instant::now(),
            downloads: 0,
            folder_id: None,
            favicon: String::new(),
            pinned: false,
            muted: false,
            active: Rc::new(Cell::new(false)),
        });
        self.windows[wi].active = Some(id);
        self.windows[wi].view = None;
        self.ensure_active(wi)?;
        self.layout(wi)?;
        self.sync(wi)?;
        Ok(())
    }
    fn select_tab(&mut self, wi: usize, id: u64) -> Result<()> {
        if !self.windows[wi].tabs.iter().any(|t| t.id == id) {
            return Ok(());
        }
        if let Some(ti) = self.active_index(wi) {
            self.windows[wi].tabs[ti].background_since = Instant::now();
        }
        self.windows[wi].active = Some(id);
        self.windows[wi].view = None;
        self.ensure_active(wi)?;
        self.layout(wi)?;
        self.sync(wi)
    }
    fn content_builder(&self, wi: usize) -> WebViewBuilder<'static> {
        let directory = self.store.borrow().directory.clone();
        let check = directory.clone();
        let p = self.proxy.clone();
        let id = self.windows[wi].window.id();
        let opener = self.windows[wi].active.unwrap_or(0);
        let source = self
            .active_index(wi)
            .map(|ti| self.windows[wi].tabs[ti].url.clone())
            .unwrap_or_default();
        WebViewBuilder::new()
            .with_environment(self.environment.clone())
            .with_incognito(self.windows[wi].private)
            .with_profile_name(self.windows[wi].profile_name.clone())
            .with_devtools(false)
            .with_general_autofill_enabled(false)
            .with_initialization_script(extension_store::popup_context_script(&source))
            .with_ipc_handler(move |request| {
                if request.body().len() > 8192
                    || !extension_store::registered_url(&check, &request.uri().to_string())
                {
                    return;
                }
                if let Ok(v) = serde_json::from_str::<Value>(request.body()) {
                    if v["action"] == "open" {
                        if let Some(url) = v["url"].as_str() {
                            let same = url::Url::parse(&request.uri().to_string())
                                .ok()
                                .zip(url::Url::parse(url).ok())
                                .is_some_and(|(a, b)| {
                                    a.scheme() == b.scheme() && a.host_str() == b.host_str()
                                });
                            if model::is_web(url) || same {
                                let _ = p.send_event(Event::ExtensionOpen(id, opener, url.into()));
                            }
                        }
                    }
                }
            })
            .with_navigation_handler(move |url| {
                url == "about:blank"
                    || model::is_page_url(&url)
                    || extension_store::registered_url(&directory, &url)
            })
            .with_url("about:blank")
            .with_focused(false)
    }
    fn ensure_active(&mut self, wi: usize) -> Result<()> {
        let Some(ti) = self.active_index(wi) else {
            return Ok(());
        };
        if self.windows[wi].tabs[ti].view.is_some() || self.windows[wi].tabs[ti].url == HOME {
            return Ok(());
        }
        let id = self.windows[wi].window.id();
        let tab = self.windows[wi].tabs[ti].id;
        let p = self.proxy.clone();
        let p3 = p.clone();
        let p4 = p.clone();
        let store = self.store.clone();
        let download_tab = tab;
        let view = self
            .content_builder(wi)
            .with_bounds(self.page_bounds(wi))
            .with_document_title_changed_handler(move |title| {
                let _ = p.send_event(Event::Title(id, tab, title));
            })
            .with_download_started_handler(move |url, path| {
                let folder = store.borrow().settings.download_folder.clone();
                if let Err(e) = std::fs::create_dir_all(&folder) {
                    let _ = p3.send_event(Event::Notice(id, e.to_string()));
                    return false;
                }
                let name = path
                    .file_name()
                    .filter(|n| !n.is_empty())
                    .unwrap_or(std::ffi::OsStr::new("download"));
                let mut destination = folder.join(name);
                let stem = destination
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let ext = destination
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                for n in 1..10000 {
                    if !destination.exists() {
                        break;
                    }
                    destination = folder.join(format!("{stem} ({n}){ext}"));
                }
                *path = destination;
                let _ = p3.send_event(Event::Download(id, download_tab, true, url));
                true
            })
            .with_download_completed_handler(move |_, _, success| {
                let _ = p4.send_event(Event::Download(
                    id,
                    download_tab,
                    false,
                    if success {
                        "Файл скачан".into()
                    } else {
                        "Загрузка прервана".into()
                    },
                ));
            })
            .build_as_child(&self.windows[wi].window)?;
        engine::configure(&view, &self.store.borrow().settings, false)?;
        if extension_store::registered_url(
            &self.store.borrow().directory,
            &self.windows[wi].tabs[ti].url,
        ) {
            unsafe {
                view.controller().SetZoomFactor(1.0)?;
                view.webview().Settings()?.SetIsWebMessageEnabled(true)?;
            }
        }
        unsafe {
            view.webview()
                .cast::<ICoreWebView2_8>()?
                .SetIsMuted(self.windows[wi].tabs[ti].muted)?;
        }
        engine::hooks(
            &view,
            id,
            tab,
            self.proxy.clone(),
            self.windows[wi].tabs[ti].active.clone(),
            self.windows[wi].window.hwnd(),
            privacy::PermissionContext {
                store: self.store.clone(),
                private_rules: self.windows[wi]
                    .private
                    .then(|| self.windows[wi].private_permissions.clone()),
            },
        )?;
        protection::hook(
            &view,
            self.store.clone(),
            self.protection.clone(),
            self.windows[wi].private,
            self.environment.clone(),
        )?;
        protection::apply(
            &view,
            &self.store.borrow().settings.privacy,
            self.windows[wi].private,
            self.protection.clone(),
            self.proxy.clone(),
            id,
        )?;
        if let Some(smoke) = &self.smoke {
            smoke::map_fixture(&view, &smoke.directory)?;
        }
        let url = self.windows[wi].tabs[ti].url.clone();
        self.windows[wi].tabs[ti].loading = true;
        view.load_url(&url)?;
        self.windows[wi].tabs[ti].view = Some(view);
        Ok(())
    }
    fn navigate(&mut self, wi: usize, input: &str) -> Result<()> {
        let url = model::resolve(input, &self.store.borrow().settings.search_engine)?;
        if self.active_index(wi).is_none() {
            self.add_tab(wi, url)?;
            return Ok(());
        }
        let ti = self.active_index(wi).unwrap();
        self.windows[wi].view = None;
        self.windows[wi].tabs[ti].url = url.clone();
        self.windows[wi].tabs[ti].loading = url != HOME;
        if url == HOME {
            self.windows[wi].tabs[ti].view.take();
            self.windows[wi].tabs[ti].title = "Новая вкладка".into();
            self.windows[wi].tabs[ti].loading = false;
        } else if let Some(view) = &self.windows[wi].tabs[ti].view {
            view.load_url(&url)?;
        } else {
            self.ensure_active(wi)?;
        }
        self.layout(wi)?;
        if !self.windows[wi].private {
            self.save_folder_tabs()?;
        }
        self.sync(wi)
    }
    fn service(&mut self, wi: usize) -> Result<ICoreWebView2> {
        if self.windows[wi].service.is_none() {
            let view = self
                .content_builder(wi)
                .with_visible(false)
                .with_bounds(rect(0., 0., 1., 1.))
                .build_as_child(&self.windows[wi].window)?;
            engine::configure(&view, &self.store.borrow().settings, false)?;
            self.windows[wi].service = Some(view);
        }
        Ok(self.windows[wi].service.as_ref().unwrap().webview())
    }
    fn close_tab(&mut self, wi: usize, id: Option<u64>) -> Result<()> {
        let id = id.or(self.windows[wi].active);
        let Some(ti) = self.windows[wi].tabs.iter().position(|t| Some(t.id) == id) else {
            return Ok(());
        };
        if self.windows[wi].tabs[ti].downloads > 0
            && !platform::confirm(
                &self.windows[wi].window,
                &i18n::text(
                    &self.store.borrow().settings.language,
                    "Закрытие вкладки прервёт загрузку файла. Продолжить?",
                ),
            )
        {
            return Ok(());
        }
        let closing_tab = self.windows[wi].tabs[ti].id;
        for transfer in &mut self.windows[wi].transfers {
            if transfer.tab == closing_tab
                && (transfer.info.state == "progress" || transfer.info.can_resume)
            {
                unsafe {
                    let _ = transfer.operation.Cancel();
                }
                let _ = transfer.sample();
            }
        }
        self.windows[wi].tabs.remove(ti);
        if !self.windows[wi].private {
            self.save_folder_tabs()?;
        }
        if self.windows[wi].active == id {
            self.windows[wi].active = self.windows[wi]
                .tabs
                .get(ti.min(self.windows[wi].tabs.len().saturating_sub(1)))
                .map(|t| t.id);
        }
        if self.windows[wi].tabs.is_empty() {
            self.add_tab(wi, HOME.into())?;
        } else {
            self.ensure_active(wi)?;
            self.layout(wi)?;
            self.sync(wi)?;
        }
        Ok(())
    }
    fn discard(&mut self, wi: usize, ti: usize) -> bool {
        let active = self.windows[wi].active;
        let tab = &mut self.windows[wi].tabs[ti];
        if Some(tab.id) == active || tab.loading || tab.downloads > 0 || tab.view.is_none() {
            return false;
        }
        tab.view.take();
        tab.active.set(false);
        true
    }
    fn start_cleanup(
        &mut self,
        wi: usize,
        cookies: bool,
        cache: bool,
        purpose: CleanupPurpose,
    ) -> Result<()> {
        if self.windows[wi].busy {
            return Ok(());
        }
        let core = self.service(wi)?;
        if matches!(purpose, CleanupPurpose::Exit) {
            for t in &mut self.windows[wi].tabs {
                if let Some(v) = &t.view {
                    unsafe {
                        let _ = v.webview().Stop();
                    }
                }
                t.view.take();
                t.loading = false;
            }
        } else {
            for ti in 0..self.windows[wi].tabs.len() {
                self.discard(wi, ti);
            }
        }
        self.windows[wi].busy = true;
        self.windows[wi].status = "Очистка данных…".into();
        self.sync(wi)?;
        if matches!(purpose, CleanupPurpose::Exit) {
            privacy::exit_cleanup(
                &core,
                &self.store.borrow().settings,
                self.proxy.clone(),
                self.windows[wi].window.id(),
            )?;
            return Ok(());
        }
        engine::cleanup(
            &core,
            cookies,
            cache,
            self.proxy.clone(),
            self.windows[wi].window.id(),
            purpose,
        )?;
        Ok(())
    }
    fn request_close(&mut self, wi: usize) -> Result<()> {
        if self.windows[wi].busy {
            return Ok(());
        }
        if self.windows[wi].tabs.iter().any(|t| t.downloads > 0)
            && !platform::confirm(
                &self.windows[wi].window,
                &i18n::text(
                    &self.store.borrow().settings.language,
                    "Выход прервёт загрузки файлов. Продолжить?",
                ),
            )
        {
            return Ok(());
        }
        if !self.windows[wi].private {
            self.save_folder_tabs()?;
        }
        let settings = self.store.borrow().settings.clone();
        if !self.windows[wi].private
            && self.windows.iter().filter(|w| !w.private).count() == 1
            && (settings.clear_cache_on_exit
                || settings.clear_cookies_on_exit
                || settings.privacy.clear_site_data_on_exit
                || settings.privacy.clears_history()
                || !settings.privacy.remembers_history())
        {
            self.start_cleanup(
                wi,
                settings.clear_cookies_on_exit,
                settings.clear_cache_on_exit,
                CleanupPurpose::Exit,
            )
        } else {
            platform::close_animation(&self.windows[wi].window);
            self.windows.remove(wi);
            Ok(())
        }
    }
    fn toggle_menu(&mut self, wi: usize) -> Result<()> {
        self.toggle_popup(wi, "menu")
    }
    fn close_popup(&mut self, wi: usize) -> Result<bool> {
        let Some(popup) = self.windows[wi].popup.take() else {
            return Ok(false);
        };
        popup.set_visible(false)?;
        if self.windows[wi].popup_kind != "extension" {
            popup.set_memory_usage_level(MemoryUsageLevel::Low)?;
            self.windows[wi].popup_cache = Some(popup);
        }
        Ok(true)
    }
    fn warm_popup(&mut self, wi: usize) -> Result<()> {
        if self.windows[wi].popup_cache.is_some() || self.windows[wi].popup.is_some() {
            return Ok(());
        }
        let id = self.windows[wi].window.id();
        let popup = self
            .ui_builder(id, true)
            .with_visible(false)
            .with_focused(false)
            .with_bounds(self.popup_bounds(wi, 350., 480.))
            .build_as_child(&self.windows[wi].window)?;
        engine::configure(&popup, &self.store.borrow().settings, true)?;
        engine::shortcuts(&popup, id, self.proxy.clone(), true)?;
        popup.set_memory_usage_level(MemoryUsageLevel::Low)?;
        self.windows[wi].popup_cache = Some(popup);
        Ok(())
    }
    fn toggle_popup(&mut self, wi: usize, kind: &str) -> Result<()> {
        let same = self.windows[wi].popup_kind == kind;
        if self.close_popup(wi)? && same {
            return self.sync(wi);
        }
        self.windows[wi].popup_kind = kind.to_owned();
        self.windows[wi].popup_session += 1;
        let id = self.windows[wi].window.id();
        // Keep one trusted panel per window; extension views never enter this cache.
        self.warm_popup(wi)?;
        let (width, height) = self.popup_dimensions(wi, kind);
        let popup = self.windows[wi]
            .popup_cache
            .take()
            .context("Popup cache unavailable")?;
        popup.set_memory_usage_level(MemoryUsageLevel::Normal)?;
        popup.set_bounds(self.popup_bounds(wi, width, height))?;
        self.windows[wi].popup = Some(popup);
        self.sync(wi)?;
        if self.windows[wi].popup_loaded {
            let proxy = self.proxy.clone();
            self.windows[wi]
                .popup
                .as_ref()
                .unwrap()
                .evaluate_script_with_callback("true", move |_| {
                    let _ = proxy.send_event(Event::Ipc(id, true, r#"{"cmd":"ready"}"#.into()));
                })?;
        }
        if kind == "page" && !self.windows[wi].private {
            self.load_extensions(wi)?;
        }
        Ok(())
    }
    fn popup_ready(&mut self, wi: usize) -> Result<()> {
        if self.windows[wi].popup_kind == "extension" {
            return Ok(());
        }
        self.windows[wi].popup_loaded = true;
        let Some(popup) = &self.windows[wi].popup else {
            return Ok(());
        };
        // Wait for the packaged document to load before taking focus. The
        // originating button can still own focus while its click is completing.
        popup.set_visible(true)?;
        popup.focus()?;
        popup.evaluate_script("window.revealPopup?.();window.focusPopup?.()")?;
        Ok(())
    }
    fn popup_bounds(&self, wi: usize, width: f64, height: f64) -> Rect {
        let w = &self.windows[wi];
        let settings = &self.store.borrow().settings;
        let size = w
            .window
            .inner_size()
            .to_logical::<f64>(w.window.scale_factor());
        let width = width.min(size.width - 24.);
        let height = height.min(size.height - 102.);
        if matches!(
            w.popup_kind.as_str(),
            "tab-menu" | "tab-folders" | "folder-menu"
        ) {
            let (x, y) = w.popup_anchor.unwrap_or((16., 150.));
            return rect(
                x.clamp(8., (size.width - width - 8.).max(8.)),
                y.clamp(48., (size.height - height - 8.).max(48.)),
                width,
                height,
            );
        }
        if matches!(w.popup_kind.as_str(), "downloads" | "organize") {
            let footer = if settings.browser_layout == "compact" && w.popup_kind == "downloads" {
                108.
            } else {
                60.
            };
            rect(12., (size.height - height - footer).max(48.), width, height)
        } else if settings.browser_layout == "unified"
            && settings.sidebar_visible
            && w.view.is_none()
        {
            rect(
                settings.page_origin().0 + 12.,
                48.,
                width.min(size.width - settings.page_origin().0 - 24.),
                height.min(size.height - 60.),
            )
        } else {
            let size = w
                .window
                .inner_size()
                .to_logical::<f64>(w.window.scale_factor());
            rect(size.width - width - 12., 90., width, height)
        }
    }
    fn popup_dimensions(&self, wi: usize, kind: &str) -> (f64, f64) {
        let settings = &self.store.borrow().settings;
        if kind == "extension" {
            return self.windows[wi].extension_size;
        }
        if kind == "downloads" {
            (
                350.,
                if self.windows[wi].transfers.is_empty() {
                    156.
                } else {
                    120. + self.windows[wi].transfers.len().min(3) as f64 * 76.
                },
            )
        } else if kind == "menu"
            && settings.browser_layout == "unified"
            && settings.sidebar_visible
            && self.windows[wi].view.is_none()
        {
            (560., 340.)
        } else {
            popup_size(kind)
        }
    }
    fn save_folder_tabs(&mut self) -> Result<()> {
        let mut store = self.store.borrow_mut();
        for folder in &mut store.folders {
            folder.pages = self
                .windows
                .iter()
                .filter(|w| !w.private)
                .flat_map(|w| &w.tabs)
                .filter(|t| t.folder_id == Some(folder.id))
                .map(|t| model::FolderPage {
                    url: t.url.clone(),
                    title: t.title.clone(),
                })
                .collect();
        }
        store.save_folders()
    }
    fn tab_action(
        &mut self,
        wi: usize,
        message: &Value,
        target: &EventLoopWindowTarget<Event>,
    ) -> Result<()> {
        let id = message["id"]
            .as_u64()
            .or(self.windows[wi].menu_tab)
            .context("Выберите вкладку")?;
        let Some(ti) = self.windows[wi].tabs.iter().position(|t| t.id == id) else {
            return Ok(());
        };
        match message["action"].as_str().unwrap_or_default() {
            "duplicate" | "window" => {
                let url = self.windows[wi].tabs[ti].url.clone();
                let folder = self.windows[wi].tabs[ti].folder_id;
                if message["action"] == "window" {
                    let private = self.windows[wi].private;
                    self.open_window(target, private, true)?;
                    let new_wi = self.windows.len() - 1;
                    self.add_tab(new_wi, url)?;
                } else {
                    self.add_tab(wi, url)?;
                    let new_ti = self.active_index(wi).context("Вкладка не создана")?;
                    self.windows[wi].tabs[new_ti].folder_id = folder;
                    if !self.windows[wi].private {
                        self.save_folder_tabs()?;
                    }
                }
            }
            "reload" => {
                if let Some(view) = &self.windows[wi].tabs[ti].view {
                    unsafe {
                        view.webview().Reload()?;
                    }
                } else {
                    self.select_tab(wi, id)?;
                }
            }
            "pin" => {
                self.windows[wi].tabs[ti].pinned = !self.windows[wi].tabs[ti].pinned;
                self.windows[wi].tabs.sort_by_key(|t| !t.pinned);
            }
            "mute" => {
                let muted = !self.windows[wi].tabs[ti].muted;
                let host = url::Url::parse(&self.windows[wi].tabs[ti].url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned));
                for tab in &mut self.windows[wi].tabs {
                    let same_host = host.is_some()
                        && url::Url::parse(&tab.url)
                            .ok()
                            .and_then(|u| u.host_str().map(str::to_owned))
                            == host;
                    if tab.id == id || same_host {
                        if let Some(view) = &tab.view {
                            unsafe {
                                view.webview()
                                    .cast::<ICoreWebView2_8>()?
                                    .SetIsMuted(muted)?;
                            }
                        }
                        tab.muted = muted;
                    }
                }
            }
            "close" => self.close_tab(wi, Some(id))?,
            "close_others" | "close_right" => {
                let ids = self.windows[wi]
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, t)| {
                        t.id != id && !t.pinned && (message["action"] == "close_others" || *i > ti)
                    })
                    .map(|(_, t)| t.id)
                    .collect::<Vec<_>>();
                for closing in ids {
                    self.close_tab(wi, Some(closing))?;
                }
            }
            _ => bail!("Неизвестное действие с вкладкой"),
        }
        self.sync(wi)
    }
    fn folder_command(&mut self, wi: usize, command: &str, message: &Value) -> Result<()> {
        let mut folders = if self.windows[wi].private {
            self.windows[wi].private_folders.clone()
        } else {
            self.store.borrow().folders.clone()
        };
        match command {
            "folder_create" => {
                let name = model::folder_name(message["name"].as_str().unwrap_or_default())?;
                let id = folders
                    .iter()
                    .map(|f| f.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .context("Слишком много папок")?;
                folders.push(model::TabFolder {
                    id,
                    name,
                    collapsed: false,
                    pages: vec![],
                });
            }
            "folder_rename" | "folder_toggle" => {
                let id = message["id"].as_u64().context("Выберите папку")?;
                let folder = folders
                    .iter_mut()
                    .find(|f| f.id == id)
                    .context("Папка не найдена")?;
                if command == "folder_toggle" {
                    folder.collapsed = !folder.collapsed;
                } else {
                    folder.name = model::folder_name(message["name"].as_str().unwrap_or_default())?;
                }
            }
            "folder_delete" => {
                let id = message["id"].as_u64().context("Выберите папку")?;
                folders.retain(|f| f.id != id);
                // Deleting the container leaves its pages open in the main list.
                let private = self.windows[wi].private;
                let window_id = self.windows[wi].window.id();
                for window in &mut self.windows {
                    if window.private == private && (!private || window.window.id() == window_id) {
                        for tab in &mut window.tabs {
                            if tab.folder_id == Some(id) {
                                tab.folder_id = None;
                            }
                        }
                    }
                }
            }
            "folder_reorder" => {
                let id = message["folder"].as_u64().context("Выберите папку")?;
                let from = folders
                    .iter()
                    .position(|f| f.id == id)
                    .context("Папка не найдена")?;
                let before = message["before"]
                    .as_u64()
                    .map(|id| {
                        folders
                            .iter()
                            .position(|f| f.id == id)
                            .context("Папка не найдена")
                    })
                    .transpose()?;
                model::move_before(&mut folders, from, before);
            }
            "folder_move" | "tab_reorder" => {
                let folder = message["folder"].as_u64();
                anyhow::ensure!(
                    folder.is_none() || folders.iter().any(|f| Some(f.id) == folder),
                    "Папка не найдена"
                );
                let id = message["tab"]
                    .as_u64()
                    .or(self.windows[wi].active)
                    .context("Выберите вкладку")?;
                let tabs = &mut self.windows[wi].tabs;
                let from = tabs
                    .iter()
                    .position(|t| t.id == id)
                    .context("Вкладка не найдена")?;
                if command == "tab_reorder" {
                    let before = message["before"]
                        .as_u64()
                        .map(|id| {
                            tabs.iter()
                                .position(|t| t.id == id && t.folder_id == folder)
                                .context("Вкладка назначения не найдена")
                        })
                        .transpose()?;
                    tabs[from].folder_id = folder;
                    model::move_before(tabs, from, before);
                } else {
                    tabs[from].folder_id = folder;
                }
                if let Some(target) = folders.iter_mut().find(|f| Some(f.id) == folder) {
                    target.collapsed = false;
                }
            }
            _ => bail!("Неизвестное действие с папкой"),
        }
        if command == "folder_rename" {
            self.windows[wi].folder_rename = None;
        }
        if self.windows[wi].private {
            self.windows[wi].private_folders = folders;
        } else {
            self.store.borrow_mut().folders = folders;
            self.save_folder_tabs()?;
        }
        self.sync_all()
    }
    fn set_view(&mut self, wi: usize, view: Option<String>) -> Result<()> {
        self.close_popup(wi)?;
        self.windows[wi].view = view;
        if self.windows[wi].view.as_deref() == Some("privacy") {
            self.measure_storage(wi);
        }
        if self.windows[wi].view.as_deref() == Some("permissions") {
            self.import_permissions(wi)?;
        }
        self.layout(wi)?;
        self.sync(wi)?;
        if matches!(
            self.windows[wi].view.as_deref(),
            Some("extensions" | "extension-store")
        ) {
            self.load_extensions(wi)?;
        }
        Ok(())
    }
    fn refresh_catalog(&mut self) -> Result<()> {
        if self.catalog_loading {
            return Ok(());
        }
        self.catalog_loading = true;
        if self.catalog.is_empty() {
            self.catalog = extension_store::builtin_catalog();
        }
        let proxy = self.proxy.clone();
        let network_proxy = self.network_proxy;
        std::thread::spawn(move || {
            let _ = proxy.send_event(Event::Catalog(extension_store::catalog(network_proxy)));
        });
        self.sync_all()
    }
    fn prepare_extension(&mut self, wi: usize, package_id: &str) -> Result<()> {
        anyhow::ensure!(
            !self.windows[wi].private,
            "Расширения доступны в обычном окне"
        );
        if self.windows[wi].store_busy || self.windows[wi].pending_install.is_some() {
            return Ok(());
        }
        let package = self
            .catalog
            .iter()
            .find(|p| p.id == package_id && !p.download.is_empty())
            .context("Обновите каталог расширений")?
            .clone();
        self.windows[wi].store_busy = true;
        self.windows[wi].status = format!("Загрузка {}…", package.name);
        let directory = self.store.borrow().directory.join("Extensions");
        let id = self.windows[wi].window.id();
        let proxy = self.proxy.clone();
        let network_proxy = self.network_proxy;
        std::thread::spawn(move || {
            let result = extension_store::prepare(&package, &directory, network_proxy);
            let _ = proxy.send_event(Event::PackagePrepared(
                id,
                result.map_err(|e| e.to_string()),
            ));
        });
        self.sync(wi)
    }
    fn confirm_extension(&mut self, wi: usize) -> Result<()> {
        if self.windows[wi].store_busy {
            return Ok(());
        }
        let pending = self.windows[wi]
            .pending_install
            .as_ref()
            .context("Сначала выберите расширение")?
            .clone();
        let core = self.service(wi)?;
        let id = self.windows[wi].window.id();
        let proxy = self.proxy.clone();
        self.windows[wi].store_busy = true;
        self.windows[wi].status = format!("Установка {}…", pending.name);
        unsafe {
            engine::profile(&core)?
                .cast::<ICoreWebView2Profile7>()?
                .AddBrowserExtension(
                    &HSTRING::from(pending.folder.as_path()),
                    &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
                        move |result, extension| {
                            let result = (|| -> Result<String> {
                                result?;
                                let extension = extension.context("Не получен ID расширения")?;
                                Ok(engine::string(|p| extension.Id(p))?)
                            })();
                            let _ = proxy.send_event(Event::StoreInstalled(
                                id,
                                result.map_err(|e| e.to_string()),
                            ));
                            Ok(())
                        },
                    )),
                )?;
        }
        self.sync(wi)
    }
    fn load_extensions(&mut self, wi: usize) -> Result<()> {
        if self.windows[wi].private {
            bail!("Управление расширениями доступно в обычном окне.")
        }
        let directory = self.store.borrow().directory.clone();
        if let Ok(bytes) = std::fs::read(directory.join("installed-extensions.json")) {
            if let Ok(registry) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(folder) =
                    registry["ikkiomngodfmfdmeomohmjeobdgpholf"]["folder"].as_str()
                {
                    extension_store::apply_compatibility(
                        std::path::Path::new(folder),
                        "ikkiomngodfmfdmeomohmjeobdgpholf",
                    )?;
                }
            }
        }
        let core = self.service(wi)?;
        let id = self.windows[wi].window.id();
        let p = self.proxy.clone();
        unsafe {
            engine::profile(&core)?
                .cast::<ICoreWebView2Profile7>()?
                .GetBrowserExtensions(&ProfileGetBrowserExtensionsCompletedHandler::create(
                    Box::new(move |result, list| {
                        let result = (|| -> windows::core::Result<Vec<ExtensionInfo>> {
                            result?;
                            let Some(list) = list else { return Ok(vec![]) };
                            let mut count = 0;
                            list.Count(&mut count)?;
                            let mut entries = vec![];
                            for i in 0..count {
                                let ex = list.GetValueAtIndex(i)?;
                                let mut enabled = BOOL::default();
                                ex.IsEnabled(&mut enabled)?;
                                entries.push(ExtensionInfo {
                                    id: engine::string(|v| ex.Id(v))?,
                                    name: engine::string(|v| ex.Name(v))?,
                                    enabled: enabled.as_bool(),
                                    icon: String::new(),
                                });
                            }
                            Ok(entries)
                        })();
                        let _ =
                            p.send_event(Event::Extensions(id, result.map_err(|e| e.to_string())));
                        Ok(())
                    }),
                ))?;
        }
        Ok(())
    }
    fn extension_action(
        &mut self,
        wi: usize,
        action: &str,
        extension_id: &str,
        confirmed: bool,
    ) -> Result<()> {
        if self.windows[wi].private {
            bail!("Расширения доступны в обычном окне.")
        }
        let core = self.service(wi)?;
        let profile = engine::profile(&core)?.cast::<ICoreWebView2Profile7>()?;
        let id = self.windows[wi].window.id();
        let p = self.proxy.clone();
        if action == "extension_install" {
            let Some(folder) = rfd::FileDialog::new()
                .set_title(i18n::text(
                    &self.store.borrow().settings.language,
                    "Папка расширения с manifest.json",
                ))
                .pick_folder()
            else {
                return Ok(());
            };
            if !folder.join("manifest.json").is_file() {
                bail!("В папке отсутствует manifest.json.")
            }
            if !platform::confirm(
                &self.windows[wi].window,
                &i18n::text(
                    &self.store.borrow().settings.language,
                    "Установить доверенное расширение? Оно получит доступ согласно manifest.json.",
                ),
            ) {
                return Ok(());
            }
            unsafe {
                profile.AddBrowserExtension(
                    &HSTRING::from(folder.as_path()),
                    &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
                        move |result, _| {
                            let _ = p.send_event(Event::ExtensionChanged(
                                id,
                                result.map_err(|e| e.to_string()),
                            ));
                            Ok(())
                        },
                    )),
                )?;
            }
        } else {
            if action == "extension_remove" && !confirmed {
                bail!("Подтвердите удаление расширения");
            }
            let wanted = extension_id.to_owned();
            let action = action.to_owned();
            unsafe {
                profile.GetBrowserExtensions(
                    &ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                        move |result, list| {
                            result?;
                            if let Some(list) = list {
                                let mut count = 0;
                                list.Count(&mut count)?;
                                for i in 0..count {
                                    let ex = list.GetValueAtIndex(i)?;
                                    if engine::string(|v| ex.Id(v))? != wanted {
                                        continue;
                                    }
                                    let p = p.clone();
                                    if action == "extension_remove" {
                                        ex.Remove(
                                            &BrowserExtensionRemoveCompletedHandler::create(
                                                Box::new(move |result| {
                                                    let _ = p.send_event(Event::ExtensionChanged(
                                                        id,
                                                        result.map_err(|e| e.to_string()),
                                                    ));
                                                    Ok(())
                                                }),
                                            ),
                                        )?;
                                    } else {
                                        let mut enabled = BOOL::default();
                                        ex.IsEnabled(&mut enabled)?;
                                        ex.Enable(
                                            !enabled.as_bool(),
                                            &BrowserExtensionEnableCompletedHandler::create(
                                                Box::new(move |result| {
                                                    let _ = p.send_event(Event::ExtensionChanged(
                                                        id,
                                                        result.map_err(|e| e.to_string()),
                                                    ));
                                                    Ok(())
                                                }),
                                            ),
                                        )?;
                                    }
                                    break;
                                }
                            }
                            Ok(())
                        },
                    )),
                )?;
            }
        }
        Ok(())
    }
    fn change_setting(&mut self, key: &str, value: &Value, commit: bool) -> Result<()> {
        {
            let mut store = self.store.borrow_mut();
            let s = &mut store.settings;
            match key {
                "Glass" => s.glass = value.as_bool().context("Ожидался переключатель")?,
                "DarkMode" => {
                    s.site_color_scheme = if value.as_bool().context("Ожидался переключатель")?
                    {
                        "dark"
                    } else {
                        "light"
                    }
                    .into()
                }
                "GlassTransparency" => {
                    s.glass_transparency = value
                        .as_u64()
                        .context("Некорректная прозрачность")?
                        .min(100) as u8
                }
                "SidebarWidth" => {
                    let width = value.as_u64().context("Некорректная ширина")?.min(480) as u32;
                    match s.browser_layout.as_str() {
                        "unified" => s.unified_sidebar_width = width,
                        "compact" => s.compact_sidebar_width = width,
                        _ => s.sidebar_width = width,
                    }
                }
                "Privacy" => {
                    s.privacy = serde_json::from_value(value.clone())?;
                }
                "SidebarVisible" => {
                    s.sidebar_visible = value.as_bool().context("Ожидался переключатель")?
                }
                "SavePasswords" => {
                    s.save_passwords = value.as_bool().context("Ожидался переключатель")?
                }
                "ClearCookiesOnExit" => {
                    s.clear_cookies_on_exit = value.as_bool().context("Ожидался переключатель")?
                }
                "ClearCacheOnExit" => {
                    s.clear_cache_on_exit = value.as_bool().context("Ожидался переключатель")?
                }
                "DiscardMinutes" => {
                    s.discard_minutes = value.as_u64().context("Некорректный интервал")? as u32
                }
                "Zoom" => s.zoom = value.as_f64().context("Некорректный масштаб")?,
                "DownloadUnits" => {
                    s.download_units = value.as_str().context("Выберите биты или байты")?.into()
                }
                "Dns" => {
                    let dns: network::DnsSettings = serde_json::from_value(value.clone())?;
                    dns.validate()?;
                    s.dns = dns;
                }
                "Language" => s.language = value.as_str().context("Некорректный язык")?.into(),
                "SearchEngine" => {
                    s.search_engine = value.as_str().context("Выберите поисковую систему")?.into()
                }
                "SiteColorScheme" => {
                    s.site_color_scheme = value.as_str().context("Выберите цветовую схему")?.into()
                }
                "BrowserLayout" => {
                    s.browser_layout = value.as_str().context("Выберите макет")?.into()
                }
                _ => bail!("Неизвестная настройка"),
            }
            s.normalize();
            if commit {
                store.save_settings()?;
            }
        }
        if matches!(
            key,
            "Glass" | "DarkMode" | "Zoom" | "SiteColorScheme" | "Privacy"
        ) {
            let settings = self.store.borrow().settings.clone();
            for w in &self.windows {
                let rules = if w.private {
                    w.private_permissions.borrow().clone()
                } else {
                    settings.privacy.permissions.clone()
                };
                platform::material(&w.window, settings.glass, settings.dark_mode);
                engine::configure(&w.shell, &settings, true)?;
                if let Some(p) = &w.popup {
                    engine::configure(p, &settings, true)?;
                }
                if let Some(p) = &w.popup_cache {
                    engine::configure(p, &settings, true)?;
                }
                if let Some(service) = &w.service {
                    engine::configure(service, &settings, false)?;
                }
                for tab in &w.tabs {
                    if let Some(v) = &tab.view {
                        engine::configure(v, &settings, false)?;
                        protection::apply(
                            v,
                            &settings.privacy,
                            w.private,
                            self.protection.clone(),
                            self.proxy.clone(),
                            w.window.id(),
                        )?;
                        privacy::apply_audio(
                            &v.webview(),
                            &settings.privacy,
                            &rules,
                            &tab.url,
                            tab.muted,
                        )?;
                    }
                }
            }
        }
        if key == "SidebarWidth" && !commit {
            for wi in 0..self.windows.len() {
                self.layout(wi)?;
            }
            return Ok(());
        }
        self.sync_all()
    }
    fn command(
        &mut self,
        wi: usize,
        message: Value,
        target: &EventLoopWindowTarget<Event>,
    ) -> Result<()> {
        let cmd = message["cmd"].as_str().context("Отсутствует команда")?;
        if self.windows[wi].busy && !matches!(cmd, "ready" | "close_menu") {
            return Ok(());
        }
        match cmd {
            "ready" => self.sync(wi)?,
            "dns_save" => {
                let result =
                    serde_json::from_value::<network::DnsSettings>(message["value"].clone())
                        .map_err(anyhow::Error::from)
                        .and_then(|dns| {
                            dns.validate()?;
                            Ok(dns)
                        });
                match result {
                    Ok(dns) => {
                        self.change_setting("Dns", &serde_json::to_value(dns)?, true)?;
                        self.windows[wi].status =
                            "Настройки DNS сохранены. Перезапустите браузер для применения.".into();
                    }
                    Err(error) => self.windows[wi].status = error.to_string(),
                }
                self.sync(wi)?;
            }
            "new_tab" => {
                let existing = self.windows[wi]
                    .tabs
                    .iter()
                    .find(|t| t.url == HOME && t.folder_id.is_none())
                    .map(|t| t.id);
                if let Some(id) = existing {
                    self.select_tab(wi, id)?;
                } else {
                    self.add_tab(wi, HOME.into())?;
                }
                self.windows[wi].shell.focus()?;
                self.windows[wi]
                    .shell
                    .evaluate_script("document.getElementById('home-input')?.focus()")?;
            }
            "extension_store" => {
                anyhow::ensure!(!self.windows[wi].private, "Откройте магазин в обычном окне");
                self.open_window(target, false, true)?;
                let store_window = self.windows.len() - 1;
                self.windows[store_window].window.set_title(&i18n::text(
                    &self.store.borrow().settings.language,
                    "SlimBrowser · Магазин расширений",
                ));
                self.set_view(store_window, Some("extension-store".into()))?;
                self.refresh_catalog()?;
            }
            "store_refresh" => self.refresh_catalog()?,
            "store_prepare" => {
                self.prepare_extension(wi, message["id"].as_str().context("Выберите расширение")?)?
            }
            "store_confirm" => self.confirm_extension(wi)?,
            "store_cancel" => {
                if !self.windows[wi].store_busy {
                    self.windows[wi].pending_install = None;
                    self.windows[wi].status.clear();
                    self.sync(wi)?;
                }
            }
            "store_source" => {
                let source = self
                    .catalog
                    .iter()
                    .find(|p| Some(p.id.as_str()) == message["id"].as_str())
                    .context("Выберите расширение")?
                    .source
                    .clone();
                self.add_tab(wi, source)?;
            }
            "select_tab" => {
                self.select_tab(wi, message["id"].as_u64().context("Неизвестная вкладка")?)?
            }
            "close_tab" => self.close_tab(wi, message["id"].as_u64())?,
            "navigate" => self.navigate(wi, message["value"].as_str().context("Введите адрес")?)?,
            "toggle_sidebar" => {
                let visible = !self.store.borrow().settings.sidebar_visible;
                self.change_setting("SidebarVisible", &json!(visible), true)?;
            }
            "toggle_menu" => self.toggle_menu(wi)?,
            "page_actions" => self.toggle_popup(wi, "page")?,
            "organize" => {
                self.windows[wi].folder_edit = None;
                self.toggle_popup(wi, "organize")?;
            }
            "folder_menu" => {
                self.windows[wi].folder_edit = message["id"].as_u64();
                self.windows[wi].popup_anchor = Some((
                    message["x"].as_f64().unwrap_or(16.),
                    message["y"].as_f64().unwrap_or(150.),
                ));
                self.toggle_popup(wi, "folder-menu")?;
            }
            "folder_edit_inline" => {
                self.windows[wi].folder_rename = message["id"].as_u64();
                self.sync(wi)?;
            }
            "folder_create" | "folder_rename" | "folder_toggle" | "folder_move"
            | "folder_reorder" | "tab_reorder" | "folder_delete" => {
                self.folder_command(wi, cmd, &message)?
            }
            "tab_menu" => {
                self.windows[wi].menu_tab = message["id"].as_u64();
                self.windows[wi].popup_anchor = Some((
                    message["x"].as_f64().unwrap_or(16.),
                    message["y"].as_f64().unwrap_or(150.),
                ));
                self.toggle_popup(wi, "tab-menu")?;
            }
            "tab_folders" => self.toggle_popup(wi, "tab-folders")?,
            "tab_action" => self.tab_action(wi, &message, target)?,
            "translate_menu" => self.toggle_popup(wi, "translate")?,
            "translate" => {
                let ti = self
                    .active_index(wi)
                    .context("Откройте страницу для перевода.")?;
                let language = message["language"]
                    .as_str()
                    .context("Выберите язык перевода.")?;
                let target = model::translation_url(&self.windows[wi].tabs[ti].url, language)?;
                {
                    let mut store = self.store.borrow_mut();
                    store.settings.translation_language = language.into();
                    store.save_settings()?;
                }
                self.navigate(wi, &target)?;
            }
            "copy_link" | "share_page" => {
                let ti = self.active_index(wi).context("Откройте страницу.")?;
                let tab = &self.windows[wi].tabs[ti];
                anyhow::ensure!(model::is_web(&tab.url), "Откройте страницу.");
                if cmd == "copy_link" {
                    clipboard_win::set_clipboard_string(&tab.url)
                        .map_err(|e| anyhow::anyhow!("Буфер обмена: {e}"))?;
                    self.windows[wi].status = "Ссылка скопирована".into();
                    self.sync(wi)?;
                } else {
                    self.windows[wi].share_session = Some(platform::share_page(
                        &self.windows[wi].window,
                        &tab.url,
                        &tab.title,
                    )?);
                }
            }
            "screenshot" => {
                let ti = self
                    .active_index(wi)
                    .context("Откройте страницу для снимка.")?;
                let view = self.windows[wi].tabs[ti]
                    .view
                    .as_ref()
                    .context("Откройте страницу для снимка.")?;
                let name = format!(
                    "Страница-{}.png",
                    chrono::Local::now().format("%Y-%m-%d-%H-%M-%S")
                );
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(i18n::text(
                        &self.store.borrow().settings.language,
                        "Сохранить снимок видимой области страницы",
                    ))
                    .add_filter("PNG", &["png"])
                    .set_file_name(name)
                    .save_file()
                {
                    engine::capture_png(
                        &view.webview(),
                        path,
                        self.proxy.clone(),
                        self.windows[wi].window.id(),
                    )?;
                }
            }
            "close_menu" => {
                self.close_popup(wi)?;
                self.sync(wi)?;
            }
            "new_window" => self.open_window(target, false, false)?,
            "empty_window" => self.open_window(target, false, true)?,
            "private_window" => self.open_window(target, true, false)?,
            "close_window" => self.request_close(wi)?,
            "minimize" => {
                self.finish_window_motion(wi)?;
                self.windows[wi].window.set_minimized(true);
            }
            "maximize" | "fullscreen" => {
                let w = &self.windows[wi];
                let current = w.motion.as_ref().map(|m| m.mode).unwrap_or_else(|| {
                    if w.window.fullscreen().is_some() {
                        WindowMode::Fullscreen
                    } else if w.window.is_maximized() {
                        WindowMode::Maximized
                    } else {
                        WindowMode::Normal
                    }
                });
                let mode = if cmd == "maximize" {
                    if current == WindowMode::Maximized {
                        WindowMode::Normal
                    } else {
                        WindowMode::Maximized
                    }
                } else if current == WindowMode::Fullscreen {
                    w.windowed_mode
                } else {
                    WindowMode::Fullscreen
                };
                self.animate_window(wi, mode)?;
            }
            "drag" => {
                let _ = self.windows[wi].window.drag_window();
            }
            "resize" => {
                let dir = match message["value"].as_str().unwrap_or("") {
                    "n" => ResizeDirection::North,
                    "s" => ResizeDirection::South,
                    "e" => ResizeDirection::East,
                    "w" => ResizeDirection::West,
                    "ne" => ResizeDirection::NorthEast,
                    "nw" => ResizeDirection::NorthWest,
                    "se" => ResizeDirection::SouthEast,
                    _ => ResizeDirection::SouthWest,
                };
                let _ = self.windows[wi].window.drag_resize_window(dir);
            }
            "view" => {
                let view = message["value"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned);
                self.set_view(wi, view)?;
            }
            "focus_address" => {
                if self.store.borrow().settings.browser_layout == "unified"
                    && !self.store.borrow().settings.sidebar_visible
                {
                    self.change_setting("SidebarVisible", &json!(true), true)?;
                }
                self.windows[wi].shell.focus()?;
                self.windows[wi]
                    .shell
                    .evaluate_script("window.focusAddress?.()")?;
            }
            "back" | "forward" | "reload" => {
                if let Some(ti) = self.active_index(wi) {
                    if let Some(v) = &self.windows[wi].tabs[ti].view {
                        match cmd {
                            "back" => v.go_back()?,
                            "forward" => v.go_forward()?,
                            _ => {
                                if self.windows[wi].tabs[ti].loading {
                                    unsafe {
                                        v.webview().Stop()?;
                                    }
                                } else {
                                    v.reload()?;
                                }
                            }
                        }
                    } else {
                        self.ensure_active(wi)?;
                    }
                }
            }
            "escape" => {
                let had_popup = self.close_popup(wi)?;
                if had_popup {
                    self.sync(wi)?;
                } else if self.windows[wi].view.is_some() {
                    self.set_view(wi, None)?;
                } else if let Some(ti) = self.active_index(wi) {
                    if let Some(v) = &self.windows[wi].tabs[ti].view {
                        unsafe {
                            v.webview().Stop()?;
                        }
                    }
                }
            }
            "setting" => self.change_setting(
                message["key"].as_str().context("Отсутствует настройка")?,
                &message["value"],
                message["commit"].as_bool().unwrap_or(true),
            )?,
            "bookmark" => {
                if let Some(ti) = self.active_index(wi) {
                    let t = &self.windows[wi].tabs[ti];
                    self.store.borrow_mut().add_entry(true, &t.title, &t.url)?;
                    self.windows[wi].status = "Страница добавлена в закладки".into();
                }
                self.sync_all()?;
            }
            "clear_library" => {
                let bookmarks = message["value"] == "bookmarks";
                if message["confirmed"] == true {
                    if !bookmarks && self.windows[wi].private {
                        self.windows[wi].private_history.clear();
                    } else {
                        let mut store = self.store.borrow_mut();
                        if bookmarks {
                            store.bookmarks.clear()
                        } else {
                            store.history.clear()
                        }
                        store.save_library(bookmarks)?;
                    }
                    self.sync_all()?;
                }
            }
            "delete_bookmark" => {
                let url = message["url"].as_str().context("Выберите закладку")?;
                let mut store = self.store.borrow_mut();
                store.bookmarks.retain(|entry| entry.url != url);
                store.save_library(true)?;
                drop(store);
                self.sync_all()?;
            }
            "clean" => self.start_cleanup(wi, false, true, CleanupPurpose::Manual)?,
            "clear_data" => {
                if platform::confirm(
                    &self.windows[wi].window,
                    &i18n::text(
                        &self.store.borrow().settings.language,
                        "Очистить cookies и кэш? Сайты могут запросить повторный вход.",
                    ),
                ) {
                    self.start_cleanup(wi, true, true, CleanupPurpose::Manual)?;
                }
            }
            "download_folder" => {
                if let Some(folder) = rfd::FileDialog::new()
                    .set_title(i18n::text(
                        &self.store.borrow().settings.language,
                        "Папка загрузок",
                    ))
                    .pick_folder()
                {
                    let mut store = self.store.borrow_mut();
                    store.settings.download_folder = folder;
                    store.save_settings()?;
                    drop(store);
                    self.sync_all()?;
                }
            }
            "open_downloads" => {
                use std::os::windows::process::CommandExt;
                let path = self.store.borrow().settings.download_folder.clone();
                std::fs::create_dir_all(&path)?;
                std::process::Command::new("explorer.exe")
                    .arg(path)
                    .creation_flags(0x08000000)
                    .spawn()?;
            }
            "downloads" => self.toggle_popup(wi, "downloads")?,
            "download_action" => {
                let id = message["id"].as_u64().context("Выберите загрузку")?;
                let transfer = self.windows[wi]
                    .transfers
                    .iter_mut()
                    .find(|d| d.info.id == id)
                    .context("Загрузка не найдена")?;
                unsafe {
                    match message["action"].as_str() {
                        Some("pause") => transfer.operation.Pause()?,
                        Some("resume") => transfer.operation.Resume()?,
                        Some("cancel") => transfer.operation.Cancel()?,
                        _ => bail!("Неизвестное действие"),
                    }
                }
                transfer.sample()?;
                self.sync(wi)?;
            }
            "download_reveal" => {
                use std::os::windows::process::CommandExt;
                let id = message["id"].as_u64().context("Выберите загрузку")?;
                let transfer = self.windows[wi]
                    .transfers
                    .iter()
                    .find(|d| d.info.id == id)
                    .context("Загрузка не найдена")?;
                std::process::Command::new("explorer.exe")
                    .arg("/select,")
                    .arg(&transfer.path)
                    .creation_flags(0x08000000)
                    .spawn()?;
            }
            "save_password" => {
                let item: Credential = serde_json::from_value(message["value"].clone())?;
                self.store.borrow().save_credential(item)?;
                self.windows[wi].status = "Пароль сохранён".into();
                self.sync(wi)?;
            }
            "copy_password" | "delete_password" => {
                let origin = message["origin"].as_str().context("Выберите запись")?;
                let username = message["username"].as_str().context("Выберите запись")?;
                let mut items = self.store.borrow().credentials()?;
                if let Some(index) = items
                    .iter()
                    .position(|c| c.origin == origin && c.username == username)
                {
                    if cmd == "copy_password" {
                        clipboard_win::set_clipboard_string(&items[index].password)
                            .map_err(|e| anyhow::anyhow!("Буфер обмена: {e}"))?;
                        self.windows[wi].status = "Пароль скопирован в буфер обмена".into();
                    } else if platform::confirm(
                        &self.windows[wi].window,
                        &i18n::text(
                            &self.store.borrow().settings.language,
                            "Удалить пароль для {0}?",
                        )
                        .replace("{0}", origin),
                    ) {
                        items.remove(index);
                        self.store.borrow().save_credentials(&items)?;
                    }
                }
                self.sync(wi)?;
            }
            "extension_open" => {
                let extension_id = message["id"].as_str().context("Выберите расширение")?;
                self.windows[wi]
                    .extensions
                    .iter()
                    .find(|x| x.id == extension_id && x.enabled)
                    .context("Включите расширение перед открытием")?;
                let url = extension_store::popup_url(&self.store.borrow().directory, extension_id)?;
                let source_tab = self.windows[wi].active.unwrap_or(0);
                self.close_popup(wi)?;
                self.windows[wi].extension_size = extension_store::popup_dimensions(extension_id);
                self.windows[wi].popup_kind = "extension".into();
                self.windows[wi].popup_anchor = None;
                let allowed_extension = extension_id.to_owned();
                let view = self
                    .content_builder(wi)
                    .with_navigation_handler(move |url| {
                        url == "about:blank"
                            || model::is_web(&url)
                            || url::Url::parse(&url).is_ok_and(|u| {
                                u.scheme() == "chrome-extension"
                                    && u.host_str() == Some(allowed_extension.as_str())
                            })
                    })
                    .with_initialization_script(extension_store::popup_style_script(extension_id))
                    .with_bounds(self.popup_bounds(
                        wi,
                        self.windows[wi].extension_size.0,
                        self.windows[wi].extension_size.1,
                    ))
                    .with_url(&url)
                    .with_focused(true)
                    .build_as_child(&self.windows[wi].window)?;
                engine::configure(&view, &self.store.borrow().settings, false)?;
                unsafe {
                    view.controller().SetZoomFactor(1.0)?;
                    view.webview().Settings()?.SetIsWebMessageEnabled(true)?;
                }
                engine::extension_popup_hooks(
                    &view,
                    self.windows[wi].window.id(),
                    source_tab,
                    self.proxy.clone(),
                )?;
                self.windows[wi].popup = Some(view);
                self.sync(wi)?;
            }
            "default_browser" => {
                os_integration::show_default_settings()?;
            }
            "reload_all" => {
                for w in &self.windows {
                    for t in &w.tabs {
                        if let Some(v) = &t.view {
                            v.reload()?;
                        }
                    }
                }
            }
            "extensions_refresh" => self.load_extensions(wi)?,
            "extension_install" | "extension_toggle" | "extension_remove" => self
                .extension_action(
                    wi,
                    cmd,
                    message["id"].as_str().unwrap_or_default(),
                    message["confirmed"] == true,
                )?,
            "permission_save" => {
                let kind = message["kind"].as_str().context("Выберите разрешение")?;
                anyhow::ensure!(
                    privacy::CATEGORIES
                        .iter()
                        .any(|(k, _, supported)| *k == kind && *supported),
                    "Это разрешение недоступно в движке"
                );
                let mut rules: Vec<privacy::PermissionRule> =
                    serde_json::from_value(message["rules"].clone())?;
                for rule in &mut rules {
                    rule.origin = privacy::origin(&rule.origin)?;
                    anyhow::ensure!(
                        rule.kind == kind && ["allow", "deny"].contains(&rule.state.as_str()),
                        "Некорректное правило"
                    );
                }
                if self.windows[wi].private {
                    let mut saved = self.windows[wi].private_permissions.borrow_mut();
                    saved.retain(|r| r.kind != kind);
                    saved.extend(rules);
                } else {
                    let mut store = self.store.borrow_mut();
                    store
                        .settings
                        .privacy
                        .permissions
                        .retain(|r| r.kind != kind);
                    store.settings.privacy.permissions.extend(rules);
                    store.save_settings()?;
                }
                let mut policy = self.store.borrow().settings.privacy.clone();
                policy.blocked_permissions.retain(|k| k != kind);
                if message["blocked"] == true {
                    policy.blocked_permissions.push(kind.into());
                }
                self.change_setting("Privacy", &serde_json::to_value(policy)?, true)?;
                self.reset_permissions(wi, Some(kind))?;
                self.windows[wi].status = "Разрешения сохранены".into();
                self.sync_all()?;
            }
            "site_data_refresh" => self.refresh_site_data(wi)?,
            "site_data_clear" => {
                anyhow::ensure!(
                    message["confirmed"] == true,
                    "Подтвердите удаление данных сайта"
                );
                let site = privacy::origin(message["origin"].as_str().context("Выберите сайт")?)?;
                let core = self.service(wi)?;
                privacy::clear_origin(
                    &core,
                    &site,
                    self.proxy.clone(),
                    self.windows[wi].window.id(),
                )?;
            }
            "browsing_clear" => {
                anyhow::ensure!(message["confirmed"] == true, "Подтвердите удаление данных");
                let hours = message["hours"].as_u64().unwrap_or(0).min(24 * 365);
                let since = if hours == 0 {
                    0.
                } else {
                    (chrono::Utc::now().timestamp() - hours as i64 * 3600) as f64
                };
                let mut kinds = COREWEBVIEW2_BROWSING_DATA_KINDS(0);
                if message["cookies"] == true {
                    kinds.0 |= COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0
                        | COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0;
                }
                if message["cache"] == true {
                    kinds.0 |= engine::cleanup_kinds(false, true).0;
                }
                if message["history"] == true {
                    kinds.0 |= COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0;
                }
                anyhow::ensure!(kinds.0 != 0, "Выберите данные для удаления");
                let core = self.service(wi)?;
                privacy::clear_data(
                    &core,
                    kinds,
                    since,
                    message["history"] == true,
                    self.proxy.clone(),
                    self.windows[wi].window.id(),
                )?;
                self.windows[wi].busy = true;
                self.windows[wi].status = "Удаляем выбранные данные…".into();
                self.sync(wi)?;
            }
            "reset_permissions" => {
                if self.windows[wi].private {
                    self.windows[wi].private_permissions.borrow_mut().clear();
                } else {
                    let mut store = self.store.borrow_mut();
                    store.settings.privacy.permissions.clear();
                    store.save_settings()?;
                }
                self.reset_permissions(wi, None)?;
                self.sync_all()?;
            }
            _ => bail!("Неизвестная команда: {cmd}"),
        }
        Ok(())
    }
    fn measure_storage(&self, wi: usize) {
        if self.windows[wi].private {
            return;
        }
        let root = self
            .store
            .borrow()
            .directory
            .join("WebView2/EBWebView")
            .join(&self.windows[wi].profile_name);
        let proxy = self.proxy.clone();
        let id = self.windows[wi].window.id();
        std::thread::spawn(move || {
            let _ = proxy.send_event(Event::StorageSize(id, privacy::storage_size(&root)));
        });
    }
    fn refresh_site_data(&mut self, wi: usize) -> Result<()> {
        let mut known = self.windows[wi]
            .tabs
            .iter()
            .map(|t| t.url.clone())
            .collect::<Vec<_>>();
        if !self.windows[wi].private {
            known.extend(self.store.borrow().history.iter().map(|h| h.url.clone()));
        }
        let core = self.service(wi)?;
        privacy::site_data(
            &core,
            known,
            self.proxy.clone(),
            self.windows[wi].window.id(),
        )
    }
    fn import_permissions(&mut self, wi: usize) -> Result<()> {
        let profile = engine::profile(&self.service(wi)?)?.cast::<ICoreWebView2Profile4>()?;
        let proxy = self.proxy.clone();
        let id = self.windows[wi].window.id();
        unsafe {
            profile.GetNonDefaultPermissionSettings(
                &GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
                    move |result, list| {
                        result?;
                        let mut rules = vec![];
                        if let Some(list) = list {
                            let mut count = 0;
                            list.Count(&mut count)?;
                            for i in 0..count {
                                let setting = list.GetValueAtIndex(i)?;
                                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                                setting.PermissionKind(&mut kind)?;
                                let mut state = COREWEBVIEW2_PERMISSION_STATE::default();
                                setting.PermissionState(&mut state)?;
                                if state == COREWEBVIEW2_PERMISSION_STATE_DEFAULT {
                                    continue;
                                }
                                let origin = engine::string(|p| setting.PermissionOrigin(p))?;
                                let kind = privacy::category(kind).to_owned();
                                if privacy::origin(&origin).is_ok() && kind != "unknown" {
                                    rules.push(privacy::PermissionRule {
                                        origin,
                                        kind,
                                        state: if state == COREWEBVIEW2_PERMISSION_STATE_ALLOW {
                                            "allow"
                                        } else {
                                            "deny"
                                        }
                                        .into(),
                                    });
                                }
                            }
                        }
                        let _ = proxy.send_event(Event::NativePermissions(id, rules));
                        Ok(())
                    },
                )),
            )?;
        }
        Ok(())
    }
    fn reset_permissions(&mut self, wi: usize, only: Option<&str>) -> Result<()> {
        let only = only.map(str::to_owned);
        let profile = engine::profile(&self.service(wi)?)?.cast::<ICoreWebView2Profile4>()?;
        let p = self.proxy.clone();
        let id = self.windows[wi].window.id();
        let keep = profile.clone();
        unsafe {
            profile.GetNonDefaultPermissionSettings(
                &GetNonDefaultPermissionSettingsCompletedHandler::create(Box::new(
                    move |result, list| {
                        result?;
                        if let Some(list) = list {
                            let mut count = 0;
                            list.Count(&mut count)?;
                            for i in 0..count {
                                let setting = list.GetValueAtIndex(i)?;
                                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                                setting.PermissionKind(&mut kind)?;
                                if only
                                    .as_ref()
                                    .is_some_and(|key| key != privacy::category(kind))
                                {
                                    continue;
                                }
                                let origin = engine::string(|v| setting.PermissionOrigin(v))?;
                                let p = p.clone();
                                keep.SetPermissionState(
                                    kind,
                                    &HSTRING::from(origin),
                                    COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
                                    &SetPermissionStateCompletedHandler::create(Box::new(
                                        move |result| {
                                            let _ = p.send_event(Event::Notice(
                                                id,
                                                match result {
                                                    Ok(_) => "Разрешения сброшены".into(),
                                                    Err(e) => e.to_string(),
                                                },
                                            ));
                                            Ok(())
                                        },
                                    )),
                                )?;
                            }
                        }
                        let _ = p.send_event(Event::Notice(id, "Разрешения сброшены".into()));
                        Ok(())
                    },
                )),
            )?;
        }
        Ok(())
    }
    fn handle(&mut self, event: Event, target: &EventLoopWindowTarget<Event>) -> Result<()> {
        match event {
            Event::Ipc(id, popup, text) => {
                if let Some(wi) = self.index(id) {
                    let message: Value = serde_json::from_str(&text)?;
                    if popup
                        && !matches!(message["cmd"].as_str(), Some("ready" | "download_action"))
                    {
                        self.close_popup(wi)?;
                    }
                    let ready = message["cmd"] == "ready" && !popup;
                    let popup_ready = message["cmd"] == "ready" && popup;
                    self.command(wi, message, target)?;
                    if popup_ready {
                        self.popup_ready(wi)?;
                        if self.windows[wi].popup.is_some()
                            && self.smoke.as_ref().is_some_and(|s| s.phase == 50)
                        {
                            self.smoke_menu_ready(wi)?;
                        } else if self.windows[wi].popup.is_some()
                            && self.smoke.as_ref().is_some_and(|s| s.phase == 51)
                        {
                            self.smoke_wide_menu_ready(wi)?;
                        } else if self.windows[wi].popup.is_some()
                            && self
                                .smoke
                                .as_ref()
                                .is_some_and(|s| matches!(s.phase, 80..=82))
                        {
                            self.smoke_refinement_menu_ready(wi)?;
                        }
                    }
                    if ready && self.smoke.is_none() {
                        let _ = self.proxy.send_event(Event::WarmPopup(id));
                    }
                    if ready && self.smoke.as_ref().is_some_and(|s| s.phase == 0) {
                        self.begin_smoke()?;
                    }
                }
            }
            Event::ExtensionOpen(id, opener, url) => {
                if let Some(wi) = self.index(id) {
                    let folder = self.windows[wi]
                        .tabs
                        .iter()
                        .find(|t| t.id == opener)
                        .and_then(|t| t.folder_id);
                    self.close_popup(wi)?;
                    self.add_tab(wi, url)?;
                    if let Some(ti) = self.active_index(wi) {
                        self.windows[wi].tabs[ti].folder_id = folder;
                    }
                    self.sync(wi)?;
                }
            }
            Event::ExternalOpen(urls) => {
                let wi = if let Some(wi) = self.windows.iter().position(|w| !w.private) {
                    wi
                } else {
                    self.open_window(target, false, false)?;
                    self.windows.len() - 1
                };
                for url in urls {
                    self.add_tab(wi, url)?;
                }
                self.windows[wi].window.set_minimized(false);
                self.windows[wi].window.set_focus();
            }
            Event::Key(id, cmd) => {
                if let Some(wi) = self.index(id) {
                    self.command(wi, json!({"cmd":cmd}), target)?;
                }
            }
            Event::NewTab(id, opener, url) => {
                if let Some(wi) = self.index(id) {
                    if model::is_web(&url) {
                        let folder = self.windows[wi]
                            .tabs
                            .iter()
                            .find(|t| t.id == opener)
                            .and_then(|t| t.folder_id);
                        self.add_tab(wi, url)?;
                        let ti = self.active_index(wi).context("Вкладка не создана")?;
                        self.windows[wi].tabs[ti].folder_id = folder;
                        if let Some(folder) = folder {
                            let mut store = self.store.borrow_mut();
                            let folders = if self.windows[wi].private {
                                &mut self.windows[wi].private_folders
                            } else {
                                &mut store.folders
                            };
                            if let Some(f) = folders.iter_mut().find(|f| f.id == folder) {
                                f.collapsed = false;
                            }
                        }
                        if !self.windows[wi].private {
                            self.save_folder_tabs()?;
                        }
                        self.sync(wi)?;
                    }
                }
            }
            Event::NativePermissions(id, incoming) => {
                if let Some(wi) = self.index(id) {
                    if self.windows[wi].private {
                        let mut rules = self.windows[wi].private_permissions.borrow_mut();
                        for rule in incoming {
                            if !rules
                                .iter()
                                .any(|r| r.kind == rule.kind && r.origin == rule.origin)
                            {
                                rules.push(rule);
                            }
                        }
                    } else {
                        let mut store = self.store.borrow_mut();
                        for rule in incoming {
                            if !store
                                .settings
                                .privacy
                                .permissions
                                .iter()
                                .any(|r| r.kind == rule.kind && r.origin == rule.origin)
                            {
                                store.settings.privacy.permissions.push(rule);
                            }
                        }
                        store.save_settings()?;
                    }
                    self.sync(wi)?;
                }
            }
            Event::CosmeticNodes(id, tab, url, value) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi]
                        .tabs
                        .iter()
                        .find(|t| t.id == tab && t.url == url)
                    {
                        if let Some(view) = &t.view {
                            let data = serde_json::from_str::<String>(&value)
                                .ok()
                                .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                                .unwrap_or_default();
                            let get = |key: &str| {
                                data[key]
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|v| v.as_str().map(str::to_owned))
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default()
                            };
                            view.evaluate_script(&self.protection.cosmetic_script(
                                &self.store.borrow().settings.privacy,
                                &url,
                                &get("classes"),
                                &get("ids"),
                            ))?;
                        }
                    }
                }
            }
            Event::StorageSize(id, bytes) => {
                if let Some(wi) = self.index(id) {
                    self.windows[wi].storage_bytes = Some(bytes);
                    self.sync(wi)?;
                }
            }
            Event::PermissionDecision(id, rule) => {
                if let Some(wi) = self.index(id) {
                    if self.windows[wi].private {
                        privacy::set_rule(
                            &mut self.windows[wi].private_permissions.borrow_mut(),
                            rule,
                        );
                    } else {
                        let mut store = self.store.borrow_mut();
                        privacy::set_rule(&mut store.settings.privacy.permissions, rule);
                        store.save_settings()?;
                    }
                    self.sync_all()?;
                }
            }
            Event::SiteData(id, result) => {
                if let Some(wi) = self.index(id) {
                    match result {
                        Ok(sites) => self.windows[wi].site_data = sites,
                        Err(e) => self.windows[wi].status = e,
                    }
                    self.sync(wi)?;
                }
            }
            Event::SiteDataCleared(id, result) => {
                if let Some(wi) = self.index(id) {
                    if self.smoke.as_ref().is_some_and(|s| s.phase == 90) {
                        let _ = self.proxy.send_event(Event::SmokeResult(
                            "CDP removes selected site data without clearing other sites".into(),
                            result.is_ok(),
                        ));
                    }
                    self.windows[wi].status = match result { Ok(()) => "Данные выбранного сайта удалены. Открытая страница может записать их снова.".into(), Err(e) => format!("Не удалось удалить данные: {e}") };
                    self.refresh_site_data(wi)?;
                    self.measure_storage(wi);
                    self.sync(wi)?;
                }
            }
            Event::BrowsingCleared(id, since, history, result) => {
                if let Some(wi) = self.index(id) {
                    self.windows[wi].busy = false;
                    match result {
                        Ok(()) => {
                            if history {
                                let retain = |h: &SiteEntry| {
                                    chrono::DateTime::parse_from_rfc3339(&h.visited)
                                        .is_ok_and(|d| (d.timestamp() as f64) < since)
                                };
                                if self.windows[wi].private {
                                    self.windows[wi].private_history.retain(retain);
                                } else {
                                    let mut store = self.store.borrow_mut();
                                    store.history.retain(retain);
                                    store.save_library(false)?;
                                }
                            }
                            self.measure_storage(wi);
                            self.windows[wi].status = "Выбранные данные удалены".into();
                            self.refresh_site_data(wi)?;
                        }
                        Err(e) => {
                            self.windows[wi].status = format!("Не удалось удалить данные: {e}")
                        }
                    }
                    self.sync_all()?;
                }
            }
            Event::Title(id, tab, title) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi].tabs.iter_mut().find(|t| t.id == tab) {
                        t.title = title;
                    }
                    self.sync(wi)?;
                    if !self.windows[wi].private {
                        self.save_folder_tabs()?;
                    }
                }
            }
            Event::Favicon(id, tab, favicon) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi].tabs.iter_mut().find(|t| t.id == tab) {
                        t.favicon = favicon;
                    }
                    self.sync(wi)?;
                }
            }
            Event::Page(id, tab, started, url) => {
                if url == "about:blank" {
                    return Ok(());
                }
                if let Some(wi) = self.index(id) {
                    if let Some(ti) = self.windows[wi].tabs.iter().position(|t| t.id == tab) {
                        self.windows[wi].tabs[ti].loading = started;
                        if model::is_web(&url) {
                            self.windows[wi].tabs[ti].url = url.clone();
                        }
                        if let Some(view) = &self.windows[wi].tabs[ti].view {
                            let policy = self.store.borrow().settings.privacy.clone();
                            let rules = if self.windows[wi].private {
                                self.windows[wi].private_permissions.borrow().clone()
                            } else {
                                policy.permissions.clone()
                            };
                            privacy::apply_audio(
                                &view.webview(),
                                &policy,
                                &rules,
                                &url,
                                self.windows[wi].tabs[ti].muted,
                            )?;
                        }
                        if !started {
                            if self.windows[wi].active == Some(tab) {
                                self.windows[wi].status.clear();
                            }
                            let title = self.windows[wi].tabs[ti].title.clone();
                            if self.windows[wi].private
                                && self.store.borrow().settings.privacy.remembers_history()
                            {
                                model::add_entry(
                                    &mut self.windows[wi].private_history,
                                    &title,
                                    &url,
                                    1000,
                                );
                            } else if self.store.borrow().settings.privacy.remembers_history() {
                                self.store.borrow_mut().add_entry(false, &title, &url)?;
                            }
                            if !self.store.borrow().settings.privacy.remembers_history() {
                                if let Some(view) = &self.windows[wi].tabs[ti].view {
                                    privacy::forget_engine_history(
                                        &view.webview(),
                                        self.proxy.clone(),
                                        self.windows[wi].window.id(),
                                    )?;
                                }
                            }
                            let guard = {
                                let p = &self.store.borrow().settings.privacy;
                                p.tracking_mode == "strict"
                                    || (p.tracking_mode == "custom"
                                        && p.suspected_fingerprints
                                        && (self.windows[wi].private
                                            || p.fingerprinting_scope == "always"))
                            };
                            if guard {
                                if let Some(view) = &self.windows[wi].tabs[ti].view {
                                    view.evaluate_script(include_str!(
                                        "../ui/fingerprint-guard.js"
                                    ))?;
                                }
                            }
                            if let Some(view) = &self.windows[wi].tabs[ti].view {
                                let p = self.proxy.clone();
                                let id = self.windows[wi].window.id();
                                let expected = url.clone();
                                view.evaluate_script_with_callback("JSON.stringify({classes:[...new Set([...document.querySelectorAll('[class]')].flatMap(n=>[...n.classList]))].slice(0,10000),ids:[...document.querySelectorAll('[id]')].map(n=>n.id).slice(0,10000)})",move|value|{let _=p.send_event(Event::CosmeticNodes(id,tab,expected.clone(),value));})?;
                            }
                            self.autofill(wi, ti)?;
                            self.smoke_page(wi, ti)?;
                        }
                        self.sync(wi)?;
                    }
                }
            }
            Event::NavigationFailed(id, tab, error) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi].tabs.iter_mut().find(|t| t.id == tab) {
                        t.loading = false;
                    }
                    if self.windows[wi].active == Some(tab) {
                        if let Some(error) = error {
                            self.windows[wi].status = error;
                        }
                    }
                    self.sync(wi)?;
                }
            }
            Event::Refresh(id, tab) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi].tabs.iter_mut().find(|t| t.id == tab) {
                        if let Some(v) = &t.view {
                            let url = unsafe { engine::string(|p| v.webview().Source(p))? };
                            if model::is_web(&url) {
                                t.url = url;
                            }
                        }
                    }
                    self.sync(wi)?;
                }
            }
            Event::Notice(id, message) => {
                if let Some(wi) = self.index(id) {
                    self.windows[wi].status = message;
                    self.sync(wi)?;
                }
            }
            Event::WarmPopup(id) => {
                if let Some(wi) = self.index(id) {
                    self.warm_popup(wi)?;
                }
            }
            Event::PopupLostFocus(id) => {
                if let Some(wi) = self.index(id) {
                    if self.close_popup(wi)? {
                        self.sync(wi)?;
                    }
                }
            }
            Event::Download(id, tab, started, message) => {
                if let Some(wi) = self.index(id) {
                    if let Some(t) = self.windows[wi].tabs.iter_mut().find(|t| t.id == tab) {
                        if started {
                            t.downloads += 1
                        } else {
                            t.downloads = t.downloads.saturating_sub(1)
                        }
                    }
                    self.windows[wi].status = if started {
                        "Загрузка файла…".into()
                    } else {
                        message
                    };
                    self.sync(wi)?;
                }
            }
            Event::DownloadStarted(id, tab, pending) => {
                let operation = downloads::take_pending(pending).context("Загрузка не найдена")?;
                if let Some(wi) = self.index(id) {
                    let transfer = downloads::Transfer::new(self.next_download, tab, operation)?;
                    self.next_download += 1;
                    self.windows[wi].transfers.push(transfer);
                    if self.windows[wi].popup.is_none()
                        || self.windows[wi].popup_kind != "downloads"
                    {
                        self.toggle_popup(wi, "downloads")?;
                    } else {
                        self.layout(wi)?;
                        self.sync(wi)?;
                    }
                }
            }
            Event::CleanupDone(id, purpose, result) => {
                if let Some(wi) = self.index(id) {
                    self.windows[wi].busy = false;
                    match result {
                        Ok(()) => match purpose {
                            CleanupPurpose::Exit => {
                                if self.store.borrow().settings.privacy.clears_history() {
                                    let mut store = self.store.borrow_mut();
                                    store.history.clear();
                                    store.save_library(false)?;
                                }
                                if self.smoke.is_some() {
                                    self.check(
                                        true,
                                        "Exit cleanup completes before window destruction",
                                    )?;
                                    self.finish_smoke(None);
                                } else {
                                    platform::close_animation(&self.windows[wi].window);
                                    self.windows.remove(wi);
                                }
                            }
                            CleanupPurpose::Manual => {
                                self.windows[wi].status =
                                    "Очистка завершена, неактивные вкладки выгружены".into();
                                self.sync(wi)?;
                            }
                            CleanupPurpose::Smoke => self.smoke_cleanup_done(wi)?,
                        },
                        Err(error) => {
                            self.windows[wi].status =
                                format!("Не удалось очистить данные: {error}");
                            self.ensure_active(wi)?;
                            self.layout(wi)?;
                            self.sync(wi)?;
                            if self.smoke.is_some() {
                                self.finish_smoke(Some(error));
                            }
                        }
                    }
                }
            }
            Event::Extensions(id, result) => {
                if let Some(wi) = self.index(id) {
                    let mut entries = result.map_err(anyhow::Error::msg)?;
                    for x in &mut entries {
                        x.icon = extension_store::icon(&self.store.borrow().directory, &x.id);
                    }
                    if entries
                        .iter()
                        .any(|x| x.id == "cfhdojbkjhnklbpkdaibdccddilifddb")
                    {
                        self.extension_action(
                            wi,
                            "extension_remove",
                            "cfhdojbkjhnklbpkdaibdccddilifddb",
                            true,
                        )?;
                    }
                    entries.retain(|x| x.id != "cfhdojbkjhnklbpkdaibdccddilifddb");
                    self.windows[wi].extensions = entries;
                    self.sync(wi)?;
                }
            }
            Event::ExtensionChanged(id, result) => {
                result.map_err(anyhow::Error::msg)?;
                if let Some(wi) = self.index(id) {
                    self.load_extensions(wi)?;
                }
            }
            Event::SmokeResult(name, value) => self.smoke_result(&name, value, target)?,
            Event::Catalog(packages) => {
                self.catalog = packages;
                self.catalog_loading = false;
                self.sync_all()?;
            }
            Event::PackagePrepared(id, result) => {
                if let Some(wi) = self.index(id) {
                    self.windows[wi].store_busy = false;
                    match result {
                        Ok(pending) => {
                            self.windows[wi].pending_install = Some(pending);
                            self.windows[wi].status.clear();
                        }
                        Err(error) => {
                            self.windows[wi].status =
                                format!("Не удалось загрузить расширение: {error}")
                        }
                    }
                    self.sync(wi)?;
                }
            }
            Event::StoreInstalled(id, result) => {
                let success = result.is_ok();
                if let Some(wi) = self.index(id) {
                    self.windows[wi].store_busy = false;
                    match result {
                        Ok(extension_id) => {
                            if let Some(pending) = &self.windows[wi].pending_install {
                                extension_store::register(
                                    &self.store.borrow().directory,
                                    &extension_id,
                                    pending,
                                )?;
                            }
                            self.windows[wi].pending_install = None;
                            self.windows[wi].status = "Расширение установлено".into();
                            self.load_extensions(wi)?;
                        }
                        Err(error) => {
                            self.windows[wi].status =
                                format!("WebView2 не установил расширение: {error}")
                        }
                    }
                    self.sync_all()?;
                    if self.smoke.as_ref().is_some_and(|s| s.phase == 83) {
                        let _ = self.proxy.send_event(Event::SmokeResult(
                            "Confirmed store package installs through WebView2".into(),
                            success,
                        ));
                    }
                }
            }
            Event::ScreenshotDone(id, path, result) => {
                result.map_err(anyhow::Error::msg)?;
                if self.smoke.as_ref().is_some_and(|s| s.phase == 60) {
                    self.smoke_screenshot_done(&path)?;
                } else if let Some(wi) = self.index(id) {
                    self.windows[wi].status = format!("Снимок сохранён: {}", path.display());
                    self.sync(wi)?;
                }
            }
        }
        Ok(())
    }
    fn autofill(&self, wi: usize, ti: usize) -> Result<()> {
        if self.windows[wi].private || !self.store.borrow().settings.save_passwords {
            return Ok(());
        }
        let origin = model::secure_origin(&self.windows[wi].tabs[ti].url);
        let Some(origin) = origin else { return Ok(()) };
        let credentials = self.store.borrow().credentials()?;
        let matching = credentials
            .iter()
            .filter(|c| c.origin == origin)
            .collect::<Vec<_>>();
        if matching.len() == 1 {
            if let Some(view) = &self.windows[wi].tabs[ti].view {
                view.evaluate_script(&model::autofill_script(matching[0])?)?;
            }
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<()> {
        self.next_tick = Instant::now() + Duration::from_secs(1);
        let blocked = self.protection.blocked.get();
        if blocked != self.last_blocked {
            self.last_blocked = blocked;
            self.sync_all()?;
        }
        let update_theme = {
            let store = self.store.borrow();
            store.settings.site_color_scheme == "system"
                && store.settings.dark_mode != platform::system_dark()
        };
        if update_theme {
            self.change_setting("SiteColorScheme", &json!("system"), false)?;
        }
        for wi in 0..self.windows.len() {
            let mut changed = false;
            for transfer in &mut self.windows[wi].transfers {
                if transfer.info.state == "progress" {
                    transfer.sample()?;
                    changed = true;
                }
            }
            if changed {
                let window = &mut self.windows[wi];
                for tab in &mut window.tabs {
                    tab.downloads = window
                        .transfers
                        .iter()
                        .filter(|d| {
                            d.tab == tab.id && (d.info.state == "progress" || d.info.can_resume)
                        })
                        .count();
                }
            }
            if changed {
                self.sync(wi)?;
            }
        }
        let minutes = self.store.borrow().settings.discard_minutes;
        if self.smoke.as_ref().is_some_and(|s| s.phase == 92)
            && self.windows[0].window.is_maximized()
        {
            self.smoke.as_mut().unwrap().phase = 94;
            let valid = platform::matches_monitor_area(&self.windows[0].window, false)?;
            let _ = self.proxy.send_event(Event::SmokeResult(
                "Maximized window keeps Windows work area".into(),
                valid,
            ));
        } else if self.smoke.as_ref().is_some_and(|s| s.phase == 93)
            && self.windows[0].window.fullscreen().is_some()
        {
            self.smoke.as_mut().unwrap().phase = 95;
            let valid = platform::matches_monitor_area(&self.windows[0].window, true)?;
            let _ = self.proxy.send_event(Event::SmokeResult(
                "Fullscreen remains separate from maximization".into(),
                valid,
            ));
        }
        if self.smoke.as_ref().is_some_and(|s| s.phase == 96) {
            if let Some(x) = self.windows[0]
                .extensions
                .iter()
                .find(|x| x.name == "Store fixture")
            {
                let extension = x.id.clone();
                self.smoke.as_mut().unwrap().phase = 97;
                let _ = self.proxy.send_event(Event::Ipc(
                    self.windows[0].window.id(),
                    false,
                    json!({"cmd":"extension_open","id":extension}).to_string(),
                ));
            }
        }
        if self.smoke.as_ref().is_some_and(|s| s.phase == 97) && self.windows[0].popup.is_some() {
            let p = self.proxy.clone();
            self.windows[0]
                .popup
                .as_ref()
                .unwrap()
                .evaluate_script_with_callback(
                    "!!document.getElementById('launch')",
                    move |value| {
                        if value == "true" {
                            let _ = p.send_event(Event::SmokeResult(
                                "Extension popup document loaded".into(),
                                true,
                            ));
                        }
                    },
                )?;
        }
        self.smoke_download_tick()?;
        if minutes > 0 {
            for wi in 0..self.windows.len() {
                let mut changed = false;
                for ti in 0..self.windows[wi].tabs.len() {
                    if self.windows[wi].tabs[ti].background_since.elapsed()
                        >= Duration::from_secs(minutes as u64 * 60)
                    {
                        changed |= self.discard(wi, ti);
                    }
                }
                if changed {
                    self.sync(wi)?;
                }
            }
        }
        if self
            .smoke
            .as_ref()
            .is_some_and(|s| s.started.elapsed() > Duration::from_secs(60))
        {
            self.finish_smoke(Some("Превышено время ожидания smoke-test".into()));
        }
        Ok(())
    }
}

fn run() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
    }
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|a| a == "--choose-default-browser") {
        os_integration::show_default_settings()?;
        return Ok(());
    }
    if args.iter().any(|a| a == "--register-browser") {
        os_integration::register_browser()?;
        return Ok(());
    }
    let smoke = args.iter().any(|a| a == "--smoke-test");
    let compat_source = args
        .iter()
        .position(|a| a == "--extension-test")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let isolated = smoke || compat_source.is_some() || args.iter().any(|a| a == "--ui-preview");
    let _instance = if isolated {
        None
    } else {
        match platform::single_instance()? {
            Some(instance) => Some(instance),
            None => {
                os_integration::forward(&os_integration::launch_urls(&args))?;
                return Ok(());
            }
        }
    };
    let directory = if isolated {
        std::env::temp_dir().join(format!(
            "SlimBrowser-Rust-{}-{}",
            std::process::id(),
            chrono::Local::now().timestamp_millis()
        ))
    } else {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA отсутствует")?)
            .join("SlimBrowserRust")
    };
    let store = Store::open(directory.clone(), !isolated)?;
    let event_loop = EventLoopBuilder::<Event>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let smoke_state = smoke.then(|| smoke::Smoke::new(directory));
    let mut app = App::new(store, proxy, smoke_state)?;
    app.open_window(&event_loop, false, false)?;
    if !isolated {
        os_integration::start(&app.store.borrow().directory, app.proxy.clone())?;
        os_integration::register_browser()?;
        app.load_extensions(0)?;
        for url in os_integration::launch_urls(&args) {
            app.add_tab(0, url)?;
        }
    }
    if isolated && !smoke {
        if let Some(view) = args
            .windows(2)
            .find(|a| a[0] == "--preview-view")
            .map(|a| a[1].clone())
            .filter(|v| {
                [
                    "privacy",
                    "appearance",
                    "permissions",
                    "layout",
                    "tracking",
                    "general",
                    "extensions",
                ]
                .contains(&v.as_str())
            })
        {
            let view = if view == "tracking" {
                app.store.borrow_mut().settings.privacy.tracking_mode = "custom".into();
                "privacy".into()
            } else {
                view
            };
            app.set_view(0, Some(view))?;
        }
    }
    if let Some(source) = compat_source {
        compat::run(&mut app, &source)?;
        if args.iter().any(|a| a == "--keep-preview") {
            app.set_view(0, Some("extensions".into()))?;
        } else {
            return Ok(());
        }
    }
    event_loop.run(move |event, target, flow| {
        let id = match &event {
            TaoEvent::WindowEvent { window_id, .. } => Some(*window_id),
            _ => None,
        };
        let result = (|| -> Result<()> {
            match event {
                TaoEvent::UserEvent(event) => app.handle(event, target)?,
                TaoEvent::WindowEvent {
                    window_id,
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    if let Some(wi) = app.index(window_id) {
                        app.request_close(wi)?;
                    }
                }
                TaoEvent::WindowEvent {
                    window_id,
                    event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                    ..
                } => {
                    if let Some(wi) = app.index(window_id) {
                        app.layout(wi)?;
                    }
                }
                TaoEvent::MainEventsCleared => {
                    if app.windows.iter().any(|w| w.motion.is_some())
                        && Instant::now() >= app.next_frame
                    {
                        app.animate_frame()?;
                    }
                    if Instant::now() >= app.next_tick {
                        app.tick()?;
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            if app.smoke.is_some() {
                app.finish_smoke(Some(format!("{error:#}")));
            } else if let Some(wi) = id
                .and_then(|id| app.index(id))
                .or(if app.windows.is_empty() {
                    None
                } else {
                    Some(0)
                })
            {
                app.windows[wi].busy = false;
                app.windows[wi].status = format!("{error:#}");
                let _ = app.sync(wi);
                platform::error(&format!("{error:#}"));
            }
        }
        *flow = if app.windows.is_empty() {
            ControlFlow::ExitWithCode(app.exit_code)
        } else {
            ControlFlow::WaitUntil(if app.windows.iter().any(|w| w.motion.is_some()) {
                app.next_tick.min(app.next_frame)
            } else {
                app.next_tick
            })
        };
    });
}
fn main() {
    if let Err(error) = run() {
        let message = format!("Не удалось запустить SlimBrowser Rust:\n{error:#}");
        if std::env::args().any(|a| a == "--smoke-test") {
            let path = std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("smoke-test.txt");
            let _ = std::fs::write(path, format!("FAIL {message}"));
        } else {
            platform::error(&message);
        }
        std::process::exit(1);
    }
}
