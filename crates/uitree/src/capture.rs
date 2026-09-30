//! Bounded-scope acquisition. COM objects stay on the capture worker.
use crate::{ElementIdentity, Observation, SaveUIElement, UITree, UITreeError};
#[path = "capture_probe.rs"]
mod probe;
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
    pub path: Vec<ElementIdentity>,
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
    elements: HashMap<ElementIdentity, UIElement>,
    excluded_process: Option<u32>,
    probe: probe::Collisions<UIElement>,
    diagnostic_ancestry: Vec<String>,
}

impl UiaCapture {
    pub(crate) fn excluding_process(process: u32) -> Self {
        Self {
            excluded_process: Some(process),
            ..Self::default()
        }
    }
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
        let id = target.identity();
        if !id.is_resolvable() {
            return Err(
                "Snapshot-only occurrence cannot be re-resolved; refresh its stable parent".into(),
            );
        }
        // Handle-less references may be exposed through multiple host paths.
        // Rewalk their qualified path rather than trusting a provider-only match.
        if id.handle != 0
            && let Some(element) = self.elements.get(&id)
        {
            if id.matches_live(element) {
                return Ok(element.clone());
            }
            self.elements.remove(&id);
        }
        if target.get_handle() != 0
            && let Ok(element) = a.element_from_handle(Handle::from(target.get_handle()))
            && id.matches_live(&element)
        {
            return Ok(element);
        }
        // Native targets retain the owning-window/path fallback if direct HWND
        // resolution is unavailable (for example a popup site bridge).
        let anchor = if id.handle == 0 {
            id.native_anchor()
        } else {
            None
        };
        let path = if let Some(anchor) = anchor {
            let start = request
                .path
                .iter()
                .rposition(|p| p == anchor)
                .ok_or("Qualified occurrence has no native anchor in target path")?;
            &request.path[start..]
        } else {
            &request.path[..]
        };
        let mut element = if let Some(anchor) = anchor {
            let element = a
                .element_from_handle(Handle::from(anchor.handle))
                .map_err(|e| e.to_string())?;
            if !anchor.matches_provider(&element) {
                return Err("Native ancestor identity changed".into());
            }
            element
        } else if request.window_handle != 0 {
            a.element_from_handle(Handle::from(request.window_handle))
                .map_err(|e| e.to_string())?
        } else {
            a.get_root_element().map_err(|e| e.to_string())?
        };
        let mut context = anchor.cloned();
        for (position, expected) in path.iter().enumerate() {
            check(request.deadline, cancel)?;
            if position == 0 && expected.matches_provider(&element) {
                context = expected.child_context();
                continue;
            }
            if expected.handle == 0 && expected.ancestor.as_deref() != context.as_ref() {
                return Err("Target occurrence ancestry changed".into());
            }
            let children = children(a, &element, request.deadline, cancel)?;
            let mut matches = children
                .into_iter()
                .filter(|e| expected.matches_provider(e));
            element = matches
                .next()
                .ok_or("Target path changed; parent reconciliation required")?;
            if matches.next().is_some() {
                return Err("Ambiguous live identity; parent reconciliation required".into());
            }
            context = expected.child_context();
        }
        if !id.matches_provider(&element) {
            return Err("Target identity changed".into());
        }
        Ok(element)
    }
    pub(crate) fn walk(
        &mut self,
        a: &UIAutomation,
        element: UIElement,
        depth: usize,
        deadline: Instant,
        cancel: &AtomicBool,
        count: &mut usize,
        ancestor: Option<&ElementIdentity>,
    ) -> Result<Observation, String> {
        check(deadline, cancel)?;
        *count += 1;
        if *count > 100_000 {
            return Err("Capture node limit exceeded; coverage incomplete".into());
        }
        let original_id = if log::log_enabled!(log::Level::Debug) {
            Some(element.get_runtime_id())
        } else {
            None
        };
        let cached = element
            .build_updated_cache(&cache_request(a)?)
            .map_err(|e| e.to_string())?;
        let cached_id = log::log_enabled!(log::Level::Debug).then(|| cached.get_runtime_id());
        if cached_id
            .as_ref()
            .is_some_and(|id| id.as_ref().map_or(true, |id| id.is_empty()))
        {
            log::debug!(
                "tree_missing_identity ancestry={:?} ancestor_identity={:?} name={:?} control_type={:?} native_handle={:?} class={:?} framework={:?} provider={:?} original_before={:?} cached_runtime_id={:?} original_after={:?}",
                self.diagnostic_ancestry,
                ancestor,
                cached.get_cached_name(),
                cached.get_cached_control_type(),
                cached.get_cached_native_window_handle(),
                cached.get_cached_classname(),
                cached.get_cached_framework_id(),
                cached.get_cached_provider_description(),
                original_id,
                cached_id,
                element.get_runtime_id()
            );
        }
        let mut properties = SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
        properties.qualify(ancestor);
        let context = properties.child_context();
        if log::log_enabled!(log::Level::Debug) {
            self.probe.observe(&properties.identity(), &element);
        }
        // Bound retained COM references. Eviction only affects resolution cost, not identity.
        if self.elements.len() >= 100_000 {
            self.elements.clear();
        }
        if properties.get_handle() != 0 {
            self.elements.insert(properties.identity(), element.clone());
        }
        let observed_children = if depth == 0 {
            None
        } else {
            self.diagnostic_ancestry.push(format!(
                "name={:?} control_type={:?} native_handle={} runtime_id={:?}",
                properties.get_name(),
                properties.get_control_type(),
                properties.get_handle(),
                properties.get_runtime_id()
            ));
            let result = (|| -> Result<Vec<Observation>, String> {
                let mut observations = Vec::new();
                for child in children(a, &element, deadline, cancel)? {
                    // Test ownership before caching properties or descending into the provider.
                    // A failed ownership lookup must not publish excluded contents as valid.
                    if let Some(process) = self.excluded_process
                        && child.get_process_id().map_err(|e| e.to_string())? == process
                    {
                        continue;
                    }
                    observations.push(self.walk(
                        a,
                        child,
                        depth - 1,
                        deadline,
                        cancel,
                        count,
                        context.as_ref(),
                    )?);
                }
                Ok(observations)
            })();
            self.diagnostic_ancestry.pop();
            Some(result?)
        };
        check(deadline, cancel)?;
        Ok(Observation {
            properties,
            children: observed_children,
        })
    }
}

pub(crate) fn cache_request(a: &UIAutomation) -> Result<UICacheRequest, String> {
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
    if log::log_enabled!(log::Level::Debug) {
        cache
            .add_property(UIProperty::IsOffscreen)
            .map_err(|e| e.to_string())?;
        cache
            .add_property(UIProperty::ProviderDescription)
            .map_err(|e| e.to_string())?;
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
        self.probe = probe::Collisions::default();
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
            request
                .target
                .as_ref()
                .and_then(|p| p.identity().ancestor)
                .as_deref(),
        );
        log::debug!(
            "tree_capture kind={:?} nodes={} elapsed_us={} success={}",
            request.kind,
            count,
            started.elapsed().as_micros(),
            result.is_ok()
        );
        if let Some((id, first, second)) = self.probe.take_pair() {
            log::debug!(
                "tree_collision_probe target={:?} window_handle={} shared_identity={:?} sampling=after_capture",
                request.target.as_ref().map(|p| p.get_runtime_id()),
                request.window_handle,
                id
            );
            for (label, element) in [("first", first), ("second", second)] {
                probe::inspect(&a, &element, label, request.deadline, cancel);
            }
        }
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
            .map(|p| vec![p.identity()])
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
            request
                .target
                .as_ref()
                .and_then(|p| p.identity().ancestor)
                .as_deref(),
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
