//! 文件工具栏(编辑面板顶部):图标按钮分组 + 文档状态 + 设置入口。
//!
//! 分组(docs/ui-polish.md §4):**文件组**(新建/打开/保存/另存为/导出)|
//! **视图组**(侧边栏/主题)| **AI**(下拉:续写/摘要/commit message)|
//! 右侧文档名 + 齿轮。
//!
//! 两个刻意的选择:
//! * **AI 收成下拉**:三个 AI 命令的 label 都很长,平铺在 500px 默认宽的
//!   编辑面板上必然换行;下拉与菜单栏同形态,视觉上也把 AI 归为一类能力。
//! * **主题列表不再塞在这里**:它已升级成设置对话框的「外观」页
//!   (`crate::settings`),工具栏只留齿轮入口;键位同理,改键在「快捷键」页。
//!
//! 点击只发消息,执行在 `App::logic` 归约。

use crate::command::Command;
use crate::keymap::Keymap;
use crate::settings::SettingsTab;
use crate::state::{DocumentState, Message};
use crate::ui::icons::{self, Icon};
use eframe::egui;

/// 视图组:侧边栏与主题切换(高频、与文件操作区分开)。
const VIEW_GROUP: [Command; 2] = [Command::ToggleSidebar, Command::ToggleTheme];
/// AI 下拉里的三个命令(与菜单栏「AI」一致)。
const AI_GROUP: [Command; 3] = [
    Command::AiMockStream,
    Command::AiSummary,
    Command::AiCommitMessage,
];

/// 绘制文件工具栏。键位文本取自 `keymap`(用户可改),不是出厂默认。
pub fn ui(
    panel: &mut egui::Ui,
    document: &DocumentState,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
) {
    panel.horizontal(|ui| {
        // 左:三组命令。wrapped:面板拖窄时换行而不是溢出裁切
        ui.horizontal_wrapped(|ui| {
            for cmd in Command::FILE.iter().copied().chain([Command::ExportHtml]) {
                button(ui, cmd, keymap, outbox);
            }
            ui.separator();
            for cmd in VIEW_GROUP {
                button(ui, cmd, keymap, outbox);
            }
            ui.separator();
            // AI 下拉:三个长 label 平铺挤爆窄面板,收成一类能力
            let ai = ui.menu_button("AI", |ui| {
                for cmd in AI_GROUP {
                    if ui.button(cmd.label()).clicked() {
                        outbox.push(cmd.message());
                    }
                }
            });
            ai.response
                .on_hover_text("AI 续写 / 摘要 / commit message(菜单「AI」同款入口)");
        });

        // 右:文档名 + 设置齿轮(right_to_left 吃满剩余宽度)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::icon_button(ui, Icon::Settings, "设置").clicked() {
                outbox.push(Message::SettingsOpened(SettingsTab::Appearance));
            }
            ui.weak(document.display_name());
        });
    });
    if let Some(notice) = document.notice.as_deref() {
        panel.horizontal_wrapped(|ui| {
            ui.colored_label(ui.visuals().error_fg_color, notice);
            if ui.small_button("知道了").clicked() {
                outbox.push(Message::NoticeDismissed);
            }
        });
    }
}

/// 单个图标按钮:图标 + 名字 + 当前键位,点击发命令消息。返回按钮响应
/// (测试定位用,与 `ui::menubar::item` 同款手法)。
fn button(
    ui: &mut egui::Ui,
    cmd: Command,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let shortcut = keymap.get(cmd).map(|shortcut| shortcut.platform_text());
    let response = icons::icon_text_button(ui, cmd.icon(), cmd.label(), shortcut.as_deref());
    if response.clicked() {
        outbox.push(cmd.message());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 工具栏按钮点击发出对应命令的消息;仅渲染不产消息。键位文本来自
    /// keymap(改绑后按钮上显示的也随之变)。
    #[test]
    fn toolbar_button_sends_command_message() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        let keymap = Keymap::builtin();
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(button(ui, Command::Save, &keymap, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产消息");

        let center = rect.get().center();
        let click = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![Event::PointerMoved(center)],
            vec![click(true)],
            vec![click(false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    button(ui, Command::Save, &keymap, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Command::Save.message()]);
    }

    /// 整条工具栏(含三组按钮、AI 下拉、齿轮)渲染不 panic,且仅渲染不产
    /// 消息。
    #[test]
    fn full_toolbar_renders_without_messages() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let document = DocumentState {
            path: None,
            dirty: false,
            notice: None,
        };
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui(ui, &document, &Keymap::builtin(), &mut outbox);
        });
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty());
    }
}
