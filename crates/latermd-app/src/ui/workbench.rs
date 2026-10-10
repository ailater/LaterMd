//! macOS 工作台外壳。系统负责窗口按钮,应用负责导航与文档工具。

use crate::command::Command;
use crate::settings::SettingsTab;
use crate::state::{Message, SidebarTab, State};
use crate::ui::{icons::Icon, tokens};
use eframe::egui::{self, Align2, Rect, Sense};

pub const HEADER_H: f32 = 52.0;
const TRAFFIC_LIGHTS_W: f32 = 84.0;
const HEADER_RIGHT_W: f32 = 344.0;

fn action(ui: &mut egui::Ui, rect: Rect, icon: Icon, tip: &str, selected: bool) -> bool {
    let response = ui.allocate_rect(rect, Sense::click());
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 6.0, colors.hover);
    }
    icon.draw(ui.painter(), rect.center(), tokens::ICON, colors.secondary);
    response.on_hover_text(tip).clicked()
}

/// 空白处可拖窗;所有命令仍走既有消息归约,不抢编辑器焦点。
pub fn header(ui: &mut egui::Ui, state: &mut State, outbox: &mut Vec<Message>) {
    let bar = ui.max_rect();
    // 左上角完全让给 AppKit 的真实交通灯。
    let drag_rect = Rect::from_min_max(
        bar.left_top() + egui::vec2(TRAFFIC_LIGHTS_W, 0.0),
        bar.right_bottom(),
    );
    let drag = ui.allocate_rect(drag_rect, Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    let slot = |x: f32, w: f32| {
        Rect::from_center_size(egui::pos2(x + w / 2.0, bar.center().y), egui::vec2(w, 28.0))
    };
    let right = bar.right() - 12.0;
    let right_start = right - HEADER_RIGHT_W;

    let sidebar_x = bar.left() + TRAFFIC_LIGHTS_W + 10.0;
    if action(
        ui,
        slot(sidebar_x, 28.0),
        Icon::Sidebar,
        "显示 / 隐藏侧栏",
        state.layout.left,
    ) {
        outbox.push(Command::ToggleSidebar.message());
    }
    let title_rect = Rect::from_min_max(
        egui::pos2(sidebar_x + 48.0, bar.top()),
        egui::pos2(right_start - 16.0, bar.bottom()),
    );
    let mut font = egui::FontId::proportional(13.0);
    font.family = crate::fonts::semibold_family(ui.ctx());
    let title = crate::ui::tabs::elide_text(
        ui,
        &state.tabs.current().display_name(),
        &font,
        title_rect.width(),
    );
    ui.painter().with_clip_rect(title_rect).text(
        title_rect.left_center(),
        Align2::LEFT_CENTER,
        title,
        font,
        ui.visuals().text_color(),
    );

    crate::ui::titlebar::view_switch(ui, slot(right - 344.0, 88.0), state, outbox);
    crate::ui::titlebar::search_capsule(ui, slot(right - 244.0, 172.0), &mut state.search, outbox);
    if action(
        ui,
        slot(right - 64.0, 28.0),
        Icon::PanelRight,
        "显示 / 隐藏预览",
        state.layout.right,
    ) {
        outbox.push(Command::ToggleRightPreview.message());
    }
    if action(ui, slot(right - 28.0, 28.0), Icon::Settings, "设置", false) {
        outbox.push(Message::SettingsOpened(SettingsTab::Appearance));
    }
}

/// 返回导航占用的矩形,内容面板接在下方并独立滚动。
pub fn navigation(ui: &mut egui::Ui, active: SidebarTab, outbox: &mut Vec<Message>) -> Rect {
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("工作台")
            .size(11.0)
            .color(crate::theme::shell_tokens(ui.visuals().dark_mode).secondary),
    );
    ui.add_space(4.0);
    let top = ui.cursor().top();
    for tab in SidebarTab::ALL {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), Sense::click());
        let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
        if tab == active || response.hovered() {
            ui.painter().rect_filled(
                rect,
                6.0,
                if tab == active {
                    colors.selected_bg
                } else {
                    colors.hover
                },
            );
        }
        tab.icon().draw(
            ui.painter(),
            egui::pos2(rect.left() + 15.0, rect.center().y),
            15.0,
            if tab == active {
                colors.accent
            } else {
                colors.secondary
            },
        );
        ui.painter().text(
            egui::pos2(rect.left() + 32.0, rect.center().y),
            Align2::LEFT_CENTER,
            if tab == SidebarTab::Backlinks {
                "反向链接"
            } else {
                tab.label()
            },
            egui::FontId::proportional(13.0),
            colors.text,
        );
        if response.clicked() {
            outbox.push(Message::SidebarTabChanged(tab));
        }
    }
    let nav = Rect::from_min_max(
        egui::pos2(ui.max_rect().left(), top),
        ui.min_rect().right_bottom(),
    );
    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);
    nav
}

pub fn pane_heading(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(12.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(detail).size(11.0).weak());
        });
    });
    ui.add_space(8.0);
}
