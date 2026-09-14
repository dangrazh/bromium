use windows_strings::BSTR;

use log::{debug, error, info};
use uiautomation::UIElement;
use uiautomation::patterns::UIWindowPattern;

use bromium_common::{RuntimeIdFilter, get_ui_automation_instance};

pub fn get_ui_element_by_runtimeid(runtime_id: Vec<i32>) -> uiautomation::Result<UIElement> {
    debug!("Searching for element with runtime id: {:?}", runtime_id);
    // let automation = UIAutomation::new().unwrap();
    let uia = get_ui_automation_instance()?;
    let matcher = uia
        .create_matcher()
        .timeout(0)
        .filter(Box::new(RuntimeIdFilter(runtime_id)))
        .depth(99);
    let element = matcher.find_first();

    match element {
        Ok(e) => {
            info!("Element found by runtime id: {:?}", e);
            Ok(e)
        }
        Err(e) => {
            error!("Error finding element by runtime id: {:?}", e);
            Err(e)
        }
    }
}

use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern,
    IUIAutomationValuePattern, UIA_InvokePatternId, UIA_SelectionItemPatternId, UIA_ValuePatternId,
};

pub fn invoke_click(element: &IUIAutomationElement) -> windows::core::Result<()> {
    unsafe {
        let invoke: IUIAutomationInvokePattern =
            element.GetCurrentPatternAs(UIA_InvokePatternId)?;

        invoke.Invoke()?;
    }
    Ok(())
}

pub fn select_item(element: &IUIAutomationElement) -> windows::core::Result<()> {
    unsafe {
        let select: IUIAutomationSelectionItemPattern =
            element.GetCurrentPatternAs(UIA_SelectionItemPatternId)?;

        select.Select()?;
    }
    Ok(())
}

pub fn set_value(element: &IUIAutomationElement, text: String) -> windows::core::Result<()> {
    unsafe {
        let value: IUIAutomationValuePattern = element.GetCurrentPatternAs(UIA_ValuePatternId)?;

        // let text_wchar = text.encode_utf16().collect();
        let text_bstr = BSTR::from(text);
        value.SetValue(&text_bstr)?;
    }
    Ok(())
}

pub fn supports_invoke(element: &IUIAutomationElement) -> bool {
    unsafe { element.GetCurrentPattern(UIA_InvokePatternId).is_ok() }
}

pub fn supports_select(element: &IUIAutomationElement) -> bool {
    unsafe {
        element
            .GetCurrentPattern(UIA_SelectionItemPatternId)
            .is_ok()
    }
}

pub fn supports_value(element: &IUIAutomationElement) -> bool {
    unsafe { element.GetCurrentPattern(UIA_ValuePatternId).is_ok() }
}

/// Obtain the capability on the validated live element, never on an ancestor.
pub fn close_window(element: &UIElement) -> uiautomation::Result<()> {
    let pattern = element
        .get_pattern::<UIWindowPattern>()
        .map_err(window_pattern_error)?;
    pattern.close()
}

fn window_pattern_error(error: uiautomation::Error) -> uiautomation::Error {
    use windows::Win32::{
        Foundation::{E_NOINTERFACE, E_POINTER},
        UI::Accessibility::UIA_E_NOTSUPPORTED,
    };
    // windows-core 0.61 maps a successful null interface result to Error::empty()
    // (code 0). Other providers/bindings can report E_POINTER/unsupported instead.
    // Do not relabel unrelated provider failures as an unsupported capability.
    let unsupported = matches!(error.code(), code if code == E_NOINTERFACE.0
        || code == 0 || code == E_POINTER.0 || code == UIA_E_NOTSUPPORTED as i32);
    let context = if unsupported {
        "Element does not support window closure (UIA Window pattern unavailable)"
    } else {
        "Failed to obtain UIA Window pattern for closure"
    };
    let detail = if error.code() == 0 {
        "provider returned no pattern interface".to_owned()
    } else {
        format!("{error} (code=0x{:08X})", error.code() as u32)
    };
    uiautomation::Error::new(error.code(), &format!("{context}: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::{
        Foundation::{E_ACCESSDENIED, E_NOINTERFACE, E_POINTER},
        UI::Accessibility::UIA_E_NOTSUPPORTED,
    };

    #[test]
    fn close_unsupported_pattern_errors_are_explicit() {
        for code in [0, E_NOINTERFACE.0, E_POINTER.0, UIA_E_NOTSUPPORTED as i32] {
            let error = window_pattern_error(uiautomation::Error::new(code, "original reason"));
            assert_eq!(error.code(), code);
            assert!(error.message().contains("does not support window closure"));
            assert!(error.message().contains(if code == 0 {
                "provider returned no pattern interface"
            } else {
                "original reason"
            }));
        }
    }

    #[test]
    fn close_preserves_provider_failure_diagnostics() {
        let error =
            window_pattern_error(uiautomation::Error::new(E_ACCESSDENIED.0, "access denied"));
        assert_eq!(error.code(), E_ACCESSDENIED.0);
        assert!(
            error
                .message()
                .contains("Failed to obtain UIA Window pattern")
        );
        assert!(error.message().contains("access denied"));
        assert!(!error.message().contains("does not support"));
    }
}
