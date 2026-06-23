//! Shared GDI rectangle drawing, clearing, and hit-testing utilities.
//!
//! Provides RAII `GdiGuard` to automatically release GDI resources on drop,
//! eliminating manual cleanup at every error branch.

use windows::{
    Win32::Foundation::{COLORREF, HWND, RECT},
    Win32::Graphics::Gdi::{
        CreatePen, DeleteObject, GetDC, GetStockObject, HDC, HGDIOBJ, HOLLOW_BRUSH, HPEN,
        InvalidateRect, PS_SOLID, Rectangle, ReleaseDC, SelectObject,
    },
    core::{Error, Result},
};

/// RAII guard for GDI device-context and pen resources.
///
/// Automatically releases the desktop DC and deletes the pen on drop,
/// preventing GDI handle leaks even when errors cause early returns.
struct GdiGuard {
    hdc: HDC,
    pen: HPEN,
    old_pen: HGDIOBJ,
    old_brush: HGDIOBJ,
}

impl Drop for GdiGuard {
    fn drop(&mut self) {
        // SAFETY: All handles were obtained from valid GDI API calls during
        // construction and are released exactly once here.
        unsafe {
            if !self.old_brush.is_invalid() {
                SelectObject(self.hdc, self.old_brush);
            }
            if !self.old_pen.is_invalid() {
                SelectObject(self.hdc, self.old_pen);
            }
            if !self.pen.is_invalid() {
                let _ = DeleteObject(self.pen.into());
            }
            if !self.hdc.is_invalid() {
                ReleaseDC(Some(HWND(std::ptr::null_mut())), self.hdc);
            }
        }
    }
}

/// Draw an outlined rectangle on the desktop using GDI.
///
/// Uses a bright green pen (`#2cff05`) and a hollow brush so the interior
/// is transparent. All GDI handles are managed by [`GdiGuard`] and released
/// automatically.
pub fn draw_frame(rect: RECT, outline_width: i32) -> Result<()> {
    // SAFETY: All GDI handles are checked for validity before use and cleaned
    // up automatically by the GdiGuard Drop implementation.
    // HWND(null) targets the desktop DC, which is always valid.
    unsafe {
        let hdc = GetDC(Some(HWND(std::ptr::null_mut())));
        if hdc.is_invalid() {
            return Err(Error::from_win32());
        }

        // Green highlight: 393004 = 0x0005FF2C (little-endian COLORREF for #2cff05)
        let color = COLORREF(393004);
        let pen = CreatePen(PS_SOLID, outline_width, color);
        if pen.is_invalid() {
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        let old_pen = SelectObject(hdc, pen.into());
        if old_pen.is_invalid() {
            let _ = DeleteObject(pen.into());
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        let hollow_brush = GetStockObject(HOLLOW_BRUSH);
        if hollow_brush.is_invalid() {
            // Guard will clean up pen + DC
            let guard = GdiGuard {
                hdc,
                pen,
                old_pen,
                old_brush: HGDIOBJ::default(),
            };
            drop(guard);
            return Err(Error::from_win32());
        }

        let old_brush = SelectObject(hdc, hollow_brush);
        if old_brush.is_invalid() {
            let guard = GdiGuard {
                hdc,
                pen,
                old_pen,
                old_brush: HGDIOBJ::default(),
            };
            drop(guard);
            return Err(Error::from_win32());
        }

        // Guard now owns all resources — any subsequent error unwinds cleanly
        let guard = GdiGuard {
            hdc,
            pen,
            old_pen,
            old_brush,
        };

        if !Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom).as_bool() {
            drop(guard);
            return Err(Error::from_win32());
        }

        // guard drops here, releasing all GDI resources
        drop(guard);
        Ok(())
    }
}

/// Invalidate (clear) a rectangular region of the desktop, forcing a repaint.
pub fn clear_frame(rect: RECT) -> Result<()> {
    // SAFETY: HWND(null) targets all windows; `rect` is a valid stack-allocated RECT.
    unsafe {
        let _res = InvalidateRect(Some(HWND(std::ptr::null_mut())), Some(&rect), true);
        Ok(())
    }
}

/// Point-in-rectangle hit test using the `uiautomation::types::Rect` type.
pub fn is_inside_rectangle(rect: &uiautomation::types::Rect, x: i32, y: i32) -> bool {
    x >= rect.get_left() && x <= rect.get_right() && y >= rect.get_top() && y <= rect.get_bottom()
}
