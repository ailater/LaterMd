//! HTML 导出命令(docs/roadmap.md P0「导出」)。
//!
//! 与 `file` 的文件命令同构:UI 只产出 [`Message::ExportHtml`](crate::state::Message::ExportHtml),
//! 弹框 + IO 在归约里发生(同步 rfd 对话框阻塞事件循环的取舍见 `file`
//! 模块文档)。导出物是编辑缓冲的派生物:不认领文档路径、不清 dirty。

use std::path::{Path, PathBuf};

use eframe::egui::{self, Modifiers};

/// 导出对话框过滤器接受的扩展名(同步给 rfd,不带点)。
pub const HTML_EXTENSIONS: [&str; 2] = ["html", "htm"];

/// 未落盘文档在导出对话框里的预填文件名。
pub const UNTITLED_EXPORT_NAME: &str = "未命名.html";

/// 导出快捷键:Ctrl/Cmd+E。
pub fn shortcut() -> egui::KeyboardShortcut {
    egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::E)
}

/// 从本帧输入消费导出快捷键;已消费的按键从输入流移除,不会传给控件。
pub fn poll_shortcut(ctx: &egui::Context) -> bool {
    ctx.input_mut(|input| input.consume_shortcut(&shortcut()))
}

/// 导出文件名预填:当前文档名换 `.html` 扩展;未落盘用 [`UNTITLED_EXPORT_NAME`]。
pub fn default_name(document: Option<&Path>) -> String {
    document
        .and_then(Path::file_stem)
        .map(|stem| format!("{}.html", stem.to_string_lossy()))
        .unwrap_or_else(|| UNTITLED_EXPORT_NAME.to_owned())
}

/// 导出保存对话框,预填 `default_name`;取消返回 `None`。
pub fn save_dialog(start_dir: &Path, default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter("HTML", &HTML_EXTENSIONS)
        .set_directory(start_dir)
        .set_file_name(default_name)
        .save_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, RawInput};

    fn key_e(modifiers: Modifiers) -> Event {
        Event::Key {
            key: Key::E,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Ctrl/Cmd+E 消费一次后不回流;无修饰的 E 不触发。
    #[test]
    fn command_e_fires_exactly_once() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_e(Modifiers::COMMAND), key_e(Modifiers::NONE)],
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx().clone();
                assert!(poll_shortcut(&ctx));
                assert!(!poll_shortcut(&ctx), "同一帧重复消费");
                assert!(!poll_shortcut(&ctx), "裸 E 不是快捷键");
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn default_name_follows_document_stem() {
        assert_eq!(
            default_name(Some(Path::new("/docs/指南 v2.md"))),
            "指南 v2.html"
        );
        assert_eq!(default_name(None), UNTITLED_EXPORT_NAME);
    }
}
