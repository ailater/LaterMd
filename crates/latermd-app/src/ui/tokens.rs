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
/// Markdown 格式工具条高度(docs/ui-shell-redesign.md §11)。与 `TOOLBAR_H`
/// 同一量级:两条都在编辑区顶部,高度差一眼可见但不大。
pub const FORMAT_BAR_H: f32 = 30.0;
/// 自绘标题栏高度(docs/ui-shell-redesign.md §11,无边框模式才有)。
pub const TITLEBAR_H: f32 = 36.0;
/// 标题栏右侧窗口按钮命中区(Win 风整块,mac/Linux 同款统一)。
pub const WINDOW_BTN: egui::Vec2 = egui::Vec2::new(32.0, 24.0);
/// 左栏(导航)宽度下限(docs/ui-shell-redesign.md §11,R4):三栏旧下限
/// 160 是二分栏时代的数字,塞进四行视图导航后不够。
pub const SIDEBAR_MIN_W: f32 = 180.0;
/// 右栏(只读预览)初始宽度。
pub const PREVIEW_DEFAULT_W: f32 = 420.0;
/// 右栏宽度下限:再窄代码块与表格就只剩横向滚动了。
pub const PREVIEW_MIN_W: f32 = 260.0;
/// 左栏视图导航的行高(docs/ui-shell-redesign.md §11,M2 三段式)。
pub const NAV_ROW_H: f32 = 26.0;
/// 导航选中行的左侧竖条宽度(整行选中态的另一半)。
pub const NAV_BAR_W: f32 = 2.0;
/// 禅定模式的正文限宽(docs/ui-shell-redesign.md §11;源出 ui-design.md
/// §1.2 的「沉浸」参数)。720 约合中文 40 字/行:再宽一行要横向扫读,
/// 再窄代码块与表格就得横向滚动了。
pub const ZEN_TEXT_W: f32 = 720.0;
/// 禅定模式下「退出禅定」浮层到内容区右上角的留白。贴死边缘会与「收起
/// 到边」的视觉直觉打架,也压住滚动条。
pub const ZEN_EXIT_MARGIN: f32 = 10.0;
/// 禅定模式内容区的四周留白(docs/ui-shell-redesign.md §7)。窗口够宽时
/// 正文在剩下的空间里居中;窗口窄于 `ZEN_TEXT_W + 2×gutter` 时由 720 限宽
/// 自己收缩,不至于逼出横向滚动。
///
/// **必须是整数**:`Frame::inner_margin` 最终落成 `Margin`(i8),`f32` 转过去
/// 会被 `round()` 静默吃掉小数。
pub const ZEN_GUTTER: f32 = 24.0;

// —— 控件几何(输入框档,U0 新增)——
//
// 来源:Armas / shadcn 的 h-9 / px-3 / py-2(docs/ui-modernization.md §2.4)。
// 只经 `theme::apply_shell` 投影进 egui 的两套 `Style`,不散落调用点。

/// 输入框(TextEdit)高度。投影为 `spacing.interact_size.y` 的高度语义:
/// egui 里 TextEdit 没有独立高度字段,点击类控件(按钮/输入框/滑条)的最小
/// 高度统一取 `interact_size.y`。
pub const INPUT_H: f32 = 36.0;
/// 输入框内边距(水平)。投影为 `spacing.button_padding.x`,TextEdit 与按钮
/// 共用该字段作为框内文字到边框的留白。
pub const INPUT_PAD_X: f32 = 12.0;
/// 输入框内边距(垂直)。
pub const INPUT_PAD_Y: f32 = 8.0;

// —— 字号(U0 新增)——

/// 小一号正文(提示行、状态栏、次要标签)。shadcn `text-sm = 14px` 的 pt 值。
/// 投影为 `TextStyle::Small` 的字号;Body 13 不动(字号用户设置另行排队,
/// docs/roadmap 专题 #23,与本棒解耦)。
pub const FONT_SM: f32 = 14.0;

// —— 圆角 ——

/// 按钮圆角(shadcn rounded-sm 档)。
pub const RADIUS_SM: f32 = 4.0;
/// 控件圆角(shadcn rounded-md 档)。2026-09-27 起为 `theme::apply_shell`
/// 投影的数字真源(此前硬编码 6,ui-polish §2 的划线注记随之作废)。
///
/// 来源:Armas / shadcn 的 `rounded-md = 6px`(docs/ui-modernization.md
/// §2.4「白拿它产出的办法」——抄数值不引库)。
pub const RADIUS_MD: f32 = 6.0;

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

    /// U0 新 token 的数值口径:抄自 armas/shadcn 的公开设计值
    /// (docs/ui-modernization.md §2.4),改这里必须同步改该表。
    #[test]
    fn u0_tokens_match_sourced_values() {
        assert_eq!(RADIUS_MD, 6.0, "shadcn rounded-md");
        assert_eq!(INPUT_H, 36.0, "shadcn h-9");
        assert_eq!(INPUT_PAD_X, 12.0, "shadcn px-3");
        assert_eq!(INPUT_PAD_Y, 8.0, "shadcn py-2");
        assert_eq!(FONT_SM, 14.0, "shadcn text-sm");
    }

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
