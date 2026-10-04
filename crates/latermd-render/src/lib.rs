#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! 绘制指令 IR(Intermediate Representation)—— 只描述「画什么」,不描述「怎么画」。
//!
//! 由 pulldown-cmark 事件流生成(单一解析器铁律,AGENTS.md §3):块级结构
//! (标题层级/段落/列表/代码块/引用/表格/分隔线/图片占位)与行内样式
//! (粗体/斜体/行内代码/链接/删除线)。不携带坐标、字体或度量——把 IR
//! 翻译为具体输出是后端的职责(铁律 2:本 crate 不依赖 egui 或任何 UI
//! 框架;PDF 导出 `latermd_export::pdf` 是它除预览外的第二个消费者,
//! 即 roadmap「出现第二个消费者时抽 latermd-render」的触发场景)。
//!
//! 有意不进 IR 的东西:HTML 块/内联 HTML 事件被丢弃(PDF 无法承载原始
//! HTML,方言一致性由解析开关保证,不由 HTML 渲染保证);脚注定义单独
//! 收在 [`Document::footnotes`],由后端决定排成尾注还是页注。

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// 扩展开关,与预览(vendored `egui_markdown::parser::parse`)逐项一致。
///
/// 本函数是全 workspace 唯一的开关持有点,`latermd-export` 的 HTML 与
/// PDF 两条链路都从这里取值——预览/HTML/PDF 三方必须同方言,否则同一篇
/// 文档在三处看到三种结果。上游改动方言时只改这里,单测钉住四个扩展。
pub fn parser_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options
}

/// 解析 Markdown 源文本为拥有型绘制指令 IR。
pub fn parse(text: &str) -> Document {
    let mut state = ParseState::default();
    for event in Parser::new_ext(text, parser_options()) {
        state.event(event);
    }
    state.finish()
}

/// 一份文档的 IR:正文块序列 + 脚注定义(按出现顺序)。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    /// 顶层块序列。
    pub blocks: Vec<Block>,
    /// 脚注定义(`[^label]: 内容`),HTML 导出渲染为文末列表,PDF 排为尾注。
    pub footnotes: Vec<FootnoteDef>,
}

/// 一条脚注定义。
#[derive(Debug, Clone, PartialEq)]
pub struct FootnoteDef {
    /// 脚注标签(方括号内的原文,如 `1`)。
    pub label: String,
    /// 定义体,通常是一个段落。
    pub blocks: Vec<Block>,
}

/// 块级绘制指令。
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// 标题,`level` 为 1–6。
    Heading {
        /// 标题层级(1–6)。
        level: u8,
        /// 标题内的行内内容。
        inlines: Vec<Inline>,
    },
    /// 段落。
    Paragraph {
        /// 段落内的行内内容。
        inlines: Vec<Inline>,
    },
    /// 列表;`start` 为 `Some` 时是有序列表(值为首项序号)。
    List {
        /// `None` = 无序列表;`Some(n)` = 有序列表,首项序号 n。
        start: Option<u64>,
        /// 列表项,按出现顺序。
        items: Vec<ListItem>,
    },
    /// 代码块,`code` 为原始代码文本(含换行)。
    CodeBlock {
        /// 围栏 info 首词(如 ```` ```rust ```` 的 `rust`);缩进代码块为 `None`。
        language: Option<String>,
        /// 原始代码文本。
        code: String,
    },
    /// 引用块,内部是完整块序列(可嵌套)。
    Quote {
        /// 引用内的块。
        blocks: Vec<Block>,
    },
    /// GFM 表格。
    Table(Table),
    /// 分隔线(`---` / `***` / `___`)。
    ThematicBreak,
    /// 图片占位:整段只含一张图片时提升为块级占位,后端画占位框。
    ImagePlaceholder {
        /// 图片 URL 原文。
        url: String,
        /// 替代文本(图片嵌套内容的纯文本)。
        alt: String,
        /// 标题(`![a](u "t")` 的 t)。
        title: Option<String>,
    },
}

/// 一个列表项。
#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    /// 任务列表勾选态:`Some(true)` 已勾选、`Some(false)` 未勾选、`None` 非任务项。
    pub task: Option<bool>,
    /// 项内容,通常是一个段落。
    pub blocks: Vec<Block>,
}

/// GFM 表格。
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    /// 每列的对齐方式,与表头列数一致。
    pub alignments: Vec<Alignment>,
    /// 表头行(每列的行内内容)。
    pub header: Vec<Cell>,
    /// 数据行。
    pub rows: Vec<Vec<Cell>>,
}

/// 表格单元格:行内内容序列(GFM 单元格不含块级结构)。
pub type Cell = Vec<Inline>;

/// 列对齐方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    /// 未指定(按左对齐渲染)。
    None,
    /// `:---` 左对齐。
    Left,
    /// `:---:` 居中。
    Center,
    /// `---:` 右对齐。
    Right,
}

/// 行内绘制指令。
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    /// 一段同样式文本。
    Run(Run),
    /// 行内图片(与文本混排时的占位;整段仅一张图片会提升为
    /// [`Block::ImagePlaceholder`])。
    Image {
        /// 图片 URL 原文。
        url: String,
        /// 替代文本。
        alt: String,
        /// 标题。
        title: Option<String>,
    },
    /// 硬换行(行尾两个空格或反斜杠)。
    LineBreak,
    /// 脚注引用 `[^label]`。
    FootnoteRef {
        /// 脚注标签。
        label: String,
    },
}

/// 同样式文本段。
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// 文本内容(未转义原文;软换行为一个空格)。
    pub text: String,
    /// 样式。
    pub style: Style,
}

/// 行内样式标记的可组合集合。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    /// 粗体。
    pub bold: bool,
    /// 斜体。
    pub italic: bool,
    /// 行内代码。
    pub code: bool,
    /// 删除线。
    pub strike: bool,
    /// 链接目标:外层链接的 URL 与标题。
    pub link: Option<Link>,
}

/// 链接目标。
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// 目标 URL 原文。
    pub dest: String,
    /// 标题(`[a](u "t")` 的 t)。
    pub title: Option<String>,
}

impl Style {
    /// 是否为无任何样式的普通文本(后端可走快路径)。
    pub fn is_plain(&self) -> bool {
        *self == Self::default()
    }
}

// ---- 事件流 → IR 的状态机 ----

/// 块容器帧:引用/列表/列表项/脚注定义/表格五种嵌套容器入栈出栈。
enum Frame {
    Quote(Vec<Block>),
    Footnote(String, Vec<Block>),
    List(ListBuilder),
    Item(ListItem),
    Table(TableFrame),
}

/// 列表构建器(`List` 的 `items` 字段在 Item 帧闭合时回填)。
struct ListBuilder {
    start: Option<u64>,
    items: Vec<ListItem>,
}

/// 行内上下文:段落/标题/单元格的直接内容,或图片的替代文本。
///
/// 紧列表(tight list)的项内容没有 `Paragraph` 标签,裸文本事件直接落在
/// 容器里——这种情况下惰性压入一个隐式段落上下文([`InlineCtx::implicit`]),
/// 在容器闭合或下一个块开始时收口成 [`Block::Paragraph`]。
struct InlineCtx {
    items: Vec<Inline>,
    styles: Vec<Style>,
    /// 该上下文的产出形态。
    kind: InlineKind,
    /// 是否为紧列表等场景的隐式段落。
    implicit: bool,
}

enum InlineKind {
    /// 闭合时生成段落或标题(`Paragraph` / `Heading(level)`)。
    Block(PendingBlock),
    /// 表格单元格,闭合时回填进表格帧的当前行。
    Cell,
    /// 图片替代文本,闭合时折叠为纯文本并入父上下文。
    ImageAlt { url: String, title: Option<String> },
}

enum PendingBlock {
    Paragraph,
    Heading(u8),
}

#[derive(Default)]
struct ParseState {
    doc: Document,
    frames: Vec<Frame>,
    inlines: Vec<InlineCtx>,
    code: Option<(Option<String>, String)>,
}

impl ParseState {
    fn event(&mut self, event: Event<'_>) {
        if let Some((_, buffer)) = self.code.as_mut() {
            // 代码块内只应有 Text(含换行);其余事件按丢弃处理。
            if let Event::Text(text) = &event {
                buffer.push_str(text);
            }
            if matches!(event, Event::End(TagEnd::CodeBlock)) {
                let (language, code) = self.code.take().expect("上面已确认在代码块内");
                self.close_implicit_paragraph();
                self.push_block(Block::CodeBlock { language, code });
            }
            return;
        }

        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.push_run(text.into_string(), self.active_style().clone()),
            Event::Code(text) => {
                let mut style = self.active_style().clone();
                style.code = true;
                self.push_run(text.into_string(), style);
            }
            Event::SoftBreak => self.push_run(" ".into(), self.active_style().clone()),
            Event::HardBreak => self.push_inline(Inline::LineBreak),
            Event::FootnoteReference(label) => {
                self.push_inline(Inline::FootnoteRef {
                    label: label.into_string(),
                });
            }
            Event::Rule => {
                self.close_implicit_paragraph();
                self.push_block(Block::ThematicBreak);
            }
            // PDF 链路不承载原始 HTML,丢弃(方言一致性由解析开关保证)。
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(item)) = self.frames.last_mut() {
                    item.task = Some(checked);
                }
            }
            // 方言未开启 MATH,这两个事件不会出现;保留原文进 IR 以防将来开启。
            Event::InlineMath(text) => {
                self.push_run(text.into_string(), self.active_style().clone())
            }
            Event::DisplayMath(text) => {
                self.close_implicit_paragraph();
                self.push_block(Block::CodeBlock {
                    language: Some("math".into()),
                    code: text.into_string(),
                });
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                self.drop_empty_implicit();
                self.inlines.push(InlineCtx {
                    items: Vec::new(),
                    styles: vec![Style::default()],
                    kind: InlineKind::Block(PendingBlock::Paragraph),
                    implicit: false,
                });
            }
            Tag::Heading { level, .. } => {
                self.drop_empty_implicit();
                self.inlines.push(InlineCtx {
                    items: Vec::new(),
                    styles: vec![Style::default()],
                    kind: InlineKind::Block(PendingBlock::Heading(heading_level(level))),
                    implicit: false,
                });
            }
            Tag::BlockQuote(_) => {
                self.close_implicit_paragraph();
                self.frames.push(Frame::Quote(Vec::new()));
            }
            Tag::List(start) => {
                self.close_implicit_paragraph();
                self.frames.push(Frame::List(ListBuilder {
                    start,
                    items: Vec::new(),
                }));
            }
            Tag::Item => self.frames.push(Frame::Item(ListItem {
                task: None,
                blocks: Vec::new(),
            })),
            Tag::FootnoteDefinition(label) => {
                self.frames
                    .push(Frame::Footnote(label.into_string(), Vec::new()));
            }
            Tag::CodeBlock(kind) => {
                self.close_implicit_paragraph();
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().map(str::to_owned)
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                self.code = Some((language, String::new()));
            }
            Tag::Table(alignments) => {
                self.close_implicit_paragraph();
                self.frames.push(Frame::Table(TableFrame {
                    table: Table {
                        alignments: alignments.iter().map(alignment_of).collect(),
                        header: Vec::new(),
                        rows: Vec::new(),
                    },
                    row: Vec::new(),
                }));
            }
            Tag::TableRow => {
                if let Some(Frame::Table(frame)) = self.frames.last_mut() {
                    frame.row = Vec::new();
                }
            }
            Tag::TableCell => self.inlines.push(InlineCtx {
                items: Vec::new(),
                styles: vec![Style::default()],
                kind: InlineKind::Cell,
                implicit: false,
            }),
            Tag::Emphasis => self.push_style(|style| style.italic = true),
            Tag::Strong => self.push_style(|style| style.bold = true),
            Tag::Strikethrough => self.push_style(|style| style.strike = true),
            Tag::Link {
                dest_url, title, ..
            } => self.push_style(|style| {
                style.link = Some(Link {
                    dest: dest_url.into_string(),
                    title: (!title.is_empty()).then(|| title.into_string()),
                })
            }),
            Tag::Image {
                dest_url, title, ..
            } => self.inlines.push(InlineCtx {
                items: Vec::new(),
                styles: vec![Style::default()],
                kind: InlineKind::ImageAlt {
                    url: dest_url.into_string(),
                    title: (!title.is_empty()).then(|| title.into_string()),
                },
                implicit: false,
            }),
            // 方言未启用的标签(定义列表/上下标等)不会出现;保险起见无操作。
            Tag::HtmlBlock | Tag::MetadataBlock(_) | Tag::TableHead => {}
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => {
                let Some(ctx) = self.inlines.pop() else {
                    return;
                };
                let InlineKind::Block(pending) = ctx.kind else {
                    return;
                };
                if let Some(block) = block_from_inlines(ctx.items, pending) {
                    self.push_block(block);
                }
            }
            TagEnd::BlockQuote(_) => {
                self.close_implicit_paragraph();
                if let Some(Frame::Quote(blocks)) = self.frames.pop() {
                    self.push_block(Block::Quote { blocks });
                }
            }
            TagEnd::List(_) => {
                if let Some(Frame::List(builder)) = self.frames.pop() {
                    self.push_block(Block::List {
                        start: builder.start,
                        items: builder.items,
                    });
                }
            }
            TagEnd::Item => {
                self.close_implicit_paragraph();
                if let Some(Frame::Item(item)) = self.frames.pop() {
                    if let Some(Frame::List(builder)) = self.frames.last_mut() {
                        builder.items.push(item);
                    }
                }
            }
            TagEnd::FootnoteDefinition => {
                self.close_implicit_paragraph();
                if let Some(Frame::Footnote(label, blocks)) = self.frames.pop() {
                    self.doc.footnotes.push(FootnoteDef { label, blocks });
                }
            }
            TagEnd::CodeBlock => {
                // 正常在 event() 的代码分支闭合;走到这里说明输入异常,忽略。
            }
            TagEnd::Table => {
                if let Some(Frame::Table(frame)) = self.frames.pop() {
                    self.push_block(Block::Table(frame.table));
                }
            }
            TagEnd::TableHead => {
                if let Some(Frame::Table(frame)) = self.frames.last_mut() {
                    frame.table.header = std::mem::take(&mut frame.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(Frame::Table(frame)) = self.frames.last_mut() {
                    let row = std::mem::take(&mut frame.row);
                    frame.table.rows.push(row);
                }
            }
            TagEnd::TableCell => {
                let Some(ctx) = self.inlines.pop() else {
                    return;
                };
                if let Some(Frame::Table(frame)) = self.frames.last_mut() {
                    frame.row.push(ctx.items);
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                if let Some(ctx) = self.inlines.last_mut() {
                    ctx.styles.pop();
                }
            }
            TagEnd::Image => {
                let Some(ctx) = self.inlines.pop() else {
                    return;
                };
                let InlineKind::ImageAlt { url, title } = ctx.kind else {
                    return;
                };
                let alt = ctx
                    .items
                    .iter()
                    .map(inline_plain_text)
                    .collect::<Vec<_>>()
                    .concat();
                self.push_inline(Inline::Image { url, alt, title });
            }
            TagEnd::HtmlBlock | TagEnd::MetadataBlock(_) => {}
            // 方言未启用的标签(定义列表/上下标等)不会出现,保险起见无操作。
            _ => {}
        }
    }

    fn finish(mut self) -> Document {
        // 输入残缺(未闭合容器)时把栈里已收好的内容照常收尾,不 panic。
        self.close_implicit_paragraph();
        while let Some(frame) = self.frames.pop() {
            match frame {
                Frame::Quote(blocks) => self.push_block(Block::Quote { blocks }),
                Frame::Footnote(label, blocks) => {
                    self.doc.footnotes.push(FootnoteDef { label, blocks })
                }
                Frame::List(builder) => self.push_block(Block::List {
                    start: builder.start,
                    items: builder.items,
                }),
                Frame::Item(item) => {
                    if let Some(Frame::List(builder)) = self.frames.last_mut() {
                        builder.items.push(item);
                    }
                }
                Frame::Table(frame) => self.push_block(Block::Table(frame.table)),
            }
        }
        if let Some(ctx) = self.inlines.pop() {
            if let InlineKind::Block(pending) = ctx.kind {
                if let Some(block) = block_from_inlines(ctx.items, pending) {
                    self.push_block(block);
                }
            }
        }
        self.doc
    }

    fn push_block(&mut self, block: Block) {
        match self.frames.last_mut() {
            Some(Frame::Quote(blocks) | Frame::Footnote(_, blocks)) => blocks.push(block),
            Some(Frame::Item(item)) => item.blocks.push(block),
            _ => self.doc.blocks.push(block),
        }
    }

    /// 裸文本落在容器里(紧列表项)时,惰性开启一个隐式段落上下文。
    fn ensure_inline_ctx(&mut self) {
        if self.inlines.is_empty() {
            self.inlines.push(InlineCtx {
                items: Vec::new(),
                styles: vec![Style::default()],
                kind: InlineKind::Block(PendingBlock::Paragraph),
                implicit: true,
            });
        }
    }

    /// 显式段落/标题开始前,丢弃仍为空的隐式段落(它只是占位)。
    fn drop_empty_implicit(&mut self) {
        if let Some(ctx) = self.inlines.last() {
            if ctx.implicit && ctx.items.is_empty() {
                self.inlines.pop();
            }
        }
    }

    /// 容器闭合或下一个块开始前,把隐式段落收口成 [`Block::Paragraph`]。
    fn close_implicit_paragraph(&mut self) {
        let close = matches!(
            self.inlines.last(),
            Some(ctx) if ctx.implicit
        );
        if !close {
            return;
        }
        let ctx = self.inlines.pop().expect("上面已确认存在");
        if let InlineKind::Block(PendingBlock::Paragraph) = ctx.kind {
            if let Some(block) = block_from_inlines(ctx.items, PendingBlock::Paragraph) {
                self.push_block(block);
            }
        }
    }

    fn push_inline(&mut self, inline: Inline) {
        self.ensure_inline_ctx();
        if let Some(ctx) = self.inlines.last_mut() {
            ctx.items.push(inline);
        }
    }

    fn push_run(&mut self, text: String, style: Style) {
        if !text.is_empty() {
            self.push_inline(Inline::Run(Run { text, style }));
        }
    }

    fn active_style(&self) -> &Style {
        self.inlines
            .last()
            .and_then(|ctx| ctx.styles.last())
            .unwrap_or(&DEFAULT_STYLE)
    }

    fn push_style(&mut self, apply: impl FnOnce(&mut Style)) {
        self.ensure_inline_ctx();
        if let Some(ctx) = self.inlines.last_mut() {
            let mut style = ctx.styles.last().cloned().unwrap_or_default();
            apply(&mut style);
            ctx.styles.push(style);
        }
    }
}

static DEFAULT_STYLE: Style = Style {
    bold: false,
    italic: false,
    code: false,
    strike: false,
    link: None,
};

/// 表格帧:当前行在 TableHead/TableRow 闭合时回填。
struct TableFrame {
    table: Table,
    row: Vec<Cell>,
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn alignment_of(alignment: &pulldown_cmark::Alignment) -> Alignment {
    match alignment {
        pulldown_cmark::Alignment::None => Alignment::None,
        pulldown_cmark::Alignment::Left => Alignment::Left,
        pulldown_cmark::Alignment::Center => Alignment::Center,
        pulldown_cmark::Alignment::Right => Alignment::Right,
    }
}

fn block_from_inlines(inlines: Vec<Inline>, pending: PendingBlock) -> Option<Block> {
    if inlines.is_empty() {
        return None;
    }
    match pending {
        PendingBlock::Paragraph => {
            // 整段只有一张图片:提升为块级图片占位。
            if let [Inline::Image { url, alt, title }] = inlines.as_slice() {
                return Some(Block::ImagePlaceholder {
                    url: url.clone(),
                    alt: alt.clone(),
                    title: title.clone(),
                });
            }
            Some(Block::Paragraph { inlines })
        }
        PendingBlock::Heading(level) => Some(Block::Heading { level, inlines }),
    }
}

fn inline_plain_text(inline: &Inline) -> String {
    match inline {
        Inline::Run(run) => run.text.clone(),
        Inline::Image { alt, .. } => alt.clone(),
        Inline::LineBreak => String::new(),
        Inline::FootnoteRef { label } => label.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_options_match_preview_dialect() {
        let options = parser_options();
        assert!(options.contains(Options::ENABLE_STRIKETHROUGH));
        assert!(options.contains(Options::ENABLE_TABLES));
        assert!(options.contains(Options::ENABLE_FOOTNOTES));
        assert!(options.contains(Options::ENABLE_TASKLISTS));
        assert!(!options.contains(Options::ENABLE_SMART_PUNCTUATION));
        assert!(!options.contains(Options::ENABLE_HEADING_ATTRIBUTES));
    }

    #[test]
    fn heading_and_inline_styles_roundtrip() {
        let doc = parse("# 标题 **粗** *斜* `码` [链](https://a.example \"题\") ~~删~~");
        let [Block::Heading { level, inlines }] = doc.blocks.as_slice() else {
            panic!("应为一个标题块,实为 {:?}", doc.blocks);
        };
        assert_eq!(*level, 1);
        assert!(inlines.len() >= 6, "样式段数: {:?}", inlines);
        let bold = inlines.iter().find_map(|i| match i {
            Inline::Run(r) if r.style.bold && !r.style.code => Some(r.text.clone()),
            _ => None,
        });
        assert_eq!(bold.as_deref(), Some("粗"));
        let italic = inlines.iter().find_map(|i| match i {
            Inline::Run(r) if r.style.italic => Some(r.text.clone()),
            _ => None,
        });
        assert_eq!(italic.as_deref(), Some("斜"));
        let code = inlines.iter().find_map(|i| match i {
            Inline::Run(r) if r.style.code => Some(r.text.clone()),
            _ => None,
        });
        assert_eq!(code.as_deref(), Some("码"));
        let strike = inlines.iter().find_map(|i| match i {
            Inline::Run(r) if r.style.strike => Some(r.text.clone()),
            _ => None,
        });
        assert_eq!(strike.as_deref(), Some("删"));
        let link = inlines.iter().find_map(|i| match i {
            Inline::Run(r) => r.style.link.clone().map(|l| (r.text.clone(), l)),
            _ => None,
        });
        assert_eq!(
            link,
            Some((
                "链".into(),
                Link {
                    dest: "https://a.example".into(),
                    title: Some("题".into())
                }
            ))
        );
    }

    #[test]
    fn nested_stacks_combine() {
        let doc = parse("**粗体里的 *斜体***");
        let [Block::Paragraph { inlines }] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        let italic_bold = inlines.iter().find_map(|i| match i {
            Inline::Run(r) if r.style.bold && r.style.italic => Some(r.text.clone()),
            _ => None,
        });
        assert_eq!(italic_bold.as_deref(), Some("斜体"));
    }

    #[test]
    fn tight_list_bare_text_becomes_paragraph() {
        // 紧列表项内容没有 Paragraph 标签,裸文本必须收进隐式段落,不得丢失。
        let doc = parse("- 甲\n- 乙");
        let [Block::List { start, items }] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(*start, None);
        assert_eq!(items.len(), 2);
        let [Block::Paragraph { inlines }] = items[0].blocks.as_slice() else {
            panic!("{:?}", items[0].blocks);
        };
        let text: String = inlines
            .iter()
            .map(inline_plain_text)
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(text, "甲");
    }

    #[test]
    fn lists_ordered_task_nested() {
        let doc = parse("1. 第一\n2. [x] 已办\n   - 内层甲\n   - 内层乙\n3. [ ] 未办\n\n- 无序\n");
        let [Block::List { start, items }, Block::List { start: None, .. }] = doc.blocks.as_slice()
        else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(*start, Some(1));
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].task, None);
        assert_eq!(items[1].task, Some(true));
        assert_eq!(items[2].task, Some(false));
        // 第二项内嵌无序列表(隐式段落在列表开始前收口)
        let [Block::Paragraph { .. }, Block::List {
            start: inner,
            items: inner_items,
        }] = items[1].blocks.as_slice()
        else {
            panic!("{:?}", items[1].blocks);
        };
        assert_eq!(*inner, None);
        assert_eq!(inner_items.len(), 2);
    }

    #[test]
    fn code_block_language_and_indent() {
        let doc = parse("```rust ignore\nfn main() {}\n```\n\n    缩进代码\n");
        let [Block::CodeBlock { language, code }, Block::CodeBlock {
            language: None,
            code: indented,
        }] = doc.blocks.as_slice()
        else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(language.as_deref(), Some("rust"));
        assert_eq!(code, "fn main() {}\n");
        assert_eq!(indented, "缩进代码\n");
    }

    #[test]
    fn quote_nests_blocks() {
        let doc = parse("> 引用一\n>\n> ## 引用内标题\n> > 套娃\n");
        let [Block::Quote { blocks }] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[0], Block::Paragraph { .. }));
        assert!(matches!(
            blocks.as_slice(),
            [_, Block::Heading { level: 2, .. }, Block::Quote { .. }]
        ));
    }

    #[test]
    fn table_alignments_header_rows() {
        let doc =
            parse("| 左 | 中 | 右 | 无 |\n|:--|:-:|--:|---|\n| a | b | c | d |\n| 1 | 2 | 3 | 4 |");
        let [Block::Table(table)] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(
            table.alignments,
            vec![
                Alignment::Left,
                Alignment::Center,
                Alignment::Right,
                Alignment::None
            ]
        );
        assert_eq!(table.header.len(), 4);
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1][3].len(), 1);
    }

    #[test]
    fn thematic_break_and_breaks() {
        let doc = parse("段落一  \n换行后\n\n---\n\n下一段");
        let [Block::Paragraph { inlines }, Block::ThematicBreak, Block::Paragraph { .. }] =
            doc.blocks.as_slice()
        else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(
            inlines
                .iter()
                .filter(|i| matches!(i, Inline::LineBreak))
                .count(),
            1,
            "恰好一个硬换行:{inlines:?}"
        );
    }

    #[test]
    fn image_placeholder_promoted_and_inline_kept() {
        let doc = parse("![替代文本](/img/a.png \"题\")\n\n前面 ![行内](b.png) 后面");
        let [Block::ImagePlaceholder { url, alt, title }, Block::Paragraph { inlines }] =
            doc.blocks.as_slice()
        else {
            panic!("{:?}", doc.blocks);
        };
        assert_eq!(url, "/img/a.png");
        assert_eq!(alt, "替代文本");
        assert_eq!(title.as_deref(), Some("题"));
        assert!(
            inlines.iter().any(|i| matches!(i, Inline::Image { .. })),
            "行内图片保留:{inlines:?}"
        );
    }

    #[test]
    fn footnotes_collected_separately() {
        let doc = parse("正文[^1] 与 [^2]。\n\n[^1]: 第一条\n[^2]: 第二条");
        assert_eq!(doc.footnotes.len(), 2);
        assert_eq!(doc.footnotes[0].label, "1");
        assert!(matches!(
            doc.footnotes[0].blocks.as_slice(),
            [Block::Paragraph { .. }]
        ));
        let refs: Vec<_> = doc
            .blocks
            .iter()
            .flat_map(|b| match b {
                Block::Paragraph { inlines } => inlines
                    .iter()
                    .filter(|i| matches!(i, Inline::FootnoteRef { .. }))
                    .collect(),
                _ => vec![],
            })
            .collect();
        assert_eq!(refs.len(), 2, "脚注引用应保留在正文里");
    }

    #[test]
    fn strikethrough_dialect_is_enabled() {
        // 照 latermd-export 既有测试形态钉住同方言:~~x~~ 必须被解析为删除线。
        let doc = parse("~~过时~~ 结论");
        let [Block::Paragraph { inlines }] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        assert!(
            inlines.iter().any(|i| match i {
                Inline::Run(r) => r.style.strike && r.text == "过时",
                _ => false,
            }),
            "{inlines:?}"
        );
    }

    #[test]
    fn empty_and_dropped_html() {
        assert_eq!(parse(""), Document::default());
        let doc = parse("<div>块</div>\n\n段落 <span>内联</span> 尾");
        // HTML 块丢弃;段落的纯文本保留
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(doc.blocks[0], Block::Paragraph { .. }));
    }

    #[test]
    fn unclosed_containers_do_not_panic() {
        // 残缺 Markdown(heal 前的流式帧)不 panic,已收内容照常产出。
        let doc = parse("> 引用未闭合\n\n- 列表项未闭合\n- **粗体未闭合");
        assert!(doc.blocks.len() >= 2, "{:?}", doc.blocks);
    }

    #[test]
    fn cjk_text_survives_untouched() {
        let doc = parse("中文内容、日本語、한국어、emoji 🎉 混排");
        let [Block::Paragraph { inlines }] = doc.blocks.as_slice() else {
            panic!("{:?}", doc.blocks);
        };
        let text: String = inlines
            .iter()
            .map(inline_plain_text)
            .collect::<Vec<_>>()
            .concat();
        assert_eq!(text, "中文内容、日本語、한국어、emoji 🎉 混排");
        assert!(!text.contains('\u{FFFD}'));
    }
}
