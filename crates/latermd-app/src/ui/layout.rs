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

        // ② 次外层:编辑器。TextEdit 是立即模式控件,必须原地持有
        // `&mut` 缓冲,因此 editor 与 preview 快照的借用下放到本面板
        // 闭包内(归约/绘制二分对这对"控件附属状态"的例外见 state.rs)。
        let state = &mut self.state;
        egui::Panel::left("editor")
            .resizable(true)
            .default_size(500.0)
            .show(ui, |ui| {
                crate::ui::editor::ui(ui, &mut state.editor, &mut state.preview);
            });

        // ③ 必须最后:预览
        egui::CentralPanel::default().show(ui, |ui| {
            crate::ui::preview::ui(ui, &self.state.preview);
        });
    }
}
