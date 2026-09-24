//! 编辑面板:等宽 multiline `TextEdit`,rope 即缓冲。
//!
//! egui 0.36 的 `TextEdit` 通过 [`egui::TextBuffer`] 驱动编辑:每个按键、
//! IME 组合、内建 undo/redo 最终都落成 `insert_text` / `delete_char_range`
//! 调用,rope 因此天然吃到增量编辑,不存在"整段字符串重写"路径。
//! undo/redo 用 `TextEdit` 内建 undoer(快照存于其 widget state)。

use crate::state::PreviewState;
use latermd_editor::EditorBuffer;
use std::ops::Range;

use eframe::egui;

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

/// 绘制编辑面板,并在控件返回后维护预览快照。返回 TextEdit 的响应
/// (焦点/交互归因用,测试也用它拿 widget id)。
pub fn ui(
    panel: &mut egui::Ui,
    editor: &mut EditorBuffer,
    preview: &mut PreviewState,
) -> egui::Response {
    panel.horizontal(|ui| {
        ui.weak("源码");
        if editor.is_dirty() {
            ui.weak("· 已修改");
        }
    });

    let line_height = {
        let font = egui::FontSelection::Style(egui::TextStyle::Monospace).resolve(panel.style());
        panel.fonts_mut(|f| f.row_height(&font)) + panel.spacing().extra_text_line_spacing
    };
    // 面板剩余高度铺满编辑区(TextEdit 无 fill-height 选项,换算成行数)。
    let rows = (panel.available_height() / line_height).floor().max(1.0) as usize;

    let mut buffer = EditorText(editor);
    let output = egui::TextEdit::multiline(&mut buffer)
        // 稳定 id:光标/undo 状态跨帧保持;同样绝不能含内容长度或 hash
        .id_salt("source-editor")
        .font(egui::TextStyle::Monospace)
        .desired_width(f32::INFINITY)
        .desired_rows(rows)
        .lock_focus(true)
        .show(panel);

    // 快照只在修订号前进时重建;这是"避免每帧重解析"的第一层,
    // vendored 层的 text hash 缓存是第二层。
    if preview.synced_rev != editor.revision() {
        preview.text = editor.text().to_owned();
        preview.synced_rev = editor.revision();
    }
    output.response.response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers, RawInput};

    /// 跑一帧编辑面板,返回 TextEdit 的 widget id。`now` 逐帧递增:
    /// undoer 靠输入时间戳切分撤销组,恒定时间会把多次输入并成一步。
    fn frame(
        ctx: &egui::Context,
        events: Vec<Event>,
        now: f64,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
    ) -> egui::Id {
        let id = std::cell::Cell::new(egui::Id::NULL);
        let output = ctx.run_ui(
            RawInput {
                events,
                time: Some(now),
                ..Default::default()
            },
            |ui| {
                id.set(super::ui(ui, editor, preview).id);
            },
        );
        // egui 0.36 的 TexturesDelta drop 检查:测试里不消费绘制增量,
        // 显式丢弃(与 vendored 层测试同一处理)
        output.drop_without_applying_deltas();
        id.get()
    }

    fn test_ctx() -> egui::Context {
        // 不用 FontDefinitions::empty():空字体下 galley 不含任何字符,
        // clamp_cursor 会把光标一律钳到 0,插入位置全部退化为行首。
        egui::Context::default()
    }

    /// 中文文本事件(与 IME 组合落定时同一 `insert_text` 路径)必须逐字
    /// 进入 rope,并在同一帧刷新预览快照。
    #[test]
    fn typing_flows_into_rope_and_refreshes_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState {
            text: String::new(),
            synced_rev: editor.revision(),
        };

        let id = frame(&ctx, Vec::new(), 0.0, &mut editor, &mut preview);
        ctx.memory_mut(|m| m.request_focus(id)); // 等价于用户点击编辑区
        frame(
            &ctx,
            vec![Event::Text("你好,".into()), Event::Text("world".into())],
            0.1,
            &mut editor,
            &mut preview,
        );

        assert_eq!(editor.text(), "你好,world");
        assert!(editor.is_dirty());
        assert!(editor.revision() >= 2, "每个事件独立推进修订号");
        assert_eq!(preview.text, "你好,world", "快照与缓冲一致");
        assert_eq!(preview.synced_rev, editor.revision());
    }

    /// 空闲帧(无输入)不推进修订号、不重建快照。
    #[test]
    fn idle_frames_do_not_touch_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("x");
        let mut preview = PreviewState {
            text: editor.text().to_owned(),
            synced_rev: editor.revision(),
        };
        let id = frame(&ctx, Vec::new(), 0.0, &mut editor, &mut preview);
        let rev = editor.revision();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(&ctx, Vec::new(), 0.1, &mut editor, &mut preview);
        frame(&ctx, Vec::new(), 0.2, &mut editor, &mut preview);

        let text_ptr = preview.text.as_ptr();
        frame(&ctx, Vec::new(), 0.3, &mut editor, &mut preview);
        assert_eq!(editor.revision(), rev);
        assert_eq!(preview.synced_rev, rev);
        assert_eq!(preview.text.as_ptr(), text_ptr, "空闲帧未重建快照字符串");
    }

    /// TextEdit 内建 undo/redo:Ctrl+Z 回退一步,Ctrl+Shift+Z 重做。
    #[test]
    fn builtin_undo_redo() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState {
            text: String::new(),
            synced_rev: 0,
        };

        let id = frame(&ctx, Vec::new(), 0.0, &mut editor, &mut preview);
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Text("一".into())],
            0.1,
            &mut editor,
            &mut preview,
        );
        // egui undoer 按输入稳定时长(stable_time,默认 1s)切分撤销组:
        // 空转一帧让「一」成为已提交的撤销点,再输入「二」。
        frame(&ctx, Vec::new(), 1.5, &mut editor, &mut preview);
        frame(
            &ctx,
            vec![Event::Text("二".into())],
            1.6,
            &mut editor,
            &mut preview,
        );
        assert_eq!(editor.text(), "一二");

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(&ctx, vec![undo], 1.7, &mut editor, &mut preview);
        assert_eq!(editor.text(), "一", "undo 一步");
        assert_eq!(preview.text, "一");

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        frame(&ctx, vec![redo], 1.8, &mut editor, &mut preview);
        assert_eq!(editor.text(), "一二", "redo 恢复");
        assert_eq!(preview.text, "一二");
    }
}
