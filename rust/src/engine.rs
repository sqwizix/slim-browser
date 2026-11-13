use crate::{CleanupPurpose, Event, model::Settings};
use anyhow::{Context, Result};
use std::{cell::Cell, path::Path, rc::Rc, sync::mpsc};
use tao::{event_loop::EventLoopProxy, window::WindowId};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::CoTaskMemFree,
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT},
            WindowsAndMessaging::{IDYES, MB_ICONQUESTION, MB_YESNO, MessageBoxW},
        },
    },
    core::{BOOL, HSTRING, Interface, PWSTR},
};
use wry::{WebView, WebViewExtWindows};

const ARGS: &str = "--disk-cache-size=33554432 --media-cache-size=8388608 --disable-background-networking --disable-background-sync --disable-features=Prerender2,SpeculationRulesPrefetch,msEdgeSidebarV2 --no-first-run";
pub const UI_ARGS: &str = "--no-first-run";

pub fn environment(
    directory: &Path,
    language: &str,
    proxy: Option<std::net::SocketAddr>,
) -> Result<ICoreWebView2Environment> {
    let options = CoreWebView2EnvironmentOptions::default();
    unsafe {
        let args = match proxy {
            Some(address) => format!(
                "{ARGS} --proxy-server=socks5://{address} --proxy-bypass-list=\"<-loopback>\" --host-resolver-rules=\"MAP * ~NOTFOUND, EXCLUDE localhost\" --disable-quic --force-webrtc-ip-handling-policy=disable_non_proxied_udp"
            ),
            None => ARGS.to_owned(),
        };
        let scrolling = if crate::platform::motion_enabled() {
            "--enable-smooth-scrolling"
        } else {
            "--disable-smooth-scrolling"
        };
        options.set_additional_browser_arguments(format!("{args} {scrolling}"));
        options.set_language(language.into());
        options.set_are_browser_extensions_enabled(true);
        options.set_enable_tracking_prevention(true);
    }
    let (sender, receiver) = mpsc::channel();
    unsafe {
        CreateCoreWebView2EnvironmentWithOptions(
            windows::core::PCWSTR::null(),
            &HSTRING::from(directory),
            &ICoreWebView2EnvironmentOptions::from(options),
            &CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
                move |result, environment| {
                    let result = result.and_then(|_| {
                        environment.ok_or_else(|| {
                            windows::core::Error::from(windows::Win32::Foundation::E_POINTER)
                        })
                    });
                    let _ = sender.send(result);
                    Ok(())
                },
            )),
        )?;
    }
    wait_with_pump(receiver)?.context("Не удалось создать WebView2 environment")
}

pub fn string(
    get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>,
) -> windows::core::Result<String> {
    let mut value = PWSTR::null();
    get(&mut value)?;
    let result = unsafe { value.to_string() };
    unsafe {
        CoTaskMemFree(Some(value.0 as *const _));
    }
    Ok(result?)
}
pub fn profile(core: &ICoreWebView2) -> windows::core::Result<ICoreWebView2Profile> {
    unsafe { core.cast::<ICoreWebView2_13>()?.Profile() }
}
pub fn configure(view: &WebView, settings: &Settings, shell: bool) -> Result<()> {
    let core = view.webview();
    if !shell {
        crate::privacy::apply(&core, &settings.privacy)?;
    }
    unsafe {
        let config = core.Settings()?;
        config.SetIsStatusBarEnabled(false)?;
        config.SetAreHostObjectsAllowed(false)?;
        config.SetAreDefaultContextMenusEnabled(!shell)?;
        config.SetIsZoomControlEnabled(!shell)?;
        config.SetIsWebMessageEnabled(shell)?;
        let passwords = config.cast::<ICoreWebView2Settings4>()?;
        passwords.SetIsPasswordAutosaveEnabled(false)?;
        passwords.SetIsGeneralAutofillEnabled(false)?;
        if !shell {
            profile(&core)?
                .SetDefaultDownloadFolderPath(&HSTRING::from(settings.download_folder.as_path()))?;
        }
        view.controller()
            .SetZoomFactor(if shell { 1. } else { settings.zoom })?;
    }
    let theme = match settings.site_color_scheme.as_str() {
        "dark" => wry::Theme::Dark,
        "light" => wry::Theme::Light,
        _ => wry::Theme::Auto,
    };
    view.set_theme(theme)?;
    Ok(())
}

pub fn capture_png(
    core: &ICoreWebView2,
    path: std::path::PathBuf,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    use windows::Win32::System::Com::{
        STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET, StructuredStorage::CreateStreamOnHGlobal,
    };
    let stream = unsafe { CreateStreamOnHGlobal(Default::default(), true)? };
    let buffer = stream.clone();
    let output = path.clone();
    unsafe {
        core.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &CapturePreviewCompletedHandler::create(Box::new(move |result| {
                let result = (|| -> Result<()> {
                    result?;
                    let mut stat = STATSTG::default();
                    buffer.Stat(&mut stat, STATFLAG_NONAME)?;
                    anyhow::ensure!(stat.cbSize <= 256 * 1024 * 1024, "Снимок слишком большой.");
                    let mut bytes = vec![0_u8; stat.cbSize as usize];
                    buffer.Seek(0, STREAM_SEEK_SET, None)?;
                    let mut read = 0;
                    buffer
                        .Read(
                            bytes.as_mut_ptr() as *mut _,
                            bytes.len() as u32,
                            Some(&mut read),
                        )
                        .ok()?;
                    anyhow::ensure!(read as usize == bytes.len(), "Не удалось прочитать снимок.");
                    crate::model::write_atomic(&output, &bytes)?;
                    Ok(())
                })();
                let _ = proxy.send_event(Event::ScreenshotDone(
                    id,
                    path,
                    result.map_err(|e| e.to_string()),
                ));
                Ok(())
            })),
        )?;
    }
    Ok(())
}
pub fn cleanup_kinds(cookies: bool, cache: bool) -> COREWEBVIEW2_BROWSING_DATA_KINDS {
    COREWEBVIEW2_BROWSING_DATA_KINDS(
        (if cookies {
            COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0
        } else {
            0
        }) | (if cache {
            COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE.0
        } else {
            0
        }),
    )
}
pub fn cleanup(
    core: &ICoreWebView2,
    cookies: bool,
    cache: bool,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
    purpose: CleanupPurpose,
) -> Result<()> {
    unsafe {
        profile(core)?
            .cast::<ICoreWebView2Profile2>()?
            .ClearBrowsingData(
                cleanup_kinds(cookies, cache),
                &ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
                    let result = result.map_err(|e| e.to_string());
                    let _ = proxy.send_event(Event::CleanupDone(id, purpose, result));
                    Ok(())
                })),
            )?;
    }
    Ok(())
}
pub fn hooks(
    view: &WebView,
    id: WindowId,
    tab: u64,
    proxy: EventLoopProxy<Event>,
    active: Rc<Cell<bool>>,
    parent: isize,
    permissions: crate::privacy::PermissionContext,
) -> Result<()> {
    let crate::privacy::PermissionContext {
        store,
        private_rules,
    } = permissions;
    let core = view.webview();
    let favicon_proxy = proxy.clone();
    let mut favicon_token = 0;
    unsafe {
        core.cast::<ICoreWebView2_15>()?.add_FaviconChanged(
            &FaviconChangedEventHandler::create(Box::new(move |sender, _| {
                let Some(sender) = sender else {
                    return Ok(());
                };
                let p = favicon_proxy.clone();
                sender.cast::<ICoreWebView2_15>()?.GetFavicon(
                    COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
                    &GetFaviconCompletedHandler::create(Box::new(move |result, stream| {
                        let mut data = String::new();
                        if result.is_ok() {
                            if let Some(stream) = stream {
                                use base64::Engine;
                                use windows::Win32::System::Com::{
                                    STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
                                };
                                let mut stat = STATSTG::default();
                                stream.Stat(&mut stat, STATFLAG_NONAME)?;
                                if stat.cbSize > 0 && stat.cbSize <= 1024 * 1024 {
                                    let mut bytes = vec![0u8; stat.cbSize as usize];
                                    let mut read = 0;
                                    stream.Seek(0, STREAM_SEEK_SET, None)?;
                                    stream
                                        .Read(
                                            bytes.as_mut_ptr().cast(),
                                            bytes.len() as u32,
                                            Some(&mut read),
                                        )
                                        .ok()?;
                                    if read as usize == bytes.len() {
                                        data = format!(
                                            "data:image/png;base64,{}",
                                            base64::engine::general_purpose::STANDARD.encode(bytes)
                                        );
                                    }
                                }
                            }
                        }
                        let _ = p.send_event(Event::Favicon(id, tab, data));
                        Ok(())
                    })),
                )?;
                Ok(())
            })),
            &mut favicon_token,
        )?;
    }
    let download_proxy = proxy.clone();
    let mut download_token = 0;
    unsafe {
        core.cast::<ICoreWebView2_4>()?.add_DownloadStarting(
            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let operation = args.DownloadOperation()?;
                    let pending = crate::downloads::enqueue(operation);
                    if download_proxy
                        .send_event(Event::DownloadStarted(id, tab, pending))
                        .is_err()
                    {
                        crate::downloads::take_pending(pending);
                    }
                }
                Ok(())
            })),
            &mut download_token,
        )?;
    }
    let mut token = 0_i64;
    let navigation = Rc::new(Cell::new(0_u64));
    let p = proxy.clone();
    unsafe {
        view.controller().add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                let _ = p.send_event(Event::PopupLostFocus(id));
                Ok(())
            })),
            &mut token,
        )?;
    }
    let p = proxy.clone();
    let navigation_store = store.clone();
    let current_navigation = navigation.clone();
    unsafe {
        core.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    let url = string(|v| args.Uri(v))?;
                    if let Some(core) = sender {
                        core.Settings()?.SetIsWebMessageEnabled(
                            crate::extension_store::registered_url(
                                &navigation_store.borrow().directory,
                                &url,
                            ),
                        )?;
                    }
                    if crate::model::is_page_url(&url)
                        || url::Url::parse(&url).is_ok_and(|u| u.scheme() == "chrome-extension")
                    {
                        let mut navigation_id = 0;
                        args.NavigationId(&mut navigation_id)?;
                        current_navigation.set(navigation_id);
                        let _ = p.send_event(Event::Page(id, tab, true, url));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    let p = proxy.clone();
    unsafe {
        core.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                let _ = p.send_event(Event::Refresh(id, tab));
                Ok(())
            })),
            &mut token,
        )?;
    }
    let p = proxy.clone();
    unsafe {
        core.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |_, _| {
                let _ = p.send_event(Event::Refresh(id, tab));
                Ok(())
            })),
            &mut token,
        )?;
    }
    let p = proxy.clone();
    let popup_store = store.clone();
    unsafe {
        core.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                    let mut user = BOOL::default();
                    args.IsUserInitiated(&mut user)?;
                    let url = string(|v| args.Uri(v))?;
                    let settings = popup_store.borrow();
                    let source = sender
                        .as_ref()
                        .and_then(|s| string(|p| s.Source(p)).ok())
                        .and_then(|s| crate::privacy::origin(&s).ok());
                    let exception = source
                        .is_some_and(|o| settings.settings.privacy.popup_exceptions.contains(&o));
                    if (user.as_bool() || !settings.settings.privacy.block_popups || exception)
                        && crate::model::is_web(&url)
                    {
                        let _ = p.send_event(Event::NewTab(id, tab, url));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    let p = proxy.clone();
    unsafe {
        core.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |sender, args| {
                if let Some(args) = args {
                    let mut navigation_id = 0;
                    args.NavigationId(&mut navigation_id)?;
                    // Ignore completions for the initial about:blank or a superseded request.
                    if navigation_id != navigation.get() {
                        return Ok(());
                    }
                    let mut success = BOOL::default();
                    args.IsSuccess(&mut success)?;
                    if success.as_bool() {
                        if let Some(sender) = sender {
                            let url = string(|v| sender.Source(v))?;
                            let _ = p.send_event(Event::Page(id, tab, false, url));
                        }
                    } else {
                        let mut error = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut error)?;
                        let message = if error == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED {
                            None
                        } else {
                            Some(
                                match error {
                                    COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => {
                                        "Адрес сайта не найден."
                                    }
                                    COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => {
                                        "Сайт не ответил вовремя. Попробуйте обновить страницу."
                                    }
                                    COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
                                    | COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_RESET => {
                                        "Связь с сайтом прервалась. Обновите страницу."
                                    }
                                    _ => "Не удалось загрузить страницу. Попробуйте обновить её.",
                                }
                                .to_owned(),
                            )
                        };
                        let _ = p.send_event(Event::NavigationFailed(id, tab, message));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    let permission_proxy = proxy.clone();
    unsafe {
        core.add_PermissionRequested(
            &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                if let Ok(args3) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
                    args3.SetSavesInProfile(false)?;
                }
                let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
                args.PermissionKind(&mut kind)?;
                let category = crate::privacy::category(kind);
                let uri = string(|p| args.Uri(p))?;
                let site = crate::privacy::origin(&uri).unwrap_or_default();
                let policy = store.borrow().settings.privacy.clone();
                let rules = private_rules
                    .as_ref()
                    .map(|r| r.borrow().clone())
                    .unwrap_or_else(|| policy.permissions.clone());
                let decision = policy.decision(&rules, &site, category);
                let mut initiated = BOOL::default();
                args.IsUserInitiated(&mut initiated)?;
                let allowed = if !active.get() || site.is_empty() {
                    false
                } else if let Some(allow) = decision {
                    allow
                } else if initiated.as_bool() {
                    let label = crate::privacy::CATEGORIES
                        .iter()
                        .find(|(id, _, _)| *id == category)
                        .map(|(_, label, _)| *label)
                        .unwrap_or("Дополнительное разрешение");
                    let host = crate::model::display_address(&uri);
                    let language = store.borrow().settings.language.clone();
                    let label = crate::i18n::text(&language, label);
                    let prompt = crate::i18n::text(
                        &language,
                        "{0}: разрешить доступ?\nРешение можно изменить в настройках разрешений.",
                    )
                    .replace("{0}", &label);
                    let text = crate::platform::wide(&format!("{host}\n{site}\n\n{prompt}"));
                    let title = crate::platform::wide(&crate::i18n::text(
                        &language,
                        "Slim Browser · Разрешение сайта",
                    ));
                    let allow = MessageBoxW(
                        Some(HWND(parent as *mut _)),
                        windows::core::PCWSTR(text.as_ptr()),
                        windows::core::PCWSTR(title.as_ptr()),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES;
                    let rule = crate::privacy::PermissionRule {
                        origin: site,
                        kind: category.into(),
                        state: if allow { "allow" } else { "deny" }.into(),
                    };
                    let _ = permission_proxy.send_event(Event::PermissionDecision(id, rule));
                    allow
                } else {
                    false
                };
                args.SetState(if allowed {
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW
                } else {
                    COREWEBVIEW2_PERMISSION_STATE_DENY
                })?;
                Ok(())
            })),
            &mut token,
        )?;
    }
    shortcuts(view, id, proxy, false)
}

pub fn shortcuts(
    view: &WebView,
    id: WindowId,
    proxy: EventLoopProxy<Event>,
    shell: bool,
) -> Result<()> {
    let mut token = 0_i64;
    unsafe {
        view.controller().add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                args.KeyEventKind(&mut kind)?;
                if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                {
                    return Ok(());
                }
                let mut key = 0;
                args.VirtualKey(&mut key)?;
                if shell && key == 0x1B {
                    return Ok(());
                }
                let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                let command = match (key, ctrl, shift, alt) {
                    (0x4C, true, _, false) => "focus_address",
                    (0x54, true, _, false) => "new_tab",
                    (0x57, true, _, false) => "close_tab",
                    (0x4E, true, true, false) | (0x50, true, true, false) => "private_window",
                    (0x4E, true, false, false) => "new_window",
                    (0x74, false, _, false) => "reload",
                    (0x7A, false, _, false) => "fullscreen",
                    (0x25, false, _, true) => "back",
                    (0x27, false, _, true) => "forward",
                    (0x1B, false, _, false) => "escape",
                    _ => return Ok(()),
                };
                args.SetHandled(true)?;
                let _ = proxy.send_event(Event::Key(id, command.into()));
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}

pub fn extension_popup_hooks(
    view: &WebView,
    id: WindowId,
    opener: u64,
    proxy: EventLoopProxy<Event>,
) -> Result<()> {
    let p = proxy.clone();
    let mut token = 0;
    unsafe {
        view.webview().add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |sender, args| {
                if let (Some(core), Some(args)) = (sender, args) {
                    let url = string(|p| args.Uri(p))?;
                    core.Settings()?
                        .SetIsWebMessageEnabled(url.starts_with("chrome-extension://"))?;
                }
                Ok(())
            })),
            &mut token,
        )?;

        view.webview().add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    args.SetHandled(true)?;
                    let mut user = BOOL::default();
                    args.IsUserInitiated(&mut user)?;
                    let url = string(|v| args.Uri(v))?;
                    if user.as_bool() && crate::model::is_web(&url) {
                        let _ = p.send_event(Event::NewTab(id, opener, url));
                    }
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    shortcuts(view, id, proxy, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_cleanup_masks() {
        assert_eq!(
            cleanup_kinds(true, false),
            COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES
        );
        assert_eq!(
            cleanup_kinds(false, true).0 & COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0,
            0
        );
        assert_eq!(cleanup_kinds(false, false).0, 0);
    }
}
