//! 编辑器顶部的标签条(#11「multi-tabs」,docs/auto-plan.md 规格)。
//!
//! 形态:一排 chip(文件名 + dirty 星 + 关闭 ×)。**单个未命名且干净的空
//! 标签不画条** —— 此时标签条零信息量,省一行高度;一旦有第二个标签或
//! 当前文档落盘,条即出现,关闭入口(× 与 Ctrl+W)随之可用。
//!
//! 交互:点击名字区 = 激活;点击 × = 请求关闭(脏标签由归约侧弹确认模态,
//! 见 `State::request_close_tab`)。chip 自绘(与工具栏图标按钮同一套
//! 手法),选中态用填充底色 —— 与侧边栏页签的下划线区分层级。

use crate::state::Message;
use crate::tabs::TabsState;
use crate::ui::tokens::{RADIUS_SM, SPACE_SM, SPACE_XS};
use eframe::egui::{self, Align2, Sense};

/// chip 高度(比工具栏矮一档:标签条更密集)。
const CHIP_H: f32 = 24.0;
/// 关闭 × 的方框边长。
const CLOSE: f32 = 12.0;

/// 绘制标签条;返回是否实际绘制(单个未命名空标签不画,测试据此断言)。
pub fn ui(panel: &mut egui::Ui, tabs: &TabsState, outbox: &mut Vec<Message>) -> bool {
    let hide = tabs.tabs.len() == 1
        && tabs.tabs[0].document.path.is_none()
        && !tabs.current().editor.is_dirty();
    if hide {
        return false;
    }
    // 标签放不下时水平滚动而非换行(同 vendored 表格的横向滚动手法):
    // 换行会让标签条高度随标签数成倍增长,把编辑区顶得上下跳;单行 +
    // 滚动(垂直滚轮在仅水平可滚的 ScrollArea 里自动转为水平)高度恒定。
    egui::ScrollArea::horizontal()
        .id_salt("tabs-bar")
        .auto_shrink([false, true])
        .show(panel, |ui| {
            ui.horizontal(|ui| {
                for index in 0..tabs.tabs.len() {
                    chip(ui, tabs, index, outbox);
                }
            });
        });
    true
}

/// 单个标签 chip:名字区点击激活,× 区点击请求关闭。
fn chip(ui: &mut egui::Ui, tabs: &TabsState, index: usize, outbox: &mut Vec<Message>) {
    let tab = &tabs.tabs[index];
    let selected = index == tabs.active;
    let name = tab.document.display_name();
    let text_color = if selected {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let name_w = ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(name.clone(), font.clone(), text_color))
        .rect
        .width();
    let size = egui::vec2(SPACE_SM + name_w + SPACE_XS + CLOSE + SPACE_SM, CHIP_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - SPACE_SM - CLOSE,
            rect.center().y - CLOSE / 2.0,
        ),
        egui::vec2(CLOSE, CLOSE),
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // WorkBuddy 风活动页签:浅蓝底 + 蓝字 + 底部 2px 蓝条;未选中悬停浅灰
        let accent = crate::ui::tokens::accent(ui);
        let selected_bg = crate::theme::shell_tokens(ui.visuals().dark_mode).selected_bg;
        let hover_bg = crate::theme::shell_tokens(ui.visuals().dark_mode).hover;
        let bg = if selected {
            selected_bg
        } else if response.hovered() {
            hover_bg
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect_filled(rect, RADIUS_SM, bg);
        let text_color = if selected { accent } else { text_color };
        painter.text(
            egui::pos2(rect.left() + SPACE_SM, rect.center().y),
            Align2::LEFT_CENTER,
            &name,
            font,
            text_color,
        );
        if selected {
            // 底部 2px 强调条:WorkBuddy 标签的视觉锚点
            let bar = egui::Rect::from_min_max(
                egui::pos2(rect.left() + SPACE_SM, rect.bottom() - 2.0),
                egui::pos2(rect.right() - SPACE_SM, rect.bottom()),
            );
            painter.rect_filled(bar, 1.0, accent);
        }
        // 关闭 ×:悬停该 chip 时才上色(常驻会显得噪)
        let cross = if response.hovered() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let stroke = egui::Stroke::new(1.2, cross);
        painter.line_segment(
            [
                egui::pos2(close_rect.left(), close_rect.top()),
                egui::pos2(close_rect.right(), close_rect.bottom()),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(close_rect.right(), close_rect.top()),
                egui::pos2(close_rect.left(), close_rect.bottom()),
            ],
            stroke,
        );
    }

    if response.clicked() {
        // 命中判定用本帧指针位置:落在 × 区 = 关闭,落在名字区 = 激活
        let close_hit = response
            .interact_pointer_pos()
            .is_some_and(|pos| close_rect.expand(2.0).contains(pos));
        if close_hit {
            outbox.push(Message::TabCloseRequested(index));
        } else {
            outbox.push(Message::TabActivate(index));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::TabsState;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 单个未命名空标签不画条(零信息量);落盘或变脏后条出现。
    #[test]
    fn bar_hidden_for_single_empty_tab_only() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("");
        let mut outbox = Vec::new();

        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(!super::ui(ui, &tabs, &mut outbox), "单个未命名空标签不画条");
        });
        output.drop_without_applying_deltas();

        tabs.current_mut().editor.insert_chars(0, "写了字");
        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(super::ui(ui, &tabs, &mut outbox), "dirty 后条出现");
        });
        output.drop_without_applying_deltas();

        // 落盘后即使不脏也显示(有关闭入口的信息量)
        tabs.current_mut().editor.clear_dirty();
        tabs.current_mut().document.path = Some(std::path::PathBuf::from("/a.md"));
        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(super::ui(ui, &tabs, &mut outbox));
        });
        output.drop_without_applying_deltas();
    }

    /// 标签多到放不下时保持单行水平滚动,不换行:同一组标签在窄/宽容器里
    /// 条高一致(换行实现的高度随标签数成倍增长,编辑区会被顶得上下跳)。
    #[test]
    fn bar_keeps_single_line_when_chips_overflow() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("");
        for i in 0..6 {
            tabs.open_tab(
                Some(std::path::PathBuf::from(format!(
                    "一个很长很长的文档标题第{i}篇.md"
                ))),
                "",
            );
        }
        let mut outbox = Vec::new();
        let heights = [220.0, 2000.0].map(|width| {
            let mut height = None;
            let output = ctx.run_ui(RawInput::default(), |ui| {
                ui.set_max_width(width);
                assert!(super::ui(ui, &tabs, &mut outbox), "多标签必画条");
                height = Some(ui.min_rect().height());
            });
            output.drop_without_applying_deltas();
            height.unwrap()
        });
        assert_eq!(heights[0], heights[1], "窄容器不换行,条高恒定");
        // 单行高度与 chip 高度同量级(留行距与滚动条余量),远小于 6 行
        assert!(heights[1] < 2.0 * CHIP_H, "条高 {}", heights[1]);
    }

    /// chip 交互:点名字区发 TabActivate,点 × 区发 TabCloseRequested。
    #[test]
    fn chip_click_zones_send_different_messages() {
        let ctx = egui::Context::default();
        let tabs = TabsState::new("第一篇");
        let mut tabs = tabs;
        tabs.open_tab(None, "第二篇");
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧拿 chip 0 的位置(画在原始位置)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            chip(ui, &tabs, 0, &mut outbox);
            rect.set(ui.min_rect());
        });
        output.drop_without_applying_deltas();
        let rect = rect.get();
        let name_pos = egui::pos2(rect.left() + 3.0, rect.center().y);
        let close_pos = egui::pos2(rect.right() - SPACE_SM - CLOSE / 2.0, rect.center().y);
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        // 点名字区 → TabActivate(0)
        for events in [
            vec![Event::PointerMoved(name_pos)],
            vec![click(name_pos, true)],
            vec![click(name_pos, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    chip(ui, &tabs, 0, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::TabActivate(0)]);
        outbox.clear();

        // 点 × 区 → TabCloseRequested(0)
        for events in [
            vec![Event::PointerMoved(close_pos)],
            vec![click(close_pos, true)],
            vec![click(close_pos, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    chip(ui, &tabs, 0, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::TabCloseRequested(0)]);
    }
}
