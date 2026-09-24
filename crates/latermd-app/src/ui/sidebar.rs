//! 侧边栏:页签栏 + 当前页签内容。大纲页签接真数据(P0 廉价版:点击跳
//! 编辑器光标,不滚动预览),文件树/搜索仍占位。

use crate::state::{Message, SidebarTab};
use latermd_md::OutlineItem;

use eframe::egui;

/// 大纲层级每深一级的缩进宽度(px)。
const OUTLINE_INDENT: f32 = 14.0;

/// 绘制侧边栏内容。
///
/// `active_tab` 与 `outbox` 是从 `App` 上解构出的不相交借用,使本函数可以与
/// `show_collapsible` 原地持有的 `&mut visible` 并存(见 `layout.rs`)。
pub fn ui(
    panel: &mut egui::Ui,
    active_tab: &mut SidebarTab,
    outline: &[OutlineItem],
    cursor_byte: Option<usize>,
    outbox: &mut Vec<Message>,
) {
    tab_bar(panel, active_tab, outbox);
    panel.add_space(4.0);
    match *active_tab {
        SidebarTab::Files => placeholder(panel, "文件树占位(P0 文件树基础版)"),
        SidebarTab::Search => placeholder(panel, "搜索占位(P1)"),
        SidebarTab::Outline => outline_panel(panel, outline, cursor_byte, outbox),
    }
}

/// 页签栏。点击只发消息,归约在下一帧 `App::logic` 完成。
fn tab_bar(panel: &mut egui::Ui, active_tab: &mut SidebarTab, outbox: &mut Vec<Message>) {
    panel.horizontal(|ui| {
        for tab in SidebarTab::ALL {
            if ui
                .selectable_label(*active_tab == tab, tab.label())
                .clicked()
            {
                outbox.push(Message::SidebarTabChanged(tab));
            }
        }
    });
}

/// 大纲列表。数据来自 [`crate::state::PreviewState`] 的快照,文档变化时随
/// 预览同一时机重算,空闲帧零开销。
fn outline_panel(
    panel: &mut egui::Ui,
    outline: &[OutlineItem],
    cursor_byte: Option<usize>,
    outbox: &mut Vec<Message>,
) {
    egui::ScrollArea::vertical()
        .id_salt("outline-scroll")
        // 不收缩宽度,让长标题换行而不是把面板撑宽
        .auto_shrink([false, false])
        .show(panel, |ui| {
            if outline.is_empty() {
                ui.weak("无标题:文档里还没有 Markdown 标题");
            }
            let active = active_index(outline, cursor_byte);
            for (index, item) in outline.iter().enumerate() {
                outline_row(ui, item, active == Some(index), outbox);
            }
        });
}

/// 光标所在的当前小节:起始位置不晚于光标的最后一个标题。
///
/// 光标落在正文段落里时高亮其所属小节(与常见编辑器大纲一致);无光标
/// 信息或光标在首个标题之前则不高亮。
fn active_index(outline: &[OutlineItem], cursor_byte: Option<usize>) -> Option<usize> {
    cursor_byte.and_then(|cursor| outline.iter().rposition(|item| item.span.start <= cursor))
}

/// 单条大纲:按层级缩进的 SelectableLabel,点击发消息(跳转在 `logic`
/// 归约 + `ui::editor` 应用)。返回标签响应,独立成函数便于点击测试定位。
fn outline_row(
    ui: &mut egui::Ui,
    item: &OutlineItem,
    selected: bool,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let response = ui
        .horizontal(|ui| {
            ui.add_space(OUTLINE_INDENT * item.level.saturating_sub(1) as f32);
            ui.selectable_label(selected, &item.text)
        })
        .inner;
    if response.clicked() {
        outbox.push(Message::OutlineItemClicked(item.span.clone()));
    }
    response
}

fn placeholder(panel: &mut egui::Ui, text: &str) {
    panel.weak(text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    fn item(level: u8, text: &str, start: usize) -> OutlineItem {
        OutlineItem {
            level,
            text: text.to_owned(),
            span: start..start + text.len(),
        }
    }

    /// 当前小节判定:光标在正文里高亮所属标题,首个标题之前/无光标不高亮。
    #[test]
    fn active_index_follows_cursor_section() {
        let outline = vec![item(1, "甲", 0), item(2, "乙", 10), item(3, "丙", 20)];
        assert_eq!(active_index(&outline, None), None);
        assert_eq!(active_index(&outline, Some(0)), Some(0), "恰在标题起点");
        assert_eq!(active_index(&outline, Some(5)), Some(0), "甲的正文");
        assert_eq!(active_index(&outline, Some(10)), Some(1));
        assert_eq!(active_index(&outline, Some(29)), Some(2));
    }

    /// 点击大纲条目:发出的消息携带该标题的源码区间;仅渲染不产生消息。
    #[test]
    fn clicking_item_sends_span_message() {
        let ctx = egui::Context::default();
        let items = [item(1, "标题一", 0), item(2, "标题二", 10)];
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧只渲染,借 Cell 拿到条目的屏幕位置
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(outline_row(ui, &items[1], false, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");

        // 第二帧在条目中心按下并抬起 → clicked
        let center = rect.get().center();
        let click = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(center), click(true), click(false)],
                ..Default::default()
            },
            |ui| {
                outline_row(ui, &items[1], false, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::OutlineItemClicked(10..19)]);
    }
}
