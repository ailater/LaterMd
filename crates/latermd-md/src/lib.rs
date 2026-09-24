#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! 拥有型 Markdown 文档模型与大纲数据层。
//!
//! 包裹 vendored [`egui_markdown`] 的解析器(pulldown-cmark 前端),把借用型
//! [`egui_markdown::parser::parse`] 的结果转成拥有型 [`MarkdownDoc`],文档模型
//! 因此可以脱离源字符串的生命周期,存进编辑器缓冲或跨线程传递。
//!
//! 数据层 crate:不依赖 egui/eframe;把 token 流翻译为绘制是 UI 侧的职责。

use std::ops::Range;

use egui_markdown::types::{TableData, Token};
use pulldown_cmark::CowStr;

/// 大纲条目,对应一个标题 token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineItem {
    /// 标题层级(1–6)。
    pub level: u8,
    /// 标题纯文本,不含 `#` 标记。
    pub text: String,
    /// 标题在 [`MarkdownDoc::text`] 中的字节区间,大纲点击跳转用它定位光标。
    pub span: Range<usize>,
}

/// 拥有型文档模型:源文本、token 流与平行的源码区间。
///
/// `tokens` 与 `spans` 等长且区间连续(继承自 vendored 解析器的不变量),任意
/// 字节偏移都落在恰好一个 token 的区间内 —— 大纲跳转与 Live Preview 依赖这一点。
pub struct MarkdownDoc {
    /// 源文本。
    pub text: String,
    /// 拥有型 token 流,与 `spans` 平行。
    pub tokens: Vec<Token<'static>>,
    /// 各 token 的源码字节区间,与 `tokens` 平行。
    pub spans: Vec<Range<usize>>,
}

impl MarkdownDoc {
    /// 提取标题大纲。
    ///
    /// 只遍历顶层 token,不深入表格单元格;凡 [`Token::Text`] 的样式带 heading
    /// 层级即产出一条。同一标题行含内联格式时会拆成多条,廉价版大纲可接受。
    pub fn outline(&self) -> Vec<OutlineItem> {
        self.tokens
            .iter()
            .zip(&self.spans)
            .filter_map(|(token, span)| match token {
                Token::Text { text, style } => style.heading.map(|level| OutlineItem {
                    level,
                    text: text.to_string(),
                    span: span.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

/// 提取标题大纲(借用型快路径)。
///
/// 与 [`MarkdownDoc::outline`] 产出一致,但不做借转拥有的整篇 token 拷贝,
/// 只为命中的标题分配字符串。编辑器每次修订号前进都要重算大纲,应走这一
/// 条;已持有 [`MarkdownDoc`] 的离线场景用方法形态即可。
pub fn outline(text: &str) -> Vec<OutlineItem> {
    let md = egui_markdown::parser::parse(text);
    md.tokens
        .iter()
        .zip(&md.spans)
        .filter_map(|(token, span)| match token {
            Token::Text { text, style } => style.heading.map(|level| OutlineItem {
                level,
                text: text.to_string(),
                span: span.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// 解析 Markdown 文本,产出拥有型文档模型(统一入口)。
///
/// 代价是两次拷贝:源文本进 `String`,借用型 `CowStr::Borrowed` 转堆上的
/// `CowStr::Boxed`;换来的是结果不借用任何外部数据。
pub fn parse(text: &str) -> MarkdownDoc {
    let md = egui_markdown::parser::parse(text);
    MarkdownDoc {
        tokens: md.tokens.iter().map(token_to_owned).collect(),
        spans: md.spans,
        text: text.to_string(),
    }
}

/// 借用型 token 转 `Token<'static>`。
///
/// 做法与 vendor `label.rs` 的 `tokens_to_owned` 一致:所有 `CowStr` 一律转
/// `CowStr::Boxed`(堆分配,不指向源文本),Table 递归处理单元格。
fn token_to_owned(token: &Token<'_>) -> Token<'static> {
    match token {
        Token::Newline => Token::Newline,
        Token::Text { text, style } => Token::Text {
            text: cowstr_to_owned(text),
            style: style.clone(),
        },
        Token::CodeBlock { text, language } => Token::CodeBlock {
            text: cowstr_to_owned(text),
            language: language.as_ref().map(cowstr_to_owned),
        },
        Token::Link { text, href, title } => Token::Link {
            text: cowstr_to_owned(text),
            href: cowstr_to_owned(href),
            title: title.as_ref().map(cowstr_to_owned),
        },
        Token::ListMarker {
            marker,
            indent_level,
        } => Token::ListMarker {
            marker: cowstr_to_owned(marker),
            indent_level: *indent_level,
        },
        Token::Image { alt, url, title } => Token::Image {
            alt: cowstr_to_owned(alt),
            url: cowstr_to_owned(url),
            title: title.as_ref().map(cowstr_to_owned),
        },
        Token::Table(data) => Token::Table(TableData {
            alignments: data.alignments.clone(),
            headers: data
                .headers
                .iter()
                .map(|cell| cell.iter().map(token_to_owned).collect())
                .collect(),
            rows: data
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.iter().map(token_to_owned).collect())
                        .collect()
                })
                .collect(),
        }),
        Token::HorizontalRule => Token::HorizontalRule,
        Token::BlockquoteStart => Token::BlockquoteStart,
        Token::BlockquoteEnd => Token::BlockquoteEnd,
        Token::TaskListMarker {
            checked,
            indent_level,
        } => Token::TaskListMarker {
            checked: *checked,
            indent_level: *indent_level,
        },
        Token::FootnoteRef { label } => Token::FootnoteRef {
            label: cowstr_to_owned(label),
        },
        Token::FootnoteDef { label } => Token::FootnoteDef {
            label: cowstr_to_owned(label),
        },
    }
}

fn cowstr_to_owned(s: &CowStr<'_>) -> CowStr<'static> {
    CowStr::Boxed(s.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// span 覆盖源文本 `needle` 的字节区间。
    fn covers(doc: &MarkdownDoc, span: &Range<usize>, needle: &str) -> bool {
        let Some(start) = doc.text.find(needle) else {
            return false;
        };
        let end = start + needle.len();
        span.start <= start && span.end >= end
    }

    #[test]
    fn outline_levels_h1_to_h6() {
        let src = "# one\n\n## two\n\n### three\n\n#### four\n\n##### five\n\n###### six";
        let doc = parse(src);
        let outline = doc.outline();
        let levels: Vec<u8> = outline.iter().map(|item| item.level).collect();
        assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);
        let texts: Vec<&str> = outline.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts, vec!["one", "two", "three", "four", "five", "six"]);
        for item in &outline {
            assert!(
                covers(
                    &doc,
                    &item.span,
                    &format!("{} {}", "#".repeat(item.level as usize), item.text)
                ),
                "span {:?} misses heading {:?}",
                item.span,
                item.text
            );
        }
    }

    #[test]
    fn outline_chinese_headings() {
        let src = "# 架构设计\n\n正文段落。\n\n## 中文二级标题\n";
        let doc = parse(src);
        let outline = doc.outline();
        assert_eq!(outline.len(), 2);
        assert_eq!(
            (outline[0].level, outline[0].text.as_str()),
            (1, "架构设计")
        );
        assert_eq!(
            (outline[1].level, outline[1].text.as_str()),
            (2, "中文二级标题")
        );
        assert!(covers(&doc, &outline[0].span, "# 架构设计"));
        assert!(covers(&doc, &outline[1].span, "## 中文二级标题"));
    }

    #[test]
    fn outline_empty_without_headings() {
        let src = "只有段落和 **粗体**。\n\n- 列表项\n\n```rust\nfn main() {}\n```\n\n> 引用\n";
        let doc = parse(src);
        assert!(!doc.tokens.is_empty());
        assert!(doc.outline().is_empty());
        assert!(outline(src).is_empty());
    }

    /// 借用型快路径与拥有型方法形态产出一致,两处实现不得漂移。
    #[test]
    fn borrowed_outline_matches_doc_outline() {
        let src = "# 甲\n\n## 乙 *强调*\n\n正文 ### 非标题\n";
        assert_eq!(outline(src), parse(src).outline());
    }

    #[test]
    fn spans_parallel_and_contiguous() {
        let docs = [
            "# 标题\n\n段落 **粗体** 与 `code`。\n\n- a\n- b\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nfn f() {}\n```\n",
            "> 引用\n\n[^1]: 脚注\n\n引用[^1]。\n\n---\n",
            "无格式文本",
            "",
        ];
        for src in docs {
            let doc = parse(src);
            assert_eq!(doc.tokens.len(), doc.spans.len(), "not parallel: {src:?}");
            let mut prev_end = 0;
            for (i, span) in doc.spans.iter().enumerate() {
                assert_eq!(span.start, prev_end, "span {i} not contiguous: {src:?}");
                assert!(span.end >= span.start, "span {i} inverted: {src:?}");
                assert!(span.end <= src.len(), "span {i} beyond source: {src:?}");
                prev_end = span.end;
            }
        }
    }

    /// 拥有型的意义:文档模型活得比源字符串久。
    #[test]
    fn doc_outlives_source() {
        let doc = {
            let src = "# 临时标题".to_string();
            parse(&src)
        };
        assert_eq!(doc.outline()[0].text, "临时标题");
        assert_eq!(doc.text, "# 临时标题");
    }

    /// 借转拥有不丢数据:表格的表头/行/单元格逐层对照。
    #[test]
    fn table_survives_owned_conversion() {
        let doc = parse("| 列甲 | 列乙 |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        let borrowed = egui_markdown::parser::parse(&doc.text);
        let owned_table = doc
            .tokens
            .iter()
            .find_map(|t| match t {
                Token::Table(data) => Some(data),
                _ => None,
            })
            .expect("owned table token");
        let borrowed_table = borrowed
            .tokens
            .iter()
            .find_map(|t| match t {
                Token::Table(data) => Some(data),
                _ => None,
            })
            .expect("borrowed table token");
        assert_eq!(owned_table.alignments, borrowed_table.alignments);
        assert_eq!(owned_table.headers.len(), borrowed_table.headers.len());
        assert_eq!(owned_table.rows.len(), borrowed_table.rows.len());
        let cell_text = |data: &TableData<'_>, row: usize, col: usize| -> String {
            data.rows[row][col]
                .iter()
                .map(|t| t.text().to_string())
                .collect()
        };
        assert_eq!(
            cell_text(owned_table, 0, 0),
            cell_text(borrowed_table, 0, 0)
        );
        assert_eq!(
            cell_text(owned_table, 1, 1),
            cell_text(borrowed_table, 1, 1)
        );
    }
}
