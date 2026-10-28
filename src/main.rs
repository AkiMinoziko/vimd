// vimd —— 独立 Rust TUI。
//
// 用法：
//   vimd 文件.md
//   vimd
//   vimd --help
//
// 阅读态是渲染后的 Markdown；按 i 进入源码编辑，Esc 回到阅读。
// 不依赖 Vim/Neovim，编辑行为由 app.rs 自己实现。

mod app;
mod editor;
mod kitty;
mod markdown;
mod theme;
mod ui;

use std::env;
use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event};

use app::App;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = r#"vimd {version} —— 终端 Markdown 阅读 / 编辑工具

用法:
  vimd 文件.md     打开 Markdown 文件
  vimd             空缓冲区
  vimd --help      显示帮助
  vimd --version   显示版本
  vimd --color     强制启用颜色（默认）
  vimd --no-color  单色输出

阅读态:
  j/k, Ctrl-D/Ctrl-U, g/G    移动
  i/a/I/A, o/O               进入编辑
  :                          命令（:w、:q、:wq）
  /                          搜索（n/N 下一个/上一个）
  F2 或 :settings            设置页
  ?                          快捷键帮助

编辑态:
  直接输入；Enter 换行；Esc 回阅读；Ctrl-S 保存

  :w / :q / :wq 等命令都可用。

说明: 终端是固定单元格网格，无法逐行设置字号。
vimd 用颜色、字重和标题装饰线来区分层级。
"#;

fn print_help() {
    print!("{}", HELP.replace("{version}", VERSION));
}

fn main() -> io::Result<()> {
    let mut path: Option<PathBuf> = None;
    let mut force_color: Option<bool> = None;
    let mut end_options = false;

    for arg in env::args().skip(1) {
        if !end_options && arg == "--" {
            end_options = true;
            continue;
        }
        if !end_options {
            match arg.as_str() {
                "--help" | "-h" => {
                    print_help();
                    return Ok(());
                }
                "--version" | "-V" => {
                    println!("vimd {VERSION}");
                    return Ok(());
                }
                "--color" => {
                    force_color = Some(true);
                    continue;
                }
                "--no-color" => {
                    force_color = Some(false);
                    continue;
                }
                _ => {}
            }
        }
        if path.is_none() {
            path = Some(PathBuf::from(arg));
        }
    }

    // vimd 是阅读器，颜色是核心排版手段；默认强制开启。
    // 需要遵守 NO_COLOR 或使用单色终端时传 --no-color。
    let color_enabled = force_color.unwrap_or(true);
    crossterm::style::force_color_output(color_enabled);

    let mut app = App::new(path, color_enabled);

    ratatui::run(|terminal| {
        loop {
            if app.scaled_dirty {
                ui::clear_scaled_headings(&mut app)?;
                app.scaled_dirty = false;
            }
            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            ui::paint_scaled_headings(&mut app)?;
            if app.should_quit {
                break;
            }
            match event::read()? {
                Event::Key(key) => app.handle_key(key),
                Event::Paste(text) => app.handle_paste(&text),
                Event::Resize(_, _) => app.scaled_dirty = true,
                _ => {}
            }
        }
        Ok(())
    })
}
