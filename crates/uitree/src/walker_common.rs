//! Shared helpers for tree-walker modules, eliminating duplicated root-setup
//! and child-sorting logic across `uiexplore`, `uiexplore_iter`, and `uiexplore_xml`.

use crate::UITreeMap;
use crate::common_types::UIElementInTree;
use crate::save_ui_element::SaveUIElement;
use bromium_common::format_runtime_id;
use uiautomation::UIElement;

/// Hard upper bound on sibling iterations to prevent infinite loops
/// from COM UIAutomation cycles (e.g. crashed or hung processes).
pub const MAX_SIBLINGS: usize = 10_000;

/// Create the root-level `UITreeMap` and initial `UIElementInTree` from a
/// raw `UIElement`.
///
/// Returns `(tree, runtime_id, SaveUIElement)` with the root already pushed
/// into `ui_elements`.
pub fn setup_root(
    root: &UIElement,
    ui_elements: &mut Vec<UIElementInTree>,
) -> (UITreeMap<()>, String) {
    let ui_elem_props = SaveUIElement::new(root, 0, 999);
    let runtime_id = format_runtime_id(ui_elem_props.get_runtime_id());
    let item = format!(
        "'{}' {} ({} | {} | {})",
        ui_elem_props.get_name(),
        ui_elem_props.get_localized_control_type(),
        ui_elem_props.get_classname(),
        ui_elem_props.get_framework_id(),
        runtime_id
    );
    let tree = UITreeMap::new(item, runtime_id.clone(), ());
    let ui_elem_in_tree = UIElementInTree::new(ui_elem_props, 0);
    ui_elements.push(ui_elem_in_tree);
    (tree, runtime_id)
}

/// Sort UI elements by z-order (ascending), then by bounding-rectangle area
/// (ascending) as a tie-breaker. This ensures that smaller (more specific)
/// elements appear after larger containers at the same z-order.
pub fn sort_elements(ui_elements: &mut [UIElementInTree]) {
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
}

/// Format a `SaveUIElement` into the tree-node item string used by all walkers.
pub fn format_node_item(props: &SaveUIElement, runtime_id: &str) -> String {
    format!(
        "'{}' {} ({} | {} | {})",
        props.get_name(),
        props.get_localized_control_type(),
        props.get_classname(),
        props.get_framework_id(),
        runtime_id
    )
}
