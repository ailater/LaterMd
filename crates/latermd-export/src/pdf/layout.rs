//! IR → A4 页面布局:断行、缩进、表格列宽与块高。
//!
//! 产出 [`PlacedBlock`] 序列——每个块自带行序列(行内含已定位的绘制原语,
//! 原语坐标相对所在行的行盒左上角),垂直方向只记录块内相对 top,分页由
//! 上层([`super::paginate`])裁决。布局不接触 krilla 的页面/文档类型。
//!
//! 超宽内容策略(与 HTML 导出 `overflow-x: auto` 对应的 PDF 形态):
//! 代码块与表格单元格按各自列宽断行;连续无空格、无法按断行规则断开的
//! 长串(长 URL 等)在**任意字符边界硬断**——不截断、不丢内容;表格列宽
//! 先取自然宽,放不下时按比例收窄(等比缩排,极端多列时整体压回内容区宽)。

use krilla::text::KrillaGlyph;
use latermd_render::{Alignment, Block, Document, FootnoteDef, Inline, ListItem, Run, Table};

use super::shaping::{Fonts, ShapedText, Slot};
use super::PdfExportOptions;

/// 标题字号倍率(与 #23 F4 的预览分级一致,导出观感同源)。
const HEADING_SCALES: [f32; 6] = [2.0, 1.55, 1.3, 1.15, 1.08, 1.0];
/// 代码字号倍率。
const CODE_SCALE: f32 = 0.9;
/// 每层引用缩进 / 每层列表缩进。
const QUOTE_INDENT: f32 = 14.0;
const LIST_INDENT: f32 = 18.0;
/// 代码块内边距。
const CODE_PADDING: f32 = 6.0;
/// 表格单元格内边距。
const CELL_PADDING_H: f32 = 5.0;
const CELL_PADDING_V: f32 = 3.0;
/// 表格最小列宽(自然宽按比例收窄时的下限)。
const MIN_COL_WIDTH: f32 = 16.0;

/// 墨色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ink {
    /// 正文黑。
    Black,
    /// 灰阶(0–255)。
    Gray(u8),
    /// 链接蓝。
    Blue,
}

/// 一条可绘制原语;坐标相对所在行的行盒左上角。
pub(super) enum Prim {
    /// 一段同样式字形。
    Glyphs {
        /// 行内 x。
        x: f32,
        /// 行内基线 y。
        baseline: f32,
        /// 字体槽位。
        slot: Slot,
        /// 字号。
        size: f32,
        /// 字形(advance 已按 units_per_em 归一,绘制时乘字号)。
        glyphs: Vec<KrillaGlyph>,
        /// 字形区间所指向的完整文本(供 ToUnicode 文本提取)。
        text: String,
        /// 墨色。
        ink: Ink,
    },
    /// 填充矩形。
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        ink: Ink,
    },
    /// 线段(分隔线/引用条/下划线/删除线/表格边框)。
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        ink: Ink,
    },
}

/// 布局行。
pub(super) struct PlacedLine {
    /// 相对块顶。
    pub top: f32,
    /// 行盒高(含行距)。
    pub height: f32,
    /// 原语。
    pub prims: Vec<Prim>,
}

/// 布局块:分页的最小裁决单位(块级不可分割;块高超过整页内容高时按
/// [`PlacedBlock::cuts`] 给出的行边界兜底拆分)。
pub(super) struct PlacedBlock {
    /// 与上一块的间距;分页后位于页首时忽略。
    pub above: f32,
    /// 行序列。
    pub lines: Vec<PlacedLine>,
    /// 允许作为分页切点的行下标(段落/代码 = 所有行边界,表格 = 行边界,
    /// 列表 = 项边界)。
    pub cuts: Vec<usize>,
}

impl PlacedBlock {
    /// 块总高(含块内间距)。
    pub fn height(&self) -> f32 {
        self.lines
            .last()
            .map(|line| line.top + line.height)
            .unwrap_or(0.0)
    }
}

/// 布局上下文。
pub(super) struct LayoutContext<'a> {
    /// 字体集。
    pub fonts: &'a Fonts,
    /// 导出选项。
    pub opts: &'a PdfExportOptions,
    /// 内容区宽度(A4 宽 − 左右边距)。
    pub width: f32,
}

impl LayoutContext<'_> {
    /// 正文行高(pt)。
    fn line_height(&self) -> f32 {
        self.opts.body_font_size * self.opts.line_height
    }

    /// 块间距(pt)。
    fn gap(&self) -> f32 {
        self.line_height() * 0.5
    }
}

/// 文档级入口:顶层块序列 + 尾注;首块无上间距。
pub(super) fn layout_document(doc: &Document, ctx: &LayoutContext) -> Vec<PlacedBlock> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        layout_into(&mut out, block, 0.0, ctx);
    }
    if !doc.footnotes.is_empty() {
        layout_footnotes(&mut out, &doc.footnotes, ctx);
    }
    if let Some(first) = out.first_mut() {
        first.above = 0.0;
    }
    out
}

fn layout_into(out: &mut Vec<PlacedBlock>, block: &Block, indent: f32, ctx: &LayoutContext) {
    match block {
        Block::Heading { level, inlines } => {
            let scale = HEADING_SCALES[(level.saturating_sub(1) as usize).min(5)];
            let size = ctx.opts.body_font_size * scale;
            let (runs, items) = build_runs(inlines, size, true, ctx);
            let mut lines = layout_runs(
                &runs,
                &items,
                indent,
                (ctx.width - indent).max(0.0),
                false,
                ctx,
            );
            // 标题下间距:垫一条空行
            let top = lines.last().map(|l| l.top + l.height).unwrap_or(0.0);
            lines.push(PlacedLine {
                top,
                height: size * 0.28,
                prims: Vec::new(),
            });
            out.push(finish_block(lines, ctx.gap().max(size * 0.6)));
        }
        Block::Paragraph { inlines } => {
            let (runs, items) = build_runs(inlines, ctx.opts.body_font_size, false, ctx);
            let lines = layout_runs(
                &runs,
                &items,
                indent,
                (ctx.width - indent).max(0.0),
                false,
                ctx,
            );
            if !lines.is_empty() {
                out.push(finish_block(lines, ctx.gap()));
            }
        }
        Block::List { start, items } => {
            if let Some(list) = layout_list(start, items, indent, ctx) {
                out.push(list);
            }
        }
        Block::CodeBlock { code, .. } => {
            let lines = layout_code(code, indent, ctx);
            out.push(finish_block(lines, ctx.gap()));
        }
        Block::Quote { blocks } => {
            let mut parts = Vec::new();
            for block in blocks {
                layout_into(&mut parts, block, indent + QUOTE_INDENT, ctx);
            }
            let mut quote = merge_parts(parts);
            for line in &mut quote.lines {
                line.prims.push(Prim::Line {
                    x1: indent + 2.0,
                    y1: 0.0,
                    x2: indent + 2.0,
                    y2: line.height,
                    width: 1.5,
                    ink: Ink::Gray(150),
                });
            }
            out.push(quote);
        }
        Block::Table(table) => {
            out.push(layout_table(table, indent, ctx));
        }
        Block::ThematicBreak => {
            let height = ctx.line_height();
            let lines = vec![PlacedLine {
                top: 0.0,
                height,
                prims: vec![Prim::Line {
                    x1: indent,
                    y1: height * 0.5,
                    x2: indent + (ctx.width - indent).max(0.0),
                    y2: height * 0.5,
                    width: 0.8,
                    ink: Ink::Gray(170),
                }],
            }];
            out.push(finish_block(lines, ctx.gap()));
        }
        Block::ImagePlaceholder { alt, .. } => {
            let lines = layout_image_placeholder(alt, indent, ctx);
            out.push(finish_block(lines, ctx.gap()));
        }
    }
}

/// 行序列 → 块(行边界全部可作为分页切点)。
fn finish_block(lines: Vec<PlacedLine>, above: f32) -> PlacedBlock {
    finish_block_with_cuts(lines, above, None)
}

/// 行序列 → 块,切点可显式指定(表格 = 行边界);`None` = 所有行边界。
fn finish_block_with_cuts(
    lines: Vec<PlacedLine>,
    above: f32,
    cuts: Option<Vec<usize>>,
) -> PlacedBlock {
    let cuts = cuts.unwrap_or_else(|| (1..=lines.len()).collect());
    PlacedBlock { above, lines, cuts }
}

/// 把多个子块合并成一个块(引用/列表):子块边界与子块自身的切点
/// 都是合法分页切点;子块间距沿用子块的 `above`。
fn merge_parts(parts: Vec<PlacedBlock>) -> PlacedBlock {
    let mut merged = PlacedBlock {
        above: 0.0,
        lines: Vec::new(),
        cuts: Vec::new(),
    };
    for (index, part) in parts.into_iter().enumerate() {
        let pad = if index == 0 { 0.0 } else { part.above };
        if index == 0 {
            merged.above = part.above;
        }
        let base = merged.height() + pad;
        let line_base = merged.lines.len();
        for mut line in part.lines {
            line.top += base;
            merged.lines.push(line);
        }
        merged
            .cuts
            .extend(part.cuts.into_iter().map(|cut| cut + line_base));
        if !merged.lines.is_empty() {
            merged.cuts.push(merged.lines.len());
        }
    }
    merged
}

// ---- 段落:运行构建 → 整形 → 断行 → 行装配 ----

/// 一段同样式文本的整形结果与其绘制参数。
struct ParaRun {
    text: String,
    slot: Slot,
    size: f32,
    ink: Ink,
    link: bool,
    strike: bool,
    bg: bool,
    shaped: ShapedText,
}

enum ParaItem {
    Run(usize),
    Break,
}

/// 断行单元:一个 cluster(不可劈开的字形组)或一个硬换行标记。
#[derive(Clone)]
struct Unit {
    run: usize,
    glyphs: std::ops::Range<usize>,
    width: f32,
    break_after: bool,
    blank: bool,
    forced: bool,
}

/// 把行内指令翻译为运行序列(含整形)。
fn build_runs(
    inlines: &[Inline],
    base_size: f32,
    bold_all: bool,
    ctx: &LayoutContext,
) -> (Vec<ParaRun>, Vec<ParaItem>) {
    let mut runs: Vec<ParaRun> = Vec::new();
    let mut items: Vec<ParaItem> = Vec::new();
    let push = |runs: &mut Vec<ParaRun>,
                items: &mut Vec<ParaItem>,
                text: String,
                slot: Slot,
                size: f32,
                ink: Ink,
                link: bool,
                strike: bool,
                bg: bool,
                ctx: &LayoutContext| {
        let shaped = ctx.fonts.shape(slot, &text);
        runs.push(ParaRun {
            text,
            slot,
            size,
            ink,
            link,
            strike,
            bg,
            shaped,
        });
        items.push(ParaItem::Run(runs.len() - 1));
    };
    for inline in inlines {
        match inline {
            Inline::Run(Run { text, style }) => {
                let slot = if style.code {
                    Slot::Mono
                } else if style.bold || bold_all {
                    Slot::Bold
                } else {
                    Slot::Regular
                };
                let size = base_size * if style.code { CODE_SCALE } else { 1.0 };
                let linked = style.link.is_some();
                push(
                    &mut runs,
                    &mut items,
                    text.clone(),
                    slot,
                    size,
                    if linked { Ink::Blue } else { Ink::Black },
                    linked,
                    style.strike,
                    style.code,
                    ctx,
                );
            }
            Inline::Image { alt, .. } => {
                // 行内图片画占位底色 + 替代文本;空 alt 用统一占位词。
                let text = if alt.is_empty() {
                    "[图片]".to_owned()
                } else {
                    alt.clone()
                };
                push(
                    &mut runs,
                    &mut items,
                    text,
                    Slot::Mono,
                    base_size * 0.85,
                    Ink::Gray(90),
                    false,
                    false,
                    true,
                    ctx,
                );
            }
            Inline::LineBreak => items.push(ParaItem::Break),
            Inline::FootnoteRef { label } => push(
                &mut runs,
                &mut items,
                format!("[{label}]"),
                Slot::Regular,
                base_size * 0.8,
                Ink::Black,
                false,
                false,
                false,
                ctx,
            ),
        }
    }
    (runs, items)
}

/// 断行机会:空格之后,或 CJK 与任意字符之间(自写规则,与 ADR-006 §6
/// 「仅 CJK 字符间可断 + 西文按空格」一致,不引入 unicode-linebreak)。
fn can_break_between(a: char, b: char) -> bool {
    a == ' ' || is_cjk(a) || is_cjk(b)
}

/// CJK 及全角区段的粗判定(含谚文/假名/CJK 标点/全角形式)。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF
        | 0x2E80..=0x9FFF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0x20000..=0x3FFFD)
}

fn build_units(runs: &[ParaRun], items: &[ParaItem], break_anywhere: bool) -> Vec<Unit> {
    let mut units = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            ParaItem::Break => units.push(Unit {
                run: 0,
                glyphs: 0..0,
                width: 0.0,
                break_after: true,
                blank: false,
                forced: true,
            }),
            ParaItem::Run(run_idx) => {
                let run = &runs[*run_idx];
                // 行内顺序的下一个文本 run(run 边界能否断行取决于两侧字符)
                let next_text = items[index + 1..].iter().find_map(|next| match next {
                    ParaItem::Run(next_idx) => Some(runs[*next_idx].text.as_str()),
                    ParaItem::Break => None,
                });
                let mut glyph_start = 0usize;
                for (end_byte, next_glyph) in &run.shaped.cuts {
                    let width = run.shaped.glyphs[glyph_start..*next_glyph]
                        .iter()
                        .map(|glyph| glyph.x_advance)
                        .sum::<f32>()
                        * run.size;
                    let cluster_start = run.shaped.glyphs[glyph_start]
                        .text_range
                        .start
                        .min(*end_byte);
                    let break_after = if break_anywhere {
                        true
                    } else if *end_byte >= run.text.len() {
                        match (
                            run.text.chars().next_back(),
                            next_text.and_then(|text| text.chars().next()),
                        ) {
                            (Some(a), Some(b)) => can_break_between(a, b),
                            _ => true,
                        }
                    } else {
                        match (
                            run.text[..*end_byte].chars().next_back(),
                            run.text[*end_byte..].chars().next(),
                        ) {
                            (Some(a), Some(b)) => can_break_between(a, b),
                            _ => true,
                        }
                    };
                    units.push(Unit {
                        run: *run_idx,
                        glyphs: glyph_start..*next_glyph,
                        width,
                        break_after,
                        blank: run.text[cluster_start..*end_byte].trim().is_empty(),
                        forced: false,
                    });
                    glyph_start = *next_glyph;
                }
            }
        }
    }
    units
}

/// 贪心断行。每行至少消费一个单元(防死循环);行首空白跳过、行尾空白
/// 不绘制;无断点的超长串在单元边界硬断(见模块文档的超宽策略)。
fn wrap_units(units: &[Unit], limit: f32) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    while start < units.len() {
        while start < units.len() && units[start].blank && !units[start].forced {
            start += 1;
        }
        if start >= units.len() {
            break;
        }
        if limit <= 0.0 || limit.is_nan() {
            lines.push(start..trim_trailing_blank(units, start, units.len()));
            break;
        }
        let mut width = 0.0;
        let mut last_break: Option<usize> = None;
        let mut index = start;
        let mut forced_end = None;
        while index < units.len() {
            let unit = &units[index];
            if unit.forced {
                forced_end = Some(index + 1);
                break;
            }
            if width + unit.width > limit && index > start {
                break;
            }
            width += unit.width;
            if unit.break_after {
                last_break = Some(index + 1);
            }
            index += 1;
        }
        let end = match forced_end {
            Some(end) => end,
            None if index >= units.len() => units.len(),
            None => last_break.unwrap_or(index),
        };
        let end = end.max(start + 1).min(units.len());
        lines.push(start..trim_trailing_blank(units, start, end));
        start = end.max(start + 1);
    }
    lines
}

fn trim_trailing_blank(units: &[Unit], start: usize, end: usize) -> usize {
    let mut end = end;
    while end > start + 1 && units[end - 1].blank && !units[end - 1].forced {
        end -= 1;
    }
    end
}

/// 运行序列 → 行序列(断行、行盒高度、基线与原语全装配)。
///
/// `x_start` 是行首原语的起点 x,`limit` 是断行宽度上限——两者独立
/// (表格单元格的起点在列内,上限是列宽,不是内容区右缘减起点)。
fn layout_runs(
    runs: &[ParaRun],
    items: &[ParaItem],
    x_start: f32,
    limit: f32,
    break_anywhere: bool,
    ctx: &LayoutContext,
) -> Vec<PlacedLine> {
    let units = build_units(runs, items, break_anywhere);
    let mut lines = Vec::new();
    for range in wrap_units(&units, limit.max(0.0)) {
        let mut ascent: f32 = 0.0;
        let mut descent: f32 = 0.0;
        let mut line_height: f32 = 0.0;
        for unit in &units[range.clone()] {
            if unit.forced {
                continue;
            }
            let run = &runs[unit.run];
            let (run_ascent, run_descent) = ctx.fonts.vertical(run.slot, run.size);
            ascent = ascent.max(run_ascent);
            descent = descent.min(run_descent);
            line_height = line_height.max(run.size * ctx.opts.line_height);
        }
        if line_height <= 0.0 {
            // 只有硬换行标记的空行:按正文行盒垫高
            line_height = ctx.line_height();
            let (a, d) = ctx.fonts.vertical(Slot::Regular, ctx.opts.body_font_size);
            ascent = a;
            descent = d;
        }
        let leading = (line_height - (ascent - descent)).max(0.0);
        let baseline = leading / 2.0 + ascent;
        let mut prims = Vec::new();
        let mut x = x_start;
        let mut index = range.start;
        while index < range.end {
            let unit = &units[index];
            if unit.forced {
                index += 1;
                continue;
            }
            // 合并连续同 run 单元为一个绘制原语
            let run_idx = unit.run;
            let mut glyph_end = unit.glyphs.end;
            let mut width = unit.width;
            let mut next = index + 1;
            while next < range.end && !units[next].forced && units[next].run == run_idx {
                glyph_end = units[next].glyphs.end;
                width += units[next].width;
                next += 1;
            }
            let run = &runs[run_idx];
            if run.bg {
                let (a, d) = ctx.fonts.vertical(run.slot, run.size);
                prims.push(Prim::Rect {
                    x: x - 1.5,
                    y: baseline - a - 1.0,
                    w: width + 3.0,
                    h: a - d + 2.0,
                    ink: Ink::Gray(240),
                });
            }
            prims.push(Prim::Glyphs {
                x,
                baseline,
                slot: run.slot,
                size: run.size,
                glyphs: run.shaped.glyphs[unit.glyphs.start..glyph_end].to_vec(),
                text: run.text.clone(),
                ink: run.ink,
            });
            if run.link {
                prims.push(Prim::Line {
                    x1: x,
                    y1: baseline + 0.12 * run.size,
                    x2: x + width,
                    y2: baseline + 0.12 * run.size,
                    width: 0.6,
                    ink: Ink::Blue,
                });
            }
            if run.strike {
                prims.push(Prim::Line {
                    x1: x,
                    y1: baseline - 0.3 * run.size,
                    x2: x + width,
                    y2: baseline - 0.3 * run.size,
                    width: 0.6,
                    ink: run.ink,
                });
            }
            x += width;
            index = next;
        }
        lines.push(PlacedLine {
            top: 0.0,
            height: line_height,
            prims,
        });
    }
    let mut top = 0.0;
    for line in &mut lines {
        line.top = top;
        top += line.height;
    }
    lines
}

// ---- 列表 ----

fn layout_list(
    start: &Option<u64>,
    items: &[ListItem],
    indent: f32,
    ctx: &LayoutContext,
) -> Option<PlacedBlock> {
    let base = start.unwrap_or(1);
    let mut item_blocks = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let mut parts = Vec::new();
        for block in &item.blocks {
            layout_into(&mut parts, block, indent + LIST_INDENT, ctx);
        }
        if parts.is_empty() {
            continue;
        }
        // 项内多块合并;项间距用小档
        let mut merged = merge_parts(parts);
        merged.above = ctx.gap() * 0.35;
        if index == 0 {
            merged.above = 0.0;
        }
        attach_marker(
            &mut merged,
            start.is_some(),
            base + index as u64,
            item.task,
            indent,
            ctx,
        );
        item_blocks.push(merged);
    }
    if item_blocks.is_empty() {
        return None;
    }
    let mut list = merge_parts(item_blocks);
    list.above = ctx.gap();
    Some(list)
}

/// 把列表标记画到项首行(对齐首行首个字形基线)。
fn attach_marker(
    item: &mut PlacedBlock,
    ordered: bool,
    number: u64,
    task: Option<bool>,
    indent: f32,
    ctx: &LayoutContext,
) {
    let Some(line) = item.lines.first_mut() else {
        return;
    };
    let (baseline, size) = line
        .prims
        .iter()
        .find_map(|prim| match prim {
            Prim::Glyphs { baseline, size, .. } => Some((*baseline, *size)),
            _ => None,
        })
        .unwrap_or((line.height * 0.75, ctx.opts.body_font_size));
    match task {
        Some(checked) => {
            // 任务框:实心=已勾选,灰框=未勾选
            let side = 7.5_f32;
            let top = baseline - side + 1.5;
            line.prims.push(Prim::Rect {
                x: indent + 1.0,
                y: top,
                w: side,
                h: side,
                ink: if checked { Ink::Black } else { Ink::Gray(130) },
            });
            if !checked {
                line.prims.push(Prim::Rect {
                    x: indent + 1.8,
                    y: top + 0.8,
                    w: side - 1.6,
                    h: side - 1.6,
                    ink: Ink::Gray(255),
                });
            }
        }
        None => {
            let marker = if ordered {
                format!("{number}.")
            } else {
                "•".to_owned()
            };
            let shaped = ctx.fonts.shape(Slot::Regular, &marker);
            let known = !shaped.glyphs.is_empty()
                && shaped
                    .glyphs
                    .iter()
                    .all(|glyph| glyph.glyph_id.to_u32() != 0);
            if known {
                line.prims.push(Prim::Glyphs {
                    x: indent + 1.0,
                    baseline,
                    slot: Slot::Regular,
                    size,
                    glyphs: shaped.glyphs,
                    text: marker,
                    ink: Ink::Black,
                });
            } else {
                // 字体缺 • 字形:退化为短横线标记,绝不出豆腐块
                line.prims.push(Prim::Line {
                    x1: indent + 1.5,
                    y1: baseline - size * 0.22,
                    x2: indent + 7.0,
                    y2: baseline - size * 0.22,
                    width: 1.1,
                    ink: Ink::Black,
                });
            }
        }
    }
}

// ---- 代码块 ----

fn layout_code(code: &str, indent: f32, ctx: &LayoutContext) -> Vec<PlacedLine> {
    let size = ctx.opts.body_font_size * CODE_SCALE;
    let line_height = size * ctx.opts.line_height;
    let box_width = (ctx.width - indent).max(0.0);
    let text_x = indent + CODE_PADDING;
    let limit = (box_width - 2.0 * CODE_PADDING).max(0.0);
    let body = code.strip_suffix('\n').unwrap_or(code);
    let mut lines = Vec::new();
    for source in body.split('\n') {
        let shaped = ctx.fonts.shape(Slot::Mono, source);
        let runs = vec![ParaRun {
            text: source.to_owned(),
            slot: Slot::Mono,
            size,
            ink: Ink::Black,
            link: false,
            strike: false,
            bg: false,
            shaped,
        }];
        let items = vec![ParaItem::Run(0)];
        // 代码行:任意字符边界可断(超宽不截断,按字符硬断)
        for mut line in layout_runs(&runs, &items, text_x, limit, true, ctx) {
            line.height = line_height;
            line.prims.insert(
                0,
                Prim::Rect {
                    x: indent,
                    y: 0.0,
                    w: box_width,
                    h: line_height,
                    ink: Ink::Gray(246),
                },
            );
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(PlacedLine {
            top: 0.0,
            height: line_height,
            prims: vec![Prim::Rect {
                x: indent,
                y: 0.0,
                w: box_width,
                h: line_height,
                ink: Ink::Gray(246),
            }],
        });
    }
    lines
}

// ---- 表格 ----

/// 表格布局:列宽先自然宽(单行完整宽)→ 放不下按比例收窄 → 单元格按列宽
/// 断行;行高统一,行边界是分页切点(一行单元格文本不跨页拆);边框按行内
/// 的短线段画(跨页不断线)。
fn layout_table(table: &Table, indent: f32, ctx: &LayoutContext) -> PlacedBlock {
    let columns = table.alignments.len();
    if columns == 0 {
        return finish_block_with_cuts(Vec::new(), 0.0, Some(Vec::new()));
    }
    let available = (ctx.width - indent).max(0.0);
    let size = ctx.opts.body_font_size;
    let line_height = size * ctx.opts.line_height;
    let empty_cell: Vec<Inline> = Vec::new();

    struct CellRuns {
        runs: Vec<ParaRun>,
        items: Vec<ParaItem>,
        natural: f32,
    }
    let build_cell = |cell: &[Inline], bold: bool| -> CellRuns {
        let (runs, items) = build_runs(cell, size, bold, ctx);
        let natural: f32 = runs
            .iter()
            .map(|run| {
                run.shaped
                    .glyphs
                    .iter()
                    .map(|glyph| glyph.x_advance)
                    .sum::<f32>()
                    * run.size
            })
            .sum();
        CellRuns {
            runs,
            items,
            natural: natural.min(available),
        }
    };
    let header: Vec<CellRuns> = table
        .header
        .iter()
        .map(|cell| build_cell(cell, true))
        .collect();
    let rows: Vec<Vec<CellRuns>> = table
        .rows
        .iter()
        .map(|row| {
            (0..columns)
                .map(|column| build_cell(row.get(column).unwrap_or(&empty_cell), false))
                .collect()
        })
        .collect();

    // 列宽求解
    let mut natural = vec![0.0_f32; columns];
    for (column, cell) in header.iter().enumerate() {
        if column < columns {
            natural[column] = natural[column].max(cell.natural);
        }
    }
    for row in &rows {
        for (column, cell) in row.iter().enumerate() {
            if column < columns {
                natural[column] = natural[column].max(cell.natural);
            }
        }
    }
    let total: f32 = natural.iter().sum();
    let mut widths = if total <= available && total > 0.0 {
        natural
            .iter()
            .map(|width| width + (available - total) / columns as f32)
            .collect::<Vec<_>>()
    } else if total > 0.0 {
        natural
            .iter()
            .map(|width| (available * width / total).max(MIN_COL_WIDTH))
            .collect::<Vec<_>>()
    } else {
        vec![available / columns as f32; columns]
    };
    let width_sum: f32 = widths.iter().sum();
    if width_sum > available && width_sum > 0.0 {
        let scale = available / width_sum;
        widths.iter_mut().for_each(|width| *width *= scale);
    }
    let table_width: f32 = widths.iter().sum();
    let column_x = |column: usize| indent + widths[..column].iter().sum::<f32>();

    // 逐行装配:每列的第 k 条文本线共享同一行盒;对齐偏移平移原语
    let mut row_line_starts: Vec<usize> = Vec::new();
    let mut lines: Vec<PlacedLine> = Vec::new();
    let mut row_index = 0usize;
    for (is_header, row) in
        std::iter::once((true, &header)).chain(rows.iter().map(|row| (false, row)))
    {
        let cell_lines: Vec<Vec<PlacedLine>> = row
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let inner = (widths[column] - 2.0 * CELL_PADDING_H).max(0.0);
                let cell_x = column_x(column) + CELL_PADDING_H;
                layout_runs(&cell.runs, &cell.items, cell_x, inner, false, ctx)
                    .into_iter()
                    .map(|mut line| {
                        line.height = line_height;
                        line
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let depth = cell_lines
            .iter()
            .map(|lines| lines.len())
            .max()
            .unwrap_or(0)
            .max(1);
        row_line_starts.push(lines.len());
        for line_index in 0..depth {
            let mut prims = Vec::new();
            if is_header {
                prims.push(Prim::Rect {
                    x: indent,
                    y: 0.0,
                    w: table_width,
                    h: line_height,
                    ink: Ink::Gray(240),
                });
            }
            for (column, lines) in cell_lines.iter().enumerate() {
                let Some(cell_line) = lines.get(line_index) else {
                    continue;
                };
                let inner = (widths[column] - 2.0 * CELL_PADDING_H).max(0.0);
                let used = cell_line_width(cell_line);
                let offset = match table.alignments.get(column) {
                    Some(Alignment::Center) => ((inner - used) / 2.0).max(0.0),
                    Some(Alignment::Right) => (inner - used).max(0.0),
                    _ => 0.0,
                };
                for prim in &cell_line.prims {
                    prims.push(shift_prim(prim, offset, 0.0));
                }
            }
            // 竖向边框按行画短线段,跨页不断线
            for column in 0..=columns {
                let x = column_x(column);
                prims.push(Prim::Line {
                    x1: x,
                    y1: 0.0,
                    x2: x,
                    y2: line_height,
                    width: 0.5,
                    ink: Ink::Gray(200),
                });
            }
            if line_index + 1 == depth {
                prims.push(Prim::Line {
                    x1: indent,
                    y1: line_height,
                    x2: indent + table_width,
                    y2: line_height,
                    width: if is_header { 1.0 } else { 0.5 },
                    ink: if is_header {
                        Ink::Gray(120)
                    } else {
                        Ink::Gray(200)
                    },
                });
            }
            if row_index == 0 && line_index == 0 {
                prims.push(Prim::Line {
                    x1: indent,
                    y1: 0.0,
                    x2: indent + table_width,
                    y2: 0.0,
                    width: 0.5,
                    ink: Ink::Gray(150),
                });
            }
            lines.push(PlacedLine {
                top: 0.0,
                height: line_height,
                prims,
            });
        }
        row_index += 1;
    }
    // 首尾行垫垂直内边距,重排 top
    let last_index = lines.len().saturating_sub(1);
    for (index, line) in lines.iter_mut().enumerate() {
        if index == 0 || index == last_index {
            line.height += CELL_PADDING_V;
        }
    }
    let mut top = 0.0;
    for line in lines.iter_mut() {
        line.top = top;
        top += line.height;
    }
    finish_block_with_cuts(
        lines,
        ctx.gap(),
        Some(row_line_starts.into_iter().filter(|cut| *cut > 0).collect()),
    )
}

/// 单元格一行的文本宽(最长原语右缘)。
fn cell_line_width(line: &PlacedLine) -> f32 {
    line.prims.iter().fold(0.0_f32, |acc, prim| match prim {
        Prim::Glyphs {
            x, glyphs, size, ..
        } => acc.max(x + glyphs.iter().map(|g| g.x_advance).sum::<f32>() * size),
        _ => acc,
    })
}

fn shift_prim(prim: &Prim, dx: f32, dy: f32) -> Prim {
    match prim {
        Prim::Glyphs {
            x,
            baseline,
            slot,
            size,
            glyphs,
            text,
            ink,
        } => Prim::Glyphs {
            x: x + dx,
            baseline: baseline + dy,
            slot: *slot,
            size: *size,
            glyphs: glyphs.clone(),
            text: text.clone(),
            ink: *ink,
        },
        Prim::Rect { x, y, w, h, ink } => Prim::Rect {
            x: x + dx,
            y: y + dy,
            w: *w,
            h: *h,
            ink: *ink,
        },
        Prim::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ink,
        } => Prim::Line {
            x1: x1 + dx,
            y1: y1 + dy,
            x2: x2 + dx,
            y2: y2 + dy,
            width: *width,
            ink: *ink,
        },
    }
}

// ---- 图片占位 / 尾注 ----

fn layout_image_placeholder(alt: &str, indent: f32, ctx: &LayoutContext) -> Vec<PlacedLine> {
    let size = ctx.opts.body_font_size * 0.85;
    let shaped = ctx.fonts.shape(Slot::Mono, alt);
    let text_width = shaped.glyphs.iter().map(|g| g.x_advance).sum::<f32>() * size;
    let width = (text_width + 16.0)
        .max(72.0)
        .min((ctx.width - indent).max(0.0));
    let height = 30.0;
    let (ascent, descent) = ctx.fonts.vertical(Slot::Mono, size);
    let baseline = (height - (ascent - descent)) / 2.0 + ascent;
    vec![PlacedLine {
        top: 0.0,
        height,
        prims: vec![
            Prim::Rect {
                x: indent,
                y: 0.0,
                w: width,
                h: height,
                ink: Ink::Gray(238),
            },
            Prim::Glyphs {
                x: indent + 8.0,
                baseline,
                slot: Slot::Mono,
                size,
                glyphs: shaped.glyphs,
                text: alt.to_owned(),
                ink: Ink::Gray(90),
            },
        ],
    }]
}

fn layout_footnotes(out: &mut Vec<PlacedBlock>, footnotes: &[FootnoteDef], ctx: &LayoutContext) {
    // 分隔线(短横线)+ 每条脚注:前缀 [label] + 定义体首段,其余块照常排
    let separator = vec![PlacedLine {
        top: 0.0,
        height: ctx.line_height() * 0.6,
        prims: vec![Prim::Line {
            x1: 0.0,
            y1: ctx.line_height() * 0.3,
            x2: 96.0,
            y2: ctx.line_height() * 0.3,
            width: 0.8,
            ink: Ink::Gray(170),
        }],
    }];
    out.push(finish_block(separator, ctx.gap() * 1.6));
    for footnote in footnotes {
        let mut inlines: Vec<Inline> = vec![Inline::Run(Run {
            text: format!("[{}] ", footnote.label),
            style: latermd_render::Style::default(),
        })];
        let rest: Vec<Block> = footnote.blocks.iter().skip(1).cloned().collect();
        if let Some(Block::Paragraph { inlines: first }) = footnote.blocks.first() {
            inlines.extend(first.iter().cloned());
        }
        let (runs, items) = build_runs(&inlines, ctx.opts.body_font_size, false, ctx);
        let lines = layout_runs(&runs, &items, 8.0, (ctx.width - 8.0).max(0.0), false, ctx);
        if !lines.is_empty() {
            out.push(finish_block(lines, ctx.gap() * 0.6));
        }
        for block in &rest {
            layout_into(out, block, 8.0, ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(width: f32, break_after: bool, blank: bool) -> Unit {
        Unit {
            run: 0,
            glyphs: 0..0,
            width,
            break_after,
            blank,
            forced: false,
        }
    }

    #[test]
    fn cjk_break_rule() {
        assert!(can_break_between('文', '字'));
        assert!(can_break_between('字', 'a'));
        assert!(can_break_between('a', '文'));
        assert!(
            !can_break_between('a', ' '),
            "空格之前不可断(否则空格落到下行行首)"
        );
        assert!(can_break_between(' ', 'w'), "空格之后可断");
        assert!(!can_break_between('a', 'b'));
        assert!(!can_break_between('l', 'd'));
        assert!(is_cjk('。'), "CJK 标点可断");
        assert!(is_cjk('한'), "谚文");
        assert!(!is_cjk('a'));
        assert!(!is_cjk(' '));
    }

    #[test]
    fn wrap_breaks_after_space_and_drops_it() {
        // "h e l l o ␣ w o r l d":仅空格后可断;limit 6 断在空格处,
        // 空格本体不绘制(行 0..5 而非 0..6)
        let units: Vec<Unit> = ['h', 'e', 'l', 'l', 'o', ' ', 'w', 'o', 'r', 'l', 'd']
            .into_iter()
            .map(|c| unit(1.0, c == ' ', c == ' '))
            .collect();
        let lines = wrap_units(&units, 6.0);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            (lines[0].clone(), lines[1].clone()),
            (0..5, 6..11),
            "{lines:?}"
        );
    }

    #[test]
    fn wrap_hard_breaks_unbreakable_overflow() {
        // 10 个不可断单元、每行只容 1 个 → 逐单元硬断,不死循环、不丢单元
        let units: Vec<Unit> = (0..10).map(|_| unit(3.0, false, false)).collect();
        let lines = wrap_units(&units, 4.0);
        let covered: usize = lines.iter().map(|range| range.len()).sum();
        assert_eq!(covered, 10);
        assert!(
            lines.iter().all(|range| range.len() == 1),
            "放不下时逐单元硬断:{lines:?}"
        );
    }

    #[test]
    fn wrap_forced_break_ends_line() {
        let mut units: Vec<Unit> = vec![unit(1.0, false, false); 3];
        units.push(Unit {
            run: 0,
            glyphs: 0..0,
            width: 0.0,
            break_after: true,
            blank: false,
            forced: true,
        });
        units.extend(vec![unit(1.0, false, false); 2]);
        let lines = wrap_units(&units, 100.0);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0].end, 4, "硬换行标记收在首行行尾:{lines:?}");
        assert_eq!((lines[1].start, lines[1].end), (4, 6));
    }

    #[test]
    fn wrap_drops_trailing_blanks() {
        // 文本(2)+文本(2,断点)+空白+空白,limit 4 → 单行且行尾空白被剥
        let units = vec![
            unit(2.0, false, false),
            unit(2.0, true, false),
            unit(1.0, true, true),
            unit(1.0, true, true),
        ];
        let lines = wrap_units(&units, 4.0);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0], 0..2, "行尾空白不占行:{lines:?}");
    }
}
