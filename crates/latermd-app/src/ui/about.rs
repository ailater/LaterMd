//! 「关于 LaterMD」对话框(#71 M1):帮助菜单「关于 LaterMD…」的落点。
//!
//! 内容五件:应用名 + 版本(`CARGO_PKG_VERSION`,workspace 单一事实源)+
//! 一句话定位(AGENTS.md §1)+ 仓库链接(egui `open_url` 交系统浏览器,
//! 与预览外链同路)+ MIT 许可证。观感复用 #70 设置弹窗基线:窗底显式取
//! 当前主题 shell 色、两列行走 `settings::settings_row`(标签列定宽)、
//! 间距用外壳 token,明暗主题各自成立。
//!
//! 关闭语义照既有浮窗:全窗蒙层点击关(快捷键蒙层同款 scrim)、标题栏 X
//! 关(egui `Window::open` 内建)、Esc 关(`ui::layout::reduce` 消费裸 Esc,
//! 润色确认浮窗同款)。三条出口都汇成 `crate::state::Message::AboutClosed`
//! 在归约落地 —— 本模块只绘制与发消息,不碰状态(铁律)。

use eframe::egui;

/// 仓库主页;链接点击经 egui `open_url` 打开。
pub const REPO_URL: &str = "https://github.com/ailater/LaterMd";
/// 仓库链接的显示文案(裸域名形,不带协议头,与浏览器地址栏习惯一致)。
pub const REPO_LABEL: &str = "github.com/ailater/LaterMd";

/// 一句话定位(AGENTS.md §1 产品定义的首句)。
pub const TAGLINE: &str = "跨平台、版本化、可对话、可演化的 Markdown 知识工作台";

/// 关于对话框状态:开/关全在归约(`Message::AboutOpened` /
/// `AboutClosed`),UI 只读。M2「检查更新」的在途/结果状态在此扩展。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AboutState {
    /// 对话框是否可见。
    pub open: bool,
}

/// 对话框最小宽:关于窗是只读速览卡,一行定位句 + 两列行在此宽内单行
/// 放下;非 resizable,实际宽 = max(此值, 内容宽)。
const ABOUT_MIN_W: f32 = 420.0;

/// 画关于窗,返回**是否请求关闭**(蒙层点击或标题栏 X)。调用方
/// (`ui::layout::draw_overlay_dialogs`)把它翻成
/// `crate::state::Message::AboutClosed` 发归约;本函数不改任何状态。
pub fn dialog(ui: &mut egui::Ui) -> bool {
    let ctx = ui.ctx().clone();
    let screen = ctx.viewport_rect();

    // 全窗蒙层(快捷键蒙层同款手法与色值):盖住下层、点击即关。先画,
    // 窗卡片后画盖在其上 —— 点卡片不误关,点卡片外的蒙层底才关。
    let mut scrim_clicked = false;
    egui::Area::new(egui::Id::new("about-scrim"))
        .interactable(true)
        .fixed_pos(egui::Pos2::ZERO)
        .show(&ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let tint = if dark {
                egui::Color32::from_black_alpha(crate::shortcut_overlay::SCRIM_ALPHA_DARK)
            } else {
                egui::Color32::from_white_alpha(crate::shortcut_overlay::SCRIM_ALPHA_LIGHT)
            };
            ui.painter()
                .rect_filled(screen, egui::CornerRadius::ZERO, tint);
            if ui.allocate_rect(screen, egui::Sense::click()).clicked() {
                scrim_clicked = true;
            }
        });

    let mut open = true;
    // 窗底显式取当前主题 shell 色(#70 M1 同款:不依赖投影也在场,明暗
    // 各走各的 token);圆角/阴影/内边距仍走 egui 出厂 window 档。
    let shell = crate::theme::shell_tokens(ui.visuals().dark_mode);
    egui::Window::new("关于 LaterMD")
        // 显式 id(设置窗同款):窗口拖动位置记忆与文案解耦
        .id(egui::Id::new("about-dialog"))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .min_width(ABOUT_MIN_W)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .frame(egui::Frame::window(ui.style()).fill(shell.content))
        .show(&ctx, |ui| {
            ui.add_space(crate::ui::tokens::SPACE_SM);
            // 应用名 + 版本居中成组(关于窗惯例:身份信息居中,数据行两列)
            ui.vertical_centered(|ui| {
                ui.heading("LaterMD");
                ui.label(format!("版本 {}", env!("CARGO_PKG_VERSION")));
            });
            ui.add_space(crate::ui::tokens::SPACE_SM);
            ui.label(TAGLINE);
            ui.add_space(crate::ui::tokens::SPACE_MD);
            ui.separator();
            ui.add_space(crate::ui::tokens::SPACE_SM);
            // 两列行复用设置页基线:标签列定宽,控件列同起点
            crate::settings::settings_row(ui, "仓库", |ui| {
                ui.hyperlink_to(REPO_LABEL, REPO_URL);
            });
            crate::settings::settings_row(ui, "许可协议", |ui| {
                ui.label("MIT License");
            });
            ui.add_space(crate::ui::tokens::SPACE_SM);
        });
    scrim_clicked || !open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Message, State};
    use egui::{Event, PointerButton, RawInput};
    use std::cell::Cell;

    /// 一帧的可见文本(shapes 的 galley 原文拼接;galley.job.text 恒是
    /// 完整原文,截断不影响断言)。egui 0.36 的 `Window` 首帧只注册
    /// Area、次帧才画内容(无头实测 shapes 首帧无文本),取证一律取
    /// 预热后的帧。
    fn dialog_frame_text(ctx: &egui::Context, events: Vec<Event>) -> String {
        let mut close = false;
        let output = ctx.run_ui(
            RawInput {
                events,
                ..Default::default()
            },
            |ui| close = dialog(ui),
        );
        let _ = close;
        let text = output
            .shapes
            .iter()
            .filter_map(|clipped| {
                let egui::epaint::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                Some(text.galley.job.text.clone())
            })
            .collect::<Vec<_>>()
            .join("\n");
        output.drop_without_applying_deltas();
        text
    }

    /// 五件内容全部渲染:应用名、版本号、一句话定位、仓库链接、MIT
    /// 许可证。版本号断言用编译期注入的 `CARGO_PKG_VERSION` —— 渲染
    /// 侧漏画版本行(或写死成别的)时这里红,非恒真。
    #[test]
    fn renders_name_version_tagline_repo_link_and_license() {
        for dark in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_theme(if dark {
                egui::Theme::Dark
            } else {
                egui::Theme::Light
            });
            // 首帧预热(Area 注册),次帧起有内容
            let _ = dialog_frame_text(&ctx, Vec::new());
            let text = dialog_frame_text(&ctx, Vec::new());
            assert!(text.contains("LaterMD"), "应用名在场(实际:{text:?})");
            assert!(
                text.contains(env!("CARGO_PKG_VERSION")),
                "版本号 {} 在场(实际:{text:?})",
                env!("CARGO_PKG_VERSION")
            );
            assert!(text.contains(TAGLINE), "一句话定位在场(实际:{text:?})");
            assert!(text.contains(REPO_LABEL), "仓库链接文案在场(实际:{text:?})");
            assert!(text.contains("MIT"), "MIT 许可证在场(实际:{text:?})");
            // 纯渲染不请求关闭(run_ui 交回 FullOutput,布尔经 Cell 带出;
            // 输出必须显式消费,字体纹理 delta 直接丢弃会 panic)
            let close = Cell::new(false);
            let output = ctx.run_ui(RawInput::default(), |ui| close.set(dialog(ui)));
            output.drop_without_applying_deltas();
            assert!(!close.get(), "纯渲染不请求关闭({dark:?} 主题)");
        }
    }

    /// 仓库链接点击走系统浏览器:egui `Hyperlink` 内建 `open_url`,断言
    /// 从 platform_output 的 `OpenUrl` 命令取证(live.rs 复制按钮同款)。
    #[test]
    fn repo_link_click_opens_url() {
        let ctx = egui::Context::default();
        // 首帧预热(窗 Area 注册),次帧定位链接文本矩形
        let _ = dialog_frame_text(&ctx, Vec::new());
        let output = ctx.run_ui(RawInput::default(), |ui| {
            dialog(ui);
        });
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        let rect = shapes
            .iter()
            .find_map(|clipped| {
                let egui::epaint::Shape::Text(text) = &clipped.shape else {
                    return None;
                };
                text.galley
                    .job
                    .text
                    .contains(REPO_LABEL)
                    .then(|| clipped.shape.visual_bounding_rect())
            })
            .expect("链接文本已渲染");

        let center = rect.center();
        let click = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(center), click(true), click(false)],
                ..Default::default()
            },
            |ui| {
                dialog(ui);
            },
        );
        let urls: Vec<String> = output
            .platform_output
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(
            urls,
            vec![REPO_URL.to_owned()],
            "点击仓库链接应经 open_url 打开 {REPO_URL}"
        );
    }

    /// 蒙层点击请求关闭:点窗卡片外的蒙层底(屏幕角落)→ `dialog` 返回
    /// true;不点击的帧返回 false。X 按钮是 egui `Window::open` 内建
    /// widget(无公开矩形探针,设置窗先例同样未无头点击),其关闭与蒙层
    /// 共用同一返回通道。
    #[test]
    fn scrim_click_requests_close() {
        let ctx = egui::Context::default();
        // 首帧预热(Area 注册),空帧:不关
        let close = Cell::new(false);
        let output = ctx.run_ui(RawInput::default(), |ui| close.set(dialog(ui)));
        output.drop_without_applying_deltas();
        assert!(!close.get(), "无交互不请求关闭");
        let close = Cell::new(false);
        let output = ctx.run_ui(RawInput::default(), |ui| close.set(dialog(ui)));
        output.drop_without_applying_deltas();
        assert!(!close.get(), "预热后的空帧同样不请求关闭");

        // 点屏幕角落(窗卡片之外):请求关闭
        let corner = ctx.viewport_rect().right_bottom() - egui::vec2(4.0, 4.0);
        let click = |pressed| Event::PointerButton {
            pos: corner,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let close = Cell::new(false);
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(corner), click(true), click(false)],
                ..Default::default()
            },
            |ui| close.set(dialog(ui)),
        );
        output.drop_without_applying_deltas();
        assert!(close.get(), "点击蒙层底应请求关闭");
    }

    /// 开/关归约:两条消息只翻 `about.open`,别的状态不动。
    #[test]
    fn opened_closed_messages_flip_about_state() {
        let mut state = State::default();
        assert!(!state.about.open, "出厂关");
        state.apply(Message::AboutOpened);
        assert!(state.about.open, "AboutOpened 归约打开");
        state.apply(Message::AboutClosed);
        assert!(!state.about.open, "AboutClosed 归约关闭");
        // 幂等方向:关着再关、开着再开不炸
        state.apply(Message::AboutClosed);
        state.apply(Message::AboutOpened);
        assert!(state.about.open);
    }
}
