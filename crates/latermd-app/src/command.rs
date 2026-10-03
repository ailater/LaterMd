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
    /// 快速打开…(#24):居中浮层模糊搜文件树全部 md(打开)与命令全集
    /// (执行)。命令与键位先注册(本条),浮层状态机与绘制在 C3 接入,
    /// 归约暂只翻转最小标志。
    QuickOpen,
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
    /// 恢复最近关闭的标签(#45,Cmd/Ctrl+Shift+T,浏览器/VS Code 同款):
    /// 从关闭栈弹出最近一条**已落盘**路径重开;未落盘新标签关闭不入栈,
    /// 跨会话不恢复(取舍见 preview-typography-and-keymap-plan §3.3)。
    TabRestore,
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
    /// 插入图片(docs/image-plan.md A 段):开「图片框」对话框,填 alt 与
    /// URL 后插入 `![alt](url)`。
    ///
    /// 与上面十六条不同 —— 那些点了就直接改文档,这条要先收两个输入,因此
    /// 归约是「开对话框」而不是「执行格式」。它也没有
    /// [`Self::format_action`]:真正的文本动作由 `compose::insert_image` 承担。
    ImageInsert,
    /// 插入 Emoji(docs/emoji-plan.md E1):开 Emoji 面板,点选即在光标处
    /// 插入纯 Unicode 字符。与 `ImageInsert` 同属「对话框类动作」—— emoji
    /// 字符无法从 text+sel 推导,不走 `format_action`;真正的文本动作由
    /// `compose::insert_emoji` 承担。
    EmojiPicker,
    /// 复制选中(无选中复制当前行)——编辑器语义,坤哥 2026-09-29 指令。
    DuplicateSelection,
    /// 复制当前行(选区多行时复制全部涉及行)。
    DuplicateLine,
    /// 打开文档内查找条(Ctrl+F)。
    FindInDoc,
    /// 打开查找条的替换行(Ctrl+H,查找条同开);已展开时再按收起替换行。
    ReplaceInDoc,
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
    pub const ALL: [Command; 38] = [
        Self::New,
        Self::Open,
        Self::QuickOpen,
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
        Self::TabRestore,
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
        Self::ImageInsert,
        Self::EmojiPicker,
        Self::DuplicateSelection,
        Self::DuplicateLine,
        Self::FindInDoc,
        Self::ReplaceInDoc,
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
            Self::DuplicateSelection => FormatAction::DuplicateSelection,
            Self::DuplicateLine => FormatAction::DuplicateLine,
            _ => return None,
        })
    }

    /// 稳定 id:快捷键表 `keymap.json` 的键。命令的显示名会随文案调整,
    /// id 不随,存档才不会因改 label 而失效。
    pub fn id(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Open => "open",
            Self::QuickOpen => "quick_open",
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
            Self::TabRestore => "tab_restore",
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
            Self::ImageInsert => "image_insert",
            Self::EmojiPicker => "emoji_picker",
            Self::DuplicateSelection => "duplicate_selection",
            Self::DuplicateLine => "duplicate_line",
            Self::FindInDoc => "find_in_doc",
            Self::ReplaceInDoc => "replace_in_doc",
            Self::ToggleRightPreview => "toggle_right_preview",
            Self::ToggleZen => "toggle_zen",
        }
    }

    /// 菜单与按钮的显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::New => "新建",
            Self::Open => "打开",
            Self::QuickOpen => "快速打开…",
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
            Self::TabRestore => "恢复关闭的标签",
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
            // 图片单独取名:工具条按钮显示的是 `FormatAction::Image` 的
            // 「图片」,命令层要的是动作名「插入图片」(快捷键设置页里
            // 「图片」两个字说不清是干什么的)
            Self::ImageInsert => "插入图片",
            Self::EmojiPicker => "插入 Emoji",
            Self::DuplicateSelection => "复制选中(Ctrl+D)",
            Self::DuplicateLine => "复制当前行",
            Self::FindInDoc => "查找",
            Self::ReplaceInDoc => "替换",
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
            // 快速打开(#24):VS Code / 主流编辑器同款;出厂表 P 键无占用者
            Self::QuickOpen => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::P),
            Self::Save => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::S),
            Self::SaveAs => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::S)
            }
            Self::ExportHtml => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::E),
            // 主题 → Alt+T(preview-typography-and-keymap-plan.md §3.2 定案):
            // Cmd/Ctrl+Shift+T 让位给 TabRestore(浏览器「恢复关闭标签」同款,
            // #45 K2)。Alt 系与 VS Code「颜色主题」习惯相通;Alt 是修饰键,
            // `bindable()` 不需要开后门。旧 keymap.json 里值仍等于旧默认
            // (Cmd/Ctrl+Shift+T)的条目由 `keymap::load_from` 迁移到新默认。
            Self::ToggleTheme => egui::KeyboardShortcut::new(Modifiers::ALT, egui::Key::T),
            Self::ToggleSidebar => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Backslash)
            }
            Self::TabNext => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::Tab),
            Self::TabClose => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::W),
            // 恢复关闭的标签(#45 K2):浏览器/VS Code 同款。K1 已把主题
            // 改排 Alt+T 让出此键位,出厂表占用者就是本命令。
            Self::TabRestore => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::T)
            }
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
            // 图片:加 Shift 是因为 Ctrl/Cmd+I 已是斜体(与链接同款处理:
            // 占用键位前先看格式命令已经占过什么)。
            Self::ImageInsert => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::I)
            }
            // Emoji 面板:Ctrl/Cmd+Shift+E 在出厂表里无占用者(与导出的
            // Ctrl/Cmd+E 只差一个 Shift;消费顺序按修饰键个数降序,Shift
            // 组合先被问到,两者互不抢 —— 与 SaveAs 之于 Save 同款共存,
            // 见 `emoji_shortcut_overlaps_export_but_does_not_conflict`)。
            Self::EmojiPicker => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::E)
            }
            Self::DuplicateSelection => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::D)
            }
            Self::DuplicateLine => {
                egui::KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, egui::Key::D)
            }
            Self::FindInDoc => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::F),
            // Ctrl/Cmd+H:主流编辑器的替换键位,出厂表无占用者
            Self::ReplaceInDoc => egui::KeyboardShortcut::new(Modifiers::COMMAND, egui::Key::H),
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
            // 快速打开是「搜文件」,与文档内查找共用放大镜
            Self::QuickOpen => Icon::Search,
            Self::Save => Icon::Save,
            Self::SaveAs => Icon::SaveAs,
            Self::ExportHtml => Icon::Export,
            Self::ToggleTheme => Icon::Theme,
            Self::ToggleSidebar => Icon::Sidebar,
            Self::AiMockStream | Self::AiCommitMessage | Self::AiSummary => Icon::Ai,
            Self::TabNext | Self::TabClose | Self::TabRestore => Icon::Files,
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
            Self::ImageInsert => Icon::Image,
            Self::EmojiPicker => Icon::Emoji,
            Self::DuplicateSelection
            | Self::DuplicateLine
            | Self::FindInDoc
            | Self::ReplaceInDoc => Icon::Search,
            Self::ToggleRightPreview => Icon::PanelRight,
            Self::ToggleZen => Icon::Zen,
        }
    }

    /// 归约入口:命令翻成状态消息,执行在 `State::apply`。
    pub fn message(self) -> Message {
        match self {
            Self::New => Message::FileCommand(FileCmd::New),
            Self::Open => Message::FileCommand(FileCmd::Open),
            Self::QuickOpen => Message::ToggleQuickOpen,
            Self::Save => Message::FileCommand(FileCmd::Save),
            Self::SaveAs => Message::FileCommand(FileCmd::SaveAs),
            Self::ExportHtml => Message::ExportHtml,
            Self::ToggleTheme => Message::ToggleTheme,
            Self::ToggleSidebar => Message::SidebarToggled,
            Self::AiMockStream => Message::AiStart,
            Self::AiCommitMessage => Message::AiCommitRequested,
            Self::AiSummary => Message::AiSummaryRequested,
            Self::ImageInsert => Message::ImageDialogOpened,
            Self::EmojiPicker => Message::EmojiPickerToggle(true),
            Self::ToggleLivePreview => Message::ToggleLivePreview,
            Self::TabNext => Message::TabNext,
            Self::TabClose => Message::TabCloseActive,
            Self::TabRestore => Message::TabRestore,
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
            | Self::FormatTask
            | Self::DuplicateSelection
            | Self::DuplicateLine => Message::FormatRequested(self.format_action().unwrap()),
            Self::FindInDoc => Message::FindBarToggled(true),
            Self::ReplaceInDoc => Message::ReplaceBarToggled(true),
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

    /// Emoji 面板键位的撞键核查(emoji-plan §6.3 / decisions-pending #9
    /// 口径):出厂表里无占用者;它与导出(Ctrl/Cmd+E)只差一个 Shift,
    /// `consume_shortcut` 的 `matches_logically` 会忽略多余 Shift —— 靠
    /// 「修饰键个数降序」的消费顺序共存(与 SaveAs/Save 同款):Shift 组合
    /// 先被问到,两个键各自只触发一条命令。
    #[test]
    fn emoji_shortcut_overlaps_export_but_does_not_conflict() {
        let emoji = crate::keymap::Shortcut {
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
            key: Key::E,
        };
        // #9 口径的静态核查:撞键检测按整条 Shortcut 相等,出厂表无人占用
        assert_eq!(
            Keymap::builtin().conflict(Command::EmojiPicker, emoji),
            None,
            "Ctrl/Cmd+Shift+E 不该撞任何出厂键位"
        );

        for (key, modifiers, expected) in [
            (
                Key::E,
                Modifiers::COMMAND | Modifiers::SHIFT,
                vec![Command::EmojiPicker],
            ),
            (Key::E, Modifiers::COMMAND, vec![Command::ExportHtml]),
        ] {
            let ctx = egui::Context::default();
            let output = ctx.run_ui(
                RawInput {
                    events: vec![key_event(key, modifiers)],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(poll_shortcuts(ui.ctx(), &Keymap::builtin()), expected);
                },
            );
            output.drop_without_applying_deltas();
        }
    }

    /// 全部命令的绑定都能被各自按键触发,且消费一次后同帧不回流。
    #[test]
    fn every_command_fires_exactly_once() {
        let ctx = egui::Context::default();
        let bindings = [
            (Command::New, Key::N, Modifiers::COMMAND),
            (Command::Open, Key::O, Modifiers::COMMAND),
            (Command::ExportHtml, Key::E, Modifiers::COMMAND),
            (Command::ToggleTheme, Key::T, Modifiers::ALT),
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

    /// 替换命令(#17):Ctrl/Cmd+H 出厂即绑且只触发这一条;命令映射到
    /// 替换条打开消息(开/关语义在归约侧翻转发)。
    #[test]
    fn replace_shortcut_fires_replace_command() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::H, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::ReplaceInDoc]
                );
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(
            Command::ReplaceInDoc.message(),
            Message::ReplaceBarToggled(true)
        );
    }

    /// 键位改排(#45 K1,preview-typography §3.2 定案):主题出厂键 = Alt+T,
    /// 不撞任何出厂键位;Cmd/Ctrl+Shift+T 的占用者 == TabRestore(#45 K2
    /// 落地后的联动断言,浏览器「恢复关闭标签」同款),且不与任何其他
    /// 出厂键位冲突。
    #[test]
    fn theme_is_alt_t_and_restore_slot_is_free() {
        let alt_t = crate::keymap::Shortcut {
            modifiers: Modifiers::ALT,
            key: Key::T,
        };
        assert_eq!(
            Command::ToggleTheme.default_shortcut().map(|shortcut| {
                crate::keymap::Shortcut {
                    modifiers: shortcut.modifiers,
                    key: shortcut.logical_key,
                }
            }),
            Some(alt_t),
            "ToggleTheme 出厂默认 = Alt+T"
        );
        assert_eq!(Keymap::builtin().get(Command::ToggleTheme), Some(alt_t));
        // #9 口径的撞键核查:Alt+T 不撞任何出厂键位
        assert_eq!(
            Keymap::builtin().conflict(Command::ToggleTheme, alt_t),
            None,
            "Alt+T 不该撞任何出厂键位"
        );

        let restore_slot = crate::keymap::Shortcut {
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
            key: Key::T,
        };
        assert_eq!(
            Keymap::builtin().get(Command::TabRestore),
            Some(restore_slot),
            "Cmd/Ctrl+Shift+T 的占用者 = TabRestore(#45 K2)"
        );
        assert_eq!(
            Keymap::builtin().conflict(Command::TabRestore, restore_slot),
            None,
            "Cmd/Ctrl+Shift+T 不该撞任何其他出厂键位"
        );
    }

    /// #45 K2 默认键位:Cmd/Ctrl+Shift+T 真按下只触发 TabRestore 一条
    /// (K1 已把主题挪到 Alt+T,该键位无第二个消费者);消息映射到
    /// `Message::TabRestore`。
    #[test]
    fn restore_shortcut_fires_only_tab_restore() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::T, Modifiers::COMMAND | Modifiers::SHIFT)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::TabRestore]
                );
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(Command::TabRestore.message(), Message::TabRestore);
    }

    /// Alt+T 真按键只触发主题切换:`matches_logically` 对「显式不要
    /// Ctrl/Cmd」的组合要求事件确实没按 Ctrl/Cmd,故 Cmd/Ctrl+Alt+T 不会
    /// 误中;与 ToggleRightPreview(Cmd/Ctrl+Alt+R)不同键,互不抢。
    #[test]
    fn alt_t_fires_only_toggle_theme() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::T, Modifiers::ALT)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::ToggleTheme]
                );
            },
        );
        output.drop_without_applying_deltas();
    }

    /// 快速打开(#24 C2)默认键位:Cmd/Ctrl+P,出厂表无第二个占用者
    /// (撞键拒绝口径,decisions-pending.md 现状登记在 #22「界面打磨
    /// 批次」;本文件既有测试注释引作 #9,与该文件现状不符)。真按键
    /// 只触发这一条,消息映射到 `Message::ToggleQuickOpen`。
    #[test]
    fn quick_open_is_ctrl_p_and_conflict_free() {
        let ctrl_p = crate::keymap::Shortcut {
            modifiers: Modifiers::COMMAND,
            key: Key::P,
        };
        assert_eq!(
            Command::QuickOpen.default_shortcut().map(|shortcut| {
                crate::keymap::Shortcut {
                    modifiers: shortcut.modifiers,
                    key: shortcut.logical_key,
                }
            }),
            Some(ctrl_p),
            "QuickOpen 出厂默认 = Cmd/Ctrl+P"
        );
        assert_eq!(Keymap::builtin().get(Command::QuickOpen), Some(ctrl_p));
        assert_eq!(
            Keymap::builtin().conflict(Command::QuickOpen, ctrl_p),
            None,
            "Cmd/Ctrl+P 不该撞任何出厂键位"
        );

        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(Key::P, Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                assert_eq!(
                    poll_shortcuts(ui.ctx(), &Keymap::builtin()),
                    vec![Command::QuickOpen]
                );
            },
        );
        output.drop_without_applying_deltas();
        assert_eq!(Command::QuickOpen.message(), Message::ToggleQuickOpen);
    }

    /// ALL 数组与命令集同步:新命令忘了进 ALL 的话,快捷键派发、设置页
    /// 遍历与 keymap 存档都会漏掉它。长度与无重复钉在这里,加命令时随
    /// 实现更新。
    #[test]
    fn all_commands_listed_exactly_once() {
        assert_eq!(Command::ALL.len(), 38);
        let mut ids: Vec<_> = Command::ALL.iter().map(|cmd| cmd.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Command::ALL.len(), "ALL 里不得有重复命令");
    }
}
