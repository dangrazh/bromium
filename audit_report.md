# Bromium Workspace Audit Report

**Date:** 2026-11-19
**Scope:** Whole workspace (`bromium`, `bromium-common`, `uitree`, `xmlutil`, `uiexplore`, `winevent-monitor`, `screen-capture`), including the uncommitted working tree on `main` (HEAD `4569022`).
**Finding IDs:** `AR-01` … `AR-28`. The `AR-` prefix avoids collisions with earlier audits (`F-`, `CF-`, `R-`, `P-`, `RA-`, `H-/M-/L-`) in `bromium_docs_for_agents/done/`.

---

## 1. Summary

| Severity | Count | IDs |
|---|---|---|
| High | 4 | AR-01, AR-02, AR-03, AR-04 |
| Medium | 11 | AR-05 – AR-15 |
| Low | 13 | AR-16 – AR-28 |
| **Total** | **28** | |

**Baseline health**
- `cargo test`: **210 passed, 0 failed, 2 ignored.**
- `cargo clippy --all-targets`: about 13 warnings, all minor (see AR-26).
- `bromium.pyi` and `README.md` match the current PyO3 surface. All public methods, keyword arguments and exceptions are documented.
- The `TreeService` design has a clear contract: one capture worker, dirty-region tracking, deadline-bound readers and coverage certification. Caller deadlines and the "only a clean no-match may launch" rule are well tested.

**Main themes**
1. **Robustness of the shared tree state.** One bad element name can disable every XPath query (AR-01), and one panic can disable the whole service (AR-02).
2. **Cost of change propagation.** Ordinary desktop activity triggers whole-window recaptures (AR-03). Each commit then clones and re-serializes the entire desktop (AR-04), and each reader clones and re-parses it again (AR-05). These costs multiply together, and they are the main performance lever left.
3. **Liveness under hung providers.** Several single-threaded components can be held up by one unresponsive application (AR-08 – AR-11).

**How the findings were established**
- **Verified:** reproduced, or shown by test or tool output. AR-01 was reproduced with a standalone probe against `xmlutil` and `quick-xml`.
- **Code reading:** follows directly from the code cited.
- **Likely:** depends on runtime or third-party behaviour that was not executed here. These are marked inline.

---

## 2. Findings

### High

#### AR-01 — An invalid XML character in any element property breaks XPath queries for the whole desktop *(Verified)*
- **Location:** `crates/uitree/src/uiexplore_xml.rs:512-532` (`UITree::project`). `crates/xmlutil/src/xpath_eval.rs:28-39`.
- **Description:** Element properties (`Name`, `AutomationId`, `ClassName`, …) are written as XML attributes with `quick-xml`. That library escapes only `& < > " '`. Characters that XML 1.0 forbids (U+0000–U+0008, U+000B, U+000C, U+000E–U+001F, U+FFFE/FFFF) are written unchanged. When the document is parsed for XPath, the parser rejects the whole document.
- **Reproduction:** The probe used a Pane with `Name="esc\u{1b}[0m"`, a terminal title with an ANSI escape. Result: `Failed to parse XML: … a non-XML character '\u{1b}' found`. A `\u{1}` produced the same error.
- **Impact:** One window, tab or list item anywhere on the desktop with such a character makes every `get_element_by_xpath`, `get_elements_by_xpath`, `__contains__`, `launch_or_activate_app` and UI Explore query fail. Terminal titles, log viewers and some custom controls produce these characters routinely. The failure looks like an XPath error, so it is hard to diagnose.
- **Related (Verified):** Newlines and tabs in attribute values are not encoded as `&#10;`/`&#9;`. XML attribute-value normalization turns them into spaces, so `//Pane[@Name='line1\nline2']` never matches the element.
- **Recommendation:** Sanitize every attribute value in `project()`. Replace characters XML forbids with U+FFFD or a visible escape, and encode `\n`, `\r` and `\t` as character references. Add a unit test that commits an observation containing these characters and then queries it.

#### AR-02 — Mutex poisoning stops the tree service, and probably the host process *(Code reading; abort is Likely)*
- **Location:** `crates/uitree/src/service.rs`. There are about 40 `.lock().unwrap()` calls, for example at 149, 195, 919, 1055, 1131 and 1152. `run()` calls `s.tree.commit(...)` while holding the lock and outside `catch_unwind` at line 944. Panicking helpers include `UITree::node()` (`uiexplore_xml.rs:129`), `UITree::view()` (`uiexplore_xml.rs:102,107`) and `UITreeMap` indexing (`tree_map.rs:28-34`).
- **Description:** Only the provider capture call is wrapped in `catch_unwind` (`service.rs:914-917`). Any panic while `state` is locked poisons the mutex. Examples are a stale index reaching `node()` during `commit`/`merge`/`rebuild`, or the `expect`s in `view()`. After that:
  - The capture worker thread is dead, so coverage is never repaired.
  - Every later `TreeService` call panics on `lock().unwrap()`. Through PyO3 this reaches Python as `PanicException`, a `BaseException` subclass that `except Exception` does not catch.
  - The UIA event callbacks also lock with `unwrap()` (`service.rs:1055, 1131, 1152`) and run inside a COM callback. Unless the `uiautomation` wrapper catches unwinds (not verified), a panic there crosses an `extern "system"` boundary and aborts the process. That would kill the user's Python interpreter.
- **Recommendation:** Recover from poisoning with `lock().unwrap_or_else(PoisonError::into_inner)`, or switch to `parking_lot::Mutex`, which does not poison. Put `commit` inside the existing `catch_unwind` and keep the tree unchanged on failure. `commit` already builds a candidate before swapping it in, so this is safe. Make the event-callback bodies panic-free, and wrap them in `catch_unwind` as well.

#### AR-03 — Routine WinEvents trigger whole-window subtree recaptures, and overflow triggers a full-desktop rewalk *(Code reading)*
- **Location:** `crates/winevent-monitor/src/winevent.rs:142-172`. `crates/uitree/src/service.rs:1220-1308`, especially 1225-1230, 1233 and 1301.
- **Description:** The hook subscribes to `ObjectLocationChange`, `ObjectValueChange`, `ObjectReorder`, `ObjectNameChange` and others. The `object_id` filter (`< 0 && != -4`) still lets through `OBJID_WINDOW` (0), `OBJID_CLIENT` (-4) and all positive custom child IDs. Every event that does not match a specific arm falls through to `_ => mark(&mut s, window, CaptureKind::Subtree)`. Examples are location changes, value changes, and name changes with `object_id != 0`. In practice:
  - Typing in an editor, a ticking clock or a progress bar causes a full recapture of that window's subtree.
  - Dragging or resizing a window, scrolling, and Chromium/WebView animations do the same, repeatedly.
  - The channel holds 1024 events. When it overflows, every top-level window is marked `Subtree`, which means a full-desktop rewalk. Scrolling a browser can plausibly overflow it within one 50 ms poll.
- **Impact:** Constant background COM traffic and CPU load in the inspected applications. AR-04 makes this worse, because each recapture is followed by a full-desktop rebuild. It also delays user queries, because the worker is busy.
- **Recommendation:** Map `ObjectLocationChange` and `ObjectValueChange` to `CaptureKind::Properties` on the cached element for that HWND. Ignore location changes for windows whose subtree has not been observed. On overflow, mark only desktop membership and rely on the existing age-based repair for subtrees. Also consider removing `ObjectLocationChange` from the hook, since bounds are already covered by UIA `BoundingRectangle` property events.

#### AR-04 — Each commit clones and re-serializes the entire desktop tree while holding the service lock *(Code reading)*
- **Location:** `crates/uitree/src/uiexplore_xml.rs:229-241` (`commit`) and 466-489 (`rebuild`). `crates/uitree/src/service.rs:944`, plus 259 and 283 in `publish_point_partial`.
- **Description:** `commit` runs for every capture, including a one-element `Properties` refresh. It does `self.clone()` of the whole `UITree`: the arena map, coverage map, XML string, `ui_elements` (one more clone of every `SaveUIElement`) and the runtime-index map. Then `rebuild()` rebuilds the runtime index, re-serializes the entire desktop to XML, re-clones every element's properties into `ui_elements`, and re-sorts them. All of this happens inside `shared.state.lock()`, so Python readers, the GUI and the UIA event callbacks wait for the full O(N) rebuild. A point publication adds two more full clones.
- **Impact:** CPU and allocation cost grows with desktop size for every small change. Lock hold time grows the same way. Combined with AR-03, this is the main steady-state cost of the library.
- **Recommendation:** Publish immutable snapshots as `Arc<UITree>`, so readers clone an `Arc` instead of the tree. Build the candidate outside the lock and swap it in after a revision check. Make XML and `ui_elements` projections lazy and incremental: rebuild only the affected window subtree, or regenerate the XML on first query per revision.

### Medium

#### AR-05 — Each Python query clones the tree several times, re-parses the XML, and partly holds the GIL *(Code reading)*
- **Location:** `crates/bromium/src/windriver.rs:1064-1087` (`prepare_query`), 652-663, 808-841 and 963-966. `crates/uitree/src/service.rs:148`. `crates/uitree/src/uiexplore_xml.rs:94-109`. `crates/xmlutil/src/xpath_eval.rs:28-39`.
- **Description:** One `get_element_by_xpath` iteration does all of the following:
  - `snapshot()`, a full clone under the lock.
  - A validation `query()` on that snapshot, **while holding the GIL**.
  - `ensure_query()`, which returns `tree.view(title)`. That is a second full clone, plus a full `rebuild()` when `window_title` is set.
  - A second `query()`.

  The XPath document cache is thread-local and keyed by comparing the **entire XML string**. Every new revision forces a full re-parse, and every query does an O(len) string comparison and copy. The loop repeats every 100 ms until the timeout. The validation query on the GIL-holding path breaks the CLAUDE.md rule that "every blocking call must release the GIL". On a large desktop it blocks all Python threads during a multi-megabyte parse.
- **Recommendation:** Validate XPath syntax by compiling only, not evaluating, or move validation inside `allow_threads`. Key the parsed-document cache by `(tree identity, revision)` rather than by string content. Cache `view(title)` per revision. Together with AR-04's `Arc` snapshots, this reduces each iteration to roughly one parse per revision.

#### AR-06 — Identity keys grow with element depth, so XML and index size grow with N × depth *(Code reading)*
- **Location:** `crates/uitree/src/save_ui_element.rs:22-34` (`ElementIdentity::key`) and 131-138 (`identity()`). `crates/uitree/src/uiexplore_xml.rs:517` (`NodeKey` attribute).
- **Description:** Elements without a window handle are qualified by their full ancestor chain up to the nearest native window. `key()` builds the chain recursively, which is O(d²) string work. The result is written as `NodeKey` on **every** element and used as the arena map key. `identity()`, `Hash`, `Eq` and `Clone` are all recursive, and `identity()` is called throughout the hot paths in `mark`, `merge`, `request` and `publish_point_partial`. Browser and WebView content has a single HWND and DOM depths of 30–100+. NodeKeys there reach kilobytes each, so the XML and its parsed DOM grow with N × depth.
- **Recommendation:** Give each arena node a compact per-tree NodeKey, such as the node index plus a generation number, and keep a side map to `ElementIdentity`. Represent the ancestor chain as `Arc<ElementIdentity>` or as a parent node index, so cloning and hashing do not walk the whole chain.

#### AR-07 — The capture walker makes about 3–4 cross-process calls per element where about 1 would do *(Code reading)*
- **Location:** `crates/uitree/src/capture.rs:216` (`build_updated_cache` per element), 291-311 (`cache_request` rebuilt per element) and 324-356 (`get_first_child`/`get_next_sibling` without a cache). Line 272 (live `get_process_id`). `crates/uitree/src/save_ui_element.rs:281` (live `get_runtime_id()`).
- **Description:** For each element, the walker makes these separate calls:
  - one sibling-navigation call
  - one `BuildUpdatedCache` call
  - one live `GetRuntimeId` call, because RuntimeId is not in the cache request
  - in the UI Explore process-exclusion mode, one live `ProcessId` call

  When Debug logging is on, it makes two more live `get_runtime_id()` calls (`capture.rs:210-219, 233`). CLAUDE.md names COM call count as the main cost.
- **Recommendation:** Use `IUIAutomationTreeWalker::GetFirstChildBuildCache`/`GetNextSiblingBuildCache` with one reused `UICacheRequest`. This keeps control-view walker semantics. Add `RuntimeId` and `ProcessId` to that request, and read the cached runtime ID in `from_cache`.

#### AR-08 — One hung or very large application can monopolize the single capture worker *(Code reading)*
- **Location:** `crates/uitree/src/service.rs:128` (`capture_timeout` 120 s), 849 (per-request deadline), 852-997 (single worker) and 978-981 (fixed 1 s retry).
- **Description:** All provider work runs on one thread, and each request may run until `capture_timeout`, which is 120 s by default. A failed region is retried after a fixed **1 s** delay, with no exponential backoff and no cap. If an application is hung, the worker spends almost all its time timing out on it. UIA's own transaction timeout bounds each call, but the walk makes many calls. Meanwhile, repairs for other windows and Python `ensure_*` waits stall until their deadlines and fail with `StaleTreeError`. The interest-first scheduling with every 4th job FIFO limits starvation but does not stop it.
- **Recommendation:** Use exponential backoff per region, for example 1 s, 2 s, 4 s and so on up to 60 s, and reset it on success. Use a much shorter per-request deadline for background work than for explicit refreshes. Set the `IUIAutomation2` `ConnectionTimeout`/`TransactionTimeout`. Consider a small pool of workers, one per window.

#### AR-09 — The event thread makes blocking provider calls and registers expensive subtree-scope handlers *(Code reading)*
- **Location:** `crates/uitree/src/service.rs:1110-1218`.
- **Description:** For each new top-level window, the single event thread calls `element_from_handle` and `get_runtime_id()` (live calls), then registers `TreeScope::Subtree` property-change and structure-change handlers. One hung window blocks the whole loop. While it is blocked, other windows are not subscribed, and the WinEvent channel is not drained, so it overflows and triggers AR-03's full rewalk. Subtree-scope property handlers on `BoundingRectangle` for every top-level window are known to be expensive in UIA, both client-side and in providers.
- **Recommendation:** Move subscriptions to a separate thread with a per-window timeout and skip list, so WinEvent draining never waits for providers. Prefer `TreeScope::Element|Children` where it is enough, or subscribe only to windows that callers have queried, using the existing `interest` map.

#### AR-10 — `deadline_worker` can become permanently stuck *(Code reading)*
- **Location:** `crates/bromium/src/deadline_worker.rs:24-55`. Used by `crates/bromium/src/app_control.rs:102-153`.
- **Description:** A single process-wide thread runs every launch and activation job from a queue with 8 slots. COM calls cannot be cancelled. If `set_focus()` blocks on a hung application, the worker never returns. Every later `launch_or_activate_app` waits behind it until its deadline, then fails with `"activation capacity unavailable"` once the queue is full, for the rest of the process lifetime.
- **Recommendation:** When a job passes its deadline, abandon that worker thread and start a new one, with a cap on abandoned threads. Alternatively, spawn one thread per job, bounded by a semaphore.

#### AR-11 — Element actions have no execution deadline *(Code reading)*
- **Location:** `crates/bromium/src/windriver.rs:279-339` and 369-396 (`with_ui_element`).
- **Description:** `send_click`, `send_text`, `close` and similar actions release the GIL, but they can block indefinitely on a hung provider, so the Python call never returns. The stub documents this ("no enforced execution deadline"), but automation scripts generally need a bound.
- **Recommendation:** Run actions on the deadline worker after AR-10 is fixed, using `timeout_ms`. At minimum, set the UIA transaction timeout so provider calls fail instead of hanging.

#### AR-12 — Probable GDI bitmap leak on every `take_screenshot()` *(Code reading; confirm with the GDI object count)*
- **Location:** `crates/screen-capture/src/mswindows/capture.rs:98-147` (`capture_monitor`, used by `Monitor::capture_image` through `impl_monitor.rs:282,315`) and 153-262 (`capture_window` failure path).
- **Description:** `capture_monitor` selects the bitmap into the memory DC and never selects the previous object back. Scope guards drop in reverse order, so `DeleteObject(bitmap)` runs while the bitmap is still selected. That call fails (it logs `"DeleteObject(...) failed"`), and the bitmap leaks. `capture_window` restores the previous object only on the success path. Its "all strategies failed" early return has the same problem.
- **Impact:** One leaked screen-sized bitmap (MBs) per screenshot, and eventually exhaustion of the 10,000 GDI handles allowed per process in long-running automation.
- **Recommendation:** Use a guard that restores the previous selection, declared after the bitmap guard so it drops first. Check the log for `DeleteObject` errors to confirm.

#### AR-13 — Some `WinDriver` methods still block while holding the GIL *(Code reading)*
- **Location:** `crates/bromium/src/windriver.rs:881-947` (`get_screen_context`, `take_screenshot`: monitor enumeration, BitBlt, PNG encoding and file write). 963-966 and 1071-1075 (XPath validation over the full document, see AR-05). 644-649 and 680-725 (`__iter__`/`find_elements` building thousands of `Element`s).
- **Recommendation:** Wrap the blocking Rust work in `py.allow_threads`, and build the Python objects afterwards.

#### AR-14 — Each log record takes several mutexes and flushes to disk *(Code reading)*
- **Location:** `crates/bromium/src/logging.rs:57-90`.
- **Description:** `log()` calls `enabled()`, which locks `LOG_LEVEL`. It then locks `LOG_TO_CONSOLE`, `LOG_TO_FILE` and `LOG_FILE`, and flushes the `BufWriter` after **every** line, which defeats the buffering. Logging runs on the capture worker, the event thread and inside UIA callbacks. At Debug level the walker and scheduler log per element and per job, so all of these threads contend on file I/O.
- **Recommendation:** Store the level and flags in atomics; `log::max_level()` already filters by level. Flush on Error level or on a timer, or write through a channel to a dedicated writer thread.

#### AR-15 — Linear scans and XPath evaluation inside the service lock on hot paths *(Code reading)*
- **Location:** `crates/uitree/src/service.rs`:
  - 1253-1258 and 1275-1283: two O(N) scans by HWND **per WinEvent**
  - 920-926: an O(N) `HashSet` of all indices after every job
  - 589-599 and 713-731: `wait_scopes` re-walks coverage, and `query_roots` evaluates the prefix XPath on every wake, under the lock
  - 311-324 and 406-429: `cached_count`/`status` are O(N × depth)
- **Description:** `query_roots` runs `tree.query(prefix)` while holding `state`. Because of AR-05's string-keyed cache, this can trigger a full XML parse inside the lock, which blocks the capture worker and every UIA event callback.
- **Recommendation:** Keep an HWND → index map in `rebuild()`. Compute prefix roots once per revision, outside the lock. Track live indices incrementally.

### Low

#### AR-16 — XPaths that select attributes or text return "not found" instead of an error
- **Location:** `crates/uitree/src/uiexplore_xml.rs:664-684`.
- **Description:** `query` rewrites every expression as `({xpath})/@NodeKey`, with a special case only for a trailing `/@RtID`. For example, `//Button/@Name` becomes `(//Button/@Name)/@NodeKey`, which is always empty. `get_element_by_xpath` then polls for the full timeout and raises `ElementNotFoundError`.
- **Recommendation:** Reject expressions whose result is not a set of element nodes with `ValueError`. One way is to evaluate the raw expression once and check the node kinds. Alternatively, document the restriction in the stub.

#### AR-17 — `refresh(window_title=…)` silently changes the driver's scope
- **Location:** `crates/bromium/src/windriver.rs:1000-1018`.
- **Description:** Passing a title to `refresh`/`refresh_ui_tree` permanently replaces `self.window_title`. All later queries are then filtered to that title, which is a surprising side effect for a refresh call.
- **Recommendation:** Treat the argument as a one-off scope, or document the behaviour explicitly in `bromium.pyi`/`README.md`.

#### AR-18 — Exception types are inconsistent with the project convention
- **Location:**
  - `crates/bromium/src/screen_context.rs:623-637`: `PyRuntimeError`, which CLAUDE.md says must not leak.
  - `crates/bromium/src/windriver.rs:993-995`: a plain `TimeoutError` for the launch deadline, while other deadline paths raise `StaleTreeError`/`TreeConstructionError`.
- **Recommendation:** Map display-enumeration failures to `AutomationError`. Raise a domain exception that subclasses `TimeoutError` for the launch deadline, so existing `except TimeoutError` code keeps working.

#### AR-19 — Unknown log levels are accepted silently, and one Python docstring is wrong
- **Location:** `crates/bromium/src/windriver.rs:51-53`. `crates/bromium/src/logging.rs:110-121`, 370-372 and 406.
- **Description:** `init_logging` parses with `LevelFilter::from_str`, which rejects `"warning"`. `set_log_level` uses `LogLevel::from`, which accepts it. Both fall back to `Info` for unknown strings without any error. The `#[pyfunction]` docstring for `set_log_level` says it "Accepts a LogLevel enum value or a string", but only `str` is accepted. The `.pyi` stub is correct; the runtime `__doc__` is not.
- **Recommendation:** Use one shared parser and raise `ValueError` on unknown levels. Fix the Rust docstring.

#### AR-20 — Screenshots accumulate in `%TEMP%` without limit
- **Location:** `crates/bromium/src/windriver.rs:901-947`.
- **Recommendation:** Add an optional output path or directory parameter and document the cleanup responsibility, or apply a retention policy.

#### AR-21 — Logging defaults and side effects
- **Location:** `crates/bromium/src/logging.rs:25-28` and 283-294.
- **Description:** The static default level is `Debug`, while the documented default is `Info`. Calling `get_log_file()` creates `~/.bromium/bromium_<ts>.log` even if logging was never initialized.
- **Recommendation:** Default the static level to `Info`. Make `get_log_file()` side-effect free; for example, return an empty string or `None` when no file is set.

#### AR-22 — The legacy runtime-ID fallback walks the whole desktop
- **Location:** `crates/bromium/src/uiauto.rs:271-297`. `crates/uitree/src/save_ui_element.rs:350-367`.
- **Description:** This path is used for user-constructed `Element(...)` objects with `handle=0`. It runs `FindAll` to depth 99 with a Rust-side filter, which costs one live `GetRuntimeId` call per desktop element. It also logs expected misses at `error!` level.
- **Recommendation:** Restrict the search to the window that owns the element, and log misses at `debug!`. Alternatively, deprecate handle-less construction.

#### AR-23 — Point-resolution diagnostics are large, mixed with production logic, and change behaviour at Debug level
- **Location:** `crates/uitree/src/point_probe.rs`, `point_host_probe.rs`, `point_msaa_probe.rs`, `point_mismatch.rs`, `point_diagnostics.rs` and `capture_probe.rs`. Together they are about 2,100 lines including tests.
- **Description:** These probes are enabled by `log::log_enabled!(Debug)`. Turning on Debug logging therefore adds provider calls, changes timing, and can hide the race conditions it is meant to diagnose.
- **Recommendation:** Put them behind a Cargo feature (for example `point-diagnostics`) so release builds behave the same at every log level. Keep only the cheap, read-only logging unconditional.

#### AR-24 — Dead and misleading code
- **Location and recommendation:**
  - `crates/bromium/src/windriver.rs:1088-1094`: `refresh_ui_tree_top_2` is unused. Remove it.
  - `crates/bromium/src/windriver.rs:1095-1107` and 1336-1374: `extract_root_element_hint` exists only under `#[cfg(test)]`, and six tests exercise this test-only helper. Remove them.
  - `crates/uitree/src/walker_common.rs`: not declared as a module, so it is never compiled. Remove it.
  - `crates/uitree/src/uiexplore_xml.rs:739-748`: `get_all_elements_par_xml` now just calls the serial capture, yet `crates/uitree/src/main.rs:82` still benchmarks the two against each other. Deprecate the function and update the benchmark binary.
  - `crates/uiexplore/src/main.rs:176-189`: `launch_start_screen` is `#[allow(dead_code)]`. Remove it, or wire it up.
  - `crates/bromium-common/src/timeout.rs` (doc comment): the comment says keeping `_handle` prevents the thread from being detached. Dropping a `JoinHandle` detaches the thread. Fix the comment.

#### AR-25 — The architecture section of CLAUDE.md describes code that no longer exists
- **Location:** `CLAUDE.md`, sections "uitree::UITree is the central data structure", "Tree construction…", "Scoped refresh in XPath lookups", and the `uiexplore` crate description.
- **Description:** It describes `xml_dom_tree` with `node_to_elem` and an `xpath_cache: Mutex<…>` field, `find_scoped_root_element`, wholesale replacement of `self.ui_tree` with a scoped walk, `append_or_replace_subtree` assembling parallel results, and the splash-screen signal-file handshake. None of these match the current `TreeService` design: capture worker, event thread, dirty/coverage model, point publication and thread-local XPath document cache. Contributors and coding agents that rely on this file will be misled.
- **Recommendation:** Rewrite the section around `service.rs`, `capture.rs` and the coverage/staleness contract, and add the new audit-ID prefix.

#### AR-26 — Tooling hygiene
- **Clippy warnings (about 13):**
  - `crates/uitree/src/capture.rs:72`: items after a test module
  - `crates/uitree/src/point_host.rs:475`, `point_menu.rs:528,560`: casting to the same type
  - `crates/uitree/src/service.rs:1334,1543`: very complex type
  - `crates/uiexplore/src/app_ui.rs:291,849`: single-pattern `match`; unnecessary `clone` for a slice
  - `crates/bromium/src/windriver.rs:1129`: field assignment after `Default::default()`
- **Stray test target:** `crates/bromium/tests/fixture_launcher.rs` has its own `fn main`. Cargo auto-discovers it and compiles it as an integration-test target on every `cargo test`, where the harness ignores `main` and reports 0 tests. Move it to `examples/` or a `[[bin]]`, or set `autotests = false`.
- **Release script:** `ci.ps1` runs `git add .`, which will include the currently untracked `crates/uitree/src/point_identityless.rs` and any other stray files in the release commit.

#### AR-27 — UI Explore evaluates XPath on the GUI thread without caching, and maps results by a non-unique ID
- **Location:** `crates/uiexplore/src/app_ui.rs:778-781` and 809-812.
- **Description:** `xpath_eval::eval_xpath` parses the full desktop XML on the egui thread for every evaluation, which freezes the UI on large desktops. Results are projected through `@RtID`, the raw provider runtime ID, which is not unique for aliased elements. The canonical key is `@NodeKey`.
- **Recommendation:** Use `UITree::query` on the worker thread, and highlight by `NodeKey`.

#### AR-28 — UI Explore starts one unbounded thread per request
- **Location:** `crates/uiexplore/src/app_ui.rs:789-797` and 984-1000.
- **Description:** Every submitted query and every cursor-settle spawns a detached thread. With `verify_xpath` enabled, each hover can start a 120 s full-desktop `verify_locator`/`refresh(None)`. Superseded requests are not cancelled, so threads and capture work pile up.
- **Recommendation:** Use one long-lived request thread with "latest wins" semantics, or skip a new request while one is still pending.

---

## 3. Suggested remediation order

| Phase | Findings | Rationale |
|---|---|---|
| 1 — Correctness and safety | AR-01, AR-02, AR-12 | Small, local fixes. They remove the total-failure modes and a resource leak. |
| 2 — Event-storm containment | AR-03, AR-09, AR-08 | Cuts background work at its source before optimizing the remaining work. |
| 3 — Publication model | AR-04, AR-05, AR-15, AR-06 | Use `Arc` snapshots, lazy or incremental projections, and compact keys. This is the largest change, and most of the gain depends on it. |
| 4 — Capture efficiency | AR-07 | Use BuildCache walker navigation. This can be measured with the existing `tree_capture … elapsed_us` logs. |
| 5 — Liveness and API polish | AR-10, AR-11, AR-13, AR-14, AR-16 – AR-22 | Bounded actions, GIL hygiene and consistent errors. |
| 6 — Maintenance | AR-23 – AR-28 | Feature-gate the diagnostics, remove dead code, refresh CLAUDE.md, fix clippy warnings. |
