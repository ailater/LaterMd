//! 自绘标题栏与无边框窗口骨架(docs/ui-shell-redesign.md §3,决策 D1)。
//!
//! 无边框(`ViewportBuilder::with_decorations(false)`)后窗口 chrome 全部由
//! 本模块接管:36px 标题栏(拖拽 / 双击最大化 / 六按钮)+ 屏幕四边四角的
//! 透明缩放命令区。`LATERMD_NATIVE_DECORATIONS=1` 时两块都不绘制
//! (逃生口,行为 = 原生装饰的现状),开关在 `main` 启动时读一次。
//!
//! 命中矩形划分、缩放方向、最大化图标这些**纯几何/纯逻辑**抽成函数供
//! 单测钉住;`ui` 的交互区用 `Ui::allocate_rect`(绝对摆放)直接消费
//! 同一批纯函数,测试与绘制零偏差。

use crate::state::{Message, State};
use crate::ui::icons::Icon;
use crate::ui::tokens::{DANGER, ICON, ICON_SM, RADIUS_SM, SPACE_SM};
use eframe::egui::{self, Color32, CursorIcon, PointerButton, Pos2, Rect, ResizeDirection};
use eframe::egui::{Sense, ViewportCommand};

/// 四边缩放命中条厚度。
pub const EDGE_T: f32 = 6.0;
/// 四角缩放命中块边长(盖住边条交叠,角上命中对角方向)。
pub const CORNER_T: f32 = 12.0;

/// 标题栏右端六个窗口按钮,从左到右(docs/ui-shell-redesign.md §3.1 顺序)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleButton {
    /// 关闭/打开左栏(翻转侧边栏可见性)。
    PanelLeft,
    /// 关闭/打开右栏(消息由三分栏重排棒消费)。
    PanelRight,
    /// 禅定模式(本棒占位禁用,M4 实装)。
    Zen,
    /// 最小化。
    Minimize,
    /// 最大化 / 还原(按当前状态切图标)。
    Maximize,
    /// 关闭窗口。
    Close,
}

/// 标题按钮的绘制顺序(= 命中矩形从左到右的顺序)。
pub const TITLE_BUTTONS: [TitleButton; 6] = [
    TitleButton::PanelLeft,
    TitleButton::PanelRight,
    TitleButton::Zen,
    TitleButton::Minimize,
    TitleButton::Maximize,
    TitleButton::Close,
];

/// 按钮图标;最大化态的 [`TitleButton::Maximize`] 画还原图标
/// (条件与 `ctx.input(|i| i.viewport().maximized)` 的取值一一对应)。
pub fn icon_of(button: TitleButton, maximized: bool) -> Icon {
    match button {
        TitleButton::PanelLeft => Icon::Sidebar,
        TitleButton::PanelRight => Icon::PanelRight,
        TitleButton::Zen => Icon::Zen,
        TitleButton::Minimize => Icon::Minimize,
        TitleButton::Maximize if maximized => Icon::Restore,
        TitleButton::Maximize => Icon::Maximize,
        TitleButton::Close => Icon::Close,
    }
}

/// 六个按钮的命中矩形:`WINDOW_BTN` 整块从标题栏右缘连续向左排,垂直
/// 居中,无间隙(VS Code / Chrome 同款)。绘制与测试共用本函数。
pub fn button_rects(bar: Rect) -> [Rect; 6] {
    let top = bar.center().y - crate::ui::tokens::WINDOW_BTN.y / 2.0;
    let mut rects = [Rect::NOTHING; 6];
    for (i, rect) in rects.iter_mut().enumerate() {
        let left = bar.right() - (TITLE_BUTTONS.len() - i) as f32 * crate::ui::tokens::WINDOW_BTN.x;
        *rect = Rect::from_min_size(Pos2::new(left, top), crate::ui::tokens::WINDOW_BTN);
    }
    rects
}

/// 屏幕四周的八个缩放命中区:四边 6px 条 + 四角 12px 块(角块盖住边条,
/// 同一命中点只归属一个方向)。`from_two_pos` 自带归一,窗口极小不翻转。
pub fn edge_zones(screen: Rect) -> [(ResizeDirection, Rect); 8] {
    let (l, t, r, b) = (screen.left(), screen.top(), screen.right(), screen.bottom());
    [
        (
            ResizeDirection::North,
            Rect::from_two_pos(
                Pos2::new(l + CORNER_T, t),
                Pos2::new(r - CORNER_T, t + EDGE_T),
            ),
        ),
        (
            ResizeDirection::South,
            Rect::from_two_pos(
                Pos2::new(l + CORNER_T, b - EDGE_T),
                Pos2::new(r - CORNER_T, b),
            ),
        ),
        (
            ResizeDirection::West,
            Rect::from_two_pos(
                Pos2::new(l, t + CORNER_T),
                Pos2::new(l + EDGE_T, b - CORNER_T),
            ),
        ),
        (
            ResizeDirection::East,
            Rect::from_two_pos(
                Pos2::new(r - EDGE_T, t + CORNER_T),
                Pos2::new(r, b - CORNER_T),
            ),
        ),
        (
            ResizeDirection::NorthWest,
            Rect::from_two_pos(Pos2::new(l, t), Pos2::new(l + CORNER_T, t + CORNER_T)),
        ),
        (
            ResizeDirection::NorthEast,
            Rect::from_two_pos(Pos2::new(r - CORNER_T, t), Pos2::new(r, t + CORNER_T)),
        ),
        (
            ResizeDirection::SouthWest,
            Rect::from_two_pos(Pos2::new(l, b - CORNER_T), Pos2::new(l + CORNER_T, b)),
        ),
        (
            ResizeDirection::SouthEast,
            Rect::from_two_pos(Pos2::new(r - CORNER_T, b - CORNER_T), Pos2::new(r, b)),
        ),
    ]
}

/// 缩放命中区对应的系统光标。
fn cursor_of(direction: ResizeDirection) -> CursorIcon {
    match direction {
        ResizeDirection::North => CursorIcon::ResizeNorth,
        ResizeDirection::South => CursorIcon::ResizeSouth,
        ResizeDirection::East => CursorIcon::ResizeEast,
        ResizeDirection::West => CursorIcon::ResizeWest,
        ResizeDirection::NorthEast => CursorIcon::ResizeNorthEast,
        ResizeDirection::SouthEast => CursorIcon::ResizeSouthEast,
        ResizeDirection::NorthWest => CursorIcon::ResizeNorthWest,
        ResizeDirection::SouthWest => CursorIcon::ResizeSouthWest,
    }
}

/// 屏幕四周的透明缩放命令区;命中即发 `BeginResize(方向)`。
///
/// **必须画在 `ui::layout::draw` 的最后,且不建独立 Area。** egui 0.36
/// 里各 Panel 与根 `Ui` 共用同一绘制层,而命中测试会把「更高层里靠近
/// 指针的 widget」所在层之外整体排除(`hit_test.rs` 的 included_layers
/// 启发式):zones 一旦放进 Foreground 层的 Area(无论共用一个还是八区
/// 各一个),panel 内容在离边缘一个命中半径之外的按钮点击也会被整体吞掉
/// (实测,见 `edge_zones_and_titlebar_coexist` 这条回归)。同一层内则按
/// 距离裁决、同距时后画者胜 —— zones 在最后分配,边缘 6/12px 内缩放
/// 手势稳定获胜,按钮等内容正常交互。`Sense::drag()`(纯拖拽感知):egui
/// 对只感知拖拽的 widget 按下当帧即判拖拽(interaction.rs),`BeginResize`
/// 因此是「按下即缩放」的原生手感,不必等位移越过点击容差。
pub fn edge_resize_zones(ui: &mut egui::Ui) {
    let screen = ui.ctx().input(|i| i.viewport_rect());
    for (direction, rect) in edge_zones(screen) {
        let response = ui
            .allocate_rect(rect, Sense::drag())
            .on_hover_cursor(cursor_of(direction));
        if response.drag_started_by(PointerButton::Primary) {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

/// 标题栏内容(挂在 `Panel::top("titlebar")` 内,定高 `TITLEBAR_H`,
/// panel frame 内边距须为 0,命中矩形才与右缘对齐)。
pub fn ui(ui: &mut egui::Ui, state: &State, outbox: &mut Vec<Message>) {
    let bar = ui.max_rect();
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

    // 背景:整条标题栏可拖拽 / 双击最大化。先铺背景,按钮后画 —— 同层
    // 命中测试后画者优先,按钮矩形天然盖过背景。click_and_drag 双感知时
    // egui 把「按下后位移越过点击容差(6px)」才判为拖拽,StartDrag 因此
    // 在真正开拖的那帧发出;静按压仍是 click,双击最大化由此可用。
    let background = ui.allocate_rect(bar, Sense::click_and_drag());
    if background.drag_started_by(PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    if background.double_clicked_by(PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }

    // 左段:应用图标 + 「LaterMD — 文档名*」。文案取 `window_title()`,
    // 与原生窗口标题同一数据源,不另立一份状态。
    if ui.is_rect_visible(bar) {
        let painter = ui.painter();
        let font = egui::TextStyle::Button.resolve(ui.style());
        let text_pos = Pos2::new(bar.left() + SPACE_SM + ICON_SM + SPACE_SM, bar.center().y);
        Icon::Files.draw(
            painter,
            Pos2::new(bar.left() + SPACE_SM + ICON_SM / 2.0, bar.center().y),
            ICON_SM,
            crate::ui::tokens::accent(ui),
        );
        painter.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            state.tabs.current().document.window_title(),
            font,
            ui.visuals().text_color(),
        );
    }

    let rects = button_rects(bar);
    let toggle_shortcut = state
        .keymap
        .get(crate::command::Command::ToggleSidebar)
        .map(|shortcut| ctx.format_shortcut(&shortcut.keyboard()));
    for (button, rect) in TITLE_BUTTONS.into_iter().zip(rects) {
        window_button(
            ui,
            &ctx,
            button,
            rect,
            maximized,
            state.layout.left,
            toggle_shortcut.as_deref(),
            outbox,
        );
    }
}

/// 单个窗口按钮:命中区整块 `WINDOW_BTN`,hover 浅底,关闭键 hover 用
/// 警示色;禅定键本棒占位禁用(无底色、点击不触发)。动作只发视口命令
/// 与 [`Message`],不在 UI 侧改状态。
#[allow(clippy::too_many_arguments)]
fn window_button(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    button: TitleButton,
    rect: Rect,
    maximized: bool,
    sidebar_open: bool,
    toggle_shortcut: Option<&str>,
    outbox: &mut Vec<Message>,
) {
    let enabled = button != TitleButton::Zen;
    let response = ui.allocate_rect(
        rect,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = response.hovered();

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if enabled && hovered {
            let fill = if button == TitleButton::Close {
                DANGER
            } else {
                ui.visuals().widgets.hovered.bg_fill
            };
            painter.rect_filled(rect, RADIUS_SM, fill);
        }
        let color = if !enabled {
            ui.visuals().weak_text_color()
        } else if button == TitleButton::Close && hovered {
            Color32::WHITE
        } else {
            ui.visuals().text_color()
        };
        icon_of(button, maximized).draw(painter, rect.center(), ICON, color);
    }

    // 悬浮提示在前(消费 response 返回同型值),动作判定在后。
    let response = match button {
        TitleButton::PanelLeft => {
            let action = if sidebar_open {
                "关闭左侧"
            } else {
                "打开左侧"
            };
            response.on_hover_text(match toggle_shortcut {
                Some(shortcut) => format!("{action}({shortcut})"),
                None => action.to_owned(),
            })
        }
        TitleButton::PanelRight => response.on_hover_text("关闭右侧预览"),
        TitleButton::Zen => response.on_hover_text("禅定模式(即将推出)"),
        TitleButton::Minimize => response.on_hover_text("最小化"),
        TitleButton::Maximize => {
            response.on_hover_text(if maximized { "还原" } else { "最大化" })
        }
        TitleButton::Close => response.on_hover_text("关闭"),
    };
    match button {
        TitleButton::PanelLeft if response.clicked() => outbox.push(Message::SidebarToggled),
        TitleButton::PanelRight if response.clicked() => outbox.push(Message::RightPanelToggled),
        TitleButton::Minimize if response.clicked() => {
            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
        }
        TitleButton::Maximize if response.clicked() => {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }
        TitleButton::Close if response.clicked() => {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use egui::{Event, RawInput, ViewportCommand};

    const SCREEN: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(900.0, 600.0));

    fn click(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// 跑一帧标题栏,返回视口命令(标题命令不在本模块,应为空)。
    fn frame(
        ctx: &egui::Context,
        state: &State,
        outbox: &mut Vec<Message>,
        events: Vec<Event>,
    ) -> Vec<ViewportCommand> {
        let output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            |ui| super::ui(ui, state, outbox),
        );
        let commands = output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .cloned()
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        commands
    }

    /// 跑一帧边缘命令区,返回视口命令。
    fn zone_frame(ctx: &egui::Context, events: Vec<Event>) -> Vec<ViewportCommand> {
        let output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            edge_resize_zones,
        );
        let commands = output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .cloned()
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        commands
    }

    /// 复刻 `draw` 的骨架顺序跑一帧:标题栏 panel 在前、边缘命令区最后
    /// (与生产同序),返回该帧是否产出 `SidebarToggled`。
    fn shell_frame(ctx: &egui::Context, events: Vec<Event>) -> (bool, Vec<ViewportCommand>) {
        let state = State::default();
        let mut outbox = Vec::new();
        let output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(SCREEN),
                ..Default::default()
            },
            |ui| {
                egui::Panel::top("titlebar")
                    .exact_size(crate::ui::tokens::TITLEBAR_H)
                    .frame(
                        egui::Frame::default()
                            .inner_margin(egui::Margin::ZERO)
                            .fill(ui.visuals().panel_fill),
                    )
                    .show(ui, |ui| super::ui(ui, &state, &mut outbox));
                edge_resize_zones(ui);
            },
        );
        let commands = output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .cloned()
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        (outbox.contains(&Message::SidebarToggled), commands)
    }

    /// 六按钮从右缘等宽连续排布:无重叠、右缘贴齐、垂直居中,顺序与
    /// `TITLE_BUTTONS` 一致(左 → 右)。
    #[test]
    fn button_rects_tile_from_the_right_edge() {
        let bar = Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 36.0));
        let rects = button_rects(bar);
        let btn = crate::ui::tokens::WINDOW_BTN;

        for (i, rect) in rects.iter().enumerate() {
            assert_eq!((rect.width(), rect.height()), (btn.x, btn.y));
            assert_eq!(
                rect.right(),
                bar.right() - (5 - i) as f32 * btn.x,
                "第 {i} 个按钮右缘"
            );
            assert_eq!(rect.center().y, bar.center().y, "垂直居中");
            if i > 0 {
                assert!(
                    rect.left() >= rects[i - 1].right(),
                    "按钮 {i} 与前一个不重叠(贴边允许)"
                );
            }
        }
        assert_eq!(rects[5].right(), bar.right(), "关闭键贴右缘");
        assert_eq!(rects[0].left(), bar.right() - 6.0 * btn.x, "左起第一个");
    }

    /// 最大化按钮的图标随窗口状态切换,其余按钮不受影响。
    #[test]
    fn maximize_icon_switches_with_window_state() {
        assert_eq!(icon_of(TitleButton::Maximize, false), Icon::Maximize);
        assert_eq!(icon_of(TitleButton::Maximize, true), Icon::Restore);
        for button in TITLE_BUTTONS
            .iter()
            .filter(|b| **b != TitleButton::Maximize)
        {
            assert_eq!(icon_of(*button, false), icon_of(*button, true));
        }
    }

    /// 八个命中区:边条 6px 厚且让开角块、角块 12px 见方且贴角、方向与
    /// 所在边一致、八区两两不重叠。
    #[test]
    fn edge_zones_geometry_and_directions() {
        let zones = edge_zones(SCREEN);
        assert_eq!(zones.len(), 8);

        let zone = |direction| zones.iter().find(|(d, _)| *d == direction).unwrap().1;
        // 四角块:12×12 贴角
        assert_eq!(
            zone(ResizeDirection::NorthWest),
            Rect::from_min_size(Pos2::ZERO, egui::vec2(CORNER_T, CORNER_T))
        );
        assert_eq!(
            zone(ResizeDirection::SouthEast),
            Rect::from_min_size(
                Pos2::new(SCREEN.right() - CORNER_T, SCREEN.bottom() - CORNER_T),
                egui::vec2(CORNER_T, CORNER_T)
            )
        );
        // 北边条:6px 厚,水平让开两个角块
        let north = zone(ResizeDirection::North);
        assert_eq!(north.height(), EDGE_T);
        assert_eq!(north.left(), CORNER_T);
        assert_eq!(north.right(), SCREEN.right() - CORNER_T);
        // 东边条:6px 厚,垂直让开两个角块
        let east = zone(ResizeDirection::East);
        assert_eq!(east.width(), EDGE_T);
        assert_eq!(east.top(), CORNER_T);
        assert_eq!(east.bottom(), SCREEN.bottom() - CORNER_T);
        // 八区两两不重叠(贴边允许):同一命中点只归属一个方向
        let overlaps = |a: Rect, b: Rect| {
            a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
        };
        for (i, (_, a)) in zones.iter().enumerate() {
            for (_, b) in zones.iter().skip(i + 1) {
                assert!(!overlaps(*a, *b), "{a:?} 与 {b:?} 内部不应重叠");
            }
        }
    }

    /// 无头跑完整标题栏:按下标题区发 `StartDrag`,双击发 `Maximized`,
    /// 三个窗口按钮发 `Minimized` / `Maximized` / `Close`;左/右栏钮发
    /// 消息(归约在 `State::apply`),禅定占位不触发。
    #[test]
    fn titlebar_interactions_send_commands_and_messages() {
        let ctx = egui::Context::default();
        let state = State::default();
        let mut outbox = Vec::new();

        // 纯渲染帧:除首帧 egui 自发的主题同步外,不发任何视口命令
        let first = frame(&ctx, &state, &mut outbox, Vec::new());
        assert!(
            first
                .iter()
                .all(|c| matches!(c, ViewportCommand::SetTheme(_))),
            "{first:?}"
        );

        // 拖拽:标题区(避开右端按钮排)按下并拖过点击容差(6px)后,
        // 开拖帧发 StartDrag;静按压仍是 click(双击最大化的前提)
        let grab = Pos2::new(SCREEN.center().x, 18.0);
        frame(&ctx, &state, &mut outbox, vec![Event::PointerMoved(grab)]);
        frame(&ctx, &state, &mut outbox, vec![click(grab, true)]);
        let commands = frame(
            &ctx,
            &state,
            &mut outbox,
            vec![Event::PointerMoved(grab + egui::vec2(12.0, 0.0))],
        );
        assert!(
            commands.contains(&ViewportCommand::StartDrag),
            "{commands:?}"
        );
        frame(
            &ctx,
            &state,
            &mut outbox,
            vec![click(grab + egui::vec2(12.0, 0.0), false)],
        );

        // 双击:快速两击,第二击的抬起帧发 Maximized(true)(无头视口默认
        // 非最大化;帧行间隔 1/60s,远小于双击判定窗口)
        let mut double_click_commands = Vec::new();
        for pressed in [true, false, true, false] {
            double_click_commands.extend(frame(
                &ctx,
                &state,
                &mut outbox,
                vec![click(grab, pressed)],
            ));
        }
        assert!(
            double_click_commands.contains(&ViewportCommand::Maximized(true)),
            "双击应发 Maximized(true),实际 {double_click_commands:?}"
        );

        // 六按钮逐个点击:命中矩形即纯函数给出的划分(绘制同源)
        let rects = button_rects(SCREEN);
        let press = |i: usize, state: &State, outbox: &mut Vec<Message>| {
            let center = rects[i].center();
            frame(&ctx, state, outbox, vec![Event::PointerMoved(center)]);
            frame(&ctx, state, outbox, vec![click(center, true)]);
            frame(&ctx, state, outbox, vec![click(center, false)])
        };

        press(0, &state, &mut outbox); // 关闭左栏 → 消息
        assert_eq!(outbox, vec![Message::SidebarToggled]);
        press(1, &state, &mut outbox); // 关闭右栏 → 消息
        assert_eq!(
            outbox,
            vec![Message::SidebarToggled, Message::RightPanelToggled]
        );
        let commands = press(2, &state, &mut outbox); // 禅定:禁用占位
        assert!(
            outbox.len() == 2 && commands.is_empty(),
            "禁用按钮不产消息/命令"
        );
        assert!(press(3, &state, &mut outbox).contains(&ViewportCommand::Minimized(true)));
        assert!(press(4, &state, &mut outbox).contains(&ViewportCommand::Maximized(true)));
        assert!(press(5, &state, &mut outbox).contains(&ViewportCommand::Close));
    }

    /// 八个边缘命令区:命中即发对应方向的 BeginResize。
    #[test]
    fn edge_zone_presses_send_begin_resize() {
        let ctx = egui::Context::default();
        // 热身一帧:Area 首遍为 sizing pass,widget 不参与命中测试
        zone_frame(&ctx, Vec::new());
        for (direction, rect) in edge_zones(SCREEN) {
            let center = rect.center();
            zone_frame(&ctx, vec![Event::PointerMoved(center)]);
            let commands = zone_frame(&ctx, vec![click(center, true)]);
            assert!(
                commands.contains(&ViewportCommand::BeginResize(direction)),
                "{direction:?} 命中区按下即发 BeginResize,实际 {commands:?}"
            );
            zone_frame(&ctx, vec![click(center, false)]);
        }
    }

    /// **跨层命中回归**(zones 曾放 Foreground 层 Area,把命中半径之外
    /// 的 panel 按钮点击整体吞掉):按生产顺序同帧渲染标题栏 panel + 边缘
    /// 命令区,按钮点击必须产消息、边缘按下必须发 BeginResize —— 两类
    /// 交互共存。
    #[test]
    fn edge_zones_and_titlebar_coexist() {
        let ctx = egui::Context::default();
        // 帧序与点击协议同 menubar 测试:先移动、再按下、后抬起
        let bar = Rect::from_min_max(Pos2::ZERO, Pos2::new(900.0, crate::ui::tokens::TITLEBAR_H));
        let button_center = button_rects(bar)[0].center();
        shell_frame(&ctx, Vec::new());
        shell_frame(&ctx, vec![Event::PointerMoved(button_center)]);
        shell_frame(&ctx, vec![click(button_center, true)]);
        let (clicked, _) = shell_frame(&ctx, vec![click(button_center, false)]);
        assert!(clicked, "边缘命令区不得吞掉标题栏按钮的点击");

        // 换一个 ctx 只点边缘:北边条中心(x 让开角块),按下当帧即发
        // BeginResize(North)
        let ctx = egui::Context::default();
        let north_center = egui::pos2(SCREEN.center().x, EDGE_T / 2.0);
        shell_frame(&ctx, Vec::new());
        shell_frame(&ctx, vec![Event::PointerMoved(north_center)]);
        let (_, commands) = shell_frame(&ctx, vec![click(north_center, true)]);
        assert!(
            commands.contains(&ViewportCommand::BeginResize(ResizeDirection::North)),
            "北边条按下应发 BeginResize(North),实际 {commands:?}"
        );
    }
}
