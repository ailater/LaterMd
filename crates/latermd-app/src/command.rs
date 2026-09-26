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

use crate::compose::FormatAction;
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
    /// AI:Mock 流式续写(P1 联调入口,decisions-pending #3)。流式进行中
    /// 再次触发在归约侧被忽略;不绑快捷键,避免与现有键位冲突。
    AiMockStream,
    /// AI:生成 commit message(P1):读仓库未提交改动(staged 优先),
    /// 合成 conventional 中文 subject 并弹建议对话框。流式进行中触发
    /// 同样被忽略(与 AiMockStream 共用防重入)。
    AiCommitMessage,
    /// AI:生成摘要(P1):文档全文喂 provider,3-5 条要点以引用块形式
    /// 流式追加到文档末尾;文档已含「AI 摘要」节则先移除再插入。流式
    /// 进行中触发同样被忽略(共用防重入)。
    AiSummary,
    /// 切到下一个标签(P1.5「多标签」,Ctrl/Cmd+Tab 循环)。
    TabNext,
    /// 关闭当前标签(脏则确认模态;Ctrl/Cmd+W)。
    TabClose,
    /// 源码模式 ↔ Live Preview 互换(P3):共用一个 rope buffer,切模式不丢
    /// 光标也不丢 undo 栈。默认键 Cmd/Ctrl+/(与主流编辑器的「切换注释」
    /// 同键位,工作台里没有注释语义)。
    ToggleLivePreview,
    // —— 格式(docs/ui-shell-redesign.md §6.3,左侧即工具条顺序)——
    /// 加粗。Ctrl/Cmd+B — 这里兑现了 command.rs 原先「Ctrl+B 留给将来的
    /// 加粗」的注释,ToggleSidebar 因此早在当初就避开了 Ctrl+B。
    FormatBold,
    /// 斜体。
    FormatItalic,
    /// 删除线。
    FormatStrike,
    /// 行内代码。反引号;`Cmd+E` 已被 ExportHtml 占,取 VS Code 同款。
    FormatInlineCode,
    /// 链接。
    FormatLink,
    /// 一级标题。
    FormatH1,
    /// 二级标题。
    FormatH2,
    /// 三级标题。
    FormatH3,
    /// 引用。
    FormatQuote,
    /// 围栏代码块(info string 空)。
    FormatCodeBlock,
    /// 分割线。
    FormatDivider,
    /// 2×2 表格骨架。
    FormatTable,
    /// 无序列表。Shift+8 是 VS Code 同款。
    FormatBullet,
    /// 有序列表。
    FormatOrdered,
    /// 任务列表(三态循环)。
    FormatTask,
    /// 右侧只读预览栏展开/折叠(§3.1)。
    ToggleRightPreview,
    /// 禅定模式(§7)。F11:`KeyboardShortcut` 允许无修饰的 F1-F12。
    ToggleZen,
}

impl Command {
    /// 文件组:菜单「文件」子菜单与工具栏按钮共用的顺序。
    pub const FILE: [Command; 4] = [Self::New, Self::Open, Self::Save, Self::SaveAs];

    /// 全部命令(快捷键设置页与绑定表遍历的顺序,见 `crate::keymap`)。
    ///
    /// 顺序 = UI 上的自然归属:文件 → 视图 → AI → 标签 → 格式按工具条分组
    /// 从左到右。
    pub const ALL: [Command; 30] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::ExportHtml,
        Self::ToggleTheme,
        Self::ToggleSidebar,
        Self::AiMockStream,
        Self::AiCommitMessage,
        Self::AiSummary,
        Self::TabNext,
        Self::TabClose,
        Self::ToggleLivePreview,
        Self::FormatBold,
        Self::FormatItalic,
        Self::FormatStrike,
        Self::FormatInlineCode,
        Self::FormatLink,
        Self::FormatH1,
        Self::FormatH2,
        Self::FormatH3,
        Self::FormatQuote,
        Self::FormatCodeBlock,
        Self::FormatDivider,
        Self::FormatTable,
        Self::FormatBullet,
        Self::FormatOrdered,
        Self::FormatTask,
        Self::ToggleRightPreview,
        Self::ToggleZen,
    ];

    /// 格式命令 → 对应的动作,非格式命令为 `None`。
    pub fn format_action(self) -> Option<FormatAction> {
        Some(match self {
            Self::FormatBold => FormatAction::Bold,
            Self::FormatItalic => FormatAction::Italic,
            Self::FormatStrike => FormatAction::Strike,
            Self::FormatInlineCode => FormatAction::InlineCode,
            Self::FormatLink => FormatAction::Link,
            Self::FormatH1 => FormatAction::H1,
            Self::FormatH2 => FormatAction::H2,
            Self::FormatH3 => FormatAction::H3,
            Self::FormatQuote => FormatAction::Quote,
            Self::FormatCodeBlock => FormatAction::CodeBlock,
            Self::FormatDivider => FormatAction::Divider,
            Self::FormatTable => FormatAction::Table,
            Self::FormatBullet => FormatAction::Bullet,
            Self::FormatOrdered => FormatAction::Ordered,
            Self::FormatTask => FormatAction::Task,
            _ => return None,
        })
    }

    /// 稳定 id:快捷键表 `keymap.json` 的键。命令的显示名会随文案调整,
    /// id 不随,存档才不会因改 label 而失效。
    pub fn id(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::ExportHtml => "export_html",
            Self::ToggleTheme => "toggle_theme",
            Self::ToggleSidebar => "toggle_sidebar",
            Self::AiMockStream => "ai_mock_stream",
            Self::AiCommitMessage => "ai_commit_message",
            Self::AiSummary => "ai_summary",
            Self::TabNext => "tab_next",
            Self::TabClose => "tab_close",
            Self::ToggleLivePreview => "toggle_live_preview",
            Self::FormatBold => "format_bold",
            Self::FormatItalic => "format_italic",
            Self::FormatStrike => "format_strike",
            Self::FormatInlineCode => "format_inline_code",
            Self::FormatLink => "format_link",
            Self::FormatH1 => "format_h1",
            Self::FormatH2 => "format_h2",
            Self::FormatH3 => "format_h3",
            Self::FormatQuote => "format_quote",
            Self::FormatCodeBlock => "format_code_block",
            Self::FormatDivider => "format_divider",
            Self::FormatTable => "format_table",
            Self::FormatBullet => "format_bullet",
            Self::FormatOrdered => "format_ordered",
            Self::FormatTask => "format_task",
            Self::ToggleRightPreview => "toggle_right_preview",
            Self::ToggleZen => "toggle_zen",
        }
    }

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
            Self::AiMockStream => "AI: Mock 流式续写",
            Self::AiCommitMessage => "AI: 生成 commit message",
            Self::AiSummary => "AI: 生成摘要",
            Self::TabNext => "下一个标签",
            Self::TabClose => "关闭标签",
            Self::ToggleLivePreview => "切换 Live Preview",
            Self::FormatBold => FormatAction::Bold.label(),
            Self::FormatItalic => FormatAction::Italic.label(),
            Self::FormatStrike => FormatAction::Strike.label(),
            Self::FormatInlineCode => FormatAction::InlineCode.label(),
            Self::FormatLink => FormatAction::Link.label(),
            Self::FormatH1 => FormatAction::H1.label(),
            Self::FormatH2 => FormatAction::H2.label(),
            Self::FormatH3 => FormatAction::H3.label(),
            Self::FormatQuote => FormatAction::Quote.label(),
            Self::FormatCodeBlock => FormatAction::CodeBlock.label(),
            Self::FormatDivider => FormatAction::Divider.label(),
            Self::FormatTable => FormatAction::Table.label(),
            Self::FormatBullet => FormatAction::Bullet.label(),
            Self::FormatOrdered => FormatAction::Ordered.label(),
            Self::FormatTask => FormatAction::Task.label(),
            Self::ToggleRightPreview => "切换预览栏",
            Self::ToggleZen => "禅定模式",
        }
    }

    /// **出厂默认**的快捷键;`None` = 不绑定(菜单里只显示名字)。
    ///
    /// 实际生效的键位在 [`crate::keymap::Keymap`](用户可改,`keymap.json`);
    /// 这里只是默认值来源。全部文件/视图命令都有默认绑定,菜单栏负责展示
    /// 以保证可发现性;AI 命令一律 `None`:联调入口不抢键位,等 provider
    /// 选型定案再定。
    ///
    /// ToggleSidebar 取 Ctrl/Cmd+\\ 而非更常见的 Ctrl+B:Markdown 工作台的
    /// Ctrl+B 要留给将来的加粗(与主流 Markdown 编辑器一致)。
    pub fn default_shortcut(self) -> Option<egui::KeyboardShortcut> {
        let shortcut = match self {
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
            Self::TabNext => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Tab),
            Self::TabClose => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::W),
            Self::ToggleLivePreview => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Slash)
            }
            Self::FormatBold => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::B),
            Self::FormatItalic => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::I),
            Self::FormatStrike => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::X)
            }
            Self::FormatInlineCode => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Backtick)
            }
            // 链接加 Shift 而非 VS Code 的裸 Ctrl+K:本项目 Ctrl+K 被快捷
            // 键设置的捕获模式测试当作「任意空闲键」,占上去会让那条回归失真。
            Self::FormatLink => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::K)
            }
            Self::FormatH1 => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Num1),
            Self::FormatH2 => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Num2),
            Self::FormatH3 => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Num3),
            Self::FormatQuote => egui::KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                egui::Key::Period,
            ),
            Self::FormatCodeBlock => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::C)
            }
            Self::FormatBullet => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Num8)
            }
            Self::FormatOrdered => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Num7)
            }
            Self::FormatTask => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::Num9)
            }
            Self::ToggleRightPreview => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::ALT, egui::Key::R)
            }
            Self::ToggleZen => egui::KeyboardShortcut::new(Modifiers::NONE, egui::Key::F11),
            // 无快捷键。前三条是 AI 联调入口:不抢键位,等 provider 选型
            // 定案再定;后两条只从工具条按钮触发(插画布性质的动作,不像
            // 加粗那样高频到需要键位)。
            Self::AiMockStream
            | Self::AiCommitMessage
            | Self::AiSummary
            | Self::FormatDivider
            | Self::FormatTable => return None,
        };
        Some(shortcut)
    }

    /// 图标(工具栏按钮用;`ui::icons` 自绘,不依赖字体)。
    pub fn icon(self) -> crate::ui::icons::Icon {
        use crate::ui::icons::Icon;
        match self {
            Self::New => Icon::New,
            Self::Open => Icon::Open,
            Self::Save => Icon::Save,
            Self::SaveAs => Icon::SaveAs,
            Self::ExportHtml => Icon::Export,
            Self::ToggleTheme => Icon::Theme,
            Self::ToggleSidebar => Icon::Sidebar,
            Self::AiMockStream | Self::AiCommitMessage | Self::AiSummary => Icon::Ai,
            Self::TabNext | Self::TabClose => Icon::Files,
            Self::ToggleLivePreview => Icon::Zen,
            Self::FormatBold
            | Self::FormatItalic
            | Self::FormatStrike
            | Self::FormatH1
            | Self::FormatH2
            | Self::FormatH3 => Icon::Table,
            Self::FormatInlineCode => Icon::CodeInline,
            Self::FormatLink => Icon::Link,
            Self::FormatQuote => Icon::Quote,
            Self::FormatCodeBlock => Icon::CodeBlock,
            Self::FormatDivider => Icon::Divider,
            Self::FormatTable => Icon::Table,
            Self::FormatBullet => Icon::BulletList,
            Self::FormatOrdered => Icon::OrderedList,
            Self::FormatTask => Icon::TaskList,
            Self::ToggleRightPreview => Icon::PanelRight,
            Self::ToggleZen => Icon::Zen,
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
            Self::AiMockStream => Message::AiStart,
            Self::AiCommitMessage => Message::AiCommitRequested,
            Self::AiSummary => Message::AiSummaryRequested,
            Self::ToggleLivePreview => Message::ToggleLivePreview,
            Self::TabNext => Message::TabNext,
            Self::TabClose => Message::TabCloseActive,
            Self::ToggleRightPreview => Message::RightPanelToggled,
            Self::ToggleZen => Message::ZenToggled,
            // 十六条格式动作一条 match 收干:动作枚举已经在 cmd 里定死了,
            // 这里只把它装进消息,语义一律看 `compose::apply`
            Self::FormatBold
            | Self::FormatItalic
            | Self::FormatStrike
            | Self::FormatInlineCode
            | Self::FormatLink
            | Self::FormatH1
            | Self::FormatH2
            | Self::FormatH3
            | Self::FormatQuote
            | Self::FormatCodeBlock
            | Self::FormatDivider
            | Self::FormatTable
            | Self::FormatBullet
            | Self::FormatOrdered
            | Self::FormatTask => Message::FormatRequested(self.format_action().unwrap()),
        }
    }
}

/// 从本帧输入消费全部命令快捷键,返回被触发的命令。
///
/// 键位取自 `keymap`(用户可改)而非出厂默认;**消费顺序按修饰键个数降序**
/// —— 通用化了原先「SaveAs 必须先于 Save」的特例:`consume_shortcut` 底层的
/// `matches_logically` 忽略多余 Shift,先问 Ctrl/Cmd+S 的话 Ctrl/Cmd+Shift+S
/// 会被它抢先吃掉(egui 文档要求 most specific first)。
///
/// 只在 `App::logic` 调用:logic 先于 `App::ui` 运行,本帧按键事件此刻
/// 可见;消费即从输入流移除,TextEdit 即使聚焦也收不到。普通字符输入
/// (无 COMMAND 修饰)不匹配任何绑定,原样放行给控件。
pub fn poll_shortcuts(ctx: &egui::Context, keymap: &crate::keymap::Keymap) -> Vec<Command> {
    let mut bound: Vec<(Command, crate::keymap::Shortcut)> = Command::ALL
        .iter()
        .filter_map(|cmd| keymap.get(*cmd).map(|shortcut| (*cmd, shortcut)))
        .collect();
    // 修饰键多的先匹配(most specific first);同位数保持 ALL 的顺序,
    // 排序用稳定排序
    bound.sort_by_key(|(_, shortcut)| std::cmp::Reverse(modifier_count(shortcut.modifiers)));
    bound
        .into_iter()
        .filter_map(|(cmd, shortcut)| {
            ctx.input_mut(|input| input.consume_shortcut(&shortcut.keyboard()))
                .then_some(cmd)
        })
        .collect()
}

/// 修饰键个数(决定消费优先级)。
fn modifier_count(modifiers: egui::Modifiers) -> u32 {
    u32::from(modifiers.command)
        + u32::from(modifiers.shift)
        + u32::from(modifiers.alt)
        + u32::from(modifiers.mac_cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Keymap;
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
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::SaveAs]
                );
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
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::Save]
                );
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
                    assert_eq!(poll_shortcuts(&ctx, &Keymap::builtin()), vec![cmd]);
                    assert!(
                        poll_shortcuts(&ctx, &Keymap::builtin()).is_empty(),
                        "同一帧重复消费"
                    );
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
                assert!(poll_shortcuts(ui.ctx(), &Keymap::builtin()).is_empty());
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
        assert_eq!(Command::AiMockStream.message(), Message::AiStart);
        assert_eq!(
            Command::AiCommitMessage.message(),
            Message::AiCommitRequested
        );
        assert_eq!(Command::AiSummary.message(), Message::AiSummaryRequested);
    }

    /// AI 命令不绑快捷键(ask 约束:避免与现有键位冲突),其余命令全部有绑定。
    #[test]
    fn only_ai_commands_lack_shortcut() {
        for cmd in [
            Command::New,
            Command::Open,
            Command::Save,
            Command::SaveAs,
            Command::ExportHtml,
            Command::ToggleTheme,
            Command::ToggleSidebar,
        ] {
            assert!(cmd.default_shortcut().is_some(), "{cmd:?} 应有默认快捷键");
        }
        assert_eq!(Command::AiMockStream.default_shortcut(), None);
        assert_eq!(Command::AiCommitMessage.default_shortcut(), None);
        assert_eq!(Command::AiSummary.default_shortcut(), None);
    }

    /// 改绑生效:把「保存」改到 Ctrl+K 后,原 Ctrl+S 不再触发任何命令,
    /// 新键位触发保存 —— 键位来自 keymap 而不是硬编码。
    #[test]
    fn rebound_shortcut_replaces_default() {
        let mut keymap = Keymap::builtin();
        keymap.set(
            Command::Save,
            Some(crate::keymap::Shortcut {
                modifiers: Modifiers::COMMAND,
                key: Key::K,
            }),
        );

        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::S, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert!(poll_shortcuts(ui.ctx(), &keymap).is_empty(), "旧键位已解绑");
            },
        );
        output.drop_without_applying_deltas();

        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::K, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(poll_shortcuts(ui.ctx(), &keymap), vec![Command::Save]);
            },
        );
        output.drop_without_applying_deltas();
    }

    /// 未绑定的命令不消费任何键(清除绑定 = 只能从菜单触发)。
    #[test]
    fn unbound_command_consumes_nothing() {
        let mut keymap = Keymap::builtin();
        keymap.set(Command::Save, None);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::S, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert!(poll_shortcuts(ui.ctx(), &keymap).is_empty());
            },
        );
        output.drop_without_applying_deltas();
    }
}
