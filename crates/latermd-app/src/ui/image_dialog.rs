//! 「图片框」对话框(docs/image-plan.md A 段 + B 段本地文件来源)。
//!
//! ## 分工
//!
//! 本模块**只收输入、只发消息**:alt 与 url 两个草稿字符串归 UI 原地持有
//! (和 `SettingsState` 持草稿同款 —— `TextEdit` 是立即模式控件,草稿必须
//! 能就地 `&mut`),插入文本的归约在 `state`(走 `compose::insert_image`),
//! 本地文件的复制与地址回填同样在归约(`Message::ImageFilePickRequested`)。
//!
//! ## 为什么它是唯一「点了不改文档」的工具条动作
//!
//! `![alt](url)` 要两个输入,没法像加粗那样从选区推断 —— 富文本工具条里
//! 其余十六个动作都能从「当前选区 + 一个标记」推出结果,只有图片不行。
//! 因此它的按钮与快捷键都只开对话框。
//!
//! ## 三条来源共用这一个出口
//!
//! B 段(本地文件)先以「浏览…」按钮落最简形态:选中即复制进
//! `<doc名>.assets/` 并把相对地址回填 url 栏(网络地址仍可直接手填);
//! C 段(图床上传)再扩成来源页签。插入动作始终是同一个
//! `Message::ImageInserted{ alt, url }` —— 汇流点只有一个,单测才钉得住。

use eframe::egui;

/// 图片框的草稿状态(归约置 `open`,UI 改 `alt` / `url`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageDialogState {
    /// 对话框是否可见。
    pub open: bool,
    /// 替代文字草稿;开对话框时按选区预填,浏览本地文件时空则补文件名。
    pub alt: String,
    /// 图片地址草稿;手填网络地址,或由「浏览…」回填复制后的相对地址。
    pub url: String,
}

/// 画对话框,返回**(插入, 浏览, 取消)**三个按钮的响应,测试定位用(与
/// `ui::layout::commit_dialog` 同款手法)。
///
/// 地址为空时禁用「插入」:`![](url)` 里 url 是唯一的实质内容,空着插进去
/// 只会在预览里留一个破图占位。alt 允许为空 —— 装饰性图片的 alt 本来就
/// 该空着(无障碍规范)。「浏览」复制的是**文件本体**,url 栏回填后用户
/// 仍可改(比如手改 alt 再插)。
pub fn dialog(
    ui: &mut egui::Ui,
    draft: &mut ImageDialogState,
) -> (egui::Response, egui::Response, egui::Response) {
    let mut browse = None;
    let mut buttons = None;
    egui::Window::new("插入图片")
        // 固定初始位置:浮窗出现位置可预期(不与菜单栏重叠),拖动后由
        // Area 记忆保持;显式初始位也让无头测试的帧间位置稳定。
        .default_pos([80.0, 120.0])
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("替代文字(alt)");
            ui.text_edit_singleline(&mut draft.alt)
                .on_hover_text("图片加载失败时显示的文字;装饰性图片可留空");
            ui.label("图片地址");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut draft.url).on_hover_text(
                    "网络地址直接填;或点「浏览…」选本地图片,复制到文档旁的 .assets/ 目录后自动回填相对地址",
                );
                browse = Some(
                    ui.button("浏览…").on_hover_text(
                        "选本地图片,复制到 <doc名>.assets/ 目录(需要文档已保存)",
                    ),
                );
            });
            ui.horizontal(|ui| {
                let ready = !draft.url.trim().is_empty();
                let insert = ui.add_enabled(ready, egui::Button::new("插入"));
                let cancel = ui.button("取消");
                buttons = Some((insert, cancel));
            });
        });
    let (insert, cancel) = buttons.expect("浮窗必然绘制插入/取消按钮");
    (insert, browse.expect("浮窗必然绘制浏览按钮"), cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 渲染不 panic(空草稿 / 已填草稿两态),三个按钮的可用性与既有约定
    /// 一致:地址空禁用插入,取消与浏览恒可点。
    #[test]
    fn dialog_renders_in_both_draft_states() {
        // 浮窗首帧只完成注册、按钮响应还未参与命中(与 ui::layout 点击测试
        // 的「sizing pass」同一节奏),断言落在稳定的第三帧。
        for draft in [
            ImageDialogState::default(),
            ImageDialogState {
                open: true,
                alt: "示意图".to_owned(),
                url: "https://x/y.png".to_owned(),
            },
        ] {
            let mut draft = draft;
            let ctx = egui::Context::default();
            let mut seen = None;
            for _ in 0..3 {
                let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    let (insert, browse, cancel) = dialog(ui, &mut draft);
                    seen = Some((insert.enabled(), browse.enabled(), cancel.enabled()));
                });
                output.drop_without_applying_deltas();
            }
            let (insert_enabled, browse_enabled, cancel_enabled) = seen.expect("至少跑了一帧");
            // 地址空 → 插入按钮被禁用(不产出可点的响应)
            assert_eq!(
                insert_enabled,
                !draft.url.trim().is_empty(),
                "地址为空时禁用插入"
            );
            assert!(browse_enabled, "浏览恒可点(禁用与否由归约侧提示)");
            assert!(cancel_enabled, "取消永远可点");
        }
    }
}
