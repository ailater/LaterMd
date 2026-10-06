//! Deferred off-screen highlighting ([`MarkdownLabel::defer_offscreen_highlight`]).
//!
//! The contract under test:
//! - a flush range that starts below the viewport is laid out **without**
//!   syntect colors, but with identical text and identical geometry — deferral
//!   may not move anything;
//! - the moment such a range is no longer below the fold it is rebuilt with
//!   full highlighting, before it can be seen.

use egui::{pos2, vec2, Context, Id, RawInput, Rect, ScrollArea, UiBuilder};
use egui_markdown::{MarkdownLabel, MarkdownStyle};

fn screen() -> Rect {
  Rect::from_min_size(pos2(0.0, 0.0), vec2(500.0, 300.0))
}

/// Sections of heading + paragraph + a colorful rust fence (keyword, string and
/// comment produce distinct syntect colors). With `segmentation_admission: 3`
/// every fence becomes its own single-fence flush range, so "range below the
/// fold" maps 1:1 to "fence below the fold".
fn doc(sections: usize) -> String {
  let mut text = String::new();
  for i in 0..sections {
    text.push_str(&format!("## 第 {i} 节\n\n第 {i} 段正文,足够一行以上。\n\n"));
    text.push_str("```rust\nfn main() {\n    let s = \"字符串\";\n    let n = 42; // 注释\n}\n```\n\n");
  }
  text
}

/// One entry per painted galley: its text, size, and the colors of its
/// monospace (code) sections. Code colors are what deferral is allowed to
/// change; text and size are what it must never change.
#[derive(Debug, Clone)]
struct GalleyProbe {
  text: String,
  size: egui::Vec2,
  /// Top y of the galley in screen space — tells "below the fold" (300px) apart.
  top_y: f32,
  code_colors: Vec<egui::Color32>,
}

fn collect(shape: &egui::epaint::Shape, out: &mut Vec<GalleyProbe>) {
  match shape {
    egui::epaint::Shape::Text(t) => out.push(GalleyProbe {
      text: t.galley.text().to_owned(),
      size: t.galley.size(),
      top_y: t.pos.y,
      code_colors: t
        .galley
        .job
        .sections
        .iter()
        .filter(|s| s.format.font_id.family == egui::FontFamily::Monospace)
        .map(|s| s.format.color)
        .collect(),
    }),
    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
    _ => {}
  }
}

fn probes(output: &egui::FullOutput) -> Vec<GalleyProbe> {
  let mut out = Vec::new();
  for clipped in &output.shapes {
    collect(&clipped.shape, &mut out);
  }
  out
}

/// Cold render of the whole document into a fresh context (no ScrollArea, so
/// nothing is culled and every range paints exactly once).
fn render_once(text: &str, defer: bool, style: &MarkdownStyle) -> Vec<GalleyProbe> {
  let ctx = Context::default();
  let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
    let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
    let mut label = MarkdownLabel::new(Id::new("defer"), text).wrap().style(style);
    if defer {
      label = label.defer_offscreen_highlight(true);
    }
    label.show(&mut child);
  });
  let probes = probes(&output);
  output.textures_delta.clear();
  probes
}

fn style_small_admission() -> MarkdownStyle {
  MarkdownStyle { segmentation_admission: 3, ..Default::default() }
}

fn distinct_code_colors(g: &GalleyProbe) -> usize {
  g.code_colors.iter().collect::<std::collections::HashSet<_>>().len()
}

/// Cold frame with the flag on: every range below the fold must paint the same
/// text in the same geometry as the flag-off render, as a single-color
/// placeholder; the first (visible) range keeps full highlighting.
#[test]
fn deferred_ranges_keep_text_and_geometry() {
  let text = doc(6);
  let style = style_small_admission();
  let plain = render_once(&text, false, &style);
  let deferred = render_once(&text, true, &style);

  assert_eq!(
    deferred.iter().map(|g| (&g.text, g.size)).collect::<Vec<_>>(),
    plain.iter().map(|g| (&g.text, g.size)).collect::<Vec<_>>(),
    "deferral must not change galley texts or geometry"
  );
  // The document must actually overflow the 300px viewport, or nothing is deferred.
  let total: f32 = plain.iter().map(|g| g.size.y).sum();
  assert!(total > 400.0, "sample too short to defer anything: {total}");
  assert!(plain.iter().any(|g| distinct_code_colors(g) >= 2), "flag-off sample must carry syntect colors");

  let deferred_count = deferred.iter().filter(|g| g.top_y > 300.0).count();
  assert!(deferred_count >= 2, "expected some below-fold ranges, got {deferred_count}");
  for g in &deferred {
    if g.code_colors.is_empty() {
      continue; // non-code galley
    }
    if g.top_y > 300.0 {
      // Starts below the fold: plain placeholder.
      assert_eq!(distinct_code_colors(g), 1, "below-fold range must be plain: {:?}", g.code_colors);
    } else {
      // Touches the viewport: highlighted even with the flag on.
      assert!(distinct_code_colors(g) >= 2, "visible range must keep syntect colors: {:?}", g.code_colors);
    }
  }
}

/// Frame 2 after jumping to the bottom: previously deferred ranges rebuild
/// with full highlighting in that same frame (identical geometry as always),
/// before they can be seen.
#[test]
fn deferred_ranges_rehighlight_when_visible() {
  let text = doc(6);
  let style = style_small_admission();
  let plain = render_once(&text, false, &style);

  let ctx = Context::default();
  let mut content_bottom = 0.0f32;
  // Frame 0 = cold render; frame 1 requests the scroll (applied at its `end`,
  // after painting); frame 2 paints at the scrolled offset — the frame under
  // test.
  for frame in 0..3 {
    let mut output = ctx.run_ui(RawInput { screen_rect: Some(screen()), ..Default::default() }, |ui| {
      let mut child = ui.new_child(UiBuilder::new().max_rect(screen()));
      // `animated(false)` makes `scroll_to_*` apply within the same pass
      // (headless input has no advancing time, so animations never progress).
      ScrollArea::vertical().id_salt("defer-scroll").auto_shrink([false, false]).animated(false).show(
        &mut child,
        |inner| {
          if frame == 1 {
            // Jump to the bottom in one step: the bottom fences enter the
            // viewport and must be highlighted in this same frame.
            inner.scroll_to_rect_animation(
              Rect::from_min_size(pos2(0.0, content_bottom), vec2(500.0, 10.0)),
              Some(egui::Align::BOTTOM),
              egui::style::ScrollAnimation::none(),
            );
          }
          MarkdownLabel::new(Id::new("defer"), &text).wrap().style(&style).defer_offscreen_highlight(true).show(inner);
          content_bottom = content_bottom.max(inner.min_rect().bottom());
        },
      );
    });
    if frame < 2 {
      output.textures_delta.clear();
      continue;
    }
    let frame2 = probes(&output);
    output.textures_delta.clear();

    // The scroll must actually have moved the viewport to the bottom: the
    // topmost painted galley is scrolled off (top_y < 0) and content flows
    // past the viewport bottom edge.
    assert!(!frame2.is_empty(), "frame 2 painted nothing");
    assert!(
      frame2[0].top_y < 0.0 && frame2.last().unwrap().top_y > 200.0,
      "scroll to bottom did not happen: tops = {:?}",
      frame2.iter().map(|g| g.top_y).collect::<Vec<_>>()
    );

    // Geometry: every galley painted in the scrolled frame must match the
    // always-highlighted render by text and size.
    for g in &frame2 {
      assert!(
        plain.iter().any(|p| p.text == g.text && p.size == g.size),
        "scrolled-frame galley diverges from the always-highlighted render: {:?}",
        (&g.text, g.size)
      );
    }
    // Behavior: bottom fences are visible now, so at least one galley must
    // carry full syntect colors (≥2 distinct colors) — not the placeholder.
    assert!(
      frame2.iter().any(|g| distinct_code_colors(g) >= 2),
      "rehighlight on visible did not happen: {:?}",
      frame2.iter().map(|g| g.code_colors.clone()).collect::<Vec<_>>()
    );
  }
}

/// Same geometry contract for inline fences inside big text ranges (default
/// admission, so fences are not their own flush ranges).
#[test]
fn deferred_ranges_keep_text_and_geometry_inline() {
  let text = doc(6);
  let style = MarkdownStyle::default();
  let plain = render_once(&text, false, &style);
  let deferred = render_once(&text, true, &style);

  assert_eq!(
    deferred.iter().map(|g| (&g.text, g.size)).collect::<Vec<_>>(),
    plain.iter().map(|g| (&g.text, g.size)).collect::<Vec<_>>(),
    "deferral must not change galley texts or geometry (inline fences)"
  );
}
