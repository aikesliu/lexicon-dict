#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod fonts;
mod query;
mod rich;
mod speech;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{
    Align, Button, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Label, Layout, Margin,
    RichText, ScrollArea, Sense, Stroke, TextEdit, TextWrapMode, Vec2, ViewportCommand,
};

use config::{display_dict_name, Gui, Paths};
use query::{BlockData, Engine, Hit};

// ---------------------------------------------------------------------------
// 配色
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    bg: Color32,
    panel: Color32,
    card: Color32,
    card_hover: Color32,
    input: Color32,
    border: Color32,
    border_hi: Color32,
    text: Color32,
    muted: Color32,
    faint: Color32,
    accent: Color32,
    accent_dim: Color32,
    blue: Color32,
    danger: Color32,
    selected: Color32,
}

impl Palette {
    fn dark() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(13, 16, 22),
            panel: Color32::from_rgb(18, 22, 30),
            card: Color32::from_rgb(24, 29, 39),
            card_hover: Color32::from_rgb(28, 34, 45),
            input: Color32::from_rgb(22, 27, 36),
            border: Color32::from_rgb(35, 43, 56),
            border_hi: Color32::from_rgb(56, 68, 86),
            text: Color32::from_rgb(230, 234, 242),
            muted: Color32::from_rgb(140, 149, 166),
            faint: Color32::from_rgb(92, 102, 120),
            accent: Color32::from_rgb(232, 180, 90),
            accent_dim: Color32::from_rgb(120, 94, 48),
            blue: Color32::from_rgb(122, 162, 247),
            danger: Color32::from_rgb(240, 113, 120),
            selected: Color32::from_rgb(32, 38, 51),
        }
    }
    fn light() -> Self {
        Self {
            dark: false,
            bg: Color32::from_rgb(247, 248, 250),
            panel: Color32::from_rgb(255, 255, 255),
            card: Color32::from_rgb(255, 255, 255),
            card_hover: Color32::from_rgb(250, 251, 253),
            input: Color32::from_rgb(240, 242, 246),
            border: Color32::from_rgb(224, 228, 235),
            border_hi: Color32::from_rgb(196, 203, 214),
            text: Color32::from_rgb(31, 35, 40),
            muted: Color32::from_rgb(100, 110, 125),
            faint: Color32::from_rgb(140, 148, 160),
            accent: Color32::from_rgb(166, 106, 22),
            accent_dim: Color32::from_rgb(228, 203, 160),
            blue: Color32::from_rgb(37, 99, 235),
            danger: Color32::from_rgb(200, 60, 70),
            selected: Color32::from_rgb(255, 248, 232),
        }
    }
}

// ---------------------------------------------------------------------------
// 数据
// ---------------------------------------------------------------------------

struct Tab {
    word: String,
    hits: Vec<Hit>,
    selected: i32,
    visible: usize,
    total: usize,
    error: Option<String>,
}

impl Tab {
    fn new(word: String) -> Self {
        Self {
            word,
            hits: Vec::new(),
            selected: -1,
            visible: 10,
            total: 0,
            error: None,
        }
    }
}

struct Shot {
    path: String,
    frames: u32,
}

struct DictRow {
    id: String,
    name: String,
    count: u64,
    enabled: bool,
}

struct SettingsDraft {
    gui: Gui,
    core: lexicon_core::common::Config,
    dicts: Vec<DictRow>,
    selected: usize,
    new_dir: String,
    tab: usize,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct App {
    paths: Paths,
    gui: Gui,
    engine: Option<Engine>,
    palette: Palette,
    tabs: Vec<Tab>,
    active: usize,
    search: String,
    search_id: Id,
    status: String,
    status_error: bool,
    debounce_at: Option<Instant>,
    last_queried: String,
    nav: bool,
    focus_results: bool,
    help: bool,
    show_settings: bool,
    settings: Option<SettingsDraft>,
    textures: HashMap<PathBuf, egui::TextureHandle>,
    scroll_to: Option<usize>,
    query_gen: u64,
    convert_rx: Option<Receiver<Result<String, String>>>,
    converting: bool,
    shot: Option<Shot>,
    stats: String,
    fonts_ready: bool,
    frame_no: u32,
    focus_search_next: bool,
}

impl App {
    fn new(
        cc: &eframe::CreationContext<'_>,
        paths: Paths,
        gui: Gui,
        shot: Option<Shot>,
        test_query: Option<String>,
    ) -> Self {
        fonts::install(&cc.egui_ctx);
        let mut gui = gui;
        if let Ok(t) = std::env::var("LEXICON_THEME") {
            gui.theme = t;
        }
        let palette = if gui.theme == "light" {
            Palette::light()
        } else {
            Palette::dark()
        };
        let core = paths.load_core();
        let engine = Engine::load(core.clone());
        let dict_count = engine.dict_count();
        let entries = engine.total_entries;
        let mut app = Self {
            paths,
            gui,
            engine: Some(engine),
            palette,
            tabs: Vec::new(),
            active: 0,
            search: String::new(),
            search_id: Id::new("search"),
            status: String::new(),
            status_error: false,
            debounce_at: None,
            last_queried: String::new(),
            nav: false,
            focus_results: false,
            help: false,
            show_settings: false,
            settings: None,
            textures: HashMap::new(),
            scroll_to: None,
            query_gen: 0,
            convert_rx: None,
            converting: false,
            shot,
            stats: format!("{dict_count} 本词典 · {} 词条", fmt_count(entries)),
            fonts_ready: true,
            frame_no: 0,
            focus_search_next: false,
        };
        let has_source_dirs = app
            .engine
            .as_ref()
            .is_some_and(|e| !e.config.source_dirs.is_empty());
        if dict_count == 0 && has_source_dirs {
            app.start_convert();
        }
        app.status = if dict_count == 0 {
            if app.converting {
                "首次启动：正在扫描并转换词典…".into()
            } else {
                app.empty_hint()
            }
        } else {
            "输入单词开始查询".into()
        };
        if let Some(q) = test_query {
            app.search = q.clone();
            let ctx = cc.egui_ctx.clone();
            app.query_now(&ctx);
        }
        app
    }

    fn empty_hint(&self) -> String {
        "没有已转换的词典。打开设置 → 词典目录，添加 .ld2 所在目录，然后点“扫描并转换”。".into()
    }

    fn apply_style(&self, ctx: &egui::Context) {
        let p = self.palette;
        ctx.set_theme(if p.dark {
            egui::ThemePreference::Dark
        } else {
            egui::ThemePreference::Light
        });
        ctx.all_styles_mut(|style| {
            style.spacing.item_spacing = Vec2::new(10.0, 10.0);
            style.spacing.button_padding = Vec2::new(10.0, 6.0);
            style.spacing.menu_margin = Margin::same(8);
            style.visuals = if p.dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            let v = &mut style.visuals;
            v.panel_fill = p.bg;
            v.window_fill = p.panel;
            v.window_stroke = Stroke::new(1.0, p.border);
            v.window_corner_radius = CornerRadius::same(14);
            v.menu_corner_radius = CornerRadius::same(10);
            v.extreme_bg_color = p.input;
            v.faint_bg_color = p.card;
            v.override_text_color = Some(p.text);
            v.hyperlink_color = p.blue;
            v.selection.bg_fill = p.accent.gamma_multiply(0.35);
            v.selection.stroke = Stroke::new(1.0, p.accent);
            v.widgets.noninteractive.bg_fill = p.panel;
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
            v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.hovered.bg_fill = p.card_hover;
            v.widgets.hovered.weak_bg_fill = p.card_hover;
            v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.active.bg_fill = p.card_hover;
            v.widgets.active.fg_stroke = Stroke::new(1.0, p.accent);
            v.window_shadow = egui::Shadow {
                offset: [0, 10],
                blur: 30,
                spread: 0,
                color: Color32::from_black_alpha(if p.dark { 140 } else { 40 }),
            };
            style.text_styles.insert(
                egui::TextStyle::Body,
                FontId::new(14.5, FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            );
            style.text_styles.insert(
                egui::TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            );
        });
    }

    fn start_convert(&mut self) {
        if self.converting {
            return;
        }
        let Some(engine) = &self.engine else { return };
        let config = engine.config.clone();
        let (tx, rx) = channel();
        self.convert_rx = Some(rx);
        self.converting = true;
        std::thread::spawn(move || {
            let res = lexicon_core::convert::convert_directory(
                &config.source_dirs,
                &config.cache_dir,
                lexicon_core::convert::ConvertOptions {
                    jobs: Some(1),
                    ..Default::default()
                },
            )
            .map(|s| format!("转换完成：{} 本（发现 {}）", s.converted, s.total))
            .map_err(|e| e.to_string());
            let _ = tx.send(res);
        });
    }

    fn poll_convert(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.convert_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(res) => {
                self.converting = false;
                match res {
                    Ok(msg) => {
                        self.status = msg;
                        self.reload_engine();
                    }
                    Err(e) => {
                        self.status = format!("转换失败：{e}");
                        self.status_error = true;
                    }
                }
                ctx.request_repaint();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.convert_rx = Some(rx);
                ctx.request_repaint_after(Duration::from_millis(250));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.converting = false;
            }
        }
    }

    fn reload_engine(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.reload();
            self.stats = format!(
                "{} 本词典 · {} 词条",
                engine.dict_count(),
                fmt_count(engine.total_entries)
            );
        }
        self.requery_active();
    }

    fn requery_active(&mut self) {
        self.tabs.clear();
        self.active = 0;
        if !self.search.trim().is_empty() {
            let words = parse_words(&self.search);
            self.run_words(words);
        }
    }

    fn fmt_count(n: u64) -> String {
        let s = n.to_string();
        let mut out = String::with_capacity(s.len() + s.len() / 3);
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        out
    }

    fn parse_words(text: &str) -> Vec<String> {
        parse_words(text)
    }

    fn run_words(&mut self, words: Vec<String>) {
        if self.engine.is_none() {
            return;
        }
        // 移除已不在输入中的 tab
        self.tabs.retain(|t| words.iter().any(|w| w == &t.word));
        let mut first_new = None;
        let mut last = 0usize;
        for w in &words {
            if let Some(i) = self.tabs.iter().position(|t| &t.word == w) {
                last = i;
                continue;
            }
            let (hits, total) = {
                let engine = self.engine.as_ref().unwrap();
                (engine.query(w), engine.dict_count())
            };
            let mut tab = Tab::new(w.clone());
            tab.total = total;
            tab.hits = hits;
            tab.visible = tab.hits.len().min(self.gui.page_size.max(1));
            if !tab.hits.is_empty() {
                tab.selected = 0;
            }
            self.tabs.push(tab);
            let idx = self.tabs.len() - 1;
            if first_new.is_none() {
                first_new = Some(idx);
            }
            last = idx;
        }
        self.active = first_new.unwrap_or(last);
        self.nav = true;
        self.focus_results = true;
        self.scroll_to = None;
        self.update_status();
    }

    fn query_now(&mut self, ctx: &egui::Context) {
        self.debounce_at = None;
        let text = self.search.trim().to_string();
        if text.is_empty() {
            self.status = "输入单词开始查询".into();
            return;
        }
        let words = parse_words(&text);
        if words.is_empty() {
            return;
        }
        self.last_queried = text;
        self.query_gen = self.query_gen.wrapping_add(1);
        self.run_words(words);
        // 关键：查询后把焦点从输入框移走，否则 j/k 会被当作输入。
        ctx.memory_mut(|m| m.surrender_focus(self.search_id));
        self.grab_results(ctx);
    }

    fn grab_results(&mut self, ctx: &egui::Context) {
        self.nav = true;
        let id = self.results_focus_id();
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// 结果导航的滚动策略：已完整可见就不动；一旦需要滚动，从卡片顶部开始显示。
    /// 不能居中，否则高于视口的词条会从中间开始阅读。
    fn scroll_align(viewport: egui::Rect, card: egui::Rect) -> Option<Align> {
        if viewport.contains_rect(card) {
            None
        } else {
            Some(Align::TOP)
        }
    }

    fn results_focus_id(&self) -> Id {
        Id::new("results_focus")
    }

    fn update_status(&mut self) {
        if self.converting {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            self.status = "输入单词开始查询".into();
            return;
        };
        let multi = if self.tabs.len() > 1 {
            format!("[{}/{}] ", self.active + 1, self.tabs.len())
        } else {
            String::new()
        };
        self.status = if let Some(e) = &tab.error {
            format!("{multi}[错误] {e}")
        } else if tab.hits.is_empty() {
            format!(
                "{multi}未找到「{}」（已查询 {} 个词典）",
                tab.word, tab.total
            )
        } else if tab.visible < tab.hits.len() {
            format!(
                "{multi}「{}」命中 {}/{}，已显示 {} 条（继续滚动加载）",
                tab.word,
                tab.hits.len(),
                tab.total,
                tab.visible
            )
        } else {
            format!(
                "{multi}「{}」命中 {}/{}",
                tab.word,
                tab.hits.len(),
                tab.total
            )
        };
        self.status_error = tab.error.is_some();
    }

    fn tick(&mut self, ctx: &egui::Context) {
        if let Some(at) = self.debounce_at {
            if Instant::now() >= at {
                self.debounce_at = None;
                let text = self.search.trim().to_string();
                if !text.is_empty() && text != self.last_queried {
                    self.query_now(ctx);
                }
            } else {
                ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
            }
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.show_settings = false;
                self.settings = None;
            }
            return;
        }
        if self.help {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Questionmark))
            {
                self.help = false;
            }
            return;
        }
        let search_focused = ctx.memory(|m| m.has_focus(self.search_id));
        if search_focused {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.search.clear();
                self.status = "输入单词开始查询".into();
            }
            return;
        }
        let sh = self.gui.shortcuts.clone();
        // egui::InputState::consume_key 会忽略多余的 Shift/Alt，所以必须先匹配
        // 带修饰键的组合（Shift+Tab、Shift+V…），否则会被 Tab / v 抢走。
        let mut binds: Vec<(&str, u8, egui::Modifiers, egui::Key)> = config::ACTIONS
            .iter()
            .filter_map(|(name, _)| {
                let spec = sh.get(*name).cloned().unwrap_or_default();
                let (mods, key) = parse_shortcut(&spec)?;
                let weight = mods.shift as u8 + mods.alt as u8 + mods.ctrl as u8;
                Some((*name, weight, mods, key))
            })
            .collect();
        binds.sort_by_key(|(_, weight, _, _)| std::cmp::Reverse(*weight));
        let mut action: Option<&'static str> = None;
        for (name, _, mods, key) in binds {
            if ctx.input_mut(|i| i.consume_key(mods, key)) {
                action = Some(match name {
                    "next" => "next",
                    "prev" => "prev",
                    "page_down" => "page_down",
                    "page_up" => "page_up",
                    "next_tab" => "next_tab",
                    "prev_tab" => "prev_tab",
                    "copy" => "copy",
                    "us" => "us",
                    "uk" => "uk",
                    "first" => "first",
                    "last" => "last",
                    "focus" => "focus",
                    "help" => "help",
                    "close" => "close",
                    _ => "",
                });
                break;
            }
        }
        let Some(action) = action else { return };
        match action {
            "next" => self.move_selection(1),
            "prev" => self.move_selection(-1),
            "page_down" => self.move_selection(self.gui.page_size.max(1) as i32),
            "page_up" => self.move_selection(-(self.gui.page_size.max(1) as i32)),
            "next_tab" => self.switch_tab(1),
            "prev_tab" => self.switch_tab(-1),
            "copy" => self.copy_selected(ctx),
            "us" => self.speak_selected("us"),
            "uk" => self.speak_selected("uk"),
            "first" => self.jump_selection(false),
            "last" => self.jump_selection(true),
            "focus" => {
                // 下一帧再聚焦，避免本次按下的 "/" 被 TextEdit 当成文本插入。
                self.focus_search_next = true;
            }
            "help" => self.help = !self.help,
            "close" => {
                ctx.memory_mut(|m| m.request_focus(self.search_id));
                self.nav = false;
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let page = self.gui.page_size.max(1);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.hits.is_empty() {
            return;
        }
        let mut target = if tab.selected < 0 {
            0
        } else {
            tab.selected + delta
        };
        if target >= tab.visible as i32 && tab.visible < tab.hits.len() {
            tab.visible = (tab.visible + page).min(tab.hits.len());
        }
        target = target.clamp(0, tab.visible as i32 - 1);
        tab.selected = target;
        self.scroll_to = Some(target as usize);
        self.update_status();
    }

    fn jump_selection(&mut self, last: bool) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.hits.is_empty() {
            return;
        }
        if last {
            tab.visible = tab.hits.len();
            tab.selected = tab.hits.len() as i32 - 1;
        } else {
            tab.selected = 0;
        }
        self.scroll_to = Some(tab.selected.max(0) as usize);
        self.update_status();
    }

    fn switch_tab(&mut self, delta: i32) {
        if self.tabs.len() < 2 {
            return;
        }
        let n = self.tabs.len() as i32;
        self.active = ((self.active as i32 + delta + n) % n) as usize;
        self.scroll_to = None;
        self.nav = true;
        self.update_status();
    }

    fn copy_selected(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.selected < 0 || tab.selected as usize >= tab.hits.len() {
            return;
        }
        let hit = &tab.hits[tab.selected as usize];
        let text = if hit.blocks.is_empty() {
            rich::plain(&hit.pango)
        } else {
            hit.blocks
                .iter()
                .filter_map(|b| match b {
                    BlockData::Text(p) => Some(rich::plain(p)),
                    BlockData::Image { .. } => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        ctx.copy_text(text.trim().to_string());
        self.status = "已复制".into();
    }

    fn speak_selected(&self, accent: &str) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.selected < 0 || tab.selected as usize >= tab.hits.len() {
            return;
        }
        speech::speak(&tab.hits[tab.selected as usize].word, accent);
    }

    // -----------------------------------------------------------------------
    // 绘制
    // -----------------------------------------------------------------------

    fn draw(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(ui.max_rect(), 0, p.bg);
        if self.show_settings {
            self.settings_window(&ctx);
            return;
        }
        if self.help {
            self.help_screen(ui);
            return;
        }

        Frame::new()
            .inner_margin(Margin {
                left: 22,
                right: 22,
                top: 14,
                bottom: 12,
            })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.header(ui);
                ui.add_space(2.0);
                self.search_bar(ui);
                ui.add_space(2.0);
                self.tab_bar(ui);
                self.status_row(ui);
                let footer_h = 22.0;
                let h = (ui.available_height() - footer_h).max(80.0);
                self.results(ui, h);
                self.footer(ui);
            });
        self.resize_grip(ui);
    }

    fn resize_grip(&self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let rect = egui::Rect::from_min_size(
            ui.max_rect().right_bottom() - Vec2::splat(18.0),
            Vec2::splat(16.0),
        );
        let resp = ui.interact(rect, Id::new("window-resize-grip"), Sense::drag());
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
        }
        if resp.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(
                egui::ResizeDirection::SouthEast,
            ));
        }
        if resp.hovered() || resp.dragged() {
            let c = self.palette.faint;
            for i in 0..3 {
                let off = 4.0 + i as f32 * 4.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.right() - off, rect.bottom() - 2.0),
                        egui::pos2(rect.right() - 2.0, rect.bottom() - off),
                    ],
                    Stroke::new(1.0, c),
                );
            }
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.horizontal(|ui| {
            logo(ui);
            ui.add_space(1.0);
            ui.label(
                RichText::new("词典")
                    .size(16.5)
                    .family(fonts::bold())
                    .color(p.text),
            );

            // 中间留出可拖动区域；右侧是应用按钮 + 自绘窗口按钮。
            const RIGHT_W: f32 = 238.0;
            let drag_w = (ui.available_width() - RIGHT_W).max(0.0);
            if drag_w > 1.0 {
                let (_, drag) =
                    ui.allocate_exact_size(Vec2::new(drag_w, 28.0), Sense::click_and_drag());
                let drag = drag.on_hover_cursor(egui::CursorIcon::Grab);
                if drag.double_clicked() {
                    toggle_maximized(ui.ctx());
                } else if drag.drag_started() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                window_controls(ui, p);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(2.0);
                let (divider, _) = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover());
                ui.painter().rect_filled(divider, 0, p.border);

                if icon_button(ui, Icon::Refresh, p)
                    .on_hover_text("重新加载词典")
                    .clicked()
                {
                    self.reload_engine();
                    self.status = "词典已重载".into();
                }
                if icon_button(ui, Icon::Settings, p)
                    .on_hover_text("设置")
                    .clicked()
                {
                    self.open_settings();
                }
                let icon = if p.dark { Icon::Moon } else { Icon::Sun };
                if icon_button(ui, icon, p).on_hover_text("切换主题").clicked() {
                    self.gui.theme = if p.dark {
                        "light".into()
                    } else {
                        "dark".into()
                    };
                    self.palette = if p.dark {
                        Palette::light()
                    } else {
                        Palette::dark()
                    };
                    self.paths.save_gui(&self.gui);
                }
            });
        });
    }

    fn search_bar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let focused = ui.ctx().memory(|m| m.has_focus(self.search_id));
        let stroke = Stroke::new(
            if focused { 1.6 } else { 1.0 },
            if focused { p.accent } else { p.border },
        );
        Frame::new()
            .fill(p.input)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::symmetric(9, 2))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    search_icon(ui, if focused { p.accent } else { p.faint });
                    ui.add_space(3.0);
                    let width = (ui.available_width() - 10.0).max(120.0);
                    let te = TextEdit::singleline(&mut self.search)
                        .id(self.search_id)
                        .hint_text("输入单词，回车查询；多个单词用空格分隔")
                        .font(FontId::new(15.5, FontFamily::Proportional))
                        .text_color(p.text)
                        .margin(Margin::symmetric(2, 0))
                        .align(egui::Align2::LEFT_CENTER)
                        .desired_width(width)
                        .frame(Frame::NONE);
                    let resp = ui.add_sized([width, 30.0], te);
                    if resp.changed() {
                        self.debounce_at = Some(
                            Instant::now() + Duration::from_millis(self.gui.debounce_ms.max(1)),
                        );
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.query_now(&ui.ctx().clone());
                    }
                    if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.query_now(&ui.ctx().clone());
                    }
                });
            });
        if !self.tabs.is_empty() && !ui.ctx().memory(|m| m.has_focus(self.search_id)) {
            ui.ctx().request_repaint();
        }
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        if self.tabs.len() <= 1 {
            return;
        }
        let p = self.palette;
        ui.horizontal_wrapped(|ui| {
            for i in 0..self.tabs.len() {
                let active = i == self.active;
                let text = RichText::new(&self.tabs[i].word)
                    .size(12.5)
                    .color(if active { p.accent } else { p.muted });
                let btn = Button::new(text)
                    .frame(false)
                    .fill(if active {
                        p.selected
                    } else {
                        Color32::TRANSPARENT
                    })
                    .corner_radius(CornerRadius::same(8))
                    .min_size(Vec2::new(0.0, 26.0));
                if ui.add(btn).clicked() {
                    self.active = i;
                    self.nav = true;
                    self.scroll_to = Some(0);
                    self.update_status();
                }
            }
        });
    }

    fn status_row(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.horizontal(|ui| {
            let color = if self.status_error { p.danger } else { p.muted };
            ui.label(RichText::new(&self.status).size(12.0).color(color));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(tab) = self.tabs.get(self.active) {
                    if !tab.hits.is_empty() {
                        ui.label(
                            RichText::new(format!(
                                "{} / {} 条",
                                tab.visible.min(tab.hits.len()),
                                tab.hits.len()
                            ))
                            .size(11.0)
                            .color(p.faint),
                        );
                    }
                }
            });
        });
    }

    fn results(&mut self, ui: &mut egui::Ui, height: f32) {
        // 固定结果区高度，空状态/结果很少时 footer 才不会跟着内容上浮。
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            Vec2::new(width, height),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(height);
                self.results_inner(ui, height);
            },
        );
    }

    fn results_inner(&mut self, ui: &mut egui::Ui, height: f32) {
        let p = self.palette;
        let scroll_to = self.scroll_to.take();
        let Some(tab) = self.tabs.get(self.active) else {
            self.empty_state(ui, height);
            return;
        };
        let tab_word = tab.word.clone();
        let tab_hits = tab.hits.clone();
        let tab_visible = tab.visible;
        let tab_selected = tab.selected;
        let tab_error = tab.error.clone();
        if tab_hits.is_empty() {
            let msg = tab_error.unwrap_or_else(|| "未找到匹配的词条".to_string());
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), height),
                Layout::top_down(Align::Center),
                |ui| {
                    ui.add_space(height * 0.28);
                    ui.label(RichText::new("∅").size(34.0).color(p.faint));
                    ui.add_space(6.0);
                    ui.label(RichText::new(msg).size(13.0).color(p.muted));
                },
            );
            return;
        }
        let focus_id = self.results_focus_id();
        let scroll = ScrollArea::vertical()
            // 词和命中数进入 salt：重新查询后从顶部开始，不继承上一轮的滚动位置。
            .id_salt((
                "results",
                self.active,
                tab_word,
                tab_hits.len(),
                self.query_gen,
            ))
            .auto_shrink([false, false])
            .max_height(height);
        let mut clicked_card: Option<usize> = None;
        scroll.show(ui, |ui| {
            let _focus = ui.interact(ui.max_rect(), focus_id, Sense::click());
            ui.add_space(2.0);
            for i in 0..tab_visible.min(tab_hits.len()) {
                let selected = tab_selected == i as i32;
                let hit = &tab_hits[i];
                let resp = self.card(ui, hit, selected, i);
                if resp.clicked() {
                    clicked_card = Some(i);
                }
                // 只在导航动作发生后的那一帧滚动，不能每帧都强制回选中项，
                // 否则鼠标滚轮会被立即拉回去。
                // 卡片完整可见时不动；需要移动时一律从顶部开始显示。
                if selected && scroll_to == Some(i) {
                    if let Some(align) = Self::scroll_align(ui.clip_rect(), resp.rect) {
                        ui.scroll_to_rect_animation(
                            resp.rect,
                            Some(align),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                }
                ui.add_space(2.0);
            }
            ui.add_space(6.0);
        });
        if let Some(i) = clicked_card {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.selected = i as i32;
            }
            self.grab_results(ui.ctx());
            self.update_status();
        }
    }

    fn card(
        &mut self,
        ui: &mut egui::Ui,
        hit: &Hit,
        selected: bool,
        index: usize,
    ) -> egui::Response {
        let p = self.palette;
        let fill = if selected { p.selected } else { p.card };
        let frame = Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin {
                left: 16,
                right: 14,
                top: 11,
                bottom: 9,
            });
        let resp = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 词头 + 操作
                ui.horizontal(|ui| {
                    let word_job = rich::job(
                        &[rich::Span {
                            text: hit.word.clone(),
                            bold: true,
                            italic: false,
                            underline: false,
                            color: Some(p.accent),
                        }],
                        21.0,
                        p.accent,
                    );
                    ui.add(Label::new(word_job).selectable(true));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pill(ui, "复制", p).clicked() {
                            let text = if hit.blocks.is_empty() {
                                rich::plain(&hit.pango)
                            } else {
                                hit.blocks
                                    .iter()
                                    .filter_map(|b| match b {
                                        BlockData::Text(x) => Some(rich::plain(x)),
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            };
                            ui.ctx().copy_text(text.trim().to_string());
                        }
                        if pill(ui, "UK", p).on_hover_text("英音").clicked() {
                            speech::speak(&hit.word, "uk");
                        }
                        if pill(ui, "US", p).on_hover_text("美音").clicked() {
                            speech::speak(&hit.word, "us");
                        }
                    });
                });
                ui.add_space(2.0);
                if hit.blocks.is_empty() {
                    let job = rich::job(&rich::parse(hit.pango.trim()), self.gui.font_size, p.text);
                    ui.add(
                        Label::new(job)
                            .selectable(true)
                            .wrap_mode(TextWrapMode::Wrap),
                    );
                } else {
                    for b in &hit.blocks {
                        match b {
                            BlockData::Text(markup) => {
                                let job = rich::job(
                                    &rich::parse(markup.trim()),
                                    self.gui.font_size,
                                    p.text,
                                );
                                ui.add(
                                    Label::new(job)
                                        .selectable(true)
                                        .wrap_mode(TextWrapMode::Wrap),
                                );
                            }
                            BlockData::Image { path } => {
                                if let Some(tex) = self.texture(ui.ctx(), path) {
                                    let size = tex.size_vec2();
                                    let max = Vec2::new(120.0, 120.0);
                                    let scale = (max.x / size.x).min(max.y / size.y).min(1.0);
                                    ui.add(egui::Image::new(&tex).fit_to_exact_size(size * scale));
                                }
                            }
                        }
                    }
                }
                ui.add_space(4.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 16.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.label(
                            RichText::new(display_dict_name(&hit.dict_name))
                                .size(11.5)
                                .color(p.faint),
                        );
                    },
                );
                let _ = index;
            })
            .response;
        let resp = resp.interact(Sense::click());
        if selected {
            let r = resp.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(r.left() + 1.0, r.top() + 12.0),
                    Vec2::new(3.0, (r.height() - 24.0).max(6.0)),
                ),
                CornerRadius::same(2),
                p.accent,
            );
        }
        resp
    }

    fn texture(&mut self, ctx: &egui::Context, path: &PathBuf) -> Option<egui::TextureHandle> {
        if let Some(t) = self.textures.get(path) {
            return Some(t.clone());
        }
        let img = image::open(path).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let (nw, nh) = {
            let scale = (120.0 / w as f32).min(120.0 / h as f32).min(1.0);
            (
                (w as f32 * scale).round() as u32,
                (h as f32 * scale).round() as u32,
            )
        };
        let img = if nw != w || nh != h {
            image::imageops::resize(
                &img,
                nw.max(1),
                nh.max(1),
                image::imageops::FilterType::Triangle,
            )
        } else {
            img
        };
        let (w, h) = img.dimensions();
        let color =
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let tex = ctx.load_texture(path.to_string_lossy(), color, egui::TextureOptions::LINEAR);
        self.textures.insert(path.clone(), tex.clone());
        Some(tex)
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, height: f32) {
        let p = self.palette;
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), height),
            Layout::top_down(Align::Center),
            |ui| {
                ui.add_space(height * 0.3);
                big_icon(ui, p);
                ui.add_space(8.0);
                ui.label(
                    RichText::new("输入一个单词，开始离线查词")
                        .size(14.0)
                        .color(p.muted),
                );
            },
        );
    }

    fn footer(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let sh = self.gui.shortcuts.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.stats).size(11.0).color(p.faint));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let hint = format!(
                    "{}/{} 移动 · {}/{} 翻页 · {} 输入 · {} 帮助",
                    sh.get("next").map(|s| s.as_str()).unwrap_or("j"),
                    sh.get("prev").map(|s| s.as_str()).unwrap_or("k"),
                    sh.get("page_down").map(|s| s.as_str()).unwrap_or("f"),
                    sh.get("page_up").map(|s| s.as_str()).unwrap_or("b"),
                    sh.get("focus").map(|s| s.as_str()).unwrap_or("/"),
                    sh.get("help").map(|s| s.as_str()).unwrap_or("?"),
                );
                ui.label(RichText::new(hint).size(11.0).color(p.faint));
            });
        });
    }

    fn help_screen(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        ui.allocate_ui_with_layout(ui.available_size(), Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() * 0.16).max(20.0));
            Frame::new()
                .fill(p.panel)
                .stroke(Stroke::new(1.0, p.border_hi))
                .corner_radius(CornerRadius::same(16))
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.set_width(440.0);
                    ui.label(
                        RichText::new("快捷键")
                            .size(18.0)
                            .family(fonts::bold())
                            .color(p.text),
                    );
                    ui.add_space(10.0);
                    let sh = &self.gui.shortcuts;
                    for (k, label) in config::ACTIONS {
                        ui.horizontal(|ui| {
                            let key = sh.get(k).map(|s| s.as_str()).unwrap_or("");
                            let key = if key.is_empty() { "（禁用）" } else { key };
                            ui.add_sized(
                                [90.0, 18.0],
                                Label::new(
                                    RichText::new(key).monospace().size(12.0).color(p.accent),
                                ),
                            );
                            ui.label(RichText::new(label).size(12.5).color(p.muted));
                        });
                    }
                    ui.add_space(12.0);
                    ui.label(RichText::new("按 ? 或 Esc 返回").size(11.5).color(p.faint));
                });
        });
    }

    fn open_settings(&mut self) {
        let Some(engine) = &self.engine else { return };
        let core = engine.config.clone();
        let dicts = engine
            .dict_rows()
            .into_iter()
            .map(|(id, info, enabled)| DictRow {
                name: display_dict_name(if info.display_name.is_empty() {
                    &id
                } else {
                    &info.display_name
                }),
                count: info.entry_count,
                enabled,
                id,
            })
            .collect();
        self.settings = Some(SettingsDraft {
            gui: self.gui.clone(),
            core,
            dicts,
            selected: 0,
            new_dir: String::new(),
            tab: 0,
        });
        self.show_settings = true;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        let Some(mut d) = self.settings.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut convert = false;
        let frame = Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.border_hi))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(20));
        // 设置窗口不能固定 720x600：主窗口变窄/变矮时会被裁掉。
        // 宽度跟随窗口，但保留 400pt 下限；高度同理，正文放进滚动区。
        let screen = ctx.content_rect().size();
        // 扣掉外框 margin（左右/上下各 20pt），避免窄窗时窗口贴边被裁。
        let width = 720.0_f32.min((screen.x - 48.0).max(400.0));
        let height = 600.0_f32.min((screen.y - 48.0).max(360.0));
        egui::Window::new("设置")
            .title_bar(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(frame)
            .fade_in(false)
            .fade_out(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_height(height);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("设置")
                            .size(20.0)
                            .family(fonts::bold())
                            .color(p.text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("X").size(14.0).color(p.muted))
                                    .frame(false),
                            )
                            .clicked()
                        {
                            open = false;
                        }
                    });
                });
                ui.add_space(8.0);
                let tabs = ["词典", "目录", "查询", "快捷键", "外观"];
                let mut tab = d.tab;
                ui.horizontal(|ui| {
                    for (i, name) in tabs.iter().enumerate() {
                        if ui
                            .selectable_label(tab == i, RichText::new(*name).size(13.0))
                            .clicked()
                        {
                            tab = i;
                        }
                    }
                });
                d.tab = tab;
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);
                let body_h = (height - 180.0).max(100.0);
                ScrollArea::vertical()
                    .id_salt("settings_body")
                    .max_height(body_h)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match tab {
                            0 => {
                                settings_dicts(ui, &mut d, p, &mut convert);
                            }
                            1 => settings_dirs(ui, &mut d, p, &mut convert),
                            2 => settings_query(ui, &mut d, p),
                            3 => settings_keys(ui, &mut d, p),
                            _ => settings_look(ui, &mut d, p),
                        }
                    });
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if tab == 3 && ui.button("恢复默认快捷键").clicked() {
                        d.gui.shortcuts = Gui::default().shortcuts;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(Button::new(RichText::new("保存").color(p.bg)).fill(p.accent))
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("取消").clicked() {
                            open = false;
                        }
                    });
                });
            });
        if save {
            self.apply_settings(d, ctx);
            self.settings = None;
            self.show_settings = false;
            return;
        }
        if convert {
            self.start_convert();
            self.settings = None;
            self.show_settings = false;
            return;
        }
        self.show_settings = open;
        if open {
            self.settings = Some(d);
        }
    }

    fn apply_settings(&mut self, d: SettingsDraft, ctx: &egui::Context) {
        let mut core = d.core.clone();
        core.dict_order = d.dicts.iter().map(|r| r.id.clone()).collect();
        core.dict_enabled = d
            .dicts
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.id.clone())
            .collect();
        let mut d = d;
        d.core = core;
        self.gui = d.gui;
        self.palette = if self.gui.theme == "light" {
            Palette::light()
        } else {
            Palette::dark()
        };
        if let Some(engine) = &mut self.engine {
            engine.config = d.core.clone();
            engine.reload();
            self.stats = format!(
                "{} 本词典 · {} 词条",
                engine.dict_count(),
                fmt_count(engine.total_entries)
            );
        }
        self.textures.clear();
        self.paths.save_core(&d.core);
        self.paths.save_gui(&self.gui);
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
            self.gui.width,
            self.gui.height,
        )));
        self.status = "设置已保存".into();
        self.tabs.clear();
        self.active = 0;
        if !self.search.trim().is_empty() {
            self.query_now(ctx);
        } else {
            self.update_status();
        }
    }
}

fn settings_dicts(
    ui: &mut egui::Ui,
    d: &mut SettingsDraft,
    p: Palette,
    convert: &mut bool,
) -> Option<egui::Rect> {
    let mut first_checkbox = None;
    ui.label(
        RichText::new("勾选启用；选中后可用 ↑↓ 调整顺序，保存后生效。")
            .size(11.5)
            .color(p.faint),
    );
    ui.add_space(4.0);
    // 注意：不要在整个 row 上再叠一层 interact(Sense::click())，那会盖住 checkbox，
    // 导致复选框点不动。这里让 checkbox 自己处理点击，名称文字负责选中行。
    let selected = d.selected;
    let mut clicked_row = None;
    for (i, row) in d.dicts.iter_mut().enumerate() {
        let bg = if selected == i {
            p.selected
        } else {
            Color32::TRANSPARENT
        };
        Frame::new()
            .fill(bg)
            .corner_radius(CornerRadius::same(7))
            .inner_margin(Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let cb = ui.checkbox(&mut row.enabled, "");
                    if first_checkbox.is_none() {
                        first_checkbox = Some(cb.rect);
                    }
                    let name = ui.add(
                        Label::new(RichText::new(&row.name).size(13.0).color(p.text))
                            .sense(Sense::click()),
                    );
                    if cb.clicked() || name.clicked() {
                        clicked_row = Some(i);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(fmt_count(row.count))
                                .size(10.5)
                                .color(p.faint),
                        );
                    });
                });
            });
    }
    if let Some(i) = clicked_row {
        d.selected = i;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("↑ 上移").clicked() && d.selected > 0 {
            d.dicts.swap(d.selected, d.selected - 1);
            d.selected -= 1;
        }
        if ui.button("↓ 下移").clicked() && d.selected + 1 < d.dicts.len() {
            d.dicts.swap(d.selected, d.selected + 1);
            d.selected += 1;
        }
        if ui.button("全选").clicked() {
            for r in &mut d.dicts {
                r.enabled = true;
            }
        }
        if ui.button("全不选").clicked() {
            for r in &mut d.dicts {
                r.enabled = false;
            }
        }
        if ui.button("扫描并转换…").clicked() {
            *convert = true;
        }
    });
    first_checkbox
}

fn settings_dirs(ui: &mut egui::Ui, d: &mut SettingsDraft, p: Palette, convert: &mut bool) {
    ui.label(
        RichText::new("把 .ld2 放进这些目录，然后点“扫描并转换”。")
            .size(11.5)
            .color(p.faint),
    );
    ui.add_space(4.0);
    Frame::new()
        .fill(p.card)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if d.core.source_dirs.is_empty() {
                ui.label(RichText::new("（没有源目录）").size(12.0).color(p.faint));
            }
            let dirs = d.core.source_dirs.clone();
            for (i, dir) in dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(dir.to_string_lossy())
                            .size(12.0)
                            .color(p.muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("移除").clicked() {
                            d.core.source_dirs.remove(i);
                        }
                    });
                });
            }
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            TextEdit::singleline(&mut d.new_dir)
                .hint_text("选择词典所在目录")
                .desired_width(460.0),
        );
        if ui.button("添加").clicked() {
            let path = PathBuf::from(d.new_dir.trim());
            if !d.new_dir.trim().is_empty() && !d.core.source_dirs.contains(&path) {
                d.core.source_dirs.push(path);
                d.new_dir.clear();
            }
        }
    });
    ui.add_space(8.0);
    if ui.button("扫描并转换").clicked() {
        *convert = true;
    }
}

fn settings_query(ui: &mut egui::Ui, d: &mut SettingsDraft, p: Palette) {
    ui.checkbox(&mut d.core.case_insensitive, "大小写不敏感");
    ui.checkbox(&mut d.core.prefix_fallback, "无精确匹配时前缀回退");
    ui.add_space(8.0);
    egui::Grid::new("query_opts")
        .num_columns(2)
        .spacing([24.0, 8.0])
        .show(ui, |ui| {
            ui.label(RichText::new("每词典前缀上限").size(12.0).color(p.muted));
            ui.add(egui::DragValue::new(&mut d.core.prefix_match_limit).range(1..=50));
            ui.end_row();
            ui.label(RichText::new("输入防抖（毫秒）").size(12.0).color(p.muted));
            ui.add(
                egui::DragValue::new(&mut d.gui.debounce_ms)
                    .range(0..=2000)
                    .speed(10),
            );
            ui.end_row();
            ui.label(RichText::new("每批显示条数").size(12.0).color(p.muted));
            ui.add(egui::DragValue::new(&mut d.gui.page_size).range(3..=50));
            ui.end_row();
        });
}

fn settings_keys(ui: &mut egui::Ui, d: &mut SettingsDraft, p: Palette) {
    ui.label(
        RichText::new("格式：j / Shift+G / Tab / Escape；留空表示禁用。")
            .size(11.5)
            .color(p.faint),
    );
    ui.add_space(6.0);
    egui::Grid::new("shortcuts_grid")
        .num_columns(4)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            for (n, (key, label)) in config::ACTIONS.iter().enumerate() {
                ui.add_sized(
                    [100.0, 20.0],
                    Label::new(RichText::new(*label).size(12.0).color(p.muted)),
                );
                let mut val = d.gui.shortcut(key).to_string();
                if ui
                    .add_sized(
                        [136.0, 26.0],
                        TextEdit::singleline(&mut val).margin(Margin::symmetric(6, 3)),
                    )
                    .changed()
                {
                    d.gui.set_shortcut(key, val);
                }
                if n % 2 == 1 {
                    ui.end_row();
                }
            }
        });
}

fn settings_look(ui: &mut egui::Ui, d: &mut SettingsDraft, p: Palette) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("主题").size(12.0).color(p.muted));
        ui.radio_value(&mut d.gui.theme, "dark".to_string(), "深色");
        ui.radio_value(&mut d.gui.theme, "light".to_string(), "浅色");
    });
    ui.add_space(8.0);
    egui::Grid::new("look_grid")
        .num_columns(2)
        .spacing([24.0, 8.0])
        .show(ui, |ui| {
            ui.label(RichText::new("正文字号").size(12.0).color(p.muted));
            ui.add(
                egui::DragValue::new(&mut d.gui.font_size)
                    .range(11.0..=28.0)
                    .speed(0.2),
            );
            ui.end_row();
            ui.label(RichText::new("窗口宽度").size(12.0).color(p.muted));
            ui.add(
                egui::DragValue::new(&mut d.gui.width)
                    .range(640.0..=2000.0)
                    .speed(2.0),
            );
            ui.end_row();
            ui.label(RichText::new("窗口高度").size(12.0).color(p.muted));
            ui.add(
                egui::DragValue::new(&mut d.gui.height)
                    .range(480.0..=1600.0)
                    .speed(2.0),
            );
            ui.end_row();
        });
}

fn toggle_maximized(ctx: &egui::Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

#[derive(Clone, Copy)]
enum WinButton {
    Minimize,
    Maximize,
    Restore,
    Close,
}

fn window_controls(ui: &mut egui::Ui, p: Palette) {
    ui.spacing_mut().item_spacing.x = 2.0;
    let maximized = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
    if window_button(ui, WinButton::Close, p).clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    let max_kind = if maximized {
        WinButton::Restore
    } else {
        WinButton::Maximize
    };
    if window_button(ui, max_kind, p).clicked() {
        toggle_maximized(ui.ctx());
    }
    if window_button(ui, WinButton::Minimize, p).clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn window_button(ui: &mut egui::Ui, kind: WinButton, p: Palette) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 28.0), Sense::click());
    let hovered = resp.hovered();
    let close = matches!(kind, WinButton::Close);
    if hovered {
        let bg = if close {
            Color32::from_rgb(232, 17, 35)
        } else {
            p.card_hover
        };
        ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let c = rect.center();
    let color = if close && hovered {
        Color32::WHITE
    } else if hovered {
        p.accent
    } else {
        p.muted
    };
    let stroke = Stroke::new(1.2, color);
    match kind {
        WinButton::Minimize => {
            ui.painter()
                .line_segment([c - Vec2::new(5.0, 0.0), c + Vec2::new(5.0, 0.0)], stroke);
        }
        WinButton::Maximize => {
            let r = egui::Rect::from_center_size(c, Vec2::splat(10.0));
            ui.painter()
                .rect_stroke(r, CornerRadius::same(1), stroke, egui::StrokeKind::Inside);
        }
        WinButton::Restore => {
            let back = egui::Rect::from_center_size(c + Vec2::new(-1.5, -1.5), Vec2::splat(8.5));
            let front = egui::Rect::from_center_size(c + Vec2::new(1.5, 1.5), Vec2::splat(8.5));
            ui.painter().rect_stroke(
                back,
                CornerRadius::same(1),
                Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
            let cover = if hovered { p.card_hover } else { p.bg };
            ui.painter()
                .rect_filled(front, CornerRadius::same(1), cover);
            ui.painter().rect_stroke(
                front,
                CornerRadius::same(1),
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        WinButton::Close => {
            ui.painter()
                .line_segment([c - Vec2::splat(5.0), c + Vec2::splat(5.0)], stroke);
            ui.painter()
                .line_segment([c + Vec2::new(-5.0, 5.0), c + Vec2::new(5.0, -5.0)], stroke);
        }
    }
    resp
}

#[derive(Clone, Copy)]
enum Icon {
    Sun,
    Moon,
    Settings,
    Refresh,
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, p: Palette) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let hovered = resp.hovered();
    let bg = if hovered { p.card_hover } else { p.bg };
    if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(7), bg);
    }
    let c = rect.center();
    let stroke = Stroke::new(1.5, if hovered { p.accent } else { p.muted });
    match icon {
        Icon::Sun => {
            ui.painter().circle_stroke(c, 4.6, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::new(a.cos(), a.sin());
                ui.painter()
                    .line_segment([c + d * 6.8, c + d * 9.0], stroke);
            }
        }
        Icon::Moon => {
            let color = if hovered { p.accent } else { p.muted };
            ui.painter()
                .circle_filled(c - Vec2::new(1.0, 1.0), 6.2, color);
            ui.painter()
                .circle_filled(c + Vec2::new(2.8, -1.2), 6.0, bg);
        }
        Icon::Settings => {
            // 齿轮：外圈 + 内孔 + 8 个短齿
            ui.painter().circle_stroke(c, 5.0, stroke);
            ui.painter().circle_stroke(c, 1.8, stroke);
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.0;
                let d = Vec2::new(a.cos(), a.sin());
                ui.painter()
                    .line_segment([c + d * 6.3, c + d * 8.6], Stroke::new(2.2, stroke.color));
            }
        }
        Icon::Refresh => {
            let mut pts = Vec::new();
            let (start, end) = (-0.35 * std::f32::consts::PI, 1.35 * std::f32::consts::PI);
            let n = 28;
            for i in 0..=n {
                let a = start + (end - start) * i as f32 / n as f32;
                pts.push(c + Vec2::new(a.cos(), a.sin()) * 6.2);
            }
            ui.painter().add(egui::Shape::line(pts, stroke));
            // 箭头
            let a = start;
            let tip = c + Vec2::new(a.cos(), a.sin()) * 6.2;
            let t1 = tip + Vec2::new(3.4, 1.4);
            let t2 = tip + Vec2::new(0.8, 4.0);
            ui.painter().line_segment([tip, t1], stroke);
            ui.painter().line_segment([tip, t2], stroke);
        }
    }
    resp
}

fn section_title(text: &str, p: Palette) -> RichText {
    RichText::new(text)
        .size(13.0)
        .color(p.accent)
        .family(fonts::bold())
}

fn pill(ui: &mut egui::Ui, text: &str, p: Palette) -> egui::Response {
    ui.add(
        Button::new(RichText::new(text).size(10.5).color(p.muted))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(0.0, 22.0)),
    )
}

fn logo(ui: &mut egui::Ui) {
    // 与桌面图标共用同一张图：打开的书 + 书签，而不是“方框里一个字”。
    let ctx = ui.ctx().clone();
    let texture = ctx
        .data(|d| d.get_temp::<egui::TextureHandle>(Id::new("app-logo")))
        .unwrap_or_else(|| {
            let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
                .expect("embedded icon")
                .to_rgba8();
            let (w, h) = img.dimensions();
            let color =
                egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
            let t = ctx.load_texture("app-logo", color, egui::TextureOptions::LINEAR);
            ctx.data_mut(|d| d.insert_temp(Id::new("app-logo"), t.clone()));
            t
        });
    ui.add(egui::Image::new(&texture).fit_to_exact_size(Vec2::splat(28.0)));
}

fn search_icon(ui: &mut egui::Ui, color: Color32) {
    // 与右侧 30pt 高的输入框同高，保证搜索图标与文字在垂直方向严格居中。
    let (rect, _) = ui.allocate_exact_size(Vec2::new(18.0, 30.0), Sense::hover());
    let c = rect.center();
    ui.painter().circle_stroke(c, 5.5, Stroke::new(1.4, color));
    ui.painter().line_segment(
        [c + Vec2::new(4.0, 4.0), c + Vec2::new(8.0, 8.0)],
        Stroke::new(1.4, color),
    );
}

fn big_icon(ui: &mut egui::Ui, p: Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(54.0, 54.0), Sense::hover());
    let c = rect.center();
    ui.painter()
        .circle_stroke(c, 18.0, Stroke::new(2.0, p.accent_dim));
    ui.painter().line_segment(
        [c + Vec2::new(12.0, 12.0), c + Vec2::new(22.0, 22.0)],
        Stroke::new(2.0, p.accent_dim),
    );
}

fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn parse_words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in
        text.split(|c: char| c.is_whitespace() || matches!(c, ',' | '，' | '、' | ';' | '；'))
    {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        if !out.iter().any(|w| w == p) {
            out.push(p.to_string());
        }
    }
    out
}

fn parse_shortcut(s: &str) -> Option<(egui::Modifiers, egui::Key)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut mods = egui::Modifiers::NONE;
    let mut rest = s.to_string();
    loop {
        let lower = rest.to_ascii_lowercase();
        let mut found = None;
        for (name, m) in [
            ("ctrl+", egui::Modifiers::CTRL),
            ("control+", egui::Modifiers::CTRL),
            ("alt+", egui::Modifiers::ALT),
            ("shift+", egui::Modifiers::SHIFT),
        ] {
            if lower.starts_with(name) {
                mods |= m;
                found = Some(rest[name.len()..].to_string());
                break;
            }
        }
        match found {
            Some(r) => rest = r,
            None => break,
        }
    }
    let key = match rest.to_ascii_lowercase().as_str() {
        "tab" => egui::Key::Tab,
        "escape" | "esc" => egui::Key::Escape,
        "enter" | "return" => egui::Key::Enter,
        "space" => egui::Key::Space,
        "backspace" => egui::Key::Backspace,
        "/" => egui::Key::Slash,
        "?" => {
            mods |= egui::Modifiers::SHIFT;
            egui::Key::Questionmark
        }
        ";" => egui::Key::Semicolon,
        "'" => egui::Key::Quote,
        "," => egui::Key::Comma,
        "." => egui::Key::Period,
        "-" => egui::Key::Minus,
        "=" => egui::Key::Equals,
        "[" => egui::Key::OpenBracket,
        "]" => egui::Key::CloseBracket,
        "\\" => egui::Key::Backslash,
        "`" => egui::Key::Backtick,
        k if k.len() == 1 => {
            let c = k.chars().next().unwrap().to_ascii_uppercase();
            match c {
                'A'..='Z' => key_letter(c),
                '0'..='9' => key_digit(c),
                _ => return None,
            }
        }
        k if k.starts_with('f') && k[1..].parse::<u8>().is_ok() => {
            let n: u8 = k[1..].parse().ok()?;
            key_f(n)?
        }
        _ => return None,
    };
    Some((mods, key))
}

fn key_letter(c: char) -> egui::Key {
    use egui::Key::*;
    match c {
        'A' => A,
        'B' => B,
        'C' => C,
        'D' => D,
        'E' => E,
        'F' => F,
        'G' => G,
        'H' => H,
        'I' => I,
        'J' => J,
        'K' => K,
        'L' => L,
        'M' => M,
        'N' => N,
        'O' => O,
        'P' => P,
        'Q' => Q,
        'R' => R,
        'S' => S,
        'T' => T,
        'U' => U,
        'V' => V,
        'W' => W,
        'X' => X,
        'Y' => Y,
        'Z' => Z,
        _ => Z,
    }
}

fn key_digit(c: char) -> egui::Key {
    use egui::Key::*;
    match c {
        '0' => Num0,
        '1' => Num1,
        '2' => Num2,
        '3' => Num3,
        '4' => Num4,
        '5' => Num5,
        '6' => Num6,
        '7' => Num7,
        '8' => Num8,
        '9' => Num9,
        _ => Num0,
    }
}

fn key_f(n: u8) -> Option<egui::Key> {
    use egui::Key::*;
    Some(match n {
        1 => F1,
        2 => F2,
        3 => F3,
        4 => F4,
        5 => F5,
        6 => F6,
        7 => F7,
        8 => F8,
        9 => F9,
        10 => F10,
        11 => F11,
        12 => F12,
        _ => return None,
    })
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.paths.save_gui(&self.gui);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // 截图模式下窗口静止时 eframe 会停止重绘，必须主动请求帧才能走到第 20 帧。
        if self.shot.is_some() {
            ctx.request_repaint();
        }
        self.frame_no += 1;
        if self.frame_no == 3 {
            if let Ok(k) = std::env::var("LEXICON_TEST_KEY") {
                if let Some(ch) = k.chars().next() {
                    let key = match ch.to_ascii_uppercase() {
                        'J' => Some(egui::Key::J),
                        'K' => Some(egui::Key::K),
                        'F' => Some(egui::Key::F),
                        'B' => Some(egui::Key::B),
                        'G' => Some(egui::Key::G),
                        'T' => Some(egui::Key::Tab),
                        'Y' => Some(egui::Key::Y),
                        '/' => Some(egui::Key::Slash),
                        _ => None,
                    };
                    if let Some(key) = key {
                        ctx.input_mut(|i| {
                            i.events.push(egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: if ch.is_uppercase() {
                                    egui::Modifiers::SHIFT
                                } else {
                                    egui::Modifiers::NONE
                                },
                            });
                        });
                    }
                    if ch == '/' {
                        ctx.input_mut(|i| i.events.push(egui::Event::Text("/".into())));
                    }
                }
            }
        }
        if self.frame_no == 5 && std::env::var_os("LEXICON_TEST_KEY").is_some() {
            eprintln!("SEARCH_AFTER_KEY={:?}", self.search);
            eprintln!(
                "ACTIVE_AFTER_KEY={} tabs={}",
                self.active,
                self.tabs
                    .get(self.active)
                    .map(|t| t.word.as_str())
                    .unwrap_or("")
            );
        }
        if self.frame_no == 8 {
            if let Ok(q) = std::env::var("LEXICON_TEST_QUERY2") {
                self.search = q;
                self.query_now(&ctx);
            }
        }
        if self.frame_no == 9 && std::env::var_os("LEXICON_TEST_SCROLL").is_some() {
            let pos = ctx.content_rect().center();
            ctx.input_mut(|i| i.events.push(egui::Event::PointerMoved(pos)));
        }
        if self.frame_no == 10 {
            if let Ok(v) = std::env::var("LEXICON_TEST_SCROLL") {
                if let Ok(dy) = v.parse::<f32>() {
                    ctx.input_mut(|i| {
                        i.smooth_scroll_delta = egui::vec2(0.0, dy);
                    });
                }
            }
        }
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            if rect.width() > 400.0 && rect.height() > 300.0 {
                self.gui.width = rect.width();
                self.gui.height = rect.height();
            }
        }
        if self.focus_search_next {
            ctx.memory_mut(|m| m.request_focus(self.search_id));
            self.nav = false;
            self.focus_search_next = false;
        }
        self.apply_style(&ctx);
        self.poll_convert(&ctx);
        self.tick(&ctx);
        self.handle_keys(&ctx);

        // 自动测试/截图
        let (mut auto_query, mut shot_now, mut settings_shot) = (false, false, false);
        if let Some(s) = &mut self.shot {
            s.frames += 1;
            if s.frames == 2 {
                auto_query = true;
            }
            if s.frames == 20 {
                shot_now = true;
            }
        }
        if std::env::var_os("LEXICON_SHOT_HELP").is_some() {
            self.help = true;
        }
        if std::env::var_os("LEXICON_SHOT_SETTINGS").is_some() {
            if self.settings.is_none() {
                self.open_settings();
            }
            if let Ok(tab) = std::env::var("LEXICON_SHOT_TAB") {
                if let Some(d) = &mut self.settings {
                    d.tab = tab.parse::<usize>().unwrap_or(0);
                }
            }
            settings_shot = true;
        }
        let _ = auto_query;
        self.draw(ui);
        if std::env::var_os("LEXICON_DEBUG_FOCUS").is_some() {
            if let Some(s) = &self.shot {
                if s.frames == 4 {
                    let focused = ctx.memory(|m| m.focused());
                    let search = ctx.memory(|m| m.has_focus(self.search_id));
                    let results = ctx.memory(|m| m.has_focus(self.results_focus_id()));
                    eprintln!("FOCUS focused={focused:?} search={search} results={results}");
                }
            }
        }

        if shot_now || settings_shot {
            if let Some(shot) = self.shot.take() {
                let path = shot.path.clone();
                let ctx2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let w = img.width() as u32;
                    let h = img.height() as u32;
                    let mut bytes = Vec::with_capacity((w * h * 4) as usize);
                    for p in img.pixels.iter() {
                        bytes.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
                    }
                    let _ = image::save_buffer(&path, &bytes, w, h, image::ColorType::Rgba8);
                    println!("screenshot {path} {w}x{h}");
                    ctx2.send_viewport_cmd(ViewportCommand::Close);
                });
            }
        }
    }
}

fn install_panic_log(path: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let text = format!("{info}\n");
        let _ = std::fs::write(&path, &text);
        eprintln!("{text}");
    }));
}

fn main() -> eframe::Result {
    let paths = Paths::discover();
    // 崩溃时至少留下可诊断的一行；Windows 的 GUI 子系统没有控制台。
    install_panic_log(paths.core_config.with_file_name("crash.log"));
    let gui = paths.load_gui();
    let shot = std::env::var("LEXICON_SHOT")
        .ok()
        .map(|path| Shot { path, frames: 0 });
    let test_query = std::env::var("LEXICON_TEST_QUERY").ok();
    let size = [gui.width, gui.height];
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()
        .map(|img| {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            }
        });
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(size)
        .with_min_inner_size([640.0, 480.0])
        .with_app_id("io.github.lexicon.dict")
        .with_title("词典")
        // 自绘标题栏：不要系统的最小化/最大化/关闭按钮。
        .with_decorations(false);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }
    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "词典",
        opts,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                paths.clone(),
                gui.clone(),
                shot,
                test_query,
            )))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_split() {
        assert_eq!(parse_words("hello world"), vec!["hello", "world"]);
        assert_eq!(parse_words("  foo，bar、baz "), vec!["foo", "bar", "baz"]);
        assert_eq!(parse_words("foo foo"), vec!["foo"]);
    }

    #[test]
    fn shortcuts_parse() {
        assert_eq!(
            parse_shortcut("j"),
            Some((egui::Modifiers::NONE, egui::Key::J))
        );
        assert_eq!(
            parse_shortcut("Shift+G"),
            Some((egui::Modifiers::SHIFT, egui::Key::G))
        );
        assert_eq!(
            parse_shortcut("Tab"),
            Some((egui::Modifiers::NONE, egui::Key::Tab))
        );
        assert_eq!(
            parse_shortcut("?"),
            Some((egui::Modifiers::SHIFT, egui::Key::Questionmark))
        );
        assert!(parse_shortcut("").is_none());
    }

    #[test]
    fn counts() {
        assert_eq!(fmt_count(1), "1");
        assert_eq!(fmt_count(19050), "19,050");
        assert_eq!(fmt_count(5018570), "5,018,570");
    }

    #[test]
    fn scroll_alignment() {
        let view = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        // 已完整可见：不移动。
        let inside = egui::Rect::from_min_size(egui::pos2(1.0, 1.0), egui::vec2(50.0, 50.0));
        assert_eq!(App::scroll_align(view, inside), None);
        // 高卡片被裁切：从顶部开始显示，不能居中后从中间阅读。
        let tall = egui::Rect::from_min_size(egui::pos2(0.0, 50.0), egui::vec2(100.0, 200.0));
        assert_eq!(App::scroll_align(view, tall), Some(Align::TOP));
        // 未完整可见时（包括短卡片在下方）都从顶部开始显示。
        let below = egui::Rect::from_min_size(egui::pos2(0.0, 110.0), egui::vec2(100.0, 30.0));
        assert_eq!(App::scroll_align(view, below), Some(Align::TOP));
        let above = egui::Rect::from_min_size(egui::pos2(0.0, -20.0), egui::vec2(100.0, 30.0));
        assert_eq!(App::scroll_align(view, above), Some(Align::TOP));
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn checkbox_click_toggles_dict() {
        let ctx = egui::Context::default();
        let mut d = SettingsDraft {
            gui: Gui::default(),
            core: lexicon_core::common::Config::default(),
            dicts: vec![
                DictRow {
                    id: "a".into(),
                    name: "A 词典".into(),
                    count: 1,
                    enabled: true,
                },
                DictRow {
                    id: "b".into(),
                    name: "B 词典".into(),
                    count: 1,
                    enabled: true,
                },
            ],
            selected: 0,
            new_dir: String::new(),
            tab: 0,
        };
        let p = Palette::dark();
        let mut convert = false;
        let mut run = |events: Vec<egui::Event>| {
            let mut input = egui::RawInput::default();
            input.screen_rect = Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(700.0, 500.0),
            ));
            input.events = events;
            let mut first = None;
            let mut out = ctx.run_ui(input, |ui| {
                first = settings_dicts(ui, &mut d, p, &mut convert);
            });
            out.textures_delta.clear();
            first
        };
        let cb = run(vec![]).expect("dict checkbox");
        let pos = cb.center();
        run(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        run(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!d.dicts[0].enabled, "checkbox click should toggle enabled");
        assert!(d.dicts[1].enabled);
    }
}
