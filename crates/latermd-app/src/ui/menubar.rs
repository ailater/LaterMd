//! 顶部菜单栏:全部命令的可发现性入口(docs/roadmap.md P0「快捷键」;
//! 全量覆盖矩阵与豁免口径见 docs/menu-coverage.md)。
//!
//! 菜单结构是**常量清单**([`MENUS`]:标题 + 助记字母 + 分组段):绘制与
//! 覆盖测试共用同一事实源,新命令忘进菜单时 `every_command_has_a_menu_entry`
//! 红。分组规范:同类聚组、段与段之间分隔线、常用在前;菜单栏顺序
//! 文件 → 编辑 → 格式 → 视图 → 导出 → AI → 设置 → 帮助(高频编辑动作
//! 在前,派生动作在后,帮助居末位是桌面惯例)。键位文本取自 `keymap`(用户可改,与工具栏按钮 tooltip
//! 的 decisions-pending #32 同源先例一致)—— 改键后两处同时变。
//!
//! #62 M2 的 Alt 助记键:egui 0.36.2 **不支持** `&X` 助记语法(egui/epaint
//! 全 crate `mnemonic` 零命中,`Button`/`MenuButton` 文本路径不解析 `&`,
//! 核验记录见 decisions-pending #119),故在菜单层自建轻量助记:
//! * 标题层:Alt+字母 打开对应菜单(`Popup::open_id` 驱动 egui 公开的
//!   popup 记忆,严格匹配「仅 Alt」——Ctrl+Alt+R 等组合不误触);
//! * 条目层:菜单展开时裸字母触发对应条目并收起(Windows 惯例);
//! * 单击 Alt(按下→抬起,中间无其他输入)聚焦菜单栏首个菜单。
//!
//! 助记字母与 keymap 的冲突审计由 `title_mnemonics_unique_and_clear_of_
//! keymap_alt_bindings` 遍历断言零冲突;macOS 无 Alt 助记惯例,整个
//! 助记层(显示后缀 + 键盘触发)在 mac 编译目标下关闭。
//!
//! 点击只产出 [`Message`],执行在 `App::logic` 归约 —— 与快捷键入口
//! (`crate::command::poll_shortcuts`)殊途同归。外层 top panel 在 `ui::layout`。

use crate::command::Command;
use crate::keymap::Keymap;
use crate::live::RenderMode;
use crate::settings::SettingsTab;
use crate::state::{Message, State};
use crate::ui::tokens;

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

/// 「编辑」一段:文档内动作(undo/redo 是 TextEdit 内建,不列)。插入
/// 目录(#66 M2)同段:与复制行/查找/替换/跳转同族的不作用选区的文档级
/// 辅助动作(归位口径 decisions-pending #127)。
const EDIT_MENU: [&[Command]; 1] = [&[
    Command::DuplicateSelection,
    Command::DuplicateLine,
    Command::FindInDoc,
    Command::ReplaceInDoc,
    Command::GotoLine,
    Command::InsertToc,
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

/// 「视图」两段:布局/外观开关(打字机 #64 与专注模式 #64 M2、Live 同段,
/// 同为编辑区呈现方式开关;minimap #67 M2 归此段「专注」之后「主题」
/// 之前 —— 编辑区呈现方式开关聚在主题互换之前,位置取舍见
/// decisions-pending #129);禅定模式是整套面板组合(不是普通开关),
/// 独立一段隔开。
const VIEW_MENU: [&[Command]; 2] = [
    &[
        Command::ToggleSidebar,
        Command::ToggleRightPreview,
        Command::ToggleLivePreview,
        Command::TypewriterToggle,
        Command::FocusModeToggle,
        Command::ToggleMinimap,
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

/// 菜单清单:数组顺序即菜单栏从左到右的显示顺序,段内顺序即条目顺序;
/// 第二项是该菜单标题的助记字母(Alt 命名空间,**全局唯一**,与 keymap
/// 全部 Alt 类绑定的冲突审计见模块测试)。「设置」「帮助」不是命令
/// (直达设置页 / 关于窗,照「设置」先例单独绘制),不进本表。
pub(crate) const MENUS: [(&str, char, &[&[Command]]); 6] = [
    ("文件", 'F', &FILE_MENU),
    ("编辑", 'E', &EDIT_MENU),
    ("格式", 'O', &FORMAT_MENU),
    ("视图", 'V', &VIEW_MENU),
    ("导出", 'X', &EXPORT_MENU),
    ("AI", 'A', &AI_MENU),
];

/// 「设置」菜单的标题助记字母(与 [`MENUS`] 同一 Alt 命名空间)。
const SETTINGS_MNEMONIC: char = 'S';

/// 「帮助」菜单(#71 M1)的标题助记字母(H = Help 词首;与 [`MENUS`]
/// 的 F/E/O/V/X/A/S 及 `SETTINGS_MNEMONIC` 在 Alt 命名空间互不撞,keymap
/// 出厂 Alt 系七条也没有 Alt+H —— 冲突审计见
/// `title_mnemonics_unique_and_clear_of_keymap_alt_bindings`)。
const HELP_MNEMONIC: char = 'H';

/// 「帮助」菜单条目「关于 LaterMD…」的条目助记(A = About 词首;帮助
/// 菜单内唯一)。
const ABOUT_MNEMONIC: char = 'A';

/// 「设置」菜单两个直达页的条目助记(该菜单内唯一)。
const SETTINGS_TAB_MNEMONICS: [(SettingsTab, char); 2] =
    [(SettingsTab::Appearance, 'A'), (SettingsTab::Keymap, 'K')];

/// 菜单条目的助记字母(**菜单内**唯一;菜单展开时裸字母触发)。字母是
/// Windows 惯例位(中文菜单取英义词首/次字母),数字 1/2/3 给三档标题
/// (与快捷键 Ctrl+1/2/3 同族)。与 keymap 的关系:keymap 出厂表无任何
/// 裸字母绑定(`Shortcut::bindable` 拒绝),两个命名空间零交集 —— 冲突
/// 面只存在于标题层(Alt+字母),那层由 `MENUS` 表单独审计。
fn command_mnemonic(cmd: Command) -> char {
    match cmd {
        Command::New => 'N',
        Command::Open => 'O',
        Command::QuickOpen => 'Q',
        Command::Save => 'S',
        Command::SaveAs => 'A',
        Command::TabNext => 'T',
        Command::TabClose => 'C',
        Command::TabRestore => 'R',
        Command::DuplicateSelection => 'D',
        Command::DuplicateLine => 'L',
        Command::FindInDoc => 'F',
        Command::ReplaceInDoc => 'H',
        Command::GotoLine => 'G',
        // T = TOC 词首;编辑菜单内 D/L/F/H/G 均未占 T,菜单内唯一
        Command::InsertToc => 'T',
        Command::FormatBold => 'B',
        Command::FormatItalic => 'I',
        Command::FormatStrike => 'K',
        Command::FormatInlineCode => 'C',
        Command::FormatLink => 'L',
        Command::FormatH1 => '1',
        Command::FormatH2 => '2',
        Command::FormatH3 => '3',
        Command::FormatQuote => 'Q',
        Command::FormatCodeBlock => 'E',
        Command::FormatDivider => 'D',
        Command::FormatTable => 'T',
        Command::FormatBullet => 'U',
        Command::FormatOrdered => 'O',
        Command::FormatTask => 'W',
        Command::ImageInsert => 'P',
        Command::EmojiPicker => 'M',
        Command::ToggleSidebar => 'S',
        Command::ToggleRightPreview => 'P',
        Command::ToggleLivePreview => 'V',
        // W = typewriter 的词尾;视图菜单内 S/P/V/T 已被占用,菜单内唯一
        // (条目层助记是菜单内命名空间,与全局 Alt 层无关)
        Command::TypewriterToggle => 'W',
        // F = Focus 的词首;视图菜单内 S/P/V/W/T/Z 均未占 F,菜单内唯一
        Command::FocusModeToggle => 'F',
        // M = Minimap 的词首;视图菜单内 S/P/V/W/F/T/Z 均未占 M,菜单内唯一
        Command::ToggleMinimap => 'M',
        Command::ToggleTheme => 'T',
        Command::ToggleZen => 'Z',
        Command::ExportHtml => 'H',
        Command::ExportPdf => 'P',
        Command::AiMockStream => 'M',
        Command::AiCommitMessage => 'C',
        Command::AiSummary => 'S',
    }
}

/// 菜单条的 Alt 助记状态(单击 Alt 的武装标志)挂在 egui 临时数据上:
/// 它是纯 UI 会话态,不进 `State` 存档,重启自然归零。
fn alt_arm_id() -> egui::Id {
    egui::Id::new("menubar-alt-arm")
}

/// 显示名 + 助记后缀:Win/Linux 显示「文件(F)」形态(mac 不显示)。
/// egui 的下划线只能作用于整段 `RichText`,无法只给单个字母画线
/// (epaint 无部分字形装饰),故用括号后缀 —— 中文 Windows 软件的标准形态。
fn label_with_mnemonic(label: &str, letter: char) -> String {
    if cfg!(target_os = "macos") {
        label.to_owned()
    } else {
        format!("{label}({letter})")
    }
}

/// 布偶开关命令的当前值(菜单条目勾选态;#67 M2):`Some(true/false)` =
/// 开/关,`None` = 非开关类命令(菜单条目不带勾选列)。
///
/// 口径:只覆盖**有布尔语义**的开关 —— 视图菜单的侧边栏/预览栏/Live/
/// 打字机/专注/minimap/禅定;「切换主题」是明暗互换而非开/关(哪个算
/// 「开」没有自然答案),不加勾选态。真值从 [`State`] 单一事实源取,
/// 与设置页/标题栏按钮同源(取舍见 decisions-pending #129)。
pub(crate) fn toggle_checked(cmd: Command, state: &State) -> Option<bool> {
    Some(match cmd {
        Command::ToggleSidebar => state.layout.left,
        Command::ToggleRightPreview => state.layout.right,
        Command::ToggleLivePreview => state.render_mode == RenderMode::Live,
        Command::TypewriterToggle => state.theme.show_typewriter,
        Command::FocusModeToggle => state.theme.show_focus_mode,
        Command::ToggleMinimap => state.theme.show_minimap,
        Command::ToggleZen => state.layout.zen,
        _ => return None,
    })
}

/// 勾选前缀:开 = `✓ `(U+2713 + 空格),关 = 两个半角空格占位(开关条目
/// 间的文字主体对齐;✓ 与两空格的字宽随字体略有出入,观感留真机核对)。
/// `None` = 非开关条目,无前缀。
fn check_prefix(checked: Option<bool>) -> &'static str {
    match checked {
        Some(true) => "✓ ",
        Some(false) => "  ",
        None => "",
    }
}

/// egui `Key` → 助记字母。`Key::name` 对字母/数字键恰好返回单字符
/// ("A".."Z"、"0".."9"),其余键名多字符,自然被过滤。
fn key_letter(key: egui::Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(single), None) => Some(single.to_ascii_uppercase()),
        _ => None,
    }
}

/// 本帧「仅 Alt + 字母」按键(标题层助记的触发面)。严格匹配:`matches_logically`
/// 会忽略事件多余的 Alt(egui 的宽松语义),而助记必须**只在**纯 Alt 时触发,
/// 否则 Ctrl+Alt+R(切换预览栏)会同时炸开菜单 —— 这里用位相等收死。
fn alt_menu_letter(ctx: &egui::Context) -> Option<char> {
    if cfg!(target_os = "macos") {
        return None;
    }
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } if *modifiers == egui::Modifiers::ALT => key_letter(*key),
            _ => None,
        })
    })
}

/// 本帧裸字母按键(条目层助记的触发面;菜单展开时才被 [`fire_item_letter`]
/// 消费,平时裸字母归编辑器输入)。
fn bare_key_letter(ctx: &egui::Context) -> Option<char> {
    if cfg!(target_os = "macos") {
        return None;
    }
    ctx.input(|input| {
        input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } if modifiers.is_none() => key_letter(*key),
            _ => None,
        })
    })
}

/// 单击 Alt(按下→抬起,中间无其他按键/输入)的检测:跨帧武装状态机,
/// Win/Linux「单击 Alt 聚焦菜单栏」惯例的实现面。Alt+字母的组合会在字母
/// 按下帧污染武装标志,不会误触发;Alt 键事件本身可达(egui-winit 把
/// `KeyCode::AltLeft/Right` 映射为 `Key::AltLeft/Right` 照常推 `Event::Key`,
/// lib.rs 核验记录见 decisions-pending #119)。
fn single_alt_click(ctx: &egui::Context) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let mut armed = ctx.data_mut(|data| data.get_temp::<bool>(alt_arm_id()).unwrap_or(false));
    let mut fired = false;
    ctx.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Key {
                    key: egui::Key::AltLeft | egui::Key::AltRight,
                    pressed: true,
                    repeat: false,
                    ..
                } => armed = true,
                // 长按 Alt 的 repeat 事件不污染(保持武装)
                egui::Event::Key {
                    key: egui::Key::AltLeft | egui::Key::AltRight,
                    pressed: true,
                    ..
                } => {}
                egui::Event::Key { pressed: true, .. } => armed = false,
                egui::Event::Text(_) => armed = false,
                // 失焦即缴械(#69):Alt 按下发生在本窗口、抬起却被 WM/他窗
                // 吃掉的切窗场景里,armed 会跨会话遗留 —— 之后从别处
                // Ctrl+Alt 切回时收到的孤立 Alt release 会被误判成单击,
                // 炸开「文件」菜单。egui-winit 从 winit 的 WindowEvent::
                // Focused 推送本事件(0.36.2 lib.rs:444),失焦帧必然早于
                // 下一次聚焦,在此清零即可靠切断遗留链。
                egui::Event::WindowFocused(false) => armed = false,
                egui::Event::Key {
                    key: egui::Key::AltLeft | egui::Key::AltRight,
                    pressed: false,
                    ..
                } => {
                    if armed {
                        fired = true;
                    }
                    armed = false;
                }
                _ => {}
            }
        }
    });
    // 指针按下同样污染(Alt+点击是窗口操作/拖拽的常见前缀)
    if ctx.input(|input| input.pointer.any_pressed()) {
        armed = false;
    }
    ctx.data_mut(|data| data.insert_temp(alt_arm_id(), armed));
    fired
}

/// 消费触发助记的那次按键(Key 事件连同同字母的 Text 事件):不消费的话,
/// 部分平台上 Alt+字母会带 `Event::Text` 流进聚焦的 TextEdit 插出字符
/// (egui-winit 的 `is_cmd` 不含 Alt)。
fn consume_letter_events(ctx: &egui::Context, letter: char) {
    let upper = letter.to_ascii_uppercase();
    let lower = letter.to_ascii_lowercase();
    ctx.input_mut(|input| {
        input.events.retain(|event| match event {
            egui::Event::Key { key, .. } => key_letter(*key).is_none_or(|ch| ch != upper),
            egui::Event::Text(text) => {
                let text = text.as_str();
                text.is_empty() || !text.chars().all(|ch| ch == lower || ch == upper)
            }
            _ => true,
        });
    });
}

/// 菜单展开帧的条目层助记:本帧裸字母命中本菜单某条目时,发消息并收起
/// 菜单。只在命中的菜单/条目上消费事件 —— 未命中字母原样放行(菜单开着
/// 时编辑器照常收字的现状不被扩大)。
fn fire_item_letter(ui: &egui::Ui, sections: &[&[Command]], outbox: &mut Vec<Message>) {
    let Some(letter) = bare_key_letter(ui.ctx()) else {
        return;
    };
    for cmd in sections.iter().flat_map(|section| section.iter().copied()) {
        if command_mnemonic(cmd) == letter {
            consume_letter_events(ui.ctx(), letter);
            outbox.push(cmd.message());
            ui.close();
            return;
        }
    }
}

/// 「设置」菜单展开帧的条目层助记(与 [`fire_item_letter`] 同口径,目标
/// 是设置页直达而非命令)。
fn fire_settings_item_letter(ui: &egui::Ui, outbox: &mut Vec<Message>) {
    let Some(letter) = bare_key_letter(ui.ctx()) else {
        return;
    };
    for (tab, mnemonic) in SETTINGS_TAB_MNEMONICS {
        if mnemonic == letter {
            consume_letter_events(ui.ctx(), letter);
            outbox.push(Message::SettingsOpened(tab));
            ui.close();
            return;
        }
    }
}

/// 「帮助」菜单展开帧的条目层助记(与 [`fire_settings_item_letter`] 同
/// 口径,目标是关于窗而非命令;#71 M1)。
fn fire_help_item_letter(ui: &egui::Ui, outbox: &mut Vec<Message>) {
    let Some(letter) = bare_key_letter(ui.ctx()) else {
        return;
    };
    if ABOUT_MNEMONIC == letter {
        consume_letter_events(ui.ctx(), letter);
        outbox.push(Message::AboutOpened);
        ui.close();
    }
}

/// 绘制菜单栏内容(挂在 top panel 内)。开关类条目的勾选态从 `state`
/// 取真值(与设置页/标题栏按钮同一事实源)。
pub fn ui(bar: &mut egui::Ui, keymap: &Keymap, state: &State, outbox: &mut Vec<Message>) {
    ui_with_probe(bar, keymap, state, outbox, None::<fn(usize, egui::Id)>);
}

/// 同 [`ui`],额外把每个菜单的 `(栏内序号, popup id)` 交给 `probe`
/// (`None` 即不探针;仿 `ui::format_bar::ui_with_probe` 先例)。
/// 无头测试用 popup id 断言「Alt 助记打开了对应菜单」——popup 的开合
/// 状态在 egui memory,`Popup::is_id_open` 按 id 查询。
#[allow(clippy::type_complexity)]
pub(crate) fn ui_with_probe(
    bar: &mut egui::Ui,
    keymap: &Keymap,
    state: &State,
    outbox: &mut Vec<Message>,
    mut probe: Option<impl FnMut(usize, egui::Id)>,
) {
    egui::MenuBar::new().ui(bar, |ui| {
        // 排版统一(#62 M2):标题间距用外壳 token(工具栏分组同档),
        // 字号/悬停态/内边距两侧(菜单条与下拉)同吃 `menu_style`
        // (egui 0.36 的 MenuBar 与 Popup::menu 继承同一份,无需重复投影)。
        ui.spacing_mut().item_spacing.x = tokens::SPACE_MD;
        // 助记输入检测每帧一次(循环外),菜单循环里只查结果
        let open_letter = alt_menu_letter(ui.ctx());
        let open_first = single_alt_click(ui.ctx());
        for (index, (title, mnemonic, sections)) in MENUS.iter().enumerate() {
            let response = ui
                .menu_button(label_with_mnemonic(title, *mnemonic), |ui| {
                    draw_sections(ui, sections, keymap, state, outbox);
                    fire_item_letter(ui, sections, outbox);
                })
                .response;
            if let Some(probe) = probe.as_mut() {
                probe(
                    index,
                    egui::containers::Popup::default_response_id(&response),
                );
            }
            // Alt+字母 / 单击 Alt → 打开对应(首个)菜单。open_id 写的是
            // egui popup 记忆,下一帧 menu_button 读到即展开;命中帧同时
            // 消费按键,防字母/文本漏进聚焦的编辑器。
            if open_letter == Some(*mnemonic) || (open_first && index == 0) {
                egui::containers::Popup::open_id(
                    ui.ctx(),
                    egui::containers::Popup::default_response_id(&response),
                );
                if let Some(letter) = open_letter {
                    consume_letter_events(ui.ctx(), letter);
                } else {
                    // 单击 Alt:消费 Alt 键的抬起事件即可(无字母副作用)
                    ui.ctx().input_mut(|input| {
                        input.events.retain(|event| {
                            !matches!(
                                event,
                                egui::Event::Key {
                                    key: egui::Key::AltLeft | egui::Key::AltRight,
                                    ..
                                }
                            )
                        });
                    });
                }
            }
        }
        // 设置:菜单保留两个直达页(外观 / 快捷键),完整五页由工具栏齿轮开
        let response = ui
            .menu_button(label_with_mnemonic("设置", SETTINGS_MNEMONIC), |ui| {
                for (tab, mnemonic) in SETTINGS_TAB_MNEMONICS {
                    if ui
                        .button(label_with_mnemonic(tab.label(), mnemonic))
                        .clicked()
                    {
                        outbox.push(Message::SettingsOpened(tab));
                    }
                }
                fire_settings_item_letter(ui, outbox);
            })
            .response;
        if let Some(probe) = probe.as_mut() {
            probe(
                MENUS.len(),
                egui::containers::Popup::default_response_id(&response),
            );
        }
        if open_letter == Some(SETTINGS_MNEMONIC) {
            egui::containers::Popup::open_id(
                ui.ctx(),
                egui::containers::Popup::default_response_id(&response),
            );
            consume_letter_events(ui.ctx(), SETTINGS_MNEMONIC);
        }
        // 帮助(#71 M1):非命令菜单(条目直达关于窗),照「设置」同款
        // 绘制与 Alt 助记;桌面惯例居菜单栏末位
        let response = ui
            .menu_button(label_with_mnemonic("帮助", HELP_MNEMONIC), |ui| {
                if ui
                    .button(label_with_mnemonic("关于 LaterMD…", ABOUT_MNEMONIC))
                    .clicked()
                {
                    outbox.push(Message::AboutOpened);
                }
                fire_help_item_letter(ui, outbox);
            })
            .response;
        if let Some(probe) = probe.as_mut() {
            probe(
                MENUS.len() + 1,
                egui::containers::Popup::default_response_id(&response),
            );
        }
        if open_letter == Some(HELP_MNEMONIC) {
            egui::containers::Popup::open_id(
                ui.ctx(),
                egui::containers::Popup::default_response_id(&response),
            );
            consume_letter_events(ui.ctx(), HELP_MNEMONIC);
        }
    });
}

/// 一份菜单的分组段:段间画分隔线(首段之前不画),段内逐条渲染。
/// 独立成函数是为了让无头测试不经过 `menu_button`(未展开的菜单不执行
/// 闭包)就能渲染全部条目 —— 分组渲染不 panic 的断言面。
fn draw_sections(
    ui: &mut egui::Ui,
    sections: &[&[Command]],
    keymap: &Keymap,
    state: &State,
    outbox: &mut Vec<Message>,
) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            ui.separator();
        }
        for cmd in *section {
            item(ui, *cmd, keymap, toggle_checked(*cmd, state), outbox);
        }
    }
}

/// 单个菜单项:显示名(带助记后缀,开关类条目前置勾选列 `checked`)+
/// 当前键位(未绑快捷键的命令只显示名字);点击发消息(egui 菜单内点击
/// 任意控件自动收起)。返回按钮响应,独立成函数便于点击测试定位。
pub fn item(
    ui: &mut egui::Ui,
    cmd: Command,
    keymap: &Keymap,
    checked: Option<bool>,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let mut button = egui::Button::new(format!(
        "{}{}",
        check_prefix(checked),
        label_with_mnemonic(cmd.label(), command_mnemonic(cmd))
    ));
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
        for (title, _, sections) in MENUS {
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
            rect.set(item(ui, Command::ToggleSidebar, &keymap, None, &mut outbox).rect);
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
                item(ui, Command::ToggleSidebar, &keymap, None, &mut outbox);
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
            rect.set(item(ui, Command::AiMockStream, &keymap, None, &mut outbox).rect);
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
                item(ui, Command::AiMockStream, &keymap, None, &mut outbox);
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
    /// 菜单栏(收起态)在**明暗两主题**下各渲染一帧(#62 M2 排版统一的
    /// 不 panic 断言面)。
    #[test]
    fn all_menu_sections_render_without_panic() {
        let keymap = Keymap::builtin();
        let state = State::default();
        let mut outbox = Vec::new();
        for (_, _, sections) in MENUS {
            let ctx = egui::Context::default();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                draw_sections(ui, sections, &keymap, &state, &mut outbox);
            });
            output.drop_without_applying_deltas();
        }
        assert!(outbox.is_empty(), "纯渲染不产生消息");
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            let ctx = egui::Context::default();
            ctx.set_theme(theme);
            let output = ctx.run_ui(RawInput::default(), |ui| {
                super::ui(ui, &keymap, &state, &mut outbox);
            });
            output.drop_without_applying_deltas();
            assert!(outbox.is_empty(), "菜单栏收起态渲染不产生消息({theme:?})");
        }
    }

    /// 新增菜单入口(格式 17 条 + 视图 3 条,以及既有全部条目)逐条点击
    /// 一次,发出的消息就是该命令的归约入口 —— 与快捷键触发
    /// (`command::poll_shortcuts` → `cmd.message()`)殊途同归。
    #[test]
    fn clicking_every_menu_item_sends_its_command_message() {
        for (_, _, sections) in MENUS {
            for cmd in sections.iter().flat_map(|s| s.iter().copied()) {
                let ctx = egui::Context::default();
                let mut outbox = Vec::new();
                let keymap = Keymap::builtin();
                let rect = Cell::new(Rect::NOTHING);
                ctx.run_ui(RawInput::default(), |ui| {
                    rect.set(item(ui, cmd, &keymap, None, &mut outbox).rect);
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
                        item(ui, cmd, &keymap, None, &mut outbox);
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
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            item(ui, Command::Save, &keymap, None, &mut outbox);
            item(ui, Command::GotoLine, &keymap, None, &mut outbox);
        });
        let expected_save = ctx.format_shortcut(&save_key);
        let expected_goto = ctx.format_shortcut(&goto_key);
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
        let old_text = expected_save.clone();
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            item(ui, Command::Save, &rebound, None, &mut outbox);
        });
        let expected_new = ctx.format_shortcut(&new_key);
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

    // —— #62 M2:Alt 助记键(冲突审计 + 触发行为 + 排版统一)——

    /// 全部菜单标题助记字母(含「设置」「帮助」),供冲突审计与显示断言共用。
    fn title_letters() -> Vec<char> {
        MENUS
            .iter()
            .map(|(_, letter, _)| *letter)
            .chain([SETTINGS_MNEMONIC, HELP_MNEMONIC])
            .collect()
    }

    /// **冲突审计(本模块核心红线)**:标题助记字母全局唯一,且逐一对照
    /// keymap 出厂表的全部 Alt 类绑定 —— 非 mac 上任何「modifiers 含 Alt」
    /// 的绑定,其主键都不得出现在标题字母集(当前占用者:Alt+T 主题
    /// #45、Ctrl+Alt+R 预览栏);mac 的 ⌥⌘F(替换,带 ⌘)按编译目标豁免
    /// ——mac 不启用助记层(决策 #119),且严格位匹配下纯 Alt+F 与
    /// ⌥⌘F 本就不是同一组合。遍历断言,非恒真:给「格式」分了 F、或将来
    /// 新命令绑了 Alt+F,这里红。
    #[test]
    fn title_mnemonics_unique_and_clear_of_keymap_alt_bindings() {
        let letters = title_letters();
        let mut sorted = letters.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            letters.len(),
            "标题助记字母全局唯一:{letters:?}"
        );

        let keymap = Keymap::builtin();
        for cmd in Command::ALL {
            let Some(shortcut) = keymap.get(cmd) else {
                continue;
            };
            // 纯 Alt 绑定(任何平台都与助记层同组合,必须避开);带 Ctrl/Cmd
            // 的 Alt 组合在非 mac 上也一并避开(保守口径,防将来把检测改宽)
            let clashes = shortcut.modifiers.alt
                && (!cfg!(target_os = "macos")
                    || shortcut.modifiers.matches_exact(egui::Modifiers::ALT));
            if !clashes {
                continue;
            }
            let key_letter = super::key_letter(shortcut.key);
            if let Some(letter) = key_letter {
                assert!(
                    !letters.contains(&letter),
                    "{} 绑定 {} 与菜单标题助记 {} 撞键",
                    cmd.id(),
                    shortcut.platform_text(),
                    letter
                );
            }
        }
    }

    /// 条目助记在**每个菜单内**唯一(跨菜单允许重复:裸字母按当前展开的
    /// 菜单局部解析);同时钉住「keymap 出厂表无裸字母绑定」—— 条目层
    /// 裸字母命名空间与快捷键命名空间零交集的前提(`bindable` 拒绝裸
    /// 字母是机制,这里是出厂表的事实断言)。
    #[test]
    fn item_mnemonics_unique_per_menu_and_keymap_has_no_bare_letters() {
        for (title, _, sections) in MENUS {
            let letters: Vec<char> = sections
                .iter()
                .flat_map(|section| section.iter().copied())
                .map(command_mnemonic)
                .collect();
            let mut sorted = letters.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                letters.len(),
                "「{title}」菜单内条目助记唯一:{letters:?}"
            );
        }
        let settings_letters: Vec<char> = SETTINGS_TAB_MNEMONICS
            .iter()
            .map(|(_, letter)| *letter)
            .collect();
        let mut sorted = settings_letters.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), settings_letters.len(), "「设置」菜单内唯一");

        for cmd in Command::ALL {
            let Some(shortcut) = Keymap::builtin().get(cmd) else {
                continue;
            };
            // 条目助记的命名空间是裸「字母/数字」(key_letter 能映射的键);
            // 无修饰功能键(F11 禅定)不在其中,与助记层零交集
            if shortcut.modifiers.is_none() && super::key_letter(shortcut.key).is_some() {
                panic!(
                    "{} 的出厂绑定 {} 是裸字母/数字键 —— 会与条目助记(展开态裸字母)互抢",
                    cmd.id(),
                    shortcut.platform_text()
                );
            }
        }
    }

    /// 助记后缀的平台口径(#62 M2):macOS 无 Alt 助记惯例(菜单助记符由
    /// 系统菜单层处理,自绘菜单不显示),不渲染后缀;其余平台渲染
    /// 「文件(F)」形态。按编译目标断言,两侧在各自平台 CI 上跑。
    #[test]
    fn mnemonic_suffix_per_platform() {
        if cfg!(target_os = "macos") {
            assert_eq!(
                label_with_mnemonic("文件", 'F'),
                "文件",
                "mac 不显示助记后缀"
            );
        } else {
            assert_eq!(label_with_mnemonic("文件", 'F'), "文件(F)");
        }
    }

    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn key_release(key: egui::Key, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }
    }

    /// 无头帧跑菜单栏并收集每个菜单的 popup id(`(栏内序号, id)`,
    /// 序号 0-5 对应 [`MENUS`],6 = 设置,7 = 帮助)。返回 popup id 表供
    /// 开合断言。
    fn menubar_frame(ctx: &egui::Context, events: Vec<Event>) -> Vec<(usize, egui::Id)> {
        let keymap = Keymap::builtin();
        let state = State::default();
        let mut outbox = Vec::new();
        let mut ids = Vec::new();
        let output = ctx.run_ui(
            RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(
                    ui,
                    &keymap,
                    &state,
                    &mut outbox,
                    Some(|index, id| {
                        ids.push((index, id));
                    }),
                );
            },
        );
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty(), "开合菜单本身不产生命令消息:{outbox:?}");
        ids
    }

    /// Alt+F 打开「文件」菜单(标题层助记的行为面):事件帧 + 展开帧之后,
    /// 文件菜单的 popup 在 egui memory 里是开的,其余菜单都没开。
    /// Alt+T(主题键 #45)不开任何菜单 —— 字母避让审计的行为面。
    #[test]
    fn alt_letter_opens_matching_menu_only() {
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![key_event(egui::Key::F, egui::Modifiers::ALT)]);
        menubar_frame(&ctx, vec![]);
        let file_id = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        assert_eq!(
            egui::containers::Popup::is_id_open(&ctx, file_id),
            !cfg!(target_os = "macos"),
            "Alt+F 仅在非 macOS 平台打开菜单"
        );
        for (index, id) in &ids {
            if *index != 0 {
                assert!(
                    !egui::containers::Popup::is_id_open(&ctx, *id),
                    "Alt+F 不该顺带打开第 {index} 个菜单"
                );
            }
        }

        // Alt+T 是主题切换的出厂键(#45 K1),不分配给任何菜单 —— 按下后
        // 没有菜单被打开(它仍触发主题命令这件事由 command.rs 的
        // `alt_t_fires_only_toggle_theme` 钉住,此处只审菜单侧)。
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![key_event(egui::Key::T, egui::Modifiers::ALT)]);
        menubar_frame(&ctx, vec![]);
        for (_, id) in &ids {
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, *id),
                "Alt+T 不该打开任何菜单"
            );
        }

        // 严格匹配的对照:Ctrl+Alt+R(切换预览栏)带 Ctrl,不是纯 Alt,
        // 不触发助记 —— 检测层用位相等而非 matches_logically 的原因。
        let ctx = egui::Context::default();
        let ids = menubar_frame(
            &ctx,
            vec![key_event(
                egui::Key::R,
                egui::Modifiers::COMMAND | egui::Modifiers::ALT,
            )],
        );
        menubar_frame(&ctx, vec![]);
        for (_, id) in &ids {
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, *id),
                "Ctrl+Alt+R 是命令快捷键,不该触发菜单助记"
            );
        }
    }

    /// 基础交互核验(#62 M2):Alt 打开的菜单,Esc 关闭、点击菜单外区域
    /// 关闭 —— 两者都是 egui 0.36 popup 的内建行为(`popup.rs` 的
    /// `key_pressed(Escape)` 与 `close_behavior` 点击分支),这里无头复测
    /// 走的是本仓菜单条的完整路径,守「将来升级 egui 时开合语义悄悄变」。
    /// 悬停切换菜单的现状也在此核验:egui 顶层 `MenuButton` 只在点击时
    /// toggle(menu.rs 实读),悬停另一个标题不切换 —— 无头断言钉住现状,
    /// 将来 egui 改了行为或本仓想补 Windows 式悬停切换,这条测试是落点。
    #[test]
    fn open_menu_closes_on_escape_and_outside_click() {
        // Esc 关闭
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![]);
        let file = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        egui::containers::Popup::open_id(&ctx, file);
        menubar_frame(&ctx, vec![]);
        let file_id = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        assert!(
            egui::containers::Popup::is_id_open(&ctx, file_id),
            "前置:菜单已开"
        );
        menubar_frame(
            &ctx,
            vec![key_event(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        menubar_frame(&ctx, vec![]);
        assert!(
            !egui::containers::Popup::is_id_open(&ctx, file_id),
            "Esc 应关闭打开的菜单"
        );

        // 点击菜单外区域关闭
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![]);
        let file = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        egui::containers::Popup::open_id(&ctx, file);
        menubar_frame(&ctx, vec![]);
        let file_id = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        assert!(
            egui::containers::Popup::is_id_open(&ctx, file_id),
            "前置:菜单已开"
        );
        let far = egui::Pos2::new(640.0, 400.0);
        let click = |pressed| Event::PointerButton {
            pos: far,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        menubar_frame(
            &ctx,
            vec![Event::PointerMoved(far), click(true), click(false)],
        );
        menubar_frame(&ctx, vec![]);
        assert!(
            !egui::containers::Popup::is_id_open(&ctx, file_id),
            "点击菜单外应关闭菜单"
        );

        // 悬停切换现状:菜单开着,指针移到另一标题上悬停 —— 顶层 MenuButton
        // 只点击切换,悬停不开新菜单、原菜单保持(现状即如此,核验记录)
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![]);
        let file = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        egui::containers::Popup::open_id(&ctx, file);
        menubar_frame(&ctx, vec![]);
        let (file_id, edit_id) = (
            ids.iter().find(|(index, _)| *index == 0).unwrap().1,
            ids.iter().find(|(index, _)| *index == 1).unwrap().1,
        );
        assert!(
            egui::containers::Popup::is_id_open(&ctx, file_id),
            "前置:文件菜单开"
        );
        let edit_title = egui::Pos2::new(120.0, 12.0);
        menubar_frame(
            &ctx,
            vec![
                Event::PointerMoved(edit_title),
                Event::PointerMoved(edit_title),
            ],
        );
        menubar_frame(&ctx, vec![]);
        assert!(
            egui::containers::Popup::is_id_open(&ctx, file_id)
                && !egui::containers::Popup::is_id_open(&ctx, edit_id),
            "悬停另一标题不切换菜单(egui 顶层 MenuButton 现状:仅点击切换)"
        );
    }

    /// #69 切窗误触回归:LaterMD 内按下 Alt 武装后切走(release 被 WM/他窗
    /// 吃掉)、再从别处 Ctrl+Alt 切回时收到的孤立 Alt release,不得触发
    /// 单击聚焦 —— 失焦帧必须缴械。修复前 armed 跨会话遗留,孤立 release
    /// 炸开「文件」菜单(坤哥 2026-10-08 报的实测症状:Ctrl+Alt 切窗老触
    /// 发文件菜单)。
    #[test]
    fn alt_release_after_refocus_does_not_fire_stale_armed() {
        let ctx = egui::Context::default();
        let ids = menubar_frame(
            &ctx,
            vec![key_event(egui::Key::AltLeft, egui::Modifiers::NONE)],
        );
        // 失焦:release 没有回到本窗口(被 WM/目标窗口吃掉)
        menubar_frame(&ctx, vec![Event::WindowFocused(false)]);
        // 从别的应用 Ctrl+Alt 切回:聚焦 + 孤立 Alt release(无对应 press)
        menubar_frame(&ctx, vec![Event::WindowFocused(true)]);
        menubar_frame(
            &ctx,
            vec![key_release(egui::Key::AltLeft, egui::Modifiers::ALT)],
        );
        menubar_frame(&ctx, vec![]);
        for (index, id) in &ids {
            let _ = index;
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, *id),
                "遗留 armed + 孤立 Alt release 不触发菜单聚焦"
            );
        }

        // 变体:未武装状态下失焦再聚焦,孤立 release 同样不触发(修复前
        // 本就不触发,钉住防将来把 release 分支改坏)
        let ctx = egui::Context::default();
        let ids = menubar_frame(&ctx, vec![Event::WindowFocused(false)]);
        menubar_frame(&ctx, vec![Event::WindowFocused(true)]);
        menubar_frame(
            &ctx,
            vec![key_release(egui::Key::AltLeft, egui::Modifiers::ALT)],
        );
        menubar_frame(&ctx, vec![]);
        for (index, id) in &ids {
            let _ = index;
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, *id),
                "未武装的孤立 Alt release 不触发"
            );
        }
    }

    /// 单击 Alt(按下→抬起,中间无其他输入)聚焦菜单栏:打开首个菜单
    /// 「文件」;Alt+字母的组合(按下→字母按下→抬起)不触发 —— 字母
    /// 按下帧污染武装标志。
    #[test]
    fn single_alt_click_opens_first_menu() {
        let ctx = egui::Context::default();
        let ids = menubar_frame(
            &ctx,
            vec![key_event(egui::Key::AltLeft, egui::Modifiers::NONE)],
        );
        let ids2 = menubar_frame(
            &ctx,
            vec![key_release(egui::Key::AltLeft, egui::Modifiers::ALT)],
        );
        assert_eq!(ids, ids2, "两帧的 popup id 稳定");
        menubar_frame(&ctx, vec![]);
        let file_id = ids.iter().find(|(index, _)| *index == 0).unwrap().1;
        assert_eq!(
            egui::containers::Popup::is_id_open(&ctx, file_id),
            !cfg!(target_os = "macos"),
            "单击 Alt 仅在非 macOS 平台聚焦菜单"
        );

        // 对照:Alt+字母组合不触发单击语义(那由 Alt+字母助记负责)
        let ctx = egui::Context::default();
        menubar_frame(
            &ctx,
            vec![key_event(egui::Key::AltLeft, egui::Modifiers::NONE)],
        );
        menubar_frame(
            &ctx,
            vec![
                key_event(egui::Key::T, egui::Modifiers::ALT),
                key_release(egui::Key::AltLeft, egui::Modifiers::ALT),
            ],
        );
        menubar_frame(&ctx, vec![]);
        for (index, id) in &ids {
            let _ = index;
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, *id),
                "Alt+T 组合的抬起不该触发单击聚焦"
            );
        }
    }

    /// 菜单展开态的裸字母触发条目(条目层助记):Alt+E 开「编辑」→
    /// 展开帧按 F → 发出「查找」的归约消息,菜单收起;同字母的 Key 与
    /// Text 事件都被消费(不落进聚焦的编辑器)。
    #[test]
    fn bare_letter_in_open_menu_fires_item_and_closes() {
        let ctx = egui::Context::default();
        let keymap = Keymap::builtin();
        let state = State::default();
        let mut outbox = Vec::new();
        let mut ids = Vec::new();
        // 帧 1:Alt+E 开「编辑」
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(egui::Key::E, egui::Modifiers::ALT)],
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(
                    ui,
                    &keymap,
                    &state,
                    &mut outbox,
                    Some(|index, id| {
                        ids.push((index, id));
                    }),
                );
            },
        );
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty());
        if cfg!(target_os = "macos") {
            let edit_id = ids.iter().find(|(index, _)| *index == 1).unwrap().1;
            egui::containers::Popup::open_id(&ctx, edit_id);
        }
        // 帧 2:展开帧(闭包执行,无输入)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui_with_probe(ui, &keymap, &state, &mut outbox, Some(|_, _| {}));
        });
        output.drop_without_applying_deltas();
        // 帧 3:裸 F → 命中「查找」(F),发消息并收起
        let output = ctx.run_ui(
            RawInput {
                events: vec![
                    key_event(egui::Key::F, egui::Modifiers::NONE),
                    Event::Text("f".to_owned()),
                ],
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(ui, &keymap, &state, &mut outbox, Some(|_, _| {}));
            },
        );
        output.drop_without_applying_deltas();
        if cfg!(target_os = "macos") {
            assert!(outbox.is_empty(), "macOS 裸字母不触发菜单命令");
            let edit_id = ids.iter().find(|(index, _)| *index == 1).unwrap().1;
            assert!(egui::containers::Popup::is_id_open(&ctx, edit_id));
            return;
        }
        assert_eq!(
            outbox,
            vec![Message::FindBarToggled(true)],
            "展开的「编辑」菜单里按 F 应触发「查找」"
        );
        let edit_id = ids.iter().find(|(index, _)| *index == 1).unwrap().1;
        // 帧 4:菜单已收起(close 标记在下一帧生效为 popup 关闭)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui_with_probe(ui, &keymap, &state, &mut Vec::new(), Some(|_, _| {}));
        });
        output.drop_without_applying_deltas();
        assert!(
            !egui::containers::Popup::is_id_open(&ctx, edit_id),
            "条目触发后菜单应收起"
        );
    }

    /// 「设置」菜单直达页的真实点击路径(#67 M1 抽查):「设置」不在
    /// [`MENUS`] 表里(非命令,直达设置页),`clicking_every_menu_item…`
    /// 不遍历它 —— 这里走完整 popup 路径:Alt+S 打开 → 展开帧定位条目
    /// 文本矩形 → 指针逐帧点击 → 发出 [`Message::SettingsOpened`] 的
    /// 对应页;消息再喂进 `State::apply` 落出打开的页签(点击 → 消息 →
    /// 归约一段齐,齿轮路径的等价归约断言在 layout.rs 已有)。
    #[test]
    fn settings_menu_direct_tabs_click_through_and_reduce() {
        for (needle, expected) in [
            ("外观", SettingsTab::Appearance),
            ("快捷键", SettingsTab::Keymap),
        ] {
            let ctx = egui::Context::default();
            // 显式打开 popup，让点击测试在没有 Alt 助记层的 macOS 也执行。
            let keymap = Keymap::builtin();
            let state = State::default();
            let mut outbox = Vec::new();
            let ids = menubar_frame(&ctx, vec![]);
            let settings_id = ids
                .iter()
                .find(|(index, _)| *index == MENUS.len())
                .unwrap()
                .1;
            egui::containers::Popup::open_id(&ctx, settings_id);

            // 帧 2-3:两次空帧 —— egui 0.36 的 MenuButton 从 popup 记忆
            // 开态到闭包真正绘制隔一帧(事件帧写记忆 → 次帧菜单按钮收
            // 到开态 → 再次帧闭包执行画出条目;与 `bare_letter…` 三帧
            // 节奏同因,这里实证后取末帧 shapes 定位条目矩形)
            let mut located = None;
            for _ in 0..2 {
                let mut outbox = Vec::new();
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    super::ui(ui, &keymap, &state, &mut outbox);
                });
                let shapes = output.shapes.clone();
                output.drop_without_applying_deltas();
                assert!(outbox.is_empty(), "展开不点击不产生消息");
                located = shapes.iter().find_map(|clipped| {
                    let egui::epaint::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    text.galley
                        .job
                        .text
                        .contains(needle)
                        .then(|| clipped.shape.visual_bounding_rect())
                });
                if located.is_some() {
                    break;
                }
            }
            let rect = located.unwrap_or_else(|| panic!("{needle:?} 条目未绘制"));

            // 帧 4-6:moved → press → release(与 layout 的面板层点击测试
            // 同节奏,指针停在条目中心)
            let center = rect.center();
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
                let mut frame_outbox = std::mem::take(&mut outbox);
                let output = ctx.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| super::ui(ui, &keymap, &state, &mut frame_outbox),
                );
                output.drop_without_applying_deltas();
                outbox = frame_outbox;
            }
            assert_eq!(
                outbox,
                vec![Message::SettingsOpened(expected)],
                "{needle:?} 条目点击应发出直达该页的消息"
            );

            // 归约一段:直达页消息落 State,设置窗开在对应页签
            let mut state = crate::state::State::default();
            state.apply(Message::SettingsOpened(expected));
            assert!(state.settings.open, "归约后设置窗打开");
            assert_eq!(state.settings.tab, expected, "归约后落在直达页签");
        }
    }

    /// 「帮助」菜单(#71 M1)的完整路径:打开 popup → 真实点击
    /// 「关于 LaterMD…」条目 → 发出 [`Message::AboutOpened`] → 归约后
    /// `about.open` 为真(点击 → 消息 → 归约一段齐;帮助非命令菜单,
    /// `clicking_every_menu_item…` 不遍历它,这里补齐真实 popup 路径)。
    #[test]
    fn help_menu_about_entry_click_through_and_reduce() {
        let ctx = egui::Context::default();
        let keymap = Keymap::builtin();
        let state = State::default();
        let mut outbox = Vec::new();
        // 点击测试显式展开 popup，与设置菜单测试一致，不依赖 Alt 助记键。
        let ids = menubar_frame(&ctx, vec![]);
        let help_id = ids
            .iter()
            .find(|(index, _)| *index == MENUS.len() + 1)
            .unwrap()
            .1;
        egui::containers::Popup::open_id(&ctx, help_id);

        // 帧 2-3:两次空帧 —— MenuButton 从 popup 记忆开态到闭包画出条目
        // 隔一帧(设置菜单同款节奏),取末帧 shapes 定位条目矩形
        let mut located = None;
        for _ in 0..2 {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                super::ui(ui, &keymap, &state, &mut outbox);
            });
            let shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
            assert!(outbox.is_empty(), "展开不点击不产生消息");
            located = shapes.iter().find_map(|clipped| {
                let egui::epaint::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                text.galley
                    .job
                    .text
                    .contains("关于 LaterMD…")
                    .then(|| clipped.shape.visual_bounding_rect())
            });
            if located.is_some() {
                break;
            }
        }
        let rect = located.expect("「关于 LaterMD…」条目未绘制");

        // 帧 4-6:moved → press → release,点击条目中心
        let center = rect.center();
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
            let output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| super::ui(ui, &keymap, &state, &mut outbox),
            );
            output.drop_without_applying_deltas();
        }
        assert_eq!(
            outbox,
            vec![Message::AboutOpened],
            "「关于 LaterMD…」条目点击应发出开窗消息"
        );

        // 归约一段:消息落 State,关于窗开
        let mut state = crate::state::State::default();
        state.apply(Message::AboutOpened);
        assert!(state.about.open, "归约后关于窗打开");
    }

    /// 帮助菜单展开态的裸字母触发(条目层助记):Alt+H 开「帮助」→
    /// 展开帧按 A → 发出 [`Message::AboutOpened`],菜单收起。
    #[test]
    fn bare_letter_in_open_help_menu_fires_about() {
        let ctx = egui::Context::default();
        let keymap = Keymap::builtin();
        let state = State::default();
        let mut outbox = Vec::new();
        let mut ids = Vec::new();
        // 帧 1:Alt+H 开「帮助」
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_event(egui::Key::H, egui::Modifiers::ALT)],
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(
                    ui,
                    &keymap,
                    &state,
                    &mut outbox,
                    Some(|index, id| {
                        ids.push((index, id));
                    }),
                );
            },
        );
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty());
        let help_id = ids
            .iter()
            .find(|(index, _)| *index == MENUS.len() + 1)
            .unwrap()
            .1;
        if cfg!(target_os = "macos") {
            assert!(
                !egui::containers::Popup::is_id_open(&ctx, help_id),
                "macOS Alt+H 不打开帮助菜单"
            );
            egui::containers::Popup::open_id(&ctx, help_id);
        }
        // 帧 2:展开帧(闭包执行,无输入)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui_with_probe(ui, &keymap, &state, &mut outbox, Some(|_, _| {}));
        });
        output.drop_without_applying_deltas();
        // 帧 3:裸 A → 命中「关于 LaterMD…」,发消息并收起
        let output = ctx.run_ui(
            RawInput {
                events: vec![
                    key_event(egui::Key::A, egui::Modifiers::NONE),
                    Event::Text("a".to_owned()),
                ],
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(ui, &keymap, &state, &mut outbox, Some(|_, _| {}));
            },
        );
        output.drop_without_applying_deltas();
        if cfg!(target_os = "macos") {
            assert!(outbox.is_empty(), "macOS 裸字母不触发帮助菜单命令");
            assert!(egui::containers::Popup::is_id_open(&ctx, help_id));
            return;
        }
        assert_eq!(
            outbox,
            vec![Message::AboutOpened],
            "展开的「帮助」菜单里按 A 应触发「关于 LaterMD…」"
        );
        // 帧 4:菜单已收起(close 标记在下一帧生效为 popup 关闭)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui_with_probe(ui, &keymap, &state, &mut Vec::new(), Some(|_, _| {}));
        });
        output.drop_without_applying_deltas();
        assert!(
            !egui::containers::Popup::is_id_open(&ctx, help_id),
            "条目触发后菜单应收起"
        );
    }

    /// 开关类菜单条目的勾选态(#67 M2):`toggle_checked` 从 [`State`] 取
    /// 真值(与设置页/标题栏按钮同一事实源),布偶开关命令全覆盖、非开关
    /// 命令(含明暗互换的「切换主题」)`None`;勾选前缀只在开态渲染 ✓,
    /// 关态为占位空格、非开关条目无前缀。
    #[test]
    fn view_menu_toggle_items_show_check_state() {
        // 真值层:出厂默认下各开关的当前值(与 ThemeSettings/LayoutSettings
        // 的出厂字段一致)
        let state = State::default();
        assert_eq!(toggle_checked(Command::ToggleSidebar, &state), Some(true));
        assert_eq!(
            toggle_checked(Command::ToggleRightPreview, &state),
            Some(true)
        );
        assert_eq!(
            toggle_checked(Command::ToggleLivePreview, &state),
            Some(false),
            "出厂源码模式,Live 未开"
        );
        assert_eq!(
            toggle_checked(Command::TypewriterToggle, &state),
            Some(false)
        );
        assert_eq!(
            toggle_checked(Command::FocusModeToggle, &state),
            Some(false)
        );
        assert_eq!(toggle_checked(Command::ToggleMinimap, &state), Some(true));
        assert_eq!(toggle_checked(Command::ToggleZen, &state), Some(false));
        // 明暗互换不是开/关,主题条目无勾选语义;非视图命令同样 None
        assert_eq!(toggle_checked(Command::ToggleTheme, &state), None);
        assert_eq!(toggle_checked(Command::Save, &state), None);

        // 勾选态随状态翻转:minimap 关掉后,同一命令取到 Some(false)
        let mut state = state;
        state.apply(Message::ToggleMinimap);
        assert_eq!(
            toggle_checked(Command::ToggleMinimap, &state),
            Some(false),
            "归约翻转后勾选态立即跟随(三方一致的菜单侧)"
        );

        // 渲染层:✓ 前缀只出现在 Some(true) 的条目文本里
        fn item_text(cmd: Command, checked: Option<bool>) -> String {
            let ctx = egui::Context::default();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                item(ui, cmd, &Keymap::builtin(), checked, &mut Vec::new());
            });
            let text = output
                .shapes
                .iter()
                .filter_map(|clipped| {
                    let egui::epaint::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    Some(text.galley.job.text.clone())
                })
                .collect::<Vec<_>>()
                .join("\n");
            output.drop_without_applying_deltas();
            text
        }
        let on = item_text(Command::ToggleMinimap, Some(true));
        assert!(
            on.contains("✓") && on.contains(&label_with_mnemonic("Minimap 缩略图", 'M')),
            "开态条目应渲染 ✓ 前缀 + 带助记后缀的显示名(实际:{on:?})"
        );
        let off = item_text(Command::ToggleMinimap, Some(false));
        assert!(
            !off.contains("✓") && off.contains(&label_with_mnemonic("Minimap 缩略图", 'M')),
            "关态条目不渲染 ✓(占位空格保持开关条目对齐;实际:{off:?})"
        );
        let plain = item_text(Command::ToggleTheme, None);
        assert!(!plain.contains("✓"), "非开关条目无勾选前缀(实际:{plain:?})");
    }

    /// minimap 菜单开关的**全链路**(#67 M2):菜单条目真实点击 → 发出
    /// 翻转消息 → `State::apply` 翻转 `theme.show_minimap` → settings.json
    /// 即时落盘(重启 load_from 仍在)。勾选态取值与翻转后的字段一致 ——
    /// 菜单、快捷键、设置外观页复选框三方共用同一字段同一归约。
    #[test]
    fn minimap_menu_item_click_flips_theme_and_persists() {
        let dir =
            std::env::temp_dir().join(format!("latermd-menubar-minimap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut state = State::default();
        state.settings_dir = Some(dir.clone());
        assert!(state.theme.show_minimap, "出厂默认开");
        assert_eq!(toggle_checked(Command::ToggleMinimap, &state), Some(true));

        // 两帧点击:渲染帧拿矩形,点击帧按下并抬起
        let ctx = egui::Context::default();
        let keymap = Keymap::builtin();
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(
                item(
                    ui,
                    Command::ToggleMinimap,
                    &keymap,
                    toggle_checked(Command::ToggleMinimap, &state),
                    &mut outbox,
                )
                .rect,
            );
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");
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
                item(
                    ui,
                    Command::ToggleMinimap,
                    &keymap,
                    toggle_checked(Command::ToggleMinimap, &state),
                    &mut outbox,
                );
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::ToggleMinimap], "菜单点击发出翻转消息");

        // 归约 + 落盘:字段翻转、勾选态跟随、settings.json 重启仍在
        state.apply(Message::ToggleMinimap);
        assert!(!state.theme.show_minimap, "菜单开关翻转字段");
        assert_eq!(
            toggle_checked(Command::ToggleMinimap, &state),
            Some(false),
            "翻转后勾选态一致"
        );
        let reloaded = crate::theme::ThemeSettings::load_from(&dir).unwrap();
        assert!(!reloaded.show_minimap, "菜单开关即时落盘,重启后仍为关");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 排版统一(#62 M2):菜单条标题与下拉条目同走 egui `Button` 的
    /// `TextStyle::Button` 档 —— 两处 galley 字号必须一致(守门将来一侧
    /// 改字号另一侧没跟);收起态菜单条本身在窄窗口(400px)渲染不
    /// panic(溢出策略=单行裁切,见 decisions-pending #119)。
    #[test]
    fn menubar_and_item_share_font_size_and_narrow_render_is_safe() {
        fn font_sizes(output: &FullOutput) -> Vec<(String, f32)> {
            output
                .shapes
                .iter()
                .filter_map(|clipped| {
                    let egui::epaint::Shape::Text(text) = &clipped.shape else {
                        return None;
                    };
                    text.galley
                        .job
                        .sections
                        .first()
                        .map(|section| (text.galley.job.text.clone(), section.format.font_id.size))
                })
                .collect()
        }

        let keymap = Keymap::builtin();
        let state = State::default();
        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            super::ui(ui, &keymap, &state, &mut Vec::new());
        });
        let bar_texts = font_sizes(&output);
        output.drop_without_applying_deltas();
        let title = bar_texts
            .iter()
            .find(|(text, _)| text.starts_with(&label_with_mnemonic("文件", 'F')))
            .expect("菜单条渲染出带助记后缀的标题");
        let bar_size = title.1;

        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            item(ui, Command::Save, &keymap, None, &mut Vec::new());
        });
        let item_texts = font_sizes(&output);
        output.drop_without_applying_deltas();
        let entry = item_texts
            .iter()
            .find(|(text, _)| text.starts_with(&label_with_mnemonic("保存", 'S')))
            .expect("菜单条目渲染出带助记后缀的显示名");
        assert_eq!(
            bar_size, entry.1,
            "菜单条标题与下拉条目字号一致(排版统一,当前 Button 档)"
        );

        // 窄窗口渲染不 panic(单行裁切现状的守门面)
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 300.0),
                )),
                ..Default::default()
            },
            |ui| {
                super::ui(ui, &keymap, &state, &mut Vec::new());
            },
        );
        output.drop_without_applying_deltas();
    }
}
