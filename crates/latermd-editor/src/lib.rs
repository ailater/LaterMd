#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Rope 支撑的编辑器缓冲。
//!
//! ropey 的 [`Rope`] 是树状存储,借不出整段 `&str`,而 egui 的
//! `TextEdit` 通过 `TextBuffer` trait 每帧借 `as_str()`。因此
//! [`EditorBuffer`] 同时持有 rope 与扁平镜像字符串:编辑走同一次增量
//! splice 双写(插入 O(M + log N),删除 O(log N)),从不全量重建;镜像
//! 只服务"借出 `&str`"这一件事。rope 是查询基底(char/byte 互转、后续
//! 大纲 span 映射与增量解析都打在它上面)。
//!
//! 本 crate 不依赖 egui;egui 侧的 `TextBuffer` 适配(孤儿规则所需的
//! newtype)在 `latermd-app` 内。

use std::ops::Range;

use ropey::Rope;

/// 编辑器缓冲:rope + 扁平镜像 + 未保存标志 + 修订号。
pub struct EditorBuffer {
    rope: Rope,
    mirror: String,
    dirty: bool,
    rev: u64,
}

impl EditorBuffer {
    /// 以初始文本构造(文件加载/新建入口)。
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            mirror: text.to_owned(),
            dirty: false,
            rev: 0,
        }
    }

    /// 全文,即与 rope 同步的扁平镜像。
    pub fn text(&self) -> &str {
        &self.mirror
    }

    /// 是否有未保存修改。
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 保存动作成功后清除未保存标志。
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// 修订号:任何真实内容变更 +1。消费方(预览快照)用它判断是否需要重建。
    pub fn revision(&self) -> u64 {
        self.rev
    }

    /// 全文字符数(Unicode 标量)。
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// 字符偏移 → 字节偏移。egui 的 `CCursor.index` 是字符偏移,大纲
    /// token 的 `source_span` 是字节偏移,两者互转即靠这一对函数。
    ///
    /// `char_idx` 超界时钳制到末尾,不 panic(编辑器不该被一个越界光标打死)。
    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.rope.char_to_byte(char_idx.min(self.rope.len_chars()))
    }

    /// 字节偏移 → 字符偏移。落在多字节字符中间时归到所属字符的起点
    /// (ropey 语义);`byte_idx` 超界时钳制到末尾。
    pub fn byte_to_char(&self, byte_idx: usize) -> usize {
        self.rope.byte_to_char(byte_idx.min(self.rope.len_bytes()))
    }

    /// 在字符偏移处插入文本(增量)。空文本为无操作,不推进修订号。
    pub fn insert_chars(&mut self, char_idx: usize, text: &str) {
        if text.is_empty() {
            return;
        }
        let char_idx = char_idx.min(self.rope.len_chars());
        let byte_idx = self.rope.char_to_byte(char_idx);
        self.rope.insert(char_idx, text);
        // char_to_byte 产出的必是字符边界;同一次编辑双写,镜像不做全量重建。
        self.mirror.insert_str(byte_idx, text);
        self.touch();
    }

    /// 删除字符区间(增量)。空区间/越界钳制后为空则无操作。
    pub fn remove_chars(&mut self, char_range: Range<usize>) {
        let len = self.rope.len_chars();
        let start = char_range.start.min(len);
        let end = char_range.end.min(len);
        if start >= end {
            return;
        }
        let (byte_start, byte_end) = (self.rope.char_to_byte(start), self.rope.char_to_byte(end));
        self.rope.remove(start..end);
        self.mirror.replace_range(byte_start..byte_end, "");
        self.touch();
    }

    /// 整体替换。只有内容真的不同才生效(egui 内建 undo/redo 经
    /// `TextBuffer::replace_with` 走到这里,undo 栈不会推入相同快照,
    /// 但防御性短路仍保留,免得无谓推进修订号触发预览重建)。
    pub fn replace_all(&mut self, text: &str) {
        if self.mirror == text {
            return;
        }
        self.rope = Rope::from_str(text);
        self.mirror.clear();
        self.mirror.push_str(text);
        self.touch();
    }

    /// 载入一整篇外部内容(打开文件 / 新建空文档),并复位未保存标志:
    /// 内容来自磁盘而非用户修改,不构成 dirty。
    pub fn load(&mut self, text: &str) {
        self.replace_all(text);
        self.clear_dirty();
    }

    /// 全文快照(喂给预览/导出)。
    pub fn snapshot(&self) -> String {
        self.mirror.clone()
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.rev += 1;
    }
}

impl Default for EditorBuffer {
    fn default() -> Self {
        Self::new("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中英混排样本:ASCII(1 字节)与 CJK(3 字节)混在同一行。
    const CJK: &str = "a你好bc,世界d";

    /// rope 与镜像一致 + 末尾偏移互转闭合。
    fn assert_invariants(buf: &EditorBuffer) {
        assert_eq!(buf.rope.to_string(), buf.mirror, "rope 与镜像失同步");
        assert_eq!(buf.char_to_byte(buf.len_chars()), buf.mirror.len());
    }

    #[test]
    fn insert_then_remove_keeps_mirror_in_sync() {
        let mut buf = EditorBuffer::new("hello");
        buf.insert_chars(5, " 世界");
        buf.insert_chars(0, ">> ");
        buf.remove_chars(0..3);
        buf.insert_chars(2, "你好");
        buf.remove_chars(20..25); // 越界,钳制后为空区间:无操作
        assert_eq!(buf.text(), "he你好llo 世界");
        assert_invariants(&buf);
    }

    #[test]
    fn edits_are_incremental_not_full_rewrite() {
        // 增量性的可观测证据:插入后行内容精确保持,修订号恰好推进一次;
        // 空操作(空文本、空区间)不推进。
        let mut buf = EditorBuffer::new("l0\nl1\nl2\n");
        let before = buf.revision();
        buf.insert_chars(3, "插入"); // l0 行尾、l1 行前
        assert_eq!(buf.revision(), before + 1);
        assert_eq!(buf.text(), "l0\n插入l1\nl2\n");
        buf.insert_chars(0, ""); // 空文本:无操作
        buf.remove_chars(2..2); // 空区间:无操作
        assert_eq!(buf.revision(), before + 1);
        assert_invariants(&buf);
    }

    #[test]
    fn byte_char_roundtrip_cjk() {
        let buf = EditorBuffer::new(CJK);
        // 每个字符边界上,byte→char→byte 幂等。
        for char_idx in 0..=buf.len_chars() {
            let byte_idx = buf.char_to_byte(char_idx);
            assert_eq!(buf.byte_to_char(byte_idx), char_idx);
        }
        // "你"占字节 1..4,落在中间(2)归到所属字符。
        assert_eq!(buf.byte_to_char(2), 1);
        assert_eq!(buf.char_to_byte(1), 1);
        assert_eq!(buf.char_to_byte(2), 4);
        // 越界钳制,不 panic。
        assert_eq!(buf.char_to_byte(usize::MAX), CJK.len());
        assert_eq!(buf.byte_to_char(usize::MAX), buf.len_chars());
    }

    #[test]
    fn dirty_flag_and_revision() {
        let mut buf = EditorBuffer::new("x");
        assert!(!buf.is_dirty());
        buf.insert_chars(1, "y");
        assert!(buf.is_dirty());
        assert_eq!(buf.revision(), 1);
        buf.clear_dirty();
        assert!(!buf.is_dirty());
        assert_eq!(buf.revision(), 1, "clear_dirty 不回退修订号");
        buf.replace_all("xy"); // 与当前内容相同:短路,不推进
        assert_eq!(buf.revision(), 1);
        buf.replace_all("z");
        assert_eq!(buf.revision(), 2);
        assert_invariants(&buf);
    }

    #[test]
    fn replace_all_and_undo_style_roundtrip() {
        // 模拟 egui 内建 undo 的路径:replace_with = 全删 + 从 0 插入。
        let mut buf = EditorBuffer::new("# 标题\n\n正文");
        buf.remove_chars(0..buf.len_chars());
        buf.insert_chars(0, "# 新标题\n\n新正文");
        assert_eq!(buf.text(), "# 新标题\n\n新正文");
        assert_invariants(&buf);
    }

    #[test]
    fn load_resets_dirty_but_keeps_revision_monotonic() {
        let mut buf = EditorBuffer::new("old");
        buf.insert_chars(3, "!");
        assert!(buf.is_dirty());
        let rev = buf.revision();

        buf.load("# 新文档");
        assert_eq!(buf.text(), "# 新文档");
        assert!(!buf.is_dirty(), "磁盘内容不是用户修改");
        assert!(
            buf.revision() >= rev,
            "修订号只前进不回退(快照缓存依赖此约定)"
        );

        // 载入与当前内容相同的文本:replace_all 短路,dirty 仍须复位。
        buf.insert_chars(0, "x");
        buf.load("x# 新文档");
        assert!(!buf.is_dirty());
        assert_invariants(&buf);
    }

    #[test]
    fn snapshot_matches_text() {
        let buf = EditorBuffer::new(CJK);
        assert_eq!(buf.snapshot(), CJK);
    }
}
