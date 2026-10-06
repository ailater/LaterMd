use egui::{vec2, Color32, Context, FontId, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{layout, MarkdownLabel, MarkdownStyle};

/// Render `text` into `ctx` and return every string painted as text.
fn painted_text(ctx: &Context, text: &str) -> Vec<String> {
  painted_text_with(ctx, text, false)
}

fn painted_text_with(ctx: &Context, text: &str, scroll_code_blocks: bool) -> Vec<String> {
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    MarkdownLabel::new(Id::new("test"), text).scroll_code_blocks(scroll_code_blocks).show(&mut child);
  });

  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  output.textures_delta.clear();
  out
}

/// Same as [`painted_text`], on a viewport tall enough that no range is culled.
fn painted_text_tall(ctx: &Context, text: &str) -> Vec<String> {
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 12000.0));
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    MarkdownLabel::new(Id::new("test-tall"), text).wrap().show(&mut child);
  });

  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  output.textures_delta.clear();
  out
}

fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
  match shape {
    egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
    _ => {}
  }
}

/// Editing a word must be reflected even when the edit does not change the token count.
#[test]
fn edit_within_segmented_doc_is_reflected() {
  // A table forces the segmented render path, which caches each flush range separately.
  let doc = |word: &str| format!("Intro {word} paragraph.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nOutro.");

  let ctx = Context::default();
  let before = painted_text(&ctx, &doc("alpha"));
  assert!(before.iter().any(|t| t.contains("alpha")), "first render missing 'alpha': {before:?}");

  let after = painted_text(&ctx, &doc("bravo"));
  assert!(after.iter().any(|t| t.contains("bravo")), "edited text not re-rendered: {after:?}");
  assert!(!after.iter().any(|t| t.contains("alpha")), "stale galley still painted: {after:?}");
}

/// Same, for an edit after the block element (a later flush range).
#[test]
fn edit_after_block_is_reflected() {
  let doc = |word: &str| format!("Intro.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nOutro {word} here.");

  let ctx = Context::default();
  let before = painted_text(&ctx, &doc("alpha"));
  assert!(before.iter().any(|t| t.contains("alpha")), "first render missing 'alpha': {before:?}");

  let after = painted_text(&ctx, &doc("bravo"));
  assert!(after.iter().any(|t| t.contains("bravo")), "edited text not re-rendered: {after:?}");
  assert!(!after.iter().any(|t| t.contains("alpha")), "stale galley still painted: {after:?}");
}

/// Editing the body of a scrolling fence must invalidate the code-block galley cache.
#[test]
fn edit_within_scrolling_code_block_is_reflected() {
  let doc = |word: &str| format!("Intro.\n\n```rust\nfn main() {{\n    let x = \"{word}\";\n}}\n```\n\nOutro.");

  let ctx = Context::default();
  let before = painted_text_with(&ctx, &doc("alpha"), true);
  assert!(before.iter().any(|t| t.contains("alpha")), "first render missing 'alpha': {before:?}");

  let after = painted_text_with(&ctx, &doc("bravo"), true);
  assert!(after.iter().any(|t| t.contains("bravo")), "edited fence not re-rendered: {after:?}");
  assert!(!after.iter().any(|t| t.contains("alpha")), "stale code galley still painted: {after:?}");
}

/// Growing a scrolling fence mid-line then completing the line must paint the new tail.
#[test]
fn streaming_append_scrolling_code_block_is_reflected() {
  let ctx = Context::default();

  let partial = "Intro.\n\n```rust\nfn main() {\n    let x = \"alp\n```\n";
  // heal closes the fence; body is still the incomplete string inside.
  let mid = painted_text_with(&ctx, partial, true);
  assert!(mid.iter().any(|t| t.contains("alp")), "partial stream missing 'alp': {mid:?}");
  assert!(!mid.iter().any(|t| t.contains("alpha")), "partial stream should not yet contain 'alpha': {mid:?}");

  let grown = "Intro.\n\n```rust\nfn main() {\n    let x = \"alpha\";\n}\n```\n";
  let after = painted_text_with(&ctx, grown, true);
  assert!(after.iter().any(|t| t.contains("alpha")), "appended stream missing 'alpha': {after:?}");
}

/// A non-prefix edit of a scrolling fence must rebuild rather than keep a stale galley.
#[test]
fn non_prefix_edit_scrolling_code_block_is_reflected() {
  let ctx = Context::default();

  let first = "```rust\nfn alpha() {}\n```\n";
  let before = painted_text_with(&ctx, first, true);
  assert!(before.iter().any(|t| t.contains("alpha")), "first render missing 'alpha': {before:?}");

  let second = "```rust\nfn bravo() {}\n```\n";
  let after = painted_text_with(&ctx, second, true);
  assert!(after.iter().any(|t| t.contains("bravo")), "non-prefix edit missing 'bravo': {after:?}");
  assert!(!after.iter().any(|t| t.contains("alpha")), "stale fence still painted: {after:?}");
}

/// `needs_segmentation` decides the render path without laying out; it must agree with the
/// segment breaks `build_layout` would have produced.
#[test]
fn needs_segmentation_matches_build_layout() {
  let docs = [
    "Just **plain** text with `code` and a [link](https://example.com).",
    "# Heading\n\nParagraph.\n\n---\n\nAfter the rule.",
    "- one\n- two\n  - nested\n\n1. first\n2. second",
    "Intro.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nOutro.",
    "> quoted **text**\n>\n> > nested",
    "Text\n\n```rust\nfn main() {}\n```\n\nMore.",
    "![alt](https://example.com/x.png)",
    "A footnote[^1].\n\n[^1]: body.",
    "- [x] done\n- [ ] todo",
  ];

  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let style = MarkdownStyle::default();

  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    for doc in docs {
      for scroll_code_blocks in [false, true] {
        let md = egui_markdown::parse(doc);
        let predicted = layout::needs_segmentation(&md.tokens, scroll_code_blocks, None, &style);
        let built = layout::build_layout(
          ui,
          &md.tokens,
          FontId::proportional(14.0),
          Color32::WHITE,
          None,
          ui.available_width(),
          false,
          None,
          scroll_code_blocks,
          true,
          true,
          &style,
          Default::default(),
        );
        assert_eq!(
          predicted,
          !built.segment_breaks.is_empty(),
          "scroll_code_blocks={scroll_code_blocks} disagreement on:\n{doc}"
        );
      }
    }
  });
  output.textures_delta.clear();
}

/// A repeated frame of an unchanged segmented document hits the flush ranges'
/// shaped-galley cache: what is painted and the anchors recorded must be
/// byte-identical to the frame that shaped them.
#[test]
fn repeat_frame_reuses_shaped_galley_identically() {
  use egui_markdown::section_anchors;
  let body: String = (0..520).map(|i| format!("    let step_{i} = log.tail()?;\n")).collect();
  let doc = format!("Intro.\n\n```rust\nfn claim() {{\n{body}}}\n```\n\nOutro.");

  let ctx = Context::default();
  let id = Id::new("shaped-repeat");
  // Tall enough that no flush range is ever viewport-culled: culling is a warm-frame
  // behavior and would legitimately paint fewer shapes on the second frame.
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 12000.0));
  let mut frames = Vec::new();
  for _ in 0..2 {
    let mut anchors = None;
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
      MarkdownLabel::new(id, &doc).wrap().show(&mut child);
      anchors = section_anchors(&child, id);
    });
    let mut painted = Vec::new();
    for clipped in &output.shapes {
      collect(&clipped.shape, &mut painted);
    }
    output.textures_delta.clear();
    frames.push((painted, anchors.unwrap_or_default()));
  }

  let (first, second) = (&frames[0], &frames[1]);
  assert!(!first.0.is_empty(), "nothing painted on the shaping frame");
  assert_eq!(first.0, second.0, "cached-galley frame painted different text");
  assert_eq!(first.1, second.1, "cached-galley frame recorded different anchors");
}

/// Changing the wrap width must re-shape the cached flush ranges: the anchors
/// after a resize must match a context that has only ever seen the new width —
/// a stale shaped galley would keep the old geometry.
#[test]
fn wrap_change_reshapes_cached_galley() {
  use egui_markdown::section_anchors;
  let paragraph = "长段落需要换行来检验宽度变化确实改变了排版:".to_string()
    + &"雾凇沆沆沆沆 ".repeat(40)
    + "\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n尾段 another wrapping sensitive line of latin text.\n";

  let render_at = |ctx: &Context, width: f32| -> Vec<egui_markdown::SectionAnchor> {
    let id = Id::new("shaped-resize");
    let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(width, 2000.0));
    let mut anchors = None;
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
      MarkdownLabel::new(id, &paragraph).wrap().show(&mut child);
      anchors = section_anchors(&child, id);
    });
    output.textures_delta.clear();
    anchors.unwrap_or_default()
  };

  let resized = Context::default();
  let wide = render_at(&resized, 500.0);
  let _narrow_first = render_at(&resized, 220.0);
  let narrow_second = render_at(&resized, 220.0);

  let fresh = Context::default();
  let narrow_fresh = render_at(&fresh, 220.0);

  assert!(!wide.is_empty() && !narrow_second.is_empty());
  // The wrap change must actually matter for geometry: some anchor moved.
  let wide_bottom = wide.iter().map(|a| a.y).fold(0.0f32, f32::max);
  let narrow_bottom = narrow_second.iter().map(|a| a.y).fold(0.0f32, f32::max);
  assert!(
    (wide_bottom - narrow_bottom).abs() > 1.0,
    "wrap change did not change layout: {wide_bottom} vs {narrow_bottom}"
  );
  assert_eq!(narrow_second, narrow_fresh, "resized context kept stale galley geometry");
}

/// Appending a line to an admitted (non-scrolling) fence must paint the new
/// line — the fence's flush range is rebuilt while other ranges reuse their
/// shaped galleys.
#[test]
fn streaming_append_to_admitted_fence_is_reflected() {
  let body = |lines: usize| {
    let inner: String = (0..lines).map(|i| format!("    let step_{i} = log.tail()?;\n")).collect();
    format!("Intro.\n\n```rust\nfn claim() {{\n{inner}}}\n```\n\nOutro.")
  };

  let ctx = Context::default();
  let before = painted_text_tall(&ctx, &body(520));
  assert!(before.iter().any(|t| t.contains("step_519")), "first render missing tail line: {before:?}");

  let after = painted_text_tall(&ctx, &body(521));
  assert!(after.iter().any(|t| t.contains("step_520")), "appended line not painted: {after:?}");
  assert!(after.iter().any(|t| t.contains("Intro.")) && after.iter().any(|t| t.contains("Outro.")));
}
