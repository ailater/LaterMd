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

/// 把文档切成**块**(P3 Live Preview 的骨架):返回每个块的源码字节区间。
///
/// 三条规定(由 Live Preview 的编辑语义倒逼出来):
///
/// 1. **覆盖全文、连续、无重叠**:块区间首尾相接且并集等于整篇 —— 光标块
///    在 Live Preview 下是被编辑的区间,若有字节落在任何块之外,在那儿敲
///    一个字符就会**静默丢失**。
/// 2. **块间空行归前一块**:段落之间的 `\n\n` 收进上一块的尾部(最后一块吃
///    到文末),这样每块的源码都是可直接独立解析的片段,富渲染也不会因为
///    前导空行在顶部空出一段。
/// 3. **块级 token 独占一块**:代码块、表格、分隔线、标题各自成块 —— 它们
///    内部的结构(表格行、代码缩进)不该被相邻段落的源码混进来。
///
/// 空文档返回空表;纯空行文档整篇算一块(编辑需要落点)。
pub fn blocks(text: &str) -> Vec<Range<usize>> {
    let md = egui_markdown::parser::parse(text);
    // ① 先按 token 划出「内容段」(记录内容起点与 span 终点),Newline 单独
    //    处理 —— 它的字节在 ② 里随边界分配。
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < md.tokens.len() {
        match &md.tokens[i] {
            Token::Newline => {
                i += 1;
            }
            // 块级:独占一段(代码块从此包含它的 ``` 起始 fence)。起点同样
            // 要跳过前导空行 —— 否则分隔符会归到代码块头上,富渲染时顶部
            // 凭空空出一段
            Token::CodeBlock { .. } | Token::Table(_) | Token::HorizontalRule => {
                let span = &md.spans[i];
                segments.push((skip_newlines(text, span.start), span.end));
                i += 1;
            }
            Token::Text { style, .. } if style.heading.is_some() => {
                let span = &md.spans[i];
                segments.push((content_start(text, span, &md.tokens[i]), span.end));
                i += 1;
            }
            // 其余:连续的非块级 token 合并成一段(列表项与它的文本同段)
            _ => {
                let start = content_start(text, &md.spans[i], &md.tokens[i]);
                let mut end = md.spans[i].end;
                let mut j = i + 1;
                while j < md.tokens.len() && !is_segment_break(&md.tokens[j]) {
                    end = md.spans[j].end;
                    j += 1;
                }
                segments.push((start, end));
                i = j;
            }
        }
    }
    if segments.is_empty() {
        // 没有内容 token(空文档 / 纯空行):整篇一块,保证有编辑落点
        return if text.is_empty() {
            Vec::new()
        } else {
            // 不用 `vec![0..len]`:clippy 的 single_range_in_vec_init 会把它
            // 读成「长度为 1 的 Range 序列」的误写;这里确实只要一个区间
            std::iter::once(0..text.len()).collect()
        };
    }
    // ② 连续化:每块**吃到下一块的内容起点**(块间空行归前一块),最后一块
    //    到文末。
    let mut blocks = Vec::with_capacity(segments.len());
    let mut cursor = 0_usize;
    for (index, (start, _end)) in segments.iter().enumerate() {
        let block_end = if index + 1 == segments.len() {
            text.len()
        } else {
            segments[index + 1].0.max(*start)
        };
        let block_start = cursor.min(*start);
        if block_end > block_start {
            blocks.push(block_start..block_end);
            cursor = block_end;
        }
    }
    blocks
}

/// 从 `pos` 起跳过连续的换行与回车(CRLF 的 `\r` 也算分隔符的一部分)。
fn skip_newlines(text: &str, mut pos: usize) -> usize {
    let bytes = text.as_bytes();
    while pos < bytes.len() && (bytes[pos] == b'\n' || bytes[pos] == b'\r') {
        pos += 1;
    }
    pos.min(text.len())
}

/// 段的**内容起点**。
///
/// 不能直接拿 span 起点:vendored 的平铺 span 会吸收前一块尾部的换行(后一段
/// 的 span 里含着 `\n\n` 前缀),代码块闭合的 ``` 也会被算进后一段的 span。
/// 这里用 token 自带的文本在 span 区间内定位真正的起点;块级 token(代码块
/// 的 fence 属于内容)没有这一步,直接用 span 起点。
fn content_start(text: &str, span: &Range<usize>, token: &Token<'_>) -> usize {
    let needle: Option<&str> = match token {
        Token::Text { text, .. } => Some(text.as_ref()),
        Token::ListMarker { marker, .. } => Some(marker.as_ref()),
        Token::Link { text, .. } => Some(text.as_ref()),
        Token::Image { alt, .. } => Some(alt.as_ref()),
        _ => None,
    };
    let end = span.end.min(text.len());
    match needle {
        Some(needle) if !needle.is_empty() && span.start <= end => text[span.start..end]
            .find(needle)
            .map_or(span.start, |offset| span.start + offset),
        _ => span.start,
    }
}

/// 是否要在此 token 处断开内容段(空行与块级 token 都算)。
fn is_segment_break(token: &Token<'_>) -> bool {
    match token {
        Token::Newline => true,
        Token::CodeBlock { .. } | Token::Table(_) | Token::HorizontalRule => true,
        Token::Text { style, .. } => style.heading.is_some(),
        _ => false,
    }
}

/// `[[wikilink]]` 展开成的链接 scheme(P3「双向链接」)。
///
/// 预览层按它拦截点击(打开同名文档),非本 scheme 的链接仍交系统浏览器。
pub const WIKI_SCHEME: &str = "wiki://";

/// 一条 `[[wikilink]]`:目标文档名与它在源码中的字节区间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wikilink {
    /// 目标文档名(`[[目标]]` 或 `[[目标|显示名]]` 的前半段)。
    pub target: String,
    /// 链接显示名;省略显示名时与目标同名。
    pub label: String,
    /// `[[` 起到 `]]` 止的源码区间。
    pub span: Range<usize>,
}

/// 抽出全部 `[[wikilink]]`。
///
/// **围栏代码块内的 `[[…]]` 不算链接** —— Rust 的 `arr[[0]]`、嵌套容器字面量
/// 都长这样,展开会把代码改坏。判定与 CommonMark 一致:以 ``` / ~~~ 围栏
/// 切换「代码中」状态。
pub fn wikilinks(text: &str) -> Vec<Wikilink> {
    let mut links = Vec::new();
    let mut in_code = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let rest = &text[index..];
        // 行首围栏切换代码态
        let line_start = index == 0 || bytes[index - 1] == b'\n';
        if line_start {
            let trimmed = rest.trim_start_matches([' ', '\t']);
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code = !in_code;
            }
        }
        if !in_code && rest.starts_with("[[") {
            if let Some(end) = rest.find("]]") {
                let inner = &rest[2..end];
                if !inner.contains('\n') && !inner.contains('[') {
                    let (target, label) = match inner.split_once('|') {
                        Some((target, label)) => (target.trim(), label.trim()),
                        None => (inner.trim(), inner.trim()),
                    };
                    if !target.is_empty() {
                        links.push(Wikilink {
                            target: target.to_owned(),
                            label: label.to_owned(),
                            span: index..index + end + 2,
                        });
                    }
                }
                index += end + 2;
                continue;
            }
        }
        // 按字符前进(不切断 UTF-8)
        let step = text[index..].chars().next().map_or(1, char::len_utf8);
        index += step;
    }
    links
}

/// 把 `[[目标]]` 展开成 Markdown 链接 `[显示名](<wiki://目标>)`,供**预览
/// 渲染**使用;源码本身一字不改(roadmap P0 验收:「`.md` 保持原样」)。
///
/// 目标用尖括号包裹:CommonMark 的 `<…>` 链接目标允许空格,中文与带空格的
/// 文档名才不会被解析器截断。
pub fn expand_wikilinks(text: &str) -> String {
    let links = wikilinks(text);
    if links.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + links.len() * 8);
    let mut cursor = 0;
    for link in links {
        out.push_str(&text[cursor..link.span.start]);
        out.push('[');
        out.push_str(&link.label);
        out.push_str("](<");
        out.push_str(WIKI_SCHEME);
        out.push_str(&link.target);
        out.push_str(">)");
        cursor = link.span.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// 定位指定标题的「节」在源文本中的字节区间:从该标题起到下一个**不深于**
/// 它的标题前(无则到文末)。更深层级的标题(`###`)是该节的子内容,一并
/// 属于节;标题文本按 `trim` 后全等匹配,层级精确匹配。
///
/// 起点与终点都取大纲条目的 `span.start`:平铺 span 会吸收前一块尾部的
/// 换行(含标题前的空行),因此移除该区间会连同节的前导空行一起带走,
/// 删后正文仍以合法换行结尾、下一个标题前也仍留有自己的空行。
pub fn heading_section_span(text: &str, level: u8, heading: &str) -> Option<Range<usize>> {
    let outline = outline(text);
    let index = outline
        .iter()
        .position(|item| item.level == level && item.text.trim() == heading)?;
    let start = outline[index].span.start;
    let end = outline[index + 1..]
        .iter()
        .find(|item| item.level <= level)
        .map_or_else(|| text.len(), |next| next.span.start);
    Some(start..end)
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

    /// wikilink:普通链接、`[[目标|显示名]]`、中文与带空格的目标。
    #[test]
    fn wikilinks_capture_target_label_and_span() {
        let text = "见 [[架构决策]] 与 [[Note One|笔记一]]、[[中文 文档]]。";
        let links = wikilinks(text);
        let targets: Vec<&str> = links.iter().map(|link| link.target.as_str()).collect();
        let labels: Vec<&str> = links.iter().map(|link| link.label.as_str()).collect();
        assert_eq!(targets, vec!["架构决策", "Note One", "中文 文档"]);
        assert_eq!(labels, vec!["架构决策", "笔记一", "中文 文档"]);
        for link in &links {
            assert!(
                text[link.span.start..link.span.end].starts_with("[["),
                "区间应对齐 [[…]]"
            );
            assert!(text[link.span.start..link.span.end].ends_with("]]"));
        }
    }

    /// 围栏代码块内的 `[[…]]` 不是链接(Rust 的 `arr[[0]]` 会被误伤)。
    #[test]
    fn wikilinks_skip_fenced_code_blocks() {
        let text = "正文 [[目标]]\n\n```rust\nlet v = arr[[0]];\n```\n\n尾 [[另一个]]\n";
        let links = wikilinks(text);
        let targets: Vec<&str> = links.iter().map(|link| link.target.as_str()).collect();
        assert_eq!(targets, vec!["目标", "另一个"]);
    }

    /// 展开:变成 `<wiki://目标>` 形式的链接,源码其它部分逐字保留。
    #[test]
    fn expand_wikilinks_rewrites_only_the_links() {
        let text = "见 [[架构决策]] 的下一节。";
        let expanded = expand_wikilinks(text);
        assert_eq!(expanded, "见 [架构决策](<wiki://架构决策>) 的下一节。");
        // 无链接时原样返回(不产生无谓拷贝路径上的差异)
        assert_eq!(expand_wikilinks("没有链接"), "没有链接");
        // 展开结果真能被解析成链接 token(否则拦截无从谈起)
        let doc = parse(&expanded);
        assert!(doc
            .tokens
            .iter()
            .any(|token| matches!(token, Token::Link { .. })));
    }

    /// 块划分的自检:区间首尾相接、并集等于整篇 —— Live Preview 下光标块
    /// 是被编辑的区间,任何落在块外的字节都会在敲键时静默丢失。
    fn assert_covers_text(text: &str, blocks: &[Range<usize>]) {
        let mut cursor = 0;
        for block in blocks {
            assert_eq!(block.start, cursor, "块区间不连续: {blocks:?}");
            assert!(block.end > block.start, "空块区间: {block:?}");
            cursor = block.end;
        }
        assert_eq!(cursor, text.len(), "未覆盖到文末: {blocks:?}");
    }

    /// 段落按空行切分,块间空行归前一块(每块可独立解析)。
    #[test]
    fn blocks_split_paragraphs_and_own_trailing_blank_line() {
        let text = "# 标题\n\n正文一段\n\n正文二段\n";
        let blocks = blocks(text);
        let slices: Vec<&str> = blocks.iter().map(|b| &text[b.start..b.end]).collect();
        assert_eq!(slices, vec!["# 标题\n\n", "正文一段\n\n", "正文二段\n"]);
        assert_covers_text(text, &blocks);
    }

    /// 代码块整块成一段:**含闭合 fence**(vendored 的 span 只到内容末尾,
    /// 直接拿 span 当边界会把 ``` 切到下一块)。
    #[test]
    fn blocks_keep_fenced_code_block_intact() {
        let text = "前言\n\n```rust\nfn main() {}\n```\n\n后记\n";
        let blocks = blocks(text);
        let slices: Vec<&str> = blocks.iter().map(|b| &text[b.start..b.end]).collect();
        assert_eq!(
            slices,
            vec!["前言\n\n", "```rust\nfn main() {}\n```\n\n", "后记\n"]
        );
        assert_covers_text(text, &blocks);
    }

    /// 表格与列表:表格整块;列表**每项**一块(每项都可单独进 Live 编辑)。
    #[test]
    fn blocks_cover_tables_and_list_items() {
        let text = "| a | b |\n|---|---|\n| 1 | 2 |\n\n尾段\n";
        let table_blocks = blocks(text);
        assert_eq!(table_blocks.len(), 2, "{table_blocks:?}");
        assert!(text[table_blocks[0].start..table_blocks[0].end].starts_with("| a |"));
        assert_covers_text(text, &table_blocks);

        let list = "- 一\n- 二\n\n尾\n";
        let list_blocks = blocks(list);
        let slices: Vec<&str> = list_blocks.iter().map(|b| &list[b.start..b.end]).collect();
        assert_eq!(slices, vec!["- 一\n", "- 二\n\n", "尾\n"]);
        assert_covers_text(list, &list_blocks);
    }

    /// 边界:空文档无块;纯空行整篇一块(编辑需要落点);单段无换行也成块。
    #[test]
    fn blocks_handle_empty_and_degenerate_input() {
        assert!(blocks("").is_empty());
        let blank = "\n\n\n";
        assert_eq!(blocks(blank), vec![0..blank.len()]);
        let single = "hello world";
        assert_eq!(blocks(single), vec![0..single.len()]);
    }

    /// CRLF 与中文:块的字节边界仍连续覆盖(CRLF 的 `\r` 属于行内容)。
    #[test]
    fn blocks_cover_crlf_and_cjk_text() {
        let text = "中文标题\r\n\r\n正文内容\r\n";
        let blocks = blocks(text);
        assert_covers_text(text, &blocks);
        assert!(!blocks.is_empty());
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

    /// 节定位(AI 摘要旧节清理的依据,边界行为经探针实测):起点吸收标题
    /// 前的空行,终点到下一同级/更浅标题前 —— 删除区间后,正文与下一个
    /// 标题之间仍留有换行,标题保持合法。
    #[test]
    fn heading_section_span_bounds_middle_section() {
        let src = "# 甲\n\n正文甲。\n\n## AI 摘要\n\n> - 要点\n\n## 乙\n\n正文乙。\n";
        let span = heading_section_span(src, 2, "AI 摘要").expect("定位到摘要节");
        assert_eq!(&src[span.clone()], "\n\n## AI 摘要\n\n> - 要点\n");
        let remainder = format!("{}{}", &src[..span.start], &src[span.end..]);
        assert_eq!(remainder, "# 甲\n\n正文甲。\n## 乙\n\n正文乙。\n");
        // 删除后的文本里旧节彻底消失,其余标题原样
        assert!(!remainder.contains("AI 摘要"));
        assert_eq!(outline(&remainder).len(), 2);
    }

    /// 更深层级的标题(`###`)是节的子内容,一并属于节;节在文末时区间
    /// 到文末,起点同样吸收前导空行。
    #[test]
    fn heading_section_span_swallows_subheadings_and_tail() {
        // ### 子节归入 ## AI 摘要节
        let src = "# 甲\n\n## AI 摘要\n\n### 子节\n\n内容\n\n## 乙\n\n正文乙\n";
        let span = heading_section_span(src, 2, "AI 摘要").expect("定位到摘要节");
        assert_eq!(&src[span.clone()], "\n\n## AI 摘要\n\n### 子节\n\n内容");

        // 文末节:删完只剩前文,正文结尾不带多余换行
        let src = "正文\n\n## AI 摘要\n\n> - 只有一条要点";
        let span = heading_section_span(src, 2, "AI 摘要").expect("定位到摘要节");
        assert_eq!(&src[span.clone()], "\n\n## AI 摘要\n\n> - 只有一条要点");
        assert_eq!(&src[..span.start], "正文");
    }

    /// 没有目标标题、标题文本不精确匹配或层级不符时返回 `None`
    /// (调用方据此跳过移除,直接追加新节)。
    #[test]
    fn heading_section_span_absent_or_mismatched_is_none() {
        assert_eq!(heading_section_span("# 甲\n\n正文\n", 2, "AI 摘要"), None);
        // 文本变体不算同一节:保守匹配,避免误删用户手写内容
        let src = "# 甲\n\n## AI 摘要(旧)\n\n> - 要点\n";
        assert_eq!(heading_section_span(src, 2, "AI 摘要"), None);
        // 层级不同不算:用户改写成 # / ### 后的旧节不在此口径内
        let src = "# AI 摘要\n\n> - 要点\n";
        assert_eq!(heading_section_span(src, 2, "AI 摘要"), None);
        assert!(heading_section_span(src, 1, "AI 摘要").is_some());
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
