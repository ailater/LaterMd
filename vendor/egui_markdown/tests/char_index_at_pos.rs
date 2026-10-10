use egui::{pos2, vec2, Context, Id, Pos2, RawInput, Rect, UiBuilder};
use egui_markdown::{block_span_rects, char_index_at_pos, MarkdownLabel};

fn screen() -> Rect {
  Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 2000.0))
}

/// Render `text` and return the text shapes painted for it (galley text plus
/// every glyph's screen position, in render order), so tests can target a
/// specific glyph without modelling font metrics.
struct PaintedText {
  text: String,
  glyphs: Vec<Pos2>,
}

fn paint(text: &str, id: Id) -> Vec<PaintedText> {
  let ctx = Context::default();
  let mut painted = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
  });
  for clipped in &output.shapes {
    if let egui::Shape::Text(t) = &clipped.shape {
      painted.push(PaintedText {
        text: t.galley.text().to_owned(),
        glyphs: t.galley.rows.iter().flat_map(|row| row.glyphs.iter()).map(|g| t.pos + g.pos.to_vec2()).collect(),
      });
    }
  }
  output.textures_delta.clear();
  painted
}

/// Clicking on a glyph maps back to that glyph's character index — the basis
/// for "place the caret where the user clicked" in live-preview editors.
#[test]
fn clicking_a_glyph_resolves_to_its_character_index() {
  let text = "alpha beta\ngamma delta\n";
  let id = Id::new("hit");
  let painted = paint(text, id);
  let shape = painted.iter().find(|s| s.text.contains("gamma")).expect("paragraph rendered");
  // Soft breaks render as spaces (CommonMark), so "gamma" starts at char 11.
  let gamma = shape.text.find("gamma").expect("gamma in rendered text");
  // A hair inside the glyph's left half hits that glyph per the midpoint rule.
  let target = shape.glyphs[gamma] + vec2(1.0, 0.0);
  let ctx = Context::default();
  let mut hit = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    hit = char_index_at_pos(ui, id, target);
  });
  output.textures_delta.clear();
  assert_eq!(hit, Some(gamma), "click on 'gamma' start -> its char index");
}

/// Left of a row's first glyph and past its last glyph collapse to the row
/// start and the row end respectively: the usual editor hit-testing edges.
#[test]
fn row_edges_collapse_to_start_and_end() {
  let text = "abcdefghij\n";
  let id = Id::new("edges");
  let painted = paint(text, id);
  let shape = painted.first().expect("text painted");
  let left = shape.glyphs[0];
  let right = shape.glyphs[9];
  let ctx = Context::default();
  let mut hits = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    hits.push(char_index_at_pos(ui, id, left + vec2(-50.0, 0.0)));
    hits.push(char_index_at_pos(ui, id, right + vec2(300.0, 0.0)));
  });
  output.textures_delta.clear();
  assert_eq!(hits, vec![Some(0), Some(10)], "row-edge collapses");
}

/// Gaps between blocks and block widgets carry no text geometry: the hit test
/// reports None there, and callers fall back to the block table.
#[test]
fn outside_text_galleys_yields_none() {
  let text = "# Title\n\nbody\n";
  let id = Id::new("gap");
  paint(text, id);
  let ctx = Context::default();
  let mut hit = None;
  let mut blocks = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(id, text).wrap().show(&mut child);
    // Far below the last text row: inside no galley.
    hit = char_index_at_pos(ui, id, pos2(10.0, 5000.0));
    blocks = block_span_rects(ui, id);
  });
  output.textures_delta.clear();
  assert!(hit.is_none(), "no text geometry below the document");
  assert!(blocks.is_some_and(|b| !b.is_empty()), "block table still answers there");
}

/// A label not rendered this frame (or ever) yields None rather than stale or
/// fabricated geometry.
#[test]
fn unrendered_label_yields_none() {
  let ctx = Context::default();
  let mut hit = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    hit = char_index_at_pos(ui, Id::new("never"), pos2(5.0, 5.0));
  });
  output.textures_delta.clear();
  assert_eq!(hit, None);
}
