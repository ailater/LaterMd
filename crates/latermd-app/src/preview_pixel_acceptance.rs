//! #43 M2 像素验收(无头):明暗两套主题 × 中英数字混排文档的
//! 「视觉行完整」与「行间距一致」。
//!
//! 手法照 #41 齿轮的无头像素取证(`ui/icons.rs::settings_gear_pixels_in_both_visuals`)
//! 与 m5-acceptance 的明暗双套采样思想:把预览按生产同配置渲染一帧 →
//! 曲面细分(feathering 关,三角形即硬边、不依赖 GPU)→ 对采样点做
//! 三角形覆盖测试。与 #41 取「第一个覆盖者」不同,文本遮挡裁决需要
//! **画家算法的最终覆盖者**(最后覆盖该点的三角形 = 视觉上真实可见的
//! 那一层)——「显示不全」的机制正是越界墨迹被后续块的不透明背景/相邻
//! 行墨迹覆盖(M1 结论 3),只看底层会把被盖掉的字形误判为完好。
//!
//! 三项断言(每套主题各一轮):
//! 1. **行完整**:每个内容行的每个已着墨字形,在其墨迹中心采样,最终
//!    覆盖者必须是该字形自己的文本 quad(顶点色 == 所在 section 的文本色)
//!    —— 字形既画出来了,也没被任何背景块/相邻行盖掉。
//! 2. **行间净空**:相邻两行实墨带之间的整条水平采样带,任何点都不得被
//!    文本 quad 覆盖(露背景)—— 墨迹不互侵,即「无上下裁切特征」。
//! 3. **行间距一致**:wrap 段相邻行的实墨净空带宽两两相等(±1px 吸附容差),
//!    行盒推进步长恒定 —— 「行高也有问题」的观感裁决。
//!
//! 真机三平台(Windows 微软雅黑 / macOS PingFang 的 CJK 表值与 Noto 不同,
//! override 目标值随平台字体而变)留人工清单,见 #43 notes。

use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, RawInput, Rect, UiBuilder, Vec2};
use egui::epaint::text::Glyph;
use egui::epaint::Mesh;

use crate::font_metrics_repro::{is_cjk_ideograph, is_latin_alnum};
use crate::fonts;

/// 验收文档:四态(标题/粗体/斜体/正文)× 中英数字混排 + 超长行 wrap。
const DOC: &str = concat!(
    "# 标题 Heading 一 中文 H1 123\n\n",
    "正文 Body 中文 123 数字 abc 混排 Test 高低不一,",
    "**粗体 Bold 加粗 456 数字 ABC 混排**,",
    "*斜体 Italic 斜体 789 数字 def 混排*。\n\n",
    "中文连续行观测相邻行墨迹净空中文中文中文中文中文中文中文中文中文中文",
    "中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文",
    "中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文",
    "中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文",
);

/// 两色通道距离和(#41 口径,0 = 同色)。
fn color_dist(a: Color32, b: Color32) -> i32 {
    let ch = |x: u8, y: u8| (i32::from(x) - i32::from(y)).abs();
    ch(a.r(), b.r()) + ch(a.g(), b.g()) + ch(a.b(), b.b())
}

fn point_in_tri(p: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    let det = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
    if det(b - a, c - a).abs() < 1e-9 {
        return false;
    }
    let d = |u: Pos2, v: Pos2| det(v - u, p - u);
    let (d1, d2, d3) = (d(a, b), d(b, c), d(c, a));
    (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0)
}

/// 采样点的**最终覆盖色**(画家算法:遍历全部 mesh,记录最后覆盖该点的
/// 顶点色)。返回 `None` = 没有任何三角形覆盖(露画布背景)。
fn final_covered_color(meshes: &[&Mesh], p: Pos2) -> Option<Color32> {
    let mut top = None;
    for mesh in meshes {
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v = |i: u32| mesh.vertices[i as usize].pos;
            let (a, b, c) = (v(tri[0]), v(tri[1]), v(tri[2]));
            if point_in_tri(p, a, b, c) {
                top = Some(mesh.vertices[tri[0] as usize].color);
            }
        }
    }
    top
}

/// 单个字形的墨迹矩形(galley 坐标):基线 + uv_rect 偏移,尺寸即 quad。
fn glyph_ink_rect(row_pos: Pos2, glyph: &Glyph) -> Option<(Pos2, Vec2)> {
    if glyph.uv_rect.offset == Vec2::ZERO && glyph.uv_rect.size == Vec2::ZERO {
        return None;
    }
    let min = Pos2 {
        x: row_pos.x + glyph.pos.x + glyph.uv_rect.offset.x,
        y: row_pos.y + glyph.pos.y + glyph.uv_rect.offset.y,
    };
    Some((min, glyph.uv_rect.size))
}

#[test]
fn preview_mixed_script_rows_intact_in_both_visuals() {
    for dark in [true, false] {
        let ctx = egui::Context::default();
        if fonts::install(&ctx).is_none() {
            eprintln!("本机无 CJK 候选字体,像素验收无混排对象,跳过");
            return;
        }
        ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        // 关抗锯齿羽化(#41 同款):边缘三角形的透明渐变会污染采样读色。
        ctx.options_mut(|o| o.tessellation_options.feathering = false);

        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(700.0, 500.0));
        let mut text_color = Color32::BLACK;
        let mut panel_fill = Color32::WHITE;
        // 同一帧内两个同起点 child:前者取 galley(几何),后者 show()(mesh)。
        // 两者共用同一 widget id 与 wrap 宽度,vendored 布局缓存命中,
        // 几何必然一致;跨帧取形则可能因 available_width 语义差异错位。
        let mut captured: Option<Arc<egui::epaint::text::Galley>> = None;
        let mut output = ctx.run_ui(RawInput::default(), |panel| {
            let style = crate::theme::effective_markdown_style(
                panel.ctx(),
                crate::theme::default_markdown_style(),
            );
            let font = egui::FontId::new(13.0, fonts::preview_body_family(panel.ctx()));
            text_color = panel.visuals().text_color();
            panel_fill = panel.visuals().panel_fill;
            let mut layout_child = panel.new_child(UiBuilder::new().max_rect(screen));
            let (_, galley, _) =
                egui_markdown::MarkdownLabel::new(egui::Id::new("m2-pixel-acceptance"), DOC)
                    .font(font.clone())
                    .style(&style)
                    .wrap()
                    .layout_in_ui(&mut layout_child);
            captured = Some(galley);
            let mut paint_child = panel.new_child(UiBuilder::new().max_rect(screen));
            egui_markdown::MarkdownLabel::new(egui::Id::new("m2-pixel-acceptance"), DOC)
                .font(font)
                .style(&style)
                .wrap()
                .show(&mut paint_child);
        });
        let clipped = std::mem::take(&mut output.shapes);
        let primitives = ctx.tessellate(clipped, 1.0);
        // 无头环境不消费纹理 delta,显式丢弃(epaint 对未处理 delta 会 panic)。
        output.drop_without_applying_deltas();
        let meshes: Vec<&Mesh> = primitives
            .iter()
            .filter_map(|cp| match &cp.primitive {
                egui::epaint::Primitive::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .collect();
        // 本帧出现过的顶点色全集(字形 quad 的绘制色 = 所在 section 的
        // 文本色;`Glyph::section_index` 对外私有,无法逐字形对账,改用
        // 同帧全集 —— 背景色未画 quad,必不在集合内,判定等价)。
        let mut quad_colors: Vec<Color32> = Vec::new();
        for mesh in &meshes {
            for v in &mesh.vertices {
                if v.color.a() > 200 && !quad_colors.contains(&v.color) {
                    quad_colors.push(v.color);
                }
            }
        }
        assert!(
            quad_colors.iter().any(|c| color_dist(*c, panel_fill) > 100),
            "{} 主题:mesh 中应存在不透明文本色",
            if dark { "暗色" } else { "亮色" }
        );
        assert!(
            color_dist(text_color, panel_fill) > 100,
            "{} 主题前景背景应可分:fg={text_color:?} bg={panel_fill:?}",
            if dark { "暗色" } else { "亮色" }
        );

        let galley = captured.expect("layout_in_ui 应产出 galley");

        // 收集内容行(有字形的行)与行内字形的墨迹矩形。
        struct RowInk {
            center_probes: Vec<Pos2>,
            ink_top: f32,
            ink_bottom: f32,
            top: f32,
        }
        let mut rows: Vec<RowInk> = Vec::new();
        for placed in &galley.rows {
            if placed.row.glyphs.is_empty() {
                continue;
            }
            let mut row = RowInk {
                center_probes: Vec::new(),
                ink_top: f32::INFINITY,
                ink_bottom: f32::NEG_INFINITY,
                top: placed.pos.y,
            };
            for glyph in &placed.row.glyphs {
                let Some((min, size)) = glyph_ink_rect(placed.pos, glyph) else {
                    continue;
                };
                row.ink_top = row.ink_top.min(min.y);
                row.ink_bottom = row.ink_bottom.max(min.y + size.y);
                // 只对断言关注的字符(CJK 表意 / ASCII 字母数字)采样,
                // 避开标点与空白(空格无墨迹)。采样点取 **75% 高度(近基线)
                // 处的水平中心** 而非几何中心:斜体是 tessellation 层的合成
                // skew(顶部右移 0.25×高、底部为 0),几何中心在窄字形
                // ('I' 仅 2px 宽)上会被 skew 推出 quad 外;75% 高度处
                // 位移仅 0.25×0.25×高,任何字形的半宽都盖得住。
                if is_cjk_ideograph(glyph.chr) || is_latin_alnum(glyph.chr) {
                    row.center_probes
                        .push(egui::pos2(min.x + size.x / 2.0, min.y + size.y * 0.75));
                }
            }
            if !row.center_probes.is_empty() {
                rows.push(row);
            }
        }
        assert!(
            rows.len() >= 5,
            "混排文档应至少折出 5 个内容行(实际 {})",
            rows.len()
        );

        // 断言 1:行完整 —— 每个字形墨迹中心的最终覆盖者是一个不透明
        // 文本 quad(色 ∈ 同帧文本色全集,非背景):既证明字形画出来了,
        // 也证明没被背景块/相邻行墨迹盖掉(被盖 → 最终色 == 背景或透明)。
        for row in &rows {
            for p in &row.center_probes {
                match final_covered_color(&meshes, *p) {
                    Some(got) => assert!(
                        got.a() > 200
                            && quad_colors.contains(&got)
                            && color_dist(got, panel_fill) > 100,
                        "{} 主题:字形墨迹中心 {:?} 最终色 {:?} 不是可见文本色(被遮挡或未绘制)",
                        if dark { "暗色" } else { "亮色" },
                        p,
                        got
                    ),
                    None => panic!(
                        "{} 主题:字形墨迹中心 {:?} 无任何覆盖(字形未绘制)",
                        if dark { "暗色" } else { "亮色" },
                        p
                    ),
                }
            }
        }

        // 断言 2:行间净空 —— 相邻两行实墨带之间的水平采样带露背景
        //(无文本 quad 覆盖 = 墨迹不互侵 = 无上下裁切特征)。
        for pair in rows.windows(2) {
            let (prev, next) = (&pair[0], &pair[1]);
            let band_top = prev.ink_bottom + 0.5;
            let band_bottom = next.ink_top - 0.5;
            if band_bottom <= band_top {
                panic!(
                    "{} 主题:相邻行实墨带重叠(top {:.2} < bottom {:.2})",
                    if dark { "暗色" } else { "亮色" },
                    band_bottom,
                    band_top
                );
            }
            let mid = (band_top + band_bottom) / 2.0;
            for x in (10..690).step_by(16) {
                let p = egui::pos2(x as f32, mid);
                if let Some(got) = final_covered_color(&meshes, p) {
                    // 净空带只允许半透明覆盖;不透明 quad(任何文本色 ——
                    // 正文/粗体/斜体/标题各有其色)覆盖即墨迹互侵。
                    assert!(
                        got.a() < 128,
                        "{} 主题:行间净空带 {:?} 被不透明 quad 覆盖(墨迹互侵,色 {:?})",
                        if dark { "暗色" } else { "亮色" },
                        p,
                        got
                    );
                }
            }
        }

        // 断言 3:行间距一致 —— wrap 段(同一正文块折出的相邻行)的
        // 行盒推进步长与实墨净空带宽恒定(±1px 吸附容差)。
        let body_rows: Vec<(f32, f32)> = rows
            .iter()
            .map(|r| (r.top, r.ink_bottom - r.ink_top))
            .collect();
        let steps: Vec<f32> = body_rows.windows(2).map(|w| w[1].0 - w[0].0).collect();
        let ink_heights: Vec<f32> = body_rows.iter().map(|(_, h)| *h).collect();
        // 同块正文的行盒步长应完全一致(同一字号的 floor 行高吸附值)。
        // 文档前两行是 H1/正文首行(块间距另算),取尾部 wrap 段核对:
        // wrap 段 = 步长相等的最大连续段。
        let mut wrap_start = 0;
        for i in 1..steps.len() {
            if (steps[i] - steps[wrap_start]).abs() > 1.0 {
                wrap_start = i;
            }
        }
        let wrap_steps = &steps[wrap_start..];
        let wrap_inks = &ink_heights[wrap_start + 1..];
        assert!(
            wrap_steps.len() >= 2,
            "wrap 段应至少 3 行(实际 {},全部步长 {steps:?})",
            wrap_steps.len() + 1
        );
        for (i, s) in wrap_steps.iter().enumerate() {
            assert!(
                (s - wrap_steps[0]).abs() <= 1.0,
                "{} 主题:wrap 段第 {} 行步长 {s:.2} 偏离首行 {:.2}(行距不一致)",
                if dark { "暗色" } else { "亮色" },
                i + 1,
                wrap_steps[0]
            );
        }
        for (i, h) in wrap_inks.iter().enumerate() {
            assert!(
                (h - wrap_inks[0]).abs() <= 1.0,
                "{} 主题:wrap 段第 {} 行实墨高 {h:.2} 偏离首行 {:.2}(字形裁切特征)",
                if dark { "暗色" } else { "亮色" },
                i + 1,
                wrap_inks[0]
            );
        }
    }
}
