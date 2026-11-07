# Slim Browser development

The current Windows app is written in Rust. Its HTML/CSS/JavaScript shell is
embedded at compile time; websites use WebView2 Evergreen. No npm dependencies
or Node.js runtime are included in the app.

See [English](../README.md) or [Русский](../README.ru.md) for features and installation.

## Requirements and checks

Windows 11 x64, WebView2 Evergreen, stable Rust (`x86_64-pc-windows-msvc`), MSVC C++
Build Tools and Windows SDK. Node.js 22+ runs UI tests; Python 3.11+ generates notices.
`.cargo/config.toml` enables static CRT; `Cargo.lock` pins dependencies.

From the repository root:

```powershell
.\build-rust.ps1 -Test -OutputDirectory artifacts/SlimBrowser-0.5.0
node --test rust/ui/*.test.cjs rust/tests/*.test.cjs
```

From `rust/`:

```powershell
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
```

Two network-dependent Rust tests are ignored by default. Run explicitly with
`cargo test --locked -- --ignored` to test live DNS and extension servers.
`build-rust.ps1 -Test` also runs the real WebView2 smoke test in an isolated
temporary profile and writes `smoke-test.txt` beside the executable.

## Packaging

From the repository root, after building:

```powershell
python packaging/make-notices.py
.\packaging\build-installer.ps1 -BrowserExe .\artifacts\SlimBrowser-0.5.0\SlimBrowser-Rust.exe -OutputDirectory .\artifacts\release
```

The notices archive is checked in for redistribution. Update Cargo version,
resource version, MSI version and ProductCode together for new releases. Never
replace an installed MSI with changed content using the same ProductCode.
There is no signing certificate or automatic app updater in the repository.

## Source map

| Module | Responsibility |
| --- | --- |
| `main.rs` | Windows, tabs, IPC and shell state |
| `engine.rs` | WebView2 configuration, navigation and site permissions |
| `model.rs` | Settings, URLs, persistence, credentials and folders |
| `i18n.rs` | Native dialog localization using the shared catalog |
| `privacy.rs`, `protection.rs` | Site data, permissions, ad/tracker filtering |
| `network.rs` | Browser DNS, DoH and SOCKS |
| `extension_store.rs`, `compat.rs` | Publisher packages and extension compatibility |
| `downloads.rs`, `os_integration.rs` | Downloads and Windows integration |
| `smoke.rs` | Integration checks against real WebView2 |
| `ui/app.js`, `ui/privacy.js` | Browser shell and settings |
| `ui/i18n.js`, `ui/locales/en.json` | Russian/English localization |

## Localization

`Language` supports `ru-RU` and `en-US`. Old `be-BY` settings normalize to Russian;
the Belarusian page-translation choice is also removed. Changes update shell
windows immediately; the engine language fully changes after restarting all windows.

Russian source messages are keys. Use `tr('Настройки')` for static text and tagged
templates for dynamic labels. Preserve numbered placeholders in the English catalog.
Page titles, folder names and user data are not translated. Native system button
captions, websites and extension UIs are managed by their respective owners.

## Local data and legacy code

The app uses `%LOCALAPPDATA%/SlimBrowserRust`. First launch can migrate settings,
bookmarks, history and DPAPI credentials from `%LOCALAPPDATA%/SlimBrowser`, the
earlier C# profile. Cookies and extensions are not migrated; DPAPI requires the
same Windows user. Root-level C# source is retained for reference and is not
built into the current Rust app.
