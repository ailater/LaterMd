//! M0 主验证 2 的**交互帧率**实测载体(roadmap 验收:「10 万字 md 滚动到中部 ≥ 55fps」)。
//!
//! 与 `benches/longdoc.rs` 的分工:bench 测的是 headless 单帧渲染耗时;这里跑**真实
//! 窗口**,含合成器、呈现与 wgpu 提交,每帧计时并输出分位数。egui 默认按需重绘,
//! 故每帧显式 `request_repaint()` 打满帧率。
//!
//! 这是验证载体,不是产品代码:不进 bin,不接 State/Message,用完可删。
//!
//! ```bash
//! cargo run --release --example scrollbench      # Ctrl+C 或等 20 秒自动退出
//! ```
//!
//! **本机限制(读数据时必须一并看)**:Deepin 上 Vulkan loader 只枚举出 llvmpipe
//! (软件 adapter),硬件 Vulkan 未启用,所以这里测到的是**软件渲染下的下限**,
//! 不是真机 GPU 的表现。真机需重跑本命令。

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use eframe::egui::{self, Id, ScrollArea};
use egui_markdown::MarkdownLabel;

/// 与 benches/longdoc.rs 同一生成逻辑:中英混排的块型分布。
fn generate_long_doc(target_chars: usize) -> String {
    let mut doc = String::with_capacity(target_chars + 4096);
    let mut section = 0usize;
    while doc.len() < target_chars {
        section += 1;
        match section % 6 {
            0 => {
                doc.push_str(&format!("## 第 {section} 节 标题\n\n"));
                doc.push_str("这是一段中文正文,混排 Latin words 与 **加粗**、`行内代码`。\n\n");
            }
            1 => {
                doc.push_str(
                    "```rust\nfn claim(log: &Log, seq: u64) -> Result<(), ClaimError> {\n",
                );
                for i in 0..12 {
                    let _ = doc.write_fmt(format_args!("    let step_{i} = log.tail()?;\n"));
                }
                doc.push_str("    log.insert(seq)\n}\n```\n\n");
            }
            2 => doc.push_str("- 列表项一\n- 列表项二\n  - 嵌套项\n- 列表项三\n\n"),
            3 => {
                doc.push_str("| 列 A | 列 B | 列 C |\n|-------|-------|-------|\n");
                doc.push_str("| 单元格 | 单元格 | 单元格 |\n\n");
            }
            4 => doc.push_str("> 引用块,用于检验块级容器的排版与换行开销。\n\n---\n\n"),
            _ => doc.push_str("正文段落:雾凇沆砀,天与云与山与水,上下一白。\n\n"),
        }
    }
    doc
}

/// 预热帧数:覆盖字体加载、10 万字首次排版、以及从顶部冲到中部的滚动段。
const WARMUP_FRAMES: u64 = 120;

struct ScrollBench {
    doc: String,
    /// 当前滚动偏移(px),每帧推进,到中部后在附近来回微滚。
    offset: f32,
    direction: f32,
    /// 每帧耗时样本(ms),跳过 warmup 后的滚动期帧。
    samples: Vec<f32>,
    /// 帧序号:前 [`WARMUP_FRAMES`] 帧(字体加载 / 首次排版 / 冲到中部)不计入样本。
    frame_index: u64,
    last_frame: Option<Instant>,
    started: Instant,
    /// 是否已打印过统计(避免重复)。
    reported: bool,
}

impl eframe::App for ScrollBench {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_index += 1;
        let now = Instant::now();
        // warmup 帧里混着字体加载、10 万字首次排版与「冲到中部」的滚动,
        // 它们不代表稳态滚动,计入会污染分位数
        if self.frame_index > WARMUP_FRAMES {
            if let Some(last) = self.last_frame {
                self.samples
                    .push(now.duration_since(last).as_secs_f32() * 1000.0);
            }
        }
        self.last_frame = Some(now);

        // 打满帧率:egui 默认按需重绘,不请求就只剩输入事件触发的零星帧
        ui.ctx().request_repaint();

        // 推进滚动:先冲到中部(约 40,000px),之后在附近来回微滚模拟持续滚动
        if self.offset < 40_000.0 {
            self.offset += 400.0;
        } else {
            self.offset += 12.0 * self.direction;
            if self.offset > 44_000.0 || self.offset < 36_000.0 {
                self.direction = -self.direction;
            }
        }
        let offset = self.offset;

        ScrollArea::vertical()
            .id_salt("scrollbench")
            .vertical_scroll_offset(offset)
            .show(ui, |ui| {
                MarkdownLabel::new(Id::new("scrollbench-md"), &self.doc)
                    .wrap()
                    .show(ui);
            });

        // 跑满 20 秒后收尾:打印统计并关闭窗口
        if !self.reported && self.started.elapsed() > Duration::from_secs(20) {
            self.reported = true;
            report(&mut self.samples);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// 输出分位数:p50 / p95 / p99 / max 的帧耗时(ms)与折算 fps。
fn report(samples: &mut [f32]) {
    if samples.is_empty() {
        eprintln!("scrollbench: 没有采到帧");
        return;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pick = |q: f32| {
        let idx = ((samples.len() - 1) as f32 * q).round() as usize;
        samples[idx]
    };
    let total: f32 = samples.iter().sum();
    let mean = total / samples.len() as f32;

    eprintln!("--- scrollbench 结果 ---");
    eprintln!("帧数: {} (含启动与冲到中部的阶段)", samples.len());
    eprintln!("平均: {mean:.2} ms  ({:.1} fps)", 1000.0 / mean);
    for (name, q) in [("p50", 0.5), ("p95", 0.95), ("p99", 0.99)] {
        let ms = pick(q);
        eprintln!("{name}: {ms:.2} ms  ({:.1} fps)", 1000.0 / ms);
    }
    let max = samples[samples.len() - 1];
    eprintln!("max: {max:.2} ms  ({:.1} fps)", 1000.0 / max);
    eprintln!("验收线 55fps = 18.18 ms/帧");
}

fn main() -> eframe::Result<()> {
    // 前几帧含字体加载与首次排版,单独看分位数即可,不做特殊剔除
    let doc = generate_long_doc(100_000);
    eprintln!(
        "scrollbench: 文档 {} 字符 / {} 行",
        doc.len(),
        doc.lines().count()
    );
    eframe::run_native(
        "LaterMD scrollbench",
        eframe::NativeOptions::default(),
        Box::new(move |_cc| {
            Ok(Box::new(ScrollBench {
                doc,
                offset: 0.0,
                direction: 1.0,
                samples: Vec::new(),
                frame_index: 0,
                last_frame: None,
                started: Instant::now(),
                reported: false,
            }))
        }),
    )
}
