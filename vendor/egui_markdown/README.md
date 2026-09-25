# egui_markdown(vendored copy)

> **本目录是 vendored 副本**,来源 `membrane-io/egui_markdown` @ `4f3075f`(main,2026-09),
> 经 `git subtree add --squash` 引入。以下内容为上游原始 README,未改动。
> 合并上游 patch(`git subtree pull --prefix=vendor/egui_markdown https://github.com/membrane-io/egui_markdown.git main --squash`)时,按下表处理冲突:

| # | 与上游的差异 | 理由 |
|---|---|---|
| 1 | 删除 `[workspace]` 段(Cargo.toml) | 由 LaterMD 根 workspace 接管,保证单依赖图(同一 egui 版本) |
| 2 | egui 0.34 → 0.36.2(Cargo.toml × 2) | 修 epaint 0.35 强类型重构(PR #8245)的 15 个编译错误,修法见 [docs/vendor-upgrade-checklist.md](../../docs/vendor-upgrade-checklist.md)。实测补充:`ByteIndex` 需在 `mod syntect_code` 内单独 import;另有 checklist 未预见的 `TexturesDelta` drop 检查导致 10 个测试失败,已在测试中补 `output.textures_delta.clear()` |
| 3 | **删除 `membrane` feature**(约 23 处 cfg) | 上游自家产品定制,在 published egui 0.36 中无对应 API(上游 README 自述"does not compile against published egui"),对我们无价值。连带删除 `egui_markdown_style` 的 5 个 membrane 字段、`tests/indent.rs`(整文件 membrane-only)与 `tests/width.rs` 的一个 membrane 测试 |
| 4 | 删除本目录 `rust-toolchain.toml` | 根目录已钉 1.98.0,子目录文件会遮蔽根配置 |
| 5 | **新增 `Markdown::spans` 平行数组**(parser.rs / types.rs) | 顶层 token 的源码 byte range,连续无洞(`spans.len() == tokens.len()`)。大纲导航与 Live Preview 的 prerequisite,LaterMD 定制 |
| 6 | pulldown-cmark 钉 0.13.4 | 上游写 `0.13.0`,锁定补丁版本 |

`bash check.sh` 六项门禁在本目录运行时作用于整个 LaterMD workspace。

---

[![crates.io](https://img.shields.io/crates/v/egui_markdown.svg)](https://crates.io/crates/egui_markdown)
[![docs.rs](https://docs.rs/egui_markdown/badge.svg)](https://docs.rs/egui_markdown)
[![license](https://img.shields.io/crates/l/egui_markdown.svg)](https://github.com/iamseeley/egui_markdown#license)
![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-brightgreen.svg)

A markdown parser and renderer for [egui](https://github.com/emilk/egui).

It parses [CommonMark](https://commonmark.org/) markdown into a token stream. It then
renders that stream as one interactive egui widget, with selectable text, clickable links,
syntax-highlighted code blocks, tables, images, blockquotes, and lists.

## Quick Start

```rust
use eframe::egui;
use egui_markdown::MarkdownLabel;

fn show_markdown(ui: &mut egui::Ui) {
    let text = "# Hello\n\nThis is **bold** and *italic*.";
    MarkdownLabel::new(ui.id().with("md"), text).show(ui);
}
```

## Supported Markdown

**CommonMark** - headings (H1-H6), paragraphs, bold, italic, strikethrough,
inline code, fenced code blocks, links, images, blockquotes (nested), ordered
and unordered lists (nested), horizontal rules.

**GitHub Flavored Markdown** - tables (with column alignment), task lists,
footnotes.

## Features

- **Text selection** - rendered markdown is selectable in the same way as normal egui text.
- **Scrollable code blocks** - a long line scrolls horizontally and does not wrap.
- **Code block overlays** - a callback attaches buttons, such as a copy button or a language badge, to a code block.
- **Syntax highlighting** - highlighting through `syntect`, with built-in base16-ocean dark and light themes, and support for custom themes.
- **Custom link handlers** - the `LinkHandler` trait lets you style links, respond to clicks, and override the layout. It can also render a link as an inline or block-level widget.
- **Streaming and heal mode** - `.heal(true)` closes unclosed code fences, bold, italic, links, and tables, so that partial LLM output renders correctly.
- **Configurable style** - `MarkdownStyle` controls the inline code colors, and the code block padding, radius, stroke, and font size. It also controls the heading scales, the horizontal rule stroke, the blockquote indent, the table stroke, corner radius, and cell padding, and the block spacing. `MarkdownStyle::ui()` gives a built-in interactive editor.
- **Bold font family** - uses a registered `"bold"` font family when one exists, and the strong text color when none exists.
- **Layout caching and viewport culling** - layout caching based on a hash, with viewport culling per segment, for smooth scrolling through large documents.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `syntax_highlighting` | Yes | Syntax-highlighted code blocks through `syntect`. |
| `images` | No | Inline image rendering through the `egui_extras` image support. |
| `svg` | No | SVG image support through `egui_extras`. |
| `membrane` | No | Rendering that needs [the Membrane egui fork](https://github.com/membrane-io/egui). It does not compile against published egui. See the section below. |

### The `membrane` feature

Every feature above works against published egui. The `membrane` feature does not. It
calls `epaint::text::LeadingSpace`, `TextFormat::bg_corner_radius`, and a two-dimensional
`TextFormat::expand_bg`. Published egui has none of these three, so this feature causes a
compile error unless you patch egui to
[the fork](https://github.com/membrane-io/egui).

```toml
[patch.crates-io]
egui = { path = "path/to/egui/crates/egui" }
epaint = { path = "path/to/egui/crates/epaint" }
egui_extras = { path = "path/to/egui/crates/egui_extras" }
emath = { path = "path/to/egui/crates/emath" }
ecolor = { path = "path/to/egui/crates/ecolor" }
```

Patch `emath` and `ecolor` even when you do not depend on them directly. Without those two
entries the build has two incompatible copies of the types that egui shares between crates.
Add `eframe` as well when you use it.

Three things render differently without the feature. An inline code background has square
corners, and its padding expands by the same amount horizontally and vertically rather than
by separate amounts. A row that soft-wraps inside a list, or inside another indented block,
returns to the left margin. It does not keep the indentation of the line it continues. A
long run of text with no spaces breaks between two glyphs, and does not overrun the
available width.
[`OverflowWrap`](https://docs.rs/egui_markdown/latest/egui_markdown/enum.OverflowWrap.html)
selects between those two behaviors explicitly.

The parse, the selection, the links, the tables, and the syntax highlighting are the same
with the feature and without it.

## Customization

### `MarkdownStyle`

Use [`MarkdownStyle`](https://docs.rs/egui_markdown/latest/egui_markdown/style/struct.MarkdownStyle.html) to control the appearance of every markdown element.

```rust
use egui_markdown::{MarkdownLabel, MarkdownStyle};

let mut style = MarkdownStyle::default();
style.heading.scales[0] = 2.0; // Bigger H1
style.code_font_size = 14.0;
MarkdownLabel::new(id, text).style(&style).show(ui);
```

`MarkdownStyle` also has a `ui()` method that renders an interactive editor for every
style field. See the `advanced` example.

### `LinkHandler`

Implement the [`LinkHandler`](https://docs.rs/egui_markdown/latest/egui_markdown/link/trait.LinkHandler.html) trait
to customize the style of a link, the response to a click, and the rendering. Implement
`inline_widget()` to render a link as an inline widget, such as a user mention or a status
badge. Implement `is_block_widget()` to render a link as a block-level widget, such as an
embed or a card.

### Code Themes

Pass a custom `syntect::highlighting::Theme` to `.code_theme()` to override the
built-in base16-ocean theme.

### Streaming

Enable `.heal(true)` on `MarkdownLabel` to close unclosed code fences and inline constructs
before the parse. Without this, the parser treats the text that follows an unclosed
construct as part of that construct. Use it for streaming LLM output.

## Examples

| Example | Description |
|---------|-------------|
| `simple` | An editor and the rendered output. |
| `advanced` | A style editor, custom link handlers, inline widgets, code block buttons, and a streaming simulation. |

```sh
cargo run --example simple
cargo run --example advanced
```

## License

MIT or Apache-2.0
