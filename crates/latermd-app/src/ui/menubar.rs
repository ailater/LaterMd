//! 顶部菜单栏:全部命令的可发现性入口(docs/roadmap.md P0「快捷键」;
//! 全量覆盖矩阵与豁免口径见 docs/menu-coverage.md)。
//!
//! 菜单结构是**常量清单**([`MENUS`]:标题 + 分组段):绘制与覆盖测试共用
//! 同一事实源,新命令忘进菜单时 `every_command_has_a_menu_entry` 红。
//! 分组规范:同类聚组、段与段之间分隔线、常用在前;菜单栏顺序
//! 文件 → 编辑 → 格式 → 视图 → 导出 → AI → 设置(高频编辑动作在前,
//! 派生动作在后)。键位文本取自 `keymap`(用户可改,与工具栏按钮 tooltip
//! 的 decisions-pending #32 同源先例一致)—— 改键后两处同时变。
//!
//! 点击只产出 [`Message`],执行在 `App::logic` 归约 —— 与快捷键入口
//! (`crate::command::poll_shortcuts`)殊途同归。外层 top panel 在 `ui::layout`。

use crate::command::Command;
use crate::keymap::Keymap;
use crate::settings::SettingsTab;
use crate::state::Message;

use eframe::egui;

/// 「文件」两段:文件动作(常用序:新建 → 打开 → 快速打开 → 保存 →
/// 另存为)+ 标签命令挂尾部(编辑器惯例:文件 → 关闭标签页),与
/// Ctrl+Tab / Ctrl+W / Ctrl+Shift+T 的快捷键入口互为可发现性。
const FILE_MENU: [&[Command]; 2] = [
    &[
        Command::New,
        Command::Open,
        Command::QuickOpen,
        Command::Save,
        Command::SaveAs,
    ],
    &[Command::TabNext, Command::TabClose, Command::TabRestore],
];

/// 「编辑」一段:文档内动作(undo/redo 是 TextEdit 内建,不列)。
const EDIT_MENU: [&[Command]; 1] = [&[
    Command::DuplicateSelection,
    Command::DuplicateLine,
    Command::FindInDoc,
    Command::ReplaceInDoc,
    Command::GotoLine,
]];

/// 「格式」五段:行内 / 标题 / 块 / 列表四段与格式工具条
/// (`ui::format_bar` 的 `FormatGroup`)同一口径;末尾「插入」段收两个
/// 对话框类动作(图片要 alt+url、Emoji 要点选,点了不改文档,与上面
/// 十五条直接改文档的格式动作隔开)。
const FORMAT_MENU: [&[Command]; 5] = [
    &[
        Command::FormatBold,
        Command::FormatItalic,
        Command::FormatStrike,
        Command::FormatInlineCode,
        Command::FormatLink,
    ],
    &[Command::FormatH1, Command::FormatH2, Command::FormatH3],
    &[
        Command::FormatQuote,
        Command::FormatCodeBlock,
        Command::FormatDivider,
        Command::FormatTable,
    ],
    &[
        Command::FormatBullet,
        Command::FormatOrdered,
        Command::FormatTask,
    ],
    &[Command::ImageInsert, Command::EmojiPicker],
];

/// 「视图」两段:布局/外观开关;禅定模式是整套面板组合(不是普通开关),
/// 独立一段隔开。
const VIEW_MENU: [&[Command]; 2] = [
    &[
        Command::ToggleSidebar,
        Command::ToggleRightPreview,
        Command::ToggleLivePreview,
        Command::ToggleTheme,
    ],
    &[Command::ToggleZen],
];

/// 「导出」一段:两者都是派生物,不触碰文档落盘身份。
const EXPORT_MENU: [&[Command]; 1] = [&[Command::ExportHtml, Command::ExportPdf]];

/// 「AI」一段。
const AI_MENU: [&[Command]; 1] = [&[
    Command::AiMockStream,
    Command::AiCommitMessage,
    Command::AiSummary,
]];

/// 菜单清单:数组顺序即菜单栏从左到右的显示顺序,段内顺序即条目顺序。
/// 「设置」不是命令(直达设置页),不进本表,由 [`ui`] 单独绘制。
const MENUS: [(&str, &[&[Command]]); 6] = [
    ("文件", &FILE_MENU),
    ("编辑", &EDIT_MENU),
    ("格式", &FORMAT_MENU),
    ("视图", &VIEW_MENU),
    ("导出", &EXPORT_MENU),
    ("AI", &AI_MENU),
];

/// 绘制菜单栏内容(挂在 top panel 内)。
pub fn ui(bar: &mut egui::Ui, keymap: &Keymap, outbox: &mut Vec<Message>) {
    egui::MenuBar::new().ui(bar, |ui| {
        for (title, sections) in MENUS {
            ui.menu_button(title, |ui| {
                draw_sections(ui, sections, keymap, outbox);
            });
        }
        // 设置:菜单保留两个直达页(外观 / 快捷键),完整五页由工具栏齿轮开
        ui.menu_button("设置", |ui| {
            for tab in [SettingsTab::Appearance, SettingsTab::Keymap] {
                if ui.button(tab.label()).clicked() {
                    outbox.push(Message::SettingsOpened(tab));
                }
            }
        });
    });
}

/// 一份菜单的分组段:段间画分隔线(首段之前不画),段内逐条渲染。
/// 独立成函数是为了让无头测试不经过 `menu_button`(未展开的菜单不执行
/// 闭包)就能渲染全部条目 —— 分组渲染不 panic 的断言面。
fn draw_sections(
    ui: &mut egui::Ui,
    sections: &[&[Command]],
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        for cmd in *section {
            item(ui, *cmd, keymap, outbox);
        }
    }
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
    use crate::command::CommandGroup;
    use crate::state::Message;
    use egui::{Event, FullOutput, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 命令分组 → 该组命令应挂的菜单标题。Tab 组挂「文件」尾部是既有口径
    /// (`CommandGroup::Tab` 文档:编辑器惯例,蒙层才独立成组);其余组与
    /// 菜单一一对应 —— 菜单归属与命令注册表同一事实源,蒙层/设置页共用。
    fn expected_menu(group: CommandGroup) -> &'static str {
        match group {
            CommandGroup::File | CommandGroup::Tab => "文件",
            CommandGroup::Edit => "编辑",
            CommandGroup::Format => "格式",
            CommandGroup::View => "视图",
            CommandGroup::Export => "导出",
            CommandGroup::Ai => "AI",
            // every_command_has_a_group(command.rs)已钉住无人落 Other,
            // 这里只做映射完备,不兜底
            CommandGroup::Other => "其他",
        }
    }

    /// 菜单常量里收集到的 (菜单标题, 命令) 全集。
    fn menu_entries() -> Vec<(&'static str, Command)> {
        let mut entries = Vec::new();
        for (title, sections) in MENUS {
            for section in sections {
                for cmd in *section {
                    entries.push((title, *cmd));
                }
            }
        }
        entries
    }

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

    /// AI 菜单项(无快捷键的命令之一)正常渲染并可点击发起:item() 的
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

    /// 「编辑」菜单含全部五条文档内动作(菜单归属与命令注册表同一事实源,
    /// 蒙层/设置页共用)。
    #[test]
    fn edit_menu_lists_every_edit_command() {
        let edit: Vec<Command> = EDIT_MENU.iter().flat_map(|s| s.iter().copied()).collect();
        assert!(edit.contains(&Command::GotoLine), "跳转到行进「编辑」菜单");
        for cmd in &edit {
            assert_eq!(
                cmd.group(),
                CommandGroup::Edit,
                "{cmd:?} 挂在「编辑」菜单就必须是 Edit 组"
            );
        }
        // 反向补漏:注册表里每一条 Edit 组命令都必须在菜单里(新命令忘记
        // 加菜单时在这里红)
        for cmd in Command::ALL {
            if cmd.group() == CommandGroup::Edit {
                assert!(
                    edit.contains(&cmd),
                    "{cmd:?} 是 Edit 组命令却不在「编辑」菜单"
                );
            }
        }
    }

    /// 全命令覆盖断言(矩阵的测试面,docs/menu-coverage.md):遍历
    /// `Command::ALL`,除矩阵标注豁免项外每条命令都恰好出现在一个菜单里
    /// 一次。当前豁免清单为空(注册表不含上下文类浮标动作,全量命令都
    /// 适合进菜单);新命令加进 ALL 而忘进 MENUS 时这里红。
    #[test]
    fn every_command_has_a_menu_entry() {
        let entries = menu_entries();
        for cmd in Command::ALL {
            let count = entries.iter().filter(|(_, listed)| *listed == cmd).count();
            assert_eq!(
                count, 1,
                "{cmd:?} 应恰好出现在一个菜单里一次(实际 {count} 次);\
                 豁免口径见 docs/menu-coverage.md"
            );
        }
    }

    /// 菜单归属与命令分组一致(分组规范的测试面):每条命令挂的菜单 ==
    /// `Command::group()` 对应菜单,Tab 组挂「文件」尾部是 `CommandGroup`
    /// 文档的既定豁免。菜单常量里出现「不属于这个菜单的命令」时红。
    #[test]
    fn menu_placement_matches_command_group() {
        for (title, cmd) in menu_entries() {
            assert_eq!(
                title,
                expected_menu(cmd.group()),
                "{cmd:?}({}) 应挂在「{}」菜单",
                cmd.group().label(),
                expected_menu(cmd.group())
            );
        }
    }

    /// 分组渲染不 panic:全部菜单的分组段展开渲染(`menu_button` 未展开时
    /// 闭包不执行,所以直接调 `draw_sections`),纯渲染不产生消息;整条
    /// 菜单栏(收起态)同样渲染一帧。
    #[test]
    fn all_menu_sections_render_without_panic() {
        let ctx = egui::Context::default();
        let keymap = Keymap::builtin();
        let mut outbox = Vec::new();
        for (_, sections) in MENUS {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                draw_sections(ui, sections, &keymap, &mut outbox);
            });
            output.drop_without_applying_deltas();
        }
        assert!(outbox.is_empty(), "纯渲染不产生消息");
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui(ui, &keymap, &mut outbox);
        });
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty(), "菜单栏收起态渲染不产生消息");
    }

    /// 新增菜单入口(格式 17 条 + 视图 3 条,以及既有全部条目)逐条点击
    /// 一次,发出的消息就是该命令的归约入口 —— 与快捷键触发
    /// (`command::poll_shortcuts` → `cmd.message()`)殊途同归。
    #[test]
    fn clicking_every_menu_item_sends_its_command_message() {
        for (_, sections) in MENUS {
            for cmd in sections.iter().flat_map(|s| s.iter().copied()) {
                let ctx = egui::Context::default();
                let mut outbox = Vec::new();
                let keymap = Keymap::builtin();
                let rect = Cell::new(Rect::NOTHING);
                ctx.run_ui(RawInput::default(), |ui| {
                    rect.set(item(ui, cmd, &keymap, &mut outbox).rect);
                })
                .drop_without_applying_deltas();
                assert!(outbox.is_empty(), "{cmd:?} 仅渲染不产生消息");

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
                        item(ui, cmd, &keymap, &mut outbox);
                    },
                )
                .drop_without_applying_deltas();
                assert_eq!(
                    outbox,
                    vec![cmd.message()],
                    "{cmd:?} 的菜单入口应发出该命令的归约消息"
                );
            }
        }
    }

    /// 菜单项右侧的键位文本与 keymap 同源(照 #32 tooltip 同源先例):
    /// 出厂绑定如实显示,用户改绑后跟随新键位、旧键位文本消失。抽查
    /// 三面:Save(常用带修饰键)、GotoLine(近期新增)、ExportPdf(出厂
    /// 无绑定的对照 —— 无绑定条目不该挤出任何键位文本)。
    #[test]
    fn menu_item_shortcuts_follow_keymap() {
        fn shape_text(output: &FullOutput) -> String {
            output
                .shapes
                .iter()
                .filter_map(|clipped| {
                    let egui::epaint::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    Some(text.galley.job.text.clone())
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        let keymap = Keymap::builtin();
        let save_key = keymap
            .get(Command::Save)
            .expect("Save 出厂有绑定")
            .keyboard();
        let goto_key = keymap
            .get(Command::GotoLine)
            .expect("GotoLine 出厂有绑定")
            .keyboard();

        let ctx = egui::Context::default();
        let expected_save = ctx.format_shortcut(&save_key);
        let expected_goto = ctx.format_shortcut(&goto_key);
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            item(ui, Command::Save, &keymap, &mut outbox);
            item(ui, Command::GotoLine, &keymap, &mut outbox);
        });
        let text = shape_text(&output);
        output.drop_without_applying_deltas();
        assert!(
            text.contains(&expected_save),
            "保存条目应显示出厂键位 {expected_save:?}(实际绘制:{text:?})"
        );
        assert!(
            text.contains(&expected_goto),
            "跳转条目应显示出厂键位 {expected_goto:?}(实际绘制:{text:?})"
        );

        // 用户改绑:保存改 Ctrl+K 后,菜单项跟随新键位,旧键位文本不再出现
        let mut rebound = Keymap::builtin();
        rebound.set(
            Command::Save,
            Some(crate::keymap::Shortcut {
                modifiers: egui::Modifiers::COMMAND,
                key: egui::Key::K,
            }),
        );
        let new_key = rebound.get(Command::Save).expect("改绑后有绑定").keyboard();
        let ctx = egui::Context::default();
        let expected_new = ctx.format_shortcut(&new_key);
        let old_text = expected_save.clone();
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            item(ui, Command::Save, &rebound, &mut outbox);
        });
        let text = shape_text(&output);
        output.drop_without_applying_deltas();
        assert!(
            text.contains(&expected_new),
            "改绑后菜单项应显示 {expected_new:?}(实际绘制:{text:?})"
        );
        assert!(
            !text.contains(&old_text),
            "改绑后旧键位 {old_text:?} 不该继续显示(实际绘制:{text:?})"
        );

        // 无绑定对照:ExportPdf 出厂无键位(command.rs 刻意不绑),条目
        // 只显示名字 —— 渲染面由 ai_item_renders_without_shortcut… 同款
        // None 分支覆盖,这里钉住「出厂确实无绑定」这一前提
        assert_eq!(keymap.get(Command::ExportPdf), None);
    }
}
