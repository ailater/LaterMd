//! Markdown 格式工具条(docs/ui-shell-redesign.md §6)。
//!
//! 十六个动作分四组:行内 / 标题 / 块 / 列表。点击只发
//! [`Message::FormatRequested`],语义全在 [`crate::compose`] —— 本模块
//! 一行 Markdown 逻辑都没有。
//!
//! ## 形态选择
//!
//! 加粗/斜体/删除线/H1-H3 用 **RichText**(`B` / `I` / `S` / `H1` 等)
//! 而不是自绘图标:这三个是「形态」抽象概念,线段自绘只能画出没有辨识度
//! 的矩形;而 `RichText::strong()/italics()/strikethrough()` 是 egui 内建
//! 富文本能力,零字形依赖风险(§6.1)。
//! 其余用自绘线段图标,遵守 ui-polish §1.1「图标是矢量自绘,不是字体字符」。
//!
//! ## 为什么要单独一条
//!
//! 文件工具栏是「对**文档**做文件级动作」,本条是「对**选区**做文本级动
//! 作」,心智模型不同;分开之后各自的宽度压力也小得多 —— 十六个按钮平铺
//! 进文件栏会把文档名挤没。

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
/// 可到 400px 左右,十六个按钮不可能一行排完。
pub fn ui(panel: &mut egui::Ui, keymap: &Keymap, outbox: &mut Vec<Message>) {
    ui_with_probe(panel, keymap, outbox, None::<fn(FormatAction, egui::Rect)>)
}

/// 同 [`ui`],额外把每个按钮的 `(动作, 矩形)` 交给 `probe`(`None` 即不探针)。
///
/// 无头测试量按钮位置用:十六个按钮的具体坐标由 `horizontal_wrapped` 的换
/// 行演算 + 它前面的标签条/文件工具栏共同决定,手搓必然与真实帧错位。
pub fn ui_with_probe(
    panel: &mut egui::Ui,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
    probe: Option<impl FnMut(FormatAction, egui::Rect)>,
) {
    let mut probe = probe;
    panel.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;
        for group in FormatGroup::ALL {
            for action in group.actions() {
                let response = button(ui, *action, keymap);
                if let Some(probe) = probe.as_mut() {
                    probe(*action, response.rect);
                }
                if response.clicked() {
                    outbox.push(Message::FormatRequested(*action));
                }
            }
            // 组之间是竖向分隔条,最后一组之后不画
            if group != FormatGroup::ALL[FormatGroup::ALL.len() - 1] {
                ui.separator();
            }
        }
    });
}

/// 单个按钮。返回响应以便测试定位(与 `ui::menubar::item` 同款手法)。
fn button(ui: &mut egui::Ui, action: FormatAction, keymap: &Keymap) -> egui::Response {
    match action {
        // 形态类走富文本:B / I / S 是抽象概念的自解释字形,H1-H3 用同名
        // 数字。其余一律自绘线段图标。
        FormatAction::Bold => rich(ui, action, "B", |text| text.strong()),
        FormatAction::Italic => rich(ui, action, "I", |text| text.italics()),
        FormatAction::Strike => rich(ui, action, "S", |text| text.strikethrough()),
        FormatAction::H1 => rich(ui, action, "H1", |text| text.strong()),
        FormatAction::H2 => rich(ui, action, "H2", |text| text.strong()),
        FormatAction::H3 => rich(ui, action, "H3", |text| text.strong()),
        _ => icon_button(ui, action, keymap),
    }
}

/// 富文本按钮:`B` / `I` / `S` / `H1` 等。tooltip 带当前键位 —— 形态字形
/// 本身不解释自己,靠 tooltip 兜可读性(ui-polish §1.1 的同款要求)。
fn rich(
    ui: &mut egui::Ui,
    action: FormatAction,
    glyph: &str,
    style: impl Fn(egui::RichText) -> egui::RichText,
) -> egui::Response {
    let width = ui.spacing().interact_size.y.max(tokens::ICON + 8.0);
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
        // `impl ToString` 会把 RichText 降级成纯字符串,strong / italics /
        // strikethrough 在这一个转换里全丢 —— 加粗按钮于是长得跟普通按钮没
        // 两样,删除线干脆看不见。
        let mut text = egui::RichText::new(glyph).size(tokens::ICON_SM);
        if !enabled {
            text = text.color(ui.visuals().weak_text_color());
        }
        let galley = egui::WidgetText::from(style(text)).into_galley(
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

    /// 十六个动作各有一个 command:这是「键位与图标都挂在命令层」的守卫,
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

    /// 自绘图标基本不重合。十六个动作里六个形态类走 RichText(不经
    /// `icon_of`),余下十个各有各的画面。
    #[test]
    fn icons_are_distinct_per_action() {
        use std::collections::HashSet;
        let seen: HashSet<String> = FormatAction::ALL
            .iter()
            .map(|action| format!("{:?}", icon_of(*action)))
            .collect();
        // 6 个形态类共用一个占位值 + 10 个自绘各一枚 = 11 枚;任何两枚被
        // 借来借去都会掉到 10 以下(`Plain` 曾借 `Table` 的旧账)。
        assert_eq!(
            seen.len(),
            11,
            "图标疑似复用(除形态类占位外不应有重码):{:?}",
            seen.iter().collect::<Vec<_>>()
        );
    }
}
