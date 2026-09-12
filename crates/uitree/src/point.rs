use crate::{UIElementInTree, UITree};
use windows::Win32::{
    Foundation::POINT,
    UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint},
};

pub fn window_at_point(x: i32, y: i32) -> Option<isize> {
    // SAFETY: read-only native hit testing with a by-value screen point.
    let hwnd = unsafe { WindowFromPoint(POINT { x, y }) };
    if hwnd.is_invalid() {
        return None;
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    (!root.is_invalid()).then_some(root.0 as isize)
}

pub fn element_at_point(tree: &UITree, x: i32, y: i32) -> Option<&UIElementInTree> {
    let handle = window_at_point(x, y)?;
    let window = tree
        .children(0)
        .iter()
        .copied()
        .find(|&id| tree.node(id).1.get_handle() == handle)?;
    tree.get_elements()
        .iter()
        .filter(|e| tree.is_descendant(e.get_tree_index(), window))
        .filter(|e| {
            bromium_common::rectangle::is_inside_rectangle(
                e.get_element_props().get_bounding_rectangle(),
                x,
                y,
            )
        })
        .min_by_key(|e| e.get_element_props().get_bounding_rect_size())
}
