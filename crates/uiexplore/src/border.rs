
// use std::ptr::null_mut;
// use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
// use windows::Win32::UI::WindowsAndMessaging::{
//     WNDCLASSW, RegisterClassW, CreateWindowExW, DefWindowProcW, DestroyWindow, UnregisterClassW,
//     CS_HREDRAW, CS_VREDRAW, WM_PAINT, WM_DESTROY, WS_OVERLAPPEDWINDOW,
// };
// use windows::core::PCWSTR;
// use windows::Win32::System::LibraryLoader::GetModuleHandleW;


// pub fn draw_border(rect: RECT) -> windows::core::Result<HWND> {
//     unsafe extern "system" fn wnd_proc(
//         hwnd: HWND,
//         msg: u32,
//         wparam: WPARAM,
//         lparam: LPARAM,
//     ) -> LRESULT {
//         match msg {
//             WM_PAINT => {
//                 let dc = unsafe {GetDC(Some(hwnd)) };
//                 let left = rect.left;
//                 let top = rect.top;
//                 let right = rect.right;
//                 let bottom = rect.bottom;

//                 unsafe {
//                     Rectangle(dc, left, top, right, bottom);
//                     ReleaseDC(Some(hwnd), dc);
//                 }
//                 LRESULT(0)
//             }
//             WM_DESTROY => LRESULT(0),
//             _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
//         }
//     }
    
//     let h_module = unsafe { GetModuleHandleW(None) }.unwrap();
//     let name: Vec<u16> = format!("HighlightingBorder\0").encode_utf16().collect();
//     let class_name = PCWSTR(name.as_ptr());
//     let wnd_class = WNDCLASSW {
//         style: CS_HREDRAW | CS_VREDRAW,
//         lpfnWndProc: Some(wnd_proc),
//         hInstance: h_module.into(),
//         lpszClassName: PCWSTR(class_name.as_ptr()),
//         ..Default::default()
//     };
//     unsafe {
//         RegisterClassW(&wnd_class);
//         let hwnd = CreateWindowExW(
//             Default::default(),
//             PCWSTR(class_name.as_ptr()),
//             PCWSTR(class_name.as_ptr()),
//             WS_OVERLAPPEDWINDOW,
//             rect.left,
//             rect.top,
//             rect.right - rect.left,
//             rect.bottom - rect.top,
//             None,
//             None,
//             null_mut(),
//             null_mut(),
//         );
//         Ok(hwnd)
//     }
// }

// pub fn clear_border(hwnd: HWND) -> windows::core::Result<()> {
    
//     let h_module = unsafe { GetModuleHandleW(None) }.unwrap();

//     let name: Vec<u16> = format!("HighlightingBorder\0").encode_utf16().collect();
//     let class_name = PCWSTR(name.as_ptr());
    
//     unsafe {
//         DestroyWindow(hwnd);
//         UnregisterClassW(
//             class_name,
//             Some(h_module.into()),
//         );
//     }
//     Ok(())
// }
