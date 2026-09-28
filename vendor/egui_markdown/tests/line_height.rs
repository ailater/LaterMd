use egui::{Color32, Context, FontId, RawInput, Rect, UiBuilder};
use egui_markdown::{layout, MarkdownStyle};

const DOC: &str = "# Title\n\nBody text.";

/// Build `doc` at `body_size` under `style`, returning the (font size, line height)
/// of every laid-out section, in order.
fn section_rows(doc: &str, body_size: f32, style: &MarkdownStyle) -> Vec<(f32, Option<f32>)> {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 2000.0));
  let mut rows = Vec::new();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    let md = egui_markdown::parse(doc);
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

fn close(a: f32, b: f32) -> bool {
  (a - b).abs() < 0.01
}

#[test]
fn heading_row_height_scales_with_font_size() {
  let style = MarkdownStyle::default();
  let rows = section_rows(DOC, 13.0, &style);
  let heading = rows.iter().find(|(size, _)| close(*size, 13.0 * style.heading.scales[0])).expect("no heading section");
  let body = rows.iter().find(|(size, _)| close(*size, 13.0)).expect("no body section");
  assert!(
    close(heading.1.unwrap_or(0.0), heading.0 * style.line_height_ratio),
    "heading row height {:?} should follow its font size {}",
    heading.1,
    heading.0
  );
  assert!(
    close(body.1.unwrap_or(0.0), body.0 * style.line_height_ratio),
    "body row height {:?} should follow its font size {}",
    body.1,
    body.0
  );
  // The fixed-height bug this replaces: every row was 17px, so an H1 at 13pt body
  // (20.8pt glyphs) was clipped and wrapped lines drew on top of each other.
  assert!(
    heading.1.unwrap_or(0.0) > body.1.unwrap_or(0.0) + 1.0,
    "heading row height {:?} must outgrow the body's {:?}",
    heading.1,
    body.1
  );
}

#[test]
fn body_row_height_follows_ratio() {
  let style = MarkdownStyle { line_height_ratio: 1.8, ..Default::default() };
  let rows = section_rows("Just body text.", 13.0, &style);
  let body = rows.iter().find(|(size, _)| close(*size, 13.0)).expect("no body section");
  assert!(close(body.1.unwrap_or(0.0), 13.0 * 1.8), "ratio 1.8 should give a 23.4px row, got {:?}", body.1);
}

#[test]
fn default_ratio_preserves_previous_body_row_height() {
  let style = MarkdownStyle::default();
  assert_eq!(style.line_height_ratio, 1.30);
  let rows = section_rows("Just body text.", 13.0, &style);
  let body = rows.iter().find(|(size, _)| close(*size, 13.0)).expect("no body section");
  // 13pt * 1.30 = 16.9px, matching the fixed height the constant used to hardcode.
  assert!(close(body.1.unwrap_or(0.0), 16.9), "default row height {:?} should stay ~17px", body.1);
}
