use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use win_event_hook::WinEventHook;
pub use win_event_hook::events::{Event, NamedEvent};
use win_event_hook::handles::OpaqueHandle;
use win_event_hook::handles::builtins::WindowHandle;

use windows::Win32::Foundation::HWND;

pub struct WinEventMonitor {
    hook: WinEventHook,
    rx_channel: Receiver<WinEventInfo>,
    overflow: Arc<AtomicBool>,
}

impl Default for WinEventMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl WinEventMonitor {
    pub fn new() -> Self {
        let (hook, rx, overflow) =
            create_hook().expect("Failed to install WinEvent hook — is the event loop running?");

        WinEventMonitor {
            hook,
            rx_channel: rx,
            overflow,
        }
    }

    /// Create a new `WinEventMonitor`, returning an error if hook installation fails.
    pub fn try_new() -> Result<Self, win_event_hook::errors::Error> {
        let (hook, rx, overflow) = create_hook()?;

        Ok(WinEventMonitor {
            hook,
            rx_channel: rx,
            overflow,
        })
    }

    pub fn take_overflow(&self) -> bool {
        self.overflow.swap(false, Ordering::AcqRel)
    }

    pub fn check_for_events(&mut self) -> Vec<WinEvtMonitorEvent> {
        let mut output: Vec<WinEvtMonitorEvent> = Vec::new();

        let mut rx_iter = self.rx_channel.try_iter().peekable();
        if rx_iter.peek().is_none() {
            return output;
        }

        for event_info in rx_iter {
            let hwnd = *event_info.hwnd;
            if !hwnd.is_invalid() {
                output.push(WinEvtMonitorEvent {
                    event: event_info.event,
                    hwnd,
                    object_id: event_info.object_id,
                    child_id: event_info.child_id,
                    thread_id: event_info.thread_id,
                    timestamp: event_info.timestamp,
                });
            }
        }
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
    pub object_id: i32,
    pub child_id: i32,
    pub thread_id: u32,
    pub timestamp: u32,
}

impl WinEvtMonitorEvent {
    pub fn get_event(&self) -> Event {
        self.event
    }

    pub fn get_hwnd(&self) -> HWND {
        self.hwnd
    }
}

#[derive(Debug)]
struct WinEventInfo {
    event: Event,
    hwnd: OpaqueHandle<WindowHandle>,
    object_id: i32,
    child_id: i32,
    thread_id: u32,
    timestamp: u32,
}

fn create_event_handler(
    tx: SyncSender<WinEventInfo>,
    overflow: Arc<AtomicBool>,
) -> impl Fn(Event, OpaqueHandle<WindowHandle>, i32, i32, u32, u32) {
    move |ev, ohwnd: OpaqueHandle<WindowHandle>, object_id, child_id, thread_id, timestamp| {
        // log::debug!("Event received: {:?} on hwnd: {:?}", ev, ohwnd);
        if tx
            .try_send(WinEventInfo {
                event: ev,
                hwnd: ohwnd,
                object_id,
                child_id,
                thread_id,
                timestamp,
            })
            .is_err()
        {
            overflow.store(true, Ordering::Release);
        }
        // log::debug!("Event sent to channel");
    }
}

fn create_hook()
-> Result<(WinEventHook, Receiver<WinEventInfo>, Arc<AtomicBool>), win_event_hook::errors::Error> {
    // Create channel for communication
    let (tx, rx) = sync_channel(1024);
    let overflow = Arc::new(AtomicBool::new(false));

    // Create hook config
    let config = win_event_hook::Config::builder()
        .skip_own_process()
        .with_dedicated_thread()
        .with_events(vec![
            // A hidden object is shown. The system sends this event for the following user interface elements: caret, cursor, and window object. Server applications send this event for their accessible objects.
            // Clients assume that when this event is sent by a parent object, all child objects are already displayed. Therefore, server applications do not send this event for the child objects.
            // Hidden objects include the STATE_SYSTEM_INVISIBLE flag; shown objects do not include this flag. The EVENT_OBJECT_SHOW event also indicates that the STATE_SYSTEM_INVISIBLE flag is cleared. Therefore, servers do not send the EVENT_STATE_CHANGE event in this case.
            Event::Named(NamedEvent::ObjectNameChange),
            Event::Named(NamedEvent::ObjectValueChange),
            Event::Named(NamedEvent::ObjectReorder),
            Event::Named(NamedEvent::SystemForeground),
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
    log::info!("Installing hook");
    let handler = create_event_handler(tx, Arc::clone(&overflow));
    let hook = win_event_hook::WinEventHook::install(config, handler)?;
    Ok((hook, rx, overflow))
}
