//! LaterMD 应用入口 —— M0 技术验证载体。
//!
//! 渲染后端选择见 docs/adr-002 §3.4:默认 wgpu;`LATERMD_RENDERER=glow` 仅在
//! 启用 `glow` feature 的构建中生效(驱动黑名单逃生口,不进主产物)。
//! App trait 采用 egui 0.36 的 `logic` / `ui` 二分(见 docs/adr-005 §2.3)。

use eframe::egui;

fn main() -> eframe::Result<()> {
    let renderer = match std::env::var("LATERMD_RENDERER").as_deref() {
        #[cfg(feature = "glow")]
        Ok("glow") => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let opts = eframe::NativeOptions { renderer, ..Default::default() };
    eframe::run_native("LaterMD", opts, Box::new(|_cc| Ok(Box::new(LaterMdApp::default()))))
}

/// 应用根状态。P0 阶段将承载 `State`/`Message` 归约(docs/adr-005 §5)。
#[derive(Default)]
struct LaterMdApp {
    frame_count: u64,
}

impl eframe::App for LaterMdApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 只做状态归约,严格禁止在此绘制任何 UI(docs/adr-005 §2.3)。
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frame_count += 1;
        let backend = if frame.wgpu_render_state().is_some() { "wgpu" } else { "glow" };
        ui.heading("LaterMD");
        ui.label(format!("M0 骨架运行中 · 第 {} 帧", self.frame_count));
        ui.label(format!("渲染后端: {backend}"));
    }
}
