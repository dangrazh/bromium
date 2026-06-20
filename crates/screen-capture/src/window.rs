use image::RgbaImage;
use windows::Win32::Foundation::HWND;

use crate::{Monitor, error::ScreenCaptureResult, mswindows::impl_window::ImplWindow};

#[derive(Debug, Clone)]
pub struct Window {
    pub(crate) impl_window: ImplWindow,
}

impl Window {
    pub(crate) fn new(impl_window: ImplWindow) -> Window {
        Window { impl_window }
    }
}

impl From<HWND> for Window {
    fn from(hwnd: HWND) -> Self {
        Window::new(ImplWindow::new(hwnd))
    }
}

impl Window {
    /// List all windows, sorted by z coordinate.
    pub fn all() -> ScreenCaptureResult<Vec<Window>> {
        let windows = ImplWindow::all()?
            .iter()
            .map(|impl_window| Window::new(impl_window.clone()))
            .collect();

        Ok(windows)
    }
}

impl Window {
    /// The window id
    pub fn id(&self) -> ScreenCaptureResult<u32> {
        self.impl_window.id()
    }
    /// The window process id
    pub fn pid(&self) -> ScreenCaptureResult<u32> {
        self.impl_window.pid()
    }
    /// The window handle
    pub fn hwnd(&self) -> ScreenCaptureResult<HWND> {
        self.impl_window.hwnd()
    }
    /// The window app name
    pub fn app_name(&self) -> ScreenCaptureResult<String> {
        self.impl_window.app_name()
    }
    /// The window title
    pub fn title(&self) -> ScreenCaptureResult<String> {
        self.impl_window.title()
    }
    /// The window current monitor
    pub fn current_monitor(&self) -> ScreenCaptureResult<Monitor> {
        Ok(Monitor::new(self.impl_window.current_monitor()?))
    }
    /// The window x coordinate.
    pub fn x(&self) -> ScreenCaptureResult<i32> {
        self.impl_window.x()
    }
    /// The window y coordinate.
    pub fn y(&self) -> ScreenCaptureResult<i32> {
        self.impl_window.y()
    }
    /// The window z coordinate.
    pub fn z(&self) -> ScreenCaptureResult<i32> {
        self.impl_window.z()
    }
    /// The window pixel width.
    pub fn width(&self) -> ScreenCaptureResult<u32> {
        self.impl_window.width()
    }
    /// The window pixel height.
    pub fn height(&self) -> ScreenCaptureResult<u32> {
        self.impl_window.height()
    }
    /// The window is minimized.
    pub fn is_minimized(&self) -> ScreenCaptureResult<bool> {
        self.impl_window.is_minimized()
    }
    /// The window is maximized.
    pub fn is_maximized(&self) -> ScreenCaptureResult<bool> {
        self.impl_window.is_maximized()
    }
    /// The window is focused.
    pub fn is_focused(&self) -> ScreenCaptureResult<bool> {
        self.impl_window.is_focused()
    }
}

impl Window {
    pub fn capture_image(&self) -> ScreenCaptureResult<RgbaImage> {
        self.impl_window.capture_image()
    }
}
