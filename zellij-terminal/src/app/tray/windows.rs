use std::cell::{Cell, RefCell};

use anyhow::{anyhow, Result};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, GetMonitorInfoW, MonitorFromPoint, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, GetWindowLongPtrW, PostMessageW, RegisterClassExW,
    RegisterWindowMessageW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu, GWLP_USERDATA,
    HICON, HMENU, HWND_MESSAGE, ICONINFO, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    SM_CXSMICON, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CONTEXTMENU,
    WM_LBUTTONUP, WM_NULL, WNDCLASSEXW,
};

use super::{find_action, menu, Anchor, MenuEntry, Tray, TrayEvent, TrayEvents};
use crate::app::icon::{render, DARK_INK, LIGHT_INK};
use crate::app::model::{MenuState, TrayState};
use crate::app::platform_windows::wide;

const CALLBACK: u32 = WM_APP + 1;
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const CLASS_NAME: &str = "ZellijTrayWindow";

struct WinState {
    events: TrayEvents,
    menu: Cell<MenuState>,
    cursor: Cell<Option<(i32, i32)>>,
    taskbar_created: u32,
    data: RefCell<NOTIFYICONDATAW>,
    visible: Cell<bool>,
}

pub struct NotifyIcon {
    hwnd: HWND,
    state: Box<WinState>,
    icon: HICON,
    look: Option<(crate::app::model::TrayLook, u32, bool)>,
}

fn light_taskbar() -> bool {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("SystemUsesLightTheme");
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            &mut value as *mut u32 as *mut _,
            &mut size,
        )
    };
    read == 0 && value == 1
}

fn icon_size() -> u32 {
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) };
    if size > 0 {
        size as u32
    } else {
        16
    }
}

fn make_icon(image: &crate::app::icon::Rgba) -> HICON {
    unsafe {
        let mut header: BITMAPINFO = std::mem::zeroed();
        header.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: image.width as i32,
            biHeight: -(image.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(
            std::ptr::null_mut(),
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if color.is_null() || bits.is_null() {
            return std::ptr::null_mut();
        }
        let bgra = image.to_bgra();
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
        let mask_bytes = vec![0u8; (image.width.div_ceil(16) * 2 * image.height) as usize];
        let mask = CreateBitmap(
            image.width as i32,
            image.height as i32,
            1,
            1,
            mask_bytes.as_ptr() as *const _,
        );
        let info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

fn set_tip(data: &mut NOTIFYICONDATAW, tip: &str) {
    let encoded: Vec<u16> = tip.encode_utf16().take(127).collect();
    data.szTip = [0; 128];
    data.szTip[..encoded.len()].copy_from_slice(&encoded);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WinState;
    if pointer.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &*pointer };
    if message == state.taskbar_created && state.visible.get() {
        let data = state.data.borrow();
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &*data);
            Shell_NotifyIconW(NIM_SETVERSION, &*data);
        }
        return 0;
    }
    if message == CALLBACK {
        let event = (lparam as u32) & 0xffff;
        let x = (wparam as u32 & 0xffff) as i16 as i32;
        let y = ((wparam as u32 >> 16) & 0xffff) as i16 as i32;
        match event {
            NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP => {
                state.cursor.set(Some((x, y)));
                (state.events)(TrayEvent::Activate {
                    at: Some((x, y)),
                    token: None,
                });
            },
            WM_CONTEXTMENU => {
                state.cursor.set(Some((x, y)));
                show_menu(hwnd, state, x, y);
            },
            _ => {},
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn append(menu: HMENU, entries: &[MenuEntry]) {
    for entry in entries {
        unsafe {
            match entry {
                MenuEntry::Separator { .. } => {
                    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                },
                MenuEntry::Item {
                    id,
                    label,
                    enabled,
                    checked,
                    ..
                } => {
                    let text = wide(label);
                    let mut flags = if *enabled {
                        MF_STRING
                    } else {
                        MF_STRING | MF_GRAYED
                    };
                    if *checked == Some(true) {
                        flags |= MF_CHECKED;
                    }
                    AppendMenuW(menu, flags, *id as usize, text.as_ptr());
                },
                MenuEntry::Submenu { label, items, .. } => {
                    let submenu = CreatePopupMenu();
                    append(submenu, items);
                    let text = wide(label);
                    AppendMenuW(menu, MF_POPUP, submenu as usize, text.as_ptr());
                },
            }
        }
    }
}

fn show_menu(hwnd: HWND, state: &WinState, x: i32, y: i32) {
    let entries = menu(&state.menu.get());
    unsafe {
        let popup = CreatePopupMenu();
        append(popup, &entries);
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            popup,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            x,
            y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(popup);
        if let Some(action) = find_action(&entries, chosen) {
            (state.events)(TrayEvent::Menu(action));
        }
    }
}

impl NotifyIcon {
    pub fn open(events: TrayEvents, initial: &TrayState) -> Result<NotifyIcon> {
        let class = wide(CLASS_NAME);
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut window_class: WNDCLASSEXW = unsafe { std::mem::zeroed() };
        window_class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        window_class.lpfnWndProc = Some(window_proc);
        window_class.hInstance = instance;
        window_class.lpszClassName = class.as_ptr();
        unsafe { RegisterClassExW(&window_class) };
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(anyhow!("the tray message window could not be created"));
        }
        let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
        let mut data: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = CALLBACK;
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        let state = Box::new(WinState {
            events,
            menu: Cell::new(initial.menu),
            cursor: Cell::new(None),
            taskbar_created,
            data: RefCell::new(data),
            visible: Cell::new(false),
        });
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*state as *const WinState as isize);
        }
        let mut tray = NotifyIcon {
            hwnd,
            state,
            icon: std::ptr::null_mut(),
            look: None,
        };
        tray.update(initial);
        tray.set_visible(true);
        Ok(tray)
    }

    fn apply(&self, message: u32) {
        let data = self.state.data.borrow();
        unsafe {
            Shell_NotifyIconW(message, &*data);
        }
    }
}

impl Tray for NotifyIcon {
    fn update(&mut self, state: &TrayState) {
        self.state.menu.set(state.menu);
        let size = icon_size();
        let light = light_taskbar();
        let key = (state.look.clone(), size, light);
        if self.look.as_ref() != Some(&key) {
            let ink = if light { DARK_INK } else { LIGHT_INK };
            let icon = make_icon(&render(size, &state.look, ink));
            if !icon.is_null() {
                let previous = std::mem::replace(&mut self.icon, icon);
                self.state.data.borrow_mut().hIcon = icon;
                if !previous.is_null() {
                    unsafe { DestroyIcon(previous) };
                }
            }
            self.look = Some(key);
        }
        set_tip(&mut self.state.data.borrow_mut(), &state.tooltip);
        if self.state.visible.get() {
            self.apply(NIM_MODIFY);
        }
    }

    fn set_visible(&mut self, visible: bool) {
        if visible == self.state.visible.get() {
            return;
        }
        self.state.visible.set(visible);
        if visible {
            self.apply(NIM_ADD);
            self.apply(NIM_SETVERSION);
        } else {
            self.apply(NIM_DELETE);
        }
    }

    fn anchor(&self) -> Option<Anchor> {
        let (x, y) = self.state.cursor.get().or_else(|| {
            let mut point = POINT { x: 0, y: 0 };
            (unsafe { GetCursorPos(&mut point) } != 0).then_some((point.x, point.y))
        })?;
        Some(Anchor::Above {
            x: x as f64,
            y: y as f64,
        })
    }

    fn refresh_appearance(&mut self) {
        let light = light_taskbar();
        if let Some((look, size, was_light)) = self.look.clone() {
            if was_light != light {
                self.look = None;
                let tooltip = String::from_utf16_lossy(
                    &self
                        .state
                        .data
                        .borrow()
                        .szTip
                        .iter()
                        .copied()
                        .take_while(|c| *c != 0)
                        .collect::<Vec<u16>>(),
                );
                let _ = size;
                self.update(&TrayState {
                    look,
                    tooltip,
                    menu: self.state.menu.get(),
                });
            }
        }
    }
}

pub fn work_area(x: i32, y: i32) -> Option<(f64, f64, f64, f64)> {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let RECT {
            left,
            top,
            right,
            bottom,
        } = info.rcWork;
        Some((left as f64, top as f64, right as f64, bottom as f64))
    }
}

impl Drop for NotifyIcon {
    fn drop(&mut self) {
        if self.state.visible.get() {
            self.apply(NIM_DELETE);
        }
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}
