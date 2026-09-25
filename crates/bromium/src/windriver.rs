use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pyo3::prelude::*;

use crate::exceptions::{
    AutomationError, ElementNotFoundError, StaleTreeError, TreeConstructionError,
};
use crate::screen_context::ScreenContext;
use crate::uiauto::{
    close_window, get_ui_element_by_runtimeid, invoke_click, select_item, set_value,
    supports_invoke, supports_select, supports_value,
};
use uitree::{SaveUIElementXML, TreeService, UITreeXML};

use crate::app_control::launch_or_activate_application;

use screen_capture::Monitor;

use std::fs;

use crate::logging;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

use uiautomation::UIElement;

use log::{debug, error, info, trace};

/// Monotonic counter for unique screenshot filenames.
static SCREENSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[pyclass]
#[derive(Debug, Clone)]
pub struct Bromium {}

#[pymethods]
impl Bromium {
    #[staticmethod]
    #[pyo3(signature = (log_path=None, log_level=None, enable_console=None, enable_file=None))]
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
    #[pyo3(signature = (timeout_ms=None, window_title=None))]
    pub fn get_win_driver(
        py: Python<'_>,
        timeout_ms: Option<u64>,
        window_title: Option<String>,
    ) -> PyResult<WinDriver> {
        debug!(
            "Bromium::get_win_driver called with timeout: {}ms",
            timeout_ms.unwrap_or(120000)
        );
        let driver = WinDriver::new(py, timeout_ms, window_title)?;
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
    service: Option<TreeService>,
    node_token: Option<usize>,
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
            service: None,
            node_token: None,
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

    pub fn __eq__(&self, other: &Element) -> bool {
        self.runtime_id == other.runtime_id
    }

    pub fn __hash__(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.runtime_id.hash(&mut hasher);
        hasher.finish()
    }

    // ─── Properties (Pythonic attribute access) ───────────────────────────────

    /// The name of the UI element.
    #[getter]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The XPath locator for this element within the UI tree.
    #[getter]
    pub fn xpath(&self) -> &str {
        &self.xpath
    }

    /// The native window handle (HWND) of this element.
    #[getter]
    pub fn handle(&self) -> isize {
        self.handle
    }

    /// The UI Automation control type (e.g. "Button", "Edit", "Window").
    #[getter]
    pub fn control_type(&self) -> &str {
        &self.control_type
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

    /// Request closure of this live element through its Window pattern.
    /// Does not close ancestors, terminate processes, or wait for disappearance.
    /// Unsupported patterns/provider errors raise AutomationError; obsolete
    /// identities raise ElementNotFoundError. Like other actions, releases the
    /// GIL but has no enforced execution deadline.
    pub fn close(&self, py: Python<'_>) -> PyResult<()> {
        let result = with_ui_element(py, self, "close", close_window);
        // The target can disappear during Close. Reconcile its parent's immediate
        // children as well as normal action coverage, without waiting for events.
        if let Some(service) = &self.service {
            service.invalidate_parent_membership(&self.runtime_id);
        }
        result
    }

    pub fn send_click(&self, py: Python<'_>) -> PyResult<()> {
        with_ui_element(py, self, "click", |e| {
            let raw = e.as_ref();
            if supports_invoke(raw) {
                invoke_click(raw).map_err(Into::into)
            } else if supports_select(raw) {
                select_item(raw).map_err(Into::into)
            } else {
                e.click()
            }
        })
    }

    pub fn send_double_click(&self, py: Python<'_>) -> PyResult<()> {
        with_ui_element(py, self, "double_click", |e| e.double_click())
    }
    pub fn send_right_click(&self, py: Python<'_>) -> PyResult<()> {
        with_ui_element(py, self, "right_click", |e| e.right_click())
    }
    pub fn hold_click(&self, py: Python<'_>, holdkeys: String) -> PyResult<()> {
        with_ui_element(py, self, "hold_click", |e| e.hold_click(&holdkeys))
    }
    pub fn send_keys(&self, py: Python<'_>, keys: String) -> PyResult<()> {
        with_ui_element(py, self, "send_keys", |e| e.send_keys(&keys, 20))
    }
    pub fn send_text(&self, py: Python<'_>, text: String) -> PyResult<()> {
        with_ui_element(py, self, "send_text", |e| {
            if supports_value(e.as_ref()) {
                set_value(e.as_ref(), text).map_err(Into::into)
            } else {
                if e.is_keyboard_focusable()? && !e.has_keyboard_focus()? {
                    e.set_focus()?;
                }
                e.send_text(&text, 20)
            }
        })
    }
    pub fn hold_send_keys(
        &self,
        py: Python<'_>,
        holdkeys: String,
        keys: String,
        interval: u64,
    ) -> PyResult<()> {
        with_ui_element(py, self, "hold_send_keys", |e| {
            e.hold_send_keys(&holdkeys, &keys, interval)
        })
    }
    pub fn show_context_menu(&self, py: Python<'_>) -> PyResult<()> {
        with_ui_element(py, self, "show_context_menu", |e| e.show_context_menu())
    }
}

impl Default for Element {
    fn default() -> Self {
        Element {
            name: String::new(),
            xpath: String::new(),
            handle: 0,
            control_type: String::new(),
            service: None,
            node_token: None,
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
fn with_ui_element<F>(
    py: Python<'_>,
    element: &Element,
    action_name: &str,
    action: F,
) -> PyResult<()>
where
    F: FnOnce(&UIElement) -> Result<(), uiautomation::Error> + Send,
{
    // Only Rust-owned values and COM interfaces created on this thread live inside
    // the detached operation. Python errors are constructed after reacquiring the GIL.
    let result = py.allow_threads(|| {
        let _invalidation = ActionInvalidation(element.service.clone(), element.runtime_id.clone());
        let live = convert_to_ui_element(element).map_err(|e| (true, e.to_string()))?;
        action(&live).map_err(|e| (false, e.to_string()))
    });
    result.map_err(|(resolution, reason)| {
        let message = format!(
            "{action_name} failed for runtime_id={:?}: {reason}",
            element.runtime_id
        );
        if resolution {
            ElementNotFoundError::new_err(message)
        } else {
            AutomationError::new_err(message)
        }
    })
}

fn convert_to_ui_element(element: &Element) -> Result<UIElement, uiautomation::Error> {
    if element.runtime_id.is_empty() {
        return Err(uiautomation::Error::new(
            1,
            "Empty runtime ID cannot identify an action target",
        ));
    }
    if let Some(service) = &element.service {
        let token = element.node_token.ok_or_else(|| {
            uiautomation::Error::new(1, "Element no longer belongs to the captured revision")
        })?;
        return service
            .resolve_live_expected(&element.runtime_id, Some(token))
            .map_err(|e| uiautomation::Error::new(1, &e));
    }
    // Legacy constructor compatibility: prefer validated HWND resolution, and
    // retain the unbound runtime-ID search only when no usable HWND was supplied.
    if element.handle != 0 {
        let automation = bromium_common::get_ui_automation_instance()?;
        let live =
            automation.element_from_handle(uiautomation::types::Handle::from(element.handle))?;
        if live.get_runtime_id()? != element.runtime_id {
            return Err(uiautomation::Error::new(
                1,
                "Native handle now identifies a different element",
            ));
        }
        return Ok(live);
    }
    get_ui_element_by_runtimeid(element.runtime_id.clone())
}

struct ActionInvalidation(Option<TreeService>, Vec<i32>);
impl Drop for ActionInvalidation {
    fn drop(&mut self) {
        if let Some(service) = &self.0 {
            service.invalidate_action(&self.1);
        }
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
    service: TreeService,
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
            props.get_xpath().map(str::to_owned).unwrap_or_else(|| {
                if props.get_runtime_id().is_empty() {
                    String::new()
                } else {
                    format!(
                        "//*[@RtID='{}']",
                        bromium_common::format_runtime_id(props.get_runtime_id())
                    )
                }
            }),
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

    fn attach(&self, mut element: Element) -> Element {
        element.service = Some(self.service.clone());
        element.node_token = self.ui_tree.index_for_id(&element.runtime_id);
        element
    }

    /// Collect all elements in the tree as Python `Element` objects.
    fn all_elements(&self) -> Vec<Element> {
        self.service
            .cached_view(self.window_title.as_deref())
            .get_elements()
            .iter()
            .map(|uit| {
                let mut element = self.attach(Self::element_from_save_ui(uit.get_element_props()));
                element.node_token = Some(uit.get_tree_index());
                element
            })
            .collect()
    }
}

#[pymethods]
impl WinDriver {
    #[new]
    #[pyo3(signature = (timeout_ms=None, window_title=None))]
    pub fn new(
        py: Python<'_>,
        timeout_ms: Option<u64>,
        window_title: Option<String>,
    ) -> PyResult<Self> {
        let service = TreeService::new();
        let ui_tree = py
            .allow_threads(|| {
                service.membership(Instant::now() + Duration::from_secs(DEFAULT_TREE_TIMEOUT_SECS))
            })
            .map_err(|e| TreeConstructionError::new_err(e.to_string()))?;
        Ok(Self {
            timeout_ms: timeout_ms.unwrap_or(120000),
            tree_timeout_secs: DEFAULT_TREE_TIMEOUT_SECS,
            ui_tree,
            window_title,
            service,
        })
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "<WinDriver timeout_ms={} element_count={} window_title={:?}>",
            self.timeout_ms,
            self.element_count(),
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

    /// Manual refresh and provider-job timeout in seconds (default: 120).
    #[getter]
    pub fn tree_timeout_secs(&self) -> u64 {
        self.tree_timeout_secs
    }

    /// Set subsequent manual-refresh and provider-job budgets; not an action timeout.
    #[setter]
    pub fn set_tree_timeout_secs(&mut self, secs: u64) {
        self.tree_timeout_secs = secs;
        self.service.set_capture_timeout(Duration::from_secs(secs));
    }

    /// Number of UI elements currently in the tree.
    #[getter]
    pub fn element_count(&self) -> usize {
        self.service.cached_count(self.window_title.as_deref())
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
        self.element_count()
    }

    /// Iterate over all elements in the UI tree (`for elem in driver`).
    pub fn __iter__(&self) -> ElementIterator {
        ElementIterator {
            elements: self.all_elements(),
            index: 0,
        }
    }

    /// Check if an element with the given XPath exists in the tree (`xpath in driver`).
    pub fn __contains__(&mut self, py: Python<'_>, xpath: String) -> PyResult<bool> {
        self.prepare_query(
            py,
            Some(&xpath),
            Instant::now() + Duration::from_millis(self.timeout_ms),
        )?;
        Ok(!self
            .ui_tree
            .query(&xpath)
            .map_err(pyo3::exceptions::PyValueError::new_err)?
            .is_empty())
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
        &mut self,
        py: Python<'_>,
        control_type: Option<String>,
        name: Option<String>,
    ) -> PyResult<Vec<Element>> {
        debug!(
            "WinDriver::find_elements called with control_type={:?}, name={:?}",
            control_type, name
        );

        self.prepare_query(
            py,
            None,
            Instant::now() + Duration::from_millis(self.timeout_ms),
        )?;
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
        Ok(results.into_iter().map(|e| self.attach(e)).collect())
    }

    // ─── Actions ─────────────────────────────────────────────────────────────

    pub fn get_cursor_pos(&self) -> PyResult<(i32, i32)> {
        debug!("WinDriver::get_cursor_pos called.");
        let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid stack-allocated POINT; GetCursorPos writes into it.
        unsafe {
            GetCursorPos(&mut point)
                .map_err(|e| AutomationError::new_err(format!("GetCursorPos failed: {}", e)))?;
        }
        Ok((point.x, point.y))
    }

    #[pyo3(signature = (window_title=None))]
    pub fn refresh(&mut self, py: Python<'_>, window_title: Option<String>) -> PyResult<()> {
        debug!("WinDriver::refresh called.");
        self.refresh_ui_tree(py, window_title)
    }

    pub fn get_element_by_coordinates(
        &mut self,
        py: Python<'_>,
        x: i32,
        y: i32,
    ) -> PyResult<Element> {
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        loop {
            self.ui_tree = self.service.snapshot();
            let handle = uitree::window_at_point(x, y)
                .ok_or_else(|| ElementNotFoundError::new_err("No window at point"))?;
            let mut window = self
                .ui_tree
                .children(0)
                .iter()
                .copied()
                .find(|&id| self.ui_tree.node(id).1.get_handle() == handle);
            if window.is_none() {
                self.ui_tree = py
                    .allow_threads(|| self.service.membership(deadline))
                    .map_err(|e| stale_error(py, e))?;
                window = self
                    .ui_tree
                    .children(0)
                    .iter()
                    .copied()
                    .find(|&id| self.ui_tree.node(id).1.get_handle() == handle);
            }
            let id = window.ok_or_else(|| {
                ElementNotFoundError::new_err("Window has no exposed UIA element")
            })?;
            if self
                .window_title
                .as_ref()
                .is_some_and(|title| !self.ui_tree.node(id).1.get_name().contains(title))
            {
                return Err(ElementNotFoundError::new_err(
                    "Point is outside the configured window scope",
                ));
            }
            self.ui_tree = py
                .allow_threads(|| self.service.ensure_region(id, deadline))
                .map_err(|e| stale_error(py, e))?;
            if uitree::window_at_point(x, y) != Some(handle) {
                if Instant::now() >= deadline {
                    return Err(stale_error(
                        py,
                        uitree::StaleTree {
                            reason: "Window under pointer changed during query".into(),
                            scope: self.window_title.clone(),
                            revision: self.ui_tree.revision(),
                            coverage: "geometry".into(),
                        },
                    ));
                }
                continue;
            }
            let hit = uitree::element_at_point(&self.ui_tree, x, y)
                .ok_or_else(|| ElementNotFoundError::new_err("No exposed element at point"))?;
            let mut result = Self::element_from_save_ui(hit.get_element_props());
            result.xpath = self
                .ui_tree
                .get_xpath_for_element(hit.get_tree_index(), false)
                .unwrap_or_default();
            return Ok(self.attach(result));
        }
    }

    /// Find a single element by XPath. If not found immediately, retries
    /// until `timeout_ms` elapses. When `timeout_ms` is `None`, the driver's
    /// default `timeout_ms` is used; pass `Some(0)` to disable retrying.
    #[pyo3(signature = (xpath, timeout_ms=None))]
    pub fn get_element_by_xpath(
        &mut self,
        py: Python<'_>,
        xpath: String,
        timeout_ms: Option<u64>,
    ) -> PyResult<Element> {
        let timeout = timeout_ms.unwrap_or(self.timeout_ms);
        let deadline = Instant::now() + Duration::from_millis(timeout);
        loop {
            self.prepare_query(py, Some(&xpath), deadline)?;
            if let Some(props) = self
                .ui_tree
                .query(&xpath)
                .map_err(pyo3::exceptions::PyValueError::new_err)?
                .first()
            {
                let mut element = Self::element_from_save_ui(props);
                element.xpath = xpath.clone();
                return Ok(self.attach(element));
            }
            if Instant::now() >= deadline {
                return Err(ElementNotFoundError::new_err(format!(
                    "Element not found for xpath '{xpath}'"
                )));
            }
            py.allow_threads(|| {
                thread::sleep(
                    Duration::from_millis(100)
                        .min(deadline.saturating_duration_since(Instant::now())),
                )
            });
            // Events and age validation repair coverage; do not rebuild on every clean miss.
        }
    }

    pub fn get_elements_by_xpath(
        &mut self,
        py: Python<'_>,
        xpath: String,
    ) -> PyResult<Vec<Element>> {
        self.prepare_query(
            py,
            Some(&xpath),
            Instant::now() + Duration::from_millis(self.timeout_ms),
        )?;
        debug!("WinDriver::get_elements_by_xpath called.");

        debug!("Searching for elements with xpath: {}", xpath);
        trace!("UI Tree has {} elements", self.ui_tree.get_elements().len());
        let elements = self
            .ui_tree
            .query(xpath.as_str())
            .map_err(pyo3::exceptions::PyValueError::new_err)?;

        if elements.is_empty() {
            debug!("No elements found for xpath: {}", xpath);
        }

        let results: Vec<Element> = elements
            .iter()
            .map(|element| self.attach(Self::element_from_save_ui(element)))
            .collect();
        Ok(results)
    }

    pub fn pretty_print_ui_tree(&self) -> PyResult<()> {
        debug!("WinDriver::pretty_print_tree called.");
        self.service
            .cached_view(self.window_title.as_deref())
            .pretty_print_tree();
        Ok(())
    }

    pub fn get_screen_context(&self) -> PyResult<ScreenContext> {
        debug!("WinDriver::get_screen_context called.");

        let screen_context = ScreenContext::new()?;
        Ok(screen_context)
    }

    pub fn take_screenshot(&self) -> PyResult<String> {
        debug!("WinDriver::take_screenshot called.");

        let monitors = Monitor::all().map_err(|e| {
            error!("Failed to get monitors for screenshot: {}", e);
            AutomationError::new_err("Failed to enumerate monitors")
        })?;
        if monitors.is_empty() {
            error!("No monitors found for screenshot");
            return Err(AutomationError::new_err("No monitors found"));
        }
        debug!("Found {} monitors", monitors.len());

        let out_dir = std::env::temp_dir().join("bromium_screenshots");
        fs::create_dir_all(&out_dir).map_err(|e| {
            error!("Error creating screenshot directory: {:?}", e);
            AutomationError::new_err(format!(
                "Failed to create screenshot directory '{}': {}",
                out_dir.display(),
                e
            ))
        })?;
        info!("Created screenshot directory at {:?}", out_dir);

        let Some(monitor) = monitors
            .into_iter()
            .find(|m| m.is_primary().unwrap_or(false))
        else {
            return Err(AutomationError::new_err("No primary monitor found"));
        };
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
    ///     Element: Captured metadata for the matched application element.
    pub fn launch_or_activate_app(
        &mut self,
        py: Python<'_>,
        app_path: String,
        xpath: String,
    ) -> PyResult<Element> {
        self.service
            .snapshot()
            .query(&xpath)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        debug!(
            "WinDriver::launch_or_activate_app called with {} as app path and {} as xpath element.",
            app_path, xpath
        );

        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        let result = py.allow_threads(|| {
            launch_or_activate_application(
                &self.service,
                self.window_title.as_deref(),
                &app_path,
                &xpath,
                deadline,
            )
        });
        match result {
            Ok(save_ui_elem) => {
                self.ui_tree = self.service.cached_view(self.window_title.as_deref());
                info!("Application launched or activated successfully.");
                let ui_elem = Self::element_from_save_ui(&save_ui_elem);
                Ok(self.attach(ui_elem))
            }
            Err(crate::app_control::AppControlError::Stale(e)) => Err(stale_error(py, e)),
            Err(crate::app_control::AppControlError::InvalidQuery(e)) => {
                Err(pyo3::exceptions::PyValueError::new_err(e))
            }
            Err(crate::app_control::AppControlError::Deadline(e)) => {
                Err(pyo3::exceptions::PyTimeoutError::new_err(e))
            }
            Err(e) => Err(AutomationError::new_err(e.to_string())),
        }
    }

    #[pyo3(signature = (window_title=None))]
    pub fn refresh_ui_tree(
        &mut self,
        py: Python<'_>,
        window_title: Option<String>,
    ) -> PyResult<()> {
        let title = window_title.or_else(|| self.window_title.clone());
        let tree = py
            .allow_threads(|| {
                self.service.refresh(
                    title.as_deref(),
                    Instant::now() + Duration::from_secs(self.tree_timeout_secs),
                )
            })
            .map_err(|e| stale_error(py, e))?;
        self.ui_tree = tree;
        self.window_title = title;
        Ok(())
    }

    #[getter]
    pub fn tree_status(&self) -> String {
        format!("scope={:?} {}", self.window_title, self.service.status())
    }

    /// Refresh only a cached element's region, preserving the driver's title scope.
    #[pyo3(signature = (element, timeout_ms=None))]
    pub fn refresh_region(
        &mut self,
        py: Python<'_>,
        element: &Element,
        timeout_ms: Option<u64>,
    ) -> PyResult<()> {
        let tree = self.service.snapshot();
        let index = tree.index_for_id(&element.runtime_id).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("Element is no longer in this driver's tree")
        })?;
        self.service.request_region(index);
        let deadline =
            Instant::now() + Duration::from_millis(timeout_ms.unwrap_or(self.timeout_ms));
        py.allow_threads(|| self.service.ensure_region(index, deadline))
            .map_err(|e| stale_error(py, e))?;
        self.ui_tree = self.service.cached_view(self.window_title.as_deref());
        Ok(())
    }

    /// Explicit nonblocking cached revision introspection.
    pub fn snapshot_elements(&mut self) -> Vec<Element> {
        self.ui_tree = self.service.snapshot();
        self.all_elements()
    }
}

// Shared service integration.
impl WinDriver {
    fn prepare_query(
        &mut self,
        py: Python<'_>,
        xpath: Option<&str>,
        deadline: Instant,
    ) -> PyResult<()> {
        self.ui_tree = self.service.snapshot();
        if let Some(xpath) = xpath {
            self.ui_tree
                .query(xpath)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        let tree = py
            .allow_threads(|| match xpath {
                Some(xpath) => {
                    self.service
                        .ensure_query(xpath, self.window_title.as_deref(), deadline)
                }
                None => self.service.ensure(self.window_title.as_deref(), deadline),
            })
            .map_err(|e| stale_error(py, e))?;
        self.ui_tree = tree;
        Ok(())
    }
    pub fn refresh_ui_tree_top_2(&mut self) -> PyResult<()> {
        self.ui_tree = self
            .service
            .membership(Instant::now() + Duration::from_millis(self.timeout_ms))
            .map_err(|e| StaleTreeError::new_err(e.to_string()))?;
        Ok(())
    }
    #[cfg(test)]
    fn extract_root_element_hint(xpath: &str) -> Option<(&str, String)> {
        for tag in &["Window", "Pane"] {
            let pattern = format!("{}[@Name='", tag);
            if let Some(start) = xpath.find(&pattern) {
                let after = &xpath[start + pattern.len()..];
                if let Some(end) = after.find("']") {
                    return Some((tag, after[..end].to_string()));
                }
            }
        }
        None
    }
}

fn stale_error(py: Python<'_>, stale: uitree::StaleTree) -> PyErr {
    let error = StaleTreeError::new_err(stale.to_string());
    let value = error.value(py);
    let _ = value.setattr("reason", stale.reason);
    let _ = value.setattr("scope", stale.scope);
    let _ = value.setattr("revision", stale.revision);
    let _ = value.setattr("coverage", stale.coverage);
    error
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

    #[test]
    fn test_extract_root_element_hint_window() {
        let result =
            WinDriver::extract_root_element_hint("//Window[@Name='Calculator']//Button[@Name='1']");
        assert_eq!(result, Some(("Window", "Calculator".to_string())));
    }

    #[test]
    fn test_extract_root_element_hint_pane() {
        let result =
            WinDriver::extract_root_element_hint("//Pane[@Name='Desktop']//Button[@Name='Start']");
        assert_eq!(result, Some(("Pane", "Desktop".to_string())));
    }

    #[test]
    fn test_extract_root_element_hint_window_priority_over_pane() {
        let result =
            WinDriver::extract_root_element_hint("//Window[@Name='App']/Pane[@Name='Content']");
        assert_eq!(result, Some(("Window", "App".to_string())));
    }

    #[test]
    fn test_extract_root_element_hint_no_match() {
        let result = WinDriver::extract_root_element_hint("//Button[@Name='OK']");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_root_element_hint_name_with_spaces() {
        let result =
            WinDriver::extract_root_element_hint("//Window[@Name='Notepad - Untitled']//Edit");
        assert_eq!(result, Some(("Window", "Notepad - Untitled".to_string())));
    }

    #[test]
    fn test_extract_root_element_hint_absolute_path() {
        let result = WinDriver::extract_root_element_hint("/Window[@Name='MyApp']/Panel/Button");
        assert_eq!(result, Some(("Window", "MyApp".to_string())));
    }
}
