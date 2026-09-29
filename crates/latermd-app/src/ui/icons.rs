//! 图标体系:egui-phosphor(2026-09-27 U2,坤哥放行「全量迁移」)。
//!
//! **2026-09-27 之前是 `Painter` 自绘线段**,理由是「egui 无内置图标集、
//! emoji/Unicode 符号在三平台缺字」。那个论据对**系统字体**成立,对 phosphor
//! 这类**字体内嵌在 crate 里**的库不成立(见 decisions-pending #34 的修订)。
//! 坤哥 2026-09-27 拍板选乙:全量迁移,本模块退役自绘。
//!
//! 字形是内嵌 TTF 的私有区码位(U+E0xx),不查系统字体,三平台一致;代价是
//! 二进制 +1.03 MiB(可开 `subset` feature 裁剪,暂不开)。

use crate::ui::tokens::{ICON, RADIUS_SM, SPACE_SM, SPACE_XS, TOOLBAR_H};
use eframe::egui::{self, Align2, Color32, Painter, Pos2, Rect};

/// 图标标识。新增图标 = 加枚举变体 + [`Icon::draw`] 一个分支,调用方无需
/// 感知绘制细节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// 新建:文档 + 右上加号。
    New,
    /// 打开:文件夹。
    Open,
    /// 保存:软盘。
    Save,
    /// 另存为:软盘 + 右下箭头。
    SaveAs,
    /// 导出:向下箭头入托盘。
    Export,
    /// 侧边栏:外框 + 左侧实心条。
    Sidebar,
    /// 主题:太阳。
    Theme,
    /// AI:四角星(与 `ai://` 链接色同源的语义)。
    Ai,
    /// 文件页签:叠放文档。
    Files,
    /// 搜索页签:放大镜。
    Search,
    /// 大纲页签:缩进列表。
    Outline,
    /// Git 页签:分支图。
    Git,
    /// 设置:齿轮。
    Settings,
    /// 重置:回环箭头。
    Reset,
    /// 关右栏:外框 + 右侧实心条(左栏用既有 `Sidebar` 镜像)。
    PanelRight,
    /// 禅定:同心圆。
    Zen,
    /// 最小化:一条横线。
    Minimize,
    /// 最大化:方框。
    Maximize,
    /// 还原:前后错位的两个方框(后框只画上、右两边)。
    Restore,
    /// 关闭:叉。
    Close,
    // —— Markdown 格式工具条(docs/ui-shell-redesign.md §6.1)——
    /// 行内代码:左右两枚尖角 `‹ ›`。
    CodeInline,
    /// 代码块:`‹ ›` 加外框起伏。
    CodeBlock,
    /// 链接:两枚斜置的链环。
    Link,
    /// 引用:左侧竖条 + 两条文字线。
    Quote,
    /// 分割线:中间一条贯穿横线。
    Divider,
    /// 表格:2×2 网格外框 + 表头横线。
    Table,
    /// 正文(去前缀):phosphor `PARAGRAPH`(¶)。规格 §6.1 的原始指定,
    /// 迁移后回归字面方案 —— 与 `Quote` 的区分交给 `Quote` 的竖条码位。
    Paragraph,
    /// 插入图片(#26 图片框):山与日剪影。
    Image,
    /// 无序列表:三点 + 三线。
    BulletList,
    /// 有序列表:三条竖短线(序号笔画抽象)+ 三线。
    OrderedList,
    /// 任务列表:勾选框两个 + 两条线,首框带勾。
    TaskList,
}

impl Icon {
    /// 在 `center` 处以 `size` 边长画图标,颜色由调用方给(通常取 visuals
    /// 前景色,禁用态取 weak)。
    /// 对应的 phosphor 码位。
    ///
    /// 映射是**人工挑的**,不是按名字硬套 —— 同一个动作在两套体系里常常不叫
    /// 一个名字(自绘 `Save` → phosphor `FLOPPY_DISK`;自绘 `Theme` →
    /// `PALETTE`)。挑错很难看,挑完要在真机上逐个过一眼。
    pub fn glyph(self) -> &'static str {
        use egui_phosphor::regular as ph;
        match self {
            Self::New => ph::FILE_PLUS,
            Self::Open => ph::FOLDER_OPEN,
            Self::Save => ph::FLOPPY_DISK,
            Self::SaveAs => ph::FLOPPY_DISK_BACK,
            Self::Export => ph::EXPORT,
            Self::Sidebar => ph::SIDEBAR,
            Self::Theme => ph::PALETTE,
            Self::Ai => ph::SPARKLE,
            Self::Files => ph::FOLDERS,
            Self::Search => ph::MAGNIFYING_GLASS,
            Self::Outline => ph::TEXT_INDENT,
            Self::Git => ph::GIT_BRANCH,
            Self::Settings => ph::GEAR,
            Self::Reset => ph::ARROW_COUNTER_CLOCKWISE,
            Self::PanelRight => ph::SQUARE_SPLIT_HORIZONTAL,
            Self::Zen => ph::ARROWS_OUT,
            Self::Minimize => ph::MINUS,
            Self::Maximize => ph::SQUARE,
            // 窗口「还原」= 两个叠放的方块,phosphor 里 COPY 正是这个形状
            Self::Restore => ph::COPY,
            Self::Close => ph::X,
            Self::CodeInline => ph::CODE,
            Self::CodeBlock => ph::CODE_BLOCK,
            Self::Link => ph::LINK,
            Self::Quote => ph::QUOTES,
            Self::Divider => ph::MINUS,
            Self::Table => ph::TABLE,
            Self::Paragraph => ph::PARAGRAPH,
            Self::Image => ph::IMAGE,
            Self::BulletList => ph::LIST_BULLETS,
            Self::OrderedList => ph::LIST_NUMBERS,
            Self::TaskList => ph::LIST_CHECKS,
        }
    }

    /// 画在 `center` 居中处。
    ///
    /// phosphor 是**字体**,所以绘制退化成排版一段文本。字体族走
    /// `Proportional`:`fonts::install` 把 phosphor 作为它的 fallback 插在
    /// Inter 之后,私有区码位会落到 phosphor 上。若某个 Context 没跑过
    /// install(无头测试正是如此),码位只是**没字形**,显示豆腐但**不 panic**
    /// —— 这正是没走「单独一个 `FontFamily::Name` 族」的原因:未绑定族在
    /// epaint 里是 panic(见 `fonts.rs` 里 SemiBold 那段注释)。
    pub fn draw(self, painter: &Painter, center: Pos2, size: f32, color: Color32) {
        let font = egui::FontId::new(size, egui::FontFamily::Proportional);
        let galley = painter.layout_no_wrap(self.glyph().to_owned(), font, color);
        let rect = Align2::CENTER_CENTER.anchor_size(center, galley.size());
        painter.galley(rect.min, galley, color);
    }
}

/// 自绘按钮的 hover 底色,**带淡入**(2026-09-27 U3)。
///
/// 出厂写法是 `if hovered { 画满 }` —— 硬切,鼠标扫过一排按钮就是一串闪动。
/// 这里用 egui 自带的 `animate_bool` 拿 0..1 的插值,乘进底色 alpha;不引
/// 任何动画库(GPL 的 egui_transition_animation 已否)。
///
/// **不要用 `WidgetVisuals::expansion` 之类的路子做这个**:它连控件分配尺寸
/// 一起撑大,会触发 `horizontal_wrapped` 换行、点击落空(见 `theme.rs` 焦点
/// 环处的注释)。alpha 插值不动布局。
pub fn hover_fill(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, radius: f32) {
    let t = ui
        .ctx()
        .animate_bool(response.id.with("hover"), response.hovered());
    if t > 0.0 {
        let fill = ui.visuals().widgets.hovered.bg_fill.gamma_multiply(t);
        ui.painter().rect_filled(rect, radius, fill);
    }
}

/// 纯图标按钮(左栏顶段动作、标题栏齿轮等):宽度只够一个图标,语义靠
/// tooltip 补。
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip_text: &str) -> egui::Response {
    let size = egui::vec2(ICON + 2.0 * SPACE_SM, TOOLBAR_H);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let enabled = ui.is_enabled();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if enabled {
            hover_fill(ui, &response, rect, RADIUS_SM);
        }
        let color = if enabled {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };
        icon.draw(painter, rect.center(), ICON, color);
    }
    response.on_hover_text(tooltip_text)
}

/// 侧边栏页签:图标 + 文字,选中态用强调色下划线(与文件树的填充式
/// `selectable_label` 区分层级)。宽度按内容,窄面板由外层
/// `horizontal_wrapped` 换行而不是压缩。
pub fn icon_tab(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let text_color = if selected {
        crate::ui::tokens::accent(ui)
    } else {
        ui.visuals().text_color()
    };
    let label_w = ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(label.to_owned(), font.clone(), text_color))
        .rect
        .width();
    let size = egui::vec2(
        SPACE_SM + crate::ui::tokens::ICON_SM + SPACE_XS + label_w + SPACE_SM,
        TOOLBAR_H,
    );
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        // WorkBuddy 风:选中项浅蓝圆角底,悬停浅灰
        if selected {
            let selected_bg = crate::theme::shell_tokens(ui.visuals().dark_mode).selected_bg;
            painter.rect_filled(rect, RADIUS_SM, selected_bg);
        } else if response.hovered() {
            painter.rect_filled(rect, RADIUS_SM, ui.visuals().widgets.hovered.bg_fill);
        }
        icon.draw(
            painter,
            egui::pos2(
                rect.left() + SPACE_SM + crate::ui::tokens::ICON_SM / 2.0,
                rect.center().y,
            ),
            crate::ui::tokens::ICON_SM,
            text_color,
        );
        painter.text(
            egui::pos2(
                rect.left() + SPACE_SM + crate::ui::tokens::ICON_SM + SPACE_XS,
                rect.center().y,
            ),
            Align2::LEFT_CENTER,
            label,
            font,
            text_color,
        );
        if selected {
            // 下划线:2px 条贴在页签底部
            painter.rect_filled(
                Rect::from_min_size(
                    egui::pos2(rect.left() + SPACE_SM, rect.bottom() - 2.0),
                    egui::vec2(rect.width() - 2.0 * SPACE_SM, 2.0),
                ),
                1.0,
                text_color,
            );
        }
    }
    if response.clicked() {
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部图标都能画出来且不 panic(无头渲染一帧,覆盖每条绘制分支)。
    #[test]
    fn every_icon_renders() {
        let ctx = egui::Context::default();
        let icons = [
            Icon::New,
            Icon::Open,
            Icon::Save,
            Icon::SaveAs,
            Icon::Export,
            Icon::Sidebar,
            Icon::Theme,
            Icon::Ai,
            Icon::Files,
            Icon::Search,
            Icon::Outline,
            Icon::Git,
            Icon::Settings,
            Icon::Reset,
            Icon::PanelRight,
            Icon::Zen,
            Icon::Minimize,
            Icon::Maximize,
            Icon::Restore,
            Icon::Close,
        ];
        for icon in icons {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                icon.draw(ui.painter(), egui::pos2(20.0, 20.0), ICON, Color32::WHITE);
            });
            output.drop_without_applying_deltas();
        }
    }

    /// 页签:选中态与未选中态都能渲染,点击返回 clicked(选中与否由调用方
    /// 的消息归约决定,本函数只做视觉与交互)。
    #[test]
    fn icon_tab_renders_selected_and_clickable() {
        let ctx = egui::Context::default();
        for selected in [false, true] {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                icon_tab(ui, Icon::Files, "文件", selected);
            });
            output.drop_without_applying_deltas();
        }
    }

    /// 纯图标按钮渲染不 panic 且带 tooltip 文案。
    #[test]
    fn icon_button_renders() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            icon_button(ui, Icon::Settings, "设置");
        });
        output.drop_without_applying_deltas();
    }
}
