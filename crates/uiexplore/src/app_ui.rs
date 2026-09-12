use time::{Duration, OffsetDateTime as DateTime};
use xmlutil::{XpathResult, xpath_eval};

use std::sync::mpsc::{Receiver, channel};
use std::thread;

use eframe::egui;
use egui::Response; // TextBuffer
// use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

#[allow(unused)]
use crate::{AppContext, border_window::BorderWindow};
use uitree::{SaveUIElementXML, UITreeXML};

#[derive(Clone, Debug)]
struct TreeState {
    active_element: Option<SaveUIElementXML>,
    active_ui_element: Option<usize>,
    path_to_active_ui_element: Option<Vec<usize>>,
    refresh_path_to_active_ui_element: bool,
}

impl TreeState {
    fn new() -> Self {
        Self {
            active_element: None,
            active_ui_element: None,
            path_to_active_ui_element: None,
            refresh_path_to_active_ui_element: false,
        }
    }

    fn update_state(&mut self, new_active_element: SaveUIElementXML, new_active_ui_element: usize) {
        // Only update the state if there is a change in the active element
        let is_new = self
            .active_element
            .as_ref()
            .is_none_or(|current| new_active_element.get_runtime_id() != current.get_runtime_id());

        if is_new {
            self.active_element = Some(new_active_element);
            self.active_ui_element = Some(new_active_ui_element);
            self.refresh_path_to_active_ui_element = true;
        }
    }

    fn update_path_to_active_ui_element(&mut self, ui_tree: &UITreeXML) {
        match self.active_ui_element {
            Some(active_ui_element) => {
                let path = ui_tree.get_tree().get_path_to_element(active_ui_element);
                self.path_to_active_ui_element = Some(path);
            }
            None => {
                self.path_to_active_ui_element = None;
            }
        }
        self.refresh_path_to_active_ui_element = false;
    }

    // fn get_active_element_mut(&mut self) -> Option<&mut SaveUIElementXML> {
    //     if self.active_element.is_some() {
    //         self.active_element.as_mut()
    //     } else {
    //         None
    //     }
    // }
}
#[derive(Clone)]
struct AppStatusMsg {
    status_msg: String,
    expiry: Option<DateTime>,
}

impl AppStatusMsg {
    #[allow(dead_code)]
    fn new(msg: String) -> Self {
        AppStatusMsg {
            status_msg: msg,
            expiry: None,
        }
    }

    fn new_with_duration(msg: String, display_for_time: Duration) -> Self {
        let dur = display_for_time;
        let expiry = DateTime::now_utc() + dur;

        AppStatusMsg {
            status_msg: msg,
            expiry: Some(expiry),
        }
    }

    fn has_display_duration(&self) -> bool {
        if let Some(_exp) = self.expiry {
            return true;
        }
        false
    }

    fn is_expired(&self) -> bool {
        let now = DateTime::now_utc();
        if let Some(exp) = self.expiry
            && now > exp
        {
            return true;
        }
        false
    }
}

struct HistoryEntry {
    summary: String,
    entries: Vec<String>,
}

#[derive(Default)]
struct DeduplicatedHistory {
    history: std::collections::VecDeque<HistoryEntry>,
}

impl DeduplicatedHistory {
    fn add(&mut self, summary: String, full: String) {
        if let Some(entry) = self.history.back_mut()
            && entry.summary == summary
        {
            entry.entries.push(full);
            return;
        }
        self.history.push_back(HistoryEntry {
            summary,
            entries: vec![full],
        });
        if self.history.len() > 100 {
            self.history.pop_front();
        }
    }

    fn ui(&self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

                for HistoryEntry { summary, entries } in self.history.iter().rev() {
                    ui.horizontal(|ui| {
                        let response = ui.code(summary);
                        if entries.len() < 2 {
                            response
                        } else {
                            response | ui.weak(format!(" x{}", entries.len()))
                        }
                    })
                    .inner
                    .on_hover_ui(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                        for entry in entries.iter().rev() {
                            ui.code(entry);
                        }
                    });
                }
            });
    }
}

#[derive(Debug)]
enum AppMode {
    Normal,
    NeedsTreeRefresh,
}

#[derive(PartialEq)]
enum DisplayMode {
    Explore,
    XpathTest,
}

// #[allow(dead_code)]
pub struct UIExplorer {
    app_context: AppContext,
    recording: bool,
    show_history: bool,
    highlighting: bool,
    simple_xpath: bool,
    xpath_input: Option<String>,
    xpath_eval_result: Option<XpathResult>,
    xpath_highlighting: bool,
    ui_tree: UITreeXML,
    tree_state: Option<TreeState>,
    history: DeduplicatedHistory,
    status_msg: Option<AppStatusMsg>,
    app_mode: AppMode,
    display_mode: DisplayMode,
    service: uitree::TreeService,
    pending_query: Option<(String, Receiver<Result<UITreeXML, uitree::StaleTree>>)>,
    evaluated_xpath: Option<String>,
    border_window: Option<BorderWindow>,
}

impl UIExplorer {
    #[allow(dead_code)]
    pub fn new(caption: String) -> Self {
        Self::new_with_state(
            caption,
            AppContext::new_from_screen(0.4, 0.8),
            UITreeXML::empty(),
        )
    }

    // #[allow(dead_code)]
    pub fn new_with_state(_caption: String, app_context: AppContext, ui_tree: UITreeXML) -> Self {
        let border_window = BorderWindow::new()
            .map_err(|e| eprintln!("Failed to create border overlay: {e}"))
            .ok();

        Self {
            app_context,
            recording: false,
            show_history: false,
            highlighting: false,
            simple_xpath: false,
            xpath_input: None,
            xpath_eval_result: None,
            xpath_highlighting: false,
            service: uitree::TreeService::from_tree(ui_tree.clone()),
            pending_query: None,
            evaluated_xpath: None,
            ui_tree,
            tree_state: None,
            history: DeduplicatedHistory::default(),
            status_msg: None,
            app_mode: AppMode::Normal,
            display_mode: DisplayMode::Explore,
            border_window,
        }
    }

    #[inline(always)]
    fn render_ui_tree(&mut self, ui: &mut egui::Ui, state: &mut TreeState) {
        let tree = &self.ui_tree;
        Self::render_ui_tree_recursive(ui, tree, 0, state);
    }

    #[inline(always)]
    fn render_ui_tree_recursive(
        ui: &mut egui::Ui,
        tree: &UITreeXML,
        idx: usize,
        state: &mut TreeState,
    ) {
        for &child_index in tree.children(idx) {
            let (name, ui_element) = tree.node(child_index);

            // flag if this is the active element
            let mut is_active_element: bool = false;
            if let Some(active_id) = state.active_ui_element
                && active_id == child_index
            {
                is_active_element = true;
            }

            if tree.children(child_index).is_empty() {
                // Node has no children, so just show a label
                let lbl = egui::Label::new(format!(
                    "  {}{}",
                    name,
                    if tree
                        .coverage(child_index)
                        .is_some_and(|c| c.children_observed)
                    {
                        ""
                    } else {
                        " [not captured]"
                    }
                ))
                .truncate()
                .sense(egui::Sense::click());
                let entry: Response = if is_active_element {
                    let weak_bg_fill = ui
                        .ctx()
                        .theme()
                        .default_visuals()
                        .widgets
                        .inactive
                        .weak_bg_fill;
                    egui::Frame::new()
                        .fill(weak_bg_fill)
                        .show(ui, |ui| {
                            ui.add(lbl).on_hover_cursor(egui::CursorIcon::Default);
                        })
                        .response
                } else {
                    ui.add(lbl).on_hover_cursor(egui::CursorIcon::Default)
                };

                if entry.clicked() {
                    state.update_state(ui_element.clone(), child_index);
                }
                if entry.hovered() {
                    entry.highlight();
                }
            } else {
                // Render children under collapsing header
                let header: egui::CollapsingHeader;

                // truncate long names to avoid UI issues
                let limit: usize = 100;
                let mut short_name: String = name.to_string();
                let name_len = name.chars().count();
                if name_len > limit {
                    short_name = name.chars().take(limit).collect();
                    short_name.push_str("...");
                }

                // println!("Header: {:?} - checking current element: {:?} against path {:?}", name, child_index, state.path_to_active_ui_element);
                if !is_in_path_to_active_element(child_index, &state.path_to_active_ui_element) {
                    // header is not on path, render a standard CollapsingHeader
                    // unless it's the root node (index = 1), in which case we want to show it open by default
                    if child_index == 1 {
                        header = egui::CollapsingHeader::new(short_name)
                            .id_salt(format!("ch_node{}", child_index))
                            .open(Some(true));
                    } else {
                        header = egui::CollapsingHeader::new(short_name)
                            .default_open(false)
                            .id_salt(format!("ch_node{}", child_index))
                    }
                } else {
                    // println!("element is in path");
                    // println!("Element: {:?} ; {:?} is in path ; {:?}", name, child_index, state.path_to_active_ui_element);
                    if is_active_element {
                        // show background to visually highlight the active element
                        header = egui::CollapsingHeader::new(short_name)
                            .id_salt(format!("ch_node{}", child_index))
                            .open(Some(true))
                            .show_background(true);
                    } else {
                        header = egui::CollapsingHeader::new(short_name)
                            .id_salt(format!("ch_node{}", child_index))
                            .open(Some(true));
                    }
                }

                let header_resp = header.show(ui, |ui| {
                    // Recursively render children
                    Self::render_ui_tree_recursive(ui, tree, child_index, state);
                });

                if name_len > limit {
                    header_resp.header_response.clone().on_hover_text(name);
                }
                if header_resp.header_response.clicked() {
                    state.update_state(ui_element.clone(), child_index);
                }
            }
        }
    }

    #[inline(always)]
    fn render_status_bar(&mut self, ctx: &egui::Context) {
        // status bar
        egui::TopBottomPanel::bottom("bottom_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(2.0);

                ui.horizontal(|ui| match self.app_mode {
                    AppMode::Normal => {
                        if let Some(msg) = &self.status_msg {
                            ui.label(&msg.status_msg);
                        } else {
                            ui.label("Ready");
                        }
                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);
                        ui.label(format!(
                            "{} Elements detected",
                            self.ui_tree.get_elements().len()
                        ));
                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);
                        ui.label(format!(
                            "Screeninfo: {}x{} @ {:.1}x",
                            self.app_context.screen_width,
                            self.app_context.screen_height,
                            self.app_context.screen_scale
                        ));
                        ui.add_space(2.0);
                    }
                    _ => {
                        ui.label("Refreshing UI Tree...");
                    }
                });
            });
    }

    #[inline(always)]
    fn render_options_bar(&mut self, ctx: &egui::Context, state: &mut TreeState) {
        // options bar
        egui::TopBottomPanel::top("top_panel").resizable(true).show(ctx, |ui| {

            ui.add_space(4.0);

            // process egui input events
            ui.input(|i| {

                for event in &i.raw.events {

                    if !self.recording && matches!(
                        event,
                        egui::Event::PointerMoved { .. }
                            | egui::Event::MouseMoved { .. }
                            | egui::Event::Touch { .. }
                    )
                {

                    continue;
                }

                    // for the visual event summary
                    if self.show_history {
                        let summary = event_summary(event, &self.ui_tree);
                        let full = format!("{event:#?}");
                        self.history.add(summary, full);
                    }

                    // update the actual active element
                    self.process_event(event, state);
                }
            });

            // render the ui elements
            ui.horizontal(|ui| {

                ui.label(self.service.status());
                ui.label("Mode: ");
                ui.radio_value(&mut self.display_mode, DisplayMode::Explore, "Explore");
                ui.radio_value(&mut self.display_mode, DisplayMode::XpathTest, "Test Xpath");

                // store the previous highligting setting
                let prev_highlight = self.highlighting;

                match self.display_mode {
                    DisplayMode::XpathTest => {

                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);

                        ui.checkbox(&mut self.highlighting, "Show Highlight Rectangle");

                        //skip rendering further options
                    },

                    DisplayMode::Explore => {

                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);

                        ui.label("Incremental updates active");
                        if ui.button("🔄").on_hover_text("Refresh selected region (or desktop membership)").clicked() {
                            self.app_mode = AppMode::NeedsTreeRefresh;
                        }
                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);

                        ui.checkbox(&mut self.simple_xpath, "Simple XPath").on_hover_text("When enabled, the generated XPath will avoid using the Name attribute even if it is unique. This can be useful when a pure positional path is desired.");

                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);

                        ui.checkbox(&mut self.highlighting, "Show Highlight Rectangle");
                        ui.checkbox(&mut self.recording, "Track Cursor").on_hover_text("When enabled, the element under the mouse cursor is automatically selected. Press Escape to disable tracking.");
                        if self.recording {
                            ui.checkbox(&mut self.show_history, "Show Event History");
                        }

                    },
                }

                // When highlighting is toggled off, hide the border overlay
                if prev_highlight && !self.highlighting
                    && let Some(border) = &self.border_window {
                        border.hide();
                }


            });

            ui.add_space(4.0);

            match self.display_mode {
                DisplayMode::XpathTest => {
                    // skip rendering of the history
                },
                DisplayMode::Explore => {
                    // render event history if enabled
                    if self.show_history {
                        ui.add_space(6.0);
                        self.history.ui(ui);
                    }
                }
            }

        });
    }

    #[inline(always)]
    fn render_ui_element_tree_screen(&mut self, ctx: &egui::Context, state: &mut TreeState) {
        // UI tree (or placeholder while updating)
        egui::SidePanel::left("left_panel")
            .min_width(600.0)
            .max_width(1400.0)
            .show(ctx, |ui| {
                // .min_width(300.0).max_width(600.0)
                match self.app_mode {
                    AppMode::Normal => {
                        egui::ScrollArea::vertical()
                            .auto_shrink(false)
                            .show(ui, |ui| {
                                ui.add_space(4.0);
                                // log::debug!("running 'render_ui_tree' function on UIExplorer");
                                self.render_ui_tree(ui, state);
                            });
                    }
                    _ => {
                        ui.centered_and_justified(|ui| {
                            ui.label("Refreshing UI Tree...");
                        });
                    }
                }
            });
    }

    #[inline(always)]
    fn render_ui_element_details_screen(&mut self, ctx: &egui::Context, state: &mut TreeState) {
        // main screen with element details
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                // Optionally render the frame around the active element on the screen
                if state.active_element.is_some() {
                    self.process_highlighting(state);
                }

                if let Some(active_element) = &state.active_element {
                    // display the element properties
                    egui::Grid::new("some_unique_id")
                        .min_col_width(100.0)
                        .max_col_width(800.0)
                        .show(ui, |ui| {
                            ui.label("Name:");
                            ui.label(active_element.get_name());
                            ui.end_row();

                            ui.label("Control Type:");
                            ui.label(active_element.get_control_type().to_owned());
                            ui.end_row();

                            ui.label("Localized Control Type:");
                            ui.label(active_element.get_localized_control_type());
                            if ui.button("📋").clicked() {
                                ui.ctx().copy_text(
                                    active_element.get_localized_control_type().to_owned(),
                                );
                                self.set_status(
                                    "Value copied to clipboard".to_string(),
                                    Duration::seconds(2),
                                );
                            }
                            ui.end_row();

                            ui.label("Framework ID:");
                            ui.label(active_element.get_framework_id());
                            ui.end_row();

                            ui.label("Class Name:");
                            ui.label(active_element.get_classname());
                            if ui.button("📋").clicked() {
                                ui.ctx()
                                    .copy_text(active_element.get_classname().to_owned());
                                self.set_status(
                                    "Value copied to clipboard".to_string(),
                                    Duration::seconds(2),
                                );
                            }
                            ui.end_row();

                            ui.label("Runtime ID:");
                            ui.label(
                                active_element
                                    .get_runtime_id()
                                    .iter()
                                    .map(|x| x.to_string())
                                    .collect::<Vec<String>>()
                                    .join("-"),
                            );
                            ui.end_row();

                            ui.label("Surrounding Rectangle:");
                            ui.label(format!("{:?}", active_element.get_bounding_rectangle()));
                            ui.end_row();

                            ui.label("level:");
                            ui.label(active_element.get_level().to_string());
                            ui.end_row();

                            ui.label("z-order:");
                            ui.label(active_element.get_z_order().to_string());
                            ui.end_row();

                            ui.label("Automation ID:");
                            ui.label(active_element.get_automation_id().to_owned());
                            ui.end_row();

                            let xpath = self
                                .ui_tree
                                .get_xpath_for_element(
                                    state.active_ui_element.unwrap_or(0),
                                    self.simple_xpath,
                                )
                                .unwrap_or_default();
                            ui.label("XPath:");
                            ui.label(xpath.clone());
                            if ui.button("📋").clicked() {
                                ui.ctx().copy_text(xpath);
                                self.set_status(
                                    "XPath copied to clipboard".to_string(),
                                    Duration::seconds(2),
                                );
                            }
                        });
                } else {
                    ui.label("No active element");
                }
            });
        });
    }

    #[inline(always)]
    fn render_xpath_screen(&mut self, ctx: &egui::Context, state: &mut TreeState) {
        let input = self.xpath_input.get_or_insert_with(String::new);
        let mut submit = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("XPath (queries wait up to 5 seconds for relevant cached coverage)");
            let response = ui.text_edit_singleline(input);
            if response.changed() {
                self.xpath_eval_result = None;
                self.evaluated_xpath = None;
                self.xpath_highlighting = false;
                if let Some(border) = &self.border_window {
                    border.hide();
                }
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = Some(input.clone());
            }
            if self.pending_query.is_some() {
                ui.spinner();
                ui.label("Updating relevant coverage…");
            }
            if let Some(result) = &self.xpath_eval_result {
                if result.is_success() {
                    ui.label(format!(
                        "{} matches · revision {}",
                        result.get_result_count(),
                        self.ui_tree.revision()
                    ));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for item in result.get_result_items() {
                            ui.monospace(item.get_item_xml());
                        }
                    });
                } else {
                    ui.label(result.get_error_msg());
                }
            }
        });
        if let Some(expr) = submit.filter(|_| self.pending_query.is_none()) {
            // Validate before scheduling acquisition.
            if self.ui_tree.query(&expr).is_err() {
                self.xpath_eval_result = Some(xpath_eval::eval_xpath(
                    &expr,
                    self.ui_tree.get_xml_dom_tree(),
                ));
                return;
            }
            let service = self.service.clone();
            let (tx, rx) = channel();
            let wake = ctx.clone();
            let query = expr.clone();
            thread::spawn(move || {
                let result = service.ensure_query(
                    &query,
                    None,
                    std::time::Instant::now() + std::time::Duration::from_secs(5),
                );
                let _ = tx.send(result);
                wake.request_repaint();
            });
            self.pending_query = Some((expr, rx));
            self.xpath_eval_result = None;
        }
        if let Some((expr, rx)) = &self.pending_query {
            match rx.try_recv() {
                Ok(Ok(tree)) => {
                    let expr = expr.clone();
                    if self.xpath_input.as_deref() == Some(expr.as_str())
                        && tree.revision() == self.service.revision()
                    {
                        self.ui_tree = tree;
                        self.xpath_eval_result = Some(xpath_eval::eval_xpath(
                            &format!("({expr})/@RtID"),
                            self.ui_tree.get_xml_dom_tree(),
                        ));
                        self.evaluated_xpath = Some(expr);
                    }
                    self.pending_query = None;
                }
                Ok(Err(e)) => {
                    self.set_status(e.to_string(), Duration::seconds(30));
                    self.pending_query = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.set_status(
                        "Query worker disconnected; cached result unavailable".into(),
                        Duration::seconds(30),
                    );
                    self.pending_query = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(expr) = &self.evaluated_xpath
            && let Ok(elements) = self.ui_tree.query(expr)
        {
            if elements.len() == 1 {
                let props = elements[0];
                if let Some(id) = self.ui_tree.index_for_id(props.get_runtime_id()) {
                    state.update_state(props.clone(), id);
                    self.process_highlighting(state);
                }
            } else if let Some(border) = &self.border_window {
                border.hide();
            }
        }
    }

    #[inline(always)]
    fn process_event(&mut self, event: &egui::Event, state: &mut TreeState) {
        match event {
            egui::Event::MouseMoved { .. } => {
                self.track_point(state);
            }
            egui::Event::Key {
                key: egui::Key::Escape,
                pressed: false,
                ..
            } => {
                // physical_key, repeat, modifiers
                // log::debug!("Key event received: {:?}, pressed: {}", key, pressed);
                // check if tracking is enabled, if yes, desable tracking
                // if not, ignore the escape key
                if self.recording {
                    self.recording = false;
                    self.set_status("Tracking disabled".to_string(), Duration::seconds(2));
                } else {
                    self.set_status(
                        "No tracking active, ignoring Escape key".to_string(),
                        Duration::seconds(2),
                    );
                }
            }
            _ => (),
        }
    }

    #[inline(always)]
    fn track_point(&mut self, state: &mut TreeState) {
        let mut point = POINT::default();
        if unsafe { GetCursorPos(&mut point) }.is_err() {
            return;
        }
        let window = uitree::window_at_point(point.x, point.y).and_then(|handle| {
            self.ui_tree
                .children(0)
                .iter()
                .copied()
                .find(|&id| self.ui_tree.node(id).1.get_handle() == handle)
        });
        if let Some(window) = window {
            // Zero wait schedules missing repair; never block an egui frame on COM.
            if let Ok(tree) = self
                .service
                .ensure_region(window, std::time::Instant::now())
                && tree.revision() == self.ui_tree.revision()
                && let Some(element) = uitree::element_at_point(&tree, point.x, point.y)
            {
                state.update_state(
                    element.get_element_props().clone(),
                    element.get_tree_index(),
                );
                return;
            }
        } else {
            self.service.request_region(0);
        }
        state.active_element = None;
        state.active_ui_element = None;
        if let Some(border) = &self.border_window {
            border.hide();
        }
    }

    #[inline(always)]
    fn process_highlighting(&mut self, state: &TreeState) {
        if let Some(border) = &self.border_window {
            if self.highlighting {
                if let Some(active_element) = &state.active_element {
                    let bounds = active_element.get_bounding_rectangle();
                    let rect = RECT {
                        left: bounds.get_left(),
                        top: bounds.get_top(),
                        right: bounds.get_right(),
                        bottom: bounds.get_bottom(),
                    };
                    border.update(rect);
                }
            } else {
                border.hide();
            }
        }
    }

    fn set_status(&mut self, msg: String, duration: Duration) {
        let status_msg = AppStatusMsg::new_with_duration(msg, duration);
        self.status_msg = Some(status_msg);
    }

    fn clear_status(&mut self) {
        self.status_msg = None;
    }
}

impl eframe::App for UIExplorer {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let wake = ctx.clone();
        self.service.set_waker(move || wake.request_repaint());
        // Take ownership of the TreeState to avoid cloning every frame.
        // It is stored back into self.tree_state at the end of update().
        let mut state = self.tree_state.take().unwrap_or_else(TreeState::new);

        if state.refresh_path_to_active_ui_element {
            state.update_path_to_active_ui_element(&self.ui_tree);
            // println!("Path to active ui element {:?} set to : {:?}", state.active_ui_element,  state.path_to_active_ui_element);
        }

        // manage the AppStatusMsg lifecycle
        if let Some(status_msg) = &self.status_msg {
            if status_msg.is_expired() {
                self.clear_status();
            } else if status_msg.has_display_duration() {
                // switch from reactive mode to continuous mode to
                // ensure the status messages is cleared after the
                // specified time, even if there is no event triggered
                ctx.request_repaint();
            }
        }

        // Polling is bounded and independent of pointer activity; queries additionally
        // wake the GUI on completion. Only committed revisions are rendered.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        if matches!(self.app_mode, AppMode::NeedsTreeRefresh) {
            self.service
                .request_region(state.active_ui_element.unwrap_or(0));
            self.app_mode = AppMode::Normal;
        }
        if self.service.revision() != self.ui_tree.revision() {
            let current = self.service.snapshot();
            let selected = state
                .active_element
                .as_ref()
                .map(|p| p.get_runtime_id().to_vec());
            self.ui_tree = current;
            self.xpath_eval_result = None;
            self.evaluated_xpath = None;
            self.xpath_highlighting = false;
            if let Some(id) = selected.and_then(|id| self.ui_tree.index_for_id(&id)) {
                state.active_element = Some(self.ui_tree.node(id).1.clone());
                state.active_ui_element = Some(id);
                state.refresh_path_to_active_ui_element = true;
            } else {
                state = TreeState::new();
                if let Some(border) = &self.border_window {
                    border.hide();
                }
            }
        }
        if let Some(id) = state.active_ui_element
            && !self
                .ui_tree
                .coverage(id)
                .is_some_and(|c| c.children_observed)
        {
            self.service.request_region(id);
        }
        if self.recording {
            self.track_point(&mut state);
        }

        // Rendering the ui

        // options bar
        self.render_options_bar(ctx, &mut state);

        // status bar
        self.render_status_bar(ctx);

        // Check the display mode and swich views as needed

        match self.display_mode {
            DisplayMode::Explore => {
                // UI tree
                self.render_ui_element_tree_screen(ctx, &mut state);

                // main screen with element details
                self.render_ui_element_details_screen(ctx, &mut state);
            }
            DisplayMode::XpathTest => {
                // Xpath testing screen
                self.render_xpath_screen(ctx, &mut state);
            }
        }

        // finally update the state
        self.tree_state = Some(state);
    }
}

fn event_summary(event: &egui::Event, tree: &UITreeXML) -> String {
    match event {
        egui::Event::PointerMoved { .. } => "PointerMoved { .. }".to_owned(),
        egui::Event::MouseMoved { .. } => {
            let cursor_position = unsafe {
                let mut cursor_pos = POINT::default();
                if GetCursorPos(&mut cursor_pos).is_err() {
                    return "Cursor unavailable".into();
                }
                cursor_pos
            };

            if let Some(ui_element_props) =
                uitree::element_at_point(tree, cursor_position.x, cursor_position.y)
            {
                // format!("MouseMoved {{ x: {}, y: {} }} over {}", cursor_position.x, cursor_position.y, ui_element_props.name)
                let ui_element_props = ui_element_props.get_element_props();
                let control_type: String = ui_element_props.get_control_type().to_string();
                format!(
                    "MouseMoved over {{ name: '{}', control_type: '{}' bounding_rect: {} }}",
                    ui_element_props.get_name(),
                    control_type,
                    ui_element_props.get_bounding_rectangle()
                )
            } else {
                // format!("MouseMoved {{ x: {}, y: {} }} ", cursor_position.x, cursor_position.y)
                "MouseMoved { .. }".to_owned()
            }
        }
        egui::Event::Zoom { .. } => "Zoom { .. }".to_owned(),
        egui::Event::Touch { phase, .. } => format!("Touch {{ phase: {phase:?}, .. }}"),
        egui::Event::MouseWheel { unit, .. } => format!("MouseWheel {{ unit: {unit:?}, .. }}"),

        _ => format!("{event:?}"),
    }
}

fn is_in_path_to_active_element(
    current_element: usize,
    path_to_active_element: &Option<Vec<usize>>,
) -> bool {
    if path_to_active_element.is_none() {
        return false;
    }

    let path_to_active_element = path_to_active_element.as_ref().unwrap();
    for &index in path_to_active_element {
        if index == current_element {
            return true;
        }
    }
    false
}
