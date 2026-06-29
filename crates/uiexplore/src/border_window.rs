//! Overlay border window for highlighting UI elements.
//!
//! Creates a transparent, click-through, always-on-top Win32 popup window that
//! draws a coloured border frame.  Moving the border to a new rectangle is
//! instantaneous (just a `SetWindowPos`) and cleanup is automatic — destroying
//! the window removes all on-screen artefacts with no `InvalidateRect` hacks.

use std::sync::OnceLock;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW,
    DestroyWindow, GetClientRect, HWND_TOPMOST, LWA_COLORKEY, RegisterClassW,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, WNDCLASSW, WM_DESTROY, WM_PAINT,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// The colour key used for full transparency (magenta — unlikely to collide
/// with the border colour or any UI element).
const TRANSPARENT_KEY: COLORREF = COLORREF(0x00_FF_00_FF); // RGB(255, 0, 255)
/// Border colour — bright green matching the previous GDI highlight.
const BORDER_COLOR: COLORREF = COLORREF(0x00_05_FF_2C); // RGB(0x2C, 0xFF, 0x05)
/// Border thickness in pixels.
const BORDER_WIDTH: i32 = 4;

/// Wide-string class name for the overlay window.
const CLASS_NAME: windows::core::PCWSTR = windows::core::w!("UIExploreBorderOverlay");
/// Wide-string window title (not visible — toolwindow).
const WINDOW_TITLE: windows::core::PCWSTR = windows::core::w!("UIExplore Border");

/// Ensures the window class is registered exactly once.
static CLASS_REGISTERED: OnceLock<u16> = OnceLock::new();

/// A transparent, click-through overlay window that draws a coloured border.
///
/// The window is always-on-top and does not appear in the taskbar.  Its
/// interior is fully transparent (via `LWA_COLORKEY`) so only the painted
/// border frame is visible.
///
/// # Lifecycle
/// * [`BorderWindow::new()`] — creates (hidden) the overlay window.
/// * [`BorderWindow::update()`] — moves/resizes to a new rectangle and shows.
/// * [`BorderWindow::hide()`] — hides without destroying.
/// * On [`Drop`] — destroys the Win32 window, cleaning up all artefacts.
pub struct BorderWindow {
    hwnd: HWND,
}

impl BorderWindow {
    /// Create a new (initially hidden) border overlay window.
    pub fn new() -> windows::core::Result<Self> {
        let hinstance = unsafe { GetModuleHandleW(None)? };

        Self::register_class(hinstance.into())?;

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                CLASS_NAME,
                WINDOW_TITLE,
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                0,
                0,
                None,
                None,
                Some(hinstance.into()),
                None,
            )?
        };

        // Make the background colour fully transparent via colour-keying.
        unsafe {
            SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 0, LWA_COLORKEY)?;
        }

        Ok(Self { hwnd })
    }

    /// Move and resize the border to surround `rect`, making it visible.
    ///
    /// `rect` uses raw screen coordinates (not DPI-scaled).
    pub fn update(&self, rect: RECT) {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // SWP_NOACTIVATE keeps focus on the application being inspected.
        // SWP_SHOWWINDOW makes the overlay visible if it was hidden.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOPMOST),
                rect.left,
                rect.top,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }

    /// Hide the border overlay without destroying it.
    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    /// Register the window class (idempotent — only runs once).
    fn register_class(hinstance: HINSTANCE) -> windows::core::Result<()> {
        CLASS_REGISTERED.get_or_init(|| {
            // Create a solid brush with the transparent key colour.
            // This brush is used as the default background and will be made
            // invisible by the LWA_COLORKEY setting.
            let bg_brush = unsafe { CreateSolidBrush(TRANSPARENT_KEY) };

            let wc = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(border_wnd_proc),
                hInstance: hinstance,
                lpszClassName: CLASS_NAME,
                hbrBackground: bg_brush,
                ..Default::default()
            };

            // SAFETY: `wc` is fully initialised.
            unsafe { RegisterClassW(&wc) }
        });
        Ok(())
    }
}

impl Drop for BorderWindow {
    fn drop(&mut self) {
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Window procedure
// ---------------------------------------------------------------------------

/// Window procedure for the border overlay.
///
/// On `WM_PAINT` it fills the client area with the transparent colour key,
/// then paints four filled rectangles (top, bottom, left, right edges) in the
/// border colour.  The colour-keyed interior is invisible, leaving only the
/// border visible on screen.
unsafe extern "system" fn border_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: all GDI / windowing calls operate on handles obtained from valid
    // Win32 API calls within this callback.  Brush handles are deleted after
    // use and BeginPaint/EndPaint are correctly paired.
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                if !hdc.is_invalid() {
                    let mut client = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client);

                    let w = client.right;
                    let h = client.bottom;
                    let bw = BORDER_WIDTH;

                    // Fill the whole client area with the transparent key colour
                    // so colour-keying makes the interior invisible.
                    let bg_brush = CreateSolidBrush(TRANSPARENT_KEY);
                    FillRect(hdc, &client, bg_brush);

                    // Paint the four border edges with the highlight colour.
                    let border_brush = CreateSolidBrush(BORDER_COLOR);

                    // Top edge
                    let top = RECT { left: 0, top: 0, right: w, bottom: bw };
                    FillRect(hdc, &top, border_brush);

                    // Bottom edge
                    let bottom = RECT { left: 0, top: h - bw, right: w, bottom: h };
                    FillRect(hdc, &bottom, border_brush);

                    // Left edge (between top and bottom)
                    let left = RECT { left: 0, top: bw, right: bw, bottom: h - bw };
                    FillRect(hdc, &left, border_brush);

                    // Right edge (between top and bottom)
                    let right = RECT { left: w - bw, top: bw, right: w, bottom: h - bw };
                    FillRect(hdc, &right, border_brush);

                    // Clean up GDI objects
                    let _ = DeleteObject(bg_brush.into());
                    let _ = DeleteObject(border_brush.into());

                    let _ = EndPaint(hwnd, &ps);
                }
                LRESULT(0)
            }
            WM_DESTROY => DefWindowProcW(hwnd, msg, wparam, lparam),
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}
