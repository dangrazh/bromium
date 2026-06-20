use std::sync::mpsc::Receiver;

use image::RgbaImage;

use crate::{
    VideoRecorder, error::ScreenCaptureResult, mswindows::impl_monitor::ImplMonitor,
    video_recorder::Frame,
};

#[derive(Debug, Clone)]
pub struct Monitor {
    pub(crate) impl_monitor: ImplMonitor,
}

impl Monitor {
    pub(crate) fn new(impl_monitor: ImplMonitor) -> Monitor {
        Monitor { impl_monitor }
    }
}

impl Monitor {
    pub fn all() -> ScreenCaptureResult<Vec<Monitor>> {
        let monitors = ImplMonitor::all()?
            .iter()
            .map(|impl_monitor| Monitor::new(impl_monitor.clone()))
            .collect();

        Ok(monitors)
    }

    pub fn from_point(x: i32, y: i32) -> ScreenCaptureResult<Monitor> {
        let impl_monitor = ImplMonitor::from_point(x, y)?;

        Ok(Monitor::new(impl_monitor))
    }
}

impl Monitor {
    /// Unique identifier associated with the screen.
    pub fn id(&self) -> ScreenCaptureResult<u32> {
        self.impl_monitor.id()
    }
    /// Unique identifier associated with the screen.
    pub fn name(&self) -> ScreenCaptureResult<String> {
        self.impl_monitor.name()
    }
    /// The screen x coordinate.
    pub fn x(&self) -> ScreenCaptureResult<i32> {
        self.impl_monitor.x()
    }
    /// The screen x coordinate.
    pub fn y(&self) -> ScreenCaptureResult<i32> {
        self.impl_monitor.y()
    }
    /// The screen pixel width.
    pub fn width(&self) -> ScreenCaptureResult<u32> {
        self.impl_monitor.width()
    }
    /// The screen pixel height.
    pub fn height(&self) -> ScreenCaptureResult<u32> {
        self.impl_monitor.height()
    }
    /// Can be 0, 90, 180, 270, represents screen rotation in clock-wise degrees.
    pub fn rotation(&self) -> ScreenCaptureResult<f32> {
        self.impl_monitor.rotation()
    }
    /// Output device's pixel scale factor.
    pub fn scale_factor(&self) -> ScreenCaptureResult<f32> {
        self.impl_monitor.scale_factor()
    }
    /// The screen refresh rate.
    pub fn frequency(&self) -> ScreenCaptureResult<f32> {
        self.impl_monitor.frequency()
    }
    /// Whether the screen is the main screen
    pub fn is_primary(&self) -> ScreenCaptureResult<bool> {
        self.impl_monitor.is_primary()
    }

    /// Whether the screen is builtin
    pub fn is_builtin(&self) -> ScreenCaptureResult<bool> {
        self.impl_monitor.is_builtin()
    }
}

impl Monitor {
    /// Capture image of the monitor
    pub fn capture_image(&self) -> ScreenCaptureResult<RgbaImage> {
        self.impl_monitor.capture_image()
    }

    pub fn capture_region(
        &self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> ScreenCaptureResult<RgbaImage> {
        self.impl_monitor.capture_region(x, y, width, height)
    }

    pub fn video_recorder(&self) -> ScreenCaptureResult<(VideoRecorder, Receiver<Frame>)> {
        let (impl_video_recorder, sx) = self.impl_monitor.video_recorder()?;

        Ok((VideoRecorder::new(impl_video_recorder), sx))
    }
}

#[cfg(test)]
mod tests {
    use crate::ScreenCaptureError;

    use super::*;

    #[test]
    fn test_capture_region_out_of_bounds() {
        let monitors = Monitor::all().unwrap();
        let monitor = &monitors[0]; // Get first monitor

        // Try to capture a region that extends beyond monitor bounds
        let x = monitor.width().unwrap() / 2;
        let y = monitor.height().unwrap() / 2;
        let width = monitor.width().unwrap();
        let height = monitor.height().unwrap();

        let result = monitor.capture_region(x, y, width, height);

        match result {
            Err(ScreenCaptureError::InvalidCaptureRegion(_)) => (),
            _ => panic!("Expected InvalidCaptureRegion error"),
        }
    }
}
