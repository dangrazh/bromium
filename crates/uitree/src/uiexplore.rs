use crate::UITreeMap;
use crate::common_types::UIElementInTree;
use crate::error::UITreeError;
use crate::save_ui_element::SaveUIElement;
use crate::uiexplore_xml::UITree;
use bromium_common::{format_runtime_id, printfmt};

use std::sync::mpsc::Sender;
use uiautomation::core::UIAutomation;
use uiautomation::{UIElement, UITreeWalker};

/// Hard upper bound on sibling iterations to prevent infinite loops
/// from COM UIAutomation cycles (e.g. crashed or hung processes).
const MAX_SIBLINGS: usize = 10_000;

pub fn get_all_elements(tx: Sender<Result<UITree, UITreeError>>, max_depth: Option<usize>) {
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
        get_element(
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
fn get_element(
    tree: &mut UITreeMap<()>,
    ui_elements: &mut Vec<UIElementInTree>,
    parent: usize,
    walker: &UITreeWalker,
    element: &UIElement,
    level: usize,
    mut z_order: usize,
    max_depth: Option<usize>,
) {
    if let Some(limit) = max_depth
        && level > limit
    {
        return;
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
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, parent);
    ui_elements.push(ui_elem_in_tree);

    if let Ok(child) = walker.get_first_child(element) {
        get_element(
            tree,
            ui_elements,
            parent,
            walker,
            &child,
            level + 1,
            z_order,
            max_depth,
        );
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
            if level + 1 == 1 {
                z_order += 1;
            }
            get_element(
                tree,
                ui_elements,
                parent,
                walker,
                &sibling,
                level + 1,
                z_order,
                max_depth,
            );
            next = sibling;
        }
    }
}
