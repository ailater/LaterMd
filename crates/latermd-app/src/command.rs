//! 统一命令层(docs/roadmap.md P0「快捷键」)。
//!
//! [`Command`] 是全部用户命令的唯一清单:label、快捷键与 [`Message`] 映射
//! 只在这里定义一份,顶部菜单栏(`ui::menubar`)、编辑面板工具栏按钮与
//! `App::logic` 的快捷键消费三处入口共用。命令的执行(弹框 + IO + 状态
//! 变更)全部发生在 [`crate::state::State::apply`] 的归约里;本模块与 UI
//! 一样只产出消息。
//!
//! 平台自适应:`Modifiers::COMMAND` 在 Windows/Linux 是 Ctrl、macOS 是 Cmd
//! (egui 内建),菜单里经 `Context::format_shortcut` 按平台显示。

use crate::file::FileCmd;
use crate::state::Message;
use eframe::egui::{self, Modifiers};

/// 用户命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// 新建空文档。
    New,
    /// 打开已有文件。
    Open,
    /// 保存;从未落盘时等价于另存为。
    Save,
    /// 另存为(总是弹框)。
    SaveAs,
    /// 导出当前文档为 HTML(派生物,不触碰文档落盘身份)。
    ExportHtml,
    /// 明暗主题互换;定向选择仍走工具栏「设置」菜单。
    ToggleTheme,
    /// 侧边栏展开/折叠。
    ToggleSidebar,
}

impl Command {
    /// 文件组:菜单「文件」子菜单与工具栏按钮共用的顺序。
    pub const FILE: [Command; 4] = [Self::New, Self::Open, Self::Save, Self::SaveAs];

    /// 菜单与按钮的显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "新建",
            Self::Open => "打开",
            Self::Save => "保存",
            Self::SaveAs => "另存为",
            Self::ExportHtml => "导出 HTML",
            Self::ToggleTheme => "切换主题",
            Self::ToggleSidebar => "切换侧边栏",
        }
    }

    /// 绑定的快捷键;全部命令都有绑定,菜单栏负责展示以保证可发现性。
    ///
    /// ToggleSidebar 取 Ctrl/Cmd+\\ 而非更常见的 Ctrl+B:Markdown 工作台的
    /// Ctrl+B 要留给将来的加粗(与主流 Markdown 编辑器一致)。
    pub fn shortcut(self) -> egui::KeyboardShortcut {
        match self {
            Self::New => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::N),
            Self::Open => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::O),
            Self::Save => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::S),
            Self::SaveAs => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::S)
            }
            Self::ExportHtml => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::E),
            Self::ToggleTheme => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::T)
            }
            Self::ToggleSidebar => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Backslash)
            }
        }
    }

    /// 归约入口:命令翻成状态消息,执行在 `State::apply`。
    pub fn message(self) -> Message {
        match self {
            Self::New => Message::FileCommand(FileCmd::New),
            Self::Open => Message::FileCommand(FileCmd::Open),
            Self::Save => Message::FileCommand(FileCmd::Save),
            Self::SaveAs => Message::FileCommand(FileCmd::SaveAs),
            Self::ExportHtml => Message::ExportHtml,
            Self::ToggleTheme => Message::ToggleTheme,
            Self::ToggleSidebar => Message::SidebarToggled,
        }
    }
}

/// 快捷键消费顺序:SaveAs 必须先于 Save —— `consume_shortcut` 底层的
/// `matches_logically` 忽略多余 Shift,先问 Save 的话 Ctrl/Cmd+Shift+S
/// 会被它抢先吃掉(egui 文档要求 most specific first)。其余命令键位
/// 互不相撞,顺序无关。
const POLL_ORDER: [Command; 7] = [
    Command::SaveAs,
    Command::Save,
    Command::New,
    Command::Open,
    Command::ExportHtml,
    Command::ToggleTheme,
    Command::ToggleSidebar,
];

/// 从本帧输入消费全部命令快捷键,返回被触发的命令。
///
/// 只在 `App::logic` 调用:logic 先于 `App::ui` 运行,本帧按键事件此刻
/// 可见;消费即从输入流移除,TextEdit 即使聚焦也收不到。普通字符输入
/// (无 COMMAND 修饰)不匹配任何绑定,原样放行给控件。
pub fn poll_shortcuts(ctx: &egui::Context) -> Vec<Command> {
    POLL_ORDER
        .iter()
        .filter_map(|cmd| {
            let shortcut = cmd.shortcut();
            ctx.input_mut(|input| input.consume_shortcut(&shortcut))
                .then_some(*cmd)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, RawInput};

    fn key_event(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Ctrl/Cmd+Shift+S 只触发 SaveAs 一条;`matches_logically` 忽略多余
    /// Shift,若先消费 Save 会误中(顺序约束见 [`POLL_ORDER`] 文档)。
    #[test]
    fn shift_save_fires_only_save_as() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::S, Modifiers::COMMAND | Modifiers::SHIFT)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(poll_shortcuts(ui.ctx()), vec![Command::SaveAs]);
            },
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn plain_save_fires_only_save() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::S, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(poll_shortcuts(ui.ctx()), vec![Command::Save]);
            },
        );
        output.drop_without_applying_deltas();
    }

    /// 全部命令的绑定都能被各自按键触发,且消费一次后同帧不回流。
    #[test]
    fn every_command_fires_exactly_once() {
        let ctx = egui::Context::default();
        let bindings = [
            (Command::New, Key::N, Modifiers::COMMAND),
            (Command::Open, Key::O, Modifiers::COMMAND),
            (Command::ExportHtml, Key::E, Modifiers::COMMAND),
            (
                Command::ToggleTheme,
                Key::T,
                Modifiers::COMMAND | Modifiers::SHIFT,
            ),
            (Command::ToggleSidebar, Key::Backslash, Modifiers::COMMAND),
        ];
        for (cmd, key, modifiers) in bindings {
            let output = ctx.run_ui(
                RawInput {
                    events: vec![key_event(key, modifiers)],
                    ..Default::default()
                },
                |ui| {
                    let ctx = ui.ctx().clone();
                    assert_eq!(poll_shortcuts(&ctx), vec![cmd]);
                    assert!(poll_shortcuts(&ctx).is_empty(), "同一帧重复消费");
                },
            );
            output.drop_without_applying_deltas();
        }
    }

    /// 无修饰的普通字符不是任何命令的快捷键(不劫持 TextEdit 输入的
    /// 第一道保证;第二道是 logic 阶段消费、ui 阶段控件见不到,见
    /// `ui::layout` 的集成测试)。
    #[test]
    fn bare_keys_do_not_fire() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![
                    key_event(Key::S, Modifiers::NONE),
                    key_event(Key::N, Modifiers::NONE),
                    key_event(Key::E, Modifiers::NONE),
                    key_event(Key::T, Modifiers::NONE),
                ],
                ..Default::default()
            },
            |ui| {
                assert!(poll_shortcuts(ui.ctx()).is_empty());
            },
        );
        output.drop_without_applying_deltas();
    }

    /// 命令到消息的映射:文件组与导出走既有消息,两个开关是新消息。
    #[test]
    fn message_mapping_covers_all_commands() {
        assert_eq!(Command::New.message(), Message::FileCommand(FileCmd::New));
        assert_eq!(Command::Open.message(), Message::FileCommand(FileCmd::Open));
        assert_eq!(Command::Save.message(), Message::FileCommand(FileCmd::Save));
        assert_eq!(
            Command::SaveAs.message(),
            Message::FileCommand(FileCmd::SaveAs)
        );
        assert_eq!(Command::ExportHtml.message(), Message::ExportHtml);
        assert_eq!(Command::ToggleTheme.message(), Message::ToggleTheme);
        assert_eq!(Command::ToggleSidebar.message(), Message::SidebarToggled);
    }
}
