#![allow(unused)]

use bromium_common::{get_ui_automation_instance, printfmt};

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use uiautomation::types::Handle;
use uiautomation::{UIAutomation, UIElement};

use win_event_hook::WinEventHook;
pub use win_event_hook::events::{Event, NamedEvent};
use win_event_hook::handles::OpaqueHandle;
use win_event_hook::handles::builtins::WindowHandle;

use windows::Win32::Foundation::HWND;

pub struct WinEventMonitor {
    hook: WinEventHook,
    rx_channel: Receiver<WinEventInfo>,
    last_hwnd: HWND,
    mouse_hwnd: HWND,
    uia: UIAutomation,
}

impl Default for WinEventMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl WinEventMonitor {
    pub fn new() -> Self {
        // The mouse cursor constant (0x0) to filter mouse events later on
        let mouse_hwnd: HWND = HWND::default();

        // create the hook
        let (hook, rx) = create_hook();

        let last_hwnd: HWND = HWND::default();

        // Initialize UIAutomation
        let uia = get_ui_automation_instance()
            .expect("Failed to create UIAutomation instance — COM may not be initialized");

        WinEventMonitor {
            hook,
            rx_channel: rx,
            last_hwnd,
            mouse_hwnd,
            uia,
        }
    }

    pub fn check_for_events(&mut self) -> Vec<WinEvtMonitorEvent> {
        // println!("Checking for events in WinEventMonitor");
        let mut output: Vec<WinEvtMonitorEvent> = Vec::new();

        // Main event processing
        let mut i = 0;
        let mut name = "no name retrieved".to_string();
        let mut rt_id: Vec<i32> = vec![0, 0, 0, 0];

        // Check for new events — use peekable() to avoid consuming the first event
        let mut rx_iter = self.rx_channel.try_iter().peekable();
        if rx_iter.peek().is_none() {
            // printfmt!("No WinEvents in channel");
            return output;
        }

        // let mut cnt = 0;
        for event_info in rx_iter {
            // cnt += 1;
            let hwnd = *event_info.hwnd;
            if hwnd.0 != self.mouse_hwnd.0 {
                if self.last_hwnd.0 != hwnd.0 {
                    self.last_hwnd = hwnd;

                    // TODO: This is currently very unreliable, often fails to get the element from the hwnd
                    // and sometimes even hangs the app. Needs more investigation. Potiential solution is to
                    // run this loop continuously in a separate thread and cache the results in a vector.

                    // let handle: Handle = Handle::from(hwnd.0 as isize);
                    // let element: Result<UIElement, uiautomation::Error> = self.uia.element_from_handle(handle);
                    // match element {
                    //     Ok(e) => {
                    //         name = e.get_name().unwrap_or("".to_string());
                    //         rt_id = e.get_runtime_id().unwrap_or(vec![0, 0, 0, 0]);
                    //     }
                    //     Err(_e) => {
                    //         // name = format!("Failed to get element from handle: {:?}", e);
                    //         name = "invalid hwnd".to_string();
                    //     }
                    // }
                }

                let evt_monitor_event = WinEvtMonitorEvent {
                    event: event_info.event,
                    hwnd: *event_info.hwnd,
                    ui_element_name: name.clone(),
                    ui_element_runtime_id: rt_id.clone(),
                };
                output.push(evt_monitor_event);
            }
        }
        // printfmt!("Processed {} WinEvents from channel", cnt);
        output
    }
}

impl Drop for WinEventMonitor {
    fn drop(&mut self) {
        if let Err(e) = self.hook.uninstall() {
            eprintln!("Failed to uninstall WinEvent hook during drop: {:?}", e);
        }
    }
}

#[derive(Debug)]
pub struct WinEvtMonitorEvent {
    event: Event,
    hwnd: HWND,
    ui_element_name: String,
    ui_element_runtime_id: Vec<i32>,
}

impl WinEvtMonitorEvent {
    pub fn get_event(&self) -> Event {
        self.event
    }

    pub fn get_hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn get_ui_element_name(&self) -> String {
        self.ui_element_name.clone()
    }

    pub fn get_ui_element_runtime_id(&self) -> Vec<i32> {
        self.ui_element_runtime_id.clone()
    }
}

#[derive(Debug)]
struct WinEventInfo {
    event: Event,
    hwnd: OpaqueHandle<WindowHandle>,
}

fn create_event_handler(
    tx: Sender<WinEventInfo>,
) -> impl Fn(Event, OpaqueHandle<WindowHandle>, i32, i32, u32, u32) {
    move |ev, ohwnd: OpaqueHandle<WindowHandle>, _, _, _, _| {
        // printfmt!("Event received: {:?} on hwnd: {:?}", ev, ohwnd);
        tx.send(WinEventInfo {
            event: ev,
            hwnd: ohwnd,
        })
        .unwrap_or_else(|e| eprintln!("Failed to send event: {}", e));
        // printfmt!("Event sent to channel");
    }
}

fn create_hook() -> (WinEventHook, Receiver<WinEventInfo>) {
    // Create channel for communication
    let (tx, rx): (Sender<WinEventInfo>, Receiver<WinEventInfo>) = channel();

    // Create hook config
    let config = win_event_hook::Config::builder()
        .skip_own_process()
        .with_dedicated_thread()
        .with_events(vec![
            // A hidden object is shown. The system sends this event for the following user interface elements: caret, cursor, and window object. Server applications send this event for their accessible objects.
            // Clients assume that when this event is sent by a parent object, all child objects are already displayed. Therefore, server applications do not send this event for the child objects.
            // Hidden objects include the STATE_SYSTEM_INVISIBLE flag; shown objects do not include this flag. The EVENT_OBJECT_SHOW event also indicates that the STATE_SYSTEM_INVISIBLE flag is cleared. Therefore, servers do not send the EVENT_STATE_CHANGE event in this case.
            Event::Named(NamedEvent::ObjectShow),
            // An object is hidden. The system sends this event for the following user interface elements: caret and cursor. Server applications send this event for their accessible objects.
            // When this event is generated for a parent object, all child objects are already hidden. Server applications do not send this event for the child objects.
            // Hidden objects include the STATE_SYSTEM_INVISIBLE flag; shown objects do not include this flag. The EVENT_OBJECT_HIDE event also indicates that the STATE_SYSTEM_INVISIBLE flag is set. Therefore, servers do not send the EVENT_STATE_CHANGE event in this case.
            Event::Named(NamedEvent::ObjectHide),
            // An object has been created. The system sends this event for the following user interface elements: caret, header control, list-view control, tab control, toolbar control, tree view control, and window object. Server applications send this event for their accessible objects.
            // Before sending the event for the parent object, servers must send it for all of an object's child objects. Servers must ensure that all child objects are fully created and ready to accept IAccessible calls from clients before the parent object sends this event.
            // Because a parent object is created after its child objects, clients must make sure that an object's parent has been created before calling IAccessible::get_accParent, particularly if in-context hook functions are used.
            Event::Named(NamedEvent::ObjectCreate),
            // An object has been destroyed. The system sends this event for the following user interface elements: caret, header control, list-view control, tab control, toolbar control, tree view control, and window object. Server applications send this event for their accessible objects.
            // Clients assume that all of an object's children are destroyed when the parent object sends this event.
            // After receiving this event, clients do not call an object's IAccessible properties or methods. However, the interface pointer must remain valid as long as there is a reference count on it (due to COM rules), but the UI element may no longer be present. Further calls on the interface pointer may return failure errors; to prevent this, servers create proxy objects and monitor their life spans.
            Event::Named(NamedEvent::ObjectDestroy),
            // An object has changed location, shape, or size. The system sends this event for the following user interface elements: caret and window objects. Server applications send this event for their accessible objects.
            // This event is generated in response to a change in the top-level object within the object hierarchy; it is not generated for any children that the object might have. For example, if the user resizes a window, the system sends this notification for the window, but not for the menu bar, title bar, scroll bar, or other objects that have also changed.
            // The system does not send this event for every non-floating child window when the parent moves. However, if an application explicitly resizes child windows as a result of resizing the parent window, the system sends multiple events for the resized children.
            // If an object's State property is set to STATE_SYSTEM_FLOATING, the server sends EVENT_OBJECT_LOCATIONCHANGE whenever the object changes location. If an object does not have this state, servers only trigger this event when the object moves in relation to its parent. For this event notification, the idChild parameter of the WinEventProc callback function identifies the child object that has changed.
            Event::Named(NamedEvent::ObjectLocationChange),
            // A window object is about to be restored. This event is sent by the system, never by servers.
            Event::Named(NamedEvent::SystemMinimizeEnd),
            // The movement or resizing of a window has finished. This event is sent by the system, never by servers.
            Event::Named(NamedEvent::SystemMoveSizeEnd),
        ])
        .finish();

    // Create handler and install hook
    printfmt!("Installing hook");
    let handler = create_event_handler(tx);
    let hook = win_event_hook::WinEventHook::install(config, handler)
        .expect("Failed to install WinEvent hook");
    (hook, rx)
}
