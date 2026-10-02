//! #50 M1 编辑器专用等宽族与混排基线对齐验收(无头)。
//!
//! 验收对象:`fonts::FAMILY_EDITOR_MONO`(`editor-mono` 族)——链头与
//! `FontFamily::Monospace` 同为内置 Hack(出厂行 metrics),链尾是 CJK
//! 等宽 face 的「反向 override 副本」(行 metrics 改写为链头同款 em 值,
//! #43 M2 手法反向:预览把链头对齐 CJK、行高随 CJK 变;编辑器把 CJK 对齐
//! 链头、纯 ASCII 行盒几何不变 —— 否决线要求链头 metrics 不可动)。
//! 字号仍走 #23 F3 的 `TextStyle::Monospace` 档投影(`theme::
//! apply_font_size`,按值键控 staleness),族由投影一并写入,源码
//! TextEdit / 行号槽 / Live 活动块经 `FontSelection::Style(Monospace)`
//! 自动跟随。
//!
//! 四组证据:
//! 1. 混排基线:同 galley 内 CJK 与拉丁字形的布局基线差 == 0;对照组
//!    (现状 `Monospace` 族)偏差 +1~+3px —— 量具对病灶敏感,不是恒真;
//! 2. 行盒高一致:纯拉丁 / 纯 CJK / 混排行的行盒高彼此相等,且与旧族
//!    完全相等(链头未动);
//! 3. 视觉尺寸:同一 CJK 探针字形的墨迹高在新旧族完全相等(scale 未动,
//!    大小观感的最终判据留真机目视);
//! 4. 否决线:纯 ASCII 文档 rows 数与行盒高对新族完全不变(①);预览侧
//!    专用族链与代码块共用的 `Monospace` 原生链零改动(②)。
//!
//! 明暗两主题各跑一轮(字体链与主题无关,主题断言按任务书口径保留)。
//! 无 CJK 候选环境如实跳过(#43 口径);真机目视项:坤哥截图同款混排
//! 文档在源码页的基线观感、行号槽/Live 活动块跟随、明暗两主题。

use std::sync::Arc;

use eframe::egui::{self, Color32, FontId, RawInput};

use crate::font_metrics_repro::is_cjk_ideograph;
use crate::fonts;
use crate::theme::{ThemeMode, ThemeSettings};

/// 混排观测行:CJK 与拉丁字母、数字同行(源码页最常见形态)。
const MIXED_LINE: &str = "甲post中文123行Hn一";

/// 纯 ASCII 文档(否决线一):短行 + 超宽折行行 + 空行,覆盖行数与
/// 行盒高两个量。不得含任何非 ASCII 字符。
const ASCII_DOC: &str = concat!(
    "fn main() {\n",
    "    let greeting = \"hello, LaterMD editor typography!\";\n",
    "    println!(\"{greeting}\");\n",
    "}\n",
    "\n",
    "This long line is deliberately longer than the wrap width used by the test so that it ",
    "breaks into several visual rows, and the row count must match exactly between the two ",
    "families because every advance width comes from the identical chain head face.\n",
    "tail line",
);

/// 折行宽(否决线一的 wrap 输入;编辑器 TextEdit 同为 wrap 布局)。
const WRAP_W: f32 = 400.0;

/// #43 口径的无 CJK 环境检测:候选全失配时验收无混排对象,打印原因并
/// 跳过(不硬 panic 红门禁),与 preview_pixel/typography_acceptance 同款。
fn cjk_fonts_missing() -> bool {
    fonts::install(&egui::Context::default()).is_none()
}

/// 生产链路 context:装字体 + 出厂排版偏好投影(字号 15 走 `apply_font_size`,
/// 族随投影写入 `TextStyle::Monospace` 档)。`ctx.fonts` 须先过一帧
/// `run_ui` 才可用(egui 0.36 的 fonts 惰性初始化),与 fonts.rs 测试同款。
fn editor_ctx(dark: bool) -> egui::Context {
    let ctx = egui::Context::default();
    assert!(
        fonts::install(&ctx).is_some(),
        "editor_ctx 的调用方须先经 cjk_fonts_missing() 做无 CJK 跳过(#43 口径)"
    );
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    ThemeSettings::default().apply(
        &ctx,
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
    );
    ctx
}

/// 投影后的编辑器档 FontId(字号与族都从投影读 —— 生产 TextEdit 的取值
/// 路径:`TextStyle::Monospace` 档 + `FontSelection::Style` 解析)。
fn projected_editor_font(ctx: &egui::Context) -> FontId {
    ctx.style_of(ctx.theme())
        .text_styles
        .get(&egui::TextStyle::Monospace)
        .expect("出厂 Monospace 档恒存在")
        .clone()
}

/// 行内「CJK 基线 − 拉丁基线」(#43 M1 取证同式:基线 = placed.pos.y +
/// glyph.pos.y,已含 epaint 的整像素吸附,取各族字形的最大值作代表)。
fn baseline_cjk_minus_latin(galley: &egui::Galley) -> Option<f32> {
    let row = &galley.rows[0].row;
    let max_base = |pick: &dyn Fn(char) -> bool| -> Option<f32> {
        row.glyphs
            .iter()
            .filter(|g| pick(g.chr))
            .map(|g| g.pos.y)
            .fold(None::<f32>, |acc, v| Some(acc.map_or(v, |a| a.max(v))))
    };
    let cjk = max_base(&|c| is_cjk_ideograph(c))?;
    let latin = max_base(&|c| c.is_ascii_alphanumeric())?;
    Some(cjk - latin)
}

/// 指定字符的墨迹高(点;uv 尺寸即光栅化结果,同 face 同字号恒等)。
fn glyph_ink_height(galley: &egui::Galley, chr: char) -> Option<f32> {
    galley
        .rows
        .iter()
        .flat_map(|placed| placed.row.glyphs.iter())
        .find(|g| g.chr == chr)
        .map(|g| g.uv_rect.size.y)
}

fn layout_no_wrap(ctx: &egui::Context, text: &str, font: &FontId) -> Arc<egui::Galley> {
    ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE))
}

fn layout_wrapped(ctx: &egui::Context, text: &str, font: &FontId) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = WRAP_W;
    job.append(
        text,
        0.0,
        egui::TextFormat::simple(font.clone(), Color32::WHITE),
    );
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// 证据 1+2+3:混排基线偏差归零、行盒高一致且不偏离旧族、字形墨迹尺寸
/// 不被改动;全字号档(12-24,滑杆整数步进全域)与明暗两主题。
#[test]
fn mixed_line_baseline_row_height_and_ink_in_both_visuals() {
    if cjk_fonts_missing() {
        eprintln!("本机无 CJK 候选字体,编辑器混排验收无混排对象,跳过");
        return;
    }
    for dark in [true, false] {
        let theme_name = if dark { "暗色" } else { "亮色" };
        let ctx = editor_ctx(dark);
        let editor_font = projected_editor_font(&ctx);
        assert_eq!(
            editor_font.family,
            egui::FontFamily::Name(Arc::from(fonts::FAMILY_EDITOR_MONO)),
            "{theme_name}: 投影后编辑器档应为专用等宽族(#50 M1)"
        );
        assert_eq!(editor_font.size, crate::theme::EDITOR_FONT_SIZE_DEFAULT);

        // 对照组:换族前的 `Monospace`(链头同 face,病灶 witness)。
        let stock_font = FontId::new(editor_font.size, egui::FontFamily::Monospace);
        let new_galley = layout_no_wrap(&ctx, MIXED_LINE, &editor_font);
        let old_galley = layout_no_wrap(&ctx, MIXED_LINE, &stock_font);

        // ① 基线:新族精确归零;对照组必须呈现偏差(量具非恒真)。
        let old_dev = baseline_cjk_minus_latin(&old_galley).expect("混排行应含 CJK 与拉丁");
        let new_dev = baseline_cjk_minus_latin(&new_galley).expect("混排行应含 CJK 与拉丁");
        assert!(
            old_dev >= 1.0,
            "{theme_name}: 对照(现状 Monospace 族)应存在 ≥1px 基线偏差,实测 {old_dev}"
        );
        assert_eq!(
            new_dev, 0.0,
            "{theme_name}: 新族混排基线偏差应精确为 0(实测 {new_dev})"
        );

        // ② 行盒高:混排行与旧族完全相等(链头未动);纯拉丁/纯 CJK/
        //    混排三种行的行盒高彼此一致(行盒高一致,行内不跳变)。
        let row_h =
            |font: &FontId, text: &str| layout_no_wrap(&ctx, text, font).rows[0].rect().height();
        assert_eq!(
            new_galley.rows[0].rect().height(),
            old_galley.rows[0].rect().height(),
            "{theme_name}: 混排行行盒高不得偏离旧族"
        );
        let latin_h = row_h(&editor_font, "only latin 123");
        let cjk_h = row_h(&editor_font, "只有中文一行");
        assert_eq!(latin_h, cjk_h, "{theme_name}: 纯拉丁与纯 CJK 行盒高应一致");
        assert_eq!(
            new_galley.rows[0].rect().height(),
            latin_h,
            "{theme_name}: 混排行行盒高应与纯脚本行一致"
        );

        // ③ 视觉尺寸:同一字形墨迹高在新旧族完全相等 —— 字号观感由 face
        //    设计决定,本族不引入缩放(scale=1);CJK/拉丁的墨迹高比
        //    (Noto ideograph ≈0.88em vs Hack cap ≈0.72em)是 face 设计
        //    值,平衡观感留真机目视。
        let probe = |galley: &egui::Galley, chr: char| {
            glyph_ink_height(galley, chr)
                .unwrap_or_else(|| panic!("{theme_name}: {chr} 未被 shaping"))
        };
        assert_eq!(
            probe(&new_galley, '中'),
            probe(&old_galley, '中'),
            "{theme_name}: CJK 探针字形墨迹高不得被新族改动"
        );
        assert_eq!(
            probe(&new_galley, 'H'),
            probe(&old_galley, 'H'),
            "{theme_name}: 拉丁探针字形墨迹高不得被新族改动"
        );
        eprintln!(
            "[M1 {theme_name}] 基线偏差 旧{old_dev:+.3}px → 新 {new_dev:+.3}px;行盒 {:.2}px;墨迹高 中{:.2}px/H{:.2}px(比 {:.2})",
            new_galley.rows[0].rect().height(),
            probe(&new_galley, '中'),
            probe(&new_galley, 'H'),
            probe(&new_galley, '中') / probe(&new_galley, 'H'),
        );

        // ④ 全字号档:滑杆 12-24 整数步进全域,基线偏差恒为 0 ——
        //    副本表值是 em 改写,字号无关性由公式保证,这里逐档实证。
        for size in (12i16..=24).map(f32::from) {
            let font = FontId::new(size, editor_font.family.clone());
            let galley = layout_no_wrap(&ctx, MIXED_LINE, &font);
            assert_eq!(
                baseline_cjk_minus_latin(&galley),
                Some(0.0),
                "{theme_name}: {size}pt 档混排基线偏差应精确为 0"
            );
        }
    }
}

/// 否决线一:纯 ASCII 文档的 rows 数、逐行行盒高、总高对编辑器专用族
/// **完全不变**(链头 = 同一内置 Hack face,advance 与行 metrics 全等;
/// 折行位置因此逐行一致)。同时钉编辑器行高公式(`ui/editor.rs` 的
/// `row_height + extra_text_line_spacing`)在新族下不变。
#[test]
fn pure_ascii_layout_is_invariant_to_the_editor_family() {
    if cjk_fonts_missing() {
        eprintln!("本机无 CJK 候选字体,编辑器混排验收无混排对象,跳过");
        return;
    }
    assert!(
        ASCII_DOC.is_ascii(),
        "防御:否决线文档必须纯 ASCII(混入 CJK 会失去鉴别力)"
    );
    for dark in [true, false] {
        let theme_name = if dark { "暗色" } else { "亮色" };
        let ctx = editor_ctx(dark);
        let editor_family = projected_editor_font(&ctx).family;
        for size in [12.0, 15.0, 24.0] {
            let new_font = FontId::new(size, editor_family.clone());
            let old_font = FontId::new(size, egui::FontFamily::Monospace);
            let new_galley = layout_wrapped(&ctx, ASCII_DOC, &new_font);
            let old_galley = layout_wrapped(&ctx, ASCII_DOC, &old_font);

            assert_eq!(
                new_galley.rows.len(),
                old_galley.rows.len(),
                "{theme_name} {size}pt: 纯 ASCII 文档 rows 数改变"
            );
            for (idx, (new_row, old_row)) in new_galley
                .rows
                .iter()
                .zip(old_galley.rows.iter())
                .enumerate()
            {
                assert_eq!(
                    new_row.rect().height(),
                    old_row.rect().height(),
                    "{theme_name} {size}pt 第 {idx} 行: 行盒高改变"
                );
            }
            assert_eq!(
                new_galley.size().y,
                old_galley.size().y,
                "{theme_name} {size}pt: 纯 ASCII 文档总高改变"
            );

            // 编辑器行高公式(ui/editor.rs):desired_rows 的输入。
            let line_height = |font: &FontId| {
                ctx.fonts_mut(|f| f.row_height(font))
                    + ctx.style_of(ctx.theme()).spacing.extra_text_line_spacing
            };
            assert_eq!(
                line_height(&new_font),
                line_height(&old_font),
                "{theme_name} {size}pt: 编辑器行高公式输入改变"
            );
            eprintln!(
                "[M1 否决线一 {theme_name} {size}pt] 纯 ASCII:rows={} 总高={:.2}px 行高={:.2}px,新旧族全等",
                new_galley.rows.len(),
                new_galley.size().y,
                line_height(&new_font),
            );
        }
    }
}

/// 否决线二:预览侧零改动 —— 预览正文族仍是 #43 M2 的 `Inter-Preview`
/// 副本族且混排基线仍为 0;`bold` 别名族、代码块共用的 `Monospace` 原生
/// 链、以及预览链涉及的全部 font_data(无 tweak)分毫不动。编辑器族的
/// 引入不得外溢到预览渲染链路。
#[test]
fn preview_side_is_untouched_by_the_editor_family() {
    if cjk_fonts_missing() {
        eprintln!("本机无 CJK 候选字体,编辑器混排验收无混排对象,跳过");
        return;
    }
    let ctx = editor_ctx(true);
    assert_eq!(
        fonts::preview_body_family(&ctx),
        egui::FontFamily::Name(Arc::from(fonts::FAMILY_PREVIEW_BODY)),
        "预览正文族仍是 Inter-Preview 副本族"
    );
    ctx.fonts(|f| {
        let defs = f.definitions();
        // 预览正文族链:链头 override Inter 副本,链尾原生 CJK 比例 face
        // (键名为 fonts.rs 私有常量的字面值,改动时应同步本测试)。
        let preview =
            &defs.families[&egui::FontFamily::Name(Arc::from(fonts::FAMILY_PREVIEW_BODY))];
        assert_eq!(
            preview.first().map(String::as_str),
            Some("Inter-Regular-Preview")
        );
        assert_eq!(
            preview.last().map(String::as_str),
            Some("latermd-cjk-proportional")
        );
        // `bold` 别名族链头仍是 SemiBold 副本。
        let bold = &defs.families[&egui::FontFamily::Name(Arc::from(fonts::FAMILY_BOLD))];
        assert_eq!(
            bold.first().map(String::as_str),
            Some("Inter-SemiBold-Preview")
        );
        assert_eq!(
            bold.last().map(String::as_str),
            Some("latermd-cjk-proportional")
        );
        // 代码块(vendored `FontId::monospace` 硬编码)共用的原生等宽链:
        // 链头 Hack、链尾原生 CJK 等宽 —— 预览代码块渲染分毫不动。
        let mono = &defs.families[&egui::FontFamily::Monospace];
        assert_eq!(mono.first().map(String::as_str), Some("Hack"));
        assert_eq!(
            mono.last().map(String::as_str),
            Some("latermd-cjk-monospace")
        );
        assert!(
            !mono.iter().any(|n| n == "latermd-cjk-monospace-editor"),
            "原生等宽链不得混入编辑器副本"
        );
        // 预览链涉及的 font_data 一概无 tweak(编辑器侧改动不得外溢)。
        for key in [
            "Hack",
            "latermd-cjk-monospace",
            "latermd-cjk-proportional",
            "Inter-Regular-Preview",
            "Inter-SemiBold-Preview",
        ] {
            assert_eq!(
                defs.font_data[key].tweak,
                egui::FontTweak::default(),
                "{key}: 预览链字体不得携带 tweak"
            );
        }
    });

    // 预览混排基线仍为 0(#43 M2 行为保留,编辑器族引入后不变)。
    let font = FontId::new(
        crate::theme::editor_font_size(&ctx),
        fonts::preview_body_family(&ctx),
    );
    let galley = layout_no_wrap(&ctx, MIXED_LINE, &font);
    assert_eq!(
        baseline_cjk_minus_latin(&galley),
        Some(0.0),
        "预览混排基线应保持 #43 M2 的归零状态"
    );
}
