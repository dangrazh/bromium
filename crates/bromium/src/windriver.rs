use std::thread;
use std::sync::Mutex;
use std::sync::mpsc::{channel, Receiver, Sender};

use pyo3::prelude::*;
// use windows::UI;
// use uiautomation::types::Handle;

use crate::sreen_context::ScreenContext;
use crate::uiauto::{get_ui_element_by_runtimeid}; // get_ui_element_by_xpath, get_element_by_xpath
use uitree::{SaveUIElementXML, UITreeXML, get_all_elements_xml};
// use crate::uiexplore::UITree;
use crate::app_control::launch_or_activate_application;

#[allow(unused_imports)]
use crate::commons::execute_with_timeout;
#[allow(unused_imports)]
use screen_capture::{Window, Monitor}; 

use fs_extra::dir;

use windows::Win32::Foundation::{POINT, RECT}; //HWND, 
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos}; //WindowFromPoint

use uiautomation::{UIElement}; //UIAutomation, 

use log::{debug, error, info, trace, warn};

pub static WINDRIVER: Mutex<Option<WinDriver>> = Mutex::new(None);


#[pyclass]
#[derive(Debug, Clone)]
pub struct Element {
    name: String,
    xpath: String,
    handle: isize,
    runtime_id: Vec<i32>,
    bounding_rectangle: RECT,

}


#[pymethods]
impl Element {

    #[new]
    pub fn new(name: String, xpath: String, handle: isize, runtime_id: Vec<i32>, bounding_rectangle: (i32, i32, i32, i32)) -> Self {
        debug!("Creating new Element: name='{}', xpath='{}', handle={}", name, xpath, handle);
        let bounding_rectangle  = RECT {
            left: bounding_rectangle.0,
            top: bounding_rectangle.1,
            right: bounding_rectangle.2,
            bottom: bounding_rectangle.3,
        };
        Element { name, xpath, handle, runtime_id , bounding_rectangle}
    }

    pub fn __repr__(&self) -> PyResult<String> {
        PyResult::Ok(format!("<Element\nname='{}'\nhandle = {}\nruntime_id = {:?}\nbounding_rectangle = {:?}>", self.name, self.handle, self.runtime_id, self.bounding_rectangle))
    }

    pub fn __str__(&self) -> PyResult<String> {
        PyResult::Ok(self.name.clone())
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_xpath(&self) -> String {
        self.xpath.clone()
    }

    pub fn get_handle(&self) -> isize {
        self.handle
    }

    pub fn get_runtime_id(&self) -> Vec<i32> {
        self.runtime_id.clone()
    }
    
    // Region mouse methods
    pub fn send_click(&self) -> PyResult<()> {
        debug!("Element::send_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.click() {
                Ok(_) => {
                    info!("Successfully clicked on element: {:#?}", e);
                }
                Err(e) => {
                    error!("Error clicking on element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Click failed"));
                }
                
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    pub fn send_double_click(&self) -> PyResult<()> {
        debug!("Element::send_double_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.double_click() {
                Ok(_) => {
                    info!("Double clicked on element: {:#?}", e);
                }
                Err(e) => {
                    error!("Error double clicking on element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Double click failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    pub fn send_right_click(&self) -> PyResult<()> {
        debug!("Element::send_right_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.right_click() {
                Ok(_) => {
                    info!("Right clicked on element: {:#?}", e);
                }
                Err(e) => {
                    error!("Error right clicking on element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Right click failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    pub fn hold_click(&self, holdkeys: String) -> PyResult<()> {
        debug!("Element::hold_click called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.hold_click(&holdkeys) {
                Ok(_) => {
                    info!("Hold clicked on element: {:#?}", e);
                }
                Err(e) => {
                    error!("Error hold clicking on element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Hold click failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    // Region keyboard methods
    pub fn send_keys(&self, keys: String) -> PyResult<()> {
        debug!("Element::send_keys called with keys: '{}' for element: {}", keys, self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.send_keys(&keys, 20) { // 20 ms interval for sending keys
                Ok(_) => {
                    info!("Sent keys '{}' to element: {:#?}", keys, e);
                }
                Err(e) => {
                    error!("Error sending keys to element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Send keys failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }    

    pub fn send_text(&self, text: String) -> PyResult<()> {
        debug!("Element::send_text called with text: '{}' for element: {}", text, self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.send_text(&text, 20) { // 20 ms interval for sending text
                Ok(_) => {
                    info!("Sent text '{}' to element: {:#?}", text, e);
                }
                Err(e) => {
                    error!("Error sending text to element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Send text failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    pub fn hold_send_keys(&self, holdkeys: String, keys: String, interval: u64) -> PyResult<()> {
        debug!("Element::hold_send_keys called with keys: '{}' for element: {}", keys, self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.hold_send_keys(&holdkeys, &keys, interval) { // hold for the specified duration
                Ok(_) => {
                    info!("Hold sent keys '{}' to element: {:#?}", keys, e);
                }
                Err(e) => {
                    error!("Error holding send keys to element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Hold send keys failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

    // Region misc methods
    pub fn show_context_menu(&self) -> PyResult<()> {
        debug!("Element::show_context_menu called for element: {}", self.name);
        if let Ok(e) = convert_to_ui_element(self) {
            match e.show_context_menu() {
                Ok(_) => {
                    info!("Context menu shown for element: {:#?}", e);
                }
                Err(e) => {
                    error!("Error showing context menu for element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Show context menu failed"));
                }
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
    }

}

impl From<&UIElement> for Element {
    fn from(ui_element: &UIElement) -> Self {
        debug!("Element::from called.");
        let bound_rect_res = ui_element.get_bounding_rectangle();
        let bounding_rect: RECT;
        match bound_rect_res {
            Ok(bounding_rect_inner) => {bounding_rect = bounding_rect_inner.into();},
            Err(e) => {
                error!("Error getting bounding rectangle: {:?}", e);
                bounding_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
            }
        }

        let native_handle: isize = ui_element.get_native_window_handle().unwrap_or_default().into();
        Element {
            name: ui_element.get_name().unwrap_or("".to_string()),
            xpath: String::new(), // XPath is not available here
            handle: native_handle,
            runtime_id: ui_element.get_runtime_id().unwrap_or(vec![0,0,0,0]),
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
    fn from (ui_element: &SaveUIElementXML) -> Self {
    debug!("Element::from called.");
    if let Some(props) = ui_element.get_ui_automation_ui_element() {
    
        let bound_rect_res = props.get_bounding_rectangle();
        let bounding_rect: RECT;
        match bound_rect_res {
            Ok(bounding_rect_inner) => {bounding_rect = bounding_rect_inner.into();},
            Err(e) => {
                error!("Error getting bounding rectangle: {:?}", e);
                bounding_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 }
            }
        }

        let native_handle: isize = props.get_native_window_handle().unwrap_or_default().into();
        Element {
            name: props.get_name().unwrap_or("".to_string()),
            xpath: String::new(), // XPath is not available here
            handle: native_handle,
            runtime_id: props.get_runtime_id().unwrap_or(vec![0,0,0,0]),
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
    if let Some(ui_element) = get_ui_element_by_runtimeid(element.get_runtime_id()) {
        debug!("Element found by runtime id.");
        return Ok(ui_element);
    } else {
        // TODO: This is a fallback in case the runtime id method fails.
        // If we end up here, it means the element is stale. We should refresh the UI tree and try again.
        error!("Element not found.");
        return Err(uiautomation::Error::new(uiautomation::errors::ERR_NOTFOUND, "could not find element"));
    }
}



#[pyclass]
#[derive(Debug, Clone)]
pub struct WinDriver {
    timeout_ms: u64,
    ui_tree: UITreeXML,
    items_in_ui_tree: usize,
    tree_needs_update: bool,
    // TODO: Add screen context to get scaling factor later on
}

impl WinDriver {
    pub fn get_ui_tree(&self) -> &UITreeXML {
        &self.ui_tree
    }
}

#[pymethods]
impl WinDriver {
    #[new]
    pub fn new(timeout_ms: u64) -> PyResult<Self> {
        debug!("Creating new WinDriver with timeout: {}ms", timeout_ms);

        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<UITreeXML>) = channel();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, None, None);
        });
        info!("Spawned separate thread to get ui tree");
        
        let ui_tree: UITreeXML = rx.recv().unwrap();
        let items_in_ui_tree = ui_tree.get_elements().len();
        debug!("UI tree received with {} elements", items_in_ui_tree);
        
        let driver = WinDriver { timeout_ms, ui_tree, items_in_ui_tree, tree_needs_update: false };

        *WINDRIVER.lock().unwrap() = Some(driver.clone());

        info!("WinDriver successfully created");
        Ok(driver)
    }

    pub fn __repr__(&self) -> PyResult<String> {
        PyResult::Ok(format!("<WinDriver timeout={}>, ui_tree={{object}}, items_in_ui_tree={}, tree_needs_update={}", self.timeout_ms, self.items_in_ui_tree, self.tree_needs_update))
    }

    pub fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }

    pub fn get_timeout(&self) -> u64 {
        self.timeout_ms
    }

    pub fn get_no_of_ui_elements(&self) -> usize {
        self.ui_tree.get_elements().len()
    }

    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    pub fn get_curser_pos(&self) -> PyResult<(i32, i32)> {
        debug!("WinDriver::get_curser_pos called.");
        let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            let _res= GetCursorPos(&mut point);
            PyResult::Ok((point.x, point.y))
        }
    }

    pub fn reload(&self) -> PyResult<Self> {
        debug!("WinDriver::reload called.");
        let driver: Self;
        {
            driver = WINDRIVER.lock().unwrap().clone().unwrap();
        }
        
        Ok(driver)
    }

    pub fn get_ui_element_by_coordinates(&self, x: i32, y: i32) -> PyResult<Element> {
        debug!("WinDriver::get_ui_element_by_coordinates called for coordinates: ({}, {})", x, y);

        let cursor_position = POINT { x, y };

        if let Some(ui_element_in_tree) = crate::rectangle::get_point_bounding_rect(&cursor_position, self.ui_tree.get_elements()) {
            let xpath = self.ui_tree.get_xpath_for_element(ui_element_in_tree.get_tree_index(), true);
            trace!("Found element with xpath: {}", xpath);

            let ui_element_props = ui_element_in_tree.get_element_props();
            let ui_element_props = ui_element_props.get_element();
            let bounding_rect = ui_element_props.get_bounding_rectangle();
            let element = Element::new(
                ui_element_props.get_name().clone(),
                xpath,
                ui_element_props.get_handle(),
                ui_element_props.get_runtime_id().clone(),
                (bounding_rect.get_left(), bounding_rect.get_top(), bounding_rect.get_right(), bounding_rect.get_bottom())
            );
            info!("Successfully found element at ({}, {}): {}", x, y, element.name);
            return PyResult::Ok(element)
        } else {
            warn!("No element found at coordinates ({}, {})", x, y);
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found at the given coordinates"))
        }

    }

    fn get_ui_element_by_xpath(&self, xpath: String) -> PyResult<Element> {
        debug!("WinDriver::get_ui_element_by_xpath called.");
        
        // let ui_elem = get_element_by_xpath(xpath.clone(), &self.ui_tree);
        debug!("Searching for element with xpath: {}", xpath);
        debug!("UI Tree has {} elements", self.ui_tree.get_elements().len());
        let ui_elem = self.ui_tree.get_element_by_xpath(xpath.as_str());
        if ui_elem.is_none() {
            debug!("Element not found for xpath: {}", xpath);
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        
        let element = ui_elem.unwrap();
        // PyResult::Ok(element)
        let name = element.get_name().clone();
        let xpath = xpath.clone();
        let handle = element.get_handle();
        let runtime_id = element.get_runtime_id().clone();
        let bounding_rectangle = element.get_bounding_rectangle();
        PyResult::Ok(Element::new(name, xpath, handle, runtime_id, (bounding_rectangle.get_left(), bounding_rectangle.get_top(), bounding_rectangle.get_right(), bounding_rectangle.get_bottom())))
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
                return PyResult::Err(pyo3::exceptions::PyValueError::new_err("No monitors found"));
            } else {
                debug!("Found {} monitors", mons.len());
                monitors = mons;
            }
        } else {
            error!("Failed to get monitors for screenshot");
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Failed to get monitors"));
        }

        let mut out_dir = std::env::temp_dir();
        out_dir = out_dir.join("bromium_screenshots");
        match dir::create_all(out_dir.clone(), true) {
            Ok(_) => {
                info!("Created screenshot directory at {:?}", out_dir);
            }
            Err(e) => {
                error!("Error creating screenshot directory: {:?}", e);
                return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Failed to create screenshot directory"));
            }
        }
        
        let primary_monitor: Option<Monitor> = monitors.into_iter().find(|m| m.is_primary().unwrap_or(false));
        if primary_monitor.is_none() {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("No primary monitor found"));
        }
        
        let monitor = primary_monitor.unwrap();
        let image = monitor.capture_image().unwrap();
        let filename = format!(
            "monitor-{}.png",
            normalized(monitor.name().unwrap()));
        let filenameandpath = out_dir.join(filename);
        match image.save(filenameandpath.clone()) {
            Ok(_) => {
                info!("Screenshot saved successfully to: {}", filenameandpath.to_str().unwrap());
                PyResult::Ok(filenameandpath.to_str().unwrap().to_string())
            }
            Err(e) => {
                error!("Error saving screenshot: {:?}", e);
                PyResult::Err(pyo3::exceptions::PyValueError::new_err("Failed to save screenshot"))
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
    pub fn launch_or_activate_app(&self, app_path: String, xpath: String) -> PyResult<Element> {
        debug!("WinDriver::launch_or_activate_app called with {} as app path and {} as xpath element.", app_path, xpath);

        let result = launch_or_activate_application(&app_path, &xpath);
        match result {
            Ok(save_ui_elem) => {
                info!("Application launched or activated successfully.");
                let ui_elem = Element::from(&save_ui_elem);
                PyResult::Ok(ui_elem)
            }
            Err(e) => {
                error!("Error launching or activating application: {}", e);
                PyResult::Err(pyo3::exceptions::PyValueError::new_err(format!("Failed to launch or activate application: {}", e)))
            }
        }
    }

    pub fn refresh_ui_tree(&mut self) -> PyResult<()> {
        debug!("WinDriver::refresh called.");
        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<UITreeXML>) = channel();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, None, None);
        });
        info!("Spawned separate thread to refresh ui tree");
        
        let ui_tree: UITreeXML = rx.recv().unwrap();
        
        self.ui_tree = ui_tree;
        self.tree_needs_update = false;
        
        {
            *WINDRIVER.lock().unwrap() = Some(self.clone());
        }

        info!("UITree successfully refreshed");
        debug!("UI Tree has now {} elements", self.ui_tree.get_elements().len());
        PyResult::Ok(())
    }

    pub fn refresh_ui_tree_top_2(&mut self) -> PyResult<()> {
        debug!("WinDriver::refresh called.");
        // get the ui tree in a separate thread
        let (tx, rx): (Sender<_>, Receiver<UITreeXML>) = channel();
        thread::spawn(|| {
            debug!("Spawning thread to get UI tree");
            get_all_elements_xml(tx, None, Some(2 as usize), None);
        });
        info!("Spawned separate thread to refresh ui tree");
        
        let ui_tree: UITreeXML = rx.recv().unwrap();
        
        self.ui_tree = ui_tree;
        self.tree_needs_update = false;
        
        {
            *WINDRIVER.lock().unwrap() = Some(self.clone());
        }

        info!("UITree successfully refreshed");
        PyResult::Ok(())
    }
}

fn normalized(filename: String) -> String {
    filename.replace(['|', '\\', ':', '/'], "")
}