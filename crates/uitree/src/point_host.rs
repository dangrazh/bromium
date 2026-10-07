//! Focused reconciliation of empty Teams native hosts with sibling WebView controls.
//! Cached paths are hints only. Never selects by cached position/name/rectangle.
use crate::{ElementIdentity, UITree};
use std::time::{Duration, Instant};
use uiautomation::{
    UIAutomation, UIElement, UITreeWalker,
    core::UICacheRequest,
    types::{ControlType, UIProperty},
};

const NODES: usize = 256;
const DEPTH: usize = 64;

fn semantic(kind: &str) -> bool {
    matches!(
        kind,
        "ComboBox"
            | "Edit"
            | "Button"
            | "CheckBox"
            | "RadioButton"
            | "Hyperlink"
            | "MenuItem"
            | "ListItem"
            | "TreeItem"
            | "TabItem"
            | "Slider"
            | "Spinner"
    )
}

pub(super) fn cached_hints(
    tree: &UITree,
    window: usize,
    x: i32,
    y: i32,
) -> Vec<Vec<ElementIdentity>> {
    if tree.node(window).1.get_classname() != "TeamsWebView" {
        return vec![];
    }
    let end = Instant::now() + Duration::from_millis(10);
    let mut result = Vec::new();
    for element in tree.get_elements().iter().take(4096) {
        if Instant::now() >= end || result.len() >= 8 {
            break;
        }
        let p = element.get_element_props();
        let id = element.get_tree_index();
        if p.get_framework_id() != "Chrome"
            || !semantic(p.get_control_type())
            || !p.identity().is_resolvable()
            || !tree.is_descendant(id, window)
            || !bromium_common::rectangle::is_inside_rectangle(p.get_bounding_rectangle(), x, y)
        {
            continue;
        }
        let mut path = Vec::new();
        let mut node = id;
        for _ in 0..DEPTH {
            path.push(tree.node(node).1.identity());
            if node == window {
                path.reverse();
                result.push(path);
                break;
            }
            node = tree.get_tree().node(node).parent;
        }
    }
    result
}

#[derive(Clone)]
struct Info {
    identity: ElementIdentity,
    host: bool,
    owner: bool,
    window: bool,
    document: bool,
    semantic: bool,
    contains: bool,
    visible: bool,
}
trait Source {
    type Node: Clone;
    fn info(&mut self, node: &Self::Node) -> Result<Info, String>;
    fn parent(&mut self, node: &Self::Node) -> Result<Option<Self::Node>, String>;
    fn first(&mut self, node: &Self::Node) -> Result<Option<Self::Node>, String>;
    fn next(&mut self, node: &Self::Node) -> Result<Option<Self::Node>, String>;
}

fn provider_same(a: &ElementIdentity, b: &ElementIdentity) -> bool {
    !a.runtime_id.is_empty() && a.runtime_id == b.runtime_id && a.handle == b.handle
}

fn select<S: Source>(
    source: &mut S,
    raw: S::Node,
    hints: &[Vec<ElementIdentity>],
) -> Result<S::Node, String> {
    let hit = source.info(&raw)?;
    if !hit.host || !hit.contains || !hit.visible {
        return Ok(raw);
    }
    // This path is for empty native shims, not actual populated containers.
    if source.first(&raw)?.is_some() {
        return Ok(raw);
    }
    let mut owner = raw.clone();
    let mut owner_info = hit.clone();
    for depth in 0..DEPTH {
        if owner_info.window {
            break;
        }
        let Some(parent) = source.parent(&owner)? else {
            return Ok(raw);
        };
        owner = parent;
        owner_info = source.info(&owner)?;
        if depth + 1 == DEPTH {
            return Err("Host point ancestry limit exceeded".into());
        }
    }
    if !owner_info.owner || !owner_info.visible || !owner_info.contains {
        return Ok(raw);
    }
    let mut remaining = NODES;
    // Hints only prioritize branches. Every competing point-covering branch is
    // still checked live, including controls added since the cached observation.
    let hints: Vec<_> = hints
        .iter()
        .take(8)
        .filter(|p| {
            p.first()
                .is_some_and(|id| provider_same(id, &owner_info.identity))
        })
        .collect();
    let selected = walk(
        source,
        owner.clone(),
        owner_info.clone(),
        false,
        0,
        &mut remaining,
        &hints,
    )?;
    let Some((selected, selected_info)) = selected else {
        return Ok(raw);
    };
    // Independently confirm the upward control-view ancestry, including document
    // and owner, after the downward search. Snapshot positions are never evidence.
    let mut node = selected.clone();
    let mut document = false;
    let mut verified_path = Vec::new();
    for depth in 0..DEPTH {
        let current = source.info(&node)?;
        if depth == 0
            && (!provider_same(&current.identity, &selected_info.identity)
                || !current.contains
                || !current.visible
                || !current.semantic)
        {
            return Err("Host point candidate changed during verification".into());
        }
        if !current.identity.is_resolvable() {
            return Err("Host point ancestry has missing identity".into());
        }
        document |= current.document;
        verified_path.push(current.identity.clone());
        if current.window {
            if !document
                || !provider_same(&current.identity, &owner_info.identity)
                || !current.owner
            {
                return Err("Host point candidate changed window or document ancestry".into());
            }
            ElementIdentity::qualify_path(&mut verified_path);
            if verified_path[0] != selected_info.identity {
                return Err("Host point candidate occurrence ancestry changed".into());
            }
            log::debug!(
                "point_host_reconcile result=selected original={:?} target={:?} visited={}",
                hit.identity,
                selected_info.identity,
                NODES - remaining
            );
            return Ok(selected);
        }
        node = source
            .parent(&node)?
            .ok_or("Host point candidate detached during verification")?;
    }
    Err("Host point verification ancestry limit exceeded".into())
}

#[allow(clippy::too_many_arguments)]
fn walk<S: Source>(
    source: &mut S,
    node: S::Node,
    info: Info,
    in_document: bool,
    depth: usize,
    remaining: &mut usize,
    hints: &[&Vec<ElementIdentity>],
) -> Result<Option<(S::Node, Info)>, String> {
    if *remaining == 0 || depth >= DEPTH {
        return Err("Host point search node/depth limit exceeded".into());
    }
    *remaining -= 1;
    if !info.contains || !info.visible {
        return Ok(None);
    }
    if depth > 0 && info.window {
        return Ok(None);
    }
    if !info.identity.is_resolvable() {
        return Err("Host point search has missing identity".into());
    }
    let in_document = in_document || info.document;
    let mut result = None;
    let mut child = source.first(&node)?;
    let mut deferred = Vec::new();
    let mut siblings = 0;
    // Stream siblings: immediately descend into a covering branch instead of
    // reserving the remaining node budget for queued off-point siblings.
    while let Some(current) = child {
        siblings += 1;
        if siblings > NODES {
            return Err("Host point sibling limit exceeded".into());
        }
        let mut current_info = source.info(&current)?;
        current_info
            .identity
            .qualify(info.identity.child_context().as_ref());
        let preferred = hints
            .iter()
            .any(|path| path.contains(&current_info.identity));
        if !hints.is_empty() && !preferred && current_info.contains && current_info.visible {
            deferred.push((current.clone(), current_info));
            child = source.next(&current)?;
            continue;
        }
        if let Some(candidate) = walk(
            source,
            current.clone(),
            current_info,
            in_document,
            depth + 1,
            remaining,
            hints,
        )? {
            if result.is_some() {
                return Err("Ambiguous host point candidates in overlapping branches".into());
            }
            result = Some(candidate);
        }
        child = source.next(&current)?;
    }
    for (current, current_info) in deferred {
        if let Some(candidate) = walk(
            source,
            current,
            current_info,
            in_document,
            depth + 1,
            remaining,
            hints,
        )? {
            if result.is_some() {
                return Err("Ambiguous host point candidates in overlapping branches".into());
            }
            result = Some(candidate);
        }
    }
    Ok(result.or_else(|| (in_document && info.semantic).then_some((node, info))))
}

struct Live {
    walker: UITreeWalker,
    cache: UICacheRequest,
    deadline: Instant,
    calls: usize,
    excluded: Option<u32>,
    x: i32,
    y: i32,
}
fn take_call(deadline: Instant, remaining: &mut usize) -> Result<(), String> {
    if Instant::now() >= deadline {
        return Err("Host point query deadline expired".into());
    }
    if *remaining == 0 {
        return Err("Host point provider call limit exceeded".into());
    }
    *remaining -= 1;
    Ok(())
}
impl Live {
    fn read<T>(
        &mut self,
        call: impl FnOnce(&Self) -> uiautomation::Result<T>,
    ) -> Result<T, String> {
        take_call(self.deadline, &mut self.calls)?;
        call(self).map_err(|e| format!("Host point provider error: {e}"))
    }
    fn step(
        &mut self,
        call: impl FnOnce(&Self) -> uiautomation::Result<UIElement>,
    ) -> Result<Option<UIElement>, String> {
        self.read(|s| match call(s) {
            Ok(n) => Ok(Some(n)),
            Err(e) if e.code() == 0 => Ok(None),
            Err(e) => Err(e),
        })
    }
}
impl Source for Live {
    type Node = UIElement;
    fn info(&mut self, node: &UIElement) -> Result<Info, String> {
        if let Some(excluded) = self.excluded {
            if self.read(|_| node.get_process_id())? == excluded {
                return Err("Host point belongs to excluded process".into());
            }
        }
        let runtime = self.read(|_| node.get_runtime_id())?;
        let handle: isize = self.read(|_| node.get_native_window_handle())?.into();
        let cached = self.read(|s| node.build_updated_cache(&s.cache))?;
        let cached_runtime = self.read(|_| cached.get_runtime_id())?;
        let after = self.read(|_| node.get_runtime_id())?;
        let handle_after: isize = self.read(|_| node.get_native_window_handle())?.into();
        let cached_handle: isize = cached
            .get_cached_native_window_handle()
            .map_err(|e| e.to_string())?
            .into();
        if runtime != after
            || runtime != cached_runtime
            || handle != handle_after
            || handle != cached_handle
        {
            return Err("Host point identity changed during property capture".into());
        }
        let identity = ElementIdentity {
            runtime_id: runtime,
            handle,
            ancestor: None,
            occurrence: None,
        };
        let kind = cached
            .get_cached_control_type()
            .map_err(|e| e.to_string())?;
        let name = cached.get_cached_name().map_err(|e| e.to_string())?;
        let class = cached.get_cached_classname().map_err(|e| e.to_string())?;
        let framework = cached
            .get_cached_framework_id()
            .map_err(|e| e.to_string())?;
        let rect = cached
            .get_cached_bounding_rectangle()
            .map_err(|e| e.to_string())?;
        Ok(Info {
            identity,
            host: name.is_empty()
                && kind == ControlType::Pane
                && class.starts_with("Chrome_WidgetWin_"),
            owner: kind == ControlType::Window && class == "TeamsWebView",
            window: kind == ControlType::Window,
            document: kind == ControlType::Document && framework == "Chrome",
            semantic: framework == "Chrome" && semantic(&format!("{kind:?}")),
            contains: bromium_common::rectangle::is_inside_rectangle(&rect, self.x, self.y),
            visible: !cached.is_cached_offscreen().map_err(|e| e.to_string())?,
        })
    }
    fn parent(&mut self, node: &UIElement) -> Result<Option<UIElement>, String> {
        self.step(|s| s.walker.get_parent(node))
    }
    fn first(&mut self, node: &UIElement) -> Result<Option<UIElement>, String> {
        self.step(|s| s.walker.get_first_child(node))
    }
    fn next(&mut self, node: &UIElement) -> Result<Option<UIElement>, String> {
        self.step(|s| s.walker.get_next_sibling(node))
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reconcile(
    automation: &UIAutomation,
    raw: UIElement,
    x: i32,
    y: i32,
    deadline: Instant,
    excluded: Option<u32>,
    hints: &[Vec<ElementIdentity>],
) -> Result<UIElement, String> {
    if Instant::now() >= deadline {
        return Err("Host point query deadline expired".into());
    }
    // Leave Office, native menus and other unrelated hit routes intact.
    if !raw
        .get_classname()
        .is_ok_and(|class| class.starts_with("Chrome_WidgetWin_"))
    {
        return Ok(raw);
    }
    let cache = crate::capture::cache_request(automation)?;
    cache
        .add_property(UIProperty::IsOffscreen)
        .map_err(|e| e.to_string())?;
    let mut source = Live {
        walker: automation
            .get_control_view_walker()
            .map_err(|e| e.to_string())?,
        cache,
        deadline,
        calls: 4096,
        excluded,
        x,
        y,
    };
    select(&mut source, raw, hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_deadline_and_call_limit_are_not_reset() {
        let mut calls = 1;
        assert!(
            take_call(Instant::now(), &mut calls)
                .unwrap_err()
                .contains("deadline")
        );
        assert_eq!(calls, 1);
        let deadline = Instant::now() + Duration::from_secs(1);
        assert!(take_call(deadline, &mut calls).is_ok());
        assert!(
            take_call(deadline, &mut calls)
                .unwrap_err()
                .contains("limit")
        );
    }
    struct Fixture {
        nodes: Vec<(Info, Option<usize>)>,
        visited: Vec<usize>,
        change_on_revisit: Option<usize>,
        fail_on: Option<usize>,
    }
    impl Source for Fixture {
        type Node = usize;
        fn info(&mut self, node: &usize) -> Result<Info, String> {
            if self.fail_on == Some(*node) {
                return Err("provider failure or deadline".into());
            }
            let changed = self.change_on_revisit == Some(*node) && self.visited.contains(node);
            self.visited.push(*node);
            let mut info = self.nodes[*node].0.clone();
            if changed {
                info.identity.runtime_id.push(999);
            }
            Ok(info)
        }
        fn parent(&mut self, node: &usize) -> Result<Option<usize>, String> {
            Ok(self.nodes[*node].1)
        }
        fn first(&mut self, node: &usize) -> Result<Option<usize>, String> {
            Ok(self.nodes.iter().position(|n| n.1 == Some(*node)))
        }
        fn next(&mut self, node: &usize) -> Result<Option<usize>, String> {
            Ok((*node + 1..self.nodes.len()).find(|&n| self.nodes[n].1 == self.nodes[*node].1))
        }
    }
    fn fixture() -> Fixture {
        let nodes = (0..4)
            .map(|i| {
                (
                    Info {
                        identity: ElementIdentity {
                            runtime_id: vec![42, i as i32],
                            handle: if i == 0 { 100 } else { 0 },
                            ancestor: None,
                            occurrence: None,
                        },
                        host: i == 1,
                        owner: i == 0,
                        window: i == 0,
                        document: i == 2,
                        semantic: i == 3,
                        contains: true,
                        visible: true,
                    },
                    match i {
                        0 => None,
                        1 | 2 => Some(0),
                        _ => Some(2),
                    },
                )
            })
            .collect();
        Fixture {
            nodes,
            visited: vec![],
            change_on_revisit: None,
            fail_on: None,
        }
    }
    #[test]
    fn empty_host_resolves_sibling_webview_search_control() {
        let mut source = fixture();
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 3);
    }
    fn hint(source: &Fixture, mut node: usize) -> Vec<ElementIdentity> {
        let mut path = Vec::new();
        loop {
            path.push(source.nodes[node].0.identity.clone());
            let Some(parent) = source.nodes[node].1 else {
                break;
            };
            node = parent;
        }
        ElementIdentity::qualify_path(&mut path);
        path.reverse();
        path
    }
    #[test]
    fn cached_and_stale_hints_never_replace_live_evidence() {
        let mut source = fixture();
        let mut path = hint(&source, 3);
        assert_eq!(select(&mut source, 1, &[path.clone()]).unwrap(), 3);
        path.last_mut().unwrap().runtime_id.push(999);
        assert_eq!(select(&mut source, 1, &[path.clone()]).unwrap(), 3);
        source.nodes[3].0.visible = false;
        assert_eq!(select(&mut source, 1, &[path]).unwrap(), 1);
    }
    #[test]
    fn warm_hint_does_not_hide_new_overlapping_candidate_or_duplicate_occurrence() {
        for duplicate in [false, true] {
            let mut source = fixture();
            let path = hint(&source, 3);
            let mut other = source.nodes[3].clone();
            if !duplicate {
                other.0.identity.runtime_id.push(999);
            }
            source.nodes.push(other);
            assert!(
                select(&mut source, 1, &[path])
                    .unwrap_err()
                    .contains("Ambiguous")
            );
        }
    }
    #[test]
    fn unrelated_hosts_populated_panes_and_non_document_controls_are_untouched() {
        let mut source = fixture();
        source.nodes[1].0.host = false;
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 1);
        assert_eq!(source.visited, vec![1]);
        let mut source = fixture();
        source.nodes[0].0.owner = false;
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 1);
        let mut source = fixture();
        source.nodes[3].1 = Some(1);
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 1);
        let mut source = fixture();
        source.nodes[2].0.document = false;
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 1);
    }
    #[test]
    fn offpoint_hidden_and_other_window_controls_do_not_override_hit() {
        for mode in 0..3 {
            let mut source = fixture();
            match mode {
                0 => source.nodes[3].0.contains = false,
                1 => source.nodes[3].0.visible = false,
                _ => source.nodes[2].0.window = true,
            }
            assert_eq!(select(&mut source, 1, &[]).unwrap(), 1);
        }
    }
    #[test]
    fn changed_missing_and_failed_candidate_identity_fail_closed() {
        let mut source = fixture();
        source.change_on_revisit = Some(3);
        assert!(select(&mut source, 1, &[]).unwrap_err().contains("changed"));
        let mut source = fixture();
        source.nodes[3].0.identity.runtime_id.clear();
        assert!(
            select(&mut source, 1, &[])
                .unwrap_err()
                .contains("identity")
        );
        let mut source = fixture();
        source.fail_on = Some(3);
        assert!(select(&mut source, 1, &[]).is_err());
    }
    #[test]
    fn nested_semantic_controls_are_not_ambiguous_siblings() {
        let mut source = fixture();
        source.nodes[2].0.semantic = true;
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 3);
    }
    #[test]
    fn streams_covering_descendants_before_offpoint_siblings_and_bounds_work() {
        let mut source = fixture();
        for i in 0..100 {
            let mut other = source.nodes[3].clone();
            other.0.contains = false;
            other.0.identity.runtime_id.push(i);
            other.1 = Some(0);
            source.nodes.push(other);
        }
        assert_eq!(select(&mut source, 1, &[]).unwrap(), 3);
        assert!(
            source.visited.iter().position(|id| *id == 3)
                < source.visited.iter().position(|id| *id == 4)
        );
        for _ in 0..NODES {
            source.nodes.push(source.nodes[4].clone());
        }
        assert!(select(&mut source, 1, &[]).unwrap_err().contains("limit"));
    }
}
