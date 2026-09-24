//! 文件工具栏(编辑面板顶部):四个命令按钮 + 当前文件名/未保存标记 +
//! 上次文件操作的失败提示。点击只发消息,执行在 `App::logic`。

use crate::file::FileCmd;
use crate::state::{DocumentState, Message};
use eframe::egui;

/// 绘制文件工具栏。
pub fn ui(panel: &mut egui::Ui, document: &DocumentState, outbox: &mut Vec<Message>) {
    // wrapped:面板被拖窄时按钮换行而不是溢出裁切
    panel.horizontal_wrapped(|ui| {
        for cmd in FileCmd::ALL {
            let mut button = egui::Button::new(cmd.label());
            if let Some(shortcut) = cmd.shortcut() {
                button = button.shortcut_text(ui.ctx().format_shortcut(&shortcut));
            }
            if ui.add(button).clicked() {
                outbox.push(Message::FileCommand(cmd));
            }
        }
        let export_button = egui::Button::new("导出 HTML")
            .shortcut_text(ui.ctx().format_shortcut(&crate::export::shortcut()));
        if ui.add(export_button).clicked() {
            outbox.push(Message::ExportHtml);
        }
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
