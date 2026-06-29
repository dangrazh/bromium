pub use bromium_common::rectangle::is_inside_rectangle;

use windows::Win32::Foundation::POINT;

use uitree::UIElementInTreeXML;

/// Find the UI element whose bounding rectangle contains `point`,
/// preferring the **smallest-area** match (most specific / innermost element).
///
/// The desktop root (level 0) is always skipped — its bounding rect covers
/// the entire screen and would shadow every real UI element.
pub fn get_point_bounding_rect<'a>(
    point: &POINT,
    ui_elements: &'a [UIElementInTreeXML],
) -> Option<&'a UIElementInTreeXML> {
    let mut best: Option<&UIElementInTreeXML> = None;
    let mut best_area = i64::MAX;
    for element in ui_elements {
        let props = element.get_element_props();
        // Skip the desktop root (level 0) — it covers the whole screen.
        if props.get_level() == 0 {
            continue;
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
