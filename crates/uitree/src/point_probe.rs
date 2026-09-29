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
    pub path: Vec<ElementIdentity>,
    pub window: Option<(SaveUIElement, u32)>,
    /// Complete immediate children along the hit ancestry, never unrelated subtrees.
    pub observation: Observation,
}

struct Job {
    x: i32,
    y: i32,
    deadline: Instant,
    excluded_process: Option<u32>,
    result: SyncSender<Result<Discovery, String>>,
}

pub(super) fn ancestors(
    x: i32,
    y: i32,
    deadline: Instant,
    excluded_process: Option<u32>,
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
                    discover(job.x, job.y, job.deadline, job.excluded_process)
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
    check()?;
    let mut element = automation
        .element_from_point(uiautomation::types::Point::new(x, y))
        .map_err(|e| e.to_string())?;
    check()?;
    let walker = automation
        .get_control_view_walker()
        .map_err(|e| e.to_string())?;
    check()?;
    element = walker.normalize(&element).map_err(|e| e.to_string())?;
    check()?;
    if excluded_process == Some(element.get_process_id().map_err(|e| e.to_string())?) {
        return Err("Point target belongs to excluded process".into());
    }
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
                let mut observation = capture.walk(
                    &automation,
                    elements[0].clone(),
                    0,
                    deadline,
                    &cancel,
                    &mut count,
                    path[0].ancestor.as_deref(),
                )?;
                if observation.properties.identity() != path[0] {
                    return Err("Point target changed during property capture".into());
                }
                // Bottom-up spine: enumerate only each ancestor's immediate children.
                // This is also the membership evidence needed for positional locators.
                for i in 1..elements.len() - 1 {
                    check()?;
                    let parent = capture.walk(
                        &automation,
                        elements[i].clone(),
                        1,
                        deadline,
                        &cancel,
                        &mut count,
                        path[i].ancestor.as_deref(),
                    )?;
                    if parent.properties.identity() != path[i] {
                        return Err("Point ancestor changed during capture".into());
                    }
                    observation = join_spine(parent, observation)?;
                }
                check()?;
                let hit = automation
                    .element_from_point(uiautomation::types::Point::new(x, y))
                    .map_err(|e| e.to_string())?;
                let hit = walker.normalize(&hit).map_err(|e| e.to_string())?;
                if !path[0].matches_live(&hit) {
                    return Err("Point target changed during narrow capture".into());
                }
                check()?;
                log::debug!(
                    "point_lookup narrow_capture nodes={} ancestry_depth={} target={:?}",
                    count,
                    path.len(),
                    path.first()
                );
                return Ok(Discovery {
                    path,
                    window,
                    observation,
                });
            }
            Err(e) => return Err(format!("Point ancestry failed: {e}")),
        }
    }
    Err("Point ancestry depth limit exceeded".into())
}

fn join_spine(mut parent: Observation, child: Observation) -> Result<Observation, String> {
    let children = parent
        .children
        .as_mut()
        .ok_or("Point parent membership not captured")?;
    let identity = child.properties.identity();
    let matches: Vec<_> = children
        .iter()
        .enumerate()
        .filter(|(_, c)| c.properties.identity() == identity)
        .map(|(i, _)| i)
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "Point ancestry occurrence is missing or ambiguous; identity={identity:?}; matches={}",
            matches.len()
        ));
    }
    children[matches[0]] = child;
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
    fn expired_point_discovery_does_not_start_provider_work() {
        assert!(
            ancestors(0, 0, Instant::now(), None)
                .unwrap_err()
                .contains("deadline")
        );
    }
}
