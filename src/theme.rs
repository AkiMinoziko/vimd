// vimd 的配色与配置。
//
// 终端没有“逐行字号”这个概念：每个单元格尺寸固定。这里用颜色、字重、
// 标题下的装饰线来建立层级。设置页也明确显示这一点，不伪造字号选项。
//
// 配置保存到 ~/.config/vimd/config.toml，格式很简单，手写解析，避免为了
// 一个配置文件引入 serde/toml 依赖。

use ratatui::style::Color;
use std::env;
use std::fs;
use std::path::PathBuf;

/// 设置页里可循环的命名颜色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorName {
    Default,
    White,
    Silver,
    Gray,
    Charcoal,
    Ink,
    Red,
    Orange,
    Amber,
    Yellow,
    Green,
    Mint,
    Forest,
    Teal,
    Cyan,
    Sky,
    Blue,
    Navy,
    Purple,
    Violet,
    Magenta,
    Pink,
    Rust,
}

impl ColorName {
    pub const ALL: [ColorName; 23] = [
        Self::Default,
        Self::White,
        Self::Silver,
        Self::Gray,
        Self::Charcoal,
        Self::Ink,
        Self::Red,
        Self::Orange,
        Self::Amber,
        Self::Yellow,
        Self::Green,
        Self::Mint,
        Self::Forest,
        Self::Teal,
        Self::Cyan,
        Self::Sky,
        Self::Blue,
        Self::Navy,
        Self::Purple,
        Self::Violet,
        Self::Magenta,
        Self::Pink,
        Self::Rust,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::White => "white",
            Self::Silver => "silver",
            Self::Gray => "gray",
            Self::Charcoal => "charcoal",
            Self::Ink => "ink",
            Self::Red => "red",
            Self::Orange => "orange",
            Self::Amber => "amber",
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Mint => "mint",
            Self::Forest => "forest",
            Self::Teal => "teal",
            Self::Cyan => "cyan",
            Self::Sky => "sky",
            Self::Blue => "blue",
            Self::Navy => "navy",
            Self::Purple => "purple",
            Self::Violet => "violet",
            Self::Magenta => "magenta",
            Self::Pink => "pink",
            Self::Rust => "rust",
        }
    }

    pub fn parse(s: &str) -> Self {
        let s = s.trim().trim_matches('"');
        Self::ALL
            .into_iter()
            .find(|c| c.label().eq_ignore_ascii_case(s))
            .unwrap_or(Self::Default)
    }

    pub fn color(self) -> Color {
        match self {
            Self::Default => Color::Reset,
            Self::White => Color::Rgb(230, 233, 239),
            Self::Silver => Color::Rgb(180, 188, 200),
            Self::Gray => Color::Rgb(130, 140, 155),
            Self::Charcoal => Color::Rgb(80, 90, 105),
            Self::Ink => Color::Rgb(35, 38, 44),
            Self::Red => Color::Rgb(240, 110, 120),
            Self::Orange => Color::Rgb(245, 150, 90),
            Self::Amber => Color::Rgb(245, 200, 100),
            Self::Yellow => Color::Rgb(240, 225, 120),
            Self::Green => Color::Rgb(140, 210, 140),
            Self::Mint => Color::Rgb(130, 230, 190),
            Self::Forest => Color::Rgb(40, 120, 80),
            Self::Teal => Color::Rgb(100, 210, 210),
            Self::Cyan => Color::Rgb(125, 225, 245),
            Self::Sky => Color::Rgb(130, 190, 255),
            Self::Blue => Color::Rgb(120, 150, 250),
            Self::Navy => Color::Rgb(50, 80, 160),
            Self::Purple => Color::Rgb(190, 150, 250),
            Self::Violet => Color::Rgb(215, 160, 240),
            Self::Magenta => Color::Rgb(240, 140, 210),
            Self::Pink => Color::Rgb(250, 170, 190),
            Self::Rust => Color::Rgb(170, 80, 30),
        }
    }

    pub fn next(self, dir: i32) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|c| *c == self).unwrap_or(0) as i32;
        let len = all.len() as i32;
        all[((idx + dir).rem_euclid(len)) as usize]
    }
}

/// 可单独配置的 Markdown 角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    H1,
    H2,
    H3,
    Body,
    Accent,
    Code,
    Border,
}

impl Role {
    pub const ALL: [Role; 7] = [
        Self::H1,
        Self::H2,
        Self::H3,
        Self::Body,
        Self::Accent,
        Self::Code,
        Self::Border,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::H1 => "H1 颜色",
            Self::H2 => "H2 颜色",
            Self::H3 => "H3 颜色",
            Self::Body => "正文颜色",
            Self::Accent => "强调色",
            Self::Code => "代码色",
            Self::Border => "边框色",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub label: &'static str,
    pub bg: Color,
    pub surface: Color,
    pub h1: ColorName,
    pub h2: ColorName,
    pub h3: ColorName,
    pub body: ColorName,
    pub accent: ColorName,
    pub code: ColorName,
    pub border: ColorName,
    pub code_bg: Color,
    pub focus_bg: Color,
    pub muted: Color,
}

pub const PRESETS: [Preset; 4] = [
    Preset {
        name: "dsh-dark",
        label: "Dark",
        bg: Color::Rgb(13, 17, 23),
        surface: Color::Rgb(22, 27, 35),
        h1: ColorName::Cyan,
        h2: ColorName::Sky,
        h3: ColorName::Purple,
        body: ColorName::White,
        accent: ColorName::Amber,
        code: ColorName::Mint,
        border: ColorName::Charcoal,
        code_bg: Color::Rgb(30, 37, 47),
        focus_bg: Color::Rgb(28, 36, 48),
        muted: Color::Rgb(120, 132, 148),
    },
    Preset {
        name: "paper",
        label: "Paper",
        bg: Color::Rgb(247, 245, 240),
        surface: Color::Rgb(238, 235, 228),
        h1: ColorName::Navy,
        h2: ColorName::Blue,
        h3: ColorName::Purple,
        body: ColorName::Ink,
        accent: ColorName::Rust,
        code: ColorName::Forest,
        border: ColorName::Silver,
        code_bg: Color::Rgb(232, 229, 221),
        focus_bg: Color::Rgb(235, 231, 222),
        muted: Color::Rgb(115, 115, 110),
    },
    Preset {
        name: "solarized",
        label: "Solarized",
        bg: Color::Rgb(0, 43, 54),
        surface: Color::Rgb(7, 54, 66),
        h1: ColorName::Cyan,
        h2: ColorName::Sky,
        h3: ColorName::Violet,
        body: ColorName::Silver,
        accent: ColorName::Amber,
        code: ColorName::Green,
        border: ColorName::Charcoal,
        code_bg: Color::Rgb(0, 43, 54),
        focus_bg: Color::Rgb(7, 54, 66),
        muted: Color::Rgb(110, 125, 130),
    },
    Preset {
        name: "terminal",
        label: "Terminal",
        bg: Color::Reset,
        surface: Color::Reset,
        h1: ColorName::Cyan,
        h2: ColorName::Sky,
        h3: ColorName::Purple,
        body: ColorName::Default,
        accent: ColorName::Amber,
        code: ColorName::Mint,
        border: ColorName::Charcoal,
        code_bg: Color::Reset,
        focus_bg: Color::Rgb(35, 42, 54),
        muted: Color::Rgb(130, 140, 155),
    },
];

pub fn preset_by_name(name: &str) -> Preset {
    PRESETS
        .into_iter()
        .find(|p| p.name == name || p.label.eq_ignore_ascii_case(name))
        .unwrap_or(PRESETS[0])
}

pub fn preset_next(current: &str, dir: i32) -> Preset {
    let idx = PRESETS.iter().position(|p| p.name == current).unwrap_or(0) as i32;
    let len = PRESETS.len() as i32;
    PRESETS[((idx + dir).rem_euclid(len)) as usize]
}

#[derive(Clone, Debug)]
pub struct Config {
    pub preset: String,
    pub h1: String,
    pub h2: String,
    pub h3: String,
    pub body: String,
    pub accent: String,
    pub code: String,
    pub border: String,
    pub heading_rules: bool,
    pub table_borders: bool,
    pub line_numbers: bool,
    pub focus_line: bool,
    pub text_sizing: bool,
    pub h1_scale: u16,
    pub h2_scale: u16,
    pub h3_scale: u16,
}

impl Default for Config {
    fn default() -> Self {
        let p = PRESETS[0];
        Self {
            preset: p.name.to_string(),
            h1: p.h1.label().to_string(),
            h2: p.h2.label().to_string(),
            h3: p.h3.label().to_string(),
            body: p.body.label().to_string(),
            accent: p.accent.label().to_string(),
            code: p.code.label().to_string(),
            border: p.border.label().to_string(),
            heading_rules: true,
            table_borders: true,
            line_numbers: true,
            focus_line: true,
            text_sizing: true,
            h1_scale: 3,
            h2_scale: 2,
            h3_scale: 1,
        }
    }
}

impl Config {
    pub fn apply_preset(&mut self, name: &str) {
        let p = preset_by_name(name);
        self.preset = p.name.to_string();
        self.h1 = p.h1.label().to_string();
        self.h2 = p.h2.label().to_string();
        self.h3 = p.h3.label().to_string();
        self.body = p.body.label().to_string();
        self.accent = p.accent.label().to_string();
        self.code = p.code.label().to_string();
        self.border = p.border.label().to_string();
    }

    pub fn role_name(&self, role: Role) -> &str {
        match role {
            Role::H1 => &self.h1,
            Role::H2 => &self.h2,
            Role::H3 => &self.h3,
            Role::Body => &self.body,
            Role::Accent => &self.accent,
            Role::Code => &self.code,
            Role::Border => &self.border,
        }
    }

    pub fn set_role_name(&mut self, role: Role, name: &str) {
        let name = ColorName::parse(name).label().to_string();
        match role {
            Role::H1 => self.h1 = name,
            Role::H2 => self.h2 = name,
            Role::H3 => self.h3 = name,
            Role::Body => self.body = name,
            Role::Accent => self.accent = name,
            Role::Code => self.code = name,
            Role::Border => self.border = name,
        }
    }

    pub fn cycle_role(&mut self, role: Role, dir: i32) {
        let name = ColorName::parse(self.role_name(role)).next(dir);
        self.set_role_name(role, name.label());
    }

    pub fn role_color(&self, role: Role) -> Color {
        ColorName::parse(self.role_name(role)).color()
    }

    pub fn theme(&self) -> Theme {
        let p = preset_by_name(&self.preset);
        Theme {
            bg: p.bg,
            surface: p.surface,
            body: self.role_color(Role::Body),
            muted: p.muted,
            border: self.role_color(Role::Border),
            h1: self.role_color(Role::H1),
            h2: self.role_color(Role::H2),
            h3: self.role_color(Role::H3),
            accent: self.role_color(Role::Accent),
            code: self.role_color(Role::Code),
            code_bg: p.code_bg,
            focus_bg: p.focus_bg,
            heading_rules: self.heading_rules,
            table_borders: self.table_borders,
            line_numbers: self.line_numbers,
            focus_line: self.focus_line,
        }
    }

    pub fn config_path() -> Option<PathBuf> {
        if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
            if !xdg.trim().is_empty() {
                return Some(PathBuf::from(xdg).join("vimd").join("config.toml"));
            }
        }
        env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("vimd")
                .join("config.toml")
        })
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        let mut cfg = Self::default();
        let mut seen = [false; 7];
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key {
                "preset" => cfg.preset = value.to_string(),
                "h1" => {
                    cfg.h1 = value.to_string();
                    seen[0] = true;
                }
                "h2" => {
                    cfg.h2 = value.to_string();
                    seen[1] = true;
                }
                "h3" => {
                    cfg.h3 = value.to_string();
                    seen[2] = true;
                }
                "body" => {
                    cfg.body = value.to_string();
                    seen[3] = true;
                }
                "accent" => {
                    cfg.accent = value.to_string();
                    seen[4] = true;
                }
                "code" => {
                    cfg.code = value.to_string();
                    seen[5] = true;
                }
                "border" => {
                    cfg.border = value.to_string();
                    seen[6] = true;
                }
                "heading_rules" => cfg.heading_rules = value == "true",
                "table_borders" => cfg.table_borders = value == "true",
                "line_numbers" => cfg.line_numbers = value == "true",
                "focus_line" => cfg.focus_line = value == "true",
                "text_sizing" => cfg.text_sizing = value == "true",
                "h1_scale" => cfg.h1_scale = value.parse().unwrap_or(3),
                "h2_scale" => cfg.h2_scale = value.parse().unwrap_or(2),
                "h3_scale" => cfg.h3_scale = value.parse().unwrap_or(1),
                _ => {}
            }
        }
        // 允许配置里只写 preset，其余角色跟随预设。
        let p = preset_by_name(&cfg.preset);
        cfg.preset = p.name.to_string();
        if !seen[0] {
            cfg.h1 = p.h1.label().to_string();
        }
        if !seen[1] {
            cfg.h2 = p.h2.label().to_string();
        }
        if !seen[2] {
            cfg.h3 = p.h3.label().to_string();
        }
        if !seen[3] {
            cfg.body = p.body.label().to_string();
        }
        if !seen[4] {
            cfg.accent = p.accent.label().to_string();
        }
        if !seen[5] {
            cfg.code = p.code.label().to_string();
        }
        if !seen[6] {
            cfg.border = p.border.label().to_string();
        }
        cfg
    }

    pub fn save(&self) -> std::io::Result<PathBuf> {
        let Some(path) = Self::config_path() else {
            return Err(std::io::Error::other("找不到 HOME / XDG_CONFIG_HOME"));
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = format!(
            "# vimd config\n\
             # 主题、颜色与标题字号。\n\
             preset = \"{}\"\n\
             h1 = \"{}\"\n\
             h2 = \"{}\"\n\
             h3 = \"{}\"\n\
             body = \"{}\"\n\
             accent = \"{}\"\n\
             code = \"{}\"\n\
             border = \"{}\"\n\
             heading_rules = {}\n\
             table_borders = {}\n\
             line_numbers = {}\n\
             focus_line = {}\n\
             text_sizing = {}\n\
             h1_scale = {}\n\
             h2_scale = {}\n\
             h3_scale = {}\n",
            self.preset,
            self.h1,
            self.h2,
            self.h3,
            self.body,
            self.accent,
            self.code,
            self.border,
            self.heading_rules,
            self.table_borders,
            self.line_numbers,
            self.focus_line,
            self.text_sizing,
            self.h1_scale,
            self.h2_scale,
            self.h3_scale
        );
        fs::write(&path, text)?;
        Ok(path)
    }
}

/// 渲染时实际使用的主题。
#[derive(Clone, Debug)]
pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub body: Color,
    pub muted: Color,
    pub border: Color,
    pub h1: Color,
    pub h2: Color,
    pub h3: Color,
    pub accent: Color,
    pub code: Color,
    pub code_bg: Color,
    pub focus_bg: Color,
    pub heading_rules: bool,
    pub table_borders: bool,
    pub line_numbers: bool,
    pub focus_line: bool,
}
