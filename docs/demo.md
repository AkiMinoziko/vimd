# vimd

像用 Vim 一样读 Markdown。

## 阅读，也可以很漂亮

`vimd 文件.md` 打开就是排版后的视图。标题、列表、引用、代码块和表格，
都在终端里排好。

> 按 `i` 回到源码，按 `Esc` 回到阅读，`:wq` 直接写回文件。

### 代码也可以有边界

```rust
fn main() {
    println!("hello, vimd");
}
```

### 表格

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| Vim 编辑 | 支持 | i / Esc / :wq |
| 标题字号 | 支持 | Kitty 1x–4x |
| 表格框线 | 支持 | 圆角 |

---

### 保持轻量

没有 Electron，没有 WebView，没有 async runtime。
