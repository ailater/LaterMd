//! Inline-widget font scaling inside headings.
//!
//! `Token::Link` carries the enclosing heading level, and `build_layout` passes a
//! heading-scaled font to the inline-widget callbacks (`inline_widget_size` and the
//! placeholder `layout_link` call) while every plain-link path keeps the body font.
//! These tests pin both sides: the scaling for H1–H6, and the no-op for body
//! paragraphs, plain links inside headings, and the per-cell table path.

use std::cell::RefCell;

use egui::{vec2, Color32, Context, FontId, Pos2, RawInput, Rect, TextFormat, Ui, UiBuilder, Vec2};
use egui_markdown::layout::{append_link_to_job, build_layout, render_link_in_ui};
use egui_markdown::{parse, LinkHandler, LinkStyle, MarkdownStyle};

const BODY: f32 = 13.0;

/// Probe handler: renders `widget://` links as inline widgets with a transparent
/// placeholder (mirrors the emoji handler contract), passes everything else
/// through, and records every dispatch as `(href, font size, color)`.
#[derive(Default)]
struct Probe {
  calls: RefCell<Vec<Dispatch>>,
}

impl Probe {
  fn take(&self) -> Vec<Dispatch> {
    std::mem::take(&mut *self.calls.borrow_mut())
  }
}

impl LinkHandler for Probe {
  fn link_style(&self, _href: &str) -> Option<LinkStyle> {
    None
  }

  fn inline_widget_size(&self, href: &str, font: &FontId) -> Option<Vec2> {
    if !href.starts_with("widget://") {
      return None;
    }
    self.calls.borrow_mut().push(("size", font.size, Color32::TRANSPARENT));
    Some(vec2(font.size, font.size))
  }

  fn layout_link(
    &self,
    _ui: &Ui,
    text: &str,
    href: &str,
    job: &mut egui::text::LayoutJob,
    font: &FontId,
    color: Color32,
  ) -> bool {
    let widget_call = href.starts_with("widget://") && color == Color32::TRANSPARENT;
    self.calls.borrow_mut().push(("layout", font.size, color));
    if widget_call {
      let format = TextFormat { font_id: font.clone(), color, ..TextFormat::default() };
      job.append(text, 0.0, format);
      return true;
    }
    false
  }
}

/// One recorded handler dispatch: `(call kind, font size, color)`.
type Dispatch = (&'static str, f32, Color32);

/// Run `build_layout` over `doc` with the probe handler, returning the recorded
/// dispatches plus the placeholder section's `(font size, line height)` from the
/// built job.
fn probe(doc: &str) -> (Vec<Dispatch>, Option<(f32, Option<f32>)>) {
  let ctx = Context::default();
  let screen = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 2000.0));
  let handler = Probe::default();
  let mut placeholder = None;
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    let md = parse(doc);
    let built = build_layout(
      &mut child,
      &md.tokens,
      FontId::proportional(BODY),
      Color32::WHITE,
      None,
      screen.width(),
      false,
      Some(&handler as &dyn LinkHandler),
      false,
      false,
      true,
      &MarkdownStyle::default(),
      Default::default(),
    );
    placeholder = built
      .job
      .sections
      .iter()
      .find(|s| s.format.color == Color32::TRANSPARENT && !s.format.line_height.is_none())
      .map(|s| (s.format.font_id.size, s.format.line_height));
  });
  output.textures_delta.clear();
  (handler.take(), placeholder)
}

/// Every `widget://` font dispatch for `doc`, as `(call, size)` pairs.
fn widget_calls(doc: &str) -> Vec<(&'static str, f32)> {
  let (calls, _) = probe(doc);
  calls
    .into_iter()
    .filter(|(kind, _, color)| matches!(*kind, "size") || *color == Color32::TRANSPARENT)
    .map(|(kind, size, _)| (kind, size))
    .collect()
}

#[test]
fn inline_widget_font_scales_with_heading_levels() {
  let scales = MarkdownStyle::default().heading.scales;
  for level in 1..=6usize {
    let doc = format!("{} head [w](widget://x) tail", "#".repeat(level));
    let calls = widget_calls(&doc);
    let expected = BODY * scales[level - 1];
    assert_eq!(calls.len(), 2, "H{level}: one size probe + one placeholder layout: {calls:?}");
    for (kind, size) in &calls {
      assert!(
        (size - expected).abs() < f32::EPSILON,
        "H{level} {kind} dispatch got {size}, want body {BODY} × {} = {expected}",
        scales[level - 1]
      );
    }
    // The placeholder row grows with the widget, and its font matches the scaled size.
    let (_, section) = probe(&doc);
    let (font_size, line_height) = section.expect("placeholder section");
    assert!((font_size - expected).abs() < f32::EPSILON, "H{level} placeholder font {font_size} != {expected}");
    assert_eq!(line_height, Some(expected), "H{level} placeholder row height = widget size");
  }
}

/// A heading that contains only a link has no `Text` token to betray the heading
/// level — the parser carries it on the `Link` token itself.
#[test]
fn link_only_heading_scales_widget() {
  let calls = widget_calls("# [w](widget://x)");
  assert!(!calls.is_empty(), "widget dispatches happened: {calls:?}");
  for (kind, size) in &calls {
    assert!(
      (size - BODY * 2.0).abs() < f32::EPSILON,
      "{kind} dispatch in a link-only H1 got {size}, want {}",
      BODY * 2.0
    );
  }
}

/// Veto line: a widget in a body paragraph gets the body font on every dispatch,
/// and its placeholder section keeps the exact pre-change font and row height.
#[test]
fn body_paragraph_widget_keeps_body_font() {
  let calls = widget_calls("para [w](widget://x) text");
  assert_eq!(calls.len(), 2, "size + placeholder layout: {calls:?}");
  for (kind, size) in &calls {
    assert!((size - BODY).abs() < f32::EPSILON, "body {kind} dispatch got {size}, want {BODY}");
  }
  let (_, section) = probe("para [w](widget://x) text");
  let (font_size, line_height) = section.expect("placeholder section");
  assert!((font_size - BODY).abs() < f32::EPSILON);
  assert_eq!(line_height, Some(BODY));
}

/// Veto line: the heading scale never leaks past the heading block — a widget in
/// the paragraph after a heading is back to the body font.
#[test]
fn heading_scale_does_not_leak_into_following_paragraph() {
  let calls = widget_calls("# h [w](widget://a)\n\npara [w](widget://b)");
  let sizes: Vec<f32> = calls.iter().map(|(_, s)| *s).collect();
  assert_eq!(sizes, vec![BODY * 2.0, BODY * 2.0, BODY, BODY], "H1 then body: {calls:?}");
}

/// Veto line: plain link text inside a heading keeps the body font on its layout
/// dispatch, and its default fallback section is appended at the body size.
#[test]
fn plain_link_in_heading_keeps_body_font() {
  let (calls, _) = probe("# head [t](https://e.com) tail");
  let plain = calls.iter().filter(|(kind, _, color)| *kind == "layout" && *color != Color32::TRANSPARENT);
  let mut checked = 0;
  for (_, size, _) in plain {
    assert!((size - BODY).abs() < f32::EPSILON, "plain link layout dispatch in heading got {size}, want {BODY}");
    checked += 1;
  }
  assert_eq!(checked, 1, "exactly one plain layout dispatch: {calls:?}");
}

/// Veto line: the per-cell table path (`render_link_in_ui`) has no heading
/// context and keeps the passed cell font for the widget.
#[test]
fn table_cell_widget_keeps_cell_font() {
  let ctx = Context::default();
  let screen = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 2000.0));
  let handler = Probe::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
    render_link_in_ui(
      &mut child,
      "t",
      "widget://x",
      &FontId::proportional(BODY),
      &TextFormat::default(),
      Color32::BLUE,
      Some(&handler as &dyn LinkHandler),
    );
  });
  output.textures_delta.clear();
  for (kind, size, _) in handler.take() {
    assert!((size - BODY).abs() < f32::EPSILON, "table {kind} dispatch got {size}, want {BODY}");
  }
}

/// The `inline_widget_font` override is opt-in per call site: `append_link_to_job`
/// with `None` keeps the body font for a widget even when the caller could have
/// passed one (pins the argument's meaning, not just its callers).
#[test]
fn none_override_keeps_body_font_for_widget() {
  let ctx = Context::default();
  let screen = Rect::from_min_size(Pos2::ZERO, vec2(500.0, 2000.0));
  let handler = Probe::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    let child = ui.new_child(UiBuilder::new().max_rect(screen));
    let mut job = egui::text::LayoutJob::default();
    append_link_to_job(
      &child,
      &mut job,
      "t",
      "widget://x",
      &FontId::proportional(BODY),
      Some(&FontId::proportional(BODY * 9.0)),
      &TextFormat::default(),
      Color32::BLUE,
      Some(&handler as &dyn LinkHandler),
    );
    let mut job_without = egui::text::LayoutJob::default();
    append_link_to_job(
      &child,
      &mut job_without,
      "t",
      "widget://x",
      &FontId::proportional(BODY),
      None,
      &TextFormat::default(),
      Color32::BLUE,
      Some(&handler as &dyn LinkHandler),
    );
  });
  output.textures_delta.clear();
  let sizes: Vec<f32> = handler.take().into_iter().map(|(_, size, _)| size).collect();
  assert_eq!(sizes, vec![BODY * 9.0, BODY * 9.0, BODY, BODY], "Some scales, None keeps body: {sizes:?}");
}
