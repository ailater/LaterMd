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

/// 活动块**内联标记**字符的字节区间(LP2-1 半隐藏):升序、互不重叠、
/// 相邻段已合并。
///
/// 「标记」= 渲染后不产生正文的源码字符:强调/粗体/删除线的 `*`/`_`/`~~`
/// 定界符、行内代码的反引号、链接/图片的 `[`、`](目标 "标题")`、`!`、
/// autolink 的 `<`/`>`。判定完全跟随 pulldown-cmark(与 vendored 解析器
/// 同一套 options,铁律「单一解析器」):未闭合的 `**` 是普通文本不算
/// 标记;围栏/缩进代码块的内容是 `Event::Text`,不算标记;嵌套强调每层
/// 定界符都计入(`***x***` 合并出前后各一段 `***`)。块级标记(`#`、
/// `>`、列表符、表格竖线)不在本口径内 —— LP2-1 只做内联。
///
/// 输入应是**单个块**的源码([`blocks`] 保证块可独立解析);返回区间是
/// 块内偏移。pulldown 的 Start/End 事件区间覆盖整个构造(0.13 实测),
/// 所以标记 = 「构造区间 − 内部内容区间」的缝隙。
pub fn inline_marks(text: &str) -> Vec<Range<usize>> {
    inline_marks_with_pairs(text).segments
}

/// 一对成对的内联标记(LP2-2 选区扩展):一个内联构造两侧的标记,连同
/// 整个构造的范围。嵌套构造(`***x***`、链接文本里的强调)每层各成一
/// 对,区间彼此重叠是正常的 —— 它们本来就不是同一对。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkPair {
    /// 完整构造(开标记 + 内容 + 闭标记),选区扩展的目标区间。
    pub construct: Range<usize>,
    /// 开侧标记,如 `**`;链接的开侧是 `[`。
    pub opening: Range<usize>,
    /// 闭侧标记;链接的闭侧是 `](目标 "标题")` 整段。
    pub closing: Range<usize>,
}

/// [`inline_marks`] 的全量产出:半隐藏用的合并标记段(LP2-1)+ 选区
/// 扩展用的成对表(LP2-2)。两份表出自**同一次**解析,缓存侧按修订号
/// 一起换新,不会出现「段与配对来自不同版本文本」的错位。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineMarks {
    /// 合并后的标记段(升序、互不重叠、相邻已合并)—— 半隐藏绘制的粒度。
    pub segments: Vec<Range<usize>>,
    /// 成对标记表 —— 配对显形与选区扩展的依据。
    pub pairs: Vec<MarkPair>,
}

/// [`inline_marks`] 的成对扩展(LP2-2):同一次 pulldown 走查,除合并
/// 标记段外,把每个定界构造的开/闭缝隙记成一对([`MarkPair`])。
///
/// 单侧孤标记(未闭合的 `**`、`` ` ``、`[`)在 pulldown 语义里是普通文
/// 本,既不产段也不产对 —— 选区扩展对它们天然退化为「不扩展」。
pub fn inline_marks_with_pairs(text: &str) -> InlineMarks {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    // 与 vendored parser.rs 同一套 options:预览把 `~~` 渲染成删除线、把
    // `[x](y)` 渲染成链接,半隐藏的判定必须与渲染同一语义,否则会出现
    // 「遮了不渲染的字符 / 漏了渲染的定界符」
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);

    // 开着的定界构造:起点 + 内部已覆盖(内容)区间。闭合时把缝隙记为标记。
    let mut open: Vec<(usize, Vec<Range<usize>>)> = Vec::new();
    let mut marks = Vec::new();
    let mut pairs = Vec::new();

    for (event, span) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(
                Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. },
            ) => {
                // 本构造区间先计入外层(嵌套强调之于链接文本),再开自己的栈帧
                if let Some((_, covered)) = open.last_mut() {
                    covered.push(span.clone());
                }
                open.push((span.start, Vec::new()));
            }
            Event::End(
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image,
            ) => {
                let Some((start, covered)) = open.pop() else {
                    continue; // 解析器事件配平,这里只是防御
                };
                let end = span.end;
                let mut covered = covered;
                covered.sort_by_key(|range| range.start);
                let mut gaps = Vec::new();
                let mut cursor = start;
                for inner in covered {
                    let inner = inner.start.max(start)..inner.end.min(end);
                    if inner.start > cursor {
                        gaps.push(cursor..inner.start);
                    }
                    cursor = cursor.max(inner.end);
                }
                if cursor < end {
                    gaps.push(cursor..end);
                }
                // 首末缝隙即开/闭两侧标记(它们天然贴着构造两端);只剩一个
                // 缝隙的退化构造(如空文本链接)拆不出两侧,不成对
                if gaps.len() >= 2 {
                    let closing = gaps.last().expect("len >= 2").clone();
                    pairs.push(MarkPair {
                        construct: start..end,
                        opening: gaps[0].clone(),
                        closing,
                    });
                }
                marks.extend(gaps);
                // 本构造(含它自己的标记)对外层是内容
                if let Some((_, parent)) = open.last_mut() {
                    parent.push(start..end);
                }
            }
            // 行内代码是单事件(区间含反引号与内容):两端的反引号串是标记
            Event::Code(_) => {
                let bytes = text.as_bytes();
                let (start, end) = (span.start, span.end);
                let mut ticks_start = start;
                while ticks_start < end && bytes[ticks_start] == b'`' {
                    ticks_start += 1;
                }
                let mut ticks_end = end;
                while ticks_end > ticks_start && bytes[ticks_end - 1] == b'`' {
                    ticks_end -= 1;
                }
                if start < ticks_start {
                    marks.push(start..ticks_start);
                }
                if ticks_end < end {
                    marks.push(ticks_end..end);
                }
                if start < ticks_start && ticks_end < end {
                    pairs.push(MarkPair {
                        construct: start..end,
                        opening: start..ticks_start,
                        closing: ticks_end..end,
                    });
                }
                if let Some((_, covered)) = open.last_mut() {
                    covered.push(span);
                }
            }
            // 其余事件(Text/SoftBreak/块级 Start/End…):开着的构造把它们
            // 视作内容;块级事件不会出现在内联构造内部,压栈无副作用
            _ => {
                if let Some((_, covered)) = open.last_mut() {
                    covered.push(span);
                }
            }
        }
    }

    // 排序 + 合并相邻/重叠段:`***` 会得到外层与内层各一片标记
    marks.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in marks {
        if range.is_empty() {
            continue;
        }
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    InlineMarks {
        segments: merged,
        pairs,
    }
}

/// 触发选区扩展的指针手势(LP2-2)。扩展是**一次性**的:只在事件帧传入
/// [`mark_interaction`],静止帧传 `None` —— 选区不会被持续吸附到标记对。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkGesture {
    /// 双击:egui 已在帧内选出词,当前选区即词区间;扩展到包含它的
    /// **最内层**标记对(嵌套时选最小的一对)。
    DoubleClick,
    /// 拖选松手:`anchor` 是按下时光标所在的块内字符偏移。选区整个含在
    /// 标记对内、且锚点压着某一侧标记(含边界,单字符标记只有边界可压)
    /// 时,扩展到该对。
    DragRelease {
        /// 按下帧的塌缩光标(块内字符偏移)。
        anchor: usize,
    },
}

/// 标记与光标/选区的交互决策(LP2-2)。
///
/// - `revealed`:与 `InlineMarks::segments` 平行的显形表。除 LP2-1 的
///   「贴上/相交显形」外,被触碰标记的**配对另一侧**所在段一并显形
///   (识别配对);
/// - `expanded`:扩展后的选区(块内**字符偏移**,TextEdit 的域),无扩
///   展时为 `None`。选区已含完整标记对时结果是同一区间(幂等)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkInteraction {
    /// 与 `segments` 平行:`true` = 该段显形。
    pub revealed: Vec<bool>,
    /// 扩展后的选区(字符偏移);`None` = 本帧不扩展。
    pub expanded: Option<Range<usize>>,
}

/// 计算标记的显形集合与选区扩展(LP2-2 的纯函数核心)。
///
/// 输入的 `caret`/`other`/`anchor` 是块内**字符偏移**(TextEdit 的光标
/// 域),`marks` 是 [`inline_marks_with_pairs`] 的产出(字节域),函数内
/// 部完成两域换算 —— 调用方不需要自己折算 CJK 偏移。
pub fn mark_interaction(
    text: &str,
    marks: &InlineMarks,
    caret: usize,
    other: usize,
    gesture: Option<MarkGesture>,
) -> MarkInteraction {
    let caret_byte = char_to_byte(text, caret);
    let other_byte = char_to_byte(text, other);
    let collapsed = caret_byte == other_byte;
    // 「触碰」口径与 LP2-1 显形一致:塌缩光标紧邻也算(点击落在标记字形
    // 上时光标停在其边界),非空选区取开区间相交。
    let touches = |range: &Range<usize>| {
        if collapsed {
            range.start <= caret_byte && caret_byte <= range.end
        } else {
            let (low, high) = (caret_byte.min(other_byte), caret_byte.max(other_byte));
            range.start < high && low < range.end
        }
    };
    let within = |outer: &Range<usize>, inner: &Range<usize>| {
        outer.start <= inner.start && inner.end <= outer.end
    };
    let mut revealed = Vec::with_capacity(marks.segments.len());
    for segment in &marks.segments {
        let partner = marks.pairs.iter().any(|pair| {
            (touches(&pair.opening) || touches(&pair.closing))
                && (within(segment, &pair.opening) || within(segment, &pair.closing))
        });
        revealed.push(touches(segment) || partner);
    }

    // 选区扩展:只在手势帧;候选对必须把当前选区整个含住(跨对选区不吸
    // 附),拖选还要求锚点压着标记。取最内层(构造最短)的一对。
    let expanded = gesture.and_then(|gesture| {
        let selection = caret_byte.min(other_byte)..caret_byte.max(other_byte);
        let mut best: Option<&MarkPair> = None;
        for pair in &marks.pairs {
            if !within(&pair.construct, &selection) {
                continue;
            }
            let anchored = match gesture {
                MarkGesture::DoubleClick => true,
                MarkGesture::DragRelease { anchor } => {
                    let anchor = char_to_byte(text, anchor);
                    (pair.opening.start <= anchor && anchor <= pair.opening.end)
                        || (pair.closing.start <= anchor && anchor <= pair.closing.end)
                }
            };
            if !anchored {
                continue;
            }
            let better = best.is_none_or(|current| {
                pair.construct.len() < current.construct.len()
                    || (pair.construct.len() == current.construct.len()
                        && pair.opening.len() + pair.closing.len()
                            > current.opening.len() + current.closing.len())
            });
            if better {
                best = Some(pair);
            }
        }
        best.map(|pair| {
            byte_to_char(text, pair.construct.start)..byte_to_char(text, pair.construct.end)
        })
    });
    MarkInteraction { revealed, expanded }
}

/// 块内字符偏移 → 字节偏移(越界钳到文末)。
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// 块内字节偏移 → 字符偏移(按「在该字节之前开始的字符数」计)。
fn byte_to_char(text: &str, byte_index: usize) -> usize {
    text.char_indices()
        .take_while(|(byte, _)| *byte < byte_index)
        .count()
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

    // —— inline_marks(LP2-1 活动块内联标记半隐藏)——

    /// 期望表断言:(字节起点, 字面文本) 双重核对 —— 区间序列与切片内容都
    /// 必须对上,CJK 多字节的字节数写错时切片对照当场失败,不靠手算。
    fn assert_marks(text: &str, marks: &[Range<usize>], expected: &[(usize, &str)]) {
        let labeled: Vec<Range<usize>> = expected
            .iter()
            .map(|(start, label)| *start..*start + label.len())
            .collect();
        assert_eq!(marks, labeled, "期望字面 {expected:?}");
        for (range, (_, label)) in marks.iter().zip(expected) {
            assert_eq!(&text[range.clone()], *label);
        }
    }

    /// 输出的形状永远合法:升序、互不重叠、非空、不越界。
    fn assert_well_formed(text: &str, marks: &[Range<usize>]) {
        let mut prev_end = 0;
        for range in marks {
            assert!(!range.is_empty(), "空区间: {marks:?}");
            assert!(range.start >= prev_end, "重叠/乱序: {marks:?}");
            assert!(range.end <= text.len(), "越界: {marks:?}");
            prev_end = range.end;
        }
    }

    /// 四种强调定界符 + 删除线:开/闭定界符各自成段,内容不标。
    #[test]
    fn inline_marks_covers_emphasis_delimiters() {
        let text = "**b** *i* _u_ ~~s~~";
        let marks = inline_marks(text);
        assert_marks(
            text,
            &marks,
            &[
                (0, "**"),
                (3, "**"),
                (6, "*"),
                (8, "*"),
                (10, "_"),
                (12, "_"),
                (14, "~~"),
                (17, "~~"),
            ],
        );
        assert_well_formed(text, &marks);
    }

    /// 嵌套强调(`***x***`):内外层定界符合并成前后各一整段 `***`。
    #[test]
    fn inline_marks_merges_nested_emphasis() {
        let text = "***both***";
        assert_marks(text, &inline_marks(text), &[(0, "***"), (7, "***")]);

        // 强调里套行内代码:代码反引号与强调定界符都标,内容不标
        let text = "*a `b` c*";
        assert_marks(
            text,
            &inline_marks(text),
            &[(0, "*"), (3, "`"), (5, "`"), (8, "*")],
        );
    }

    /// 行内代码:只标两端反引号串;双反引号包裹(内容本身含反引号)不误伤。
    #[test]
    fn inline_marks_tags_code_backticks_only() {
        let text = "`code` 与 ``a ` b``";
        let marks = inline_marks(text);
        assert_marks(text, &marks, &[(0, "`"), (5, "`"), (11, "``"), (18, "``")]);
        // 内容不算标记
        assert!(!marks.iter().any(|r| &text[r.clone()] == "code"));
        assert!(!marks.iter().any(|r| text[r.clone()].contains('a')));
    }

    /// 链接与图片:括号与目标部分是标记,链接文本不标;autolink 只标尖括号。
    #[test]
    fn inline_marks_tags_link_brackets_and_destination() {
        let text = "[点我](https://e.com)";
        let marks = inline_marks(text);
        assert_marks(text, &marks, &[(0, "["), (7, "](https://e.com)")]);

        let text = "![alt](img.png)";
        assert_marks(text, &inline_marks(text), &[(0, "!["), (5, "](img.png)")]);

        let text = "<https://auto.link>";
        assert_marks(text, &inline_marks(text), &[(0, "<"), (18, ">")]);

        // 链接文本里套粗体:粗体定界符与链接括号都标,且合并不重复
        let text = "[**b**](u)";
        assert_marks(text, &inline_marks(text), &[(0, "[**"), (4, "**](u)")]);
    }

    /// CJK 混排与 CRLF:字节区间落在字符边界上,判定不受多字节字符影响。
    #[test]
    fn inline_marks_handles_cjk_and_crlf() {
        let text = "**中文**与*混排*和[链接](u)";
        let marks = inline_marks(text);
        assert_marks(
            text,
            &marks,
            &[
                (0, "**"),
                (8, "**"),
                (13, "*"),
                (20, "*"),
                (24, "["),
                (31, "](u)"),
            ],
        );
        assert_well_formed(text, &marks);
        for range in &marks {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }

        assert_marks(
            "**b**\r\n尾行",
            &inline_marks("**b**\r\n尾行"),
            &[(0, "**"), (3, "**")],
        );
    }

    /// 围栏/缩进代码块内容不算标记(里面的 `*`、`[` 是代码,遮了就改坏语义)。
    #[test]
    fn inline_marks_skips_code_block_contents() {
        let text = "```rust\na * b [x](y) **z**\n```\n";
        assert!(inline_marks(text).is_empty());

        let text = "    indented *x* code\n";
        assert!(inline_marks(text).is_empty());

        // 块外的散置星号是普通文本(CommonMark:两侧无紧贴内容则不构成强调)
        assert!(inline_marks("a * b").is_empty());
    }

    /// 未闭合的定界符是普通文本(pulldown 语义),不算标记。
    #[test]
    fn inline_marks_skips_unclosed_markers() {
        assert!(inline_marks("未闭合 **bold").is_empty());
        assert!(inline_marks("`unclosed code").is_empty());
        assert!(inline_marks("[unclosed link").is_empty());
    }

    /// 列表/引用块内的行内标记照标(块级标记本身 `- `、`> ` 不标)。
    #[test]
    fn inline_marks_inside_list_and_quote_blocks() {
        let text = "- 项目 **粗**\n";
        let marks = inline_marks(text);
        assert_marks(text, &marks, &[(9, "**"), (14, "**")]);
        assert!(marks[0].start > 2, "列表符 `- ` 不是标记: {marks:?}");

        let text = "> 引用 *i*\n";
        let marks = inline_marks(text);
        assert_marks(text, &marks, &[(9, "*"), (11, "*")]);
        assert!(marks[0].start > 2, "引用符 `> ` 不是标记: {marks:?}");
    }

    /// 空输入与纯文本:无标记。
    #[test]
    fn inline_marks_empty_and_plain_text() {
        assert!(inline_marks("").is_empty());
        assert!(inline_marks("普通段落,没有标记。").is_empty());
    }

    // —— inline_mark_pairs / mark_interaction(LP2-2 选区扩展与配对显形)——

    /// 期望表断言配对:(构造字面, 开标记字面, 闭标记字面) 三重核对。
    fn assert_pairs(text: &str, pairs: &[MarkPair], expected: &[(&str, &str, &str)]) {
        assert_eq!(
            pairs.len(),
            expected.len(),
            "对数不符:{pairs:?} vs {expected:?}"
        );
        for (pair, (whole, opening, closing)) in pairs.iter().zip(expected) {
            assert_eq!(&text[pair.construct.clone()], *whole);
            assert_eq!(&text[pair.opening.clone()], *opening);
            assert_eq!(&text[pair.closing.clone()], *closing);
        }
    }

    /// 成对匹配:强调/行内代码/链接/图片/autolink 各自成对,开闭两侧
    /// 字面与构造整段都对得上。
    #[test]
    fn mark_pairs_match_constructs() {
        let text = "**b** `c` [t](u) ![a](i.png) <https://x.y>";
        let marks = inline_marks_with_pairs(text);
        assert_pairs(
            text,
            &marks.pairs,
            &[
                ("**b**", "**", "**"),
                ("`c`", "`", "`"),
                ("[t](u)", "[", "](u)"),
                ("![a](i.png)", "![", "](i.png)"),
                ("<https://x.y>", "<", ">"),
            ],
        );
        // 段表与 LP2-1 完全一致(重构不得改半隐藏行为)
        assert_eq!(marks.segments, inline_marks(text));
    }

    /// 嵌套:`***x***` 内外层各成一对(pulldown 给内层 Strong 与外层
    /// Emphasis 不同的构造区间);链接文本里套强调,两对各自完整。
    #[test]
    fn mark_pairs_cover_nested_constructs_per_level() {
        let text = "***x***";
        let marks = inline_marks_with_pairs(text);
        assert_pairs(
            text,
            &marks.pairs,
            &[("**x**", "**", "**"), ("***x***", "*", "*")],
        );

        let text = "[**b**](u)";
        let marks = inline_marks_with_pairs(text);
        assert_pairs(
            text,
            &marks.pairs,
            &[("**b**", "**", "**"), ("[**b**](u)", "[", "](u)")],
        );
    }

    /// 单侧孤标记(未闭合)的退化:pulldown 视作普通文本,无段也无对,
    /// 显形/扩展随之整体退化为「什么都不发生」。
    #[test]
    fn mark_pairs_skip_single_sided_orphans() {
        for text in ["未闭合 **bold", "`半截代码", "[没目标"] {
            let marks = inline_marks_with_pairs(text);
            assert!(marks.segments.is_empty(), "{text:?}");
            assert!(marks.pairs.is_empty(), "{text:?}");
        }
        // 空文本链接拆不出两侧,不成对(段仍在,半隐藏不受影响)
        let marks = inline_marks_with_pairs("[](u)");
        assert!(marks.pairs.is_empty());
        assert_eq!(marks.segments.len(), 1, "{:?}", marks.segments);
    }

    /// 配对显形:光标/选区压住一侧标记,另一侧所在段同时显形 —— 这是
    /// LP2-1 规则(只显形被压住的段)之上的增量。
    #[test]
    fn mark_interaction_reveals_partner_side() {
        let text = "**b** x *i*";
        let marks = inline_marks_with_pairs(text);

        // 塌缩光标压住开 `**`(字节 0..2,取边界 1):开闭两段都显形,
        // 无关的 `*i*` 两段保持半隐藏
        let out = mark_interaction(text, &marks, 1, 1, None);
        assert_eq!(out.revealed, vec![true, true, false, false]);
        assert_eq!(out.expanded, None, "无手势不扩展");

        // 非空选区只盖住开 `**`(字符 0..2):同样两段显形
        let out = mark_interaction(text, &marks, 0, 2, None);
        assert_eq!(out.revealed, vec![true, true, false, false]);

        // 选区只盖内容 `b`(字符 2..3):不贴标记,全隐藏(LP2-1 口径保持)
        let out = mark_interaction(text, &marks, 2, 3, None);
        assert_eq!(out.revealed, vec![false, false, false, false]);
    }

    /// 双击扩展:词选区扩到包含它的**最内层**标记对;跨对/无对不扩展。
    #[test]
    fn mark_interaction_expands_double_click_to_innermost_pair() {
        let text = "[**b**](u) 和 `c`";
        let marks = inline_marks_with_pairs(text);

        // 双击词 `b`(字符 3..4)→ 最内层是粗体对,不是外层链接对
        let out = mark_interaction(text, &marks, 3, 4, Some(MarkGesture::DoubleClick));
        assert_eq!(out.expanded, Some(1..6), "扩到 **b**,不带链接括号");

        // 双击链接外的词 `和`(字符 11..12):无对包含,不扩展
        let out = mark_interaction(text, &marks, 11, 12, Some(MarkGesture::DoubleClick));
        assert_eq!(out.expanded, None);

        // 跨对选区(选了链接一半+一半正文)不吸附
        let text2 = "**ab** cd *ef*";
        let marks2 = inline_marks_with_pairs(text2);
        let out = mark_interaction(text2, &marks2, 0, 9, Some(MarkGesture::DoubleClick));
        assert_eq!(out.expanded, None);
    }

    /// 拖选扩展:锚点压着标记(含边界,单字符标记只有边界可压)且选区
    /// 整个含在构造内才扩展;从内容中部发起、或拖出构造外,保持原选区。
    #[test]
    fn mark_interaction_expands_drag_only_from_mark_anchor() {
        let text = "前缀 **abcd** 后缀";
        let marks = inline_marks_with_pairs(text);
        // 字符:`前缀 `(3) + `**`(2) + `abcd`(4) + `**`(2) + ...
        // 开 `**` = 字符 3..5,内容 5..9,构造 3..11

        // 锚点在开 `**` 边界(字符 5 = opening.end),拖到内容中部松手
        let out = mark_interaction(
            text,
            &marks,
            5,
            7,
            Some(MarkGesture::DragRelease { anchor: 5 }),
        );
        assert_eq!(out.expanded, Some(3..11), "从标记内侧发起 → 扩到整对");

        // 锚点在内容中部:普通拖选,不吸附
        let out = mark_interaction(
            text,
            &marks,
            6,
            8,
            Some(MarkGesture::DragRelease { anchor: 6 }),
        );
        assert_eq!(out.expanded, None);

        // 锚点压标记但拖出构造(选区含构造外的前缀):不吸附
        let out = mark_interaction(
            text,
            &marks,
            0,
            6,
            Some(MarkGesture::DragRelease { anchor: 5 }),
        );
        assert_eq!(out.expanded, None);
    }

    /// 选区已含标记:显形两段全亮;扩展幂等(结果就是当前区间)。
    #[test]
    fn mark_interaction_idempotent_when_selection_already_covers_pair() {
        let text = "**b** x";
        let marks = inline_marks_with_pairs(text);
        let out = mark_interaction(text, &marks, 0, 5, Some(MarkGesture::DoubleClick));
        assert_eq!(out.expanded, Some(0..5));
        assert_eq!(out.revealed, vec![true, true]);
    }

    /// CJK:字符偏移输入与字节区间标记在函数内部正确换算。
    #[test]
    fn mark_interaction_converts_cjk_char_and_byte_domains() {
        let text = "**中文** 尾";
        // 字符:0-1 `**`、2-3 中文、4-5 `**`;字节:开 0..2、闭 8..10
        let marks = inline_marks_with_pairs(text);
        assert_pairs(text, &marks.pairs, &[("**中文**", "**", "**")]);

        // 光标停在闭 `**` 首边界(字符 4 = 字节 8):开闭两段都显形
        let out = mark_interaction(text, &marks, 4, 4, None);
        assert_eq!(out.revealed, vec![true, true]);

        // 双击词「中文」(字符 2..4)→ 扩到字符 0..6
        let out = mark_interaction(text, &marks, 2, 4, Some(MarkGesture::DoubleClick));
        assert_eq!(out.expanded, Some(0..6));
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
