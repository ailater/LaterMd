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
//! v1 的范围(roadmap「内联标记半隐藏 v1 可简化」):块是**整块**切换。LP2-1
//! (v2)在活动块内做**内联标记半隐藏**:正文正常显示,`**` 等标记字符以半
//! 透明遮罩弱化,光标/选区贴上的段显形 —— 区间提取是纯函数
//! `latermd_md::inline_marks`,绘制是纯叠加,不碰 TextEdit 的状态与命中测试。
//! LP2-2 加**选区扩展与配对显形**:选区/光标触碰某标记时其配对另一侧同
//! 时显形;从标记内侧发起的双击/拖选把选区一次性扩到完整标记对(内容+
//! 两侧标记),让编辑标记本身有可及入口 —— 决策是纯函数
//! `latermd_md::mark_interaction`,选区仍在块内闭合(v1 既定,块是独立
//! TextEdit,不引入跨块选区行为变更)。

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
    /// 活动块的内联标记缓存(LP2-1 半隐藏 + LP2-2 选区扩展):(修订号,
    /// 块序号) → 标记段与成对表(同一次解析的产出,不会互相错位)。与块
    /// 表同一条纪律:修订号不动就不重解析(打一个字的代价 = 重解析活动
    /// 块,与 v1 每次编辑重算整篇块表同一量级)。
    marks: Option<(u64, usize, latermd_md::InlineMarks)>,
    /// 拖选锚点(LP2-2):活动块 TextEdit 上**按下帧**的塌缩光标(块内
    /// 字符偏移),拖选松手帧用于「从标记发起的拖选扩展到完整标记对」。
    /// 生命周期 = 一次按压:松手帧消费,按在别处的按下帧清除。
    drag_anchor: Option<usize>,
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
        self.marks = None;
        self.drag_anchor = None;
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
                    // 稳定 id:按块序号(不带长度/hash),AGENTS §6.7 的红线
                    let response_id = editor_id.with(("live-block", index));
                    let output = egui::TextEdit::multiline(&mut buffer)
                        .id(response_id)
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
                    // LP2-1 内联标记半隐藏 + LP2-2 选区扩展。显形集合与扩
                    // 展选区都是纯函数(`latermd_md::mark_interaction`),绘
                    // 制仍是纯叠加 —— 不挂 widget(不参与命中测试)。交互
                    // 边界:双击帧把 egui 选出的词扩到最内层标记对;拖选松
                    // 手帧只有「锚点压着标记发起」的拖选才扩(从内容中部
                    // 发起的普通拖选保持原样);键盘选区/三击选行不扩展。
                    // 光标取 `output.state`(帧内真值)而非 `output.cursor_
                    // range`:后者在焦点门控的键盘事件段产出,指针交互(选
                    // 词/拖选/按压塌缩)发生在其后、只改 state —— 用陈旧值
                    // 会把按压点读成旧光标。
                    if let Some(cursor_range) = output.state.cursor.char_range() {
                        let rev = editor.revision();
                        if !matches!(&live.marks, Some((r, i, _)) if *r == rev && *i == index) {
                            // 与 galley 同一份文本:本帧的编辑发生在 show()
                            // 内部,这里取的也是编辑后的同一切片,缓存键即
                            // 编辑后的修订号 —— 区间与字形永远对得上
                            live.marks = Some((
                                rev,
                                index,
                                latermd_md::inline_marks_with_pairs(BlockBuffer::slice(
                                    editor.text(),
                                    &range,
                                )),
                            ));
                        }
                        let bundle: &latermd_md::InlineMarks = match &live.marks {
                            Some((_, _, bundle)) => bundle,
                            None => unreachable!("缓存已在上一行建立"),
                        };
                        let caret = cursor_range.primary.index.0;
                        let other = cursor_range.secondary.index.0;

                        // 手势判定。egui 在 show() 内部已处理双击选词(词选
                        // 区就在本帧的 cursor_range 里)与按下塌缩光标;这里
                        // 只读信号:双击 → 扩词;松手 + 非空选区 + 锚点 → 扩
                        // 拖选;按下帧记/清锚点(按在别处清,防跨按压串帧)。
                        let double_clicked = output.response.response.double_clicked();
                        let primary_pressed = ui.input(|input| input.pointer.primary_pressed());
                        let primary_released = ui.input(|input| input.pointer.primary_released());
                        if primary_pressed {
                            live.drag_anchor = if output.response.response.contains_pointer() {
                                Some(caret)
                            } else {
                                None
                            };
                        }
                        let gesture = if double_clicked {
                            live.drag_anchor = None;
                            Some(latermd_md::MarkGesture::DoubleClick)
                        } else if primary_released {
                            match (live.drag_anchor.take(), caret != other) {
                                (Some(anchor), true) => {
                                    Some(latermd_md::MarkGesture::DragRelease { anchor })
                                }
                                _ => None,
                            }
                        } else {
                            None
                        };

                        let interaction = latermd_md::mark_interaction(
                            BlockBuffer::slice(editor.text(), &range),
                            bundle,
                            caret,
                            other,
                            gesture,
                        );
                        paint_mark_fades(
                            ui,
                            &output.galley,
                            output.galley_pos,
                            BlockBuffer::slice(editor.text(), &range),
                            &bundle.segments,
                            &interaction.revealed,
                        );
                        // 扩展写回:与 pending_caret 同一条通道(改 TextEdit
                        // 光标状态,下一帧生效);primary 方向保留手势原方向。
                        if let Some(expanded) = interaction.expanded {
                            let (low, high) = (expanded.start, expanded.end);
                            if low != caret.min(other) || high != caret.max(other) {
                                let mut state = output.state.clone();
                                let (primary, secondary) = if caret <= other {
                                    (low, high)
                                } else {
                                    (high, low)
                                };
                                state
                                    .cursor
                                    .set_char_range(Some(egui::text::CCursorRange::two(
                                        egui::text::CCursor::new(primary),
                                        egui::text::CCursor::new(secondary),
                                    )));
                                state.store(ui.ctx(), response_id);
                            }
                        }
                    }
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
                    // 富渲染。区域用渲染前后的 cursor 差值框出来;「点击进
                    // 编辑」的命中不走 egui widget,理由见 [`clicked_for_edit`]。
                    let top = ui.cursor().top();
                    // 字体与右栏预览同源(#23 F3):size = 用户字号偏好,
                    // 族 = 预览专用族(#43 M2 的行 metrics 对齐副本,无 CJK
                    // 回落 Proportional)。源码/Live 两种模式下排版偏好同观感。
                    MarkdownLabel::new(editor_id.with(("live-render", index)), &block_text)
                        .font(egui::FontId::new(
                            crate::theme::editor_font_size(ui.ctx()),
                            crate::fonts::preview_body_family(ui.ctx()),
                        ))
                        .wrap()
                        // 代码块复制头(#38)与右栏预览同一份:Live 模式的
                        // 富渲染块也是「code 预览的地方」。
                        .code_block_buttons(&crate::ui::preview::code_copy_buttons)
                        .show(ui);
                    let bottom = ui.cursor().top();
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(ui.min_rect().left(), top),
                        egui::pos2(ui.min_rect().right(), bottom.max(top + 1.0)),
                    );
                    if clicked_for_edit(ui, rect) {
                        activate = Some(index);
                    }
                }
            }
        });

    if let Some(index) = activate {
        live.active = Some(index);
        live.pending_caret = Some((index, live.block_char_len(editor, index)));
        live.drag_anchor = None;
    }
    if let Some(route) = route {
        live.active = Some(route.0);
        live.pending_caret = Some(route);
        live.drag_anchor = None;
    }

    // 快照同步(与源码模式同一条规则:仅修订号前进时重建)
    if preview.synced_rev != editor.revision() {
        preview.rebuild(editor);
    }
    active_response
        .unwrap_or_else(|| panel.allocate_response(egui::vec2(0.0, 0.0), egui::Sense::click()))
}

/// 富渲染块「点击进入编辑」的命中检测。**不挂 egui widget**:块级矩形一旦
/// 参与 hit-test,注册序必与块内其它可点件二选一 —— egui 0.36 同层点击
/// 平局取后注册者(hit_test.rs「In tie, pick last = topmost」),后注册的
/// 块矩形抢走代码块复制头(#38)的点击(复制失效、块误进编辑);先注册
/// 又抢不过富渲染体(`MarkdownLabel` 的 `Sense::click_and_drag` 整块响应
/// 在 `show()` 内注册得更晚,实测点击后进编辑同样失效)。所以直接读原始
/// 指针事件:本帧有主键 click(egui 已按拖动阈值判定,块内拖选文本不算
/// click,不误触发)且抬起点落在本块;再排除两类本就另有归属的点击 ——
/// 落在代码块复制按钮上的(本帧探针几何,`crate::ui::preview::
/// copy_button_rects`)和打开了链接的(链接点击归属富渲染层,不连带进
/// 编辑;OpenUrl 在 `show()` 内已进本帧输出,这里读得到)。
///
/// 抬起点核对而按不下起点核对:`press_origin` 在抬起帧已被 egui 清空
/// (input_state 释放即置 None);好在 `primary_clicked` 本身就含「未超出
/// 点击距离」判定,残余歧义最多块边界 max_click_dist 一线。
fn clicked_for_edit(ui: &egui::Ui, rect: egui::Rect) -> bool {
    if !ui.input(|input| input.pointer.primary_clicked()) {
        return false;
    }
    let Some(pos) = ui.input(|input| input.pointer.interact_pos()) else {
        return false;
    };
    if !rect.contains(pos) {
        return false;
    }
    if crate::ui::preview::copy_button_rects(ui.ctx())
        .iter()
        .any(|button| button.contains(pos))
    {
        return false;
    }
    !ui.ctx().output(|output| {
        output
            .commands
            .iter()
            .any(|cmd| matches!(cmd, egui::OutputCommand::OpenUrl(_)))
    })
}

/// 标记半隐藏的遮罩不透明度(遮罩色 = 编辑框背景色)。「弱化但可辨识」,
/// 不做用户可调的透明度配置面(规格口径)。
const MARK_FADE_ALPHA: u8 = 166;

/// 把活动块里的内联标记以半透明遮罩弱化(LP2-1)。
///
/// 遮罩 = 编辑框背景色 × α 叠在标记字形上,视觉即「标记变淡」,不改
/// TextEdit 的任何状态。显形与否由调用方经 [`latermd_md::mark_interaction`]
/// 算好(`revealed` 与 `marks` 平行):光标/选区贴上的段显形,被触碰标
/// 记的**配对另一侧**所在段也显形(LP2-2)。
fn paint_mark_fades(
    ui: &egui::Ui,
    galley: &egui::Galley,
    galley_pos: egui::Pos2,
    block_text: &str,
    marks: &[Range<usize>],
    revealed: &[bool],
) {
    if marks.is_empty() {
        return;
    }
    let bg = ui.visuals().text_edit_bg_color();
    let fade = egui::Color32::from_rgba_unmultiplied(bg.r(), bg.g(), bg.b(), MARK_FADE_ALPHA);
    let painter = ui.painter();
    // 标记是字节区间,TextEdit 的光标是字符偏移(相对同一份块文本);一次
    // 遍历换算全部区间(标记已升序,逐段各扫一遍前文是 O(len×段数))
    let mut chars = 0usize;
    let mut walk = block_text.char_indices().peekable();
    for (index, mark) in marks.iter().enumerate() {
        while walk.next_if(|(byte, _)| *byte < mark.start).is_some() {
            chars += 1;
        }
        let start = chars;
        while walk.next_if(|(byte, _)| *byte < mark.end).is_some() {
            chars += 1;
        }
        if revealed.get(index).copied().unwrap_or(false) {
            continue;
        }
        for rect in galley_mark_rects(galley, start..chars) {
            painter.rect_filled(
                rect.translate(galley_pos.to_vec2()),
                egui::CornerRadius::ZERO,
                fade,
            );
        }
    }
}

/// 标记字符区间(字符偏移)在 galley 里占的矩形(相对 galley 原点),按行
/// 切分 —— 折行/换行会把一段标记拆成多行多矩形。
fn galley_mark_rects(galley: &egui::Galley, char_range: Range<usize>) -> Vec<egui::Rect> {
    let mut rects = Vec::new();
    let mut row_start = 0;
    for row in &galley.rows {
        let row_chars = row.char_count_excluding_newline().0;
        let start = char_range.start.max(row_start);
        let end = char_range.end.min(row_start + row_chars);
        if end > start {
            let x0 = row.pos.x + row.x_offset(egui::text::CharIndex(start - row_start));
            let x1 = row.pos.x + row.x_offset(egui::text::CharIndex(end - row_start));
            rects.push(egui::Rect::from_min_max(
                egui::pos2(x0.min(x1), row.min_y()),
                egui::pos2(x0.max(x1), row.max_y()),
            ));
        }
        row_start += row.char_count_including_newline().0;
        if row_start >= char_range.end {
            break;
        }
    }
    rects
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

    // —— 代码块复制头(#38)与「整块点击进编辑」的点击优先级 ——

    /// 从帧输出抽出全部 CopyText 命令的载荷(照 ui/preview.rs tests 同款)。
    fn copied_texts(output: &eframe::egui::FullOutput) -> Vec<String> {
        output
            .platform_output
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                eframe::egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// 指针序列:移入 → 按下 → 抬起(照 ui/preview.rs tests 同款三帧;
    /// egui 的 click 判定发生在抬起帧)。
    fn click_events(pos: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let click = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![
            vec![egui::Event::PointerMoved(pos)],
            vec![click(pos, true)],
            vec![click(pos, false)],
        ]
    }

    /// 一帧 Live 列渲染的取证:复制按钮 rect 探针、CopyText 载荷、OpenUrl
    /// 目标、画出的文本 rect(正文/链接点击定位用)。
    struct LiveFrame {
        button_rects: Vec<egui::Rect>,
        copied: Vec<String>,
        opened: Vec<String>,
        texts: Vec<(String, egui::Rect)>,
    }

    /// 跑一帧 Live 面板(生产入口 `super::ui`)并收集取证。
    fn live_frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
        live: &mut LiveState,
    ) -> LiveFrame {
        let output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                super::ui(
                    ui,
                    editor,
                    preview,
                    cursor,
                    live,
                    egui::Id::new("live-copy-test"),
                );
            },
        );
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            if let egui::epaint::Shape::Text(t) = &clipped.shape {
                texts.push((
                    t.galley.text().to_owned(),
                    egui::Rect::from_min_size(t.pos, t.galley.size()),
                ));
            }
        }
        let copied = copied_texts(&output);
        let opened = output
            .platform_output
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        LiveFrame {
            // 帧后读取:帧号已前进,按「最后写入者即本帧」取原始探针。
            button_rects: crate::ui::preview::copy_button_probe(ctx).1,
            copied,
            opened,
            texts,
        }
    }

    /// Live 模式点非活动代码块的复制按钮必须复制、且不得切入编辑态(#38
    /// 评审修复):「整块点击进编辑」一旦参与 hit-test,无论注册先后都按
    /// egui 0.36 同层平局规则取后注册者(hit_test.rs `find_closest_within`)
    /// 与块内可点件二选一 —— 后注册抢按钮(复制失效、块误进编辑),先注册
    /// 被富渲染体抢(进编辑失效,实测点击后 active 仍 None)。修法 = 块级
    /// 命中不挂 widget,读原始指针事件([`clicked_for_edit`])。
    #[test]
    fn live_copy_button_click_copies_without_entering_edit() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new("# 标题\n\n正文段落。\n\n```rust\nfn a(){}\n```\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut live = LiveState::default();

        // 静帧:非活动代码块恰一枚按钮,块全部富渲染。
        let frame = live_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert!(frame.copied.is_empty(), "静帧不应复制");
        assert_eq!(
            frame.button_rects.len(),
            1,
            "非活动代码块一枚复制按钮:{:?}",
            frame.button_rects
        );
        assert_eq!(live.active, None);

        // 三帧点击按钮中心:复制恰一次、内容为块源文本;块不得切入编辑。
        let target = frame.button_rects[0].center();
        let mut copied = Vec::new();
        for events in click_events(target) {
            let frame = live_frame(
                &ctx,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
            copied.extend(frame.copied);
        }
        assert_eq!(
            copied,
            vec!["fn a(){}".to_owned()],
            "点按钮必须复制块源文本(修复前被整块 rect 抢走,这里为空)"
        );
        assert_eq!(live.active, None, "点按钮不得切入编辑态:{live:?}");
    }

    /// 点块内正文(非按钮)仍切入编辑(修复不得伤及「整块点击进编辑」本身)。
    #[test]
    fn live_click_outside_copy_button_still_enters_edit() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new("# 标题\n\n正文段落。\n\n```rust\nfn a(){}\n```\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut live = LiveState::default();

        let frame = live_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert_eq!(frame.button_rects.len(), 1);

        // 点击代码正文(galley rect 中心):离右上角复制按钮足够远。
        let (_, code_rect) = frame
            .texts
            .iter()
            .find(|(text, _)| text.contains("fn a()"))
            .expect("代码正文已渲染");
        let target = code_rect.center();
        assert!(
            !frame.button_rects[0].contains(target),
            "点击点必须避开按钮:{target:?}"
        );
        let mut copied = Vec::new();
        for events in click_events(target) {
            let frame = live_frame(
                &ctx,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
            copied.extend(frame.copied);
        }
        assert!(copied.is_empty(), "点正文不触发复制:{copied:?}");
        assert_eq!(live.active, Some(2), "点块内正文应切入编辑态:{live:?}");
    }

    /// 链接点击归属富渲染层:打开 URL,不连带把块切进编辑(修复后富渲染
    /// 块的链接恢复点击 —— 修复前被整块矩形整体抢走)。
    #[test]
    fn live_link_click_opens_url_without_entering_edit() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new("[点我](https://example.com)\n\n后续。\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut live = LiveState {
            active: Some(1),
            ..LiveState::default()
        };

        let frame = live_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let (_, link_rect) = frame
            .texts
            .iter()
            .find(|(text, _)| text.contains("点我"))
            .expect("链接文本已渲染");

        let mut opened = Vec::new();
        for events in click_events(link_rect.center()) {
            let frame = live_frame(
                &ctx,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
            opened.extend(frame.opened);
        }
        assert_eq!(
            opened,
            vec!["https://example.com".to_owned()],
            "链接点击打开 URL"
        );
        assert_eq!(live.active, Some(1), "链接点击不切进编辑:{live:?}");
    }

    // —— LP2-1 活动块内联标记半隐藏 ——

    /// 半隐藏态的测试文档:六段标记(开/闭 `**`、`[`、`](…)`、开/闭 `` ` ``),
    /// 光标目标「粗体」两字之间 = 字符 3,不贴任何标记。
    const FADE_DOC: &str = "**粗体** 与 [链接](https://e.com) 和 `代码`\n";
    const FADE_BLOCK: usize = 0;
    const FADE_CARET_CONTENT: usize = 3;

    fn fade_state(text: &str) -> (EditorBuffer, PreviewState, OutlineCursor, LiveState) {
        let editor = EditorBuffer::new(text);
        let preview = PreviewState::new(&editor);
        (
            editor,
            preview,
            OutlineCursor::default(),
            LiveState {
                active: Some(FADE_BLOCK),
                ..LiveState::default()
            },
        )
    }

    /// 遮罩取证帧驱动共用的编辑器 id(各测试独立 `Context`,不互扰)。
    const FADE_EDITOR_ID: &str = "live-fade";

    /// 跑一帧 Live 面板(生产入口 `super::ui`),取证半透明遮罩矩形
    /// (以遮罩色从帧 shapes 里挑 Rect,与 #38/#41 的 shapes 取证同款)。
    fn live_fade_frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
        live: &mut LiveState,
    ) -> Vec<egui::Rect> {
        live_timed_frame(ctx, 0.0, events, editor, preview, cursor, live)
    }

    /// 带 `RawInput.time` 的帧驱动(LP2-2 手势测试专用):双击判定依赖
    /// 相邻两次 click 的时间差(input_state 的 max_double_click_delay),
    /// 必须显式给每帧一个递增时钟。
    fn live_timed_frame(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
        live: &mut LiveState,
    ) -> Vec<egui::Rect> {
        let mut fade = egui::Color32::TRANSPARENT;
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                super::ui(
                    ui,
                    editor,
                    preview,
                    cursor,
                    live,
                    egui::Id::new(FADE_EDITOR_ID),
                );
                let bg = ui.visuals().text_edit_bg_color();
                fade = egui::Color32::from_rgba_unmultiplied(
                    bg.r(),
                    bg.g(),
                    bg.b(),
                    super::MARK_FADE_ALPHA,
                );
            },
        );
        let rects = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Rect(shape) if shape.fill == fade => Some(shape.rect),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        rects
    }

    /// 光标在正文里:全部标记半隐藏,渲染一帧不 panic、不改动缓冲、不置脏。
    #[test]
    fn live_fades_marks_when_caret_in_content() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state(FADE_DOC);

        // 经 v1 的 pending_caret 把光标交到正文中间(第一帧只落光标)
        live.pending_caret = Some((FADE_BLOCK, FADE_CARET_CONTENT));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );

        let before = editor.text().to_owned();
        let rects = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert_eq!(editor.text(), before, "渲染不改动缓冲");
        assert!(!editor.is_dirty(), "渲染不置脏");
        assert_eq!(
            live.marks
                .as_ref()
                .map(|(_, _, bundle)| bundle.segments.len()),
            Some(6),
            "六段标记:{:?}",
            live.marks
        );
        assert!(
            rects.len() >= 6,
            "每段标记至少一枚遮罩(折行只会更多):{:?}",
            rects
        );
        // 标记缓存命中:再跑一帧仍是六段(修订号未动不重解析)
        live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert_eq!(
            live.marks
                .as_ref()
                .map(|(_, _, bundle)| bundle.segments.len()),
            Some(6)
        );

        // 暗色主题:遮罩色随 visuals 取,同样逐段落位(存在性,非目视裁决)
        ctx.set_visuals(egui::Visuals::dark());
        let dark = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert!(dark.len() >= 6, "暗色下遮罩仍在:{dark:?}");
    }

    /// 光标贴上标记 → 该段显形;选区压住标记 → 相交段显形。
    #[test]
    fn live_reveals_mark_under_caret_or_selection() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state(FADE_DOC);
        live.pending_caret = Some((FADE_BLOCK, FADE_CARET_CONTENT));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let base = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        )
        .len();
        assert!(base >= 6);

        // 光标挪进开头的 `**`(两星之间 = 字符 1)→ 该段显形
        live.pending_caret = Some((FADE_BLOCK, 1));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let on_mark = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        )
        .len();
        assert!(
            on_mark < base,
            "光标贴上后应有标记段显形:{on_mark} vs 基准 {base}"
        );

        // 选区盖住链接前后(字符 9..14:`[`、链接文本、`)` → 链接两段显形
        let id = egui::Id::new("live-fade").with(("live-block", FADE_BLOCK));
        let mut state =
            egui::widgets::text_edit::TextEditState::load(&ctx, id).expect("状态已就位");
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(9),
                egui::text::CCursor::new(14),
            )));
        state.store(&ctx, id);
        let selected = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        )
        .len();
        assert!(
            selected + 2 <= base,
            "链接两段(`[` 与 `](…)`)应显形:{selected} vs 基准 {base}"
        );
    }

    /// 点击半隐藏的标记:遮罩不参与命中测试 —— TextEdit 照常获得焦点,光标
    /// 落到标记上,下一帧该段显形。
    #[test]
    fn live_click_on_faded_mark_focuses_editor_and_reveals() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state(FADE_DOC);
        live.pending_caret = Some((FADE_BLOCK, FADE_CARET_CONTENT));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let base_rects = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert!(base_rects.len() >= 6);

        // 点最靠左的遮罩矩形(即开头的 `**`)中心
        let target = base_rects
            .iter()
            .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
            .expect("至少一枚遮罩")
            .center();
        for events in click_events(target) {
            let _ = live_fade_frame(
                &ctx,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
        }
        let id = egui::Id::new("live-fade").with(("live-block", FADE_BLOCK));
        assert!(
            ctx.memory(|mem| mem.has_focus(id)),
            "点击应命中活动块 TextEdit 本体(遮罩不参与 hit-test)"
        );
        let clicked = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        )
        .len();
        assert!(
            clicked < base_rects.len(),
            "点击落点在标记上,该段应显形:{clicked} vs 基准 {}",
            base_rects.len()
        );
    }

    /// 编辑语义不回退:活动块里打一个字仍直落同一份全文缓冲(undo 栈共用),
    /// 标记缓存随修订号前进失效重算。
    #[test]
    fn live_typing_in_active_block_updates_marks_cache() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state(FADE_DOC);
        live.pending_caret = Some((FADE_BLOCK, FADE_CARET_CONTENT));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let (rev_before, _, _) = live.marks.clone().expect("缓存已建立");

        // 在「粗体」内容里插一个字(走 BlockBuffer 同一条路,= TextEdit 打字)
        editor.insert_chars(FADE_CARET_CONTENT, "字");
        assert!(editor.text().contains("粗字体**"), "落在同一份缓冲");
        assert!(editor.is_dirty());

        live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let (rev_after, _, marks) = live.marks.clone().expect("缓存已重算");
        assert!(rev_after > rev_before, "修订号前进,缓存键换新");
        assert_eq!(marks.segments.len(), 6, "标记段数不变:{:?}", marks.segments);
        // 插入点在开 `**` 之后:闭 `**` 及其后所有区间右移一个 CJK 字(3 字节)
        let text_now = editor.text();
        assert_eq!(
            &text_now[marks.segments[1].clone()],
            "**",
            "闭 ** 仍标对位置"
        );
        assert_eq!(
            marks.segments[1],
            11..13,
            "原 8..10 平移一个 CJK 字(3 字节):{marks:?}"
        );
    }

    // —— LP2-2 选区扩展与配对显形 ——

    /// 手势测试文档:一对 `**` 包着四个等宽字母(内容中部/四分位的点击
    /// 坐标可从两枚遮罩矩形折算),后跟普通文本。
    const GESTURE_DOC: &str = "**abcd** 常规文本\n";
    const GESTURE_BLOCK: usize = 0;
    /// `**abcd**` 的完整字符区间(选区扩展的目标)。
    const GESTURE_PAIR: Range<usize> = 0..8;

    fn gesture_state(text: &str) -> (EditorBuffer, PreviewState, OutlineCursor, LiveState) {
        let editor = EditorBuffer::new(text);
        let preview = PreviewState::new(&editor);
        (
            editor,
            preview,
            OutlineCursor::default(),
            LiveState {
                active: Some(GESTURE_BLOCK),
                ..LiveState::default()
            },
        )
    }

    /// 跑一帧带时钟的 Live 面板,返回遮罩矩形(双击判定依赖帧时间)。
    fn gesture_frame(
        ctx: &egui::Context,
        time: f64,
        events: Vec<egui::Event>,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
        live: &mut LiveState,
    ) -> Vec<egui::Rect> {
        live_timed_frame(ctx, time, events, editor, preview, cursor, live)
    }

    /// 活动块 TextEdit 的当前选区 (min, max)(字符偏移)。
    fn block_selection(ctx: &egui::Context) -> Option<(usize, usize)> {
        let id = egui::Id::new(FADE_EDITOR_ID).with(("live-block", GESTURE_BLOCK));
        egui::widgets::text_edit::TextEditState::load(ctx, id).and_then(|state| {
            state.cursor.char_range().map(|range| {
                let (a, b) = (range.primary.index.0, range.secondary.index.0);
                (a.min(b), a.max(b))
            })
        })
    }

    /// 主键按下/抬起事件。
    fn button_event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// 两枚 `**` 遮罩矩形按 x 排序成 (开侧, 闭侧)。
    fn pair_rects(rects: &[egui::Rect]) -> (egui::Rect, egui::Rect) {
        let mut sorted = rects.to_vec();
        sorted.sort_by(|a, b| a.min.x.total_cmp(&b.min.x));
        assert_eq!(sorted.len(), 2, "恰两枚 `**` 遮罩:{rects:?}");
        (sorted[0], sorted[1])
    }

    /// 渲染不 panic 且无副作用:标记块 + 非空选区状态连跑三帧(含选
    /// 区触碰标记的形态),缓冲不动、不置脏。
    #[test]
    fn live_renders_marks_and_selection_frames_without_panic() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state(FADE_DOC);
        live.pending_caret = Some((FADE_BLOCK, FADE_CARET_CONTENT));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );

        // 直接把一个非空选区写进 TextEdit 状态(盖住链接开 `[` 与部分内容,
        // 触碰标记的形态)
        let id = egui::Id::new("live-fade").with(("live-block", FADE_BLOCK));
        let mut state =
            egui::widgets::text_edit::TextEditState::load(&ctx, id).expect("状态已就位");
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(9),
                egui::text::CCursor::new(14),
            )));
        state.store(&ctx, id);

        let before = editor.text().to_owned();
        for _ in 0..3 {
            let _ = live_fade_frame(
                &ctx,
                Vec::new(),
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
        }
        assert_eq!(editor.text(), before, "渲染不改动缓冲");
        assert!(!editor.is_dirty(), "渲染不置脏");
    }

    /// 双击标记对内的词:选区一次性扩到完整标记对(内容+两侧标记),
    /// 扩展后两段标记随选区显形;再跑一帧不二次改写(扩展不持续吸附)。
    #[test]
    fn live_double_click_inside_pair_expands_selection_once() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = gesture_state(GESTURE_DOC);
        live.pending_caret = Some((GESTURE_BLOCK, 12)); // 光标放普通文本里
        let _ = gesture_frame(
            &ctx,
            0.0,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let rects = gesture_frame(
            &ctx,
            0.05,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let (opening, closing) = pair_rects(&rects);
        // 内容中部 = 四个等宽字母的正中(b/c 边界)
        let content_mid = egui::pos2((opening.right() + closing.left()) / 2.0, opening.center().y);

        // 双击 = 两次完整点击,第二次抬起帧 egui 判定 double_click 并选词
        for (time, pressed) in [(0.10, true), (0.15, false), (0.25, true), (0.30, false)] {
            let _ = gesture_frame(
                &ctx,
                time,
                vec![button_event(content_mid, pressed)],
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
        }
        assert_eq!(
            block_selection(&ctx),
            Some((GESTURE_PAIR.start, GESTURE_PAIR.end)),
            "双击词 abcd → 扩到 **abcd**"
        );

        // 稳定帧:选区保持原样,盖住整对 → 两段标记显形(遮罩清零)
        let rects = gesture_frame(
            &ctx,
            0.40,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert!(rects.is_empty(), "扩展选区盖住整对,两段都显形:{rects:?}");
        assert_eq!(
            block_selection(&ctx),
            Some((GESTURE_PAIR.start, GESTURE_PAIR.end)),
            "扩展是一次性的,静止帧不再改写选区"
        );
        assert_eq!(editor.text(), GESTURE_DOC, "手势不改文本");
    }

    /// 拖选松手:从标记上发起(锚点压着开 `**`)→ 扩到整对;从内容中部
    /// 发起的普通拖选 → 保持原选区不吸附(最小惊讶)。
    #[test]
    fn live_drag_release_expands_only_from_mark_anchor() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = gesture_state(GESTURE_DOC);
        live.pending_caret = Some((GESTURE_BLOCK, 12));
        let _ = gesture_frame(
            &ctx,
            0.0,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let rects = gesture_frame(
            &ctx,
            0.05,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let (opening, closing) = pair_rects(&rects);
        let on_mark = egui::pos2(opening.left() + opening.width() * 0.25, opening.center().y);
        let content_mid = egui::pos2((opening.right() + closing.left()) / 2.0, opening.center().y);
        let content_quarter = egui::pos2(
            opening.right() + (closing.left() - opening.right()) * 0.25,
            opening.center().y,
        );

        // ① 按在开 `**` 上、拖到内容中部松手 → 扩到整对
        for (time, events) in [
            (0.10, vec![egui::Event::PointerMoved(on_mark)]),
            (0.15, vec![button_event(on_mark, true)]),
            (0.30, vec![egui::Event::PointerMoved(content_mid)]),
            (0.40, vec![button_event(content_mid, false)]),
        ] {
            let _ = gesture_frame(
                &ctx,
                time,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
        }
        assert_eq!(
            block_selection(&ctx),
            Some((GESTURE_PAIR.start, GESTURE_PAIR.end)),
            "从标记发起的拖选松手 → 扩到 **abcd**"
        );

        // ② 从内容中部(四分位)发起、拖到闭 `**` 边界松手 → 保持原选区
        for (time, events) in [
            (0.60, vec![egui::Event::PointerMoved(content_quarter)]),
            (0.65, vec![button_event(content_quarter, true)]),
            (
                0.80,
                vec![egui::Event::PointerMoved(egui::pos2(
                    closing.left(),
                    opening.center().y,
                ))],
            ),
            (
                0.90,
                vec![button_event(
                    egui::pos2(closing.left(), opening.center().y),
                    false,
                )],
            ),
        ] {
            let _ = gesture_frame(
                &ctx,
                time,
                events,
                &mut editor,
                &mut preview,
                &mut cursor,
                &mut live,
            );
        }
        // 锚点在 b/c 之间的内容里:普通拖选,选区保持 bcd(3..6)不吸附
        assert_eq!(
            block_selection(&ctx),
            Some((3, 6)),
            "内容中部发起的拖选不扩展:{:?}",
            block_selection(&ctx)
        );
        assert_eq!(editor.text(), GESTURE_DOC, "手势不改文本");
    }

    /// 配对显形:选区只盖住一侧标记,配对**另一侧**所在段同时显形 ——
    /// 这是 LP2-1「只显形被压住的段」之上的增量(该实现会留 3 枚遮罩)。
    #[test]
    fn live_selection_on_mark_reveals_partner_segment() {
        let ctx = egui::Context::default();
        let (mut editor, mut preview, mut cursor, mut live) = fade_state("**粗** x *i*\n");
        // 光标放到两组标记之间的 `x` 上(字符 6):不贴任何标记
        live.pending_caret = Some((0, 6));
        let _ = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        let base = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert_eq!(base.len(), 4, "两对标记四枚遮罩:{base:?}");

        // 非空选区只盖住开 `**`(字符 0..2):开闭两段显形,`*i*` 仍半隐藏
        let id = egui::Id::new("live-fade").with(("live-block", 0));
        let mut state =
            egui::widgets::text_edit::TextEditState::load(&ctx, id).expect("状态已就位");
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(2),
            )));
        state.store(&ctx, id);
        let rects = live_fade_frame(
            &ctx,
            Vec::new(),
            &mut editor,
            &mut preview,
            &mut cursor,
            &mut live,
        );
        assert_eq!(
            rects.len(),
            2,
            "配对闭侧显形 + 无关的 *i* 两段保持半隐藏:{rects:?}"
        );
    }
}
