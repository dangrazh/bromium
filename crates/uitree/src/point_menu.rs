//! Same-window menu overlays whose provider hit-test returns underlying content.
use crate::ElementIdentity;
use std::time::Instant;
use uiautomation::{
    UIAutomation, UIElement, UITreeWalker,
    core::UICacheRequest,
    types::{ControlType, UIProperty},
};

const NODE_LIMIT: usize = 256;
const DEPTH_LIMIT: usize = 64;

fn dismiss_signature(xaml: bool, button: bool, class: &str, automation_id: &str) -> bool {
    xaml && button && class == "PopupRoot" && automation_id == "Light Dismiss"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DismissContext {
    overlay: ElementIdentity,
    menu: ElementIdentity,
    owner: ElementIdentity,
}

fn select_dismiss<S: Source>(
    source: &mut S,
    raw: S::Node,
) -> Result<(S::Node, Option<DismissContext>), String> {
    let info = source.info(&raw)?;
    if !info.dismiss || info.offscreen || !info.contains {
        return Ok((raw, None));
    }
    let hit = full_ancestry(source, raw.clone())?;
    let Some((site, site_info)) = hit
        .get(1)
        .filter(|(_, p)| p.input_site && p.identity.handle != 0 && p.contains && !p.offscreen)
    else {
        return Ok((raw, None));
    };
    let Some((_, owner)) = hit.iter().find(|(_, p)| p.window && p.identity.handle != 0) else {
        return Ok((raw, None));
    };
    let Some(focus) = source.focus()? else {
        return Ok((raw, None));
    };
    let focused = full_ancestry(source, focus)?;
    if focused
        .first()
        .is_none_or(|(_, p)| !p.flyout_item || p.offscreen)
        || !focused.iter().any(|(_, p)| p.identity == owner.identity)
    {
        return Ok((raw, None));
    }
    // Never look through the dropdown itself. Only its outside dismissal layer
    // over an independently exposed menu bar can be reconciled.
    let Some((_, menu)) = focused.iter().find(|(_, p)| p.menu && !p.offscreen) else {
        return Ok((raw, None));
    };
    if focused
        .iter()
        .any(|(_, p)| p.menu && p.contains && !p.offscreen)
    {
        return Ok((raw, None));
    }
    let mut remaining = NODE_LIMIT;
    let mut selected = None;
    for bar in source.children(site)? {
        spend(&mut remaining)?;
        let mut bar_info = source.info(&bar)?;
        if !bar_info.menu_bar || !bar_info.contains || bar_info.offscreen {
            continue;
        }
        bar_info.identity.qualify(Some(&site_info.identity));
        if !bar_info.identity.is_resolvable() {
            return Err("XAML menu bar has missing identity".into());
        }
        for heading in source.children(&bar)? {
            spend(&mut remaining)?;
            let mut heading_info = source.info(&heading)?;
            if !heading_info.heading || !heading_info.contains || heading_info.offscreen {
                continue;
            }
            heading_info.identity.qualify(Some(&bar_info.identity));
            if !heading_info.identity.is_resolvable() {
                return Err("XAML menu heading has missing identity".into());
            }
            if selected.is_some() {
                return Err("Ambiguous XAML menu headings beneath dismissal layer".into());
            }
            selected = Some((heading, heading_info.identity));
        }
    }
    let Some((heading, expected)) = selected else {
        return Ok((raw, None));
    };
    let checked = full_ancestry(source, heading.clone())?;
    if checked
        .first()
        .is_none_or(|(_, p)| p.identity != expected || !p.heading || !p.contains || p.offscreen)
        || !checked
            .iter()
            .any(|(_, p)| p.identity == site_info.identity)
        || !checked.iter().any(|(_, p)| p.identity == owner.identity)
    {
        return Err("XAML menu heading changed during verification".into());
    }
    let context = DismissContext {
        overlay: hit[0].1.identity.clone(),
        menu: menu.identity.clone(),
        owner: owner.identity.clone(),
    };
    log::debug!(
        "point_xaml_dismiss result=logical_menu_heading provider_hit={:?} selected={expected:?} active_menu={:?} owner={:?} inspected={} note=not_a_physical_click_target",
        context.overlay,
        context.menu,
        context.owner,
        NODE_LIMIT - remaining
    );
    Ok((heading, Some(context)))
}

fn spend(remaining: &mut usize) -> Result<(), String> {
    if *remaining == 0 {
        return Err("XAML dismissal search node limit exceeded".into());
    }
    *remaining -= 1;
    Ok(())
}

// Popup bridge windows can be nested below their application window in UIA.
// Keep the complete ancestry to prove common ownership, not just common desktop.
fn full_ancestry<S: Source>(
    source: &mut S,
    mut node: S::Node,
) -> Result<Vec<(S::Node, Info)>, String> {
    let mut path = Vec::new();
    for _ in 0..DEPTH_LIMIT {
        let info = source.info(&node)?;
        if !info.identity.is_resolvable() {
            return Err("XAML dismissal ancestry has missing identity".into());
        }
        path.push((node.clone(), info));
        match source.parent(&node)? {
            Some(parent) => node = parent,
            None => {
                let mut ids: Vec<_> = path.iter().map(|(_, p)| p.identity.clone()).collect();
                ElementIdentity::qualify_path(&mut ids);
                for ((_, p), id) in path.iter_mut().zip(ids) {
                    p.identity = id;
                }
                return Ok(path);
            }
        }
    }
    Err("XAML dismissal ancestry depth limit exceeded".into())
}

fn check_deadline(deadline: Instant) -> Result<(), String> {
    if Instant::now() >= deadline {
        Err("Menu point discovery deadline expired".into())
    } else {
        Ok(())
    }
}

#[derive(Clone)]
struct Info {
    identity: ElementIdentity,
    menu: bool,
    window: bool,
    contains: bool,
    offscreen: bool,
    dismiss: bool,
    input_site: bool,
    menu_bar: bool,
    heading: bool,
    flyout_item: bool,
}

trait Source {
    type Node: Clone;
    fn focus(&mut self) -> Result<Option<Self::Node>, String>;
    fn info(&mut self, node: &Self::Node) -> Result<Info, String>;
    fn parent(&mut self, node: &Self::Node) -> Result<Option<Self::Node>, String>;
    fn children(&mut self, node: &Self::Node) -> Result<Vec<Self::Node>, String>;
}

fn ancestry<S: Source>(source: &mut S, mut node: S::Node) -> Result<Vec<(S::Node, Info)>, String> {
    let mut path = Vec::new();
    for _ in 0..DEPTH_LIMIT {
        let info = source.info(&node)?;
        let anchor = info.window && info.identity.handle != 0;
        path.push((node.clone(), info));
        if anchor {
            return Ok(path);
        }
        let Some(parent) = source.parent(&node)? else {
            return Ok(path);
        };
        node = parent;
    }
    Err("Menu point ancestry limit exceeded".into())
}

fn select<S: Source>(source: &mut S, raw: S::Node) -> Result<S::Node, String> {
    let Some(focus) = source.focus()? else {
        return Ok(raw);
    };
    let mut focused = ancestry(source, focus)?;
    // Focus only establishes which menu is active. It is not the hit target.
    let Some(menu_index) = focused
        .iter()
        .position(|(_, p)| p.menu && p.contains && !p.offscreen)
    else {
        return Ok(raw);
    };
    let hit = ancestry(source, raw.clone())?;
    let Some((_, owner)) = focused.last() else {
        return Ok(raw);
    };
    if !owner.window || owner.identity.handle == 0 || !owner.identity.is_resolvable() {
        // Native popups without a Window ancestor retain the existing popup path.
        return Ok(raw);
    }
    if hit.last().is_none_or(|(_, p)| p.identity != owner.identity) {
        return Ok(raw); // A different window's focus must never override the hit.
    }
    let mut ids: Vec<_> = focused.iter().map(|(_, p)| p.identity.clone()).collect();
    ElementIdentity::qualify_path(&mut ids);
    for ((_, info), id) in focused.iter_mut().zip(&ids) {
        info.identity = id.clone();
    }
    if !ids[0].is_resolvable() {
        return Err("Active menu focus has ambiguous or missing identity".into());
    }
    let (menu, info) = &focused[menu_index];
    let mut remaining = NODE_LIMIT;
    let selected = descend(
        source,
        menu.clone(),
        info.clone(),
        &ids[0],
        0,
        &mut remaining,
    )?
    .ok_or("Active menu has no verified point target")?;
    log::debug!(
        "point_menu_overlay result=selected menu={:?} focus={:?} visited={}",
        info.identity,
        ids[0],
        NODE_LIMIT - remaining
    );
    Ok(selected)
}

fn descend<S: Source>(
    source: &mut S,
    node: S::Node,
    info: Info,
    focus: &ElementIdentity,
    depth: usize,
    remaining: &mut usize,
) -> Result<Option<S::Node>, String> {
    if *remaining == 0 || depth == DEPTH_LIMIT {
        return Err("Menu point traversal limit exceeded".into());
    }
    *remaining -= 1;
    let mut selected = None;
    // Only the active menu subtree is enumerated, never the owning window.
    // Do not prune wrappers by bounds: their children can extend beyond them.
    for child in source.children(&node)? {
        let mut child_info = source.info(&child)?;
        child_info.identity.qualify(Some(&info.identity));
        if let Some(candidate) = descend(source, child, child_info, focus, depth + 1, remaining)? {
            if selected.is_some() {
                return Err("Ambiguous overlapping menu point targets".into());
            }
            selected = Some(candidate);
        }
    }
    if selected.is_some() {
        return Ok(selected);
    }
    // A provider can label the focused visible checkbox offscreen. Relax that
    // flag only for the exact focused occurrence within a visible menu.
    if info.contains && (!info.offscreen || info.identity == *focus) {
        if !info.identity.is_resolvable() {
            return Err("Menu point target has missing identity".into());
        }
        return Ok(Some(node));
    }
    Ok(None)
}

struct Live<'a> {
    automation: &'a UIAutomation,
    walker: UITreeWalker,
    cache: UICacheRequest,
    deadline: Instant,
    x: i32,
    y: i32,
    process: u32,
}
impl Live<'_> {
    fn check(&self) -> Result<(), String> {
        check_deadline(self.deadline)
    }
}
impl Source for Live<'_> {
    type Node = UIElement;
    fn focus(&mut self) -> Result<Option<UIElement>, String> {
        self.check()?;
        let focus = match self.automation.get_focused_element() {
            Ok(focus) => focus,
            Err(error) => {
                log::debug!("point_menu_overlay focus_unavailable={error}");
                return Ok(None);
            }
        };
        self.check()?;
        if focus.get_process_id().map_err(|e| e.to_string())? != self.process {
            return Ok(None);
        }
        self.check()?;
        self.walker
            .normalize(&focus)
            .map(Some)
            .map_err(|e| e.to_string())
    }
    fn info(&mut self, node: &UIElement) -> Result<Info, String> {
        self.check()?;
        let cached = node
            .build_updated_cache(&self.cache)
            .map_err(|e| e.to_string())?;
        self.check()?;
        let kind = cached
            .get_cached_control_type()
            .map_err(|e| e.to_string())?;
        let rect = cached
            .get_cached_bounding_rectangle()
            .map_err(|e| e.to_string())?;
        let class = cached.get_cached_classname().unwrap_or_default();
        let xaml = cached.get_cached_framework_id().unwrap_or_default() == "XAML";
        let automation_id = cached.get_cached_automation_id().unwrap_or_default();
        Ok(Info {
            identity: ElementIdentity {
                runtime_id: cached.get_runtime_id().unwrap_or_default(),
                handle: cached
                    .get_cached_native_window_handle()
                    .map_err(|e| e.to_string())?
                    .into(),
                ancestor: None,
                occurrence: None,
            },
            menu: kind == ControlType::Menu,
            window: kind == ControlType::Window,
            contains: bromium_common::rectangle::is_inside_rectangle(&rect, self.x, self.y),
            offscreen: cached.is_cached_offscreen().map_err(|e| e.to_string())?,
            dismiss: dismiss_signature(xaml, kind == ControlType::Button, &class, &automation_id),
            input_site: xaml && kind == ControlType::Pane && class == "InputSiteWindowClass",
            menu_bar: xaml && kind == ControlType::MenuBar,
            heading: xaml
                && kind == ControlType::MenuItem
                && class == "Microsoft.UI.Xaml.Controls.MenuBarItem",
            flyout_item: xaml
                && kind == ControlType::MenuItem
                && matches!(
                    class.as_str(),
                    "MenuFlyoutItem" | "MenuFlyoutSubItem" | "ToggleMenuFlyoutItem"
                ),
        })
    }
    fn parent(&mut self, node: &UIElement) -> Result<Option<UIElement>, String> {
        self.check()?;
        match self.walker.get_parent(node) {
            Ok(parent) => Ok(Some(parent)),
            Err(e) if e.code() == 0 => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn children(&mut self, node: &UIElement) -> Result<Vec<UIElement>, String> {
        self.check()?;
        let mut next = self.walker.get_first_child(node);
        let mut children = Vec::new();
        loop {
            self.check()?;
            match next {
                Ok(child) => {
                    if children.len() == NODE_LIMIT {
                        return Err("Menu point sibling limit exceeded".into());
                    }
                    next = self.walker.get_next_sibling(&child);
                    children.push(child);
                }
                Err(e) if e.code() == 0 => return Ok(children),
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

pub(super) fn reconcile(
    automation: &UIAutomation,
    raw: UIElement,
    x: i32,
    y: i32,
    deadline: Instant,
) -> Result<UIElement, String> {
    check_deadline(deadline)?;
    let process = raw.get_process_id().map_err(|e| e.to_string())?;
    let cache = automation
        .create_cache_request()
        .map_err(|e| e.to_string())?;
    for property in [
        UIProperty::RuntimeId,
        UIProperty::NativeWindowHandle,
        UIProperty::ControlType,
        UIProperty::BoundingRectangle,
        UIProperty::IsOffscreen,
        UIProperty::ClassName,
        UIProperty::FrameworkId,
        UIProperty::AutomationId,
    ] {
        cache.add_property(property).map_err(|e| e.to_string())?;
    }
    let mut source = Live {
        automation,
        walker: automation
            .get_control_view_walker()
            .map_err(|e| e.to_string())?,
        cache,
        deadline,
        x,
        y,
        process,
    };
    select(&mut source, raw)
}

pub(super) fn reconcile_dismiss(
    automation: &UIAutomation,
    raw: UIElement,
    x: i32,
    y: i32,
    deadline: Instant,
) -> Result<(UIElement, Option<DismissContext>), String> {
    check_deadline(deadline)?;
    // Avoid probing unrelated controls or changing their existing lookup route.
    if raw.get_classname().ok().as_deref() != Some("PopupRoot") {
        return Ok((raw, None));
    }
    check_deadline(deadline)?;
    let process = raw.get_process_id().map_err(|e| e.to_string())?;
    let cache = automation
        .create_cache_request()
        .map_err(|e| e.to_string())?;
    for property in [
        UIProperty::RuntimeId,
        UIProperty::NativeWindowHandle,
        UIProperty::ControlType,
        UIProperty::BoundingRectangle,
        UIProperty::IsOffscreen,
        UIProperty::ClassName,
        UIProperty::FrameworkId,
        UIProperty::AutomationId,
    ] {
        cache.add_property(property).map_err(|e| e.to_string())?;
    }
    let mut source = Live {
        automation,
        walker: automation
            .get_control_view_walker()
            .map_err(|e| e.to_string())?,
        cache,
        deadline,
        x,
        y,
        process,
    };
    let (selected, context) = select_dismiss(&mut source, raw)?;
    if context.is_some() {
        check_deadline(deadline)?;
        if selected.get_process_id().map_err(|e| e.to_string())? != process {
            return Err("XAML menu heading belongs to a different process".into());
        }
    }
    Ok((selected, context))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        nodes: Vec<(Info, Option<usize>)>,
        focus: Option<usize>,
        enumerated: Vec<usize>,
    }
    impl Source for Fixture {
        type Node = usize;
        fn focus(&mut self) -> Result<Option<usize>, String> {
            Ok(self.focus)
        }
        fn info(&mut self, node: &usize) -> Result<Info, String> {
            Ok(self.nodes[*node].0.clone())
        }
        fn parent(&mut self, node: &usize) -> Result<Option<usize>, String> {
            Ok(self.nodes[*node].1)
        }
        fn children(&mut self, node: &usize) -> Result<Vec<usize>, String> {
            self.enumerated.push(*node);
            Ok(self
                .nodes
                .iter()
                .enumerate()
                // XAML exposes the dismissal layer upwards but omits it from
                // its parent's children, as observed in the Notepad provider.
                .filter_map(|(i, (info, p))| (*p == Some(*node) && !info.dismiss).then_some(i))
                .collect())
        }
    }
    fn fixture() -> Fixture {
        // Window -> [background Group, Menu -> offscreen focused CheckBox].
        Fixture {
            nodes: (0..4)
                .map(|i| {
                    (
                        Info {
                            identity: ElementIdentity {
                                runtime_id: vec![42, i as i32 + 1],
                                handle: if i == 0 { 100 } else { 0 },
                                ancestor: None,
                                occurrence: None,
                            },
                            menu: i == 2,
                            window: i == 0,
                            contains: true,
                            offscreen: i == 3,
                            dismiss: false,
                            input_site: false,
                            menu_bar: false,
                            heading: false,
                            flyout_item: false,
                        },
                        match i {
                            0 => None,
                            3 => Some(2),
                            _ => Some(0),
                        },
                    )
                })
                .collect(),
            focus: Some(3),
            enumerated: vec![],
        }
    }
    fn dismiss_fixture() -> Fixture {
        let base = fixture().nodes[1].0.clone();
        let mut nodes = Vec::new();
        for i in 0..8 {
            let mut info = base.clone();
            info.identity.runtime_id = vec![42, i as i32 + 1];
            info.identity.handle = match i {
                0 => 100,
                1 => 101,
                5 => 200,
                _ => 0,
            };
            info.window = i == 0 || i == 5;
            info.input_site = i == 1;
            info.dismiss = i == 2;
            info.menu_bar = i == 3;
            info.heading = i == 4;
            info.menu = i == 6;
            info.flyout_item = i == 7;
            info.contains = i < 5;
            nodes.push((
                info,
                match i {
                    0 => None,
                    1 | 5 => Some(0),
                    2 | 3 => Some(1),
                    4 => Some(3),
                    6 => Some(5),
                    _ => Some(6),
                },
            ));
        }
        Fixture {
            nodes,
            focus: Some(7),
            enumerated: vec![],
        }
    }
    #[test]
    fn xaml_light_dismiss_returns_menu_heading_not_dismiss_button() {
        let mut source = dismiss_fixture();
        let (node, context) = select_dismiss(&mut source, 2).unwrap();
        assert_eq!(node, 4);
        assert!(context.is_some());
        assert_eq!(source.enumerated, vec![1, 3]);
    }
    #[test]
    fn xaml_signature_is_specific_and_does_not_depend_on_localized_name() {
        assert!(dismiss_signature(true, true, "PopupRoot", "Light Dismiss"));
        for (xaml, button, class, id) in [
            (false, true, "PopupRoot", "Light Dismiss"),
            (true, false, "PopupRoot", "Light Dismiss"),
            (true, true, "Popup", "Light Dismiss"),
            (true, true, "PopupRoot", "Schließen"),
        ] {
            assert!(!dismiss_signature(xaml, button, class, id));
        }
    }
    #[test]
    fn xaml_requires_known_overlay_site_and_active_same_owner_flyout() {
        for mode in 0..7 {
            let mut s = dismiss_fixture();
            match mode {
                0 => s.nodes[2].0.dismiss = false,
                1 => s.nodes[1].0.input_site = false,
                2 => s.focus = None,
                3 => s.nodes[7].0.flyout_item = false,
                4 => s.nodes[6].0.offscreen = true,
                5 => s.nodes[5].1 = None,
                _ => s.nodes[6].0.contains = true,
            }
            assert_eq!(select_dismiss(&mut s, 2).unwrap().0, 2);
            assert!(s.enumerated.is_empty());
        }
    }
    #[test]
    fn xaml_never_uses_hidden_or_offpoint_or_nonheading_controls() {
        for mode in 0..4 {
            let mut s = dismiss_fixture();
            match mode {
                0 => s.nodes[4].0.contains = false,
                1 => s.nodes[4].0.offscreen = true,
                2 => s.nodes[4].0.heading = false,
                _ => s.nodes[3].0.offscreen = true,
            }
            assert_eq!(select_dismiss(&mut s, 2).unwrap().0, 2);
        }
    }
    #[test]
    fn xaml_duplicate_or_identityless_heading_is_not_selected() {
        let mut s = dismiss_fixture();
        s.nodes.push(s.nodes[4].clone());
        assert!(select_dismiss(&mut s, 2).unwrap_err().contains("Ambiguous"));
        let mut s = dismiss_fixture();
        s.nodes[4].0.identity.runtime_id.clear();
        assert!(select_dismiss(&mut s, 2).unwrap_err().contains("identity"));
    }
    #[test]
    fn xaml_context_detects_overlay_disappearance_and_menu_replacement() {
        let mut s = dismiss_fixture();
        let original = select_dismiss(&mut s, 2).unwrap().1;
        s.nodes[6].0.identity.runtime_id.push(999);
        assert_ne!(original, select_dismiss(&mut s, 2).unwrap().1);
        s.nodes[2].0.dismiss = false;
        assert_ne!(original, select_dismiss(&mut s, 2).unwrap().1);
    }
    #[test]
    fn xaml_reconciliation_is_bounded_and_does_not_scan_window_subtrees() {
        let mut s = dismiss_fixture();
        for _ in 0..NODE_LIMIT {
            let mut n = s.nodes[4].clone();
            n.0.contains = false;
            s.nodes.push(n);
        }
        assert!(select_dismiss(&mut s, 2).unwrap_err().contains("limit"));
        assert!(!s.enumerated.contains(&0));
        let mut s = dismiss_fixture();
        s.nodes[0].1 = Some(1);
        assert!(select_dismiss(&mut s, 2).unwrap_err().contains("depth"));
    }
    #[test]
    fn menu_overlay_replaces_background_hit_with_focused_offscreen_checkbox() {
        let mut source = fixture();
        assert_eq!(select(&mut source, 1).unwrap(), 3);
        assert!(
            !source.enumerated.contains(&0),
            "must not enumerate the window"
        );
        assert!(
            !source.enumerated.contains(&1),
            "must not enumerate background content"
        );
    }
    #[test]
    fn focus_outside_menu_or_point_outside_menu_keeps_provider_hit() {
        let mut source = fixture();
        source.focus = Some(1);
        assert_eq!(select(&mut source, 1).unwrap(), 1);
        source.focus = Some(3);
        source.nodes[2].0.contains = false;
        assert_eq!(select(&mut source, 1).unwrap(), 1);
        assert!(source.enumerated.is_empty());
    }
    #[test]
    fn focus_in_another_window_cannot_override_hit() {
        let mut source = fixture();
        let mut other = source.nodes[0].0.clone();
        other.identity.handle = 200;
        source.nodes.push((other, None));
        source.nodes[1].1 = Some(4);
        assert_eq!(select(&mut source, 1).unwrap(), 1);
        assert!(source.enumerated.is_empty());
    }
    #[test]
    fn native_popup_without_window_ancestor_keeps_provider_hit() {
        let mut source = fixture();
        source.nodes[0].0.window = false;
        assert_eq!(select(&mut source, 3).unwrap(), 3);
        assert!(source.enumerated.is_empty());
    }
    #[test]
    fn point_selects_other_menu_descendant_not_unconditionally_focus() {
        let mut source = fixture();
        source.nodes[3].0.contains = false;
        let mut other = source.nodes[3].0.clone();
        other.identity.runtime_id = vec![42, 5];
        other.offscreen = false;
        other.contains = true;
        source.nodes.push((other, Some(2)));
        assert_eq!(select(&mut source, 1).unwrap(), 4);
    }
    #[test]
    fn overlapping_siblings_are_ambiguous_even_with_focus() {
        let mut source = fixture();
        let mut other = source.nodes[3].0.clone();
        other.identity.runtime_id = vec![42, 5];
        other.offscreen = false;
        source.nodes.push((other, Some(2)));
        assert!(select(&mut source, 1).unwrap_err().contains("Ambiguous"));
    }
    #[test]
    fn offscreen_exception_is_not_applied_to_unfocused_siblings() {
        let mut source = fixture();
        let mut hidden = source.nodes[3].0.clone();
        hidden.identity.runtime_id = vec![42, 5];
        source.nodes.push((hidden, Some(2)));
        assert_eq!(select(&mut source, 1).unwrap(), 3);
        source.nodes[2].0.offscreen = true;
        assert_eq!(select(&mut source, 1).unwrap(), 1);
    }
    #[test]
    fn missing_identity_and_duplicate_focus_occurrences_fail_closed() {
        let mut source = fixture();
        source.nodes[3].0.identity.runtime_id.clear();
        assert!(select(&mut source, 1).unwrap_err().contains("identity"));
        let mut source = fixture();
        source.nodes.push(source.nodes[3].clone());
        assert!(select(&mut source, 1).unwrap_err().contains("Ambiguous"));
    }
    #[test]
    fn wrapper_and_child_are_not_mistaken_for_overlapping_siblings() {
        let mut source = fixture();
        let mut wrapper = source.nodes[3].0.clone();
        wrapper.identity.runtime_id = vec![42, 5];
        wrapper.offscreen = false;
        source.nodes.push((wrapper, Some(2)));
        source.nodes[3].1 = Some(4);
        assert_eq!(select(&mut source, 1).unwrap(), 3);
    }
    #[test]
    fn menu_closing_changes_the_verified_target() {
        let mut source = fixture();
        let initial = select(&mut source, 1).unwrap();
        source.focus = Some(1);
        let verified = select(&mut source, 1).unwrap();
        assert_ne!(
            source.nodes[initial].0.identity,
            source.nodes[verified].0.identity
        );
    }
    #[test]
    fn menu_work_and_ancestry_are_bounded() {
        let mut source = fixture();
        for i in 0..NODE_LIMIT {
            let mut info = source.nodes[3].0.clone();
            info.identity.runtime_id = vec![42, 10 + i as i32];
            info.contains = false;
            source.nodes.push((info, Some(2)));
        }
        assert!(select(&mut source, 1).unwrap_err().contains("limit"));
        let mut source = fixture();
        source.nodes[2].1 = Some(3);
        assert!(select(&mut source, 1).unwrap_err().contains("limit"));
    }
    #[test]
    fn expired_deadline_is_reported() {
        assert!(
            check_deadline(Instant::now())
                .unwrap_err()
                .contains("deadline")
        );
    }
}
