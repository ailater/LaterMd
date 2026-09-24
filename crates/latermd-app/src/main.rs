//! LaterMD 应用入口 —— M0 技术验证载体。
//!
//! 渲染后端选择见 docs/adr-002 §3.4:默认 wgpu;`LATERMD_RENDERER=glow` 仅在
//! 启用 `glow` feature 的构建中生效(驱动黑名单逃生口,不进主产物)。
//! App trait 采用 egui 0.36 的 `logic` / `ui` 二分(见 docs/adr-005 §2.3)。

mod fonts;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let renderer = match std::env::var("LATERMD_RENDERER").as_deref() {
        #[cfg(feature = "glow")]
        Ok("glow") => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let opts = eframe::NativeOptions {
        renderer,
        ..Default::default()
    };
    eframe::run_native(
        "LaterMD",
        opts,
        Box::new(|cc| {
            let font_source = fonts::install(&cc.egui_ctx);
            Ok(Box::new(LaterMdApp::new(font_source)))
        }),
    )
}

/// 应用根状态。P0 阶段将承载 `State`/`Message` 归约(docs/adr-005 §5)。
struct LaterMdApp {
    frame_count: u64,
    font_source: Option<String>,
}

impl LaterMdApp {
    fn new(font_source: Option<String>) -> Self {
        Self {
            frame_count: 0,
            font_source,
        }
    }
}

impl eframe::App for LaterMdApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 只做状态归约,严格禁止在此绘制任何 UI(docs/adr-005 §2.3)。
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.frame_count += 1;
        let backend = if frame.wgpu_render_state().is_some() {
            "wgpu"
        } else {
            "glow"
        };
        ui.heading("LaterMD · M0 技术验证");
        ui.label(format!(
            "骨架运行中 · 第 {} 帧 · 渲染后端: {backend}",
            self.frame_count
        ));
        if let Some(render_state) = frame.wgpu_render_state() {
            // M0 验证 3「报告合理 adapter」的在屏证据:选中项 + loader 枚举全集
            ui.label(format!(
                "wgpu adapter: {:?}",
                render_state.adapter.get_info()
            ));
            ui.label(format!(
                "available: {:?}",
                render_state
                    .available_adapters
                    .iter()
                    .map(|a| a.get_info())
                    .collect::<Vec<_>>()
            ));
        }
        match &self.font_source {
            Some(source) => ui.label(format!("中文字体来源: {source}")),
            None => ui.label("⚠ 未找到候选 CJK 字体,中文将显示为方块"),
        };
        // SC 与 JP 字形有别的样本字(骨/直/關),用于确认加载的是 SC 字型而非 JP
        ui.label("中文渲染(比例):雾凇沆砀,天与云与山与水,上下一白 —— 骨直關开办");
        ui.monospace("中文渲染(等宽): fn 骨直關() { 雾凇沆砀 } // abc123");
    }
}
