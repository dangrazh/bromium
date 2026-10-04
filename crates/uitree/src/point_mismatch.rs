//! Failure-only, read-only evidence. Never repairs or certifies membership.
use crate::{ElementIdentity, Observation, capture::PointEvidence};
use std::time::{Duration, Instant};
use uiautomation::{UIAutomation, UIElement, UITreeWalker, types::UIProperty};

struct Budget {
    deadline: Instant,
    remaining: usize,
}
impl Budget {
    fn available(&self) -> bool {
        self.remaining > 0 && Instant::now() < self.deadline
    }
    fn read<T>(&mut self, call: impl FnOnce() -> uiautomation::Result<T>) -> Result<T, String> {
        if !self.available() {
            return Err("Skipped(diagnostic_budget_or_query_deadline)".into());
        }
        self.remaining -= 1;
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

struct Probe<'a> {
    automation: &'a UIAutomation,
    query: Option<u64>,
    excluded_process: Option<u32>,
    budget: Budget,
}
impl Probe<'_> {
    fn allowed(&mut self, element: &UIElement) -> bool {
        self.excluded_process.is_none_or(|excluded| {
            match self.budget.read(|| element.get_process_id()) {
                Ok(pid) if pid != excluded => true,
                other => {
                    log::debug!("point_ancestry_mismatch query={:?} stage=live_sample status=excluded_or_unverified process={other:?}", self.query);
                    false
                }
            }
        })
    }
    fn sample(&mut self, stage: &str, element: &UIElement) {
        if !self.allowed(element) {
            return;
        }
        let before = self.budget.read(|| element.get_runtime_id());
        let handle_before = self.budget.read(|| element.get_native_window_handle());
        let cached = (|| -> Result<UIElement, String> {
            if !self.budget.available() {
                return Err("Skipped(diagnostic_budget_or_query_deadline)".into());
            }
            let cache = crate::capture::cache_request(self.automation)?;
            cache
                .add_property(UIProperty::IsControlElement)
                .map_err(|e| e.to_string())?;
            self.budget.read(|| element.build_updated_cache(&cache))
        })();
        match cached {
            Ok(cached) => {
                let cached_id = self.budget.read(|| cached.get_runtime_id());
                let after = self.budget.read(|| element.get_runtime_id());
                let handle_after = self.budget.read(|| element.get_native_window_handle());
                log::debug!(
                    "point_ancestry_mismatch query={:?} stage={} original_before={:?} cached_runtime_id={:?} original_after={:?} handle_before={:?} cached_handle={:?} handle_after={:?} name={:?} control_type={:?} bounds={:?} class={:?} framework={:?} provider={:?} is_control={:?}",
                    self.query,
                    stage,
                    before,
                    cached_id,
                    after,
                    handle_before,
                    cached.get_cached_native_window_handle(),
                    handle_after,
                    cached.get_cached_name(),
                    cached.get_cached_control_type(),
                    cached.get_cached_bounding_rectangle(),
                    cached.get_cached_classname(),
                    cached.get_cached_framework_id(),
                    cached.get_cached_provider_description(),
                    cached.is_cached_control_element()
                );
            }
            Err(error) => log::debug!(
                "point_ancestry_mismatch query={:?} stage={} original_before={:?} handle_before={:?} metadata_status={}",
                self.query,
                stage,
                before,
                handle_before,
                error
            ),
        }
    }
    fn view(
        &mut self,
        view: &str,
        walker: &UITreeWalker,
        parent: &UIElement,
        expected: &UIElement,
    ) {
        match self.budget.read(|| walker.get_parent(expected)) {
            Ok(actual_parent) => {
                self.sample(&format!("{view}_expected_parent_now"), &actual_parent);
                if self.allowed(&actual_parent) {
                    let same = self
                        .budget
                        .read(|| self.automation.compare_elements(parent, &actual_parent));
                    log::debug!(
                        "point_ancestry_mismatch query={:?} view={} expected_parent_compare={same:?}",
                        self.query,
                        view
                    );
                }
            }
            Err(error) => log::debug!(
                "point_ancestry_mismatch query={:?} view={} expected_parent_status={}",
                self.query,
                view,
                error
            ),
        }
        let mut next = self.budget.next(|| walker.get_first_child(parent));
        let mut visited = 0;
        let mut complete = false;
        loop {
            match next {
                Ok(None) => {
                    complete = true;
                    break;
                }
                Ok(Some(_)) if !self.budget.available() || visited >= 32 => break,
                Ok(Some(child)) => {
                    if self.allowed(&child) {
                        let same = self
                            .budget
                            .read(|| self.automation.compare_elements(&child, expected));
                        log::debug!(
                            "point_ancestry_mismatch query={:?} view={} child={} expected_compare={same:?}",
                            self.query,
                            view,
                            visited
                        );
                        self.sample(&format!("{view}_child_{visited}"), &child);
                    }
                    visited += 1;
                    next = self.budget.next(|| walker.get_next_sibling(&child));
                }
                Err(error) => {
                    log::debug!(
                        "point_ancestry_mismatch query={:?} view={} enumeration_status={}",
                        self.query,
                        view,
                        error
                    );
                    break;
                }
            }
        }
        // Explicitly report bounded/incomplete scans; they are never evidence of absence.
        log::debug!(
            "point_ancestry_mismatch query={:?} view={} visited={} complete={} remaining_calls={}",
            self.query,
            view,
            visited,
            complete,
            self.budget.remaining
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn report(
    automation: &UIAutomation,
    parent: &UIElement,
    expected: &UIElement,
    captured_parent: &Observation,
    captured_child: &Observation,
    path: &[ElementIdentity],
    parent_depth: usize,
    evidence: Option<&PointEvidence>,
    query: Option<u64>,
    deadline: Instant,
    excluded_process: Option<u32>,
    error: &str,
) {
    if !log::log_enabled!(log::Level::Debug) {
        return;
    }
    log::debug!(
        "point_ancestry_mismatch query={query:?} stage=captured error={error:?} parent_depth={parent_depth} parent={:?} expected={:?} ancestry_leaf_to_root={path:?} child_count={:?}",
        captured_parent.properties,
        captured_child.properties,
        captured_parent.children.as_ref().map(Vec::len)
    );
    for (index, child) in captured_parent
        .children
        .iter()
        .flatten()
        .take(128)
        .enumerate()
    {
        log::debug!(
            "point_ancestry_mismatch query={query:?} stage=captured_child child={index} identity={:?} properties={:?}",
            child.properties.identity(),
            child.properties
        );
    }
    if let Some(evidence) = evidence {
        for (index, record) in evidence.records.iter().enumerate() {
            log::debug!(
                "point_ancestry_mismatch query={query:?} stage=capture_reads record={index} {record}"
            );
        }
        log::debug!(
            "point_ancestry_mismatch query={query:?} stage=capture_reads_summary omitted={}",
            evidence.omitted
        );
    }
    // A fresh failure-only budget: the regular point-hit diagnostic budget may
    // have expired on a slow machine. Neither may extend the original deadline.
    let mut probe = Probe {
        automation,
        query,
        excluded_process,
        budget: Budget {
            deadline: deadline.min(Instant::now() + Duration::from_millis(500)),
            remaining: 256,
        },
    };
    log::debug!(
        "point_ancestry_mismatch query={query:?} stage=reprobe note=later_observation_not_original_capture"
    );
    probe.sample("parent_now", parent);
    probe.sample("expected_now", expected);
    if !probe.allowed(parent) || !probe.allowed(expected) {
        return;
    }
    for view in ["control", "raw"] {
        let overall_deadline = probe.budget.deadline;
        let overall_remaining = probe.budget.remaining;
        if view == "control" {
            probe.budget.deadline = overall_deadline.min(
                Instant::now() + overall_deadline.saturating_duration_since(Instant::now()) / 2,
            );
            probe.budget.remaining = overall_remaining / 2;
        }
        let allocated = probe.budget.remaining;
        let walker = probe.budget.read(|| {
            if view == "control" {
                automation.get_control_view_walker()
            } else {
                automation.get_raw_view_walker()
            }
        });
        match walker {
            Ok(walker) => probe.view(view, &walker, parent, expected),
            Err(error) => {
                log::debug!("point_ancestry_mismatch query={query:?} view={view} status={error}")
            }
        }
        probe.budget.remaining = overall_remaining - (allocated - probe.budget.remaining);
        probe.budget.deadline = overall_deadline;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_budget_never_calls_provider_when_expired_or_exhausted() {
        for (deadline, remaining) in [
            (Instant::now(), 256),
            (Instant::now() + Duration::from_secs(1), 0),
        ] {
            let mut budget = Budget {
                deadline,
                remaining,
            };
            assert!(
                budget
                    .read::<()>(|| panic!("provider called past budget"))
                    .unwrap_err()
                    .contains("Skipped")
            );
        }
        let mut budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            remaining: 1,
        };
        budget.read(|| Ok(())).unwrap();
        assert!(
            budget
                .read::<()>(|| panic!("provider called past budget"))
                .is_err()
        );
    }
}
