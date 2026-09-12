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
        self.tree
            .get_element_by_runtime_id(&format_runtime_id(id))
            .map(|n| n.index)
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
    pub fn commit(&mut self, target: usize, observation: Observation) -> Result<(), String> {
        let started = Instant::now();
        if !self.tree.has_node(target) {
            return Err("Capture target was removed".into());
        }
        let mut ids = HashSet::new();
        Self::validate_observation(&observation, &mut ids, 0)?;
        if self.initialized
            && self.tree.node(target).data.get_runtime_id()
                != observation.properties.get_runtime_id()
        {
            return Err("Capture target identity changed".into());
        }
        let mut candidate = self.clone();
        if !candidate.initialized {
            candidate.tree = UITreeMap::new(
                String::new(),
                format_runtime_id(observation.properties.get_runtime_id()),
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

    fn validate_observation(
        node: &Observation,
        ids: &mut HashSet<Vec<i32>>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 256 {
            return Err("Capture depth limit exceeded".into());
        }
        let id = node.properties.get_runtime_id();
        if id.is_empty() || !ids.insert(id.to_vec()) {
            return Err("Missing or duplicate runtime ID".into());
        }
        if let Some(children) = &node.children {
            for child in children {
                Self::validate_observation(child, ids, depth + 1)?;
            }
        }
        Ok(())
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
            let wanted: HashSet<Vec<i32>> = children
                .iter()
                .map(|c| c.properties.get_runtime_id().to_vec())
                .collect();
            for old in self.children(index).to_vec() {
                if !wanted.contains(self.tree.node(old).data.get_runtime_id()) {
                    self.remove_branch(old)?;
                }
            }
            let mut order = Vec::new();
            for child in children {
                let id = child.properties.get_runtime_id();
                let child_index = match self.index_for_id(id) {
                    Some(existing) => {
                        if self.tree.node(existing).parent != index {
                            return Err(
                                "Reparent requires reconciliation of both parent contexts".into()
                            );
                        }
                        existing
                    }
                    None => self.tree.add_child(
                        index,
                        "",
                        &format_runtime_id(id),
                        child.properties.clone(),
                    ),
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
            node.runtime_id
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
        start.push_attribute(("RtID", node.runtime_id.as_str()));
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
            .map(|(_, p)| format_runtime_id(p.get_runtime_id()))
            .unwrap_or_default();
        xmlutil::xpath_gen::get_xpath_full_from_runtime_id(&id, &self.xml_dom_tree, simple)
    }
    pub fn query(&self, xpath: &str) -> Result<Vec<&SaveUIElement>, String> {
        // Parenthesize expressions to preserve union and predicate semantics.
        let expr = if xpath.trim_end().ends_with("/@RtID") {
            xpath.to_string()
        } else {
            format!("({xpath})/@RtID")
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
        let observation = subtree.observation(0);
        if let Some(existing) = self.index_for_id(observation.properties.get_runtime_id()) {
            if self.tree.node(existing).parent != parent {
                return Err("Subtree parent mismatch".into());
            }
            self.commit(existing, observation)?;
            Ok(existing)
        } else {
            if !self.coverage(parent).is_some_and(|c| c.children_observed) {
                return Err("Parent membership not observed".into());
            }
            let id = observation.properties.get_runtime_id().to_vec();
            let mut parent_obs = self.observation(parent);
            parent_obs
                .children
                .as_mut()
                .ok_or("Missing parent children")?
                .push(observation);
            self.commit(parent, parent_obs)?;
            self.index_for_id(&id)
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
    use super::*;
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
    fn invalid_patch_rolls_back() {
        let mut t = tree();
        let xml = t.get_xml_dom_tree().to_string();
        let rev = t.revision();
        assert!(
            t.commit(
                0,
                obs(
                    1,
                    "Desktop",
                    Some(vec![obs(4, "x", None), obs(4, "y", None)])
                )
            )
            .is_err()
        );
        assert_eq!(t.revision(), rev);
        assert_eq!(t.get_xml_dom_tree(), xml);
        assert!(t.commit(0, obs(9, "wrong", None)).is_err());
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
