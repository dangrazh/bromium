use crate::context::ScreenContext;
use crate::uiauto::{get_ui_element_by_runtimeid, get_ui_element_by_xpath, get_element_by_xpath};
use crate::xpath::generate_xpath;

#[allow(unused_imports)]
use crate::commons::execute_with_timeout;


use pyo3::prelude::*;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};

use uiautomation::types::Handle;
use uiautomation::{UIAutomation, UIElement};




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
    
    pub fn send_click(&self) -> PyResult<()> {
        if let Ok(e) = convert_to_ui_element(self) {
            match e.click() {
                Ok(_) => {
                    println!("Clicked on element: {:#?}", e);
                }
                Err(e) => {
                    println!("Error clicking on element: {:?}", e);
                    return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Click failed"));
                }
                
            }
        } else {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        PyResult::Ok(())
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

    // first try to get the element by runtime id
    if let Some(ui_element) = get_ui_element_by_runtimeid(element.get_runtime_id()) {
        return Ok(ui_element);
    } else {
        // if that fails, try to get the element by xpath
        if let Some(ui_element) = get_ui_element_by_xpath(element.get_xpath()) {
            return Ok(ui_element);
        } else {
            return Err(uiautomation::Error::new(uiautomation::errors::ERR_NOTFOUND, "could not find element"));
        }
    }

}



#[pyclass]
#[derive(Debug)]
pub struct WinDriver {
    timeout_ms: u64,
}

#[pymethods]
impl WinDriver {
    #[new]
    pub fn new(timeout_ms: u64) -> PyResult<Self> {
        Ok(WinDriver { timeout_ms })
    }

    fn __repr__(&self) -> PyResult<String> {
        PyResult::Ok(format!("<WinDriver timeout={}>", self.timeout_ms))
    }

    fn __str__(&self) -> PyResult<String> {
        self.__repr__()
    }

    pub fn get_timeout(&self) -> u64 {
        self.timeout_ms
    }

    pub fn set_timeout(&mut self, timeout_ms: u64) {
        self.timeout_ms = timeout_ms;
    }

    pub fn get_curser_pos(&self) -> PyResult<(i32, i32)> {
        let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        unsafe {
            let _res= GetCursorPos(&mut point);
            PyResult::Ok((point.x, point.y))
        }
    }

    pub fn get_ui_element(&self, x: i32, y: i32) -> PyResult<Element> {

        
        // Initialize UIAutomation
        let uia = UIAutomation::new().unwrap();

        let name: String;
        let xpath: String;
        let point = windows::Win32::Foundation::POINT { x, y };
        let hwnd: HWND;
        let handle: isize;
        let runtime_id: Vec<i32>;  
        let bounding_rectangle: uiautomation::types::Rect;
        // let uia_element: Arc<Mutex<Option<UIElement>>>;

        unsafe {
            // let _res= GetCursorPos(&mut point);
            hwnd = WindowFromPoint(point);
            println!("Window handle (native): {:?}", hwnd);
            handle = hwnd.0 as isize;
            println!("Window handle (isize): {:?}", handle);
            let elem_handle: Handle = Handle::from(hwnd.0 as isize);
            println!("Element handle (isize): {:?}", elem_handle);
            let element: Result<UIElement, uiautomation::Error> = uia.element_from_handle(elem_handle);
            match element {
                Ok(e) => {
                    name = e.get_name().unwrap_or("".to_string());
                    xpath = generate_xpath(x, y);
                    runtime_id = e.get_runtime_id().unwrap_or_default();
                    bounding_rectangle = e.get_bounding_rectangle().unwrap_or_default();
                }
                Err(_e) => {
                    name = "invalid hwnd".to_string();
                    xpath = "no xpath found".to_string();
                    runtime_id = vec![];
                    bounding_rectangle = uiautomation::types::Rect::default();
                }
            }
        }

        PyResult::Ok(Element {
            name: name,
            xpath: xpath,
            handle: handle,
            runtime_id: runtime_id,
            bounding_rectangle: RECT { left: bounding_rectangle.get_left(), top: bounding_rectangle.get_top(), right: bounding_rectangle.get_right(), bottom: bounding_rectangle.get_bottom() },
        })
    }

    fn get_ui_element_by_xpath(&self, xpath: String) -> PyResult<Element> {
        
        let ui_elem = get_element_by_xpath(xpath.clone());
        if ui_elem.is_none() {
            return PyResult::Err(pyo3::exceptions::PyValueError::new_err("Element not found"));
        }
        let element = ui_elem.unwrap();
        PyResult::Ok(element)
        // let name = element.get_name();
        // let xpath = element.get_xpath();
        // let handle = element.get_handle();
        // PyResult::Ok(Element { name, xpath, handle})
    }

    pub fn get_screen_context(&self) -> PyResult<ScreenContext> {
        let screen_context = ScreenContext::new();
        PyResult::Ok(screen_context)
    }
}

