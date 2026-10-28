#!/usr/bin/env bash
# vimd-rs 的无头验收。
#
# 在独立 tmux server 里启动真实 TUI，检查：
#   阅读态隐藏标记、表格/代码框、插入态源码+行号、Esc 回阅读、
#   设置页说明、:wq 写回文件。
#
# 测试使用临时 XDG_CONFIG_HOME，不会碰用户真实配置。

set -euo pipefail

SELF="$(readlink -f -- "${BASH_SOURCE[0]}")"
ROOT="$(cd -- "$(dirname -- "$SELF")/.." && pwd)"
SAMPLE="$ROOT/test/sample.md"
SOCK="vimd-rs-test-$$"
TMP="$(mktemp -d /tmp/vimd-rs-test.XXXXXX)"
WORK="$TMP/sample.md"
CFG="$TMP/config"

cleanup() {
  tmux -L "$SOCK" kill-server >/dev/null 2>&1 || true
  rm -rf -- "$TMP"
}
trap cleanup EXIT

if ! command -v tmux >/dev/null 2>&1; then
  printf 'SKIP: 需要 tmux 才能做屏幕验收。\n'
  exit 0
fi
BIN="$ROOT/bin/vimd"
if [ ! -x "$BIN" ]; then
  if ! command -v cargo >/dev/null 2>&1; then
    printf 'FAIL: 找不到 bin/vimd，也没有 cargo 可以编译。\n' >&2
    exit 1
  fi
  printf '编译 release 版本...\n'
  (cd -- "$ROOT" && cargo build --release >/dev/null)
  BIN="$ROOT/target/release/vimd"
fi

cp -- "$SAMPLE" "$WORK"
START_CMD="$(printf 'XDG_CONFIG_HOME=%q %q --color %q' "$CFG" "$BIN" "$WORK")"
tmux -L "$SOCK" -f /dev/null new-session -d -s v -x 110 -y 34 "$START_CMD"

capture() {
  tmux -L "$SOCK" capture-pane -p -t v > "$1"
}

send() {
  tmux -L "$SOCK" send-keys -t v "$@"
}

wait_for() {
  local needle="$1"
  local i
  for i in $(seq 1 50); do
    if tmux -L "$SOCK" capture-pane -p -t v 2>/dev/null | grep -Fq -- "$needle"; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  if [ -n "${2-}" ] && [ -f "$2" ]; then
    printf '\n--- 当前屏幕 ---\n' >&2
    sed -n '1,45p' "$2" >&2
  fi
  exit 1
}

has() {
  grep -Fq -- "$2" "$1" || fail "期望出现: $2" "$1"
}

not_has() {
  if grep -Fq -- "$2" "$1"; then
    fail "期望不出现: $2" "$1"
  fi
}

wait_for 'Vimd 演示' || fail 'vimd 没有在 5 秒内显示样例标题'

# ---------- 1. 阅读态 ----------
capture "$TMP/normal.txt"
has "$TMP/normal.txt" ' vimd '
has "$TMP/normal.txt" 'NORMAL'
has "$TMP/normal.txt" '▸ frontmatter'
has "$TMP/normal.txt" 'Vimd 演示'
not_has "$TMP/normal.txt" '# Vimd 演示'
not_has "$TMP/normal.txt" '**粗体**'
not_has "$TMP/normal.txt" '`行内代码`'
not_has "$TMP/normal.txt" 'https://example.com'
not_has "$TMP/normal.txt" '| 能力 |'

# 滚到底看表格和代码框
send G
sleep 0.3
capture "$TMP/bottom.txt"
has "$TMP/bottom.txt" '╭─ rust'
has "$TMP/bottom.txt" '能力'
has "$TMP/bottom.txt" '╭──────────┬'
not_has "$TMP/bottom.txt" '| 能力 |'

# ---------- 2. 插入态 ----------
send g
send g
sleep 0.1
send j
send j
sleep 0.2
send i
sleep 0.3
capture "$TMP/insert.txt"
has "$TMP/insert.txt" 'INSERT'
has "$TMP/insert.txt" '# Vimd 演示'
has "$TMP/insert.txt" '**粗体**'
has "$TMP/insert.txt" '`行内代码`'
has "$TMP/insert.txt" '[链接](https://example.com)'
grep -Fq '5 │ # Vimd 演示' "$TMP/insert.txt" || fail '插入态没有显示第 5 行行号' "$TMP/insert.txt"

# ---------- 3. Esc 回阅读态 ----------
send Escape
sleep 0.3
capture "$TMP/back.txt"
has "$TMP/back.txt" 'Vimd 演示'
not_has "$TMP/back.txt" '# Vimd 演示'
not_has "$TMP/back.txt" '**粗体**'

# ---------- 4. 设置页 ----------
send F2
sleep 0.3
capture "$TMP/settings.txt"
has "$TMP/settings.txt" '设置'
has "$TMP/settings.txt" '主题预设'
has "$TMP/settings.txt" 'H1 颜色'
has "$TMP/settings.txt" '标题加大'
has "$TMP/settings.txt" 'H1 字号'
has "$TMP/settings.txt" '不可用'
send Escape
sleep 0.3

# ---------- 5. 编辑并 :wq ----------
send i
sleep 0.2
send -l 'X'
send Escape
sleep 0.3
send ':wq'
send Enter
sleep 0.7

if ! grep -Fq '# XVimd' "$WORK"; then
  fail ':wq 没有把编辑内容写回文件'
fi

printf 'PASS: 阅读态 / 表格 / 插入态 / Esc / 设置页 / :wq 全部通过\n'
