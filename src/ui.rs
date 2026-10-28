// vimd 的界面绘制。
//
// 视觉参考 DSH TUI：圆角框、安静的分隔线、清晰的状态栏；
// Markdown 表格和代码块也用同一套框线语言，而不是把原始 | 直接丢出来。

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo};
use crossterm::execute;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Mode, SettingKind};
use crate::theme::Theme;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = app.theme.clone();

    frame.render_widget(Block::default().style(Style::default().bg(theme.bg)), area);

    if area.width < 48 || area.height < 12 {
        frame.render_widget(
            Paragraph::new("vimd: terminal too small").style(Style::default().fg(theme.muted)),
            area,
        );
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(2),
    ])
    .split(area);
    let header = chunks[0];
    let body = chunks[1];
    let footer = chunks[2];

    draw_header(frame, app, header, &theme);
    draw_body(frame, app, body, &theme);
    draw_footer(frame, app, footer, &theme);

    if matches!(app.mode, Mode::Settings | Mode::Help) {
        // 终端没有 alpha，用 DIM 压暗背景，让弹层成为视觉焦点。
        let backdrop = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(3),
        );
        frame
            .buffer_mut()
            .set_style(backdrop, Style::default().add_modifier(Modifier::DIM));
    }

    match app.mode {
        Mode::Settings => draw_settings(frame, app, area, &theme),
        Mode::Help => draw_help(frame, app, area, &theme),
        _ => {}
    }
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::Command => "COMMAND",
        Mode::Settings => "SETTINGS",
        Mode::Help => "HELP",
    }
}

fn mode_color(mode: Mode, theme: &Theme) -> Color {
    match mode {
        Mode::Normal => theme.accent,
        Mode::Insert => theme.code,
        Mode::Command => theme.h2,
        Mode::Settings => theme.h3,
        Mode::Help => theme.muted,
    }
}

fn on_badge(theme: &Theme) -> Color {
    if theme.bg == Color::Reset {
        Color::Black
    } else {
        theme.bg
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.surface)),
        area,
    );
    let cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(24)]).split(area);

    let mut left = vec![
        Span::styled(
            " vimd ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.editor.file_label(), Style::default().fg(theme.body)),
    ];
    if app.editor.dirty {
        left.push(Span::styled(" ●", Style::default().fg(theme.accent)));
    }
    frame.render_widget(Paragraph::new(Line::from(left)), cols[0]);

    let label = mode_label(app.mode);
    let right = Line::from(vec![
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(on_badge(theme))
                .bg(mode_color(app.mode, theme))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {:>3}% ", app.percent()),
            Style::default().fg(theme.muted),
        ),
    ]);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), cols[1]);
}

fn draw_body(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let top_pad = if app.mode == Mode::Insert { 0 } else { 1 };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.bg))
        .padding(Padding::new(1, 1, top_pad, 0))
        .title(Line::from(Span::styled(
            if app.mode == Mode::Insert {
                " source "
            } else {
                " rendered "
            },
            Style::default().fg(theme.muted),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.content_area = inner;

    app.set_viewport(inner.width as usize, inner.height as usize);

    if app.mode == Mode::Insert {
        draw_insert(frame, app, inner, theme);
    } else {
        draw_normal(frame, app, inner, theme);
    }
}

fn draw_normal(frame: &mut Frame, app: &mut App, inner: Rect, theme: &Theme) {
    app.ensure_render();
    let start = app.scroll.min(app.display.len());
    let end = (start + inner.height as usize).min(app.display.len());
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (offset, d) in app.display[start..end].iter().enumerate() {
        let idx = start + offset;
        let mut line = Line::from(d.spans.clone());
        if idx == app.normal_cursor && theme.focus_line {
            line = line.style(Style::default().bg(theme.focus_bg));
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "empty document · press i to start",
            Style::default().fg(theme.muted),
        )));
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_insert(frame: &mut Frame, app: &mut App, inner: Rect, theme: &Theme) {
    let total = app.editor.lines.len();
    let digits = total.max(1).to_string().len();
    let gutter = if theme.line_numbers {
        digits as u16 + 3
    } else {
        0
    };
    let avail = inner.width.saturating_sub(gutter) as usize;
    let start = app.raw_scroll.min(total.saturating_sub(1));
    let end = (start + inner.height as usize).min(total);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for row in start..end {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if theme.line_numbers {
            let current = row == app.editor.row;
            let num_style = if current && theme.focus_line {
                Style::default()
                    .fg(theme.accent)
                    .bg(theme.focus_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.muted)
            };
            spans.push(Span::styled(
                format!("{:>width$} ", row + 1, width = digits),
                num_style,
            ));
            spans.push(Span::styled("│ ", Style::default().fg(theme.border)));
        }
        let expanded = expand_tabs(app.editor.line(row));
        let visible = slice_columns(&expanded, app.hscroll, avail);
        spans.push(Span::styled(visible, Style::default().fg(theme.body)));
        let mut line = Line::from(spans);
        if row == app.editor.row && theme.focus_line {
            line = line.style(Style::default().bg(theme.focus_bg));
        }
        lines.push(line);
    }
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);

    // 终端光标只在实际编辑时显示。
    let cx = inner.x.saturating_add(gutter) + app.cursor_screen_col();
    let cy = inner.y + (app.editor.row - start) as u16;
    let max_x = inner.x + inner.width.saturating_sub(1);
    let max_y = inner.y + inner.height.saturating_sub(1);
    frame.set_cursor_position(Position::new(cx.min(max_x), cy.min(max_y)));
}

pub fn expand_tabs(s: &str) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let n = 4 - (col % 4);
            out.push_str(&" ".repeat(n));
            col += n;
        } else {
            out.push(ch);
            col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    out
}

pub fn slice_columns(s: &str, skip: usize, max: usize) -> String {
    let mut out = String::new();
    let mut col = 0usize;
    let mut used = 0usize;
    for ch in s.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if col + w <= skip {
            col += w;
            continue;
        }
        if col < skip {
            col += w;
            continue;
        }
        if used + w > max {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.surface)),
        area,
    );
    let cols = Layout::horizontal([Constraint::Min(10), Constraint::Length(22)]).split(area);

    let (left, hint) = match app.mode {
        Mode::Normal => (
            if app.status.is_empty() {
                "就绪".to_string()
            } else {
                app.status.clone()
            },
            "i 编辑   : 命令   / 搜索   F2 设置   ? 帮助",
        ),
        Mode::Insert => (
            "INSERT · Esc 返回阅读 · Ctrl-S 保存".to_string(),
            "Enter 换行   Tab 四空格   方向键移动",
        ),
        Mode::Command => {
            let prompt = if app.command_kind == crate::app::CommandKind::Search {
                "/"
            } else {
                ":"
            };
            (format!("{prompt}{}▏", app.command), "Enter 执行   Esc 取消")
        }
        Mode::Settings => (
            if app.status.is_empty() {
                "SETTINGS".to_string()
            } else {
                app.status.clone()
            },
            "j/k 移动   h/l 修改   s 保存   r 重置   Esc 关闭",
        ),
        Mode::Help => ("按键".to_string(), "j/k 滚动   Esc 关闭"),
    };

    let status_style = if app.status.is_empty() {
        Style::default().fg(theme.muted)
    } else {
        Style::default().fg(theme.body)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(format!(" {left}"), status_style))),
        Rect::new(cols[0].x, area.y, cols[0].width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(
                "{}  {} ",
                app.position_label(),
                format!("{}%", app.percent())
            ),
            Style::default().fg(theme.muted),
        )))
        .alignment(Alignment::Right),
        Rect::new(cols[1].x, area.y, cols[1].width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {hint}"),
            Style::default().fg(theme.muted),
        ))),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

fn fill_rect(frame: &mut Frame, area: Rect, theme: &Theme) {
    let blank = " ".repeat(area.width as usize);
    let lines: Vec<Line<'static>> = (0..area.height)
        .map(|_| {
            Line::from(Span::styled(
                blank.clone(),
                Style::default().bg(theme.surface),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let w = area.width.saturating_sub(6).min(78).max(30);
    let h = area.height.saturating_sub(4).min(28).max(8);
    let modal = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    fill_rect(frame, modal, theme);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(2, 2, 1, 0))
        .title(Line::from(Span::styled(
            " 设置 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let mut rows: Vec<(Line<'static>, Option<usize>)> = Vec::new();

    let items = app.settings_items();
    for (idx, item) in items.iter().enumerate() {
        if item.kind == SettingKind::Section {
            if rows.last().is_some_and(|(line, _)| !line.spans.is_empty()) {
                rows.push((Line::from(""), None));
            }
            rows.push((
                Line::from(Span::styled(
                    format!(" {} ", item.label),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::BOLD),
                )),
                None,
            ));
            continue;
        }
        let selected = idx == app.settings_index;
        let bg = if selected {
            theme.focus_bg
        } else {
            theme.surface
        };
        let label_style = Style::default()
            .fg(if selected { theme.accent } else { theme.body })
            .bg(bg);
        let value_style = Style::default().fg(theme.body).bg(bg);
        let label_pad = 22usize.saturating_sub(item.label.width());
        let label = format!("  {}{}", item.label, " ".repeat(label_pad));
        let mut spans = vec![Span::styled(label, label_style)];
        if let Some(c) = item.color {
            spans.push(Span::styled("■ ", Style::default().fg(c).bg(bg)));
        }
        let value = item.value.clone();
        let value_w = value.width() + if item.color.is_some() { 2 } else { 0 };
        let used = 2 + 22 + value_w;
        let pad = (inner.width as usize).saturating_sub(used + 2);
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
        spans.push(Span::styled(value, value_style));
        rows.push((Line::from(spans), Some(idx)));
    }

    let selected_row = rows
        .iter()
        .position(|(_, idx)| *idx == Some(app.settings_index))
        .unwrap_or(0);
    let visible = inner.height as usize;
    if selected_row < app.settings_scroll {
        app.settings_scroll = selected_row;
    } else if selected_row >= app.settings_scroll + visible {
        app.settings_scroll = selected_row + 1 - visible;
    }
    let max_scroll = rows.len().saturating_sub(visible);
    app.settings_scroll = app.settings_scroll.min(max_scroll);
    let start = app.settings_scroll.min(rows.len());
    let end = (start + visible).min(rows.len());
    let lines: Vec<Line<'static>> = rows[start..end].iter().map(|(l, _)| l.clone()).collect();
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn draw_help(frame: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
    let w = area.width.saturating_sub(8).min(74).max(30);
    let h = area.height.saturating_sub(4).min(22).max(8);
    let modal = Rect::new(
        area.x + (area.width.saturating_sub(w)) / 2,
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h,
    );
    frame.render_widget(Clear, modal);
    fill_rect(frame, modal, theme);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(1, 1, 1, 0))
        .title(Line::from(Span::styled(
            " 按键 ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let lines: Vec<Line<'static>> = app
        .help_lines()
        .into_iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {:<18}", k), Style::default().fg(theme.h2)),
                Span::styled(v, Style::default().fg(theme.body)),
            ])
        })
        .collect();
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    app.help_scroll = app.help_scroll.min(max_scroll);
    let start = app.help_scroll.min(lines.len());
    let end = (start + inner.height as usize).min(lines.len());
    frame.render_widget(
        Paragraph::new(Text::from(lines[start..end].to_vec())),
        inner,
    );
}

// ---------------------------------------------------------------------------
// Kitty OSC 66：真正放大 H1/H2
// ---------------------------------------------------------------------------

fn fg_sgr(c: Color) -> String {
    match c {
        Color::Reset => "\x1b[39m".to_string(),
        Color::Rgb(r, g, b) => format!("\x1b[38;2;{r};{g};{b}m"),
        Color::Indexed(i) => format!("\x1b[38;5;{i}m"),
        _ => "\x1b[39m".to_string(),
    }
}

fn bg_sgr(c: Color) -> String {
    match c {
        Color::Reset => "\x1b[49m".to_string(),
        Color::Rgb(r, g, b) => format!("\x1b[48;2;{r};{g};{b}m"),
        Color::Indexed(i) => format!("\x1b[48;5;{i}m"),
        _ => "\x1b[49m".to_string(),
    }
}

/// 清掉上一帧由 OSC 66 画出的多 cell 标题。
///
/// Ratatui 的 buffer 里这些位置只是空格，它自己不会负责擦除多 cell 字符，
/// 所以滚动/重建/进入弹层之前必须由我们显式用空格覆盖。
pub fn clear_scaled_headings(app: &mut App) -> io::Result<()> {
    if app.last_scaled.is_empty() {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    let bg = if app.color_enabled {
        bg_sgr(app.theme.bg)
    } else {
        String::new()
    };
    for rect in app.last_scaled.drain(..) {
        for row in 0..rect.height {
            execute!(out, MoveTo(rect.x, rect.y + row))?;
            if !bg.is_empty() {
                out.write_all(bg.as_bytes())?;
            }
            out.write_all(" ".repeat(rect.width as usize).as_bytes())?;
            out.write_all(b"\x1b[0m")?;
        }
    }
    execute!(out, Hide)?;
    out.flush()
}

/// 在 ratatui draw 之后覆盖绘制大标题。
///
/// 只在阅读态/命令行绘制；插入态和弹层里不画，避免覆盖光标或弹层。
pub fn paint_scaled_headings(app: &mut App) -> io::Result<()> {
    if !app.text_sizing_enabled
        || app.display.is_empty()
        || matches!(app.mode, Mode::Insert | Mode::Settings | Mode::Help)
    {
        return Ok(());
    }
    let area = app.content_area;
    if area.width == 0 || area.height == 0 {
        return Ok(());
    }

    let mut out = io::stdout().lock();
    let mut new_rects = Vec::new();
    for (idx, line) in app.display.iter().enumerate() {
        let Some(h) = &line.scaled else {
            continue;
        };
        if idx < app.scroll {
            continue;
        }
        let scale = h.scale as u16;
        let y = area.y + (idx - app.scroll) as u16;
        if y + scale > area.y + area.height {
            continue;
        }
        let text_w = UnicodeWidthStr::width(h.text.as_str()) as u16 * scale;
        if text_w > area.width {
            continue;
        }
        let color = match h.level {
            1 => app.theme.h1,
            2 => app.theme.h2,
            _ => app.theme.h3,
        };
        let mut seq = String::new();
        if app.color_enabled {
            seq.push_str(&fg_sgr(color));
            seq.push_str(&bg_sgr(app.theme.bg));
            seq.push_str("\x1b[1m");
        }
        seq.push_str(&format!("\x1b]66;s={};{}\x07", h.scale, h.text));
        seq.push_str("\x1b[0m");
        execute!(out, MoveTo(area.x, y))?;
        out.write_all(seq.as_bytes())?;
        new_rects.push(Rect::new(area.x, y, text_w.max(1), scale));
    }
    app.last_scaled = new_rects;
    execute!(out, Hide)?;
    out.flush()
}
