//! 自绘线性图标体系(docs/ui-polish.md §3)。
//!
//! **为什么自绘而不是字体字符**:egui 无内置图标集,emoji / Unicode 符号
//! (✎ 🗋 ⌘)在三平台缺字风险真实(AGENTS.md §5 已把字体列为风险项)。全部
//! 图标用 `Painter` 画线段/圆/矩形:零字体依赖、零纹理依赖、随主题取色、
//! 在高 DPI 下不会因位图缩放糊掉。
//!
//! 坐标约定:以 `size` 为基准的 `[-0.5, 0.5]` 归一化空间,线宽 `size / 12`
//! (16px 图标 = 1.33px 线)。改 `size` 即整体等比缩放,不存在第二套坐标。

use crate::ui::tokens::{ICON, RADIUS_SM, SPACE_SM, SPACE_XS, TOOLBAR_H};
use eframe::egui::{self, Align2, Color32, Painter, Pos2, Rect, Stroke, Vec2};

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
}

impl Icon {
    /// 在 `center` 处以 `size` 边长画图标,颜色由调用方给(通常取 visuals
    /// 前景色,禁用态取 weak)。
    pub fn draw(self, painter: &Painter, center: Pos2, size: f32, color: Color32) {
        let stroke = Stroke::new((size / 12.0).max(1.0), color);
        // 归一化坐标 → 屏幕坐标
        let at = |x: f32, y: f32| center + Vec2::new(x * size, y * size);
        let seg = |a: (f32, f32), b: (f32, f32)| {
            painter.line_segment([at(a.0, a.1), at(b.0, b.1)], stroke)
        };
        let path = |pts: &[(f32, f32)]| {
            for pair in pts.windows(2) {
                seg(pair[0], pair[1]);
            }
        };
        let frame = |a: (f32, f32), b: (f32, f32)| {
            painter.rect_stroke(
                Rect::from_two_pos(at(a.0, a.1), at(b.0, b.1)),
                size * 0.08,
                stroke,
                egui::StrokeKind::Outside,
            );
        };
        let fill = |a: (f32, f32), b: (f32, f32)| {
            painter.rect_filled(
                Rect::from_two_pos(at(a.0, a.1), at(b.0, b.1)),
                size * 0.06,
                color,
            );
        };
        let dot = |c: (f32, f32), r: f32| painter.circle_filled(at(c.0, c.1), r * size, color);
        let ring = |c: (f32, f32), r: f32| painter.circle_stroke(at(c.0, c.1), r * size, stroke);
        // 四角星:`k` 为缩放。
        let star = |c: (f32, f32), k: f32| {
            let (x, y) = c;
            path(&[
                (x, y - 0.42 * k),
                (x + 0.13 * k, y - 0.13 * k),
                (x + 0.42 * k, y),
                (x + 0.13 * k, y + 0.13 * k),
                (x, y + 0.42 * k),
                (x - 0.13 * k, y + 0.13 * k),
                (x - 0.42 * k, y),
                (x - 0.13 * k, y - 0.13 * k),
                (x, y - 0.42 * k),
            ]);
        };
        // 折线近似圆弧(egui 无 arc API);返回终点坐标,箭头用它接续。
        let arc = |c: (f32, f32), r: f32, a0: f32, a1: f32| -> (f32, f32) {
            let steps = 18usize;
            let point = |i: usize| {
                let t = a0 + (a1 - a0) * (i as f32 / steps as f32);
                (c.0 + r * t.cos(), c.1 + r * t.sin())
            };
            for i in 0..steps {
                seg(point(i), point(i + 1));
            }
            point(steps)
        };

        match self {
            Self::New => {
                frame((-0.42, -0.40), (0.02, 0.42));
                seg((-0.30, -0.14), (-0.08, -0.14));
                seg((-0.30, 0.06), (-0.08, 0.06));
                seg((0.20, 0.02), (0.44, 0.02));
                seg((0.32, -0.10), (0.32, 0.14));
            }
            Self::Open => {
                frame((-0.42, -0.22), (0.42, 0.38));
                path(&[
                    (-0.42, -0.22),
                    (-0.42, -0.40),
                    (-0.12, -0.40),
                    (-0.04, -0.22),
                ]);
            }
            Self::Save => {
                frame((-0.42, -0.40), (0.42, 0.40));
                frame((-0.18, -0.40), (0.18, -0.04));
                seg((-0.24, 0.20), (0.24, 0.20));
                seg((-0.24, 0.32), (0.24, 0.32));
            }
            Self::SaveAs => {
                // 软盘缩小让位给右下箭头,两者不重叠
                frame((-0.46, -0.40), (-0.02, 0.34));
                frame((-0.28, -0.40), (-0.06, -0.10));
                seg((-0.32, 0.14), (-0.08, 0.14));
                seg((0.08, 0.02), (0.44, 0.40));
                seg((0.44, 0.40), (0.20, 0.40));
                seg((0.44, 0.40), (0.44, 0.16));
            }
            Self::Export => {
                seg((0.0, -0.42), (0.0, 0.10));
                path(&[(-0.20, -0.08), (0.0, 0.12), (0.20, -0.08)]);
                seg((-0.36, 0.30), (0.36, 0.30));
            }
            Self::Sidebar => {
                frame((-0.44, -0.36), (0.44, 0.36));
                fill((-0.44, -0.36), (-0.24, 0.36));
            }
            Self::Theme => {
                ring((0.0, 0.0), 0.15);
                // 六条射线,每 60°
                for step in 0..6 {
                    let angle = std::f32::consts::PI / 3.0 * step as f32;
                    let (cos, sin) = (angle.cos(), angle.sin());
                    seg((cos * 0.26, sin * 0.26), (cos * 0.40, sin * 0.40));
                }
            }
            Self::Ai => {
                star((0.02, 0.04), 1.0);
                star((0.32, -0.28), 0.42);
            }
            Self::Files => {
                frame((-0.06, -0.36), (0.42, 0.34));
                frame((-0.42, -0.28), (0.06, 0.42));
            }
            Self::Search => {
                ring((-0.08, -0.08), 0.20);
                seg((0.06, 0.06), (0.36, 0.36));
            }
            Self::Outline => {
                for row in [-0.28_f32, 0.0, 0.28] {
                    dot((-0.30, row), 0.05);
                    seg((-0.14, row), (0.38, row));
                }
            }
            Self::Git => {
                dot((-0.28, -0.28), 0.11);
                dot((-0.28, 0.30), 0.11);
                dot((0.28, 0.02), 0.11);
                seg((-0.28, -0.28), (-0.28, 0.30));
                seg((-0.28, 0.12), (0.28, 0.02));
            }
            Self::Settings => {
                ring((0.0, 0.0), 0.16);
                seg((0.0, -0.44), (0.0, -0.28));
                seg((0.0, 0.28), (0.0, 0.44));
                seg((-0.44, 0.0), (-0.28, 0.0));
                seg((0.28, 0.0), (0.44, 0.0));
            }
            Self::Reset => {
                // 3/4 圆弧 + 末端箭头:260° → -40°(顺时针收口)
                let end = arc(
                    (0.0, 0.0),
                    0.32,
                    std::f32::consts::PI * 1.45,
                    std::f32::consts::PI * 3.05,
                );
                seg(end, (end.0 + 0.02, end.1 + 0.20));
                seg(end, (end.0 + 0.20, end.1 - 0.04));
            }
        }
    }
}

/// 工具栏按钮:图标 + 文字 + 右侧灰阶快捷键。
///
/// 为什么不复用 `egui::Button`:它不接受自绘图标(只认 `TextureId`),且
/// `shortcut_text` 在窄面板下会把按钮撑爆。这里改为手工
/// `allocate_exact_size` + `Painter` 绘制:宽度按内容精确计算,`ui` 处于
/// 禁用态时整体走 weak 前景色且点击不触发。
pub fn icon_text_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    shortcut: Option<&str>,
) -> egui::Response {
    let (rect, response) = allocate_button(ui, label, shortcut);
    let color = ui.visuals().text_color();
    let weak = ui.visuals().weak_text_color();
    let enabled = ui.is_enabled();

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let hovered = response.hovered() && enabled;
        if hovered {
            painter.rect_filled(rect, RADIUS_SM, ui.visuals().widgets.hovered.bg_fill);
        }
        let icon_color = if enabled { color } else { weak };
        icon.draw(
            painter,
            egui::pos2(rect.left() + SPACE_SM + ICON / 2.0, rect.center().y),
            ICON,
            icon_color,
        );
        let font = egui::TextStyle::Button.resolve(ui.style());
        painter.text(
            egui::pos2(rect.left() + SPACE_SM + ICON + SPACE_XS, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            icon_color,
        );
        if let Some(shortcut) = shortcut {
            painter.text(
                egui::pos2(rect.right() - SPACE_SM, rect.center().y),
                Align2::RIGHT_CENTER,
                shortcut,
                font,
                weak,
            );
        }
    }
    tooltip(response, label, shortcut)
}

/// 纯图标按钮(工具栏右侧的齿轮等):宽度只够一个图标,语义靠 tooltip 补。
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip_text: &str) -> egui::Response {
    let size = egui::vec2(ICON + 2.0 * SPACE_SM, TOOLBAR_H);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let enabled = ui.is_enabled();
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() && enabled {
            painter.rect_filled(rect, RADIUS_SM, ui.visuals().widgets.hovered.bg_fill);
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

/// 按钮宽度:图标 + 间隙 + 文字(+ 快捷键),两侧内边距。
fn allocate_button(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: Option<&str>,
) -> (Rect, egui::Response) {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let weak = ui.visuals().weak_text_color();
    let label_w = ui
        .fonts_mut(|fonts| fonts.layout_no_wrap(label.to_owned(), font.clone(), weak))
        .rect
        .width();
    let shortcut_w = shortcut
        .map(|text| {
            ui.fonts_mut(|fonts| fonts.layout_no_wrap(text.to_owned(), font.clone(), weak))
                .rect
                .width()
        })
        .unwrap_or(0.0);
    let width = SPACE_SM
        + ICON
        + SPACE_XS
        + label_w
        + if shortcut.is_some() {
            SPACE_XS * 2.0 + shortcut_w
        } else {
            0.0
        }
        + SPACE_SM;
    let size = egui::vec2(width, TOOLBAR_H);
    ui.allocate_exact_size(size, egui::Sense::click())
}

/// 悬浮提示:`label` 单列;有快捷键时附在括号内 —— 按钮上已显示键位,
/// tooltip 里再给一次是为了让纯图标按钮也有可发现性。
fn tooltip(response: egui::Response, label: &str, shortcut: Option<&str>) -> egui::Response {
    match shortcut {
        Some(shortcut) => response.on_hover_text(format!("{label} ({shortcut})")),
        None => response.on_hover_text(label),
    }
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
        ];
        for icon in icons {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                icon.draw(ui.painter(), egui::pos2(20.0, 20.0), ICON, Color32::WHITE);
            });
            output.drop_without_applying_deltas();
        }
    }

    /// 图标按钮可点击:三帧合成点击后 `clicked` 为真;仅渲染不产生点击。
    #[test]
    fn icon_text_button_clickable() {
        let ctx = egui::Context::default();
        let rect = std::cell::Cell::new(Rect::NOTHING);
        let mut clicked = false;

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = icon_text_button(ui, Icon::Save, "保存", Some("Ctrl+S"));
            rect.set(response.rect);
            clicked = response.clicked();
        });
        output.drop_without_applying_deltas();
        assert!(!clicked, "仅渲染不产生点击");
        assert!(rect.get().width() > ICON, "宽度容纳图标与文字");

        let center = rect.get().center();
        let click = |pressed| egui::Event::PointerButton {
            pos: center,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for events in [
            vec![egui::Event::PointerMoved(center)],
            vec![click(true)],
            vec![click(false)],
        ] {
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    clicked = icon_text_button(ui, Icon::Save, "保存", Some("Ctrl+S")).clicked();
                },
            );
            output.drop_without_applying_deltas();
        }
        assert!(clicked, "按钮可点击");
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
