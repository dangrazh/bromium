//! Canonical cached tree and transactional capture publication.
use crate::{SaveUIElement, UIElementInTree, UITreeError, UITreeMap};
use bromium_common::format_runtime_id;
use quick_xml::{
    Writer,
    events::{BytesEnd, BytesStart, Event},
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, atomic::AtomicBool, mpsc::Sender};
use std::time::Instant;

/// Owned observation. None means children were not enumerated, not an empty list.
#[derive(Clone, Debug)]
pub struct Observation {
    pub properties: SaveUIElement,
    pub children: Option<Vec<Observation>>,
}
impl Observation {
    pub(crate) fn qualify(&mut self, ancestor: Option<&crate::ElementIdentity>) {
        self.properties.qualify(ancestor);
        let context = self.properties.child_context();
        if let Some(children) = &mut self.children {
            for child in children {
                child.qualify(context.as_ref());
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Coverage {
    pub children_observed: bool,
    pub observed_at: Instant,
    /// Membership age is not reset by a property-only observation.
    pub children_observed_at: Option<Instant>,
}

/// Arena slots are never reused within a tree, so removed indices cannot alias new nodes.
#[derive(Clone, Debug)]
pub struct UITree {
    tree: UITreeMap<SaveUIElement>,
    coverage: HashMap<usize, Coverage>,
    xml_dom_tree: String,
    ui_elements: Vec<UIElementInTree>,
    revision: u64,
    initialized: bool,
    runtime_indices: HashMap<Vec<i32>, Vec<usize>>,
    point_locator: Option<usize>,
}

impl UITree {
    pub fn empty() -> Self {
        Self {
            tree: UITreeMap::new(
                "Unobserved desktop".into(),
                String::new(),
                SaveUIElement::default(),
            ),
            coverage: HashMap::new(),
            xml_dom_tree: "<Unobserved/>".into(),
            ui_elements: Vec::new(),
            revision: 0,
            initialized: false,
            runtime_indices: HashMap::new(),
            point_locator: None,
        }
    }

    pub fn from_observation(observation: Observation) -> Result<Self, String> {
        let mut tree = Self::empty();
        tree.commit(0, observation)?;
        Ok(tree)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    pub fn get_tree(&self) -> &UITreeMap<SaveUIElement> {
        &self.tree
    }
    pub fn get_xml_dom_tree(&self) -> &str {
        &self.xml_dom_tree
    }
    pub fn get_elements(&self) -> &[UIElementInTree] {
        &self.ui_elements
    }
    /// A local filtered view preserves desktop ancestry and the committed revision.
    pub fn view(&self, title: Option<&str>) -> Self {
        let Some(title) = title else {
            return self.clone();
        };
        let mut view = self.clone();
        for window in self.children(0).to_vec() {
            if !self.node(window).1.get_name().contains(title) {
                view.remove_branch(window)
                    .expect("Validated top-level window");
            }
        }
        if view.initialized {
            view.rebuild()
                .expect("Existing properties remain serializable");
        }
        view
    }
    pub fn root(&self) -> usize {
        0
    }
    pub fn children(&self, index: usize) -> &[usize] {
        if self.tree.has_node(index) {
            self.tree.children(index)
        } else {
            &[]
        }
    }
    pub fn try_node(&self, index: usize) -> Option<(&str, &SaveUIElement)> {
        self.tree.has_node(index).then(|| {
            let node = self.tree.node(index);
            (node.name.as_str(), &node.data)
        })
    }
    /// Compatibility access; use try_node for persisted/external indices.
    pub fn node(&self, index: usize) -> (&str, &SaveUIElement) {
        self.try_node(index).expect("Invalid tree node")
    }
    pub fn for_each<F: FnMut(usize, &SaveUIElement)>(&self, f: F) {
        if self.initialized {
            self.tree.for_each(f);
        }
    }
    pub fn pretty_print_tree(&self) {
        self.for_each(|index, props| println!("{index}: {props}"));
    }
    pub fn coverage(&self, index: usize) -> Option<&Coverage> {
        self.coverage.get(&index)
    }
    pub fn index_for_id(&self, id: &[i32]) -> Option<usize> {
        if id.is_empty() {
            return None;
        }
        let matches = self.runtime_indices.get(id)?;
        (matches.len() == 1).then(|| matches[0])
    }
    pub fn index_for_identity(&self, identity: &crate::ElementIdentity) -> Option<usize> {
        self.tree
            .get_element_by_runtime_id(&identity.key())
            .map(|n| n.index)
    }
    pub fn indices_for_id(&self, id: &[i32]) -> Vec<usize> {
        self.runtime_indices.get(id).cloned().unwrap_or_default()
    }
    pub fn is_descendant(&self, mut node: usize, ancestor: usize) -> bool {
        if !self.tree.has_node(node) || !self.tree.has_node(ancestor) {
            return false;
        }
        loop {
            if node == ancestor {
                return true;
            }
            if node == 0 {
                return false;
            }
            node = self.tree.node(node).parent;
        }
    }
    pub fn owning_window(&self, mut node: usize) -> usize {
        while node != 0 && self.tree.node(node).parent != 0 {
            node = self.tree.node(node).parent;
        }
        node
    }
    pub fn observation(&self, index: usize) -> Observation {
        Observation {
            properties: self.tree.node(index).data.clone(),
            children: self
                .coverage
                .get(&index)
                .filter(|c| c.children_observed)
                .map(|_| {
                    self.children(index)
                        .iter()
                        .map(|&child| self.observation(child))
                        .collect()
                }),
        }
    }

    /// Apply a complete observation for its declared coverage, preserving unobserved descendants.
    /// All validation and projections finish before publishing the candidate.
    pub fn commit(&mut self, target: usize, mut observation: Observation) -> Result<(), String> {
        let started = Instant::now();
        if !self.tree.has_node(target) {
            return Err("Capture target was removed".into());
        }
        let context = if target == 0 {
            observation.properties.identity().ancestor.map(|a| *a)
        } else {
            self.tree
                .node(self.tree.node(target).parent)
                .data
                .child_context()
        };
        if observation.properties.identity().ancestor.is_some()
            && observation.properties.identity().ancestor.as_deref() != context.as_ref()
        {
            return Err("Capture target native ancestry changed".into());
        }
        observation.qualify(context.as_ref());
        if log::log_enabled!(log::Level::Debug) {
            if let Err(error) =
                Self::validate_observation(&observation, &observation, &mut HashMap::new(), 0)
            {
                log::debug!("tree_identity_diagnostic {error}; policy=retain_snapshot_occurrences");
            }
        }
        Self::assign_occurrences(&mut observation, context.as_ref())?;
        let mut ids = HashMap::new();
        Self::validate_observation(&observation, &observation, &mut ids, 0)?;
        if self.initialized
            && self.tree.node(target).data.identity() != observation.properties.identity()
        {
            return Err("Capture target identity changed".into());
        }
        let mut candidate = self.clone();
        if !candidate.initialized {
            candidate.tree = UITreeMap::new(
                String::new(),
                observation.properties.identity().key(),
                observation.properties.clone(),
            );
            candidate.initialized = true;
        }
        candidate.merge(target, observation)?;
        candidate.rebuild()?;
        candidate.revision = self.revision + 1;
        *self = candidate;
        log::debug!(
            "tree_commit revision={} nodes={} elapsed_us={}",
            self.revision,
            self.ui_elements.len(),
            started.elapsed().as_micros()
        );
        Ok(())
    }

    /// Add one positively discovered desktop child without asserting that its
    /// siblings were enumerated. Existing subtrees and membership age survive.
    pub(crate) fn discover_window(&mut self, properties: SaveUIElement) -> Result<usize, String> {
        let identity = properties.identity();
        let coverage = self.coverage.get(&0).cloned();
        let mut children: Vec<_> = self
            .children(0)
            .iter()
            .map(|&id| Observation {
                properties: self.node(id).1.clone(),
                children: None,
            })
            .collect();
        children.push(Observation {
            properties,
            children: None,
        });
        self.commit(
            0,
            Observation {
                properties: self.node(0).1.clone(),
                children: Some(children),
            },
        )?;
        if let Some(coverage) = coverage {
            self.coverage.insert(0, coverage);
        }
        self.index_for_identity(&identity)
            .ok_or_else(|| "Discovered window not published".into())
    }

    fn validate_observation<'a>(
        root: &'a Observation,
        node: &'a Observation,
        ids: &mut HashMap<crate::ElementIdentity, &'a Observation>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 256 {
            return Err("Capture depth limit exceeded".into());
        }
        let id = node.properties.get_runtime_id();
        if id.is_empty() && node.properties.identity().occurrence.is_none() {
            return Err(format!(
                "Missing runtime ID; element={}",
                Self::observation_path(root, node)
            ));
        }
        if let Some(first) = ids.insert(node.properties.identity(), node) {
            return Err(format!(
                "Capture contains duplicate runtime ID; runtime_id={id:?}; first={}; second={}",
                Self::observation_path(root, first),
                Self::observation_path(root, node)
            ));
        }
        if let Some(children) = &node.children {
            for child in children {
                Self::validate_observation(root, child, ids, depth + 1)?;
            }
        }
        Ok(())
    }

    /// Provider IDs are not guaranteed to identify occurrences. Retain every
    /// observation, but never carry ambiguous identity across fresh captures.
    fn assign_occurrences(
        observation: &mut Observation,
        context: Option<&crate::ElementIdentity>,
    ) -> Result<(), String> {
        fn count(
            node: &Observation,
            ids: &mut HashMap<crate::ElementIdentity, usize>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 256 {
                return Err("Capture depth limit exceeded".into());
            }
            *ids.entry(node.properties.identity()).or_default() += 1;
            for child in node.children.iter().flatten() {
                count(child, ids, depth + 1)?;
            }
            Ok(())
        }
        fn assign(
            node: &mut Observation,
            ids: &HashMap<crate::ElementIdentity, usize>,
            context: Option<&crate::ElementIdentity>,
        ) {
            let original = node.properties.identity();
            if original.runtime_id.is_empty() || ids.get(&original).copied().unwrap_or(0) > 1 {
                log::debug!(
                    "tree_snapshot_occurrence identity={:?} name={:?} control_type={:?} reason=missing_or_ambiguous_provider_identity",
                    original,
                    node.properties.get_name(),
                    node.properties.get_control_type()
                );
                node.properties.mark_snapshot_only();
            }
            node.properties.qualify(context);
            let context = node.properties.child_context();
            for child in node.children.iter_mut().flatten() {
                assign(child, ids, context.as_ref());
            }
        }
        let mut counts = HashMap::new();
        count(observation, &mut counts, 0)?;
        assign(observation, &counts, context);
        Ok(())
    }

    /// Diagnostic ancestry is relative to the capture root, not an XPath locator.
    /// Build it only on rejection; successful validation retains borrowed identities.
    fn observation_path(root: &Observation, target: &Observation) -> String {
        fn find<'a>(
            node: &'a Observation,
            target: &Observation,
            ordinal: usize,
            path: &mut Vec<(usize, &'a Observation)>,
        ) -> bool {
            path.push((ordinal, node));
            if std::ptr::eq(node, target) {
                return true;
            }
            if let Some(children) = &node.children {
                for (index, child) in children.iter().enumerate() {
                    if find(child, target, index + 1, path) {
                        return true;
                    }
                }
            }
            path.pop();
            false
        }
        let mut path = Vec::new();
        find(root, target, 1, &mut path);
        path.iter()
            .map(|(ordinal, node)| {
                let p = &node.properties;
                format!(
                    "[child={ordinal} name={:?} control_type={:?} runtime_id={:?} native_handle={}]",
                    p.get_name(),
                    p.get_control_type(),
                    p.get_runtime_id(),
                    p.get_handle()
                )
            })
            .collect::<Vec<_>>()
            .join(" -> ")
    }

    fn merge(&mut self, index: usize, observation: Observation) -> Result<(), String> {
        self.tree.node_mut(index).data = observation.properties;
        let complete = observation.children.is_some();
        let previous = self
            .coverage
            .get(&index)
            .is_some_and(|c| c.children_observed);
        let children_observed_at = if complete {
            Some(Instant::now())
        } else {
            self.coverage
                .get(&index)
                .and_then(|c| c.children_observed_at)
        };
        self.coverage.insert(
            index,
            Coverage {
                children_observed: complete || previous,
                observed_at: Instant::now(),
                children_observed_at,
            },
        );
        if let Some(children) = observation.children {
            let wanted: HashSet<crate::ElementIdentity> =
                children.iter().map(|c| c.properties.identity()).collect();
            for old in self.children(index).to_vec() {
                if !wanted.contains(&self.tree.node(old).data.identity()) {
                    self.remove_branch(old)?;
                }
            }
            let mut order = Vec::new();
            for child in children {
                let identity = child.properties.identity();
                let child_index = match self.index_for_identity(&identity) {
                    Some(existing) => {
                        if self.tree.node(existing).parent != index {
                            return Err(
                                "Reparent requires reconciliation of both parent contexts".into()
                            );
                        }
                        existing
                    }
                    None => {
                        self.tree
                            .add_child(index, "", &identity.key(), child.properties.clone())
                    }
                };
                self.merge(child_index, child)?;
                order.push(child_index);
            }
            self.tree.node_mut(index).children = order;
        }
        Ok(())
    }
    fn remove_branch(&mut self, index: usize) -> Result<(), String> {
        for child in self.children(index).to_vec() {
            self.remove_branch(child)?;
        }
        self.coverage.remove(&index);
        self.tree.remove_node(index).map_err(|e| e.to_string())
    }

    fn rebuild(&mut self) -> Result<(), String> {
        self.runtime_indices.clear();
        for node in self.tree.nodes() {
            self.runtime_indices
                .entry(node.data.get_runtime_id().to_vec())
                .or_default()
                .push(node.index);
        }
        for indices in self.runtime_indices.values_mut() {
            indices.sort_unstable();
        }
        let mut writer = Writer::new(Vec::new());
        self.ui_elements.clear();
        self.project(0, 0, 0, &mut writer)?;
        self.tree.rebuild_names();
        self.xml_dom_tree = String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())?;
        self.ui_elements.sort_by_key(|e| {
            (
                e.get_element_props().get_z_order(),
                e.get_element_props().get_bounding_rect_size(),
            )
        });
        Ok(())
    }
    fn project(
        &mut self,
        index: usize,
        level: usize,
        window: usize,
        writer: &mut Writer<Vec<u8>>,
    ) -> Result<(), String> {
        let node = self.tree.node_mut(index);
        node.data.set_context(level, window);
        node.name = format!(
            "'{}' {} ({})",
            node.data.get_name(),
            node.data.get_control_type(),
            format_runtime_id(node.data.get_runtime_id())
        );
        let props = &node.data;
        let tag = if props.get_control_type().is_empty() {
            "Unknown"
        } else {
            props.get_control_type()
        }
        .to_string();
        let mut start = BytesStart::new(&tag);
        let order = window.to_string();
        let raw_id = format_runtime_id(props.get_runtime_id());
        let handle = props.get_handle().to_string();
        start.push_attribute(("RtID", raw_id.as_str()));
        start.push_attribute(("NodeKey", node.runtime_id.as_str()));
        start.push_attribute(("NativeWindowHandle", handle.as_str()));
        start.push_attribute((
            "IdentityStatus",
            if props.identity().is_resolvable() {
                "provider"
            } else {
                "snapshot-only"
            },
        ));
        start.push_attribute(("Name", props.get_name()));
        start.push_attribute(("ControlType", props.get_control_type()));
        start.push_attribute(("AutomationId", props.get_automation_id()));
        start.push_attribute(("z-order", order.as_str()));
        writer
            .write_event(Event::Start(start))
            .map_err(|e| e.to_string())?;
        self.ui_elements
            .push(UIElementInTree::new(props.clone(), index));
        for child in self.children(index).to_vec() {
            self.project(
                child,
                level + 1,
                if index == 0 { child } else { window },
                writer,
            )?;
        }
        writer
            .write_event(Event::End(BytesEnd::new(&tag)))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn get_xpath_for_element(
        &self,
        index: usize,
        simple: bool,
    ) -> Result<String, xmlutil::xpath_gen::XpathGenError> {
        let id = self
            .try_node(index)
            .map(|(_, p)| p.identity().key())
            .unwrap_or_default();
        if self.point_locator == Some(index) {
            return xmlutil::xpath_gen::get_xpath_spine_from_attribute(
                "NodeKey",
                &id,
                &self.xml_dom_tree,
                simple,
            );
        }
        let complete_window = !simple
            && self
                .try_node(index)
                .is_some_and(|_| self.has_complete_subtree(self.owning_window(index)));
        if complete_window {
            xmlutil::xpath_gen::get_xpath_window_scoped_from_attribute(
                "NodeKey",
                &id,
                &self.xml_dom_tree,
            )
        } else {
            xmlutil::xpath_gen::get_xpath_full_from_attribute(
                "NodeKey",
                &id,
                &self.xml_dom_tree,
                simple,
            )
        }
    }
    pub(crate) fn restrict_point_locator(&mut self, index: usize) {
        self.point_locator = Some(index);
    }
    pub(crate) fn has_complete_subtree(&self, index: usize) -> bool {
        self.coverage(index).is_some_and(|c| c.children_observed)
            && self
                .children(index)
                .iter()
                .all(|&child| self.has_complete_subtree(child))
    }
    pub fn query(&self, xpath: &str) -> Result<Vec<&SaveUIElement>, String> {
        // Parenthesize expressions to preserve union and predicate semantics.
        let expr = if xpath.trim_end().ends_with("/@RtID") {
            format!("({})/@NodeKey", xpath.trim_end().trim_end_matches("/@RtID"))
        } else {
            format!("({xpath})/@NodeKey")
        };
        let result = xmlutil::xpath_eval::eval_xpath_thread_cached(&expr, &self.xml_dom_tree);
        if !result.is_success() {
            return Err(result.get_error_msg().to_owned());
        }
        Ok(result
            .get_result_items()
            .iter()
            .filter_map(|item| {
                self.tree
                    .get_element_by_runtime_id(item.get_item_value())
                    .map(|n| &n.data)
            })
            .collect())
    }
    pub fn get_element_by_xpath(&self, xpath: &str) -> Option<&SaveUIElement> {
        self.query(xpath).ok()?.into_iter().next()
    }
    pub fn get_elements_by_xpath(&self, xpath: &str) -> Option<Vec<&SaveUIElement>> {
        let result = self.query(xpath).ok()?;
        (!result.is_empty()).then_some(result)
    }
    pub fn append_or_replace_subtree(
        &mut self,
        parent: usize,
        subtree: UITree,
    ) -> Result<usize, String> {
        if !subtree.initialized || !self.tree.has_node(parent) {
            return Err("Invalid subtree/parent".into());
        }
        let mut observation = subtree.observation(0);
        observation.qualify(self.node(parent).1.child_context().as_ref());
        if let Some(existing) = self.index_for_identity(&observation.properties.identity()) {
            if self.tree.node(existing).parent != parent {
                return Err("Subtree parent mismatch".into());
            }
            self.commit(existing, observation)?;
            Ok(existing)
        } else {
            if !self.coverage(parent).is_some_and(|c| c.children_observed) {
                return Err("Parent membership not observed".into());
            }
            let id = observation.properties.identity();
            let mut parent_obs = self.observation(parent);
            parent_obs
                .children
                .as_mut()
                .ok_or("Missing parent children")?
                .push(observation);
            self.commit(parent, parent_obs)?;
            self.index_for_identity(&id)
                .ok_or("Missing inserted subtree".into())
        }
    }
}

/// Compatibility capture entry point. New consumers use TreeService's bounded worker.
pub fn get_all_elements_xml(
    tx: Sender<Result<UITree, UITreeError>>,
    root: Option<SaveUIElement>,
    max_depth: Option<usize>,
    exclude: Option<String>,
    title: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) {
    let result = crate::capture::capture_legacy(root, max_depth, exclude, title, cancel);
    let _ = tx.send(result);
}

/// Uses the same capture semantics; independent parallel merge walker retired.
pub fn get_all_elements_par_xml(
    tx: Sender<Result<UITree, UITreeError>>,
    depth: Option<usize>,
    exclude: Option<String>,
    title: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) {
    get_all_elements_xml(tx, None, depth, exclude, title, cancel);
}

#[cfg(test)]
mod tests {
    #[test]
    fn office_identity_less_sibling_does_not_block_healthy_control() {
        let missing = Observation {
            properties: SaveUIElement::default(),
            children: Some(vec![]),
        };
        let tree = UITree::from_observation(obs(
            1,
            "Desktop",
            Some(vec![missing, obs(3, "Healthy button", Some(vec![]))]),
        ))
        .expect("Missing provider identity must not reject the healthy sibling");
        assert_eq!(tree.children(0).len(), 2);
        assert!(tree.index_for_id(&[42, 3]).is_some());
    }

    #[test]
    fn office_same_parent_duplicate_rows_are_preserved() {
        let rows = || {
            obs(
                1,
                "Desktop",
                Some(vec![
                    obs(-122, "First mail", Some(vec![])),
                    obs(-122, "Second mail", Some(vec![])),
                    obs(3, "Healthy button", Some(vec![])),
                ]),
            )
        };
        let mut tree = UITree::from_observation(rows())
            .expect("Ambiguous mail rows must not reject an entire window");
        assert_eq!(tree.children(0).len(), 3);
        let tokens = tree.indices_for_id(&[42, -122]);
        assert_eq!(tokens.len(), 2);
        let healthy = tree.index_for_id(&[42, 3]).unwrap();
        tree.commit(0, rows()).unwrap();
        assert!(
            tokens.iter().all(|&id| tree.try_node(id).is_none()),
            "Ambiguous rows must never reuse a previous snapshot's token"
        );
        assert_eq!(tree.index_for_id(&[42, 3]), Some(healthy));
    }

    #[test]
    fn stable_ambiguous_stable_transition_never_revives_an_old_token() {
        let window = |rows| obs(1, "Desktop", Some(rows));
        let row = || obs(2, "Mail", Some(vec![]));
        let mut tree = UITree::from_observation(window(vec![row()])).unwrap();
        let stable = tree.index_for_id(&[42, 2]).unwrap();
        tree.commit(0, window(vec![row(), row()])).unwrap();
        assert!(tree.try_node(stable).is_none());
        let ambiguous = tree.indices_for_id(&[42, 2]);
        tree.commit(0, window(vec![row()])).unwrap();
        let recovered = tree.index_for_id(&[42, 2]).unwrap();
        assert_ne!(stable, recovered);
        assert!(ambiguous.iter().all(|&id| tree.try_node(id).is_none()));
        assert!(tree.node(recovered).1.identity().is_resolvable());
    }
    use super::*;
    #[test]
    fn scoped_locator_requires_complete_window_and_falls_back_on_duplicates() {
        let window = Observation {
            properties: SaveUIElement::fixture(2, "Notepad", "Window"),
            children: Some(vec![
                obs(3, "Settings", Some(vec![])),
                obs(4, "Unknown", None),
            ]),
        };
        let mut tree = UITree::from_observation(obs(1, "Desktop", Some(vec![window]))).unwrap();
        let target = tree.index_for_id(&[42, 3]).unwrap();
        let unknown = tree.index_for_id(&[42, 4]).unwrap();
        assert!(
            !tree
                .get_xpath_for_element(target, false)
                .unwrap()
                .contains("//")
        );
        tree.commit(unknown, obs(4, "Unknown", Some(vec![])))
            .unwrap();
        let scoped = tree.get_xpath_for_element(target, false).unwrap();
        assert!(scoped.ends_with("//Button[@Name='Settings']"));
        assert_eq!(tree.query(&scoped).unwrap()[0].get_runtime_id(), &[42, 3]);
        assert!(
            !tree
                .get_xpath_for_element(target, true)
                .unwrap()
                .contains("//")
        );
        tree.commit(
            unknown,
            obs(4, "Unknown", Some(vec![obs(5, "Settings", Some(vec![]))])),
        )
        .unwrap();
        let fallback = tree.get_xpath_for_element(target, false).unwrap();
        assert!(!fallback.contains("//"));
        let matches = tree.query(&fallback).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].get_runtime_id(), &[42, 3]);
    }
    fn obs(id: i32, name: &str, children: Option<Vec<Observation>>) -> Observation {
        Observation {
            properties: SaveUIElement::fixture(id, name, if id == 1 { "Pane" } else { "Button" }),
            children,
        }
    }
    fn tree() -> UITree {
        UITree::from_observation(obs(
            1,
            "Desktop",
            Some(vec![
                obs(2, "A", Some(vec![obs(3, "child", Some(vec![]))])),
                obs(4, "B", Some(vec![])),
            ]),
        ))
        .unwrap()
    }
    #[test]
    fn empty_and_leaf_are_safe() {
        let t = UITree::empty();
        t.pretty_print_tree();
        t.for_each(|_, _| panic!("empty"));
        assert!(t.get_elements().is_empty());
        assert!(t.try_node(99).is_none());
        let t = UITree::from_observation(obs(1, "Desktop", Some(vec![]))).unwrap();
        assert_eq!(t.get_elements().len(), 1);
        assert_eq!(t.query("/Pane").unwrap().len(), 1);
    }
    #[test]
    fn deletion_updates_all_views_and_preserves_unrelated_ids() {
        let mut t = tree();
        let b = t.index_for_id(&[42, 4]).unwrap();
        let a = t.index_for_id(&[42, 2]).unwrap();
        t.commit(a, obs(2, "renamed", Some(vec![]))).unwrap();
        assert!(t.index_for_id(&[42, 3]).is_none());
        assert_eq!(t.get_elements().len(), 3);
        assert_eq!(t.index_for_id(&[42, 4]), Some(b));
        assert!(t.query("//Button[@Name='child']").unwrap().is_empty());
        assert_eq!(t.query("//Button[@Name='renamed']").unwrap().len(), 1);
        for e in t.get_elements() {
            assert_eq!(
                t.node(e.get_tree_index()).1.get_runtime_id(),
                e.get_element_props().get_runtime_id()
            );
        }
    }
    #[test]
    fn popup_alias_preserves_both_branches_locators_and_input_lifetime() {
        fn pane(name: &str, handle: isize, child: Observation) -> Observation {
            Observation {
                properties: SaveUIElement::fixture(3, name, "Pane").with_handle(handle),
                children: Some(vec![child]),
            }
        }
        let input = pane("", 0xA0F58, obs(5, "Settings", Some(vec![])));
        let popup = pane("PopupHost", 0x6010E, obs(4, "Popup", Some(vec![])));
        let window = |panes| Observation {
            properties: SaveUIElement::fixture(2, "Notepad", "Window").with_handle(525558),
            children: Some(panes),
        };
        let mut t =
            UITree::from_observation(obs(1, "Desktop", Some(vec![window(vec![input.clone()])])))
                .unwrap();
        let input_index = t.index_for_identity(&input.properties.identity()).unwrap();
        let settings_index = t.index_for_id(&[42, 5]).unwrap();
        let owner = t.index_for_id(&[42, 2]).unwrap();
        for _ in 0..3 {
            t.commit(owner, window(vec![popup.clone(), input.clone()]))
                .unwrap();
            assert_eq!(
                t.index_for_id(&[42, 3]),
                None,
                "raw identity must be ambiguous"
            );
            assert_eq!(
                t.index_for_identity(&input.properties.identity()),
                Some(input_index)
            );
            assert_eq!(t.index_for_id(&[42, 5]), Some(settings_index));
            let results = t.query("//*[@RtID='42-3']").unwrap();
            assert_eq!(results.len(), 2);
            assert_ne!(results[0].get_handle(), results[1].get_handle());
            assert_eq!(t.query("//*[@RtID='42-3']/@RtID").unwrap().len(), 2);
            for (index, expected) in [
                (input_index, input.properties.identity()),
                (
                    t.index_for_identity(&popup.properties.identity()).unwrap(),
                    popup.properties.identity(),
                ),
            ] {
                for simple in [true, false] {
                    let xpath = t.get_xpath_for_element(index, simple).unwrap();
                    let found = t.query(&xpath).unwrap();
                    assert_eq!(found.len(), 1, "{xpath}");
                    assert_eq!(found[0].identity(), expected);
                }
            }
            let locator = t.get_xpath_for_element(settings_index, false).unwrap();
            assert_eq!(t.query(&locator).unwrap()[0].get_name(), "Settings");
            t.commit(
                input_index,
                Observation {
                    properties: input.properties.clone(),
                    children: None,
                },
            )
            .unwrap();
            assert!(t.index_for_identity(&popup.properties.identity()).is_some());
            let removed_popup = t.index_for_identity(&popup.properties.identity()).unwrap();
            t.commit(owner, window(vec![input.clone()])).unwrap();
            assert!(t.try_node(removed_popup).is_none());
            assert_eq!(t.index_for_id(&[42, 3]), Some(input_index));
            assert_eq!(t.index_for_id(&[42, 5]), Some(settings_index));
            assert_eq!(t.query(&locator).unwrap()[0].get_name(), "Settings");
        }
        let before = t.get_xml_dom_tree().to_owned();
        let mut wrong = input.clone();
        wrong.properties = wrong.properties.with_handle(99);
        assert!(
            t.commit(input_index, wrong)
                .unwrap_err()
                .contains("identity changed")
        );
        assert_eq!(t.get_xml_dom_tree(), before);
        t.commit(owner, window(vec![input.clone(), input])).unwrap();
        assert_eq!(t.indices_for_id(&[42, 3]).len(), 2);
        assert!(
            t.indices_for_id(&[42, 3])
                .iter()
                .all(|&i| !t.node(i).1.identity().is_resolvable())
        );
    }

    #[test]
    fn context_menu_occurrences_survive_narrow_updates_and_independent_removal() {
        fn host(handle: isize) -> Observation {
            Observation {
                properties: SaveUIElement::fixture(10, "PopupHost", "Pane").with_handle(handle),
                children: Some(vec![obs(
                    393,
                    "Popup",
                    Some(vec![obs(397, "Open with", Some(vec![]))]),
                )]),
            }
        }
        let mut t =
            UITree::from_observation(obs(1, "Desktop", Some(vec![host(132898), host(132972)])))
                .unwrap();
        let occurrences = t.indices_for_id(&[42, 397]);
        assert_eq!(occurrences.len(), 2);
        assert!(t.index_for_id(&[42, 397]).is_none());
        assert_ne!(
            t.node(occurrences[0]).1.identity(),
            t.node(occurrences[1]).1.identity()
        );
        let before = t.revision();
        assert!(
            t.commit(occurrences[0], t.observation(occurrences[1]))
                .unwrap_err()
                .contains("ancestry")
        );
        assert_eq!(t.revision(), before);
        for &id in &occurrences {
            let xpath = t.get_xpath_for_element(id, false).unwrap();
            let found = t.query(&xpath).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].identity(), t.node(id).1.identity());
            t.commit(id, obs(397, "Open with", Some(vec![]))).unwrap();
        }
        t.commit(0, obs(1, "Desktop", Some(vec![host(132972)])))
            .unwrap();
        assert!(t.try_node(occurrences[0]).is_none());
        assert!(t.try_node(occurrences[1]).is_some());
        t.commit(0, obs(1, "Desktop", Some(vec![host(132972), host(132972)])))
            .unwrap();
        assert_eq!(t.indices_for_id(&[42, 397]).len(), 2);
        let mut duplicate_in_host = host(132972);
        duplicate_in_host.children.as_mut().unwrap()[0]
            .children
            .as_mut()
            .unwrap()
            .push(obs(397, "Open with", Some(vec![])));
        t.commit(0, obs(1, "Desktop", Some(vec![duplicate_in_host])))
            .unwrap();
        assert_eq!(t.indices_for_id(&[42, 397]).len(), 2);
        // A recycled native handle with a new provider identity is a new occurrence.
        let mut reused = host(132972);
        reused.properties = SaveUIElement::fixture(11, "PopupHost", "Pane").with_handle(132972);
        t.commit(0, obs(1, "Desktop", Some(vec![reused]))).unwrap();
        assert!(t.try_node(occurrences[1]).is_none());
        assert!(t.index_for_id(&[42, 397]).is_some());
    }

    #[test]
    fn outlook_rows_preserve_repeated_fields_across_reorder_and_removal() {
        let row = |id| {
            obs(
                id,
                "Row",
                Some(vec![obs(-11, "With Attachments", Some(vec![]))]),
            )
        };
        let table = |rows| Observation {
            properties: SaveUIElement::fixture(329604, "Table View", "Table").with_handle(329604),
            children: Some(vec![obs(-1174428, "Today", Some(rows))]),
        };
        let mut t = UITree::from_observation(table(vec![row(-54), row(-56)])).unwrap();
        let fields = t.indices_for_id(&[42, -11]);
        assert_eq!(fields.len(), 2);
        assert!(t.index_for_id(&[42, -11]).is_none());
        let identities: Vec<_> = fields.iter().map(|&i| t.node(i).1.identity()).collect();
        assert_ne!(identities[0], identities[1]);
        for &i in &fields {
            let xpath = t.get_xpath_for_element(i, false).unwrap();
            let found = t.query(&xpath).unwrap();
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].identity(), t.node(i).1.identity());
        }
        t.commit(0, table(vec![row(-56), row(-54)])).unwrap();
        for (&i, identity) in fields.iter().zip(&identities) {
            assert_eq!(t.index_for_identity(identity), Some(i));
            t.commit(i, obs(-11, "With Attachments", Some(vec![])))
                .unwrap();
        }
        assert!(t.commit(fields[0], t.observation(fields[1])).is_err());
        t.commit(0, table(vec![row(-56)])).unwrap();
        assert!(t.try_node(fields[0]).is_none());
        assert_eq!(t.index_for_identity(&identities[1]), Some(fields[1]));
        let duplicate_row = obs(
            -56,
            "Row",
            Some(vec![
                obs(-11, "With Attachments", None),
                obs(-11, "With Attachments", None),
            ]),
        );
        t.commit(0, table(vec![duplicate_row])).unwrap();
        assert_eq!(t.indices_for_id(&[42, -11]).len(), 2);
        assert!(
            t.indices_for_id(&[42, -11])
                .iter()
                .all(|&i| !t.node(i).1.identity().is_resolvable())
        );
    }

    #[test]
    fn invalid_patch_rolls_back() {
        let mut t = tree();
        let xml = t.get_xml_dom_tree().to_string();
        let rev = t.revision();
        assert!(t.commit(0, obs(9, "wrong", None)).is_err());
        assert_eq!(t.revision(), rev);
        assert_eq!(t.get_xml_dom_tree(), xml);
        assert!(t.commit(0, obs(9, "wrong", None)).is_err());
    }
    #[test]
    fn collision_reports_both_paths_and_preserves_committed_tree() {
        let t = tree();
        let revision = t.revision();
        let xml = t.get_xml_dom_tree().to_owned();
        let observation = obs(
            1,
            "Desktop",
            Some(vec![
                obs(8, "Left", Some(vec![obs(9, "First", None)])),
                obs(10, "Right", Some(vec![obs(9, "Second\nname", None)])),
            ]),
        );
        let error =
            UITree::validate_observation(&observation, &observation, &mut HashMap::new(), 0)
                .unwrap_err();
        assert!(error.contains("runtime_id=[42, 9]"));
        let (first, second) = error.split_once("; second=").unwrap();
        assert!(first.contains("name=\"Left\""));
        assert!(first.contains("name=\"First\" control_type=\"Button\""));
        assert!(!first.contains("name=\"Right\""));
        assert!(second.contains("child=2 name=\"Right\""));
        assert!(second.contains("name=\"Second\\nname\" control_type=\"Button\""));
        assert!(
            !error.contains('\n'),
            "names must be escaped for single-line logs"
        );
        for path in [first, second] {
            assert!(path.contains("name=\"Desktop\" control_type=\"Pane\""));
        }
        assert_eq!(t.revision(), revision);
        assert_eq!(t.get_xml_dom_tree(), xml);
    }

    #[test]
    fn identical_siblings_have_distinct_diagnostic_positions() {
        let observation = obs(
            1,
            "Desktop",
            Some(vec![obs(2, "Same", None), obs(2, "Same", None)]),
        );
        let error =
            UITree::validate_observation(&observation, &observation, &mut HashMap::new(), 0)
                .unwrap_err();
        assert!(error.contains("child=1 name=\"Same\""));
        assert!(error.contains("child=2 name=\"Same\""));
    }

    #[test]
    fn ancestor_collision_includes_root_and_descendant() {
        let observation = obs(1, "Desktop", Some(vec![obs(1, "Descendant", None)]));
        let error =
            UITree::validate_observation(&observation, &observation, &mut HashMap::new(), 0)
                .unwrap_err();
        let (first, second) = error.split_once("; second=").unwrap();
        assert!(!first.contains(" -> "));
        assert!(second.contains(" -> [child=1 name=\"Descendant\""));
    }
    #[test]
    fn properties_preserve_descendants_and_order_is_canonical() {
        let mut t = tree();
        let a = t.index_for_id(&[42, 2]).unwrap();
        t.commit(a, obs(2, "new", None)).unwrap();
        assert!(t.index_for_id(&[42, 3]).is_some());
        let mut o = t.observation(0);
        o.children.as_mut().unwrap().reverse();
        t.commit(0, o).unwrap();
        assert_eq!(t.query("/Pane/Button[1]").unwrap()[0].get_name(), "B");
        assert_eq!(t.node(t.children(0)[0]).1.get_name(), "B");
    }
    #[test]
    fn missing_identity_and_reparent_are_rejected() {
        let mut t = tree();
        let rev = t.revision();
        let mut o = obs(1, "Desktop", None);
        o.properties = SaveUIElement::default();
        assert!(t.commit(0, o).is_err());
        let b = t.index_for_id(&[42, 4]).unwrap();
        assert!(
            t.commit(b, obs(4, "B", Some(vec![obs(3, "child", None)])))
                .is_err()
        );
        assert_eq!(t.revision(), rev);
    }
    #[test]
    fn invalid_xpath_is_not_absence_and_union_is_correct() {
        let t = tree();
        assert!(t.query("//[").is_err());
        assert_eq!(
            t.query("//Button[@Name='A'] | //Button[@Name='B']")
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn property_patch_preserves_membership_age() {
        let mut t = tree();
        let before = t.coverage(1).unwrap().children_observed_at;
        t.commit(1, obs(2, "renamed", None)).unwrap();
        assert_eq!(t.coverage(1).unwrap().children_observed_at, before);
    }
    #[test]
    fn churn_reclaims_storage_and_rejects_removed_targets() {
        let mut t = tree();
        let removed = t.index_for_id(&[42, 3]).unwrap();
        for id in 10..510 {
            t.commit(
                1,
                obs(2, "A", Some(vec![obs(id, "replacement", Some(vec![]))])),
            )
            .unwrap();
            assert_eq!(t.get_tree().node_count(), 4);
            assert_eq!(t.coverage.len(), 4);
            assert_eq!(t.get_elements().len(), 4);
        }
        let revision = t.revision();
        assert!(t.commit(removed, obs(3, "late", None)).is_err());
        assert_eq!(t.revision(), revision);
    }
}
