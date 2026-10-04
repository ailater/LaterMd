use egui::{pos2, vec2, Context, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{section_anchors, SectionAnchor};

fn screen() -> Rect {
  Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 2000.0))
}

/// Render `text` once and read back the anchors recorded under `id`.
fn anchors_for(text: &str) -> Vec<SectionAnchor> {
  let ctx = Context::default();
  let id = Id::new("anchors");
  let mut read = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    read = section_anchors(&child, id);
  });
  output.textures_delta.clear();
  read.unwrap_or_default()
}

use egui_markdown::MarkdownLabel;

/// Every section of a multi-paragraph document gets an anchor, in byte order,
/// with non-decreasing y — that is the contract an outline pane scrolls against.
#[test]
fn every_section_gets_an_anchor_in_document_order() {
  let text = "# One\n\nbody one\n\n## Two\n\nbody two\n";
  let anchors = anchors_for(text);
  assert!(!anchors.is_empty(), "no anchors recorded");

  for anchor in &anchors {
    assert!(anchor.byte_start <= text.len(), "{anchor:?} out of range");
  }
  // Monotonic in both dimensions: later text is never above earlier text
  for pair in anchors.windows(2) {
    assert!(pair[1].byte_start >= pair[0].byte_start, "{anchors:?}");
    assert!(pair[1].y >= pair[0].y, "{anchors:?}");
  }
}

/// Content between two headings pushes the second one further down: that is the
/// whole point of recording geometry instead of guessing from byte ratios.
#[test]
fn later_headings_sit_lower_than_earlier_ones() {
  let short = anchors_for("# A\n\n# B\n");
  // 用真实文字而不是空行:Markdown 会把连续空行折叠成一个段落间距
  let padded = anchors_for(&format!("# A\n\n{}# B\n", "正文行\n".repeat(20)));
  assert!(short.len() >= 2 && padded.len() >= 2, "{short:?} {padded:?}");
  assert!(padded.last().unwrap().y > padded.first().unwrap().y);
  assert!(padded.last().unwrap().y > short.last().unwrap().y, "中间内容应把后面的标题推下去");
}

/// Unrendered label: no anchors, and reading them does not panic.
#[test]
fn missing_label_yields_no_anchors() {
  let ctx = Context::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    assert!(section_anchors(ui, Id::new("never-rendered")).is_none());
  });
  output.textures_delta.clear();
}

/// Oracle for the single-sweep anchor computation: the straightforward
/// per-section algorithm this crate shipped before the sweep rewrite (each
/// section re-counted its text prefix and re-walked the rows from the top). It
/// is O(sections × rows) and only lives here now — the production path must
/// stay equivalent to it, section for section, row for row.
fn reference_anchors(galley: &egui::epaint::text::Galley) -> Vec<SectionAnchor> {
  let mut anchors = Vec::with_capacity(galley.job.sections.len());
  for (index, section) in galley.job.sections.iter().enumerate() {
    let byte_start = section.byte_range.start.0;
    let char_start = galley.job.text[..byte_start.min(galley.job.text.len())].chars().count();
    let mut y = f32::NAN;
    let mut char_cursor = 0usize;
    for row in &galley.rows {
      let row_len = row.row.glyphs.len();
      if char_start < char_cursor + row_len.max(1) {
        y = row.pos.y;
        break;
      }
      char_cursor += row_len.max(1);
    }
    if y.is_nan() && index + 1 == galley.job.sections.len() {
      y = galley.rect.bottom();
    }
    if !y.is_nan() {
      anchors.push(SectionAnchor { byte_start, y });
    }
  }
  anchors
}

/// The recorded anchors must equal the per-section oracle walk on the same
/// galley: the production computation is a single forward sweep for speed, and
/// any divergence in edge cases (empty rows, multibyte text, trailing sections,
/// many-section fences) shows up here as a mismatch.
#[test]
fn recorded_anchors_equal_reference_walk() {
  let fence_120 = format!(
    "```rust\n{}\n```\n",
    (0..120).map(|i| format!("    let step_{i} = log.tail()?;")).collect::<Vec<_>>().join("\n")
  );
  let docs = [
    "# 标题一\n\n正文,中文多字节 chars。\n\n## 二\n\n```rust\nfn a() {}\nfn b() {}\n```\n\n尾声\n".to_string(),
    "a\n\n\nb\n\n\n\nc\n".to_string(),
    "中文一\n\n中文二\n".to_string(),
    fence_120,
  ];
  for text in &docs {
    let ctx = Context::default();
    let id = Id::new("anchors-equiv");
    let mut recorded = None;
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
      MarkdownLabel::new(id, text).wrap().show(&mut child);
      recorded = section_anchors(&child, id);
      // Same text through the public layout path: same wrap and style in the
      // same frame produce the identical cached galley for the oracle to walk.
      let (_pos, galley, _response) = MarkdownLabel::new(id, text).wrap().layout_in_ui(&mut child);
      let expected = reference_anchors(&galley);
      let recorded = recorded.take().unwrap_or_default();
      assert!(!recorded.is_empty(), "no anchors recorded for {text:?}");
      assert_eq!(recorded.len(), expected.len(), "anchor count mismatch for {text:?}");
      for (a, b) in recorded.iter().zip(&expected) {
        assert_eq!(a.byte_start, b.byte_start, "byte_start mismatch for {text:?}");
        assert_eq!(a.y, b.y, "y mismatch for {text:?}");
      }
    });
    output.textures_delta.clear();
  }
}

/// A fence past `segmentation_admission` renders as its own flush range with
/// roughly one section per highlighted code line: every line keeps its own
/// anchor row and the byte/y ordering stays monotone. This is the streaming
/// shape — a row walk that collapsed or skipped would emit anchors that share
/// rows or lose lines.
#[test]
fn admitted_fence_anchors_track_each_line() {
  let body: String = (0..520).map(|i| format!("    let step_{i} = log.tail()?;\n")).collect();
  let text = format!("```rust\nfn claim() {{\n{body}}}\n```\n");
  let anchors = anchors_for(&text);
  assert!(anchors.len() > 500, "{} anchors — expected ~one per highlighted range", anchors.len());
  for pair in anchors.windows(2) {
    assert!(pair[1].byte_start >= pair[0].byte_start, "{anchors:?}");
    assert!(pair[1].y >= pair[0].y, "{anchors:?}");
  }
  let distinct_rows = anchors.iter().map(|a| a.y.to_bits()).collect::<std::collections::HashSet<_>>();
  assert!(distinct_rows.len() >= 500, "{} distinct rows for {} anchors", distinct_rows.len(), anchors.len());
}
