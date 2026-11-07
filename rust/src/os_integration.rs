use crate::{Event, model};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    time::Duration,
};
use tao::event_loop::EventLoopProxy;
#[derive(Serialize, Deserialize)]
struct Channel {
    port: u16,
    token: String,
    pid: u32,
}
pub fn profile() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA missing")?)
            .join("SlimBrowserRust"),
    )
}
pub fn launch_urls(args: &[String]) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter_map(|s| {
            if model::is_page_url(s) {
                Some(s.clone())
            } else if !s.starts_with('-') && Path::new(s).is_absolute() && Path::new(s).is_file() {
                url::Url::from_file_path(s)
                    .ok()
                    .map(String::from)
                    .filter(|u| model::is_local_document(u))
            } else {
                None
            }
        })
        .take(32)
        .collect()
}
pub fn start(directory: &Path, proxy: EventLoopProxy<Event>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let token = format!("{:?}", unsafe {
        windows::Win32::System::Com::CoCreateGuid()?
    });
    let channel = Channel {
        port: listener.local_addr()?.port(),
        token: token.clone(),
        pid: std::process::id(),
    };
    model::write_atomic(
        &directory.join("launch-channel.json"),
        &serde_json::to_vec(&channel)?,
    )?;
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(700)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(700)));
            let mut line = String::new();
            if BufReader::new((&mut stream).take(65537))
                .read_line(&mut line)
                .is_err()
                || line.len() > 65536
            {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value["token"].as_str() != Some(token.as_str()) {
                continue;
            }
            let Some(urls) = value["urls"].as_array() else {
                continue;
            };
            let urls = urls
                .iter()
                .filter_map(|u| u.as_str())
                .filter(|u| model::is_web(u) || model::is_local_document(u))
                .take(32)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if proxy.send_event(Event::ExternalOpen(urls)).is_ok() {
                let _ = stream.write_all(b"OK\n");
            }
        }
    });
    Ok(())
}
pub fn forward(urls: &[String]) -> Result<()> {
    forward_at(&profile()?, urls)
}
pub fn forward_at(directory: &Path, urls: &[String]) -> Result<()> {
    let bytes = std::fs::read(directory.join("launch-channel.json"))
        .context("Закройте предыдущую версию браузера и запустите обновлённую")?;
    ensure!(bytes.len() < 1024, "Некорректный канал запуска");
    let channel: Channel = serde_json::from_slice(&bytes)?;
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(channel.pid);
    }
    let address: SocketAddr = ([127, 0, 0, 1], channel.port).into();
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut data = serde_json::to_vec(&serde_json::json!({"token":channel.token,"urls":urls}))?;
    data.push(b'\n');
    ensure!(data.len() < 65536, "Слишком много адресов");
    stream.write_all(&data)?;
    let mut ack = [0; 3];
    stream.read_exact(&mut ack)?;
    ensure!(&ack == b"OK\n", "Браузер не принял ссылку");
    Ok(())
}
const BROWSER_CAPABILITIES: &str = r"Software\Clients\StartMenuInternet\Slim Browser\Capabilities";

fn machine_registration_matches() -> bool {
    use windows::Win32::System::Registry::*;
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let expected = format!("\"{}\" --open-url \"%1\"", exe.to_string_lossy());
    let required = [
        (
            r"Software\RegisteredApplications",
            "Slim Browser",
            BROWSER_CAPABILITIES,
        ),
        (
            r"Software\Classes\SlimBrowser.Url\shell\open\command",
            "",
            expected.as_str(),
        ),
        (
            r"Software\Classes\SlimBrowser.Html\shell\open\command",
            "",
            expected.as_str(),
        ),
        (
            r"Software\Clients\StartMenuInternet\Slim Browser\Capabilities\URLAssociations",
            "http",
            "SlimBrowser.Url",
        ),
        (
            r"Software\Clients\StartMenuInternet\Slim Browser\Capabilities\URLAssociations",
            "https",
            "SlimBrowser.Url",
        ),
        (
            r"Software\Clients\StartMenuInternet\Slim Browser\Capabilities\FileAssociations",
            ".htm",
            "SlimBrowser.Html",
        ),
        (
            r"Software\Clients\StartMenuInternet\Slim Browser\Capabilities\FileAssociations",
            ".html",
            "SlimBrowser.Html",
        ),
    ];
    required.iter().all(|(path, name, expected)| {
        read_registry_string(HKEY_LOCAL_MACHINE, path, name).as_deref() == Some(*expected)
    })
}

fn read_registry_string(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    name: &str,
) -> Option<String> {
    use windows::{Win32::System::Registry::*, core::PCWSTR};
    let path = crate::platform::wide(path);
    let name = crate::platform::wide(name);
    let mut length = 0;
    let flags = RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY;
    unsafe {
        RegGetValueW(
            root,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            None,
            None,
            Some(&mut length),
        )
        .ok()
        .ok()?;
        if length == 0 || length > 65536 {
            return None;
        }
        let mut value = vec![0u16; (length as usize).div_ceil(2)];
        RegGetValueW(
            root,
            PCWSTR(path.as_ptr()),
            PCWSTR(name.as_ptr()),
            flags,
            None,
            Some(value.as_mut_ptr().cast()),
            Some(&mut length),
        )
        .ok()
        .ok()?;
        let end = value.iter().position(|v| *v == 0).unwrap_or(value.len());
        String::from_utf16(&value[..end]).ok()
    }
}

pub fn register_browser() -> Result<()> {
    // Do not shadow a verified system registration with a second per-user entry.
    if machine_registration_matches() {
        return Ok(());
    }
    use windows::Win32::System::Registry::*;
    use windows::Win32::UI::Shell::*;
    use windows::core::PCWSTR;
    let exe = std::env::current_exe()?.to_string_lossy().into_owned();
    let command = format!("\"{exe}\" --open-url \"%1\"");
    let base = r"Software\Clients\StartMenuInternet\Slim Browser";
    let cap = format!(r"{base}\Capabilities");
    let progid = r"Software\Classes\SlimBrowser.Html";
    let mut values = vec![
        (base.into(), "", "Slim Browser".into()),
        (format!(r"{base}\DefaultIcon"), "", format!("{exe},0")),
        (
            format!(r"{base}\shell\open\command"),
            "",
            format!("\"{exe}\""),
        ),
        (cap.clone(), "ApplicationName", "Slim Browser".into()),
        (
            cap.clone(),
            "ApplicationDescription",
            "Slim Browser для веб-сайтов и ссылок".into(),
        ),
        (cap.clone(), "ApplicationIcon", format!("{exe},0")),
        (progid.into(), "", "Slim Browser HTML".into()),
        (progid.into(), "URL Protocol", "".into()),
        (progid.into(), "FriendlyTypeName", "Slim Browser".into()),
        (
            format!(r"{progid}\Application"),
            "ApplicationName",
            "Slim Browser".into(),
        ),
        (
            format!(r"{progid}\Application"),
            "ApplicationIcon",
            format!("{exe},0"),
        ),
        (
            format!(r"{progid}\Application"),
            "ApplicationDescription",
            "Slim Browser для веб-сайтов и ссылок".into(),
        ),
        (
            format!(r"{progid}\Application"),
            "ApplicationCompany",
            "Slim Browser".into(),
        ),
        (format!(r"{progid}\DefaultIcon"), "", format!("{exe},0")),
        (format!(r"{progid}\shell\open\command"), "", command.clone()),
        (
            r"Software\Microsoft\Windows\CurrentVersion\App Paths\SlimBrowser.exe".into(),
            "",
            exe.clone(),
        ),
    ];
    values.push((
        format!(r"{cap}\Startmenu"),
        "StartMenuInternet",
        "Slim Browser".into(),
    ));
    let url_progid = r"Software\Classes\SlimBrowser.Url";
    values.extend([
        (url_progid.into(), "", "Slim Browser URL".into()),
        (url_progid.into(), "URL Protocol", "".into()),
        (format!(r"{url_progid}\DefaultIcon"), "", format!("{exe},0")),
        (format!(r"{url_progid}\shell\open\command"), "", command),
        (
            format!(r"{url_progid}\Application"),
            "ApplicationName",
            "Slim Browser".into(),
        ),
        (
            format!(r"{url_progid}\Application"),
            "ApplicationDescription",
            "Slim Browser для веб-сайтов и ссылок".into(),
        ),
        (
            format!(r"{url_progid}\Application"),
            "ApplicationIcon",
            format!("{exe},0"),
        ),
    ]);
    for protocol in ["http", "https"] {
        values.push((
            format!(r"{cap}\URLAssociations"),
            protocol,
            "SlimBrowser.Url".into(),
        ));
    }
    for extension in [".htm", ".html"] {
        values.push((
            format!(r"{cap}\FileAssociations"),
            extension,
            "SlimBrowser.Html".into(),
        ));
        values.push((
            format!(r"Software\Classes\{extension}\OpenWithProgids"),
            "SlimBrowser.Html",
            "".into(),
        ));
    }
    // Publish capabilities only after every ProgID and association is complete.
    values.push((
        r"Software\RegisteredApplications".into(),
        "Slim Browser",
        cap.clone(),
    ));
    // Opening an existing key preserves its other values; the old PowerShell
    // New-Item -Force recreated keys and erased earlier capabilities.
    for (path, name, value) in values {
        let path = crate::platform::wide(&path);
        let name = crate::platform::wide(name);
        let data = crate::platform::wide(&value)
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut key = HKEY::default();
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                None,
                &mut key,
                None,
            )
            .ok()?;
            let result = RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&data));
            let _ = RegCloseKey(key);
            result.ok()?;
        }
    }
    unsafe {
        // Migrate the old registration whose key did not match ApplicationName.
        let registered = crate::platform::wide(r"Software\RegisteredApplications");
        let legacy = crate::platform::wide("SlimBrowser");
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(registered.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
        .is_ok()
        {
            let _ = RegDeleteValueW(key, PCWSTR(legacy.as_ptr()));
            let _ = RegCloseKey(key);
        }
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
    Ok(())
}
pub fn show_default_settings() -> Result<()> {
    register_browser()?;
    let parameter = if machine_registration_matches() {
        "registeredAppMachine"
    } else {
        "registeredAppUser"
    };
    crate::platform::open_system_url(&format!(
        "ms-settings:defaultapps?{parameter}=Slim%20Browser"
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_input_only_accepts_supported_targets() {
        assert_eq!(
            launch_urls(&[
                "app".into(),
                "--open-url".into(),
                "https://example.com/a?q=1".into(),
                "javascript:alert(1)".into(),
                "https://u:p@example.com".into()
            ]),
            vec!["https://example.com/a?q=1"]
        );
    }
}
