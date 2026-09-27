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

// —— 圆角 ——

/// 按钮圆角。
pub const RADIUS_SM: f32 = 4.0;
/// 标准 egui 控件的圆角(2026-09-27 U0,抄 armas 同档)。
///
/// 与 `RADIUS_SM` 并存的原因:后者是自绘小件(圆点、角标)的量级,前者给
/// 按钮/输入框这类有面积的东西 —— 面积越大,同样的圆角看着越尖。`theme.rs`
/// 的 `apply_shell_to` 此前硬写 `same(6)`,本轮起改引本常量,避免「token 写着
/// 4、界面跑着 6」的两处真源。
///
/// **类型是 `u8` 不是 `f32`**:egui 0.36 的 `CornerRadius::same` 签名收 `u8`
/// (`epaint/src/corner_radius.rs:59`),本常量只喂它,故按它的口径定义。
pub const RADIUS: u8 = 6;

// —— 控件(2026-09-27 U0:数值抄 armas,见 docs/ui-modernization.md)——

/// 输入框内边距(水平)。
pub const INPUT_PAD_X: f32 = 12.0;
/// 输入框内边距(垂直)。
pub const INPUT_PAD_Y: f32 = 8.0;
/// 标准控件的统一高度(输入框 / 按钮 / 复选框 / 下拉)。
///
/// **按钮与输入框同高** —— 设计系统的常规做法:高度统一才有横向节奏,
/// 同排的「输入框 + 按钮」(如设置页的 base_url / model)不会一高一矮。
///
/// 曾担心它与自绘工具条(`TOOLBAR_H` 28 / `FORMAT_BAR_H` 30)同屏高矮不齐,
/// 经核**该担心不成立**:工具条是自绘的,高度走自己的常量,**不读
/// `interact_size`**;且两者分处编辑区顶部与弹窗,不同屏。故不另设按钮高度。
///
/// 出厂值 18 过于局促(像开发者工具),36 是 armas 同档。
pub const INPUT_H: f32 = 36.0;
/// 控件之间的间距(水平 / 垂直)。出厂是 (8, 3),垂直 3 太挤。
pub const CONTROL_GAP: egui::Vec2 = egui::Vec2::new(8.0, 6.0);
/// 正文与按钮字号。
///
/// egui 出厂 13。中文在 13 下笔画挤(字形本身比拉丁密),14 是可读性下限之上
/// 最贴近现代排版的一档。西文走 Inter、中文走 CJK fallback(U1),同一字号
/// 下两者基线不同,需在真机上目视基线对齐。
pub const FONT_SM: f32 = 14.0;
/// 次级文字:提示行、状态栏。
pub const FONT_XS: f32 = 12.0;
/// 标题字号。
pub const FONT_LG: f32 = 18.0;

// —— 细节层(2026-09-27 U3:焦点环 / 滚动条 / 分隔线)——

/// 焦点环描边宽度。egui 出厂没有焦点环(只换底色),键盘操作时看不出焦点在哪。
///
/// **不要用 `WidgetVisuals::expansion` 做外扩**:它连控件的分配尺寸一起撑大,
/// 会让 `horizontal_wrapped` 的工具条换行、点击落空(见 `theme.rs` 的注释)。
/// 环画在控件矩形内即可。
pub const FOCUS_RING: f32 = 2.0;
/// 滚动条宽度。出厂 12 偏粗(像开发者工具),8 是「细浅条」的量级。
pub const SCROLL_W: f32 = 8.0;
/// 滚动条与内容的内边距。
pub const SCROLL_INNER: f32 = 4.0;
/// 滚动条与容器外缘的留白。
pub const SCROLL_OUTER: f32 = 2.0;

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
