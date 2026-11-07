use crate::{Event, engine, model};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use tao::{event_loop::EventLoopProxy, window::WindowId};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::core::{HSTRING, Interface};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct PrivacySettings {
    pub history_mode: String,
    pub clear_history_on_exit: bool,
    pub tracking_mode: String,
    pub block_trackers: bool,
    pub strict_tracking: bool,
    pub do_not_track: bool,
    pub adblock_enabled: bool,
    pub adblock_exceptions: Vec<String>,
    pub basic_site_fixes: bool,
    pub minor_site_fixes: bool,
    pub cookies: bool,
    pub cookie_mode: String,
    pub tracking_content: bool,
    pub tracking_scope: String,
    pub cryptominers: bool,
    pub known_fingerprints: bool,
    pub suspected_fingerprints: bool,
    pub fingerprinting_scope: String,
    pub block_popups: bool,
    pub popup_exceptions: Vec<String>,
    pub blocked_permissions: Vec<String>,
    pub permissions: Vec<PermissionRule>,
    pub clear_site_data_on_exit: bool,
    pub site_data_exceptions: Vec<String>,
}
impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            history_mode: "remember".into(),
            clear_history_on_exit: false,
            tracking_mode: "standard".into(),
            block_trackers: true,
            strict_tracking: false,
            do_not_track: false,
            adblock_enabled: true,
            adblock_exceptions: vec![],
            basic_site_fixes: true,
            minor_site_fixes: false,
            cookies: true,
            cookie_mode: "third-party".into(),
            tracking_content: true,
            tracking_scope: "private".into(),
            cryptominers: true,
            known_fingerprints: true,
            suspected_fingerprints: false,
            fingerprinting_scope: "private".into(),
            block_popups: true,
            popup_exceptions: vec![],
            blocked_permissions: vec![],
            permissions: vec![],
            clear_site_data_on_exit: false,
            site_data_exceptions: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PermissionRule {
    pub origin: String,
    pub kind: String,
    pub state: String,
}
pub struct PermissionContext {
    pub store: Rc<RefCell<model::Store>>,
    pub private_rules: Option<Rc<RefCell<Vec<PermissionRule>>>>,
}
pub const CATEGORIES: &[(&str, &str, bool)] = &[
    ("geolocation", "Местоположение", true),
    ("camera", "Камера", true),
    ("devices", "Приложения и службы устройства", true),
    ("local-network", "Устройства локальной сети", false),
    ("microphone", "Микрофон", true),
    ("speaker", "Динамик", true),
    ("notifications", "Уведомления", true),
    ("autoplay", "Автовоспроизведение", true),
    ("vr", "Виртуальная реальность", false),
    ("clipboard", "Буфер обмена", true),
    ("files", "Файлы и папки", true),
    ("fonts", "Локальные шрифты", true),
    ("sensors", "Датчики", true),
    ("downloads", "Автоматические загрузки", true),
    ("windows", "Управление окнами", true),
];
pub fn origin(value: &str) -> Result<String> {
    ensure!(
        model::is_web(value),
        "Введите полный HTTP/HTTPS-адрес сайта"
    );
    Ok(url::Url::parse(value)?.origin().ascii_serialization())
}
pub fn category(kind: COREWEBVIEW2_PERMISSION_KIND) -> &'static str {
    match kind {
        COREWEBVIEW2_PERMISSION_KIND_GEOLOCATION => "geolocation",
        COREWEBVIEW2_PERMISSION_KIND_CAMERA => "camera",
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE => "microphone",
        COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS => "notifications",
        COREWEBVIEW2_PERMISSION_KIND_CLIPBOARD_READ => "clipboard",
        COREWEBVIEW2_PERMISSION_KIND_AUTOPLAY => "autoplay",
        COREWEBVIEW2_PERMISSION_KIND_FILE_READ_WRITE => "files",
        COREWEBVIEW2_PERMISSION_KIND_LOCAL_FONTS => "fonts",
        COREWEBVIEW2_PERMISSION_KIND_OTHER_SENSORS => "sensors",
        COREWEBVIEW2_PERMISSION_KIND_MIDI_SYSTEM_EXCLUSIVE_MESSAGES => "devices",
        COREWEBVIEW2_PERMISSION_KIND_MULTIPLE_AUTOMATIC_DOWNLOADS => "downloads",
        COREWEBVIEW2_PERMISSION_KIND_WINDOW_MANAGEMENT => "windows",
        _ => "unknown",
    }
}
impl PrivacySettings {
    pub fn normalize(&mut self) {
        if !["third-party", "strict"].contains(&self.cookie_mode.as_str()) {
            self.cookie_mode = "third-party".into();
        }
        if !["always", "private"].contains(&self.tracking_scope.as_str()) {
            self.tracking_scope = "private".into();
        }
        if !["always", "private"].contains(&self.fingerprinting_scope.as_str()) {
            self.fingerprinting_scope = "private".into();
        }
        if !["remember", "custom", "never"].contains(&self.history_mode.as_str()) {
            self.history_mode = "remember".into();
        }
        if !["standard", "strict", "custom"].contains(&self.tracking_mode.as_str()) {
            self.tracking_mode = "standard".into();
        }
        self.permissions.retain(|r| {
            origin(&r.origin).is_ok_and(|o| o == r.origin)
                && CATEGORIES
                    .iter()
                    .any(|(k, _, supported)| *k == r.kind && *supported)
                && ["allow", "deny"].contains(&r.state.as_str())
        });
        self.blocked_permissions.retain(|k| {
            CATEGORIES
                .iter()
                .any(|(id, _, supported)| id == k && *supported)
        });
        for list in [
            &mut self.popup_exceptions,
            &mut self.site_data_exceptions,
            &mut self.adblock_exceptions,
        ] {
            *list = list.iter().filter_map(|s| origin(s).ok()).collect();
            list.sort();
            list.dedup();
        }
    }
    pub fn remembers_history(&self) -> bool {
        self.history_mode != "never"
    }
    pub fn clears_history(&self) -> bool {
        self.history_mode == "custom" && self.clear_history_on_exit
    }
    pub fn decision(&self, rules: &[PermissionRule], site: &str, kind: &str) -> Option<bool> {
        rules
            .iter()
            .rev()
            .find(|r| r.origin == site && r.kind == kind)
            .map(|r| r.state == "allow")
            .or_else(|| {
                self.blocked_permissions
                    .iter()
                    .any(|k| k == kind)
                    .then_some(false)
            })
    }
    pub fn tracking_level(&self) -> COREWEBVIEW2_TRACKING_PREVENTION_LEVEL {
        match self.tracking_mode.as_str() {
            "strict" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
            "custom" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
            _ => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
        }
    }
}
pub fn apply(core: &ICoreWebView2, settings: &PrivacySettings) -> Result<()> {
    unsafe {
        engine::profile(core)?
            .cast::<ICoreWebView2Profile3>()?
            .SetPreferredTrackingPreventionLevel(settings.tracking_level())?;
    }
    Ok(())
}
pub fn apply_audio(
    core: &ICoreWebView2,
    policy: &PrivacySettings,
    rules: &[PermissionRule],
    url: &str,
    muted: bool,
) -> Result<()> {
    let denied = origin(url).is_ok_and(|o| policy.decision(rules, &o, "speaker") == Some(false));
    unsafe {
        core.cast::<ICoreWebView2_8>()?
            .SetIsMuted(muted || denied)?;
    }
    Ok(())
}
pub fn set_rule(rules: &mut Vec<PermissionRule>, rule: PermissionRule) {
    rules.retain(|r| r.origin != rule.origin || r.kind != rule.kind);
    if rule.state != "ask" {
        rules.push(rule);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SiteData {
    pub origin: String,
    pub cookies: u32,
}
pub fn storage_size(root: &std::path::Path) -> u64 {
    use std::os::windows::fs::MetadataExt;
    fn size(path: &std::path::Path) -> u64 {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return 0;
        };
        if meta.file_attributes() & 0x400 != 0 {
            return 0;
        }
        if meta.is_file() {
            return meta.len();
        }
        std::fs::read_dir(path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| size(&e.path()))
                    .sum()
            })
            .unwrap_or(0)
    }
    [
        "Cache",
        "Code Cache",
        "Network/Cookies",
        "Network/Cookies-journal",
        "Local Storage",
        "IndexedDB",
        "Session Storage",
        "Service Worker",
        "File System",
        "blob_storage",
        "SharedStorage",
        "Shared Dictionary",
        "Storage",
    ]
    .iter()
    .map(|name| size(&root.join(name)))
    .sum()
}
pub fn site_data(
    core: &ICoreWebView2,
    known: Vec<String>,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    unsafe {
        core.cast::<ICoreWebView2_2>()?
            .CookieManager()?
            .GetCookies(
                &HSTRING::new(),
                &GetCookiesCompletedHandler::create(Box::new(move |result, cookies| {
                    let result = (|| -> Result<Vec<SiteData>> {
                        result?;
                        let mut sites = std::collections::BTreeMap::<String, u32>::new();
                        for site in &known {
                            if let Ok(site) = origin(site) {
                                sites.entry(site).or_default();
                            }
                        }
                        if let Some(cookies) = cookies {
                            let mut count = 0;
                            cookies.Count(&mut count)?;
                            for i in 0..count {
                                let cookie = cookies.GetValueAtIndex(i)?;
                                let domain = engine::string(|p| cookie.Domain(p))?;
                                let site = format!("https://{}", domain.trim_start_matches('.'));
                                if let Ok(site) = origin(&site) {
                                    *sites.entry(site).or_default() += 1;
                                }
                            }
                        }
                        Ok(sites
                            .into_iter()
                            .map(|(origin, cookies)| SiteData { origin, cookies })
                            .collect())
                    })();
                    let _ =
                        proxy.send_event(Event::SiteData(id, result.map_err(|e| e.to_string())));
                    Ok(())
                })),
            )?;
    }
    Ok(())
}
pub fn clear_origin(
    core: &ICoreWebView2,
    site: &str,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    let site = origin(site)?;
    let params = serde_json::json!({"origin":site,"storageTypes":"all"}).to_string();
    unsafe {
        core.CallDevToolsProtocolMethod(
            &HSTRING::from("Storage.clearDataForOrigin"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                let _ = proxy.send_event(Event::SiteDataCleared(
                    id,
                    result.map_err(|e| e.to_string()),
                ));
                Ok(())
            })),
        )?;
    }
    Ok(())
}
pub fn clear_data(
    core: &ICoreWebView2,
    kinds: COREWEBVIEW2_BROWSING_DATA_KINDS,
    since: f64,
    history: bool,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    unsafe {
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
            let _ = proxy.send_event(Event::BrowsingCleared(
                id,
                since,
                history,
                result.map_err(|e| e.to_string()),
            ));
            Ok(())
        }));
        let profile = engine::profile(core)?.cast::<ICoreWebView2Profile2>()?;
        if since > 0. {
            profile.ClearBrowsingDataInTimeRange(
                kinds,
                since,
                chrono::Utc::now().timestamp() as f64 + 1.,
                &handler,
            )?;
        } else {
            profile.ClearBrowsingData(kinds, &handler)?;
        }
    }
    Ok(())
}
pub fn forget_engine_history(
    core: &ICoreWebView2,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    unsafe {
        engine::profile(core)?
            .cast::<ICoreWebView2Profile2>()?
            .ClearBrowsingData(
                COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY,
                &ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
                    if let Err(error) = result {
                        let _ = proxy.send_event(Event::Notice(
                            id,
                            format!("Не удалось очистить журнал движка: {error}"),
                        ));
                    }
                    Ok(())
                })),
            )?;
    }
    Ok(())
}

pub fn exit_cleanup(
    core: &ICoreWebView2,
    settings: &model::Settings,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    let policy = &settings.privacy;
    let mut kinds = engine::cleanup_kinds(
        settings.clear_cookies_on_exit || policy.clear_site_data_on_exit,
        settings.clear_cache_on_exit,
    );
    if policy.clear_site_data_on_exit {
        kinds.0 |= COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0;
    }
    if policy.clears_history() || !policy.remembers_history() {
        kinds.0 |= COREWEBVIEW2_BROWSING_DATA_KINDS_BROWSING_HISTORY.0;
    }
    let exceptions = policy.site_data_exceptions.clone();
    let profile = engine::profile(core)?.cast::<ICoreWebView2Profile2>()?;
    unsafe {
        let manager = core.cast::<ICoreWebView2_2>()?.CookieManager()?;
        let keep = manager.clone();
        manager.GetCookies(
            &HSTRING::new(),
            &GetCookiesCompletedHandler::create(Box::new(move |result, list| {
                let result = (|| -> Result<()> {
                    result?;
                    let mut saved = vec![];
                    if let Some(list) = list {
                        let mut count = 0;
                        list.Count(&mut count)?;
                        for i in 0..count {
                            let cookie = list.GetValueAtIndex(i)?;
                            let domain = engine::string(|p| cookie.Domain(p))?;
                            let domain = domain.trim_start_matches('.');
                            if exceptions.iter().any(|s| {
                                url::Url::parse(s)
                                    .ok()
                                    .and_then(|u| u.host_str().map(str::to_owned))
                                    .is_some_and(|h| {
                                        h == domain || h.ends_with(&format!(".{domain}"))
                                    })
                            }) {
                                saved.push(cookie);
                            }
                        }
                    }
                    let p = proxy.clone();
                    let manager = keep.clone();
                    profile.ClearBrowsingData(
                        kinds,
                        &ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
                            let result = result.and_then(|_| {
                                for cookie in &saved {
                                    manager.AddOrUpdateCookie(cookie)?;
                                }
                                Ok(())
                            });
                            let _ = p.send_event(Event::CleanupDone(
                                id,
                                crate::CleanupPurpose::Exit,
                                result.map_err(|e| e.to_string()),
                            ));
                            Ok(())
                        })),
                    )?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = proxy.send_event(Event::CleanupDone(
                        id,
                        crate::CleanupPurpose::Exit,
                        Err(error.to_string()),
                    ));
                }
                Ok(())
            })),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_rules_precede_global_blocks_and_are_origin_scoped() {
        let mut p = PrivacySettings::default();
        p.blocked_permissions.push("camera".into());
        let rule = PermissionRule {
            origin: "https://example.com".into(),
            kind: "camera".into(),
            state: "allow".into(),
        };
        assert_eq!(
            p.decision(std::slice::from_ref(&rule), "https://example.com", "camera"),
            Some(true)
        );
        assert_eq!(
            p.decision(&[rule], "https://other.example.com", "camera"),
            Some(false)
        );
        assert_eq!(p.decision(&[], "https://example.com", "microphone"), None);
    }
    #[test]
    fn history_and_privacy_survive_settings_roundtrip() {
        let s = model::Settings::from_json(
            r#"{"Privacy":{"HistoryMode":"never","TrackingMode":"strict"},"SidebarWidth":999}"#,
        )
        .unwrap();
        assert!(!s.privacy.remembers_history());
        assert_eq!(s.sidebar_width, 480);
        let restored = model::Settings::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(!restored.privacy.remembers_history());
        assert_eq!(
            restored.privacy.tracking_level(),
            COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT
        );
    }
}
