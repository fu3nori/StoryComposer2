#![windows_subsystem = "windows"]

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

const MAX_PLOTS: usize = 1024;
const MAX_UNDO_HISTORY: usize = 100;
const SETTINGS_FILE: &str = "sc.dat";

// Available font sizes
const FONT_SIZES: &[f32] = &[12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0];
const DEFAULT_FONT_SIZE: f32 = 20.0;

// Available fonts
#[derive(Clone, PartialEq, Serialize, Deserialize)]
enum FontChoice {
    YuGothic,
    Meiryo,
    MSGothic,
    MSMincho,
}

impl FontChoice {
    fn display_name(&self) -> &'static str {
        match self {
            FontChoice::YuGothic => "游ゴシック",
            FontChoice::Meiryo => "メイリオ",
            FontChoice::MSGothic => "MS ゴシック",
            FontChoice::MSMincho => "MS 明朝",
        }
    }

    fn font_path(&self) -> &'static str {
        match self {
            FontChoice::YuGothic => "C:\\Windows\\Fonts\\YuGothM.ttc",
            FontChoice::Meiryo => "C:\\Windows\\Fonts\\meiryo.ttc",
            FontChoice::MSGothic => "C:\\Windows\\Fonts\\msgothic.ttc",
            FontChoice::MSMincho => "C:\\Windows\\Fonts\\msmincho.ttc",
        }
    }

    fn all() -> &'static [FontChoice] {
        &[FontChoice::YuGothic, FontChoice::Meiryo, FontChoice::MSGothic, FontChoice::MSMincho]
    }
}

impl Default for FontChoice {
    fn default() -> Self {
        FontChoice::YuGothic
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AppSettings {
    font_size: f32,
    font_choice: FontChoice,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            font_choice: FontChoice::default(),
        }
    }
}

impl AppSettings {
    fn load() -> Self {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let settings_path = exe_dir.join(SETTINGS_FILE);
                if let Ok(content) = std::fs::read_to_string(&settings_path) {
                    if let Ok(settings) = serde_json::from_str(&content) {
                        return settings;
                    }
                }
            }
        }
        Self::default()
    }

    fn save(&self) {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let settings_path = exe_dir.join(SETTINGS_FILE);
                if let Ok(json) = serde_json::to_string_pretty(self) {
                    let _ = std::fs::write(&settings_path, json);
                }
            }
        }
    }
}

fn setup_font(ctx: &egui::Context, font_choice: &FontChoice) {
    let mut fonts = FontDefinitions::default();

    let font_path = font_choice.font_path();

    if let Ok(font_data) = std::fs::read(font_path) {
        fonts.font_data.insert(
            "japanese_font".to_owned(),
            FontData::from_owned(font_data),
        );

        fonts.families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "japanese_font".to_owned());

        fonts.families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "japanese_font".to_owned());

        ctx.set_fonts(fonts);
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PlotFragment {
    id: usize,
    text: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SaveData {
    plots: Vec<PlotFragment>,
    composed_text: String,
}

#[derive(Clone)]
struct AppState {
    plots: Vec<PlotFragment>,
    composed_text: String,
}

struct StoryComposerApp {
    plots: Vec<PlotFragment>,
    composed_text: String,
    next_id: usize,
    current_file_path: Option<PathBuf>,

    // Undo/Redo
    undo_stack: VecDeque<AppState>,
    redo_stack: VecDeque<AppState>,

    // Search/Replace dialog
    show_search_dialog: bool,
    show_replace_dialog: bool,
    search_text: String,
    replace_text: String,
    search_results: Vec<SearchResult>,
    current_search_index: usize,

    // UI state
    delete_confirm_id: Option<usize>,
    pending_action: Option<(usize, PlotAction)>,

    // Font settings
    settings: AppSettings,
    font_changed: bool,

    // Document state
    is_dirty: bool,
    show_close_confirm: bool,
}

#[derive(Clone)]
#[allow(dead_code)]
struct SearchResult {
    location: SearchLocation,
    plot_index: Option<usize>,
    start: usize,
    end: usize,
}

#[derive(Clone, PartialEq)]
enum SearchLocation {
    Plot,
    ComposedText,
}

impl StoryComposerApp {
    fn new(settings: AppSettings) -> Self {
        Self {
            plots: vec![PlotFragment { id: 0, text: String::new() }],
            composed_text: String::new(),
            next_id: 1,
            current_file_path: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            show_search_dialog: false,
            show_replace_dialog: false,
            search_text: String::new(),
            replace_text: String::new(),
            search_results: Vec::new(),
            current_search_index: 0,
            delete_confirm_id: None,
            pending_action: None,
            settings,
            font_changed: false,
            is_dirty: false,
            show_close_confirm: false,
        }
    }

    fn save_state_for_undo(&mut self) {
        let state = AppState {
            plots: self.plots.clone(),
            composed_text: self.composed_text.clone(),
        };
        self.undo_stack.push_back(state);
        if self.undo_stack.len() > MAX_UNDO_HISTORY {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
        self.is_dirty = true;
    }

    fn undo(&mut self) {
        if let Some(state) = self.undo_stack.pop_back() {
            let current = AppState {
                plots: self.plots.clone(),
                composed_text: self.composed_text.clone(),
            };
            self.redo_stack.push_back(current);
            if self.redo_stack.len() > MAX_UNDO_HISTORY {
                self.redo_stack.pop_front();
            }
            self.plots = state.plots;
            self.composed_text = state.composed_text;
        }
    }

    fn redo(&mut self) {
        if let Some(state) = self.redo_stack.pop_back() {
            let current = AppState {
                plots: self.plots.clone(),
                composed_text: self.composed_text.clone(),
            };
            self.undo_stack.push_back(current);
            if self.undo_stack.len() > MAX_UNDO_HISTORY {
                self.undo_stack.pop_front();
            }
            self.plots = state.plots;
            self.composed_text = state.composed_text;
        }
    }

    fn compose(&mut self) {
        self.save_state_for_undo();
        self.composed_text = self.plots
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<&str>>()
            .join("\n\n---\n\n");
    }

    fn new_document(&mut self) {
        self.save_state_for_undo();
        self.plots = vec![PlotFragment { id: 0, text: String::new() }];
        self.composed_text = String::new();
        self.next_id = 1;
        self.current_file_path = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.is_dirty = false;
    }

    fn add_plot_after(&mut self, index: usize) {
        if self.plots.len() >= MAX_PLOTS {
            return;
        }
        self.save_state_for_undo();
        let new_plot = PlotFragment {
            id: self.next_id,
            text: String::new(),
        };
        self.next_id += 1;
        self.plots.insert(index + 1, new_plot);
    }

    fn remove_plot(&mut self, index: usize) {
        if self.plots.len() <= 1 {
            return;
        }
        self.save_state_for_undo();
        self.plots.remove(index);
    }

    fn move_plot_up(&mut self, index: usize) {
        if index == 0 {
            return;
        }
        self.save_state_for_undo();
        self.plots.swap(index, index - 1);
    }

    fn move_plot_down(&mut self, index: usize) {
        if index >= self.plots.len() - 1 {
            return;
        }
        self.save_state_for_undo();
        self.plots.swap(index, index + 1);
    }

    fn save_file(&mut self, path: &PathBuf) -> Result<(), String> {
        let save_data = SaveData {
            plots: self.plots.clone(),
            composed_text: self.composed_text.clone(),
        };
        let json = serde_json::to_string_pretty(&save_data)
            .map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        self.is_dirty = false;
        Ok(())
    }

    fn load_file(&mut self, path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let save_data: SaveData = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        self.save_state_for_undo();
        self.plots = save_data.plots;
        self.composed_text = save_data.composed_text;
        self.next_id = self.plots.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        Ok(())
    }

    fn get_default_dir() -> Option<PathBuf> {
        dirs::document_dir()
    }

    fn search(&mut self) {
        self.search_results.clear();
        self.current_search_index = 0;

        if self.search_text.is_empty() {
            return;
        }

        // Search in plots
        for (i, plot) in self.plots.iter().enumerate() {
            let mut start = 0;
            while let Some(pos) = plot.text[start..].find(&self.search_text) {
                let actual_start = start + pos;
                self.search_results.push(SearchResult {
                    location: SearchLocation::Plot,
                    plot_index: Some(i),
                    start: actual_start,
                    end: actual_start + self.search_text.len(),
                });
                start = actual_start + 1;
            }
        }

        // Search in composed text
        let mut start = 0;
        while let Some(pos) = self.composed_text[start..].find(&self.search_text) {
            let actual_start = start + pos;
            self.search_results.push(SearchResult {
                location: SearchLocation::ComposedText,
                plot_index: None,
                start: actual_start,
                end: actual_start + self.search_text.len(),
            });
            start = actual_start + 1;
        }
    }

    fn replace_all(&mut self) {
        if self.search_text.is_empty() {
            return;
        }
        self.save_state_for_undo();

        // Replace in plots
        for plot in &mut self.plots {
            plot.text = plot.text.replace(&self.search_text, &self.replace_text);
        }

        // Replace in composed text
        self.composed_text = self.composed_text.replace(&self.search_text, &self.replace_text);

        self.search_results.clear();
    }

    fn export_text(&self) {
        if let Some(default_dir) = Self::get_default_dir() {
            let file = rfd::FileDialog::new()
                .add_filter("テキストファイル", &["txt"])
                .set_directory(&default_dir)
                .set_file_name("export.txt")
                .save_file();
            if let Some(mut path) = file {
                if path.extension().is_none() {
                    path.set_extension("txt");
                }
                // Convert LF to CRLF
                let crlf_text = self.composed_text.replace('\n', "\r\n");
                // Write as UTF-8 with BOM for better compatibility
                if let Err(e) = std::fs::write(&path, crlf_text.as_bytes()) {
                    eprintln!("Export error: {}", e);
                }
            }
        }
    }
}

#[allow(dead_code)]
fn flat_button(ui: &mut egui::Ui, text: &str, size: egui::Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let bg_color = if response.hovered() {
            egui::Color32::from_rgb(70, 130, 180)
        } else {
            egui::Color32::from_rgb(50, 100, 150)
        };

        ui.painter().rect_filled(rect, 4.0, bg_color);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(14.0),
            visuals.text_color(),
        );
    }

    response
}

// Vertical offset for Japanese font centering (adjust as needed)
const TEXT_Y_OFFSET: f32 = 3.0;
const MENU_BUTTON_Y_OFFSET: f32 = 5.0; // Additional offset for menu buttons (ファイル, 編集, 検索)
#[allow(dead_code)]
const MENU_LABEL_Y_OFFSET: f32 = 5.0; // Offset for menu bar labels (文字サイズ:, フォント:)

// メニューバー
const MENU_BAR_HEIGHT: f32 = 40.0;
const MENU_ROW_HEIGHT: f32 = 36.0;

// ComboBox（TEXT + PAD*2 <= HEIGHT を満たすこと）
const MENU_COMBO_TEXT_SIZE: f32 = 15.0;
const MENU_COMBO_PAD_Y: f32 = 6.0;              // 6*2 + 15 = 27 <= 36
const MENU_COMBO_HEIGHT: f32 = MENU_ROW_HEIGHT;

fn menu_bar_label(ui: &mut egui::Ui, text: &str) {
    let font_id = egui::FontId::proportional(13.0);

    // このブロック（allocate_ui_with_layout）の高さをそのまま使う
    let h = ui.available_height();
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font_id.clone(), egui::Color32::WHITE);

    let desired_size = egui::vec2(galley.size().x, h);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    if ui.is_rect_visible(rect) {
        ui.painter().text(
            egui::pos2(rect.min.x, rect.center().y + TEXT_Y_OFFSET),
            egui::Align2::LEFT_CENTER,
            text,
            font_id,
            egui::Color32::WHITE,
        );
    }
}

fn menu_combo_scope<R>(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let mut s = ui.style().as_ref().clone();

        // ComboBoxはボタン扱いなので TextStyle::Button が効く
        s.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(MENU_COMBO_TEXT_SIZE),
        );

        // 高さ・パディングも少し盛る
        s.spacing.interact_size.y = MENU_COMBO_HEIGHT;
        s.spacing.button_padding.y = MENU_COMBO_PAD_Y;

        ui.set_style(s);
        f(ui)
    })
    .inner
}

fn styled_menu_button(ui: &mut egui::Ui, text: &str, color: egui::Color32) -> egui::Response {
    styled_menu_button_w(ui, text, color, 100.0)
}

fn styled_menu_button_w(ui: &mut egui::Ui, text: &str, color: egui::Color32, width: f32) -> egui::Response {
    let font_id = egui::FontId::proportional(13.0);
    let h = ui.spacing().interact_size.y;
    let desired_size = egui::vec2(width, h);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg_color = if response.hovered() {
            egui::Color32::from_rgb(
                (color.r() as u16 + 30).min(255) as u8,
                (color.g() as u16 + 30).min(255) as u8,
                (color.b() as u16 + 30).min(255) as u8,
            )
        } else {
            color
        };

        ui.painter().rect_filled(rect, 4.0, bg_color);

        ui.painter().text(
            egui::pos2(rect.center().x, rect.center().y + TEXT_Y_OFFSET),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            egui::Color32::WHITE,
        );
    }

    response
}

fn menu_item(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let font_id = egui::FontId::proportional(13.0);
    let available_width = ui.available_width();
    let desired_size = egui::vec2(available_width, 26.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let is_focused = response.hovered();
        if is_focused {
            ui.painter().rect_filled(rect, 2.0, ui.style().visuals.widgets.hovered.bg_fill);
        }

        ui.painter().text(
            egui::pos2(rect.min.x + 8.0, rect.center().y + TEXT_Y_OFFSET),
            egui::Align2::LEFT_CENTER,
            text,
            font_id,
            egui::Color32::WHITE,
        );
    }

    response
}

fn small_flat_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let font_id = egui::FontId::proportional(14.0);
    let size = egui::vec2(26.0, 26.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg_color = if response.hovered() {
            egui::Color32::from_rgb(80, 140, 190)
        } else {
            egui::Color32::from_rgb(60, 110, 160)
        };

        ui.painter().rect_filled(rect, 4.0, bg_color);

        ui.painter().text(
            egui::pos2(rect.center().x, rect.center().y + TEXT_Y_OFFSET),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            egui::Color32::WHITE,
        );
    }

    response
}

fn custom_menu_button<R>(
    ui: &mut egui::Ui,
    text: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let font_id = egui::FontId::proportional(13.0);
    let h = ui.spacing().interact_size.y;
    let desired_size = egui::vec2(60.0, h);

    let popup_id = ui.make_persistent_id(text);
    let is_open = ui.memory(|m| m.is_popup_open(popup_id));

    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    if ui.is_rect_visible(rect) {
        let is_focused = is_open || response.hovered();
        let bg_color = if is_focused {
            ui.style().visuals.widgets.hovered.bg_fill
        } else {
            egui::Color32::TRANSPARENT
        };

        ui.painter().rect_filled(rect, 2.0, bg_color);

        ui.painter().text(
            egui::pos2(rect.center().x, rect.center().y + MENU_BUTTON_Y_OFFSET),
            egui::Align2::CENTER_CENTER,
            text,
            font_id,
            egui::Color32::WHITE,
        );
    }

    let inner = egui::popup::popup_below_widget(ui, popup_id, &response, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
        add_contents(ui)
    });

    egui::InnerResponse::new(inner, response)
}

impl eframe::App for StoryComposerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // IME入力中（Preeditあり）またはCommit時はEnterキーを消費して改行を抑制
        // これにより、変換確定時にEnterキーを押しても改行されなくなる
        let (has_preedit, has_commit) = ctx.input(|i| {
            let mut preedit = false;
            let mut commit = false;
            for event in &i.events {
                if let egui::Event::Ime(ime) = event {
                    match ime {
                        egui::ImeEvent::Preedit(text) => preedit = !text.is_empty(),
                        egui::ImeEvent::Commit(_) => commit = true,
                        _ => {}
                    }
                }
            }
            (preedit, commit)
        });

        if has_preedit || has_commit {
            ctx.input_mut(|i| {
                i.events.retain(|e| {
                    !matches!(e, egui::Event::Key { key: egui::Key::Enter, .. })
                });
            });
        }

        // Handle close request
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.is_dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_close_confirm = true;
            }
        }

        // Close confirmation dialog
        if self.show_close_confirm {
            egui::Window::new("確認")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("ドキュメントを保存していませんがよろしいですか？");
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let total_width = ui.available_width();
                            ui.add_space(total_width / 2.0 - 60.0);
                            if ui.button("はい").clicked() {
                                self.show_close_confirm = false;
                                self.is_dirty = false;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            if ui.button("いいえ").clicked() {
                                self.show_close_confirm = false;
                            }
                        });
                    });
                });
        }

        // Apply font change if needed
        if self.font_changed {
            setup_font(ctx, &self.settings.font_choice);
            self.settings.save();
            self.font_changed = false;
        }

        // Apply dark theme with custom flat styling
        let mut style = (*ctx.style()).clone();
        style.visuals = egui::Visuals::dark();

        // Dark background colors
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(25, 28, 33);
        style.visuals.window_fill = egui::Color32::from_rgb(32, 36, 42);
        style.visuals.panel_fill = egui::Color32::from_rgb(32, 36, 42);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(38, 42, 50);

        // Widget colors - flat design with visible borders
        style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(55, 60, 70);
        style.visuals.widgets.inactive.weak_bg_fill = egui::Color32::from_rgb(50, 55, 65);
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 75, 85));
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 205, 215));

        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(70, 130, 180);
        style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(65, 120, 170);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 160, 210));
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(50, 110, 160);
        style.visuals.widgets.active.weak_bg_fill = egui::Color32::from_rgb(45, 100, 150);
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 140, 190));
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

        // Button rounding for flat look
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(4.0);

        // Spacing
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);

        ctx.set_style(style);

        // Menu bar with better spacing
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(MENU_BAR_HEIGHT)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add_space(8.0);

                        // Compose button on the left with accent color
                        if styled_menu_button(ui, "文書生成", egui::Color32::from_rgb(46, 139, 87)).clicked() {
                            self.compose();
                        }

                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // File menu
                    custom_menu_button(ui, "ファイル", |ui| {
                        ui.set_min_width(180.0);
                        if menu_item(ui, "新規作成").clicked() {
                            self.new_document();
                            ui.close_menu();
                        }
                        ui.separator();
                        if menu_item(ui, "名前を付けて保存").clicked() {
                            if let Some(default_dir) = Self::get_default_dir() {
                                let file = rfd::FileDialog::new()
                                    .add_filter("SCRF", &["scrf"])
                                    .set_directory(&default_dir)
                                    .set_file_name("untitled.scrf")
                                    .save_file();
                                if let Some(mut path) = file {
                                    if path.extension().is_none() || path.extension().unwrap() != "scrf" {
                                        path.set_extension("scrf");
                                    }
                                    if let Err(e) = self.save_file(&path) {
                                        eprintln!("Save error: {}", e);
                                    } else {
                                        self.current_file_path = Some(path);
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if menu_item(ui, "上書き保存").clicked() {
                            if let Some(ref path) = self.current_file_path.clone() {
                                if let Err(e) = self.save_file(path) {
                                    eprintln!("Save error: {}", e);
                                }
                            } else {
                                if let Some(default_dir) = Self::get_default_dir() {
                                    let file = rfd::FileDialog::new()
                                        .add_filter("SCRF", &["scrf"])
                                        .set_directory(&default_dir)
                                        .set_file_name("untitled.scrf")
                                        .save_file();
                                    if let Some(mut path) = file {
                                        if path.extension().is_none() || path.extension().unwrap() != "scrf" {
                                            path.set_extension("scrf");
                                        }
                                        if let Err(e) = self.save_file(&path) {
                                            eprintln!("Save error: {}", e);
                                        } else {
                                            self.current_file_path = Some(path);
                                        }
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                        if menu_item(ui, "ファイルを開く").clicked() {
                            if let Some(default_dir) = Self::get_default_dir() {
                                let file = rfd::FileDialog::new()
                                    .add_filter("SCRF", &["scrf"])
                                    .set_directory(&default_dir)
                                    .pick_file();
                                if let Some(path) = file {
                                    if let Err(e) = self.load_file(&path) {
                                        eprintln!("Load error: {}", e);
                                    } else {
                                        self.current_file_path = Some(path);
                                    }
                                }
                            }
                            ui.close_menu();
                        }
                    });

                    ui.add_space(5.0);

                    // Edit menu
                    custom_menu_button(ui, "編集", |ui| {
                        ui.set_min_width(200.0);
                        if menu_item(ui, "元に戻す (Ctrl+Z)").clicked() {
                            self.undo();
                            ui.close_menu();
                        }
                        if menu_item(ui, "やり直し (Ctrl+Y)").clicked() {
                            self.redo();
                            ui.close_menu();
                        }
                        ui.separator();
                        if menu_item(ui, "切り取り (Ctrl+X)").clicked() {
                            ui.close_menu();
                        }
                        if menu_item(ui, "コピー (Ctrl+C)").clicked() {
                            ui.close_menu();
                        }
                        if menu_item(ui, "貼り付け (Ctrl+V)").clicked() {
                            ui.close_menu();
                        }
                    });

                    ui.add_space(5.0);

                    // Search menu
                    custom_menu_button(ui, "検索", |ui| {
                        ui.set_min_width(150.0);
                        if menu_item(ui, "文字列検索...").clicked() {
                            self.show_search_dialog = true;
                            self.show_replace_dialog = false;
                            ui.close_menu();
                        }
                        if menu_item(ui, "置換...").clicked() {
                            self.show_replace_dialog = true;
                            self.show_search_dialog = false;
                            ui.close_menu();
                        }
                    });

                    ui.add_space(5.0);

                    // Help button
                    custom_menu_button(ui, "ヘルプ", |ui| {
                        ui.set_min_width(200.0);
                        if menu_item(ui, "ReadMEを開く").clicked() {
                            if let Ok(exe_path) = std::env::current_exe() {
                                if let Some(exe_dir) = exe_path.parent() {
                                    let readme_path = exe_dir.join("ReadME.txt");
                                    let _ = std::process::Command::new("cmd")
                                        .args(["/c", "start", "", &readme_path.to_string_lossy()])
                                        .spawn();
                                }
                            }
                            ui.close_menu();
                        }
                    });

                    ui.add_space(5.0);

                    // License button
                    custom_menu_button(ui, "ライセンス", |ui| {
                        ui.set_min_width(200.0);
                        if menu_item(ui, "LICENSEを開く").clicked() {
                            if let Ok(exe_path) = std::env::current_exe() {
                                if let Some(exe_dir) = exe_path.parent() {
                                    let license_path = exe_dir.join("LICENSE");
                                    let _ = std::process::Command::new("cmd")
                                        .args(["/c", "start", "", &license_path.to_string_lossy()])
                                        .spawn();
                                }
                            }
                            ui.close_menu();
                        }
                    });

                    ui.add_space(15.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Font size selector
                    ui.allocate_ui_with_layout(
                        egui::vec2(170.0, MENU_COMBO_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            menu_bar_label(ui, "文字サイズ:");
                            let current_size_text = format!("{}px", self.settings.font_size as i32);

                            menu_combo_scope(ui, |ui| {
                                egui::ComboBox::from_id_salt("font_size_combo")
                                    .selected_text(current_size_text)
                                    .width(70.0)
                                    .show_ui(ui, |ui| {
                                        for &size in FONT_SIZES {
                                            let label = format!("{}px", size as i32);
                                            if ui
                                                .selectable_value(&mut self.settings.font_size, size, &label)
                                                .changed()
                                            {
                                                self.settings.save();
                                            }
                                        }
                                    });
                            });
                        },
                    );

                    ui.add_space(10.0);

                    // Font selector
                    ui.allocate_ui_with_layout(
                        egui::vec2(200.0, MENU_COMBO_HEIGHT),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            menu_bar_label(ui, "フォント:");
                            let current_font_name = self.settings.font_choice.display_name();

                            menu_combo_scope(ui, |ui| {
                                egui::ComboBox::from_id_salt("font_choice_combo")
                                    .selected_text(current_font_name)
                                    .width(120.0)
                                    .show_ui(ui, |ui| {
                                        for font in FontChoice::all() {
                                            if ui
                                                .selectable_value(
                                                    &mut self.settings.font_choice,
                                                    font.clone(),
                                                    font.display_name(),
                                                )
                                                .changed()
                                            {
                                                self.font_changed = true;
                                            }
                                        }
                                    });
                            });
                        },
                    );

                    // Export button - right aligned
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if styled_menu_button_w(ui, "テキストにエクスポート", egui::Color32::from_rgb(46, 139, 87), 160.0).clicked() {
                            self.export_text();
                        }
                    });
                    });
                });
            });

        // Keyboard shortcuts
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Z)) {
            self.undo();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Y)) {
            self.redo();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            if let Some(ref path) = self.current_file_path.clone() {
                let _ = self.save_file(path);
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F)) {
            self.show_search_dialog = true;
            self.show_replace_dialog = false;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::H)) {
            self.show_replace_dialog = true;
            self.show_search_dialog = false;
        }

        // Search dialog
        if self.show_search_dialog {
            egui::Window::new("文字列検索")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("検索文字列:");
                        ui.text_edit_singleline(&mut self.search_text);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("検索").clicked() {
                            self.search();
                        }
                        if ui.button("閉じる").clicked() {
                            self.show_search_dialog = false;
                        }
                    });
                    if !self.search_results.is_empty() {
                        ui.label(format!("{}件見つかりました", self.search_results.len()));
                    }
                });
        }

        // Replace dialog
        if self.show_replace_dialog {
            egui::Window::new("置換")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("検索文字列:");
                        ui.text_edit_singleline(&mut self.search_text);
                    });
                    ui.horizontal(|ui| {
                        ui.label("置換文字列:");
                        ui.text_edit_singleline(&mut self.replace_text);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("すべて置換").clicked() {
                            self.replace_all();
                        }
                        if ui.button("閉じる").clicked() {
                            self.show_replace_dialog = false;
                        }
                    });
                });
        }

        // Delete confirmation dialog
        if let Some(delete_id) = self.delete_confirm_id {
            egui::Window::new("確認")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("本当にこのプロットを削除しますか？");
                    ui.horizontal(|ui| {
                        if ui.button("はい").clicked() {
                            if let Some(index) = self.plots.iter().position(|p| p.id == delete_id) {
                                self.remove_plot(index);
                            }
                            self.delete_confirm_id = None;
                        }
                        if ui.button("いいえ").clicked() {
                            self.delete_confirm_id = None;
                        }
                    });
                });
        }

        // Collect pending action from previous frame
        let pending_action = self.pending_action.take();
        if let Some((index, act)) = pending_action {
            match act {
                PlotAction::AddAfter => self.add_plot_after(index),
                PlotAction::RequestDelete(id) => self.delete_confirm_id = Some(id),
                PlotAction::MoveUp => self.move_plot_up(index),
                PlotAction::MoveDown => self.move_plot_down(index),
            }
        }

        // Status bar (define first so it reserves space)
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref path) = self.current_file_path {
                    ui.label(format!("ファイル: {}", path.display()));
                } else {
                    ui.label("ファイル: 未保存");
                }
                ui.separator();
                ui.label(format!("Undo: {} | Redo: {}", self.undo_stack.len(), self.redo_stack.len()));
            });
        });

        // Get current font size for text areas
        let font_size = self.settings.font_size;

        // Main content - fixed 50/50 split
        egui::CentralPanel::default().show(ctx, |ui| {
            let total_width = ui.available_width();
            let panel_width = (total_width - 10.0) / 2.0; // 10px for separator
            let panel_height = ui.available_height();

            ui.horizontal(|ui| {
                // Left pane - Plots (fixed 50%)
                ui.allocate_ui(egui::vec2(panel_width, panel_height), |ui| {
                    ui.vertical(|ui| {
                        ui.heading("プロット");
                        ui.add_space(10.0);

                        egui::ScrollArea::vertical()
                            .id_salt("left_scroll")
                            .show(ui, |ui| {
                                let plots_len = self.plots.len();
                                let text_width = panel_width - 120.0;

                                for i in 0..plots_len {
                                    let plot_id = self.plots[i].id;

                                    ui.horizontal(|ui| {
                                        // Plot number
                                        ui.label(format!("#{:3}", i + 1));

                                        // Calculate rows based on content (minimum 10, expand as needed)
                                        let line_count = self.plots[i].text.lines().count().max(1);
                                        let display_rows = line_count.max(10);

                                        // Text area - expands with content
                                        let text_edit = egui::TextEdit::multiline(&mut self.plots[i].text)
                                            .desired_width(text_width)
                                            .desired_rows(display_rows)
                                            .font(egui::FontId::monospace(font_size));
                                        if ui.add(text_edit).changed() {
                                            self.is_dirty = true;
                                        }

                                        // Buttons
                                        ui.vertical(|ui| {
                                            if small_flat_button(ui, "+").clicked() && plots_len < MAX_PLOTS {
                                                self.pending_action = Some((i, PlotAction::AddAfter));
                                            }
                                            if small_flat_button(ui, "-").clicked() && plots_len > 1 {
                                                self.pending_action = Some((i, PlotAction::RequestDelete(plot_id)));
                                            }
                                            if small_flat_button(ui, "↑").clicked() && i > 0 {
                                                self.pending_action = Some((i, PlotAction::MoveUp));
                                            }
                                            if small_flat_button(ui, "↓").clicked() && i < plots_len - 1 {
                                                self.pending_action = Some((i, PlotAction::MoveDown));
                                            }
                                        });
                                    });
                                    ui.add_space(10.0);
                                }

                                ui.add_space(20.0);
                                ui.label(format!("プロット数: {} / {}", self.plots.len(), MAX_PLOTS));
                            });
                    });
                });

                // Separator
                ui.separator();

                // Right pane - Composed text (fixed 50%)
                ui.allocate_ui(egui::vec2(panel_width, panel_height), |ui| {
                    ui.vertical(|ui| {
                        ui.heading("出力テキスト");
                        ui.add_space(10.0);

                        egui::ScrollArea::vertical()
                            .id_salt("right_scroll")
                            .show(ui, |ui| {
                                let text_width = panel_width - 40.0;

                                // Calculate rows based on content (minimum 60, expand as needed)
                                let line_count = self.composed_text.lines().count().max(1);
                                let display_rows = line_count.max(60);

                                let text_edit = egui::TextEdit::multiline(&mut self.composed_text)
                                    .desired_width(text_width)
                                    .desired_rows(display_rows)
                                    .font(egui::FontId::monospace(font_size));
                                if ui.add(text_edit).changed() {
                                    self.is_dirty = true;
                                }
                            });
                    });
                });
            });
        });
    }
}

#[derive(Clone, Copy)]
enum PlotAction {
    AddAfter,
    RequestDelete(usize),
    MoveUp,
    MoveDown,
}

fn load_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("ico/128.ico");
    let img = image::load_from_memory(icon_bytes).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

fn get_screen_size() -> (f32, f32) {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn GetSystemMetrics(nIndex: i32) -> i32;
        }
        unsafe {
            let width = GetSystemMetrics(0) as f32;  // SM_CXSCREEN
            let height = GetSystemMetrics(1) as f32; // SM_CYSCREEN
            (width, height)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        (1920.0, 1080.0)
    }
}

fn main() -> eframe::Result<()> {
    let settings = AppSettings::load();

    let (screen_w, screen_h) = get_screen_size();
    let window_w = screen_w * 0.7;
    let window_h = screen_h * 0.7;

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([window_w, window_h])
        .with_min_inner_size([800.0, 600.0])
        .with_title("StoryComposer2");

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "StoryComposer2",
        options,
        Box::new(move |cc| {
            setup_font(&cc.egui_ctx, &settings.font_choice);
            Ok(Box::new(StoryComposerApp::new(settings)))
        }),
    )
}
