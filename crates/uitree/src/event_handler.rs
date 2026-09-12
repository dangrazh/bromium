//! UIA property callback ABI adapter for windows 0.61.
//!
//! Its generated trampoline receives VARIANT by value and drops it on return.
//! UIA's [in] argument is borrowed even though the ABI passes the struct by value.
//! This adapter deliberately does not inspect or destroy that borrowed payload.
use std::{
    ffi::c_void,
    mem::ManuallyDrop,
    sync::atomic::{AtomicU32, Ordering, fence},
};
use uiautomation::{UIElement, events::UIPropertyChangedEventHandler, types::UIProperty};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_NOINTERFACE, E_POINTER, S_OK},
        System::Variant::VARIANT,
        UI::Accessibility::{
            IUIAutomationElement, IUIAutomationPropertyChangedEventHandler,
            IUIAutomationPropertyChangedEventHandler_Vtbl, UIA_PROPERTY_ID,
        },
    },
    core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface},
};

type Callback = dyn Fn(&UIElement, UIProperty) + Send + Sync;

#[repr(C)]
struct Handler {
    vtable: &'static IUIAutomationPropertyChangedEventHandler_Vtbl,
    references: AtomicU32,
    callback: Box<Callback>,
}

static VTABLE: IUIAutomationPropertyChangedEventHandler_Vtbl =
    IUIAutomationPropertyChangedEventHandler_Vtbl {
        base__: IUnknown_Vtbl {
            QueryInterface: query_interface,
            AddRef: add_ref,
            Release: release,
        },
        HandlePropertyChangedEvent: property_changed,
    };

pub(crate) fn property_handler(
    callback: impl Fn(&UIElement, UIProperty) + Send + Sync + 'static,
) -> UIPropertyChangedEventHandler {
    let handler = Box::new(Handler {
        vtable: &VTABLE,
        references: AtomicU32::new(1),
        callback: Box::new(callback),
    });
    // SAFETY: repr(C) begins with the matching COM vtable; ownership of reference
    // count 1 transfers to the interface. Release exclusively destroys the box.
    unsafe { IUIAutomationPropertyChangedEventHandler::from_raw(Box::into_raw(handler).cast()) }
        .into()
}

unsafe extern "system" fn query_interface(
    this: *mut c_void,
    iid: *const GUID,
    output: *mut *mut c_void,
) -> HRESULT {
    if output.is_null() || iid.is_null() {
        return E_POINTER;
    }
    // SAFETY: COM supplies valid pointers and a live Handler reference.
    unsafe {
        *output = std::ptr::null_mut();
        if *iid == IUnknown::IID || *iid == IUIAutomationPropertyChangedEventHandler::IID {
            *output = this;
            add_ref(this);
            S_OK
        } else {
            E_NOINTERFACE
        }
    }
}

unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
    // SAFETY: every COM caller owns a live reference throughout this call.
    unsafe { &*this.cast::<Handler>() }
        .references
        .fetch_add(1, Ordering::Relaxed)
        + 1
}

unsafe extern "system" fn release(this: *mut c_void) -> u32 {
    // SAFETY: COM transfers one live reference; only its final release frees storage.
    let remaining = unsafe { &*this.cast::<Handler>() }
        .references
        .fetch_sub(1, Ordering::Release)
        - 1;
    if remaining == 0 {
        fence(Ordering::Acquire);
        unsafe {
            drop(Box::from_raw(this.cast::<Handler>()));
        }
    }
    remaining
}

unsafe extern "system" fn property_changed(
    this: *mut c_void,
    sender: *mut c_void,
    property: UIA_PROPERTY_ID,
    value: VARIANT,
) -> HRESULT {
    // The by-value ABI copy borrows the provider's allocations: never VariantClear it.
    let _borrowed_payload = ManuallyDrop::new(value);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: UIA lends sender for this call. UIElement::from clones its COM
        // reference; no unmarshal or raw interface transfer to another thread occurs.
        let sender = unsafe { IUIAutomationElement::from_raw_borrowed(&sender) };
        if let Some(sender) = sender
            && let Ok(property) = property.try_into()
        {
            let handler = unsafe { &*this.cast::<Handler>() };
            (handler.callback)(&UIElement::from(sender), property);
        }
    }));
    if outcome.is_ok() { S_OK } else { E_FAIL }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn property_callback_does_not_release_borrowed_variant() {
        let handler: IUIAutomationPropertyChangedEventHandler = property_handler(|_, _| {}).into();
        let value = VARIANT::from("borrowed callback string");
        for _ in 0..100 {
            // The wrapper passes an ABI copy. The original must remain owned by us.
            unsafe {
                handler
                    .HandlePropertyChangedEvent(None, UIProperty::Name.into(), &value)
                    .unwrap();
            }
            // SAFETY: value was constructed as VT_BSTR and the callback cannot mutate it.
            let text = unsafe { &value.Anonymous.Anonymous.Anonymous.bstrVal };
            assert_eq!(text.to_string(), "borrowed callback string");
        }
        let unknown = handler.cast::<IUnknown>().unwrap();
        drop(handler);
        assert!(
            unknown
                .cast::<IUIAutomationPropertyChangedEventHandler>()
                .is_ok()
        );
    }
}
