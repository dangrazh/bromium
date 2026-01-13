// use std::sync::Mutex;

use uiautomation::{UIAutomation, UIElement};
use uiautomation::types::Handle;
use log::{debug, info, warn, error};

#[derive(Debug, Clone)]
pub struct SaveUIElement {
    name: String,
    classname: String,
    control_type: String,
    localized_control_type: String,
    framework_id: String,
    runtime_id: Vec<i32>,
    automation_id: String,
    handle: isize,
    bounding_rect: uiautomation::types::Rect,
    bounding_rect_size: i32,
    level: usize,
    z_order: usize,
    xpath: Option<String>,
}

impl SaveUIElement {
    pub fn new(from_element: UIElement, level: usize, z_order: usize) -> Self {
        let mut elem = SaveUIElement::from(from_element);
        elem.z_order = z_order;
        elem.level = level;
        elem
    }

    pub fn get_name(&self) -> &String {
        &self.name
    } 
    pub fn get_classname(&self) -> &String {
        &self.classname
    }
    pub fn get_control_type(&self) -> &String {
        &self.control_type
    }
    pub fn get_localized_control_type(&self) -> &String {
        &self.localized_control_type
    }
    pub fn get_framework_id(&self) -> &String {
        &self.framework_id
    }
    pub fn get_runtime_id(&self) -> &Vec<i32> {
        &self.runtime_id
    }   
    pub fn get_automation_id(&self) -> &String {
        &self.automation_id
    }
    pub fn get_handle(&self) -> isize {
        self.handle
    }
    pub fn get_bounding_rect_size(&self) -> i32 {
        self.bounding_rect_size
    }
    pub fn get_bounding_rectangle(&self) -> &uiautomation::types::Rect {
        &self.bounding_rect
    }
    pub fn get_level(&self) -> usize {
        self.level
    }
    pub fn get_z_order(&self) -> usize {
        self.z_order
    }
    pub fn get_xpath(&self) -> Option<&String> {
        self.xpath.as_ref()
    }

    // return reference to self to avoid
    // code using the SaveUIElement from breaking
    // after we changed the internal implementation
    pub fn get_element(&self) -> &Self {
        self
    }

    pub fn set_focus(&self) -> uiautomation::Result<()> {
        debug!("Setting focus to element with runtime id: {:?}", self.runtime_id);
        if let Some(elem) = self.get_ui_automation_ui_element() {
            elem.set_focus()
        } else {
            Err(uiautomation::Error::new(1, "Element not found for setting focus"))
        }
    }

    pub fn set_xpath(&mut self, xpath: String) {
        self.xpath = Some(xpath)
    }

    pub fn get_ui_automation_ui_element(&self) -> Option<UIElement> {
        debug!("Getting ui element from SaveUIElement with runtime id: {:?}", self.runtime_id);
        
        let runtime_id: Vec<i32> = self.runtime_id.clone();
        
        let uia = get_ui_automation_instance().unwrap();
        let matcher = uia.create_matcher().timeout(0).filter(Box::new(RuntimeIdFilter(runtime_id))).depth(99);
        let element = matcher.find_first();
        
        match element {
            Ok(e) => {
                info!("Element found by runtime id: {:?}", e);
                Some(e)
            },
            Err(e) => {
                error!("Error finding element by runtime id: {:?}", e);
                None
            }
        }
        
    }



}

impl std::fmt::Display for SaveUIElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SaveUIElement {{ name: {}, classname: {}, control_type: {}, localized_control_type: {}, framework_id: {}, runtime_id: {:?}, automation_id: {}, handle: {}, bounding_rect: {:?}, bounding_rect_size: {}, level: {}, z_order: {}, xpath: {:?} }}",
            self.name,
            self.classname,
            self.control_type,
            self.localized_control_type,
            self.framework_id,
            self.runtime_id,
            self.automation_id,
            self.handle,
            self.bounding_rect,
            self.bounding_rect_size,
            self.level,
            self.z_order,
            self.xpath,
        )
    }
}

impl Default for SaveUIElement {
    fn default() -> Self {
        SaveUIElement {
            name: String::new(),
            classname: String::new(),
            control_type: String::new(),
            localized_control_type: String::new(),
            framework_id: String::new(),
            runtime_id: Vec::new(),
            automation_id: String::new(),
            handle: 0,
            bounding_rect: uiautomation::types::Rect::new(0, 0, 0, 0),
            bounding_rect_size: 0,
            level: 0,
            z_order: 0,
            xpath: None,
        }
    }
}


impl From<UIElement> for SaveUIElement {
    fn from(item: UIElement) -> Self {

        let name: String = item.get_name().unwrap_or("".to_string());
        let classname: String = item.get_classname().unwrap_or("".to_string());
        
        let mut control_type: String = "".to_string();
        if let Ok(ctrl_type) =  item.get_control_type() {
            control_type = ctrl_type.to_string();    
        }

        let localized_control_type: String = item.get_localized_control_type().unwrap_or("".to_string());
        let framework_id: String = item.get_framework_id().unwrap_or("".to_string());
        let runtime_id: Vec<i32> = item.get_runtime_id().unwrap_or(Vec::new());
        let automation_id: String = item.get_automation_id().unwrap_or("".to_string());
        let handle : isize = item.get_native_window_handle().unwrap_or(Handle::from(0 as isize)).into();
        let bounding_rect: uiautomation::types::Rect = item.get_bounding_rectangle().unwrap_or(uiautomation::types::Rect::new(0, 0, 0, 0));
        let bounding_rect_size: i32 = (bounding_rect.get_right() - bounding_rect.get_left()) * (bounding_rect.get_bottom() - bounding_rect.get_top());
        // let element = Mutex::new(None);
        
        SaveUIElement {
            name,
            classname,
            control_type,
            localized_control_type,
            framework_id,
            runtime_id,
            automation_id,
            handle,
            bounding_rect,
            bounding_rect_size,
            level: 0,
            z_order: 0,
            xpath: None,
            // element,
        }
    }
}

impl TryFrom<&SaveUIElement> for UIElement {
    type Error = ();

    fn try_from(value: &SaveUIElement) -> Result<Self, Self::Error> {
        if let Some(elem) = value.get_ui_automation_ui_element() {
            Ok(elem)
        } else {
            Err(())
        
        }
    }
}

// #[allow(dead_code)]
pub fn get_ui_automation_instance() -> Option<UIAutomation> {
    debug!("Creating UIAutomation instance");

    let uia: UIAutomation;
    let uia_res = UIAutomation::new();
    
    match uia_res {
        Ok(uia_ok) => {
            uia = uia_ok;
            info!("UIAutomation instance created successfully");
        },
        Err(e) => {
            warn!("Failed to create UIAutomation instance, trying direct method: {:?}", e);
            let uia_direct_res = UIAutomation::new_direct();
            match uia_direct_res {
                Ok(uia_direct_ok) => {
                    uia = uia_direct_ok;
                    info!("UIAutomation instance created successfully using direct method.");
                },
                Err(e_direct) => {
                    error!("Failed to create UIAutomation instance using direct method: {:?}", e_direct);
                    return None; // Return None if we cannot create a UIAutomation instance
                }
            }
        }
        
    }
    Some(uia)

}

// #[allow(dead_code)]
struct RuntimeIdFilter(Vec<i32>);

impl uiautomation::filters::MatcherFilter for RuntimeIdFilter {
    fn judge(&self, element: &UIElement) -> uiautomation::Result<bool> {
        // self is the element we are looking for
        // element is the element we are checking against
        let id = element.get_runtime_id()?;
        Ok(id == self.0)
    }
}

// #[allow(dead_code)]
// pub fn get_ui_element_by_runtimeid(runtime_id: Vec<i32>) -> Option<UIElement> {
//     debug!("Searching for element with runtime id: {:?}", runtime_id);
//     // let automation = UIAutomation::new().unwrap();
//     let uia = get_ui_automation_instance().unwrap();
//     let matcher = uia.create_matcher().timeout(0).filter(Box::new(RuntimeIdFilter(runtime_id))).depth(99);
//     let element = matcher.find_first();
    
//     match element {
//         Ok(e) => {
//             info!("Element found by runtime id: {:?}", e);
//             Some(e)
//         },
//         Err(e) => {
//             error!("Error finding element by runtime id: {:?}", e);
//             None
//         }
//     }
    
// }
