use log::{debug, error, info};
use uiautomation::UIElement;
use uiautomation::types::Handle;

use bromium_common::{RuntimeIdFilter, get_ui_automation_instance};

/// Tree occurrence: provider identity plus native handle, qualified by the full
/// parent chain up to the nearest native anchor for handle-less elements.
/// The raw runtime ID remains unchanged for public APIs and event matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElementIdentity {
    pub runtime_id: Vec<i32>,
    pub handle: isize,
    /// Immediate parent occurrence, recursively ending at a native anchor.
    /// Never based on sibling position, name or bounds.
    pub ancestor: Option<Box<ElementIdentity>>,
    /// Publication-only identity for ambiguous or missing provider IDs. Never
    /// used to re-identify a live element; fresh observations get fresh tokens.
    pub occurrence: Option<u64>,
}
impl ElementIdentity {
    pub fn key(&self) -> String {
        let mut key = format!(
            "{}@{}",
            bromium_common::format_runtime_id(&self.runtime_id),
            self.handle
        );
        if let Some(token) = self.occurrence {
            key.push_str(&format!("!{token}"));
        }
        self.ancestor
            .as_ref()
            .map_or(key.clone(), |a| format!("{key}~{}", a.key()))
    }
    pub(crate) fn qualify(&mut self, ancestor: Option<&ElementIdentity>) {
        self.ancestor = if self.handle == 0 {
            ancestor.cloned().map(Box::new)
        } else {
            None
        };
    }
    /// Point ancestry is ordered leaf-to-root, unlike capture traversal.
    pub(crate) fn qualify_path(path: &mut [Self]) {
        let mut context = None;
        for id in path.iter_mut().rev() {
            id.qualify(context.as_ref());
            context = id.child_context();
        }
    }
    pub(crate) fn child_context(&self) -> Option<Self> {
        (self.handle != 0 || self.ancestor.is_some() || self.occurrence.is_some())
            .then(|| self.clone())
    }
    pub fn is_resolvable(&self) -> bool {
        !self.runtime_id.is_empty()
            && self.occurrence.is_none()
            && self.ancestor.as_ref().is_none_or(|a| a.is_resolvable())
    }
    pub(crate) fn native_anchor(&self) -> Option<&Self> {
        let mut current = self;
        loop {
            if current.handle != 0 {
                return Some(current);
            }
            current = current.ancestor.as_deref()?;
        }
    }
    /// Provider properties only; callers traversing a known path supply occurrence context.
    pub(crate) fn matches_provider(&self, element: &UIElement) -> bool {
        self.is_resolvable()
            && element.get_runtime_id().ok().as_ref() == Some(&self.runtime_id)
            && element
                .get_native_window_handle()
                .ok()
                .map(|h| -> isize { h.into() })
                == Some(self.handle)
    }
    pub fn matches_live(&self, element: &UIElement) -> bool {
        if !self.matches_provider(element) {
            return false;
        }
        let Some(mut expected) = self.ancestor.as_deref() else {
            return true;
        };
        let Ok(a) = get_ui_automation_instance() else {
            return false;
        };
        let Ok(walker) = a.get_control_view_walker() else {
            return false;
        };
        let mut current = element.clone();
        for _ in 0..256 {
            let Ok(parent) = walker.get_parent(&current) else {
                return false;
            };
            if !expected.matches_provider(&parent) {
                return false;
            }
            let Some(next) = expected.ancestor.as_deref() else {
                return true;
            };
            expected = next;
            current = parent;
        }
        false
    }
}

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
    ancestor: Option<Box<ElementIdentity>>,
    occurrence: Option<u64>,
    bounding_rect: uiautomation::types::Rect,
    bounding_rect_size: i64,
    level: usize,
    z_order: usize,
    xpath: Option<String>,
}

impl SaveUIElement {
    pub fn identity(&self) -> ElementIdentity {
        ElementIdentity {
            runtime_id: self.runtime_id.clone(),
            handle: self.handle,
            ancestor: self.ancestor.clone(),
            occurrence: self.occurrence,
        }
    }
    pub(crate) fn qualify(&mut self, ancestor: Option<&ElementIdentity>) {
        let mut identity = self.identity();
        identity.qualify(ancestor);
        self.ancestor = identity.ancestor;
    }
    pub(crate) fn child_context(&self) -> Option<ElementIdentity> {
        self.identity().child_context()
    }
    pub(crate) fn mark_snapshot_only(&mut self) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        if self.occurrence.is_none() {
            self.occurrence = Some(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        }
    }
    #[cfg(test)]
    pub(crate) fn with_handle(mut self, handle: isize) -> Self {
        self.handle = handle;
        self
    }
    #[cfg(test)]
    pub(crate) fn with_rectangle(mut self, left: i32, top: i32, right: i32, bottom: i32) -> Self {
        self.bounding_rect = uiautomation::types::Rect::new(left, top, right, bottom);
        self.bounding_rect_size = i64::from(right - left) * i64::from(bottom - top);
        self
    }
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
        let bounding_rect_size = (i64::from(bounding_rect.get_right())
            - i64::from(bounding_rect.get_left()))
            * (i64::from(bounding_rect.get_bottom()) - i64::from(bounding_rect.get_top()));

        SaveUIElement {
            name,
            classname,
            control_type,
            localized_control_type,
            framework_id,
            runtime_id,
            automation_id,
            handle,
            ancestor: None,
            occurrence: None,
            bounding_rect,
            bounding_rect_size,
            level,
            z_order,
            xpath: None,
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }
    pub fn get_classname(&self) -> &str {
        &self.classname
    }
    pub fn get_control_type(&self) -> &str {
        &self.control_type
    }
    pub fn get_localized_control_type(&self) -> &str {
        &self.localized_control_type
    }
    pub fn get_framework_id(&self) -> &str {
        &self.framework_id
    }
    pub fn get_runtime_id(&self) -> &[i32] {
        &self.runtime_id
    }
    pub fn get_automation_id(&self) -> &str {
        &self.automation_id
    }
    pub fn get_handle(&self) -> isize {
        self.handle
    }
    pub fn get_bounding_rect_size(&self) -> i64 {
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
    pub fn get_xpath(&self) -> Option<&str> {
        self.xpath.as_deref()
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

    pub(crate) fn set_context(&mut self, level: usize, z_order: usize) {
        self.level = level;
        self.z_order = z_order;
        self.xpath = None;
    }

    /// Read one cached property bundle. A failed read rejects the observation.
    pub(crate) fn from_cache(element: &UIElement) -> uiautomation::Result<Self> {
        let runtime_id = element.get_runtime_id()?;
        let bounding_rect = element.get_cached_bounding_rectangle()?;
        Ok(Self {
            name: element.get_cached_name()?,
            classname: element.get_cached_classname()?,
            control_type: element.get_cached_control_type()?.to_string(),
            localized_control_type: element.get_cached_localized_control_type()?,
            framework_id: element.get_cached_framework_id()?,
            runtime_id,
            automation_id: element.get_cached_automation_id()?,
            handle: element.get_cached_native_window_handle()?.into(),
            ancestor: None,
            occurrence: None,
            bounding_rect_size: (i64::from(bounding_rect.get_right())
                - i64::from(bounding_rect.get_left()))
                * (i64::from(bounding_rect.get_bottom()) - i64::from(bounding_rect.get_top())),
            bounding_rect,
            level: 0,
            z_order: 0,
            xpath: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn fixture(id: i32, name: &str, control_type: &str) -> Self {
        Self {
            runtime_id: vec![42, id],
            name: name.into(),
            control_type: control_type.into(),
            ..Self::default()
        }
    }

    pub fn get_ui_automation_ui_element(&self) -> Option<UIElement> {
        if !self.identity().is_resolvable() {
            return None;
        }
        debug!(
            "Getting ui element from SaveUIElement with runtime id: {:?}",
            self.runtime_id
        );

        let uia = match get_ui_automation_instance() {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to create UIAutomation instance: {}", e);
                return None;
            }
        };

        // Fast path: O(1) lookup by window handle when available
        if self.handle != 0 {
            let handle = Handle::from(self.handle);
            match uia.element_from_handle(handle) {
                Ok(e) if self.identity().matches_live(&e) => {
                    debug!("Element found by handle: {}", self.handle);
                    return Some(e);
                }
                Ok(_) => return None,
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

        match matcher.find_all() {
            Ok(elements) => {
                let mut matches = elements
                    .into_iter()
                    .filter(|e| self.identity().matches_live(e));
                let e = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
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
            ancestor: None,
            occurrence: None,
            bounding_rect: uiautomation::types::Rect::new(0, 0, 0, 0),
            bounding_rect_size: 0,
            level: 0,
            z_order: 0,
            xpath: None,
        }
    }
}

impl TryFrom<&SaveUIElement> for UIElement {
    type Error = crate::error::UITreeError;

    fn try_from(value: &SaveUIElement) -> Result<Self, Self::Error> {
        value
            .get_ui_automation_ui_element()
            .ok_or(crate::error::UITreeError::UIAutomation(
                "could not resolve UIElement from SaveUIElement".to_string(),
            ))
    }
}
