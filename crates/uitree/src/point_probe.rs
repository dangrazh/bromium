//! Bounded provider discovery for point queries; no desktop subtree traversal.
use crate::{ElementIdentity, Observation, SaveUIElement};
use std::{
    sync::{
        OnceLock,
        mpsc::{self, SyncSender},
    },
    time::Instant,
};

#[derive(Clone, Debug)]
pub(super) struct Discovery {
    pub diagnostic_id: Option<u64>,
    pub dismiss_context: Option<super::menu::DismissContext>,
    pub path: Vec<ElementIdentity>,
    pub window: Option<(SaveUIElement, u32)>,
    /// Immediate children along the hit ancestry; `partial_parents` explicitly
    /// identifies lists supplemented by independently verified popup edges.
    pub observation: Observation,
    pub partial_parents: Vec<ElementIdentity>,
}

struct Job {
    x: i32,
    y: i32,
    deadline: Instant,
    excluded_process: Option<u32>,
    host_hints: Vec<Vec<ElementIdentity>>,
    resolve_xaml_overlays: bool,
    result: SyncSender<Result<Discovery, String>>,
}

pub(super) fn ancestors(
    x: i32,
    y: i32,
    deadline: Instant,
    excluded_process: Option<u32>,
    host_hints: Vec<Vec<ElementIdentity>>,
    resolve_xaml_overlays: bool,
) -> Result<Discovery, String> {
    static QUEUE: OnceLock<SyncSender<Job>> = OnceLock::new();
    if Instant::now() >= deadline {
        return Err("Point discovery deadline expired".into());
    }
    let queue = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Job>(1);
        std::thread::spawn(move || {
            for job in rx {
                if Instant::now() >= job.deadline {
                    continue;
                }
                let result = std::panic::catch_unwind(|| {
                    discover(
                        job.x,
                        job.y,
                        job.deadline,
                        job.excluded_process,
                        &job.host_hints,
                        job.resolve_xaml_overlays,
                    )
                })
                .unwrap_or_else(|_| Err("Point discovery worker panicked".into()));
                let _ = job.result.try_send(result);
            }
        });
        tx
    });
    let (tx, rx) = mpsc::sync_channel(1);
    queue
        .try_send(Job {
            x,
            y,
            deadline,
            excluded_process,
            host_hints,
            resolve_xaml_overlays,
            result: tx,
        })
        .map_err(|_| "Point discovery capacity unavailable")?;
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| {
            "Point discovery deadline expired (provider call may still be running)".to_string()
        })?
}

fn discover(
    x: i32,
    y: i32,
    deadline: Instant,
    excluded_process: Option<u32>,
    host_hints: &[Vec<ElementIdentity>],
    resolve_xaml_overlays: bool,
) -> Result<Discovery, String> {
    let check = || {
        if Instant::now() < deadline {
            Ok(())
        } else {
            Err("Point discovery deadline expired".to_string())
        }
    };
    check()?;
    let automation = bromium_common::get_ui_automation_instance().map_err(|e| e.to_string())?;
    let diagnostic = super::diagnostics::Diagnostics::new(x, y, deadline);
    check()?;
    let mut element = automation
        .element_from_point(uiautomation::types::Point::new(x, y))
        .map_err(|e| e.to_string())?;
    if let Some(excluded) = excluded_process {
        check()?;
        if element.get_process_id().map_err(|e| e.to_string())? == excluded {
            return Err("Point target belongs to excluded process".into());
        }
    }
    diagnostic.element("initial_raw", &element, &automation);
    check()?;
    let walker = automation
        .get_control_view_walker()
        .map_err(|e| e.to_string())?;
    check()?;
    element = walker.normalize(&element).map_err(|e| e.to_string())?;
    diagnostic.element("initial_control_view", &element, &automation);
    diagnostic.focus(&automation, excluded_process);
    check()?;
    if excluded_process == Some(element.get_process_id().map_err(|e| e.to_string())?) {
        return Err("Point target belongs to excluded process".into());
    }
    element = super::menu::reconcile(&automation, element, x, y, deadline)?;
    diagnostic.element("initial_menu_reconciled", &element, &automation);
    element = super::host::reconcile(
        &automation,
        element,
        x,
        y,
        deadline,
        excluded_process,
        host_hints,
    )?;
    diagnostic.element("initial_host_reconciled", &element, &automation);
    let (mut element, dismiss_context) = if resolve_xaml_overlays {
        super::menu::reconcile_dismiss(&automation, element, x, y, deadline)?
    } else {
        (element, None)
    };
    diagnostic.element("initial_xaml_reconciled", &element, &automation);
    let mut path = Vec::new();
    let mut elements = Vec::new();
    for depth in 0..64 {
        check()?;
        let runtime_id = element.get_runtime_id().map_err(|e| e.to_string())?;
        check()?;
        let handle = element
            .get_native_window_handle()
            .map_err(|e| e.to_string())?
            .into();
        let identity = ElementIdentity {
            runtime_id,
            handle,
            ancestor: None,
            occurrence: None,
        };
        if identity.runtime_id.is_empty() {
            return Err("Point target ancestry has missing provider identity".into());
        }
        log::debug!(
            "point_lookup uia_ancestor_depth={} identity={:?}",
            depth,
            identity
        );
        path.push(identity);
        elements.push(element.clone());
        check()?;
        match walker.get_parent(&element) {
            Ok(parent) => element = parent,
            Err(e) if e.code() == 0 => {
                // Only the immediate child of the actual UIA desktop can be
                // introduced without enumerating unrelated desktop siblings.
                check()?;
                let desktop = automation.get_root_element().map_err(|e| e.to_string())?;
                if !path.last().is_some_and(|id| id.matches_live(&desktop)) {
                    return Err("Point ancestry did not reach the UIA desktop".into());
                }
                ElementIdentity::qualify_path(&mut path);
                let window = if elements.len() >= 2 {
                    let started = Instant::now();
                    log::debug!(
                        "point_capture_timing query={:?} stage=window_begin identity={:?}",
                        diagnostic.id,
                        path[path.len() - 2]
                    );
                    let element = &elements[elements.len() - 2];
                    check()?;
                    let process = element.get_process_id().map_err(|e| e.to_string())?;
                    let cached = element
                        .build_updated_cache(&crate::capture::cache_request(&automation)?)
                        .map_err(|e| e.to_string())?;
                    let mut properties =
                        SaveUIElement::from_cache(&cached).map_err(|e| e.to_string())?;
                    properties.qualify(path[path.len() - 2].ancestor.as_deref());
                    if properties.identity() != path[path.len() - 2]
                        || !properties.identity().matches_live(element)
                    {
                        return Err("Point window identity changed during discovery".into());
                    }
                    log::debug!(
                        "point_capture_timing query={:?} stage=window_end elapsed_us={}",
                        diagnostic.id,
                        started.elapsed().as_micros()
                    );
                    Some((properties, process))
                } else {
                    None
                };
                let Some((_, _)) = &window else {
                    return Err("Point target is the desktop, not a window control".into());
                };
                let mut capture = excluded_process.map_or_else(
                    crate::capture::UiaCapture::default,
                    crate::capture::UiaCapture::excluding_process,
                );
                let cancel = std::sync::atomic::AtomicBool::new(false);
                let mut count = 0;
                let mut partial_parents = Vec::new();
                let started = Instant::now();
                log::debug!(
                    "point_capture_timing query={:?} stage=target_begin identity={:?}",
                    diagnostic.id,
                    path[0]
                );
                let target_result = capture.walk(
                    &automation,
                    elements[0].clone(),
                    0,
                    deadline,
                    &cancel,
                    &mut count,
                    path[0].ancestor.as_deref(),
                );
                log::debug!(
                    "point_capture_timing query={:?} stage=target_end elapsed_us={} error={:?}",
                    diagnostic.id,
                    started.elapsed().as_micros(),
                    target_result.as_ref().err()
                );
                let mut observation = target_result?;
                if observation.properties.identity() != path[0] {
                    return Err("Point target changed during property capture".into());
                }
                super::host_probe::report(
                    &automation,
                    &elements[0],
                    &elements[elements.len() - 2],
                    &observation.properties,
                    x,
                    y,
                    diagnostic.id,
                    deadline,
                    excluded_process,
                );
                // Bottom-up spine: enumerate only each ancestor's immediate children.
                // This is also the membership evidence needed for positional locators.
                for i in 1..elements.len() - 1 {
                    check()?;
                    let started = Instant::now();
                    log::debug!(
                        "point_capture_timing query={:?} stage=parent_begin depth={} identity={:?} remaining_ms={}",
                        diagnostic.id,
                        i,
                        path[i],
                        deadline.saturating_duration_since(started).as_millis()
                    );
                    capture.begin_point_evidence();
                    let parent_result = capture.walk(
                        &automation,
                        elements[i].clone(),
                        1,
                        deadline,
                        &cancel,
                        &mut count,
                        path[i].ancestor.as_deref(),
                    );
                    log::debug!(
                        "point_capture_timing query={:?} stage=parent_end depth={} elapsed_us={} children={:?} nodes={} error={:?}",
                        diagnostic.id,
                        i,
                        started.elapsed().as_micros(),
                        parent_result
                            .as_ref()
                            .ok()
                            .and_then(|p| p.children.as_ref().map(Vec::len)),
                        count,
                        parent_result.as_ref().err()
                    );
                    let parent = parent_result?;
                    if parent.properties.identity() != path[i] {
                        return Err("Point ancestor changed during capture".into());
                    }
                    let evidence = capture.take_point_evidence();
                    let popup = observation.properties.get_classname() == "Popup"
                        && observation.properties.get_framework_id() == "XAML"
                        && observation.properties.get_control_type() == "Window";
                    // Upward evidence is checked again after the final hit test.
                    let verified_popup = popup
                        && path[i - 1].is_resolvable()
                        && path[i].matches_provider(
                            &walker
                                .get_parent(&elements[i - 1])
                                .map_err(|e| e.to_string())?,
                        );
                    let partial = verified_popup
                        && parent.children.as_ref().is_some_and(|children| {
                            !children.iter().any(|c| {
                                c.properties.identity() == observation.properties.identity()
                            })
                        });
                    if partial {
                        partial_parents.push(path[i].clone());
                    }
                    observation = join_spine_verified(
                        parent,
                        observation,
                        verified_popup,
                        |parent, child, error| {
                            super::mismatch::report(
                                &automation,
                                &elements[i],
                                &elements[i - 1],
                                parent,
                                child,
                                &path,
                                i,
                                evidence.as_ref(),
                                diagnostic.id,
                                deadline,
                                excluded_process,
                                error,
                            );
                        },
                    )?;
                }
                check()?;
                let hit = automation
                    .element_from_point(uiautomation::types::Point::new(x, y))
                    .map_err(|e| e.to_string())?;
                if let Some(excluded) = excluded_process {
                    check()?;
                    if hit.get_process_id().map_err(|e| e.to_string())? == excluded {
                        return Err("Point verification target belongs to excluded process".into());
                    }
                }
                diagnostic.element("verification_raw", &hit, &automation);
                let hit = walker.normalize(&hit).map_err(|e| e.to_string())?;
                diagnostic.element("verification_control_view", &hit, &automation);
                let hit = super::menu::reconcile(&automation, hit, x, y, deadline)?;
                diagnostic.element("verification_menu_reconciled", &hit, &automation);
                let hit = super::host::reconcile(
                    &automation,
                    hit,
                    x,
                    y,
                    deadline,
                    excluded_process,
                    host_hints,
                )?;
                diagnostic.element("verification_host_reconciled", &hit, &automation);
                let (hit, verified_dismiss_context) = if resolve_xaml_overlays {
                    super::menu::reconcile_dismiss(&automation, hit, x, y, deadline)?
                } else {
                    (hit, None)
                };
                if dismiss_context != verified_dismiss_context {
                    return Err(
                        "XAML dismissal overlay or active menu changed during point capture".into(),
                    );
                }
                diagnostic.element("verification_xaml_reconciled", &hit, &automation);
                if !path[0].matches_live(&hit) {
                    return Err("Point target changed during narrow capture".into());
                }
                // A positive-only popup edge must remain attached to the same
                // live ancestry, not merely share a name or rectangle.
                if !partial_parents.is_empty() {
                    let mut current = hit;
                    for (i, expected) in path.iter().enumerate() {
                        check()?;
                        if !expected.matches_provider(&current) {
                            return Err("Point popup ancestry changed during verification".into());
                        }
                        if i + 1 < path.len() {
                            current = walker.get_parent(&current).map_err(|e| e.to_string())?;
                        }
                    }
                }
                check()?;
                log::debug!(
                    "point_lookup narrow_capture nodes={} ancestry_depth={} target={:?}",
                    count,
                    path.len(),
                    path.first()
                );
                return Ok(Discovery {
                    diagnostic_id: diagnostic.id,
                    dismiss_context,
                    path,
                    window,
                    observation,
                    partial_parents,
                });
            }
            Err(e) => return Err(format!("Point ancestry failed: {e}")),
        }
    }
    Err("Point ancestry depth limit exceeded".into())
}

#[cfg(test)]
fn join_spine(parent: Observation, child: Observation) -> Result<Observation, String> {
    join_spine_reporting(parent, child, |_, _, _| {})
}

#[cfg(test)]
fn join_spine_reporting(
    parent: Observation,
    child: Observation,
    report: impl FnOnce(&Observation, &Observation, &str),
) -> Result<Observation, String> {
    join_spine_verified(parent, child, false, report)
}

fn join_spine_verified(
    mut parent: Observation,
    child: Observation,
    verified_popup: bool,
    report: impl FnOnce(&Observation, &Observation, &str),
) -> Result<Observation, String> {
    let children = parent
        .children
        .as_ref()
        .ok_or("Point parent membership not captured")?;
    let identity = child.properties.identity();
    let matches: Vec<_> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.properties.identity() == identity)
        .map(|(i, _)| i)
        .collect();
    if matches.len() != 1 {
        let error = format!(
            "Point ancestry occurrence is missing or ambiguous; identity={identity:?}; matches={}",
            matches.len()
        );
        report(&parent, &child, &error);
        if matches.is_empty() && verified_popup {
            log::debug!(
                "point_popup_branch parent={:?} child={:?} membership=partial source=verified_point_ancestry",
                parent.properties.identity(),
                identity
            );
            parent.children.as_mut().unwrap().push(child);
            return Ok(parent);
        }
        return Err(error);
    }
    parent.children.as_mut().unwrap()[matches[0]] = child;
    Ok(parent)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: i32, children: Option<Vec<Observation>>) -> Observation {
        Observation {
            properties: SaveUIElement::fixture(id, "Control", "Pane"),
            children,
        }
    }
    #[test]
    fn point_popup_missing_from_enumeration_is_preserved_as_partial() {
        let popup = Observation {
            properties: SaveUIElement::fixture(705, "Popup", "Window"),
            children: Some(vec![node(114, None)]),
        };
        let result = join_spine_verified(
            node(1, Some(vec![node(2, None)])),
            popup,
            true,
            |_, _, _| {},
        );
        assert!(
            result.is_ok(),
            "verified popup must not be lost: {result:?}"
        );
        let parent = result.unwrap();
        assert_eq!(parent.children.as_ref().unwrap().len(), 2);
        assert_eq!(
            parent.children.as_ref().unwrap()[0]
                .properties
                .get_runtime_id(),
            &[42, 2]
        );
        assert_eq!(
            parent.children.as_ref().unwrap()[1]
                .children
                .as_ref()
                .unwrap()[0]
                .properties
                .get_runtime_id(),
            &[42, 114]
        );
    }
    #[test]
    fn popup_repair_rejects_unverified_missing_or_ambiguous_children() {
        assert!(
            join_spine_verified(node(1, Some(vec![])), node(2, None), false, |_, _, _| {}).is_err()
        );
        assert!(
            join_spine_verified(
                node(1, Some(vec![node(2, None), node(2, None)])),
                node(2, None),
                true,
                |_, _, _| {}
            )
            .is_err()
        );
        assert!(join_spine_verified(node(1, None), node(2, None), true, |_, _, _| {}).is_err());
    }
    #[test]
    fn spine_preserves_ambiguous_and_identity_less_siblings_without_descending() {
        let missing = Observation {
            properties: SaveUIElement::default(),
            children: None,
        };
        let parent = node(
            1,
            Some(vec![node(2, None), node(3, None), node(3, None), missing]),
        );
        let spine = join_spine(parent, node(2, Some(vec![node(4, None)]))).unwrap();
        let tree = crate::UITree::from_observation(spine).unwrap();
        assert_eq!(tree.children(0).len(), 4);
        assert_eq!(tree.indices_for_id(&[42, 3]).len(), 2);
        assert!(tree.index_for_id(&[42, 4]).is_some());
    }
    #[test]
    fn spine_does_not_guess_between_ambiguous_hit_occurrences() {
        let error = join_spine(
            node(1, Some(vec![node(2, None), node(2, None)])),
            node(2, None),
        )
        .unwrap_err();
        assert!(error.contains("ambiguous"));
        assert!(error.contains("matches=2"));
        assert!(join_spine(node(1, Some(vec![])), node(2, None)).is_err());
    }
    #[test]
    fn mismatch_report_receives_unmodified_capture_and_expected_occurrence() {
        for count in [0, 2] {
            let mut reported = false;
            let result = join_spine_reporting(
                node(1, Some(vec![node(2, None); count])),
                node(2, None),
                |parent, expected, error| {
                    reported = true;
                    assert_eq!(parent.children.as_ref().unwrap().len(), count);
                    assert_eq!(
                        expected.properties.identity(),
                        node(2, None).properties.identity()
                    );
                    assert!(error.contains(&format!("matches={count}")));
                },
            );
            assert!(result.is_err());
            assert!(reported);
        }
        assert!(
            join_spine_reporting(
                node(1, Some(vec![node(2, None)])),
                node(2, None),
                |_, _, _| panic!("successful join must not probe")
            )
            .is_ok()
        );
    }
    #[test]
    fn expired_point_discovery_does_not_start_provider_work() {
        assert!(
            ancestors(0, 0, Instant::now(), None, vec![], true)
                .unwrap_err()
                .contains("deadline")
        );
    }
}
