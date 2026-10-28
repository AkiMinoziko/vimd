#!/usr/bin/env bash
# 安装独立 Rust 版 vimd。
#
# 用法：
#   ./install.sh                 有 bin/vimd 就用它，否则先编译 release
#   ./install.sh /some/bin       安装到指定目录
#   VIMD_REBUILD=1 ./install.sh  强制从源码编译 release 再安装
#   ./install.sh --uninstall     删除 ~/.local/bin/vimd
#
# 只复制可执行文件，不修改 shell 配置，不需要 root。
# 如果已有旧的 vimd（例如 Vim 配置版软链接），会先替换它。

set -euo pipefail

SELF="$(readlink -f -- "${BASH_SOURCE[0]}")"
ROOT="$(cd -- "$(dirname -- "$SELF")" && pwd)"
DEST="${1:-$HOME/.local/bin}"

if [ "${1-}" = "--uninstall" ]; then
  rm -f -- "$HOME/.local/bin/vimd"
  printf '已移除 %s\n' "$HOME/.local/bin/vimd"
  exit 0
fi

BIN="$ROOT/bin/vimd"
if [ "${VIMD_REBUILD:-0}" = "1" ] || [ ! -x "$BIN" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    printf 'install.sh: 找不到 cargo，请先安装 Rust 工具链。\n' >&2
    exit 127
  fi
  printf '编译 release...\n'
  (cd -- "$ROOT" && cargo build --release)
  BIN="$ROOT/target/release/vimd"
fi

if [ ! -f "$BIN" ]; then
  printf 'install.sh: 可执行文件不存在: %s\n' "$BIN" >&2
  exit 1
fi

mkdir -p -- "$DEST"
rm -f -- "$DEST/vimd"
cp -- "$BIN" "$DEST/vimd"
chmod 755 -- "$DEST/vimd"

printf '已安装: %s\n' "$DEST/vimd"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *)
    printf '\n注意: %s 不在当前 PATH 中。\n' "$DEST"
    printf '把下面一行加入 ~/.bashrc 或 ~/.zshrc：\n'
    printf '  export PATH="%s:$PATH"\n' "$DEST"
    ;;
esac
