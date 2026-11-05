//! Read-only evidence for identity-less MSAA hits. Never supplies tree identity.
use std::time::{Duration, Instant};
use uiautomation::{UIAutomation, UIElement, patterns::UILegacyIAccessiblePattern};
use windows::core::{IUnknown, Interface};

const CHILD_LIMIT: usize = 16;
const CALL_LIMIT: usize = 256;

struct Budget {
    deadline: Instant,
    calls: usize,
}

impl Budget {
    fn available(&self) -> bool {
        self.calls < CALL_LIMIT && Instant::now() < self.deadline
    }

    fn read<T, E: std::fmt::Debug>(
        &mut self,
        call: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, String> {
        if !self.available() {
            return Err("Skipped(deadline_or_call_limit)".into());
        }
        self.calls += 1;
        call().map_err(|e| format!("{e:?}"))
    }
}

struct Evidence {
    // Retain canonical COM identities until comparisons finish, preventing
    // pointer reuse. Equality here is local evidence, not a persistent key.
    object: Result<IUnknown, String>,
    child: Result<i32, String>,
}

fn sample(b: &mut Budget, query: u64, stage: &str, element: &UIElement) -> Evidence {
    let runtime = b.read(|| element.get_runtime_id());
    let name = b.read(|| element.get_name());
    let kind = b.read(|| element.get_control_type());
    let handle = b.read(|| element.get_native_window_handle());
    let bounds = b.read(|| element.get_bounding_rectangle());
    let provider = b.read(|| element.get_provider_description());
    log::debug!(
        "point_msaa_probe query={query} stage={stage} runtime={runtime:?} name={name:?} type={kind:?} handle={handle:?} bounds={bounds:?} provider={provider:?}"
    );
    let pattern = match b.read(|| element.get_pattern::<UILegacyIAccessiblePattern>()) {
        Ok(pattern) => pattern,
        Err(error) => {
            log::debug!(
                "point_msaa_probe query={query} stage={stage} legacy_pattern_error={error}"
            );
            return Evidence {
                object: Err(error.clone()),
                child: Err(error),
            };
        }
    };
    let child = b.read(|| pattern.get_child_id());
    let legacy_name = b.read(|| pattern.get_name());
    let role = b.read(|| pattern.get_role());
    let state = b.read(|| pattern.get_state());
    // SAFETY: the pattern owns a valid COM interface on the existing UIA worker.
    // GetIAccessible and QueryInterface are read-only; no MSAA actions are called.
    let object = b
        .read(|| unsafe { pattern.as_ref().GetIAccessible() })
        .and_then(|accessible| b.read(|| accessible.cast::<IUnknown>()));
    log::debug!(
        "point_msaa_probe query={query} stage={stage} child_id={child:?} legacy_name={legacy_name:?} role={role:?} state={state:?} accessible_status={:?}",
        object.as_ref().map(|_| "available")
    );
    Evidence { object, child }
}

fn compare(query: u64, stage: &str, hit: &Evidence, other: &Evidence) {
    let same_object = hit
        .object
        .as_ref()
        .ok()
        .zip(other.object.as_ref().ok())
        .map(|(a, b)| a == b);
    let same_child = hit
        .child
        .as_ref()
        .ok()
        .zip(other.child.as_ref().ok())
        .map(|(a, b)| a == b);
    log::debug!(
        "point_msaa_probe query={query} stage={stage} same_iunknown={same_object:?} same_child_id={same_child:?} diagnostic_only=true identity_not_proven=true"
    );
}

pub(super) fn probe(
    automation: &UIAutomation,
    element: &UIElement,
    query: Option<u64>,
    ancestry_depth: usize,
    query_deadline: Instant,
) {
    let Some(query) = query.filter(|_| log::log_enabled!(log::Level::Debug)) else {
        return;
    };
    let started = Instant::now();
    let mut b = Budget {
        deadline: query_deadline.min(started + Duration::from_secs(2)),
        calls: 0,
    };
    log::debug!(
        "point_msaa_probe query={query} stage=begin ancestry_depth={ancestry_depth} child_limit={CHILD_LIMIT} call_limit={CALL_LIMIT}"
    );
    let mut inspected = 0;
    let hit = sample(&mut b, query, "hit_before", element);
    let result = (|| -> Result<&str, String> {
        let walker = b.read(|| automation.get_control_view_walker())?;
        let parent = b.read(|| walker.get_parent(element))?;
        let parent_evidence = sample(&mut b, query, "parent", &parent);
        compare(query, "parent_vs_hit", &hit, &parent_evidence);
        let mut next = b.read(|| walker.get_first_child(&parent))?;
        loop {
            if !b.available() {
                return Ok("budget_exhausted");
            }
            let stage = format!("child_{inspected}");
            let child = sample(&mut b, query, &stage, &next);
            compare(query, &stage, &hit, &child);
            let uia_equal = b.read(|| automation.compare_elements(element, &next));
            log::debug!(
                "point_msaa_probe query={query} stage={stage} uia_compare={uia_equal:?} identity_not_proven=true"
            );
            inspected += 1;
            if inspected == CHILD_LIMIT {
                return Ok("child_limit_reached");
            }
            // Code zero is the UIA wrapper's documented end-of-siblings signal.
            match b.read(|| Ok::<_, String>(walker.get_next_sibling(&next)))? {
                Ok(sibling) => next = sibling,
                Err(error) if error.code() == 0 => return Ok("enumeration_complete"),
                Err(error) => return Err(format!("enumeration_error={error:?}")),
            }
        }
    })();
    let after = sample(&mut b, query, "hit_after", element);
    compare(query, "hit_before_vs_after", &hit, &after);
    log::debug!(
        "point_msaa_probe query={query} stage=end result={result:?} inspected={inspected} calls={} elapsed_ms={} note=provider_calls_cannot_be_preempted",
        b.calls,
        started.elapsed().as_millis()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_probe_budget_does_not_call_provider() {
        let mut budget = Budget {
            deadline: Instant::now(),
            calls: 0,
        };
        assert!(
            budget
                .read::<(), ()>(|| panic!("provider must not run"))
                .is_err()
        );
        assert_eq!(budget.calls, 0);
    }

    #[test]
    fn probe_call_limit_is_enforced_even_for_provider_errors() {
        let mut budget = Budget {
            deadline: Instant::now() + Duration::from_secs(10),
            calls: 0,
        };
        for _ in 0..CALL_LIMIT {
            assert!(budget.read::<(), _>(|| Err("provider failure")).is_err());
        }
        assert!(
            budget
                .read::<(), ()>(|| panic!("call limit exceeded"))
                .is_err()
        );
        assert_eq!(budget.calls, CALL_LIMIT);
    }
}
