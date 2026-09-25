//! M0 主验证 2「长文档性能」的自建基准 —— 上游 bench 规模固定（100 节 / ~7KB），
//! 覆盖不到 roadmap 的验收规模（10 万字滚动到中部）与流式行数矩阵（500/2000/10000 行）。
//!
//! 说明：这里测的是 **headless 单帧渲染耗时**，不是真实交互帧率。egui 的帧预算是
//! 16.7ms（60fps），验收标准的 55fps 对应 18.2ms。真实 fps 需在窗口里实测
//! （含合成器与呈现开销），本 bench 给出的是渲染路径本身的量级与增长趋势。

use std::fmt::Write as _;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eframe::egui::{self, Id, RawInput, Rect, ScrollArea, UiBuilder};
use egui_markdown::MarkdownLabel;

/// 生成约 `target_chars` 个字符的混合 Markdown（中英混排，含标题 / 段落 / 列表 /
/// 代码块 / 表格 / 引用），贴近真实技术文档的块型分布。
fn generate_long_doc(target_chars: usize) -> String {
    let mut doc = String::with_capacity(target_chars + 4096);
    let mut section = 0usize;
    while doc.len() < target_chars {
        section += 1;
        match section % 6 {
            0 => {
                doc.push_str(&format!("## 第 {section} 节 标题\n\n"));
                doc.push_str("这是一段用于测量中文排版成本的正文,混排 Latin words 与 **加粗**、`行内代码` 和 [链接](https://example.com)。\n\n");
            }
            1 => {
                doc.push_str("```rust\n");
                doc.push_str("fn claim(log: &Log, seq: u64) -> Result<(), ClaimError> {\n");
                for i in 0..12 {
                    let _ = doc.write_fmt(format_args!("    let step_{i} = log.tail()?;\n"));
                }
                doc.push_str("    log.insert(seq)\n}\n```\n\n");
            }
            2 => {
                doc.push_str("- 列表项一\n- 列表项二\n  - 嵌套项\n- 列表项三\n\n");
            }
            3 => {
                doc.push_str("| 列 A | 列 B | 列 C |\n|-------|-------|-------|\n");
                doc.push_str("| 单元格 | 单元格 | 单元格 |\n| 单元格 | 单元格 | 单元格 |\n\n");
            }
            4 => {
                doc.push_str(
                    "> 引用块,含 **加粗** 与一段较长文字,用于检验块级容器的排版与换行开销。\n\n",
                );
                doc.push_str("---\n\n");
            }
            _ => {
                doc.push_str("正文段落:雾凇沆砀,天与云与山与水,上下一白。湖上影子,惟长堤一痕、湖心亭一点。\n\n");
            }
        }
    }
    doc
}

/// 生成 `lines` 行的 rust 代码 fence，用于流式追加的规模矩阵。
fn code_fence(lines: usize) -> String {
    let mut body =
        String::from("```rust\nfn claim(log: &Log, seq: u64) -> Result<(), ClaimError> {\n");
    for i in 0..lines {
        let _ = body.write_fmt(format_args!("    let step_{i} = log.tail()?;\n"));
    }
    body.push_str("    log.insert(seq)\n}\n```\n");
    body
}

/// 渲染一帧：`scroll` 为 ScrollArea 的垂直偏移，用于模拟「滚动到中部」。
fn frame(ctx: &egui::Context, doc: &str, scroll: f32) {
    let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(700.0, 900.0));
    let _ = ctx.run_ui(
        RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |ui| {
            let mut child = ui.new_child(UiBuilder::new().max_rect(screen));
            ScrollArea::vertical()
                .id_salt("bench-scroll")
                .vertical_scroll_offset(scroll)
                .show(&mut child, |ui| {
                    MarkdownLabel::new(Id::new("bench-md"), black_box(doc))
                        .wrap()
                        .show(ui);
                });
        },
    );
}

/// 10 万字文档：冷首帧（解析 + 全量排版）、稳态帧、滚动到中部。
fn bench_long_doc(c: &mut Criterion) {
    let doc = generate_long_doc(100_000);
    let chars = doc.len();
    eprintln!("long doc: {chars} chars, {} lines", doc.lines().count());

    let mut group = c.benchmark_group("long_doc_100k");
    group.sample_size(10);

    // 冷首帧：新 Context，galley 缓存为空，解析 + 排版全量发生
    group.bench_function("cold_first_frame", |b| {
        b.iter_with_setup(egui::Context::default, |ctx| frame(&ctx, &doc, 0.0))
    });

    // 稳态帧：缓存热，输入不变（对应「停下来不动」的每帧成本）
    let ctx = egui::Context::default();
    for _ in 0..5 {
        frame(&ctx, &doc, 0.0);
    }
    group.bench_function("steady_state_top", |b| b.iter(|| frame(&ctx, &doc, 0.0)));

    // 滚动到中部：验收标准的场景，验证视口剔除在大文档下是否真的只算可见区
    let ctx_mid = egui::Context::default();
    for _ in 0..5 {
        frame(&ctx_mid, &doc, 40_000.0);
    }
    group.bench_function("steady_state_scroll_middle", |b| {
        b.iter(|| frame(&ctx_mid, &doc, 40_000.0))
    });

    group.finish();
}

/// 流式追加的规模矩阵：500 / 2000 / 10000 行下「追加一行后整帧渲染」的成本。
/// 对照上游 bench（固定 100 行起步）看增长是否线性。
fn bench_streaming_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming_append_by_lines");
    group.sample_size(10);

    for lines in [500usize, 2000, 10000] {
        let body = code_fence(lines);
        let ctx = egui::Context::default();
        frame(&ctx, &body, 0.0);

        let mut line_idx = lines;
        group.bench_function(format!("append_1_line_at_{lines}"), move |b| {
            b.iter(|| {
                // 只追加一行的文本（不是重建整篇），再渲染；模拟 LLM 流式吐 token 的稳态
                let mut doc = body.clone();
                let _ = doc.write_fmt(format_args!("    let step_{line_idx} = log.tail()?;\n"));
                line_idx += 1;
                frame(&ctx, &doc, 0.0);
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_long_doc, bench_streaming_scale);
criterion_main!(benches);
