//! 字体绑定与文本整形。
//!
//! 布局(测宽)与绘制共用同一份整形结果: [`Fonts::shape`] 产出的
//! [`KrillaGlyph`](krilla::text::KrillaGlyph) 既用于累加行宽,又原样交给
//! `Surface::draw_glyphs` 绘制——不存在「测出来的宽」与「画出来的宽」两套
//! 数值。垂直度量(ascent/descent)经 skrifa 读取,与 krilla 内部同一解析器。

use std::sync::Arc;

use krilla::text::{Font as KrillaFont, GlyphId, KrillaGlyph};
use krilla::Data;
use rustybuzz::{Direction, UnicodeBuffer};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use super::PdfFont;

/// 字体槽位:正文 / 粗体 / 等宽(代码)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    /// 正文。
    Regular,
    /// 粗体(未提供独立粗体字体时回落 Regular)。
    Bold,
    /// 等宽代码(未提供时回落 Regular)。
    Mono,
}

/// 一个可绘制字体:krilla 句柄(绘制)+ 原始字节(整形/度量)。
#[derive(Clone)]
pub(super) struct Face {
    data: Arc<Vec<u8>>,
    index: u32,
    font: KrillaFont,
    upem: f32,
}

/// 三槽位字体集。
pub(super) struct Fonts {
    regular: Face,
    bold: Face,
    mono: Face,
}

impl Fonts {
    /// 绑定字体。正文字体不可读是硬错误;粗体/等宽给了但读不出时回落
    /// 正文(可用性优先,不因可选字重坏掉整份导出)。
    pub(super) fn new(fonts: &super::PdfFonts) -> Result<Self, super::PdfError> {
        let regular = Face::new(&fonts.regular).ok_or(super::PdfError::RegularFontUnreadable)?;
        let bold = fonts
            .bold
            .as_ref()
            .and_then(Face::new)
            .unwrap_or_else(|| regular.clone());
        let mono = fonts
            .mono
            .as_ref()
            .and_then(Face::new)
            .unwrap_or_else(|| regular.clone());
        Ok(Self {
            regular,
            bold,
            mono,
        })
    }

    pub(super) fn face(&self, slot: Slot) -> &Face {
        match slot {
            Slot::Regular => &self.regular,
            Slot::Bold => &self.bold,
            Slot::Mono => &self.mono,
        }
    }

    /// 槽位字体在给定字号下的 (ascent, descent);descent 为负值。
    pub(super) fn vertical(&self, slot: Slot, size: f32) -> (f32, f32) {
        self.face(slot).vertical(size)
    }

    /// krilla 绘制句柄。
    pub(super) fn krilla(&self, slot: Slot) -> &KrillaFont {
        &self.face(slot).font
    }

    /// 整形一段文本(rustybuzz,与 krilla 内部同一引擎同一版本)。
    pub(super) fn shape(&self, slot: Slot, text: &str) -> ShapedText {
        shape_text(self.face(slot), text)
    }
}

impl Face {
    fn new(pdf_font: &PdfFont) -> Option<Self> {
        let data = Arc::new(pdf_font.data.clone());
        let font = KrillaFont::new(Data::from(data.clone()), pdf_font.index)?;
        let upem = font.units_per_em();
        if upem <= 0.0 {
            return None;
        }
        Some(Self {
            data,
            index: pdf_font.index,
            font,
            upem,
        })
    }

    fn vertical(&self, size: f32) -> (f32, f32) {
        let Ok(font_ref) = FontRef::from_index(self.data.as_slice(), self.index) else {
            // 解析不出度量时用保守估值,保证流程不中断。
            return (size * 0.8, -size * 0.25);
        };
        let metrics = font_ref.metrics(Size::new(size), LocationRef::default());
        let fallback = (size * 0.8, -size * 0.25);
        match (metrics.ascent, metrics.descent) {
            (a, d) if a > 0.0 && d < 0.0 => (a, d),
            _ => fallback,
        }
    }
}

/// 整形结果:字形序列 + 合法切分点。
///
/// `cuts` 是 cluster(字形簇)边界表:`(文本内字节偏移, 下一簇首字形的
/// 下标)`。断行只允许发生在 cluster 边界——一个 cluster(如合字、附加
/// 符号组)绝不能从中间劈开。rustybuzz 的 cluster 值即文本字节偏移
/// (LTR 单调递增),与 krilla 内部 `naive_shape` 的语义一致。
pub(super) struct ShapedText {
    /// 字形序列(advance 已按 units_per_em 归一,乘字号即点数)。
    pub glyphs: Vec<KrillaGlyph>,
    /// cluster 边界表,按字节偏移升序。
    pub cuts: Vec<(usize, usize)>,
}

/// 整形 `text`;字体解析失败时返回空(布局层按零宽处理,不 panic)。
fn shape_text(face: &Face, text: &str) -> ShapedText {
    let mut shaped = ShapedText {
        glyphs: Vec::new(),
        cuts: Vec::new(),
    };
    if text.is_empty() {
        return shaped;
    }
    let Some(rb_face) = rustybuzz::Face::from_slice(face.data.as_slice(), face.index) else {
        return shaped;
    };
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    // 本模块只做 LTR 排版(中文/西文混排均为 LTR),RTL 不在 v1 范围。
    buffer.set_direction(Direction::LeftToRight);
    let output = rustybuzz::shape(&rb_face, &[], buffer);

    let infos = output.glyph_infos();
    let positions = output.glyph_positions();
    let count = output.len();
    let upem = face.upem;
    for i in 0..count {
        let info = &infos[i];
        let pos = positions[i];
        let start = info.cluster as usize;
        // cluster 结束字节 = 后续第一个不同 cluster 的起点(多字形簇的每个
        // 字形都拿到整簇区间,与 krilla naive_shape 的做法一致)。
        let end = infos[i + 1..]
            .iter()
            .find(|next| (next.cluster as usize) != start)
            .map(|next| next.cluster as usize)
            .unwrap_or(text.len());
        shaped.glyphs.push(KrillaGlyph::new(
            GlyphId::new(info.glyph_id),
            pos.x_advance as f32 / upem,
            pos.x_offset as f32 / upem,
            pos.y_offset as f32 / upem,
            0.0,
            start..end,
            None,
        ));
    }
    // 切分点:每簇结束字节 → 下一簇首字形下标。
    let mut i = 0;
    while i < count {
        let cluster = infos[i].cluster as usize;
        let mut j = i + 1;
        while j < count && infos[j].cluster as usize == cluster {
            j += 1;
        }
        let end_byte = if j < count {
            infos[j].cluster as usize
        } else {
            text.len()
        };
        if end_byte > cluster {
            shaped.cuts.push((end_byte, j));
        }
        i = j;
    }
    shaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_face() -> Option<Face> {
        // 任一可用系统字体即可验证整形几何;无字体环境跳过(断言不恒真:
        // 有字体的机器上必跑)。
        let data = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").ok()?;
        Face::new(&PdfFont { data, index: 0 })
    }

    #[test]
    fn shape_latin_cluster_boundaries() {
        let Some(face) = test_face() else {
            eprintln!("跳过:本机无 DejaVu Sans");
            return;
        };
        let shaped = shape_text(&face, "hi 你");
        assert_eq!(shaped.glyphs.len(), 4, "h/i/空格/你 各一簇");
        assert_eq!(shaped.cuts.len(), 4);
        // 每个簇边界都能映射到正确的字形下标
        assert_eq!(shaped.cuts[0], (1, 1));
        assert_eq!(shaped.cuts[2], (3, 3));
        assert_eq!(shaped.cuts[3], ("hi 你".len(), 4));
        // 宽度有值且有限:advance 归一化后乘字号
        let width: f32 = shaped.glyphs.iter().map(|g| g.x_advance).sum::<f32>() * 11.0;
        assert!(
            width > 10.0 && width < 40.0,
            "三个窄字符+空格 11pt 宽度异常: {width}"
        );
    }

    #[test]
    fn shape_empty_and_unreadable() {
        let Some(face) = test_face() else {
            eprintln!("跳过:本机无 DejaVu Sans");
            return;
        };
        let empty = shape_text(&face, "");
        assert!(empty.glyphs.is_empty() && empty.cuts.is_empty());
        let bad = Face::new(&PdfFont {
            data: vec![0u8; 16],
            index: 0,
        });
        assert!(bad.is_none(), "坏字体必须拒绝,不得 panic");
    }

    #[test]
    fn vertical_metrics_positive_ascent() {
        let Some(face) = test_face() else {
            eprintln!("跳过:本机无 DejaVu Sans");
            return;
        };
        let (ascent, descent) = face.vertical(11.0);
        assert!(
            ascent > 5.0 && ascent < 12.0,
            "11pt 的 ascent 量级异常: {ascent}"
        );
        assert!(
            descent < 0.0 && descent > -6.0,
            "11pt 的 descent 量级异常: {descent}"
        );
    }
}
