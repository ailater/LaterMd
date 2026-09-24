//! 侧边栏:页签栏 + 当前页签内容(目前全部占位)。

use crate::state::{Message, SidebarTab};
use eframe::egui;

/// 绘制侧边栏内容。
///
/// `active_tab` 与 `outbox` 是从 `App` 上解构出的不相交借用,使本函数可以与
/// `show_collapsible` 原地持有的 `&mut visible` 并存(见 `layout.rs`)。
pub fn ui(panel: &mut egui::Ui, active_tab: &mut SidebarTab, outbox: &mut Vec<Message>) {
    tab_bar(panel, active_tab, outbox);
    panel.add_space(4.0);
    match *active_tab {
        SidebarTab::Files => placeholder(panel, "文件树占位(P0 文件树基础版)"),
        SidebarTab::Search => placeholder(panel, "搜索占位(P1)"),
        SidebarTab::Outline => placeholder(panel, "大纲占位(P0 廉价版)"),
    }
}

/// 页签栏。点击只发消息,归约在下一帧 `App::logic` 完成。
fn tab_bar(panel: &mut egui::Ui, active_tab: &mut SidebarTab, outbox: &mut Vec<Message>) {
    panel.horizontal(|ui| {
        for tab in SidebarTab::ALL {
            if ui
                .selectable_label(*active_tab == tab, tab.label())
                .clicked()
            {
                outbox.push(Message::SidebarTabChanged(tab));
            }
        }
    });
}

fn placeholder(panel: &mut egui::Ui, text: &str) {
    panel.weak(text);
}
