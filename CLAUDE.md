# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Rust workspace (edition 2024, resolver 3) with two user-facing products built on the Windows UI Automation COM API:

1. **`bromium`** — a PyO3/maturin Python extension module (`import bromium`) for automating Windows desktop apps.
2. **`uiexplore`** — an eframe/egui desktop app (inspect.exe-like) for browsing the live desktop UI tree and testing XPath locators.

Everything is Windows-only and COM-bound; most code paths cannot be exercised on other platforms.

## Commands

```powershell
cargo build                       # whole workspace
cargo build --release
cargo build --profile release-with-debug   # release + debug symbols
cargo test                        # unit tests (all in-crate #[cfg(test)] modules)
cargo test -p uitree              # single crate
cargo test -p xmlutil test_eval_xpath      # single test by name
cargo clippy --all-targets
cargo fmt

cargo run -p uiexplore            # launch the UI Explore desktop app
```

Python extension (from `crates/bromium`, with the local venv activated):

```powershell
maturin develop                   # build + install into the active venv
maturin build --release
python tests/app_start_danipc.py  # the ad-hoc integration scripts live in tests/ and examples/
```

`ci.ps1 -Patch | -Minor` is the release pipeline: bumps `crates/bromium/Cargo.toml` version, `cargo build --release`, `git add . && git commit`, then `maturin develop`/`build --release`/`publish`. It hardcodes the workspace path and expects a venv at `crates/bromium/.pyo3venv`. **It commits and publishes to PyPI — never run it to "just build".**

There are no Rust integration-test directories; `crates/bromium/tests/` and `crates/bromium/examples/` hold Python scripts run by hand against real applications.

## Crate graph

```
bromium (cdylib+rlib, PyO3)  ─┬─> uitree ──┬─> xmlutil
                              ├─> screen-capture
                              └─> bromium-common
uiexplore (bin: uiexplore, start_screen) ─┬─> uitree, xmlutil
                                          ├─> winevent-monitor
                                          └─> bromium-common
```

- **`bromium-common`** — shared primitives: `get_ui_automation_instance()` (the single `UIAutomation` entry point), `RuntimeIdFilter`, `execute_with_timeout`, `format_runtime_id` / `format_runtime_id_dotted` (canonical `"1-2-3"` formatting, `"0-0-0-0"` for empty), `rectangle`.
- **`uitree`** — walks the live UI Automation tree and produces a `UITree`.
- **`xmlutil`** — XML serialization, XPath evaluation (`xee-xpath` + `xot`/`roxmltree`), and XPath *generation* from a runtime ID.
- **`screen-capture`** — standalone Win32 screenshot/monitor/window/video capture library.
- **`winevent-monitor`** — polling wrapper over `win_event_hook`; `check_for_events()` drains queued WinEvents (used by UI Explore to react to desktop changes).
- **`uiexplore`** — the GUI. `main.rs` builds the tree on a worker thread while `bin/start_screen.rs` shows a splash window as a *separate process*; the two coordinate through `signal_file` (parent writes a signal file named by the child's PID to tell it to close).

## Core architecture

### `uitree::UITree` is the central data structure

One tree build produces four parallel representations that must stay consistent:

- `tree: UITreeMap<()>` — arena/index-based tree in `tree_map.rs`. Removal is a **tombstone** (`is_alive = false`), so indices stay stable; any traversal must skip dead nodes.
- `xml_dom_tree: String` — the whole desktop serialized as XML (`quick-xml`), which is what XPath queries actually run against.
- `ui_elements: Vec<UIElementInTree>` — the flat list of elements with their properties.
- `node_to_elem: Vec<usize>` — maps tree-node index → position in `ui_elements`, built by runtime ID with a tree-index fallback for elements whose runtime ID is empty.
- `xpath_cache: Mutex<Option<XpathDocCache>>` — a parsed-document cache so repeated XPath queries don't re-parse the XML string. It is deliberately **not** cloned (`Clone` resets it to `None`).

If you change how nodes are added/removed, rebuild `node_to_elem` (`rebuild_node_to_elem`) and invalidate the XPath cache.

### Tree construction is always off-thread and cancellable

`get_all_elements_xml(tx, root_element, max_depth, calling_window_caption, target_window_caption, cancel)` and its parallel sibling `get_all_elements_par_xml` send their result over an `mpsc::Sender`. Note the asymmetry of the two window-caption parameters: `calling_window_caption` **excludes** that window (so the tool doesn't walk itself), `target_window_caption` restricts the walk to only that window.

Callers spawn the walker on a thread, `recv_timeout` on the channel, and on timeout set the shared `Arc<AtomicBool>` cancel flag so the orphaned walker unwinds instead of leaking. Follow this pattern for any new tree build — the walker checks the flag at each step and a send after the receiver is gone must not panic.

### Every blocking call must release the GIL

In `crates/bromium/src/windriver.rs`, all tree construction, waits, and sleeps are wrapped in `py.allow_threads(...)`. A blocking COM call made while holding the GIL freezes the caller's whole Python process. Preserve this whenever adding a `WinDriver` method that can block.

### Scoped refresh in XPath lookups

`WinDriver::get_element_by_xpath` first queries the cached tree. On a miss (and a non-zero timeout) it loops: `find_scoped_root_element(&xpath)` parses a `Window[@Name='…']` / `Pane[@Name='…']` predicate out of the XPath and, when a matching top-level element is found, the retry walk is rooted at that element instead of the desktop. This is the main performance lever — a full desktop walk is thousands of cross-process COM round-trips.

Note the consequence: the scoped result **replaces** `self.ui_tree` wholesale, so after a scoped retry the driver's tree (and its XML document element) is rooted at that window, not at the desktop `Pane`. Absolute XPaths that start above the scoped root will no longer match. `UITree::append_or_replace_subtree` — which merges a subtree by runtime ID into an existing tree — is *not* used here; it is used by the parallel walker `get_all_elements_par_xml` to assemble per-window subtrees, and it is the only place that invalidates the XPath cache and rebuilds `node_to_elem`.

### COM call cost dominates everything

Each property read on a `UIElement` is a cross-process RPC. When touching the walker, batch property reads, avoid cloning COM smart pointers, and don't add per-element allocations. `bromium_docs_for_agents/done/PERF_UITREE_REPORT.md` documents the measured hot paths (findings `P-01`…`P-08`).

## Conventions

- Shared third-party crates go in the root `[workspace.dependencies]` and are referenced as `foo.workspace = true`. Add new Win32 APIs by extending the `windows` feature list there, not by adding a second `windows` dependency.
- Crates that use `unsafe` (`bromium`, `bromium-common`) set `#![deny(unsafe_op_in_unsafe_fn)]` — keep explicit `unsafe {}` blocks inside `unsafe fn`.
- Python-facing errors map to the custom exceptions in `crates/bromium/src/exceptions.rs`: `ElementNotFoundError`, `AutomationError`, `TreeConstructionError` (a `TimeoutError` subclass). Don't leak `PyRuntimeError` from `WinDriver`/`Element` methods.
- `crates/bromium/bromium.pyi` is the hand-maintained type stub and `README.md` the hand-maintained API reference — both must be updated when the PyO3 surface changes.
- Code comments reference audit finding IDs (`F-05`, `R-P04`, `CF-28`); those trace back to the reports in `bromium_docs_for_agents/done/`. Keep the reference when editing such a line.
- `uitree` keeps deprecated aliases (`UITreeXML`, `SaveUIElementXML`, `UIElementInTreeXML`) pointing at the unified types; prefer the canonical names in new code.
