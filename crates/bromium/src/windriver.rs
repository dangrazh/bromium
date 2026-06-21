use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use pyo3::prelude::*;

use crate::exceptions::{AutomationError, ElementNotFoundError, TreeConstructionError};
use crate::screen_context::ScreenContext;
use crate::uiauto::{
    get_ui_element_by_runtimeid, invoke_click, select_item, set_value, supports_invoke,
    supports_select, supports_value,
};
use uitree::conversion::ConvertFromControlType;
use uitree::{SaveUIElementXML, UITreeError, UITreeXML, get_all_elements_xml};

use crate::app_control::launch_or_activate_application;

use screen_capture::Monitor;

use fs_extra::dir;

use crate::logging;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use uiautomation::UIElement;

use crate::logging::FromStrLevelFilter;
use log::{debug, error, info, trace, warn};

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
        let log_level_parsed: log::LevelFilter = match log_level {
            Some(level_str) => log::LevelFilter::from_str(level_str),
            None => log::LevelFilter::Info,
        };
        debug!("Log level parsed: {:?}", log_level_parsed);
        logging::init_logger(log_dir, log_level_parsed, enable_console, enable_file);
        info!("Bromium logging initialized.");
        PyResult::Ok(())
    }

    pub fn __repr__(&self) -> PyResult<String> {
        PyResult::Ok("<Bromium>".to_string())
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
        PyResult::Ok(driver)
    }

    #[staticmethod]
    pub fn get_version() -> PyResult<String> {
        let version = env!("CARGO_PKG_VERSION").to_string();
        PyResult::Ok(version)
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
        PyResult::Ok(format!(
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
        PyResult::Ok(self.name.clone())
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

    // Region mouse methods
    pub fn send_click(&self) -> PyResult<()> {
        debug!("Element::send_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            let raw_element = e.as_ref();
            if supports_invoke(raw_element) {
                debug!("Element supports Invoke pattern, using invoke_click.");
                match invoke_click(raw_element) {
                    Ok(_) => {
                        info!(
                            "Successfully invoked click on element: {}",
                            e.get_name().unwrap_or("Name not set".to_string())
                        );
                    }
                    Err(err) => {
                        error!("Error invoking click on element: {:?}", err);
                        return Err(AutomationError::new_err(format!(
                            "Invoke click failed on element '{}' (runtime_id={:?}): {}",
                            self.name, self.runtime_id, err
                        )));
                    }
                }
            } else if supports_select(raw_element) {
                debug!("Element supports Select pattern, using select_item.");
                match select_item(raw_element) {
                    Ok(_) => {
                        info!(
                            "Successfully selected item on element: {}",
                            e.get_name().unwrap_or("Name not set".to_string())
                        );
                    }
                    Err(err) => {
                        error!("Error selecting item on element: {:?}", err);
                        return Err(AutomationError::new_err(format!(
                            "Select item failed on element '{}' (runtime_id={:?}): {}",
                            self.name, self.runtime_id, err
                        )));
                    }
                }
            } else {
                debug!(
                    "Element does not support Invoke or Select pattern, using standard click as fallback."
                );
                match e.click() {
                    Ok(_) => {
                        info!(
                            "Successfully clicked on element: {}",
                            e.get_name().unwrap_or("Name not set".to_string())
                        );
                    }
                    Err(err) => {
                        error!("Error clicking on element: {:?}", err);
                        return Err(AutomationError::new_err(format!(
                            "Click failed on element '{}' (runtime_id={:?}): {}",
                            self.name, self.runtime_id, err
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

    pub fn send_double_click(&self) -> PyResult<()> {
        debug!(
            "Element::send_double_click called for element: {}",
            self.name
        );
        if let Ok(e) = convert_to_ui_element(self) {
            match e.double_click() {
                Ok(_) => {
                    info!("Double clicked on element: {:#?}", e);
                }
                Err(err) => {
                    error!("Error double clicking on element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "Double click failed on element '{}' (runtime_id={:?}): {}",
                        self.name, self.runtime_id, err
                    )));
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

    pub fn send_right_click(&self) -> PyResult<()> {
        debug!(
            "Element::send_right_click called for element: {}",
            self.name
        );
        if let Ok(e) = convert_to_ui_element(self) {
            match e.right_click() {
                Ok(_) => {
                    info!("Right clicked on element: {:#?}", e);
                }
                Err(err) => {
                    error!("Error right clicking on element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "Right click failed on element '{}' (runtime_id={:?}): {}",
                        self.name, self.runtime_id, err
                    )));
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

    pub fn hold_click(&self, holdkeys: String) -> PyResult<()> {
        debug!("Element::hold_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.hold_click(&holdkeys) {
                Ok(_) => {
                    info!("Hold clicked on element: {:#?}", e);
                }
                Err(err) => {
                    error!("Error hold clicking on element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "Hold click failed on element '{}' with holdkeys='{}' (runtime_id={:?}): {}",
                        self.name, holdkeys, self.runtime_id, err
                    )));
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

    // Region keyboard methods
    pub fn send_keys(&self, keys: String) -> PyResult<()> {
        debug!(
            "Element::send_keys called with keys: '{}' for element: {}",
            keys, self.name
        );
        if let Ok(e) = convert_to_ui_element(self) {
            match e.send_keys(&keys, 20) {
                Ok(_) => {
                    info!("Sent keys '{}' to element: {:#?}", keys, e);
                }
                Err(err) => {
                    error!("Error sending keys to element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "send_keys failed on element '{}' with keys='{}' (runtime_id={:?}): {}",
                        self.name, keys, self.runtime_id, err
                    )));
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
        if let Ok(e) = convert_to_ui_element(self) {
            match e.hold_send_keys(&holdkeys, &keys, interval) {
                Ok(_) => {
                    info!("Hold sent keys '{}' to element: {:#?}", keys, e);
                }
                Err(err) => {
                    error!("Error holding send keys to element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "hold_send_keys failed on element '{}' with holdkeys='{}', keys='{}' (runtime_id={:?}): {}",
                        self.name, holdkeys, keys, self.runtime_id, err
                    )));
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

    // Region misc methods
    pub fn show_context_menu(&self) -> PyResult<()> {
        debug!(
            "Element::show_context_menu called for element: {}",
            self.name
        );
        if let Ok(e) = convert_to_ui_element(self) {
            match e.show_context_menu() {
                Ok(_) => {
                    info!("Context menu shown for element: {:#?}", e);
                }
                Err(err) => {
                    error!("Error showing context menu for element: {:?}", err);
                    return Err(AutomationError::new_err(format!(
                        "show_context_menu failed on element '{}' (runtime_id={:?}): {}",
                        self.name, self.runtime_id, err
                    )));
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
}

impl From<&UIElement> for Element {
    fn from(ui_element: &UIElement) -> Self {
        debug!("Element::from called.");
        let bound_rect_res = ui_element.get_bounding_rectangle();
        let bounding_rect: RECT = match bound_rect_res {
            Ok(bounding_rect_inner) => bounding_rect_inner.into(),
            Err(e) => {
                error!("Error getting bounding rectangle: {:?}", e);
                RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                }
            }
        };

        let native_handle: isize = ui_element
            .get_native_window_handle()
            .unwrap_or_default()
            .into();

        let control_type: String = match ui_element.get_control_type() {
            Ok(ct) => ct.as_str().to_string(),
            Err(_) => "Control Type undefined".to_string(),
        };

        Element {
            name: ui_element.get_name().unwrap_or_default(),
            xpath: String::new(),
            handle: native_handle,
            control_type,
            runtime_id: ui_element.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]),
            bounding_rectangle: RECT {
                left: bounding_rect.left,
                top: bounding_rect.top,
                right: bounding_rect.right,
                bottom: bounding_rect.bottom,
            },
        }
    }
}

impl From<&SaveUIElementXML> for Element {
    fn from(ui_element: &SaveUIElementXML) -> Self {
        debug!("Element::from called.");
        if let Some(props) = ui_element.get_ui_automation_ui_element() {
            let bound_rect_res = props.get_bounding_rectangle();
            let bounding_rect: RECT = match bound_rect_res {
                Ok(bounding_rect_inner) => bounding_rect_inner.into(),
                Err(e) => {
                    error!("Error getting bounding rectangle: {:?}", e);
                    RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    }
                }
            };

            let control_type: String = match props.get_control_type() {
                Ok(ct) => ct.as_str().to_string(),
                Err(_) => "Control Type undefined".to_string(),
            };

            let native_handle: isize = props.get_native_window_handle().unwrap_or_default().into();
            Element {
                name: props.get_name().unwrap_or_default(),
                xpath: String::new(),
                handle: native_handle,
                control_type,
                runtime_id: props.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]),
                bounding_rectangle: RECT {
                    left: bounding_rect.left,
                    top: bounding_rect.top,
                    right: bounding_rect.right,
                    bottom: bounding_rect.bottom,
                },
            }
        } else {
            error!("UIAutomation element properties not found in SaveUIElementXML");
            Element::default()
        }
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

#[pyclass]
#[derive(Debug, Clone)]
pub struct WinDriver {
    timeout_ms: u64,
    ui_tree: UITreeXML,
    tree_needs_update: bool,
    window_title: Option<String>,
}

impl WinDriver {
    pub fn get_ui_tree(&self) -> &UITreeXML {
        &self.ui_tree
    }

    /// Convert a `SaveUIElement` (from the uitree crate) into a Python-facing `Element`.
    fn element_from_save_ui(props: &SaveUIElementXML) -> Element {
        let bounding_rect = props.get_bounding_rectangle();
        Element::new(
            props.get_name().clone(),
            props.get_xpath().cloned().unwrap_or_default(),
            props.get_handle(),
            props.get_control_type().clone(),
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
        if let Some(title) = window_title.clone() {
            debug!(
                "Creating new WinDriver with timeout: {}ms and window title filter: '{}'",
                timeout_ms, title
            );
        } else {
            debug!("Creating new WinDriver with timeout: {}ms", timeout_ms);
        }

        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        let window_title_1 = window_title.clone();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, Some(2), None, window_title_1);
        });
        info!("Spawned separate thread to get ui tree");

        let ui_tree: UITreeXML = rx
            .recv_timeout(Duration::from_secs(120))
            .map_err(|e| {
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
            ui_tree,
            tree_needs_update: false,
            window_title,
        };

        info!("WinDriver successfully created");
        Ok(driver)
    }

    pub fn __repr__(&self) -> PyResult<String> {
        PyResult::Ok(format!(
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
        unsafe {
            let _res = GetCursorPos(&mut point);
            PyResult::Ok((point.x, point.y))
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
            let ui_element_props = ui_element_props.get_element();
            let bounding_rect = ui_element_props.get_bounding_rectangle();
            let control_type = ui_element_props.get_control_type();

            let element = Element::new(
                ui_element_props.get_name().clone(),
                xpath,
                ui_element_props.get_handle(),
                control_type.clone(),
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
            PyResult::Ok(element)
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
                while start_time.elapsed().as_millis() < effective_timeout as u128 {
                    let window_title_filter = self.window_title.clone();
                    self.refresh_ui_tree(window_title_filter)?;
                    let ui_elem_retry = self.ui_tree.get_element_by_xpath(xpath.as_str());
                    if let Some(element) = ui_elem_retry {
                        debug!("Element found after refresh.");
                        let name = element.get_name().clone();
                        let xpath = xpath.clone();
                        let handle = element.get_handle();
                        let control_type = element.get_control_type();
                        let runtime_id = element.get_runtime_id().to_vec();
                        let bounding_rectangle = element.get_bounding_rectangle();
                        return Ok(Element::new(
                            name,
                            xpath,
                            handle,
                            control_type.clone(),
                            runtime_id,
                            (
                                bounding_rectangle.get_left(),
                                bounding_rectangle.get_top(),
                                bounding_rectangle.get_right(),
                                bounding_rectangle.get_bottom(),
                            ),
                        ));
                    }
                    trace!("Element still not found after refresh, trying again.");
                    std::thread::sleep(std::time::Duration::from_millis(250));
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
        // PyResult::Ok(element)
        let name = element.get_name().clone();
        let xpath = xpath.clone();
        let handle = element.get_handle();
        let control_type = element.get_control_type();
        let runtime_id = element.get_runtime_id().to_vec();
        let bounding_rectangle = element.get_bounding_rectangle();
        PyResult::Ok(Element::new(
            name,
            xpath,
            handle,
            control_type.clone(),
            runtime_id,
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
        let ui_elems = self.ui_tree.get_elements_by_xpath(xpath.as_str());
        if ui_elems.is_none() {
            debug!("No elements found for xpath: {}", xpath);
            return Err(ElementNotFoundError::new_err(format!(
                "No elements found for xpath '{}'",
                xpath
            )));
        }

        let elements = ui_elems.unwrap();
        let mut results: Vec<Element> = Vec::new();
        // PyResult::Ok(element)
        for element in &elements {
            trace!("Found element: {:?}", element);
            let name = element.get_name().clone();
            let xpath = xpath.clone();
            let handle = element.get_handle();
            let control_type = element.get_control_type();
            let runtime_id = element.get_runtime_id().to_vec();
            let bounding_rectangle = element.get_bounding_rectangle();
            let elem = Element::new(
                name,
                xpath,
                handle,
                control_type.clone(),
                runtime_id,
                (
                    bounding_rectangle.get_left(),
                    bounding_rectangle.get_top(),
                    bounding_rectangle.get_right(),
                    bounding_rectangle.get_bottom(),
                ),
            );
            results.push(elem);
        }
        PyResult::Ok(results)
    }

    pub fn pretty_print_ui_tree(&self) -> PyResult<()> {
        debug!("WinDriver::pretty_print_tree called.");
        self.ui_tree.pretty_print_tree();
        PyResult::Ok(())
    }

    pub fn get_screen_context(&self) -> PyResult<ScreenContext> {
        debug!("WinDriver::get_screen_context called.");

        let screen_context = ScreenContext::new();
        PyResult::Ok(screen_context)
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
        match dir::create_all(out_dir.clone(), true) {
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
        let filename = format!("monitor-{}.png", monitor_name);
        let filenameandpath = out_dir.join(filename);
        match image.save(filenameandpath.clone()) {
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
                let ui_elem = Element::from(&save_ui_elem);
                PyResult::Ok(ui_elem)
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

        // handle optional window title parameter
        // if a window title is provided, use it to filter the UI tree and
        // ignore the potentially stored window title in the WinDriver instance
        let window_title_filter: Option<String>;
        if window_title.is_none() {
            if let Some(stored_title) = &self.window_title {
                window_title_filter = Some(stored_title.clone());
            } else {
                window_title_filter = None;
            }
        } else {
            window_title_filter = window_title;
        }
        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, None, None, window_title_filter);
        });
        info!("Spawned separate thread to refresh ui tree");

        let ui_tree = rx
            .recv_timeout(Duration::from_secs(120))
            .map_err(|e| {
                TreeConstructionError::new_err(format!(
                    "UI tree refresh failed (timeout or channel error): {}",
                    e
                ))
            })?
            .map_err(|e| {
                TreeConstructionError::new_err(format!("UI tree refresh failed: {}", e))
            })?;

        self.ui_tree = ui_tree;
        self.tree_needs_update = false;

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
        let (tx, rx): (Sender<_>, Receiver<Result<UITreeXML, UITreeError>>) = channel();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree (depth=2)");
            get_all_elements_xml(tx, None, Some(2_usize), None, None);
        });

        let ui_tree = rx
            .recv_timeout(Duration::from_secs(120))
            .map_err(|e| {
                TreeConstructionError::new_err(format!(
                    "UI tree refresh failed (timeout or channel error): {}",
                    e
                ))
            })?
            .map_err(|e| {
                TreeConstructionError::new_err(format!("UI tree refresh failed: {}", e))
            })?;

        self.ui_tree = ui_tree;
        self.tree_needs_update = false;

        info!("UITree successfully refreshed (shallow)");
        Ok(())
    }
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}
