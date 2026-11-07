use crate::{Event, engine, model, privacy::PrivacySettings};
use adblock::{
    Engine,
    lists::{FilterSet, ParseOptions},
    request::Request,
};
use anyhow::Result;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    io::{Cursor, Read},
    rc::Rc,
};
use tao::{event_loop::EventLoopProxy, window::WindowId};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::core::{HSTRING, Interface};
use wry::{WebView, WebViewExtWindows};

pub struct Protection {
    ads: Engine,
    tracking: Engine,
    mining: Engine,
    fingerprints: Engine,
    pub blocked: Cell<u64>,
    scripts: RefCell<HashMap<usize, String>>,
    youtube_scripts: RefCell<HashMap<usize, String>>,
}
fn engine(text: String) -> Engine {
    let mut set = FilterSet::new(false);
    set.add_filter_list(text, ParseOptions::default());
    Engine::new_with_filter_set(set)
}
impl Protection {
    pub fn new() -> Result<Self> {
        let mut zip = zip::ZipArchive::new(Cursor::new(include_bytes!(
            "../assets/protection/filters.zip"
        )))?;
        let mut read = |name: &str| -> Result<String> {
            let mut s = String::new();
            zip.by_name(name)?.read_to_string(&mut s)?;
            Ok(s)
        };
        Ok(Self {
            ads: engine(read("russian.txt")?),
            tracking: engine(read("easyprivacy.txt")?),
            mining: engine(read("nocoin.txt")?),
            fingerprints: engine(read("fingerprinting.txt")?),
            blocked: Cell::new(0),
            scripts: RefCell::new(HashMap::new()),
            youtube_scripts: RefCell::new(HashMap::new()),
        })
    }
    pub fn blocked_request(
        &self,
        policy: &PrivacySettings,
        private: bool,
        url: &str,
        source: &str,
        kind: &str,
        method: &str,
    ) -> bool {
        let Ok(request) = Request::new(url, source, kind, method) else {
            return false;
        };
        let ads = policy.adblock_enabled
            && !crate::privacy::origin(source)
                .is_ok_and(|o| policy.adblock_exceptions.contains(&o));
        let custom = policy.tracking_mode == "custom";
        let strict = policy.tracking_mode == "strict";
        let content = if custom {
            policy.tracking_content && (policy.tracking_scope == "always" || private)
        } else {
            strict || private
        };
        let miner = if custom { policy.cryptominers } else { true };
        let known = if custom {
            policy.known_fingerprints
        } else {
            true
        };
        let fixes = custom && policy.basic_site_fixes && is_service(url, false);
        let minor =
            custom && policy.basic_site_fixes && policy.minor_site_fixes && is_service(url, true);
        (ads && self.ads.check_network_request(&request).should_block())
            || (content
                && !fixes
                && !minor
                && self.tracking.check_network_request(&request).should_block())
            || (miner && self.mining.check_network_request(&request).should_block())
            || (known
                && self
                    .fingerprints
                    .check_network_request(&request)
                    .should_block())
    }
    pub fn cosmetic_script(
        &self,
        policy: &PrivacySettings,
        url: &str,
        classes: &[String],
        ids: &[String],
    ) -> String {
        if !policy.adblock_enabled
            || crate::privacy::origin(url).is_ok_and(|o| policy.adblock_exceptions.contains(&o))
        {
            return "document.getElementById('slim-adblock-style')?.remove()".into();
        }
        let resources = self.ads.url_cosmetic_resources(url);
        let mut selectors = resources.hide_selectors;
        if !resources.generichide {
            selectors.extend(self.ads.hidden_class_id_selectors(
                classes,
                ids,
                &resources.exceptions,
            ));
        }
        let selectors = selectors
            .into_iter()
            .filter(|s| !s.contains(":has-text("))
            .collect::<Vec<_>>();
        let groups = selectors
            .chunks(150)
            .map(|g| format!("{}{{display:none!important}}", g.join(",")))
            .collect::<String>();
        format!(
            "(()=>{{let s=document.getElementById('slim-adblock-style');if(!s){{s=document.createElement('style');s.id='slim-adblock-style';(document.head||document.documentElement).append(s)}}s.textContent={}}})()",
            serde_json::to_string(&groups).unwrap_or_default()
        )
    }
}
pub fn youtube_script(policy: &PrivacySettings) -> String {
    format!(
        "({})({})",
        include_str!("../ui/youtube-guard.js"),
        serde_json::json!({
            "enabled": policy.adblock_enabled,
            "exceptions": policy.adblock_exceptions,
        })
    )
}
fn is_service(url: &str, minor: bool) -> bool {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_default();
    let domains = if minor {
        &[
            "youtube.com",
            "youtube-nocookie.com",
            "ytimg.com",
            "vimeo.com",
            "disqus.com",
        ][..]
    } else {
        &[
            "recaptcha.net",
            "gstatic.com",
            "accounts.google.com",
            "challenges.cloudflare.com",
        ][..]
    };
    domains
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}
pub fn hook(
    view: &WebView,
    store: Rc<RefCell<model::Store>>,
    protection: Rc<Protection>,
    private: bool,
    environment: ICoreWebView2Environment,
) -> Result<()> {
    let core = view.webview();
    let mut token = 0;
    unsafe {
        core.cast::<ICoreWebView2_22>()?
            .AddWebResourceRequestedFilterWithRequestSourceKinds(
                &HSTRING::from("*"),
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |sender, args| {
                let (Some(core), Some(args)) = (sender, args) else {
                    return Ok(());
                };
                let request = args.Request()?;
                let url = engine::string(|p| request.Uri(p))?;
                if !model::is_web(&url) {
                    return Ok(());
                }
                let source = engine::string(|p| core.Source(p)).unwrap_or_default();
                let source = if model::is_web(&source) {
                    source
                } else {
                    url.clone()
                };
                let policy = &store.borrow().settings.privacy;
                if policy.tracking_mode == "custom" && policy.do_not_track {
                    request
                        .Headers()?
                        .SetHeader(&HSTRING::from("DNT"), &HSTRING::from("1"))?;
                }
                let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                args.ResourceContext(&mut context)?;
                let kind = match context {
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT => "document",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_SCRIPT => "script",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_IMAGE => "image",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_STYLESHEET => "stylesheet",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FONT => "font",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_MEDIA => "media",
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_XML_HTTP_REQUEST
                    | COREWEBVIEW2_WEB_RESOURCE_CONTEXT_FETCH => "xmlhttprequest",
                    _ => "other",
                };
                let method = engine::string(|p| request.Method(p))?;
                if protection.blocked_request(policy, private, &url, &source, kind, &method) {
                    let response = environment.CreateWebResourceResponse(
                        None,
                        403,
                        &HSTRING::from("Blocked by Slim Browser"),
                        &HSTRING::from("Content-Type: text/plain\r\nCache-Control: no-store"),
                    )?;
                    args.SetResponse(&response)?;
                    protection.blocked.set(protection.blocked.get() + 1);
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    Ok(())
}
pub fn apply(
    view: &WebView,
    policy: &PrivacySettings,
    private: bool,
    protection: Rc<Protection>,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    let core = view.webview();
    let cookies =
        policy.tracking_mode == "strict" || (policy.tracking_mode == "custom" && policy.cookies);
    let params=serde_json::json!({"enableThirdPartyCookieRestriction":cookies,"disableThirdPartyCookieMetadata":cookies&&policy.cookie_mode=="strict","disableThirdPartyCookieHeuristics":cookies&&policy.cookie_mode=="strict"}).to_string();
    call(
        &core,
        "Network.setCookieControls",
        &params,
        proxy.clone(),
        id,
    )?;
    let key = core.as_raw() as usize;
    if let Some(script) = protection.youtube_scripts.borrow_mut().remove(&key) {
        unsafe {
            core.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(script))?;
        }
    }
    let source = youtube_script(policy);
    let target = core.clone();
    let future_source = source.clone();
    let youtube_protection = protection.clone();
    AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
        Box::new(move |handler| unsafe {
            target
                .AddScriptToExecuteOnDocumentCreated(&HSTRING::from(future_source), &handler)
                .map_err(Into::into)
        }),
        Box::new(move |result, identifier| {
            result?;
            youtube_protection
                .youtube_scripts
                .borrow_mut()
                .insert(key, identifier.to_string());
            Ok(())
        }),
    )?;
    view.evaluate_script(&source)?;
    if let Some(script) = protection.scripts.borrow_mut().remove(&key) {
        unsafe {
            core.RemoveScriptToExecuteOnDocumentCreated(&HSTRING::from(script))?;
        }
    }
    let enabled = policy.tracking_mode == "strict"
        || (policy.tracking_mode == "custom"
            && policy.suspected_fingerprints
            && (private || policy.fingerprinting_scope == "always"));
    if enabled {
        let target = core.clone();
        let source = include_str!("../ui/fingerprint-guard.js");
        let fingerprint_protection = protection.clone();
        AddScriptToExecuteOnDocumentCreatedCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe {
                target
                    .AddScriptToExecuteOnDocumentCreated(&HSTRING::from(source), &handler)
                    .map_err(Into::into)
            }),
            Box::new(move |result, identifier| {
                result?;
                fingerprint_protection
                    .scripts
                    .borrow_mut()
                    .insert(key, identifier.to_string());
                Ok(())
            }),
        )?;
        view.evaluate_script(source)?;
    }
    Ok(())
}
fn call(
    core: &ICoreWebView2,
    method: &str,
    params: &str,
    proxy: EventLoopProxy<Event>,
    id: WindowId,
) -> Result<()> {
    let name = method.to_owned();
    unsafe {
        core.CallDevToolsProtocolMethod(
            &HSTRING::from(method),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                if let Err(e) = result {
                    let _ = proxy
                        .send_event(Event::Notice(id, format!("Защита: {name} недоступен: {e}")));
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
    fn network_toggle_and_origin_exception_really_change_matching() {
        let p = Protection::new().unwrap();
        let mut s = PrivacySettings::default();
        assert!(p.blocked_request(
            &s,
            false,
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://example.com/",
            "script",
            "GET"
        ));
        s.adblock_enabled = false;
        assert!(!p.blocked_request(
            &s,
            false,
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://example.com/",
            "script",
            "GET"
        ));
        s.adblock_enabled = true;
        s.adblock_exceptions.push("https://example.com".into());
        assert!(!p.blocked_request(
            &s,
            false,
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://example.com/",
            "script",
            "GET"
        ));
    }
    #[test]
    fn cryptomining_is_independent_of_ads() {
        let p = Protection::new().unwrap();
        let mut s = PrivacySettings {
            adblock_enabled: false,
            tracking_mode: "custom".into(),
            tracking_content: false,
            known_fingerprints: false,
            ..Default::default()
        };
        assert!(p.blocked_request(
            &s,
            false,
            "https://coin-hive.com/lib/coinhive.min.js",
            "https://example.com/",
            "script",
            "GET"
        ));
        s.cryptominers = false;
        assert!(!p.blocked_request(
            &s,
            false,
            "https://coin-hive.com/lib/coinhive.min.js",
            "https://example.com/",
            "script",
            "GET"
        ));
    }
}
