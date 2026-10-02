use egui::{vec2, Color32, Context, FontId, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{layout, parse, MarkdownLabel, MarkdownStyle};

/// Build `doc` at `body_size` under `style`, returning the (min_y, max_y, text) of
/// every laid-out row plus the total galley height, in order.
fn galley_rows(doc: &str, body_size: f32, style: &MarkdownStyle) -> (Vec<(f32, f32, String)>, f32) {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let mut rows = Vec::new();
  let mut total = 0.0;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    let md = parse(doc);
    let built = layout::build_layout(
      &mut child,
      &md.tokens,
      FontId::proportional(body_size),
      Color32::WHITE,
      None,
      screen.width(),
      false,
      None,
      false,
      style,
      Default::default(),
    );
    let galley = child.ctx().fonts_mut(|f| f.layout_job(built.job));
    for row in &galley.rows {
      rows.push((row.min_y(), row.max_y(), row.row.text().to_owned()));
    }
    total = galley.size().y;
  });
  output.textures_delta.clear();
  (rows, total)
}

/// The (font size, line height) of every layout section, in order
/// (same probe as `line_height.rs`, which covers the row-height side).
fn section_rows(doc: &str, body_size: f32, style: &MarkdownStyle) -> Vec<(f32, Option<f32>)> {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let mut rows = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    let md = parse(doc);
    let built = layout::build_layout(
      &mut child,
      &md.tokens,
      FontId::proportional(body_size),
      Color32::WHITE,
      None,
      screen.width(),
      false,
      None,
      false,
      style,
      Default::default(),
    );
    for section in &built.job.sections {
      rows.push((section.format.font_id.size, section.format.line_height));
    }
  });
  output.textures_delta.clear();
  rows
}

/// Height of the whole `MarkdownLabel` widget (cache, segmentation, and all)
/// rendered once into a fresh context.
fn label_height(doc: &str, style: &MarkdownStyle) -> f32 {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let mut allocated = 0.0;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    MarkdownLabel::new(Id::new("heading_spacing"), doc).style(style).show(&mut child);
    allocated = child.min_rect().height();
  });
  output.textures_delta.clear();
  allocated
}

/// The first row whose text is a single space, i.e. a heading spacer row.
fn spacer_row(rows: &[(f32, f32, String)]) -> Option<(f32, f32)> {
  rows.iter().find(|(_, _, text)| text == " ").map(|(a, b, _)| (*a, *b))
}

fn close(a: f32, b: f32) -> bool {
  (a - b).abs() < 0.01
}

#[test]
fn default_heading_scales_and_ratios_match_shipped_values() {
  let style = MarkdownStyle::default();
  assert_eq!(style.heading.scales, [2.0, 1.55, 1.30, 1.15, 1.08, 1.0]);
  // The body rhythm is unchanged: this module deliberately ships with the previous
  // body line height; only the heading hierarchy above it moves.
  assert_eq!(style.line_height_ratio, 1.30);
  assert_eq!(style.block_spacing, 8.0);
  assert_eq!(style.heading_space_above, 4.0);
  // Adjacent levels stay distinguishable (the old tail of 1.1/1.05/1.0 packed
  // H4–H6 within 0.6pt of each other).
  for window in style.heading.scales.windows(2) {
    assert!(window[0] - window[1] >= 0.03, "adjacent heading scales {window:?} are too close");
  }
}

#[test]
fn heading_levels_render_at_the_shipped_scales() {
  let style = MarkdownStyle::default();
  for (i, scale) in style.heading.scales.iter().enumerate() {
    // H6 shares the body size by design (it is *the* body-size level), so it has no
    // renderable signature of its own; its separation from H5 is pinned by the
    // adjacent-scale assertion in the defaults test above.
    if *scale <= 1.0 {
      continue;
    }
    let doc = format!("{} heading\n\nbody text", "#".repeat(i + 1));
    let rows = section_rows(&doc, 13.0, &style);
    let expected_size = 13.0 * scale;
    let heading = rows
      .iter()
      .find(|(size, _)| close(*size, expected_size))
      .unwrap_or_else(|| panic!("no section at the H{} size {expected_size}", i + 1));
    assert!(
      close(heading.1.unwrap_or(0.0), expected_size * style.line_height_ratio),
      "H{} row height {:?} should follow its font size {expected_size}",
      i + 1,
      heading.1
    );
  }
}

#[test]
fn spacer_row_above_a_mid_document_heading_measures_the_style_fields() {
  let doc = "Body one.\n\n# Title\n\nTail.";
  for (heading_space, expected) in [(0.0, 8.0), (4.0, 12.0), (40.0, 48.0)] {
    let style = MarkdownStyle { heading_space_above: heading_space, ..Default::default() };
    let (rows, _) = galley_rows(doc, 13.0, &style);
    let spacer = spacer_row(&rows).unwrap_or_else(|| panic!("heading_space={heading_space}: no spacer row"));
    let height = spacer.1 - spacer.0;
    assert!(
      (height - expected).abs() < 0.5,
      "heading_space={heading_space}: spacer row is {height}px, want {expected}px (block_spacing + heading_space_above)"
    );
    // The spacer sits between the blank line after the preceding paragraph and the
    // heading row itself.
    let heading_row = rows.iter().find(|(_, _, text)| text == "Title").expect("heading row");
    assert!(spacer.0 < heading_row.0, "spacer must sit above the heading row");
  }
}

#[test]
fn total_height_grows_by_the_spacer_delta_per_heading() {
  let two_headings = "Body one.\n\n## Section\n\nBody two.\n\n# Top\n\nTail.";
  let one_heading = "Body one.\n\n# Top\n\nTail.";
  let delta = 36.0_f32;
  for doc in [two_headings, one_heading] {
    let headings = if doc == two_headings { 2.0 } else { 1.0 };
    let low = MarkdownStyle { heading_space_above: 4.0, ..Default::default() };
    let high = MarkdownStyle { heading_space_above: 4.0 + delta, ..Default::default() };
    let (_, low_h) = galley_rows(doc, 13.0, &low);
    let (_, high_h) = galley_rows(doc, 13.0, &high);
    let grew = high_h - low_h;
    assert!(
      (grew - headings * delta).abs() < 0.5,
      "{doc:?}: height grew {grew}px for {headings} headings and a {delta}px delta, want {}",
      headings * delta
    );
  }
}

#[test]
fn heading_at_document_start_gets_no_spacer_row() {
  let doc = "# Heading at doc start\n\nBody";
  for heading_space in [0.0, 4.0, 40.0] {
    let style = MarkdownStyle { heading_space_above: heading_space, ..Default::default() };
    let (rows, total) = galley_rows(doc, 13.0, &style);
    assert!(
      spacer_row(&rows).is_none(),
      "heading_space={heading_space}: a doc-start heading must not grow a spacer row"
    );
    assert_eq!(rows.len(), 3, "heading_space={heading_space}: row count {} changed with the style field", rows.len());
    assert!((total - 68.0).abs() < 1.0, "heading_space={heading_space}: total {total} must not depend on the field");
  }
}

#[test]
fn body_only_document_height_is_invariant_to_heading_space() {
  // The veto line for this feature: body text must render pixel-identically
  // whatever the heading breathing room is set to.
  let doc = "Just body, no heading.";
  for heading_space in [0.0, 4.0, 40.0] {
    let style = MarkdownStyle { heading_space_above: heading_space, ..Default::default() };
    let (rows, total) = galley_rows(doc, 13.0, &style);
    assert_eq!(rows.len(), 1, "heading_space={heading_space}: body-only doc grew extra rows");
    assert!((total - 17.0).abs() < 0.5, "heading_space={heading_space}: body-only doc is {total}px tall, want 17px");
    // Same through the whole-widget path (caching and galley allocation included).
    let via_label = label_height(doc, &style);
    assert!(
      (via_label - 17.0).abs() < 1.0,
      "heading_space={heading_space}: label height {via_label}px for a body-only doc, want ~17px"
    );
  }
}

#[test]
fn whole_label_height_follows_heading_space_in_a_code_block_document() {
  // A plain fenced code block (no scrolling, no link handler) does not trigger
  // segmentation, so this document renders through the whole-document galley
  // path — the path most host apps use for ordinary prose + code documents.
  let doc = "Body one.\n\n```rust\nfn main() {}\n```\n\n# Title\n\nTail.";
  let low = MarkdownStyle { heading_space_above: 4.0, ..Default::default() };
  let high = MarkdownStyle { heading_space_above: 4.0 + 40.0, ..Default::default() };
  let grew = label_height(doc, &high) - label_height(doc, &low);
  assert!(
    (grew - 40.0).abs() < 0.5,
    "code-block document: label height grew {grew}px for one heading and a 40px delta, want 40px"
  );
}

#[test]
fn table_document_height_follows_heading_space_through_segmented_flushing() {
  // The table forces segmentation, so the heading is laid out inside a flushed
  // range of its own; the spacer must engage there too.
  let doc = "Intro\n\n# Title\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nTail.";
  let low = MarkdownStyle { heading_space_above: 4.0, ..Default::default() };
  let high = MarkdownStyle { heading_space_above: 4.0 + 40.0, ..Default::default() };
  let grew = label_height(doc, &high) - label_height(doc, &low);
  assert!(
    (grew - 40.0).abs() < 0.5,
    "table document: label height grew {grew}px for one heading and a 40px delta, want 40px"
  );
}

#[test]
fn a_heading_directly_after_a_block_keeps_plain_block_spacing() {
  // After a block element, `render_token_range` skips the newlines and hands the
  // heading to `flush_text_range` as the first token of its range, so the spacer
  // deliberately does not fire: block-to-heading already gets `block_spacing`,
  // exactly like any other block.
  let doc = "Intro\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n# Title\n\nTail.";
  let low = MarkdownStyle { heading_space_above: 4.0, ..Default::default() };
  let high = MarkdownStyle { heading_space_above: 4.0 + 40.0, ..Default::default() };
  let grew = label_height(doc, &high) - label_height(doc, &low);
  assert!(grew.abs() < 0.5, "heading directly after a table must not grow extra space with the field (grew {grew}px)");
}

#[test]
fn a_heading_with_inline_spans_emits_exactly_one_spacer_row() {
  // `# **Bold** and plain` splits into several Text tokens that all carry the
  // heading style; only the first (whose predecessor is a newline) may emit the
  // spacer.
  let doc = "Body one.\n\n# **Bold** and plain\n\nTail.";
  let style = MarkdownStyle::default();
  let (rows, _) = galley_rows(doc, 13.0, &style);
  let spacers = rows.iter().filter(|(_, _, text)| text == " ").count();
  assert_eq!(spacers, 1, "heading with inline spans emitted {spacers} spacer rows, want exactly 1");
}
