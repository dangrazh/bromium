use log::{debug, error, info};
use uiautomation::UIElement;
use uiautomation::types::Handle;

use bromium_common::{RuntimeIdFilter, get_ui_automation_instance};

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
    /// Construct a `SaveUIElement` by extracting all properties from a `UIElement`
    /// reference. The `UIElement` is borrowed — no COM `AddRef`/`Release` is needed.
    pub fn new(element: &UIElement, level: usize, z_order: usize) -> Self {
        let name = element.get_name().unwrap_or_default();
        let classname = element.get_classname().unwrap_or_default();
        let control_type = element
            .get_control_type()
            .map(|ct| ct.to_string())
            .unwrap_or_default();
        let localized_control_type = element.get_localized_control_type().unwrap_or_default();
        let framework_id = element.get_framework_id().unwrap_or_default();
        let runtime_id = element.get_runtime_id().unwrap_or_default();
        let automation_id = element.get_automation_id().unwrap_or_default();
        let handle: isize = element
            .get_native_window_handle()
            .unwrap_or(Handle::from(0_isize))
            .into();
        let bounding_rect = element
            .get_bounding_rectangle()
            .unwrap_or(uiautomation::types::Rect::new(0, 0, 0, 0));
        let bounding_rect_size = (bounding_rect.get_right() - bounding_rect.get_left())
            * (bounding_rect.get_bottom() - bounding_rect.get_top());

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
            level,
            z_order,
            xpath: None,
        }
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
    pub fn get_runtime_id(&self) -> &[i32] {
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
        debug!(
            "Setting focus to element with runtime id: {:?}",
            self.runtime_id
        );
        if let Some(elem) = self.get_ui_automation_ui_element() {
            elem.set_focus()
        } else {
            Err(uiautomation::Error::new(
                1,
                "Element not found for setting focus",
            ))
        }
    }

    pub fn set_xpath(&mut self, xpath: String) {
        self.xpath = Some(xpath)
    }

    pub fn get_ui_automation_ui_element(&self) -> Option<UIElement> {
        debug!(
            "Getting ui element from SaveUIElement with runtime id: {:?}",
            self.runtime_id
        );

        let Some(uia) = get_ui_automation_instance() else {
            error!("Failed to create UIAutomation instance");
            return None;
        };

        // Fast path: O(1) lookup by window handle when available
        if self.handle != 0 {
            let handle = Handle::from(self.handle);
            match uia.element_from_handle(handle) {
                Ok(e) => {
                    debug!("Element found by handle: {}", self.handle);
                    return Some(e);
                }
                Err(e) => {
                    debug!(
                        "element_from_handle failed ({}), falling back to runtime ID search",
                        e
                    );
                }
            }
        }

        // Fallback: full tree search by runtime ID
        let runtime_id: Vec<i32> = self.runtime_id.clone();
        let matcher = uia
            .create_matcher()
            .timeout(0)
            .filter(Box::new(RuntimeIdFilter(runtime_id)))
            .depth(99);

        match matcher.find_first() {
            Ok(e) => {
                info!("Element found by runtime id: {:?}", e);
                Some(e)
            }
            Err(e) => {
                error!("Error finding element by runtime id: {:?}", e);
                None
            }
        }
    }
}

impl std::fmt::Display for SaveUIElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SaveUIElement {{ name: {}, classname: {}, control_type: {}, localized_control_type: {}, framework_id: {}, runtime_id: {:?}, automation_id: {}, handle: {}, bounding_rect: {:?}, bounding_rect_size: {}, level: {}, z_order: {}, xpath: {:?} }}",
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
