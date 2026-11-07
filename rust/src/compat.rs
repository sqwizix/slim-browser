//! Explicit, isolated compatibility probe; never opens the user's browser profile.
use super::*;
use std::{fs, path::Path, sync::mpsc};

pub fn run(app: &mut App, source: &Path) -> Result<()> {
    let output = std::env::current_exe()?.with_file_name("extension-compatibility.json");
    let core = app.service(0)?;
    let profile = engine::profile(&core)?.cast::<ICoreWebView2Profile7>()?;
    let source_view = app
        .content_builder(0)
        .with_bounds(rect(0., 94., 640., 500.))
        .build_as_child(&app.windows[0].window)?;
    smoke::map_fixture(&source_view, &app.store.borrow().directory)?;
    source_view.load_url("https://slim.test/index.html")?;
    let mut report = vec![];
    for (name, id) in [
        ("faceit", "ikkiomngodfmfdmeomohmjeobdgpholf"),
        ("tunnelbear", "omdakjcmkglenbhjadbccaookpfjihpa"),
        ("ambient", "paponcgjfojgemddooebbgniglhkajkj"),
    ] {
        let result = (|| -> Result<Value> {
            let folder = app.store.borrow().directory.join("Extensions").join(name);
            extension_store::stage_crx(
                &fs::read(source.join(format!("{name}.crx")))?,
                id,
                &folder,
            )?;
            let manifest: Value = serde_json::from_slice(&fs::read(folder.join("manifest.json"))?)?;
            let (sender, receiver) = mpsc::channel();
            unsafe {
                profile.AddBrowserExtension(
                    &HSTRING::from(folder.as_path()),
                    &ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
                        move |result, extension| {
                            let result = result
                                .and_then(|_| {
                                    extension.ok_or_else(|| {
                                        windows::core::Error::from(
                                            windows::Win32::Foundation::E_POINTER,
                                        )
                                    })
                                })
                                .and_then(|ex| engine::string(|p| ex.Id(p)));
                            let _ = sender.send(result.map_err(|e| e.to_string()));
                            Ok(())
                        },
                    )),
                )?;
            }
            let installed_id = wait_with_pump(receiver)?.map_err(anyhow::Error::msg)?;
            anyhow::ensure!(installed_id == id, "Extension identity changed");
            let popup = manifest["action"]["default_popup"]
                .as_str()
                .context("No popup")?;
            std::fs::write(
                folder.join("slim-package.json"),
                serde_json::to_vec(&json!({"name":name,"popup":popup}))?,
            )?;
            extension_store::register(
                &app.store.borrow().directory,
                id,
                &extension_store::PendingInstall {
                    id: name.into(),
                    name: name.into(),
                    version: manifest["version"].as_str().unwrap_or_default().into(),
                    permissions: vec![],
                    folder: folder.clone(),
                },
            )?;
            let expected = format!("chrome-extension://{id}/{popup}");
            let (width, height) = extension_store::popup_dimensions(id);
            let view = app.content_builder(0)
                .with_navigation_handler(|url| url == "about:blank" || url.starts_with("chrome-extension://"))
                .with_initialization_script(format!("{};globalThis.__probeErrors=[];addEventListener('error',e=>__probeErrors.push(e.message));addEventListener('unhandledrejection',e=>__probeErrors.push(String(e.reason)))",extension_store::popup_context_script("https://slim.test/index.html")))
                .with_initialization_script(extension_store::popup_style_script(id))
                .with_bounds(rect(0.,94.,width,height))
                .build_as_child(&app.windows[0].window)?;
            engine::configure(&view, &app.store.borrow().settings, false)?;
            unsafe {
                view.controller().SetZoomFactor(1.0)?;
                view.webview().Settings()?.SetIsWebMessageEnabled(true)?;
            }
            let (sender, receiver) = mpsc::channel();
            let mut token = 0;
            unsafe {
                view.webview().add_NavigationCompleted(
                    &NavigationCompletedEventHandler::create(Box::new(move |sender_core, args| {
                        if let (Some(core), Some(args)) = (sender_core, args) {
                            if engine::string(|p| core.Source(p))?
                                .starts_with("chrome-extension://")
                            {
                                let mut success = BOOL::default();
                                args.IsSuccess(&mut success)?;
                                let _ = sender.send(success.as_bool());
                            }
                        }
                        Ok(())
                    })),
                    &mut token,
                )?;
            }
            view.load_url(&expected)?;
            anyhow::ensure!(wait_with_pump(receiver)?, "Extension popup did not load");
            view.evaluate_script("globalThis.__tabProbe={};if(chrome.tabs?.query)chrome.tabs.query({active:true},t=>{__tabProbe={count:t?.length,urls:t?.map(x=>x.url),error:chrome.runtime.lastError?.message}});globalThis.__proxyProbe={available:!!chrome.proxy?.settings};if(chrome.proxy?.settings)chrome.proxy.settings.get({incognito:false},r=>{__proxyProbe={available:true,result:r,error:chrome.runtime.lastError?.message}})")?;
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                let _ = sender.send(());
            });
            wait_with_pump(receiver)?;
            let (sender, receiver) = mpsc::channel();
            view.evaluate_script_with_callback("JSON.stringify({viewport:[innerWidth,innerHeight],body:[document.body?.getBoundingClientRect().width,document.body?.getBoundingClientRect().height],url:location.href,title:document.title,text:document.body?.innerText.slice(0,1600),errors:globalThis.__probeErrors,tabs:globalThis.__tabProbe,proxy:globalThis.__proxyProbe})",move|value|{let _=sender.send(value);})?;
            let result = wait_with_pump(receiver)?;
            let decoded: String = serde_json::from_str(&result)?;
            let popup: Value = serde_json::from_str(&decoded)?;
            let wallpaper = if name == "faceit" {
                Some(wallpaper_probe(app, id).unwrap_or_else(|e| json!({"error":format!("{e:#}")})))
            } else {
                None
            };
            Ok(
                json!({"id":installed_id,"version":manifest["version"],"installed":true,"popup":popup,"wallpaper":wallpaper}),
            )
        })();
        report.push(match result {
            Ok(value) => json!({"package":name,"result":value}),
            Err(e) => json!({"package":name,"error":format!("{e:#}")}),
        });
        model::write_atomic(
            &output,
            &serde_json::to_vec_pretty(
                &json!({"profile":app.store.borrow().directory,"packages":report}),
            )?,
        )?;
    }
    Ok(())
}

fn wallpaper_probe(app: &App, id: &str) -> Result<Value> {
    let view = app.content_builder(0)
        .with_initialization_script("globalThis.__mediaErrors=[];addEventListener('error',e=>{const t=e.target;if(t?.src)__mediaErrors.push({tag:t.tagName,url:t.src,error:t.error?.code})},true)")
        .with_bounds(rect(0., 94., 1000., 700.))
        .build_as_child(&app.windows[0].window)?;
    engine::configure(&view, &app.store.borrow().settings, false)?;
    let (sender, receiver) = mpsc::channel();
    let mut token = 0;
    unsafe {
        view.webview().add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |core, args| {
                if let (Some(core), Some(args)) = (core, args) {
                    if !engine::string(|p| core.Source(p))?.ends_with("/wallpaper.html") {
                        return Ok(());
                    }
                    let mut success = BOOL::default();
                    args.IsSuccess(&mut success)?;
                    let _ = sender.send(success.as_bool());
                }
                Ok(())
            })),
            &mut token,
        )?;
    }
    view.load_url(&format!("chrome-extension://{id}/wallpaper.html"))?;
    anyhow::ensure!(wait_with_pump(receiver)?, "Wallpaper page did not load");
    let params = json!({"expression":include_str!("../ui/ext-wallpapers-probe.js"),"awaitPromise":true,"returnByValue":true,"timeout":60000}).to_string();
    let (sender, receiver) = mpsc::channel();
    unsafe {
        view.webview().CallDevToolsProtocolMethod(
            &HSTRING::from("Runtime.evaluate"),
            &HSTRING::from(params),
            &CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
                let _ = sender.send(result.map(|_| json.to_string()).map_err(|e| e.to_string()));
                Ok(())
            })),
        )?;
    }
    let raw = wait_with_pump(receiver)?.map_err(anyhow::Error::msg)?;
    let result: Value = serde_json::from_str(&raw)?;
    anyhow::ensure!(
        result["exceptionDetails"].is_null(),
        "Wallpaper probe: {result}"
    );
    Ok(result["result"]["value"].clone())
}
