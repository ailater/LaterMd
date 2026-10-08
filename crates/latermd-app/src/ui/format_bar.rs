//! Markdown 格式工具条(docs/ui-shell-redesign.md §6)。
//!
//! 十七个动作分四组:行内 / 标题 / 块 / 列表。点击只发
//! [`Message::FormatRequested`],语义全在 [`crate::compose`] —— 本模块
//! 一行 Markdown 逻辑都没有。
//!
//! 唯一的例外是**图片**:它要 alt 与 url 两个输入,点了是开「图片框」对话
//! 框(发 [`Message::ImageDialogOpened`]),不是改文档(docs/image-plan.md
//! A 段)。
//!
//! ## 形态选择
//!
//! 加粗/斜体/删除线/H1-H3 用 **RichText**(`B` / `I` / `S` / `H1` 等)
//! 而不是自绘图标:这三个是「形态」抽象概念,线段自绘只能画出没有辨识度
//! 的矩形;而 `RichText::italics()/strikethrough()` 与 Inter SemiBold
//! 字重(U1,`fonts::semibold_family`)是 egui 内建富文本能力,零字形
//! 依赖风险(§6.1)。
//! 其余用自绘线段图标,遵守 ui-polish §1.1「图标是矢量自绘,不是字体字符」。
//!
//! ## 为什么要单独一条
//!
//! 文件动作收在左栏顶段(`ui::sidebar` 的 top_actions),本条是「对**选
//! 区**做文本级动作」,与文件级动作分居两处,心智模型不同;单独一条也让
//! 十七个按钮有完整的横向空间,不与谁挤。

use crate::command::Command;
use crate::compose::{FormatAction, FormatGroup};
use crate::keymap::Keymap;
use crate::state::Message;
use crate::ui::icons::{self, Icon};
use crate::ui::tokens;
use eframe::egui;

/// 绘制整条格式工具条。键位文案取自 `keymap`(用户可改)。
///
/// 面板拖窄时逐组换行(`horizontal_wrapped`)而不是溢出裁切 —— 中间栏最窄
/// 可到 400px 左右,十七个按钮不可能一行排完。
pub fn ui(panel: &mut egui::Ui, keymap: &Keymap, outbox: &mut Vec<Message>) {
    ui_with_probe(
        panel,
        keymap,
        outbox,
        None::<fn(FormatAction, egui::Rect)>,
        None::<fn(egui::Rect)>,
    )
}

/// 同 [`ui`],额外把每个按钮的 `(动作, 矩形)` 交给 `probe`(`None` 即不探针)。
///
/// 无头测试量按钮位置用:十七个按钮的具体坐标由 `horizontal_wrapped` 的换
/// 行演算 + 它前面的标签条/提示行共同决定,手搓必然与真实帧错位。
/// `emoji_probe` 同理,量的是末尾那枚 Emoji 入口(不是 FormatAction,进
/// 不了 `probe` 的载荷)。
/// 点了某个格式动作按钮该发什么消息。
///
/// **图片是唯一「点了不改文档」的动作**:它要 alt 与 url 两个输入,点了
/// 是开「图片框」对话框(发 `ImageDialogOpened`),真正的写入走
/// `compose::insert_image`(docs/image-plan.md A 段)。
///
/// 直出按钮与溢出菜单里的条目**共用这一个函数** —— 两处若各写一份,
/// 迟早会漂移出「直出的图片发格式请求、菜单里的图片发对话框」这种
/// 方向相反的 bug(2026-10-08 S2-2 引入溢出菜单时的显式约束)。
fn request(action: FormatAction, outbox: &mut Vec<Message>) {
    outbox.push(if action == FormatAction::Image {
        Message::ImageDialogOpened
    } else {
        Message::FormatRequested(action)
    });
}

pub fn ui_with_probe(
    panel: &mut egui::Ui,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
    probe: Option<impl FnMut(FormatAction, egui::Rect)>,
    emoji_probe: Option<impl FnMut(egui::Rect)>,
) {
    let mut probe = probe;
    let mut emoji_probe = emoji_probe;
    // 「直出 + 溢出」必须**恰好**覆盖全部四组,不多不少、无重复。
    // 这条断言拦住的是「将来给 `FormatGroup` 加了第五组,却忘了放进
    // DIRECT 或 OVERFLOW」—— 那会让第五组**静默地从工具条消失**:
    // 不报编译错、菜单栏照常有它、只有常驻按钮不见了。
    // `debug_assert!` 而非 `assert!`:一帧一次的 UI 绘制路径不该付
    // 运行时检查的钱,而 debug 构建(测试与开发)已经覆盖。
    debug_assert_eq!(
        FormatGroup::DIRECT.len() + FormatGroup::OVERFLOW.len(),
        FormatGroup::ALL.len(),
        "DIRECT + OVERFLOW 必须恰好覆盖 FormatGroup::ALL(防新增组被遗忘)"
    );
    debug_assert!(
        FormatGroup::DIRECT
            .iter()
            .chain(FormatGroup::OVERFLOW.iter())
            .all(|g| FormatGroup::ALL.contains(g)),
        "DIRECT / OVERFLOW 里的组必须都出自 ALL"
    );
    panel.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;

        // ① 直出组(2026-10-08 S2-2:只有行内五项,理由见
        // `FormatGroup::DIRECT`)。
        for group in FormatGroup::DIRECT {
            for action in group.actions() {
                let response = button(ui, *action, keymap);
                if let Some(probe) = probe.as_mut() {
                    probe(*action, response.rect);
                }
                if response.clicked() {
                    request(*action, outbox);
                }
            }
        }

        ui.separator();

        // ② 溢出菜单:标题 / 块 / 列表三组十二项(低频,按语境才用)。
        //
        // 菜单项**走同一个 `button()`**,故 tooltip 口径、图标画法、
        // 点击语义与直出完全一致;分段之间画分隔线(与菜单栏同款)。
        //
        // 「探针在菜单内也生效」是刻意的:无头测试据此在菜单打开后拿到条目
        // 矩形,否则菜单里的动作**没有任何测试能定位** —— 等于格式动作
        // 的点击路径在溢出后就失去覆盖。
        ui.menu_button("更多", |ui| {
            for (index, group) in FormatGroup::OVERFLOW.iter().enumerate() {
                if index > 0 {
                    ui.separator();
                }
                for action in group.actions() {
                    let response = button(ui, *action, keymap);
                    if let Some(probe) = probe.as_mut() {
                        probe(*action, response.rect);
                    }
                    if response.clicked() {
                        request(*action, outbox);
                    }
                }
            }
        });

        // ③ Emoji 面板入口(docs/emoji-plan.md E1):第二个「对话框类动作」,
        // 与 Image 同款 —— 点了只开面板,不进 `FormatAction` 四组(emoji
        // 字符无法从 text+sel 推导,§6.1 的边界)。它留在直出位:面板是
        // 「插入一个字符」的高频动作,与低频的段落级格式不同层。
        ui.separator();
        let response = icons::icon_button(ui, Icon::Emoji, &emoji_tooltip(keymap));
        if let Some(probe) = emoji_probe.as_mut() {
            probe(response.rect);
        }
        if response.clicked() {
            outbox.push(Message::EmojiPickerToggle(true));
        }
    });
}

/// Emoji 按钮 tooltip(带当前键位,与动作按钮的 tooltip 同款口径)。
fn emoji_tooltip(keymap: &Keymap) -> String {
    match keymap.get(Command::EmojiPicker) {
        Some(shortcut) => format!("插入 Emoji({})", shortcut.platform_text()),
        None => "插入 Emoji".to_owned(),
    }
}

/// 单个按钮。返回响应以便测试定位(与 `ui::menubar::item` 同款手法)。
fn button(ui: &mut egui::Ui, action: FormatAction, keymap: &Keymap) -> egui::Response {
    match action {
        // 形态类走富文本:B / H1-H3 是抽象概念的自解释字形,字重取 Inter
        // SemiBold(U1,真实字重替代 strong() 的人造粗);I / S 叠加斜体 /
        // 删除线。其余一律自绘线段图标。
        FormatAction::Bold => rich(ui, action, "B", Glyph::Weight),
        FormatAction::Italic => rich(ui, action, "I", Glyph::Italic),
        FormatAction::Strike => rich(ui, action, "S", Glyph::Strike),
        FormatAction::H1 => rich(ui, action, "H1", Glyph::Weight),
        FormatAction::H2 => rich(ui, action, "H2", Glyph::Weight),
        FormatAction::H3 => rich(ui, action, "H3", Glyph::Weight),
        _ => icon_button(ui, action, keymap),
    }
}

/// 形态按钮的附加富文本形态(SemiBold 字重是 `rich` 内统一施加的底座)。
enum Glyph {
    /// 无附加形态:视觉重量全靠 SemiBold 字形。
    Weight,
    Italic,
    Strike,
}

/// 富文本按钮:`B` / `I` / `S` / `H1` 等。tooltip 带当前键位 —— 形态字形
/// 本身不解释自己,靠 tooltip 兜可读性(ui-polish §1.1 的同款要求)。
fn rich(ui: &mut egui::Ui, action: FormatAction, glyph: &str, shape: Glyph) -> egui::Response {
    // 原先取 `interact_size.y`(egui 出厂 18)当宽度;U0 投影后它涨到
    // INPUT_H=36,六个形态按钮共宽 108px,把 Task 挤过 horizontal_wrapped
    // 的换行点(实测按钮挪到第二行,task_button_cycling 测试点击落空)。
    // 形态按钮的宽度是**字形的排版需求**(ICON+8),不是「可交互最小高度」,
    // 后者只该作用于按钮高度 —— 高度这排恒取 FORMAT_BAR_H,本就与
    // interact_size 无关。改成只按排版宽度取值,U0 的高度投影不再外溢成
    // 宽度副作用。
    let width = tokens::ICON + 8.0;
    let size = egui::vec2(width, tokens::FORMAT_BAR_H);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let enabled = ui.is_enabled();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() && enabled {
            painter.rect_filled(
                rect,
                tokens::RADIUS_SM,
                ui.visuals().widgets.hovered.bg_fill,
            );
        }
        // 走 `WidgetText::into_galley` 而不是 `painter.text`:后者签名的
        // `impl ToString` 会把 RichText 降级成纯字符串,字重 / 斜体 /
        // 删除线在这一个转换里全丢 —— 加粗按钮于是长得跟普通按钮没两
        // 样,删除线干脆看不见。
        let mut text = egui::RichText::new(glyph)
            .size(tokens::ICON_SM)
            .family(crate::fonts::semibold_family(ui.ctx()));
        match shape {
            Glyph::Weight => {}
            Glyph::Italic => text = text.italics(),
            Glyph::Strike => text = text.strikethrough(),
        }
        if !enabled {
            text = text.color(ui.visuals().weak_text_color());
        }
        let galley = egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Body,
        );
        let rect = egui::Align2::CENTER_CENTER.anchor_size(rect.center(), galley.size());
        painter.galley(rect.min, galley, ui.visuals().text_color());
    }
    response.on_hover_text(command_tooltip(action))
}

/// 自绘图标按钮(形态抽象概念以外的动作)。tooltip 带当前键位 —— 这十六
/// 个动作没有别的常驻入口可以提示键位。
fn icon_button(ui: &mut egui::Ui, action: FormatAction, keymap: &Keymap) -> egui::Response {
    let label = action.label();
    let tooltip = match command_of(action).and_then(|cmd| keymap.get(cmd)) {
        Some(shortcut) => format!("{label}({})", shortcut.platform_text()),
        None => label.to_owned(),
    };
    icons::icon_button(ui, icon_of(action), &tooltip)
}

/// 动作 → 自绘图标。放 `ui` 层而不是 `compose`:后者发誓不碰 egui。
fn icon_of(action: FormatAction) -> Icon {
    use Icon::*;
    match action {
        FormatAction::InlineCode => CodeInline,
        FormatAction::Link => Link,
        FormatAction::Plain => Paragraph,
        FormatAction::Quote => Quote,
        FormatAction::CodeBlock => CodeBlock,
        FormatAction::Divider => Divider,
        FormatAction::Table => Table,
        FormatAction::Image => Image,
        // duplicate 不上工具条(键盘动作,menu/快捷键层即可),这里只为
        // match 穷尽;若将来要上,补自绘或映射既有图标
        FormatAction::DuplicateSelection | FormatAction::DuplicateLine => SaveAs,
        FormatAction::Bullet => BulletList,
        FormatAction::Ordered => OrderedList,
        FormatAction::Task => TaskList,
        // 形态类(B/I/S/H1-H3)走 RichText,不经过这里;写在这里只为让
        // match 穷尽:将来误把它们派到 `icon_button` 时,一眼能看出不对
        FormatAction::Bold
        | FormatAction::Italic
        | FormatAction::Strike
        | FormatAction::H1
        | FormatAction::H2
        | FormatAction::H3 => Close,
    }
}

/// 动作 → 命令(键位与图标都挂在命令层,工具条不另抄一份)。
///
/// 返回 `None` 表示只有工具条按钮、没有对应命令 —— 目前只有 `Plain`。
fn command_of(action: FormatAction) -> Option<Command> {
    use Command::*;
    Some(match action {
        FormatAction::Bold => FormatBold,
        FormatAction::Italic => FormatItalic,
        FormatAction::Strike => FormatStrike,
        FormatAction::InlineCode => FormatInlineCode,
        FormatAction::Link => FormatLink,
        FormatAction::H1 => FormatH1,
        FormatAction::H2 => FormatH2,
        FormatAction::H3 => FormatH3,
        // 「正文」(去前缀)只在工具条上有按钮:菜单不收它(§6.3 的工具条
        // 已经是它的家),也就没有 Command 与键位。
        FormatAction::Plain => return None,
        FormatAction::Quote => FormatQuote,
        FormatAction::CodeBlock => FormatCodeBlock,
        FormatAction::Divider => FormatDivider,
        FormatAction::Table => FormatTable,
        FormatAction::Image => ImageInsert,
        FormatAction::DuplicateSelection => DuplicateSelection,
        FormatAction::DuplicateLine => DuplicateLine,
        FormatAction::Bullet => FormatBullet,
        FormatAction::Ordered => FormatOrdered,
        FormatAction::Task => FormatTask,
    })
}

/// 悬浮提示:动作名(+ 键位,若该动作有命令且用户绑了键)。
fn command_tooltip(action: FormatAction) -> String {
    match command_of(action).and_then(|cmd| Keymap::builtin().get(cmd)) {
        Some(shortcut) => format!("{}({})", action.label(), shortcut.platform_text()),
        None => action.label().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 十七个动作各有一个 command:这是「键位与图标都挂在命令层」的守卫,
    /// 也是 `Plain` 目前没有 Command 这一事实的唯一登记处。
    #[test]
    fn every_action_has_a_command_except_plain() {
        for action in FormatAction::ALL {
            if action == FormatAction::Plain {
                assert_eq!(command_of(action), None, "正文只在工具条上有按钮");
                continue;
            }
            assert!(command_of(action).is_some(), "{action:?} 缺 Command");
        }
    }

    /// 整条工具条渲染不 panic,且不自发消息(明暗两套 visuals 都过一遍)。
    #[test]
    fn bar_renders_without_messages() {
        for dark in [true, false] {
            let ctx = egui::Context::default();
            if !dark {
                ctx.set_theme(egui::Theme::Light);
            }
            let mut outbox = Vec::new();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                super::ui(ui, &Keymap::builtin(), &mut outbox);
            });
            output.drop_without_applying_deltas();
            assert!(outbox.is_empty(), "仅渲染不产生消息");
        }
    }

    /// 点加粗按钮发 `FormatRequested(Bold)`。
    ///
    /// 走完整 `ui()` 而不是孤立 `button()`:十六个按钮挤在同一行、共享同一
    /// 命中层,只测孤立按钮会漏掉「相邻按钮抢走了这次点击」 —— M1 已经在
    /// 标题栏上踩过一次。
    #[test]
    fn clicking_first_button_requests_bold() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        ctx.run_ui(RawInput::default(), |ui| {
            let response = button(ui, FormatAction::Bold, &Keymap::builtin());
            rect.set(response.rect);
        })
        .drop_without_applying_deltas();
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
                |ui| super::ui(ui, &Keymap::builtin(), &mut outbox),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::FormatRequested(FormatAction::Bold)]);
    }

    /// 点图片按钮是**开对话框**,不是发格式请求 —— 十七个动作里只有它走
    /// 这条路径(需要 alt 与 url 两个输入)。
    ///
    /// **2026-10-08 S2-2 改版后图片在「更多」菜单里**,故本测试比改版前
    /// 多一段:先点开菜单,再在**菜单已展开**的帧里用探针取 Image 矩形。
    ///
    /// 探针在菜单内生效是刻意设计(`ui_with_probe` 的 ② 段):否则菜单里
    /// 的动作没有任何测试能定位 —— 等于十二个低频动作的点击路径在溢出
    /// 后彻底失去覆盖,且「探针只对直出按钮生效」这件事本身不会报错。
    #[test]
    fn clicking_image_requests_the_dialog_not_a_format() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 800.0));
        let mut outbox = Vec::new();
        let keymap = Keymap::builtin();

        // —— 第 1 段:定位「更多」按钮并点开菜单 ——
        // 走 shapes 文本定位而非探针:菜单按钮本身不是 FormatAction,
        // 探针不覆盖它。菜单栏的同类测试(`menubar.rs` 设置直达页)已实证
        // 同一手法。
        let mut more_rect = None;
        for _ in 0..3 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| super::ui(ui, &keymap, &mut Vec::new()),
            );
            let shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
            more_rect = shapes.iter().find_map(|clipped| {
                let egui::epaint::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                (text.galley.job.text == "更多").then(|| clipped.shape.visual_bounding_rect())
            });
            if more_rect.is_some() {
                break;
            }
        }
        let more_center = more_rect
            .unwrap_or_else(|| panic!("工具条上找不到「更多」按钮"))
            .center();
        let click_at = |pos: egui::Pos2, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            Vec::new(),
            vec![Event::PointerMoved(more_center)],
            vec![click_at(more_center, true)],
            vec![click_at(more_center, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| super::ui(ui, &keymap, &mut Vec::new()),
            )
            .drop_without_applying_deltas();
        }

        // —— 第 2 段:菜单展开后取 Image 矩形 ——
        // egui 0.36 的 MenuButton 从「popup 记忆开态」到「闭包真正绘制
        // 条目」隔一帧(事件帧写记忆 → 次帧按钮收到开态 → 再次帧闭包执行),
        // 故最多试三帧,拿不到就明说而不是静默跳过(与 decisions-pending
        // #128 记录的 menubar 同款时序)。
        let rect = Cell::new(Rect::NOTHING);
        for _ in 0..3 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| {
                    super::ui_with_probe(
                        ui,
                        &keymap,
                        &mut Vec::new(),
                        Some(|action: FormatAction, button_rect: Rect| {
                            if action == FormatAction::Image {
                                rect.set(button_rect);
                            }
                        }),
                        None::<fn(Rect)>,
                    );
                },
            );
            output.drop_without_applying_deltas();
            if rect.get() != Rect::NOTHING {
                break;
            }
        }
        let center = rect.get().center();
        assert!(
            rect.get() != Rect::NOTHING,
            "菜单展开后探针应拿到 Image 条目的位置"
        );

        // —— 第 3 段:点它 ——
        for events in [
            vec![Event::PointerMoved(center)],
            vec![click_at(center, true)],
            vec![click_at(center, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| super::ui(ui, &keymap, &mut outbox),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::ImageDialogOpened]);
    }

    /// **直出 + 溢出恰好覆盖全部四组**(2026-10-08 S2-2 守门)。
    ///
    /// 拦的是「将来给 `FormatGroup` 加了第五组,却忘了放进 DIRECT 或
    /// OVERFLOW」—— 那会让新组**静默从工具条消失**:不报编译错、菜单栏
    /// 照常有它、只有常驻按钮不见了。生产路径有同款 `debug_assert!`,
    /// 本测试是它的可读版本(且在 release 下也跑)。
    #[test]
    fn direct_and_overflow_cover_every_group_exactly_once() {
        assert_eq!(
            FormatGroup::DIRECT.len() + FormatGroup::OVERFLOW.len(),
            FormatGroup::ALL.len(),
            "两组之和须等于全部组数"
        );
        for group in FormatGroup::ALL {
            let in_direct = FormatGroup::DIRECT.contains(&group);
            let in_overflow = FormatGroup::OVERFLOW.contains(&group);
            assert!(
                in_direct ^ in_overflow,
                "{group:?} 必须恰好属于 DIRECT / OVERFLOW 之一 \
                 (direct={in_direct} overflow={in_overflow})"
            );
        }
    }

    /// 直出位是行内 + 列表两组(2026-10-08 S2-2 的**收益断言**)。
    ///
    /// 锁的是「工具条只留高频」这条 ui-polish §1.2 原则。列表组在列,
    /// 不是因为它高频,而是因为 `Task` 是唯一的**多步交互**(三态循环)——
    /// 进菜单会让「一次状态切换」从 3 次点击涨到 6 次,详见
    /// `FormatGroup::DIRECT` 的文档。
    ///
    /// 哪天有人把标题 / 块组挪回直出位(看起来「更方便」),本测试变红。
    #[test]
    fn direct_row_is_inline_plus_list_only() {
        // **比切片而不是定长数组**:数组长度一变(如把 Heading 挪进
        // DIRECT),`assert_eq!([T; 3], [T; 2])` 是**编译错误**而非断言
        // 失败 —— 守门测试若以「编不过」的方式拦回归,就不是守门测试,而是
        // 一根会误伤正确改动的拦路桩(首版就踩了这个)。
        assert_eq!(
            FormatGroup::DIRECT.as_slice(),
            &[FormatGroup::Inline, FormatGroup::List],
            "直出组应是行内 + 列表(列表因 Task 三态循环而必须直出)"
        );
        assert_eq!(
            FormatGroup::OVERFLOW.as_slice(),
            &[FormatGroup::Heading, FormatGroup::Block],
            "溢出组应是标题 + 块"
        );
        // 收益量化:17 项里 8 项直出、9 项进菜单,加「更多」与 emoji 共
        // 10 个可见槽位(改版前 18 个)。
        let direct: usize = FormatGroup::DIRECT.iter().map(|g| g.actions().len()).sum();
        let overflow: usize = FormatGroup::OVERFLOW
            .iter()
            .map(|g| g.actions().len())
            .sum();
        assert_eq!(
            direct + overflow,
            FormatAction::ALL.len(),
            "17 项一个不能少"
        );
        assert_eq!(direct, 8, "直出 8 项(行内 5 + 列表 3)");
        assert_eq!(overflow, 9, "溢出 9 项(标题 4 + 块 5)");
    }

    /// 点笑脸按钮是**开 Emoji 面板**,不是发格式请求:第二个「对话框类
    /// 动作」(docs/emoji-plan.md E1),不进 FormatAction 四组。位置经
    /// emoji 探针在同一条真实工具条布局里取(与图片测试同一手法)。
    #[test]
    fn clicking_emoji_opens_the_picker_not_a_format() {
        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 800.0));
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        ctx.run_ui(
            RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                super::ui_with_probe(
                    ui,
                    &Keymap::builtin(),
                    &mut Vec::new(),
                    None::<fn(FormatAction, Rect)>,
                    Some(|button_rect: Rect| rect.set(button_rect)),
                );
            },
        )
        .drop_without_applying_deltas();
        let center = rect.get().center();
        assert!(center.x > 0.0, "探针拿到了 Emoji 按钮的位置:{center:?}");
        let click = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        for events in [
            Vec::new(),
            Vec::new(),
            vec![Event::PointerMoved(center)],
            vec![click(true)],
            vec![click(false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ui| super::ui(ui, &Keymap::builtin(), &mut outbox),
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::EmojiPickerToggle(true)]);
    }

    /// 自绘图标基本不重合。十七个动作里六个形态类走 RichText(不经
    /// `icon_of`),余下十一个各有各的画面。
    #[test]
    fn icons_are_distinct_per_action() {
        use std::collections::HashSet;
        let seen: HashSet<String> = FormatAction::ALL
            .iter()
            .map(|action| format!("{:?}", icon_of(*action)))
            .collect();
        // 6 个形态类共用一个占位值 + 11 个自绘各一枚 = 12 枚;任何两枚被
        // 借来借去都会掉到 11 以下(`Plain` 曾借 `Table` 的旧账)。
        assert_eq!(
            seen.len(),
            12,
            "图标疑似复用(除形态类占位外不应有重码):{:?}",
            seen.iter().collect::<Vec<_>>()
        );
    }
}
