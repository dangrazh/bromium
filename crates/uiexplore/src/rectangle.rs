// use windows::Win32::Foundation::POINT;

use windows::{
    Win32::Foundation::{COLORREF, HWND, POINT, RECT},
    Win32::Graphics::Gdi::{
        CreatePen, DeleteObject, GetDC, GetStockObject, HOLLOW_BRUSH, InvalidateRect, PS_SOLID,
        Rectangle, ReleaseDC, SelectObject,
    },
    // Win32::UI::WindowsAndMessaging::*,
    core::{Error, Result},
};

use uitree::UIElementInTreeXML;

// TODO: Change the return value to contain both the element and the index
//       and add the index as an input parameter as well to start looping from that index
//       as the rectangles are sorted by size
pub fn get_point_bounding_rect<'a>(
    point: &'a POINT,
    ui_elements: &'a [UIElementInTreeXML],
) -> Option<&'a UIElementInTreeXML> {
    // pub fn get_point_bounding_rect(point: &Pos2, ui_elements: &Vec<UIElementProps>) -> Option<&UIElementProps> {
    // printfmt!("Searching for element at point: {{ x: {}, y: {} }} in tree with {} elements.", point.x, point.y, ui_elements.len());
    // let mut cntr = 0;
    for element in ui_elements {
        // cntr += 1;
        // printfmt!("point: {{ x: {}, y: {} }} searching element: {}", point.x, point.y, cntr);
        // if cntr == 27 {
        //     dbg!(element);
        // }
        let rect = element
            .get_element_props()
            .get_element()
            .get_bounding_rectangle();
        if is_inside_rectangle(rect, point.x, point.y) {
            // printfmt!("point: {{ x: {}, y: {} }} searched elements: {} / Found element: {{ name: '{}', control_type: '{}' bounding_rect: {} }}", point.x, point.y, cntr, element.get_element_props().get_element().get_name(), element.get_element_props().get_element().get_control_type(), element.get_element_props().get_element().get_bounding_rectangle());
            return Some(element);
        }
    }
    // printfmt!("NO ELEMENT FOUND! Searched elements: {}", cntr);
    None
}

pub fn is_inside_rectangle(rect: &uiautomation::types::Rect, x: i32, y: i32) -> bool {
    x >= rect.get_left() && x <= rect.get_right() && y >= rect.get_top() && y <= rect.get_bottom()
}

pub fn draw_frame(rect: RECT, outline_width: i32) -> Result<()> {
    // SAFETY: All GDI handles are checked for validity before use and cleaned up
    // in reverse order. HWND(null) targets the desktop DC, which is always valid.
    unsafe {
        // Get DC and check for NULL
        let hdc = GetDC(Some(HWND(std::ptr::null_mut())));
        if hdc.is_invalid() {
            return Err(Error::from_win32());
        }

        // Create a bright yellow pen and check result
        // 2747903 is the U32 little endian representation of hex #ffed29
        // 393004 is the U32 little endian representation of hex #2cff05
        let color = COLORREF(393004);
        let pen = CreatePen(PS_SOLID, outline_width, color);
        if pen.is_invalid() {
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        // Select pen and check result
        let old_pen = SelectObject(hdc, pen.into());
        if old_pen.is_invalid() {
            let _del_res = DeleteObject(pen.into());
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        // Get stock hollow brush and check result
        let hollow_brush = GetStockObject(HOLLOW_BRUSH);
        if hollow_brush.is_invalid() {
            SelectObject(hdc, old_pen);
            let _del_res = DeleteObject(pen.into());
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        // Select brush and check result
        let old_brush = SelectObject(hdc, hollow_brush);
        if old_brush.is_invalid() {
            SelectObject(hdc, old_pen);
            let _del_res = DeleteObject(pen.into());
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        // Draw rectangle
        if !Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom).as_bool() {
            SelectObject(hdc, old_brush);
            SelectObject(hdc, old_pen);
            let _del_res = DeleteObject(pen.into());
            ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);
            return Err(Error::from_win32());
        }

        // Cleanup in reverse order of creation
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _del_res = DeleteObject(pen.into());
        ReleaseDC(Some(HWND(std::ptr::null_mut())), hdc);

        Ok(())
    }
}

pub fn clear_frame(rect: RECT) -> Result<()> {
    // SAFETY: HWND(null) targets all windows; `rect` is a valid stack-allocated RECT.
    unsafe {
        // Force redraw of the region
        let _res = InvalidateRect(Some(HWND(std::ptr::null_mut())), Some(&rect), true);
        Ok(())
    }
}
