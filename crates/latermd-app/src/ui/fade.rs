//! 自研轻量动效(docs/ui-modernization.md §3 U3):一个布尔目标的 alpha
//! 交叉淡化,替代 GPL 的 `egui_transition_animation`,零新增依赖。
//!
//! 基元是 egui 0.36 内建的 `Context::animate_bool_with_time`:target 翻转时
//! 从当前值向端点线性插值,进行中自动 `request_repaint`,无需手写帧驱动。
//! 时长与 `style.animation_time` 挂钩(可访问性总闸:用户调 0 即无动画,
//! 内建实现直接落端点且不产帧)。
//!
//! 落点只有两处,其余刻意不做(面板开合 egui 自带滑动、列表项级动效与
//! 禅定进出易晕收益低,见 decisions-pending #45):
//!
//! - **编辑/预览模式切换**:`ui::editor` 用 [`crossfade`] 把当前模式的整块
//!   内容从透明渐入,双向对称;
//! - **浮层**(设置 / 图片框 / 各确认模态):egui `Area` 的 `fade_in` 在
//!   0.36.2 **已内建**(containers/area.rs,时长即 `style.animation_time`,
//!   opacity < 1 时自动要帧),`egui::Window` 默认启用 —— LaterMD 侧零代码。

use eframe::egui;

/// 模式切换淡入的基准时长(秒,docs/ui-modernization.md §2.6:0.15s 就够,
/// 更长在文本编辑器里显得拖沓)。作为 `style.animation_time` 的上限参与
/// 计算:调 0 关动画,调得更小则尊重更快的偏好。
pub const FADE_S: f32 = 0.15;

/// 布尔目标的交叉淡化 alpha:target 翻转后在 [`FADE_S`] 内从旧端点线性
/// 走向新端点(`true → 1`,`false → 0`),收敛后不再产帧。
///
/// 同一 [`egui::Id`] 每帧调用一次,动画状态由 egui 动画管理器持有;id 必须
/// 稳定且不含内容长度(AGENTS §6.7 同款纪律)。首次调用直接返回端点 ——
/// 应用首帧因此不闪一次多余的淡入。
pub fn crossfade(ctx: &egui::Context, id: egui::Id, target: bool) -> f32 {
    let time = ctx.global_style().animation_time.clamp(0.0, FADE_S);
    ctx.animate_bool_with_time(id, target, time)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每帧跑一次 crossfade,推进受控时间(与 editor/layout 测试同手法:
    /// `RawInput::time` 驱动 egui 动画管理器)。
    fn frame(ctx: &egui::Context, time: f64, target: bool) -> (f32, std::time::Duration) {
        let mut alpha = 0.0;
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(time),
                ..Default::default()
            },
            |ui| alpha = crossfade(ui.ctx(), egui::Id::new("crossfade"), target),
        );
        let delay = output
            .viewport_output
            .values()
            .map(|viewport| viewport.repaint_delay)
            .min()
            .unwrap();
        output.drop_without_applying_deltas();
        (alpha, delay)
    }

    /// 状态随时间收敛:target 翻转后 alpha 从接近 0 单调走向 1,超过时长即
    /// 收敛在 1,此后不再要帧(egui 视口首两帧自带 settle 重绘,断言落在
    /// 其后的帧,与 layout 的空转回归测试同口径)。
    #[test]
    fn crossfade_converges_to_one_and_stops_requesting_frames() {
        let ctx = egui::Context::default();
        // Source 起步(live=false):首调直接端点 0,无闪变
        let (alpha, _) = frame(&ctx, 0.0, false);
        assert_eq!(alpha, 0.0);

        // 切到 Live:动画起步(远未到 1)
        let (alpha, _) = frame(&ctx, 0.016, true);
        assert!(
            (0.0..0.5).contains(&alpha),
            "切换帧 alpha 从低端起步,实际 {alpha}"
        );

        // 0.32s(> FADE_S)后收敛在 1,且不再要帧;途中单调不减
        let mut delay = None;
        let mut previous = alpha;
        for step in 2..=25u32 {
            let (alpha, d) = frame(&ctx, f64::from(step) * 0.016, true);
            assert!(alpha >= previous, "单调走向 1:{previous} → {alpha}");
            previous = alpha;
            delay = Some(d);
        }
        assert_eq!(previous, 1.0, "收敛在端点 1");
        assert_eq!(delay, Some(std::time::Duration::MAX), "收敛后不产帧");
    }

    /// 可访问性总闸:`style.animation_time` 为 0 时 crossfade 恒给端点
    /// (target=true 恒 1),且全程不产帧。
    #[test]
    fn zero_animation_time_disables_fade_without_repaint() {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let mut delay = None;
        for step in 0..=3u32 {
            let (alpha, d) = frame(&ctx, f64::from(step) * 0.016, true);
            assert_eq!(alpha, 1.0, "关动画即端点,第 {step} 帧也不闪");
            delay = Some(d);
        }
        assert_eq!(delay, Some(std::time::Duration::MAX), "不产帧");
    }
}
