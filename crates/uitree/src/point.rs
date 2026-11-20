use crate::{StaleTree, TreeService};
use crate::{UIElementInTree, UITree};
use std::time::Instant;
use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::{
        GA_ROOT, GW_OWNER, GetAncestor, GetClassNameW, GetWindow, WindowFromPoint,
    },
};
#[path = "point_diagnostics.rs"]
mod diagnostics;
#[path = "point_fallback.rs"]
mod fallback;
#[path = "point_host.rs"]
mod host;
#[path = "point_host_probe.rs"]
mod host_probe;
#[path = "point_identityless.rs"]
mod identityless;
#[path = "point_menu.rs"]
mod menu;
#[path = "point_mismatch.rs"]
mod mismatch;
#[path = "point_msaa_probe.rs"]
mod msaa_probe;
#[path = "point_probe.rs"]
mod probe;

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativePoint {
    root: isize,
    owners: Vec<isize>,
    class: String,
}
fn native_point(x: i32, y: i32) -> Option<NativePoint> {
    let root = window_at_point(x, y)?;
    let mut hwnd = HWND(root as *mut _);
    let mut buffer = [0u16; 256];
    // SAFETY: valid fixed output buffer; native inspection is read-only.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let class = String::from_utf16_lossy(&buffer[..len.max(0) as usize]);
    let mut owners = Vec::new();
    for _ in 0..16 {
        // SAFETY: GetWindow only queries ownership; handles may disappear safely.
        let Ok(owner) = (unsafe { GetWindow(hwnd, GW_OWNER) }) else {
            break;
        };
        if owner.is_invalid() || owner.0 as isize == root || owners.contains(&(owner.0 as isize)) {
            break;
        }
        owners.push(owner.0 as isize);
        hwnd = owner;
    }
    Some(NativePoint {
        root,
        owners,
        class,
    })
}

pub fn window_at_point(x: i32, y: i32) -> Option<isize> {
    // SAFETY: read-only native hit testing with a by-value screen point.
    let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
    if hwnd.is_invalid() {
        return None;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    (!root.is_invalid()).then_some(root.0 as isize)
}

pub fn element_at_point(tree: &UITree, x: i32, y: i32) -> Option<&UIElementInTree> {
    let handle = window_at_point(x, y)?;
    let window = cached_region(tree, handle)?;
    element_in_region(tree, window, x, y)
}

pub fn cached_region(tree: &UITree, handle: isize) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    let mut matches = tree.get_elements().iter().filter(|e| {
        e.get_tree_index() != tree.root() && e.get_element_props().get_handle() == handle
    });
    let first = matches.next()?.get_tree_index();
    matches.next().is_none().then_some(first)
}

fn stale(service: &TreeService, title: Option<&str>, reason: impl Into<String>) -> StaleTree {
    StaleTree {
        reason: reason.into(),
        scope: title.map(str::to_owned),
        revision: service.revision(),
        coverage: "coordinate mapping incomplete".into(),
    }
}

pub(crate) fn cached_host_hints(tree: &UITree, x: i32, y: i32) -> Vec<Vec<crate::ElementIdentity>> {
    let Some(window) = window_at_point(x, y).and_then(|h| cached_region(tree, h)) else {
        return vec![];
    };
    host::cached_hints(tree, window, x, y)
}

fn in_scope(tree: &UITree, region: usize, native: &NativePoint, title: Option<&str>) -> bool {
    let Some(title) = title else {
        return true;
    };
    if tree
        .node(tree.owning_window(region))
        .1
        .get_name()
        .contains(title)
    {
        return true;
    }
    // Only menus/popups inherit an owner's scope; an owned dialog stays separate.
    let popup = native.class == "#32768"
        || native.class.contains("Popup")
        || tree.node(region).1.get_control_type() == "Menu";
    popup
        && native
            .owners
            .iter()
            .filter_map(|h| cached_region(tree, *h))
            .any(|id| {
                tree.node(tree.owning_window(id))
                    .1
                    .get_name()
                    .contains(title)
            })
}

/// Popup-aware point query. Provider ancestry is discovered on a bounded worker;
/// only owned metadata crosses threads. Every wait shares the caller's deadline.
pub fn resolve_point(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
) -> Result<Option<(UITree, usize)>, StaleTree> {
    resolve_point_with_xaml_overlays(service, x, y, title, deadline, true)
}

/// Logical XAML menu-heading reconciliation can be disabled for provider-hit inspection.
pub fn resolve_point_with_xaml_overlays(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    resolve_xaml_overlays: bool,
) -> Result<Option<(UITree, usize)>, StaleTree> {
    resolve_point_routed(service, x, y, title, deadline, resolve_xaml_overlays, false)
        .map(|found| found.map(|hit| (hit.tree, hit.index)))
}

/// How a coordinate query located its element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointRoute {
    /// Bottom-up: hit test, then the provider's parent chain to the desktop.
    BottomUp,
    /// Bottom-up failed structurally; the top-level window under the point was
    /// recaptured top-down and the hit element relocated in that capture.
    ScopedFallback,
}
impl PointRoute {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BottomUp => "point",
            Self::ScopedFallback => "point_fallback_scoped",
        }
    }
}
impl std::fmt::Display for PointRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resolved coordinate query: the published tree, the element's index and the route used.
#[derive(Debug, Clone)]
pub struct PointHit {
    pub tree: UITree,
    pub index: usize,
    pub route: PointRoute,
}

/// Like [`resolve_point_with_xaml_overlays`], but when the bottom-up ancestry
/// fails structurally (missing provider identity, ancestry not reaching the
/// desktop, conflicting or ambiguous cached ancestry, repeated invalidation),
/// the top-level window under the point is recaptured top-down and the hit
/// element is relocated there. The fallback costs a whole-window walk on the
/// single capture worker, so it is meant for one-shot queries, not hover tracking.
pub fn resolve_point_with_fallback(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    resolve_xaml_overlays: bool,
) -> Result<Option<PointHit>, StaleTree> {
    resolve_point_routed(service, x, y, title, deadline, resolve_xaml_overlays, true)
}

fn resolve_point_routed(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    resolve_xaml_overlays: bool,
    allow_fallback: bool,
) -> Result<Option<PointHit>, StaleTree> {
    let result = resolve_point_or_fallback(
        service,
        x,
        y,
        title,
        deadline,
        allow_fallback,
        || native_point(x, y),
        || {
            probe::ancestors(
                x,
                y,
                deadline,
                service.point_capture_context().1,
                service.point_host_hints(x, y),
                resolve_xaml_overlays,
            )
        },
    );
    match &result {
        Err(error) => log::debug!(
            "point_lookup result=failed x={} y={} scope={:?} error={}",
            x,
            y,
            title,
            error
        ),
        Ok(None) => log::debug!(
            "point_lookup result=not_found_or_outside_scope x={} y={} scope={:?}",
            x,
            y,
            title
        ),
        _ => {}
    }
    result.map_err(|mut error| {
        error.scope = title.map(str::to_owned);
        error
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_point_or_fallback(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    allow_fallback: bool,
    native_read: impl FnMut() -> Option<NativePoint>,
    ancestry: impl FnMut() -> Result<probe::Discovery, probe::DiscoveryError>,
) -> Result<Option<PointHit>, StaleTree> {
    let failure = match resolve_point_attempt(service, x, y, title, deadline, native_read, ancestry)
    {
        Ok(found) => {
            return Ok(found.map(|(tree, index)| PointHit {
                tree,
                index,
                route: PointRoute::BottomUp,
            }));
        }
        Err(failure) => failure,
    };
    let (Some(hint), true) = (failure.fallback, allow_fallback) else {
        return Err(failure.stale);
    };
    let started = Instant::now();
    let result = fallback::scoped(service, x, y, title, deadline, &hint);
    log::info!(
        "point_fallback x={} y={} reason={:?} window_class={:?} target_type={:?} target_name={:?} elapsed_ms={} outcome={}",
        x,
        y,
        failure.stale.reason,
        hint.native.class,
        hint.target.get_control_type(),
        hint.target.get_name(),
        started.elapsed().as_millis(),
        match &result {
            Ok(Some(_)) => "found".to_owned(),
            Ok(None) => "outside_scope".to_owned(),
            Err(error) => format!("failed: {error}"),
        }
    );
    match result {
        Ok(found) => Ok(found.map(|(tree, index)| PointHit {
            tree,
            index,
            route: PointRoute::ScopedFallback,
        })),
        Err(error) => Err(StaleTree {
            reason: format!("{}; scoped fallback failed: {error}", failure.stale.reason),
            revision: service.revision(),
            ..failure.stale
        }),
    }
}

/// A bottom-up failure, carrying what a scoped fallback needs when it applies.
struct PointFailure {
    stale: StaleTree,
    fallback: Option<Box<fallback::Hint>>,
}

#[cfg(test)]
fn resolve_point_with(
    service: &TreeService,
    _x: i32,
    _y: i32,
    title: Option<&str>,
    deadline: Instant,
    native_read: impl FnMut() -> Option<NativePoint>,
    ancestry: impl FnMut() -> Result<probe::Discovery, probe::DiscoveryError>,
) -> Result<Option<(UITree, usize)>, StaleTree> {
    resolve_point_attempt(service, _x, _y, title, deadline, native_read, ancestry)
        .map_err(|f| f.stale)
}

fn resolve_point_attempt(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    mut native_read: impl FnMut() -> Option<NativePoint>,
    mut ancestry: impl FnMut() -> Result<probe::Discovery, probe::DiscoveryError>,
) -> Result<Option<(UITree, usize)>, PointFailure> {
    let fail = |reason: &str| PointFailure {
        stale: stale(service, title, reason),
        fallback: None,
    };
    if Instant::now() >= deadline {
        return Err(fail("Coordinate query deadline expired"));
    }
    let Some(native) = native_read() else {
        return Ok(None);
    };
    let mut expected_target = None;
    for attempt in 1..=3 {
        if Instant::now() >= deadline {
            return Err(fail("Coordinate query deadline expired"));
        }
        if native_read().as_ref() != Some(&native) {
            return Err(fail("Native popup/window changed during coordinate query"));
        }
        let (epoch, _) = service.point_capture_context();
        log::debug!("point_lookup route=narrow_uia_spine native={:?}", native);
        let discovery = ancestry().map_err(|error| {
            let reason = error.reason().to_owned();
            PointFailure {
                stale: stale(service, title, reason),
                fallback: match error {
                    probe::DiscoveryError::Structural {
                        target: Some(target),
                        ..
                    } => Some(Box::new(fallback::Hint {
                        target: *target,
                        native: native.clone(),
                    })),
                    _ => None,
                },
            }
        })?;
        let diagnostic_id = discovery.diagnostic_id;
        if native_read().as_ref() != Some(&native) {
            return Err(fail("Native popup/window changed during coordinate query"));
        }
        let (Some(target), Some(desktop), Some((_, process))) = (
            discovery.path.first(),
            discovery.path.last(),
            discovery.window,
        ) else {
            return Err(fail("Point ancestry has no desktop/window anchor"));
        };
        let identity = (
            target.clone(),
            desktop.clone(),
            process,
            discovery.dismiss_context.clone(),
        );
        if expected_target
            .as_ref()
            .is_some_and(|expected| expected != &identity)
        {
            return Err(fail("Point target changed during invalidation retry"));
        }
        expected_target = Some(identity);
        // Kept for a scoped fallback if publication fails structurally.
        let target_properties = find_properties(&discovery.observation, target).cloned();
        let structural = |reason: String| PointFailure {
            stale: stale(service, title, reason),
            fallback: target_properties.clone().map(|target| {
                Box::new(fallback::Hint {
                    target,
                    native: native.clone(),
                })
            }),
        };
        let (tree, id) = match service.publish_point_partial(
            desktop,
            discovery.observation,
            target,
            process,
            epoch,
            deadline,
            &discovery.partial_parents,
            discovery.compared_snapshot,
        ) {
            Ok(result) => result,
            Err(crate::service::PointPublishError::Invalidated) => {
                let delay = std::time::Duration::from_millis(25 * attempt);
                // A snapshot token cannot pin a target across a new acquisition.
                let retry = !discovery.compared_snapshot
                    && attempt < 3
                    && deadline.saturating_duration_since(Instant::now()) > delay;
                log::debug!(
                    "point_retry attempt={} max_attempts=3 retry={} query={:?} target={:?} reason=invalidated remaining_ms={}",
                    attempt,
                    retry,
                    diagnostic_id,
                    target,
                    deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                );
                if !retry {
                    return Err(structural(
                        crate::service::PointPublishError::Invalidated.to_string(),
                    ));
                }
                std::thread::sleep(delay);
                continue;
            }
            Err(error) => return Err(structural(error.to_string())),
        };
        if !in_scope(&tree, id, &native, title) {
            return Ok(None);
        }
        diagnostics::snapshot(&tree, id, x, y, diagnostic_id);
        return Ok(Some((tree, id)));
    }
    unreachable!("last invalidated attempt returns an error")
}

fn find_properties<'a>(
    observation: &'a crate::Observation,
    identity: &crate::ElementIdentity,
) -> Option<&'a crate::SaveUIElement> {
    if &observation.properties.identity() == identity {
        return Some(&observation.properties);
    }
    observation
        .children
        .iter()
        .flatten()
        .find_map(|child| find_properties(child, identity))
}

fn element_in_region(tree: &UITree, window: usize, x: i32, y: i32) -> Option<&UIElementInTree> {
    tree.get_elements()
        .iter()
        .filter(|e| tree.is_descendant(e.get_tree_index(), window))
        .filter(|e| {
            bromium_common::rectangle::is_inside_rectangle(
                e.get_element_props().get_bounding_rectangle(),
                x,
                y,
            )
        })
        .min_by_key(|e| {
            (
                e.get_element_props().get_bounding_rect_size(),
                std::cmp::Reverse(e.get_element_props().get_level()),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Observation, SaveUIElement};
    fn node(
        id: i32,
        name: &str,
        kind: &str,
        handle: isize,
        children: Vec<Observation>,
    ) -> Observation {
        Observation {
            properties: SaveUIElement::fixture(id, name, kind)
                .with_handle(handle)
                .with_rectangle(0, 0, 100, 100),
            children: Some(children),
        }
    }

    struct NoBroadCapture;
    impl crate::capture::Capture for NoBroadCapture {
        fn capture(
            &mut self,
            _: &crate::capture::CaptureRequest,
            _: &std::sync::atomic::AtomicBool,
        ) -> Result<Observation, String> {
            Err("unrelated Office descendant unavailable".into())
        }
    }
    fn fixture() -> (TreeService, probe::Discovery, NativePoint) {
        let desktop = node(1, "Desktop", "Pane", 1, vec![]);
        let window = node(
            2,
            "Outlook",
            "Window",
            222,
            vec![
                node(
                    3,
                    "Ribbon",
                    "Pane",
                    0,
                    vec![node(4, "Settings", "Button", 0, vec![])],
                ),
                Observation {
                    properties: SaveUIElement::default(),
                    children: None,
                },
                node(-122, "First mail", "DataItem", 0, vec![]),
                node(-122, "Second mail", "DataItem", 0, vec![]),
            ],
        );
        let mut observation = window.clone();
        observation.qualify(None);
        let target = observation.children.as_ref().unwrap()[0]
            .children
            .as_ref()
            .unwrap()[0]
            .properties
            .identity();
        let discovery = probe::Discovery {
            compared_snapshot: false,
            partial_parents: vec![],
            diagnostic_id: None,
            dismiss_context: None,
            path: vec![target, desktop.properties.identity()],
            window: Some((window.properties.clone(), 456)),
            observation,
        };
        let service = TreeService::with_capture(
            UITree::from_observation(node(
                1,
                "Desktop",
                "Pane",
                1,
                vec![node(2, "Outlook", "Window", 222, vec![])],
            ))
            .unwrap(),
            || NoBroadCapture,
        );
        (
            service,
            discovery,
            NativePoint {
                root: 222,
                owners: vec![],
                class: "rctrl_renwnd32".into(),
            },
        )
    }
    #[test]
    fn compared_snapshot_does_not_retry_after_invalidation() {
        let (service, mut discovery, native) = fixture();
        discovery.compared_snapshot = true;
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let mut attempts = 0;
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                attempts += 1;
                service.invalidate(window, crate::capture::CaptureKind::Subtree);
                Ok(discovery.clone())
            },
        );
        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn office_narrow_point_succeeds_while_unrelated_capture_remains_dirty() {
        let (service, discovery, native) = fixture();
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        service.invalidate(window, crate::capture::CaptureKind::Subtree);
        let start = Instant::now();
        let (tree, id) = resolve_point_with(
            &service,
            10,
            10,
            Some("Outlook"),
            start + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || Ok(discovery.clone()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(tree.node(id).1.get_name(), "Settings");
        assert_eq!(tree.indices_for_id(&[42, -122]).len(), 2);
        assert_eq!(
            tree.children(window).len(),
            4,
            "identity-less sibling retained"
        );
        assert!(
            service.ensure(Some("Outlook"), Instant::now()).is_err(),
            "point publication must not certify an unrelated dirty subtree"
        );
        let xpath = tree.get_xpath_for_element(id, false).unwrap();
        assert!(
            !xpath.contains("NodeKey"),
            "reusable candidate must not leak session identity: {xpath}"
        );
        assert!(
            !tree
                .get_reusable_locator(id, false)
                .unwrap()
                .snapshot_unique
        );
        assert_eq!(
            tree.query(&xpath).unwrap()[0].identity(),
            tree.node(id).1.identity()
        );
    }
    #[test]
    fn uncached_window_is_discovered_without_desktop_enumeration() {
        let (_, discovery, native) = fixture();
        let service = TreeService::with_capture(
            UITree::from_observation(node(1, "Desktop", "Pane", 1, vec![])).unwrap(),
            || NoBroadCapture,
        );
        let (tree, id) = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || Ok(discovery.clone()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(tree.node(id).1.get_name(), "Settings");
        assert!(service.snapshot().index_for_id(&[42, 4]).is_some());
    }
    #[test]
    fn disappearing_popup_does_not_return_underlying_element() {
        let (service, discovery, mut native) = fixture();
        let mut reads = 0;
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || {
                reads += 1;
                if reads > 1 {
                    native.root = 999;
                }
                Some(native.clone())
            },
            || Ok(discovery.clone()),
        )
        .unwrap_err();
        assert!(result.reason.contains("changed"));
    }
    #[test]
    fn point_capture_uses_original_deadline_and_does_not_publish_late_results() {
        let (service, discovery, native) = fixture();
        let revision = service.revision();
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_millis(10),
            || Some(native.clone()),
            || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                Ok(discovery.clone())
            },
        )
        .unwrap_err();
        assert!(result.reason.contains("deadline"));
        assert_eq!(service.revision(), revision);
    }
    #[test]
    fn point_capture_rejects_events_during_acquisition() {
        let (service, discovery, native) = fixture();
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let mut attempts = 0;
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                attempts += 1;
                service.invalidate(window, crate::capture::CaptureKind::Subtree);
                Ok(discovery.clone())
            },
        )
        .unwrap_err();
        assert!(result.reason.contains("invalidated"));
        assert_eq!(attempts, 3);
    }
    #[test]
    fn structural_failure_is_not_retried() {
        let (service, _, native) = fixture();
        let mut attempts = 0;
        let error = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                attempts += 1;
                Err("Point ancestry occurrence is missing or ambiguous".into())
            },
        )
        .unwrap_err();
        assert!(error.reason.contains("ambiguous"));
        assert_eq!(attempts, 1);
    }
    #[test]
    fn invalidation_retry_does_not_extend_original_deadline() {
        let (service, discovery, native) = fixture();
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let mut attempts = 0;
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_millis(20),
            || Some(native.clone()),
            || {
                attempts += 1;
                service.invalidate(window, crate::capture::CaptureKind::Subtree);
                Ok(discovery.clone())
            },
        );
        assert!(result.is_err());
        assert!(attempts <= 1, "insufficient budget for the first backoff");
    }
    #[test]
    fn retry_never_switches_target() {
        let (service, discovery, native) = fixture();
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let mut attempts = 0;
        let error = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                attempts += 1;
                let mut next = discovery.clone();
                if attempts == 1 {
                    service.invalidate(window, crate::capture::CaptureKind::Subtree);
                } else {
                    next.path[0] = next.path[1].clone();
                }
                Ok(next)
            },
        )
        .unwrap_err();
        assert!(error.reason.contains("target changed"));
        assert_eq!(attempts, 2);
    }
    #[test]
    fn transient_invalidation_reacquires_narrow_capture() {
        let (service, discovery, native) = fixture();
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let mut attempts = 0;
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                attempts += 1;
                if attempts == 1 {
                    service.invalidate(window, crate::capture::CaptureKind::Subtree);
                }
                Ok(discovery.clone())
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(result.0.node(result.1).1.get_name(), "Settings");
    }
    #[test]
    fn point_scope_is_checked_without_broad_capture() {
        let (service, discovery, native) = fixture();
        assert!(
            resolve_point_with(
                &service,
                10,
                10,
                Some("Excel"),
                Instant::now() + std::time::Duration::from_secs(2),
                || Some(native.clone()),
                || Ok(discovery.clone())
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn popup_hit_ignores_smaller_underlying_control_and_supports_submenu() {
        let menu = node(
            3,
            "Menu",
            "Menu",
            333,
            vec![
                node(4, "New", "MenuItem", 0, vec![]),
                node(
                    5,
                    "Submenu",
                    "Menu",
                    555,
                    vec![node(6, "Text document", "MenuItem", 0, vec![])],
                ),
            ],
        );
        let mut behind = node(7, "Behind", "Button", 0, vec![]);
        behind.properties = behind.properties.with_rectangle(5, 5, 15, 15);
        let tree = UITree::from_observation(node(
            1,
            "Desktop",
            "Pane",
            1,
            vec![node(2, "Explorer", "Window", 222, vec![behind, menu])],
        ))
        .unwrap();
        let popup = cached_region(&tree, 555).unwrap();
        let hit = element_in_region(&tree, popup, 10, 10).unwrap();
        assert!(tree.is_descendant(hit.get_tree_index(), popup));
        assert_ne!(hit.get_element_props().get_name(), "Behind");
        assert_eq!(cached_region(&tree, 0), None);
    }
    #[test]
    fn separate_menu_inherits_scope_but_owned_dialog_does_not() {
        let tree = UITree::from_observation(node(
            1,
            "Desktop",
            "Pane",
            1,
            vec![
                node(2, "Notepad", "Window", 222, vec![]),
                node(3, "Context", "Menu", 333, vec![]),
                node(4, "Save dialog", "Window", 444, vec![]),
            ],
        ))
        .unwrap();
        let native = NativePoint {
            root: 333,
            owners: vec![222],
            class: "#32768".into(),
        };
        assert!(in_scope(
            &tree,
            cached_region(&tree, 333).unwrap(),
            &native,
            Some("Notepad")
        ));
        assert!(!in_scope(
            &tree,
            cached_region(&tree, 333).unwrap(),
            &native,
            Some("Explorer")
        ));
        let native = NativePoint {
            root: 444,
            owners: vec![222],
            class: "#32770".into(),
        };
        assert!(!in_scope(
            &tree,
            cached_region(&tree, 444).unwrap(),
            &native,
            Some("Notepad")
        ));
    }
    #[test]
    fn popup_native_root_can_be_nested_beneath_application_in_uia() {
        let popup = Observation {
            properties: SaveUIElement::fixture(3, "PopupHost", "Pane").with_handle(333),
            children: Some(vec![]),
        };
        let app = Observation {
            properties: SaveUIElement::fixture(2, "Notepad", "Window").with_handle(222),
            children: Some(vec![popup]),
        };
        let tree = UITree::from_observation(Observation {
            properties: SaveUIElement::fixture(1, "Desktop", "Pane"),
            children: Some(vec![app]),
        })
        .unwrap();
        assert_eq!(cached_region(&tree, 333), tree.index_for_id(&[42, 3]));
    }

    #[test]
    fn popup_occurrence_and_point_ancestry_use_the_same_native_context() {
        let make_host = |handle| {
            node(
                3,
                "PopupHost",
                "Pane",
                handle,
                vec![node(
                    4,
                    "Popup",
                    "Window",
                    0,
                    vec![node(5, "Open", "MenuItem", 0, vec![])],
                )],
            )
        };
        let tree = UITree::from_observation(node(
            1,
            "Desktop",
            "Pane",
            1,
            vec![node(
                2,
                "Explorer",
                "Window",
                222,
                vec![make_host(333), make_host(444)],
            )],
        ))
        .unwrap();
        let mut selected = Vec::new();
        for handle in [333, 444] {
            let region = cached_region(&tree, handle).unwrap();
            let hit = element_in_region(&tree, region, 10, 10).unwrap();
            assert_eq!(hit.get_element_props().get_name(), "Open");
            let mut path = vec![
                node(5, "Open", "MenuItem", 0, vec![]).properties.identity(),
                node(4, "Popup", "Window", 0, vec![]).properties.identity(),
                make_host(handle).properties.identity(),
            ];
            crate::ElementIdentity::qualify_path(&mut path);
            assert_eq!(
                tree.index_for_identity(&path[0]),
                Some(hit.get_tree_index())
            );
            selected.push(hit.clone());
        }
        assert_ne!(selected[0], selected[1]);
        let set: std::collections::HashSet<_> = selected.into_iter().collect();
        assert_eq!(set.len(), 2);
    }

    /// Serves the Outlook window from `fixture()` for scoped region requests.
    struct OutlookCapture;
    impl crate::capture::Capture for OutlookCapture {
        fn capture(
            &mut self,
            request: &crate::capture::CaptureRequest,
            _: &std::sync::atomic::AtomicBool,
        ) -> Result<Observation, String> {
            let window = node(
                2,
                "Outlook",
                "Window",
                222,
                vec![node(
                    3,
                    "Ribbon",
                    "Pane",
                    0,
                    vec![node(4, "Settings", "Button", 0, vec![])],
                )],
            );
            Ok(match request.kind {
                crate::capture::CaptureKind::Subtree => window,
                _ => node(1, "Desktop", "Pane", 1, vec![window]),
            })
        }
    }
    fn fallback_service() -> TreeService {
        TreeService::with_capture(
            UITree::from_observation(node(
                1,
                "Desktop",
                "Pane",
                1,
                vec![node(2, "Outlook", "Window", 222, vec![])],
            ))
            .unwrap(),
            || OutlookCapture,
        )
    }
    fn structural_with_target() -> probe::DiscoveryError {
        probe::DiscoveryError::Structural {
            reason: "Point target ancestry has missing provider identity".into(),
            target: Some(Box::new(
                SaveUIElement::fixture(4, "Settings", "Button").with_rectangle(0, 0, 100, 100),
            )),
        }
    }

    #[test]
    fn structural_failure_falls_back_to_scoped_capture() {
        let (_, _, native) = fixture();
        let service = fallback_service();
        let hit = resolve_point_or_fallback(
            &service,
            10,
            10,
            Some("Outlook"),
            Instant::now() + std::time::Duration::from_secs(5),
            true,
            || Some(native.clone()),
            || Err(structural_with_target()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(hit.route, PointRoute::ScopedFallback);
        assert_eq!(hit.tree.node(hit.index).1.get_name(), "Settings");
    }

    #[test]
    fn fallback_is_not_attempted_when_disabled_or_final() {
        let (_, _, native) = fixture();
        let service = fallback_service();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let revision = service.revision();
        let disabled = resolve_point_or_fallback(
            &service,
            10,
            10,
            None,
            deadline,
            false,
            || Some(native.clone()),
            || Err(structural_with_target()),
        )
        .unwrap_err();
        assert!(!disabled.reason.contains("fallback"), "{}", disabled.reason);
        let final_error = resolve_point_or_fallback(
            &service,
            10,
            10,
            None,
            deadline,
            true,
            || Some(native.clone()),
            || {
                Err(probe::DiscoveryError::Final(
                    "Point target belongs to excluded process".into(),
                ))
            },
        )
        .unwrap_err();
        assert!(final_error.reason.contains("excluded"));
        assert!(!final_error.reason.contains("fallback"));
        assert_eq!(service.revision(), revision, "no capture was published");
    }

    #[test]
    fn failed_fallback_keeps_the_original_reason() {
        let (_, _, native) = fixture();
        let service = fallback_service();
        let error = resolve_point_or_fallback(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(5),
            true,
            || Some(native.clone()),
            || {
                Err(probe::DiscoveryError::Structural {
                    reason: "Point ancestry did not reach the UIA desktop".into(),
                    target: Some(Box::new(
                        SaveUIElement::fixture(99, "Gone", "Button").with_rectangle(0, 0, 100, 100),
                    )),
                })
            },
        )
        .unwrap_err();
        assert!(
            error
                .reason
                .starts_with("Point ancestry did not reach the UIA desktop"),
            "{}",
            error.reason
        );
        assert!(error.reason.contains("scoped fallback failed"));
    }

    #[test]
    fn successful_bottom_up_reports_its_route() {
        let (service, discovery, native) = fixture();
        let hit = resolve_point_or_fallback(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            true,
            || Some(native.clone()),
            || Ok(discovery.clone()),
        )
        .unwrap()
        .unwrap();
        assert_eq!(hit.route, PointRoute::BottomUp);
        assert_eq!(hit.tree.node(hit.index).1.get_name(), "Settings");
    }
}
