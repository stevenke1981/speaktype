//! Floating recording indicator: a separate, small, frameless, transparent,
//! always-on-top Slint window that never takes keyboard focus.
//!
//! The window is exactly the size of the pill (no oversized surface behind it,
//! which was the v0.4 egui bug). On Windows it is additionally given
//! WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW (no focus, no taskbar/Alt-Tab entry), a
//! rounded window region (no corners even where the compositor ignores alpha),
//! and it is shown/hidden with SW_SHOWNOACTIVATE / SW_HIDE so recording never
//! moves focus away from the app you are dictating into.

use crate::IndicatorWindow;
use slint::winit_030::{winit, WinitWindowAccessor};
use slint::ComponentHandle;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set while the indicator component is being constructed, so the winit
/// window-attributes hook (installed in main.rs) only changes that window.
pub static CREATING_INDICATOR: AtomicBool = AtomicBool::new(false);

/// winit attributes for the indicator window (see `CREATING_INDICATOR`).
pub fn indicator_attributes(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    if !CREATING_INDICATOR.load(Ordering::SeqCst) {
        return attrs;
    }
    let attrs = attrs
        .with_active(false)
        .with_decorations(false)
        .with_resizable(false)
        .with_transparent(true)
        .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
        // Created off-screen; moved to the bottom-centre when first shown.
        .with_position(winit::dpi::PhysicalPosition::new(-32000, -32000));
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs.with_skip_taskbar(true)
    };
    attrs
}

pub struct Indicator {
    pub window: IndicatorWindow,
    visible: bool,
    realized: bool,
    prepared: bool,
}

impl Indicator {
    pub fn new(ui_font: &str) -> Result<Self, slint::PlatformError> {
        CREATING_INDICATOR.store(true, Ordering::SeqCst);
        let window = IndicatorWindow::new();
        CREATING_INDICATOR.store(false, Ordering::SeqCst);
        let window = window?;
        window.set_ui_font(ui_font.into());
        Ok(Self { window, visible: false, realized: false, prepared: false })
    }

    /// Show the pill without activating it. Returns `false` if the native window
    /// does not exist yet (before the event loop runs); call again shortly.
    pub fn show(&mut self) -> bool {
        if self.visible {
            return true;
        }
        if !self.realized {
            // First show creates the native window (inactive, off-screen); after
            // that it is only shown/hidden natively so it never activates.
            if let Err(err) = self.window.show() {
                speaktype::modules::error::log_error("indicator", err.to_string());
                return true;
            }
            self.realized = true;
        }
        if !self.prepared {
            if !self.window.window().has_winit_window() {
                return false;
            }
            self.window.window().with_winit_window(|w| {
                let _ = w.set_cursor_hittest(false);
                #[cfg(windows)]
                win::prepare(w);
            });
            self.prepared = true;
        }
        self.place();
        #[cfg(windows)]
        self.window.window().with_winit_window(win::show_no_activate);
        #[cfg(not(windows))]
        let _ = self.window.show();
        self.visible = true;
        true
    }

    pub fn hide(&mut self) {
        if !self.visible {
            return;
        }
        #[cfg(windows)]
        self.window.window().with_winit_window(win::hide);
        #[cfg(not(windows))]
        let _ = self.window.hide();
        self.visible = false;
    }

    /// Bottom-centre of the primary monitor, above the taskbar.
    fn place(&self) {
        self.window.window().with_winit_window(|w| {
            let Some(monitor) = w.primary_monitor().or_else(|| w.current_monitor()).or_else(|| w.available_monitors().next())
            else {
                return;
            };
            let scale = monitor.scale_factor();
            let msize = monitor.size();
            let mpos = monitor.position();
            let win = w.outer_size();
            let (ww, wh) = if win.width > 1 && win.height > 1 {
                (win.width as i32, win.height as i32)
            } else {
                ((184.0 * scale) as i32, (40.0 * scale) as i32)
            };
            let x = mpos.x + (msize.width as i32 - ww) / 2;
            let y = mpos.y + msize.height as i32 - wh - (88.0 * scale) as i32;
            w.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
        });
    }
}

#[cfg(windows)]
mod win {
    use slint::winit_030::winit;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, SetWindowRgn};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_TOPMOST,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    };

    fn hwnd(w: &winit::window::Window) -> Option<HWND> {
        match w.window_handle().ok()?.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
            _ => None,
        }
    }

    pub fn prepare(w: &winit::window::Window) {
        let Some(h) = hwnd(w) else { return };
        let size = w.inner_size();
        // SAFETY: `h` is the live HWND of this winit window, owned by this thread.
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize);
            if size.width > 1 && size.height > 1 {
                let r = size.height as i32;
                let rgn = CreateRoundRectRgn(0, 0, size.width as i32 + 1, size.height as i32 + 1, r, r);
                // The system owns the region after a successful SetWindowRgn.
                SetWindowRgn(h, rgn, 1);
            }
            ShowWindow(h, SW_HIDE);
        }
    }

    pub fn show_no_activate(w: &winit::window::Window) {
        let Some(h) = hwnd(w) else { return };
        // SAFETY: valid HWND owned by this thread; flags never activate the window.
        unsafe {
            ShowWindow(h, SW_SHOWNOACTIVATE);
            SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        w.request_redraw();
    }

    pub fn hide(w: &winit::window::Window) {
        let Some(h) = hwnd(w) else { return };
        // SAFETY: valid HWND owned by this thread.
        unsafe {
            ShowWindow(h, SW_HIDE);
        }
    }
}
