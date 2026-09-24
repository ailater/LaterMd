//! LaterMD 应用入口。
//!
//! 渲染后端选择见 docs/adr-002 §3.4:默认 wgpu;`LATERMD_RENDERER=glow` 仅在
//! 启用 `glow` feature 的构建中生效(驱动黑名单逃生口,不进主产物)。
//! App trait 采用 egui 0.36 的 `logic` / `ui` 二分,三栏布局见 `ui::layout`
//! (docs/adr-005)。

mod fonts;
mod state;
mod ui;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let renderer = match std::env::var("LATERMD_RENDERER").as_deref() {
        #[cfg(feature = "glow")]
        Ok("glow") => eframe::Renderer::Glow,
        _ => eframe::Renderer::Wgpu,
    };
    let opts = eframe::NativeOptions {
        renderer,
        // 三栏的最小可用宽度:侧边栏下限 160 + 编辑器 500 + 预览余量(docs/adr-005)
        viewport: egui::ViewportBuilder::default().with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LaterMD",
        opts,
        Box::new(|cc| {
            if fonts::install(&cc.egui_ctx).is_none() {
                // M0 验证 UI 已退役,字体失配只在终端告警,不静默吞掉
                eprintln!("LaterMD: 未找到候选 CJK 字体,中文将显示为方块");
            }
            Ok(Box::new(LaterMdApp::default()))
        }),
    )
}

/// 应用根:状态 + 待归约消息队列。归约在 [`eframe::App::logic`],绘制在
/// [`eframe::App::ui`]。后台任务通道(P1,docs/adr-005 §5.2)将来汇入同一队列。
#[derive(Default)]
struct LaterMdApp {
    state: state::State,
    outbox: Vec<state::Message>,
}
