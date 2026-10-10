use egui::{vec2, Context, FontId, Id, RawInput, Rect};

use egui_markdown::MarkdownLabel;

fn render(doc: &str, width: f32, id: &str) -> f32 {
  let ctx = Context::default();
  let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(width, 400.0));
  let total = std::cell::Cell::new(0.0f32);
  let out = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
    MarkdownLabel::new(Id::new(id), doc).font(FontId::proportional(15.0)).wrap().show(ui);
    total.set(ui.min_rect().height());
  });
  out.drop_without_applying_deltas();
  total.get()
}

/// Under a context with no image loaders installed, an image URL can never
/// resolve: egui reports the load as failed and `Image` falls back to a
/// 24×24 source size. With the widget's default `ImageFit::Fraction(1×1)`
/// that fallback was scaled UP to fill the entire available area (measured:
/// a 10 000 × 10 000 rect under the default headless viewport), so one
/// broken `![](...)` swallowed the whole document layout.
///
/// The label pins `fit_to_original_size`, which never upscales: the fallback
/// stays at its native size and a legitimately oversized image still shrinks
/// to the column width via `max_width`.
#[test]
fn a_broken_image_does_not_reserve_the_available_area() {
  let doc = "段落甲。\n\n![缺失的图](https://no-such-host.invalid/x.png)\n\n段落乙。\n";
  let total = render(doc, 600.0, "broken-image");

  // 两段正文(~40px)+ 一个 24px 量级的失败占位,远小于可用区高度;
  // Fraction 放大版会吃掉大半个可用区(600px 栏宽下 ≈600px 的方)。
  assert!(total < 200.0, "破图占位应收缩到失败回退的量级(实测 {total}px)");
}

/// 不放大语义本身:加载成功的**小图**不该被拉伸到铺满栏宽。用 `bytes://`
/// 喂一张真实的 1×1 PNG,断言 label 消费高度不因栏宽变宽而变大。
#[test]
fn a_small_loaded_image_is_not_upscaled_to_the_column_width() {
  // 1×1 白色 PNG(67 字节,无需外部资产)
  const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
  ];

  let height_at = |width: f32| {
    let ctx = Context::default();
    // egui 内建 bytes:// 加载器,无需外部 loader
    ctx.include_bytes("bytes://probe-bytes.png", PNG_1X1.to_vec());
    let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(width, 400.0));
    let total = std::cell::Cell::new(0.0f32);
    let out = ctx.run_ui(RawInput { screen_rect: Some(screen), ..Default::default() }, |ui| {
      MarkdownLabel::new(Id::new("small-image"), "![小图](bytes://probe-bytes.png)")
        .font(FontId::proportional(15.0))
        .wrap()
        .show(ui);
      total.set(ui.min_rect().height());
    });
    out.drop_without_applying_deltas();
    total.get()
  };

  let narrow = height_at(200.0);
  let wide = height_at(600.0);
  assert!(
    (narrow - wide).abs() < 1.0,
    "1×1 图在 {narrow}px 与 {wide}px 栏宽下的占位应一致(永不放大),实测 {narrow} vs {wide}"
  );
}
