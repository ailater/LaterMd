//! 界面设计 token(docs/ui-polish.md §2)。
//!
//! 只做**语义级**常量:间距、尺寸、圆角与四个语义色。不做每控件样式树
//! (roadmap 专题「明确不做」),也不做序列化 —— 皮肤文件(阶段 4.5 批次 B)
//! 才需要 serde 结构,本轮常量足够。
//!
//! 落在这里而非散在各 UI 模块,是为了让「工具栏按钮高度」这类数字只有
//! 一个真源;改一个 token 即全界面跟随。

use eframe::egui::{self, Color32};

// —— 间距 ——

/// 图标与文字的间隙。
pub const SPACE_XS: f32 = 4.0;
/// 按钮内边距(水平)。
pub const SPACE_SM: f32 = 6.0;
/// 工具栏分组间距。
pub const SPACE_MD: f32 = 10.0;

// —— 尺寸 ——

/// 图标方框边长(按钮内)。
pub const ICON: f32 = 16.0;
/// 图标边长(页签等紧凑位)。
pub const ICON_SM: f32 = 13.0;
/// 工具栏按钮高度。
pub const TOOLBAR_H: f32 = 28.0;

// —— 圆角 ——

/// 按钮圆角。
pub const RADIUS_SM: f32 = 4.0;

// —— 语义色 ——

/// 强调色:页签选中、选中态下划线、主按钮。WorkBuddy 风(2026-09-26 定):
/// 飞书系蓝,浅色 #3370FF、暗色 #6C9FFF。
///
/// AI 专属元素(ai:// 链接、指令卡)仍用紫罗兰 —— 见 `ui::preview` 的
/// `ai_link_color`:强调色中立化之后,AI 是"唯一用紫罗兰的东西",反而更醒目。
pub fn accent(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x6C, 0x9F, 0xFF)
    } else {
        Color32::from_rgb(0x33, 0x70, 0xFF)
    }
}

/// 可行动降级(与回滚 dirty 警示同档黄)。
pub const WARN: Color32 = Color32::from_rgb(0xEB, 0xB4, 0x3C);
/// 不可逆警示(与回滚确认文案同档红)。
pub const DANGER: Color32 = Color32::from_rgb(0xEB, 0x60, 0x60);
/// 成功 / 已配置态。
pub const OK: Color32 = Color32::from_rgb(0x60, 0xC8, 0x78);

#[cfg(test)]
mod tests {
    use super::*;

    /// 两套 visuals 下强调色不同(明暗各一档),且都非空色。
    #[test]
    fn accent_differs_per_theme() {
        let ctx = egui::Context::default();
        let mut light = None;
        let mut dark = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            light = Some(accent(ui));
        })
        .drop_without_applying_deltas();
        let ctx = egui::Context::default();
        ctx.set_theme(egui::Theme::Light);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            dark = Some(accent(ui));
        })
        .drop_without_applying_deltas();
        assert_ne!(light, dark, "明暗两档强调色不同");
    }
}
