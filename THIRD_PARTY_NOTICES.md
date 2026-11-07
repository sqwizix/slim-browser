# Third-party software

The MIT license at the repository root covers original Slim Browser code.
Dependencies, filter lists and third-party extension packages retain their own licenses.

| Component | License / distribution |
| --- | --- |
| Rust dependencies | Exact versions in `rust/Cargo.lock`; notices in `rust/assets/ThirdParty.zip` |
| adblock-rust | MPL-2.0; unmodified crate source included in `ThirdParty.zip` |
| EasyList and EasyPrivacy | GPLv3; source lists and license in `rust/assets/protection/` |
| RU AdList / combined Russian list | Includes CC BY 3.0 portions; original authors and notices retained in the list headers |
| NoCoin | MIT; license alongside the filter list |
| WebView2 Evergreen | Microsoft runtime, installed separately; not relicensed by this project |
| Store extensions | Downloaded from their publishers when requested; not included in Slim Browser releases |

See [filter attribution](rust/assets/protection/NOTICE.md). The corresponding
source filter files are distributed in this repository and in the notices archive.
The browser icon is included in `rust/assets/`.

Slim Browser is an independent project. Zen Browser is a presentation inspiration;
this repository does not contain Zen Browser or Firefox source code.
