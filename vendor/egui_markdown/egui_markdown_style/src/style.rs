//! Customizable visual styling for all markdown elements.

use std::hash::{Hash, Hasher};

use egui::{self, Color32, DragValue, Grid, Ui};

/// Visual styling for markdown rendering.
///
/// All fields have sensible defaults matching the previously hardcoded values.
/// Dark/light theme adaptation is automatic via `InlineCodeStyle`'s per-theme color
/// fields and egui's `Visuals::dark_mode`.
///
/// Install a context-wide default with [`crate::set_style`]; widgets read it via
/// [`crate::global_style`] when no per-widget override is set.
///
/// # Example
///
/// ```
/// use egui_markdown_style::MarkdownStyle;
///
/// let mut style = MarkdownStyle::default();
/// style.heading.scales[0] = 2.0; // Bigger H1
/// ```
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MarkdownStyle {
  /// Styling for inline code spans.
  pub inline_code: InlineCodeStyle,
  /// Styling for fenced code blocks.
  pub code_block: CodeBlockStyle,
  /// Styling for heading levels 1–6.
  pub heading: HeadingStyle,
  /// Styling for horizontal rules.
  pub horizontal_rule: HorizontalRuleStyle,
  /// Styling for blockquotes.
  pub blockquote: BlockquoteStyle,
  /// Styling for list markers (bullets and numbers).
  pub list: ListStyle,
  /// Styling for tables.
  #[cfg_attr(feature = "serde", serde(default))]
  pub table: TableStyle,
  /// Vertical spacing between block elements in pixels.
  pub block_spacing: f32,
  /// Font size for code blocks. Default: `10.0`.
  pub code_font_size: f32,
  /// Row height as a multiple of the font size. Default: `1.30`.
  ///
  /// Applied to body text and headings alike, so headings scale into the rhythm
  /// instead of being clipped by a height tuned for body-size glyphs.
  #[cfg_attr(feature = "serde", serde(default = "default_line_height_ratio"))]
  pub line_height_ratio: f32,
  /// Lower bound for the row height, in font-size multiples (em). Default: `1.0`
  /// (no floor; the ratio alone decides).
  ///
  /// Fallback faces used for non-Latin scripts (CJK in particular) carry much
  /// taller row metrics than the Latin chain head — Noto Sans CJK needs ≈1.448em
  /// where the default ratio of 1.30 leaves only 1.30em. A row shorter than the
  /// fallback face's own row height still *advances* by that row height, so the
  /// overflowing CJK ink visually collides with the next row or gets occluded by
  /// the next block's opaque background ("clipped" rows). A host that registers a
  /// CJK fallback chain sets this to the fallback face's row height in em, and
  /// every row — Latin-only or mixed — then clears the tallest face it may
  /// contain, which also keeps row spacing uniform across a mixed document.
  #[cfg_attr(feature = "serde", serde(default = "default_min_line_height_em"))]
  pub min_line_height_em: f32,
  /// Extra vertical space inserted above a heading, in pixels, added on top of
  /// `block_spacing`. Default: `4.0`.
  ///
  /// This is emitted in the layout job as a transparent spacer row of height
  /// `block_spacing + heading_space_above` ahead of a heading's first row, so it
  /// applies both to a whole-document galley and to a segmented range flushed on
  /// its own. Headings at the very start of a document (or of a flushed range)
  /// get no spacer. The spacer row counts towards a `max_rows` / truncate budget.
  #[cfg_attr(feature = "serde", serde(default = "default_heading_space_above"))]
  pub heading_space_above: f32,
  /// Minimum number of lines in a fenced code block for it to be laid out as its
  /// own segment — the way tables, images and blockquotes already are — even when
  /// code blocks are not scrollable. Default: `500`.
  ///
  /// The threshold is measured **per code block** (its line count), not per document
  /// (token or byte totals): `build_layout` runs on whole documents *and* on individual
  /// flushed ranges, and a whole-document measure is not knowable from inside a range,
  /// so the two callers would disagree about the same tokens. A per-block measure
  /// decides identically in every context, and admission never inserts or removes
  /// tokens, so a fence crossing the threshold cannot shift the token indices that
  /// later block widgets bake into their ids.
  ///
  /// The default is deliberately conservative: a hand-written document almost never
  /// carries a single 500-line fence, so ordinary documents keep the whole-document
  /// galley path — and its pixel output — unchanged. Long fences (pasted logs,
  /// streaming LLM output) are exactly the case where re-laying-out the whole document
  /// on every append stops being affordable, and they flip to per-block layout and
  /// caching instead.
  #[cfg_attr(feature = "serde", serde(default = "default_segmentation_admission"))]
  pub segmentation_admission: usize,
  /// Language used for syntax highlighting when no language is specified.
  pub default_code_language: String,
}

/// Serde default for [`MarkdownStyle::line_height_ratio`]: ~17px at the 13pt
/// body size, matching the previous hardcoded row height.
#[cfg(feature = "serde")]
fn default_line_height_ratio() -> f32 {
  1.30
}

/// Serde default for [`MarkdownStyle::min_line_height_em`]: no floor.
#[cfg(feature = "serde")]
fn default_min_line_height_em() -> f32 {
  1.0
}

/// Serde default for [`MarkdownStyle::heading_space_above`]: a modest 4px of
/// extra breathing room above headings, on top of `block_spacing`.
#[cfg(feature = "serde")]
fn default_heading_space_above() -> f32 {
  4.0
}

/// Serde default for [`MarkdownStyle::segmentation_admission`]: a fence must
/// reach 500 lines before it is laid out as its own segment.
#[cfg(feature = "serde")]
fn default_segmentation_admission() -> usize {
  500
}

impl Default for MarkdownStyle {
  fn default() -> Self {
    Self {
      inline_code: InlineCodeStyle::default(),
      code_block: CodeBlockStyle::default(),
      heading: HeadingStyle::default(),
      horizontal_rule: HorizontalRuleStyle::default(),
      blockquote: BlockquoteStyle::default(),
      list: ListStyle::default(),
      table: TableStyle::default(),
      block_spacing: 8.0,
      code_font_size: 10.0,
      line_height_ratio: 1.30,
      min_line_height_em: 1.0,
      heading_space_above: 4.0,
      segmentation_admission: 500,
      default_code_language: String::new(),
    }
  }
}

impl Hash for MarkdownStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.inline_code.hash(state);
    self.code_block.hash(state);
    self.heading.hash(state);
    self.horizontal_rule.hash(state);
    self.blockquote.hash(state);
    self.list.hash(state);
    self.table.hash(state);
    self.block_spacing.to_bits().hash(state);
    self.code_font_size.to_bits().hash(state);
    self.line_height_ratio.to_bits().hash(state);
    self.min_line_height_em.to_bits().hash(state);
    self.heading_space_above.to_bits().hash(state);
    self.segmentation_admission.hash(state);
    self.default_code_language.hash(state);
  }
}

impl MarkdownStyle {
  /// Show an interactive editor for all style fields, plus a dark/light switch.
  pub fn ui(&mut self, ui: &mut Ui) {
    ui.horizontal(|ui| {
      let dark_mode = ui.visuals().dark_mode;
      if ui.selectable_label(dark_mode, "Dark").clicked() {
        ui.ctx().set_visuals(egui::Visuals::dark());
      }
      if ui.selectable_label(!dark_mode, "Light").clicked() {
        ui.ctx().set_visuals(egui::Visuals::light());
      }
      ui.separator();
      if ui.button("Reset").clicked() {
        *self = Self::default();
      }
    });

    ui.separator();
    self.render_style(ui);
  }

  /// Edit the markdown style fields.
  pub fn render_style(&mut self, ui: &mut Ui) {
    ui.label("Block spacing:");
    ui.add(DragValue::new(&mut self.block_spacing).range(0.0..=40.0).speed(0.5));

    ui.horizontal(|ui| {
      ui.label("Line height:");
      ui.add(DragValue::new(&mut self.line_height_ratio).range(1.0..=3.0).speed(0.01));
    });

    ui.horizontal(|ui| {
      ui.label("Min line height (em):");
      ui.add(DragValue::new(&mut self.min_line_height_em).range(1.0..=3.0).speed(0.01));
    });

    ui.horizontal(|ui| {
      ui.label("Heading space above:");
      ui.add(DragValue::new(&mut self.heading_space_above).range(0.0..=40.0).speed(0.5));
    });

    ui.horizontal(|ui| {
      ui.label("Segment code blocks at (lines):");
      ui.add(DragValue::new(&mut self.segmentation_admission).range(0..=100_000).speed(1.0));
    });

    ui.separator();

    egui::CollapsingHeader::new("Inline Code").default_open(true).show(ui, |ui| {
      self.inline_code.ui(ui);
    });

    egui::CollapsingHeader::new("Code Blocks").default_open(true).show(ui, |ui| {
      self.code_block.ui(ui);
      ui.separator();
      ui.horizontal(|ui| {
        ui.label("Font size:");
        ui.add(DragValue::new(&mut self.code_font_size).range(6.0..=30.0).speed(0.5));
      });
      ui.horizontal(|ui| {
        ui.label("Default language:");
        ui.add(egui::TextEdit::singleline(&mut self.default_code_language).desired_width(80.0));
      });
    });

    egui::CollapsingHeader::new("Headings").default_open(true).show(ui, |ui| {
      self.heading.ui(ui);
    });

    egui::CollapsingHeader::new("Horizontal Rules").default_open(false).show(ui, |ui| {
      self.horizontal_rule.ui(ui);
    });

    egui::CollapsingHeader::new("Blockquotes").default_open(false).show(ui, |ui| {
      self.blockquote.ui(ui);
    });

    egui::CollapsingHeader::new("Lists").default_open(false).show(ui, |ui| {
      self.list.ui(ui);
    });

    egui::CollapsingHeader::new("Tables").default_open(false).show(ui, |ui| {
      self.table.ui(ui);
    });
  }
}

/// Styling for inline code spans (backtick-delimited).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InlineCodeStyle {
  /// Text color in dark mode.
  pub color_dark: Color32,
  /// Text color in light mode.
  pub color_light: Color32,
  /// Background color in dark mode.
  pub background_dark: Color32,
  /// Background color in light mode.
  pub background_light: Color32,
  /// How much to expand the background rectangle horizontally beyond the text bounds (in pixels).
  pub expand_bg: f32,
}

impl Default for InlineCodeStyle {
  fn default() -> Self {
    Self {
      color_dark: Color32::from_rgb(255, 152, 0),
      color_light: Color32::from_rgb(204, 102, 0),
      background_dark: Color32::from_gray(50),
      background_light: Color32::from_gray(225),
      expand_bg: 3.0,
    }
  }
}

impl Hash for InlineCodeStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.color_dark.hash(state);
    self.color_light.hash(state);
    self.background_dark.hash(state);
    self.background_light.hash(state);
    self.expand_bg.to_bits().hash(state);
  }
}

impl InlineCodeStyle {
  /// Resolve color for the current theme.
  pub fn color(&self, dark_mode: bool) -> Color32 {
    if dark_mode {
      self.color_dark
    } else {
      self.color_light
    }
  }

  /// Resolve background for the current theme.
  pub fn background(&self, dark_mode: bool) -> Color32 {
    if dark_mode {
      self.background_dark
    } else {
      self.background_light
    }
  }

  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("inline_code_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Color (dark):");
      ui.color_edit_button_srgba(&mut self.color_dark);
      ui.end_row();

      ui.label("Color (light):");
      ui.color_edit_button_srgba(&mut self.color_light);
      ui.end_row();

      ui.label("Background (dark):");
      ui.color_edit_button_srgba(&mut self.background_dark);
      ui.end_row();

      ui.label("Background (light):");
      ui.color_edit_button_srgba(&mut self.background_light);
      ui.end_row();

      ui.label("Expand bg:");
      ui.add(DragValue::new(&mut self.expand_bg).range(0.0..=10.0).speed(0.1));
      ui.end_row();
    });
  }
}

/// Styling for fenced code blocks.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CodeBlockStyle {
  /// Padding `[left, top, right, bottom]`.
  pub padding: [f32; 4],
  /// Corner radius for the code block border.
  pub corner_radius: f32,
  /// Stroke width for the code block border.
  pub stroke_width: f32,
}

impl Default for CodeBlockStyle {
  fn default() -> Self {
    Self { padding: [4.0, 6.0, 12.0, 6.0], corner_radius: 3.0, stroke_width: 1.0 }
  }
}

impl Hash for CodeBlockStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    for v in &self.padding {
      v.to_bits().hash(state);
    }
    self.corner_radius.to_bits().hash(state);
    self.stroke_width.to_bits().hash(state);
  }
}

impl CodeBlockStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("code_block_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Padding left:");
      ui.add(DragValue::new(&mut self.padding[0]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Padding top:");
      ui.add(DragValue::new(&mut self.padding[1]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Padding right:");
      ui.add(DragValue::new(&mut self.padding[2]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Padding bottom:");
      ui.add(DragValue::new(&mut self.padding[3]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Corner radius:");
      ui.add(DragValue::new(&mut self.corner_radius).range(0.0..=20.0).speed(0.5));
      ui.end_row();

      ui.label("Stroke width:");
      ui.add(DragValue::new(&mut self.stroke_width).range(0.0..=5.0).speed(0.1));
      ui.end_row();
    });
  }
}

/// Styling for heading levels 1–6.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HeadingStyle {
  /// Font size multipliers for H1–H6.
  pub scales: [f32; 6],
}

impl Default for HeadingStyle {
  fn default() -> Self {
    // Spread the six levels far enough apart to stay distinguishable: at a 13pt body
    // this yields ~26.0 / 20.2 / 16.9 / 15.0 / 14.0 / 13.0pt, so the adjacent lower
    // levels differ by ~1pt instead of ~0.6pt and H6 alone matches the body size.
    // H1 at 2.0× sits at the top of the customary range for Latin body text (1.8–2.0×)
    // and also suits dense CJK glyphs, which read best at the larger end of that range.
    Self { scales: [2.0, 1.55, 1.30, 1.15, 1.08, 1.0] }
  }
}

impl Hash for HeadingStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    for v in &self.scales {
      v.to_bits().hash(state);
    }
  }
}

impl HeadingStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("heading_style").num_columns(2).striped(true).show(ui, |ui| {
      for (i, scale) in self.scales.iter_mut().enumerate() {
        ui.label(format!("H{}:", i + 1));
        ui.add(DragValue::new(scale).range(0.5..=4.0).speed(0.01));
        ui.end_row();
      }
    });
  }
}

/// Styling for horizontal rules (`---`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HorizontalRuleStyle {
  /// Stroke width for horizontal rule lines.
  pub stroke_width: f32,
  /// Vertical space reserved for the rule row (points).
  pub height: f32,
}

impl Default for HorizontalRuleStyle {
  fn default() -> Self {
    Self { stroke_width: 1.0, height: 8.0 }
  }
}

impl Hash for HorizontalRuleStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.stroke_width.to_bits().hash(state);
    self.height.to_bits().hash(state);
  }
}

impl HorizontalRuleStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("hr_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Stroke width:");
      ui.add(DragValue::new(&mut self.stroke_width).range(0.0..=5.0).speed(0.1));
      ui.end_row();
      ui.label("Height:");
      ui.add(DragValue::new(&mut self.height).range(1.0..=40.0).speed(0.5));
      ui.end_row();
    });
  }
}

/// Styling for blockquotes.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BlockquoteStyle {
  /// Horizontal indent per nesting depth in pixels.
  pub indent_per_depth: f32,
  /// Width of the vertical bar drawn at the left edge of a blockquote.
  pub stroke_width: f32,
}

impl Default for BlockquoteStyle {
  fn default() -> Self {
    Self { indent_per_depth: 12.0, stroke_width: 1.0 }
  }
}

impl Hash for BlockquoteStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.indent_per_depth.to_bits().hash(state);
    self.stroke_width.to_bits().hash(state);
  }
}

impl BlockquoteStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("blockquote_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Indent per depth:");
      ui.add(DragValue::new(&mut self.indent_per_depth).range(0.0..=40.0).speed(0.5));
      ui.end_row();

      ui.label("Stroke width:");
      ui.add(DragValue::new(&mut self.stroke_width).range(0.0..=5.0).speed(0.1));
      ui.end_row();
    });
  }
}

/// Styling for list markers.
///
/// Markers are right-aligned in a slot whose width is measured from the body font, so every item
/// of a nesting level starts its text at the same x. These fields adjust that arrangement: `gap`
/// moves the text away from the marker column (wrapped rows follow it), while the two `nudge`
/// fields move a marker left of the column without moving any text.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ListStyle {
  /// Space between the marker column and the item text, in points.
  pub gap: f32,
  /// How far left of the marker column to draw a bullet, in points.
  pub bullet_nudge: f32,
  /// How far left of the marker column to draw a number, in points.
  pub number_nudge: f32,
  /// Font size multiplier for the bullet glyph. The row height is unaffected.
  pub bullet_scale: f32,
}

impl Default for ListStyle {
  fn default() -> Self {
    Self { gap: 0.0, bullet_nudge: 0.0, number_nudge: 0.0, bullet_scale: 1.0 }
  }
}

impl Hash for ListStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.gap.to_bits().hash(state);
    self.bullet_nudge.to_bits().hash(state);
    self.number_nudge.to_bits().hash(state);
    self.bullet_scale.to_bits().hash(state);
  }
}

fn default_cell_padding() -> [f32; 4] {
  [10.0, 6.0, 10.0, 6.0]
}

/// Styling for markdown tables.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TableStyle {
  /// Width of separator lines between cells. `0.0` draws none.
  pub stroke_width: f32,
  /// Corner radius of the outer table stroke. Only visible when [`Self::stroke_width`] is non-zero.
  pub corner_radius: f32,
  /// Inner cell padding `[left, top, right, bottom]`.
  #[cfg_attr(feature = "serde", serde(default = "default_cell_padding"))]
  pub cell_padding: [f32; 4],
  /// Fill the header row with [`egui::Visuals::faint_bg_color`], behind the cell text.
  #[cfg_attr(feature = "serde", serde(default))]
  pub header_fill: bool,
  /// Zebra-stripe the body rows using `egui_extras`' built-in striped rows,
  /// which paint the same [`egui::Visuals::faint_bg_color`].
  #[cfg_attr(feature = "serde", serde(default))]
  pub zebra_fill: bool,
}

impl Default for TableStyle {
  fn default() -> Self {
    Self {
      stroke_width: 0.0,
      corner_radius: 0.0,
      cell_padding: default_cell_padding(),
      header_fill: false,
      zebra_fill: false,
    }
  }
}

impl Hash for TableStyle {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.stroke_width.to_bits().hash(state);
    self.corner_radius.to_bits().hash(state);
    for v in &self.cell_padding {
      v.to_bits().hash(state);
    }
    self.header_fill.hash(state);
    self.zebra_fill.hash(state);
  }
}

impl TableStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("table_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Stroke width:");
      ui.add(DragValue::new(&mut self.stroke_width).range(0.0..=5.0).speed(0.1));
      ui.end_row();

      ui.label("Corner radius:");
      ui.add(DragValue::new(&mut self.corner_radius).range(0.0..=20.0).speed(0.5));
      ui.end_row();

      ui.label("Cell padding left:");
      ui.add(DragValue::new(&mut self.cell_padding[0]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Cell padding top:");
      ui.add(DragValue::new(&mut self.cell_padding[1]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Cell padding right:");
      ui.add(DragValue::new(&mut self.cell_padding[2]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Cell padding bottom:");
      ui.add(DragValue::new(&mut self.cell_padding[3]).range(0.0..=30.0).speed(0.5));
      ui.end_row();

      ui.label("Header fill:");
      ui.checkbox(&mut self.header_fill, "");
      ui.end_row();

      ui.label("Zebra fill:");
      ui.checkbox(&mut self.zebra_fill, "");
      ui.end_row();
    });
  }
}

impl ListStyle {
  fn ui(&mut self, ui: &mut Ui) {
    Grid::new("list_style").num_columns(2).striped(true).show(ui, |ui| {
      ui.label("Marker gap:");
      ui.add(DragValue::new(&mut self.gap).range(0.0..=40.0).speed(0.25));
      ui.end_row();

      ui.label("Bullet nudge:");
      ui.add(DragValue::new(&mut self.bullet_nudge).range(0.0..=40.0).speed(0.25));
      ui.end_row();

      ui.label("Number nudge:");
      ui.add(DragValue::new(&mut self.number_nudge).range(0.0..=40.0).speed(0.25));
      ui.end_row();

      ui.label("Bullet scale:");
      ui.add(DragValue::new(&mut self.bullet_scale).range(0.5..=4.0).speed(0.05));
      ui.end_row();
    });
  }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
  use super::*;

  /// Serialize a style, drop the field named `field` from the document, and
  /// deserialize it back — the shape of a theme file saved before `field` existed.
  fn style_without_field(field: &str) -> MarkdownStyle {
    let mut value = serde_json::to_value(MarkdownStyle::default()).expect("serialize default style");
    value.as_object_mut().expect("style serializes to a map").remove(field);
    serde_json::from_value(value).expect("deserialize legacy style")
  }

  #[test]
  fn heading_space_above_defaults_when_missing_and_keeps_explicit_values() {
    // Old theme files predate the field; they must deserialize with the default.
    let legacy = style_without_field("heading_space_above");
    assert_eq!(legacy.heading_space_above, 4.0);
    // The same legacy tolerance covers the previously added line-height fields.
    assert_eq!(style_without_field("line_height_ratio").line_height_ratio, 1.30);
    assert_eq!(style_without_field("min_line_height_em").min_line_height_em, 1.0);

    // An explicit zero is a real preference ("no extra heading space"), not a
    // missing value, and must survive a round trip.
    let mut value = serde_json::to_value(MarkdownStyle::default()).expect("serialize");
    value["heading_space_above"] = serde_json::Value::from(0.0_f32);
    let explicit_zero: MarkdownStyle = serde_json::from_value(value).expect("deserialize explicit zero");
    assert_eq!(explicit_zero.heading_space_above, 0.0);

    let tuned = MarkdownStyle { heading_space_above: 9.5, ..Default::default() };
    let round: MarkdownStyle =
      serde_json::from_str(&serde_json::to_string(&tuned).expect("serialize tuned")).expect("round trip");
    assert_eq!(round.heading_space_above, 9.5);
  }

  #[test]
  fn segmentation_admission_defaults_when_missing_and_keeps_explicit_values() {
    // Old theme files predate the field; they must deserialize with the default.
    let legacy = style_without_field("segmentation_admission");
    assert_eq!(legacy.segmentation_admission, 500);

    // An explicit zero is a real preference ("every fence is a segment"), not a
    // missing value, and must survive a round trip.
    let mut value = serde_json::to_value(MarkdownStyle::default()).expect("serialize");
    value["segmentation_admission"] = serde_json::Value::from(0_u64);
    let explicit_zero: MarkdownStyle = serde_json::from_value(value).expect("deserialize explicit zero");
    assert_eq!(explicit_zero.segmentation_admission, 0);

    let tuned = MarkdownStyle { segmentation_admission: 120, ..Default::default() };
    let round: MarkdownStyle =
      serde_json::from_str(&serde_json::to_string(&tuned).expect("serialize tuned")).expect("round trip");
    assert_eq!(round.segmentation_admission, 120);
  }
}
