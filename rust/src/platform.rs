use anyhow::Result;
use tao::{platform::windows::WindowExtWindows, window::Window};
use windows::{
    Win32::{
        Foundation::{HLOCAL, HWND, LocalFree},
        Graphics::Dwm::{
            DWMWA_BORDER_COLOR, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        },
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
        UI::{
            Controls::MARGINS,
            WindowsAndMessaging::{
                IDYES, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_YESNO, MessageBoxW,
            },
        },
    },
    core::PCWSTR,
};

pub struct Instance(windows::Win32::Foundation::HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
pub fn single_instance() -> Result<Option<Instance>> {
    use windows::Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError},
        System::Threading::CreateMutexW,
    };
    let name = wide("Local\\SlimBrowserRust.SingleInstance");
    unsafe {
        let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            Ok(None)
        } else {
            Ok(Some(Instance(handle)))
        }
    }
}

pub fn encrypt(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt(bytes, true)
}
pub fn decrypt(bytes: &[u8]) -> Result<Vec<u8>> {
    crypt(bytes, false)
}
fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into()?,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        } else {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )?;
        }
        let slice = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = slice.to_vec();
        slice.fill(0);
        let _ = LocalFree(Some(HLOCAL(output.pbData as *mut _)));
        Ok(result)
    }
}
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub fn error(message: &str) {
    let text = wide(message);
    let title = wide("SlimBrowser Rust");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
pub fn confirm(window: &Window, message: &str) -> bool {
    let text = wide(message);
    let title = wide("SlimBrowser Rust");
    unsafe {
        MessageBoxW(
            Some(HWND(window.hwnd() as *mut _)),
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}
pub fn material(window: &Window, glass: bool, dark: bool) {
    let hwnd = HWND(window.hwnd() as *mut _);
    let backdrop: i32 = if glass { 3 } else { 1 };
    let dark: i32 = dark.into();
    let round: i32 = if edge_to_edge(window) { 1 } else { 2 };
    let border: u32 = 0xFFFFFFFE;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const _ as *const _,
            4,
        );
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &border as *const _ as *const _, 4);
        let m = if glass { -1 } else { 0 };
        let _ = DwmExtendFrameIntoClientArea(
            hwnd,
            &MARGINS {
                cxLeftWidth: m,
                cxRightWidth: m,
                cyTopHeight: m,
                cyBottomHeight: m,
            },
        );
    }
}
pub fn system_dark() -> bool {
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    UISettings::new()
        .and_then(|s| s.GetColorValue(UIColorType::Background))
        .map(|c| (c.R as u32 + c.G as u32 + c.B as u32) < 384)
        .unwrap_or(false)
}

pub fn edge_to_edge(window: &Window) -> bool {
    window.is_maximized() || window.fullscreen().is_some()
}
pub fn matches_monitor_area(window: &Window, full: bool) -> Result<bool> {
    use windows::Win32::{Graphics::Gdi::*, UI::WindowsAndMessaging::GetWindowRect};
    let hwnd = HWND(window.hwnd() as *mut _);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let mut actual = windows::Win32::Foundation::RECT::default();
    unsafe {
        GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info).ok()?;
        GetWindowRect(hwnd, &mut actual)?;
    }
    let expected = if full { info.rcMonitor } else { info.rcWork };
    Ok(actual == expected)
}
pub fn rounding(window: &Window) {
    let round: i32 = if edge_to_edge(window) { 1 } else { 2 };
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(window.hwnd() as *mut _),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const _ as *const _,
            4,
        );
    }
}
// Keep the resize/system-menu styles for Windows behavior, but own the entire
// client area. DWM must not paint a second set of caption buttons when maximized.
pub fn custom_frame(window: &Window) -> Result<()> {
    use windows::Win32::UI::{Shell::SetWindowSubclass, WindowsAndMessaging::*};
    let hwnd = HWND(window.hwnd() as *mut _);
    unsafe {
        let transitions_disabled = 0i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            windows::Win32::Graphics::Dwm::DWMWA_TRANSITIONS_FORCEDISABLED,
            &transitions_disabled as *const _ as *const _,
            4,
        );
        SetWindowSubclass(hwnd, Some(frame_proc), 0x534C494D, 0).ok()?;
        // Tao keeps WS_CAPTION on undecorated top-level windows. Strip it
        // here as well as on later style updates so activation cannot paint
        // the native caption over the custom title bar.
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            GetWindowLongPtrW(hwnd, GWL_STYLE) & !(WS_CAPTION.0 as isize),
        );
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
    }
    Ok(())
}
unsafe extern "system" fn frame_proc(
    hwnd: HWND,
    msg: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::{LPARAM, LRESULT},
        Graphics::Gdi::*,
        UI::{Shell::DefSubclassProc, WindowsAndMessaging::*},
    };
    unsafe {
        match msg {
            WM_GETMINMAXINFO => {
                let result = DefSubclassProc(hwnd, msg, w, l);
                let mut monitor = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if GetMonitorInfoW(
                    MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                    &mut monitor,
                )
                .as_bool()
                {
                    let info = &mut *(l.0 as *mut MINMAXINFO);
                    info.ptMaxPosition.x = monitor.rcWork.left - monitor.rcMonitor.left;
                    info.ptMaxPosition.y = monitor.rcWork.top - monitor.rcMonitor.top;
                    info.ptMaxSize.x = monitor.rcWork.right - monitor.rcWork.left;
                    info.ptMaxSize.y = monitor.rcWork.bottom - monitor.rcWork.top;
                }
                result
            }
            WM_STYLECHANGING if w.0 as i32 == GWL_STYLE.0 => {
                let styles = &mut *(l.0 as *mut STYLESTRUCT);
                styles.styleNew &= !WS_CAPTION.0;
                DefSubclassProc(hwnd, msg, w, l)
            }
            WM_NCCALCSIZE => {
                // A fullscreen window can retain WS_MAXIMIZE after entering
                // from a maximized window, but must cover the whole monitor.
                if w.0 != 0
                    && IsZoomed(hwnd).as_bool()
                    && GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_THICKFRAME.0 as isize != 0
                {
                    let params = &mut *(l.0 as *mut NCCALCSIZE_PARAMS);
                    let mut monitor = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    if GetMonitorInfoW(
                        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                        &mut monitor,
                    )
                    .as_bool()
                    {
                        params.rgrc[0] = monitor.rcWork;
                    }
                }
                LRESULT(0)
            }
            WM_NCPAINT => LRESULT(0),
            WM_NCHITTEST => {
                if IsZoomed(hwnd).as_bool()
                    || GetWindowLongPtrW(hwnd, GWL_STYLE) & WS_THICKFRAME.0 as isize == 0
                {
                    return LRESULT(HTCLIENT as isize);
                }
                let mut point = windows::Win32::Foundation::POINT {
                    x: (l.0 as u16 as i16) as i32,
                    y: ((l.0 >> 16) as u16 as i16) as i32,
                };
                let _ = ScreenToClient(hwnd, &mut point);
                let mut area = windows::Win32::Foundation::RECT::default();
                let _ = GetClientRect(hwnd, &mut area);
                let left = point.x < 6;
                let right = point.x >= area.right - 6;
                let top = point.y < 6;
                let bottom = point.y >= area.bottom - 6;
                let hit = if top {
                    if left {
                        HTTOPLEFT
                    } else if right {
                        HTTOPRIGHT
                    } else {
                        HTTOP
                    }
                } else if bottom {
                    if left {
                        HTBOTTOMLEFT
                    } else if right {
                        HTBOTTOMRIGHT
                    } else {
                        HTBOTTOM
                    }
                } else if left {
                    HTLEFT
                } else if right {
                    HTRIGHT
                } else {
                    HTCLIENT
                };
                LRESULT(hit as isize)
            }
            WM_NCACTIVATE => DefSubclassProc(hwnd, msg, w, LPARAM(-1)),
            _ => DefSubclassProc(hwnd, msg, w, l),
        }
    }
}

pub struct ShareSession {
    manager: windows::ApplicationModel::DataTransfer::DataTransferManager,
    token: i64,
}
impl Drop for ShareSession {
    fn drop(&mut self) {
        let _ = self.manager.RemoveDataRequested(self.token);
    }
}
pub fn share_page(window: &Window, url: &str, title: &str) -> Result<ShareSession> {
    use windows::{
        ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
        Foundation::{TypedEventHandler, Uri},
        Win32::UI::Shell::IDataTransferManagerInterop,
        core::HSTRING,
    };
    let interop: IDataTransferManagerInterop =
        windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    let hwnd = HWND(window.hwnd() as *mut _);
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    let url = url.to_owned();
    let title = if title.is_empty() {
        url.clone()
    } else {
        title.to_owned()
    };
    let token = manager.DataRequested(&TypedEventHandler::<
        DataTransferManager,
        DataRequestedEventArgs,
    >::new(move |_, args| {
        let args = args.as_ref().ok_or_else(windows::core::Error::empty)?;
        let data = args.Request()?.Data()?;
        data.Properties()?.SetTitle(&HSTRING::from(&title))?;
        data.SetWebLink(&Uri::CreateUri(&HSTRING::from(&url))?)?;
        Ok(())
    }))?;
    let session = ShareSession { manager, token };
    unsafe {
        interop.ShowShareUIForWindow(hwnd)?;
    }
    Ok(session)
}

pub fn open_system_url(url: &str) -> Result<()> {
    use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let operation = wide("open");
    let target = wide(url);
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    anyhow::ensure!(
        result.0 as usize > 32,
        "Не удалось открыть параметры Windows"
    );
    Ok(())
}

pub fn motion_enabled() -> bool {
    windows::UI::ViewManagement::UISettings::new()
        .and_then(|s| s.AnimationsEnabled())
        .unwrap_or(true)
}
pub fn window_bounds(window: &Window) -> Result<[f64; 4]> {
    let mut bounds = windows::Win32::Foundation::RECT::default();
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
            HWND(window.hwnd() as *mut _),
            &mut bounds,
        )?;
    }
    Ok([
        bounds.left as f64,
        bounds.top as f64,
        (bounds.right - bounds.left) as f64,
        (bounds.bottom - bounds.top) as f64,
    ])
}
pub fn monitor_bounds(window: &Window, full: bool) -> Result<[f64; 4]> {
    use windows::Win32::Graphics::Gdi::*;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(HWND(window.hwnd() as *mut _), MONITOR_DEFAULTTONEAREST),
            &mut info,
        )
        .ok()?;
    }
    let r = if full { info.rcMonitor } else { info.rcWork };
    Ok([
        r.left as f64,
        r.top as f64,
        (r.right - r.left) as f64,
        (r.bottom - r.top) as f64,
    ])
}
pub fn place_window(window: &Window, bounds: [f64; 4]) -> Result<()> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        SetWindowPos(
            HWND(window.hwnd() as *mut _),
            None,
            bounds[0].round() as i32,
            bounds[1].round() as i32,
            bounds[2].round().max(1.) as i32,
            bounds[3].round().max(1.) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    Ok(())
}
pub fn close_animation(window: &Window) {
    if !motion_enabled() || !window.is_visible() {
        return;
    }
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let _ = AnimateWindow(HWND(window.hwnd() as *mut _), 90, AW_HIDE | AW_BLEND);
    }
}
