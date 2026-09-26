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
