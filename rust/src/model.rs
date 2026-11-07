use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;
use zeroize::Zeroizing;

pub const HOME: &str = "slim://newtab";
pub const SEARCH_ENGINES: &[(&str, &str)] = &[
    ("google", "Google"),
    ("bing", "Bing"),
    ("duckduckgo", "DuckDuckGo"),
];
pub const TRANSLATION_LANGUAGES: &[(&str, &str)] = &[
    ("ru", "Русский"),
    ("en", "English"),
    ("uk", "Українська"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("es", "Español"),
    ("pl", "Polski"),
];
pub fn translation_url(page: &str, language: &str) -> Result<String> {
    if !is_web(page) {
        bail!("Откройте страницу, которую нужно перевести.")
    }
    if !TRANSLATION_LANGUAGES
        .iter()
        .any(|(code, _)| *code == language)
    {
        bail!("Выберите язык перевода.")
    }
    let mut url = Url::parse("https://translate.google.com/translate")?;
    url.query_pairs_mut()
        .append_pair("sl", "auto")
        .append_pair("tl", language)
        .append_pair("u", page);
    Ok(url.into())
}

pub fn is_web(value: &str) -> bool {
    Url::parse(value).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}

pub fn is_local_document(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        u.scheme() == "file"
            && (u.host_str().is_none() || u.host_str() == Some("localhost"))
            && u.to_file_path().ok().is_some_and(|p| {
                p.is_file()
                    && p.extension().is_some_and(|e| {
                        ["html", "htm"].contains(&e.to_string_lossy().to_ascii_lowercase().as_str())
                    })
            })
    })
}
pub fn is_page_url(value: &str) -> bool {
    is_web(value) || is_local_document(value)
}
pub fn resolve(input: &str, search_engine: &str) -> Result<String> {
    let text = input.trim();
    if text.is_empty() {
        return Ok(HOME.into());
    }
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if !is_web(text) {
            bail!("Некорректный адрес. Адреса с логином или паролем не поддерживаются.");
        }
        return Ok(Url::parse(text)?.into());
    }
    if !text.chars().any(char::is_whitespace) && !text.contains("://") {
        if let Ok(url) = Url::parse(&format!("https://{text}")) {
            if is_web(url.as_str())
                && url.host_str().is_some_and(|host| {
                    host.contains('.') || host == "localhost" || host.contains(':')
                })
            {
                return Ok(url.into());
            }
        }
    }
    let endpoint = match search_engine {
        "bing" => "https://www.bing.com/search",
        "duckduckgo" => "https://duckduckgo.com/",
        _ => "https://www.google.com/search",
    };
    let mut url = Url::parse(endpoint)?;
    url.query_pairs_mut().append_pair("q", text);
    Ok(url.into())
}

pub fn secure_origin(value: &str) -> Option<String> {
    if !is_web(value) {
        return None;
    }
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https").then(|| url.origin().ascii_serialization())
}

pub fn display_address(value: &str) -> String {
    if value == HOME {
        return String::new();
    }
    Url::parse(value)
        .map(|u| match u.port() {
            Some(p) => format!("{}:{p}", u.host_str().unwrap_or_default()),
            None => u.host_str().unwrap_or_default().to_owned(),
        })
        .unwrap_or_default()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub glass: bool,
    pub glass_transparency: u8,
    pub sidebar_visible: bool,
    pub sidebar_width: u32,
    pub unified_sidebar_width: u32,
    pub compact_sidebar_width: u32,
    pub download_folder: PathBuf,
    pub language: String,
    pub zoom: f64,
    pub dark_mode: bool,
    pub save_passwords: bool,
    pub discard_minutes: u32,
    pub clear_cookies_on_exit: bool,
    pub clear_cache_on_exit: bool,
    pub translation_language: String,
    pub search_engine: String,
    pub site_color_scheme: String,
    pub browser_layout: String,
    pub download_units: String,
    pub dns: crate::network::DnsSettings,
    pub privacy: crate::privacy::PrivacySettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            glass: true,
            glass_transparency: 65,
            sidebar_visible: true,
            sidebar_width: 222,
            unified_sidebar_width: 280,
            compact_sidebar_width: 72,
            download_folder: PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default())
                .join("Downloads"),
            language: "ru-RU".into(),
            zoom: 1.,
            dark_mode: crate::platform::system_dark(),
            save_passwords: true,
            discard_minutes: 5,
            clear_cookies_on_exit: false,
            clear_cache_on_exit: false,
            translation_language: "ru".into(),
            search_engine: "google".into(),
            site_color_scheme: "system".into(),
            browser_layout: "split".into(),
            download_units: "bytes".into(),
            dns: crate::network::DnsSettings::default(),
            privacy: crate::privacy::PrivacySettings::default(),
        }
    }
}
impl Settings {
    pub fn from_json(value: &str) -> Result<Self> {
        let raw: serde_json::Value = serde_json::from_str(value)?;
        let mut settings: Self = serde_json::from_value(raw.clone())?;
        if raw.get("SiteColorScheme").is_none() && raw.get("DarkMode").is_some() {
            settings.site_color_scheme = if settings.dark_mode { "dark" } else { "light" }.into();
        }
        if raw.get("ClearOnExit").and_then(|v| v.as_bool()) == Some(true) {
            if raw.get("ClearCookiesOnExit").is_none() {
                settings.clear_cookies_on_exit = true;
            }
            if raw.get("ClearCacheOnExit").is_none() {
                settings.clear_cache_on_exit = true;
            }
        }
        settings.normalize();
        Ok(settings)
    }
    pub fn normalize(&mut self) {
        self.sidebar_width = self.sidebar_width.clamp(180, 480);
        self.unified_sidebar_width = self.unified_sidebar_width.clamp(240, 480);
        self.compact_sidebar_width = self.compact_sidebar_width.clamp(64, 160);
        self.privacy.normalize();
        if !["bytes", "bits"].contains(&self.download_units.as_str()) {
            self.download_units = "bytes".into();
        }
        if self.dns.validate().is_err() {
            self.dns = crate::network::DnsSettings::default();
        }
        if !SEARCH_ENGINES
            .iter()
            .any(|(code, _)| *code == self.search_engine)
        {
            self.search_engine = "google".into();
        }
        if !["system", "light", "dark"].contains(&self.site_color_scheme.as_str()) {
            self.site_color_scheme = "system".into();
        }
        self.dark_mode = match self.site_color_scheme.as_str() {
            "dark" => true,
            "light" => false,
            _ => crate::platform::system_dark(),
        };
        if !["unified", "split", "compact"].contains(&self.browser_layout.as_str()) {
            self.browser_layout = "split".into();
        }
        if !TRANSLATION_LANGUAGES
            .iter()
            .any(|(code, _)| *code == self.translation_language)
        {
            self.translation_language = "ru".into();
        }
        self.glass_transparency = self.glass_transparency.min(100);
        if ![0, 5, 15, 30].contains(&self.discard_minutes) {
            self.discard_minutes = 5;
        }
        if ![0.75, 0.9, 1., 1.1, 1.25, 1.5].contains(&self.zoom) {
            self.zoom = 1.;
        }
        if !["ru-RU", "en-US"].contains(&self.language.as_str()) {
            self.language = "ru-RU".into();
        }
        if !self.download_folder.is_absolute() {
            self.download_folder = Self::default().download_folder;
        }
    }
    pub fn page_origin(&self) -> (f64, f64) {
        if !self.sidebar_visible {
            return (
                0.,
                if self.browser_layout == "unified" {
                    42.
                } else {
                    94.
                },
            );
        }
        match self.browser_layout.as_str() {
            "unified" => (self.unified_sidebar_width as f64, 42.),
            "compact" => (self.compact_sidebar_width as f64, 94.),
            _ => (self.sidebar_width as f64, 94.),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct SiteEntry {
    pub title: String,
    pub url: String,
    pub visited: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TabFolder {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub pages: Vec<FolderPage>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FolderPage {
    pub url: String,
    pub title: String,
}
pub fn folder_name(input: &str) -> Result<String> {
    let name = input.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        bail!("Название папки должно содержать от 1 до 80 символов.");
    }
    Ok(name.into())
}

#[derive(Clone, Serialize, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
#[serde(rename_all = "PascalCase")]
pub struct Credential {
    pub origin: String,
    pub username: String,
    pub password: String,
}

pub struct Store {
    pub directory: PathBuf,
    pub settings: Settings,
    pub bookmarks: Vec<SiteEntry>,
    pub history: Vec<SiteEntry>,
    pub folders: Vec<TabFolder>,
}
impl Store {
    pub fn open(directory: PathBuf, import: bool) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        let new_profile = !directory.join("settings.json").exists();
        if import && new_profile {
            let previous = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                .join("SlimBrowser");
            // Copy only portable application files. Never open or copy a live WebView2 profile.
            for file in [
                "settings.json",
                "bookmarks.json",
                "history.json",
                "passwords.dpapi",
            ] {
                let source = previous.join(file);
                let target = directory.join(file);
                if source.is_file() && !target.exists() {
                    fs::copy(&source, &target)
                        .with_context(|| format!("Не удалось скопировать {file}"))?;
                }
            }
        }
        let path = directory.join("settings.json");
        let settings = if path.exists() {
            Settings::from_json(&fs::read_to_string(path)?)?
        } else {
            Settings::default()
        };
        let read = |name| -> Result<Vec<SiteEntry>> {
            let path = directory.join(name);
            if path.exists() {
                Ok(serde_json::from_slice::<Vec<SiteEntry>>(&fs::read(path)?)?
                    .into_iter()
                    .filter(|e| is_web(&e.url))
                    .collect())
            } else {
                Ok(Vec::new())
            }
        };
        let folders_path = directory.join("tab-folders.json");
        let folders = if folders_path.exists() {
            let mut folders: Vec<TabFolder> = serde_json::from_slice(&fs::read(folders_path)?)?;
            for folder in &mut folders {
                folder.name = folder_name(&folder.name)?;
                folder.pages.retain(|p| p.url == HOME || is_web(&p.url));
            }
            folders
        } else {
            Vec::new()
        };
        let store = Self {
            bookmarks: read("bookmarks.json")?,
            history: read("history.json")?,
            directory,
            settings,
            folders,
        };
        store.save_settings()?;
        Ok(store)
    }
    pub fn save_settings(&self) -> Result<()> {
        write_atomic(
            &self.directory.join("settings.json"),
            &serde_json::to_vec_pretty(&self.settings)?,
        )
    }
    pub fn save_folders(&self) -> Result<()> {
        write_atomic(
            &self.directory.join("tab-folders.json"),
            &serde_json::to_vec_pretty(&self.folders)?,
        )
    }
    pub fn save_library(&self, bookmarks: bool) -> Result<()> {
        write_atomic(
            &self.directory.join(if bookmarks {
                "bookmarks.json"
            } else {
                "history.json"
            }),
            &serde_json::to_vec(if bookmarks {
                &self.bookmarks
            } else {
                &self.history
            })?,
        )
    }
    pub fn add_entry(&mut self, bookmarks: bool, title: &str, url: &str) -> Result<()> {
        if !is_web(url) || (!bookmarks && !self.settings.privacy.remembers_history()) {
            return Ok(());
        }
        add_entry(
            if bookmarks {
                &mut self.bookmarks
            } else {
                &mut self.history
            },
            title,
            url,
            if bookmarks { 5000 } else { 1000 },
        );
        self.save_library(bookmarks)
    }
    pub fn credentials(&self) -> Result<Vec<Credential>> {
        let path = self.directory.join("passwords.dpapi");
        if !path.exists() {
            return Ok(Vec::new());
        }
        let bytes = Zeroizing::new(crate::platform::decrypt(&fs::read(path)?)?);
        serde_json::from_slice(&bytes).context("Не удалось прочитать зашифрованные пароли")
    }
    pub fn save_credentials(&self, credentials: &[Credential]) -> Result<()> {
        let bytes = Zeroizing::new(serde_json::to_vec(credentials)?);
        write_atomic(
            &self.directory.join("passwords.dpapi"),
            &crate::platform::encrypt(&bytes)?,
        )
    }
    pub fn save_credential(&self, mut item: Credential) -> Result<()> {
        if !self.settings.save_passwords {
            bail!("Включите сохранение паролей.");
        }
        item.origin = secure_origin(&item.origin).context("Для паролей требуется адрес HTTPS.")?;
        if item.username.trim().is_empty() || item.password.is_empty() {
            bail!("Введите логин и пароль.");
        }
        let mut items = self.credentials()?;
        items.retain(|i| i.origin != item.origin || i.username != item.username);
        items.push(item);
        self.save_credentials(&items)
    }
}

pub fn add_entry(items: &mut Vec<SiteEntry>, title: &str, url: &str, limit: usize) {
    if !is_web(url) {
        return;
    }
    items.retain(|e| e.url != url);
    items.insert(
        0,
        SiteEntry {
            title: title.into(),
            url: url.into(),
            visited: chrono::Local::now().to_rfc3339(),
        },
    );
    items.truncate(limit);
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temp, path).context("Не удалось сохранить файл")
}

pub fn autofill_script(item: &Credential) -> Result<String> {
    Ok(format!(
        r#"(()=>{{const c={};if(window.top!==window||location.origin!==c.Origin)return;const visible=e=>!e.disabled&&!e.readOnly&&e.getClientRects().length>0;const p=[...document.querySelectorAll('input[type="password"]')].find(e=>visible(e)&&e.autocomplete!=='new-password');if(!p||p.value)return;const scope=p.form||document;const u=[...scope.querySelectorAll('input')].find(e=>visible(e)&&(e.autocomplete==='username'||e.type==='email'))||[...scope.querySelectorAll('input[type="text"]')].find(visible);const set=(e,v)=>{{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,v);e.dispatchEvent(new Event('input',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}));}};if(u&&!u.value)set(u,c.Username);set(p,c.Password);}})()"#,
        serde_json::to_string(item)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn google_translation_encodes_page_and_language() {
        let page = "https://example.com/path?q=кот&next=https%3A%2F%2Fexample.org%2Fx#раздел";
        let result = Url::parse(&translation_url(page, "en").unwrap()).unwrap();
        assert_eq!(result.host_str(), Some("translate.google.com"));
        let pairs = result
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs["tl"], "en");
        assert_eq!(pairs["sl"], "auto");
        assert_eq!(pairs["u"], page);
    }
    #[test]
    fn translation_rejects_internal_pages_and_unknown_language() {
        assert!(translation_url(HOME, "ru").is_err());
        assert!(translation_url("https://example.com", "bad").is_err());
        assert!(translation_url("https://user:secret@example.com", "ru").is_err());
    }
    #[test]
    fn addresses() {
        for (input, expected) in [
            ("", HOME),
            ("github.com", "https://github.com/"),
            ("localhost:3000", "https://localhost:3000/"),
            ("127.0.0.1:8080", "https://127.0.0.1:8080/"),
            ("http://localhost:8080/test", "http://localhost:8080/test"),
        ] {
            assert_eq!(resolve(input, "google").unwrap(), expected);
        }
    }
    #[test]
    fn query_encoding() {
        let query = "кот & пес / # 🦀";
        let url = Url::parse(&resolve(query, "google").unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("www.google.com"));
        assert_eq!(url.query_pairs().next().unwrap().1, query);
    }
    #[test]
    fn folders_preserve_names_and_pages_on_disk() {
        let dir = std::env::temp_dir().join(format!("slim-folder-test-{}", std::process::id()));
        let mut store = Store::open(dir.clone(), false).unwrap();
        store.folders.push(TabFolder {
            id: 1,
            name: folder_name("  Работа 🗂  ").unwrap(),
            collapsed: true,
            pages: vec![FolderPage {
                url: "https://example.com/path".into(),
                title: "Пример".into(),
            }],
        });
        store.save_folders().unwrap();
        let restored = Store::open(dir.clone(), false).unwrap();
        assert_eq!(restored.folders[0].name, "Работа 🗂");
        assert!(restored.folders[0].collapsed);
        assert_eq!(restored.folders[0].pages[0].url, "https://example.com/path");
        assert!(folder_name(" ").is_err());
        assert!(folder_name("wrong\nname").is_err());
        assert!(folder_name(&"a".repeat(81)).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn search_engines_encode_queries_and_preserve_addresses() {
        let query = "кот & пес / # 🦀";
        for (engine, host) in [
            ("google", "www.google.com"),
            ("bing", "www.bing.com"),
            ("duckduckgo", "duckduckgo.com"),
        ] {
            let url = Url::parse(&resolve(query, engine).unwrap()).unwrap();
            assert_eq!(url.host_str(), Some(host));
            assert_eq!(
                url.query_pairs().find(|(key, _)| key == "q").unwrap().1,
                query
            );
            assert_eq!(
                resolve("https://example.com/path?q=1#part", engine).unwrap(),
                "https://example.com/path?q=1#part"
            );
            assert!(resolve("https://user:secret@example.com", engine).is_err());
        }
    }
    #[test]
    fn layout_and_site_theme_preferences_survive_restart() {
        let s = Settings::from_json(r#"{"SearchEngine":"duckduckgo","SiteColorScheme":"dark","DarkMode":false,"BrowserLayout":"compact"}"#).unwrap();
        assert_eq!(s.search_engine, "duckduckgo");
        assert_eq!(s.site_color_scheme, "dark");
        assert!(s.dark_mode);
        assert_eq!(s.page_origin(), (72., 94.));
        let mut restored = Settings::from_json(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.browser_layout, "compact");
        restored.browser_layout = "unified".into();
        assert_eq!(restored.page_origin(), (280., 42.));
        restored.sidebar_visible = false;
        assert_eq!(restored.page_origin(), (0., 42.));
        let invalid = Settings::from_json(
            r#"{"SearchEngine":"bad","SiteColorScheme":"bad","BrowserLayout":"bad"}"#,
        )
        .unwrap();
        assert_eq!(invalid.search_engine, "google");
        assert_eq!(invalid.site_color_scheme, "system");
        assert_eq!(invalid.browser_layout, "split");
        assert_eq!(
            Settings::from_json(r#"{"DarkMode":true}"#)
                .unwrap()
                .site_color_scheme,
            "dark"
        );
    }
    #[test]
    fn unicode_domains() {
        assert!(
            resolve("пример.рф", "google")
                .unwrap()
                .starts_with("https://xn--")
        );
    }
    #[test]
    fn uppercase_http_scheme() {
        assert_eq!(
            resolve(" HTTPS://EXAMPLE.COM/path ", "google").unwrap(),
            "https://example.com/path"
        );
        assert!(resolve("HTTPS://user:secret@example.com", "google").is_err());
    }
    #[test]
    fn credentials_are_not_sent_to_search() {
        assert!(resolve("https://user:secret@example.com", "google").is_err());
        assert!(!is_web("https://user:secret@example.com"));
    }
    #[test]
    fn script_is_not_executed() {
        assert!(
            resolve("javascript:alert(1)", "google")
                .unwrap()
                .starts_with("https://www.google.com/search?")
        );
    }
    #[test]
    fn origin_isolation() {
        assert!(secure_origin("http://example.com").is_none());
        assert_eq!(
            secure_origin("https://example.com:8443/login"),
            Some("https://example.com:8443".into())
        );
        assert_ne!(
            secure_origin("https://example.com:8443"),
            secure_origin("https://example.com")
        );
    }
    #[test]
    fn legacy_settings() {
        let s = Settings::from_json(r#"{"ClearOnExit":true}"#).unwrap();
        assert!(s.clear_cache_on_exit && s.clear_cookies_on_exit);
        let s = Settings::from_json(r#"{"ClearOnExit":true,"ClearCookiesOnExit":false}"#).unwrap();
        assert!(s.clear_cache_on_exit && !s.clear_cookies_on_exit);
    }
    #[test]
    fn settings_validation() {
        let s = Settings::from_json(
            r#"{"GlassTransparency":200,"DiscardMinutes":1,"Zoom":5,"Language":"xx"}"#,
        )
        .unwrap();
        assert_eq!(s.glass_transparency, 100);
        assert_eq!(s.discard_minutes, 5);
        assert_eq!(s.zoom, 1.);
        assert_eq!(s.language, "ru-RU");
    }
    #[test]
    fn interface_language_migrates_and_persists() {
        let s = Settings::from_json(r#"{"Language":"be-BY","TranslationLanguage":"be"}"#).unwrap();
        assert_eq!(s.language, "ru-RU");
        assert_eq!(s.translation_language, "ru");
        let s = Settings::from_json(r#"{"Language":"en-US"}"#).unwrap();
        let saved = serde_json::to_string(&s).unwrap();
        assert_eq!(Settings::from_json(&saved).unwrap().language, "en-US");
    }
    #[test]
    fn settings_persist() {
        let dir = std::env::temp_dir().join(format!("slim-rust-test-{}", std::process::id()));
        let mut s = Store::open(dir.clone(), false).unwrap();
        s.settings.sidebar_visible = false;
        s.settings.glass_transparency = 27;
        s.settings.clear_cache_on_exit = true;
        s.settings.search_engine = "bing".into();
        s.settings.site_color_scheme = "dark".into();
        s.settings.browser_layout = "unified".into();
        s.save_settings().unwrap();
        let s = Store::open(dir.clone(), false).unwrap();
        assert!(!s.settings.sidebar_visible);
        assert_eq!(s.settings.glass_transparency, 27);
        assert!(s.settings.clear_cache_on_exit && !s.settings.clear_cookies_on_exit);
        assert_eq!(s.settings.search_engine, "bing");
        assert_eq!(s.settings.site_color_scheme, "dark");
        assert_eq!(s.settings.browser_layout, "unified");
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn library_deduplication() {
        let mut entries = vec![];
        add_entry(&mut entries, "A", "https://example.com", 10);
        add_entry(&mut entries, "B", "https://example.com", 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "B");
        add_entry(&mut entries, "C", "javascript:bad", 10);
        assert_eq!(entries.len(), 1);
    }
    #[test]
    fn dpapi_roundtrip() {
        let bytes = b"unique-test-secret";
        let encrypted = crate::platform::encrypt(bytes).unwrap();
        assert!(!encrypted.windows(bytes.len()).any(|w| w == bytes));
        assert_eq!(crate::platform::decrypt(&encrypted).unwrap(), bytes);
        assert!(crate::platform::decrypt(&[1, 2, 3]).is_err());
    }
    #[test]
    fn script_serialization() {
        let c = Credential {
            origin: "https://example.com".into(),
            username: "\";alert(1)//".into(),
            password: "secret\"\\".into(),
        };
        let script = autofill_script(&c).unwrap();
        assert!(script.contains("location.origin!==c.Origin"));
        assert!(script.contains("window.top!==window"));
        assert!(script.contains("\\\";alert(1)//"));
    }
}

/// Reorder existing items without cloning or changing their identity.
pub fn move_before<T>(items: &mut Vec<T>, from: usize, before: Option<usize>) {
    let before = before.unwrap_or(items.len());
    if from >= items.len() || before > items.len() || before == from {
        return;
    }
    let item = items.remove(from);
    items.insert(before - usize::from(from < before), item);
}
#[cfg(test)]
mod reorder_tests {
    #[test]
    fn moving_items_preserves_identity_and_handles_both_directions() {
        let mut items = vec![1, 2, 3, 4];
        super::move_before(&mut items, 0, Some(3));
        assert_eq!(items, vec![2, 3, 1, 4]);
        super::move_before(&mut items, 3, Some(0));
        assert_eq!(items, vec![4, 2, 3, 1]);
        super::move_before(&mut items, 1, None);
        assert_eq!(items, vec![4, 3, 1, 2]);
        super::move_before(&mut items, 1, Some(1));
        super::move_before(&mut items, 99, None);
        assert_eq!(items, vec![4, 3, 1, 2]);
    }
}
