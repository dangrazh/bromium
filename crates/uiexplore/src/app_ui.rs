use time::{Duration, OffsetDateTime as DateTime};
use xmlutil::{xpath_eval, XpathResult};

use std::thread;
use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use egui::Response;  // TextBuffer
// use egui_code_editor::{CodeEditor, ColorTheme, Syntax};

use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::Win32::Foundation::{POINT, RECT};


#[allow(unused)]
use crate::{rectangle, AppContext}; //winevent
use uitree::{get_all_elements_xml, SaveUIElementXML, UIElementInTreeXML, UITreeXML }; //SaveUIElement, 
use winevent_monitor::{WinEventMonitor};

#[derive(Clone, Debug)]
struct TreeState {
    active_element: Option<SaveUIElementXML>,
    prev_element: Option<SaveUIElementXML>,
    clear_frame: bool,
    active_ui_element: Option<usize>,
    path_to_active_ui_element: Option<Vec<usize>>,
    refresh_path_to_active_ui_element: bool,
}

impl TreeState {
    fn new() -> Self {
        Self {
            active_element: None,
            prev_element: None,
            clear_frame: false,
            active_ui_element: None,
            path_to_active_ui_element: None,
            refresh_path_to_active_ui_element: false,
        }
    }

    fn update_state(&mut self, new_active_element: SaveUIElementXML, new_active_ui_element: usize) {

        // if we have an active element, check if the new one is different and if yes,
        // update the state to reflect the change
        if let Some(current_element) = &self.active_element {
            // only update the state if there is a change in the active element
            if new_active_element.get_element().get_runtime_id() != current_element.get_element().get_runtime_id() {
                self.prev_element = Some(current_element.clone());
                self.clear_frame = true;    
                self.active_element = Some(new_active_element);
                self.active_ui_element = Some(new_active_ui_element);
                self.refresh_path_to_active_ui_element = true;
            }
        } else {
            // there was no active element, so set the active element, 
            // and the active ui element to the proviced values and
            // default the prev element to the provided active
            self.prev_element = Some(new_active_element.clone());
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
        
            },
            None => {
                self.path_to_active_ui_element  = None;
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
        if let Some(exp) = self.expiry {
            if now > exp {
                return true;
            }
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
        if let Some(entry) = self.history.back_mut() {
            if entry.summary == summary {
                entry.entries.push(full);
                return;
            }
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
// #[allow(dead_code)]
struct LastRefresh {
    time: std::time::Instant,
}
#[derive(Debug)]
enum AppMode {
    Normal(LastRefresh),
    NeedsTreeRefresh,
    IsRefreshingTree(Receiver<UITreeXML>),
}

#[derive(PartialEq)]
enum DisplayMode { Explore, XpathTest }


// #[allow(dead_code)]
pub struct UIExplorer {
    app_name: String,
    app_context: AppContext,
    recording: bool,
    show_history: bool,
    highlighting: bool,
    auto_refresh: bool,
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
    winevent_monitor: WinEventMonitor,
}

impl UIExplorer {
    #[allow(dead_code)]
    pub fn new(caption: String) -> Self {

        // get the ui tree in a separate thread
        let app_name = caption.clone();
        let (tx, rx): (Sender<_>, Receiver<UITreeXML>) = channel();
        thread::spawn(|| {
            get_all_elements_xml(tx, None, None, Some(app_name));
        });

        let ui_tree = rx.recv().unwrap();
        let app_context = AppContext::new_from_screen(0.4, 0.8);

        Self {
            app_name: caption,
            app_context,
            recording: false,
            show_history: false,
            highlighting: false,
            auto_refresh: false,
            simple_xpath: false,
            xpath_input: None,
            xpath_eval_result: None,
            xpath_highlighting: false,
            ui_tree,
            tree_state: None,
            history: DeduplicatedHistory::default(),
            status_msg: None,
            app_mode: AppMode::Normal(LastRefresh { time: std::time::Instant::now() }),
            display_mode: DisplayMode::Explore,
            winevent_monitor: WinEventMonitor::new(),
        }


    }
    
    // #[allow(dead_code)]
    pub fn new_with_state(caption: String, app_context: AppContext, ui_tree: UITreeXML) -> Self {

        Self {
            app_name: caption,
            app_context,
            recording: false,
            show_history: false,
            highlighting: false,
            auto_refresh: false,
            simple_xpath: false,
            xpath_input: None,
            xpath_eval_result: None,
            xpath_highlighting: false,
            ui_tree,
            tree_state: None,
            history: DeduplicatedHistory::default(),
            status_msg: None,
            app_mode: AppMode::Normal(LastRefresh { time: std::time::Instant::now() }),
            display_mode: DisplayMode::Explore,
            winevent_monitor: WinEventMonitor::new(),
        }
    }

    #[inline(always)]
    fn render_ui_tree(&mut self, ui: &mut egui::Ui, state: &mut TreeState) {
        let tree = &self.ui_tree;
        Self::render_ui_tree_recursive(ui, tree, 0, state);
    }

    #[inline(always)]
    fn render_ui_tree_recursive(ui: &mut egui::Ui, tree: &UITreeXML, idx: usize, state: &mut TreeState) {
        
        for &child_index in tree.children(idx) {
            let (name, ui_element) = tree.node(child_index);

            // flag if this is the active element
            let mut is_active_element: bool = false;
            if let Some(active_id) = state.active_ui_element {
                if active_id == child_index {
                    is_active_element = true;
                }
            }
            
            if tree.children(child_index).is_empty() {
                // Node has no children, so just show a label
                let lbl = egui::Label::new(format!("  {}", name)).truncate();
                let entry: Response;
                // let entry = ui.label(format!("  {}", name)).on_hover_cursor(egui::CursorIcon::Default);
                if is_active_element{
                    // show background to visually highlight the active element
                    let weak_bg_fill = ui.ctx().theme().default_visuals().widgets.inactive.weak_bg_fill;        
                    let tmp_entry = egui::Frame::new()
                    .fill(weak_bg_fill) 
                    .show(ui, |ui| {
                       ui.add(lbl).on_hover_cursor(egui::CursorIcon::Default);
                    });
                    entry = tmp_entry.response;
                } else {
                    // render standard label without any visual highlights
                    entry = ui.add(lbl).on_hover_cursor(egui::CursorIcon::Default);                    
                }
                
                if entry.clicked() {
                    state.update_state(ui_element.clone(), child_index);
                }
                if entry.hovered() {
                    entry.highlight();                    
                }
            }
            else {
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
                        .open(Some(false))
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
                
                let header_resp = header
                    .show(ui, |ui| {
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
        egui::TopBottomPanel::bottom("bottom_panel").resizable(false).show(ctx, |ui| {

            ui.add_space(2.0);
        
            ui.horizontal(|ui| {
                
                match self.app_mode {
                    AppMode::Normal(_) => {

                        if let Some(msg) = &self.status_msg {
                            ui.label(&msg.status_msg);
                        } else {
                            ui.label("Ready");
                        }
                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);
                        // ui.vertical(|ui| {
                        ui.add_space(2.0);
                        ui.label(" | ");
                        ui.add_space(2.0);
                        ui.label(format!("{} Elements detected | Screeninfo: {}x{} @ {:.1}x", self.ui_tree.get_elements().len(), self.app_context.screen_width, self.app_context.screen_height, self.app_context.screen_scale));
                        // ui.label(format!("Elements detected: {}", self.ui_tree.get_elements().len()));                  
                        // });
                        ui.add_space(2.0);
                    },
                    _ => {
                        ui.label("Refreshing UI Tree...");
                    },
                }
            });
        });

    }

    #[inline(always)]
    fn render_options_bar(&mut self, ctx: &egui::Context, mut state: &mut TreeState) {

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

                    // update the last refresh time to avoid excessive refreshes
                    // when auto_refresh is set and mouse is moving
                    if self.auto_refresh {
                        match self.app_mode {
                            AppMode::Normal(_) => {
                                // only process mouse move events in normal mode
                                self.app_mode = AppMode::Normal(LastRefresh { time: std::time::Instant::now() });
                            },
                            _ => (), // only process mouse move events in normal mode
                        }
                    }
                    continue;
                }
                    
                    // for the visual event summary
                    if self.show_history {
                        let summary = event_summary(event, self.ui_tree.get_elements());
                        let full = format!("{event:#?}");
                        self.history.add(summary, full);    
                    }

                    // update the actual active element
                    self.process_event(event, &mut state);
                }
            });
    
            // render the ui elements
            ui.horizontal(|ui| {
                
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

                        if ui.checkbox(&mut self.auto_refresh, "Auto Refresh").on_hover_text("When enabled, the UI tree is automatically refreshed when changes are detected in the Windows UI Tree.").clicked() {
                            if self.auto_refresh {
                                match self.app_mode {
                                    AppMode::Normal(_) => {
                                        // only process mouse move events in normal mode
                                        self.app_mode = AppMode::Normal(LastRefresh { time: std::time::Instant::now() });
                                    },
                                    _ => (), // only process mouse move events in normal mode
                                }
                                self.set_status("Auto Refresh enabled".to_string(), Duration::seconds(2));
                            } else {
                                self.set_status("Auto Refresh disabled".to_string(), Duration::seconds(2));
                            }
                        }
                        // only show the refresh button when auto refresh is disabled
                        if !self.auto_refresh {
                            if ui.button("🔄").on_hover_text("Refresh").clicked() {
                                self.app_mode = AppMode::NeedsTreeRefresh;
                                self.set_status("Refreshing UI Tree...".to_string(), Duration::seconds(5));
                            }    
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

                // manage the highlighting lifecycle
                let new_highlight = self.highlighting;
                
                // clear any highlighted surrounding rectangle as 
                if new_highlight != prev_highlight && new_highlight == false {
                    printfmt!("Old highlight value was {}, new one is {}", prev_highlight, new_highlight);
                    let rect: RECT = RECT { 
                        left: 0, 
                        top: 0, 
                        right: self.app_context.screen_width, 
                        bottom: self.app_context.screen_height, 
                    };
                    rectangle::clear_frame(rect).unwrap();
                    state.clear_frame = false;
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
    fn render_ui_element_tree_screen(&mut self, ctx: &egui::Context, mut state: &mut TreeState) {

        // UI tree (or placeholder while updating)
        egui::SidePanel::left("left_panel")
        .min_width(600.0)
        .max_width(1400.0)                
        .show(ctx, |ui| { // .min_width(300.0).max_width(600.0)
            match self.app_mode {
                AppMode::Normal(_) => {
                    egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        // printfmt!("running 'render_ui_tree' function on UIExplorer");
                        self.render_ui_tree(ui, &mut state);
        
                    });
        
                },
                _ => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Refreshing UI Tree...");
                    });
                },
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
                    // // Optionally render the frame around the active element on the screen
                    // if self.highlighting {
                    //     let left: f32 = active_element.get_element().get_bounding_rectangle().get_left() as f32 * self.app_context.screen_scale;
                    //     let top: f32 = active_element.get_element().get_bounding_rectangle().get_top() as f32 * self.app_context.screen_scale;
                    //     let right: f32 = active_element.get_element().get_bounding_rectangle().get_right() as f32 * self.app_context.screen_scale;
                    //     let bottom: f32 = active_element.get_element().get_bounding_rectangle().get_bottom() as f32 * self.app_context.screen_scale;

                    //     let rect: RECT = RECT { 
                    //         left: left as i32, 
                    //         top: top as i32, 
                    //         right: right as i32, 
                    //         bottom: bottom as i32, 
                    //     };
                        
                    //     if let Some(prev_element) = &state.prev_element {
                    //         let prev_left: f32 = prev_element.get_element().get_bounding_rectangle().get_left() as f32 * self.app_context.screen_scale;
                    //         let prev_top: f32 = prev_element.get_element().get_bounding_rectangle().get_top() as f32 * self.app_context.screen_scale;
                    //         let prev_right: f32 = prev_element.get_element().get_bounding_rectangle().get_right() as f32 * self.app_context.screen_scale;
                    //         let prev_bottom: f32 = prev_element.get_element().get_bounding_rectangle().get_bottom() as f32 * self.app_context.screen_scale;

                    //         let prev_rect: RECT = RECT {
                    //             left: prev_left as i32, 
                    //             top: prev_top as i32, 
                    //             right: prev_right as i32, 
                    //             bottom: prev_bottom as i32,     
                    //         };
                    //         if state.clear_frame { //rect != prev_rect && 
                    //             printfmt!("Cleanup needed - new: {:?} vs old: {:?}", rect, prev_rect);
                    //             rectangle::clear_frame(prev_rect).unwrap();
                    //             rectangle::draw_frame(rect, 4).unwrap();
                    //             state.clear_frame = false;
                    //         } else {
                    //             rectangle::draw_frame(rect, 4).unwrap();
                    //         }
                    //     } else {
                    //         rectangle::draw_frame(rect, 4).unwrap();
                    //     }
                    // } 
                    
                    // display the element properties 
                    egui::Grid::new("some_unique_id").min_col_width(100.0).max_col_width(800.0)
                    .show(ui, |ui| {
                        ui.label("Name:");
                        ui.label(active_element.get_element().get_name());
                        ui.end_row();
                    
                        ui.label("Control Type:");
                        ui.label(active_element.get_element().get_control_type().to_owned());
                        ui.end_row();

                        ui.label("Localized Control Type:");
                        ui.label(active_element.get_element().get_localized_control_type());
                        if ui.button("📋").clicked() {
                            ui.ctx().copy_text(active_element.get_element().get_localized_control_type().to_owned());
                            self.set_status("Value copied to clipboard".to_string(), Duration::seconds(2));
                        }
                        ui.end_row();

                        ui.label("Framework ID:");
                        ui.label(active_element.get_element().get_framework_id());
                        ui.end_row();

                        ui.label("Class Name:");
                        ui.label(active_element.get_element().get_classname());
                        if ui.button("📋").clicked() {
                            ui.ctx().copy_text(active_element.get_element().get_classname().to_owned());
                            self.set_status("Value copied to clipboard".to_string(), Duration::seconds(2));
                        }
                        ui.end_row();

                        ui.label("Runtime ID:");
                        ui.label(active_element.get_element().get_runtime_id().iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-"));
                        ui.end_row();

                        ui.label("Surrounding Rectangle:");
                        ui.label(format!("{:?}", active_element.get_element().get_bounding_rectangle()));
                        ui.end_row();
                        
                        ui.label("level:");
                        ui.label(active_element.get_level().to_string());
                        ui.end_row();
                        
                        ui.label("z-order:");
                        ui.label(active_element.get_z_order().to_string());
                        ui.end_row();

                        ui.label("Automation ID:");
                        ui.label(active_element.get_element().get_automation_id().to_owned()); 
                        ui.end_row();


                        let xpath = self.ui_tree.get_xpath_for_element(state.active_ui_element.unwrap_or(0), self.simple_xpath);
                        ui.label("XPath:");
                        ui.label(xpath.clone());
                        if ui.button("📋").clicked() {
                            ui.ctx().copy_text(xpath);
                            self.set_status("XPath copied to clipboard".to_string(), Duration::seconds(2));
                        }
                    });    

                }
                else {
                    ui.label("No active element");
                }

            });

    
        });

    }

    #[inline(always)]
    fn render_xpath_screen(&mut self, ctx: &egui::Context, state: &mut TreeState) {

        // Store the input in a struct field to persist between frames
        if self.xpath_input.is_none() {
            self.xpath_input = Some(String::new());
        }
        let xpath_input = self.xpath_input.as_mut().unwrap();

        let screen_size = ctx.screen_rect();
        let screen_width = screen_size.width();
        let elem_width = screen_width * 0.9;

        egui::CentralPanel::default().show(ctx, |ui| {
            // let mut result = "".to_string();
            let placeholder = "Enter the xpath expression you want to test and press the <ENTER> key".to_string();

            ui.add_space(4.0);
            // Text edit with hint text
            let response = ui.add(
                egui::TextEdit::singleline(xpath_input)
                    .hint_text(placeholder)
                    .desired_width(elem_width)
            );

            // Render the theme selector
            let mut theme =
                egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());

            ui.collapsing("Theme", |ui| {
                ui.group(|ui| {
                    theme.ui(ui);
                    theme.clone().store_in_memory(ui.ctx());
                });
            });


            // Check if Enter was pressed while the text edit had focus 
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                
                // Use the entered text and evaluate the expression
                let expr = xpath_input.clone();
                let srcxml = self.ui_tree.get_xml_dom_tree().to_owned(); // your XML source
                let eval_result = xpath_eval::eval_xpath(expr, srcxml);
                self.xpath_eval_result = Some(eval_result);

            }

            if let Some(outcome) = self.xpath_eval_result.clone() {
                ui.add_space(8.0);
              
                // render the result
                if !outcome.is_success() {                    
                    // display the error message
                    ui.add(egui::TextEdit::multiline(&mut outcome.get_error_msg())
                                                    .desired_width(elem_width)
                                                    .code_editor()
                        );                    
                } else {
                    // display the result
                    let res_cnt = outcome.get_result_count();
                    let item_cnt = format!("Number of items matching expression: {}", res_cnt);
                    let mut itms: String;
                    if res_cnt > 1 {
                        itms = outcome.get_result_items().iter().map(|s| s.get_item_xml()).collect::<Vec<_>>().join("\n\n----------------------- next item -----------------------\n\n\n");
                    } else {
                        itms = outcome.get_result_items().iter().map(|s| s.get_item_xml()).collect::<Vec<_>>().join("\n");
                    }

                    // if single item found, set flag to do the highlighting (if highlighting is enabled)
                    if res_cnt == 1 {
                        self.xpath_highlighting = true; 
                    } else {
                        self.xpath_highlighting = false;
                    }

                    ui.label(item_cnt);
                    ui.add_space(6.0);
                    ui.label("Matching elements:");
                    ui.add_space(6.0);

                    // render the code editor 
                    let language = "xml".to_string();

                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
                        let mut layout_job = egui_extras::syntax_highlighting::highlight(
                            ui.ctx(),
                            ui.style(),
                            &theme,
                            buf.as_str(),
                            language.as_str(),
                        );
                        layout_job.wrap.max_width = wrap_width;
                        ui.fonts(|f| f.layout_job(layout_job))
                    };

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut itms)
                                .font(egui::TextStyle::Monospace) // for cursor height
                                .code_editor()
                                .desired_rows(10)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter),
                        );
                    });

                }
            } 
        });

        // if do_highligting is true, set active element to found item and render the 
        // frame around the active element on the screen (if highlighting is enabled)
        if self.xpath_highlighting {
            if let Some(elem) = self.ui_tree.get_element_by_xpath(xpath_input.as_str()) {
                // printfmt!("Element found by xpath: {}", elem.get_name());
                let runtime_id = elem.get_runtime_id().iter().map(|x| x.to_string()).collect::<Vec<String>>().join("-");
                if let Some(node) = self.ui_tree.get_tree().get_element_by_runtime_id(runtime_id.as_str()) {
                    let idx = node.index;
                    // printfmt!("Index in tree: {}", idx);
                    // update the state with the new active element
                    state.update_state(elem.clone(), idx);
                    self.process_highlighting(state);
                } else {
                    // printfmt!("Element not found in tree by runtime id");
                }
            } else {
                // printfmt!("No element found by xpath");
            }
        }


    }


    #[inline(always)]
    fn process_event(&mut self, event: &egui::Event, state: &mut TreeState) {
        
        match event {
            egui::Event::MouseMoved { .. } => { 
                // printfmt!("Mouse moved event received");
                // printfmt!("Getting cursor position");

                let cursor_position = unsafe {
                    let mut cursor_pos = POINT::default();
                    GetCursorPos(&mut cursor_pos).unwrap();
                    cursor_pos.x = (cursor_pos.x as f32 / self.app_context.screen_scale) as i32;
                    cursor_pos.y = (cursor_pos.y as f32 / self.app_context.screen_scale) as i32;
                    cursor_pos
                };
                // printfmt!("getting bouding rectangle for cursor position: ({}, {})", cursor_position.x, cursor_position.y);
                // printfmt!("Searching {} elements in the UI tree", self.ui_tree.get_elements().len());
                if let Some(ui_element_props) = rectangle::get_point_bounding_rect(&cursor_position, self.ui_tree.get_elements()) {
                    // printfmt!("Updating state with element found at cursor position: {}", ui_element_props.get_element_props().get_element().get_name());
                    state.update_state(ui_element_props.get_element_props().clone(), ui_element_props.get_tree_index());
                } 
            }
            egui::Event::Key { key, pressed, ..} => { // physical_key, repeat, modifiers 
                // printfmt!("Key event received: {:?}, pressed: {}", key, pressed);
                if key == &egui::Key::Escape && !*pressed  {                 
                    // check if tracking is enabled, if yes, desable tracking
                    // if not, ignore the escape key
                    if self.recording == true {
                        self.recording = false;
                        self.set_status("Tracking disabled".to_string(), Duration::seconds(2));
                    } else {
                        self.set_status("No tracking active, ignoring Escape key".to_string(), Duration::seconds(2));
                    }
                }
            }
            _ => (),
        }
    }


    #[inline(always)]
    fn process_highlighting(&mut self, state: &mut TreeState) {
        // Optionally render the frame around the active element on the screen
        if self.highlighting {
            
            let active_element = state.active_element.as_ref().unwrap();
            
            let left: f32 = active_element.get_element().get_bounding_rectangle().get_left() as f32 * self.app_context.screen_scale;
            let top: f32 = active_element.get_element().get_bounding_rectangle().get_top() as f32 * self.app_context.screen_scale;
            let right: f32 = active_element.get_element().get_bounding_rectangle().get_right() as f32 * self.app_context.screen_scale;
            let bottom: f32 = active_element.get_element().get_bounding_rectangle().get_bottom() as f32 * self.app_context.screen_scale;

            let rect: RECT = RECT { 
                left: left as i32, 
                top: top as i32, 
                right: right as i32, 
                bottom: bottom as i32, 
            };
            
            if let Some(prev_element) = &state.prev_element {
                let prev_left: f32 = prev_element.get_element().get_bounding_rectangle().get_left() as f32 * self.app_context.screen_scale;
                let prev_top: f32 = prev_element.get_element().get_bounding_rectangle().get_top() as f32 * self.app_context.screen_scale;
                let prev_right: f32 = prev_element.get_element().get_bounding_rectangle().get_right() as f32 * self.app_context.screen_scale;
                let prev_bottom: f32 = prev_element.get_element().get_bounding_rectangle().get_bottom() as f32 * self.app_context.screen_scale;

                let prev_rect: RECT = RECT {
                    left: prev_left as i32, 
                    top: prev_top as i32, 
                    right: prev_right as i32, 
                    bottom: prev_bottom as i32,     
                };
                if state.clear_frame { //rect != prev_rect && 
                    printfmt!("Cleanup needed - new: {:?} vs old: {:?}", rect, prev_rect);
                    rectangle::clear_frame(prev_rect).unwrap();
                    rectangle::draw_frame(rect, 4).unwrap();
                    state.clear_frame = false;
                } else {
                    rectangle::draw_frame(rect, 4).unwrap();
                }
            } else {
                rectangle::draw_frame(rect, 4).unwrap();
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

        // manage the TreeState
        let mut state: TreeState;
        if let Some(tree_state) = &self.tree_state { //.active_element 
            state = tree_state.clone();
        } else {
            state = TreeState::new();
        }        

        if state.refresh_path_to_active_ui_element {
            state.update_path_to_active_ui_element(&self.ui_tree);
            // println!("Path to active ui element {:?} set to : {:?}", state.active_ui_element,  state.path_to_active_ui_element);
        }

        // manage the AppStatusMsg lifecycle
        if let Some(status_msg) = &self.status_msg {
            if status_msg.is_expired() {
                self.clear_status();
            } else {
                if status_msg.has_display_duration() {
                    // switch from reactive mode to continuous mode to 
                    // ensure the status messages is cleared after the 
                    // specified time, even if there is no event triggered
                    ctx.request_repaint();
                }
            }
        }

        // manage the AppMode / Tree refresh lifecycle
        match &self.app_mode {
            AppMode::Normal(last_refesh) => {
                
                if self.auto_refresh {
                    // switch from reactive mode to continuous mode to 
                    // ensure the UI is rendered and with that the WinEvents are processed
                    // continuously even if the mouse is outside the app window
                    ctx.request_repaint();
                }
                
                // check for WinEvents indicating a change in the UI tree
                // to avoid excessive refresh, only check every 2 seconds and
                // only if not currently recording (tracking) the cursor
                if !self.recording && self.auto_refresh && last_refesh.time.elapsed().as_secs() > 2 {
                    printfmt!("Checking for WinEvents");
                    let winevents = self.winevent_monitor.check_for_events();
                    let relevant_events = winevents.iter().filter(|e| e.get_ui_element_name() != "UI Explore").count();
                    let ui_elem_events = winevents.iter().filter(|e| e.get_ui_element_name() == "UI Explore").count();
                    if winevents.len() > 0 {
                        printfmt!("Checked for WinEvents, found {} relevant and {} UI Explore events", relevant_events, ui_elem_events);
                        if relevant_events > 0 {
                            // printfmt!("Triggering UI tree refresh");
                            self.app_mode = AppMode::NeedsTreeRefresh;
                            self.set_status("UI Tree change detected, refreshing...".to_string(), Duration::seconds(5));
                        }
                    }
                }
            }
            AppMode::NeedsTreeRefresh => {
                let app_name = self.app_name.clone();
                let (tx, rx): (Sender<_>, Receiver<UITreeXML>) = channel();
                thread::spawn(|| {
                    get_all_elements_xml(tx, None, None, Some(app_name));
                });
                self.app_mode = AppMode::IsRefreshingTree(rx);
                state.active_element = None;
            }
            AppMode::IsRefreshingTree(rx) => {
                if let Ok(new_ui_tree) = rx.try_recv() {
                    self.ui_tree = new_ui_tree;
                    state = TreeState::new(); // reset the tree state
                    self.tree_state = Some(state);
                    self.app_mode = AppMode::Normal(LastRefresh { time: std::time::Instant::now() });
                    self.set_status("UI Tree refreshed".to_string(), Duration::seconds(2));
                    // return to avoid conflicts with the UI rendering below in particular the state variable
                    return;
                }                
            }
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

            },
            DisplayMode::XpathTest => {
                // Xpath testing screen
                self.render_xpath_screen(ctx, &mut state);
            }
        }

        // finally update the state
        self.tree_state = Some(state);

    }



}


fn event_summary(event: &egui::Event, ui_elements: &Vec<UIElementInTreeXML>) -> String {
    match event {
        egui::Event::PointerMoved { .. }   => {        
            "PointerMoved { .. }".to_owned()
        }
        egui::Event::MouseMoved { .. } => { 
            let cursor_position = unsafe {
                let mut cursor_pos = POINT::default();
                GetCursorPos(&mut cursor_pos).unwrap();
                cursor_pos
            };

            if let Some(ui_element_props) = rectangle::get_point_bounding_rect(&cursor_position, ui_elements) {
                // format!("MouseMoved {{ x: {}, y: {} }} over {}", cursor_position.x, cursor_position.y, ui_element_props.name)
                let ui_element_props = ui_element_props.get_element_props();
                let control_type: String = ui_element_props.get_element().get_control_type().to_string();        
                format!("MouseMoved over {{ name: '{}', control_type: '{}' bounding_rect: {} }}", ui_element_props.get_element().get_name(), control_type, ui_element_props.get_element().get_bounding_rectangle())
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

fn is_in_path_to_active_element(current_element: usize, path_to_active_element: &Option<Vec<usize>>) -> bool {
    
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