use windows_strings::*;

use log::{debug, error, info};
use uiautomation::UIElement;

use bromium_common::{RuntimeIdFilter, get_ui_automation_instance};

pub fn get_ui_element_by_runtimeid(runtime_id: Vec<i32>) -> Option<UIElement> {
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
            Some(e)
        }
        Err(e) => {
            error!("Error finding element by runtime id: {:?}", e);
            None
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

#[cfg(test)]
mod tests {
    use bromium_common::get_ui_automation_instance;
    #[allow(unused_imports)]
    use log::debug;

    #[test]
    fn test_ui_automation_creation_sta() {
        debug!("UIAutomation::test_ui_automation_creation_sta called.");

        use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

        let _result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };

        let uia = get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");
    }

    #[test]
    fn test_ui_automation_creation_mta() {
        debug!("UIAutomation::test_ui_automation_creation_mta called.");

        let uia = get_ui_automation_instance();
        assert!(uia.is_some(), "Failed to create UIAutomation instance");
    }
}
