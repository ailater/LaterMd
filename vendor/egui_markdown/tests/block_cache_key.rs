//! Block-level cache keys: the height caches behind `("block_sz", index)` and the
//! per-range flush caches must be keyed by each block's own content, so appending
//! at the end of a document re-renders only the edited block.
//!
//! Observability: a fence that renders always reaches the shape list, even
//! off-screen (egui does not cull shapes at record time — `ui.is_rect_visible`
//! culling is manual), while a culled fence paints nothing. Marker presence in a
//! later frame is therefore exact hit/miss evidence for off-screen fences:
//! absent means the cull hit, present means it re-rendered. The screen is shorter
//! than one 30-line fence, so the first fence always straddles the fold and
//! renders (the control); everything after it is off-screen. Tables cannot use
//! painted text as evidence (`egui_extras` culls table rows below the fold even
//! when the table renders), so their hit/miss is pinned through the stored cache
//! entry and the recorded block geometry, read back through egui temp data under
//! the same ids the renderer derives (`id.with(("block_sz", index))`,
//! `id.with(("flush_sz", start))`), using only public types.

use egui::{vec2, Context, Id, Pos2, RawInput, Rect, UiBuilder, Vec2, Visuals};
use egui_markdown::{block_span_rects, parse, BlockSpanRect, MarkdownLabel, MarkdownStyle, Token};

/// A screen shorter than one 30-line fence, so the first fence straddles the fold
/// and renders, and every later block is fully off-screen.
fn screen() -> Rect {
  Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 300.0))
}

/// A fenced code block whose body is `body_lines` lines, with `marker` on the
/// first body line so painted output can be attributed to this exact block.
fn fence(marker: &str, body_lines: usize) -> String {
  let mut doc = format!("```rust\n// {marker}\n");
  for _ in 0..body_lines.saturating_sub(1) {
    doc.push_str("    let step = log.tail()?;\n");
  }
  doc.push_str("```\n");
  doc
}

/// Token indices of every fenced code block in `doc`.
fn fence_indices(doc: &str) -> Vec<usize> {
  parse(doc).tokens.iter().enumerate().filter_map(|(i, t)| matches!(t, Token::CodeBlock { .. }).then_some(i)).collect()
}

/// Token index and source span of the (first) table in `doc`.
fn table_span(doc: &str) -> (usize, std::ops::Range<usize>) {
  let md = parse(doc);
  let index = md.tokens.iter().position(|t| matches!(t, Token::Table(_))).unwrap_or_else(|| panic!("doc has no table"));
  (index, md.spans[index].clone())
}

fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
  match shape {
    egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
    _ => {}
  }
}

fn collect_pos(shape: &egui::epaint::Shape, out: &mut Vec<(Pos2, String)>) {
  match shape {
    egui::epaint::Shape::Text(t) => out.push((t.pos, t.galley.text().to_owned())),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect_pos(s, out)),
    _ => {}
  }
}

/// Render `doc` once and return the text of every painted galley, in paint order.
/// `dark_mode` selects the visuals for the frame.
fn paint(ctx: &Context, doc: &str, scroll_code_blocks: bool, style: &MarkdownStyle, dark_mode: bool) -> Vec<String> {
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    ctx.set_visuals(if dark_mode { Visuals::dark() } else { Visuals::light() });
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).scroll_code_blocks(scroll_code_blocks).style(style).show(&mut child);
  });
  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  output.textures_delta.clear();
  out
}

/// Render `doc` once and also return the block table recorded by that frame.
fn paint_with_blocks(
  ctx: &Context,
  doc: &str,
  scroll_code_blocks: bool,
  style: &MarkdownStyle,
) -> (Vec<String>, Vec<BlockSpanRect>) {
  let mut painted = Vec::new();
  let mut blocks = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).scroll_code_blocks(scroll_code_blocks).style(style).show(&mut child);
    blocks = block_span_rects(&child, Id::new("test")).unwrap_or_default();
  });
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut painted);
  }
  output.textures_delta.clear();
  (painted, blocks)
}

/// Render `doc` once and return a painting fingerprint: total shape count plus,
/// for every text shape, its screen position and galley text. Cache hits must not
/// change any of it.
fn fingerprint(
  ctx: &Context,
  doc: &str,
  scroll_code_blocks: bool,
  style: &MarkdownStyle,
) -> (usize, Vec<(Pos2, String)>) {
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).scroll_code_blocks(scroll_code_blocks).style(style).show(&mut child);
  });
  let count = output.shapes.len();
  let mut texts = Vec::new();
  for clipped in &output.shapes {
    collect_pos(&clipped.shape, &mut texts);
  }
  output.textures_delta.clear();
  (count, texts)
}

/// The content hash the renderer stored for the block widget at token `index`.
fn block_hash(ctx: &Context, index: usize) -> Option<u64> {
  ctx.data(|d| d.get_temp::<(u64, f32)>(Id::new("test").with(("block_sz", index)))).map(|(hash, _)| hash)
}

/// The context hash the renderer stored for the flushed range starting at token `start`.
fn flush_range_hash(ctx: &Context, start: usize) -> Option<u64> {
  ctx.data(|d| d.get_temp::<(u64, f32, Vec2)>(Id::new("test").with(("flush_sz", start)))).map(|(hash, _, _)| hash)
}

/// The recorded rect of the block whose source span starts at `span_start`.
fn block_rect(blocks: &[BlockSpanRect], span_start: usize) -> Rect {
  blocks
    .iter()
    .find(|b| b.span.start == span_start)
    .map(|b| b.rect)
    .unwrap_or_else(|| panic!("no block recorded at span start {span_start}"))
}

fn painted(painted: &[String], marker: &str) -> bool {
  painted.iter().any(|t| t.contains(marker))
}

/// Appending a line to the last fence invalidates only that block. The first
/// frame paints every fence — proving off-screen renders reach the shape list —
/// so in the second frame an off-screen fence that paints nothing can only mean
/// its cached height was reused (a cull hit), and one that paints means a miss.
///
/// Expected matrix: the straddling first fence always renders; the unchanged
/// off-screen fence hits; the grown off-screen tail fence misses and re-renders.
#[test]
fn tail_append_invalidates_only_the_tail_block() {
  let ctx = Context::default();
  let style = MarkdownStyle::default();
  let doc_before = format!("{}{}{}", fence("mk-one", 30), fence("mk-two", 30), fence("mk-three", 30));
  let doc_after = format!("{}{}{}", fence("mk-one", 30), fence("mk-two", 30), fence("mk-three", 31));

  let first = paint(&ctx, &doc_before, true, &style, true);
  for marker in ["mk-one", "mk-two", "mk-three"] {
    assert!(painted(&first, marker), "cold frame must render every block ({marker} missing)");
  }

  let second = paint(&ctx, &doc_after, true, &style, true);
  assert!(painted(&second, "mk-one"), "the straddling first fence always renders");
  assert!(!painted(&second, "mk-two"), "an unchanged off-screen fence must cull from its cached height");
  assert!(painted(&second, "mk-three"), "the grown tail fence must re-render (cache miss)");
}

/// Editing the body of a middle fence without changing its token structure keeps
/// every token index stable, so exactly the edited block misses and re-renders;
/// the unchanged fence after the edit culls from its cache, and no stale wording
/// from the old marker survives.
#[test]
fn middle_edit_invalidates_only_the_edited_block() {
  let ctx = Context::default();
  let style = MarkdownStyle::default();
  let doc_before = format!("{}{}{}", fence("mk-one", 30), fence("mk-two alpha", 30), fence("mk-three", 30));
  let doc_after = format!("{}{}{}", fence("mk-one", 30), fence("mk-two bravo", 30), fence("mk-three", 30));

  let first = paint(&ctx, &doc_before, true, &style, true);
  assert!(painted(&first, "mk-two alpha"), "cold frame must render the middle fence");

  let second = paint(&ctx, &doc_after, true, &style, true);
  assert!(painted(&second, "mk-two bravo"), "the edited fence must re-render");
  assert!(!painted(&second, "mk-two alpha"), "stale wording from the edit must not survive");
  assert!(!painted(&second, "mk-three"), "the unchanged fence after the edit must cull from its cache");
}

/// Inserting a fence at the top shifts every later token index, so the later
/// blocks find no entry under their new ids and re-render (cold, never stale).
/// Everything must still paint: correctness over cache reuse.
#[test]
fn head_insert_re_renders_shifted_blocks_without_stale_output() {
  let ctx = Context::default();
  let style = MarkdownStyle::default();
  let doc_before = format!("{}{}", fence("mk-one", 30), fence("mk-two", 30));
  let doc_after = format!("{}{}{}", fence("mk-zero", 30), fence("mk-one", 30), fence("mk-two", 30));

  let first = paint(&ctx, &doc_before, true, &style, true);
  assert!(painted(&first, "mk-two"), "cold frame must render the tail fence");

  let second = paint(&ctx, &doc_after, true, &style, true);
  for marker in ["mk-zero", "mk-one", "mk-two"] {
    assert!(painted(&second, marker), "every block must paint after its id shifted ({marker})");
  }
}

/// The height-cache key is the block's own token (plus style and handler), not its
/// position or its document: the same fence source stores the same hash at a
/// different token index in a different document, different fence content hashes
/// differently, and a style change re-keys.
#[test]
fn block_key_is_content_addressed() {
  let style = MarkdownStyle::default();
  let doc_solo = fence("mk-shared", 6);
  let doc_with_intro = format!("Different intro paragraph.\n\n{}", fence("mk-shared", 6));
  let doc_other_content = fence("mk-other", 6);
  let solo_index = fence_indices(&doc_solo)[0];
  let with_intro_index = fence_indices(&doc_with_intro)[0];
  assert_ne!(solo_index, with_intro_index, "the fixture must actually shift the fence's token index");

  let ctx = Context::default();
  paint(&ctx, &doc_solo, true, &style, true);
  let hash_solo = block_hash(&ctx, solo_index).expect("entry written for the fence");

  let other_ctx = Context::default();
  paint(&other_ctx, &doc_with_intro, true, &style, true);
  let hash_with_intro = block_hash(&other_ctx, with_intro_index).expect("entry written for the fence");
  assert_eq!(
    hash_solo, hash_with_intro,
    "the same fence token must key identically regardless of position and surrounding text"
  );

  let third_ctx = Context::default();
  paint(&third_ctx, &doc_other_content, true, &style, true);
  let hash_other = block_hash(&third_ctx, fence_indices(&doc_other_content)[0]).expect("entry written");
  assert_ne!(hash_solo, hash_other, "different fence content must key differently");

  let styled_ctx = Context::default();
  let other_style = MarkdownStyle { code_font_size: style.code_font_size + 4.0, ..MarkdownStyle::default() };
  paint(&styled_ctx, &doc_solo, true, &other_style, true);
  let hash_styled = block_hash(&styled_ctx, solo_index).expect("entry written under the other style");
  assert_ne!(hash_solo, hash_styled, "a style change must re-key the block entry");
}

/// A document over the admission threshold (`segmentation_admission`), combining
/// both mechanisms: the intro and the straddling first fence flush as their own
/// ranges, the second fence and the table sit off-screen, and the tail fence is
/// the streamed block. Appending a line to the tail fence misses only that
/// fence's range — every other cache keeps its exact key, and an unchanged key
/// means the stored entry satisfies the lookup, which is the cache-hit branch.
#[test]
fn admitted_doc_tail_append_misses_only_the_tail_range() {
  let style = MarkdownStyle { segmentation_admission: 3, ..MarkdownStyle::default() };
  let intro = "Intro.\n\n";
  let table = "| tblcell | b |\n|---|---|\n| 1 | 2 |\n\n";
  let doc_before = format!("{intro}{}{table}{}", fence("mk-one", 30), fence("mk-two", 30));
  let doc_after = format!("{intro}{}{table}{}", fence("mk-one", 30), fence("mk-two", 31));

  let [one_index, two_index] = fence_indices(&doc_before)[..] else { panic!("fixture must have two fences") };
  let (table_index, table_span) = table_span(&doc_before);

  let ctx = Context::default();
  let (first, _) = paint_with_blocks(&ctx, &doc_before, false, &style);
  assert!(painted(&first, "Intro"), "the visible intro always renders");
  assert!(painted(&first, "mk-one"), "cold frame must render the first fence");
  assert!(painted(&first, "mk-two"), "cold frame must render the tail fence");
  let intro_hash = flush_range_hash(&ctx, 0).expect("intro flush cached");
  let one_range_hash = flush_range_hash(&ctx, one_index).expect("first fence range cached");
  let two_range_hash = flush_range_hash(&ctx, two_index).expect("tail fence range cached");
  let table_entry = block_hash(&ctx, table_index).expect("table height cached");

  let (second, second_blocks) = paint_with_blocks(&ctx, &doc_after, false, &style);
  assert!(painted(&second, "mk-one"), "the straddling first fence always renders");
  assert!(painted(&second, "mk-two"), "the grown tail fence must re-render (cache miss)");

  // Per-range keys: unchanged ranges keep their exact key, the edited one re-keys.
  assert_eq!(flush_range_hash(&ctx, 0), Some(intro_hash), "intro range key must be stable across the append");
  assert_eq!(
    flush_range_hash(&ctx, one_index),
    Some(one_range_hash),
    "unchanged fence range key must be stable across the append"
  );
  assert_ne!(flush_range_hash(&ctx, two_index), Some(two_range_hash), "the appended-to fence range must re-key");
  assert_eq!(block_hash(&ctx, table_index), Some(table_entry), "table entry key must be stable across the append");

  // The culled table keeps its recorded geometry: the cached height the cull
  // reused matches what a fresh render of the same document measures.
  let cold_ctx = Context::default();
  let (_, cold_blocks) = paint_with_blocks(&cold_ctx, &doc_after, false, &style);
  assert_eq!(
    block_rect(&second_blocks, table_span.start).height(),
    block_rect(&cold_blocks, table_span.start).height(),
    "a cull hit must reserve the height a fresh render would measure for the table"
  );
}

/// Editing an off-screen table re-measures it: its recorded geometry grows to the
/// new content and matches a cold render. This is the over-narrow-key guard — a
/// key that ignored the table's content would reuse the stale smaller height.
#[test]
fn table_edit_remeasures_offscreen_table() {
  let style = MarkdownStyle::default();
  let small_table = "| a | b |\n|---|---|\n| 1 | 2 |\n\n";
  let big_table = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n| 5 | 6 |\n\n";
  let doc_small = format!("Intro.\n\n{}{small_table}Tail.", fence("mk-one", 30));
  let doc_big = format!("Intro.\n\n{}{big_table}Tail.", fence("mk-one", 30));

  let ctx = Context::default();
  let (_, small_blocks) = paint_with_blocks(&ctx, &doc_small, false, &style);
  // The "Tail." paragraph sits off-screen behind the fence; a painted-text check
  // is not meaningful for it (a correct cull paints nothing), so the geometry of
  // the table block is the whole story here.
  let (_, big_blocks) = paint_with_blocks(&ctx, &doc_big, false, &style);

  let small_span = table_span(&doc_small).1;
  let big_span = table_span(&doc_big).1;
  let small_height = block_rect(&small_blocks, small_span.start).height();
  let warm_height = block_rect(&big_blocks, big_span.start).height();
  assert!(warm_height > small_height, "an edited table must re-measure ({warm_height} > {small_height})");

  let cold_ctx = Context::default();
  let (_, cold_blocks) = paint_with_blocks(&cold_ctx, &doc_big, false, &style);
  assert_eq!(
    warm_height,
    block_rect(&cold_blocks, big_span.start).height(),
    "the re-measured table height must match a cold render"
  );
}

/// Cache keys must not change what is painted: a second frame on a hot context —
/// where the per-block and per-range caches hit — paints the same shapes at the
/// same positions as the cold first frame. This is the pixel-identity veto line,
/// on the whole-document path (default style, ordinary documents stay there) and
/// on the segmented path (table doc and scrolling-code doc, everything visible).
#[test]
fn hot_cache_frames_paint_identical_shapes() {
  let docs = [
    ("Intro paragraph.\n\n```rust\nfn main() {}\n```\n\nOutro paragraph.", false),
    ("# Heading\n\nBody with `code` and **bold**.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n", false),
    ("Before.\n\n```rust\nfn a() {}\n```\n\nAfter.", true),
  ];
  for (doc, scroll) in docs {
    let ctx = Context::default();
    let style = MarkdownStyle::default();
    let cold = fingerprint(&ctx, doc, scroll, &style);
    let warm = fingerprint(&ctx, doc, scroll, &style);
    assert_eq!(cold, warm, "hot-cache frame must paint byte-identical output:\n{doc}");
    assert!(warm.1.iter().any(|(_, text)| !text.is_empty()), "fixture must paint something:\n{doc}");
  }
}

/// The same documents render under dark and light visuals without panicking, on
/// both render paths, with cold and hot caches, painting the visible content.
#[test]
fn dark_and_light_visuals_render_without_panic() {
  for (dark_mode, label) in [(true, "dark"), (false, "light")] {
    // Segmented path: a visible table plus scrolling code blocks, cold then hot.
    let ctx = Context::default();
    let style = MarkdownStyle { segmentation_admission: 3, ..MarkdownStyle::default() };
    let doc = format!("| a | b |\n|---|---|\n| 1 | 2 |\n{}{}", fence("mk-one", 30), fence("mk-two", 30));
    for frame in 0..2 {
      let painted_now = paint(&ctx, &doc, true, &style, dark_mode);
      assert!(painted(&painted_now, "1"), "visuals={label} frame={frame} lost the visible table");
      assert!(painted(&painted_now, "mk-one"), "visuals={label} frame={frame} lost the first fence");
    }

    // Whole-document path under the default style.
    let plain_ctx = Context::default();
    let plain = "Intro.\n\n```rust\nfn main() {}\n```\n\nOutro.";
    let painted_plain = paint(&plain_ctx, plain, false, &MarkdownStyle::default(), dark_mode);
    assert!(painted(&painted_plain, "Outro"), "visuals={label} lost the plain outro");
  }
}
