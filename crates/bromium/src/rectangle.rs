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

pub fn get_point_bounding_rect<'a>(
    point: &'a POINT,
    ui_elements: &'a Vec<UIElementInTreeXML>,
) -> Option<&'a UIElementInTreeXML> {
    // let mut cntr = 0;
    for element in ui_elements {
        // cntr += 1;
        let bounding_rect = &element.get_element_props().get_bounding_rectangle();
        if is_inside_rectancle(bounding_rect, point.x, point.y) {
            // println!("point: {{ x: {}, y: {} }} searched elements: {} / Found element: {{ name: '{}', control_type: '{}' bounding_rect: {} }}", point.x, point.y, cntr, element.name, element.control_type, element.bounding_rect);
            return Some(element);
        }
    }
    // printfmt!("NO ELEMENT FOUND! Searched elements: {}", cntr);
    None
}

pub fn is_inside_rectancle(rect: &uiautomation::types::Rect, x: i32, y: i32) -> bool {
    x >= rect.get_left() && x <= rect.get_right() && y >= rect.get_top() && y <= rect.get_bottom()
}

#[allow(dead_code)]
pub fn draw_frame(rect: RECT, outline_width: i32) -> Result<()> {
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

#[allow(dead_code)]
pub fn clear_frame(rect: RECT) -> Result<()> {
    unsafe {
        // Force redraw of the region
        let _res = InvalidateRect(Some(HWND(std::ptr::null_mut())), Some(&rect), true);
        Ok(())
    }
}
