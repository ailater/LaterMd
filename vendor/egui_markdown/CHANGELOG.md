# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `OverflowWrap` enum (`Normal`, `BreakAll`) and `MarkdownLabel::overflow_wrap`, which break a
  run of text that is wider than the available width.
- `MarkdownLabel::wrap_mode`, `.wrap()`, `.truncate()`, and `.extend()`, which mirror the same
  methods on the egui `Label`. Truncate elides after `max_lines` rows, which defaults to 1, and
  sets `BreakAll`.
- `TableStyle::stroke_width` on `MarkdownStyle`. A non-zero width draws separator lines
  between table cells. Default is `0.0` (no stroke).
- `TableStyle::corner_radius` on the outer table stroke.
- `TableStyle::cell_padding` (`[left, top, right, bottom]`) for the inset inside each cell.
- Overflow chrome on tables: a matching stroke on the visible cut edge, and a
  light inner shadow when the table does not fit horizontally or vertically.
- `MarkdownStyle::line_height_ratio` (default `1.30`): row height as a multiple of the font
  size, replacing the fixed `17px` height. Rows now derive their height from their own font
  size, so headings grow into the rhythm instead of being clipped by a height tuned for
  body-size glyphs.
- `MarkdownStyle::min_line_height_em` (default `1.0`, no floor): lower bound for the row height
  in font-size multiples. Hosts that register a CJK fallback chain set this to the fallback
  face's row-height need (e.g. Noto Sans CJK ≈ `1.448`): a row shorter than that still
  *advances* by its height, so the overflowing CJK ink collides with the next row or gets
  occluded by the next block's opaque background. When the floor engages (above the ratio),
  `0.75px` of absolute slack covers epaint's whole-pixel row snapping so the row still clears
  the need at every font size; rows also stay uniform across Latin-only and mixed lines. The
  default of `1.0` never engages for sane sizes, so Latin-only documents keep their rhythm
  to the pixel.
- `TableStyle::header_fill` and `TableStyle::zebra_fill` (both default `false`): fill the
  header row with `Visuals::faint_bg_color`, and zebra-stripe the body rows via
  `egui_extras`' built-in striped rows (same color). Both adapt to light and dark visuals.
- `MarkdownStyle::heading_space_above` (default `4.0`): extra vertical space above a
  heading, on top of `block_spacing`. It is emitted as a transparent spacer row of
  `block_spacing + heading_space_above` points ahead of the heading's first row, so it
  reaches both render paths — the whole-document galley and separately flushed segment
  ranges. A heading that starts the document (or a flushed range) or directly follows a
  block element keeps the plain `block_spacing`, and setting the field to `0.0` makes the
  spacer row exactly `block_spacing` tall. The spacer row counts towards a
  `max_rows` / truncate budget.
- `MarkdownStyle::segmentation_admission` (default `500`): a fenced code block whose body
  reaches this many lines is laid out as its own segment — the way tables, images and
  blockquotes already are — even when code blocks are not scrollable. The threshold is
  measured per code block (its line count), not per document (token or byte totals):
  `build_layout` runs on whole documents *and* on individual flushed ranges, and a
  whole-document measure is not knowable from inside a range, so the two callers would
  disagree about the same tokens; a per-block measure decides identically in every
  context, and admission never inserts or removes tokens, so a fence crossing the
  threshold cannot shift the token indices later block widgets bake into their ids. The
  admitted fence renders as its own flushed range with the same in-galley shape (padding,
  highlighting, wrapping, background), but laid out, cached and viewport-culled per
  block, so appending lines to it no longer re-lays-out the rest of the document. The
  default is deliberately conservative: a hand-written document almost never carries a
  single 500-line fence, so ordinary documents keep the whole-document galley path — and
  its pixel output — unchanged. `usize::MAX` disables admission entirely.

### Changed

- Default heading size scales widened from `1.6/1.35/1.2/1.1/1.05/1.0` to
  `2.0/1.55/1.30/1.15/1.08/1.0`: at a 13pt body this renders ~26.0/20.2/16.9/15.0/14.0/13.0pt,
  so the adjacent low levels H4–H6 stay visually distinguishable (they used to pack within
  ~0.6pt of each other, with H6 identical to body text) and H1 sits at the top of the
  customary 1.8–2.0× range for Latin body text, which also suits dense CJK glyphs. Hosts
  that set their own `scales` are unaffected, and body text is untouched.

- **Breaking:** `build_layout` now takes `max_width: f32` and `break_anywhere: bool`, and no
  longer reads `ui.wrap_mode()` itself. A caller that caches the resulting job must write the
  live wrap values over it before each shape, as `MarkdownLabel` already does.

- **Breaking:** `needs_segmentation` now takes `style: &MarkdownStyle`, and `build_layout`
  takes `segment_large_code_blocks: bool` before `style`, so both can honor
  `MarkdownStyle::segmentation_admission`. Whole-document callers pass `true` to stay in
  sync with `needs_segmentation`; a caller laying out a single admitted fence as its own
  flushed range passes `false` so the fence renders inline within that range.
- The height caches behind off-screen culling of block widgets (tables, scrolling code
  blocks, images) are now keyed by each block's own token — plus the style and link-handler
  id the previous whole-document key covered — instead of by the hash of the whole document
  text. Appending at the end of a document (streaming output) now re-measures only the
  edited block; every other off-screen block culls from its cached height. The entry stays
  keyed by token index, so an edit that changes the token count earlier in the document
  shifts later blocks to fresh (never stale) entries. Rendering output is unchanged: cache
  keys only decide when a block is re-laid-out, and a second frame on a hot context paints
  the same shapes as a cold one.

### Fixed

- `TextWrapMode::Truncate` on the surrounding `Ui`, and now on the widget builders, truncates
  the text. It previously behaved as wrap.

## [0.1.0] - 2026-03-23

### Added

- CommonMark markdown parser via `pulldown-cmark` with extensions: tables, strikethrough, footnotes, task lists.
- `MarkdownLabel` widget with text selection, clickable links, and cached layout.
- Syntax-highlighted code blocks via `syntect` (feature: `syntax_highlighting`).
- Custom syntax theme support via `MarkdownLabel::code_theme()` - pass your own `syntect::highlighting::Theme` instead of the built-in default.
- Scrollable code blocks with horizontal scroll and copy button overlay.
- Code block background fill using `ui.visuals().code_bg_color`.
- Image rendering via `egui_extras` (feature: `images`, `svg`).
- Table rendering with column alignment and pre-measured column widths.
- `heal_table()`, which completes a partial table separator in streaming input.
- Blockquote rendering with configurable indent and vertical bar.
- Horizontal rules.
- Nested ordered and unordered lists.
- Task list checkboxes.
- Footnote references and definitions.
- `heal()` function to auto-close unclosed code fences, bold, italic, strikethrough, inline code, and links for streaming input.
- `MarkdownStyle` for customizable visual styling (inline code, code blocks, headings, horizontal rules, blockquotes, block spacing, code font size, default code language).
- `LinkHandler` trait for custom link styling (`link_style`), click handling (`click`), inline layout (`layout_link`), inline widgets (`inline_widget_size` / `paint_inline_widget`), and block-level widgets (`is_block_widget` / `block_widget`).
- `LinkHandler::id()` for cache invalidation when handler behavior changes.
- Differentiated heading sizes: H1=1.6x, H2=1.35x, H3=1.2x, H4=1.1x, H5=1.05x, H6=1.0x.
- `section_for_char()` on-demand lookup (replaces per-frame allocation).
- Language alias mapping (`ts`/`tsx`/`jsx` to `javascript`) for broader syntax highlighting coverage.
- `render_galley` wrap-width fix - text re-wraps correctly when the container resizes.
- Two examples: `simple` (editor + rendered output), `advanced` (style editor, custom link handlers, inline widgets, streaming simulation).
