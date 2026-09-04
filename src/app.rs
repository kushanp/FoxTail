use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, FontId, Key, Layout, PointerButton, Pos2, Rect, RichText, ScrollArea,
    Sense, Stroke, TextEdit, Ui, Vec2, ViewportCommand,
};
use foxtail::config::Config;
use foxtail::engine::{EncodingKind, TailedFile};
use foxtail::filter::{CompiledFilter, LineFilter};
use foxtail::highlight::Highlighter;
use foxtail::util::{format_bytes, format_count};
use regex::RegexBuilder;
use rfd::FileDialog;

use crate::theme::{self, FOX, FIND};

pub fn run() -> eframe::Result {
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 420.0])
            .with_title("FoxTail")
            .with_drag_and_drop(true)
            .with_icon(crate::icon::app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "FoxTail",
        options,
        Box::new(move |cc| Ok(Box::new(FoxTailApp::new(cc, files)))),
    )
}

struct Tab {
    engine: TailedFile,
    follow: bool,
    filter: LineFilter,
    compiled: CompiledFilter,
    filter_matches: Vec<u64>,
    filter_scan: u64,
    find_query: String,
    find_regex: bool,
    find_case: bool,
    find_error: Option<String>,
    /// Inclusive absolute line range.
    selected: Option<(u64, u64)>,
    scroll_to: Option<u64>,
    seen_lines: u64,
    status: Option<String>,
}

impl Tab {
    fn new(engine: TailedFile, follow: bool) -> Self {
        let filter = LineFilter::default();
        let compiled = filter.compile();
        Self {
            engine,
            follow,
            filter,
            compiled,
            filter_matches: Vec::new(),
            filter_scan: 0,
            find_query: String::new(),
            find_regex: false,
            find_case: false,
            find_error: None,
            selected: None,
            scroll_to: None,
            seen_lines: 0,
            status: None,
        }
    }

    fn reset_filter_scan(&mut self) {
        self.compiled = self.filter.compile();
        self.filter_matches.clear();
        self.filter_scan = 0;
    }

    fn view_count(&self) -> u64 {
        if self.filter.is_active() {
            self.filter_matches.len() as u64
        } else {
            self.engine.view_line_count()
        }
    }

    fn pump_filter(&mut self, budget: usize) {
        if !self.filter.is_active() {
            return;
        }
        let total = self.engine.view_line_count();
        if self.filter_scan >= total {
            return;
        }
        let end = (self.filter_scan + budget as u64).min(total);
        let n = (end - self.filter_scan) as usize;
        let lines = self.engine.read_view_lines(self.filter_scan, n);
        for (i, line) in lines.iter().enumerate() {
            if self.compiled.allows(line) {
                self.filter_matches.push(self.filter_scan + i as u64);
            }
        }
        self.filter_scan = end;
    }

    fn has_unseen(&self) -> bool {
        self.engine.view_line_count() > self.seen_lines
    }
}

struct FoxTailApp {
    tabs: Vec<Tab>,
    active: usize,
    config: Config,
    highlighter: Highlighter,
    show_find: bool,
    show_highlights: bool,
    show_help: bool,
    show_goto: bool,
    goto_needs_focus: bool,
    goto_text: String,
    status: String,
    applied_on_top: bool,
}

impl FoxTailApp {
    fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let config = Config::load();
        let highlighter = Highlighter::new(&config.highlight_rules);
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            config,
            highlighter,
            show_find: false,
            show_highlights: false,
            show_help: false,
            show_goto: false,
            goto_needs_focus: false,
            goto_text: String::new(),
            status: "Open a log file to start tailing.".into(),
            applied_on_top: false,
        };
        for path in files {
            app.open_path(path);
        }
        app
    }

    fn open_dialog(&mut self) {
        let mut dlg = FileDialog::new()
            .add_filter("Log files", &["log", "txt", "out", "err", "trace"])
            .add_filter("All files", &["*"]);
        if let Some(tab) = self.tabs.get(self.active) {
            if let Some(dir) = tab.engine.path().parent() {
                dlg = dlg.set_directory(dir);
            }
        }
        if let Some(files) = dlg.pick_files() {
            for path in files {
                self.open_path(path);
            }
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        if let Some(idx) = self
            .tabs
            .iter()
            .position(|t| t.engine.path() == path.as_path())
        {
            self.active = idx;
            self.status = format!("Already open: {}", path.display());
            return;
        }
        match TailedFile::open(&path, self.config.tab_width) {
            Ok(engine) => {
                self.config.remember_file(&path);
                let follow = self.config.follow_tail;
                self.tabs.push(Tab::new(engine, follow));
                self.active = self.tabs.len() - 1;
                self.status = format!("Opened {}", path.display());
            }
            Err(err) => {
                self.status = format!("Failed to open {}: {err}", path.display());
            }
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if idx < self.active {
            self.active -= 1;
        }
    }

    fn active_tab(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    fn rebuild_highlighter(&mut self) {
        self.highlighter = Highlighter::new(&self.config.highlight_rules);
        self.config.save();
    }

    fn handle_dropped(&mut self, ui: &Ui) {
        let dropped: Vec<PathBuf> = ui.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        for path in dropped {
            self.open_path(path);
        }
    }

    fn handle_shortcuts(&mut self, ui: &Ui) {
        let typing = ui.ctx().egui_wants_keyboard_input();
        let (ctrl, shift, alt) = ui.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.modifiers.alt,
            )
        });

        let press = |ui: &Ui, key: Key| ui.input(|i| i.key_pressed(key));

        if ctrl && press(ui, Key::O) {
            self.open_dialog();
        }
        if ctrl && press(ui, Key::W) {
            self.close_tab(self.active);
        }
        if ctrl && press(ui, Key::F) {
            self.show_find = true;
        }
        if ctrl && press(ui, Key::H) {
            self.show_highlights = !self.show_highlights;
        }
        if ctrl && press(ui, Key::G) {
            self.show_goto = true;
            self.goto_needs_focus = true;
        }
        if ctrl && press(ui, Key::L) {
            if let Some(tab) = self.active_tab() {
                tab.follow = !tab.follow;
                if tab.follow {
                    tab.scroll_to = Some(tab.engine.view_line_count().saturating_sub(1));
                }
            }
        }
        if press(ui, Key::F1) {
            self.show_help = !self.show_help;
        }
        if press(ui, Key::F5) {
            if let Some(tab) = self.active_tab() {
                match tab.engine.reload() {
                    Ok(()) => {
                        tab.reset_filter_scan();
                        tab.status = Some("Reloaded".into());
                    }
                    Err(err) => tab.status = Some(format!("Reload failed: {err}")),
                }
            }
        }
        if press(ui, Key::F3) {
            self.find_step(!shift);
        }
        if press(ui, Key::Escape) {
            self.show_find = false;
            self.show_goto = false;
        }
        if ctrl && (press(ui, Key::Plus) || press(ui, Key::Equals)) {
            self.config.font_size = (self.config.font_size + 1.0).min(48.0);
        }
        if ctrl && press(ui, Key::Minus) {
            self.config.font_size = (self.config.font_size - 1.0).max(8.0);
        }
        if ctrl && press(ui, Key::Num0) {
            self.config.font_size = 13.0;
        }
        if ctrl && !shift && press(ui, Key::Tab) {
            if !self.tabs.is_empty() {
                self.active = (self.active + 1) % self.tabs.len();
            }
        }
        if ctrl && shift && press(ui, Key::Tab) {
            if !self.tabs.is_empty() {
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
            }
        }
        if ctrl && press(ui, Key::C) && !typing {
            self.copy_selection(ui);
        }
        if ctrl && press(ui, Key::A) && !typing {
            if let Some(tab) = self.active_tab() {
                let last = tab.engine.view_line_count().saturating_sub(1);
                tab.selected = Some((0, last));
            }
        }
        if ctrl && press(ui, Key::Home) {
            if let Some(tab) = self.active_tab() {
                tab.follow = false;
                tab.scroll_to = Some(0);
            }
        }
        if ctrl && press(ui, Key::End) {
            if let Some(tab) = self.active_tab() {
                tab.follow = true;
                tab.scroll_to = Some(tab.engine.view_line_count().saturating_sub(1));
            }
        }
        if !typing && !ctrl && !alt && press(ui, Key::Slash) {
            self.show_find = true;
        }
    }

    fn find_step(&mut self, forward: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.find_query.is_empty() {
            self.show_find = true;
            return;
        }
        let start = tab
            .selected
            .map(|(_, e)| {
                if forward {
                    e.saturating_add(1)
                } else {
                    e.saturating_sub(1)
                }
            })
            .unwrap_or(0);
        match find_line(
            tab,
            start,
            forward,
            &tab.find_query.clone(),
            tab.find_regex,
            tab.find_case,
        ) {
            Ok(Some(line)) => {
                tab.selected = Some((line, line));
                tab.scroll_to = Some(line);
                tab.follow = false;
                tab.find_error = None;
                self.status = format!("Match at line {}", line + 1);
            }
            Ok(None) => {
                tab.find_error = None;
                self.status = "No match".into();
            }
            Err(err) => tab.find_error = Some(err),
        }
    }

    fn copy_selection(&mut self, ui: &Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some((a, b)) = tab.selected else {
            self.status = "Nothing selected".into();
            return;
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let count = (end - start + 1).min(50_000) as usize;
        let lines = tab.engine.read_view_lines(start, count);
        let text = lines.join("\r\n");
        ui.ctx().copy_text(text);
        self.status = format!("Copied {} line(s)", lines.len());
    }

    fn apply_goto(&mut self) {
        let Ok(n) = self.goto_text.trim().parse::<u64>() else {
            self.status = "Go to: enter a 1-based line number".into();
            return;
        };
        if n == 0 {
            return;
        }
        if let Some(tab) = self.active_tab() {
            let line = n - 1;
            tab.follow = false;
            tab.scroll_to = Some(line);
            tab.selected = Some((line, line));
            self.show_goto = false;
        }
    }
}

impl eframe::App for FoxTailApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            self.config.save();
        }
        for tab in &mut self.tabs {
            let was_indexed = tab.engine.is_index_complete();
            match tab.engine.poll() {
                Ok(result) => {
                    if result.truncated {
                        tab.reset_filter_scan();
                        tab.status = Some("File truncated / rotated — reindexed".into());
                    }
                    if !was_indexed && tab.engine.is_index_complete() {
                        tab.reset_filter_scan();
                    }
                    if result.new_lines > 0 || result.grew {
                        tab.pump_filter(8_000);
                    }
                }
                Err(err) => tab.status = Some(format!("I/O: {err}")),
            }
            tab.pump_filter(4_000);
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.seen_lines = tab.engine.view_line_count();
        }
        let following = self.tabs.iter().any(|t| t.follow) || !self.tabs.is_empty();
        let ms = if following {
            self.config.poll_ms
        } else {
            self.config.poll_ms.max(250)
        };
        ctx.request_repaint_after(Duration::from_millis(ms));
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if !self.applied_on_top {
            set_always_on_top(ui, self.config.always_on_top);
            self.applied_on_top = true;
        }

        self.handle_dropped(ui);
        self.handle_shortcuts(ui);

        egui::Panel::top("chrome").show(ui, |ui| {
            self.menu_bar(ui);
            ui.separator();
            self.toolbar(ui);
            if !self.tabs.is_empty() {
                ui.separator();
                self.tab_bar(ui);
            }
            if self.show_find {
                ui.separator();
                self.find_bar(ui);
            }
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            self.status_bar(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.tabs.is_empty() {
                draw_empty(ui);
            } else {
                self.draw_log(ui);
            }
        });

        self.highlight_window(ui);
        self.help_window(ui);
        self.goto_window(ui);
    }
}

impl FoxTailApp {
    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open…\tCtrl+O").clicked() {
                    ui.close();
                    self.open_dialog();
                }
                ui.menu_button("Recent", |ui| {
                    if self.config.recent_files.is_empty() {
                        ui.label(RichText::new("None").color(theme::DIM));
                    }
                    let recents = self.config.recent_files.clone();
                    for path in recents {
                        let label = path.display().to_string();
                        if ui.button(label).clicked() {
                            ui.close();
                            self.open_path(path);
                        }
                    }
                });
                if ui.button("Close tab\tCtrl+W").clicked() {
                    ui.close();
                    self.close_tab(self.active);
                }
                if ui.button("Close all").clicked() {
                    ui.close();
                    self.tabs.clear();
                    self.active = 0;
                }
                ui.separator();
                if ui.button("Reload\tF5").clicked() {
                    ui.close();
                    if let Some(tab) = self.active_tab() {
                        let _ = tab.engine.reload();
                        tab.reset_filter_scan();
                    }
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    self.config.save();
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Find…\tCtrl+F").clicked() {
                    ui.close();
                    self.show_find = true;
                }
                if ui.button("Find next\tF3").clicked() {
                    ui.close();
                    self.find_step(true);
                }
                if ui.button("Find previous\tShift+F3").clicked() {
                    ui.close();
                    self.find_step(false);
                }
                if ui.button("Go to line…\tCtrl+G").clicked() {
                    ui.close();
                    self.show_goto = true;
                    self.goto_needs_focus = true;
                }
                ui.separator();
                if ui.button("Copy\tCtrl+C").clicked() {
                    ui.close();
                    self.copy_selection(ui);
                }
                if ui.button("Select all\tCtrl+A").clicked() {
                    ui.close();
                    if let Some(tab) = self.active_tab() {
                        let last = tab.engine.view_line_count().saturating_sub(1);
                        tab.selected = Some((0, last));
                    }
                }
            });
            ui.menu_button("View", |ui| {
                if ui
                    .checkbox(&mut self.config.word_wrap, "Word wrap")
                    .changed()
                {
                    self.config.save();
                }
                if ui
                    .checkbox(&mut self.config.show_line_numbers, "Line numbers")
                    .changed()
                {
                    self.config.save();
                }
                if ui
                    .checkbox(&mut self.config.always_on_top, "Always on top")
                    .changed()
                {
                    set_always_on_top(ui, self.config.always_on_top);
                    self.config.save();
                }
                ui.separator();
                if ui.button("Zoom in\tCtrl++").clicked() {
                    self.config.font_size = (self.config.font_size + 1.0).min(48.0);
                }
                if ui.button("Zoom out\tCtrl+-").clicked() {
                    self.config.font_size = (self.config.font_size - 1.0).max(8.0);
                }
                if ui.button("Reset zoom\tCtrl+0").clicked() {
                    self.config.font_size = 13.0;
                }
            });
            ui.menu_button("Highlight", |ui| {
                if ui.button("Highlight rules…\tCtrl+H").clicked() {
                    ui.close();
                    self.show_highlights = true;
                }
                if ui.button("Reset default rules").clicked() {
                    ui.close();
                    self.config.highlight_rules = foxtail::highlight::default_rules();
                    self.rebuild_highlighter();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Keyboard shortcuts\tF1").clicked() {
                    ui.close();
                    self.show_help = true;
                }
                ui.label(RichText::new("FoxTail 0.1.0").color(theme::DIM));
            });
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("Open").clicked() {
                self.open_dialog();
            }

            let follow = self.tabs.get(self.active).map(|t| t.follow);
            if let Some(mut follow) = follow {
                if ui
                    .add(egui::Checkbox::new(&mut follow, "Follow tail"))
                    .on_hover_text("Ctrl+L — like tail -f / scroll lock")
                    .changed()
                {
                    if let Some(tab) = self.active_tab() {
                        tab.follow = follow;
                        if follow {
                            tab.scroll_to =
                                Some(tab.engine.view_line_count().saturating_sub(1));
                        }
                    }
                }
            }

            if ui
                .checkbox(&mut self.config.word_wrap, "Wrap")
                .changed()
            {
                self.config.save();
            }

            ui.separator();
            ui.label("Include");
            let mut include = self
                .tabs
                .get(self.active)
                .map(|t| t.filter.include.clone())
                .unwrap_or_default();
            let inc = ui.add(
                TextEdit::singleline(&mut include)
                    .desired_width(140.0)
                    .hint_text("show matching"),
            );
            ui.label("Exclude");
            let mut exclude = self
                .tabs
                .get(self.active)
                .map(|t| t.filter.exclude.clone())
                .unwrap_or_default();
            let exc = ui.add(
                TextEdit::singleline(&mut exclude)
                    .desired_width(140.0)
                    .hint_text("hide matching"),
            );

            let mut regex = self
                .tabs
                .get(self.active)
                .map(|t| t.filter.is_regex)
                .unwrap_or(false);
            let mut case = self
                .tabs
                .get(self.active)
                .map(|t| t.filter.case_sensitive)
                .unwrap_or(false);
            let rx = ui.toggle_value(&mut regex, ".*").on_hover_text("Regex filter");
            let cs = ui
                .toggle_value(&mut case, "Aa")
                .on_hover_text("Case sensitive");

            if inc.changed() || exc.changed() || rx.changed() || cs.changed() {
                if let Some(tab) = self.active_tab() {
                    tab.filter.include = include;
                    tab.filter.exclude = exclude;
                    tab.filter.is_regex = regex;
                    tab.filter.case_sensitive = case;
                    tab.reset_filter_scan();
                }
            }

            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                let mut enc = self
                    .tabs
                    .get(self.active)
                    .map(|t| t.engine.encoding())
                    .unwrap_or(EncodingKind::Utf8);
                egui::ComboBox::from_id_salt("encoding")
                    .selected_text(enc.label())
                    .show_ui(ui, |ui| {
                        for kind in EncodingKind::all() {
                            ui.selectable_value(&mut enc, kind, kind.label());
                        }
                    });
                if let Some(tab) = self.active_tab() {
                    if tab.engine.encoding() != enc {
                        if let Err(err) = tab.engine.set_encoding(enc) {
                            tab.status = Some(format!("Encoding: {err}"));
                        } else {
                            tab.reset_filter_scan();
                        }
                    }
                }
                ui.label("Encoding");
            });
        });
    }

    fn tab_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let mut close = None;
            let mut activate = None;
            for (i, tab) in self.tabs.iter().enumerate() {
                let mut title = tab.engine.file_name();
                if tab.has_unseen() && i != self.active {
                    title = format!("● {title}");
                }
                if !tab.engine.is_index_complete() {
                    title = format!("{title} …");
                }
                let mut text = RichText::new(title).monospace();
                if tab.has_unseen() && i != self.active {
                    text = text.color(FOX);
                }
                let resp = ui.selectable_label(i == self.active, text);
                if resp.clicked() {
                    activate = Some(i);
                }
                if resp.clicked_by(PointerButton::Middle) {
                    close = Some(i);
                }
                resp.on_hover_text(tab.engine.path().display().to_string());
                if ui.small_button("×").clicked() {
                    close = Some(i);
                }
            }
            if ui.button("+").clicked() {
                self.open_dialog();
            }
            if let Some(i) = activate {
                self.active = i;
            }
            if let Some(i) = close {
                self.close_tab(i);
            }
        });
    }

    fn find_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Find");
            let mut query = self
                .tabs
                .get(self.active)
                .map(|t| t.find_query.clone())
                .unwrap_or_default();
            let edit = ui.add(
                TextEdit::singleline(&mut query)
                    .desired_width(240.0)
                    .hint_text("search"),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.find_step(true);
            }
            let mut regex = self
                .tabs
                .get(self.active)
                .map(|t| t.find_regex)
                .unwrap_or(false);
            let mut case = self
                .tabs
                .get(self.active)
                .map(|t| t.find_case)
                .unwrap_or(false);
            let rx = ui.toggle_value(&mut regex, ".*");
            let cs = ui.toggle_value(&mut case, "Aa");
            if ui.button("Prev").clicked() {
                self.find_step(false);
            }
            if ui.button("Next").clicked() {
                self.find_step(true);
            }
            if let Some(tab) = self.active_tab() {
                tab.find_query = query;
                tab.find_regex = regex;
                tab.find_case = case;
                if let Some(err) = &tab.find_error {
                    ui.colored_label(Color32::from_rgb(230, 80, 80), err);
                }
            }
            let _ = (rx, cs);
            if ui.button("×").clicked() {
                self.show_find = false;
            }
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some(tab) = self.tabs.get(self.active) {
                ui.label(tab.engine.path().display().to_string());
                ui.separator();
                ui.label(format_bytes(tab.engine.file_size()));
                ui.separator();
                let lines = tab.engine.line_count();
                let shown = tab.view_count();
                if tab.filter.is_active() {
                    ui.label(format!(
                        "{} / {} lines",
                        format_count(shown),
                        format_count(lines)
                    ));
                    if tab.filter_scan < tab.engine.view_line_count() {
                        ui.label(RichText::new("filtering…").color(FOX));
                    }
                } else {
                    ui.label(format!("{} lines", format_count(lines)));
                }
                ui.separator();
                ui.label(tab.engine.encoding().label());
                if !tab.engine.is_index_complete() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!(
                            "indexing {p:.0}%",
                            p = tab.engine.index_progress() * 100.0
                        ))
                        .color(FOX),
                    );
                }
                ui.separator();
                if tab.follow {
                    ui.colored_label(FOX, "Following");
                } else {
                    ui.label(RichText::new("Paused").color(theme::DIM));
                }
                if let Some(msg) = &tab.status {
                    ui.separator();
                    ui.label(msg);
                }
            } else {
                ui.label(&self.status);
            }
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0} px", self.config.font_size)).color(theme::DIM),
                );
            });
        });
    }

    fn draw_log(&mut self, ui: &mut Ui) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };

        if ui.input(|i| i.smooth_scroll_delta.y > 1.0) {
            tab.follow = false;
        }

        let font_size = self.config.font_size;
        let row_h = font_size + 6.0;
        let show_ln = self.config.show_line_numbers;
        let wrap = self.config.word_wrap;
        let total_rows = tab.view_count() as usize;
        let digits = tab
            .engine
            .view_line_count()
            .max(1)
            .checked_ilog10()
            .unwrap_or(0)
            + 1;
        let ln_w = if show_ln {
            (digits as f32 + 1.5) * font_size * 0.62
        } else {
            0.0
        };

        let mut area = if wrap {
            ScrollArea::vertical()
        } else {
            ScrollArea::both()
        };
        area = area
            .id_salt(tab.engine.path().to_string_lossy().as_ref())
            .stick_to_bottom(tab.follow)
            .auto_shrink([false, false]);

        if let Some(line) = tab.scroll_to.take() {
            let row = if tab.filter.is_active() {
                tab.filter_matches
                    .iter()
                    .position(|&l| l == line)
                    .unwrap_or(0)
            } else {
                line as usize
            };
            area = area.vertical_scroll_offset(row as f32 * (row_h + ui.spacing().item_spacing.y));
        }

        let highlighter = &self.highlighter;
        let find_query = tab.find_query.clone();
        let find_regex = tab.find_regex;
        let find_case = tab.find_case;
        let selected = tab.selected;
        let filter_on = tab.filter.is_active();

        let output = area.show_rows(ui, row_h, total_rows, |ui, range| {
            let rows: Vec<(u64, String)> = if filter_on {
                range
                    .clone()
                    .filter_map(|row| {
                        let line = tab.filter_matches.get(row).copied()?;
                        Some((line, tab.engine.read_line(line)))
                    })
                    .collect()
            } else {
                let lines = tab
                    .engine
                    .read_view_lines(range.start as u64, range.end.saturating_sub(range.start));
                lines
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| (range.start as u64 + i as u64, s))
                    .collect()
            };

            let mut clicked = None;
            let mut shift = false;
            for (line_no, text) in &rows {
                let (resp, is_shift) = draw_row(
                    ui,
                    *line_no,
                    text,
                    row_h,
                    ln_w,
                    font_size,
                    show_ln,
                    wrap,
                    highlighter,
                    selected,
                    &find_query,
                    find_regex,
                    find_case,
                );
                if resp.clicked() {
                    clicked = Some(*line_no);
                    shift = is_shift;
                }
                resp.context_menu(|ui| {
                    if ui.button("Copy line").clicked() {
                        ui.ctx().copy_text(text.clone());
                        ui.close();
                    }
                    if ui.button("Copy file path").clicked() {
                        ui.ctx().copy_text(tab.engine.path().display().to_string());
                        ui.close();
                    }
                });
            }
            (clicked, shift)
        });

        if let (Some(line), shift) = output.inner {
            if shift {
                if let Some((a, _)) = tab.selected {
                    tab.selected = Some((a, line));
                } else {
                    tab.selected = Some((line, line));
                }
            } else {
                tab.selected = Some((line, line));
            }
        }
    }

    fn highlight_window(&mut self, ui: &mut Ui) {
        if !self.show_highlights {
            return;
        }
        let mut open = self.show_highlights;
        egui::Window::new("Highlight rules")
            .open(&mut open)
            .resizable(true)
            .default_width(640.0)
            .show(ui.ctx(), |ui| {
                ui.label("First matching enabled rule wins.");
                ui.add_space(4.0);
                let mut changed = false;
                let mut remove = None;
                let mut move_up = None;
                let n = self.config.highlight_rules.len();
                for i in 0..n {
                    ui.horizontal(|ui| {
                        let rule = &mut self.config.highlight_rules[i];
                        changed |= ui.checkbox(&mut rule.enabled, "").changed();
                        changed |= ui
                            .add(
                                TextEdit::singleline(&mut rule.pattern)
                                    .desired_width(220.0)
                                    .hint_text("pattern"),
                            )
                            .changed();
                        changed |= ui
                            .toggle_value(&mut rule.is_regex, ".*")
                            .on_hover_text("Regex")
                            .changed();
                        changed |= ui
                            .toggle_value(&mut rule.case_sensitive, "Aa")
                            .on_hover_text("Case sensitive")
                            .changed();
                        changed |= ui
                            .toggle_value(&mut rule.bold, "B")
                            .on_hover_text("Bold")
                            .changed();
                        ui.label("Fg");
                        changed |= ui.color_edit_button_srgb(&mut rule.fg).changed();
                        ui.label("Bg");
                        changed |= ui.color_edit_button_srgb(&mut rule.bg).changed();
                        if ui.small_button("↑").clicked() && i > 0 {
                            move_up = Some(i);
                        }
                        if ui.small_button("✕").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = move_up {
                    self.config.highlight_rules.swap(i, i - 1);
                    changed = true;
                }
                if let Some(i) = remove {
                    self.config.highlight_rules.remove(i);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Add rule").clicked() {
                        self.config.highlight_rules.push(
                            foxtail::highlight::HighlightRule::new(
                                "",
                                [224, 224, 230],
                                [40, 40, 48],
                            ),
                        );
                        changed = true;
                    }
                    if ui.button("Reset defaults").clicked() {
                        self.config.highlight_rules = foxtail::highlight::default_rules();
                        changed = true;
                    }
                });
                if changed {
                    self.rebuild_highlighter();
                }
            });
        self.show_highlights = open;
    }

    fn help_window(&mut self, ui: &mut Ui) {
        if !self.show_help {
            return;
        }
        let mut open = self.show_help;
        egui::Window::new("FoxTail")
            .open(&mut open)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.heading("Real-time log viewer");
                ui.label("Follow growing log files with highlighting, filters, and search.");
                ui.separator();
                ui.label("Ctrl+O    Open files");
                ui.label("Ctrl+W    Close tab");
                ui.label("Ctrl+Tab  Next tab");
                ui.label("Ctrl+F    Find");
                ui.label("F3        Find next");
                ui.label("Ctrl+G    Go to line");
                ui.label("Ctrl+L    Toggle follow tail");
                ui.label("Ctrl+H    Highlight rules");
                ui.label("Ctrl+C    Copy selection");
                ui.label("F5        Reload");
                ui.label("Drop files onto the window to open them.");
            });
        self.show_help = open;
    }

    fn goto_window(&mut self, ui: &mut Ui) {
        if !self.show_goto {
            return;
        }
        let mut open = self.show_goto;
        egui::Window::new("Go to line")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Line");
                    let r = ui.add(
                        TextEdit::singleline(&mut self.goto_text)
                            .desired_width(100.0)
                            .hint_text("1"),
                    );
                    if self.goto_needs_focus {
                        r.request_focus();
                        self.goto_needs_focus = false;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        self.apply_goto();
                    }
                    if ui.button("Go").clicked() {
                        self.apply_goto();
                    }
                });
            });
        self.show_goto = open;
    }
}

fn draw_empty(ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("FoxTail").size(36.0).color(FOX).strong());
            ui.label(RichText::new("Real-time log viewer").color(theme::DIM));
            ui.add_space(12.0);
            ui.label("Open a file with Ctrl+O, pass paths on the command line,");
            ui.label("or drop log files onto this window.");
            ui.add_space(8.0);
            ui.label(
                RichText::new("Follow tail · highlighting · include/exclude · search")
                    .color(theme::DIM)
                    .italics(),
            );
        });
    });
}

fn draw_row(
    ui: &mut Ui,
    line_no: u64,
    text: &str,
    row_h: f32,
    ln_w: f32,
    font_size: f32,
    show_ln: bool,
    wrap: bool,
    highlighter: &Highlighter,
    selected: Option<(u64, u64)>,
    find_query: &str,
    find_regex: bool,
    find_case: bool,
) -> (egui::Response, bool) {
    let avail = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail, row_h), Sense::click());
    let shift = ui.input(|i| i.modifiers.shift);

    let is_sel = selected.is_some_and(|(a, b)| {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        line_no >= lo && line_no <= hi
    });
    let find_hit = !find_query.is_empty()
        && line_matches(text, find_query, find_regex, find_case).unwrap_or(false);
    let style = highlighter.style_for(text);

    let mut bg = theme::ROW;
    let mut fg = theme::TEXT;
    let mut bold = false;
    if let Some(s) = style {
        bg = theme::rgb(s.bg);
        fg = theme::rgb(s.fg);
        bold = s.bold;
    }
    if is_sel {
        bg = Color32::from_rgb(
            bg.r().saturating_add(28),
            bg.g().saturating_add(18),
            bg.b().saturating_add(8),
        );
    }

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, bg);
    if is_sel {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            0.0,
            FOX,
        );
    } else if find_hit {
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            0.0,
            FIND,
        );
    }

    let font = FontId::monospace(font_size);
    let mut x = rect.min.x + 6.0;
    if show_ln {
        let ln = format!("{}", line_no + 1);
        painter.text(
            Pos2::new(x + ln_w - 8.0, rect.min.y + 2.0),
            Align2::RIGHT_TOP,
            ln,
            font.clone(),
            theme::DIM,
        );
        x += ln_w;
        painter.line_segment(
            [
                Pos2::new(x - 4.0, rect.min.y),
                Pos2::new(x - 4.0, rect.max.y),
            ],
            Stroke::new(1.0, Color32::from_rgb(40, 40, 48)),
        );
    }

    let text_rect = Rect::from_min_max(Pos2::new(x, rect.min.y), rect.max);
    let wrap_width = if wrap {
        (text_rect.width() - 4.0).max(1.0)
    } else {
        f32::INFINITY
    };
    let text_size = if bold { font_size + 0.5 } else { font_size };
    let galley = ui.fonts_mut(|f| {
        f.layout(
            text.to_owned(),
            FontId::monospace(text_size),
            fg,
            wrap_width,
        )
    });
    painter.galley(Pos2::new(x, rect.min.y + 1.0), galley, fg);

    (resp, shift)
}

fn line_matches(line: &str, query: &str, regex: bool, case: bool) -> Result<bool, String> {
    if query.is_empty() {
        return Ok(false);
    }
    if regex {
        let re = RegexBuilder::new(query)
            .case_insensitive(!case)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(re.is_match(line))
    } else if case {
        Ok(line.contains(query))
    } else {
        Ok(foxtail::highlight::contains_ignore_ascii_case(line, query))
    }
}

fn find_line(
    tab: &mut Tab,
    start: u64,
    forward: bool,
    query: &str,
    regex: bool,
    case: bool,
) -> Result<Option<u64>, String> {
    if tab.filter.is_active() {
        let matches = tab.filter_matches.clone();
        if matches.is_empty() {
            return Ok(None);
        }
        let n = matches.len();
        let start_idx = if forward {
            matches.iter().position(|&l| l >= start).unwrap_or(0)
        } else {
            matches
                .iter()
                .rposition(|&l| l <= start)
                .unwrap_or(n - 1)
        };
        let limit = n.min(200_000);
        for step in 0..limit {
            let idx = if forward {
                (start_idx + step) % n
            } else {
                (start_idx + n - step) % n
            };
            let line_no = matches[idx];
            let text = tab.engine.read_line(line_no);
            if line_matches(&text, query, regex, case)? {
                return Ok(Some(line_no));
            }
        }
        return if n > 200_000 {
            Err("Search limited to 200,000 lines — press F3 to continue".into())
        } else {
            Ok(None)
        };
    }

    let total = tab.engine.view_line_count();
    if total == 0 {
        return Ok(None);
    }
    let mut i = start.min(total.saturating_sub(1));
    let mut seen = 0u64;
    const BATCH: usize = 512;
    while seen < total.min(200_000) {
        if forward {
            let take = ((total - i) as usize).min(BATCH);
            let lines = tab.engine.read_view_lines(i, take);
            for (k, line) in lines.iter().enumerate() {
                if line_matches(line, query, regex, case)? {
                    return Ok(Some(i + k as u64));
                }
            }
            seen += take as u64;
            i = if i + take as u64 >= total {
                0
            } else {
                i + take as u64
            };
        } else {
            let line = tab.engine.read_line(i);
            if line_matches(&line, query, regex, case)? {
                return Ok(Some(i));
            }
            i = if i == 0 { total - 1 } else { i - 1 };
            seen += 1;
        }
    }
    if total > 200_000 {
        Err("Search limited to 200,000 lines — press F3 to continue".into())
    } else {
        Ok(None)
    }
}

fn set_always_on_top(ui: &Ui, on: bool) {
    ui.ctx().send_viewport_cmd(ViewportCommand::WindowLevel(if on {
        egui::WindowLevel::AlwaysOnTop
    } else {
        egui::WindowLevel::Normal
    }));
}
