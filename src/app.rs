// vimd 的应用状态与输入处理。
//
// 模式：
//   Normal   —— 渲染阅读态，j/k 滚动，i 进入编辑
//   Insert   —— 源码编辑态，显示行号，Esc 回阅读态
//   Command  —— : 命令或 / 搜索
//   Settings —— 设置页
//   Help     —— 快捷键帮助
//
// 这里的“Vim”是行为上的轻量模仿：不实现完整寄存器/宏/文本对象，
// 但保留阅读和编辑 Markdown 最常用的动作与 :wq。

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Color;
use unicode_width::UnicodeWidthChar;

use crate::editor::Editor;
use crate::kitty;
use crate::markdown::{self, DisplayLine, RenderOptions};
use crate::theme::{ColorName, Config, Role, Theme, preset_by_name, preset_next};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Settings,
    Help,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Command,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKind {
    Preset,
    Color(Role),
    HeadingRules,
    TableBorders,
    TextSizing,
    HeadingScale(u8),
    LineNumbers,
    FocusLine,
    Section,
}

#[derive(Clone, Debug)]
pub struct SettingItem {
    pub label: String,
    pub value: String,
    pub kind: SettingKind,
    pub color: Option<Color>,
    pub hint: String,
}

pub struct App {
    pub editor: Editor,
    pub config: Config,
    pub theme: Theme,
    pub display: Vec<DisplayLine>,
    pub mode: Mode,
    pub normal_cursor: usize,
    pub scroll: usize,
    pub raw_scroll: usize,
    pub hscroll: usize,
    pub command: String,
    pub command_kind: CommandKind,
    pub status: String,
    pub settings_index: usize,
    pub settings_scroll: usize,
    pub settings_dirty: bool,
    pub help_scroll: usize,
    pub should_quit: bool,
    pub pending: Option<char>,
    pub last_search: Option<String>,
    pub content_width: usize,
    pub content_height: usize,
    pub dirty_render: bool,
    pub text_sizing_supported: bool,
    pub text_sizing_reason: String,
    pub text_sizing_enabled: bool,
    pub color_enabled: bool,
    pub content_area: Rect,
    pub last_scaled: Vec<Rect>,
    pub scaled_dirty: bool,
}

impl App {
    pub fn new(path: Option<PathBuf>, color_enabled: bool) -> Self {
        let config = Config::load();
        let theme = config.theme();
        let support = kitty::detect_text_sizing();
        let text_sizing_enabled = support.supported && config.text_sizing;
        let mut status = String::new();
        let editor = match path {
            Some(p) => match Editor::open(p.clone()) {
                Ok(e) => e,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    let mut e = Editor::empty();
                    e.set_path(p);
                    status = "新文件".to_string();
                    e
                }
                Err(err) => {
                    status = format!("打开失败: {err}");
                    Editor::empty()
                }
            },
            None => Editor::empty(),
        };
        let render_options = if text_sizing_enabled {
            RenderOptions {
                h1_scale: config.h1_scale.clamp(1, 4),
                h2_scale: config.h2_scale.clamp(1, 4),
                h3_scale: config.h3_scale.clamp(1, 4),
            }
        } else {
            RenderOptions::default()
        };
        let display = markdown::render(&editor.lines, 100, &theme, render_options);
        Self {
            editor,
            config,
            theme,
            display,
            mode: Mode::Normal,
            normal_cursor: 0,
            scroll: 0,
            raw_scroll: 0,
            hscroll: 0,
            command: String::new(),
            command_kind: CommandKind::Command,
            status,
            settings_index: 0,
            settings_scroll: 0,
            settings_dirty: false,
            help_scroll: 0,
            should_quit: false,
            pending: None,
            last_search: None,
            content_width: 100,
            content_height: 24,
            dirty_render: false,
            text_sizing_supported: support.supported,
            text_sizing_reason: support.reason,
            text_sizing_enabled,
            color_enabled,
            content_area: Rect::new(0, 0, 0, 0),
            last_scaled: Vec::new(),
            scaled_dirty: true,
        }
    }

    fn render_options(&self) -> RenderOptions {
        if self.text_sizing_enabled {
            RenderOptions {
                h1_scale: self.config.h1_scale.clamp(1, 4),
                h2_scale: self.config.h2_scale.clamp(1, 4),
                h3_scale: self.config.h3_scale.clamp(1, 4),
            }
        } else {
            RenderOptions::default()
        }
    }

    pub fn set_viewport(&mut self, width: usize, height: usize) {
        let width = width.max(16);
        let height = height.max(4);
        if width != self.content_width {
            self.content_width = width;
            self.dirty_render = true;
            self.scaled_dirty = true;
        }
        if height != self.content_height {
            self.content_height = height;
            self.scaled_dirty = true;
        }
        self.ensure_normal_visible();
        self.ensure_insert_visible();
    }

    pub fn rebuild(&mut self) {
        self.display = markdown::render(
            &self.editor.lines,
            self.content_width,
            &self.theme,
            self.render_options(),
        );
        self.dirty_render = false;
        if self.display.is_empty() {
            self.normal_cursor = 0;
            return;
        }
        // 让阅读态光标跟随“当前源码行”，而不是把源码光标拉回旧位置。
        // 搜索跳转、:goto、编辑后 Esc 都依赖这个顺序。
        let target = self.editor.row;
        self.normal_cursor = self
            .display
            .iter()
            .position(|l| l.source_line >= target)
            .unwrap_or(self.display.len() - 1);
        self.ensure_normal_visible();
        self.sync_editor_from_normal();
    }

    pub fn ensure_render(&mut self) {
        if self.dirty_render || self.display.is_empty() {
            self.rebuild();
        }
    }

    fn sync_editor_from_normal(&mut self) {
        if let Some(line) = self.display.get(self.normal_cursor) {
            self.editor.row = line.source_line.min(self.editor.max_line());
            self.editor.col = line.source_col.min(self.editor.line_len(self.editor.row));
        }
    }

    pub fn sync_normal_from_source(&mut self) {
        if self.display.is_empty() {
            self.normal_cursor = 0;
            return;
        }
        if let Some(idx) = self
            .display
            .iter()
            .position(|l| l.source_line >= self.editor.row)
        {
            self.normal_cursor = idx;
        } else {
            self.normal_cursor = self.display.len() - 1;
        }
        self.ensure_normal_visible();
    }

    pub fn ensure_normal_visible(&mut self) {
        if self.display.is_empty() {
            self.normal_cursor = 0;
            self.scroll = 0;
            return;
        }
        self.normal_cursor = self.normal_cursor.min(self.display.len() - 1);
        let old_scroll = self.scroll;
        if self.normal_cursor < self.scroll {
            self.scroll = self.normal_cursor;
        } else if self.normal_cursor >= self.scroll + self.content_height {
            self.scroll = self.normal_cursor + 1 - self.content_height;
        }
        let max_scroll = self.display.len().saturating_sub(self.content_height);
        self.scroll = self.scroll.min(max_scroll);
        if self.scroll != old_scroll {
            self.scaled_dirty = true;
        }
    }

    pub fn ensure_insert_visible(&mut self) {
        if self.editor.row < self.raw_scroll {
            self.raw_scroll = self.editor.row;
        } else if self.editor.row >= self.raw_scroll + self.content_height {
            self.raw_scroll = self.editor.row + 1 - self.content_height;
        }
        let max_scroll = self.editor.lines.len().saturating_sub(self.content_height);
        self.raw_scroll = self.raw_scroll.min(max_scroll);

        let digits = self.editor.lines.len().to_string().len();
        let gutter = if self.theme.line_numbers {
            digits + 3
        } else {
            0
        };
        let visible = self.content_width.saturating_sub(gutter).max(8);
        let cx = self.cursor_display_col();
        if cx < self.hscroll {
            self.hscroll = cx;
        } else if cx >= self.hscroll + visible {
            self.hscroll = cx + 1 - visible;
        }
    }

    fn cursor_display_col(&self) -> usize {
        let mut col = 0usize;
        for c in self
            .editor
            .line(self.editor.row)
            .chars()
            .take(self.editor.col)
        {
            if c == '\t' {
                col += 4 - (col % 4);
            } else {
                col += c.width().unwrap_or(0);
            }
        }
        col
    }

    pub fn cursor_screen_col(&self) -> u16 {
        self.cursor_display_col()
            .saturating_sub(self.hscroll)
            .min(u16::MAX as usize) as u16
    }

    pub fn percent(&self) -> usize {
        if self.editor.lines.is_empty() {
            0
        } else {
            (self.editor.row + 1) * 100 / self.editor.lines.len()
        }
    }

    pub fn position_label(&self) -> String {
        format!("{}:{}", self.editor.row + 1, self.editor.col + 1)
    }

    // ------------------------------------------------------------------
    // 输入
    // ------------------------------------------------------------------

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return;
        }
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Insert => self.handle_insert(key),
            Mode::Command => self.handle_command(key),
            Mode::Settings => self.handle_settings(key),
            Mode::Help => self.handle_help(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    self.save_current();
                    return;
                }
                KeyCode::Char('d') => {
                    self.move_normal(self.content_height as i32 / 2);
                    return;
                }
                KeyCode::Char('u') => {
                    self.move_normal(-(self.content_height as i32 / 2));
                    return;
                }
                KeyCode::Char('r') => {
                    if self.editor.redo() {
                        self.after_edit();
                        self.status = "重做".into();
                    }
                    return;
                }
                _ => {}
            }
        }

        if let Some(p) = self.pending.take() {
            match (p, key.code) {
                ('d', KeyCode::Char('d')) => {
                    self.sync_editor_from_normal();
                    self.editor.delete_line();
                    self.after_edit();
                    self.status = "已删除行".into();
                    return;
                }
                ('y', KeyCode::Char('y')) => {
                    self.sync_editor_from_normal();
                    self.editor.yank_line();
                    self.status = "已复制行".into();
                    return;
                }
                ('g', KeyCode::Char('g')) => {
                    self.normal_cursor = 0;
                    self.sync_editor_from_normal();
                    self.ensure_normal_visible();
                    return;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_normal(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_normal(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.move_normal(self.content_height as i32),
            KeyCode::PageUp => self.move_normal(-(self.content_height as i32)),
            KeyCode::Home => {
                self.normal_cursor = 0;
                self.sync_editor_from_normal();
                self.ensure_normal_visible();
            }
            KeyCode::End => {
                self.normal_cursor = self.display.len().saturating_sub(1);
                self.sync_editor_from_normal();
                self.ensure_normal_visible();
            }
            KeyCode::Char('G') => {
                self.normal_cursor = self.display.len().saturating_sub(1);
                self.sync_editor_from_normal();
                self.ensure_normal_visible();
            }
            KeyCode::Char('g') => {
                self.pending = Some('g');
                self.status = "g…".into();
            }
            KeyCode::Char('d') => {
                self.pending = Some('d');
                self.status = "d…".into();
            }
            KeyCode::Char('y') => {
                self.pending = Some('y');
                self.status = "y…".into();
            }
            KeyCode::Char('i') => self.enter_insert(InsertKind::AtCursor),
            KeyCode::Char('a') => self.enter_insert(InsertKind::Append),
            KeyCode::Char('I') => self.enter_insert(InsertKind::FirstNonBlank),
            KeyCode::Char('A') => self.enter_insert(InsertKind::End),
            KeyCode::Char('o') => self.enter_insert(InsertKind::OpenBelow),
            KeyCode::Char('O') => self.enter_insert(InsertKind::OpenAbove),
            KeyCode::Char('x') => {
                self.sync_editor_from_normal();
                self.editor.delete_forward();
                self.after_edit();
            }
            KeyCode::Char('p') => {
                self.sync_editor_from_normal();
                self.editor.paste_line();
                self.after_edit();
                self.status = "已粘贴".into();
            }
            KeyCode::Char('u') => {
                if self.editor.undo() {
                    self.after_edit();
                    self.status = "撤销".into();
                } else {
                    self.status = "已经是最早状态".into();
                }
            }
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.command_kind = CommandKind::Command;
                self.command.clear();
                self.status.clear();
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Command;
                self.command_kind = CommandKind::Search;
                self.command.clear();
                self.status.clear();
            }
            KeyCode::Char('n') => self.search_next(false),
            KeyCode::Char('N') => self.search_next(true),
            KeyCode::Char('?') => {
                self.mode = Mode::Help;
                self.help_scroll = 0;
                self.scaled_dirty = true;
            }
            KeyCode::F(2) => self.open_settings(),
            KeyCode::Esc => {
                self.status.clear();
                self.pending = None;
            }
            _ => {}
        }
    }

    fn move_normal(&mut self, delta: i32) {
        if self.display.is_empty() {
            return;
        }
        let next = self.normal_cursor as i32 + delta;
        self.normal_cursor = next.clamp(0, self.display.len() as i32 - 1) as usize;
        self.sync_editor_from_normal();
        self.ensure_normal_visible();
    }

    fn enter_insert(&mut self, kind: InsertKind) {
        self.sync_editor_from_normal();
        match kind {
            InsertKind::AtCursor => {}
            InsertKind::Append => self.editor.end(),
            InsertKind::FirstNonBlank => self.editor.first_nonblank(),
            InsertKind::End => self.editor.end(),
            InsertKind::OpenBelow => {
                self.editor.open_below();
                self.dirty_render = true;
            }
            InsertKind::OpenAbove => {
                self.editor.open_above();
                self.dirty_render = true;
            }
        }
        self.mode = Mode::Insert;
        self.status.clear();
        self.scaled_dirty = true;
        self.ensure_insert_visible();
    }

    fn leave_insert(&mut self) {
        self.mode = Mode::Normal;
        self.dirty_render = true;
        self.scaled_dirty = true;
        self.rebuild();
        self.sync_normal_from_source();
        self.ensure_normal_visible();
    }

    pub fn handle_paste(&mut self, text: &str) {
        if self.mode != Mode::Insert {
            return;
        }
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\n' => {
                    self.editor.insert_newline();
                    self.dirty_render = true;
                }
                _ => {
                    self.editor.insert_char(ch);
                    self.dirty_render = true;
                }
            }
        }
        self.ensure_insert_visible();
    }

    fn handle_insert(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('s') => {
                    self.save_current();
                    return;
                }
                KeyCode::Char('r') => {
                    if self.editor.redo() {
                        self.after_edit();
                    }
                    return;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Esc => self.leave_insert(),
            KeyCode::F(2) => {
                self.leave_insert();
                self.open_settings();
            }
            KeyCode::Char(c) => {
                self.editor.insert_char(c);
                self.dirty_render = true;
                self.ensure_insert_visible();
            }
            KeyCode::Enter => {
                self.editor.insert_newline();
                self.dirty_render = true;
                self.ensure_insert_visible();
            }
            KeyCode::Tab => {
                self.editor.insert_str("    ");
                self.dirty_render = true;
                self.ensure_insert_visible();
            }
            KeyCode::Backspace => {
                self.editor.backspace();
                self.dirty_render = true;
                self.ensure_insert_visible();
            }
            KeyCode::Delete => {
                self.editor.delete_forward();
                self.dirty_render = true;
                self.ensure_insert_visible();
            }
            KeyCode::Left => {
                self.editor.move_left();
                self.ensure_insert_visible();
            }
            KeyCode::Right => {
                self.editor.move_right();
                self.ensure_insert_visible();
            }
            KeyCode::Up => {
                self.editor.move_up();
                self.ensure_insert_visible();
            }
            KeyCode::Down => {
                self.editor.move_down();
                self.ensure_insert_visible();
            }
            KeyCode::Home => {
                self.editor.home();
                self.ensure_insert_visible();
            }
            KeyCode::End => {
                self.editor.end();
                self.ensure_insert_visible();
            }
            _ => {}
        }
    }

    fn handle_command(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command.clear();
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.command);
                self.mode = Mode::Normal;
                if self.command_kind == CommandKind::Search {
                    self.perform_search(&text);
                } else {
                    self.execute_command(&text);
                }
            }
            KeyCode::Backspace => {
                self.command.pop();
            }
            KeyCode::Char(c) => self.command.push(c),
            _ => {}
        }
    }

    fn handle_settings(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.scaled_dirty = true;
                self.theme = self.config.theme();
                self.rebuild();
                if self.settings_dirty {
                    match self.config.save() {
                        Ok(p) => self.status = format!("设置已应用 · 已保存到 {}", p.display()),
                        Err(e) => self.status = format!("设置已应用 · 保存失败: {e}"),
                    }
                    self.settings_dirty = false;
                } else {
                    self.status = "设置已应用".into();
                }
            }
            KeyCode::Char('j') | KeyCode::Down => self.settings_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.settings_move(-1),
            KeyCode::Char('h') | KeyCode::Left => self.change_setting(-1),
            KeyCode::Char('l') | KeyCode::Right => self.change_setting(1),
            KeyCode::Enter | KeyCode::Char(' ') => self.change_setting(1),
            KeyCode::Char('s') => match self.config.save() {
                Ok(p) => {
                    self.status = format!("已保存到 {}", p.display());
                    self.settings_dirty = false;
                }
                Err(e) => self.status = format!("保存失败: {e}"),
            },
            KeyCode::Char('r') => {
                self.config = Config::default();
                self.theme = self.config.theme();
                self.dirty_render = true;
                let items = self.settings_items();
                self.settings_index = items
                    .iter()
                    .position(|i| i.kind != SettingKind::Section)
                    .unwrap_or(0);
                self.settings_scroll = 0;
                self.settings_dirty = true;
                self.status = "已恢复默认设置".into();
            }
            _ => {}
        }
    }

    fn handle_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.scaled_dirty = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1)
            }
            _ => {}
        }
    }

    pub fn help_lines(&self) -> Vec<(String, String)> {
        vec![
            ("j / k".into(), "上下移动，阅读态按渲染行".into()),
            ("Ctrl-D / Ctrl-U".into(), "翻半页".into()),
            ("g g / G".into(), "跳到开头 / 结尾".into()),
            (
                "i / a / I / A".into(),
                "进入编辑：当前位置 / 行尾 / 行首 / 行尾".into(),
            ),
            ("o / O".into(), "在下方 / 上方新开一行并编辑".into()),
            ("Esc".into(), "回到渲染阅读态".into()),
            (":".into(), "命令模式，例如 :w、:q、:wq".into()),
            ("/".into(), "搜索；n / N 跳下一个 / 上一个".into()),
            ("F2 或 :settings".into(), "打开设置页".into()),
            ("?".into(), "显示这个帮助".into()),
            ("u / Ctrl-R".into(), "撤销 / 重做".into()),
            (
                "x / dd / yy / p".into(),
                "删除字符 / 删除行 / 复制行 / 粘贴行".into(),
            ),
            ("F2".into(), "设置页里改颜色；终端无法逐行改字号".into()),
        ]
    }

    // ------------------------------------------------------------------
    // 命令
    // ------------------------------------------------------------------

    pub fn execute_command(&mut self, raw: &str) {
        let raw = raw.trim();
        if raw.is_empty() {
            return;
        }
        let mut parts = raw.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();
        match cmd {
            "w" | "write" => {
                if arg.is_empty() {
                    self.save_current();
                } else {
                    self.save_as(PathBuf::from(arg));
                }
            }
            "q" | "quit" => {
                if self.editor.dirty {
                    self.status = "有未保存修改（用 :q! 强制退出）".into();
                } else {
                    self.should_quit = true;
                }
            }
            "q!" | "quit!" => self.should_quit = true,
            "wq" | "x" => {
                if arg.is_empty() {
                    if self.save_current() {
                        self.should_quit = true;
                    }
                } else {
                    if self.save_as(PathBuf::from(arg)) {
                        self.should_quit = true;
                    }
                }
            }
            "wq!" => {
                let ok = if arg.is_empty() {
                    self.save_current()
                } else {
                    self.save_as(PathBuf::from(arg))
                };
                if ok {
                    self.should_quit = true;
                }
            }
            "e" | "edit" => {
                if arg.is_empty() {
                    self.status = "E32: 没有文件名".into();
                    return;
                }
                if self.editor.dirty {
                    self.status = "有未保存修改（用 :e! 放弃）".into();
                    return;
                }
                self.load_file(PathBuf::from(arg));
            }
            "e!" => {
                if !arg.is_empty() {
                    self.load_file(PathBuf::from(arg));
                }
            }
            "settings" => self.open_settings(),
            "help" => {
                self.mode = Mode::Help;
                self.help_scroll = 0;
                self.scaled_dirty = true;
            }
            "theme" => {
                if arg.is_empty() {
                    self.status = format!("主题: {}", self.config.preset);
                } else {
                    let p = preset_by_name(arg);
                    self.config.apply_preset(p.name);
                    self.theme = self.config.theme();
                    self.dirty_render = true;
                    self.status = format!("主题: {}", p.label);
                }
            }
            "set" => match arg {
                "number" => {
                    self.config.line_numbers = true;
                    self.theme = self.config.theme();
                    self.status = "行号: 开".into();
                }
                "nonumber" => {
                    self.config.line_numbers = false;
                    self.theme = self.config.theme();
                    self.status = "行号: 关".into();
                }
                _ => self.status = format!("未知选项: {arg}"),
            },
            "goto" | "g" => {
                if let Ok(n) = arg.parse::<usize>() {
                    self.editor.row = n.saturating_sub(1).min(self.editor.max_line());
                    self.editor.col = 0;
                    self.dirty_render = true;
                    self.rebuild();
                    self.sync_normal_from_source();
                    self.scroll = self.normal_cursor.saturating_sub(3);
                    self.ensure_normal_visible();
                    self.status = format!("第 {n} 行");
                } else {
                    self.status = "用法: :goto N".into();
                }
            }
            "raw" => {
                self.enter_insert(InsertKind::AtCursor);
            }
            _ => self.status = format!("E492: 不是编辑器命令: {raw}"),
        }
    }

    fn save_current(&mut self) -> bool {
        match self.editor.save() {
            Ok(()) => {
                self.status = format!("已写入: {}", self.editor.full_label());
                true
            }
            Err(e) => {
                self.status = format!("写入失败: {e}");
                false
            }
        }
    }

    fn save_as(&mut self, path: PathBuf) -> bool {
        match self.editor.save_as(path) {
            Ok(()) => {
                self.status = format!("已写入: {}", self.editor.full_label());
                true
            }
            Err(e) => {
                self.status = format!("写入失败: {e}");
                false
            }
        }
    }

    fn load_file(&mut self, path: PathBuf) {
        match Editor::open(path) {
            Ok(e) => {
                self.editor = e;
                self.normal_cursor = 0;
                self.scroll = 0;
                self.raw_scroll = 0;
                self.hscroll = 0;
                self.dirty_render = true;
                self.scaled_dirty = true;
                self.rebuild();
                self.mode = Mode::Normal;
                self.status = format!("已打开: {}", self.editor.full_label());
            }
            Err(e) => self.status = format!("cannot open: {e}"),
        }
    }

    fn perform_search(&mut self, query: &str) {
        let query = if query.is_empty() {
            self.last_search.clone().unwrap_or_default()
        } else {
            self.last_search = Some(query.to_string());
            query.to_string()
        };
        if query.is_empty() {
            self.status = "搜索内容为空".into();
            return;
        }
        if let Some(row) = self.editor.find_forward(&query, self.editor.row + 1) {
            self.editor.row = row;
            self.editor.col = 0;
            self.dirty_render = true;
            self.rebuild();
            self.sync_normal_from_source();
            self.scroll = self.normal_cursor.saturating_sub(3);
            self.ensure_normal_visible();
            self.status = format!("匹配: 第 {} 行", row + 1);
        } else {
            self.status = format!("未找到: {query}");
        }
    }

    fn search_next(&mut self, backward: bool) {
        let Some(query) = self.last_search.clone() else {
            self.status = "没有上一次搜索".into();
            return;
        };
        let found = if backward {
            self.editor
                .find_backward(&query, self.editor.row.saturating_sub(1))
        } else {
            self.editor.find_forward(&query, self.editor.row + 1)
        };
        if let Some(row) = found {
            self.editor.row = row;
            self.editor.col = 0;
            self.dirty_render = true;
            self.rebuild();
            self.sync_normal_from_source();
            self.scroll = self.normal_cursor.saturating_sub(3);
            self.ensure_normal_visible();
            self.status = format!("匹配: 第 {} 行", row + 1);
        } else {
            self.status = format!("未找到: {query}");
        }
    }

    fn after_edit(&mut self) {
        self.dirty_render = true;
        self.rebuild();
        self.sync_normal_from_source();
        self.ensure_normal_visible();
    }

    // ------------------------------------------------------------------
    // 设置页
    // ------------------------------------------------------------------

    pub fn settings_items(&self) -> Vec<SettingItem> {
        let p = preset_by_name(&self.config.preset);
        let mut out = Vec::new();
        out.push(SettingItem {
            label: "主题".into(),
            value: String::new(),
            kind: SettingKind::Section,
            color: None,
            hint: String::new(),
        });
        out.push(SettingItem {
            label: "主题预设".into(),
            value: p.label.into(),
            kind: SettingKind::Preset,
            color: None,
            hint: "h/l 切换整套主题".into(),
        });
        for role in Role::ALL {
            let name = ColorName::parse(self.config.role_name(role));
            out.push(SettingItem {
                label: role.label().into(),
                value: name.label().into(),
                kind: SettingKind::Color(role),
                color: Some(name.color()),
                hint: "h/l 循环颜色".into(),
            });
        }
        out.push(SettingItem {
            label: "Markdown".into(),
            value: String::new(),
            kind: SettingKind::Section,
            color: None,
            hint: String::new(),
        });
        out.push(SettingItem {
            label: "标题装饰线".into(),
            value: if self.config.heading_rules {
                "开"
            } else {
                "关"
            }
            .into(),
            kind: SettingKind::HeadingRules,
            color: None,
            hint: "H1 / H2 下方的装饰线".into(),
        });
        out.push(SettingItem {
            label: "表格边框".into(),
            value: if self.config.table_borders {
                "开"
            } else {
                "关"
            }
            .into(),
            kind: SettingKind::TableBorders,
            color: None,
            hint: "圆角框 / 纯文本表格".into(),
        });
        out.push(SettingItem {
            label: "标题加大".into(),
            value: if !self.text_sizing_supported {
                "不可用".into()
            } else if self.text_sizing_enabled {
                "开".into()
            } else {
                "关".into()
            },
            kind: SettingKind::TextSizing,
            color: None,
            hint: self.text_sizing_reason.clone(),
        });
        for level in 1u8..=3 {
            let current = match level {
                1 => self.config.h1_scale,
                2 => self.config.h2_scale,
                _ => self.config.h3_scale,
            };
            out.push(SettingItem {
                label: format!("H{level} 字号"),
                value: if self.text_sizing_supported {
                    format!("{}x", current.clamp(1, 4))
                } else {
                    "不可用".into()
                },
                kind: SettingKind::HeadingScale(level),
                color: None,
                hint: if self.text_sizing_supported {
                    "h/l 切换 1x–4x".into()
                } else {
                    self.text_sizing_reason.clone()
                },
            });
        }
        out.push(SettingItem {
            label: "编辑器".into(),
            value: String::new(),
            kind: SettingKind::Section,
            color: None,
            hint: String::new(),
        });
        out.push(SettingItem {
            label: "行号".into(),
            value: if self.config.line_numbers {
                "开"
            } else {
                "关"
            }
            .into(),
            kind: SettingKind::LineNumbers,
            color: None,
            hint: "插入模式左侧行号".into(),
        });
        out.push(SettingItem {
            label: "当前行高亮".into(),
            value: if self.config.focus_line { "开" } else { "关" }.into(),
            kind: SettingKind::FocusLine,
            color: None,
            hint: "阅读 / 编辑时高亮当前行".into(),
        });
        out
    }

    fn open_settings(&mut self) {
        self.mode = Mode::Settings;
        self.scaled_dirty = true;
        self.status.clear();
        let items = self.settings_items();
        self.settings_index = items
            .iter()
            .position(|i| i.kind != SettingKind::Section)
            .unwrap_or(0);
        self.settings_scroll = 0;
        self.settings_dirty = false;
        if let Some(item) = items.get(self.settings_index) {
            self.status = item.hint.clone();
        }
    }

    fn settings_move(&mut self, dir: i32) {
        let items = self.settings_items();
        if items.is_empty() {
            return;
        }
        let mut idx = self.settings_index as i32;
        for _ in 0..items.len() {
            idx = (idx + dir).rem_euclid(items.len() as i32);
            if items[idx as usize].kind != SettingKind::Section {
                break;
            }
        }
        self.settings_index = idx as usize;
        if let Some(item) = self.settings_items().get(self.settings_index) {
            self.status = item.hint.clone();
        }
    }

    fn change_setting(&mut self, dir: i32) {
        let items = self.settings_items();
        let Some(item) = items.get(self.settings_index) else {
            return;
        };
        self.settings_dirty = true;
        match item.kind {
            SettingKind::Preset => {
                let p = preset_next(&self.config.preset, dir);
                self.config.apply_preset(p.name);
                self.theme = self.config.theme();
                self.dirty_render = true;
            }
            SettingKind::Color(role) => {
                self.config.cycle_role(role, dir);
                self.theme = self.config.theme();
                self.dirty_render = true;
            }
            SettingKind::HeadingRules => {
                self.config.heading_rules = !self.config.heading_rules;
                self.theme = self.config.theme();
                self.dirty_render = true;
            }
            SettingKind::TableBorders => {
                self.config.table_borders = !self.config.table_borders;
                self.theme = self.config.theme();
                self.dirty_render = true;
            }
            SettingKind::TextSizing => {
                if !self.text_sizing_supported {
                    self.status = self.text_sizing_reason.clone();
                    return;
                }
                self.config.text_sizing = !self.text_sizing_enabled;
                self.text_sizing_enabled = self.config.text_sizing;
                self.scaled_dirty = true;
                self.dirty_render = true;
            }
            SettingKind::HeadingScale(level) => {
                if !self.text_sizing_supported {
                    self.status = self.text_sizing_reason.clone();
                    return;
                }
                let field = match level {
                    1 => &mut self.config.h1_scale,
                    2 => &mut self.config.h2_scale,
                    _ => &mut self.config.h3_scale,
                };
                *field = cycle_scale(*field, dir);
                self.text_sizing_enabled = true;
                self.config.text_sizing = true;
                self.scaled_dirty = true;
                self.dirty_render = true;
            }
            SettingKind::LineNumbers => {
                self.config.line_numbers = !self.config.line_numbers;
                self.theme = self.config.theme();
            }
            SettingKind::FocusLine => {
                self.config.focus_line = !self.config.focus_line;
                self.theme = self.config.theme();
            }
            SettingKind::Section => {}
        }
    }
}

fn cycle_scale(current: u16, dir: i32) -> u16 {
    let cur = current.clamp(1, 4) as i32;
    (1 + (cur - 1 + dir).rem_euclid(4)) as u16
}

#[derive(Clone, Copy)]
enum InsertKind {
    AtCursor,
    Append,
    FirstNonBlank,
    End,
    OpenBelow,
    OpenAbove,
}
