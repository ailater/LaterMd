#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! HTML/PDF 导出(docs/roadmap.md「导出」线)。
//!
//! 纯逻辑 crate:输入 Markdown 源文本,输出可直接落盘的完整 HTML 文档
//! 或 PDF 字节。解析与渲染都走 pulldown-cmark(单一解析器铁律,AGENTS.md
//! §3),扩展开关与 vendored `egui_markdown::parser::parse` 逐项一致——
//! 开关的唯一持有点在 [`latermd_render::parser_options`](HTML 与 PDF
//! 两条链路都委托它),预览/HTML/PDF 三方同方言。
//!
//! PDF 链路(ADR-006 路线 A):`latermd-render` 产绘制指令 IR(铁律 2,
//! 零 UI 依赖),本 crate 的 pdf 模块把 IR 布局到 A4 页面并经 krilla
//! 编码为字节(见 [`export_pdf`] / [`export_document`])。

mod pdf;

pub use pdf::cjk::{discover_cjk_fonts, CjkFontCandidate, CjkFontError, CJK_SYSTEM_CANDIDATES};
pub use pdf::{
    export_document, export_pdf, PdfError, PdfExportOptions, PdfFont, PdfFonts, A4_HEIGHT, A4_WIDTH,
};

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag, TagEnd};

/// 无标题文档的 `<title>` 回退值,与应用内「未命名」词汇一致。
const FALLBACK_TITLE: &str = "未命名";

/// 内嵌样式:代码块底色、表格边框、正文 max-width 46em、代码等宽字体,
/// 外加暗色模式跟随(`prefers-color-scheme`)。刻意不引外部资源,单文件
/// 即可直接交付(docs/roadmap.md P0 验收「导出的 HTML 可直接交付他人阅读」)。
const CSS: &str = r#"body {
  margin: 0 auto;
  padding: 2rem 1rem;
  max-width: 46em;
  line-height: 1.6;
  font-family: system-ui, -apple-system, "Segoe UI", "PingFang SC", "Noto Sans CJK SC", sans-serif;
}
code, pre {
  font-family: ui-monospace, "Cascadia Code", Menlo, Consolas, "Noto Sans Mono CJK SC", monospace;
}
pre {
  padding: 0.8em 1em;
  border-radius: 6px;
  background: #f6f8fa;
  overflow-x: auto;
}
code {
  padding: 0.15em 0.35em;
  border-radius: 4px;
  background: #f6f8fa;
  font-size: 0.9em;
}
pre code {
  padding: 0;
  background: none;
}
table {
  border-collapse: collapse;
}
th, td {
  border: 1px solid #d0d7de;
  padding: 0.35em 0.75em;
}
th {
  background: #f6f8fa;
}
blockquote {
  margin-inline: 0;
  padding-inline: 1em;
  border-inline-start: 4px solid #d0d7de;
  color: #57606a;
}
img {
  max-width: 100%;
}
/* mark 底色与 app 内 `highlight_bg_color` 同一条推导式(#65 M2):
   30% 荧光黄 (255,230,0) 混 70% 出厂 extreme_bg token —— 明 #F5F6F7
   出 #f8f1ad,暗 #232427 出 #655e1b(式子记录在 decisions-pending)。
   暗档文字取出厂皮肤正文色 #e8eaed(压底色 5.5:1,≥ AA 正文线);
   明档 inherit(黑字压 #f8f1ad ≈ 13.7:1)。 */
mark {
  padding: 0.1em 0.2em;
  border-radius: 3px;
  background: #f8f1ad;
  color: inherit;
}
@media (prefers-color-scheme: dark) {
  pre, code, th {
    background: #161b22;
  }
  th, td {
    border-color: #30363d;
  }
  blockquote {
    border-color: #30363d;
    color: #8b949e;
  }
  mark {
    background: #655e1b;
    color: #e8eaed;
  }
}"#;

/// Markdown → 完整 HTML 文档(UTF-8,含内嵌 CSS)。
pub fn export_html(text: &str) -> String {
    // 事件先收进 Vec:渲染与取标题共用一次解析结果;标题取自高亮包裹
    // **之后**的事件流 —— `# ==重点==` 的标题是「重点」,`==` 是语法
    // 不是内容(与预览一致,标记不回流正文文本)。
    let events = wrap_highlights(Parser::new_ext(text, parser_options()).collect());
    let title = document_title(&events);
    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());
    format!(
        "<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{CSS}\n</style>\n</head>\n<body>\n{body}</body>\n</html>\n"
    )
}

/// `==高亮==` → `<mark>`(HTML 语义同义标签的口径:#65 M2 选原生
/// `<mark>`,观感由内嵌 CSS 接管,浏览器默认黄只作无 CSS 兜底)。
///
/// 扫描复用 [`latermd_md::highlight_spans`](唯一扫描器):与预览管线同一
/// 配对口径,不产生第二套 == 方言。豁免构造(代码块/链接/图片/强调类/
/// 脚注定义/HTML 块 —— 与 `highlight_spans` 的豁免清单同一份)内部的
/// Text 事件不扫,由事件嵌套深度门控表达:这些构造在 pulldown 事件流里
/// 本就成对出现,深度计数天然平衡。行内代码(`Event::Code`)是原子事件,
/// 到不了 Text 分支,天然豁免。
///
/// 扫描单元是**相邻 Text 事件的合并串**而非单条事件:pulldown 会把
/// HTML 实体解码成独立 Text(`&amp;` → Text("&")),`==a&amp;b==` 因此
/// 被拆进三条 Text,逐条扫描必漏标。合并到任何非 Text 事件即止(软换行/
/// 行内代码/脚注引用等都打断),不跨构造拼接,配对口径与预览整串扫描一致。
fn wrap_highlights(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut wrapped: Vec<Event<'_>> = Vec::with_capacity(events.len());
    let mut exempt_depth = 0_usize;
    let mut pending: Vec<CowStr<'_>> = Vec::new();
    for event in events {
        match event {
            Event::Start(
                tag @ (Tag::CodeBlock { .. }
                | Tag::Link { .. }
                | Tag::Image { .. }
                | Tag::HtmlBlock
                | Tag::FootnoteDefinition(_)
                | Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough),
            ) => {
                drain_pending(&mut wrapped, &mut pending);
                exempt_depth += 1;
                wrapped.push(Event::Start(tag));
            }
            Event::End(
                tag_end @ (TagEnd::CodeBlock
                | TagEnd::Link
                | TagEnd::Image
                | TagEnd::HtmlBlock
                | TagEnd::FootnoteDefinition
                | TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough),
            ) => {
                drain_pending(&mut wrapped, &mut pending);
                exempt_depth = exempt_depth.saturating_sub(1);
                wrapped.push(Event::End(tag_end));
            }
            Event::Text(text) if exempt_depth == 0 => pending.push(text),
            other => {
                drain_pending(&mut wrapped, &mut pending);
                wrapped.push(other);
            }
        }
    }
    drain_pending(&mut wrapped, &mut pending);
    wrapped
}

/// 把累积的相邻 Text 事件交给 [`push_marked_text`] 产出,再清空缓冲。
fn drain_pending<'a>(wrapped: &mut Vec<Event<'a>>, pending: &mut Vec<CowStr<'a>>) {
    if !pending.is_empty() {
        push_marked_text(wrapped, std::mem::take(pending));
    }
}

/// 相邻 Text 事件合并串按高亮段切开:`==` 标记被消费(与预览一致,不再
/// 出现在输出文本),内容包 `<mark>`;标记外片段原样保留。切出的片段转
/// owned (`CowStr::Boxed`)—— 文本切片借的是 CowStr 内部缓冲,事件流还要
/// 继续持有,owned 化一次买断生命周期;单段且无高亮的快路径零拷贝原样
/// 透传。
fn push_marked_text<'a>(out: &mut Vec<Event<'a>>, texts: Vec<CowStr<'a>>) {
    if let [only] = texts.as_slice() {
        if latermd_md::highlight_spans(only).is_empty() {
            out.push(Event::Text(texts.into_iter().next().expect("len == 1")));
            return;
        }
    }
    let joined: String = texts.iter().map(|text| &**text).collect();
    let spans = latermd_md::highlight_spans(&joined);
    if spans.is_empty() {
        // 合并串整体无配对:拆回原段透传,不动字节
        out.extend(texts.into_iter().map(Event::Text));
        return;
    }
    let mut last = 0_usize;
    for span in spans {
        out.push(Event::Text(CowStr::Boxed(
            joined[last..span.span.start].into(),
        )));
        out.push(Event::InlineHtml(CowStr::Borrowed(MARK_OPEN)));
        out.push(Event::Text(CowStr::Boxed(span.inner.into())));
        out.push(Event::InlineHtml(CowStr::Borrowed(MARK_CLOSE)));
        last = span.span.end;
    }
    out.push(Event::Text(CowStr::Boxed(joined[last..].into())));
}

/// `<mark>` 的开闭标签(`Event::InlineHtml` 由 pulldown 的 html writer
/// 原样透传,pulldown-cmark 0.13.4 html.rs `Html(html) | InlineHtml(html)`
/// 分支 —— 不走文本转义,这正是行内标记的通道)。
const MARK_OPEN: &str = "<mark>";
const MARK_CLOSE: &str = "</mark>";

/// 与 vendored `egui_markdown::parser::parse`(egui_markdown/src/parser.rs
/// `parse` 内的 options 组装)逐项一致的扩展开关,委托 [`latermd_render`]
/// 的唯一持有点——HTML 与 PDF 两条链路同方言由构造保证。
fn parser_options() -> Options {
    latermd_render::parser_options()
}

/// 取文档标题:第一个标题(任意层级)的纯文本,无标题回退 [`FALLBACK_TITLE`]。
/// 文本片段来自事件流(未转义原文),而 `<title>` 不经过 push_html 的转义,
/// 须自行转义 `& < >`。
fn document_title(events: &[Event<'_>]) -> String {
    let mut in_heading = false;
    let mut title = String::new();
    for event in events {
        match event {
            Event::Start(Tag::Heading { .. }) => in_heading = true,
            Event::End(TagEnd::Heading(_)) if in_heading => break,
            Event::Text(text) if in_heading => title.push_str(text),
            // 标题里的行内代码(`# 使用 `ropey` 的缓冲`)也是标题文本的一部分
            Event::Code(text) if in_heading => title.push_str(text),
            _ => {}
        }
    }
    if title.is_empty() {
        FALLBACK_TITLE.to_owned()
    } else {
        title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全部输出都是 UTF-8 Rust String,这里统一断言中文原样保留、无替换符。
    fn assert_no_mojibake(html: &str, expected: &str) {
        assert!(html.contains(expected), "缺 {expected:?}:\n{html}");
        assert!(!html.contains('\u{FFFD}'), "出现替换符 U+FFFD:\n{html}");
    }

    #[test]
    fn heading_renders_and_feeds_title() {
        let html = export_html("# 后来MD 指南\n\n正文段落。");
        assert_no_mojibake(&html, "<h1>后来MD 指南</h1>");
        assert!(html.contains("<title>后来MD 指南</title>"), "{html}");
    }

    #[test]
    fn gfm_table_renders_headers_and_cells() {
        let html = export_html("| 列甲 | 列乙 |\n|---|---|\n| 1 | 2 |");
        assert!(html.contains("<table>"), "{html}");
        assert_no_mojibake(&html, "<th>列甲</th>");
        assert!(html.contains("<td>2</td>"), "{html}");
    }

    #[test]
    fn fenced_code_keeps_language_class() {
        let html = export_html("```rust\nfn main() {}\n```");
        assert!(
            html.contains("<pre><code class=\"language-rust\">"),
            "{html}"
        );
        assert_no_mojibake(&html, "fn main() {}");
    }

    #[test]
    fn task_list_renders_checkboxes() {
        let html = export_html("- [ ] 待办\n- [x] 已办");
        assert!(
            html.contains("<input disabled=\"\" type=\"checkbox\" checked=\"\"/>"),
            "勾选态缺失:\n{html}"
        );
        assert!(
            html.contains("<input disabled=\"\" type=\"checkbox\"/>"),
            "未勾选态缺失:\n{html}"
        );
        // pulldown-cmark 把 checkbox 直接放在 <li> 内、文本之前
        assert_no_mojibake(&html, "待办");
        assert_no_mojibake(&html, "已办");
    }

    /// 删除线与脚注不在题面清单里,但都挂在同一组 Options 上,一并钉住。
    #[test]
    fn strikethrough_and_footnotes_are_enabled() {
        let strike = export_html("~~过时~~ 结论");
        assert!(strike.contains("<del>过时</del>"), "{strike}");

        let footnote = export_html("正文[^1]。\n\n[^1]: 脚注内容");
        assert!(footnote.contains("footnote-reference"), "{footnote}");
        assert_no_mojibake(&footnote, "脚注内容");
    }

    #[test]
    fn minimal_css_is_embedded() {
        let html = export_html("x");
        for rule in [
            "max-width: 46em",
            "border: 1px solid #d0d7de",
            "background: #f6f8fa",
            "monospace",
        ] {
            assert!(html.contains(rule), "CSS 缺 {rule}:\n{html}");
        }
        assert!(html.starts_with("<!DOCTYPE html>\n"), "{html}");
        assert!(html.contains("<meta charset=\"utf-8\">"), "{html}");
    }

    #[test]
    fn title_falls_back_and_escapes_markup() {
        assert!(export_html("只有段落。").contains(&format!("<title>{FALLBACK_TITLE}</title>")));
        let html = export_html("# a & b < c\n\n正文");
        assert!(html.contains("<title>a &amp; b &lt; c</title>"), "{html}");
        // 转义只作用于 <title>,正文交给 push_html 自己转义
        assert!(html.contains("<h1>a &amp; b &lt; c</h1>"), "{html}");
    }

    /// 标题里的行内代码同样进 `<title>`(见 `document_title` 的 Code 分支)。
    #[test]
    fn inline_code_in_heading_joins_title() {
        let html = export_html("# 使用 `ropey` 的缓冲\n");
        assert!(html.contains("<title>使用 ropey 的缓冲</title>"), "{html}");
    }

    #[test]
    fn empty_input_still_yields_complete_document() {
        let html = export_html("");
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.ends_with("</html>\n"), "{html}");
    }

    /// 同方言铁律(铁律 1):HTML 与 PDF 共用 `latermd_render::parser_options`
    /// 这一个持有点,本 crate 的 `parser_options` 只是委托——一致性由构造
    /// 保证,此处钉住四个扩展开关确实全部在位。
    #[test]
    fn pdf_dialect_matches_html_dialect() {
        assert_eq!(parser_options(), latermd_render::parser_options());
        for flag in [
            Options::ENABLE_STRIKETHROUGH,
            Options::ENABLE_TABLES,
            Options::ENABLE_FOOTNOTES,
            Options::ENABLE_TASKLISTS,
        ] {
            assert!(parser_options().contains(flag));
        }
    }

    // —— ==高亮== → <mark>(#65 M2)——

    /// 基本形态:高亮段输出 `<mark>`,标记不回流正文文本,内嵌 CSS 带
    /// mark 底色规则(明暗两档)。
    #[test]
    fn highlight_exports_as_mark() {
        let html = export_html("重点 ==高亮内容== 收尾。");
        assert_no_mojibake(&html, "重点 <mark>高亮内容</mark> 收尾。");
        // `==` 标记被消费:正文区不再出现(mark 标签与 CSS 里也不含 ==)
        assert!(!html.contains("=="), "标记不得回流正文:\n{html}");
        assert!(html.contains("mark {"), "内嵌 CSS 缺 mark 规则:\n{html}");
        assert!(
            html.contains("background: #f8f1ad"),
            "明档底色缺失:\n{html}"
        );
        assert!(
            html.contains("background: #655e1b"),
            "暗档底色缺失:\n{html}"
        );
        assert!(
            html.contains("color: #e8eaed"),
            "暗档 mark 文字色缺失:\n{html}"
        );
    }

    /// 豁免面与预览同一份:围栏/行内代码/链接/强调内部的 `==` 不进
    /// `<mark>`;豁免构造之外的照常标。
    #[test]
    fn highlight_skips_exempt_constructs() {
        let fenced = export_html("```text\n==x==\n```\n\n==真高亮==");
        assert!(
            !fenced.contains("<mark>x</mark>"),
            "围栏内不高亮:\n{fenced}"
        );
        assert!(
            fenced.contains("<mark>真高亮</mark>"),
            "围栏外照常高亮:\n{fenced}"
        );

        let code = export_html("`==x==` 与 ==真高亮==");
        assert!(
            !code.contains("<mark>x</mark>"),
            "行内代码内不高亮:\n{code}"
        );
        assert!(code.contains("<mark>真高亮</mark>"), "{code}");

        let link = export_html("[甲 ==乙==](u) 之后的 ==丙==");
        assert!(
            !link.contains("<mark>乙</mark>"),
            "链接文字内不高亮:\n{link}"
        );
        assert!(link.contains("<mark>丙</mark>"), "{link}");

        let strong = export_html("**粗 ==不标== 体** 与 ==标==");
        assert!(
            !strong.contains("<mark>不标</mark>"),
            "强调内部不高亮(与预览豁免清单一致):\n{strong}"
        );
        assert!(strong.contains("<mark>标</mark>"), "{strong}");
    }

    /// 多对与相邻对、转义文本:片段切开互不粘连,`&` 在 mark 内照常转义。
    #[test]
    fn highlight_splits_pairs_and_escapes_content() {
        // 相邻对(中间恰 4 个 `=`:闭标 2 + 开标 2,scanner 首尾相接)
        let html = export_html("==甲====乙==");
        assert_no_mojibake(&html, "<mark>甲</mark><mark>乙</mark>");

        let escaped = export_html("==a & b==");
        assert_no_mojibake(&escaped, "<mark>a &amp; b</mark>");
    }

    /// 实体输入:pulldown 把 `&amp;` 解码成独立 Text 事件,一对 `==` 被
    /// 拆进多条 Text——逐条扫描会漏标并让 `==` 回流正文。
    #[test]
    fn highlight_survives_entity_split_text_events() {
        let html = export_html("==a&amp;b== 与 ==真高亮==");
        assert_no_mojibake(&html, "<p><mark>a&amp;b</mark> 与 <mark>真高亮</mark></p>");
        assert!(!html.contains("=="), "标记不得回流正文:\n{html}");
    }

    /// 表格 cell 内照常高亮(表格不在豁免清单,与预览一致);标题里的
    /// 高亮同时把 `==` 从 `<title>` 里带走(标题取自包裹后的事件流)。
    #[test]
    fn highlight_works_in_table_cells_and_cleans_title() {
        let table = export_html("| 列 |\n|---|\n| ==格== |");
        assert_no_mojibake(&table, "<td><mark>格</mark></td>");

        let titled = export_html("# ==重点==\n\n正文。");
        assert!(titled.contains("<title>重点</title>"), "{titled}");
        assert!(titled.contains("<h1><mark>重点</mark></h1>"), "{titled}");
    }

    /// 否决线:无 `==` 的文档输出零 `<mark>`;未闭合/不配对的形态同样
    /// 不产生标记(与 `highlight_spans` 口径一致,导出不做第二套判定)。
    #[test]
    fn no_highlight_means_no_mark() {
        for text in [
            "普通文档,毫无高亮。\n\n- 列表项\n",
            "未闭合 ==x 不算\n",
            "空对 ==== 与纯空白 == == 都不算\n",
            "跨行 ==甲\n乙== 不算\n",
        ] {
            let html = export_html(text);
            assert!(!html.contains("<mark>"), "缺 guard {text:?}:\n{html}");
        }
    }
}
