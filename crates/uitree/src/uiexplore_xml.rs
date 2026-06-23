use crate::common_types::UIElementInTree;
use crate::error::UITreeError;

use crate::save_ui_element::SaveUIElement;
use bromium_common::{format_runtime_id, get_ui_automation_instance};

use crate::UITreeMap;
use xmlutil::XpathQueryResult;
use xmlutil::xpath_eval::eval_xpath;
use xmlutil::xpath_gen::get_xpath_full_from_runtime_id;

use quick_xml::Writer;
use quick_xml::events::{BytesEnd, BytesStart, Event};
use std::collections::HashSet;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};

use uiautomation::{UIElement, UITreeWalker};

use log::{debug, error, info, trace, warn};

/// Hard upper bound on sibling iterations to prevent infinite loops
/// from COM UIAutomation cycles (e.g. crashed or hung processes).
const MAX_SIBLINGS: usize = 10_000;

#[derive(Debug, Clone)]
pub struct UITree {
    tree: UITreeMap<()>,
    xml_dom_tree: String,
    ui_elements: Vec<UIElementInTree>,
    node_to_elem: Vec<usize>,
}

impl UITree {
    pub fn new(
        tree: UITreeMap<()>,
        xml_dom_tree: String,
        ui_elements: Vec<UIElementInTree>,
    ) -> Self {
        let node_to_elem = Self::build_node_to_elem(&tree, &ui_elements);
        UITree {
            tree,
            xml_dom_tree,
            ui_elements,
            node_to_elem,
        }
    }

    fn build_node_to_elem(tree: &UITreeMap<()>, elements: &[UIElementInTree]) -> Vec<usize> {
        let mut rtid_to_pos: crate::UIHashMap<String, usize> = crate::UIHashMap::default();
        for (pos, elem) in elements.iter().enumerate() {
            let rtid = format_runtime_id(elem.get_element_props().get_runtime_id());
            rtid_to_pos.insert(rtid, pos);
        }
        let mut map = vec![0; tree.node_count()];
        for (i, slot) in map.iter_mut().enumerate() {
            // Skip dead (tombstone) nodes — leave their mapping at 0
            if !tree.node(i).is_alive {
                continue;
            }
            if let Some(&pos) = rtid_to_pos.get(&tree.node(i).runtime_id) {
                *slot = pos;
            }
        }
        map
    }

    fn rebuild_node_to_elem(&mut self) {
        self.node_to_elem = Self::build_node_to_elem(&self.tree, &self.ui_elements);
    }

    pub fn get_tree(&self) -> &UITreeMap<()> {
        &self.tree
    }

    pub fn get_tree_mut(&mut self) -> &mut UITreeMap<()> {
        &mut self.tree
    }

    pub fn get_xml_dom_tree(&self) -> &str {
        &self.xml_dom_tree
    }

    pub fn get_elements(&self) -> &[UIElementInTree] {
        &self.ui_elements
    }

    pub fn get_elements_mut(&mut self) -> &mut Vec<UIElementInTree> {
        &mut self.ui_elements
    }

    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(usize, &SaveUIElement),
    {
        self.tree.for_each(|idx, _| {
            let elem_pos = self.node_to_elem[idx];
            f(idx, self.ui_elements[elem_pos].get_element_props());
        });
    }

    pub fn root(&self) -> usize {
        self.tree.root()
    }

    pub fn children(&self, index: usize) -> &[usize] {
        self.tree.children(index)
    }

    pub fn node(&self, index: usize) -> (&str, &SaveUIElement) {
        let node = self.tree.node(index);
        let elem_pos = self.node_to_elem[index];
        (&node.name, self.ui_elements[elem_pos].get_element_props())
    }

    pub fn pretty_print_tree(&self) {
        self.debug_tree(self.root(), 0, 0);
    }

    fn debug_tree(&self, index: usize, indent: usize, depth: usize) {
        if depth > self.tree.node_count() {
            println!(
                "{}(Max depth exceeded at node {})",
                " ".repeat(indent),
                index
            );
            return;
        }

        let node = &self.tree.nodes()[index];
        // Skip dead (tombstone) nodes
        if !node.is_alive {
            return;
        }
        let prefix = " ".repeat(indent);
        let elem_pos = self.node_to_elem[index];
        let elem = self.ui_elements[elem_pos].get_element_props();
        println!("{}{}: {}", prefix, &node.name, elem);

        for &child in &node.children {
            self.debug_tree(child, indent + 2, depth + 1);
        }
    }

    pub fn get_xpath_for_element(
        &self,
        index: usize,
        simple_path: bool,
    ) -> Result<String, xmlutil::xpath_gen::XpathGenError> {
        let node = self.tree.node(index);
        get_xpath_full_from_runtime_id(&node.runtime_id, self.get_xml_dom_tree(), simple_path)
    }

    pub fn get_element_by_xpath(&self, xpath: &str) -> Option<&SaveUIElement> {
        let xpath = if !xpath.ends_with("/@RtID") {
            xpath.to_string() + "/@RtID"
        } else {
            xpath.to_string()
        };

        let xpath_result = eval_xpath(&xpath, self.get_xml_dom_tree());

        match xpath_result.get_result_count() {
            0 => None,
            1 => {
                let items = xpath_result.get_result_items();
                let default_result = XpathQueryResult::default();
                let itm = items.first().unwrap_or(&default_result);
                let runtime_id = itm.get_item_value();
                let node = self.get_tree().get_element_by_runtime_id(runtime_id)?;
                let elem_pos = self.node_to_elem[node.index];
                Some(self.ui_elements[elem_pos].get_element_props())
            }
            _ => {
                warn!(
                    "Warning: XPath expression returned {} results, expected only 1 result. Returning the first result.",
                    xpath_result.get_result_count()
                );
                let items = xpath_result.get_result_items();
                let default_result = XpathQueryResult::default();
                let itm = items.first().unwrap_or(&default_result);
                let runtime_id = itm.get_item_value();
                let node = self.get_tree().get_element_by_runtime_id(runtime_id)?;
                let elem_pos = self.node_to_elem[node.index];
                Some(self.ui_elements[elem_pos].get_element_props())
            }
        }
    }

    pub fn get_elements_by_xpath(&self, xpath: &str) -> Option<Vec<&SaveUIElement>> {
        let xpath = if !xpath.ends_with("/@RtID") {
            xpath.to_string() + "/@RtID"
        } else {
            xpath.to_string()
        };

        let xpath_result = eval_xpath(&xpath, self.get_xml_dom_tree());
        let mut results: Vec<&SaveUIElement> = Vec::new();
        match xpath_result.get_result_count() {
            0 => None,
            1 => {
                let items = xpath_result.get_result_items();
                let default_result = &XpathQueryResult::default();
                let itm = items.first().unwrap_or(default_result);
                let runtime_id = itm.get_item_value();
                let node = self.get_tree().get_element_by_runtime_id(runtime_id)?;
                let elem_pos = self.node_to_elem[node.index];
                results.push(self.ui_elements[elem_pos].get_element_props());
                Some(results)
            }
            _ => {
                let items = xpath_result.get_result_items();
                for itm in items {
                    let runtime_id = itm.get_item_value();
                    if let Some(node) = self.get_tree().get_element_by_runtime_id(runtime_id) {
                        let elem_pos = self.node_to_elem[node.index];
                        results.push(self.ui_elements[elem_pos].get_element_props());
                    } else {
                        warn!(
                            "Element with runtime_id '{}' not found in tree, skipping",
                            runtime_id
                        );
                    }
                }
                if results.is_empty() {
                    return None;
                }
                Some(results)
            }
        }
    }
}

impl UITree {
    pub fn append_or_replace_subtree(
        &mut self,
        parent_index: usize,
        mut subtree: UITree,
    ) -> Result<usize, String> {
        trace!("Parent index to append subtree: {}", parent_index);
        trace!(
            "Appending or replacing subtree with root: {}",
            subtree.get_tree().node(subtree.root()).name
        );
        let subtree_root = subtree.root();
        let subtree_node = subtree.get_tree().node(subtree_root);
        let subtree_runtime_id = subtree_node.runtime_id.clone();
        let subtree_name = subtree_node.name.clone();

        if !self.get_tree().has_node(parent_index) {
            error!(
                "Parent index {} does not exist in the current tree",
                parent_index
            );
            return Err("Parent index does not exist in the current tree".to_string());
        }

        if let Some(existing_node) = self
            .get_tree()
            .get_element_by_runtime_id(&subtree_runtime_id)
        {
            let existing_node_index = existing_node.index;
            debug!(
                "Subtree root already exists in the current tree at index {}. Replacing existing subtree.",
                existing_node_index
            );
            self.get_tree_mut()
                .remove_node(existing_node_index)
                .map_err(|e| e.to_string())?;
        }

        let tree_mut = self.get_tree_mut();
        let new_index = tree_mut.add_child(parent_index, &subtree_name, &subtree_runtime_id, ());
        debug!("Added subtree root to current tree at index {}", new_index);

        remove_in_place(self.get_elements_mut(), subtree.get_elements_mut());

        self.get_elements_mut().append(subtree.get_elements_mut());

        info!("Sorting UI elements by z-order and size...");
        self.get_elements_mut().sort_unstable_by(|a, b| {
            a.get_element_props()
                .get_z_order()
                .cmp(&b.get_element_props().get_z_order())
                .then(
                    a.get_element_props()
                        .get_bounding_rect_size()
                        .cmp(&b.get_element_props().get_bounding_rect_size()),
                )
        });

        self.append_children(new_index, &mut subtree, subtree_root)?;

        let current_xml_dom_tree = self.get_xml_dom_tree();
        let subtree_xml_dom_tree = subtree.get_xml_dom_tree();
        info!(
            "Merging XML DOM trees... adding new subtree: {}",
            subtree_xml_dom_tree
        );
        let new_xml_dom_tree = append_or_replace_node_by_rt_id(
            current_xml_dom_tree,
            subtree_xml_dom_tree,
            &subtree_runtime_id,
        )?;
        self.xml_dom_tree = new_xml_dom_tree;

        self.rebuild_node_to_elem();

        Ok(new_index)
    }

    fn append_children(
        &mut self,
        parent_index: usize,
        subtree: &mut UITree,
        subtree_index: usize,
    ) -> Result<(), String> {
        let children = subtree.get_tree().children(subtree_index).to_vec();
        debug!(
            "Appending {} children to parent index {}",
            children.len(),
            parent_index
        );
        for child_index in children {
            let child_node = subtree.get_tree().node(child_index);
            let child_runtime_id = child_node.runtime_id.clone();
            let child_name = child_node.name.clone();

            let new_child_index =
                self.get_tree_mut()
                    .add_child(parent_index, &child_name, &child_runtime_id, ());

            self.append_children(new_child_index, subtree, child_index)?;
        }
        Ok(())
    }
}

fn remove_in_place(orig: &mut Vec<UIElementInTree>, check: &[UIElementInTree]) {
    let set: HashSet<_> = check.iter().cloned().collect();
    orig.retain(|x| !set.contains(x));
}

fn append_or_replace_node_by_rt_id(
    current_xml_dom_tree: &str,
    xml_dom_subtree: &str,
    target_node_rt_id: &str,
) -> Result<String, String> {
    let target = target_node_rt_id.to_string();

    let mut xot = xot::Xot::new();
    let root = xot
        .parse(current_xml_dom_tree)
        .map_err(|e| format!("Failed to parse current XML: {}", e))?;
    let doc = xot
        .document_element(root)
        .map_err(|e| format!("Failed to get document element: {}", e))?;

    if let Some(existing_node) = find_node_by_rt_id(&mut xot, doc, &target) {
        let new_subtree = xot
            .parse(xml_dom_subtree)
            .map_err(|e| format!("Failed to parse subtree XML: {}", e))?;
        let new_subtree_doc = xot
            .document_element(new_subtree)
            .map_err(|e| format!("Failed to get subtree document element: {}", e))?;
        xot.replace(existing_node, new_subtree_doc)
            .map_err(|e| format!("Failed to replace node: {}", e))?;

        xot.serialize_xml_string(Default::default(), root)
            .map_err(|e| format!("Failed to serialize XML: {}", e))
    } else {
        let new_node = xot
            .parse(xml_dom_subtree)
            .map_err(|e| format!("Failed to parse subtree XML: {}", e))?;
        let new_node_doc = xot
            .document_element(new_node)
            .map_err(|e| format!("Failed to get subtree document element: {}", e))?;
        xot.append(doc, new_node_doc)
            .map_err(|e| format!("Failed to append node: {}", e))?;

        xot.serialize_xml_string(Default::default(), root)
            .map_err(|e| format!("Failed to serialize XML: {}", e))
    }
}

fn find_node_by_rt_id(xot: &mut xot::Xot, doc: xot::Node, target: &String) -> Option<xot::Node> {
    let rt_id_a = xot.add_name("RtID");
    let descendants = xot.descendants(doc);
    let rt_id_default = "n/a".to_string();
    for desc in descendants {
        let desc_attrs = xot.attributes(desc);
        let rt_id = desc_attrs.get(rt_id_a).unwrap_or(&rt_id_default);
        if rt_id == target {
            return Some(desc);
        }
    }
    None
}

pub fn get_all_elements_xml(
    tx: Sender<Result<UITree, UITreeError>>,
    root_element: Option<SaveUIElement>,
    max_depth: Option<usize>,
    calling_window_caption: Option<String>,
    target_window_caption: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) {
    info!(
        "Starting UI element retrieval with max depth: {:?} and window title filters: calling_window_caption='{}', target_window_caption='{}'",
        max_depth,
        calling_window_caption.as_deref().unwrap_or("none"),
        target_window_caption.as_deref().unwrap_or("none")
    );
    let automation = match get_ui_automation_instance() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to create UIAutomation instance: {}", e);
            let _ = tx.send(Err(UITreeError::NoUIAutomation));
            return;
        }
    };
    let walker = match automation.get_control_view_walker() {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to get control view walker: {}", e);
            let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
            return;
        }
    };

    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    let mut xml_writer = Writer::new(Cursor::new(Vec::new()));

    let root = if let Some(elem) = root_element {
        match elem.get_ui_automation_ui_element() {
            Some(e) => e,
            None => {
                error!("Failed to resolve root UIElement from SaveUIElement");
                let _ = tx.send(Err(UITreeError::UIAutomation(
                    "Failed to resolve root UIElement from SaveUIElement".to_string(),
                )));
                return;
            }
        }
    } else {
        match automation.get_root_element() {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to get root element: {}", e);
                let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
                return;
            }
        }
    };

    let ui_elem_props = SaveUIElement::new(&root, 0, 999);
    let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
    let item = format!(
        "'{}' {} ({} | {} | {})",
        ui_elem_props.get_name(),
        ui_elem_props.get_control_type(),
        ui_elem_props.get_classname(),
        ui_elem_props.get_framework_id(),
        runtime_id
    );
    let mut tree = UITreeMap::new(item, runtime_id.clone(), ());

    let mut tree_path = ui_elem_props.get_name().to_string();

    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);
    ui_elements.push(ui_elem_in_tree);

    if let Ok(_first_child) = walker.get_first_child(&root) {
        get_element(
            &mut tree,
            &mut ui_elements,
            0,
            &walker,
            &root,
            &mut xml_writer,
            0,
            0,
            max_depth,
            calling_window_caption.as_deref(),
            target_window_caption.as_deref(),
            &mut tree_path,
            cancel.as_ref(),
        );
    }

    // Check cancellation before sending results
    if cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) {
        info!("Tree construction cancelled, discarding partial results");
        let _ = tx.send(Err(UITreeError::Cancelled));
        return;
    }

    let xml_dom_tree = String::from_utf8(xml_writer.into_inner().into_inner()).unwrap_or_default();

    info!("Sorting UI elements by z-order and size...");
    ui_elements.sort_unstable_by(|a, b| {
        a.get_element_props()
            .get_z_order()
            .cmp(&b.get_element_props().get_z_order())
            .then(
                a.get_element_props()
                    .get_bounding_rect_size()
                    .cmp(&b.get_element_props().get_bounding_rect_size()),
            )
    });

    let ui_tree = UITree::new(tree, xml_dom_tree, ui_elements);

    info!(
        "Sending UI tree with {} elements to the main thread...",
        ui_tree.get_elements().len()
    );
    match tx.send(Ok(ui_tree)) {
        Ok(_) => {
            info!("UI tree sent successfully.");
        }
        Err(e) => {
            error!("Error sending UI tree: {:?}", e);
        }
    };
}

pub fn get_all_elements_par_xml(
    tx: Sender<Result<UITree, UITreeError>>,
    max_depth: Option<usize>,
    calling_window_caption: Option<String>,
    target_window_caption: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) {
    info!(
        "Starting parallel UI element retrieval with max depth: {:?} and window title filters: calling_window_caption='{}', target_window_caption='{}'",
        max_depth,
        calling_window_caption.as_deref().unwrap_or("none"),
        target_window_caption.as_deref().unwrap_or("none")
    );
    let automation = match get_ui_automation_instance() {
        Ok(a) => a,
        Err(e) => {
            error!("Failed to create UIAutomation instance: {}", e);
            let _ = tx.send(Err(UITreeError::NoUIAutomation));
            return;
        }
    };
    let walker = match automation.get_control_view_walker() {
        Ok(w) => w,
        Err(e) => {
            error!("Failed to get control view walker: {}", e);
            let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
            return;
        }
    };

    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    let mut xml_writer = Writer::new(Cursor::new(Vec::new()));

    let root = match automation.get_root_element() {
        Ok(e) => e,
        Err(e) => {
            error!("Failed to get root element: {}", e);
            let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
            return;
        }
    };
    let ui_elem_props = SaveUIElement::new(&root, 0, 999);
    let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
    let item = format!(
        "'{}' {} ({} | {} | {})",
        ui_elem_props.get_name(),
        ui_elem_props.get_localized_control_type(),
        ui_elem_props.get_classname(),
        ui_elem_props.get_framework_id(),
        runtime_id
    );
    let mut tree = UITreeMap::new(item, runtime_id.clone(), ());

    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);
    ui_elements.push(ui_elem_in_tree);

    let mut tree_path = String::new();

    if let Ok(_first_child) = walker.get_first_child(&root) {
        get_element(
            &mut tree,
            &mut ui_elements,
            0,
            &walker,
            &root,
            &mut xml_writer,
            0,
            0,
            Some(1_usize),
            calling_window_caption.as_deref(),
            target_window_caption.as_deref(),
            &mut tree_path,
            cancel.as_ref(),
        );
    }

    let xml_dom_tree = String::from_utf8(xml_writer.into_inner().into_inner()).unwrap_or_default();

    info!("Sorting UI elements by z-order and size...");
    ui_elements.sort_unstable_by(|a, b| {
        a.get_element_props()
            .get_z_order()
            .cmp(&b.get_element_props().get_z_order())
            .then(
                a.get_element_props()
                    .get_bounding_rect_size()
                    .cmp(&b.get_element_props().get_bounding_rect_size()),
            )
    });

    let mut ui_tree = UITree::new(tree, xml_dom_tree, ui_elements);
    debug!(
        "This is the top level tree we are processing:\n{}",
        ui_tree.get_xml_dom_tree()
    );

    let root_idx = ui_tree.get_tree().root();
    let root_first_child_idx = match ui_tree.get_tree().children(root_idx).first() {
        None => {
            warn!("No child elements found under the root element. Sending empty UI tree.");
            match tx.send(Ok(ui_tree.clone())) {
                Ok(_) => {
                    info!("UI tree sent successfully.");
                }
                Err(e) => {
                    error!("Error sending UI tree: {:?}", e);
                }
            };
            return;
        }
        Some(val) => *val,
    };

    let child_indices = ui_tree.get_tree().children(root_first_child_idx);
    let mut child_elements = Vec::new();
    trace!("children to process in parallel: {}", child_indices.len());
    for &child_index in child_indices {
        let elem_pos = ui_tree.node_to_elem[child_index];
        let child_save_ui_elem = ui_tree.ui_elements[elem_pos].get_element_props();
        child_elements.push(child_save_ui_elem.clone());
    }

    let child_count = child_elements.len();
    let (tx_par, rx_par) = channel::<Result<UITree, UITreeError>>();
    let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();
    for element in child_elements {
        let tx_par_clone = tx_par.clone();
        let calling_window_caption_n = calling_window_caption.clone();
        let target_window_caption_n = target_window_caption.clone();
        let cancel_clone = cancel.clone();
        debug!(
            "Spawning thread to process element: '{}'",
            element.get_name()
        );
        let handle = std::thread::spawn(move || {
            get_all_elements_xml(
                tx_par_clone,
                Some(element),
                max_depth,
                calling_window_caption_n,
                target_window_caption_n,
                cancel_clone,
            );
        });
        handles.push(handle);
    }
    drop(tx_par);

    debug!("Collecting subtrees from {} threads...", child_count);
    let mut subtrees = Vec::new();
    for _ in 0..child_count {
        match rx_par.recv() {
            Ok(Ok(subtree)) => subtrees.push(subtree),
            Ok(Err(e)) => {
                error!("Subtree build failed: {}", e);
                let _ = tx.send(Err(e));
                return;
            }
            Err(e) => {
                error!("Failed to receive subtree from thread: {}", e);
                let _ = tx.send(Err(UITreeError::ChannelRecv(e.to_string())));
                return;
            }
        }
    }

    trace!("Waiting for all threads to complete...");
    for handle in handles {
        if let Err(e) = handle.join() {
            error!("Thread panicked: {:?}", e);
        }
    }

    debug!("Appending {} subtrees to the main tree...", subtrees.len());
    for subtree in subtrees {
        match ui_tree.append_or_replace_subtree(ui_tree.get_tree().root(), subtree) {
            Ok(_) => {}
            Err(e) => {
                error!("Error appending subtree: {}", e);
            }
        }
        debug!("UI tree has now {} elements", ui_tree.get_elements().len());
    }

    info!(
        "Sending UI tree with {} elements to the main thread...",
        ui_tree.get_elements().len()
    );
    match tx.send(Ok(ui_tree)) {
        Ok(_) => {
            info!("UI tree sent successfully.");
        }
        Err(e) => {
            error!("Error sending UI tree: {:?}", e);
        }
    };
}

#[allow(clippy::too_many_arguments)]
fn get_element(
    tree: &mut UITreeMap<()>,
    ui_elements: &mut Vec<UIElementInTree>,
    parent: usize,
    walker: &UITreeWalker,
    element: &UIElement,
    xml_writer: &mut Writer<Cursor<Vec<u8>>>,
    level: usize,
    mut z_order: usize,
    max_depth: Option<usize>,
    calling_window_caption: Option<&str>,
    target_window_caption: Option<&str>,
    tree_path: &mut String,
    cancel: Option<&Arc<AtomicBool>>,
) {
    // Check cancellation flag before processing each element
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return;
    }

    if let Some(limit) = max_depth
        && level > limit
    {
        return;
    }

    let element_count = ui_elements.len();
    if element_count.is_multiple_of(100) {
        info!("Processed {} UI elements so far...", element_count);
    }

    if let Some(caption) = calling_window_caption
        && let Ok(name) = element.get_name()
        && name == caption
    {
        trace!("Skipping element with caption: {}", caption);
        return;
    }
    let prev_tree_path_len = tree_path.len();

    if level > 0 {
        let name = if element
            .get_name()
            .unwrap_or("Unnamed".to_string())
            .is_empty()
        {
            "Unnamed".to_string()
        } else {
            element.get_name().unwrap_or("Unnamed".to_string())
        };

        if tree_path.is_empty() {
            tree_path.push_str(&name);
        } else {
            tree_path.push('\\');
            tree_path.push_str(&name);
        }
        trace!("Current tree path: {}", tree_path);
        if let Some(target_caption) = target_window_caption
            && !tree_path.contains(target_caption)
        {
            trace!(
                "Skipping element with caption: {} in tree path {}, looking for target caption: {}",
                name, tree_path, target_caption
            );
            tree_path.truncate(prev_tree_path_len);
            return;
        }
    }

    let effective_z_order = if level == 0 { 999 } else { z_order };
    let ui_elem_props = SaveUIElement::new(element, level, effective_z_order);
    let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
    let item = format!(
        "'{}' {} ({} | {} | {})",
        ui_elem_props.get_name(),
        ui_elem_props.get_localized_control_type(),
        ui_elem_props.get_classname(),
        ui_elem_props.get_framework_id(),
        runtime_id
    );

    let parent = tree.add_child(parent, item.as_str(), runtime_id.as_str(), ());

    let control_type_tag = if ui_elem_props.get_control_type().is_empty() {
        "Unknown".to_string()
    } else {
        ui_elem_props.get_control_type().to_string()
    };
    let mut start = BytesStart::new(&control_type_tag);
    start.push_attribute(("RtID", runtime_id.as_str()));
    let z_order_str = effective_z_order.to_string();
    start.push_attribute(("z-order", z_order_str.as_str()));
    start.push_attribute(("Name", ui_elem_props.get_name()));
    if ui_elem_props.get_control_type().is_empty() {
        start.push_attribute(("ControlType", "No control type defined"));
    } else {
        start.push_attribute(("ControlType", ui_elem_props.get_control_type()));
    }
    if let Err(e) = xml_writer.write_event(Event::Start(start)) {
        error!(
            "Failed to write XML start event for '{}': {}",
            control_type_tag, e
        );
        return;
    }

    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, parent);
    ui_elements.push(ui_elem_in_tree);

    if let Ok(child) = walker.get_first_child(element) {
        trace!(
            "Found child element: {}",
            child.get_name().unwrap_or("Unknown".to_string())
        );
        get_element(
            tree,
            ui_elements,
            parent,
            walker,
            &child,
            xml_writer,
            level + 1,
            z_order,
            max_depth,
            calling_window_caption,
            target_window_caption,
            tree_path,
            cancel,
        );
        let mut next = child;
        let mut sibling_count: usize = 0;
        while let Ok(sibling) = walker.get_next_sibling(&next) {
            sibling_count += 1;
            if sibling_count > MAX_SIBLINGS {
                warn!(
                    "Sibling loop exceeded {MAX_SIBLINGS} iterations at depth {}, breaking to prevent infinite loop",
                    level + 1
                );
                break;
            }
            // Check cancellation in sibling loop
            if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                return;
            }
            if level + 1 == 1 {
                z_order += 1;
            }
            trace!(
                "Found sibling element: {}",
                sibling.get_name().unwrap_or("Unknown".to_string())
            );
            get_element(
                tree,
                ui_elements,
                parent,
                walker,
                &sibling,
                xml_writer,
                level + 1,
                z_order,
                max_depth,
                calling_window_caption,
                target_window_caption,
                tree_path,
                cancel,
            );
            next = sibling;
        }
    }

    if let Err(e) = xml_writer.write_event(Event::End(BytesEnd::new(&control_type_tag))) {
        error!(
            "Failed to write XML end event for '{}': {}",
            control_type_tag, e
        );
    }
    tree_path.truncate(prev_tree_path_len);
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_XML: &str = r#"<Window RtID="1.2.3" Name="MainWindow" ControlType="Window" z-order="999">
  <Panel RtID="4.5.6" Name="Header" ControlType="Panel" z-order="0">
    <Button RtID="7.8.9" Name="OK" ControlType="Button" z-order="0"/>
    <Button RtID="10.11.12" Name="Cancel" ControlType="Button" z-order="1"/>
  </Panel>
  <Panel RtID="13.14.15" Name="Content" ControlType="Panel" z-order="1">
    <Edit RtID="16.17.18" Name="Username" ControlType="Edit" z-order="0"/>
  </Panel>
</Window>"#;

    fn build_test_tree() -> UITree {
        let mut tree = UITreeMap::new("MainWindow".to_string(), "1.2.3".to_string(), ());

        let header = tree.add_child(0, "Header", "4.5.6", ());
        tree.add_child(header, "OK", "7.8.9", ());
        tree.add_child(header, "Cancel", "10.11.12", ());

        let content = tree.add_child(0, "Content", "13.14.15", ());
        tree.add_child(content, "Username", "16.17.18", ());

        let mut elements = Vec::new();
        for i in 0..tree.node_count() {
            let elem = SaveUIElement::default();
            elements.push(UIElementInTree::new(elem, i));
        }

        UITree::new(tree, TEST_XML.to_string(), elements)
    }

    #[test]
    fn test_xpath_generation_returns_valid_xpath() {
        let tree = build_test_tree();
        let xpath = tree.get_xpath_for_element(2, false);
        assert!(xpath.is_ok(), "XPath generation should succeed");
        let xpath = xpath.unwrap();
        assert!(!xpath.is_empty());
        assert!(xpath.starts_with('/'));
    }

    #[test]
    fn test_xpath_roundtrip_single_element() {
        let tree = build_test_tree();
        // Node 5 is "Username" (unique name)
        let xpath = tree.get_xpath_for_element(5, false).unwrap();

        let found = tree.get_element_by_xpath(&xpath);
        assert!(
            found.is_some(),
            "Element with generated xpath '{}' should be found",
            xpath
        );
    }

    #[test]
    fn test_xpath_roundtrip_all_nodes() {
        let tree = build_test_tree();
        for idx in 0..tree.get_tree().node_count() {
            let xpath = tree.get_xpath_for_element(idx, false).unwrap();
            let found = tree.get_element_by_xpath(&xpath);
            assert!(
                found.is_some(),
                "Roundtrip failed for node {} with xpath '{}'",
                idx,
                xpath
            );
        }
    }

    #[test]
    fn test_get_element_by_xpath_not_found() {
        let tree = build_test_tree();
        let found = tree.get_element_by_xpath("//NonExistent[@Name='ghost']");
        assert!(found.is_none());
    }

    #[test]
    fn test_get_elements_by_xpath_multiple() {
        let tree = build_test_tree();
        // Both Buttons are children of Panel — select all Button elements
        let found = tree.get_elements_by_xpath("//Button");
        assert!(found.is_some());
        assert_eq!(found.unwrap().len(), 2);
    }

    #[test]
    fn test_get_elements_by_xpath_none() {
        let tree = build_test_tree();
        let found = tree.get_elements_by_xpath("//Slider");
        assert!(found.is_none());
    }
}
