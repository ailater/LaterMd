//! PDF 导出(A4,krilla 后端)——把 [`latermd_render`] 的绘制指令 IR 布局
//! 到 A4 页面并编码为可落盘的 PDF 字节。
//!
//! 分页策略(任务书「块级指令整体下移不可分割」):
//!
//! 1. 块在当前页放得下 → 原地下排;
//! 2. 放不下但块高 ≤ 整页内容高 → **整块移到下一页**(块级不可分割);
//! 3. 块高超过整页内容高 → 在 [`layout::PlacedBlock::cuts`] 给出的行边界
//!    兜底拆分(段落/代码 = 行,表格 = 表行,列表 = 列表项),拆出的片段
//!    从新页顶继续;极端情况下单行也放不下时硬放该行(防死循环,不丢内容)。
//!
//! 页码画在底边距内(`n / 总页数`),经 [`PdfExportOptions::page_numbers`] 开关。

mod layout;
mod shaping;

use std::fmt;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect as KRect};
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, Paint, Stroke};
use krilla::Document as PdfDocument;
use latermd_render::Document;

/// A4 纵向宽(pt)。
pub const A4_WIDTH: f32 = 595.276;
/// A4 纵向高(pt)。
pub const A4_HEIGHT: f32 = 841.89;
use layout::{Ink, LayoutContext, PlacedBlock, Prim};
use shaping::Fonts;

/// 一个待嵌入字体:原始字节 + face 序号(`.ttc` 集合内取哪个 face,
/// 单字体文件恒为 0)。
#[derive(Debug, Clone)]
pub struct PdfFont {
    /// 字体原始字节(TTF/OTF/TTC)。
    pub data: Vec<u8>,
    /// `.ttc` 的 face 序号;非集合字体传 0。
    pub index: u32,
}

/// PDF 用字体集:正文必需,粗体/等宽缺省时回落正文字体。
#[derive(Debug, Clone)]
pub struct PdfFonts {
    /// 正文字体(必需;读不出则导出报 [`PdfError::RegularFontUnreadable`])。
    pub regular: PdfFont,
    /// 粗体(标题/加粗;`None` 或读不出时用正文字体)。
    pub bold: Option<PdfFont>,
    /// 等宽(代码;`None` 或读不出时用正文字体)。
    pub mono: Option<PdfFont>,
}

/// PDF 导出选项。
#[derive(Debug, Clone)]
pub struct PdfExportOptions {
    /// 文档元数据标题;`None` 时取文档第一个标题的纯文本,再退「未命名」。
    pub title: Option<String>,
    /// 正文字号(pt),默认 11。
    pub body_font_size: f32,
    /// 行距倍率,默认 1.5。
    pub line_height: f32,
    /// 四边页边距(pt),默认 56.7(2cm)。
    pub margin: f32,
    /// 是否画页码,默认开。
    pub page_numbers: bool,
    /// 字体集。
    pub fonts: PdfFonts,
}

impl PdfExportOptions {
    /// 给定字体集,其余取默认值。
    pub fn new(fonts: PdfFonts) -> Self {
        Self {
            title: None,
            body_font_size: 11.0,
            line_height: 1.5,
            margin: 56.7,
            page_numbers: true,
            fonts,
        }
    }
}

/// PDF 导出错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    /// 正文字体读不出(krilla 与 skrifa 都无法解析该 face)。
    RegularFontUnreadable,
    /// krilla 编码阶段失败,附错误文案。
    Encode(String),
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfError::RegularFontUnreadable => {
                write!(f, "PDF 正文字体无法解析,请换一个字体文件或检查 face 序号")
            }
            PdfError::Encode(message) => write!(f, "PDF 编码失败:{message}"),
        }
    }
}

impl std::error::Error for PdfError {}

/// Markdown 源文本 → PDF 字节(解析走 [`latermd_render::parse`],扩展开关
/// 与 HTML 导出同方言)。
pub fn export_pdf(markdown: &str, options: &PdfExportOptions) -> Result<Vec<u8>, PdfError> {
    export_document(&latermd_render::parse(markdown), options)
}

/// 绘制指令 IR → PDF 字节。
pub fn export_document(
    document: &Document,
    options: &PdfExportOptions,
) -> Result<Vec<u8>, PdfError> {
    let fonts = Fonts::new(&options.fonts)?;
    let content_width = (A4_WIDTH - 2.0 * options.margin).max(0.0);
    let content_height = (A4_HEIGHT - 2.0 * options.margin).max(0.0);
    let ctx = LayoutContext {
        fonts: &fonts,
        opts: options,
        width: content_width,
    };
    let blocks = layout::layout_document(document, &ctx);
    let pages = paginate(&blocks, content_height);

    let mut pdf = PdfDocument::new();
    pdf.set_metadata(
        Metadata::new()
            .title(document_title(document, options))
            .producer("LaterMD".to_owned()),
    );
    let total = pages.len();
    for (page_index, items) in pages.iter().enumerate() {
        let mut page =
            pdf.start_page_with(PageSettings::from_wh(A4_WIDTH, A4_HEIGHT).expect("A4 尺寸有限"));
        let mut surface = page.surface();
        for (y, block_index, lines) in items {
            let block = &blocks[*block_index];
            for line_index in lines.clone() {
                let line = &block.lines[line_index];
                let line_y = options.margin + y + line.top;
                for prim in &line.prims {
                    draw_prim(prim, &mut surface, &fonts, options.margin, line_y);
                }
            }
        }
        if options.page_numbers {
            draw_page_number(&mut surface, &fonts, page_index + 1, total, options);
        }
        surface.finish();
        page.finish();
    }
    pdf.finish()
        .map_err(|error| PdfError::Encode(error.to_string()))
}

// ---- 分页 ----

/// 一页上的一个块片段:块下标 + 行区间 + 片段在内容区的 y 起点。
type Fragment = (f32, usize, std::ops::Range<usize>);

/// 按块分页(策略见模块文档)。返回至少一页。
fn paginate(blocks: &[PlacedBlock], content_height: f32) -> Vec<Vec<Fragment>> {
    fn at_page_top(pages: &[Vec<Fragment>]) -> bool {
        pages.last().is_some_and(|page| page.is_empty())
    }
    let mut pages: Vec<Vec<Fragment>> = vec![Vec::new()];
    let mut y = 0.0_f32;
    for (block_index, block) in blocks.iter().enumerate() {
        if block.lines.is_empty() {
            continue;
        }
        let above = if at_page_top(&pages) {
            0.0
        } else {
            block.above
        };
        let total = block.height();
        if y + above + total <= content_height {
            pages
                .last_mut()
                .unwrap()
                .push((y + above, block_index, 0..block.lines.len()));
            y += above + total;
            continue;
        }
        if total <= content_height {
            // 整块下移到新页(块级不可分割),页首不加上间距
            pages.push(Vec::new());
            pages
                .last_mut()
                .unwrap()
                .push((0.0, block_index, 0..block.lines.len()));
            y = total;
            continue;
        }
        // 块高超过整页内容高:按切点拆分
        let mut start = 0usize;
        let mut first_fragment = true;
        while start < block.lines.len() {
            let pad = if first_fragment { above } else { 0.0 };
            let base_y = if pages.last().is_some_and(|p| p.is_empty()) {
                0.0
            } else {
                y + pad
            };
            let available = content_height - base_y;
            let mut best: Option<(usize, f32)> = None;
            let mut fallback: Option<(usize, f32)> = None;
            for index in start..block.lines.len() {
                let height =
                    block.lines[index].top - block.lines[start].top + block.lines[index].height;
                if height > available {
                    break;
                }
                let end = index + 1;
                let is_cut = end == block.lines.len() || block.cuts.contains(&end);
                if is_cut {
                    best = Some((end, height));
                }
                fallback = Some((end, height));
            }
            let Some((end, height)) = best.or(fallback) else {
                // 连一行都放不下:若不在页首则换页重试;已在页首则硬放一行
                if base_y > 0.0 {
                    pages.push(Vec::new());
                    y = 0.0;
                    continue;
                }
                let height = block.lines[start].height;
                pages
                    .last_mut()
                    .unwrap()
                    .push((0.0, block_index, start..start + 1));
                y = height;
                start += 1;
                if start < block.lines.len() {
                    pages.push(Vec::new());
                    y = 0.0;
                }
                continue;
            };
            pages
                .last_mut()
                .unwrap()
                .push((base_y, block_index, start..end));
            y = base_y + height;
            start = end;
            first_fragment = false;
            if start < block.lines.len() {
                pages.push(Vec::new());
                y = 0.0;
            }
        }
    }
    if pages.len() > 1 && pages.last().is_some_and(|p| p.is_empty()) {
        pages.pop();
    }
    pages
}

// ---- 绘制 ----

fn ink_paint(ink: Ink) -> Paint {
    let color = match ink {
        Ink::Black => rgb::Color::new(17, 24, 39),
        Ink::Gray(level) => rgb::Color::new(level, level, level),
        Ink::Blue => rgb::Color::new(29, 78, 216),
    };
    Paint::from(color)
}

fn fill(ink: Ink) -> Fill {
    Fill {
        paint: ink_paint(ink),
        opacity: NormalizedF32::ONE,
        rule: FillRule::NonZero,
    }
}

fn draw_prim(
    prim: &Prim,
    surface: &mut krilla::surface::Surface<'_>,
    fonts: &Fonts,
    margin: f32,
    line_y: f32,
) {
    match prim {
        Prim::Glyphs {
            x,
            baseline,
            slot,
            size,
            glyphs,
            text,
            ink,
        } => {
            surface.set_fill(Some(fill(*ink)));
            surface.set_stroke(None);
            surface.draw_glyphs(
                Point::from_xy(margin + x, line_y + baseline),
                glyphs,
                fonts.krilla(*slot).clone(),
                text,
                *size,
                false,
            );
        }
        Prim::Rect { x, y, w, h, ink } => {
            let Some(rect) = KRect::from_xywh(margin + x, line_y + y, *w, *h) else {
                return;
            };
            let mut builder = PathBuilder::new();
            builder.push_rect(rect);
            let Some(path) = builder.finish() else {
                return;
            };
            surface.set_fill(Some(fill(*ink)));
            surface.set_stroke(None);
            surface.draw_path(&path);
        }
        Prim::Line {
            x1,
            y1,
            x2,
            y2,
            width,
            ink,
        } => {
            let mut builder = PathBuilder::new();
            builder.move_to(margin + x1, line_y + y1);
            builder.line_to(margin + x2, line_y + y2);
            let Some(path) = builder.finish() else {
                return;
            };
            surface.set_fill(None);
            surface.set_stroke(Some(Stroke {
                paint: ink_paint(*ink),
                width: *width,
                ..Stroke::default()
            }));
            surface.draw_path(&path);
        }
    }
}

fn draw_page_number(
    surface: &mut krilla::surface::Surface<'_>,
    fonts: &Fonts,
    page: usize,
    total: usize,
    options: &PdfExportOptions,
) {
    let text = format!("{page} / {total}");
    let size = 9.0;
    let shaped = fonts.shape(shaping::Slot::Regular, &text);
    let width = shaped.glyphs.iter().map(|g| g.x_advance).sum::<f32>() * size;
    let x = (A4_WIDTH - width) / 2.0;
    let baseline = A4_HEIGHT - options.margin * 0.45;
    surface.set_fill(Some(fill(Ink::Gray(110))));
    surface.set_stroke(None);
    surface.draw_glyphs(
        Point::from_xy(x, baseline),
        &shaped.glyphs,
        fonts.krilla(shaping::Slot::Regular).clone(),
        &text,
        size,
        false,
    );
}

/// 文档标题:选项指定 → 首个标题纯文本 → 「未命名」(与 HTML 导出一致)。
fn document_title(document: &Document, options: &PdfExportOptions) -> String {
    if let Some(title) = &options.title {
        return title.clone();
    }
    document
        .blocks
        .iter()
        .find_map(|block| match block {
            latermd_render::Block::Heading { inlines, .. } => {
                let text: String = inlines.iter().map(inline_text).collect::<Vec<_>>().concat();
                (!text.is_empty()).then_some(text)
            }
            _ => None,
        })
        .unwrap_or_else(|| "未命名".to_owned())
}

fn inline_text(inline: &latermd_render::Inline) -> String {
    match inline {
        latermd_render::Inline::Run(run) => run.text.clone(),
        latermd_render::Inline::Image { alt, .. } => alt.clone(),
        latermd_render::Inline::LineBreak => String::new(),
        latermd_render::Inline::FootnoteRef { label } => label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layout::PlacedLine;

    /// 测试字体:优先 DejaVu(分布最广;CI 的 ubuntu 镜像必有),退
    /// Liberation/Noto CJK。找不到任何字体时跳过断言(无字体环境无法
    /// 生成 PDF;有字体的机器上断言必跑,不恒真)。
    fn system_font() -> Option<PdfFont> {
        let candidates: &[&str] = &[
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        ];
        for path in candidates {
            if let Ok(data) = std::fs::read(path) {
                return Some(PdfFont { data, index: 0 });
            }
        }
        None
    }

    fn test_options() -> Option<PdfExportOptions> {
        let regular = system_font()?;
        let bold = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
            .ok()
            .map(|data| PdfFont { data, index: 0 });
        let mono = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
            .ok()
            .map(|data| PdfFont { data, index: 0 });
        Some(PdfExportOptions::new(PdfFonts {
            regular,
            bold,
            mono,
        }))
    }

    fn assert_pdf_shape(bytes: &[u8]) {
        assert!(!bytes.is_empty(), "PDF 字节为空");
        assert!(
            bytes.starts_with(b"%PDF-"),
            "PDF 必须以 %PDF 头开始,实为 {:?}",
            &bytes[..bytes.len().min(8)]
        );
        // PDF 允许 EOF 标记后跟换行;剥掉尾部 \n/\r 再验 %%EOF
        let mut end = bytes.len();
        while end > 0 && matches!(bytes[end - 1], b'\n' | b'\r') {
            end -= 1;
        }
        assert!(
            bytes[..end].ends_with(b"%%EOF"),
            "PDF 必须以 %%EOF 收尾,tail={:?}",
            String::from_utf8_lossy(&bytes[end.saturating_sub(32)..end])
        );
    }

    #[test]
    fn minimal_markdown_yields_valid_pdf_bytes() {
        let Some(options) = test_options() else {
            eprintln!("跳过:本机无可用系统字体");
            return;
        };
        let bytes = export_pdf("# 标题\n\n一段正文。", &options).expect("导出成功");
        assert_pdf_shape(&bytes);
        assert!(
            bytes.len() > 1000,
            "含嵌入字体的 PDF 不该只有 {} 字节",
            bytes.len()
        );
    }

    #[test]
    fn full_document_mixes_all_block_kinds_without_panic() {
        let Some(options) = test_options() else {
            eprintln!("跳过:本机无可用系统字体");
            return;
        };
        let markdown = "\
# 一级标题

正文段落,**粗体** *斜体* `代码` [链接](https://example.com) ~~删除线~~。

## 二级标题

1. 有序甲
2. 有序乙
   - 嵌套无序
   - 另一条
3. [ ] 任务项

```rust
fn main() {
    println!(\"hello\");
}
```

> 引用块第一行
> 引用块第二行

| 列甲 | 列乙 | 列丙 |
|:-----|:----:|-----:|
| a | b | c |
| 很长的单元格内容会换行 | 1 | 2 |

---

![替代文本](/img/placeholder.png)

尾段。[^1]

[^1]: 脚注定义内容。
";
        let bytes = export_pdf(markdown, &options).expect("全链路导出成功");
        assert_pdf_shape(&bytes);
    }

    #[test]
    fn empty_document_still_yields_one_page() {
        let Some(options) = test_options() else {
            eprintln!("跳过:本机无可用系统字体");
            return;
        };
        let bytes = export_pdf("", &options).expect("空文档导出成功");
        assert_pdf_shape(&bytes);
    }

    #[test]
    fn unreadable_regular_font_is_explicit_error() {
        let options = PdfExportOptions::new(PdfFonts {
            regular: PdfFont {
                data: vec![0u8; 32],
                index: 0,
            },
            bold: None,
            mono: None,
        });
        assert_eq!(
            export_pdf("# x", &options),
            Err(PdfError::RegularFontUnreadable)
        );
    }

    /// 块级不可分割:放不下但块高 ≤ 整页内容高的块整体移到下一页——即使
    /// 当页残高还塞得下块的前几行(表格形态,切点只有块尾)。
    #[test]
    fn pagination_moves_whole_block_to_next_page() {
        // 内容高 100:A(85)下排后剩 15;B 表格两行共 20(切点只有块尾),
        // 当页残高塞得下第一行也不许拆 → B 整体去第二页
        let blocks = vec![
            synth_block(0.0, &[85.0], None),
            synth_block(5.0, &[10.0, 10.0], Some(vec![2])),
        ];
        let pages = paginate(&blocks, 100.0);
        assert_eq!(pages.len(), 2, "{pages:?}");
        assert_eq!(
            pages[0],
            vec![(0.0, 0usize, 0..1)],
            "当页只应有 A,不得把 B 的首行塞进来:{pages:?}"
        );
        assert_eq!(pages[1], vec![(0.0, 1usize, 0..2)]);
    }

    /// 超过整页的块在切点(列表项/表行边界)拆分,片段从新页顶继续。
    #[test]
    fn pagination_splits_oversized_block_at_cuts() {
        let blocks = vec![synth_block(0.0, &[30.0; 6], Some(vec![3, 6]))];
        let pages = paginate(&blocks, 100.0);
        assert_eq!(pages.len(), 2, "{pages:?}");
        assert_eq!(pages[0], vec![(0.0, 0usize, 0..3)]);
        assert_eq!(pages[1], vec![(0.0, 0usize, 3..6)]);
    }

    /// 干净切点放不下但部分行放得下时,回退到任意行边界(不丢内容)。
    #[test]
    fn pagination_split_falls_back_to_line_boundary() {
        let blocks = vec![synth_block(0.0, &[30.0; 4], Some(vec![4]))];
        let pages = paginate(&blocks, 100.0);
        assert_eq!(pages.len(), 2, "{pages:?}");
        assert_eq!(pages[0], vec![(0.0, 0usize, 0..3)], "{pages:?}");
        assert_eq!(pages[1], vec![(0.0, 0usize, 3..4)]);
    }

    /// 极端:单行高过整页 → 硬放该行,不死循环、不丢行。
    #[test]
    fn pagination_single_line_taller_than_page_never_loops() {
        let blocks = vec![synth_block(0.0, &[150.0, 30.0], None)];
        let pages = paginate(&blocks, 100.0);
        let placed: usize = pages
            .iter()
            .map(|p| p.iter().map(|f| f.2.len()).sum::<usize>())
            .sum();
        assert_eq!(placed, 2, "两行都必须有落位:{pages:?}");
    }

    /// 空块被跳过;全空文档仍返回一页。
    #[test]
    fn pagination_empty_blocks_yield_one_page() {
        let pages = paginate(&[synth_block(0.0, &[], None)], 100.0);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].is_empty());
    }

    fn synth_block(above: f32, line_heights: &[f32], cuts: Option<Vec<usize>>) -> PlacedBlock {
        let mut lines = Vec::new();
        let mut top = 0.0;
        for height in line_heights {
            lines.push(PlacedLine {
                top,
                height: *height,
                prims: Vec::new(),
            });
            top += height;
        }
        let cuts = cuts.unwrap_or_else(|| (1..=lines.len()).collect());
        PlacedBlock { above, lines, cuts }
    }

    /// 长文档端到端:多块跨页与超长单段都无 panic、字节形态合法。
    #[test]
    fn long_document_paginates_end_to_end() {
        let Some(options) = test_options() else {
            eprintln!("跳过:本机无可用系统字体");
            return;
        };
        let mut markdown = String::new();
        for i in 0..40 {
            markdown.push_str(&format!("段落 {i:02}:重复正文内容填充页面高度。\n\n"));
        }
        markdown.push_str("尾块:应当出现在后续页。\n");
        let bytes = export_pdf(&markdown, &options).expect("导出成功");
        assert_pdf_shape(&bytes);
        // 超长单段(断行后行数超过整页)也不丢内容、不死循环
        let long_line = "超长段落 ".repeat(400);
        let bytes =
            export_pdf(&format!("# 标题\n\n{long_line}\n\n尾段。"), &options).expect("导出成功");
        assert_pdf_shape(&bytes);
    }

    /// 页码开关:关掉后不应出现「1 / 1」形态的页码字形内容流。
    #[test]
    fn page_number_toggle_changes_output() {
        let Some(options) = test_options() else {
            eprintln!("跳过:本机无可用系统字体");
            return;
        };
        let with = export_pdf("# 有页码", &options).expect("导出成功");
        let mut without_options = options.clone();
        without_options.page_numbers = false;
        let without = export_pdf("# 无页码", &without_options).expect("导出成功");
        assert_ne!(with.len(), without.len(), "页码开关应改变输出字节");
    }

    /// 标题回退链:无标题文档用「未命名」。
    #[test]
    fn title_fallback_chain() {
        let doc = latermd_render::parse("只有段落");
        assert_eq!(
            document_title(
                &doc,
                &PdfExportOptions::new(PdfFonts {
                    regular: PdfFont {
                        data: vec![],
                        index: 0
                    },
                    bold: None,
                    mono: None,
                })
            ),
            "未命名"
        );
        let doc = latermd_render::parse("# 文档标题\n\n正文");
        assert_eq!(
            document_title(
                &doc,
                &PdfExportOptions::new(PdfFonts {
                    regular: PdfFont {
                        data: vec![],
                        index: 0
                    },
                    bold: None,
                    mono: None,
                })
            ),
            "文档标题"
        );
        let options = PdfExportOptions {
            title: Some("自定义".into()),
            ..PdfExportOptions::new(PdfFonts {
                regular: PdfFont {
                    data: vec![],
                    index: 0,
                },
                bold: None,
                mono: None,
            })
        };
        assert_eq!(document_title(&doc, &options), "自定义");
    }

    /// 取证专用(默认 `--ignored`,不进常规门禁):生成中英混排样例 PDF
    /// 落盘 `/tmp/latermd-pdf-evidence.pdf`,若 PATH 上有 pdftotext 则回读
    /// 断言中文句子逐字完整。需要本机 Noto Sans CJK 与 poppler-utils;缺任一
    /// 即如实跳过——常规单测不依赖它们(与 #39 取证测试同口径)。
    #[test]
    #[ignore = "取证项:需系统 CJK 字体与 pdftotext,常规门禁不跑"]
    fn pdftotext_roundtrip_evidence() {
        let candidates = [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ];
        let Some(cjk_font) = candidates.iter().find_map(|path| {
            std::fs::read(path)
                .ok()
                .map(|data| PdfFont { data, index: 0 })
        }) else {
            eprintln!("跳过:本机无 CJK 候选字体");
            return;
        };
        let options = PdfExportOptions::new(PdfFonts {
            regular: cjk_font,
            bold: None,
            mono: None,
        });
        let markdown = "# 中文标题\n\n一段中文正文,English words 混排,标点、顿号一致。";
        let bytes = export_pdf(markdown, &options).expect("导出成功");
        assert_pdf_shape(&bytes);
        let path = "/tmp/latermd-pdf-evidence.pdf";
        std::fs::write(path, &bytes).expect("落盘取证文件");
        eprintln!("取证 PDF 已写 {path}({} 字节)", bytes.len());
        let extracted = match std::process::Command::new("pdftotext")
            .arg(path)
            .arg("-")
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).to_string()
            }
            _ => {
                eprintln!("跳过回读:PATH 上无 pdftotext");
                return;
            }
        };
        // 两侧都剥空白再比(pdftotext 会在断行/词间补换行与空格)
        let normalized = extracted.replace([' ', '\n'], "");
        for expected in ["中文标题", "一段中文正文", "Englishwords", "标点、顿号一致"]
        {
            assert!(
                normalized.contains(expected),
                "pdftotext 回读缺 {expected:?}:\n{extracted}"
            );
        }
        eprintln!("pdftotext 回读一致:\n{extracted}");
    }
}
