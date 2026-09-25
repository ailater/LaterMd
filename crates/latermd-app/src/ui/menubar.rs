//! 顶部菜单栏:全部命令的可发现性入口(docs/roadmap.md P0「快捷键」)。
//!
//! 列出 [`crate::command::Command`] 的全部条目并附平台化快捷键文本;点击
//! 只产出 [`Message`],执行在 `App::logic` 归约 —— 与快捷键入口
//! (`crate::command::poll_shortcuts`)殊途同归。外层 top panel 在 `ui::layout`。

use crate::command::Command;
use crate::keymap::Keymap;
use crate::settings::SettingsTab;
use crate::state::Message;

use eframe::egui;

/// 绘制菜单栏内容(挂在 top panel 内)。键位文本取自 `keymap`(用户可改),
/// 与工具栏按钮同一口径 —— 改键后两处同时变。
pub fn ui(bar: &mut egui::Ui, keymap: &Keymap, outbox: &mut Vec<Message>) {
    egui::MenuBar::new().ui(bar, |ui| {
        ui.menu_button("文件", |ui| {
            for cmd in Command::FILE {
                item(ui, cmd, keymap, outbox);
            }
        });
        ui.menu_button("导出", |ui| {
            item(ui, Command::ExportHtml, keymap, outbox);
        });
        ui.menu_button("视图", |ui| {
            item(ui, Command::ToggleSidebar, keymap, outbox);
            item(ui, Command::ToggleTheme, keymap, outbox);
        });
        ui.menu_button("AI", |ui| {
            item(ui, Command::AiMockStream, keymap, outbox);
            item(ui, Command::AiCommitMessage, keymap, outbox);
            item(ui, Command::AiSummary, keymap, outbox);
        });
        // 设置:菜单保留两个直达页(外观 / 快捷键),完整四页由工具栏齿轮开
        ui.menu_button("设置", |ui| {
            for tab in [SettingsTab::Appearance, SettingsTab::Keymap] {
                if ui.button(tab.label()).clicked() {
                    outbox.push(Message::SettingsOpened(tab));
                }
            }
        });
    });
}

/// 单个菜单项:显示名 + 当前键位(未绑快捷键的命令只显示名字);点击发
/// 消息(egui 菜单内点击任意控件自动收起)。返回按钮响应,独立成函数便于
/// 点击测试定位。
pub fn item(
    ui: &mut egui::Ui,
    cmd: Command,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let mut button = egui::Button::new(cmd.label());
    if let Some(shortcut) = keymap.get(cmd) {
        button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut.keyboard()));
    }
    let response = ui.add(button);
    if response.clicked() {
        outbox.push(cmd.message());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Message;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 点击菜单项发出的消息就是该命令的归约入口;仅渲染不产生消息。
    #[test]
    fn clicking_item_sends_command_message() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let keymap = Keymap::builtin();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧只渲染,借 Cell 拿到条目的屏幕位置
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(item(ui, Command::ToggleSidebar, &keymap, &mut outbox).rect);
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
                item(ui, Command::ToggleSidebar, &keymap, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::SidebarToggled]);
    }

    /// AI 菜单项(唯一无快捷键的命令)正常渲染并可点击发起:item() 的
    /// shortcut 分支在 None 时不得触碰 format_shortcut。
    #[test]
    fn ai_item_renders_without_shortcut_and_sends_start() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let keymap = Keymap::builtin();
        let rect = Cell::new(Rect::NOTHING);

        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(item(ui, Command::AiMockStream, &keymap, &mut outbox).rect);
        })
        .drop_without_applying_deltas();

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
                item(ui, Command::AiMockStream, &keymap, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::AiStart]);
    }
}
