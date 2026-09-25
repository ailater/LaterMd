//! LaterMD 应用入口。
//!
//! 渲染后端选择见 docs/adr-002 §3.4:默认 wgpu;`LATERMD_RENDERER=glow` 仅在
//! 启用 `glow` feature 的构建中生效(驱动黑名单逃生口,不进主产物)。
//! App trait 采用 egui 0.36 的 `logic` / `ui` 二分,三栏布局见 `ui::layout`
//! (docs/adr-005)。

mod ai;
mod command;
mod export;
mod file;
mod filetree;
mod fonts;
mod search;
mod state;
mod theme;
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
            let theme = theme::ThemeSettings::load();
            // 首帧前装好主题,避免开场按默认深色闪一帧;此后每次切换由
            // `App::logic` 的投影维持
            theme.apply(&cc.egui_ctx);
            // 文件树设置(上次根目录 + 最近列表)同样启动即恢复
            let file_tree = filetree::FileTreeSettings::load();
            Ok(Box::new(LaterMdApp::new(theme, file_tree)))
        }),
    )
}

/// Settings 面板显示的当前渲染后端(AGENTS.md §5)。只镜像 `main` 的启动
/// 选择,同一套规则:仅 `glow` feature 构建且变量精确为 `glow` 才是 glow,
/// 未设/值不认/feature 未开一律 wgpu。`env` 由调用方传入以便无头测试。
pub(crate) fn renderer_label(env: Option<&str>) -> &'static str {
    match env {
        #[cfg(feature = "glow")]
        Some("glow") => "glow",
        _ => "wgpu",
    }
}

/// 应用根:状态 + 待归约消息队列。归约在 [`eframe::App::logic`],绘制在
/// [`eframe::App::ui`]。后台任务通道(P1,docs/adr-005 §5.2)将来汇入同一队列。
#[derive(Default)]
struct LaterMdApp {
    state: state::State,
    outbox: Vec<state::Message>,
    /// 最近一次下发给原生窗口的标题缓存;仅用于跳过重复的 set_title。
    window_title: String,
}

impl LaterMdApp {
    /// 以启动时装载的主题与文件树设置建应用(重启保持)。`Default` 恒为
    /// 深色且不走磁盘,仅供测试。
    fn new(theme: theme::ThemeSettings, file_tree: filetree::FileTreeSettings) -> Self {
        let mut app = Self::default();
        app.state.theme = theme;
        app.state.file_tree = file_tree.into();
        app
    }
}

#[cfg(test)]
mod tests {
    use super::renderer_label;

    /// 判定与 `main` 的启动选择同规则:认不认 `glow` 取决于 glow feature
    /// (两条产线都会跑到对应分支);值精确匹配小写,大小写变体不认。
    #[test]
    fn renderer_label_follows_feature_and_exact_env_value() {
        #[cfg(feature = "glow")]
        assert_eq!(renderer_label(Some("glow")), "glow");
        #[cfg(not(feature = "glow"))]
        assert_eq!(renderer_label(Some("glow")), "wgpu");
        assert_eq!(renderer_label(Some("wgpu")), "wgpu");
        assert_eq!(renderer_label(Some("GLOW")), "wgpu");
        assert_eq!(renderer_label(None), "wgpu");
    }
}
