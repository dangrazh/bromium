//! Best-effort DEBUG diagnostics, never used to build or repair the canonical tree.
use super::*;

pub(super) struct Collisions<T> {
    seen: HashMap<ElementIdentity, T>,
    pair: Option<(ElementIdentity, T, T)>,
}
impl<T> Default for Collisions<T> {
    fn default() -> Self {
        Self {
            seen: HashMap::new(),
            pair: None,
        }
    }
}
impl<T: Clone> Collisions<T> {
    pub(super) fn observe(&mut self, id: &ElementIdentity, element: &T) {
        if self.pair.is_some() || id.runtime_id.is_empty() {
            return;
        }
        if let Some(first) = self.seen.get(id) {
            self.pair = Some((id.clone(), first.clone(), element.clone()));
            self.seen.clear();
        } else {
            self.seen.insert(id.clone(), element.clone());
        }
    }
    pub(super) fn take_pair(&mut self) -> Option<(ElementIdentity, T, T)> {
        self.seen.clear();
        self.pair.take()
    }
}

struct Budget<'a> {
    deadline: Instant,
    cancel: &'a AtomicBool,
}
impl Budget<'_> {
    fn read<T: std::fmt::Debug>(&self, call: impl FnOnce() -> uiautomation::Result<T>) -> String {
        if check(self.deadline, self.cancel).is_err() {
            return "Skipped(deadline_or_cancelled)".into();
        }
        match call() {
            Ok(value) => format!("{value:?}"),
            Err(error) => format!("Error({error:?})"),
        }
    }
}

pub(super) fn inspect(
    a: &UIAutomation,
    element: &UIElement,
    label: &str,
    deadline: Instant,
    cancel: &AtomicBool,
) {
    let budget = Budget {
        deadline: deadline.min(Instant::now() + Duration::from_secs(1)),
        cancel,
    };
    let before = budget.read(|| element.get_runtime_id());
    log::debug!(
        "tree_collision_probe occurrence={} stage=original_before runtime_id={} name={} control_type={} native_handle={} class={} framework={} provider={} bounds={}",
        label,
        before,
        budget.read(|| element.get_name()),
        budget.read(|| element.get_control_type()),
        budget.read(|| element.get_native_window_handle()),
        budget.read(|| element.get_classname()),
        budget.read(|| element.get_framework_id()),
        budget.read(|| element.get_provider_description()),
        budget.read(|| element.get_bounding_rectangle())
    );
    if check(budget.deadline, cancel).is_err() {
        log::debug!(
            "tree_collision_probe occurrence={} status=skipped_deadline_or_cancelled",
            label
        );
        return;
    }
    let updated = (|| -> Result<UIElement, String> {
        let cache = cache_request(a)?;
        cache
            .add_property(UIProperty::ProviderDescription)
            .map_err(|e| e.to_string())?;
        check(budget.deadline, cancel)?;
        element
            .build_updated_cache(&cache)
            .map_err(|e| e.to_string())
    })();
    match updated {
        Ok(cached) => {
            log::debug!(
                "tree_collision_probe occurrence={} stage=updated_cache runtime_id={} name={} control_type={} native_handle={} class={} framework={} provider={} bounds={}",
                label,
                budget.read(|| cached.get_runtime_id()),
                budget.read(|| cached.get_cached_name()),
                budget.read(|| cached.get_cached_control_type()),
                budget.read(|| cached.get_cached_native_window_handle()),
                budget.read(|| cached.get_cached_classname()),
                budget.read(|| cached.get_cached_framework_id()),
                budget.read(|| cached.get_cached_provider_description()),
                budget.read(|| cached.get_cached_bounding_rectangle())
            );
        }
        Err(error) => log::debug!(
            "tree_collision_probe occurrence={} stage=updated_cache error={:?}",
            label,
            error
        ),
    }
    log::debug!(
        "tree_collision_probe occurrence={} stage=original_after runtime_id={}",
        label,
        budget.read(|| element.get_runtime_id())
    );
    if check(budget.deadline, cancel).is_err() {
        log::debug!(
            "tree_collision_probe occurrence={} children_status=skipped_deadline_or_cancelled",
            label
        );
        return;
    }
    let walker = match a.get_control_view_walker() {
        Ok(walker) => walker,
        Err(error) => {
            log::debug!(
                "tree_collision_probe occurrence={} children_error={:?}",
                label,
                error
            );
            return;
        }
    };
    if check(budget.deadline, cancel).is_err() {
        return;
    }
    let mut next = walker.get_first_child(element);
    for index in 1..=16 {
        if check(budget.deadline, cancel).is_err() {
            log::debug!(
                "tree_collision_probe occurrence={} children_status=truncated_deadline_or_cancelled",
                label
            );
            return;
        }
        match next {
            Ok(child) => {
                log::debug!(
                    "tree_collision_probe occurrence={} immediate_child={} runtime_id={} name={} control_type={} native_handle={} class={} framework={} provider={} bounds={}",
                    label,
                    index,
                    budget.read(|| child.get_runtime_id()),
                    budget.read(|| child.get_name()),
                    budget.read(|| child.get_control_type()),
                    budget.read(|| child.get_native_window_handle()),
                    budget.read(|| child.get_classname()),
                    budget.read(|| child.get_framework_id()),
                    budget.read(|| child.get_provider_description()),
                    budget.read(|| child.get_bounding_rectangle())
                );
                if index == 16 {
                    log::debug!(
                        "tree_collision_probe occurrence={} children_status=limit_reached limit=16",
                        label
                    );
                    return;
                }
                if check(budget.deadline, cancel).is_err() {
                    log::debug!(
                        "tree_collision_probe occurrence={} children_status=truncated_deadline_or_cancelled",
                        label
                    );
                    return;
                }
                next = walker.get_next_sibling(&child);
            }
            Err(error) if error.code() == 0 => {
                log::debug!(
                    "tree_collision_probe occurrence={} children_status=complete count={}",
                    label,
                    index - 1
                );
                return;
            }
            Err(error) => {
                log::debug!(
                    "tree_collision_probe occurrence={} children_error={:?}",
                    label,
                    error
                );
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retains_first_pair_not_last_overwrite_and_resets_between_captures() {
        let mut collisions = Collisions::default();
        let id = ElementIdentity {
            runtime_id: vec![42, 1],
            handle: 111,
            ancestor: None,
            occurrence: None,
        };
        let alias = ElementIdentity {
            runtime_id: vec![42, 1],
            handle: 222,
            ancestor: None,
            occurrence: None,
        };
        collisions.observe(&id, &"PopupHost");
        collisions.observe(&alias, &"other handle");
        assert!(collisions.pair.is_none());
        collisions.observe(&id, &"unnamed pane");
        collisions.observe(&id, &"third");
        assert_eq!(
            collisions.take_pair(),
            Some((id.clone(), "PopupHost", "unnamed pane"))
        );
        collisions.observe(&id, &"next capture");
        assert_eq!(collisions.take_pair(), None);
    }
    #[test]
    fn budget_skips_provider_calls_after_deadline_or_cancel() {
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now(),
            cancel: &cancel,
        };
        assert!(
            budget
                .read::<i32>(|| panic!("must not call provider"))
                .starts_with("Skipped")
        );
        cancel.store(true, Ordering::Relaxed);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(1),
            cancel: &cancel,
        };
        assert!(
            budget
                .read::<i32>(|| panic!("must not call provider"))
                .starts_with("Skipped")
        );
    }
    #[test]
    fn probe_read_preserves_errors_and_escapes_multiline_data() {
        let cancel = AtomicBool::new(false);
        let budget = Budget {
            deadline: Instant::now() + Duration::from_secs(2),
            cancel: &cancel,
        };
        assert_eq!(
            budget.read(|| Ok("name\nsecond line")),
            "\"name\\nsecond line\""
        );
        let error = budget.read::<i32>(|| Err(uiautomation::Error::new(1, "provider unavailable")));
        assert!(error.starts_with("Error("));
        assert!(error.contains("provider unavailable"));
    }
}
