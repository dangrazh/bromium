use crate::UITreeMap;
use crate::common_types::UIElementInTree;
use crate::error::UITreeError;
use crate::save_ui_element::SaveUIElement;
use crate::uiexplore_xml::UITree;
use crate::walker_common::{self, MAX_SIBLINGS};
use bromium_common::{format_runtime_id, printfmt};

use std::sync::mpsc::Sender;
use uiautomation::core::UIAutomation;
use uiautomation::{UIElement, UITreeWalker};

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

    let (mut tree, _runtime_id) = walker_common::setup_root(&root, &mut ui_elements);

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
    walker_common::sort_elements(&mut ui_elements);

    let ui_tree = UITree::new(tree, String::new(), ui_elements);

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
        let item = walker_common::format_node_item(&ui_elem_props, &runtime_id);

        let new_parent = tree.add_child(parent, &item, &runtime_id, ());
        let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, new_parent);
        ui_elements.push(ui_elem_in_tree);

        if let Ok(child) = walker.get_first_child(&element) {
            let mut siblings = vec![child.clone()];
            let mut next = child;
            let mut sibling_count: usize = 0;
            while let Ok(sibling) = walker.get_next_sibling(&next) {
                sibling_count += 1;
                if sibling_count > MAX_SIBLINGS {
                    log::warn!(
                        "Sibling loop exceeded {MAX_SIBLINGS} iterations at depth {}, breaking to prevent infinite loop",
                        level + 1
                    );
                    break;
                }
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
