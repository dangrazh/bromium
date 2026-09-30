//! DEBUG-only evidence for hit-testing discrepancies. Never selects an element.
use crate::UITree;
use std::{
    fmt::Debug,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use uiautomation::{UIAutomation, UIElement};

pub(super) struct Diagnostics {
    pub id: Option<u64>,
    deadline: Instant,
    x: i32,
    y: i32,
}

impl Diagnostics {
    pub fn new(x: i32, y: i32, query_deadline: Instant) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: log::log_enabled!(log::Level::Debug).then(|| NEXT.fetch_add(1, Ordering::Relaxed)),
            deadline: query_deadline.min(Instant::now() + Duration::from_millis(500)),
            x,
            y,
        }
    }
    fn read<T: Debug>(&self, call: impl FnOnce() -> uiautomation::Result<T>) -> String {
        if self.id.is_none() || Instant::now() >= self.deadline {
            return "Skipped(disabled_or_deadline)".into();
        }
        match call() {
            Ok(value) => format!("{value:?}"),
            Err(error) => format!("Error({error:?})"),
        }
    }
    pub fn element(&self, stage: &str, element: &UIElement, automation: &UIAutomation) {
        let Some(id) = self.id else {
            return;
        };
        let runtime = self.read(|| element.get_runtime_id());
        let sampled = (|| -> Result<UIElement, String> {
            if Instant::now() >= self.deadline {
                return Err("Skipped(deadline)".into());
            }
            let cache = crate::capture::cache_request(automation)?;
            cache
                .add_property(uiautomation::types::UIProperty::IsControlElement)
                .map_err(|e| e.to_string())?;
            if Instant::now() >= self.deadline {
                return Err("Skipped(deadline)".into());
            }
            element
                .build_updated_cache(&cache)
                .map_err(|e| format!("Error({e:?})"))
        })();
        let cached = match sampled {
            Ok(cached) => cached,
            Err(status) => {
                log::debug!(
                    "point_hit_probe query={} x={} y={} stage={} runtime_id={} metadata_status={}",
                    id,
                    self.x,
                    self.y,
                    stage,
                    runtime,
                    status
                );
                return;
            }
        };
        // Cached getters do not make additional live provider property reads.
        log::debug!(
            "point_hit_probe query={} x={} y={} stage={} runtime_id={} metadata_source=updated_cache name={:?} control_type={:?} handle={:?} bounds={:?} is_offscreen={:?} is_control={:?} class={:?} framework={:?} provider={:?}",
            id,
            self.x,
            self.y,
            stage,
            runtime,
            cached.get_cached_name(),
            cached.get_cached_control_type(),
            cached.get_cached_native_window_handle(),
            cached.get_cached_bounding_rectangle(),
            cached.is_cached_offscreen(),
            cached.is_cached_control_element(),
            cached.get_cached_classname(),
            cached.get_cached_framework_id(),
            cached.get_cached_provider_description(),
        );
    }
    pub fn focus(&self, automation: &UIAutomation, excluded_process: Option<u32>) {
        let Some(id) = self.id else {
            return;
        };
        if Instant::now() >= self.deadline {
            log::debug!(
                "point_hit_probe query={} stage=focus status=skipped_deadline",
                id
            );
            return;
        }
        match automation.get_focused_element() {
            Ok(element) => {
                if let Some(excluded) = excluded_process {
                    if Instant::now() >= self.deadline {
                        log::debug!(
                            "point_hit_probe query={} stage=focus status=skipped_deadline",
                            id
                        );
                        return;
                    }
                    match element.get_process_id() {
                        Ok(process) if process != excluded => {}
                        result => {
                            log::debug!(
                                "point_hit_probe query={} stage=focus status=excluded_or_unverified process={:?}",
                                id,
                                result
                            );
                            return;
                        }
                    }
                }
                self.element("focus", &element, automation);
            }
            Err(error) => log::debug!("point_hit_probe query={} stage=focus error={:?}", id, error),
        }
    }
}

fn menu_at_point(props: &crate::SaveUIElement, x: i32, y: i32) -> bool {
    matches!(props.get_control_type(), "Menu" | "MenuItem" | "MenuBar")
        && bromium_common::rectangle::is_inside_rectangle(props.get_bounding_rectangle(), x, y)
}

#[derive(Default)]
struct Scan {
    visited: usize,
    menus: usize,
    unobserved: usize,
    candidates: Vec<usize>,
    truncated: bool,
}

fn scan(tree: &UITree, window: usize, x: i32, y: i32, deadline: Instant) -> Scan {
    let mut result = Scan::default();
    let mut stack = vec![window];
    while !stack.is_empty()
        && result.visited < 4096
        && result.candidates.len() < 16
        && Instant::now() < deadline
    {
        let index = stack.pop().unwrap();
        result.visited += 1;
        if !tree
            .coverage(index)
            .is_some_and(|coverage| coverage.children_observed)
        {
            result.unobserved += 1;
        }
        let props = tree.node(index).1;
        if matches!(props.get_control_type(), "Menu" | "MenuItem" | "MenuBar") {
            result.menus += 1;
        }
        if menu_at_point(props, x, y) {
            result.candidates.push(index);
        }
        let available = 4096usize.saturating_sub(result.visited + stack.len());
        result.truncated |= tree.children(index).len() > available;
        stack.extend(tree.children(index).iter().rev().take(available).copied());
    }
    result.truncated |= !stack.is_empty();
    result
}

/// Uses committed metadata only. Bounds/IsOffscreen are evidence, not proof of occlusion.
pub(super) fn snapshot(tree: &UITree, target: usize, x: i32, y: i32, query: Option<u64>) {
    let Some(query) = query.filter(|_| log::log_enabled!(log::Level::Debug)) else {
        return;
    };
    let window = tree.owning_window(target);
    let result = scan(
        tree,
        window,
        x,
        y,
        Instant::now() + Duration::from_millis(10),
    );
    for &index in &result.candidates {
        let props = tree.node(index).1;
        let mut ancestry = Vec::new();
        let mut parent = index;
        for _ in 0..64 {
            let p = tree.node(parent).1;
            ancestry.push(format!(
                "name={:?} type={:?} runtime_id={:?} handle={}",
                p.get_name(),
                p.get_control_type(),
                p.get_runtime_id(),
                p.get_handle()
            ));
            if parent == window {
                break;
            }
            parent = tree.get_tree().node(parent).parent;
        }
        log::debug!(
            "point_hit_probe query={} stage=cached_menu_candidate x={} y={} revision={} window={} node={} selected={} name={:?} control_type={:?} identity={:?} bounds={:?} cached_is_offscreen={:?} properties_age_ms={:?} children_observed={:?} ancestry_leaf_to_window={:?}",
            query,
            x,
            y,
            tree.revision(),
            window,
            index,
            index == target,
            props.get_name(),
            props.get_control_type(),
            props.identity(),
            props.get_bounding_rectangle(),
            props.diagnostic_offscreen,
            tree.coverage(index)
                .map(|c| c.observed_at.elapsed().as_millis()),
            tree.coverage(index).map(|c| c.children_observed),
            ancestry
        );
    }
    log::debug!(
        "point_hit_probe query={} stage=cached_menus revision={} window={} visited={} menus_seen={} covering_cursor={} unobserved_branches={} truncated={} visibility_note=cached_IsOffscreen_is_not_occlusion_proof",
        query,
        tree.revision(),
        window,
        result.visited,
        result.menus,
        result.candidates.len(),
        result.unobserved,
        result.truncated
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn candidate_scan_is_window_scoped_bounded_and_read_only() {
        let node = |id, kind, children| crate::Observation {
            properties: crate::SaveUIElement::fixture(id, "item", kind)
                .with_rectangle(0, 0, 100, 100),
            children: Some(children),
        };
        let tree = UITree::from_observation(node(
            1,
            "Pane",
            vec![
                node(
                    2,
                    "Window",
                    (10..30).map(|id| node(id, "MenuItem", vec![])).collect(),
                ),
                node(3, "Window", vec![node(4, "MenuItem", vec![])]),
            ],
        ))
        .unwrap();
        let window = tree.index_for_id(&[42, 2]).unwrap();
        let before = tree.get_xml_dom_tree().to_owned();
        let result = scan(
            &tree,
            window,
            50,
            50,
            Instant::now() + Duration::from_secs(1),
        );
        assert_eq!(result.candidates.len(), 16);
        assert!(result.truncated);
        assert!(
            result
                .candidates
                .iter()
                .all(|&id| tree.owning_window(id) == window)
        );
        let expired = scan(&tree, window, 50, 50, Instant::now());
        assert_eq!(expired.visited, 0);
        assert!(expired.truncated);
        assert_eq!(before, tree.get_xml_dom_tree());
    }
    #[test]
    fn candidate_filter_keeps_overlapping_menu_but_not_underlying_group() {
        let props =
            |kind| crate::SaveUIElement::fixture(1, "item", kind).with_rectangle(0, 0, 100, 100);
        assert!(menu_at_point(&props("MenuItem"), 50, 50));
        assert!(menu_at_point(&props("Menu"), 50, 50));
        assert!(!menu_at_point(&props("Group"), 50, 50));
        assert!(!menu_at_point(&props("MenuItem"), 150, 50));
    }
    #[test]
    fn diagnostic_reads_skip_expired_or_disabled_calls_and_preserve_errors() {
        let mut probe = Diagnostics {
            id: Some(1),
            deadline: Instant::now(),
            x: 0,
            y: 0,
        };
        assert!(
            probe
                .read::<bool>(|| panic!("expired probe called provider"))
                .starts_with("Skipped")
        );
        probe.deadline = Instant::now() + Duration::from_secs(1);
        assert!(
            probe
                .read::<bool>(|| Err(uiautomation::Error::new(1, "failed")))
                .contains("failed")
        );
        probe.id = None;
        assert!(
            probe
                .read::<bool>(|| panic!("disabled probe called provider"))
                .starts_with("Skipped")
        );
    }
}
