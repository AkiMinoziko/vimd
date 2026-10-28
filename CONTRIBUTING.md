# Contributing

Thanks for trying `vimd`.

## Development

Requirements:

- Rust stable (edition 2024)
- Linux terminal (tested; the helper scripts use GNU `readlink`)
- `tmux` for the screen-level test

```bash
git clone https://github.com/AkiMinoziko/vimd.git
cd vimd
cargo build --release
cargo test
./test/capture.sh
```

`test/capture.sh` 会在独立的 tmux server 里启动 TUI，模拟按键并检查屏幕文本。
它不会碰你的真实 `~/.config/vimd`。

## Pull requests

- Keep the dependency set small; vimd is deliberately a small TUI.
- Run `cargo fmt` before opening a PR.
- Run `cargo test` and, if you changed the UI, `./test/capture.sh`.
- For Kitty-related changes, mention whether it was tested in a direct Kitty
  window, not only under tmux (tmux does not forward OSC 66).
- Update README/limits if behavior changes.

## Reporting terminal issues

Please include:

- terminal emulator + version
- `TERM`, `KITTY_WINDOW_ID`, and whether you are inside tmux/zellij/screen
- a small Markdown sample that reproduces the layout issue
- a screenshot if possible

## License

By contributing, you agree that your changes are licensed under the MIT
License in [LICENSE](LICENSE).
