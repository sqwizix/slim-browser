# Changelog

## 0.5.0 — 2026-11-07

- Added English interface localization alongside Russian, including settings,
  menus, accessibility labels, downloads, permission categories and native prompts.
- Interface language changes immediately in all windows. WebView2 language
  changes fully after closing every window and restarting.
- Removed Belarusian from language choices. Existing Belarusian settings
  fall back to Russian without changing saved browsing data.
- Added Russian and English project pages, screenshots, build instructions,
  contribution guidance, MIT license and third-party attribution.
- Added a Windows build workflow and portable/MSI distribution.

The Rust app and its earlier features are included in this first public source
release. Root-level C# files preserve the previous WinForms implementation.
