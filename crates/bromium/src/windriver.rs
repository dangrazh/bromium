use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pyo3::prelude::*;

use crate::exceptions::{AutomationError, ElementNotFoundError, TreeConstructionError};
use crate::screen_context::ScreenContext;
use crate::uiauto::{
    get_ui_element_by_runtimeid, invoke_click, select_item, set_value, supports_invoke,
    supports_select, supports_value,
};
use uitree::{SaveUIElementXML, UITreeError, UITreeXML, get_all_elements_xml};

use crate::app_control::launch_or_activate_application;

use screen_capture::Monitor;

use std::fs;

use crate::logging;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use uiautomation::UIElement;

use log::{debug, error, info, trace, warn};

/// Monotonic counter for unique screenshot filenames.
static SCREENSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[pyclass]
#[derive(Debug, Clone)]
pub struct Bromium {}

#[pymethods]
impl Bromium {
    #[staticmethod]
    pub fn init_logging(
        log_path: Option<&str>,
        log_level: Option<&str>,
        enable_console: Option<bool>,
        enable_file: Option<bool>,
    ) -> PyResult<()> {
        // parse log directory if provided, otherwise default to None
        let log_dir = log_path.map(std::path::PathBuf::from);
        // parse log level if provided, otherwise default to Info
        let log_level_parsed: log::LevelFilter = log_level
            .and_then(|s| s.parse().ok())
            .unwrap_or(log::LevelFilter::Info);
        debug!("Log level parsed: {:?}", log_level_parsed);
        logging::init_logger(log_dir, log_level_parsed, enable_console, enable_file);
        info!("Bromium logging initialized.");
        Ok(())
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok("<Bromium>".to_string())
    }

    pub fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }
    #[staticmethod]
    pub fn get_win_driver(timeout_ms: u64, window_title: Option<String>) -> PyResult<WinDriver> {
        debug!(
            "Bromium::get_win_driver called with timeout: {}ms",
            timeout_ms
        );
        let driver = WinDriver::new(timeout_ms, window_title)?;
        Ok(driver)
    }

    #[staticmethod]
    pub fn get_version() -> PyResult<String> {
        let version = env!("CARGO_PKG_VERSION").to_string();
        Ok(version)
    }

    #[staticmethod]
    pub fn get_log_file() -> PyResult<String> {
        logging::get_log_file()
    }

    #[staticmethod]
    pub fn set_log_file(log_file: &str) -> PyResult<()> {
        logging::set_log_file(log_file.to_string())
    }

    #[staticmethod]
    pub fn get_log_level() -> PyResult<String> {
        logging::get_log_level()
    }

    #[staticmethod]
    pub fn set_log_level(log_level: &str) -> PyResult<()> {
        let level = logging::LogLevel::from(log_level);
        logging::set_log_level(level)
    }

    #[staticmethod]
    pub fn set_log_directory(log_directory: &str) -> PyResult<()> {
        logging::set_log_directory(log_directory.to_string())
    }

    #[staticmethod]
    pub fn enable_console_logging(enable: bool) -> PyResult<()> {
        logging::enable_console_logging(enable)
    }

    #[staticmethod]
    pub fn enable_file_logging(enable: bool) -> PyResult<()> {
        logging::enable_file_logging(enable)
    }

    #[staticmethod]
    pub fn reset_log_file() -> PyResult<()> {
        logging::reset_log_file()
    }
}

#[pyclass]
#[derive(Debug, Clone)]
pub struct Element {
    name: String,
    xpath: String,
    handle: isize,
    control_type: String,
    runtime_id: Vec<i32>,
    bounding_rectangle: RECT,
}

#[pymethods]
impl Element {
    #[new]
    pub fn new(
        name: String,
        xpath: String,
        handle: isize,
        control_type: String,
        runtime_id: Vec<i32>,
        bounding_rectangle: (i32, i32, i32, i32),
    ) -> Self {
        debug!(
            "Creating new Element: name='{}', xpath='{}', handle={}, control_type='{}'",
            name, xpath, handle, control_type
        );
        let bounding_rectangle = RECT {
            left: bounding_rectangle.0,
            top: bounding_rectangle.1,
            right: bounding_rectangle.2,
            bottom: bounding_rectangle.3,
        };
        Element {
            name,
            xpath,
            handle,
            control_type,
            runtime_id,
            bounding_rectangle,
        }
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "<Element name='{}' control_type='{}' handle={} runtime_id={:?} bounding_rectangle=({}, {}, {}, {})>",
            self.name,
            self.control_type,
            self.handle,
            self.runtime_id,
            self.bounding_rectangle.left,
            self.bounding_rectangle.top,
            self.bounding_rectangle.right,
            self.bounding_rectangle.bottom
        ))
    }

    pub fn __str__(&self) -> PyResult<String> {
        Ok(self.name.clone())
    }

    // ─── Properties (Pythonic attribute access) ───────────────────────────────

    /// The name of the UI element.
    #[getter]
    pub fn name(&self) -> String {
        self.name.clone()
    }

    /// The XPath locator for this element within the UI tree.
    #[getter]
    pub fn xpath(&self) -> String {
        self.xpath.clone()
    }

    /// The native window handle (HWND) of this element.
    #[getter]
    pub fn handle(&self) -> isize {
        self.handle
    }

    /// The UI Automation control type (e.g. "Button", "Edit", "Window").
    #[getter]
    pub fn control_type(&self) -> String {
        self.control_type.clone()
    }

    /// The runtime ID uniquely identifying this element in the current session.
    #[getter]
    pub fn runtime_id(&self) -> Vec<i32> {
        self.runtime_id.clone()
    }

    /// The bounding rectangle as (left, top, right, bottom).
    #[getter]
    pub fn bounding_rectangle(&self) -> (i32, i32, i32, i32) {
        (
            self.bounding_rectangle.left,
            self.bounding_rectangle.top,
            self.bounding_rectangle.right,
            self.bounding_rectangle.bottom,
        )
    }

    // ─── Mouse methods ──────────────────────────────────────────────────

    pub fn send_click(&self) -> PyResult<()> {
        debug!("Element::send_click called for element: {}", self.name);
        let e = convert_to_ui_element(self).map_err(|_| {
            ElementNotFoundError::new_err(format!(
                "Element '{}' not found (runtime_id={:?})",
                self.name, self.runtime_id
            ))
        })?;
        let raw_element = e.as_ref();
        if supports_invoke(raw_element) {
            debug!("Element supports Invoke pattern, using invoke_click.");
            invoke_click(raw_element).map_err(|err| {
                error!("Error invoking click on element: {:?}", err);
                AutomationError::new_err(format!(
                    "Invoke click failed on element '{}' (runtime_id={:?}): {}",
                    self.name, self.runtime_id, err
                ))
            })?;
        } else if supports_select(raw_element) {
            debug!("Element supports Select pattern, using select_item.");
            select_item(raw_element).map_err(|err| {
                error!("Error selecting item on element: {:?}", err);
                AutomationError::new_err(format!(
                    "Select item failed on element '{}' (runtime_id={:?}): {}",
                    self.name, self.runtime_id, err
                ))
            })?;
        } else {
            debug!(
                "Element does not support Invoke or Select pattern, using standard click as fallback."
            );
            e.click().map_err(|err| {
                error!("Error clicking on element: {:?}", err);
                AutomationError::new_err(format!(
                    "Click failed on element '{}' (runtime_id={:?}): {}",
                    self.name, self.runtime_id, err
                ))
            })?;
        }
        info!(
            "Successfully clicked on element: {}",
            e.get_name().unwrap_or("Name not set".to_string())
        );
        Ok(())
    }

    pub fn send_double_click(&self) -> PyResult<()> {
        debug!(
            "Element::send_double_click called for element: {}",
            self.name
        );
        with_ui_element(self, "double_click", |e| e.double_click())
    }

    pub fn send_right_click(&self) -> PyResult<()> {
        debug!(
            "Element::send_right_click called for element: {}",
            self.name
        );
        with_ui_element(self, "right_click", |e| e.right_click())
    }

    pub fn hold_click(&self, holdkeys: String) -> PyResult<()> {
        debug!("Element::hold_click called for element: {}", self.name);
        with_ui_element(self, "hold_click", |e| e.hold_click(&holdkeys))
    }

    // ─── Keyboard methods ───────────────────────────────────────────────

    pub fn send_keys(&self, keys: String) -> PyResult<()> {
        debug!(
            "Element::send_keys called with keys: '{}' for element: {}",
            keys, self.name
        );
        with_ui_element(self, "send_keys", |e| e.send_keys(&keys, 20))
    }

    pub fn send_text(&self, text: String) -> PyResult<()> {
        debug!(
            "Element::send_text called with text: '{}' for element: {}",
            text, self.name
        );
        if let Ok(e) = convert_to_ui_element(self) {
            let raw_element = e.as_ref();
            if supports_value(raw_element) {
                info!("Element supports Value pattern, using set_value.");
                match set_value(raw_element, text) {
                    Ok(_) => {
                        debug!(
                            "Successfully set value on element: {}",
                            e.get_name().unwrap_or("Name not set".to_string())
                        );
                    }
                    Err(err) => {
                        error!("Error setting value on element: {:?}", err);
                        return Err(AutomationError::new_err(format!(
                            "set_value failed on element '{}' (runtime_id={:?}): {}",
                            self.name, self.runtime_id, err
                        )));
                    }
                }
            } else {
                debug!("Element does not support Value pattern, using send_text as fallback");
                // check if the element has the focus and try setting it if not
                let is_focusable: bool = e.is_keyboard_focusable().unwrap_or_default();
                let has_focus: bool = e.has_keyboard_focus().unwrap_or_default();
                if is_focusable && !has_focus {
                    debug!(
                        "setting keyboard focus to element: {}",
                        e.get_name().unwrap_or("Name not set".to_string())
                    );
                    match e.set_focus() {
                        Ok(_) => {
                            info!("Set focus to element: {}", e);
                        }
                        Err(err) => {
                            error!(
                                "could not set keyboard focus on element: {} due to error: {}",
                                e, err
                            );
                            return Err(AutomationError::new_err(format!(
                                "Could not set keyboard focus on element '{}' (runtime_id={:?}): {}",
                                self.name, self.runtime_id, err
                            )));
                        }
                    };
                }

                match e.send_text(&text, 20) {
                    Ok(_) => {
                        info!("Sent text '{}' to element: {:#?}", text, e);
                    }
                    Err(err) => {
                        error!("Error sending text to element: {:?}", err);
                        return Err(AutomationError::new_err(format!(
                            "send_text failed on element '{}' with text='{}' (runtime_id={:?}): {}",
                            self.name, text, self.runtime_id, err
                        )));
                    }
                }
            }
        } else {
            return Err(ElementNotFoundError::new_err(format!(
                "Element '{}' not found (runtime_id={:?})",
                self.name, self.runtime_id
            )));
        }
        Ok(())
    }

    pub fn hold_send_keys(&self, holdkeys: String, keys: String, interval: u64) -> PyResult<()> {
        debug!(
            "Element::hold_send_keys called with keys: '{}' for element: {}",
            keys, self.name
        );
        with_ui_element(self, "hold_send_keys", |e| {
            e.hold_send_keys(&holdkeys, &keys, interval)
        })
    }

    // ─── Misc methods ───────────────────────────────────────────────────

    pub fn show_context_menu(&self) -> PyResult<()> {
        debug!(
            "Element::show_context_menu called for element: {}",
            self.name
        );
        with_ui_element(self, "show_context_menu", |e| e.show_context_menu())
    }
}

impl Default for Element {
    fn default() -> Self {
        Element {
            name: String::new(),
            xpath: String::new(),
            handle: 0,
            control_type: String::new(),
            runtime_id: vec![],
            bounding_rectangle: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        }
    }
}

/// Resolve the underlying `UIElement` for `element` and run `action` on it.
///
/// Encapsulates the repeated convert → act → map-error pattern used by
/// every `Element` action method.
fn with_ui_element<F>(element: &Element, action_name: &str, action: F) -> PyResult<()>
where
    F: FnOnce(&UIElement) -> Result<(), uiautomation::Error>,
{
    let e = convert_to_ui_element(element).map_err(|_| {
        ElementNotFoundError::new_err(format!(
            "Element '{}' not found (runtime_id={:?})",
            element.name, element.runtime_id
        ))
    })?;
    action(&e).map_err(|err| {
        error!("{} failed on element: {:?}", action_name, err);
        AutomationError::new_err(format!(
            "{} failed on element '{}' (runtime_id={:?}): {}",
            action_name, element.name, element.runtime_id, err
        ))
    })?;
    info!(
        "{} succeeded on element: {}",
        action_name,
        e.get_name().unwrap_or("Name not set".to_string())
    );
    Ok(())
}

fn convert_to_ui_element(element: &Element) -> Result<UIElement, uiautomation::Error> {
    debug!("Element::convert_to_ui_element called.");
    // first try to get the element by runtime id
    if let Some(ui_element) = get_ui_element_by_runtimeid(element.runtime_id.clone()) {
        debug!("Element found by runtime id.");
        Ok(ui_element)
    } else {
        error!("Element not found.");
        Err(uiautomation::Error::new(
            uiautomation::errors::ERR_NOTFOUND,
            "could not find element",
        ))
    }
}

/// Python iterator over `Element` objects returned by `WinDriver.__iter__()`.
#[pyclass]
#[derive(Debug, Clone)]
pub struct ElementIterator {
    elements: Vec<Element>,
    index: usize,
}

#[pymethods]
impl ElementIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<Element> {
        if self.index < self.elements.len() {
            let elem = self.elements[self.index].clone();
            self.index += 1;
            Some(elem)
        } else {
            None
        }
    }

    fn __len__(&self) -> usize {
        self.elements.len() - self.index
    }
}

/// Default timeout (in seconds) for tree-construction `recv_timeout` calls.
const DEFAULT_TREE_TIMEOUT_SECS: u64 = 120;

#[pyclass]
#[derive(Debug, Clone)]
pub struct WinDriver {
    timeout_ms: u64,
    /// Maximum seconds to wait for a tree-construction thread to finish.
    tree_timeout_secs: u64,
    ui_tree: UITreeXML,
    window_title: Option<String>,
    /// Cancellation flag for the most recently spawned tree-construction thread.
    /// Set to `true` on timeout to signal the orphaned thread to exit early.
    cancel_flag: Arc<AtomicBool>,
}

impl WinDriver {
    pub fn get_ui_tree(&self) -> &UITreeXML {
        &self.ui_tree
    }

    /// Convert a `SaveUIElement` (from the uitree crate) into a Python-facing `Element`.
    fn element_from_save_ui(props: &SaveUIElementXML) -> Element {
        let bounding_rect = props.get_bounding_rectangle();
        Element::new(
            props.get_name().to_string(),
            props.get_xpath().unwrap_or_default().to_string(),
            props.get_handle(),
            props.get_control_type().to_string(),
            props.get_runtime_id().to_vec(),
            (
                bounding_rect.get_left(),
                bounding_rect.get_top(),
                bounding_rect.get_right(),
                bounding_rect.get_bottom(),
            ),
        )
    }

    /// Collect all elements in the tree as Python `Element` objects.
    fn all_elements(&self) -> Vec<Element> {
        self.ui_tree
            .get_elements()
            .iter()
            .map(|uit| Self::element_from_save_ui(uit.get_element_props()))
            .collect()
    }
}

#[pymethods]
impl WinDriver {
    #[new]
    pub fn new(timeout_ms: u64, window_title: Option<String>) -> PyResult<Self> {
        if let Some(title) = window_title.as_deref() {
            debug!(
                "Creating new WinDriver with timeout: {}ms and window title filter: '{}'",
                timeout_ms, title
            );
        } else {
            debug!("Creating new WinDriver with timeout: {}ms", timeout_ms);
        }

        // get the ui tree in a separate thread
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        let window_title_clone = window_title.clone();
        let cancel_clone = Some(Arc::clone(&cancel_flag));
        thread::spawn(move || {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, Some(2), None, window_title_clone, cancel_clone);
        });
        info!("Spawned separate thread to get ui tree");

        let ui_tree: UITreeXML = rx
            .recv_timeout(Duration::from_secs(DEFAULT_TREE_TIMEOUT_SECS))
            .map_err(|e| {
                // Signal the orphaned thread to stop
                cancel_flag.store(true, Ordering::Relaxed);
                error!("UI tree creation timed out or channel error: {}", e);
                TreeConstructionError::new_err(format!(
                    "UI tree creation timed out or channel error: {}",
                    e
                ))
            })?
            .map_err(|e| {
                error!("UI tree creation failed: {}", e);
                TreeConstructionError::new_err(format!("UI tree creation failed: {}", e))
            })?;
        debug!(
            "UI tree received with {} elements",
            ui_tree.get_elements().len()
        );

        let driver = WinDriver {
            timeout_ms,
            tree_timeout_secs: DEFAULT_TREE_TIMEOUT_SECS,
            ui_tree,
            window_title,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        };

        info!("WinDriver successfully created");
        Ok(driver)
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "<WinDriver timeout_ms={} element_count={} window_title={:?}>",
            self.timeout_ms,
            self.ui_tree.get_elements().len(),
            self.window_title
        ))
    }

    pub fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }

    // ─── Properties (Pythonic attribute access) ───────────────────────────────

    /// The default timeout in milliseconds for element lookup operations.
    #[getter]
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Set the default timeout in milliseconds.
    #[setter]
    pub fn set_timeout_ms(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    /// Maximum seconds to wait for UI tree construction (default: 120).
    #[getter]
    pub fn tree_timeout_secs(&self) -> u64 {
        self.tree_timeout_secs
    }

    /// Set the tree-construction timeout in seconds.
    #[setter]
    pub fn set_tree_timeout_secs(&mut self, secs: u64) {
        self.tree_timeout_secs = secs;
    }

    /// Number of UI elements currently in the tree.
    #[getter]
    pub fn element_count(&self) -> usize {
        self.ui_tree.get_elements().len()
    }

    /// The window title filter, if set.
    #[getter]
    pub fn window_title(&self) -> Option<String> {
        self.window_title.clone()
    }

    /// Set the window title filter.
    #[setter]
    pub fn set_window_title(&mut self, window_title: Option<String>) {
        self.window_title = window_title;
    }

    // ─── Collection protocols (R-08) ───────────────────────────────────���────

    /// Returns the number of UI elements in the tree (`len(driver)`).
    pub fn __len__(&self) -> usize {
        self.ui_tree.get_elements().len()
    }

    /// Iterate over all elements in the UI tree (`for elem in driver`).
    pub fn __iter__(&self) -> ElementIterator {
        ElementIterator {
            elements: self.all_elements(),
            index: 0,
        }
    }

    /// Check if an element with the given XPath exists in the tree (`xpath in driver`).
    pub fn __contains__(&self, xpath: String) -> bool {
        self.ui_tree.get_element_by_xpath(xpath.as_str()).is_some()
    }

    /// Find elements matching optional filters.
    ///
    /// Args:
    ///     control_type (str | None): Filter by control type (e.g. "Button", "Edit").
    ///         Case-insensitive partial match.
    ///     name (str | None): Filter by element name. Case-insensitive substring match.
    ///
    /// Returns:
    ///     list[Element]: All matching elements. Returns an empty list if none match.
    ///
    /// Examples:
    ///     >>> driver.find_elements(control_type="Button")
    ///     >>> driver.find_elements(name="Save")
    ///     >>> driver.find_elements(control_type="Edit", name="Search")
    #[pyo3(signature = (control_type=None, name=None))]
    pub fn find_elements(
        &self,
        control_type: Option<String>,
        name: Option<String>,
    ) -> PyResult<Vec<Element>> {
        debug!(
            "WinDriver::find_elements called with control_type={:?}, name={:?}",
            control_type, name
        );

        let ct_filter = control_type.map(|s| s.to_lowercase());
        let name_filter = name.map(|s| s.to_lowercase());

        let results: Vec<Element> = self
            .ui_tree
            .get_elements()
            .iter()
            .filter(|uit| {
                let props = uit.get_element_props();
                if let Some(ref ct) = ct_filter
                    && !props
                        .get_control_type()
                        .to_lowercase()
                        .contains(ct.as_str())
                {
                    return false;
                }
                if let Some(ref n) = name_filter
                    && !props.get_name().to_lowercase().contains(n.as_str())
                {
                    return false;
                }
                true
            })
            .map(|uit| Self::element_from_save_ui(uit.get_element_props()))
            .collect();

        debug!("find_elements returned {} results", results.len());
        Ok(results)
    }

    // ─── Actions ─────────────────────────────────────────────────────────────

    pub fn get_cursor_pos(&self) -> PyResult<(i32, i32)> {
        debug!("WinDriver::get_cursor_pos called.");
        let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid stack-allocated POINT; GetCursorPos writes into it.
        unsafe {
            let _res = GetCursorPos(&mut point);
            Ok((point.x, point.y))
        }
    }

    pub fn refresh(&mut self, window_title: Option<String>) -> PyResult<()> {
        debug!("WinDriver::refresh called.");
        self.refresh_ui_tree(window_title)
    }

    pub fn get_element_by_coordinates(&self, x: i32, y: i32) -> PyResult<Element> {
        debug!(
            "WinDriver::get_ui_element_by_coordinates called for coordinates: ({}, {})",
            x, y
        );

        let cursor_position = POINT { x, y };

        if let Some(ui_element_in_tree) =
            crate::rectangle::get_point_bounding_rect(&cursor_position, self.ui_tree.get_elements())
        {
            let xpath = self
                .ui_tree
                .get_xpath_for_element(ui_element_in_tree.get_tree_index(), true)
                .unwrap_or_default();
            trace!("Found element with xpath: {}", xpath);

            let ui_element_props = ui_element_in_tree.get_element_props();
            let bounding_rect = ui_element_props.get_bounding_rectangle();
            let control_type = ui_element_props.get_control_type();

            let element = Element::new(
                ui_element_props.get_name().to_string(),
                xpath,
                ui_element_props.get_handle(),
                control_type.to_string(),
                ui_element_props.get_runtime_id().to_vec(),
                (
                    bounding_rect.get_left(),
                    bounding_rect.get_top(),
                    bounding_rect.get_right(),
                    bounding_rect.get_bottom(),
                ),
            );
            info!(
                "Successfully found element at ({}, {}): {}",
                x, y, element.name
            );
            Ok(element)
        } else {
            warn!("No element found at coordinates ({}, {})", x, y);
            Err(ElementNotFoundError::new_err(format!(
                "No element found at coordinates ({}, {})",
                x, y
            )))
        }
    }

    /// Find a single element by XPath. If not found immediately, retries
    /// until `timeout_ms` elapses. When `timeout_ms` is `None`, the driver's
    /// default `timeout_ms` is used; pass `Some(0)` to disable retrying.
    pub fn get_element_by_xpath(
        &mut self,
        py: Python<'_>,
        xpath: String,
        timeout_ms: Option<u64>,
    ) -> PyResult<Element> {
        debug!("WinDriver::get_element_by_xpath called.");

        debug!("Searching for element with xpath: {}", xpath);
        trace!("UI Tree has {} elements", self.ui_tree.get_elements().len());
        let ui_elem = self.ui_tree.get_element_by_xpath(xpath.as_str());

        if ui_elem.is_none() {
            // Resolve effective timeout: explicit param > driver default
            let effective_timeout = timeout_ms.unwrap_or(self.timeout_ms);

            if effective_timeout > 0 {
                debug!("Element not found, retrying for {} ms.", effective_timeout);
                let start_time = std::time::Instant::now();
                let tree_timeout = Duration::from_secs(self.tree_timeout_secs);
                while start_time.elapsed().as_millis() < effective_timeout as u128 {
                    let window_title_filter = self.window_title.clone();
                    // Cancel any previously orphaned tree-construction thread
                    self.cancel_flag.store(true, Ordering::Relaxed);
                    let cancel_flag = Arc::new(AtomicBool::new(false));
                    self.cancel_flag = Arc::clone(&cancel_flag);
                    let tree_result = py.allow_threads(move || {
                        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) =
                            channel();
                        let cancel_clone = Some(cancel_flag.clone());
                        thread::spawn(move || {
                            get_all_elements_xml(
                                tx,
                                None,
                                None,
                                None,
                                window_title_filter,
                                cancel_clone,
                            );
                        });
                        let result = rx.recv_timeout(tree_timeout);
                        if result.is_err() {
                            cancel_flag.store(true, Ordering::Relaxed);
                        }
                        result
                    });
                    self.ui_tree = tree_result
                        .map_err(|e| {
                            TreeConstructionError::new_err(format!(
                                "UI tree refresh failed (timeout or channel error): {}",
                                e
                            ))
                        })?
                        .map_err(|e| {
                            TreeConstructionError::new_err(format!("UI tree refresh failed: {}", e))
                        })?;

                    let ui_elem_retry = self.ui_tree.get_element_by_xpath(xpath.as_str());
                    if let Some(element) = ui_elem_retry {
                        debug!("Element found after refresh.");
                        let bounding_rectangle = element.get_bounding_rectangle();
                        return Ok(Element::new(
                            element.get_name().to_string(),
                            xpath.clone(),
                            element.get_handle(),
                            element.get_control_type().to_string(),
                            element.get_runtime_id().to_vec(),
                            (
                                bounding_rectangle.get_left(),
                                bounding_rectangle.get_top(),
                                bounding_rectangle.get_right(),
                                bounding_rectangle.get_bottom(),
                            ),
                        ));
                    }
                    trace!("Element still not found after refresh, trying again.");
                    py.allow_threads(|| thread::sleep(Duration::from_millis(250)));
                }
                debug!(
                    "Element not found after retrying for {} ms.",
                    effective_timeout
                );
                return Err(ElementNotFoundError::new_err(format!(
                    "Element not found for xpath '{}' after retrying for {}ms",
                    xpath, effective_timeout
                )));
            } else {
                debug!("Element not found, timeout is 0, returning error without retrying");
                return Err(ElementNotFoundError::new_err(format!(
                    "Element not found for xpath '{}'",
                    xpath
                )));
            }
        }

        let element = ui_elem.unwrap();
        let bounding_rectangle = element.get_bounding_rectangle();
        Ok(Element::new(
            element.get_name().to_string(),
            xpath,
            element.get_handle(),
            element.get_control_type().to_string(),
            element.get_runtime_id().to_vec(),
            (
                bounding_rectangle.get_left(),
                bounding_rectangle.get_top(),
                bounding_rectangle.get_right(),
                bounding_rectangle.get_bottom(),
            ),
        ))
    }

    pub fn get_elements_by_xpath(&self, xpath: String) -> PyResult<Vec<Element>> {
        debug!("WinDriver::get_elements_by_xpath called.");

        debug!("Searching for elements with xpath: {}", xpath);
        trace!("UI Tree has {} elements", self.ui_tree.get_elements().len());
        let Some(elements) = self.ui_tree.get_elements_by_xpath(xpath.as_str()) else {
            debug!("No elements found for xpath: {}", xpath);
            return Err(ElementNotFoundError::new_err(format!(
                "No elements found for xpath '{}'",
                xpath
            )));
        };

        let results: Vec<Element> = elements
            .iter()
            .map(|element| {
                let bounding_rectangle = element.get_bounding_rectangle();
                Element::new(
                    element.get_name().to_string(),
                    xpath.clone(),
                    element.get_handle(),
                    element.get_control_type().to_string(),
                    element.get_runtime_id().to_vec(),
                    (
                        bounding_rectangle.get_left(),
                        bounding_rectangle.get_top(),
                        bounding_rectangle.get_right(),
                        bounding_rectangle.get_bottom(),
                    ),
                )
            })
            .collect();
        Ok(results)
    }

    pub fn pretty_print_ui_tree(&self) -> PyResult<()> {
        debug!("WinDriver::pretty_print_tree called.");
        self.ui_tree.pretty_print_tree();
        Ok(())
    }

    pub fn get_screen_context(&self) -> PyResult<ScreenContext> {
        debug!("WinDriver::get_screen_context called.");

        let screen_context = ScreenContext::new()?;
        Ok(screen_context)
    }

    pub fn take_screenshot(&self) -> PyResult<String> {
        debug!("WinDriver::take_screenshot called.");

        let monitors: Vec<Monitor>;
        if let Ok(mons) = Monitor::all() {
            if mons.is_empty() {
                error!("No monitors found for screenshot");
                return Err(AutomationError::new_err("No monitors found"));
            } else {
                debug!("Found {} monitors", mons.len());
                monitors = mons;
            }
        } else {
            error!("Failed to get monitors for screenshot");
            return Err(AutomationError::new_err("Failed to enumerate monitors"));
        }

        let mut out_dir = std::env::temp_dir();
        out_dir = out_dir.join("bromium_screenshots");
        match fs::create_dir_all(&out_dir) {
            Ok(_) => {
                info!("Created screenshot directory at {:?}", out_dir);
            }
            Err(e) => {
                error!("Error creating screenshot directory: {:?}", e);
                return Err(AutomationError::new_err(format!(
                    "Failed to create screenshot directory '{}': {}",
                    out_dir.display(),
                    e
                )));
            }
        }

        let primary_monitor: Option<Monitor> = monitors
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false));
        if primary_monitor.is_none() {
            return Err(AutomationError::new_err("No primary monitor found"));
        }

        let monitor = primary_monitor.unwrap();
        let image = monitor.capture_image().map_err(|e| {
            AutomationError::new_err(format!("Failed to capture screenshot: {}", e))
        })?;
        let monitor_name = monitor
            .name()
            .map(normalized)
            .unwrap_or_else(|_| "unknown".to_string());
        let epoch_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let seq = SCREENSHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let filename = format!("monitor-{}-{}-{}.png", monitor_name, epoch_secs, seq);
        let filenameandpath = out_dir.join(filename);
        match image.save(&filenameandpath) {
            Ok(_) => {
                let path_str = filenameandpath.to_string_lossy().to_string();
                info!("Screenshot saved successfully to: {}", path_str);
                Ok(path_str)
            }
            Err(e) => {
                error!("Error saving screenshot: {:?}", e);
                Err(AutomationError::new_err(format!(
                    "Failed to save screenshot to '{}': {}",
                    filenameandpath.display(),
                    e
                )))
            }
        }
    }

    /// Launch or activate an application using its path and an XPath
    ///
    /// Args:
    ///     app_path (str): Full path to the application executable
    ///     xpath (str): XPath that identifies an element in the application window
    ///
    /// Returns:
    ///     bool: True if the application was successfully launched or activated
    pub fn launch_or_activate_app(&mut self, app_path: String, xpath: String) -> PyResult<Element> {
        debug!(
            "WinDriver::launch_or_activate_app called with {} as app path and {} as xpath element.",
            app_path, xpath
        );

        let result = launch_or_activate_application(self, &app_path, &xpath);
        match result {
            Ok(save_ui_elem) => {
                info!("Application launched or activated successfully.");
                let ui_elem = Self::element_from_save_ui(&save_ui_elem);
                Ok(ui_elem)
            }
            Err(e) => {
                error!("Error launching or activating application: {}", e);
                Err(AutomationError::new_err(format!(
                    "Failed to launch or activate application '{}': {}",
                    app_path, e
                )))
            }
        }
    }

    pub fn refresh_ui_tree(&mut self, window_title: Option<String>) -> PyResult<()> {
        debug!("WinDriver::refresh called.");

        // Cancel any previously orphaned tree-construction thread
        self.cancel_flag.store(true, Ordering::Relaxed);
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Arc::clone(&cancel_flag);

        // handle optional window title parameter
        // if a window title is provided, use it to filter the UI tree and
        // ignore the potentially stored window title in the WinDriver instance
        let window_title_filter = window_title.or_else(|| self.window_title.clone());
        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        let cancel_clone = Some(Arc::clone(&cancel_flag));
        thread::spawn(move || {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, None, None, window_title_filter, cancel_clone);
        });
        info!("Spawned separate thread to refresh ui tree");

        let ui_tree = rx
            .recv_timeout(Duration::from_secs(self.tree_timeout_secs))
            .map_err(|e| {
                // Signal the orphaned thread to stop
                cancel_flag.store(true, Ordering::Relaxed);
                TreeConstructionError::new_err(format!(
                    "UI tree refresh failed (timeout or channel error): {}",
                    e
                ))
            })?
            .map_err(|e| {
                TreeConstructionError::new_err(format!("UI tree refresh failed: {}", e))
            })?;

        self.ui_tree = ui_tree;

        info!("UITree successfully refreshed");
        debug!(
            "UI Tree has now {} elements",
            self.ui_tree.get_elements().len()
        );
        Ok(())
    }
}

// ─── Internal (non-Python) methods ───────────────────────────────────────────
impl WinDriver {
    /// Refresh the UI tree with a shallow (depth=2) walk.
    /// Used internally by `launch_or_activate_app` for fast re-scans.
    pub fn refresh_ui_tree_top_2(&mut self) -> PyResult<()> {
        debug!("WinDriver::refresh_ui_tree_top_2 called.");

        // Cancel any previously orphaned tree-construction thread
        self.cancel_flag.store(true, Ordering::Relaxed);
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Arc::clone(&cancel_flag);

        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        let cancel_clone = Some(Arc::clone(&cancel_flag));
        thread::spawn(move || {
            debug!("Spawning thread to get UI tree (depth=2)");
            get_all_elements_xml(tx, None, Some(2_usize), None, None, cancel_clone);
        });

        let ui_tree = rx
            .recv_timeout(Duration::from_secs(self.tree_timeout_secs))
            .map_err(|e| {
                // Signal the orphaned thread to stop
                cancel_flag.store(true, Ordering::Relaxed);
                TreeConstructionError::new_err(format!(
                    "UI tree refresh failed (timeout or channel error): {}",
                    e
                ))
            })?
            .map_err(|e| {
                TreeConstructionError::new_err(format!("UI tree refresh failed: {}", e))
            })?;

        self.ui_tree = ui_tree;

        info!("UITree successfully refreshed (shallow)");
        Ok(())
    }
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_element(name: &str, xpath: &str, ct: &str, handle: isize) -> Element {
        Element::new(
            name.to_string(),
            xpath.to_string(),
            handle,
            ct.to_string(),
            vec![1, 2, 3],
            (10, 20, 110, 120),
        )
    }

    #[test]
    fn test_element_new_stores_all_fields() {
        let elem = make_element("Save", "//Button[@Name='Save']", "Button", 42);
        assert_eq!(elem.name(), "Save");
        assert_eq!(elem.xpath(), "//Button[@Name='Save']");
        assert_eq!(elem.handle(), 42);
        assert_eq!(elem.control_type(), "Button");
        assert_eq!(elem.runtime_id(), vec![1, 2, 3]);
        assert_eq!(elem.bounding_rectangle(), (10, 20, 110, 120));
    }

    #[test]
    fn test_element_default_is_empty() {
        let elem = Element::default();
        assert_eq!(elem.name(), "");
        assert_eq!(elem.xpath(), "");
        assert_eq!(elem.handle(), 0);
        assert_eq!(elem.control_type(), "");
        assert!(elem.runtime_id().is_empty());
        assert_eq!(elem.bounding_rectangle(), (0, 0, 0, 0));
    }

    #[test]
    fn test_element_repr_contains_fields() {
        let elem = make_element("OK", "/Root/Button", "Button", 99);
        let repr = elem.__repr__().unwrap();
        assert!(repr.contains("name='OK'"));
        assert!(repr.contains("control_type='Button'"));
        assert!(repr.contains("handle=99"));
    }

    #[test]
    fn test_element_str_returns_name() {
        let elem = make_element("Cancel", "", "Button", 0);
        assert_eq!(elem.__str__().unwrap(), "Cancel");
    }

    #[test]
    fn test_element_clone_is_independent() {
        let elem = make_element("A", "/a", "Edit", 1);
        let cloned = elem.clone();
        assert_eq!(cloned.name(), elem.name());
        assert_eq!(cloned.handle(), elem.handle());
    }

    #[test]
    fn test_element_iterator_yields_all() {
        let elems = vec![
            make_element("A", "", "Button", 1),
            make_element("B", "", "Edit", 2),
            make_element("C", "", "Window", 3),
        ];
        let mut iter = ElementIterator {
            elements: elems,
            index: 0,
        };
        assert_eq!(iter.__len__(), 3);
        assert_eq!(iter.__next__().unwrap().name(), "A");
        assert_eq!(iter.__len__(), 2);
        assert_eq!(iter.__next__().unwrap().name(), "B");
        assert_eq!(iter.__next__().unwrap().name(), "C");
        assert!(iter.__next__().is_none());
        assert_eq!(iter.__len__(), 0);
    }

    #[test]
    fn test_element_iterator_empty() {
        let mut iter = ElementIterator {
            elements: vec![],
            index: 0,
        };
        assert_eq!(iter.__len__(), 0);
        assert!(iter.__next__().is_none());
    }

    #[test]
    fn test_element_from_save_ui_default() {
        let save = SaveUIElementXML::default();
        let elem = WinDriver::element_from_save_ui(&save);
        assert_eq!(elem.name(), "");
        assert_eq!(elem.xpath(), "");
        assert_eq!(elem.control_type(), "");
        assert_eq!(elem.handle(), 0);
        assert!(elem.runtime_id().is_empty());
        assert_eq!(elem.bounding_rectangle(), (0, 0, 0, 0));
    }

    #[test]
    fn test_element_from_save_ui_with_xpath() {
        let mut save = SaveUIElementXML::default();
        save.set_xpath("//Button[@Name='OK']".to_string());
        let elem = WinDriver::element_from_save_ui(&save);
        assert_eq!(elem.xpath(), "//Button[@Name='OK']");
    }

    #[test]
    fn test_normalized_strips_special_chars() {
        assert_eq!(normalized("a|b\\c:d/e".to_string()), "abcde");
    }

    #[test]
    fn test_normalized_preserves_regular_chars() {
        assert_eq!(normalized("hello_world.txt".to_string()), "hello_world.txt");
    }
}
