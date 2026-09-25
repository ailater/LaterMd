use std::cell::RefCell;

use egui::{vec2, Color32, Context, FontId, Id, RawInput, Rect, UiBuilder};
use egui_markdown::link::LinkHandler;
use egui_markdown::{layout, MarkdownLabel, MarkdownStyle};

/// Handler that renders every fence whose info string starts with `ai` as a
/// block widget, recording the (language, text) it was invoked with.
struct AiFenceHandler {
  rendered: RefCell<Vec<(Option<String>, String)>>,
}

impl LinkHandler for AiFenceHandler {
  fn is_block_code_widget(&self, language: Option<&str>) -> bool {
    language.is_some_and(|info| info.starts_with("ai"))
  }

  fn block_code_widget(&self, ui: &mut egui::Ui, text: &str, language: Option<&str>) -> Option<egui::Response> {
    self.rendered.borrow_mut().push((language.map(str::to_owned), text.to_owned()));
    Some(ui.label(text.to_owned()))
  }
}

fn screen() -> Rect {
  Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(500.0, 2000.0))
}

/// The block-widget branch fires once per matching fence, in document order;
/// non-matching fences stay in the galley path.
#[test]
fn block_code_widget_fires_once_per_matching_fence_in_order() {
  let doc = "```rust\nfn main() {}\n```\n\ntext between\n\n```ai\nfirst instruction\n```\n\n```ai\nsecond\n```\n";

  let ctx = Context::default();
  let handler = AiFenceHandler { rendered: RefCell::new(Vec::new()) };
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).link_handler(&handler).show(&mut child);
  });
  output.textures_delta.clear();

  assert_eq!(
    handler.rendered.borrow().clone(),
    vec![(Some("ai".to_owned()), "first instruction".to_owned()), (Some("ai".to_owned()), "second".to_owned()),],
    "each `ai fence routes to the widget exactly once, in document order"
  );
}

/// Without an opt-in handler the fence renders as an ordinary code block:
/// `needs_segmentation` stays false and the text is painted by the galley.
#[test]
fn default_handler_keeps_fence_in_galley_path() {
  let doc = "```ai\njust a code block\n```\n";
  let md = egui_markdown::parse(doc);
  assert!(!layout::needs_segmentation(&md.tokens, false, None));

  let ctx = Context::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    MarkdownLabel::new(Id::new("test"), doc).show(&mut child);
  });
  let painted = painted_strings(&output);
  output.textures_delta.clear();
  assert!(
    painted.iter().any(|t| t.contains("just a code block")),
    "fence must render via the galley path without a handler: {painted:?}"
  );
}

/// `needs_segmentation` and `build_layout` must agree on which fences become
/// segment breaks (the segmented renderer debug-asserts the same contract).
#[test]
fn needs_segmentation_matches_build_layout_with_widget_handler() {
  let docs = [
    "```ai\ninstruction\n```\n",
    "Intro.\n\n```ai\ninstruction\n```\n\nOutro.",
    "```rust\nfn main() {}\n```\n\n```ai\ninstruction\n```\n",
    "No fences at all, just **markdown**.",
  ];

  let ctx = Context::default();
  let handler = AiFenceHandler { rendered: RefCell::new(Vec::new()) };
  let style = MarkdownStyle::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    for doc in docs {
      let md = egui_markdown::parse(doc);
      let predicted = layout::needs_segmentation(&md.tokens, false, Some(&handler));
      let built = layout::build_layout(
        ui,
        &md.tokens,
        FontId::proportional(14.0),
        Color32::WHITE,
        None,
        ui.available_width(),
        false,
        Some(&handler),
        false,
        &style,
        Default::default(),
      );
      let expected_breaks: Vec<usize> = md
        .tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| {
          matches!(token, egui_markdown::Token::CodeBlock { language, .. }
            if language.as_deref().is_some_and(|info| info.starts_with("ai")))
        })
        .map(|(index, _)| index)
        .collect();
      assert_eq!(predicted, !expected_breaks.is_empty(), "predicted path for:\n{doc}");
      assert_eq!(built.segment_breaks, expected_breaks, "segment breaks for:\n{doc}");
    }
  });
  output.textures_delta.clear();
}

fn painted_strings(output: &egui::FullOutput) -> Vec<String> {
  fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
    match shape {
      egui::epaint::Shape::Text(t) => out.push(t.galley.text().to_owned()),
      egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
      _ => {}
    }
  }
  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  out
}
