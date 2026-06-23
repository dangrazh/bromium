use display_info::DisplayInfo;

use pyo3::prelude::*;

#[pyclass]
#[derive(Debug, Clone)]
pub struct ScreenInfo {
    /// Unique identifier associated with the display.
    pub id: u32,
    /// The display name
    pub name: String,
    /// The display friendly name
    pub friendly_name: String,
    /// The display x coordinate.
    pub x: i32,
    /// The display x coordinate.
    pub y: i32,
    /// The display pixel width.
    pub width: u32,
    /// The display pixel height.
    pub height: u32,
    /// The width of a display in millimeters. This value may be 0.
    pub width_mm: i32,
    /// The height of a display in millimeters. This value may be 0.
    pub height_mm: i32,
    /// Can be 0, 90, 180, 270, represents screen rotation in clock-wise degrees.
    pub rotation: f32,
    /// Output device's pixel scale factor.
    pub scale_factor: f32,
    /// The display refresh rate.
    pub frequency: f32,
    /// Whether the screen is the main screen
    pub is_primary: bool,
}

impl From<DisplayInfo> for ScreenInfo {
    fn from(info: DisplayInfo) -> Self {
        Self {
            id: info.id,
            name: info.name,
            friendly_name: info.friendly_name,
            x: info.x,
            y: info.y,
            width: info.width,
            height: info.height,
            width_mm: info.width_mm,
            height_mm: info.height_mm,
            rotation: info.rotation,
            scale_factor: info.scale_factor,
            frequency: info.frequency,
            is_primary: info.is_primary,
        }
    }
}

#[pymethods]
impl ScreenInfo {
    #[allow(clippy::too_many_arguments)]
    #[new]
    pub fn new(
        id: u32,
        name: String,
        friendly_name: String,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        width_mm: i32,
        height_mm: i32,
        rotation: f32,
        scale_factor: f32,
        frequency: f32,
        is_primary: bool,
    ) -> Self {
        Self {
            id,
            name,
            friendly_name,
            x,
            y,
            width,
            height,
            width_mm,
            height_mm,
            rotation,
            scale_factor,
            frequency,
            is_primary,
        }
    }

    // ─── Properties (Pythonic attribute access) ───────────────────────────────

    #[getter]
    pub fn id(&self) -> u32 {
        self.id
    }
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[getter]
    pub fn friendly_name(&self) -> &str {
        &self.friendly_name
    }
    #[getter]
    pub fn x(&self) -> i32 {
        self.x
    }
    #[getter]
    pub fn y(&self) -> i32 {
        self.y
    }
    #[getter]
    pub fn width(&self) -> u32 {
        self.width
    }
    #[getter]
    pub fn height(&self) -> u32 {
        self.height
    }
    #[getter]
    pub fn width_mm(&self) -> i32 {
        self.width_mm
    }
    #[getter]
    pub fn height_mm(&self) -> i32 {
        self.height_mm
    }
    #[getter]
    pub fn rotation(&self) -> f32 {
        self.rotation
    }
    #[getter]
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }
    #[getter]
    pub fn frequency(&self) -> f32 {
        self.frequency
    }
    #[getter]
    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn __repr__(&self) -> String {
        format!(
            "<ScreenInfo id={} name='{}' friendly_name='{}' x={} y={} width={} height={} is_primary={}>",
            self.id,
            self.name,
            self.friendly_name,
            self.x,
            self.y,
            self.width,
            self.height,
            self.is_primary
        )
    }
    pub fn __str__(&self) -> String {
        self.__repr__()
    }
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct ScreenContext {
    screens: Vec<ScreenInfo>,
    primary_screen: ScreenInfo,
}

#[pymethods]
impl ScreenContext {
    #[new]
    pub fn new() -> PyResult<Self> {
        let displays = DisplayInfo::all().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Failed to enumerate displays: {}",
                e
            ))
        })?;

        let screens: Vec<ScreenInfo> = displays.into_iter().map(ScreenInfo::from).collect();

        let primary_screen = screens
            .iter()
            .find(|screen| screen.is_primary)
            .cloned()
            .or_else(|| screens.first().cloned())
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("No screens found"))?;

        Ok(Self {
            screens,
            primary_screen,
        })
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "<ScreenContext primary_screen='{}' screens_count={}>",
            self.primary_screen.name,
            self.screens.len()
        ))
    }
    pub fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }

    // ─── Properties ──────────────────────────────────────────────────────────

    /// The primary display screen.
    #[getter]
    pub fn primary_screen(&self) -> ScreenInfo {
        self.primary_screen.clone()
    }

    /// List of all available display screens.
    #[getter]
    pub fn screens(&self) -> Vec<ScreenInfo> {
        self.screens.clone()
    }
}
