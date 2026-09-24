#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! HTML 导出(docs/roadmap.md P0「导出」)。
//!
//! 纯逻辑 crate:输入 Markdown 源文本,输出可直接落盘的完整 HTML 文档。
//! 解析与渲染都走 pulldown-cmark(单一解析器铁律,AGENTS.md §3),扩展
//! 开关与 vendored `egui_markdown::parser::parse` 逐项一致(见私有
//! `parser_options`)—— 预览与导出必须同方言,否则同一篇文档两边
//! 看到两种结果。

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

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
}"#;

/// Markdown → 完整 HTML 文档(UTF-8,含内嵌 CSS)。
pub fn export_html(text: &str) -> String {
    // 事件先收进 Vec:渲染与取标题共用一次解析结果
    let events: Vec<Event> = Parser::new_ext(text, parser_options()).collect();
    let title = document_title(&events);
    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());
    format!(
        "<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{CSS}\n</style>\n</head>\n<body>\n{body}</body>\n</html>\n"
    )
}

/// 与 vendored `egui_markdown::parser::parse`(egui_markdown/src/parser.rs
/// `parse` 内的 options 组装)逐项一致的扩展开关。上游改动方言时同步这里,
/// 单测覆盖四个扩展各自的输出形态以钉住一致性。
fn parser_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_TASKLISTS);
    options
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
}
