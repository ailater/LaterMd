//! Live Preview(P3 深水区 #9):光标所在块显示源码,其余富渲染。
//!
//! **铁律(roadmap 阶段 5)**:源码模式与 Live Preview 必须是**一个编辑器 +
//! 一个 `render_mode` 标志** —— 共用同一个 rope buffer 和同一套撤销语义。
//! 本模块因此不持有任何文本副本:活动块的编辑经 [`BlockBuffer`] 直接落在同
//! 一个 [`EditorBuffer`] 上(块内偏移 + 块首偏移 = 全文偏移),切模式不丢
//! 光标也不丢 undo 栈(undo 走 TextEdit 内建 undoer,快照就是这份文本)。
//!
//! 块表来自 `latermd_md::blocks`(字节区间**连续覆盖全文**):这是安全底线
//! —— 若有字节落在任何块之外,在那儿敲一个字符就会静默丢失。
//!
//! v1 的范围(roadmap「内联标记半隐藏 v1 可简化」):块是**整块**切换,不做
//! 行内标记的半隐藏(如只对 `**` 隐藏一半)。改块的代价是整块源码裸出来。

use std::ops::Range;

use eframe::egui;
use egui_markdown::MarkdownLabel;
use latermd_editor::EditorBuffer;

use crate::state::{OutlineCursor, PreviewState};

/// 编辑器的渲染模式(P3 的那**一个标志**)。
///
/// 源码模式与 Live Preview 共用同一个 rope buffer、同一套撤销语义,区别仅
/// 在于是否应用「光标所在块显示源码」这条规则 —— 所以切换不该有任何恢复
/// 逻辑(roadmap 阶段 5 的原则)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RenderMode {
    /// 整篇源码。
    #[default]
    Source,
    /// 光标块源码、其余富渲染。
    Live,
}

impl RenderMode {
    /// 状态栏/面板标题用。
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "源码",
            Self::Live => "Live",
        }
    }

    /// 互换(命令入口)。
    pub fn opposite(self) -> Self {
        match self {
            Self::Source => Self::Live,
            Self::Live => Self::Source,
        }
    }
}

/// Live Preview 的每标签状态:块表 + 当前活动块。
#[derive(Debug, Clone, Default)]
pub struct LiveState {
    /// 块的字节区间,连续覆盖全文(`latermd_md::blocks`)。
    pub blocks: Vec<Range<usize>>,
    /// 正在以源码编辑的块序号;`None` = 没有块获得焦点。
    pub active: Option<usize>,
    /// 下一帧要落到(块, 块内字符偏移)的光标:跨块 caret 路由的交接点。
    pub pending_caret: Option<(usize, usize)>,
    /// 块表对应的修订号;只在修订号前进时重算块。
    synced_rev: Option<u64>,
}

impl LiveState {
    /// 同步块表:修订号变了才重解析。
    ///
    /// 重算后**按光标字节重新定位活动块** —— 敲回车会分裂块、删空行会合并
    /// 块,按序号记忆必然错位(在第 2 块开头敲回车后,原来的第 3 块变成第 4 块)。
    pub fn sync(&mut self, editor: &EditorBuffer, cursor_byte: Option<usize>) {
        let rev = editor.revision();
        if self.synced_rev == Some(rev) {
            // 块表没变但还没有活动块(首次进入 Live 模式):按光标补定位,
            // 不必等下一次编辑 —— 否则要「先敲一个字」才出现可编辑的块。
            if self.active.is_none() {
                self.active = cursor_byte.and_then(|byte| self.block_containing(byte));
            }
            return;
        }
        self.blocks = latermd_md::blocks(editor.text());
        self.synced_rev = Some(rev);
        self.active = cursor_byte
            .and_then(|byte| self.block_containing(byte))
            .or_else(|| self.active.filter(|index| *index < self.blocks.len()));
    }

    /// 包含该字节的块序号;落在块之间的边界上取后一块(边界属于后一块的起点)。
    pub fn block_containing(&self, byte: usize) -> Option<usize> {
        self.blocks
            .iter()
            .position(|block| byte >= block.start && byte < block.end)
            // 文末:落在最后一块的闭合端也算最后一块(末尾可编辑)
            .or_else(|| (!self.blocks.is_empty()).then(|| self.blocks.len() - 1))
    }

    /// 块的字符数(光标路由要按字符偏移算,块区间是字节)。
    pub fn block_char_len(&self, editor: &EditorBuffer, index: usize) -> usize {
        self.blocks.get(index).map_or(0, |block| {
            editor.byte_to_char(block.end) - editor.byte_to_char(block.start)
        })
    }

    /// 重置(换文档 / 关标签):不留下上一份文档的块表。
    pub fn reset(&mut self) {
        self.blocks.clear();
        self.active = None;
        self.pending_caret = None;
        self.synced_rev = None;
    }
}

/// 块的编辑代理:把 [`EditorBuffer`] 的**一块**暴露成 egui `TextBuffer`。
///
/// 与源码模式的 `EditorText` 同理(孤儿规则需本地 newtype),但 `as_str` 借的是
/// 镜像的切片、编辑按块首换算回全文 —— 因此文本只有一份真源。
struct BlockBuffer<'a> {
    buffer: &'a mut EditorBuffer,
    /// 块在全文中的字节区间。
    range: Range<usize>,
}

impl BlockBuffer<'_> {
    /// 块首的字符偏移(TextBuffer 的偏移是字符,块区间是字节)。
    fn base(&self) -> usize {
        self.buffer.byte_to_char(self.range.start)
    }

    /// 块文本切片;越界与落在字符中间都钳到字符边界(块边界来自 parser 的
    /// span,理论上是边界,但手改过的文本值得防御)。
    fn slice<'a>(text: &'a str, range: &Range<usize>) -> &'a str {
        let start = text.floor_char_boundary(range.start.min(text.len()));
        let end = text.floor_char_boundary(range.end.min(text.len()));
        &text[start..end.max(start)]
    }
}

impl egui::TextBuffer for BlockBuffer<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        Self::slice(self.buffer.text(), &self.range)
    }

    fn insert_text(&mut self, text: &str, char_index: egui::text::CharIndex) -> usize {
        self.buffer.insert_chars(self.base() + char_index.0, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<egui::text::CharIndex>) {
        let base = self.base();
        self.buffer
            .remove_chars(base + char_range.start.0..base + char_range.end.0);
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<BlockBuffer<'static>>()
    }
}

/// 绘制 Live Preview:非活动块富渲染(点击即进入编辑),活动块源码编辑。
///
/// 返回活动块 TextEdit 的响应(没有活动块时返回一块占位区域,便于测试定位)。
pub fn ui(
    panel: &mut egui::Ui,
    editor: &mut EditorBuffer,
    preview: &mut PreviewState,
    cursor: &mut OutlineCursor,
    live: &mut LiveState,
    editor_id: egui::Id,
) -> egui::Response {
    live.sync(editor, cursor.byte);

    let mut active_response: Option<egui::Response> = None;
    let mut activate: Option<usize> = None;
    let mut route: Option<(usize, usize)> = None;
    let block_count = live.blocks.len();

    egui::ScrollArea::vertical()
        .id_salt("live-preview")
        .auto_shrink([false, false])
        .show(panel, |ui| {
            if block_count == 0 {
                ui.weak("(空文档)");
            }
            for index in 0..block_count {
                let range = live.blocks[index].clone();
                // 块文本(渲染与行数估算都要用;编辑走的是同一份缓冲的切片,
                // 这里只 clone 块自己,不是整篇)
                let block_text = BlockBuffer::slice(editor.text(), &range).to_owned();
                if live.active == Some(index) {
                    let char_len = live.block_char_len(editor, index);
                    let lines = block_text.lines().count().max(1);
                    let mut buffer = BlockBuffer {
                        buffer: editor,
                        range: range.clone(),
                    };
                    let output = egui::TextEdit::multiline(&mut buffer)
                        // 稳定 id:按块序号(不带长度/hash),AGENTS §6.7 的红线
                        .id(editor_id.with(("live-block", index)))
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(lines.clamp(1, 40))
                        .show(ui);

                    // 跨块 caret 路由:块首按 ↑ 去上一块末尾;块尾按 ↓ 去下一块开头
                    if output.response.response.has_focus() {
                        let caret = output
                            .state
                            .cursor
                            .char_range()
                            .map(|range| range.primary.index.0);
                        let at_start = caret == Some(0);
                        let at_end = caret == Some(char_len);
                        if ui.input(|input| input.key_pressed(egui::Key::ArrowUp))
                            && at_start
                            && index > 0
                        {
                            let target = index - 1;
                            route = Some((target, live.block_char_len(editor, target)));
                        } else if ui.input(|input| input.key_pressed(egui::Key::ArrowDown))
                            && at_end
                            && index + 1 < block_count
                        {
                            route = Some((index + 1, 0));
                        }
                    }

                    // 光标回填(大纲/状态栏用):块内字符偏移 + 块首
                    cursor.byte = output.state.cursor.char_range().map(|range| {
                        editor.char_to_byte(
                            editor.byte_to_char(live.blocks[index].start) + range.primary.index.0,
                        )
                    });
                    let response = output.response.response;
                    // 上一帧交过来的光标:落到本块的指定字符偏移并要焦点
                    // (放在回填之后 —— `output.state` 在这里被移走)
                    if let Some((block, char_idx)) = live.pending_caret {
                        if block == index {
                            let id = response.id;
                            let mut state = output.state;
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::one(
                                    egui::text::CCursor::new(char_idx),
                                )));
                            state.store(ui.ctx(), id);
                            ui.ctx().memory_mut(|mem| mem.request_focus(id));
                            live.pending_caret = None;
                        }
                    }
                    active_response = Some(response);
                } else {
                    // 富渲染。MarkdownLabel::show 不返回响应,区域用渲染前后
                    // 的 cursor 差值框出来 —— 点击即进入编辑(精确 hit-test
                    // 需要文本布局反查,v1 不追求点击落点的像素级精确)。
                    let top = ui.cursor().top();
                    MarkdownLabel::new(editor_id.with(("live-render", index)), &block_text)
                        .wrap()
                        .show(ui);
                    let bottom = ui.cursor().top();
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(ui.min_rect().left(), top),
                        egui::pos2(ui.min_rect().right(), bottom.max(top + 1.0)),
                    );
                    if ui.allocate_rect(rect, egui::Sense::click()).clicked() {
                        activate = Some(index);
                    }
                }
            }
        });

    if let Some(index) = activate {
        live.active = Some(index);
        live.pending_caret = Some((index, live.block_char_len(editor, index)));
    }
    if let Some(route) = route {
        live.active = Some(route.0);
        live.pending_caret = Some(route);
    }

    // 快照同步(与源码模式同一条规则:仅修订号前进时重建)
    if preview.synced_rev != editor.revision() {
        preview.rebuild(editor);
    }
    active_response
        .unwrap_or_else(|| panel.allocate_response(egui::vec2(0.0, 0.0), egui::Sense::click()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(text: &str) -> (EditorBuffer, LiveState, OutlineCursor) {
        let editor = EditorBuffer::new(text);
        let mut live = LiveState::default();
        live.sync(&editor, None);
        (editor, live, OutlineCursor::default())
    }

    /// 块表连续覆盖全文(安全底线:落在块外的字节会在编辑时丢失)。
    #[test]
    fn blocks_cover_the_whole_document() {
        let (editor, live, _) = state_with("# 标题\n\n正文\n\n```rust\nfn a(){}\n```\n");
        let mut cursor = 0;
        for block in &live.blocks {
            assert_eq!(block.start, cursor, "{live:?}");
            cursor = block.end;
        }
        assert_eq!(cursor, editor.text().len());
    }

    /// 修订号不变就不重算块(每次编辑都重解析会拖垮打字)。
    #[test]
    fn sync_only_recomputes_when_revision_advances() {
        let (mut editor, mut live, _) = state_with("a\n\nb\n");
        let before = live.blocks.clone();
        live.sync(&editor, None);
        assert_eq!(live.blocks, before, "未编辑不重算");
        editor.insert_chars(0, "x");
        live.sync(&editor, None);
        assert_ne!(live.blocks, before, "编辑后重算");
    }

    /// 活动块按光标字节定位:在第 2 块开头敲回车后,活动块跟着光标而不是
    /// 停在旧序号上(按序号记忆会错位)。
    #[test]
    fn active_block_follows_cursor_after_split() {
        let text = "一\n\n二\n";
        let (mut editor, mut live, _) = state_with(text);
        let second = live.blocks[1].start;
        live.sync(&editor, Some(second));
        assert_eq!(live.active, Some(1));

        // 在块首敲回车:块表分裂,光标仍在原处 → 活动块重新定位
        editor.insert_chars(editor.byte_to_char(second), "\n");
        live.sync(&editor, Some(second));
        assert_eq!(live.active, Some(live.block_containing(second).unwrap()));

        // 合并块(删掉中间的空行):块数变少,活动块仍跟着光标
        let blocks_before = live.blocks.len();
        editor.remove_chars(editor.byte_to_char(second - 1)..editor.byte_to_char(second + 1));
        live.sync(&editor, Some(second - 1));
        assert!(live.blocks.len() < blocks_before, "块被合并了");
        assert_eq!(
            live.active,
            Some(live.block_containing(second - 1).unwrap())
        );
    }

    /// 块内编辑落在同一份缓冲上(不产生第二份文本):块首插入 = 全文对应位置插入。
    #[test]
    fn block_edits_land_in_the_shared_buffer() {
        let mut editor = EditorBuffer::new("hello\n\nworld\n");
        let mut live = LiveState::default();
        live.sync(&editor, None);
        let block = live.blocks[0].clone();
        {
            let mut buffer = BlockBuffer {
                buffer: &mut editor,
                range: block.clone(),
            };
            assert_eq!(egui::TextBuffer::as_str(&buffer), "hello\n\n");
            egui::TextBuffer::insert_text(&mut buffer, ">> ", egui::text::CharIndex(0));
        }
        assert_eq!(editor.text(), ">> hello\n\nworld\n");
        assert!(editor.is_dirty());
    }

    /// 块内删除:块内字符区间换算回全文区间(删掉的是块首两个字)。
    #[test]
    fn block_delete_maps_back_to_full_text() {
        let mut editor = EditorBuffer::new("hello\n\nworld\n");
        let mut live = LiveState::default();
        live.sync(&editor, None);
        let block = live.blocks[1].clone();
        {
            let mut buffer = BlockBuffer {
                buffer: &mut editor,
                range: block.clone(),
            };
            assert_eq!(egui::TextBuffer::as_str(&buffer), "world\n");
            egui::TextBuffer::delete_char_range(
                &mut buffer,
                egui::text::CharIndex(0)..egui::text::CharIndex(2),
            );
        }
        assert_eq!(editor.text(), "hello\n\nrld\n");
    }

    /// 块字符长度(光标路由按字符算)与中英混排一致。
    #[test]
    fn block_char_len_counts_chars_not_bytes() {
        let (editor, live, _) = state_with("你好\n\nworld\n");
        assert_eq!(live.block_char_len(&editor, 0), 4, "你好 + 空行");
        assert_eq!(live.block_char_len(&editor, 1), 6, "world + \\n");
        assert_eq!(live.block_char_len(&editor, 99), 0, "越界给 0");
    }

    /// Live 面板渲一帧不 panic(活动块走 TextEdit、其余走富渲染),且不改动
    /// 缓冲 —— 绘制不该有副作用。
    #[test]
    fn live_panel_renders_a_frame_without_touching_text() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new("# 标题\n\n正文\n\n```rust\nfn a(){}\n```\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut live = LiveState {
            active: Some(0),
            ..LiveState::default()
        };
        let before = editor.text().to_owned();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            super::ui(
                ui,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
                egui::Id::new("live-test"),
            );
        });
        output.drop_without_applying_deltas();
        assert_eq!(editor.text(), before, "渲染不改动缓冲");
        assert!(!editor.is_dirty(), "渲染不算用户修改");
        assert!(live.blocks.len() >= 3, "{live:?}");
    }

    /// 块包含判定:块内 / 文末 / 边界都给出确定的块。
    #[test]
    fn block_containing_resolves_edges() {
        let (editor, live, _) = state_with("一\n\n二\n");
        assert_eq!(live.block_containing(0), Some(0));
        assert_eq!(live.block_containing(live.blocks[1].start), Some(1));
        assert_eq!(
            live.block_containing(editor.text().len()),
            Some(live.blocks.len() - 1),
            "文末归最后一块"
        );
    }
}
