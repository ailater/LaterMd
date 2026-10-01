//! #43 M1 混排最小复现与根因定位(取证式,无头)——只产测试与证据,修复在 M2。
//!
//! 用户三症状(2026-09-30 反馈,docs/auto-plan.md #43 条目):
//! 「中文英文数字高低不一,行高也有问题,有显示不全的问题」。
//! 本模块把症状转成可断言的无头测试(正文/粗体/斜体/标题四态 × 中英数字混排)。
//!
//! ## 机制链(证据见各测试;行号引用 egui 0.36.2 / vendored HEAD)
//!
//! 1. **高低不一 = 混排基线偏差,四态全中**。egui 行 metrics 取**链头** face
//!    (`Font::styled_metrics`,epaint text/font.rs:691-703);fallback 字形
//!    (CJK 走链尾 Noto)的基线由
//!    `face_ascent + valign*(行高−line_height) + 0.5*(链头行高−face行高)`
//!    决定(epaint text/text_layout.rs:977-981,`0.5*` 是把差值**取中点**的
//!    近似对齐,不是基线对齐)。Inter(ascent 0.9688em / 行高 1.2100em)与
//!    Noto Sans CJK SC(1.1600em / 1.4480em,hhea)的 ascent 占比不同,残差
//!    `(1.16−0.9688) − (1.448−1.21)/2 = 0.0722em`,再经 `round_to_pixel`
//!    (整像素吸附,text_layout.rs:986)量化放大为 1-2px 的可见错位。
//! 2. **行高问题 = 行距公式是「字号 × 单一比例」**,不是行内实际字形 metrics。
//!    vendored `line_height_for` = `size × line_height_ratio`(默认 1.30,
//!    egui_markdown_style style.rs:59-75),`text_format` 塞进
//!    `TextFormat::line_height` 覆盖 epaint 原生行高(egui_markdown layout.rs:21-23,
//!    77-80);egui 行盒取 `max(glyph.line_height)`(text_layout.rs:964-971)
//!    → CJK face 需要 1.448em,行盒只有 1.30em,**按 em 盒论越界 0.148em**
//!    (13pt ≈1.9px,H1 ≈3.1px)。1.30 也是拉丁调校的行距比例,对 CJK 排版
//!    常规(≥1.5em)偏紧,「行高也有问题」的观感由两者叠加。
//! 3. **显示不全** = 行盒不足时 egui 不裁字形,只按行盒高推进下一行
//!    (text_layout.rs:989-992):越界墨迹与相邻行的墨迹/后续块的不透明背景
//!    (代码块底色、表格底色等)相互侵入遮挡。实墨是否真越界由
//!    `mixed_script_strict_regression_target` 用 `uv_rect` 实测裁决。
//!
//! ## 字形 metrics 的两种度量(测试里都有,勿混用)
//! - **em 盒**:face 的 `[−ascent, +|descent|+lineGap]`
//!   (`Glyph::font_face_ascent/font_face_height`)——epaint 基线公式消费的
//!   就是它。注意 Latin 字形实墨远小于 em 盒(cap≈0.73em),CJK 表意字符
//!   实墨 ≈ `[−0.08, +0.84]em`,而 CJK em 盒高达 `[−0.288, +1.16]em`
//!   (Noto hhea 对全脚本堆叠超配)。
//! - **实墨**:光栅化字形 quad(`Glyph::uv_rect.offset/size`,tessellate_glyphs
//!   按 `glyph.pos + uv_rect.offset` 贴,epaint text_layout.rs:1159)——
//!   「显示不全」的最终裁决度量。
//!
//! ## 断言归宿(#43 公共约束:门禁全程不许红着交付)
//! - `font_table_metrics_forensics`:字体表值断言(**绿**),锚定 Inter/Noto
//!   的 ascent/descent/line_gap 实际值与 skrifa 的取表策略。
//! - `mixed_script_galley_metrics_forensics`:现状值断言(**绿**)——实测与
//!   由表值+epaint 公式的推算逐项对账,把三症状的量化值钉进测试;每个
//!   断言处带 TODO(M2) 标出修复后的收紧点。
//! - `mixed_script_strict_regression_target`:**#[ignore]**,理想态断言
//!   (四态基线偏差 ≤0.5px、行盒双度量覆盖、相邻行墨迹净空 ≥0)——现状必红
//!   = 复现成功;M2 修复合入时移除 `#[ignore]`,转绿后作回归护栏。

use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId, RawInput, Rect, UiBuilder, Vec2};

use crate::fonts;

/// 预览默认正文字号:egui 0.36.2 出厂 Body(egui style.rs:1419)。
/// 预览侧不显式设字号(`MarkdownLabel` 无 `.font`),走 Default → Body。
const BODY_SIZE: f32 = 13.0;
/// vendored `MarkdownStyle::line_height_ratio` 默认值(egui_markdown_style style.rs:59-75)。
const LINE_HEIGHT_RATIO: f32 = 1.30;
/// vendored 标题字号缩放(egui_markdown_style style.rs:319,H1-H6)。
const HEADING_SCALES: [f32; 6] = [1.6, 1.35, 1.2, 1.1, 1.05, 1.0];
/// epaint 的 UI 量化网格(emath gui_rounding.rs:17;styled_metrics 按 1/32 吸附)。
const GUI_ROUNDING: f32 = 1.0 / 32.0;
/// 严格断言的基线偏差阈值:0.5px。半像素以内人眼不可辨;egui 对基线做
/// 整像素吸附(`round_to_pixel`),亚像素残差一旦超过 0.5px 就必跳 1px。
/// 任务书参考值 1px 已可辨(正文 13pt 现状即 1px),故严格线取 0.5px。
const STRICT_BASELINE_TOLERANCE: f32 = 0.5;
/// 实墨越界的判定容差:亚像素(光栅化 quad 与行盒都是整像素/1/32 网格值)。
const STRICT_INK_EPSILON: f32 = 0.01;
/// 测量对账容差:推算与实测都落在同一量化网格上,只容忍 ulp 级噪声。
const FORENSIC_EPSILON: f32 = 1.0 / 64.0;

fn round_ui(v: f32) -> f32 {
    (v / GUI_ROUNDING).round() * GUI_ROUNDING
}

fn assert_approx(a: f32, b: f32, eps: f32, msg: &str) {
    assert!((a - b).abs() <= eps, "{msg}: |{a:.4} − {b:.4}| > {eps:.4}");
}

fn is_cjk_ideograph(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}')
}

fn is_latin_alnum(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

// ---------------------------------------------------------------------------
// 取证 1:字体文件表值(Inter 与 CJK 回退的 ascent/descent/line_gap 实际值)
// ---------------------------------------------------------------------------

/// sfnt 垂直排印相关的表值集合(font units,大端)。
#[derive(Clone, Copy, Debug, PartialEq)]
struct VerticalTables {
    units_per_em: u16,
    /// hhea (ascender, descender, lineGap)。
    hhea: (i16, i16, i16),
    /// OS/2 typo (sTypoAscender, sTypoDescender, sTypoLineGap)。
    typo: (i16, i16, i16),
    /// OS/2 win (usWinAscent, usWinDescent)。
    win: (u16, u16),
    /// OS/2 fsSelection bit7(USE_TYPO_METRICS)。
    use_typo_metrics: bool,
}

impl VerticalTables {
    /// skrifa 0.44(= epaint 0.36.2 的依赖,FreeType 同款策略,skrifa
    /// metrics.rs:142-190)最终采纳的行 metrics:
    /// fsSelection bit7 置位 → OS/2 typo;否则 hhea;hhea 双零才回落 typo/win。
    fn selected(&self) -> (i16, i16, i16) {
        if self.use_typo_metrics {
            self.typo
        } else {
            self.hhea
        }
    }

    fn ascent_em(&self) -> f32 {
        self.selected().0 as f32 / self.units_per_em as f32
    }

    fn descent_em(&self) -> f32 {
        self.selected().1 as f32 / self.units_per_em as f32
    }

    fn line_gap_em(&self) -> f32 {
        self.selected().2 as f32 / self.units_per_em as f32
    }

    /// `ascent − descent + line_gap`(epaint StyledMetrics.row_height 同式)。
    fn row_height_em(&self) -> f32 {
        self.ascent_em() - self.descent_em() + self.line_gap_em()
    }
}

fn be_u16(data: &[u8], offset: usize) -> Option<u16> {
    let r = data.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([r[0], r[1]]))
}

fn be_i16(data: &[u8], offset: usize) -> Option<i16> {
    be_u16(data, offset).map(|v| v as i16)
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let r = data.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([r[0], r[1], r[2], r[3]]))
}

/// 解析一个 sfnt face(支持 .ttc 的 face index)的垂直排印表值。
/// 只读表头与固定字段,不解构 outline;失败返回 `None`(文件残缺/非 sfnt)。
fn parse_vertical_tables(data: &[u8], face_index: u32) -> Option<VerticalTables> {
    // ttcf: tag(u32) + version(u32) + numFonts(u32) + offsets[numFonts](u32);
    // 普通 sfnt: face 偏移 0。
    let face_offset = if data.get(0..4)? == b"ttcf" {
        let num_fonts = be_u32(data, 8)? as usize;
        let idx = face_index as usize;
        if idx >= num_fonts {
            return None;
        }
        be_u32(data, 12 + idx * 4)? as usize
    } else {
        0
    };
    let num_tables = be_u16(data, face_offset + 4)? as usize;
    let mut head = None;
    let mut hhea = None;
    let mut os2 = None;
    for i in 0..num_tables {
        let rec = face_offset + 12 + i * 16;
        let tag = data.get(rec..rec + 4)?;
        let offset = be_u32(data, rec + 8)? as usize;
        match tag {
            b"head" => head = Some(offset),
            b"hhea" => hhea = Some(offset),
            b"OS/2" => os2 = Some(offset),
            _ => {}
        }
    }
    let head = head?;
    let units_per_em = be_u16(data, head + 18)?;
    // hhea: version(u32) + ascender(i16)+4 + descender(i16)+6 + lineGap(i16)+8。
    let hhea_off = hhea?;
    let hhea_metrics = (
        be_i16(data, hhea_off + 4)?,
        be_i16(data, hhea_off + 6)?,
        be_i16(data, hhea_off + 8)?,
    );
    // OS/2: fsSelection(u16)+62、typo 三元组(i16)+68/70/72、win 二元组(u16)+74/76。
    let (typo, win, use_typo_metrics) = match os2 {
        Some(os2) => (
            (
                be_i16(data, os2 + 68)?,
                be_i16(data, os2 + 70)?,
                be_i16(data, os2 + 72)?,
            ),
            (be_u16(data, os2 + 74)?, be_u16(data, os2 + 76)?),
            be_u16(data, os2 + 62)? & (1 << 7) != 0,
        ),
        // 无 OS/2: skrifa 直接用 hhea。
        None => (hhea_metrics, (0, 0), false),
    };
    Some(VerticalTables {
        units_per_em,
        hhea: hhea_metrics,
        typo,
        win,
        use_typo_metrics,
    })
}

/// Inter 表值预期:upem 2048,hhea 与 OS/2 typo 同值(1984, −494, 0),
/// fsSelection bit7 置位 → 走 OS/2 typo(数值与 hhea 一致,无歧义)。
/// 断言它锚定「链头 metrics」的来源;Inter 资产换版本时此断言提醒重新量化。
#[test]
fn font_table_metrics_forensics() {
    for (name, bytes) in [
        (
            "Inter-Regular",
            include_bytes!("../../../assets/fonts/Inter-Regular.ttf") as &[u8],
        ),
        (
            "Inter-SemiBold",
            include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf") as &[u8],
        ),
    ] {
        let tables = parse_vertical_tables(bytes, 0).unwrap_or_else(|| panic!("{name} 解析失败"));
        assert_eq!(tables.units_per_em, 2048, "{name} upem 与预期不符");
        assert_eq!(tables.hhea, (1984, -494, 0), "{name} hhea 与预期不符");
        assert_eq!(tables.typo, (1984, -494, 0), "{name} OS/2 typo 与预期不符");
        assert!(
            tables.use_typo_metrics,
            "{name} 应置位 USE_TYPO_METRICS(bit7)"
        );
        assert_eq!(
            tables.selected(),
            (1984, -494, 0),
            "{name} 采纳表与预期不符"
        );
        // em 换算(打印值进失败信息,取证即文档)。
        assert_approx(
            tables.ascent_em(),
            0.96875,
            1e-4,
            &format!("{name} ascent/em"),
        );
        assert_approx(
            tables.descent_em(),
            -494.0 / 2048.0,
            1e-4,
            &format!("{name} descent/em"),
        );
        assert_approx(
            tables.line_gap_em(),
            0.0,
            1e-4,
            &format!("{name} lineGap/em"),
        );
        // 行高 = (1984 + 494) / 2048 em ≈ 1.20996。
        assert_approx(
            tables.row_height_em(),
            2478.0 / 2048.0,
            1e-4,
            &format!("{name} row_height/em"),
        );
    }

    // CJK 回退侧:本机无候选字体时如实跳过(CI runner 可能没有;本机已命中)。
    let Some((path, prop_idx, mono_idx)) = fonts::cjk_source_for_test() else {
        eprintln!("本机无 CJK 候选字体,Noto 表值断言跳过");
        return;
    };
    let data = std::fs::read(path).expect("候选字体文件读取失败(存在性已探测)");
    // 比例 face 与等宽 face 的垂直表值应当一致(同族不同字宽变体)。
    for idx in [prop_idx, mono_idx] {
        let tables = parse_vertical_tables(&data, idx)
            .unwrap_or_else(|| panic!("{path} face {idx} 解析失败"));
        assert_eq!(
            tables.units_per_em, 1000,
            "{path} face {idx} upem 与预期不符"
        );
        assert_eq!(
            tables.hhea,
            (1160, -288, 0),
            "{path} face {idx} hhea 与预期不符"
        );
        assert_eq!(
            tables.typo,
            (880, -120, 0),
            "{path} face {idx} OS/2 typo 与预期不符"
        );
        assert_eq!(
            tables.win,
            (1160, 288),
            "{path} face {idx} OS/2 win 与预期不符"
        );
        // 关键:Noto CJK 的 fsSelection 不含 bit7 → skrifa 采纳 hhea(1.448em),
        // 而不是更紧凑的 OS/2 typo(1.0em)。CJK 的 em 盒因此显著高于 Inter。
        assert!(
            !tables.use_typo_metrics,
            "{path} face {idx} 不应置位 USE_TYPO_METRICS(置位则 hhea 断言失真)"
        );
        assert_eq!(
            tables.selected(),
            (1160, -288, 0),
            "{path} face {idx} 采纳表与预期不符"
        );
        assert_approx(
            tables.ascent_em(),
            1.16,
            1e-4,
            &format!("{path} face {idx} ascent/em"),
        );
        assert_approx(
            tables.descent_em(),
            -0.288,
            1e-4,
            &format!("{path} face {idx} descent/em"),
        );
        assert_approx(
            tables.line_gap_em(),
            0.0,
            1e-4,
            &format!("{path} face {idx} lineGap/em"),
        );
        assert_approx(
            tables.row_height_em(),
            1.448,
            1e-4,
            &format!("{path} face {idx} row_height/em"),
        );
    }
}

// ---------------------------------------------------------------------------
// 取证 2:galley 侧(实测与推算逐项对账,把三症状量化值钉进现状断言)
// ---------------------------------------------------------------------------

/// 一个 face 在具体字号下的行 metrics(round_ui 网格上;与 epaint
/// `StyledMetrics` 的 ascent/row_height 同一量化)。
#[derive(Clone, Copy, Debug, PartialEq)]
struct FaceMetrics {
    ascent: f32,
    row_height: f32,
}

/// 从 `styled_metrics` 的同式推算(skrifa px_scale_factor = size/upem,
/// epaint text/font.rs:562-565 逐项 round_ui)。
fn styled_metrics(size: f32, tables: &VerticalTables) -> FaceMetrics {
    let scale = size / tables.units_per_em as f32;
    let ascent = round_ui(tables.selected().0 as f32 * scale);
    let descent = round_ui(tables.selected().1 as f32 * scale);
    let line_gap = round_ui(tables.selected().2 as f32 * scale);
    FaceMetrics {
        ascent,
        row_height: ascent - descent + line_gap,
    }
}

/// epaint galley_from_rows 的行高:吸到整像素(headless ppp=1)。
fn snapped_row_height(needed: f32) -> f32 {
    needed.round()
}

/// epaint 基线公式(text_layout.rs:977-981;vendored `text_format` 是
/// valign=BOTTOM → factor 1.0,line_height = size×ratio 的原始值,行高是
/// 吸附后的整像素值)。
fn predicted_baseline(
    face: FaceMetrics,
    head: FaceMetrics,
    row_height: f32,
    line_height: f32,
) -> f32 {
    (face.ascent + (row_height - line_height) + 0.5 * (head.row_height - face.row_height)).round()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ScriptFace {
    /// 链头拉丁 face(Inter / Inter-SemiBold):拉丁字符与数字由它 shaping。
    LatinChainHead,
    /// CJK 回退 face(Noto Sans CJK SC):汉字由它 shaping。
    CjkFallback,
    /// 其他(标点/空格等未参与断言的字符)。
    Other,
}

/// 单个 glyph 的取证快照(行相对基线换算为 galley 坐标)。
#[derive(Clone, Copy, Debug)]
struct GlyphForensics {
    ch: char,
    face: ScriptFace,
    /// 基线(galley 坐标,行顶 + 行相对 glyph.pos.y)。
    baseline: f32,
    /// em 盒(基线起算):`[baseline − face_ascent, baseline + face行高 − face_ascent]`。
    em_box: (f32, f32),
    /// 实墨(uv_rect 起算,基线相对 offset/size 换算到 galley 坐标)。
    /// `(top, bottom)`;字形未进 atlas(如空白)时为 `None`。
    ink: Option<(f32, f32)>,
    /// vendored 的行高覆盖值(size × 1.30,未吸像素)。
    line_height: f32,
    face_ascent: f32,
    face_row_height: f32,
    head_row_height: f32,
}

#[derive(Clone, Debug)]
struct RowForensics {
    text: String,
    /// 行盒(galley 坐标,`[top, bottom]`;bottom = top + row.size.y)。
    top: f32,
    bottom: f32,
    glyphs: Vec<GlyphForensics>,
    /// 行内字形的最大 face 行高(em 盒口径的「实际字形 metrics」需求)。
    max_face_row_height: f32,
    /// 行内实墨的极值范围(仅计已进 atlas 的字形)。
    ink_extent: Option<(f32, f32)>,
    /// `Some(CJK基线最大值 − 拉丁基线最大值)`,仅当该行同时含两种字形。
    baseline_cjk_minus_latin: Option<f32>,
    /// 行内 glyph 相对行盒的最大越界(正值 = 越界像素;em 盒口径)。
    overflow_top_em: f32,
    overflow_bottom_em: f32,
}

impl RowForensics {
    fn height(&self) -> f32 {
        self.bottom - self.top
    }
}

/// 把 galley 的每行/每字形换算成取证快照。分类靠 metrics 数值对账:
/// 拉丁探针与 CJK 探针各用链头字符 / 汉字 shaping 一次,行内 glyph 与之比对。
fn collect_row_forensics(
    galley: &Arc<egui::epaint::text::Galley>,
    latin_probe: FaceMetrics,
    cjk_probe: Option<FaceMetrics>,
) -> Vec<RowForensics> {
    galley
        .rows
        .iter()
        .map(|placed| {
            let top = placed.pos.y;
            let bottom = top + placed.row.size.y;
            let mut out = RowForensics {
                text: placed.row.text(),
                top,
                bottom,
                glyphs: Vec::new(),
                max_face_row_height: 0.0,
                ink_extent: None,
                baseline_cjk_minus_latin: None,
                overflow_top_em: f32::NEG_INFINITY,
                overflow_bottom_em: f32::NEG_INFINITY,
            };
            for glyph in &placed.row.glyphs {
                let face_ascent = glyph.font_face_ascent;
                let face_row_height = glyph.font_face_height;
                let face = if approx(face_ascent, latin_probe.ascent)
                    && approx(face_row_height, latin_probe.row_height)
                {
                    ScriptFace::LatinChainHead
                } else if cjk_probe.is_some_and(|probe| {
                    approx(face_ascent, probe.ascent) && approx(face_row_height, probe.row_height)
                }) {
                    ScriptFace::CjkFallback
                } else {
                    ScriptFace::Other
                };
                let baseline = placed.pos.y + glyph.pos.y;
                let em_box = (
                    baseline - face_ascent,
                    baseline + (face_row_height - face_ascent),
                );
                let ink = if glyph.uv_rect.offset == Vec2::ZERO && glyph.uv_rect.size == Vec2::ZERO
                {
                    None
                } else {
                    let t = baseline + glyph.uv_rect.offset.y;
                    Some((t, t + glyph.uv_rect.size.y))
                };
                out.max_face_row_height = out.max_face_row_height.max(face_row_height);
                if let Some((t, b)) = ink {
                    let extent = out.ink_extent.unwrap_or((t, b));
                    out.ink_extent = Some((extent.0.min(t), extent.1.max(b)));
                }
                out.overflow_top_em = out.overflow_top_em.max(top - em_box.0);
                out.overflow_bottom_em = out.overflow_bottom_em.max(em_box.1 - bottom);
                out.glyphs.push(GlyphForensics {
                    ch: glyph.chr,
                    face,
                    baseline,
                    em_box,
                    ink,
                    line_height: glyph.line_height,
                    face_ascent,
                    face_row_height,
                    head_row_height: glyph.font_height,
                });
            }
            let latin_base = out
                .glyphs
                .iter()
                .filter(|g| g.face == ScriptFace::LatinChainHead && is_latin_alnum(g.ch))
                .map(|g| g.baseline)
                .fold(None::<f32>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
            let cjk_base = out
                .glyphs
                .iter()
                .filter(|g| g.face == ScriptFace::CjkFallback && is_cjk_ideograph(g.ch))
                .map(|g| g.baseline)
                .fold(None::<f32>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
            if let (Some(l), Some(c)) = (latin_base, cjk_base) {
                out.baseline_cjk_minus_latin = Some(c - l);
            }
            out
        })
        .collect()
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.01
}

/// 一行取证的紧凑描述(断言失败信息里逐行可读)。
fn describe_rows(rows: &[RowForensics]) -> String {
    rows.iter()
        .map(|r| {
            let format_g = |g: &GlyphForensics| {
                let ink = g
                    .ink
                    .map(|(t, b)| format!("[{t:.2},{b:.2}]"))
                    .unwrap_or_else(|| "[]".into());
                format!(
                    "{}:{:?}@{}(em[{},{}],ink{},lh{})",
                    escape(g.ch),
                    g.face,
                    g.baseline,
                    g.em_box.0,
                    g.em_box.1,
                    ink,
                    g.line_height
                )
            };
            format!(
                "  row「{}」[{},{}]h={:.2} maxFaceRow={:.2} Δbase={:?} 越界em(top{:+.2},b{:+.2})\n{}",
                r.text.chars().take(12).collect::<String>(),
                r.top,
                r.bottom,
                r.height(),
                r.max_face_row_height,
                r.baseline_cjk_minus_latin,
                r.overflow_top_em,
                r.overflow_bottom_em,
                r.glyphs
                    .iter()
                    .take(6)
                    .map(format_g)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect()
}

fn escape(c: char) -> String {
    if c.is_control() || c == ' ' {
        format!("{:?}", c)
    } else {
        c.to_string()
    }
}

/// 四态用例(正文/粗体/斜体/标题 H1/H2),混排同一句式:中英数字同现。
struct CaseSpec {
    name: &'static str,
    md: &'static str,
    /// 正文侧期望字号(标题按 HEADING_SCALES 缩放)。
    size: f32,
}

const CASES: &[CaseSpec] = &[
    CaseSpec {
        name: "正文",
        md: "正文 Body 中文 123 数字 abc 混排 Test 高低不一",
        size: BODY_SIZE,
    },
    CaseSpec {
        name: "粗体",
        md: "**粗体 Bold 加粗 456 数字 ABC 混排**",
        size: BODY_SIZE,
    },
    CaseSpec {
        name: "斜体",
        md: "*斜体 Italic 斜体 789 数字 def 混排*",
        size: BODY_SIZE,
    },
    CaseSpec {
        name: "H1",
        md: "# 标题 Heading 一 中文 H1 123",
        size: BODY_SIZE * HEADING_SCALES[0],
    },
    CaseSpec {
        name: "H2",
        md: "## 二级标题 H2 中文 abc 456",
        size: BODY_SIZE * HEADING_SCALES[1],
    },
];

/// 足够长以在 700px 内折行(相邻行墨迹侵入观测)。
const WRAP_DOC: &str = "中文连续行观测相邻行墨迹净空中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文中文";

/// 无头渲染一个 markdown 片段,返回其 galley(`layout_in_ui` 与预览的
/// `show()` 走同一 `build_layout` + `layout_job` 管线;此处只取形,不绘制)。
fn render_case(ctx: &egui::Context, id_salt: &str, md: &str) -> Arc<egui::epaint::text::Galley> {
    let mut captured: Option<Arc<egui::epaint::text::Galley>> = None;
    let mut output = ctx.run_ui(RawInput::default(), |panel| {
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(700.0, 400.0));
        let mut child = panel.new_child(UiBuilder::new().max_rect(screen));
        let style = crate::theme::default_markdown_style();
        let font = FontId::new(BODY_SIZE, FontFamily::Proportional);
        let (_, galley, _) = egui_markdown::MarkdownLabel::new(egui::Id::new(id_salt), md)
            .font(font)
            .style(&style)
            .wrap()
            .layout_in_ui(&mut child);
        captured = Some(galley);
    });
    output.textures_delta.clear();
    captured.expect("layout_in_ui 应产出 galley")
}

/// 逐行摘要(跨帧稳定性断言的指纹;不含浮点全量,只取量化网格上的关键量)。
fn geometry_fingerprint(rows: &[RowForensics]) -> Vec<(String, f32, Option<f32>, f32, f32, f32)> {
    rows.iter()
        .map(|r| {
            (
                r.text.clone(),
                r.height(),
                r.baseline_cjk_minus_latin,
                r.max_face_row_height,
                r.overflow_top_em,
                r.overflow_bottom_em,
            )
        })
        .collect()
}

/// 四态 × 混排的现状量化与「推算 == 实测」对账(**现状断言,绿**;
/// 每个 TODO(M2) 处标注修复后的收紧点)。
#[test]
fn mixed_script_galley_metrics_forensics() {
    let ctx = egui::Context::default();
    let has_cjk = fonts::install(&ctx).is_some();
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    if !has_cjk {
        eprintln!("本机无 CJK 候选字体,galley 侧 CJK 断言跳过");
        return;
    }

    // 表值侧(链头与 fallback 两侧的 ground truth)。
    let inter = parse_vertical_tables(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"), 0)
        .expect("Inter-Regular 表解析失败");
    let (cjk_path, cjk_prop_idx, _) = fonts::cjk_source_for_test().expect("已确认本机有 CJK");
    let cjk_bytes = std::fs::read(cjk_path).expect("CJK 候选读取失败");
    let noto = parse_vertical_tables(&cjk_bytes, cjk_prop_idx).expect("Noto 表解析失败");
    let semibold = parse_vertical_tables(
        include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf") as &[u8],
        0,
    )
    .expect("Inter-SemiBold 表解析失败");

    // galley 探针:链头字符与汉字各 shaping 一次,取 face 实际值。
    let body_font = FontId::new(BODY_SIZE, FontFamily::Proportional);
    let probe = |font: &FontId, ch: char| {
        let galley =
            ctx.fonts_mut(|f| f.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE));
        let g = &galley.rows[0].row.glyphs[0];
        FaceMetrics {
            ascent: g.font_face_ascent,
            row_height: g.font_face_height,
        }
    };
    let inter_probe = probe(&body_font, 'H');
    let noto_probe = probe(&body_font, '中');

    // 探针与表值推算对账(skrifa 策略 + round_ui 网格)。
    assert_approx(
        inter_probe.ascent,
        styled_metrics(BODY_SIZE, &inter).ascent,
        FORENSIC_EPSILON,
        "Inter 探针 ascent 与表值推算不符",
    );
    assert_approx(
        inter_probe.row_height,
        styled_metrics(BODY_SIZE, &inter).row_height,
        FORENSIC_EPSILON,
        "Inter 探针行高与表值推算不符",
    );
    assert_approx(
        noto_probe.ascent,
        styled_metrics(BODY_SIZE, &noto).ascent,
        FORENSIC_EPSILON,
        "Noto 探针 ascent 与表值推算不符",
    );
    assert_approx(
        noto_probe.row_height,
        styled_metrics(BODY_SIZE, &noto).row_height,
        FORENSIC_EPSILON,
        "Noto 探针行高与表值推算不符",
    );

    // 全族回退链核查(PR #52 前科:每个 FontFamily::Name 是独立回退链,
    // bold/权重族漏挂 CJK 会整行变方块;此处按 shaping 实测,比链表断言强)。
    let bold_font = FontId::new(BODY_SIZE, FontFamily::Name(Arc::from(fonts::FAMILY_BOLD)));
    let semibold_font = FontId::new(
        BODY_SIZE,
        FontFamily::Name(Arc::from(fonts::FAMILY_SEMIBOLD)),
    );
    for (family_name, font) in [
        ("bold 别名族(预览标题/加粗)", &bold_font),
        ("Inter-SemiBold 族", &semibold_font),
    ] {
        let head = probe(font, 'H');
        let cjk = probe(font, '中');
        // 链头是 Inter-SemiBold(与 Regular 同 upem/同表值)。
        assert_approx(
            head.ascent,
            styled_metrics(BODY_SIZE, &semibold).ascent,
            FORENSIC_EPSILON,
            &format!("{family_name} 链头应为 Inter-SemiBold"),
        );
        assert_approx(
            cjk.row_height,
            noto_probe.row_height,
            FORENSIC_EPSILON,
            &format!("{family_name} 汉字应回落 Noto(链尾 CJK 生效)"),
        );
        assert!(
            ctx.fonts_mut(|f| f.has_glyphs(font, "中文标题")),
            "{family_name} 的 CJK 回退链不完整(方块风险,PR #52 同类)"
        );
    }
    let mono_font = FontId::new(BODY_SIZE, FontFamily::Monospace);
    let mono_cjk = probe(&mono_font, '中');
    assert_approx(
        mono_cjk.row_height,
        styled_metrics(BODY_SIZE, &noto).row_height,
        FORENSIC_EPSILON,
        "Monospace 族的 CJK 回退应为 Noto Mono CJK(同表值)",
    );

    // 四态逐案对账:行高/基线偏差/越界,推算 == 实测。探针按字号逐案取
    // (face metrics 是字号的函数,跨字号比对会分类失败)。
    for case in CASES {
        let galley = render_case(&ctx, &format!("m1-forensics-{}", case.name), case.md);
        let probe_at_size = probe(&FontId::new(case.size, FontFamily::Proportional), 'H');
        let noto_at_size_probe = probe(&FontId::new(case.size, FontFamily::Proportional), '中');
        let rows = collect_row_forensics(&galley, probe_at_size, Some(noto_at_size_probe));
        let content_rows: Vec<&RowForensics> =
            rows.iter().filter(|r| !r.glyphs.is_empty()).collect();
        assert!(!content_rows.is_empty(), "{}: 无内容行", case.name);
        println!("== {} ==\n{}", case.name, describe_rows(&rows));

        let head = styled_metrics(case.size, &inter);
        let noto_at_size = styled_metrics(case.size, &noto);
        let line_height = case.size * LINE_HEIGHT_RATIO;
        let row_height = snapped_row_height(line_height);

        for row in &content_rows {
            // 行高 = 吸附后的 size×1.30(现状:vendored 覆盖,egui 行盒取 max(line_height))。
            assert_approx(
                row.height(),
                row_height,
                FORENSIC_EPSILON,
                &format!("{} 行「{}」行高与推算不符", case.name, row.text),
            );
            // TODO(M2,#43): 现状 = 吸附(size×1.30);修复后收紧为
            // `row.height() ≥ 行内 max_face_row_height`(CJK 1.448em 需求被覆盖)。
            // 现状缺口 = 1.448em − 1.30em ≈ 0.148em(13pt ≈1.9px,H1 ≈3.1px)。
            let latin = row
                .glyphs
                .iter()
                .find(|g| g.face == ScriptFace::LatinChainHead && is_latin_alnum(g.ch))
                .unwrap_or_else(|| panic!("{} 行「{}」缺拉丁字形", case.name, row.text));
            let cjk = row
                .glyphs
                .iter()
                .find(|g| g.face == ScriptFace::CjkFallback && is_cjk_ideograph(g.ch))
                .unwrap_or_else(|| panic!("{} 行「{}」缺 CJK 字形", case.name, row.text));
            // 机制实锚:行内每个字形(含 CJK fallback 字形)携带的链头行高
            // 都是 Inter 的 —— 行高与基线锚定全部取链头,与 face 无关。
            for g in [latin, cjk] {
                assert_approx(
                    g.head_row_height,
                    head.row_height,
                    FORENSIC_EPSILON,
                    &format!("{}「{}」字形链头行高应恒为 Inter 行高", case.name, row.text),
                );
                assert_approx(
                    g.line_height,
                    line_height,
                    FORENSIC_EPSILON,
                    &format!(
                        "{}「{}」字形 line_height 应为 vendored 覆盖值",
                        case.name, row.text
                    ),
                );
            }
            // 基线 = epaint 公式逐项复算(现状断言:公式即真凶的实证)。
            assert_approx(
                latin.baseline - row.top,
                predicted_baseline(
                    FaceMetrics {
                        ascent: latin.face_ascent,
                        row_height: latin.face_row_height,
                    },
                    head,
                    row_height,
                    latin.line_height,
                ),
                FORENSIC_EPSILON,
                &format!("{} 拉丁基线与公式推算不符", case.name),
            );
            assert_approx(
                cjk.baseline - row.top,
                predicted_baseline(
                    FaceMetrics {
                        ascent: cjk.face_ascent,
                        row_height: cjk.face_row_height,
                    },
                    head,
                    row_height,
                    cjk.line_height,
                ),
                FORENSIC_EPSILON,
                &format!("{} CJK 基线与公式推算不符", case.name),
            );
            // TODO(M2,#43): 现状 = 链头与 fallback 的 ascent 占比差残差
            // `(1.16 − 0.96875) − (1.448 − 1.20996)/2 ≈ 0.0722em`,再整像素吸附;
            // 13pt 实测 +1px,H1 +2px。修复后收紧为 |Δ| ≤ 0.5px(理想 0)。
            let deviation = row.baseline_cjk_minus_latin.unwrap();
            println!(
                "{}「{}」基线偏差 CJK−拉丁 = {:+.3}px (字号 {:.2}pt, {:.4}em)",
                case.name,
                row.text.chars().take(8).collect::<String>(),
                deviation,
                case.size,
                deviation / case.size
            );
            let predicted_deviation = predicted_baseline(
                FaceMetrics {
                    ascent: noto_at_size.ascent,
                    row_height: noto_at_size.row_height,
                },
                head,
                row_height,
                line_height,
            ) - predicted_baseline(head, head, row_height, line_height);
            assert_approx(
                deviation,
                predicted_deviation,
                FORENSIC_EPSILON,
                &format!("{} 基线偏差与公式推算不符", case.name),
            );
            // em 盒越界量对账(推算只用表值+公式,不用实测 glyph 字段,
            // 避免套套逻辑):CJK em 盒顶/底由基线公式与 Noto 表值直接推得。
            let cjk_em_top_rowrel = predicted_baseline(noto_at_size, head, row_height, line_height)
                - noto_at_size.ascent;
            let cjk_em_bottom_rowrel = predicted_baseline(
                FaceMetrics {
                    ascent: noto_at_size.ascent,
                    row_height: noto_at_size.row_height,
                },
                head,
                row_height,
                line_height,
            ) + (noto_at_size.row_height - noto_at_size.ascent);
            let expected_overflow_top = (0.0 - cjk_em_top_rowrel).max(0.0);
            let expected_overflow_bottom = (cjk_em_bottom_rowrel - row_height).max(0.0);
            assert_approx(
                row.overflow_top_em,
                expected_overflow_top,
                FORENSIC_EPSILON,
                &format!("{} em 盒越顶量与表值推算不符", case.name),
            );
            assert_approx(
                row.overflow_bottom_em,
                expected_overflow_bottom,
                FORENSIC_EPSILON,
                &format!("{} em 盒越底量与表值推算不符", case.name),
            );
            println!(
                "{} em 盒越界(top {:+.3}px / bottom {:+.3}px),行盒 {:.2}px vs CJK face 需 {:.2}px",
                case.name,
                row.overflow_top_em,
                row.overflow_bottom_em,
                row.height(),
                row.max_face_row_height
            );
        }
    }

    // 相邻行墨迹净空(wrap 文档;现状断言:按 em 盒推算负净空)。
    let wrap_galley = render_case(&ctx, "m1-forensics-wrap", WRAP_DOC);
    let wrap_rows = collect_row_forensics(&wrap_galley, inter_probe, Some(noto_probe));
    let content: Vec<&RowForensics> = wrap_rows.iter().filter(|r| !r.glyphs.is_empty()).collect();
    assert!(
        content.len() >= 2,
        "wrap 文档应折出至少两行(实际 {})",
        content.len()
    );
    for pair in content.windows(2) {
        let (prev, next) = (pair[0], pair[1]);
        if let (Some((_, prev_bottom)), Some((next_top, _))) = (prev.ink_extent, next.ink_extent) {
            // 实墨口径的行间净空(负值 = 上一行实墨压进下一行区域)。
            let clearance = next_top - prev_bottom;
            println!(
                "相邻行实墨净空 = {clearance:+.3}px(行盒推进 {}px)",
                next.top - prev.top
            );
            // TODO(M2,#43): 修复后此处断言净空 ≥ 0(现状打印为主,不断言符号)。
        }
    }

    // 跨帧稳定性:同一 ctx 渲染 3 帧(show() 真实缓存路径 ×1 + 测量 ×2),
    // 几何指纹逐项相等(防 M2 修复引入跨帧漂移的回归盲区)。
    let fingerprints: Vec<_> = CASES
        .iter()
        .map(|case| {
            let g = render_case(&ctx, &format!("m1-forensics-{}", case.name), case.md);
            geometry_fingerprint(&collect_row_forensics(&g, inter_probe, Some(noto_probe)))
        })
        .collect();
    for frame in 2..=3 {
        let _ = ctx.run_ui(RawInput::default(), |ui| {
            // 真实渲染路径(show() 会填充并命中 vendored 布局缓存)。
            let style = crate::theme::default_markdown_style();
            let font = FontId::new(BODY_SIZE, FontFamily::Proportional);
            for case in CASES {
                egui_markdown::MarkdownLabel::new(
                    egui::Id::new(format!("m1-forensics-show-{}", case.name)),
                    case.md,
                )
                .font(font.clone())
                .style(&style)
                .wrap()
                .show(ui);
            }
        });
        for (i, case) in CASES.iter().enumerate() {
            let g = render_case(&ctx, &format!("m1-forensics-{}", case.name), case.md);
            let f = geometry_fingerprint(&collect_row_forensics(&g, inter_probe, Some(noto_probe)));
            assert_eq!(
                f, fingerprints[i],
                "{} 第 {} 帧几何指纹漂移(跨帧布局不稳定)",
                case.name, frame
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 取证 3:理想态断言(#[ignore];现状必红 = 复现成功,M2 修复后转正)
// ---------------------------------------------------------------------------

/// 三症状的理想态断言。**现状必红**(基线偏差 13pt 正文 +1px、H1 +2px,
/// 均 > 0.5px 阈值;em 盒越界在所有含 CJK 的行 > 0)——这正是 M1 的复现目标。
/// M2 修复合入时:移除 `#[ignore]`,断言转绿后成为回归护栏;若某项断言
/// 在修复方案下被有意豁免(如行距比例换算口径),必须在本注释下登记理由。
#[test]
#[ignore = "M1 现状复现(#43):基线偏差/em 盒越界按现状必红;M2 修复后移除本标记转正为回归护栏"]
fn mixed_script_strict_regression_target() {
    let ctx = egui::Context::default();
    let has_cjk = fonts::install(&ctx).is_some();
    ctx.run_ui(RawInput::default(), |_| {})
        .drop_without_applying_deltas();
    if !has_cjk {
        eprintln!("本机无 CJK 候选字体,严格断言无混排对象,跳过");
        return;
    }

    let body_font = FontId::new(BODY_SIZE, FontFamily::Proportional);
    let probe = |font: &FontId, ch: char| {
        let galley =
            ctx.fonts_mut(|f| f.layout_no_wrap(ch.to_string(), font.clone(), Color32::WHITE));
        let g = &galley.rows[0].row.glyphs[0];
        FaceMetrics {
            ascent: g.font_face_ascent,
            row_height: g.font_face_height,
        }
    };
    let inter_probe = probe(&body_font, 'H');
    let noto_probe = probe(&body_font, '中');

    let mut failures: Vec<String> = Vec::new();
    let mut checked_rows = 0usize;

    // 探针按字号逐案取(face metrics 是字号的函数;见 forensics 测试同款注释)。
    for case in CASES {
        let probe_at_size = probe(&FontId::new(case.size, FontFamily::Proportional), 'H');
        let noto_probe_at_size = probe(&FontId::new(case.size, FontFamily::Proportional), '中');
        let galley = render_case(&ctx, &format!("m1-strict-{}", case.name), case.md);
        let rows = collect_row_forensics(&galley, probe_at_size, Some(noto_probe_at_size));
        for row in rows.iter().filter(|r| !r.glyphs.is_empty()) {
            checked_rows += 1;
            // S2:同一视觉行内拉丁与 CJK 基线偏差 ≤ 0.5px。
            if let Some(dev) = row.baseline_cjk_minus_latin {
                if dev.abs() > STRICT_BASELINE_TOLERANCE {
                    failures.push(format!(
                        "[{}「{}」] 基线偏差 CJK−拉丁 = {:+.3}px > ±{}px",
                        case.name,
                        row.text.chars().take(8).collect::<String>(),
                        dev,
                        STRICT_BASELINE_TOLERANCE
                    ));
                }
            }
            // S1:行盒高度 ≥ 行内实际字形 metrics(em 盒口径:face 的行高需求)。
            if row.height() + STRICT_INK_EPSILON < row.max_face_row_height {
                failures.push(format!(
                    "[{}「{}」] 行盒 {:.3}px < 行内字形 face 行高 {:.3}px",
                    case.name,
                    row.text.chars().take(8).collect::<String>(),
                    row.height(),
                    row.max_face_row_height
                ));
            }
            // S3a:em 盒不越出行盒(显示不全的直接断言,保守口径)。
            if row.overflow_top_em > STRICT_INK_EPSILON {
                failures.push(format!(
                    "[{}「{}」] em 盒越出行盒顶 {:+.3}px",
                    case.name,
                    row.text.chars().take(8).collect::<String>(),
                    row.overflow_top_em
                ));
            }
            if row.overflow_bottom_em > STRICT_INK_EPSILON {
                failures.push(format!(
                    "[{}「{}」] em 盒越出行盒底 {:+.3}px",
                    case.name,
                    row.text.chars().take(8).collect::<String>(),
                    row.overflow_bottom_em
                ));
            }
            // S3b:实墨(uv_rect)不越出行盒——「显示不全」的最终裁决。
            for g in &row.glyphs {
                if let Some((ink_top, ink_bottom)) = g.ink {
                    if row.top - ink_top > STRICT_INK_EPSILON {
                        failures.push(format!(
                            "[{}「{}」字符 {}] 实墨越出行盒顶 {:+.3}px",
                            case.name,
                            row.text.chars().take(8).collect::<String>(),
                            escape(g.ch),
                            row.top - ink_top
                        ));
                    }
                    if ink_bottom - row.bottom > STRICT_INK_EPSILON {
                        failures.push(format!(
                            "[{}「{}」字符 {}] 实墨越出行盒底 {:+.3}px",
                            case.name,
                            row.text.chars().take(8).collect::<String>(),
                            escape(g.ch),
                            ink_bottom - row.bottom
                        ));
                    }
                }
            }
        }
    }

    // wrap 文档的相邻行实墨净空 ≥ 0(不粘连/不互相遮挡)。
    let wrap_galley = render_case(&ctx, "m1-strict-wrap", WRAP_DOC);
    let wrap_rows = collect_row_forensics(&wrap_galley, inter_probe, Some(noto_probe));
    let content: Vec<&RowForensics> = wrap_rows.iter().filter(|r| !r.glyphs.is_empty()).collect();
    for pair in content.windows(2) {
        let (prev, next) = (pair[0], pair[1]);
        if let (Some((_, prev_bottom)), Some((next_top, _))) = (prev.ink_extent, next.ink_extent) {
            let clearance = next_top - prev_bottom;
            if clearance < -STRICT_INK_EPSILON {
                failures.push(format!(
                    "[wrap 相邻行「{}」→「{}」] 上一行实墨压进下一行 {:+.3}px",
                    prev.text.chars().take(6).collect::<String>(),
                    next.text.chars().take(6).collect::<String>(),
                    -clearance
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "#43 严格断言失败(共 {} 项;已检 {} 行 + wrap {} 行):\n{}",
        failures.len(),
        checked_rows,
        content.len(),
        failures.join("\n")
    );
}
