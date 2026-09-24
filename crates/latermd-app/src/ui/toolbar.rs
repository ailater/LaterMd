//! 文件工具栏(编辑面板顶部):命令按钮(label/快捷键统一取自
//! `crate::command`)+ 设置菜单(主题定向选择)+ 当前文件名/未保存标记 +
//! 上次文件操作的失败提示。点击只发消息,执行在 `App::logic`。

use crate::command::Command;
use crate::state::{DocumentState, Message};
use crate::theme::ThemeMode;
use eframe::egui;

/// 绘制文件工具栏。
pub fn ui(
    panel: &mut egui::Ui,
    document: &DocumentState,
    theme_mode: ThemeMode,
    outbox: &mut Vec<Message>,
) {
    // wrapped:面板被拖窄时按钮换行而不是溢出裁切
    panel.horizontal_wrapped(|ui| {
        for cmd in Command::FILE.iter().copied().chain([Command::ExportHtml]) {
            let button = egui::Button::new(cmd.label())
                .shortcut_text(ui.ctx().format_shortcut(&cmd.shortcut()));
            if ui.add(button).clicked() {
                outbox.push(cmd.message());
            }
        }
        // 设置菜单:egui 菜单内点击任意控件自动收起,无需手工关闭
        ui.menu_button("设置", |ui| {
            for mode in ThemeMode::ALL {
                if ui
                    .selectable_label(theme_mode == mode, mode.label())
                    .clicked()
                {
                    outbox.push(Message::ThemeChanged(mode));
                }
            }
        });
        ui.separator();
        ui.weak(document.display_name());
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
