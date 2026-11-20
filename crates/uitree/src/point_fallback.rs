//! Scoped top-down recovery for coordinate queries whose bottom-up ancestry
//! failed structurally. The top-level window under the point is recaptured
//! through the shared service (the same control-view walk as any region
//! refresh) and the hit element is relocated in that fresh capture.
use super::{NativePoint, in_scope};
use crate::{SaveUIElement, TreeService, UITree, capture::CaptureKind};
use bromium_common::rectangle::is_inside_rectangle;
use std::time::{Duration, Instant};

/// Below this budget a window walk cannot complete on a slow VDI. Failing fast
/// avoids queueing a capture nobody waits for on the single capture worker.
const MIN_BUDGET: Duration = Duration::from_millis(250);

/// What bottom-up discovery learned before it failed.
#[derive(Debug, Clone)]
pub(super) struct Hint {
    /// Properties of the reconciled hit element, read live during discovery.
    pub target: SaveUIElement,
    /// The native top-level window under the point at the start of the query.
    pub native: NativePoint,
}

pub(super) fn scoped(
    service: &TreeService,
    x: i32,
    y: i32,
    title: Option<&str>,
    deadline: Instant,
    hint: &Hint,
) -> Result<Option<(UITree, usize)>, String> {
    if deadline.saturating_duration_since(Instant::now()) < MIN_BUDGET {
        return Err("insufficient time remaining for a scoped window capture".into());
    }
    let region = match service.region_for_handle(hint.native.root) {
        Some(region) => region,
        None => {
            // The window may be newer than the cached desktop membership.
            let tree = service.membership(deadline).map_err(|e| e.reason)?;
            super::cached_region(&tree, hint.native.root)
                .ok_or("top-level window at the point is not in the desktop tree")?
        }
    };
    // Force a fresh walk: the cached coverage is exactly what failed to explain the hit.
    service.invalidate(region, CaptureKind::Subtree);
    let tree = service
        .ensure_region(region, deadline)
        .map_err(|e| e.reason)?;
    let region = super::cached_region(&tree, hint.native.root).unwrap_or(region);
    let index = relocate(&tree, region, x, y, &hint.target)?;
    Ok(in_scope(&tree, index, &hint.native, title).then_some((tree, index)))
}

/// Finds the hit element inside `region`: by provider identity when it has
/// one, otherwise by an exact property match. Either way the candidate's fresh
/// bounds must still contain the point, and the match must be unique.
fn relocate(
    tree: &UITree,
    region: usize,
    x: i32,
    y: i32,
    target: &SaveUIElement,
) -> Result<usize, String> {
    let under_point = |index: usize| {
        tree.is_descendant(index, region)
            && is_inside_rectangle(tree.node(index).1.get_bounding_rectangle(), x, y)
    };
    let runtime_id = target.get_runtime_id();
    let by_identity: Vec<usize> = if runtime_id.is_empty() {
        Vec::new()
    } else {
        tree.indices_for_id(runtime_id)
            .into_iter()
            .filter(|&index| {
                under_point(index) && tree.node(index).1.get_handle() == target.get_handle()
            })
            .collect()
    };
    let candidates: Vec<usize> = match by_identity.as_slice() {
        [index] => return Ok(*index),
        // Providers may repeat runtime IDs; properties must disambiguate.
        [_, _, ..] => by_identity
            .into_iter()
            .filter(|&index| same_properties(tree.node(index).1, target))
            .collect(),
        // Identity-less target, or the provider regenerated its runtime ID.
        [] => tree
            .get_elements()
            .iter()
            .map(|element| element.get_tree_index())
            .filter(|&index| under_point(index) && same_properties(tree.node(index).1, target))
            .collect(),
    };
    match candidates.as_slice() {
        [index] => Ok(*index),
        [] => Err("hit element was not found in the scoped capture".into()),
        _ => Err(format!(
            "hit element matches {} elements in the scoped capture",
            candidates.len()
        )),
    }
}

fn same_properties(candidate: &SaveUIElement, target: &SaveUIElement) -> bool {
    candidate.get_control_type() == target.get_control_type()
        && candidate.get_classname() == target.get_classname()
        && candidate.get_name() == target.get_name()
        && candidate.get_automation_id() == target.get_automation_id()
        && candidate.get_bounding_rectangle() == target.get_bounding_rectangle()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Observation;

    fn node(
        id: i32,
        name: &str,
        kind: &str,
        handle: isize,
        rect: (i32, i32, i32, i32),
        children: Vec<Observation>,
    ) -> Observation {
        Observation {
            properties: SaveUIElement::fixture(id, name, kind)
                .with_handle(handle)
                .with_rectangle(rect.0, rect.1, rect.2, rect.3),
            children: Some(children),
        }
    }
    fn identityless(name: &str, rect: (i32, i32, i32, i32)) -> Observation {
        let mut observation = node(0, name, "Text", 0, rect, vec![]);
        observation.properties = observation.properties.with_runtime_id(vec![]);
        observation
    }
    fn desktop(window_children: Vec<Observation>) -> Observation {
        node(
            1,
            "Desktop",
            "Pane",
            1,
            (0, 0, 1000, 1000),
            vec![node(
                2,
                "Legacy App",
                "Window",
                222,
                (0, 0, 500, 500),
                window_children,
            )],
        )
    }

    /// Serves the live window content for any region request.
    struct WindowCapture(Observation);
    impl crate::capture::Capture for WindowCapture {
        fn capture(
            &mut self,
            request: &crate::capture::CaptureRequest,
            _: &std::sync::atomic::AtomicBool,
        ) -> Result<Observation, String> {
            match request.kind {
                CaptureKind::Children => Ok(desktop(vec![])),
                _ => Ok(self.0.clone()),
            }
        }
    }
    fn live_window() -> Observation {
        node(
            2,
            "Legacy App",
            "Window",
            222,
            (0, 0, 500, 500),
            vec![node(
                3,
                "Toolbar",
                "Pane",
                0,
                (0, 0, 500, 50),
                vec![
                    node(4, "Save", "Button", 0, (0, 0, 40, 40), vec![]),
                    identityless("Status", (100, 0, 200, 40)),
                ],
            )],
        )
    }
    fn service() -> TreeService {
        let tree = UITree::from_observation(desktop(vec![])).unwrap();
        TreeService::with_capture(tree, || WindowCapture(live_window()))
    }
    fn hint(target: SaveUIElement) -> Hint {
        Hint {
            target,
            native: NativePoint {
                root: 222,
                owners: vec![],
                class: "LegacyFrame".into(),
            },
        }
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn relocates_target_by_identity_after_scoped_capture() {
        let target = SaveUIElement::fixture(4, "Save", "Button").with_rectangle(0, 0, 40, 40);
        let (tree, index) = scoped(&service(), 10, 10, None, deadline(), &hint(target))
            .unwrap()
            .unwrap();
        assert_eq!(tree.node(index).1.get_name(), "Save");
        assert!(
            tree.coverage(tree.owning_window(index))
                .is_some_and(|c| c.children_observed),
            "the window was recaptured top-down"
        );
    }

    #[test]
    fn relocates_identityless_target_by_properties() {
        let target = identityless("Status", (100, 0, 200, 40)).properties;
        let (tree, index) = scoped(&service(), 150, 20, None, deadline(), &hint(target))
            .unwrap()
            .unwrap();
        assert_eq!(tree.node(index).1.get_name(), "Status");
    }

    #[test]
    fn target_no_longer_under_point_is_not_substituted() {
        // The Save button exists, but not at this point; Status is under it.
        let target = SaveUIElement::fixture(4, "Save", "Button").with_rectangle(0, 0, 40, 40);
        let error = scoped(&service(), 150, 20, None, deadline(), &hint(target)).unwrap_err();
        assert!(error.contains("not found"), "{error}");
    }

    #[test]
    fn changed_properties_are_not_matched_geometrically() {
        let target = identityless("Ready", (100, 0, 200, 40)).properties;
        let error = scoped(&service(), 150, 20, None, deadline(), &hint(target)).unwrap_err();
        assert!(error.contains("not found"), "{error}");
    }

    #[test]
    fn scope_is_applied_to_fallback_result() {
        let target = SaveUIElement::fixture(4, "Save", "Button").with_rectangle(0, 0, 40, 40);
        assert!(
            scoped(&service(), 10, 10, Some("Excel"), deadline(), &hint(target))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn insufficient_budget_does_not_queue_a_capture() {
        let service = service();
        let revision = service.revision();
        let target = SaveUIElement::fixture(4, "Save", "Button");
        let error = scoped(
            &service,
            10,
            10,
            None,
            Instant::now() + Duration::from_millis(50),
            &hint(target),
        )
        .unwrap_err();
        assert!(error.contains("insufficient"), "{error}");
        assert_eq!(service.revision(), revision);
    }
}
