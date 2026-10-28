// vimd 的文本缓冲区。
//
// 这不是完整的 Vim：只实现阅读型编辑器真正需要的那一小组操作。
// 行用 String 保存，光标列用“第几个 char”而不是字节，避免中文/emoji
// 被切在 UTF-8 中间。撤销用快照，文档规模是 Markdown，够轻。

use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Clone)]
struct Snapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
    dirty: bool,
    final_newline: bool,
}

pub struct Editor {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub dirty: bool,
    pub path: Option<PathBuf>,
    final_newline: bool,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    yank: String,
}

impl Editor {
    pub fn empty() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
            dirty: false,
            path: None,
            final_newline: true,
            undo: Vec::new(),
            redo: Vec::new(),
            yank: String::new(),
        }
    }

    pub fn open(path: PathBuf) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?;
        let final_newline = text.ends_with('\n');
        let mut lines: Vec<String> = text.split('\n').map(ToString::to_string).collect();
        if final_newline && lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        if lines.is_empty() {
            lines.push(String::new());
        }
        Ok(Self {
            lines,
            row: 0,
            col: 0,
            dirty: false,
            path: Some(path),
            final_newline,
            undo: Vec::new(),
            redo: Vec::new(),
            yank: String::new(),
        })
    }

    pub fn text(&self) -> String {
        let mut out = self.lines.join("\n");
        if self.final_newline {
            out.push('\n');
        }
        out
    }

    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "E32: No file name",
            ));
        };
        fs::write(path, self.text())?;
        self.dirty = false;
        Ok(())
    }

    pub fn save_as(&mut self, path: PathBuf) -> io::Result<()> {
        self.path = Some(path);
        self.save()
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    pub fn file_label(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| p.display().to_string()),
            None => "[untitled]".to_string(),
        }
    }

    pub fn full_label(&self) -> String {
        match &self.path {
            Some(p) => p.display().to_string(),
            None => "[untitled]".to_string(),
        }
    }

    pub fn line(&self, row: usize) -> &str {
        self.lines.get(row).map(String::as_str).unwrap_or("")
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.line(row).chars().count()
    }

    pub fn max_line(&self) -> usize {
        self.lines.len().saturating_sub(1)
    }

    pub fn clamp_cursor(&mut self) {
        self.row = self.row.min(self.max_line());
        self.col = self.col.min(self.line_len(self.row));
    }

    fn byte_col(&self, row: usize, col: usize) -> usize {
        let line = self.line(row);
        line.char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(line.len())
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
            dirty: self.dirty,
            final_newline: self.final_newline,
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snapshot) {
        self.lines = s.lines;
        self.row = s.row;
        self.col = s.col;
        self.dirty = s.dirty;
        self.final_newline = s.final_newline;
    }

    pub fn undo(&mut self) -> bool {
        let Some(prev) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(prev);
        self.clamp_cursor();
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(next);
        self.clamp_cursor();
        true
    }

    pub fn insert_char(&mut self, c: char) {
        self.push_undo();
        let byte = self.byte_col(self.row, self.col);
        self.lines[self.row].insert(byte, c);
        self.col += 1;
        self.dirty = true;
    }

    pub fn insert_str(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.push_undo();
        let byte = self.byte_col(self.row, self.col);
        self.lines[self.row].insert_str(byte, text);
        self.col += text.chars().count();
        self.dirty = true;
    }

    pub fn insert_newline(&mut self) {
        self.push_undo();
        let byte = self.byte_col(self.row, self.col);
        let tail = self.lines[self.row].split_off(byte);
        self.lines.insert(self.row + 1, tail);
        self.row += 1;
        self.col = 0;
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            self.push_undo();
            let end = self.byte_col(self.row, self.col);
            let start = self.byte_col(self.row, self.col - 1);
            self.lines[self.row].replace_range(start..end, "");
            self.col -= 1;
            self.dirty = true;
        } else if self.row > 0 {
            self.push_undo();
            let cur = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&cur);
            self.dirty = true;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.col < self.line_len(self.row) {
            self.push_undo();
            let start = self.byte_col(self.row, self.col);
            let end = self.byte_col(self.row, self.col + 1);
            self.lines[self.row].replace_range(start..end, "");
            self.dirty = true;
        } else if self.row + 1 < self.lines.len() {
            self.push_undo();
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.dirty = true;
        }
    }

    pub fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.line_len(self.row);
        }
    }

    pub fn move_right(&mut self) {
        if self.col < self.line_len(self.row) {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.line_len(self.row));
        }
    }

    pub fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.line_len(self.row));
        }
    }

    pub fn home(&mut self) {
        self.col = 0;
    }

    pub fn end(&mut self) {
        self.col = self.line_len(self.row);
    }

    pub fn first_nonblank(&mut self) {
        let line = self.line(self.row);
        self.col = line.chars().take_while(|c| c.is_whitespace()).count();
    }

    pub fn delete_line(&mut self) {
        self.push_undo();
        self.yank = self.lines[self.row].clone();
        if self.lines.len() == 1 {
            self.lines[0].clear();
        } else {
            self.lines.remove(self.row);
        }
        self.row = self.row.min(self.max_line());
        self.col = self.col.min(self.line_len(self.row));
        self.dirty = true;
    }

    pub fn yank_line(&mut self) {
        self.yank = self.lines[self.row].clone();
    }

    pub fn paste_line(&mut self) {
        if self.yank.is_empty() {
            return;
        }
        self.push_undo();
        let line = self.yank.clone();
        self.lines.insert(self.row + 1, line);
        self.row += 1;
        self.col = 0;
        self.dirty = true;
    }

    pub fn open_below(&mut self) {
        self.push_undo();
        self.lines.insert(self.row + 1, String::new());
        self.row += 1;
        self.col = 0;
        self.dirty = true;
    }

    pub fn open_above(&mut self) {
        self.push_undo();
        self.lines.insert(self.row, String::new());
        self.col = 0;
        self.dirty = true;
    }

    pub fn find_forward(&self, needle: &str, from_row: usize) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        let lower = needle.to_lowercase();
        let n = self.lines.len();
        for step in 0..n {
            let row = (from_row + step) % n;
            if self.lines[row].to_lowercase().contains(&lower) {
                return Some(row);
            }
        }
        None
    }

    pub fn find_backward(&self, needle: &str, from_row: usize) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        let lower = needle.to_lowercase();
        let n = self.lines.len();
        for step in 0..n {
            let row = (from_row + n - step) % n;
            if self.lines[row].to_lowercase().contains(&lower) {
                return Some(row);
            }
        }
        None
    }
}
