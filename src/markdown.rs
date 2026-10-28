// vimd 的 Markdown 渲染层。
//
// 输出 Vec<DisplayLine>：每一行已经排好样式，并记住它来自源文件的哪一行、
// 哪一列，方便阅读态按 i 时把光标送回源码。
//
// 这里不是 CommonMark 实现，而是一个“阅读器优先”的轻量解析器：
//   * 标题、粗体、斜体、行内代码、链接、图片 alt
//   * 列表、任务列表、引用、分隔线、围栏代码块
//   * 管道表格：用圆角框和列对齐画出来
//   * HTML 注释、frontmatter 默认隐藏
//
// 终端字号固定，所以标题不放大；用颜色、粗体、标题下装饰线来建立层级。

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;

#[derive(Clone, Debug)]
pub struct ScaledHeading {
    pub text: String,
    pub scale: u16,
    pub level: usize,
}

#[derive(Clone, Debug)]
pub struct DisplayLine {
    pub spans: Vec<Span<'static>>,
    pub source_line: usize,
    pub source_col: usize,
    pub scaled: Option<ScaledHeading>,
}

#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    pub h1_scale: u16,
    pub h2_scale: u16,
    pub h3_scale: u16,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            h1_scale: 1,
            h2_scale: 1,
            h3_scale: 1,
        }
    }
}

impl DisplayLine {
    pub fn is_blank(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.content.trim().is_empty())
    }
}

#[derive(Clone)]
struct RichSpan {
    text: String,
    style: Style,
}

fn rs(text: impl Into<String>, style: Style) -> RichSpan {
    RichSpan {
        text: text.into(),
        style,
    }
}

fn push_span(out: &mut Vec<RichSpan>, text: impl Into<String>, style: Style) {
    let text = text.into();
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.style == style {
            last.text.push_str(&text);
            return;
        }
    }
    out.push(rs(text, style));
}

fn width_of_spans(spans: &[RichSpan]) -> usize {
    spans
        .iter()
        .map(|s| UnicodeWidthStr::width(s.text.as_str()))
        .sum()
}

fn to_spans(spans: Vec<RichSpan>) -> Vec<Span<'static>> {
    spans
        .into_iter()
        .map(|s| Span::styled(s.text, s.style))
        .collect()
}

fn push_line(
    out: &mut Vec<DisplayLine>,
    spans: Vec<RichSpan>,
    source_line: usize,
    source_col: usize,
) {
    if spans.is_empty() {
        out.push(DisplayLine {
            spans: Vec::new(),
            source_line,
            source_col,
            scaled: None,
        });
    } else {
        out.push(DisplayLine {
            spans: to_spans(spans),
            source_line,
            source_col,
            scaled: None,
        });
    }
}

fn push_blank(out: &mut Vec<DisplayLine>, source_line: usize) {
    if out.is_empty() || out.last().is_some_and(DisplayLine::is_blank) {
        return;
    }
    out.push(DisplayLine {
        spans: Vec::new(),
        source_line,
        source_col: 0,
        scaled: None,
    });
}

// ---------------------------------------------------------------------------
// inline
// ---------------------------------------------------------------------------

fn inline(text: &str, base: Style, theme: &Theme) -> Vec<RichSpan> {
    parse_inline(text, base, theme, 0)
}

fn parse_inline(s: &str, base: Style, theme: &Theme, depth: usize) -> Vec<RichSpan> {
    if depth > 8 {
        return vec![rs(s, base)];
    }

    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0usize;

    let flush = |plain: &mut String, out: &mut Vec<RichSpan>| {
        if !plain.is_empty() {
            push_span(out, std::mem::take(plain), base);
        }
    };

    while i < s.len() {
        let rest = &s[i..];

        // 反斜杠转义
        if rest.starts_with('\\') {
            let mut it = rest.chars();
            it.next();
            if let Some(next) = it.next() {
                plain.push(next);
                i += 1 + next.len_utf8();
                continue;
            }
        }

        // 行内代码
        if rest.starts_with('`') {
            if let Some(end) = rest[1..].find('`') {
                flush(&mut plain, &mut out);
                let code = &rest[1..1 + end];
                let style = base.fg(theme.code).bg(theme.code_bg);
                push_span(&mut out, code, style);
                i += 1 + end + 1;
                continue;
            }
        }

        // 粗体
        if rest.starts_with("**") {
            if let Some(end) = rest[2..].find("**") {
                flush(&mut plain, &mut out);
                let inner = &rest[2..2 + end];
                let style = base.add_modifier(Modifier::BOLD);
                out.extend(parse_inline(inner, style, theme, depth + 1));
                i += 2 + end + 2;
                continue;
            }
        }

        // 删除线
        if rest.starts_with("~~") {
            if let Some(end) = rest[2..].find("~~") {
                flush(&mut plain, &mut out);
                let inner = &rest[2..2 + end];
                let style = base.add_modifier(Modifier::CROSSED_OUT);
                out.extend(parse_inline(inner, style, theme, depth + 1));
                i += 2 + end + 2;
                continue;
            }
        }

        // 图片：只显示 alt，不显示 URL
        if rest.starts_with("![") {
            if let Some((alt, url_len)) = parse_link(rest, 1) {
                flush(&mut plain, &mut out);
                push_span(&mut out, "▧ ", base.fg(theme.muted));
                let style = base.fg(theme.accent).add_modifier(Modifier::ITALIC);
                out.extend(parse_inline(alt, style, theme, depth + 1));
                let consumed = 1 + (1 + alt.len() + 2 + url_len + 1);
                i += consumed.min(rest.len());
                continue;
            }
        }

        // 链接
        if rest.starts_with('[') {
            if let Some((label, url_len)) = parse_link(rest, 0) {
                flush(&mut plain, &mut out);
                let style = base.fg(theme.accent).add_modifier(Modifier::UNDERLINED);
                out.extend(parse_inline(label, style, theme, depth + 1));
                let consumed = 1 + label.len() + 2 + url_len + 1;
                i += consumed.min(rest.len());
                continue;
            }
        }

        // 斜体：单个星号，且不能是列表/粗体
        if rest.starts_with('*') && !rest.starts_with("**") {
            let after = &rest[1..];
            if !after.starts_with(' ') {
                if let Some(end) = after.find('*') {
                    flush(&mut plain, &mut out);
                    let inner = &after[..end];
                    let style = base.add_modifier(Modifier::ITALIC);
                    out.extend(parse_inline(inner, style, theme, depth + 1));
                    i += 1 + end + 1;
                    continue;
                }
            }
        }

        let Some(ch) = rest.chars().next() else {
            break;
        };
        plain.push(ch);
        i += ch.len_utf8();
    }

    flush(&mut plain, &mut out);

    // 合并被递归拆出来的同风格片段
    let mut merged: Vec<RichSpan> = Vec::new();
    for s in out {
        push_span(&mut merged, s.text, s.style);
    }
    merged
}

/// 解析 `[label](url)`。`offset` 允许从 `!` 后面开始。
/// 返回 (label, url 的显示字节长度)。
fn parse_link(s: &str, offset: usize) -> Option<(&str, usize)> {
    let rest = &s[offset..];
    if !rest.starts_with('[') {
        return None;
    }
    let close = rest[1..].find(']')? + 1;
    if !rest[close + 1..].starts_with('(') {
        return None;
    }
    let url_start = close + 2;
    let url_end = rest[url_start..].find(')')? + url_start;
    Some((&rest[1..close], url_end - url_start))
}

// ---------------------------------------------------------------------------
// wrapping
// ---------------------------------------------------------------------------

fn wrap_rich(
    out: &mut Vec<DisplayLine>,
    content: Vec<RichSpan>,
    first_prefix: Vec<RichSpan>,
    rest_prefix: Vec<RichSpan>,
    width: usize,
    source_line: usize,
    source_col: usize,
) {
    let width = width.max(8);
    let mut current = first_prefix.clone();
    let mut current_w = width_of_spans(&current);
    let first_w = current_w;
    let rest_w = width_of_spans(&rest_prefix);
    let mut has_content = false;

    for span in content {
        for ch in span.text.chars() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if cw > 0 && current_w + cw > width && (has_content || current_w > first_w) {
                push_line(out, std::mem::take(&mut current), source_line, source_col);
                current = rest_prefix.clone();
                current_w = rest_w;
            }
            push_span(&mut current, ch.to_string(), span.style);
            current_w += cw;
            has_content = true;
        }
    }

    if has_content || !current.is_empty() {
        push_line(out, current, source_line, source_col);
    } else {
        push_line(out, Vec::new(), source_line, source_col);
    }
}

fn clip_spans(spans: &[RichSpan], max_width: usize) -> (Vec<RichSpan>, bool) {
    let mut out = Vec::new();
    let mut used = 0usize;
    let mut truncated = false;
    'outer: for span in spans {
        for ch in span.text.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + w > max_width {
                truncated = true;
                break 'outer;
            }
            push_span(&mut out, ch.to_string(), span.style);
            used += w;
        }
    }
    (out, truncated)
}

fn fit_cell(spans: &[RichSpan], width: usize, align: Alignment, base: Style) -> Vec<RichSpan> {
    let total = width_of_spans(spans);
    let (mut clipped, truncated) = if total > width {
        let (mut c, _) = clip_spans(spans, width.saturating_sub(1));
        if width > 0 {
            push_span(&mut c, "…", base);
        }
        (c, true)
    } else {
        (spans.to_vec(), false)
    };
    let _ = truncated;
    let used = width_of_spans(&clipped);
    let pad = width.saturating_sub(used);
    let (left, right) = match align {
        Alignment::Left => (0, pad),
        Alignment::Right => (pad, 0),
        Alignment::Center => (pad / 2, pad - pad / 2),
    };
    let mut out = Vec::new();
    if left > 0 {
        push_span(&mut out, " ".repeat(left), base);
    }
    out.append(&mut clipped);
    if right > 0 {
        push_span(&mut out, " ".repeat(right), base);
    }
    out
}

#[derive(Clone, Copy)]
enum Alignment {
    Left,
    Center,
    Right,
}

// ---------------------------------------------------------------------------
// block helpers
// ---------------------------------------------------------------------------

fn leading_spaces(s: &str) -> usize {
    s.chars().take_while(|c| *c == ' ').count()
}

fn heading_level(trim: &str) -> Option<usize> {
    let level = trim.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) && trim[level..].starts_with(' ') {
        Some(level)
    } else {
        None
    }
}

fn is_fence(trim: &str) -> Option<(char, usize, String)> {
    let ch = trim.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let count = trim.chars().take_while(|c| *c == ch).count();
    if count < 3 {
        return None;
    }
    let info = trim[count..].trim().to_string();
    Some((ch, count, info))
}

fn is_hr(trim: &str) -> bool {
    let mut chars = trim.chars().filter(|c| !c.is_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }
    let mut n = 1;
    for c in chars {
        if c != first {
            return false;
        }
        n += 1;
    }
    n >= 3
}

fn is_table_sep(trim: &str) -> bool {
    if !trim.starts_with('|') || !trim.ends_with('|') {
        return false;
    }
    let inner = trim.trim_matches('|');
    let mut any = false;
    for cell in inner.split('|') {
        let c = cell.trim();
        if c.is_empty() {
            return false;
        }
        let c = c.trim_start_matches(':').trim_end_matches(':');
        if c.len() < 3 || !c.chars().all(|ch| ch == '-') {
            return false;
        }
        any = true;
    }
    any
}

fn is_table_start(lines: &[String], i: usize) -> bool {
    if i + 1 >= lines.len() {
        return false;
    }
    let a = lines[i].trim();
    let b = lines[i + 1].trim();
    a.starts_with('|')
        && a.ends_with('|')
        && b.starts_with('|')
        && b.ends_with('|')
        && is_table_sep(b)
}

fn split_row(s: &str) -> Vec<String> {
    s.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

fn table_align(sep: &str) -> Vec<Alignment> {
    split_row(sep)
        .iter()
        .map(|c| {
            let c = c.trim();
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            match (left, right) {
                (true, true) => Alignment::Center,
                (false, true) => Alignment::Right,
                _ => Alignment::Left,
            }
        })
        .collect()
}

fn is_block_start(lines: &[String], i: usize) -> bool {
    let raw = &lines[i];
    let trim = raw.trim_start();
    trim.is_empty()
        || heading_level(trim).is_some()
        || is_fence(trim).is_some()
        || is_hr(trim)
        || trim.starts_with('>')
        || trim.starts_with("<!--")
        || is_table_start(lines, i)
        || list_marker(trim).is_some()
}

fn list_marker(trim: &str) -> Option<(usize, bool)> {
    if let Some(rest) = trim.strip_prefix("- ") {
        let _ = rest;
        return Some((2, false));
    }
    if trim.starts_with("* ") || trim.starts_with("+ ") {
        return Some((2, false));
    }
    let digits = trim.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && trim[digits..].starts_with(". ") {
        return Some((digits + 2, true));
    }
    None
}

fn truncate_plain(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0usize;
    let mut truncated = false;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw > max_width {
            truncated = true;
            break;
        }
        out.push(ch);
        w += cw;
    }
    if truncated && w + 1 <= max_width {
        out.push('…');
    }
    out
}

// ---------------------------------------------------------------------------
// block rendering
// ---------------------------------------------------------------------------

pub fn render(
    lines: &[String],
    width: usize,
    theme: &Theme,
    options: RenderOptions,
) -> Vec<DisplayLine> {
    let width = width.max(12);
    let body_style = Style::default().fg(theme.body);
    let border_style = Style::default().fg(theme.border);
    let mut out: Vec<DisplayLine> = Vec::new();
    let mut i = 0usize;

    // frontmatter：折叠成一行提示，但仍保留 source_line=0，
    // 这样在这一行按 i 仍然能进入源码编辑元数据。
    if !lines.is_empty() && lines[0].trim() == "---" {
        if let Some(end) = (1..lines.len()).find(|&j| lines[j].trim() == "---") {
            push_line(
                &mut out,
                vec![rs(
                    format!("▸ frontmatter · {} 行", end.saturating_sub(1)),
                    Style::default()
                        .fg(theme.muted)
                        .add_modifier(Modifier::ITALIC),
                )],
                0,
                0,
            );
            push_blank(&mut out, 0);
            i = end + 1;
        }
    }

    while i < lines.len() {
        let raw = &lines[i];
        let trim = raw.trim_start();

        if trim.is_empty() {
            push_blank(&mut out, i);
            i += 1;
            continue;
        }

        // HTML 注释（块）
        if trim.starts_with("<!--") {
            let mut j = i;
            let mut done = false;
            while j < lines.len() {
                if lines[j].contains("-->") {
                    done = true;
                    break;
                }
                j += 1;
            }
            if done {
                i = j + 1;
            } else {
                break;
            }
            continue;
        }

        // 围栏代码块
        if let Some((ch, count, info)) = is_fence(trim) {
            let mut end = lines.len();
            for j in (i + 1)..lines.len() {
                let t = lines[j].trim();
                let n = t.chars().take_while(|c| *c == ch).count();
                if n >= count && t[n..].trim().is_empty() {
                    end = j;
                    break;
                }
            }
            push_blank(&mut out, i);
            render_code_block(&mut out, lines, i, end, &info, width, theme);
            i = if end < lines.len() { end + 1 } else { end };
            continue;
        }

        // 表格
        if is_table_start(lines, i) {
            push_blank(&mut out, i);
            i = render_table(&mut out, lines, i, width, theme);
            continue;
        }

        // 分隔线
        if is_hr(trim) {
            push_blank(&mut out, i);
            let rule = "─".repeat(width);
            push_line(&mut out, vec![rs(rule, border_style)], i, 0);
            i += 1;
            continue;
        }

        // 标题
        if let Some(level) = heading_level(trim) {
            push_blank(&mut out, i);
            let lead = leading_spaces(raw);
            let after = &trim[level..];
            let spaces = after.chars().take_while(|c| *c == ' ').count();
            let mut text = after.trim();
            // 去掉 ATX 结尾的 #
            if text.ends_with('#') {
                let t = text.trim_end_matches('#').trim_end();
                if !t.is_empty() {
                    text = t;
                }
            }
            let (style, prefix, rule_color) = match level {
                1 => (
                    Style::default().fg(theme.h1).add_modifier(Modifier::BOLD),
                    "",
                    theme.h1,
                ),
                2 => (
                    Style::default().fg(theme.h2).add_modifier(Modifier::BOLD),
                    "▌ ",
                    theme.h2,
                ),
                3 => (
                    Style::default().fg(theme.h3).add_modifier(Modifier::BOLD),
                    "▎ ",
                    theme.h3,
                ),
                _ => (
                    Style::default().fg(theme.body).add_modifier(Modifier::BOLD),
                    "· ",
                    theme.border,
                ),
            };
            let prefix_spans = if prefix.is_empty() {
                Vec::new()
            } else {
                vec![rs(prefix, Style::default().fg(rule_color))]
            };
            let prefix_w = width_of_spans(&prefix_spans);
            let content = inline(text, style, theme);
            let source_col = lead + level + spaces;

            let scale = match level {
                1 => options.h1_scale,
                2 => options.h2_scale,
                3 => options.h3_scale,
                _ => 1,
            }
            .max(1) as usize;
            if scale > 1 {
                let plain: String = content
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<String>()
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect();
                let plain = truncate_plain(plain.trim(), width / scale);
                if !plain.is_empty() {
                    let scaled_w = UnicodeWidthStr::width(plain.as_str()) * scale;
                    out.push(DisplayLine {
                        spans: Vec::new(),
                        source_line: i,
                        source_col,
                        scaled: Some(ScaledHeading {
                            text: plain.clone(),
                            scale: scale as u16,
                            level,
                        }),
                    });
                    for _ in 1..scale {
                        push_line(&mut out, Vec::new(), i, source_col);
                    }
                    if theme.heading_rules && level <= 2 {
                        let rule_w = if level == 1 {
                            width
                        } else {
                            scaled_w.min(width).max(3)
                        };
                        if rule_w > 0 {
                            let glyph = if level == 1 { "━" } else { "─" };
                            push_line(
                                &mut out,
                                vec![rs(glyph.repeat(rule_w), Style::default().fg(rule_color))],
                                i,
                                source_col,
                            );
                        }
                    }
                    push_blank(&mut out, i);
                    i += 1;
                    continue;
                }
            }

            wrap_rich(
                &mut out,
                content,
                prefix_spans,
                vec![rs(" ".repeat(prefix_w), Style::default())],
                width,
                i,
                source_col,
            );
            if theme.heading_rules && level <= 2 {
                let rule_w = if level == 1 {
                    width
                } else {
                    text.chars()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                        .sum::<usize>()
                        .min(width)
                        .max(3)
                };
                if rule_w > 0 {
                    let glyph = if level == 1 { "━" } else { "─" };
                    push_line(
                        &mut out,
                        vec![rs(glyph.repeat(rule_w), Style::default().fg(rule_color))],
                        i,
                        source_col,
                    );
                }
            }
            push_blank(&mut out, i);
            i += 1;
            continue;
        }

        // 引用块
        if trim.starts_with('>') {
            push_blank(&mut out, i);
            while i < lines.len() {
                let raw = &lines[i];
                let t = raw.trim_start();
                if !t.starts_with('>') {
                    break;
                }
                let lead = leading_spaces(raw);
                let rest = t.strip_prefix('>').unwrap_or("");
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                let style = Style::default()
                    .fg(theme.muted)
                    .add_modifier(Modifier::ITALIC);
                let content = inline(rest, style, theme);
                wrap_rich(
                    &mut out,
                    content,
                    vec![rs("│ ", Style::default().fg(theme.accent))],
                    vec![rs("  ", Style::default())],
                    width,
                    i,
                    lead + 1,
                );
                i += 1;
            }
            push_blank(&mut out, i.saturating_sub(1));
            continue;
        }

        // 列表
        if let Some((marker_len, ordered)) = list_marker(trim) {
            let lead = leading_spaces(raw);
            let level = (lead / 2).min(6);
            let indent = "  ".repeat(level);
            let content = &trim[marker_len..];
            let mut prefix = vec![rs(indent.clone(), Style::default())];
            if ordered {
                let marker = &trim[..marker_len];
                push_span(&mut prefix, marker, Style::default().fg(theme.accent));
            } else {
                push_span(&mut prefix, "• ", Style::default().fg(theme.accent));
            }

            let mut rest = content;
            if rest.starts_with("[ ] ") {
                push_span(&mut prefix, "☐ ", Style::default().fg(theme.muted));
                rest = &rest[4..];
            } else if rest.starts_with("[x] ") || rest.starts_with("[X] ") {
                push_span(&mut prefix, "☑ ", Style::default().fg(theme.code));
                rest = &rest[4..];
            }

            let prefix_w = width_of_spans(&prefix);
            let content_spans = inline(rest, body_style, theme);
            wrap_rich(
                &mut out,
                content_spans,
                prefix,
                vec![rs(" ".repeat(prefix_w), Style::default())],
                width,
                i,
                lead + marker_len,
            );
            i += 1;
            continue;
        }

        // 普通段落：连续的非空、非块起始行合并
        let start = i;
        let mut parts: Vec<String> = Vec::new();
        while i < lines.len() {
            let t = lines[i].trim();
            if t.is_empty() || (i > start && is_block_start(lines, i)) {
                break;
            }
            parts.push(t.to_string());
            i += 1;
        }
        let text = parts.join(" ");
        let content = inline(&text, body_style, theme);
        let lead = leading_spaces(&lines[start]);
        wrap_rich(
            &mut out,
            content,
            Vec::new(),
            Vec::new(),
            width,
            start,
            lead,
        );
        // 段落之间如果源码没有空行，也补一个视觉空隙
        if i < lines.len() && !lines[i].trim().is_empty() {
            push_blank(&mut out, i);
        }
    }

    out
}

fn render_code_block(
    out: &mut Vec<DisplayLine>,
    lines: &[String],
    start: usize,
    end: usize,
    info: &str,
    width: usize,
    theme: &Theme,
) {
    let border = Style::default().fg(theme.border);
    let code_style = Style::default().fg(theme.code).bg(theme.code_bg);
    let label = info.split_whitespace().next().unwrap_or("");
    let inner_w = width.saturating_sub(4).max(4);

    // 顶边：╭─ lang ─────╮
    let mut top: Vec<RichSpan> = Vec::new();
    push_span(&mut top, "╭─", border);
    if !label.is_empty() {
        push_span(&mut top, " ", border);
        push_span(&mut top, label, Style::default().fg(theme.accent));
        push_span(&mut top, " ", border);
    }
    let used = width_of_spans(&top) + 1;
    if used < width {
        push_span(&mut top, "─".repeat(width - used), border);
    }
    push_span(&mut top, "╮", border);
    push_line(out, top, start, 0);

    let last = if end < lines.len() {
        end
    } else {
        lines.len().saturating_sub(1)
    };
    for idx in (start + 1)..=last.min(lines.len().saturating_sub(1)) {
        if idx == end && end < lines.len() {
            break;
        }
        let raw = lines.get(idx).map(String::as_str).unwrap_or("");
        let expanded = raw.replace('\t', "    ");
        let chars: Vec<char> = expanded.chars().collect();
        if chars.is_empty() {
            let mut row = vec![
                rs("│ ", border),
                rs(" ".repeat(inner_w), code_style),
                rs(" │", border),
            ];
            // 合并同风格
            let mut merged = Vec::new();
            for s in row.drain(..) {
                push_span(&mut merged, s.text, s.style);
            }
            push_line(out, merged, idx, 0);
            continue;
        }
        let mut pos = 0usize;
        while pos < chars.len() {
            let mut take = String::new();
            let mut w = 0usize;
            while pos < chars.len() {
                let cw = UnicodeWidthChar::width(chars[pos]).unwrap_or(0);
                if w + cw > inner_w {
                    break;
                }
                take.push(chars[pos]);
                w += cw;
                pos += 1;
            }
            if take.is_empty() {
                // 超宽单字（极少见），强行放一个
                take.push(chars[pos]);
                pos += 1;
            }
            let mut row = vec![
                rs("│ ", border),
                rs(take, code_style),
                rs(" ".repeat(inner_w.saturating_sub(w)), code_style),
                rs(" │", border),
            ];
            let mut merged = Vec::new();
            for s in row.drain(..) {
                push_span(&mut merged, s.text, s.style);
            }
            push_line(out, merged, idx, 0);
        }
    }

    let mut bottom = vec![
        rs("╰", border),
        rs("─".repeat(width.saturating_sub(2)), border),
        rs("╯", border),
    ];
    let mut merged = Vec::new();
    for s in bottom.drain(..) {
        push_span(&mut merged, s.text, s.style);
    }
    push_line(out, merged, end.min(lines.len().saturating_sub(1)), 0);
}

fn table_border(
    widths: &[usize],
    left: &str,
    mid: &str,
    right: &str,
    style: Style,
) -> Vec<RichSpan> {
    let mut spans = vec![rs(left, style)];
    for (i, w) in widths.iter().enumerate() {
        push_span(&mut spans, "─".repeat(*w + 2), style);
        if i + 1 < widths.len() {
            push_span(&mut spans, mid, style);
        }
    }
    push_span(&mut spans, right, style);
    spans
}

fn render_table(
    out: &mut Vec<DisplayLine>,
    lines: &[String],
    start: usize,
    width: usize,
    theme: &Theme,
) -> usize {
    let header = split_row(&lines[start]);
    let sep = split_row(&lines[start + 1]);
    let aligns = table_align(&lines[start + 1]);
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let mut i = start + 2;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() || !t.starts_with('|') || !t.ends_with('|') {
            break;
        }
        rows.push((i, split_row(&lines[i])));
        i += 1;
    }

    let cols = header.len().max(sep.len()).max(1);
    let mut header = header;
    header.resize(cols, String::new());
    let mut widths = vec![3usize; cols];
    for (c, cell) in header.iter().enumerate() {
        widths[c] = widths[c].max(cell.width());
    }
    for (_, row) in &rows {
        for (c, cell) in row.iter().take(cols).enumerate() {
            widths[c] = widths[c].max(cell.width());
        }
    }

    if theme.table_borders {
        while 1 + widths.iter().map(|w| w + 3).sum::<usize>() > width {
            let mut changed = false;
            if let Some((idx, _)) = widths
                .iter()
                .enumerate()
                .filter(|(_, w)| **w > 3)
                .max_by_key(|(_, w)| **w)
            {
                widths[idx] -= 1;
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    let border = Style::default().fg(theme.border);
    let header_style = Style::default()
        .fg(theme.accent)
        .add_modifier(Modifier::BOLD);
    let body_style = Style::default().fg(theme.body);

    if theme.table_borders {
        push_line(out, table_border(&widths, "╭", "┬", "╮", border), start, 0);
        push_table_row(
            out,
            &header,
            &widths,
            &aligns,
            header_style,
            start,
            border,
            theme,
        );
        push_line(
            out,
            table_border(&widths, "├", "┼", "┤", border),
            start + 1,
            0,
        );
        for (idx, (row_idx, row)) in rows.iter().enumerate() {
            push_table_row(
                out, row, &widths, &aligns, body_style, *row_idx, border, theme,
            );
            if idx + 1 < rows.len() {
                push_line(
                    out,
                    table_border(&widths, "├", "┼", "┤", border),
                    *row_idx,
                    0,
                );
            }
        }
        push_line(
            out,
            table_border(&widths, "╰", "┴", "╯", border),
            rows.last().map(|(i, _)| *i).unwrap_or(start),
            0,
        );
    } else {
        push_table_plain(out, &header, &widths, header_style, start, theme);
        if !rows.is_empty() {
            let total = widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * 2;
            push_line(
                out,
                vec![rs("─".repeat(total), Style::default().fg(theme.border))],
                start,
                0,
            );
        }
        for (row_idx, row) in &rows {
            push_table_plain(out, row, &widths, body_style, *row_idx, theme);
        }
    }

    i
}

fn push_table_row(
    out: &mut Vec<DisplayLine>,
    cells: &[String],
    widths: &[usize],
    aligns: &[Alignment],
    style: Style,
    source: usize,
    border: Style,
    theme: &Theme,
) {
    let mut spans = vec![rs("│", border)];
    for (c, w) in widths.iter().enumerate() {
        let cell = cells.get(c).map(String::as_str).unwrap_or("");
        let content = inline(cell, style, theme);
        let fitted = fit_cell(
            &content,
            *w,
            aligns.get(c).copied().unwrap_or(Alignment::Left),
            style,
        );
        push_span(&mut spans, " ", border);
        spans.extend(fitted);
        push_span(&mut spans, " ", border);
        push_span(&mut spans, "│", border);
    }
    push_line(out, spans, source, 0);
}

fn push_table_plain(
    out: &mut Vec<DisplayLine>,
    cells: &[String],
    widths: &[usize],
    style: Style,
    source: usize,
    theme: &Theme,
) {
    let mut spans = Vec::new();
    for (c, w) in widths.iter().enumerate() {
        let cell = cells.get(c).map(String::as_str).unwrap_or("");
        let content = inline(cell, style, theme);
        let fitted = fit_cell(&content, *w, Alignment::Left, style);
        spans.extend(fitted);
        if c + 1 < widths.len() {
            push_span(&mut spans, "  ", Style::default().fg(Color::Reset));
        }
    }
    push_line(out, spans, source, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Config;

    #[test]
    fn scaled_heading_reserves_rows() {
        let theme = Config::default().theme();
        let lines = vec!["# 标题".to_string(), "正文".to_string()];
        let out = render(
            &lines,
            40,
            &theme,
            RenderOptions {
                h1_scale: 3,
                h2_scale: 2,
                h3_scale: 1,
            },
        );
        let scaled = out.iter().find(|l| l.scaled.is_some()).expect("scaled h1");
        assert_eq!(scaled.scaled.as_ref().unwrap().scale, 3);
        assert_eq!(scaled.scaled.as_ref().unwrap().text, "标题");
        // 标题文字行 + 两个补高行 + H1 规则
        assert!(out[1].spans.is_empty());
        assert!(out[2].spans.is_empty());
        assert!(out[3].spans.iter().any(|s| s.content.contains('━')));
    }
}
