//! Shared incremental controller. One capture worker, bounded dirty regions,
//! short publication locks, and deadline-bound readers.
use crate::{
    UITree,
    capture::{Capture, CaptureKind, CaptureRequest, UiaCapture},
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, thiserror::Error)]
#[error(
    "Tree coverage is stale ({reason}); scope={scope:?}, revision={revision}, coverage={coverage}"
)]
pub struct StaleTree {
    pub reason: String,
    pub scope: Option<String>,
    pub revision: u64,
    pub coverage: String,
}

#[derive(Clone, Debug)]
struct Dirty {
    kind: CaptureKind,
    epoch: u64,
    retry_at: Instant,
    queued_at: Instant,
}
struct State {
    tree: UITree,
    dirty: HashMap<usize, Dirty>,
    epoch: u64,
    error: Option<String>,
    membership_at: Instant,
    capture_timeout: Duration,
    interest: HashMap<usize, Instant>,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    stop: AtomicBool,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

/// Clones share one controller and one committed tree; no copied cancellation state.
#[derive(Clone)]
pub struct TreeService {
    shared: Arc<Shared>,
    _lifetime: Arc<Lifetime>,
}
struct Lifetime(Arc<Shared>);
impl Drop for Lifetime {
    fn drop(&mut self) {
        self.0.stop.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }
}
impl std::fmt::Debug for TreeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeService")
            .field("revision", &self.snapshot().revision())
            .finish()
    }
}

impl TreeService {
    pub fn new() -> Self {
        Self::from_tree(UITree::empty())
    }
    pub fn from_tree(tree: UITree) -> Self {
        let service = Self::with_capture(tree, UiaCapture::default);
        start_events(Arc::clone(&service.shared));
        service
    }
    fn with_capture<C: Capture + 'static>(
        tree: UITree,
        factory: impl FnOnce() -> C + Send + 'static,
    ) -> Self {
        let initialized = tree.is_initialized();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                tree,
                dirty: HashMap::new(),
                epoch: 0,
                error: None,
                membership_at: Instant::now(),
                capture_timeout: Duration::from_secs(120),
                interest: HashMap::new(),
            }),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            waker: Mutex::new(None),
        });
        if !initialized {
            mark(&mut shared.state.lock().unwrap(), 0, CaptureKind::Children);
        }
        let worker = Arc::clone(&shared);
        thread::spawn(move || run(worker, factory()));
        Self {
            _lifetime: Arc::new(Lifetime(Arc::clone(&shared))),
            shared,
        }
    }
    pub fn snapshot(&self) -> UITree {
        self.shared.state.lock().unwrap().tree.clone()
    }
    pub fn revision(&self) -> u64 {
        self.shared.state.lock().unwrap().tree.revision()
    }
    pub fn set_waker(&self, wake: impl Fn() + Send + Sync + 'static) {
        *self.shared.waker.lock().unwrap() = Some(Arc::new(wake));
    }
    pub fn cached_count(&self, title: Option<&str>) -> usize {
        let s = self.shared.state.lock().unwrap();
        let roots = windows(&s.tree, title);
        s.tree
            .get_elements()
            .iter()
            .filter(|e| {
                e.get_tree_index() == 0
                    || roots
                        .iter()
                        .any(|&r| s.tree.is_descendant(e.get_tree_index(), r))
            })
            .count()
    }
    pub fn resolve_live(&self, id: &[i32]) -> Result<uiautomation::UIElement, String> {
        self.resolve_live_expected(id, None)
    }
    pub fn resolve_live_expected(
        &self,
        id: &[i32],
        expected: Option<usize>,
    ) -> Result<uiautomation::UIElement, String> {
        let (request, index) = {
            let s = self.shared.state.lock().unwrap();
            let index = s.tree.index_for_id(id).ok_or("Element no longer cached")?;
            if expected.is_some_and(|token| token != index) {
                return Err("Cached element was removed or replaced".into());
            }
            (request(&s, index, CaptureKind::Properties), index)
        };
        let live = crate::capture::resolve_live(&request)?;
        if self.shared.state.lock().unwrap().tree.index_for_id(id) != Some(index) {
            return Err("Cached element changed during live resolution".into());
        }
        Ok(live)
    }
    pub fn invalidate_action(&self, id: &[i32]) {
        let mut s = self.shared.state.lock().unwrap();
        if let Some(index) = s.tree.index_for_id(id) {
            let parent = s.tree.get_tree().node(index).parent;
            mark(
                &mut s,
                if parent == 0 { index } else { parent },
                CaptureKind::Subtree,
            );
            self.shared.changed.notify_all();
        }
    }
    pub fn cached_view(&self, title: Option<&str>) -> UITree {
        self.shared.state.lock().unwrap().tree.view(title)
    }
    pub fn set_capture_timeout(&self, timeout: Duration) {
        self.shared.state.lock().unwrap().capture_timeout = timeout;
    }
    pub fn status(&self) -> String {
        let s = self.shared.state.lock().unwrap();
        format!(
            "revision={} dirty={} unobserved={}{}",
            s.tree.revision(),
            s.dirty.len(),
            s.tree
                .get_elements()
                .iter()
                .filter(|e| !s
                    .tree
                    .coverage(e.get_tree_index())
                    .is_some_and(|c| c.children_observed))
                .count(),
            s.error
                .as_ref()
                .map(|e| format!(" error={e}"))
                .unwrap_or_default()
        )
    }
    pub fn invalidate(&self, index: usize, kind: CaptureKind) {
        let mut s = self.shared.state.lock().unwrap();
        if s.tree.get_tree().has_node(index) {
            mark(&mut s, index, kind);
        }
        self.shared.changed.notify_all();
    }
    pub fn invalidate_all(&self) {
        let mut s = self.shared.state.lock().unwrap();
        mark(&mut s, 0, CaptureKind::Children);
        for window in s.tree.children(0).to_vec() {
            mark(&mut s, window, CaptureKind::Subtree);
        }
        self.shared.changed.notify_all();
    }
    /// Nonblocking GUI request for a selected region. Root requests reconcile membership only.
    pub fn request_region(&self, index: usize) {
        let mut s = self.shared.state.lock().unwrap();
        if s.tree.get_tree().has_node(index) {
            let kind = if index == 0 {
                CaptureKind::Children
            } else {
                CaptureKind::Subtree
            };
            if !s.dirty.contains_key(&index) {
                mark(&mut s, index, kind);
            }
        }
        self.shared.changed.notify_all();
    }
    pub fn refresh(&self, title: Option<&str>, deadline: Instant) -> Result<UITree, StaleTree> {
        {
            let mut s = self.shared.state.lock().unwrap();
            mark(&mut s, 0, CaptureKind::Children);
            for window in windows(&s.tree, title) {
                mark(&mut s, window, CaptureKind::Subtree);
            }
        }
        self.shared.changed.notify_all();
        self.ensure(title, deadline)
    }
    /// Membership only: useful for launch discovery without overwriting descendant coverage.
    pub fn membership(&self, deadline: Instant) -> Result<UITree, StaleTree> {
        self.invalidate(0, CaptureKind::Children);
        self.wait_scope(None, deadline, false, None)
    }
    pub fn ensure(&self, title: Option<&str>, deadline: Instant) -> Result<UITree, StaleTree> {
        self.wait_scope(title, deadline, true, None)
    }
    pub fn ensure_region(&self, index: usize, deadline: Instant) -> Result<UITree, StaleTree> {
        self.wait_scope(None, deadline, true, Some(index))
    }
    /// Only provably downward absolute paths can narrow acquisition automatically.
    /// Other XPath expressions retain conservative declared-scope coverage.
    pub fn ensure_query(
        &self,
        xpath: &str,
        title: Option<&str>,
        deadline: Instant,
    ) -> Result<UITree, StaleTree> {
        if let Some(prefix) = absolute_window_prefix(xpath) {
            return self.wait_scopes(title, deadline, true, None, Some(&prefix));
        }
        self.ensure(title, deadline)
    }
    fn wait_scope(
        &self,
        title: Option<&str>,
        deadline: Instant,
        descendants: bool,
        focus: Option<usize>,
    ) -> Result<UITree, StaleTree> {
        self.wait_scopes(title, deadline, descendants, focus, None)
    }
    fn wait_scopes(
        &self,
        title: Option<&str>,
        deadline: Instant,
        descendants: bool,
        focus: Option<usize>,
        prefix: Option<&str>,
    ) -> Result<UITree, StaleTree> {
        let mut s = self.shared.state.lock().unwrap();
        // Age checks are bounded repair, not a full desktop capture.
        if s.membership_at.elapsed() >= Duration::from_secs(2) && !s.dirty.contains_key(&0) {
            mark(&mut s, 0, CaptureKind::Children);
        }
        if descendants {
            for window in query_roots(&s.tree, title, focus, prefix) {
                if s.tree
                    .coverage(window)
                    .and_then(|c| c.children_observed_at)
                    .is_some_and(|at| at.elapsed() >= Duration::from_secs(30))
                    && !s.dirty.contains_key(&window)
                {
                    mark(&mut s, window, CaptureKind::Subtree);
                }
            }
        }
        loop {
            // Recompute dependencies and certify them under the same publication lock.
            let roots = query_roots(&s.tree, title, focus, prefix);
            if focus.is_some_and(|id| !s.tree.get_tree().has_node(id)) {
                return Err(StaleTree {
                    reason: "Target removed during query".into(),
                    scope: title.map(str::to_owned),
                    revision: s.tree.revision(),
                    coverage: "removed".into(),
                });
            }
            if descendants && s.tree.is_initialized() {
                let mut missing = Vec::new();
                for &root in &roots {
                    unobserved(&s.tree, root, &mut missing);
                }
                for node in missing {
                    if !s.dirty.contains_key(&node) {
                        mark(&mut s, node, CaptureKind::Subtree);
                    }
                }
            }
            let pending = !s.tree.is_initialized()
                || s.dirty
                    .iter()
                    .filter(|(id, _)| s.tree.get_tree().has_node(**id))
                    .any(|(&id, d)| {
                        id == 0
                            || s.tree.get_tree().node(id).parent == 0
                                && d.kind == CaptureKind::Properties
                            || descendants
                                && roots.iter().any(|&root| {
                                    s.tree.is_descendant(id, root) || s.tree.is_descendant(root, id)
                                })
                    });
            if !pending {
                return Ok(s.tree.view(title));
            }
            for id in std::iter::once(0).chain(roots.iter().copied()) {
                s.interest
                    .entry(id)
                    .and_modify(|until| *until = (*until).max(deadline))
                    .or_insert(deadline);
            }
            self.shared.changed.notify_all();
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(StaleTree {
                    reason: s
                        .error
                        .clone()
                        .unwrap_or_else(|| "query deadline expired".into()),
                    scope: title.map(str::to_owned),
                    revision: s.tree.revision(),
                    coverage: "dirty or unobserved".into(),
                });
            }
            // Wake on publication/invalidation, not polling provider calls on the caller.
            s = self.shared.changed.wait_timeout(s, remaining).unwrap().0;
        }
    }
}
impl Default for TreeService {
    fn default() -> Self {
        Self::new()
    }
}

fn absolute_window_prefix(xpath: &str) -> Option<String> {
    if !xpath.starts_with('/') || xpath.starts_with("//") {
        return None;
    }
    let mut segments = Vec::new();
    // Deliberately small grammar; quoted slashes and complex predicates fall back.
    for segment in xpath[1..].split('/') {
        if segment.is_empty() {
            continue;
        }
        let (tag, predicate) = segment
            .split_once('[')
            .map_or((segment, None), |(a, b)| (a, Some(b)));
        if tag.is_empty()
            || !tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '*')
        {
            return None;
        }
        if let Some(predicate) = predicate {
            let body = predicate.strip_suffix(']')?;
            if !body.chars().all(|c| c.is_ascii_digit()) {
                let value = body
                    .strip_prefix("@Name=")
                    .or_else(|| body.strip_prefix("@AutomationId="))?;
                let quote = value.chars().next()?;
                if quote != '\'' && quote != '"' {
                    return None;
                }
                let literal = value.strip_prefix(quote)?.strip_suffix(quote)?;
                if literal.contains(quote) || literal.contains(['[', ']']) {
                    return None;
                }
            }
        }
        segments.push(segment);
    }
    if segments.len() < 2 {
        return None;
    }
    // The first two edges must be child edges, not descendant shortcuts.
    let prefix = format!("/{}/{}", segments[0], segments[1]);
    xpath.starts_with(&prefix).then_some(prefix)
}

fn windows(tree: &UITree, title: Option<&str>) -> Vec<usize> {
    tree.children(0)
        .iter()
        .copied()
        .filter(|&i| title.is_none_or(|title| tree.node(i).1.get_name().contains(title)))
        .collect()
}
fn query_roots(
    tree: &UITree,
    title: Option<&str>,
    focus: Option<usize>,
    prefix: Option<&str>,
) -> Vec<usize> {
    if let Some(id) = focus {
        return vec![id];
    }
    let allowed = windows(tree, title);
    if let Some(prefix) = prefix {
        return tree
            .query(prefix)
            .unwrap_or_default()
            .iter()
            .filter_map(|p| tree.index_for_id(p.get_runtime_id()))
            .filter(|id| allowed.contains(id))
            .collect();
    }
    allowed
}
fn unobserved(tree: &UITree, index: usize, output: &mut Vec<usize>) {
    if !tree.coverage(index).is_some_and(|c| {
        c.children_observed
            && c.observed_at.elapsed() < Duration::from_secs(30)
            && c.children_observed_at
                .is_some_and(|at| at.elapsed() < Duration::from_secs(30))
    }) {
        output.push(index);
        return;
    }
    for &child in tree.children(index) {
        unobserved(tree, child, output);
    }
}
fn mark(s: &mut State, index: usize, kind: CaptureKind) {
    // Desktop-wide invalidation is always decomposed into shallow membership
    // plus window patches; never send a desktop Subtree request implicitly.
    let kind = if index == 0 {
        kind.min(CaptureKind::Children)
    } else {
        kind
    };
    s.epoch += 1;
    if let Some(ancestor) = s
        .dirty
        .iter()
        .find(|(id, d)| {
            **id != index && d.kind == CaptureKind::Subtree && s.tree.is_descendant(index, **id)
        })
        .map(|(&id, _)| id)
    {
        s.dirty.get_mut(&ancestor).unwrap().epoch = s.epoch;
        return;
    }
    let kind = s.dirty.get(&index).map_or(kind, |old| old.kind.max(kind));
    // Fixed coalescing window: later events cannot defer the first event forever
    // or defeat failure backoff. This is separate from coverage age.
    let queued_at = s
        .dirty
        .get(&index)
        .map_or_else(Instant::now, |d| d.queued_at);
    let retry_at = s
        .dirty
        .get(&index)
        .map_or(queued_at + Duration::from_millis(20), |d| d.retry_at);
    s.dirty.insert(
        index,
        Dirty {
            kind,
            epoch: s.epoch,
            retry_at,
            queued_at,
        },
    );
}
fn request(s: &State, target: usize, kind: CaptureKind) -> CaptureRequest {
    let window = s.tree.owning_window(target);
    let path = s
        .tree
        .get_tree()
        .get_path_to_element(target)
        .into_iter()
        .map(|i| s.tree.node(i).1.get_runtime_id().to_vec())
        .collect();
    CaptureRequest {
        target: s
            .tree
            .is_initialized()
            .then(|| s.tree.node(target).1.clone()),
        path,
        window_handle: s.tree.node(window).1.get_handle(),
        kind,
        deadline: Instant::now() + s.capture_timeout,
    }
}
fn run<C: Capture>(shared: Arc<Shared>, mut capture: C) {
    let mut jobs = 0_u64;
    while !shared.stop.load(Ordering::Acquire) {
        let job = {
            let mut s = shared.state.lock().unwrap();
            let valid: Vec<_> = s
                .dirty
                .keys()
                .copied()
                .filter(|&id| !s.tree.get_tree().has_node(id))
                .collect();
            for id in valid {
                s.dirty.remove(&id);
            }
            s.interest.retain(|_, until| *until > Instant::now());
            // Every fourth job uses FIFO, reserving capacity for background repair.
            let next = s
                .dirty
                .iter()
                .filter(|(_, d)| d.retry_at <= Instant::now())
                .min_by_key(|(i, d)| {
                    let relevant = s.interest.keys().any(|&root| {
                        root == **i
                            || root != 0
                                && (s.tree.is_descendant(**i, root)
                                    || s.tree.is_descendant(root, **i))
                    });
                    (jobs % 4 != 3 && !relevant, d.epoch)
                })
                .map(|(&id, d)| (id, d.clone()));
            match next {
                Some((id, dirty)) => Some((id, dirty.clone(), request(&s, id, dirty.kind))),
                None => {
                    let delay = s
                        .dirty
                        .values()
                        .map(|d| d.retry_at.saturating_duration_since(Instant::now()))
                        .min()
                        .unwrap_or(Duration::from_millis(100))
                        .min(Duration::from_millis(100));
                    drop(shared.changed.wait_timeout(s, delay).unwrap());
                    None
                }
            }
        };
        let Some((id, dirty, request)) = job else {
            continue;
        };
        jobs = jobs.wrapping_add(1);
        log::debug!(
            "tree_schedule target={} kind={:?} queue_wait_us={}",
            id,
            dirty.kind,
            dirty.queued_at.elapsed().as_micros()
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture.capture(&request, &shared.stop)
        }))
        .unwrap_or_else(|_| Err("Capture worker panicked; coverage retained for retry".into()));
        let mut s = shared.state.lock().unwrap();
        let result = result.and_then(|observation| s.tree.commit(id, observation));
        match result {
            Ok(()) => {
                let covered: Vec<_> = s
                    .dirty
                    .iter()
                    .filter(|(node, d)| {
                        d.epoch <= dirty.epoch
                            && (**node == id
                                || dirty.kind == CaptureKind::Subtree
                                    && s.tree.is_descendant(**node, id))
                    })
                    .map(|(&node, _)| node)
                    .collect();
                for node in covered {
                    s.dirty.remove(&node);
                }
                if id == 0 {
                    s.membership_at = Instant::now();
                }
                s.error = None;
            }
            Err(error) => {
                s.error = Some(error);
                if let Some(d) = s.dirty.get_mut(&id) {
                    d.retry_at = Instant::now() + Duration::from_secs(1);
                }
                // A vanished target needs its parent's membership checked, never a global subtree walk.
                if id != 0 && s.tree.get_tree().has_node(id) {
                    let parent = s.tree.get_tree().node(id).parent;
                    if !s.dirty.contains_key(&parent) {
                        mark(&mut s, parent, CaptureKind::Children);
                    }
                }
            }
        }
        shared.changed.notify_all();
        drop(s);
        if let Some(wake) = shared.waker.lock().unwrap().clone() {
            wake();
        }
    }
}

/// Event callbacks must not synchronously call the live provider. Runtime identity
/// travels in the event's cache; unavailable identity falls back to its owner.
fn event_runtime_id(sender: &uiautomation::UIElement) -> Option<Vec<i32>> {
    use uiautomation::{types::UIProperty, variants::Value};
    match sender
        .get_cached_property_value(UIProperty::RuntimeId)
        .ok()?
        .get_value()
        .ok()?
    {
        Value::ArrayI4(id) => Some(id),
        _ => None,
    }
}

fn start_events(shared: Arc<Shared>) {
    thread::spawn(move || {
        use uiautomation::{
            events::*,
            types::{StructureChangeType, TreeScope, UIProperty},
        };
        let mut monitor = winevent_monitor::WinEventMonitor::try_new().ok();
        let automation = bromium_common::get_ui_automation_instance().ok();
        let mut subscriptions = HashMap::new();
        let mut desktop_handler = None;
        if let Some(a) = &automation
            && let Ok(root) = a.get_root_element()
        {
            let state = Arc::clone(&shared);
            let handler: UIStructureChangeEventHandler = (Box::new(
                move |_: &uiautomation::UIElement, _: StructureChangeType, _: Option<&[i32]>| {
                    mark(&mut state.state.lock().unwrap(), 0, CaptureKind::Children);
                    state.changed.notify_all();
                    Ok(())
                },
            )
                as Box<CustomStructureChangedEventHandlerFn>)
                .into();
            if let Err(e) =
                a.add_structure_changed_event_handler(&root, TreeScope::Children, None, &handler)
            {
                log::warn!("Desktop events unavailable: {e}");
            }
            desktop_handler = Some(handler);
            // Close the startup subscription/discovery race.
            mark(&mut shared.state.lock().unwrap(), 0, CaptureKind::Children);
            shared.changed.notify_all();
        }
        while !shared.stop.load(Ordering::Acquire) {
            let window_ids: Vec<_> = {
                let s = shared.state.lock().unwrap();
                s.tree
                    .children(0)
                    .iter()
                    .map(|&id| {
                        (
                            id,
                            s.tree.node(id).1.get_handle(),
                            s.tree.node(id).1.get_runtime_id().to_vec(),
                        )
                    })
                    .collect()
            };
            if let Some(a) = &automation {
                let removed: Vec<_> = subscriptions
                    .keys()
                    .copied()
                    .filter(|id| !window_ids.iter().any(|(live, _, _)| live == id))
                    .collect();
                for id in removed {
                    if let Some((element, property, structure)) = subscriptions.remove(&id) {
                        let _ = a.remove_property_changed_event_handler(&element, &property);
                        let _ = a.remove_structure_changed_event_handler(&element, &structure);
                    }
                }
                for (owner, handle, expected) in &window_ids {
                    if subscriptions.contains_key(owner) || *handle == 0 {
                        continue;
                    }
                    let Ok(element) =
                        a.element_from_handle(uiautomation::types::Handle::from(*handle))
                    else {
                        continue;
                    };
                    if element.get_runtime_id().ok().as_ref() != Some(expected) {
                        continue;
                    }
                    let owner = *owner;
                    let state = Arc::clone(&shared);
                    let property = crate::event_handler::property_handler(
                        move |sender: &uiautomation::UIElement, property: UIProperty| {
                            let id = event_runtime_id(sender);
                            let mut s = state.state.lock().unwrap();
                            if s.tree.get_tree().has_node(owner) {
                                let target = id.as_ref().and_then(|id| s.tree.index_for_id(id));
                                let kind = if property == UIProperty::BoundingRectangle
                                    || target.is_none()
                                {
                                    CaptureKind::Subtree
                                } else {
                                    CaptureKind::Properties
                                };
                                mark(&mut s, target.unwrap_or(owner), kind);
                                state.changed.notify_all();
                            }
                        },
                    );
                    let state = Arc::clone(&shared);
                    let structure: UIStructureChangeEventHandler = (Box::new(
                        move |sender: &uiautomation::UIElement,
                              kind: StructureChangeType,
                              _: Option<&[i32]>| {
                            let id = event_runtime_id(sender);
                            let mut s = state.state.lock().unwrap();
                            if s.tree.get_tree().has_node(owner) {
                                let target = id.as_ref().and_then(|id| s.tree.index_for_id(id));
                                let (target, kind) = match target {
                                    Some(index) if kind == StructureChangeType::ChildAdded => (
                                        s.tree.get_tree().node(index).parent,
                                        CaptureKind::Children,
                                    ),
                                    Some(index)
                                        if matches!(
                                            kind,
                                            StructureChangeType::ChildrenInvalidated
                                                | StructureChangeType::ChildrenBulkAdded
                                                | StructureChangeType::ChildrenBulkRemoved
                                        ) =>
                                    {
                                        (index, CaptureKind::Subtree)
                                    }
                                    Some(index) => (index, CaptureKind::Children),
                                    None => (owner, CaptureKind::Subtree),
                                };
                                mark(&mut s, target, kind);
                                state.changed.notify_all();
                            }
                            Ok(())
                        },
                    )
                        as Box<CustomStructureChangedEventHandlerFn>)
                        .into();
                    let event_cache = a
                        .create_cache_request()
                        .and_then(|cache| {
                            cache.set_tree_scope(TreeScope::Element)?;
                            cache.add_property(UIProperty::RuntimeId)?;
                            Ok(cache)
                        })
                        .ok();
                    let p = a.add_property_changed_event_handler(
                        &element,
                        TreeScope::Subtree,
                        event_cache.as_ref(),
                        &property,
                        &[
                            UIProperty::Name,
                            UIProperty::BoundingRectangle,
                            UIProperty::AutomationId,
                        ],
                    );
                    let t = a.add_structure_changed_event_handler(
                        &element,
                        TreeScope::Subtree,
                        event_cache.as_ref(),
                        &structure,
                    );
                    if p.is_err() || t.is_err() {
                        log::warn!(
                            "Some window events unavailable; age validation remains enabled"
                        );
                    }
                    subscriptions.insert(owner, (element, property, structure));
                    // Only already observed contents need a startup repair; untouched windows stay lazy.
                    let mut s = shared.state.lock().unwrap();
                    if s.tree.coverage(owner).is_some_and(|c| c.children_observed) {
                        mark(&mut s, owner, CaptureKind::Subtree);
                    }
                    shared.changed.notify_all();
                }
            }
            if let Some(monitor) = &mut monitor {
                let events = monitor.check_for_events();
                let overflow = monitor.take_overflow();
                if overflow || !events.is_empty() {
                    let mut s = shared.state.lock().unwrap();
                    if overflow {
                        mark(&mut s, 0, CaptureKind::Children);
                        for window in s.tree.children(0).to_vec() {
                            mark(&mut s, window, CaptureKind::Subtree);
                        }
                    }
                    for event in events {
                        use winevent_monitor::{Event, NamedEvent};
                        if event.object_id < 0 && event.object_id != -4 {
                            continue;
                        }
                        let hwnd = event.get_hwnd();
                        let root = unsafe {
                            windows::Win32::UI::WindowsAndMessaging::GetAncestor(
                                hwnd,
                                windows::Win32::UI::WindowsAndMessaging::GA_ROOT,
                            )
                        };
                        let handle = if root.is_invalid() {
                            hwnd.0 as isize
                        } else {
                            root.0 as isize
                        };
                        // Destroy notifications arrive after the HWND has ceased to resolve.
                        // Recover its owning window from the committed native identity first.
                        let cached_target = s
                            .tree
                            .get_elements()
                            .iter()
                            .find(|e| e.get_element_props().get_handle() == hwnd.0 as isize)
                            .map(|e| e.get_tree_index());
                        let window = cached_target
                            .map(|id| s.tree.owning_window(id))
                            .filter(|&id| id != 0)
                            .or_else(|| {
                                s.tree
                                    .children(0)
                                    .iter()
                                    .copied()
                                    .find(|&i| s.tree.node(i).1.get_handle() == handle)
                            });
                        if let Some(window) = window {
                            match event.get_event() {
                                Event::Named(NamedEvent::SystemForeground) => {}
                                Event::Named(NamedEvent::ObjectNameChange)
                                    if event.object_id == 0 =>
                                {
                                    let target = s
                                        .tree
                                        .get_elements()
                                        .iter()
                                        .find(|e| {
                                            e.get_element_props().get_handle() == hwnd.0 as isize
                                        })
                                        .map(|e| e.get_tree_index())
                                        .unwrap_or(window);
                                    mark(&mut s, target, CaptureKind::Properties);
                                }
                                Event::Named(
                                    NamedEvent::ObjectCreate
                                    | NamedEvent::ObjectDestroy
                                    | NamedEvent::ObjectShow
                                    | NamedEvent::ObjectHide,
                                ) if event.object_id == 0 => {
                                    if let Some(target) = cached_target {
                                        let parent = s.tree.get_tree().node(target).parent;
                                        mark(&mut s, parent, CaptureKind::Children);
                                    } else if hwnd.0 as isize == handle {
                                        mark(&mut s, 0, CaptureKind::Children);
                                    } else {
                                        mark(&mut s, window, CaptureKind::Subtree);
                                    }
                                }
                                _ => mark(&mut s, window, CaptureKind::Subtree),
                            }
                        } else {
                            mark(&mut s, 0, CaptureKind::Children);
                        }
                    }
                    shared.changed.notify_all();
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        if let Some(a) = automation {
            let _ = a.remove_all_event_handlers();
        }
        drop(subscriptions);
        drop(desktop_handler);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Observation, SaveUIElement};
    fn obs(id: i32, children: Option<Vec<Observation>>) -> Observation {
        Observation {
            properties: SaveUIElement::fixture(id, if id == 1 { "Desktop" } else { "App" }, "Pane"),
            children,
        }
    }
    struct Fake {
        calls: Arc<Mutex<Vec<CaptureKind>>>,
        delay: Duration,
    }
    impl Capture for Fake {
        fn capture(&mut self, r: &CaptureRequest, _: &AtomicBool) -> Result<Observation, String> {
            self.calls.lock().unwrap().push(r.kind);
            thread::sleep(self.delay);
            let p = r
                .target
                .clone()
                .unwrap_or_else(|| SaveUIElement::fixture(1, "Desktop", "Pane"));
            Ok(Observation {
                properties: p,
                children: if r.kind == CaptureKind::Properties {
                    None
                } else {
                    Some(vec![])
                },
            })
        }
    }
    #[test]
    fn clean_cache_does_not_capture() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let c = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![]))).unwrap(),
            move || Fake {
                calls: c,
                delay: Duration::ZERO,
            },
        );
        service.ensure(None, Instant::now()).unwrap();
        assert!(calls.lock().unwrap().is_empty());
    }
    #[test]
    fn obsolete_action_token_rejects_reused_runtime_id_without_provider_calls() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let c = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![]))]))).unwrap(),
            move || Fake {
                calls: c,
                delay: Duration::ZERO,
            },
        );
        let old = service.snapshot().index_for_id(&[42, 2]).unwrap();
        {
            let mut state = service.shared.state.lock().unwrap();
            state.tree.commit(0, obs(1, Some(vec![]))).unwrap();
            state
                .tree
                .commit(0, obs(1, Some(vec![obs(2, Some(vec![]))])))
                .unwrap();
        }
        assert!(
            service
                .resolve_live_expected(&[42, 2], Some(old))
                .unwrap_err()
                .contains("replaced")
        );
        assert!(calls.lock().unwrap().is_empty());
    }
    #[test]
    fn dirty_query_deadline_reports_stale_and_shared_job_survives() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let c = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![]))]))).unwrap(),
            move || Fake {
                calls: c,
                delay: Duration::from_millis(100),
            },
        );
        service.invalidate(1, CaptureKind::Properties);
        let start = Instant::now();
        assert!(
            service
                .ensure(Some("App"), start + Duration::from_millis(10))
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_millis(90));
        service
            .ensure(Some("App"), Instant::now() + Duration::from_secs(2))
            .unwrap();
        assert_eq!(*calls.lock().unwrap(), vec![CaptureKind::Properties]);
    }

    #[test]
    fn concurrent_waiters_share_one_repair() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let captured = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![]))]))).unwrap(),
            move || Fake {
                calls: captured,
                delay: Duration::from_millis(100),
            },
        );
        service.invalidate(1, CaptureKind::Properties);
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let service = service.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    service
                        .ensure(Some("App"), Instant::now() + Duration::from_secs(2))
                        .unwrap();
                    service.snapshot().revision()
                })
            })
            .collect();
        barrier.wait();
        let revisions: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(revisions[0], revisions[1]);
        assert_eq!(*calls.lock().unwrap(), vec![CaptureKind::Properties]);
    }

    #[test]
    fn events_during_capture_remain_dirty() {
        use std::sync::mpsc;
        struct Paused {
            started: mpsc::Sender<()>,
            release: mpsc::Receiver<()>,
            calls: Arc<std::sync::atomic::AtomicUsize>,
        }
        impl Capture for Paused {
            fn capture(
                &mut self,
                r: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    self.started.send(()).unwrap();
                    self.release.recv_timeout(Duration::from_secs(3)).unwrap();
                }
                Ok(Observation {
                    properties: r.target.clone().unwrap(),
                    children: None,
                })
            }
        }
        let (tx, rx) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![]))]))).unwrap(),
            move || Paused {
                started: tx,
                release: wait,
                calls: c,
            },
        );
        service.invalidate(1, CaptureKind::Properties);
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        service.invalidate(1, CaptureKind::Properties);
        release.send(()).unwrap();
        service
            .ensure(Some("App"), Instant::now() + Duration::from_secs(3))
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failure_preserves_revision_and_reports_reason() {
        struct Failed;
        impl Capture for Failed {
            fn capture(
                &mut self,
                _: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                Err("provider unavailable".into())
            }
        }
        let tree = UITree::from_observation(obs(1, Some(vec![]))).unwrap();
        let revision = tree.revision();
        let service = TreeService::with_capture(tree, || Failed);
        service.invalidate(0, CaptureKind::Children);
        let error = service
            .ensure(None, Instant::now() + Duration::from_millis(50))
            .unwrap_err();
        assert_eq!(error.revision, revision);
        assert_eq!(service.snapshot().revision(), revision);
        assert!(error.reason.contains("provider unavailable"));
    }

    #[test]
    fn dirty_coalescing_preserves_granularity() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![obs(3, Some(vec![]))]))])))
                .unwrap(),
            move || Fake {
                calls,
                delay: Duration::ZERO,
            },
        );
        let mut s = service.shared.state.lock().unwrap();
        mark(&mut s, 1, CaptureKind::Properties);
        mark(&mut s, 2, CaptureKind::Properties);
        assert_eq!(s.dirty.len(), 2); // ancestor properties don't cover descendants
        mark(&mut s, 1, CaptureKind::Subtree);
        let epoch = s.epoch;
        mark(&mut s, 2, CaptureKind::Properties);
        assert!(s.dirty[&1].epoch > epoch);
        mark(&mut s, 0, CaptureKind::Subtree);
        assert_eq!(s.dirty[&0].kind, CaptureKind::Children);
    }

    #[test]
    fn xpath_scope_planner_rejects_nonlocal_dependencies() {
        assert_eq!(
            absolute_window_prefix("/Pane/Window[@Name='App']//Button"),
            Some("/Pane/Window[@Name='App']".into())
        );
        for expr in [
            "//Window[@Name='App']//Button",
            "/Pane//Window/Button",
            "/Pane/Window/../Window",
            "/Pane/Window | //Button",
            "/Pane/Window[//Button]",
            "/Pane/Window[contains(@Name,'App')]",
            "/Pane/Window[@Name='a/b']",
        ] {
            assert!(absolute_window_prefix(expr).is_none(), "{expr}");
        }
    }
    #[test]
    fn absolute_query_captures_only_matching_window() {
        struct Recording(Arc<Mutex<Vec<Vec<i32>>>>);
        impl Capture for Recording {
            fn capture(
                &mut self,
                r: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                let properties = r.target.clone().unwrap();
                self.0
                    .lock()
                    .unwrap()
                    .push(properties.get_runtime_id().to_vec());
                Ok(Observation {
                    properties,
                    children: Some(vec![]),
                })
            }
        }
        let mut a = obs(2, None);
        a.properties = SaveUIElement::fixture(2, "A", "Window");
        let mut b = obs(3, None);
        b.properties = SaveUIElement::fixture(3, "B", "Window");
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recording = calls.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![a, b]))).unwrap(),
            move || Recording(recording),
        );
        service
            .ensure_query(
                "/Pane/Window[@Name='A']//Button",
                None,
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(*calls.lock().unwrap(), vec![vec![42, 2]]);
        assert!(!service.snapshot().coverage(2).unwrap().children_observed);
    }
}
