use egui::{vec2, Color32, Context, Id, RawInput, Rect, UiBuilder};
use egui_markdown::{MarkdownLabel, MarkdownStyle};

const TABLE_MD: &str = "| H1 | H2 | H3 |\n|---|---|---|\n| a | b | c |\n| d | e | f |\n| g | h | i |\n";

fn md_style(header_fill: bool, zebra_fill: bool) -> MarkdownStyle {
  let mut style = MarkdownStyle::default();
  style.table.header_fill = header_fill;
  style.table.zebra_fill = zebra_fill;
  style
}

/// Render `TABLE_MD` for three frames on one context (stable widget ids across
/// frames), returning the painted text and the faint-background rects of the
/// last frame.
fn render(header_fill: bool, zebra_fill: bool) -> (Vec<String>, Vec<Rect>) {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0));
  let style = md_style(header_fill, zebra_fill);
  let mut faint = Color32::TRANSPARENT;
  let mut texts = Vec::new();
  let mut faint_rects = Vec::new();
  for _ in 0..3 {
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
      faint = ui.visuals().faint_bg_color;
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
      MarkdownLabel::new(Id::new("table_style"), TABLE_MD).style(&style).show(&mut child);
    });
    texts.clear();
    faint_rects.clear();
    for clipped in &output.shapes {
      collect(&clipped.shape, faint, &mut texts, &mut faint_rects);
    }
    output.textures_delta.clear();
  }
  (texts, faint_rects)
}

fn collect(shape: &egui::epaint::Shape, faint: Color32, texts: &mut Vec<String>, faint_rects: &mut Vec<Rect>) {
  match shape {
    egui::epaint::Shape::Text(t) => texts.push(t.galley.text().to_owned()),
    egui::epaint::Shape::Rect(r) if r.fill == faint && r.rect.is_positive() => faint_rects.push(r.rect),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, faint, texts, faint_rects)),
    _ => {}
  }
}

/// Every toggle combination must keep rendering the table across frames without
/// panicking; `zebra_fill` also exercises `striped` + `vscroll(false)` together.
#[test]
fn toggle_combinations_render_three_frames_without_panic() {
  for header_fill in [false, true] {
    for zebra_fill in [false, true] {
      let (texts, _) = render(header_fill, zebra_fill);
      assert!(
        texts.iter().any(|t| t.contains("H1")),
        "header_fill={header_fill} zebra_fill={zebra_fill}: table did not render"
      );
    }
  }
}

#[test]
fn header_fill_paints_faint_bg_rect_behind_header_cells() {
  let (_, off) = render(false, false);
  let (_, on) = render(true, false);
  assert!(on.len() >= 3, "one faint rect per header cell expected, got {}", on.len());
  assert!(on.len() > off.len(), "baseline painted faint rects already: {off:?}");
}

#[test]
fn zebra_fill_paints_faint_bg_stripes_on_body_rows() {
  let (_, off) = render(false, false);
  let (_, on) = render(false, true);
  assert!(on.len() >= 3, "striped body rows should paint faint rects, got {}", on.len());
  assert!(on.len() > off.len(), "baseline painted faint rects already: {off:?}");
}
