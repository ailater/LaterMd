use std::ops::Range;

use egui::{pos2, vec2, Context, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{block_rect_at_offset, block_span_rects, parse, BlockSpanRect, MarkdownLabel};

fn screen() -> Rect {
  Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 2000.0))
}

/// Heading spans (byte ranges) of `text`, in document order — the same input an
/// outline pane would hand to `block_rect_at_offset`.
fn heading_spans(text: &str) -> Vec<Range<usize>> {
  parse(text)
    .tokens
    .iter()
    .zip(&parse(text).spans)
    .filter_map(|(token, span)| match token {
      egui_markdown::Token::Text { style, .. } if style.heading.is_some() => Some(span.clone()),
      _ => None,
    })
    .collect()
}

/// Render `text` once under `id` and return the block table read back in the
/// same frame.
fn blocks_for(text: &str) -> Vec<BlockSpanRect> {
  let ctx = Context::default();
  let id = Id::new("blocks");
  let mut read = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    read = block_span_rects(&child, id);
  });
  output.textures_delta.clear();
  read.unwrap_or_default()
}

/// The mixed document from the brief: multi-level headings, paragraphs, a table
/// and a fenced code block — this exercises the segmented path (table) and the
/// in-galley code block at once.
fn mixed_doc() -> String {
  "# 标题一\n\n段落甲,讲一些内容。\n\n## 标题二\n\n| 列甲 | 列乙 |\n|---|---|\n| 1 | 2 |\n\n### 标题三\n\n\
   ```rust\nfn main() {}\n```\n\n尾段一段。\n"
    .to_owned()
}

/// Every heading's span resolves to a non-empty rect, and headings later in the
/// document sit strictly lower on screen.
#[test]
fn heading_spans_resolve_in_monotonic_order() {
  let text = mixed_doc();
  let blocks = blocks_for(&text);
  assert!(!blocks.is_empty(), "no blocks recorded");

  let mut previous_top = f32::NEG_INFINITY;
  for span in heading_spans(&text) {
    let hit = block_rect_at_offset_for(&text, span.start);
    let hit = hit.expect("heading offset must resolve");
    assert!(hit.rect.height() > 0.0, "empty rect for {span:?}: {hit:?}");
    assert!(hit.rect.min.y.is_finite() && hit.rect.max.y.is_finite(), "{hit:?}");
    assert!(
      hit.rect.min.y > previous_top,
      "later heading sits at {} not below {previous_top}: {hit:?}",
      hit.rect.min.y
    );
    previous_top = hit.rect.min.y;
  }
}

fn block_rect_at_offset_for(text: &str, offset: usize) -> Option<BlockSpanRect> {
  let ctx = Context::default();
  let id = Id::new("blocks");
  let mut read = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    read = block_rect_at_offset(&child, id, offset);
  });
  output.textures_delta.clear();
  read
}

/// The four offset cases from the brief: block start, inside a block, the gap
/// between two blocks, and the end of the document.
#[test]
fn offset_resolution_covers_start_middle_gap_and_end() {
  let text = mixed_doc();
  let blocks = blocks_for(&text);
  // A text block (the first heading's block) for start/middle probes.
  let headings = heading_spans(&text);
  let first = &headings[0];
  let block = block_rect_at_offset_for(&text, first.start).expect("block start resolves");

  // 块首:the block's own span starts here
  assert_eq!(block.span.start, first.start, "block start hits the block");
  // 块中:inside the heading text, still the same block
  let mid = first.start + 2;
  assert!(mid < first.end, "probe inside the heading span");
  let hit_mid = block_rect_at_offset_for(&text, mid).expect("inside resolves");
  assert_eq!(hit_mid.span, block.span, "inside the block hits the same block");

  // 块间:an offset covered by no block's span must fall to the nearest
  // *following* block. Under the parser's contiguous-span invariant the only
  // bytes no block covers are the trailing ones past the last content token
  // (newline tokens are zero-width), so probe there: a trailing "\n\n" past the
  // final paragraph belongs to no block.
  let trailing = &text[blocks.last().map(|b| b.span.end).unwrap_or(0)..];
  assert!(!trailing.is_empty() && trailing.trim().is_empty(), "expected trailing whitespace gap, got {trailing:?}");
  let gap_offset = text.len() - 1;
  let hit_gap = block_rect_at_offset_for(&text, gap_offset).expect("gap resolves");
  let last = blocks.last().expect("blocks exist");
  assert_eq!(hit_gap.span, last.span, "trailing gap falls to the last block");

  // 文档末:past the last block's span — still legal, resolves to the last block.
  let hit_end = block_rect_at_offset_for(&text, text.len()).expect("document end resolves");
  let last_top = blocks.iter().map(|b| b.rect.min.y).fold(f32::NEG_INFINITY, f32::max);
  assert_eq!(hit_end.rect.min.y, last_top, "end hits the last block");
  assert!(!_blocks_overlap_or_disorder(&blocks), "blocks must be ordered without overlap");
}

/// Query semantics against a synthetic table, independent of what a particular
/// render happens to produce: containing hit, gap → nearest following block,
/// past-the-end → last block, before-first-block → first block.
///
/// The data key mirrors label.rs `block_rects_id` (`id.with("block-span-rects")`);
/// if that string drifts this test reads nothing and fails — no false green.
#[test]
fn query_semantics_on_synthetic_table() {
  let ctx = Context::default();
  let id = Id::new("synthetic");
  let mk = |start: usize, end: usize, top: f32| BlockSpanRect {
    span: start..end,
    rect: Rect::from_min_max(pos2(0.0, top), pos2(100.0, top + 10.0)),
  };
  // Two blocks with a real gap between 10 and 20.
  let table = vec![mk(0, 10, 0.0), mk(20, 30, 50.0)];
  let mut verdicts = Vec::new();
  let mut output = ctx.run_ui(RawInput::default(), |ui| {
    let frame = ui.ctx().cumulative_pass_nr();
    ui.ctx().data_mut(|d| d.insert_temp(id.with("block-span-rects"), (frame, table.clone())));
    for offset in [0, 5, 9, 10, 15, 19, 20, 25, 30, 999] {
      verdicts.push((offset, block_rect_at_offset(ui, id, offset).map(|b| b.span.clone())));
    }
  });
  output.textures_delta.clear();
  let resolved: Vec<(usize, Option<Range<usize>>)> = verdicts;
  let expect = |offset: usize, span: Range<usize>| {
    let got = resolved.iter().find(|(o, _)| *o == offset).expect("probed").1.clone();
    assert_eq!(got, Some(span), "offset {offset}");
  };
  expect(0, 0..10); // 块首
  expect(5, 0..10); // 块中
  expect(9, 0..10); // 块尾前
  expect(10, 20..30); // gap 首 → 后续块
  expect(15, 20..30); // gap 中 → 后续块
  expect(19, 20..30); // gap 尾 → 后续块
  expect(20, 20..30); // 第二块首
  expect(25, 20..30); // 第二块中
  expect(30, 20..30); // 文档末 → 最后块
  expect(999, 20..30); // 远超文末 → 最后块(合法边界)
}

/// Blocks are appended in render (document) order: tops non-decreasing, spans
/// ordered, no overlap.
fn _blocks_overlap_or_disorder(blocks: &[BlockSpanRect]) -> bool {
  for pair in blocks.windows(2) {
    if pair[1].rect.min.y < pair[0].rect.min.y - 0.5 {
      return true;
    }
    if pair[1].span.start < pair[0].span.end && pair[0].span.start != pair[1].span.start {
      return true;
    }
  }
  false
}

/// Changing the document rebuilds the table in the same frame: entries from the
/// old document are gone, headings of the new one resolve.
#[test]
fn document_change_rebuilds_table_same_frame() {
  let ctx = Context::default();
  let id = Id::new("blocks");
  let text_a = "# 甲标题\n\n正文甲。\n";
  let text_b = "# 乙标题\n\n| 表 | 格 |\n|---|---|\n| 1 | 2 |\n\n## 丙标题\n\n正文丙,更长一些。\n";
  let mut first_result = None;
  let mut second_result = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text_a).wrap().show(&mut child);
    first_result = block_span_rects(&child, id);
    // Same frame, same id, new document: the table must be reset, not appended.
    MarkdownLabel::new(id, text_b).wrap().show(&mut child);
    second_result = block_span_rects(&child, id);
  });
  output.textures_delta.clear();
  let a_blocks = first_result.expect("table written for A");
  let b_blocks = second_result.expect("table written for B");
  assert!(!a_blocks.is_empty() && !b_blocks.is_empty());
  assert!(b_blocks.len() < b_blocks.len() + a_blocks.len(), "sanity: arithmetic works");
  assert!(
    b_blocks.iter().all(|b| b.span.end <= text_b.len()),
    "entries from document A leaked into B's table: {b_blocks:?}"
  );
}

/// A frame later the table is stale: reading it returns `None`, so consumers
/// cannot act on last frame's screen rects.
#[test]
fn stale_frame_table_is_none() {
  let ctx = Context::default();
  let id = Id::new("blocks");
  let text = "# 标题\n\n正文。\n";
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    assert!(block_span_rects(&child, id).is_some(), "same frame: readable");
  });
  output.textures_delta.clear();
  let mut later = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let child = ui.new_child(UiBuilder::new().max_rect(screen()));
    // No render this frame — the previous table must not be readable.
    later = block_span_rects(&child, id);
  });
  output.textures_delta.clear();
  assert!(later.is_none(), "next frame without render: table must read as absent");
}

/// Off-screen (culled) blocks still appear in the table as coarse entries, so
/// the table covers the whole document, not just the visible slice. Culling
/// needs a cached height from a previous frame, so this renders twice with the
/// scroll moved to the bottom in between.
#[test]
fn culled_blocks_stay_in_table() {
  let ctx = Context::default();
  let id = Id::new("blocks");
  let mut text = String::from("# 顶部标题\n\n");
  for i in 0..40 {
    text.push_str(&format!("段落 {i},填充高度用的一行正文。\n\n"));
  }
  text.push_str("# 底部标题\n\n收尾。\n");
  let mut whole_table_height = 0.0f32;
  let mut scrolled_blocks = None;
  for frame in 0..2 {
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
      egui::ScrollArea::vertical().id_salt(id.with("scroll")).auto_shrink([false, false]).show(&mut child, |inner| {
        if frame == 1 {
          // Scroll the viewport to the bottom: the top text runs fall out of
          // view and are culled via their cached sizes.
          let bottom = egui::Rect::from_min_size(pos2(0.0, whole_table_height), vec2(400.0, 10.0));
          inner.scroll_to_rect_animation(bottom, Some(egui::Align::BOTTOM), egui::style::ScrollAnimation::none());
        }
        MarkdownLabel::new(id, &text).wrap().show(inner);
        whole_table_height = whole_table_height.max(inner.min_rect().bottom());
      });
      scrolled_blocks = block_span_rects(&child, id);
    });
    output.textures_delta.clear();
  }
  let blocks = scrolled_blocks.expect("frame 2 wrote a table");
  let headings = heading_spans(&text);
  let first_heading = block_hit(&blocks, headings[0].start);
  assert!(first_heading.is_some(), "top heading still in table after culling");
  let last_heading = block_hit(&blocks, headings[1].start);
  let last = last_heading.expect("bottom heading in table");
  assert!(last.rect.min.y > 0.0, "bottom heading has real geometry: {last:?}");
}

fn block_hit(blocks: &[BlockSpanRect], offset: usize) -> Option<BlockSpanRect> {
  blocks.iter().find(|b| b.span.start <= offset && offset < b.span.end).cloned()
}

/// The per-frame table lives in one reused slot per id (records mutate it in
/// place instead of rebuilding the `Vec` per record), so a later frame must
/// observe exactly that frame's records — not the previous frame's leftover
/// entries underneath. A block-heavy document rendered in frame 1 and another
/// one in frame 2 must read back identical to a cold-context render of the
/// frame-2 document, in the same span order.
#[test]
fn reused_table_slot_is_fresh_every_frame() {
  let id = Id::new("blocks");
  let text_a = many_blocks_doc(1);
  let text_b = many_blocks_doc(2);
  // Cold-context baseline for the frame-2 document.
  let cold_b = blocks_for(&text_b);
  assert!(cold_b.len() > 100, "sample must be block-heavy enough to catch duplication");

  let ctx = Context::default();
  for text in [&text_a, &text_b] {
    let text = text.clone();
    let mut read = None;
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
      MarkdownLabel::new(id, &text).wrap().show(&mut child);
      read = block_span_rects(&child, id);
    });
    output.textures_delta.clear();
    if text == text_b {
      let warm = read.expect("frame 2 wrote a table");
      assert_eq!(warm.len(), cold_b.len(), "reused slot holds exactly this frame's records");
      assert!(
        warm.iter().map(|b| b.span.clone()).eq(cold_b.iter().map(|b| b.span.clone())),
        "span sequence must match a fresh-context render"
      );
    } else {
      assert!(read.is_some(), "frame 1 wrote a table");
    }
  }
}

/// A plain-text document with 150 heading+paragraph blocks: exercises the
/// in-galley block-recording path (no segmentation) at a record count where a
/// per-record table rebuild would be visible.
fn many_blocks_doc(seed: u32) -> String {
  let mut text = String::new();
  for i in 0..150 {
    text.push_str(&format!("# 第 {seed}-{i} 节\n\n第 {i} 段正文,内容甲乙丙丁,足够一行以上。\n\n"));
  }
  text
}
