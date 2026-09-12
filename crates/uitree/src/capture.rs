//! Bounded-scope acquisition. COM objects stay on the capture worker.
use crate::{Observation, SaveUIElement, UITree, UITreeError};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use uiautomation::{
    UIAutomation, UIElement,
    core::UICacheRequest,
    types::{Handle, TreeScope, UIProperty},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CaptureKind {
    Properties,
    Children,
    Subtree,
}

#[derive(Clone, Debug)]
pub struct CaptureRequest {
    pub target: Option<SaveUIElement>,
    /// Path from the owning window to the target, for bounded re-resolution.
    pub path: Vec<Vec<i32>>,
    pub window_handle: isize,
    pub kind: CaptureKind,
    pub deadline: Instant,
}

pub trait Capture {
    fn capture(
        &mut self,
        request: &CaptureRequest,
        cancel: &AtomicBool,
    ) -> Result<Observation, String>;
}

#[derive(Default)]
pub struct UiaCapture {
    automation: Option<UIAutomation>,
    elements: HashMap<Vec<i32>, UIElement>,
}

impl UiaCapture {
    fn automation(&mut self) -> Result<UIAutomation, String> {
        if self.automation.is_none() {
            self.automation =
                Some(bromium_common::get_ui_automation_instance().map_err(|e| e.to_string())?);
        }
        Ok(self.automation.as_ref().unwrap().clone())
    }
    fn resolve(
        &mut self,
        a: &UIAutomation,
        request: &CaptureRequest,
        cancel: &AtomicBool,
    ) -> Result<UIElement, String> {
        let Some(target) = &request.target else {
            return a.get_root_element().map_err(|e| e.to_string());
        };
        let id = target.get_runtime_id();
        if let Some(element) = self.elements.get(id) {
            if element.get_runtime_id().ok().as_deref() == Some(id) {
                return Ok(element.clone());
            }
            self.elements.remove(id);
        }
        if target.get_handle() != 0
            && let Ok(element) = a.element_from_handle(Handle::from(target.get_handle()))
            && element.get_runtime_id().ok().as_deref() == Some(id)
        {
            return Ok(element);
        }
        let mut element = if request.window_handle != 0 {
            a.element_from_handle(Handle::from(request.window_handle))
                .map_err(|e| e.to_string())?
        } else {
            a.get_root_element().map_err(|e| e.to_string())?
        };
        for expected in &request.path {
            check(request.deadline, cancel)?;
            if element.get_runtime_id().ok().as_ref() == Some(expected) {
                continue;
            }
            let children = children(a, &element, request.deadline, cancel)?;
            element = children
                .into_iter()
                .find(|e| e.get_runtime_id().ok().as_ref() == Some(expected))
                .ok_or("Target path changed; parent reconciliation required")?;
        }
        if element.get_runtime_id().ok().as_deref() != Some(id) {
            return Err("Target identity changed".into());
        }
        Ok(element)
    }
    fn walk(
        &mut self,
        a: &UIAutomation,
        element: UIElement,
        depth: usize,
        deadline: Instant,
        cancel: &AtomicBool,
        count: &mut usize,
    ) -> Result<Observation, String> {
        check(deadline, cancel)?;
        *count += 1;
        if *count > 100_000 {
            return Err("Capture node limit exceeded; coverage incomplete".into());
        }
        let cached = element
            .build_updated_cache(&cache_request(a)?)
            .map_err(|e| e.to_string())?;
        let properties = SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
        // Bound retained COM references. Eviction only affects resolution cost, not identity.
        if self.elements.len() >= 100_000 {
            self.elements.clear();
        }
        self.elements
            .insert(properties.get_runtime_id().to_vec(), element.clone());
        let observed_children = if depth == 0 {
            None
        } else {
            let mut observations = Vec::new();
            for child in children(a, &element, deadline, cancel)? {
                observations.push(self.walk(a, child, depth - 1, deadline, cancel, count)?);
            }
            Some(observations)
        };
        check(deadline, cancel)?;
        Ok(Observation {
            properties,
            children: observed_children,
        })
    }
}

fn cache_request(a: &UIAutomation) -> Result<UICacheRequest, String> {
    let cache = a.create_cache_request().map_err(|e| e.to_string())?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(|e| e.to_string())?;
    for property in [
        UIProperty::Name,
        UIProperty::ClassName,
        UIProperty::ControlType,
        UIProperty::LocalizedControlType,
        UIProperty::FrameworkId,
        UIProperty::AutomationId,
        UIProperty::NativeWindowHandle,
        UIProperty::BoundingRectangle,
    ] {
        cache.add_property(property).map_err(|e| e.to_string())?;
    }
    Ok(cache)
}

fn children(
    a: &UIAutomation,
    element: &UIElement,
    deadline: Instant,
    cancel: &AtomicBool,
) -> Result<Vec<UIElement>, String> {
    // Control-view traversal may skip intermediate raw-view elements. Do not use
    // FindAll(Children, control-condition), which has different ancestry semantics.
    let walker = a.get_control_view_walker().map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    let mut next = walker.get_first_child(element);
    loop {
        check(deadline, cancel)?;
        match next {
            Ok(child) => {
                if result.len() == 10_000 {
                    return Err("Sibling limit exceeded".into());
                }
                next = walker.get_next_sibling(&child);
                result.push(child);
            }
            // windows-core 0.61 Type::from_abi maps a successful null interface
            // (no child/sibling) to Error::empty(), whose HRESULT is S_OK.
            // Actual failing HRESULTs, including E_POINTER, remain errors.
            Err(e) if e.code() == 0 => return Ok(result),
            Err(e) => return Err(e.to_string()),
        }
    }
}

pub fn resolve_live(request: &CaptureRequest) -> Result<UIElement, String> {
    let mut capture = UiaCapture::default();
    let a = capture.automation()?;
    capture.resolve(&a, request, &AtomicBool::new(false))
}

fn check(deadline: Instant, cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
        Err("Capture deadline expired".into())
    } else {
        Ok(())
    }
}

impl Capture for UiaCapture {
    fn capture(
        &mut self,
        request: &CaptureRequest,
        cancel: &AtomicBool,
    ) -> Result<Observation, String> {
        let started = Instant::now();
        let a = self.automation()?;
        let root = self.resolve(&a, request, cancel)?;
        let mut count = 0;
        let result = self.walk(
            &a,
            root,
            match request.kind {
                CaptureKind::Properties => 0,
                CaptureKind::Children => 1,
                CaptureKind::Subtree => 256,
            },
            request.deadline,
            cancel,
            &mut count,
        );
        log::debug!(
            "tree_capture kind={:?} nodes={} elapsed_us={} success={}",
            request.kind,
            count,
            started.elapsed().as_micros(),
            result.is_ok()
        );
        result
    }
}

pub(crate) fn capture_legacy(
    root: Option<SaveUIElement>,
    depth: Option<usize>,
    exclude: Option<String>,
    title: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<UITree, UITreeError> {
    let cancel = cancel.unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    let mut capture = UiaCapture::default();
    let request = CaptureRequest {
        window_handle: root.as_ref().map_or(0, |p| p.get_handle()),
        path: root
            .as_ref()
            .map(|p| vec![p.get_runtime_id().to_vec()])
            .unwrap_or_default(),
        target: root,
        kind: CaptureKind::Children,
        deadline: Instant::now() + Duration::from_secs(120),
    };
    let result = (|| {
        let a = capture.automation()?;
        let element = capture.resolve(&a, &request, &cancel)?;
        let mut observation = capture.walk(
            &a,
            element,
            depth.unwrap_or(256),
            request.deadline,
            &cancel,
            &mut 0,
        )?;
        if let Some(children) = &mut observation.children {
            children.retain(|c| {
                exclude
                    .as_ref()
                    .is_none_or(|s| c.properties.get_name() != s)
                    && title
                        .as_ref()
                        .is_none_or(|s| c.properties.get_name().contains(s))
            });
        }
        UITree::from_observation(observation)
    })();
    result.map_err(UITreeError::UIAutomation)
}
