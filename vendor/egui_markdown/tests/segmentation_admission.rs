use egui::{vec2, Color32, Context, FontId, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{block_span_rects, layout, parse, MarkdownLabel, MarkdownStyle, Token};

fn screen() -> Rect {
  Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0))
}

/// A rust fence whose body is exactly `body_lines` lines (for `body_lines >= 1`).
fn fence(body_lines: usize) -> String {
  let mut doc = String::from("```rust\nfn claim(log: &Log, seq: u64) -> Result<(), ClaimError> {\n");
  for _ in 0..body_lines.saturating_sub(1) {
    doc.push_str("    let step = log.tail()?;\n");
  }
  doc.push_str("```\n");
  doc
}

fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
  match shape {
    egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
    _ => {}
  }
}

/// Render `doc` under `style` and return the text of every painted galley, in
/// paint order. A whole-document render paints one galley; the segmented path
/// paints one per flushed range.
fn paint(doc: &str, style: &MarkdownStyle) -> Vec<String> {
  let ctx = Context::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).style(style).show(&mut child);
  });
  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  output.textures_delta.clear();
  out
}

/// The admission threshold is measured in fence line count and only ever admits
/// fences: plain text of any length stays on the whole-document path, and the
/// default of 500 draws the line precisely at 500 body lines.
#[test]
fn admission_threshold_decides_path_by_fence_lines() {
  let default = MarkdownStyle::default();

  // Exactly at and above the default threshold: segmented path.
  for lines in [500usize, 501, 10_000] {
    let doc = fence(lines);
    let md = parse(&doc);
    assert!(
      layout::needs_segmentation(&md.tokens, false, None, &default),
      "{lines}-line fence must be admitted at the default threshold"
    );
  }
  // One line below: whole-document path.
  let just_below = fence(499);
  let md = parse(&just_below);
  assert!(
    !layout::needs_segmentation(&md.tokens, false, None, &default),
    "a 499-line fence must stay on the whole-document path at the default threshold"
  );

  // Plain prose of any length never triggers admission by itself.
  let long_prose = "a paragraph line\n\n".repeat(600);
  let md = parse(&long_prose);
  assert!(!layout::needs_segmentation(&md.tokens, false, None, &default));

  // 0 admits every fence; usize::MAX disables admission entirely.
  let admit_all = MarkdownStyle { segmentation_admission: 0, ..Default::default() };
  let md = parse("```rust\nfn main() {}\n```\n");
  assert!(layout::needs_segmentation(&md.tokens, false, None, &admit_all));

  let admit_none = MarkdownStyle { segmentation_admission: usize::MAX, ..Default::default() };
  let huge = fence(10_000);
  let md = parse(&huge);
  assert!(!layout::needs_segmentation(&md.tokens, false, None, &admit_none));
}

/// Default style must keep ordinary documents on the whole-document galley path:
/// one painted galley carrying the entire document. This is the pixel-identity
/// veto line for the default style — the whole-document path is untouched code,
/// and staying on it is what makes ordinary documents render identically.
#[test]
fn default_style_keeps_ordinary_documents_on_whole_document_galley() {
  let docs = [
    "Intro paragraph.\n\n```rust\nfn main() {}\n```\n\nOutro paragraph.",
    "# Heading\n\nBody with `code` and **bold**.\n\n```python\nx = 1\ny = 2\n```\n\nThe end.",
    "- one\n- two\n  - nested\n\n```\nplain fence\n```",
    "Text before.\n\n```rust\nfn a() {}\n```\n\nMiddle.\n\n```rust\nfn b() {}\n```\n\nText after.",
  ];

  for doc in docs {
    let galleys = paint(doc, &MarkdownStyle::default());
    assert_eq!(galleys.len(), 1, "default style must paint one whole-document galley:\n{doc}");
    let joined: String = galleys.concat();
    for needle in ["Intro", "fn main", "Outro", "Heading", "fn a", "fn b"] {
      if doc.contains(needle) {
        assert!(joined.contains(needle), "galley text missing {needle:?}:\n{doc}");
      }
    }
  }
}

/// Over the threshold the document takes the segmented path: the fence renders
/// as its own flushed range (its own galley, same in-galley shape), the block
/// table still covers the whole document, and a second frame — which reaches
/// the per-range caches — renders identically without tripping the
/// segment-break consistency asserts.
#[test]
fn admitted_fence_renders_as_its_own_flushed_range() {
  let style = MarkdownStyle { segmentation_admission: 3, ..Default::default() };
  let doc = format!("Intro paragraph.\n\n{}Outro paragraph.", fence(6));

  let id = Id::new("test");
  let ctx = Context::default();
  let mut galleys = Vec::new();
  let mut blocks = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, &doc).style(&style).show(&mut child);
    blocks = block_span_rects(&child, id);
  });
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut galleys);
  }
  output.textures_delta.clear();

  assert!(galleys.len() >= 3, "intro, fence and outro must paint as separate galleys, got {}", galleys.len());
  let joined = galleys.concat();
  assert!(joined.contains("Intro paragraph"), "intro missing: {joined:?}");
  assert!(joined.contains("fn claim"), "fence body missing: {joined:?}");
  assert!(joined.contains("Outro paragraph"), "outro missing: {joined:?}");

  let blocks = blocks.expect("block table recorded");
  assert_eq!(blocks.first().map(|b| b.span.start), Some(0), "table must start at the document start");
  assert_eq!(blocks.last().map(|b| b.span.end), Some(doc.len()), "table must cover the document end");
  // The fence itself is one recorded block: a span whose text is exactly the fence.
  let md = parse(&doc);
  let fence_span = md
    .tokens
    .iter()
    .zip(&md.spans)
    .find_map(|(token, span)| matches!(token, Token::CodeBlock { .. }).then(|| span.clone()))
    .expect("fence token present");
  assert!(
    blocks.iter().any(|b| b.span == fence_span),
    "the admitted fence must appear as its own block ({fence_span:?}): {blocks:?}"
  );

  // Second frame on the same context: per-range caches are hot; the consistency
  // asserts in `flush_text_range` must hold on cache hits too.
  let mut second = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, &doc).style(&style).show(&mut child);
  });
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut second);
  }
  output.textures_delta.clear();
  assert_eq!(galleys.len(), second.len(), "cached frame must paint the same galley count");
}

/// `needs_segmentation` must agree with the breaks `build_layout` reports for
/// the same tokens across the whole admission range, and the single-fence range
/// an admitted fence renders as must lay the fence out inline (no break).
#[test]
fn needs_segmentation_matches_build_layout_across_admission_thresholds() {
  let docs: Vec<String> = vec![
    "Just **plain** text with `code` and a [link](https://example.com).".to_owned(),
    "Text\n\n```rust\nfn main() {}\n```\n\nMore.".to_owned(),
    fence(3),
    fence(6),
    format!("Intro.\n\n{}Outro.", fence(5)),
  ];

  let ctx = Context::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    for admission in [0usize, 3, 5, 500, usize::MAX] {
      let style = MarkdownStyle { segmentation_admission: admission, ..Default::default() };
      for doc in &docs {
        let width = child.available_width();
        let md = parse(doc);
        let predicted = layout::needs_segmentation(&md.tokens, false, None, &style);
        let built = layout::build_layout(
          &mut child,
          &md.tokens,
          FontId::proportional(14.0),
          Color32::WHITE,
          None,
          width,
          false,
          None,
          false,
          true,
          &style,
          Default::default(),
        );
        assert_eq!(predicted, !built.segment_breaks.is_empty(), "admission={admission} disagreement on:\n{doc}");

        // An admitted fence, laid out as its own single-token range, renders
        // inline: no segment break, and real text in the job.
        for (index, token) in md.tokens.iter().enumerate() {
          if let Token::CodeBlock { text, .. } = token {
            if layout::code_block_admits_segmentation(text, &style) {
              let single = layout::build_layout(
                &mut child,
                &md.tokens[index..index + 1],
                FontId::proportional(14.0),
                Color32::WHITE,
                None,
                width,
                false,
                None,
                false,
                false,
                &style,
                Default::default(),
              );
              assert!(single.segment_breaks.is_empty(), "single-fence range must not break again");
              assert!(!single.job.text.is_empty(), "single-fence range must lay out the fence body");
              assert_eq!(single.code_block_info.len(), 1, "fence must keep its copy affordance");
            }
          }
        }
      }
    }
  });
  output.textures_delta.clear();
}

/// Appending a line inside an admitted fence — the streaming case the admission
/// exists for — is reflected on the next frame, as is an edit of the plain text
/// around it (each flushed range owns its cache entry).
#[test]
fn appending_to_admitted_fence_is_reflected() {
  let style = MarkdownStyle { segmentation_admission: 3, ..Default::default() };
  let doc = |body_lines: usize, word: &str| format!("Intro {word}.\n\n{}Outro.", fence(body_lines));

  let ctx = Context::default();
  let paint = |doc: &str| -> Vec<String> {
    let mut galleys = Vec::new();
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
      MarkdownLabel::new(Id::new("test"), doc).style(&style).show(&mut child);
    });
    for clipped in &output.shapes {
      collect(&clipped.shape, &mut galleys);
    }
    output.textures_delta.clear();
    galleys
  };

  let before = paint(&doc(6, "alpha"));
  assert!(before.concat().contains("alpha"), "intro missing on first frame");
  assert_eq!(before.concat().matches("let step = log.tail()?;").count(), 5, "a 6-line fence has 5 step lines");

  let grown = paint(&doc(7, "bravo"));
  let joined = grown.concat();
  assert!(joined.contains("bravo"), "edited intro not reflected: {joined:?}");
  assert!(!joined.contains("alpha"), "stale intro still painted: {joined:?}");
  assert_eq!(
    joined.matches("let step = log.tail()?;").count(),
    6,
    "appended line missing from the grown fence: {joined:?}"
  );
}

/// A fence that sits right below the threshold keeps the whole-document galley
/// even when the document also has plain text around it — the threshold applies
/// per fence, and the render path must not flicker for the prose around it.
#[test]
fn fence_just_below_threshold_stays_whole_document() {
  let style = MarkdownStyle { segmentation_admission: 6, ..Default::default() };
  let doc = format!("Intro.\n\n{}Outro.", fence(5));

  let galleys = paint(&doc, &style);
  assert_eq!(galleys.len(), 1, "a 5-line fence under a threshold of 6 must stay whole-document");
  let joined = galleys.concat();
  assert!(joined.contains("Intro.") && joined.contains("fn claim") && joined.contains("Outro."));
}

/// Adjoining admitted fences with no prose between them — the degenerate slicing
/// case (empty leading flush, back-to-back single-fence ranges) — render fully.
#[test]
fn adjoining_admitted_fences_render_without_prose_between() {
  let style = MarkdownStyle { segmentation_admission: 3, ..Default::default() };
  let doc = format!("{}{}", fence(4), fence(4));

  let galleys = paint(&doc, &style);
  let joined = galleys.concat();
  assert_eq!(joined.matches("fn claim(log: &Log").count(), 2, "both fences must paint their bodies: {joined:?}");
  assert_eq!(
    joined.matches("let step = log.tail()?;").count(),
    6,
    "both 4-line fences must paint all 3 step lines each"
  );
}
