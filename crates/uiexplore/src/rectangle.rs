pub use bromium_common::rectangle::is_inside_rectangle;

use windows::Win32::Foundation::POINT;

use uitree::UIElementInTreeXML;

/// Find the UI element whose bounding rectangle contains `point`,
/// preferring the **smallest-area** match (most specific / innermost element).
///
/// If `target_z_order` is `Some(z)`, only elements with that z-order are
/// considered.  This allows callers to restrict the search to elements
/// belonging to a specific top-level window (whose visual z-order was
/// determined externally, e.g. via `WindowFromPoint`).
///
/// The desktop root (level 0) is always skipped — its bounding rect covers
/// the entire screen and would shadow every real UI element.
pub fn get_point_bounding_rect<'a>(
    point: &POINT,
    ui_elements: &'a [UIElementInTreeXML],
    target_z_order: Option<usize>,
) -> Option<&'a UIElementInTreeXML> {
    let mut best: Option<&UIElementInTreeXML> = None;
    let mut best_area = i64::MAX;
    for element in ui_elements {
        let props = element.get_element_props();
        // Skip the desktop root (level 0) — it covers the whole screen.
        if props.get_level() == 0 {
            continue;
        }
        // If a target z-order was specified, skip elements from other windows.
        if let Some(tz) = target_z_order {
            if props.get_z_order() != tz {
                continue;
            }
        }
        let bounding_rect = props.get_bounding_rectangle();
        if is_inside_rectangle(bounding_rect, point.x, point.y) {
            let area = (bounding_rect.get_right() as i64 - bounding_rect.get_left() as i64)
                * (bounding_rect.get_bottom() as i64 - bounding_rect.get_top() as i64);
            if area < best_area {
                best_area = area;
                best = Some(element);
            }
        }
    }
    best
}
