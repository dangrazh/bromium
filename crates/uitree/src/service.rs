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
    error: Option<String>,
}
struct State {
    tree: UITree,
    dirty: HashMap<usize, Dirty>,
    epoch: u64,
    membership_at: Instant,
    invalidated_at: HashMap<crate::ElementIdentity, u64>,
    invalidation_floor: u64,
    capture_timeout: Duration,
    interest: HashMap<usize, Instant>,
    discovery_epoch: u64,
    point_publications: HashMap<usize, u64>,
    excluded_process: Option<u32>,
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
        start_events(Arc::clone(&service.shared), None);
        service
    }
    /// An independent desktop view excluding one process before capture and subscription.
    /// Intended for inspectors which must not recursively inspect their own accessibility UI.
    /// The default service (including Python callers) remains unfiltered.
    pub fn excluding_process(process: u32) -> Self {
        let service = Self::with_capture(UITree::empty(), move || {
            UiaCapture::excluding_process(process)
        });
        service.shared.state.lock().unwrap().excluded_process = Some(process);
        start_events(Arc::clone(&service.shared), Some(process));
        service
    }
    pub(crate) fn with_capture<C: Capture + 'static>(
        tree: UITree,
        factory: impl FnOnce() -> C + Send + 'static,
    ) -> Self {
        let initialized = tree.is_initialized();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                tree,
                dirty: HashMap::new(),
                epoch: 0,
                invalidated_at: HashMap::new(),
                invalidation_floor: 0,
                membership_at: Instant::now(),
                capture_timeout: Duration::from_secs(120),
                interest: HashMap::new(),
                discovery_epoch: 0,
                point_publications: HashMap::new(),
                excluded_process: None,
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
    pub(crate) fn point_capture_context(&self) -> (u64, Option<u32>) {
        let s = self.shared.state.lock().unwrap();
        (s.epoch, s.excluded_process)
    }

    /// Publish a positive point observation without certifying unrelated dirty
    /// descendants. It runs independently of a blocked background subtree capture.
    pub(crate) fn publish_point(
        &self,
        desktop: &crate::ElementIdentity,
        observation: crate::Observation,
        target: &crate::ElementIdentity,
        process: u32,
        started_epoch: u64,
        deadline: Instant,
    ) -> Result<(UITree, usize), String> {
        let mut s = self.shared.state.lock().unwrap();
        if Instant::now() >= deadline {
            return Err("Point capture deadline expired".into());
        }
        if !s.tree.is_initialized() || s.tree.node(0).1.identity() != *desktop {
            return Err("Point capture desktop changed".into());
        }
        if s.excluded_process == Some(process) {
            return Err("Point capture process excluded".into());
        }
        let identity = observation.properties.identity();
        let existing = s.tree.index_for_identity(&identity);
        if existing.is_none()
            && s.tree
                .indices_for_id(&identity.runtime_id)
                .iter()
                .any(|&id| {
                    let cached = s.tree.node(id).1.identity();
                    cached.handle == identity.handle && !cached.is_resolvable()
                })
        {
            return Err("Point window has ambiguous cached provider identity".into());
        }
        if existing.is_some_and(|id| s.tree.owning_window(id) != id) {
            return Err("Point window ancestry conflicts with cached ancestry".into());
        }
        // Events observed after acquisition started invalidate overlapping evidence.
        // Older dirtiness remains queued; unrelated windows cannot block this hit.
        fn ancestry(
            node: &crate::Observation,
            target: &crate::ElementIdentity,
            path: &mut Vec<crate::ElementIdentity>,
        ) -> bool {
            path.push(node.properties.identity());
            if &node.properties.identity() == target {
                return true;
            }
            if node
                .children
                .iter()
                .flatten()
                .any(|child| ancestry(child, target, path))
            {
                return true;
            }
            path.pop();
            false
        }
        let mut path = Vec::new();
        if !ancestry(&observation, target, &mut path) {
            return Err("Point observation does not contain target".into());
        }
        if started_epoch < s.invalidation_floor
            || s.invalidated_at
                .iter()
                .any(|(identity, &epoch)| epoch > started_epoch && path.contains(identity))
        {
            return Err("Point window invalidated during narrow capture".into());
        }
        let mut candidate = s.tree.clone();
        let window = match existing {
            Some(id) => id,
            None => candidate.discover_window(observation.properties.clone())?,
        };
        candidate.commit(window, observation)?;
        let index = candidate
            .index_for_identity(target)
            .filter(|&id| candidate.is_descendant(id, window))
            .ok_or("Point target is missing or ambiguous in the captured ancestry")?;
        if !candidate.node(index).1.identity().is_resolvable() {
            return Err("Point target has snapshot-only identity".into());
        }
        if Instant::now() >= deadline {
            return Err("Point publication deadline expired".into());
        }
        s.tree = candidate;
        s.discovery_epoch += 1;
        let publication = s.discovery_epoch;
        s.point_publications.insert(window, publication);
        // Do not clear any dirty region: only the spine was acquired.
        let mut result = s.tree.clone();
        if !result.has_complete_subtree(window)
            || s.dirty
                .keys()
                .any(|&id| id == 0 || result.is_descendant(id, window))
        {
            result.restrict_point_locator(index);
        }
        log::debug!(
            "point_lookup published_spine window={} target={} revision={} retained_dirty_regions={}",
            window,
            index,
            result.revision(),
            s.dirty.len()
        );
        self.shared.changed.notify_all();
        drop(s);
        if let Some(wake) = self.shared.waker.lock().unwrap().clone() {
            wake();
        }
        Ok((result, index))
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
        let (request, index) = self.live_request(id, expected)?;
        let live = crate::capture::resolve_live(&request)?;
        if self
            .shared
            .state
            .lock()
            .unwrap()
            .tree
            .try_node(index)
            .is_none_or(|(_, p)| {
                Some(p.identity()) != request.target.as_ref().map(|p| p.identity())
            })
        {
            return Err("Cached element changed during live resolution".into());
        }
        Ok(live)
    }
    fn live_request(
        &self,
        id: &[i32],
        expected: Option<usize>,
    ) -> Result<(CaptureRequest, usize), String> {
        let result = {
            let s = self.shared.state.lock().unwrap();
            let index = match expected {
                Some(token)
                    if s.tree
                        .try_node(token)
                        .is_some_and(|(_, p)| p.get_runtime_id() == id) =>
                {
                    token
                }
                Some(_) => return Err("Cached element was removed or replaced".into()),
                None => s
                    .tree
                    .index_for_id(id)
                    .ok_or("Element no longer cached or runtime ID is ambiguous")?,
            };
            if !s.tree.node(index).1.identity().is_resolvable() {
                return Err("Element has missing or ambiguous provider identity; snapshot-only occurrence cannot be used for actions".into());
            }
            (request(&s, index, CaptureKind::Properties), index)
        };
        Ok(result)
    }
    pub fn invalidate_action(&self, id: &[i32]) {
        let mut s = self.shared.state.lock().unwrap();
        for index in s.tree.indices_for_id(id) {
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
    /// Reconcile possible removal after an action such as Close. For top-level
    /// windows this is shallow desktop membership, never desktop descendants.
    pub fn invalidate_parent_membership(&self, id: &[i32]) {
        let mut s = self.shared.state.lock().unwrap();
        for index in s.tree.indices_for_id(id) {
            let parent = s.tree.get_tree().node(index).parent;
            mark(&mut s, parent, CaptureKind::Children);
            self.shared.changed.notify_all();
        }
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
            {
                let errors = region_errors(&s, |_, _| true);
                if errors.is_empty() {
                    String::new()
                } else {
                    format!(" error={}", errors.join("; "))
                }
            }
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
        self.request_capture(
            index,
            if index == 0 {
                CaptureKind::Children
            } else {
                CaptureKind::Subtree
            },
        );
    }
    /// Nonblocking, shallow expansion of a GUI branch. Unknown grandchildren remain lazy.
    pub fn request_children(&self, index: usize) {
        self.request_capture(index, CaptureKind::Children);
    }
    fn request_capture(&self, index: usize, kind: CaptureKind) {
        let mut s = self.shared.state.lock().unwrap();
        if s.tree.get_tree().has_node(index) {
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
                    reason: {
                        let errors = region_errors(&s, |id, d| {
                            id == 0
                                || s.tree.get_tree().node(id).parent == 0
                                    && d.kind == CaptureKind::Properties
                                || descendants
                                    && roots.iter().any(|&root| {
                                        s.tree.is_descendant(id, root)
                                            || s.tree.is_descendant(root, id)
                                    })
                        });
                        if errors.is_empty() {
                            "query deadline expired".into()
                        } else {
                            format!("query deadline expired; {}", errors.join("; "))
                        }
                    },
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
                    .or_else(|| body.strip_prefix("@AutomationId="))
                    .or_else(|| body.strip_prefix("@NodeKey="))?;
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
            .filter_map(|p| tree.index_for_identity(&p.identity()))
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
    let mut index = index;
    let mut kind = kind;
    // Snapshot-only nodes have no safe live locator. Repair their nearest
    // resolvable parent, preserving ambiguity until a fresh observation replaces it.
    while index != 0
        && s.tree
            .try_node(index)
            .is_some_and(|(_, p)| !p.identity().is_resolvable())
    {
        index = s.tree.get_tree().node(index).parent;
        kind = CaptureKind::Subtree;
    }
    // Desktop-wide invalidation is always decomposed into shallow membership
    // plus window patches; never send a desktop Subtree request implicitly.
    let kind = if index == 0 {
        kind.min(CaptureKind::Children)
    } else {
        kind
    };
    s.epoch += 1;
    // Keep tombstones until bounded eviction: a background removal must not
    // erase evidence that a point capture was invalidated while it was running.
    if s.invalidated_at.len() >= 100_000 {
        s.invalidated_at.clear();
        s.invalidation_floor = s.epoch;
    }
    if let Some((_, properties)) = s.tree.try_node(index) {
        s.invalidated_at.insert(properties.identity(), s.epoch);
    }
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
    let error = s.dirty.get(&index).and_then(|d| d.error.clone());
    s.dirty.insert(
        index,
        Dirty {
            kind,
            epoch: s.epoch,
            retry_at,
            queued_at,
            error,
        },
    );
}
fn region_errors(s: &State, relevant: impl Fn(usize, &Dirty) -> bool) -> Vec<String> {
    let mut errors: Vec<_> = s
        .dirty
        .iter()
        .filter(|(id, d)| s.tree.get_tree().has_node(**id) && relevant(**id, d))
        .filter_map(|(&id, d)| d.error.as_ref().map(|error| (id, error)))
        .collect();
    errors.sort_by_key(|(id, _)| *id);
    errors
        .into_iter()
        .map(|(id, error)| format!("target={id}: {error}"))
        .collect()
}
fn request(s: &State, target: usize, kind: CaptureKind) -> CaptureRequest {
    let window = s.tree.owning_window(target);
    let mut path: Vec<_> = s
        .tree
        .get_tree()
        .get_path_to_element(target)
        .into_iter()
        .map(|i| s.tree.node(i).1.identity())
        .collect();
    // The arena path omits Desktop, which can anchor handle-less top-level nodes.
    if target != 0
        && s.tree.node(target).1.identity().native_anchor() == Some(&s.tree.node(0).1.identity())
    {
        path.insert(0, s.tree.node(0).1.identity());
    }
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
                                && **i != 0
                                && (s.tree.is_descendant(**i, root)
                                    || s.tree.is_descendant(root, **i))
                    });
                    (jobs % 4 != 3 && !relevant, d.epoch)
                })
                .map(|(&id, d)| (id, d.clone()));
            match next {
                Some((id, dirty)) => Some((
                    id,
                    dirty.clone(),
                    request(&s, id, dirty.kind),
                    s.discovery_epoch,
                )),
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
        let Some((id, dirty, request, discovery_epoch)) = job else {
            continue;
        };
        jobs = jobs.wrapping_add(1);
        log::debug!(
            "tree_schedule target={} kind={:?} queue_wait_us={} window_handle={}",
            id,
            dirty.kind,
            dirty.queued_at.elapsed().as_micros(),
            request.window_handle
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            capture.capture(&request, &shared.stop)
        }))
        .unwrap_or_else(|_| Err("Capture worker panicked; coverage retained for retry".into()));
        let phase = if result.is_ok() { "commit" } else { "capture" };
        let mut s = shared.state.lock().unwrap();
        let existing: std::collections::HashSet<_> = s
            .tree
            .get_elements()
            .iter()
            .map(|e| e.get_tree_index())
            .collect();
        s.point_publications.retain(|id, _| existing.contains(id));
        if discovery_epoch != s.discovery_epoch
            && (id == 0
                || s.point_publications.iter().any(|(&window, &epoch)| {
                    epoch > discovery_epoch
                        && (s.tree.is_descendant(id, window) || s.tree.is_descendant(window, id))
                }))
        {
            log::debug!(
                "tree_capture_superseded target={} identity={:?} reason=point_discovery captured_epoch={} current_epoch={}",
                id,
                request.target.as_ref().map(|p| p.identity()),
                discovery_epoch,
                s.discovery_epoch
            );
            shared.changed.notify_all();
            continue;
        }
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
            }
            Err(error) => {
                log::debug!(
                    "tree_repair_failed phase={} target={} kind={:?} runtime_id={:?} name={:?} control_type={:?} window_handle={} revision={} error={:?}",
                    phase,
                    id,
                    request.kind,
                    request.target.as_ref().map(|p| p.get_runtime_id()),
                    request.target.as_ref().map(|p| p.get_name()),
                    request.target.as_ref().map(|p| p.get_control_type()),
                    request.window_handle,
                    s.tree.revision(),
                    error
                );
                if let Some(d) = s.dirty.get_mut(&id) {
                    d.error = Some(format!("{phase}: {error}"));
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

fn excluded_window(handle: isize, excluded_process: Option<u32>) -> bool {
    let Some(excluded) = excluded_process else {
        return false;
    };
    let mut process = 0;
    // Only queries the HWND owner; never calls the accessibility provider.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
            windows::Win32::Foundation::HWND(handle as *mut _),
            Some(&mut process),
        );
    }
    process != 0 && process == excluded
}

fn start_events(shared: Arc<Shared>, excluded_process: Option<u32>) {
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
                move |sender: &uiautomation::UIElement,
                      _: StructureChangeType,
                      _: Option<&[i32]>| {
                    // Cached properties only: callbacks must not reenter a live provider.
                    if let Some(process) = excluded_process
                        && matches!(sender.get_cached_property_value(UIProperty::ProcessId)
                            .ok().and_then(|v| v.get_value().ok()),
                            Some(uiautomation::variants::Value::I4(pid)) if pid == process as i32)
                    {
                        return Ok(());
                    }
                    mark(&mut state.state.lock().unwrap(), 0, CaptureKind::Children);
                    state.changed.notify_all();
                    Ok(())
                },
            )
                as Box<CustomStructureChangedEventHandlerFn>)
                .into();
            let desktop_cache = a
                .create_cache_request()
                .and_then(|cache| {
                    cache.set_tree_scope(TreeScope::Element)?;
                    cache.add_property(UIProperty::ProcessId)?;
                    Ok(cache)
                })
                .ok();
            if let Err(e) = a.add_structure_changed_event_handler(
                &root,
                TreeScope::Children,
                desktop_cache.as_ref(),
                &handler,
            ) {
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
                    if subscriptions.contains_key(owner)
                        || *handle == 0
                        || excluded_window(*handle, excluded_process)
                    {
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
                    log::debug!("tree_subscribe target={} window_handle={}", owner, handle);
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
                        if excluded_window(hwnd.0 as isize, excluded_process) {
                            continue;
                        }
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
    #[test]
    fn snapshot_only_actions_and_descendants_fail_closed_across_refresh() {
        let rows = || {
            obs(
                1,
                Some(vec![obs(
                    2,
                    Some(vec![
                        obs(-122, Some(vec![obs(-11, Some(vec![]))])),
                        obs(-122, Some(vec![obs(-11, Some(vec![]))])),
                        obs(5, Some(vec![])),
                    ]),
                )]),
            )
        };
        let service =
            TreeService::with_capture(UITree::from_observation(rows()).unwrap(), || Fake {
                calls: Arc::default(),
                delay: Duration::ZERO,
            });
        let tree = service.snapshot();
        let weak = tree.indices_for_id(&[42, -122]);
        for raw in [[42, -122], [42, -11]] {
            for token in tree.indices_for_id(&raw) {
                assert!(
                    service
                        .live_request(&raw, Some(token))
                        .unwrap_err()
                        .contains("snapshot-only")
                );
            }
        }
        let healthy = tree.index_for_id(&[42, 5]).unwrap();
        assert!(service.live_request(&[42, 5], Some(healthy)).is_ok());
        {
            let mut s = service.shared.state.lock().unwrap();
            mark(&mut s, weak[0], CaptureKind::Properties);
            let parent = s.tree.index_for_id(&[42, 2]).unwrap();
            assert_eq!(s.dirty[&parent].kind, CaptureKind::Subtree);
            assert!(!s.dirty.contains_key(&weak[0]));
            s.tree.commit(0, rows()).unwrap();
        }
        for token in weak {
            assert!(service.live_request(&[42, -122], Some(token)).is_err());
        }
        assert!(service.live_request(&[42, 5], Some(healthy)).is_ok());
    }

    #[test]
    fn point_publication_preserves_membership_and_rejects_excluded_or_wrong_desktop() {
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, None)]))).unwrap(),
            || Fake {
                calls: Arc::default(),
                delay: Duration::ZERO,
            },
        );
        let desktop = service.snapshot().node(0).1.identity();
        let membership = service.snapshot().coverage(0).unwrap().children_observed_at;
        let window = obs(3, Some(vec![obs(4, None)]));
        let target = window.children.as_ref().unwrap()[0].properties.identity();
        let deadline = Instant::now() + Duration::from_secs(2);
        service.shared.state.lock().unwrap().excluded_process = Some(123);
        assert!(
            service
                .publish_point(&desktop, window.clone(), &target, 123, 0, deadline)
                .is_err()
        );
        assert!(
            service
                .publish_point(&target, window.clone(), &target, 456, 0, deadline)
                .is_err()
        );
        let (tree, _) = service
            .publish_point(&desktop, window, &target, 456, 0, deadline)
            .unwrap();
        assert_eq!(tree.coverage(0).unwrap().children_observed_at, membership);
        assert!(tree.index_for_id(&[42, 2]).is_some());
        assert!(
            !tree
                .coverage(tree.index_for_id(&[42, 4]).unwrap())
                .unwrap()
                .children_observed
        );
    }

    #[test]
    fn point_publication_bypasses_and_supersedes_inflight_window_capture() {
        use std::sync::mpsc;
        struct Blocked {
            started: mpsc::Sender<()>,
            release: mpsc::Receiver<()>,
        }
        impl Capture for Blocked {
            fn capture(
                &mut self,
                _: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                self.started.send(()).unwrap();
                self.release
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|e| e.to_string())?;
                Ok(obs(2, Some(vec![])))
            }
        }
        let (started_tx, started) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, None)]))).unwrap(),
            move || Blocked {
                started: started_tx,
                release: release_rx,
            },
        );
        let window = service.snapshot().index_for_id(&[42, 2]).unwrap();
        service.invalidate(window, CaptureKind::Subtree);
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let epoch = service.point_capture_context().0;
        let desktop = service.snapshot().node(0).1.identity();
        let patch = obs(2, Some(vec![obs(4, None)]));
        let target = patch.children.as_ref().unwrap()[0].properties.identity();
        service
            .publish_point(
                &desktop,
                patch,
                &target,
                456,
                epoch,
                Instant::now() + Duration::from_secs(1),
            )
            .unwrap();
        release.send(()).unwrap();
        // A second capture can start only after the first result was discarded.
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(service.snapshot().index_for_id(&[42, 4]).is_some());
        assert!(
            service
                .shared
                .state
                .lock()
                .unwrap()
                .dirty
                .contains_key(&window)
        );
        drop(release);
    }

    #[test]
    fn point_invalidation_history_survives_background_recovery() {
        let service = TreeService::with_capture(
            UITree::from_observation(obs(1, Some(vec![obs(2, Some(vec![obs(4, None)]))]))).unwrap(),
            || Fake {
                calls: Arc::default(),
                delay: Duration::ZERO,
            },
        );
        let desktop = service.snapshot().node(0).1.identity();
        let patch = obs(2, Some(vec![obs(4, None)]));
        let target = patch.children.as_ref().unwrap()[0].properties.identity();
        let epoch = service.point_capture_context().0;
        {
            let mut s = service.shared.state.lock().unwrap();
            let window = s.tree.index_for_id(&[42, 2]).unwrap();
            mark(&mut s, window, CaptureKind::Properties);
            s.dirty.clear(); // Simulate background recovery while point capture runs.
        }
        assert!(
            service
                .publish_point(
                    &desktop,
                    patch,
                    &target,
                    456,
                    epoch,
                    Instant::now() + Duration::from_secs(1)
                )
                .unwrap_err()
                .contains("invalidated")
        );
    }
    #[test]
    fn action_requests_preserve_occurrence_context_and_desktop_anchor() {
        let mut desktop = obs(1, Some(vec![obs(2, Some(vec![]))]));
        desktop.properties = desktop.properties.with_handle(111);
        let service =
            TreeService::with_capture(UITree::from_observation(desktop).unwrap(), || Fake {
                calls: Arc::default(),
                delay: Duration::ZERO,
            });
        let index = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let (capture, token) = service.live_request(&[42, 2], Some(index)).unwrap();
        assert_eq!(token, index);
        let identity = capture.target.unwrap().identity();
        assert_eq!(identity.ancestor.as_deref(), capture.path.first());
        assert_eq!(capture.path.last(), Some(&identity));
        assert_eq!(capture.path[0].handle, 111);
        {
            let mut s = service.shared.state.lock().unwrap();
            let mut removed = obs(1, Some(vec![]));
            removed.properties = removed.properties.with_handle(111);
            s.tree.commit(0, removed).unwrap();
        }
        assert!(service.live_request(&[42, 2], Some(index)).is_err());
    }

    #[test]
    fn repeated_gui_expansion_is_one_shallow_capture_with_lazy_grandchildren() {
        use std::sync::mpsc;
        struct Expansion {
            started: mpsc::Sender<CaptureKind>,
            release: mpsc::Receiver<()>,
        }
        impl Capture for Expansion {
            fn capture(
                &mut self,
                r: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                self.started.send(r.kind).unwrap();
                self.release.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(obs(2, Some(vec![obs(4, None)])))
            }
        }
        let (started_tx, started) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let tree =
            UITree::from_observation(obs(1, Some(vec![obs(2, None), obs(3, None)]))).unwrap();
        let target = tree.index_for_id(&[42, 2]).unwrap();
        let sibling = tree.index_for_id(&[42, 3]).unwrap();
        let revision = tree.revision();
        let service = TreeService::with_capture(tree, move || Expansion {
            started: started_tx,
            release: release_rx,
        });
        service.request_children(target);
        assert_eq!(
            started.recv_timeout(Duration::from_secs(2)).unwrap(),
            CaptureKind::Children
        );
        for _ in 0..100 {
            service.request_children(target);
        }
        release.send(()).unwrap();
        let mut s = service.shared.state.lock().unwrap();
        while s.tree.revision() == revision {
            let (next, timeout) = service
                .shared
                .changed
                .wait_timeout(s, Duration::from_secs(2))
                .unwrap();
            s = next;
            assert!(!timeout.timed_out(), "Expansion did not publish");
        }
        assert!(
            s.dirty.is_empty(),
            "Repeated frames queued duplicate captures"
        );
        assert!(started.try_recv().is_err());
        assert!(s.tree.coverage(target).unwrap().children_observed);
        assert!(!s.tree.coverage(sibling).unwrap().children_observed);
        let child = s.tree.index_for_id(&[42, 4]).unwrap();
        assert!(!s.tree.coverage(child).unwrap().children_observed);
        assert_eq!(s.tree.get_elements().len(), 4);
    }

    #[test]
    fn repeated_row_action_requests_keep_exact_parent_paths_and_removed_tokens_fail() {
        let row = |id| obs(id, Some(vec![obs(-11, Some(vec![]))]));
        let mut table = obs(2, Some(vec![row(-54), row(-56)]));
        table.properties = table.properties.with_handle(222);
        let tree = UITree::from_observation(obs(1, Some(vec![table]))).unwrap();
        let fields = tree.indices_for_id(&[42, -11]);
        let service = TreeService::with_capture(tree, || Fake {
            calls: Arc::default(),
            delay: Duration::ZERO,
        });
        assert!(service.live_request(&[42, -11], None).is_err());
        for (&token, row_id) in fields.iter().zip([-54, -56]) {
            let (request, resolved) = service.live_request(&[42, -11], Some(token)).unwrap();
            assert_eq!(token, resolved);
            assert_eq!(request.path[1].runtime_id, vec![42, row_id]);
            assert_eq!(
                request.path.last().unwrap().ancestor.as_deref(),
                Some(&request.path[1])
            );
            assert_eq!(
                request.target.unwrap().identity(),
                *request.path.last().unwrap()
            );
        }
        let snapshot = service.snapshot();
        let mut patch = snapshot.observation(snapshot.index_for_id(&[42, 2]).unwrap());
        patch.children.as_mut().unwrap().remove(0);
        service
            .shared
            .state
            .lock()
            .unwrap()
            .tree
            .commit(snapshot.index_for_id(&[42, 2]).unwrap(), patch)
            .unwrap();
        assert!(service.live_request(&[42, -11], Some(fields[0])).is_err());
        assert!(service.live_request(&[42, -11], Some(fields[1])).is_ok());
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
    fn closure_reconciles_only_parent_membership() {
        type ParentCapture = (CaptureKind, Vec<i32>);
        struct RecordParent(Arc<Mutex<Vec<ParentCapture>>>);
        impl Capture for RecordParent {
            fn capture(
                &mut self,
                request: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                let properties = request
                    .target
                    .clone()
                    .unwrap_or_else(|| SaveUIElement::fixture(1, "Desktop", "Pane"));
                self.0
                    .lock()
                    .unwrap()
                    .push((request.kind, properties.get_runtime_id().to_vec()));
                Ok(Observation {
                    properties,
                    children: Some(vec![]),
                })
            }
        }
        for (target, parent_id) in [(2, 1), (3, 2)] {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let captured = calls.clone();
            let service = TreeService::with_capture(
                UITree::from_observation(obs(
                    1,
                    Some(vec![obs(2, Some(vec![obs(3, Some(vec![]))]))]),
                ))
                .unwrap(),
                move || RecordParent(captured),
            );
            service.invalidate_parent_membership(&[42, target]);
            service
                .ensure(Some("App"), Instant::now() + Duration::from_secs(2))
                .unwrap();
            assert_eq!(
                *calls.lock().unwrap(),
                vec![(CaptureKind::Children, vec![42, parent_id])]
            );
            assert!(service.snapshot().index_for_id(&[42, target]).is_none());
        }
    }
    #[test]
    fn aliased_runtime_id_actions_require_token_and_keep_qualified_path() {
        let mut popup = obs(3, Some(vec![]));
        popup.properties = popup.properties.with_handle(111);
        let mut input = obs(3, Some(vec![obs(4, Some(vec![]))]));
        input.properties = input.properties.with_handle(222);
        let tree = UITree::from_observation(obs(
            1,
            Some(vec![obs(2, Some(vec![popup.clone(), input.clone()]))]),
        ))
        .unwrap();
        let popup_token = tree
            .index_for_identity(&popup.properties.identity())
            .unwrap();
        let input_token = tree
            .index_for_identity(&input.properties.identity())
            .unwrap();
        let child_token = tree.index_for_id(&[42, 4]).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let service = TreeService::with_capture(tree, move || Fake {
            calls,
            delay: Duration::ZERO,
        });
        assert!(
            service
                .live_request(&[42, 3], None)
                .unwrap_err()
                .contains("ambiguous")
        );
        for (token, handle) in [(popup_token, 111), (input_token, 222)] {
            let (request, _) = service.live_request(&[42, 3], Some(token)).unwrap();
            assert_eq!(request.target.unwrap().get_handle(), handle);
            assert_eq!(request.path.last().unwrap().handle, handle);
        }
        let (request, _) = service.live_request(&[42, 4], Some(child_token)).unwrap();
        assert!(
            request
                .path
                .iter()
                .any(|identity| identity.runtime_id == [42, 3] && identity.handle == 222)
        );
        {
            let mut s = service.shared.state.lock().unwrap();
            let owner = s.tree.index_for_id(&[42, 2]).unwrap();
            s.tree.commit(owner, obs(2, Some(vec![input]))).unwrap();
        }
        assert!(
            service
                .live_request(&[42, 3], Some(popup_token))
                .unwrap_err()
                .contains("removed or replaced")
        );
        assert_eq!(
            service
                .live_request(&[42, 3], Some(input_token))
                .unwrap()
                .0
                .target
                .unwrap()
                .get_handle(),
            222
        );
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
    fn rejected_commit_error_survives_parent_repair_until_region_recovers() {
        struct RejectUntilFixed(Arc<AtomicBool>);
        impl Capture for RejectUntilFixed {
            fn capture(
                &mut self,
                r: &CaptureRequest,
                _: &AtomicBool,
            ) -> Result<Observation, String> {
                let id = r.target.as_ref().unwrap().get_runtime_id()[1];
                Ok(if id == 1 {
                    obs(1, Some(vec![obs(2, None), obs(3, None)]))
                } else if id == 2 && !self.0.load(Ordering::SeqCst) {
                    obs(99, Some(vec![])) // Invalid target, not tolerable provider ambiguity.
                } else {
                    obs(id, Some(vec![]))
                })
            }
        }
        let fixed = Arc::new(AtomicBool::new(false));
        let capture_fixed = fixed.clone();
        let service = TreeService::with_capture(
            UITree::from_observation(obs(
                1,
                Some(vec![obs(2, Some(vec![])), obs(3, Some(vec![]))]),
            ))
            .unwrap(),
            move || RejectUntilFixed(capture_fixed),
        );
        let target = service.snapshot().index_for_id(&[42, 2]).unwrap();
        let revision = service.snapshot().revision();
        service.invalidate(target, CaptureKind::Subtree);
        let deadline = Instant::now() + Duration::from_secs(3);
        {
            let mut state = service.shared.state.lock().unwrap();
            while state.tree.revision() == revision || state.dirty.contains_key(&0) {
                assert!(Instant::now() < deadline, "parent repair did not complete");
                state = service
                    .shared
                    .changed
                    .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                    .unwrap()
                    .0;
            }
            assert!(
                state.dirty[&target]
                    .error
                    .as_ref()
                    .unwrap()
                    .contains("identity changed")
            );
            mark(&mut state, target, CaptureKind::Subtree);
            assert!(
                state.dirty[&target].error.is_some(),
                "invalidation must preserve the error"
            );
            assert!(region_errors(&state, |id, _| id != target).is_empty());
        }
        let error = service.ensure(None, Instant::now()).unwrap_err();
        assert!(error.reason.contains("identity changed"));
        assert!(error.reason.contains(&format!("target={target}")));
        fixed.store(true, Ordering::SeqCst);
        {
            let mut state = service.shared.state.lock().unwrap();
            state.dirty.get_mut(&target).unwrap().retry_at = Instant::now();
        }
        service.shared.changed.notify_all();
        service
            .ensure(None, Instant::now() + Duration::from_secs(3))
            .unwrap();
        let state = service.shared.state.lock().unwrap();
        assert!(region_errors(&state, |_, _| true).is_empty());
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
        service
            .ensure_query(
                "/*/*[@NodeKey='42-2@0']//Button",
                None,
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            vec![vec![42, 2]],
            "Generated point locators must not expand the query to unrelated windows"
        );
        assert!(!service.snapshot().coverage(2).unwrap().children_observed);
    }
}
