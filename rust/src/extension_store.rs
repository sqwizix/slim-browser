use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    time::Duration,
};
const CHROME_PACKAGES: &[(&str, &str, &str, &str)] = &[
    (
        "faceit",
        "EXT — Faceit Intelligence",
        "Статистика и инструменты FACEIT. Официальный пакет разработчика.",
        "ikkiomngodfmfdmeomohmjeobdgpholf",
    ),
    (
        "tunnelbear",
        "TunnelBear VPN",
        "VPN для браузера. Требуется аккаунт TunnelBear; подключение проверяется в панели расширения.",
        "omdakjcmkglenbhjadbccaookpfjihpa",
    ),
    (
        "ambient",
        "Ambient light for YouTube™",
        "Подсветка вокруг видео YouTube. Официальное расширение WesselKroos.",
        "paponcgjfojgemddooebbgniglhkajkj",
    ),
];
fn chrome_download(id: &str) -> String {
    let mut url = url::Url::parse("https://clients2.google.com/service/update2/crx").unwrap();
    url.query_pairs_mut()
        .append_pair("response", "redirect")
        .append_pair("prodversion", "154.0.0.0")
        .append_pair("acceptformat", "crx3")
        .append_pair("x", &format!("id={id}&installsource=ondemand&uc"));
    url.into()
}

const PACKAGES: &[(&str, &str, &str, &str)] = &[
    (
        "ubolite",
        "uBlock Origin Lite",
        "Блокировка рекламы и сетевых трекеров.",
        "uBlockOrigin/uBOL-home",
    ),
    (
        "darkreader",
        "Dark Reader",
        "Тёмное оформление страниц с настройкой яркости и контраста.",
        "darkreader/darkreader",
    ),
    (
        "stylus",
        "Stylus",
        "Пользовательские стили для оформления сайтов.",
        "openstyles/stylus",
    ),
];
#[derive(Clone, Debug, Serialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub version: String,
    pub download: String,
    pub error: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PendingInstall {
    pub id: String,
    pub name: String,
    pub version: String,
    pub permissions: Vec<String>,
    #[serde(skip)]
    pub folder: PathBuf,
}
fn client(proxy: Option<std::net::SocketAddr>) -> Result<reqwest::blocking::Client> {
    let builder = reqwest::blocking::Client::builder()
        .https_only(true)
        .no_proxy()
        .user_agent("SlimBrowser/0.1 (+https://github.com)")
        .timeout(Duration::from_secs(120));
    Ok(match proxy {
        Some(address) => builder.proxy(reqwest::Proxy::all(format!("socks5h://{address}"))?),
        None => builder,
    }
    .build()?)
}
fn limited(response: reqwest::blocking::Response, limit: u64) -> Result<Vec<u8>> {
    let response = response.error_for_status()?;
    ensure!(
        response.content_length().unwrap_or(0) <= limit,
        "Файл превышает допустимый размер"
    );
    let mut bytes = vec![];
    response.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Файл превышает допустимый размер"
    );
    Ok(bytes)
}
fn asset_link(html: &str, repo: &str) -> Option<String> {
    let prefix = format!("/{repo}/releases/download/");
    html.split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .find(|path| {
            let lower = path.to_ascii_lowercase();
            path.starts_with(&prefix)
                && lower.ends_with(".zip")
                && (lower.contains("chromium") || lower.contains("chrome"))
                && !lower.contains("mv2")
        })
        .map(|path| format!("https://github.com{path}"))
}
fn latest(client: &reqwest::blocking::Client, repo: &str) -> Result<(String, String)> {
    // GitHub's public release pages avoid the low anonymous API rate limit.
    let response = client
        .get(format!("https://github.com/{repo}/releases/latest"))
        .send()?
        .error_for_status()?;
    ensure!(
        response.url().host_str() == Some("github.com"),
        "Неожиданный адрес выпуска"
    );
    let prefix = format!("/{repo}/releases/tag/");
    let version = response
        .url()
        .path()
        .strip_prefix(&prefix)
        .context("Последний выпуск не найден")?
        .to_owned();
    ensure!(!version.is_empty(), "Не найден номер выпуска");
    let assets = String::from_utf8(limited(
        client
            .get(format!(
                "https://github.com/{repo}/releases/expanded_assets/{version}"
            ))
            .send()?,
        2 * 1024 * 1024,
    )?)?;
    Ok((
        version,
        asset_link(&assets, repo).context("В последнем выпуске нет ZIP для Chromium")?,
    ))
}
pub fn builtin_catalog() -> Vec<Package> {
    CHROME_PACKAGES
        .iter()
        .map(|(id, name, description, chrome_id)| Package {
            id: (*id).into(),
            name: (*name).into(),
            description: (*description).into(),
            source: format!("https://chromewebstore.google.com/detail/{chrome_id}"),
            version: "Актуальная версия из Chrome Web Store".into(),
            download: chrome_download(chrome_id),
            error: String::new(),
        })
        .collect::<Vec<_>>()
}
pub fn catalog(proxy: Option<std::net::SocketAddr>) -> Vec<Package> {
    let client = client(proxy);
    let mut packages = builtin_catalog();
    packages.extend(
        PACKAGES
            .iter()
            .map(|(id, name, description, repo)| {
                let result = client
                    .as_ref()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))
                    .and_then(|c| latest(c, repo));
                let (version, download, error) = match result {
                    Ok((v, d)) => (v, d, String::new()),
                    Err(e) => (String::new(), String::new(), e.to_string()),
                };
                Package {
                    id: (*id).into(),
                    name: (*name).into(),
                    description: (*description).into(),
                    source: format!("https://github.com/{repo}"),
                    version,
                    download,
                    error,
                }
            })
            .collect::<Vec<_>>(),
    );
    packages
}
pub fn prepare(
    package: &Package,
    directory: &Path,
    proxy: Option<std::net::SocketAddr>,
) -> Result<PendingInstall> {
    let chrome_id = CHROME_PACKAGES
        .iter()
        .find(|(id, _, _, _)| *id == package.id)
        .map(|(_, _, _, id)| *id);
    let repo = PACKAGES
        .iter()
        .find(|(id, _, _, _)| *id == package.id)
        .map(|(_, _, _, repo)| *repo);
    ensure!(
        repo.is_some() || chrome_id.is_some(),
        "Неизвестное расширение"
    );
    ensure!(
        chrome_id.is_some_and(|id| package.download == chrome_download(id))
            || repo.is_some_and(|repo| package
                .download
                .starts_with(&format!("https://github.com/{repo}/releases/download/"))),
        "Некорректный адрес пакета"
    );
    let bytes = limited(
        client(proxy)?.get(&package.download).send()?,
        128 * 1024 * 1024,
    )?;
    let folder = directory.join(format!(
        "{}-{}",
        package.id,
        chrono::Utc::now().timestamp_micros()
    ));
    fs::create_dir_all(&folder)?;
    let result = (|| {
        let (payload, public_key) = if let Some(id) = chrome_id {
            crx_payload(&bytes, id)?
        } else {
            (bytes.as_slice(), None)
        };
        extract(payload, &folder)?;
        let manifest_folder = if folder.join("manifest.json").is_file() {
            folder.clone()
        } else {
            fs::read_dir(&folder)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .find(|p| p.is_dir() && p.join("manifest.json").is_file())
                .context("В пакете нет manifest.json")?
        };
        let raw = fs::read(manifest_folder.join("manifest.json"))?;
        ensure!(raw.len() <= 1024 * 1024, "Слишком большой manifest.json");
        let mut manifest: Value = serde_json::from_slice(&raw)?;
        if let Some(key) = public_key {
            // Preserve the original extension identity after unpacking a CRX3.
            manifest["key"] = Value::String(key);
            crate::model::write_atomic(
                &manifest_folder.join("manifest.json"),
                &serde_json::to_vec_pretty(&manifest)?,
            )?;
        }
        ensure!(
            manifest["manifest_version"] == 3,
            "Пакет использует устаревший Manifest V2"
        );
        if let Some(id) = chrome_id {
            apply_compatibility(&manifest_folder, id)?;
        }
        let mut permissions = vec![];
        for field in [
            "permissions",
            "host_permissions",
            "optional_permissions",
            "optional_host_permissions",
        ] {
            if let Some(items) = manifest[field].as_array() {
                for item in items.iter().filter_map(Value::as_str) {
                    permissions.push(if field.starts_with("optional") {
                        format!("По запросу: {item}")
                    } else {
                        item.into()
                    });
                }
            }
        }
        if let Some(scripts) = manifest["content_scripts"].as_array() {
            for script in scripts {
                if let Some(matches) = script["matches"].as_array() {
                    for host in matches.iter().filter_map(Value::as_str) {
                        permissions.push(format!("Скрипты на страницах: {host}"));
                    }
                }
            }
        }
        permissions.sort();
        permissions.dedup();
        let popup = manifest["action"]["default_popup"]
            .as_str()
            .or_else(|| manifest["options_ui"]["page"].as_str())
            .or_else(|| manifest["options_page"].as_str())
            .unwrap_or("");
        crate::model::write_atomic(
            &manifest_folder.join("slim-package.json"),
            &serde_json::to_vec(
                &serde_json::json!({"name":package.name,"source":package.source,"popup":popup}),
            )?,
        )?;
        Ok(PendingInstall {
            id: package.id.clone(),
            name: package.name.clone(),
            version: manifest["version"]
                .as_str()
                .context("Нет версии расширения")?
                .into(),
            permissions,
            folder: manifest_folder,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&folder);
    }
    result
}
// Parse only length-delimited protobuf fields in Google's CRX3 header. Identity
// is checked against the expected store ID and the SHA-256 of the publisher key.
// Transport authenticity is provided by HTTPS to the fixed Google update URL.
fn varint(bytes: &[u8], offset: &mut usize) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*offset).context("Оборванный заголовок CRX")?;
        *offset += 1;
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("Некорректный заголовок CRX")
}
fn fields(bytes: &[u8]) -> Result<Vec<(u64, &[u8])>> {
    let mut offset = 0;
    let mut result = vec![];
    while offset < bytes.len() {
        let tag = varint(bytes, &mut offset)?;
        ensure!(tag & 7 == 2, "Неизвестное поле CRX");
        let len = usize::try_from(varint(bytes, &mut offset)?)?;
        let end = offset
            .checked_add(len)
            .context("Слишком длинный заголовок")?;
        result.push((tag >> 3, bytes.get(offset..end).context("Оборванный CRX")?));
        offset = end;
    }
    Ok(result)
}
fn crx_payload<'a>(bytes: &'a [u8], id: &str) -> Result<(&'a [u8], Option<String>)> {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    ensure!(
        bytes.get(..4) == Some(b"Cr24") && bytes.get(4..8) == Some(&3u32.to_le_bytes()),
        "Ожидался официальный пакет CRX3"
    );
    let len = u32::from_le_bytes(bytes.get(8..12).context("Оборванный CRX")?.try_into()?) as usize;
    ensure!(len <= 1024 * 1024, "Слишком большой заголовок CRX");
    let header = fields(
        bytes
            .get(12..12 + len)
            .context("Оборванный заголовок CRX")?,
    )?;
    let signed = header
        .iter()
        .find(|(n, _)| *n == 10000)
        .context("Нет идентификатора CRX")?;
    let signed_fields = fields(signed.1)?;
    let crx_id = signed_fields
        .iter()
        .find(|(n, _)| *n == 1)
        .context("Нет ID расширения")?
        .1;
    let encode_id = |bytes: &[u8]| {
        bytes
            .iter()
            .flat_map(|b| [char::from(b'a' + (b >> 4)), char::from(b'a' + (b & 15))])
            .collect::<String>()
    };
    ensure!(
        crx_id.len() == 16 && encode_id(crx_id) == id,
        "ID пакета не совпадает с выбранным расширением"
    );
    let mut key = None;
    for (_, proof) in header.iter().filter(|(n, _)| *n == 2 || *n == 3) {
        for (n, value) in fields(proof)? {
            if n == 1 && encode_id(&Sha256::digest(value)[..16]) == id {
                key = Some(base64::engine::general_purpose::STANDARD.encode(value));
            }
        }
    }
    ensure!(key.is_some(), "Ключ издателя не совпадает с ID расширения");
    Ok((bytes.get(12 + len..).context("Нет содержимого CRX")?, key))
}
pub fn stage_crx(bytes: &[u8], id: &str, folder: &Path) -> Result<()> {
    let (payload, key) = crx_payload(bytes, id)?;
    fs::create_dir_all(folder)?;
    extract(payload, folder)?;
    let path = folder.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path)?)?;
    ensure!(manifest["manifest_version"] == 3, "Manifest V3 required");
    manifest["key"] = serde_json::to_value(key.context("No publisher key")?)?;
    apply_compatibility(folder, id)?;
    crate::model::write_atomic(&path, &serde_json::to_vec_pretty(&manifest)?)
}
pub fn apply_compatibility(folder: &Path, id: &str) -> Result<()> {
    if id != "ikkiomngodfmfdmeomohmjeobdgpholf" {
        return Ok(());
    }
    let path = folder.join("wallpaper.html");
    if !path.is_file() {
        return Ok(());
    }
    let html = fs::read_to_string(&path)?;
    let marker = "<script src=\"wallpaper.js\">";
    if !html.contains(marker) {
        return Ok(());
    }
    crate::model::write_atomic(
        &folder.join("slim-wallpapers.js"),
        include_bytes!("../ui/ext-wallpapers.js"),
    )?;
    if !html.contains("src=\"slim-wallpapers.js\"") {
        crate::model::write_atomic(
            &path,
            html.replacen(
                marker,
                &format!("<script src=\"slim-wallpapers.js\"></script>{marker}"),
                1,
            )
            .as_bytes(),
        )?;
    }
    Ok(())
}
fn extract(bytes: &[u8], folder: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(archive.len() <= 10000, "В пакете слишком много файлов");
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let relative = file
            .enclosed_name()
            .context("Пакет содержит путь за пределами папки")?;
        ensure!(
            !relative
                .components()
                .any(|c| c.as_os_str().to_string_lossy().contains(':')),
            "Недопустимый путь в пакете"
        );
        ensure!(
            file.unix_mode().unwrap_or(0) & 0o170000 != 0o120000,
            "Ссылки в пакете не поддерживаются"
        );
        total = total
            .checked_add(file.size())
            .context("Слишком большой пакет")?;
        ensure!(
            total <= 384 * 1024 * 1024,
            "Распакованный пакет превышает 384 МБ"
        );
        let destination = folder.join(&relative);
        if file.is_dir() {
            fs::create_dir_all(&destination)?;
        } else {
            fs::create_dir_all(destination.parent().context("Нет родительской папки")?)?;
            let mut output = fs::File::create(&destination)?;
            std::io::copy(&mut file, &mut output)?;
        }
    }
    Ok(())
}
pub fn register(directory: &Path, extension_id: &str, pending: &PendingInstall) -> Result<()> {
    let path = directory.join("installed-extensions.json");
    let mut registry: Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        serde_json::json!({})
    };
    let meta = pending.folder.join("slim-package.json");
    let mut info: Value = if meta.exists() {
        serde_json::from_slice(&fs::read(meta)?)?
    } else {
        serde_json::json!({"name":pending.name,"popup":""})
    };
    info["folder"] = serde_json::to_value(&pending.folder)?;
    registry[extension_id] = info;
    crate::model::write_atomic(&path, &serde_json::to_vec_pretty(&registry)?)
}
pub fn popup_url(directory: &Path, id: &str) -> Result<String> {
    ensure!(
        id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)),
        "Некорректный ID расширения"
    );
    let path = directory.join("installed-extensions.json");
    let registry: Value = serde_json::from_slice(
        &fs::read(&path)
            .context("Установите расширение из магазина Slim Browser, чтобы открыть его панель")?,
    )?;
    let popup = registry[id]["popup"]
        .as_str()
        .filter(|s| !s.is_empty())
        .context("У расширения нет панели. Оно работает на страницах сайтов.")?;
    let base = url::Url::parse(&format!("chrome-extension://{id}/"))?;
    let url = base.join(popup)?;
    ensure!(
        url.scheme() == "chrome-extension" && url.host_str() == Some(id),
        "Некорректный адрес панели"
    );
    Ok(url.into())
}
pub fn popup_context_script(source_url: &str) -> String {
    let source = serde_json::to_string(source_url).unwrap_or_default();
    format!(
        r#"(()=>{{
      if(location.protocol!=='chrome-extension:'||!globalThis.chrome?.tabs?.query)return;
      const source={source},query=chrome.tabs.query.bind(chrome.tabs);
      if(globalThis.ipc?.postMessage){{
        const nativeCreate=chrome.tabs.create.bind(chrome.tabs);
        chrome.tabs.create=function(info,callback){{
          const target=new URL(info.url,location.href);
          if(!['https:','http:'].includes(target.protocol)&&!(target.protocol==='chrome-extension:'&&target.host===location.host))return nativeCreate(info,callback);
          ipc.postMessage(JSON.stringify({{action:'open',url:target.href}}));
          const result=new Promise((resolve,reject)=>{{let attempts=0;const find=()=>query({{url:target.href}},tabs=>{{if(tabs?.length)resolve(tabs[tabs.length-1]);else if(++attempts<40)setTimeout(find,100);else reject(Error('Slim Browser could not open extension tab'))}});setTimeout(find,100)}});
          if(callback){{result.then(callback);return;}}return result;
        }};
      }}
      chrome.tabs.query=function(info,callback){{
        if(!info?.active||!/^https?:\/\//.test(source))return query(info,callback);
        const target={{...info}};delete target.currentWindow;delete target.lastFocusedWindow;delete target.windowId;
        const select=tabs=>{{const found=tabs.filter(t=>t.url===source);return found.length?[found[0]]:tabs.filter(t=>/^https?:\/\//.test(t.url||'')).slice(0,1)}};
        if(typeof callback==='function')return query(target,tabs=>callback(select(tabs||[])));
        return query(target).then(select);
      }};
    }})()"#
    )
}

pub fn registered_url(directory: &Path, url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if url.scheme() != "chrome-extension" {
        return false;
    }
    let Some(id) = url.host_str() else {
        return false;
    };
    let Ok(bytes) = fs::read(directory.join("installed-extensions.json")) else {
        return false;
    };
    serde_json::from_slice::<Value>(&bytes).is_ok_and(|r| r.get(id).is_some())
}
pub fn icon(directory: &Path, id: &str) -> String {
    use base64::Engine;
    let result = (|| -> Result<String> {
        let r: Value =
            serde_json::from_slice(&fs::read(directory.join("installed-extensions.json"))?)?;
        let folder = PathBuf::from(r[id]["folder"].as_str().context("No folder")?);
        let m: Value = serde_json::from_slice(&fs::read(folder.join("manifest.json"))?)?;
        let path = ["128", "48", "32", "16"]
            .iter()
            .find_map(|size| m["icons"][*size].as_str())
            .context("No icon")?;
        let path = Path::new(path);
        ensure!(
            path.is_relative()
                && !path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "Invalid icon path"
        );
        let bytes = fs::read(folder.join(path))?;
        ensure!(
            bytes.len() < 1024 * 1024 && bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "Expected PNG icon"
        );
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })();
    result.unwrap_or_default()
}
pub fn popup_dimensions(id: &str) -> (f64, f64) {
    match id {
        "ikkiomngodfmfdmeomohmjeobdgpholf" => (356., 566.),
        "omdakjcmkglenbhjadbccaookpfjihpa" => (400., 460.),
        _ => (400., 560.),
    }
}
pub fn popup_style_script(id: &str) -> String {
    let color = if id == "ikkiomngodfmfdmeomohmjeobdgpholf" {
        "#0a0a0c"
    } else {
        "#202124"
    };
    format!(
        "addEventListener('DOMContentLoaded',()=>{{const s=document.createElement('style');s.textContent='html{{margin:0!important;padding:0!important;background:{color};scrollbar-width:thin;overflow-x:hidden}}body{{margin:0!important;width:100vw!important;height:100vh!important}}';document.head.append(s)}},{{once:true}})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wallpaper_adapter_is_scoped_and_idempotent() {
        let folder = std::env::temp_dir().join(format!("slim-ext-adapter-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        let html = "<script src=\"wallpaper.js\"></script>";
        fs::write(folder.join("wallpaper.html"), html).unwrap();
        apply_compatibility(&folder, "another-extension").unwrap();
        assert_eq!(
            fs::read_to_string(folder.join("wallpaper.html")).unwrap(),
            html
        );
        apply_compatibility(&folder, "ikkiomngodfmfdmeomohmjeobdgpholf").unwrap();
        apply_compatibility(&folder, "ikkiomngodfmfdmeomohmjeobdgpholf").unwrap();
        let adapted = fs::read_to_string(folder.join("wallpaper.html")).unwrap();
        assert_eq!(adapted.matches("slim-wallpapers.js").count(), 1);
        assert!(
            adapted.find("slim-wallpapers.js").unwrap() < adapted.find("wallpaper.js").unwrap()
        );
        assert!(folder.join("slim-wallpapers.js").is_file());
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    #[ignore = "Downloads current packages from the developers' release pages"]
    fn live_catalog_and_packages_have_valid_manifests() {
        let packages = catalog(None);
        let directory =
            std::env::temp_dir().join(format!("SlimBrowser-store-test-{}", std::process::id()));
        for package in packages {
            assert!(
                package.error.is_empty(),
                "{}: {}",
                package.name,
                package.error
            );
            let pending = prepare(&package, &directory, None).unwrap();
            assert!(pending.folder.join("manifest.json").is_file());
            assert!(!pending.version.is_empty());
            println!(
                "{} {}: {} permissions",
                pending.name,
                pending.version,
                pending.permissions.len()
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn selects_only_chromium_asset_from_the_expected_developer() {
        let html = "<a href=\"/other/repo/releases/download/v1/chrome.zip\"></a><a href=\"/openstyles/stylus/releases/download/v1/stylus-firefox.zip\"></a><a href=\"/openstyles/stylus/releases/download/v1/stylus-chrome.zip\"></a>";
        assert_eq!(
            asset_link(html, "openstyles/stylus").unwrap(),
            "https://github.com/openstyles/stylus/releases/download/v1/stylus-chrome.zip"
        );
    }
    #[test]
    fn rejects_zip_path_traversal() {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(vec![]));
        writer
            .start_file("../outside", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"bad").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let folder = std::env::temp_dir().join(format!("slim-zip-test-{}", std::process::id()));
        fs::create_dir_all(&folder).unwrap();
        assert!(extract(&bytes, &folder).is_err());
        fs::remove_dir_all(folder).unwrap();
    }
}
