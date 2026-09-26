//! 编辑面板:等宽 multiline `TextEdit`,rope 即缓冲。
//!
//! egui 0.36 的 `TextEdit` 通过 [`egui::TextBuffer`] 驱动编辑:每个按键、
//! IME 组合、内建 undo/redo 最终都落成 `insert_text` / `delete_char_range`
//! 调用,rope 因此天然吃到增量编辑,不存在"整段字符串重写"路径。
//! undo/redo 用 `TextEdit` 内建 undoer(快照存于其 widget state)。
//! 大纲跳转也在这里应用:覆写 TextEdit 持久光标并交还焦点。

use crate::live::{self, LiveState, RenderMode};
use crate::state::{OutlineCursor, PreviewState};
use latermd_editor::EditorBuffer;
use std::ops::Range;

use eframe::egui;

/// 编辑器 widget 的 id:由**标签的稳定 id** 派生(多标签 #11)—— 每个标签
/// 一套 TextEdit 持久状态(光标/undo/焦点),切标签零恢复逻辑;标签关闭后
/// id 不复用(`TabsState::next_id` 自增),新标签不会继承旧标签的光标。
pub(crate) fn tab_editor_id(tab_id: u64) -> egui::Id {
    egui::Id::new("source-editor").with(tab_id)
}

/// 把 [`EditorBuffer`] 适配成 egui `TextBuffer` 的 newtype。
///
/// 孤儿规则:`egui::TextBuffer` 与 `EditorBuffer` 都不归本 crate,
/// 直接 impl 不合法,只能包本地类型。放 app 层同时守住了
/// `latermd-editor` 不依赖 UI 框架的铁律。
struct EditorText<'a>(&'a mut EditorBuffer);

impl egui::TextBuffer for EditorText<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.0.text()
    }

    fn insert_text(&mut self, text: &str, char_index: egui::text::CharIndex) -> usize {
        self.0.insert_chars(char_index.0, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<egui::text::CharIndex>) {
        self.0.remove_chars(char_range.start.0..char_range.end.0);
    }

    // trait 无默认实现;照上游文档示例返回 TypeId(供 downcast 用)。
    // 生命周期参数不参与 TypeId,统一取 'static 形态。
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<EditorText<'static>>()
    }
}

/// `ui::editor` 与 `State` 之间的**光标协调通道**,每帧双向跑一次:
///
/// * 出去:`cursor.byte`(大纲高亮)、`selection`(格式工具条的输入);
/// * 进来:`cursor.jump_to`(大纲跳转)、`pending`(格式动作产出的新选区)。
///
/// 三者都是「UI 每帧回填/消费」的同族字段 —— 收成结构体之前它们是三条
/// 并排的 `&mut` 实参,把 [`ui`] 顶到 9 个(clippy `too_many_arguments`),
/// 而它们从来都是一起传的。
pub struct CursorChannel<'a> {
    /// 大纲↔编辑器:当前光标字节偏移出去,跳转目标进来。
    pub cursor: &'a mut OutlineCursor,
    /// 当前选区(字符区间),给格式工具条当输入;`None` = 这一帧还没渲染过。
    pub selection: &'a mut Option<(usize, usize)>,
    /// 待写回的新选区,由归约侧挂上、本模块消费。
    pub pending: &'a mut Option<(usize, usize)>,
}

/// 绘制编辑面板,并在控件返回后维护预览快照与光标协调。返回 TextEdit
/// 的响应(焦点/交互归因用,测试也用它拿 widget id)。
pub fn ui(
    panel: &mut egui::Ui,
    editor: &mut EditorBuffer,
    preview: &mut PreviewState,
    channel: CursorChannel<'_>,
    live: &mut LiveState,
    mode: RenderMode,
    editor_id: egui::Id,
) -> egui::Response {
    let CursorChannel {
        cursor,
        selection,
        pending,
    } = channel;
    panel.horizontal(|ui| {
        ui.weak(mode.label());
        if editor.is_dirty() {
            ui.weak("· 已修改");
        }
    });
    // 两种模式共用同一个 rope buffer 与同一套撤销语义(roadmap 铁律):这里
    // 只是分派,没有任何「把光标/文本从一种模式搬到另一种」的恢复逻辑。
    if mode == RenderMode::Live {
        return live::ui(panel, editor, preview, cursor, live, editor_id);
    }

    let line_height = {
        let font = egui::FontSelection::Style(egui::TextStyle::Monospace).resolve(panel.style());
        panel.fonts_mut(|f| f.row_height(&font)) + panel.spacing().extra_text_line_spacing
    };
    // 面板剩余高度铺满编辑区(TextEdit 无 fill-height 选项,换算成行数)。
    let rows = (panel.available_height() / line_height).floor().max(1.0) as usize;

    let mut buffer = EditorText(editor);
    let output = egui::TextEdit::multiline(&mut buffer)
        // 稳定 id:光标/undo 状态跨帧保持;同样绝不能含内容长度或 hash
        .id(editor_id)
        .font(egui::TextStyle::Monospace)
        .desired_width(f32::INFINITY)
        .desired_rows(rows)
        .lock_focus(true)
        .show(panel);

    // 快照只在修订号前进时重建(文本 + 大纲同源);这是"避免每帧重解析"
    // 的第一层,vendored 层的 text hash 缓存是第二层。
    if preview.synced_rev != editor.revision() {
        preview.rebuild(editor);
    }

    // 光标位置回填(大纲「当前小节」高亮用):主光标的字符偏移按当前缓冲
    // 换成字节。读持久化的 cursor 而非 output.cursor_range —— 后者在失焦
    // 帧为 None,而持久化值保留上次位置。
    cursor.byte = output
        .state
        .cursor
        .char_range()
        .map(|range| editor.char_to_byte(range.primary.index.0));

    // 格式工具条的选区双向通道(docs/ui-shell-redesign.md §6.4)。
    //
    // 出去:把当前 `CCursorRange` 抄给 `tab.selection` —— 按钮被点中时编辑器
    // 已失焦,而选区活在这个持久 state 里,归约侧拿不到。读持久化的 cursor
    // 而不是 `output.cursor_range`(后者在失焦帧是 None)。
    *selection = output
        .state
        .cursor
        .char_range()
        .map(|range| (range.primary.index.0, range.secondary.index.0));

    // 进来:格式动作产出的新选区,写回 TextEdit 持久 cursor 并把焦点还给
    // 编辑器 —— 否则用户还得自己点回编辑区才能继续打字。
    let mut format_result = pending.take();
    if let Some(jump) = cursor.jump_to.take() {
        format_result = Some((jump, jump));
    }
    if let Some((start, end)) = format_result {
        write_selection(
            panel,
            &output.response.response.id,
            &output.state,
            start,
            end,
        );
    }
    output.response.response
}

/// 把字符区间写进 `TextEdit` 的持久 cursor,并请求焦点。
fn write_selection(
    panel: &egui::Ui,
    id: &egui::Id,
    state: &egui::widgets::text_edit::TextEditState,
    start: usize,
    end: usize,
) {
    let mut state = state.clone();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )));
    state.store(panel.ctx(), *id);
    panel.ctx().memory_mut(|mem| mem.request_focus(*id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::widgets::text_edit::TextEditState;
    use egui::{Event, Key, Modifiers, RawInput};

    /// 跑一帧编辑面板,返回 TextEdit 的 widget id。`now` 逐帧递增:
    /// undoer 靠输入时间戳切分撤销组,恒定时间会把多次输入并成一步。
    fn frame(
        ctx: &egui::Context,
        events: Vec<Event>,
        now: f64,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
    ) -> egui::Id {
        let mut selection = None;
        let mut pending = None;
        frame_with_channel(
            ctx,
            events,
            now,
            editor,
            preview,
            &mut selection,
            &mut pending,
            cursor,
        )
    }

    /// 同 [`frame`],额外把 §6.4 的两个槽位交给调用方。八条实参里五条是
    /// 同一帧的被测对象、一起传是大势所趋,故单独豁免形参计数 lint ——
    /// 它只作用于本测试辅助,不去污染 `ui` 的 API 面。
    #[allow(clippy::too_many_arguments)]
    fn frame_with_channel(
        ctx: &egui::Context,
        events: Vec<Event>,
        now: f64,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        selection: &mut Option<(usize, usize)>,
        pending: &mut Option<(usize, usize)>,
        cursor: &mut OutlineCursor,
    ) -> egui::Id {
        let mut live = LiveState::default();
        let id = std::cell::Cell::new(egui::Id::NULL);
        let output = ctx.run_ui(
            RawInput {
                events,
                time: Some(now),
                ..Default::default()
            },
            |ui| {
                id.set(
                    super::ui(
                        ui,
                        editor,
                        preview,
                        CursorChannel {
                            cursor,
                            selection,
                            pending,
                        },
                        &mut live,
                        RenderMode::Source,
                        tab_editor_id(1),
                    )
                    .id,
                );
            },
        );
        // egui 0.36 的 TexturesDelta drop 检查:测试里不消费绘制增量,
        // 显式丢弃(与 vendored 层测试同一处理)
        output.drop_without_applying_deltas();
        id.get()
    }

    /// §6.4 通道的**回来那一半**:归约把新选区挂到 `pending`,本模块把它写
    /// 进 `TextEdit` 持久 cursor 并把焦点还给编辑器。
    ///
    /// 归约那一半(`FormatRequested` → 文本 + `pending_selection`)由
    /// `state::tests::format_requested_rewrites_buffer_and_stages_selection`
    /// 钉住,两边分开测是因为它们中间的耦合是**字符偏移**这个契约,不是
    /// 彼此的内部实现。
    #[test]
    fn pending_selection_is_written_back_as_persisted_cursor() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("**甲乙丙**");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = Some((2, 5));

        let id = frame_with_channel(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );

        assert!(pending.is_none(), "待写回选区已被消费");
        let state = TextEditState::load(&ctx, id).expect("已持久化 widget state");
        let range = state.cursor.char_range().expect("选区已写入");
        let span = [
            range.primary.index.0.min(range.secondary.index.0),
            range.primary.index.0.max(range.secondary.index.0),
        ];
        assert_eq!(span, [2, 5], "TextEdit 会归一化端点顺序,故只比对区间");
        assert!(ctx.memory(|m| m.has_focus(id)), "焦点还给编辑器");
    }

    /// §6.4 通道的**出去那一半**:`TextEdit` 的选区被抄到 `selection`,这样
    /// 工具条按钮点击那一帧(编辑器已失焦)归约侧才拿得到它。
    ///
    /// 用 `pending` 灌一个已知选区、下一帧读回 —— 比靠按键推光标稳得多:
    /// 后者经由 `TextEdit` 内部的 `clamp` / 归一化,端点到不准 JB,而这里要
    /// 钉的是「搬运工」本身,不是 `TextEdit` 的光标规则。
    #[test]
    fn selection_is_mirrored_out_for_the_format_bar() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = Some((1, 3));

        frame_with_channel(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        // 写入发生在回填之后(同帧先读后写),故这一帧读到的还是旧值
        frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );

        let recorded = selection.expect("选区已回填");
        let span = [recorded.0.min(recorded.1), recorded.0.max(recorded.1)];
        assert_eq!(span, [1, 3], "上一帧写进去的选区被搬了出来");
    }

    fn test_ctx() -> egui::Context {
        // 不用 FontDefinitions::empty():空字体下 galley 不含任何字符,
        // clamp_cursor 会把光标一律钳到 0,插入位置全部退化为行首。
        egui::Context::default()
    }

    /// 中文文本事件(与 IME 组合落定时同一 `insert_text` 路径)必须逐字
    /// 进入 rope,并在同一帧刷新预览快照与大纲。
    #[test]
    fn typing_flows_into_rope_and_refreshes_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("# 首标题");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id)); // 等价于用户点击编辑区
        frame(
            &ctx,
            vec![
                Event::Text("\n\n## 次标题".into()),
                Event::Text("more".into()),
            ],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        assert_eq!(editor.text(), "# 首标题\n\n## 次标题more");
        assert!(editor.is_dirty());
        assert!(editor.revision() >= 2, "每个事件独立推进修订号");
        assert_eq!(preview.text, editor.text(), "快照与缓冲一致");
        assert_eq!(preview.synced_rev, editor.revision());
        let levels: Vec<u8> = preview.outline.iter().map(|item| item.level).collect();
        assert_eq!(levels, vec![1, 2], "大纲随编辑同帧重算");
    }

    /// 空闲帧(无输入)不推进修订号、不重建快照。
    #[test]
    fn idle_frames_do_not_touch_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("x");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let rev = editor.revision();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let text_ptr = preview.text.as_ptr();
        frame(
            &ctx,
            Vec::new(),
            0.3,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.revision(), rev);
        assert_eq!(preview.synced_rev, rev);
        assert_eq!(preview.text.as_ptr(), text_ptr, "空闲帧未重建快照字符串");
    }

    /// TextEdit 内建 undo/redo:Ctrl+Z 回退一步,Ctrl+Shift+Z 重做。
    #[test]
    fn builtin_undo_redo() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Text("一".into())],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // egui undoer 按输入稳定时长(stable_time,默认 1s)切分撤销组:
        // 空转一帧让「一」成为已提交的撤销点,再输入「二」。
        frame(
            &ctx,
            Vec::new(),
            1.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            vec![Event::Text("二".into())],
            1.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一二");

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            &ctx,
            vec![undo],
            1.7,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一", "undo 一步");
        assert_eq!(preview.text, "一");

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        frame(
            &ctx,
            vec![redo],
            1.8,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一二", "redo 恢复");
        assert_eq!(preview.text, "一二");
    }

    /// AI 流式追加(程序化 insert_chars)后的 undo 语义,实测钉住:
    /// TextEdit 内建 undoer 只在绘制时喂状态,看不到程序化插入 —— 流式
    /// 结束后第一次 Ctrl+Z 整体回退到最近一次用户编辑的快照(即「一步
    /// 撤销整段 AI 续写」),再按 Ctrl+Shift+Z 重做可恢复 AI 文本。
    #[test]
    fn undo_after_ai_append_reverts_whole_stream() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Text("一".into())],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 空转让「一」成为已提交的撤销点(undoer 按 stable_time 切组)
        frame(
            &ctx,
            Vec::new(),
            1.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        // AI 流式追加(与 Message::AiChunk 归约同路径:程序化 insert_chars)
        editor.insert_chars(editor.len_chars(), "AI续写内容");
        frame(
            &ctx,
            Vec::new(),
            1.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一AI续写内容");

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            &ctx,
            vec![undo],
            1.7,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(
            editor.text(),
            "一",
            "一步撤销整段 AI 续写(回到最近用户编辑快照)"
        );

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        frame(
            &ctx,
            vec![redo],
            1.8,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一AI续写内容", "redo 恢复 AI 文本");
    }

    /// 大纲跳转:消费 jump_to 后,TextEdit 持久光标被覆写到目标字符偏移,
    /// 焦点回到编辑器;下一帧输入从新位置继续。
    #[test]
    fn outline_jump_sets_persisted_cursor_and_focus() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("# 甲\n\n正文\n\n## 乙\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(cursor.byte, None, "尚无任何光标交互");

        // 归约产出的跳转目标:二级标题行首(字符偏移)
        let heading_byte = editor.text().find("##").unwrap();
        let target = editor.byte_to_char(heading_byte);
        cursor.jump_to = Some(target);
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let state = TextEditState::load(&ctx, id).expect("已持久化 widget state");
        let range = state.cursor.char_range().expect("光标已覆写");
        assert_eq!(range.primary.index.0, target);
        assert_eq!(range.secondary.index.0, target, "无选区,两端一致");
        assert!(ctx.memory(|m| m.has_focus(id)), "焦点已还给编辑器");
        assert!(cursor.jump_to.is_none(), "跳转请求已消费");
        assert_eq!(cursor.byte, None, "回填发生在覆写之前,跳转当帧仍是旧值(空)");

        // 下一帧:输入从跳转处继续(打到 '#' 前),编辑器仍持焦点
        frame(
            &ctx,
            vec![Event::Text("!".into())],
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert!(editor.text().contains("\n\n!## 乙"), "从新光标处插入");
        assert_eq!(cursor.byte, Some(heading_byte + 1), "光标随输入前进");
    }
}
