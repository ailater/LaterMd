//! 编辑器顶部的标签条(#11「multi-tabs」,docs/auto-plan.md 规格)。
//!
//! 形态:一排 chip(文件名 + dirty 星 + 关闭 ×)。**单个未命名且干净的空
//! 标签不画条** —— 此时标签条零信息量,省一行高度;一旦有第二个标签或
//! 当前文档落盘,条即出现,关闭入口(× 与 Ctrl+W)随之可用。
//!
//! 交互:点击名字区 = 激活;点击 × = 请求关闭(脏标签由归约侧弹确认模态,
//! 见 `State::request_close_tab`);右键 = 批量操作菜单(#37:关闭左侧/
//! 右侧/全部/其他,以**被右键的标签**为基准;「重命名」是**显示别名**——
//! 只改本条 chip 的显示文本,不改盘上文件名与保存路径;标题宽度模式由后续
//! 模块交付,先入骨架禁用)。chip 自绘(与工具栏图标按钮同一套手法),
//! 选中态用填充底色 —— 与侧边栏页签的下划线区分层级。

use crate::state::Message;
use crate::tabs::{TabRename, TabsState};
use crate::ui::tokens::{RADIUS_SM, SPACE_SM, SPACE_XS};
use eframe::egui::{self, Align2, Sense};

/// chip 高度(比工具栏矮一档:标签条更密集)。
const CHIP_H: f32 = 24.0;
/// 关闭 × 的方框边长。
const CLOSE: f32 = 12.0;

/// 绘制标签条;返回是否实际绘制(单个未命名空标签不画,测试据此断言)。
pub fn ui(panel: &mut egui::Ui, tabs: &TabsState, outbox: &mut Vec<Message>) -> bool {
    let hide = tabs.tabs.len() == 1
        && tabs.tabs[0].document.path.is_none()
        && !tabs.current().editor.is_dirty();
    if hide {
        return false;
    }
    // 标签放不下时水平滚动而非换行(同 vendored 表格的横向滚动手法):
    // 换行会让标签条高度随标签数成倍增长,把编辑区顶得上下跳;单行 +
    // 滚动(垂直滚轮在仅水平可滚的 ScrollArea 里自动转为水平)高度恒定。
    egui::ScrollArea::horizontal()
        .id_salt("tabs-bar")
        .auto_shrink([false, true])
        .show(panel, |ui| {
            ui.horizontal(|ui| {
                for index in 0..tabs.tabs.len() {
                    chip(ui, tabs, index, outbox);
                }
            });
        });
    true
}

/// 右键菜单条目的响应组(`context_menu_items` 的返回值;生产闭包忽略,
/// 无头测试借此定位各条目矩形,与 `image_dialog::dialog` 返回按钮响应
/// 同款手法)。字段只在测试读取(与 `PreviewState::text` 同款豁免)。
#[allow(dead_code)]
pub(crate) struct TabMenuItems {
    pub close_left: egui::Response,
    pub close_right: egui::Response,
    pub close_all: egui::Response,
    pub close_others: egui::Response,
    pub rename: egui::Response,
    pub short_title: egui::Response,
    pub full_title: egui::Response,
}

/// 后续模块交付的条目统一禁用文案(#37 菜单骨架,现仅剩标题宽度两项)。
const PENDING_HINT: &str = "后续版本交付";

/// 单条菜单项:可用性由调用方判定(无目标即禁用),禁用态给悬停说明。
fn menu_item(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new(label));
    if enabled {
        response
    } else {
        response.on_disabled_hover_text(PENDING_HINT)
    }
}

/// 右键标签的批量操作菜单(#37)。目标是**被右键的标签本身**(`index`,
/// 菜单弹出帧的快照),不是当前活动标签;关闭左侧/右侧/其他都以它为
/// 基准并保留它。无目标可关时(最左标签的「关闭左侧」等)禁用对应条目,
/// 归约侧对空队列防御性 no-op。「重命名」(显示别名)对任何标签可用——
/// 含未命名标签;缩短标题/完整标题由后续模块交付,先入骨架(禁用)。
fn context_menu_items(
    ui: &mut egui::Ui,
    tabs: &TabsState,
    index: usize,
    outbox: &mut Vec<Message>,
) -> TabMenuItems {
    let close_left = menu_item(ui, "关闭左侧", index > 0);
    if close_left.clicked() {
        outbox.push(Message::TabBatchCloseRequested {
            kind: crate::tabs::BatchClose::Left,
            index,
        });
    }
    let close_right = menu_item(ui, "关闭右侧", index + 1 < tabs.tabs.len());
    if close_right.clicked() {
        outbox.push(Message::TabBatchCloseRequested {
            kind: crate::tabs::BatchClose::Right,
            index,
        });
    }
    let close_all = menu_item(ui, "关闭全部", true);
    if close_all.clicked() {
        outbox.push(Message::TabBatchCloseRequested {
            kind: crate::tabs::BatchClose::All,
            index,
        });
    }
    let close_others = menu_item(ui, "关闭其他", tabs.tabs.len() > 1);
    if close_others.clicked() {
        outbox.push(Message::TabBatchCloseRequested {
            kind: crate::tabs::BatchClose::Others,
            index,
        });
    }
    ui.separator();
    let rename = menu_item(ui, "重命名", true);
    if rename.clicked() {
        outbox.push(Message::TabRenameRequested { index });
    }
    ui.separator();
    let short_title = menu_item(ui, "缩短标题", false);
    let full_title = menu_item(ui, "完整标题", false);
    TabMenuItems {
        close_left,
        close_right,
        close_all,
        close_others,
        rename,
        short_title,
        full_title,
    }
}

/// 单个标签 chip:名字区点击激活,× 区点击请求关闭。
fn chip(ui: &mut egui::Ui, tabs: &TabsState, index: usize, outbox: &mut Vec<Message>) {
    let tab = &tabs.tabs[index];
    let selected = index == tabs.active;
    let name = tab.display_name();
    let text_color = if selected {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let name_w = ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(name.clone(), font.clone(), text_color))
        .rect
        .width();
    let size = egui::vec2(SPACE_SM + name_w + SPACE_XS + CLOSE + SPACE_SM, CHIP_H);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let close_rect = egui::Rect::from_min_size(
        egui::pos2(
            rect.right() - SPACE_SM - CLOSE,
            rect.center().y - CLOSE / 2.0,
        ),
        egui::vec2(CLOSE, CLOSE),
    );

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // WorkBuddy 风活动页签:浅蓝底 + 蓝字 + 底部 2px 蓝条;未选中悬停浅灰
        let accent = crate::ui::tokens::accent(ui);
        let selected_bg = crate::theme::shell_tokens(ui.visuals().dark_mode).selected_bg;
        let hover_bg = crate::theme::shell_tokens(ui.visuals().dark_mode).hover;
        let bg = if selected {
            selected_bg
        } else if response.hovered() {
            hover_bg
        } else {
            egui::Color32::TRANSPARENT
        };
        painter.rect_filled(rect, RADIUS_SM, bg);
        let text_color = if selected { accent } else { text_color };
        painter.text(
            egui::pos2(rect.left() + SPACE_SM, rect.center().y),
            Align2::LEFT_CENTER,
            &name,
            font,
            text_color,
        );
        if selected {
            // 底部 2px 强调条:WorkBuddy 标签的视觉锚点
            let bar = egui::Rect::from_min_max(
                egui::pos2(rect.left() + SPACE_SM, rect.bottom() - 2.0),
                egui::pos2(rect.right() - SPACE_SM, rect.bottom()),
            );
            painter.rect_filled(bar, 1.0, accent);
        }
        // 关闭 ×:悬停该 chip 时才上色(常驻会显得噪)
        let cross = if response.hovered() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        let stroke = egui::Stroke::new(1.2, cross);
        painter.line_segment(
            [
                egui::pos2(close_rect.left(), close_rect.top()),
                egui::pos2(close_rect.right(), close_rect.bottom()),
            ],
            stroke,
        );
        painter.line_segment(
            [
                egui::pos2(close_rect.right(), close_rect.top()),
                egui::pos2(close_rect.left(), close_rect.bottom()),
            ],
            stroke,
        );
    }

    if response.clicked() {
        // 命中判定用本帧指针位置:落在 × 区 = 关闭,落在名字区 = 激活
        let close_hit = response
            .interact_pointer_pos()
            .is_some_and(|pos| close_rect.expand(2.0).contains(pos));
        if close_hit {
            outbox.push(Message::TabCloseRequested(index));
        } else {
            outbox.push(Message::TabActivate(index));
        }
    }

    // 右键菜单(#37):挂在被右键的 chip 上(目标即该标签,非当前活动)。
    // `context_menu` 消费 Response,借 clone 注册、原 response 继续供上面的
    // 点击判定用(与标题栏设置键右键直达同手法)。
    response.clone().context_menu(|ui| {
        context_menu_items(ui, tabs, index, outbox);
    });
}

/// 重命名浮窗(#37「重命名」,**显示别名**语义):单行输入 + 确定/取消。
/// 草稿由 UI 原地改(`TabRename.draft`,与 `ImageDialogState` 同款);草稿
/// trim 后为空时「确定」禁用(归约侧对绕过 UI 的消息仍防御性拒绝)。
///
/// `file_label` 是目标标签的落盘身份(文件名或「尚未保存」),原样展示在
/// 浮窗里 —— 既明示本操作**不改盘上文件**的作用范围,也让别名盖住 chip
/// 后用户仍有地方看见真实文件名。返回(确定, 取消)响应供测试定位
/// (与 `image_dialog::dialog` 同款手法)。
pub(crate) fn rename_dialog(
    ui: &mut egui::Ui,
    rename: &mut TabRename,
    file_label: &str,
) -> (egui::Response, egui::Response) {
    let mut buttons = None;
    egui::Window::new("重命名标签")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ui.ctx().viewport_rect().center())
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("标签显示名");
            ui.text_edit_singleline(&mut rename.draft)
                .on_hover_text("只改此标签的显示名;文件名与保存路径不变");
            ui.add_space(crate::ui::tokens::SPACE_XS);
            ui.weak(format!("文件:{file_label}(重命名不改动它)"));
            ui.add_space(crate::ui::tokens::SPACE_SM);
            ui.separator();
            ui.horizontal(|ui| {
                let ready = !rename.draft.trim().is_empty();
                let confirm = ui
                    .add_enabled(ready, egui::Button::new("确定"))
                    .on_disabled_hover_text("名称不能为空;不改请点「取消」");
                let cancel = ui.button("取消");
                buttons = Some((confirm, cancel));
            });
        });
    buttons.expect("浮窗必然绘制确定/取消按钮")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::TabsState;
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;

    /// 单个未命名空标签不画条(零信息量);落盘或变脏后条出现。
    #[test]
    fn bar_hidden_for_single_empty_tab_only() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("");
        let mut outbox = Vec::new();

        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(!super::ui(ui, &tabs, &mut outbox), "单个未命名空标签不画条");
        });
        output.drop_without_applying_deltas();

        tabs.current_mut().editor.insert_chars(0, "写了字");
        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(super::ui(ui, &tabs, &mut outbox), "dirty 后条出现");
        });
        output.drop_without_applying_deltas();

        // 落盘后即使不脏也显示(有关闭入口的信息量)
        tabs.current_mut().editor.clear_dirty();
        tabs.current_mut().document.path = Some(std::path::PathBuf::from("/a.md"));
        let output = ctx.run_ui(RawInput::default(), |ui| {
            assert!(super::ui(ui, &tabs, &mut outbox));
        });
        output.drop_without_applying_deltas();
    }

    /// 标签多到放不下时保持单行水平滚动,不换行:同一组标签在窄/宽容器里
    /// 条高一致(换行实现的高度随标签数成倍增长,编辑区会被顶得上下跳)。
    #[test]
    fn bar_keeps_single_line_when_chips_overflow() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("");
        for i in 0..6 {
            tabs.open_tab(
                Some(std::path::PathBuf::from(format!(
                    "一个很长很长的文档标题第{i}篇.md"
                ))),
                "",
            );
        }
        let mut outbox = Vec::new();
        let heights = [220.0, 2000.0].map(|width| {
            let mut height = None;
            let output = ctx.run_ui(RawInput::default(), |ui| {
                ui.set_max_width(width);
                assert!(super::ui(ui, &tabs, &mut outbox), "多标签必画条");
                height = Some(ui.min_rect().height());
            });
            output.drop_without_applying_deltas();
            height.unwrap()
        });
        assert_eq!(heights[0], heights[1], "窄容器不换行,条高恒定");
        // 单行高度与 chip 高度同量级(留行距与滚动条余量),远小于 6 行
        assert!(heights[1] < 2.0 * CHIP_H, "条高 {}", heights[1]);
    }

    /// chip 交互:点名字区发 TabActivate,点 × 区发 TabCloseRequested。
    #[test]
    fn chip_click_zones_send_different_messages() {
        let ctx = egui::Context::default();
        let tabs = TabsState::new("第一篇");
        let mut tabs = tabs;
        tabs.open_tab(None, "第二篇");
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧拿 chip 0 的位置(画在原始位置)
        let output = ctx.run_ui(RawInput::default(), |ui| {
            chip(ui, &tabs, 0, &mut outbox);
            rect.set(ui.min_rect());
        });
        output.drop_without_applying_deltas();
        let rect = rect.get();
        let name_pos = egui::pos2(rect.left() + 3.0, rect.center().y);
        let close_pos = egui::pos2(rect.right() - SPACE_SM - CLOSE / 2.0, rect.center().y);
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        // 点名字区 → TabActivate(0)
        for events in [
            vec![Event::PointerMoved(name_pos)],
            vec![click(name_pos, true)],
            vec![click(name_pos, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    chip(ui, &tabs, 0, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::TabActivate(0)]);
        outbox.clear();

        // 点 × 区 → TabCloseRequested(0)
        for events in [
            vec![Event::PointerMoved(close_pos)],
            vec![click(close_pos, true)],
            vec![click(close_pos, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    chip(ui, &tabs, 0, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::TabCloseRequested(0)]);
    }

    /// 右键菜单动作消息(#37):四个批量动作都携带**被右键标签**的索引
    /// (非当前活动标签),点击才发消息。菜单直接渲染(与 menubar::item
    /// 的测法同款):条目响应里有矩形,按中心点击。
    #[test]
    fn context_menu_actions_carry_right_clicked_tab_index() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("甲");
        tabs.open_tab(None, "乙");
        tabs.open_tab(None, "丙");
        tabs.activate(2); // active = 丙,右键目标是乙(索引 1,非活动)
        let cases = [
            ("左", crate::tabs::BatchClose::Left),
            ("右", crate::tabs::BatchClose::Right),
            ("全", crate::tabs::BatchClose::All),
            ("他", crate::tabs::BatchClose::Others),
        ];
        for (name, kind) in cases {
            let mut outbox = Vec::new();
            let rect = Cell::new(Rect::NOTHING);
            ctx.run_ui(RawInput::default(), |ui| {
                let items = context_menu_items(ui, &tabs, 1, &mut outbox);
                rect.set(match kind {
                    crate::tabs::BatchClose::Left => items.close_left.rect,
                    crate::tabs::BatchClose::Right => items.close_right.rect,
                    crate::tabs::BatchClose::All => items.close_all.rect,
                    crate::tabs::BatchClose::Others => items.close_others.rect,
                });
            })
            .drop_without_applying_deltas();
            assert!(outbox.is_empty(), "{name}:仅渲染不发消息");

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
                    context_menu_items(ui, &tabs, 1, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
            assert_eq!(
                outbox,
                vec![Message::TabBatchCloseRequested { kind, index: 1 }],
                "{name}:消息携带被右键标签的索引"
            );
        }
    }

    /// 首/尾/唯一边界(#37):无目标可关的菜单项禁用(点击也不发消息),
    /// 有目标的照常可用。顺序 = 左侧 / 右侧 / 全部 / 其他。
    #[test]
    fn context_menu_disables_items_without_targets() {
        let ctx = egui::Context::default();

        let assert_items = |tabs: &TabsState, index: usize, want: [bool; 4]| {
            let mut outbox = Vec::new();
            let rects = Cell::new(Vec::<Rect>::new());
            ctx.run_ui(RawInput::default(), |ui| {
                let items = context_menu_items(ui, tabs, index, &mut outbox);
                assert_eq!(
                    [
                        items.close_left.enabled(),
                        items.close_right.enabled(),
                        items.close_all.enabled(),
                        items.close_others.enabled()
                    ],
                    want
                );
                rects.set(
                    [
                        items.close_left.rect,
                        items.close_right.rect,
                        items.close_all.rect,
                        items.close_others.rect,
                    ]
                    .to_vec(),
                );
            })
            .drop_without_applying_deltas();
            // 逐条点击**禁用**项:一律不发消息
            for (rect, enabled) in rects.take().into_iter().zip(want) {
                if enabled {
                    continue;
                }
                let center = rect.center();
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
                        context_menu_items(ui, tabs, index, &mut outbox);
                    },
                )
                .drop_without_applying_deltas();
            }
            assert!(outbox.is_empty(), "禁用项点击零消息,实际 {outbox:?}");
        };

        // 唯一标签:左/右/其他无目标;全部仍有目标(关空兜底补空标签)
        let single = TabsState::new("仅此一篇");
        assert_items(&single, 0, [false, false, true, false]);

        let mut tabs = TabsState::new("甲");
        tabs.open_tab(None, "乙");
        tabs.open_tab(None, "丙");
        // 最左:左侧无目标;最右:右侧无目标;中间:全有目标
        assert_items(&tabs, 0, [false, true, true, true]);
        assert_items(&tabs, 1, [true, true, true, true]);
        assert_items(&tabs, 2, [true, false, true, true]);
    }

    /// 菜单骨架(#37):「重命名」已交付(显示别名)—— 恒可用,点击发
    /// `TabRenameRequested` 且携带**被右键标签**的索引;缩短/完整标题仍由
    /// 后续模块交付,渲染出来但禁用(点击不发消息)。
    #[test]
    fn context_menu_rename_clicks_request_with_right_clicked_index() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("甲");
        tabs.open_tab(None, "乙");
        tabs.activate(1); // active = 乙,右键目标是甲(索引 0,非活动)
        let mut outbox = Vec::new();
        let rename_rect = Cell::new(Rect::NOTHING);
        let pending_rects = Cell::new(Vec::<Rect>::new());
        ctx.run_ui(RawInput::default(), |ui| {
            let items = context_menu_items(ui, &tabs, 0, &mut outbox);
            assert!(items.rename.enabled(), "重命名对任何标签可用");
            assert!(!items.short_title.enabled(), "缩短标题未交付,禁用");
            assert!(!items.full_title.enabled(), "完整标题未交付,禁用");
            rename_rect.set(items.rename.rect);
            pending_rects.set(vec![items.short_title.rect, items.full_title.rect]);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不发消息");

        // 缩短/完整标题:禁用,点击零消息
        for rect in pending_rects.take() {
            let center = rect.center();
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
                    context_menu_items(ui, &tabs, 0, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(outbox.is_empty(), "骨架项禁用,点击零消息");

        // 「重命名」点击 → TabRenameRequested { index: 被右键的 0 }
        let center = rename_rect.get().center();
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
                context_menu_items(ui, &tabs, 0, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::TabRenameRequested { index: 0 }],
            "重命名点击携带被右键标签的索引"
        );
    }

    /// 重命名浮窗(#37):空/纯空白草稿禁用「确定」(禁用态有说明),非空
    /// 启用;「取消」恒可点。渲染不 panic。
    #[test]
    fn rename_dialog_blank_draft_disables_confirm() {
        let ctx = egui::Context::default();
        for draft in ["", "   \t"] {
            let mut rename = TabRename {
                tab_id: 7,
                draft: draft.to_owned(),
            };
            let mut seen = None;
            for _ in 0..3 {
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    let (confirm, cancel) = rename_dialog(ui, &mut rename, "a.md");
                    seen = Some((confirm.enabled(), cancel.enabled()));
                });
                output.drop_without_applying_deltas();
            }
            let (confirm_enabled, cancel_enabled) = seen.expect("至少跑了一帧");
            assert!(!confirm_enabled, "空白草稿 {draft:?} 禁用确定");
            assert!(cancel_enabled, "取消恒可点");
        }
        let mut rename = TabRename {
            tab_id: 7,
            draft: "  笔记  ".to_owned(),
        };
        let mut confirm_enabled = false;
        for _ in 0..3 {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                let (confirm, _) = rename_dialog(ui, &mut rename, "a.md");
                confirm_enabled = confirm.enabled();
            });
            output.drop_without_applying_deltas();
        }
        assert!(confirm_enabled, "trim 后非空即可确定");
    }

    /// 右键标签弹菜单(#37 装配):secondary 点击**标签条上**的标签(chip
    /// 经 `ui` 的真实入口渲染,含 ScrollArea)后 egui 侧确有弹层打开。
    /// 菜单条目本身的行为由上面的直渲染测试覆盖。
    #[test]
    fn right_click_on_chip_opens_context_menu() {
        let ctx = egui::Context::default();
        let mut tabs = TabsState::new("甲");
        tabs.open_tab(None, "乙");
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        ctx.run_ui(RawInput::default(), |ui| {
            assert!(super::ui(ui, &tabs, &mut outbox), "多标签必画条");
            rect.set(ui.min_rect());
        })
        .drop_without_applying_deltas();
        assert!(!egui::containers::Popup::is_any_open(&ctx), "前置:无弹层");

        // 标签条最左侧必是第一个 chip(名字区靠左):取条左端内侧一点
        let pos = egui::pos2(rect.get().left() + 2.0, rect.get().top() + CHIP_H / 2.0);
        let click = |pressed| Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        };
        // 右键帧:secondary 点击让 context_menu 置开;下一帧空输入渲染,
        // 弹层内容(菜单条目)实际画出来。
        ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(pos), click(true), click(false)],
                ..Default::default()
            },
            |ui| {
                super::ui(ui, &tabs, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        ctx.run_ui(RawInput::default(), |ui| {
            super::ui(ui, &tabs, &mut outbox);
        })
        .drop_without_applying_deltas();
        assert!(
            egui::containers::Popup::is_any_open(&ctx),
            "右键后菜单弹层打开"
        );
        assert!(outbox.is_empty(), "右键本身不发消息");
    }
}
