mod error;
mod monitor;
mod mswindows;
mod video_recorder;
mod window;

pub use image;

pub use error::{ScreenCaptureError, ScreenCaptureResult};
pub use monitor::Monitor;
pub use window::Window;

pub use video_recorder::Frame;
pub use video_recorder::VideoRecorder;
