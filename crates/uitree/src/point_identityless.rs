//! Query-local matching only. No live reference or sibling index becomes identity.
use crate::{ElementIdentity, Observation, SaveUIElement};
use std::time::Instant;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

const LIMIT: usize = 256;

fn unique_match<T>(
    items: &[T],
    mut compare: impl FnMut(&T) -> Result<bool, String>,
) -> Result<usize, String> {
    let mut found = None;
    for (index, item) in items.iter().enumerate() {
        if compare(item)? {
            if found.is_some() {
                return Err("Identity-less point comparison is ambiguous".into());
            }
            found = Some(index);
        }
    }
    found.ok_or_else(|| "Identity-less point comparison has no match".into())
}

fn check(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("Identity-less point deadline expired".into())
    } else {
        Ok(())
    }
}

fn verify_membership<T>(
    old: &[T],
    new: &[T],
    mut same: impl FnMut(&T, &T) -> Result<bool, String>,
) -> Result<(), String> {
    if old.len() != new.len() {
        return Err("Identity-less point membership changed".into());
    }
    for (old, new) in old.iter().zip(new) {
        if !same(old, new)? {
            return Err("Identity-less point sibling changed during verification".into());
        }
    }
    Ok(())
}

fn same_metadata(old: &SaveUIElement, new: &SaveUIElement) -> bool {
    old.get_runtime_id() == new.get_runtime_id()
        && old.get_handle() == new.get_handle()
        && old.get_name() == new.get_name()
        && old.get_control_type() == new.get_control_type()
        && old.get_automation_id() == new.get_automation_id()
        && old.get_classname() == new.get_classname()
        && old.get_framework_id() == new.get_framework_id()
        && old.get_bounding_rectangle() == new.get_bounding_rectangle()
}

fn children(
    walker: &UITreeWalker,
    parent: &UIElement,
    deadline: Instant,
) -> Result<Vec<UIElement>, String> {
    check(deadline)?;
    let mut next = walker.get_first_child(parent);
    let mut result = Vec::new();
    loop {
        check(deadline)?;
        match next {
            Err(error) if error.code() == 0 => return Ok(result),
            Err(error) => return Err(error.to_string()),
            Ok(child) => {
                if result.len() == LIMIT {
                    return Err("Identity-less point sibling limit exceeded".into());
                }
                next = walker.get_next_sibling(&child);
                result.push(child);
            }
        }
    }
}

pub(super) struct Capture {
    pub parent: Observation,
    pub target: Observation,
    siblings: Vec<UIElement>,
    selected: usize,
}

impl Capture {
    pub fn acquire(
        a: &UIAutomation,
        target: &UIElement,
        parent: &UIElement,
        parent_id: &ElementIdentity,
        deadline: Instant,
    ) -> Result<Self, String> {
        check(deadline)?;
        if !parent_id.is_resolvable() || !parent_id.matches_live(parent) {
            return Err("Identity-less point requires an identifiable immediate parent".into());
        }
        let walker = a.get_control_view_walker().map_err(|e| e.to_string())?;
        let siblings = children(&walker, parent, deadline)?;
        let selected = unique_match(&siblings, |child| {
            check(deadline)?;
            a.compare_elements(target, child).map_err(|e| e.to_string())
        })?;
        let cache = crate::capture::cache_request(a)?;
        let mut observations = Vec::new();
        for sibling in &siblings {
            check(deadline)?;
            let cached = sibling
                .build_updated_cache(&cache)
                .map_err(|e| e.to_string())?;
            let mut properties = SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
            properties.qualify(Some(parent_id));
            observations.push(Observation {
                properties,
                children: None,
            });
        }
        let props = &mut observations[selected].properties;
        if !props.get_runtime_id().is_empty() || props.get_handle() != 0 {
            return Err("Identity-less point identity changed during capture".into());
        }
        props.mark_snapshot_only();
        let target = observations[selected].clone();
        check(deadline)?;
        let cached = parent
            .build_updated_cache(&cache)
            .map_err(|e| e.to_string())?;
        let mut properties = SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
        properties.qualify(parent_id.ancestor.as_deref());
        if properties.identity() != *parent_id {
            return Err("Identity-less point parent changed".into());
        }
        Ok(Self {
            parent: Observation {
                properties,
                children: Some(observations),
            },
            target,
            siblings,
            selected,
        })
    }

    pub fn verify(
        &self,
        a: &UIAutomation,
        hit: &UIElement,
        parent: &UIElement,
        deadline: Instant,
    ) -> Result<(), String> {
        check(deadline)?;
        let walker = a.get_control_view_walker().map_err(|e| e.to_string())?;
        let live_parent = walker.get_parent(hit).map_err(|e| e.to_string())?;
        let expected = self.parent.properties.identity();
        if !expected.matches_live(&live_parent) || !expected.matches_live(parent) {
            return Err("Identity-less point parent changed during verification".into());
        }
        let fresh = children(&walker, &live_parent, deadline)?;
        if fresh.len() != self.siblings.len() {
            return Err("Identity-less point membership changed".into());
        }
        let selected = unique_match(&fresh, |child| {
            check(deadline)?;
            a.compare_elements(hit, child).map_err(|e| e.to_string())
        })?;
        if selected != self.selected {
            return Err("Identity-less point target changed".into());
        }
        // Order only checks capture consistency; live comparison proves each edge.
        verify_membership(&self.siblings, &fresh, |old, new| {
            check(deadline)?;
            a.compare_elements(old, new).map_err(|e| e.to_string())
        })?;
        let cache = crate::capture::cache_request(a)?;
        for (old, new) in self.parent.children.as_ref().unwrap().iter().zip(&fresh) {
            check(deadline)?;
            let cached = new.build_updated_cache(&cache).map_err(|e| e.to_string())?;
            let properties = SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
            if !same_metadata(&old.properties, &properties) {
                return Err(
                    "Identity-less point sibling metadata changed during verification".into(),
                );
            }
        }
        check(deadline)?;
        if !a
            .compare_elements(&self.siblings[self.selected], hit)
            .map_err(|e| e.to_string())?
        {
            return Err("Identity-less point hit changed during verification".into());
        }
        check(deadline)?;
        log::debug!(
            "point_identityless result=verified_snapshot_only siblings={} target={:?}",
            fresh.len(),
            self.target.properties.identity()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn membership_rejects_removal_reordering_replacement_and_errors() {
        for new in [vec![1, 3], vec![2, 1, 3], vec![1, 4, 3]] {
            assert!(verify_membership(&[1, 2, 3], &new, |a, b| Ok(a == b)).is_err());
        }
        assert!(verify_membership(&[1, 2, 3], &[1, 2, 3], |a, b| Ok(a == b)).is_ok());
        assert!(verify_membership(&[1], &[1], |_, _| Err("provider error".into())).is_err());
    }
    #[test]
    fn msaa_trace_matches_static_data_without_runtime_ids() {
        assert_eq!(
            unique_match(&[false, true, false], |same| Ok(*same)).unwrap(),
            1
        );
    }
    #[test]
    fn comparison_never_guesses_on_missing_ambiguous_or_failed_evidence() {
        assert!(unique_match(&[false, false], |same| Ok(*same)).is_err());
        assert!(unique_match(&[true, true], |same| Ok(*same)).is_err());
        assert!(
            unique_match(&[true, false], |same| if *same {
                Ok(true)
            } else {
                Err("provider failed".into())
            })
            .is_err()
        );
        assert!(check(Instant::now()).is_err());
    }
}
