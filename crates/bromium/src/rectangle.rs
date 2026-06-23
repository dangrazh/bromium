pub use bromium_common::rectangle::is_inside_rectangle;

use windows::Win32::Foundation::POINT;

use uitree::UIElementInTreeXML;

/// Find the UI element whose bounding rectangle contains `point`,
/// preferring the **smallest-area** match (most specific / innermost element).
pub fn get_point_bounding_rect<'a>(
    point: &'a POINT,
    ui_elements: &'a [UIElementInTreeXML],
) -> Option<&'a UIElementInTreeXML> {
    let mut best: Option<&UIElementInTreeXML> = None;
    let mut best_area = i64::MAX;
    for element in ui_elements {
        let bounding_rect = &element.get_element_props().get_bounding_rectangle();
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
