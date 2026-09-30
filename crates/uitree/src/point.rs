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
#[path = "point_menu.rs"]
mod menu;
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
    let result = resolve_point_with(
        service,
        x,
        y,
        title,
        deadline,
        || native_point(x, y),
        || probe::ancestors(x, y, deadline, service.point_capture_context().1),
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

fn resolve_point_with(
    service: &TreeService,
    _x: i32,
    _y: i32,
    title: Option<&str>,
    deadline: Instant,
    mut native_read: impl FnMut() -> Option<NativePoint>,
    mut ancestry: impl FnMut() -> Result<probe::Discovery, String>,
) -> Result<Option<(UITree, usize)>, StaleTree> {
    if Instant::now() >= deadline {
        return Err(stale(service, title, "Coordinate query deadline expired"));
    }
    let Some(native) = native_read() else {
        return Ok(None);
    };
    let (epoch, _) = service.point_capture_context();
    log::debug!("point_lookup route=narrow_uia_spine native={:?}", native);
    let discovery = ancestry().map_err(|e| stale(service, title, e))?;
    let diagnostic_id = discovery.diagnostic_id;
    if native_read().as_ref() != Some(&native) {
        return Err(stale(
            service,
            title,
            "Native popup/window changed during coordinate query",
        ));
    }
    let (Some(target), Some(desktop), Some((_, process))) = (
        discovery.path.first(),
        discovery.path.last(),
        discovery.window,
    ) else {
        return Err(stale(
            service,
            title,
            "Point ancestry has no desktop/window anchor",
        ));
    };
    let (tree, id) = service
        .publish_point(
            desktop,
            discovery.observation,
            target,
            process,
            epoch,
            deadline,
        )
        .map_err(|e| stale(service, title, e))?;
    if !in_scope(&tree, id, &native, title) {
        return Ok(None);
    }
    diagnostics::snapshot(&tree, id, _x, _y, diagnostic_id);
    Ok(Some((tree, id)))
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
            diagnostic_id: None,
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
            xpath.contains("NodeKey"),
            "partial coverage requires an identity-scoped anchor: {xpath}"
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
        let result = resolve_point_with(
            &service,
            10,
            10,
            None,
            Instant::now() + std::time::Duration::from_secs(2),
            || Some(native.clone()),
            || {
                service.invalidate(window, crate::capture::CaptureKind::Subtree);
                Ok(discovery.clone())
            },
        )
        .unwrap_err();
        assert!(result.reason.contains("invalidated"));
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
}
