//! LaterMD 应用入口。
//!
//! 渲染后端选择见 docs/adr-002 §3.4:默认 wgpu;`LATERMD_RENDERER=glow` 仅在
//! 启用 `glow` feature 的构建中生效(驱动黑名单逃生口,不进主产物)。
//! App trait 采用 egui 0.36 的 `logic` / `ui` 二分,三栏布局见 `ui::layout`
//! (docs/adr-005)。

// Windows 发布版按 GUI 子系统链接,双击 exe 不再弹控制台黑框;debug 构建
// 保留控制台,stdout/stderr 可见。MCP stdio 通道不受影响:客户端以管道
// 方式拉起子进程,GUI 子系统下重定向的 stdio 句柄照常可读写。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod ai_config;
mod ai_key;
mod ai_link;
mod command;
mod export;
mod file;
mod filetree;
mod fonts;
mod git_diff;
mod git_panel;
mod keymap;
mod live;
mod mcp;
mod search;
mod settings;
mod state;
mod tabs;
mod theme;
mod ui;

use eframe::egui;
use latermd_mcp::{McpConfig, Server};
use std::path::PathBuf;

/// MCP stdio 通道的开关参数:`latermd --mcp-stdio`(docs/mcp-plan.md §2)。
pub(crate) const MCP_STDIO_ARG: &str = "--mcp-stdio";

/// headless MCP 通道:`latermd --mcp-stdio`,客户端以子进程方式拉起
/// (`claude mcp add latermd -- latermd --mcp-stdio`)。
///
/// 不进 GUI 事件循环、不初始化窗口;stdin 关闭即退出。**不看 `mcp.json`
/// 的 enabled** —— 用户显式用这个参数启动就是一次授权,而 `enabled` 管的是
/// 「GUI 进程内是否自动监听端口」这一件不同的事。
///
/// 文档库根取环境变量 `LATERMD_MCP_ROOT`(headless 没有文件树 UI 可交互);
/// 缺省时工具照常返回「未设置文件树根目录」。
fn run_mcp_stdio() -> eframe::Result<()> {
    let config = theme::config_dir()
        .as_deref()
        .map(McpConfig::load_from)
        .unwrap_or_default();
    let root = std::env::var("LATERMD_MCP_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);
    let server = Server::new(root, config);
    match latermd_mcp::transport::stdio::serve(&server) {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("LaterMD: MCP stdio 通道退出: {error}");
            Err(eframe::Error::AppCreation(Box::new(error)))
        }
    }
}

fn main() -> eframe::Result<()> {
    // 子进程式 MCP 通道优先:它不是 GUI 会话,不该初始化窗口与字体
    if std::env::args().any(|arg| arg == MCP_STDIO_ARG) {
        return run_mcp_stdio();
    }
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
            // `App::logic` 的投影维持。「跟随系统」在这里先探测一次,首帧
            // 就不是靠 fallback 猜的。
            let system = theme::detect_system_mode();
            theme.apply(
                &cc.egui_ctx,
                theme
                    .mode
                    .resolve(system, system.unwrap_or(theme::ThemeMode::Dark)),
            );
            // 文件树设置(上次根目录 + 最近列表)同样启动即恢复
            let file_tree = filetree::FileTreeSettings::load();
            let mut app = LaterMdApp::new(theme, file_tree);
            app.state.system_theme = system;
            app.state.system_theme_ok = system.is_some();
            Ok(Box::new(app))
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
        // 凭据状态启动即探测:设置浮窗状态行首见即真(后端不可用的
        // Linux 环境直接给回退提示,而不是「未配置」的误报)
        // 先探测凭据后端,再装载 AI 配置 —— 装配 provider 运行时要读 key
        app.state.ai_key.probe();
        app.state.load_preferences();
        app
    }
}

#[cfg(test)]
mod tests {
    use super::{renderer_label, MCP_STDIO_ARG};

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

    /// `--mcp-stdio` 只在精确匹配时生效(其它参数照常进 GUI);参数常量与
    /// main 的判定同源,避免两边写死两份字符串。
    #[test]
    fn mcp_stdio_arg_is_matched_exactly() {
        let args = |args: &[&str]| {
            args.iter()
                .map(|arg| (*arg).to_owned())
                .any(|arg| arg == MCP_STDIO_ARG)
        };
        assert!(args(&["latermd", "--mcp-stdio"]));
        assert!(!args(&["latermd"]));
        assert!(!args(&["latermd", "--mcp-stdio=1"]), "精确匹配");
    }
}
