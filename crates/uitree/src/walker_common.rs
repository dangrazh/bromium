use crate::save_ui_element::SaveUIElement;
use crate::common_types::UIElementInTree;

pub const MAX_SIBLINGS: usize = 10_000;

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
