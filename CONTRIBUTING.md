# Contributing

Thanks for helping improve Slim Browser. Bug reports, translations and focused pull requests are welcome.

1. Check existing issues before opening a new one.
2. Include Windows and WebView2 versions, reproducible steps, expected behavior and actual behavior.
3. Remove account details, tokens, cookies and browsing history from screenshots and logs.
4. For code changes, follow [the build guide](rust/README.md) and run the checks relevant to the change.

The active app is in `rust/`. Root-level C# files preserve the earlier WinForms
implementation and are not part of the current Rust executable.

Interface messages use Russian source keys and an English catalog at
`rust/ui/locales/en.json`. Keep `{0}` placeholders intact. Use tagged `tr`
templates for dynamic UI labels so user-provided values are preserved.

Keep changes focused and describe how you verified them. Never commit browser
profiles, downloaded extension packages, passwords, SDK caches or build output.
