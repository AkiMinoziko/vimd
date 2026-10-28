// Kitty 文本缩放（OSC 66）能力检测。
//
// Kitty 0.40+ 支持 `OSC 66` 文本缩放协议，可以把 H1/H2 真正画成多行大字。
// 这不是标准终端能力：tmux 不转发 OSC 66，其他终端也可能忽略它。
//
// 这里用“环境 + 版本”而不是 CPR 探测：
//   * 必须在 Kitty 里（KITTY_WINDOW_ID 存在）
//   * 不能在 tmux/screen 里（它们不会转发多 cell 文本）
//   * kitty --version >= 0.40.0
// 这样足够可靠，而且不会在非 Kitty 终端里发出未知转义序列。
//
// 如果将来需要更通用的探测，可以按 Kitty 文档发送 CPR 查询；
// 但当前目标终端就是 Kitty，版本检测更轻、更稳。

use std::env;
use std::process::Command;

#[derive(Clone, Debug)]
pub struct TextSizingSupport {
    pub supported: bool,
    pub reason: String,
}

impl TextSizingSupport {
    fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            supported: false,
            reason: reason.into(),
        }
    }
}

pub fn detect_text_sizing() -> TextSizingSupport {
    if env::var_os("KITTY_WINDOW_ID").is_none() {
        return TextSizingSupport::unsupported("未检测到 Kitty");
    }
    if env::var_os("TMUX").is_some()
        || env::var_os("ZELLIJ").is_some()
        || env::var_os("STY").is_some()
    {
        return TextSizingSupport::unsupported("tmux / zellij / screen 不转发 Kitty 文本缩放");
    }

    let output = Command::new("kitty")
        .arg("--version")
        .output()
        .or_else(|_| Command::new("/usr/bin/kitty").arg("--version").output());

    let Ok(output) = output else {
        return TextSizingSupport::unsupported("找不到 kitty 命令");
    };
    if !output.status.success() {
        return TextSizingSupport::unsupported("kitty --version 执行失败");
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let Some(version) = parse_version(&text) else {
        return TextSizingSupport::unsupported("无法解析 Kitty 版本");
    };

    if version >= (0, 40, 0) {
        TextSizingSupport {
            supported: true,
            reason: format!(
                "Kitty {}.{}.{} 支持 OSC 66",
                version.0, version.1, version.2
            ),
        }
    } else {
        TextSizingSupport::unsupported(format!(
            "Kitty {}.{}.{} 太旧，需要 0.40+",
            version.0, version.1, version.2
        ))
    }
}

fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let token = text
        .split_whitespace()
        .find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let mut parts = token.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    Some((major, minor, patch))
}
