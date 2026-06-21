use crate::UITreeMap;
use crate::common_types::{UIElementInTree, match_original_format};
use crate::error::UITreeError;
use crate::save_ui_element::SaveUIElement;
use bromium_common::{format_runtime_id, format_runtime_id_dotted, printfmt};

use std::sync::mpsc::Sender;
use uiautomation::core::UIAutomation;
use uiautomation::{UIElement, UITreeWalker};

#[derive(Debug, Clone)]
pub struct UITree {
    tree: UITreeMap<()>,
    ui_elements: Vec<UIElementInTree>,
    node_to_elem: Vec<usize>,
}

impl UITree {
    pub fn new(tree: UITreeMap<()>, ui_elements: Vec<UIElementInTree>) -> Self {
        let node_to_elem = Self::build_node_to_elem(&tree, &ui_elements);
        UITree {
            tree,
            ui_elements,
            node_to_elem,
        }
    }

    fn build_node_to_elem(tree: &UITreeMap<()>, elements: &[UIElementInTree]) -> Vec<usize> {
        let mut map = vec![0; tree.node_count()];
        for (pos, elem) in elements.iter().enumerate() {
            let ti = elem.get_tree_index();
            if ti < map.len() {
                map[ti] = pos;
            }
        }
        map
    }

    pub fn get_tree(&self) -> &UITreeMap<()> {
        &self.tree
    }

    pub fn get_elements(&self) -> &[UIElementInTree] {
        &self.ui_elements
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

    pub fn get_xpath_for_element(&self, index: usize) -> String {
        let path = self.get_xpath_raw_for_element(index);
        match_original_format(&path)
    }

    fn get_xpath_raw_for_element(&self, index: usize) -> String {
        let mut path = Vec::new();

        let path_to_element = self.tree.get_path_to_element(index);
        if path_to_element.is_empty() {
            return "/".to_string();
        }

        for &node_index in path_to_element.iter() {
            let elem_pos = self.node_to_elem[node_index];
            let ui_elem_props = self.ui_elements[elem_pos].get_element_props();

            let control_type = ui_elem_props.get_control_type();
            let control_type_localized = ui_elem_props.get_localized_control_type();
            let name = ui_elem_props.get_name();
            let class_name = ui_elem_props.get_classname();
            let automation_id = ui_elem_props.get_automation_id();

            let bounding_rect = ui_elem_props.get_bounding_rectangle();
            let left = bounding_rect.get_left();
            let top = bounding_rect.get_top();
            let right = bounding_rect.get_right();
            let bottom = bounding_rect.get_bottom();
            let width = right - left;
            let height = bottom - top;

            let runtime_id = format_runtime_id_dotted(ui_elem_props.get_runtime_id());
            path.push(format!("/{}[LocalizedControlType=\"{}\"][ClassName=\"{}\"][Name=\"{}\"][AutomationId=\"{}\"][x={}][y={}][width={}][height={}][lx={}][ly={}][position()={}][RuntimeId=\"{}\"]\n", control_type, control_type_localized, class_name, name, automation_id, left, top, width, height, left, top, "", runtime_id));
        }

        path.join("").to_string()
    }
}

pub fn get_all_elements_iterative(
    tx: Sender<Result<UITree, UITreeError>>,
    max_depth: Option<usize>,
) {
    let automation = match UIAutomation::new() {
        Ok(a) => a,
        Err(e) => {
            let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
            return;
        }
    };
    let walker = match automation.get_control_view_walker() {
        Ok(w) => w,
        Err(e) => {
            let _ = tx.send(Err(UITreeError::UIAutomation(e.to_string())));
            return;
        }
    };

    let mut ui_elements: Vec<UIElementInTree> = Vec::with_capacity(10000);

    let root = match automation.get_root_element() {
        Ok(e) => e,
        Err(e) => {
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

    if let Ok(_first_child) = walker.get_first_child(&root) {
        get_element_iterative(
            &mut tree,
            &mut ui_elements,
            0,
            &walker,
            &root,
            0,
            0,
            max_depth,
        );
    }

    printfmt!("Sorting UI elements by z-order and size...");
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

    let ui_tree = UITree::new(tree, ui_elements);

    printfmt!(
        "Sending UI tree with {} elements to the main thread...",
        ui_tree.get_elements().len()
    );
    if let Err(e) = tx.send(Ok(ui_tree)) {
        printfmt!("Failed to send UI tree — receiver dropped: {:?}", e);
    }
}

#[allow(clippy::too_many_arguments)]
fn get_element_iterative(
    tree: &mut UITreeMap<()>,
    ui_elements: &mut Vec<UIElementInTree>,
    parent: usize,
    walker: &UITreeWalker,
    element: &UIElement,
    level: usize,
    z_order: usize,
    max_depth: Option<usize>,
) {
    let mut stack = Vec::new();
    stack.push((parent, element.clone(), level, z_order));

    while let Some((parent, element, level, z_order)) = stack.pop() {
        if let Some(limit) = max_depth
            && level > limit
        {
            continue;
        }

        let effective_z_order = if level == 0 { 999 } else { z_order };
        let ui_elem_props = SaveUIElement::new(&element, level, effective_z_order);
        let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
        let item = format!(
            "'{}' {} ({} | {} | {})",
            ui_elem_props.get_name(),
            ui_elem_props.get_localized_control_type(),
            ui_elem_props.get_classname(),
            ui_elem_props.get_framework_id(),
            runtime_id
        );

        let new_parent = tree.add_child(parent, &item, &runtime_id, ());
        let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, new_parent);
        ui_elements.push(ui_elem_in_tree);

        if let Ok(child) = walker.get_first_child(&element) {
            let mut siblings = vec![child.clone()];
            let mut next = child;
            while let Ok(sibling) = walker.get_next_sibling(&next) {
                siblings.push(sibling.clone());
                next = sibling;
            }
            let sibling_z_order = z_order;
            for (i, sibling) in siblings.into_iter().enumerate().rev() {
                let next_z_order = if level + 1 == 1 {
                    sibling_z_order + i
                } else {
                    sibling_z_order
                };
                stack.push((new_parent, sibling, level + 1, next_z_order));
            }
        }
    }
}
