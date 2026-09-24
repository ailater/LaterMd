//! 三栏布局:侧边栏 / 编辑器 / 预览(docs/adr-005 §3.2)。
//!
//! 顺序铁律:panel 添加顺序决定嵌套,先加的最外层;`CentralPanel` 必须最后加。
//! `App::logic` 只归约状态,`App::ui` 只绘制,两者严格分离(铁律)。

use crate::state::SidebarState;
use crate::LaterMdApp;
use eframe::egui;

impl eframe::App for LaterMdApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 只做状态归约,严格禁止在此绘制任何 UI(docs/adr-005 §2.3)。
        let LaterMdApp { state, outbox, .. } = self;
        for message in std::mem::take(outbox) {
            state.apply(message);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ① 最外层:侧边栏(可折叠)。show_collapsible 原地持有 `&mut visible`,
        // 因此先把 sidebar 解构成 `visible` 与其余字段,闭包只捕获后者。
        let SidebarState {
            visible,
            active_tab,
        } = &mut self.state.sidebar;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .size_range(160.0..=400.0)
            .show_collapsible(ui, visible, |ui| {
                crate::ui::sidebar::ui(ui, active_tab, &mut self.outbox);
            });

        // ② 次外层:编辑器
        egui::Panel::left("editor")
            .resizable(true)
            .default_size(500.0)
            .show(ui, editor_ui);

        // ③ 必须最后:预览
        egui::CentralPanel::default().show(ui, preview_ui);
    }
}

/// 编辑器面板占位,双栏源码编辑的接入点(roadmap P0「编辑器」)。
fn editor_ui(ui: &mut egui::Ui) {
    ui.weak("编辑器占位(待接入:P0 编辑器模块)");
}

/// 预览面板占位,vendored 渲染层的接入点。
fn preview_ui(ui: &mut egui::Ui) {
    ui.weak("预览占位(待接入:vendored 渲染层)");
}
