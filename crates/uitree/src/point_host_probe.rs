//! Read-only DEBUG evidence for native Chromium hosts returned by hit testing.
//! Bounds guide a bounded probe, never selection or proof of occlusion/membership.
use crate::SaveUIElement;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use uiautomation::{
    UIAutomation, UIElement,
    types::{ControlType, UIProperty},
};

fn can_descend(depth: usize, document_depth: Option<usize>) -> bool {
    document_depth.map_or(depth < 8, |root| depth.saturating_sub(root) < 16)
}

fn document_origin(depth: usize, inherited: Option<usize>, chrome_document: bool) -> Option<usize> {
    // Only the first discovered document grants extra depth; nested documents
    // must not reset it. No new time, node or provider-call budget is allocated.
    inherited.or_else(|| (depth <= 8 && chrome_document).then_some(depth))
}

pub(super) fn is_host(props: &SaveUIElement) -> bool {
    host_fields(
        props.get_name(),
        props.get_control_type(),
        props.get_classname(),
    )
}

fn host_fields(name: &str, kind: &str, class: &str) -> bool {
    name.is_empty() && kind == "Pane" && class.starts_with("Chrome_WidgetWin_")
}

struct Budget {
    deadline: Instant,
    calls: usize,
}
impl Budget {
    fn stop_reason(&self) -> Option<&'static str> {
        if Instant::now() >= self.deadline {
            Some("deadline")
        } else if self.calls == 0 {
            Some("call_limit")
        } else {
            None
        }
    }
    fn read<T>(&mut self, call: impl FnOnce() -> uiautomation::Result<T>) -> Result<T, String> {
        if let Some(reason) = self.stop_reason() {
            return Err(format!("Skipped({reason})"));
        }
        self.calls -= 1;
        call().map_err(|e| format!("Error({e:?})"))
    }
    fn next(
        &mut self,
        call: impl FnOnce() -> uiautomation::Result<UIElement>,
    ) -> Result<Option<UIElement>, String> {
        self.read(|| match call() {
            Ok(element) => Ok(Some(element)),
            Err(error) if error.code() == 0 => Ok(None),
            Err(error) => Err(error),
        })
    }
}

/// Run on the existing point worker, using its already discovered window; never
/// enumerate desktop siblings. Errors stay diagnostic and cannot replace the hit.
#[allow(clippy::too_many_arguments)]
pub(super) fn report(
    automation: &UIAutomation,
    hit: &UIElement,
    window: &UIElement,
    props: &SaveUIElement,
    x: i32,
    y: i32,
    query: Option<u64>,
    deadline: Instant,
    excluded_process: Option<u32>,
) {
    let Some(query) = query.filter(|_| log::log_enabled!(log::Level::Debug) && is_host(props))
    else {
        return;
    };
    let started = Instant::now();
    let end = deadline.min(started + Duration::from_millis(250));
    log::debug!(
        "point_host_probe query={query} stage=begin x={x} y={y} selected_identity={:?} max_nodes_per_surface=64 max_wrapper_depth=8 max_document_depth=16 max_calls_per_surface=256 budget_ms={} selection_unchanged=true",
        props.identity(),
        end.saturating_duration_since(started).as_millis()
    );
    // Reserve time for the window's sibling content even if the host stalls.
    for (surface, root, surface_deadline) in [
        (
            "host",
            hit,
            started + end.saturating_duration_since(started) / 2,
        ),
        ("window", window, end),
    ] {
        let mut budget = Budget {
            deadline: surface_deadline,
            calls: 256,
        };
        let mut visited = 0;
        let mut pruned = 0;
        let mut reasons = BTreeSet::new();
        let result = (|| -> Result<(), String> {
            let walker = budget.read(|| automation.get_raw_view_walker())?;
            let cache = crate::capture::cache_request(automation)?;
            cache
                .add_property(UIProperty::IsControlElement)
                .map_err(|e| e.to_string())?;
            let mut stack = vec![(root.clone(), Vec::<usize>::new(), None)];
            while let Some((element, path, inherited_document)) = stack.pop() {
                if visited >= 64 {
                    reasons.insert("node_limit");
                    log::debug!(
                        "point_host_probe query={query} surface={surface} ancestry_child_indices={path:?} stage=truncated reason=node_limit"
                    );
                    break;
                }
                visited += 1;
                // Fail closed: do not sample or descend into excluded/unknown processes.
                if let Some(excluded) = excluded_process {
                    match budget.read(|| element.get_process_id()) {
                        Ok(pid) if pid != excluded => {}
                        other => {
                            log::debug!(
                                "point_host_probe query={query} surface={surface} path={path:?} status=excluded_or_unverified process={other:?}"
                            );
                            reasons.insert("excluded_or_unverified_process");
                            continue;
                        }
                    }
                }
                let runtime = budget.read(|| element.get_runtime_id());
                let cached = budget.read(|| element.build_updated_cache(&cache)).map_err(|error| {
                    log::debug!("point_host_probe query={query} surface={surface} ancestry_child_indices={path:?} runtime_id={runtime:?} metadata_status={error}");
                    error
                })?;
                let bounds = cached.get_cached_bounding_rectangle();
                let covers = bounds
                    .as_ref()
                    .ok()
                    .map(|r| bromium_common::rectangle::is_inside_rectangle(r, x, y));
                let document_depth = document_origin(
                    path.len(),
                    inherited_document,
                    cached.get_cached_control_type().ok() == Some(ControlType::Document)
                        && cached
                            .get_cached_framework_id()
                            .is_ok_and(|framework| framework == "Chrome"),
                );
                if document_depth != inherited_document {
                    log::debug!(
                        "point_host_probe query={query} surface={surface} stage=document_entry ancestry_child_indices={path:?} document_depth={document_depth:?} shared_remaining_calls={} shared_remaining_ms={}",
                        budget.calls,
                        budget
                            .deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis()
                    );
                }
                log::debug!(
                    "point_host_probe query={query} stage=live_node surface={surface} ancestry_child_indices={path:?} runtime_id={runtime:?} name={:?} control_type={:?} automation_id={:?} handle={:?} bounds={bounds:?} covers_point={covers:?} class={:?} framework={:?} provider={:?} is_control={:?} is_offscreen={:?}",
                    cached.get_cached_name(),
                    cached.get_cached_control_type(),
                    cached.get_cached_automation_id(),
                    cached.get_cached_native_window_handle(),
                    cached.get_cached_classname(),
                    cached.get_cached_framework_id(),
                    cached.get_cached_provider_description(),
                    cached.is_cached_control_element(),
                    cached.is_cached_offscreen()
                );
                // Always inspect the root's immediate children. Non-covering child
                // branches are logged but not deepened; this is not an absence proof.
                if !path.is_empty() && covers != Some(true) {
                    pruned += 1;
                    continue;
                }
                if !can_descend(path.len(), document_depth) {
                    let reason = if document_depth.is_some() {
                        "document_depth_limit"
                    } else {
                        "wrapper_depth_limit"
                    };
                    reasons.insert(reason);
                    log::debug!(
                        "point_host_probe query={query} surface={surface} stage=truncated reason={reason} ancestry_child_indices={path:?} document_depth={document_depth:?}"
                    );
                    continue;
                }
                let mut next = budget.next(|| walker.get_first_child(&element)).map_err(|error| {
                    log::debug!("point_host_probe query={query} surface={surface} ancestry_child_indices={path:?} first_child_status={error}");
                    error
                })?;
                let mut children = Vec::new();
                while let Some(child) = next {
                    if visited + stack.len() + children.len() >= 64 {
                        reasons.insert("node_limit");
                        log::debug!(
                            "point_host_probe query={query} surface={surface} stage=truncated reason=node_limit parent_child_indices={path:?} next_child_index={}",
                            children.len()
                        );
                        break;
                    }
                    let mut child_path = path.clone();
                    child_path.push(children.len());
                    children.push((child.clone(), child_path, document_depth));
                    match budget.next(|| walker.get_next_sibling(&child)) {
                        Ok(value) => next = value,
                        Err(error) => {
                            log::debug!(
                                "point_host_probe query={query} surface={surface} parent_child_indices={path:?} next_sibling_after={} status={error}",
                                children.len() - 1
                            );
                            reasons.insert(budget.stop_reason().unwrap_or("provider_error"));
                            break;
                        }
                    }
                }
                stack.extend(children.into_iter().rev());
            }
            Ok(())
        })();
        if result.is_err() {
            reasons.insert(budget.stop_reason().unwrap_or("provider_error"));
        }
        let limited = !reasons.is_empty();
        log::debug!(
            "point_host_probe query={query} stage=surface_end surface={surface} visited={visited} pruned_noncovering_or_unknown={pruned} limited={limited} truncation_reasons={reasons:?} remaining_calls={} status={result:?} note=bounded_raw_view_evidence_not_absence_or_occlusion_proof",
            budget.calls
        );
    }
    log::debug!(
        "point_host_probe query={query} stage=end elapsed_ms={} selection_unchanged=true",
        started.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovered_document_can_be_explored_beyond_native_wrapper_limit() {
        // Captured Teams path: the Chrome RootWebArea itself is at depth eight.
        assert!(can_descend(8, Some(8)));
        assert!(can_descend(9, Some(8)));
        assert!(can_descend(23, Some(8)));
        assert!(!can_descend(24, Some(8)));
        assert!(!can_descend(8, None));
        assert!(can_descend(7, None));
    }
    #[test]
    fn document_extension_is_targeted_and_cannot_reset_in_nested_documents() {
        assert_eq!(document_origin(8, None, true), Some(8));
        assert_eq!(document_origin(8, None, false), None);
        assert_eq!(document_origin(9, None, true), None);
        assert_eq!(document_origin(20, Some(8), true), Some(8));
        assert_eq!(document_origin(20, Some(8), false), Some(8));
        assert!(!can_descend(24, document_origin(24, Some(8), true)));
    }
    #[test]
    fn budget_reports_exact_stop_reason() {
        let mut budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            calls: 0,
        };
        assert_eq!(
            budget.read::<()>(|| panic!("exhausted")).unwrap_err(),
            "Skipped(call_limit)"
        );
        budget.calls = 1;
        budget.deadline = Instant::now();
        assert_eq!(
            budget.read::<()>(|| panic!("expired")).unwrap_err(),
            "Skipped(deadline)"
        );
    }
    #[test]
    fn only_unnamed_chromium_panes_trigger() {
        assert!(host_fields("", "Pane", "Chrome_WidgetWin_0"));
        assert!(!host_fields("Suche", "Pane", "Chrome_WidgetWin_0"));
        assert!(!host_fields("", "ComboBox", "Chrome_WidgetWin_0"));
        assert!(!host_fields("", "Pane", "Other"));
    }
    #[test]
    fn budget_never_calls_after_deadline_or_call_limit() {
        let mut budget = Budget {
            deadline: Instant::now(),
            calls: 1,
        };
        assert!(budget.read::<()>(|| panic!("expired")).is_err());
        budget.deadline = Instant::now() + Duration::from_secs(1);
        assert!(budget.read(|| Ok(())).is_ok());
        assert!(budget.read::<()>(|| panic!("exhausted")).is_err());
    }
    #[test]
    fn enumeration_distinguishes_end_from_failure() {
        let mut budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            calls: 2,
        };
        assert!(
            budget
                .next(|| Err(uiautomation::Error::new(0, "end")))
                .unwrap()
                .is_none()
        );
        assert!(
            budget
                .next(|| Err(uiautomation::Error::new(1, "failure")))
                .unwrap_err()
                .contains("failure")
        );
    }
}
