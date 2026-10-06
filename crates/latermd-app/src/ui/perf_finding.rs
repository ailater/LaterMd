//! #59 perf-round M1「全面取证」的无头测量 harness(纯测试模块,生产构建不编译)。
//!
//! 补 [`tab_switch_perf`](切换首帧/稳态)未覆盖的四个剖面,手法同源:整帧走
//! 生产路径([`LaterMdApp::draw`]),单件走 `ctx.run_ui`,分段计时全部用公开
//! API(`egui_markdown::{heal, parse, block_span_rects, section_anchors}`、
//! `latermd_md::{expand_wikilinks, outline}`),不触碰 vendored 一行。
//!
//! 四剖面:
//! **[EDIT] 大文档编辑帧**:生产配置 multiline TextEdit,稳态 / 键入一字 /
//! 换文本,5k/10k/20k 三档看每次键入成本随规模的增长律;
//! **[SCROLL] 滚动稳态帧**:预览单件(生产 wrap 口径)在多个滚动偏移的稳态
//! 成本 + 连续滚动序列;并用公开 API `block_span_rects`/`section_anchors`
//! 数出稳态帧实际记录的块数/锚点数(视口内外全量记录的证据);
//! **[COLDSWITCH] 冷首切构成分解**:expand_wikilinks/heal/parse 公开侧 +
//! 整帧冷帧,差值归「layout+高亮+缓存构建」;热帧对照(视口外延迟布局路线
//! 的可挽回上界 = 冷 − 热,decisions-pending #60 已写明该路线);
//! **[STARTUP] 应用启动(app 侧可测)**:状态构造 / 首帧(冷,含字体图集)/
//! 稳态帧 / 打开 20k 行文档的 open_tab + 首/次帧。eframe/wgpu/窗口创建与
//! `fonts::install` 属原生启动路径,无头不可测(blocked_external),数字只
//! 覆盖状态构造与 UI 帧侧。
//!
//! 跑法(计时口径 = release;debug 构建数字大 ~10×,只可作相对对照):
//! `cargo test -p latermd-app --release perf_finding -- --test-threads=1 --ignored --nocapture`

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use eframe::egui::{self, Id, RawInput, Rect, ScrollArea};
use egui_markdown::MarkdownLabel;

use crate::state::Message;
use crate::LaterMdApp;

/// 无头屏幕:与 [`tab_switch_perf`] 同一 1280×800,数字可与那份报告互对照。
fn screen() -> Rect {
    Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::new(1280.0, 800.0))
}

/// 中英混排样本:与 `tab_switch_perf::sample_doc` 同构(标题/段落/代码块/
/// 表格/列表/引用循环,30 行一节),保证两份 harness 的数字可直接互比。
fn sample_doc(seed: u32, target_lines: usize) -> String {
    let mut doc = String::with_capacity(640 * 1024);
    let mut lines = 0;
    let mut section = 0;
    while lines < target_lines {
        section += 1;
        let zh = match section % 3 {
            0 => "渲染层缓存与视口剔除",
            1 => "解析器的分段契约",
            _ => "编辑器缓冲与撤销栈",
        };
        let _ = writeln!(doc, "## 第 {seed}-{section} 节 {zh} (section {section})");
        let _ = writeln!(doc);
        let _ = writeln!(
            doc,
            "这是一段用于测量的中文正文,混排 English words 与 **加粗**、\
             `行内代码`、[链接](https://example.com/{seed}/{section})、\
             [[设计决策-{seed}-{section}]] 与 ![配图](./{seed}.assets/fig-{section}.png)。"
        );
        let _ = writeln!(doc);
        let _ = writeln!(doc, "```rust");
        let _ = writeln!(
            doc,
            "fn claim_{section}(log: &Log, seq: u64) -> Result<(), Error> {{"
        );
        for step in 0..12 {
            let _ = writeln!(doc, "    let step_{step} = log.tail()?;");
        }
        let _ = writeln!(doc, "    log.insert(seq)");
        let _ = writeln!(doc, "}}");
        let _ = writeln!(doc, "```");
        let _ = writeln!(doc);
        let _ = writeln!(doc, "| 列甲 | 列乙 | 列丙 |");
        let _ = writeln!(doc, "|---|---|---|");
        for row in 1..=2 {
            let _ = writeln!(doc, "| 单元 {row}-{seed} | cell {row} | {row}{row} |");
        }
        let _ = writeln!(doc);
        let _ = writeln!(
            doc,
            "- 列表项一 {}\n- 列表项二\n  - 嵌套项 {section}\n- 列表项三",
            section
        );
        let _ = writeln!(doc);
        let _ = writeln!(
            doc,
            "> 引用块 {section}:检验块级容器的排版与换行开销,中英混排 mixed text。\n"
        );
        lines += 30;
    }
    doc
}

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn fmt_us(d: Duration) -> String {
    let us = d.as_nanos() as f64 / 1_000.0;
    if us < 1_000.0 {
        format!("{us:.1} µs")
    } else {
        format!("{:.2} ms", us / 1_000.0)
    }
}

fn row(name: &str, d: Duration) {
    println!("  {name:<62} {}", fmt_us(d));
}

fn run_frame(ctx: &egui::Context, f: impl FnMut(&mut egui::Ui)) {
    ctx.run_ui(
        RawInput {
            screen_rect: Some(screen()),
            ..Default::default()
        },
        f,
    )
    .drop_without_applying_deltas();
}

/// 预览单件一帧,生产稳态口径(per-tab id + wrap + heal 关 —— #39 后稳态
/// 帧不走 heal;`scroll` 为 ScrollArea 垂直偏移)。返回耗时。
fn preview_frame(ctx: &egui::Context, id: &egui::Id, rendered: &str, scroll: f32) -> Duration {
    let start = Instant::now();
    run_frame(ctx, |ui| {
        ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .vertical_scroll_offset(scroll)
            .show(ui, |ui| {
                // 生产同配置:#59 M2 起预览开 defer_offscreen_highlight
                // (vendor ①类,几何零变化,miss 帧视口外段免 syntect)。
                MarkdownLabel::new(*id, rendered)
                    .wrap()
                    .defer_offscreen_highlight(true)
                    .show(ui);
            });
    });
    start.elapsed()
}

/// 编辑器单件一帧(生产配置:multiline + Monospace + 不限宽 + ScrollArea),
/// `buf` 由调用方持有(键入帧先改缓冲再渲染)。返回耗时。
fn editor_frame(ctx: &egui::Context, buf: &mut String) -> Duration {
    let start = Instant::now();
    run_frame(ctx, |ui| {
        ScrollArea::vertical()
            .id_salt("perf-editor-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::TextEdit::multiline(buf)
                    .id(Id::new("perf-editor"))
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(20)
                    .show(ui);
            });
    });
    start.elapsed()
}

/// [EDIT] 大文档编辑帧:三档规模 ×(稳态 / 键入一字),键盘成本的增长律
/// 是「大文档还能不能打字」的直接判据(55fps 预算 18.18ms)。
fn profile_edit() {
    println!("[EDIT] 大文档编辑帧(生产配置 TextEdit 单件,7 帧中位)");
    for lines in [5_000usize, 10_000, 20_000] {
        let doc = sample_doc(1, lines);
        let ctx = egui::Context::default();
        run_frame(&ctx, |_| {});
        let mut buf = doc.clone();
        for _ in 0..3 {
            editor_frame(&ctx, &mut buf);
        }
        let steady = {
            let mut samples = Vec::new();
            for _ in 0..7 {
                samples.push(editor_frame(&ctx, &mut buf));
            }
            median(samples)
        };
        let typed = {
            let mut samples = Vec::new();
            for i in 0..7 {
                // 每帧在缓冲尾键入一个字符:TextEdit 检测文本变化 → 整篇重排。
                let _ = write!(buf, "x{i}");
                samples.push(editor_frame(&ctx, &mut buf));
            }
            median(samples)
        };
        let switched = {
            let mut other = sample_doc(2, lines);
            let mut samples = Vec::new();
            for i in 0..5 {
                // 每帧换一处文本:egui Fonts 层 galley 缓存按 job 内容哈希键控,
                // 同文本二帧起即命中(中位数会掩盖 miss),必须逐帧换真文本。
                let _ = write!(other, "\n换文本锚点 {i}\n");
                let mut b = other.clone();
                samples.push(editor_frame(&ctx, &mut b));
            }
            median(samples)
        };
        println!("  -- {lines} 行({} 字节)--", doc.len());
        row("稳态帧(galley 缓存命中)", steady);
        row("键入 1 字符帧(整篇重排)", typed);
        row("换文本首帧(逐帧异文 = 真 miss)", switched);
    }
}

/// [SCROLL] 滚动稳态帧:20k 行预览单件在不同滚动位置的稳态成本 + 连续滚动
/// 序列;随后一帧读回块表/锚点表,数出「每帧记录的块数/锚点数」。
fn profile_scroll(rendered_big: &str) {
    println!("[SCROLL] 滚动稳态帧(20000 行预览单件,id=perf-scroll-md,7 帧中位)");
    let id = Id::new("perf-scroll-md");
    let ctx = egui::Context::default();
    run_frame(&ctx, |_| {});
    // 冷帧先垫一脚:首次渲染的解析/排版成本不混进稳态数字。
    let cold = preview_frame(&ctx, &id, rendered_big, 0.0);
    row("冷首帧(对照,1 帧)", cold);
    for scroll in [0.0f32, 100_000.0, 250_000.0, 400_000.0, 4_000_000.0] {
        let steady = {
            let mut samples = Vec::new();
            for _ in 0..7 {
                samples.push(preview_frame(&ctx, &id, rendered_big, scroll));
            }
            median(samples)
        };
        row(&format!("稳态帧 offset={scroll:.0}"), steady);
    }
    // 连续滚动:每帧推进 800px(≈滚轮一格),取后 24 帧中位(前 6 帧暖缓存)。
    let mut samples = Vec::new();
    let mut offset = 0.0f32;
    for i in 0..30 {
        offset += 800.0;
        let d = preview_frame(&ctx, &id, rendered_big, offset);
        if i >= 6 {
            samples.push(d);
        }
    }
    row("连续滚动帧(800px/帧,24 帧中位)", median(samples));

    // 记录面证据:稳态帧里块表/锚点表各记了多少条(视口只显示 ~40 行)。
    let mut blocks = 0usize;
    let mut anchors = 0usize;
    run_frame(&ctx, |ui| {
        ScrollArea::vertical()
            .id_salt(id.with("scroll"))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                MarkdownLabel::new(id, rendered_big)
                    .wrap()
                    .defer_offscreen_highlight(true)
                    .show(ui);
            });
        blocks = egui_markdown::block_span_rects(ui, id)
            .map(|b| b.len())
            .unwrap_or(0);
        anchors = egui_markdown::section_anchors(ui, id)
            .map(|a| a.len())
            .unwrap_or(0);
    });
    println!("  稳态帧记录面:块表 {blocks} 条 / 锚点 {anchors} 条(视口内可见行 ~40)");
}

/// [COLDSWITCH] 冷首切构成分解:公开侧三段 + 整帧冷帧;layout+高亮+缓存构建
/// 用差值归因(近似口径,注明);热帧对照给出视口外延迟布局的可挽回上界。
fn profile_coldswitch(doc: &str, rendered: &str, tag: &str) {
    println!("[COLDSWITCH] 冷首切构成分解({tag})");
    fn time_n(n: usize, mut f: impl FnMut()) -> Duration {
        let mut samples = Vec::new();
        for _ in 0..n {
            let start = Instant::now();
            f();
            samples.push(start.elapsed());
        }
        median(samples)
    }
    let rendered_owned = rendered.to_owned();
    let expand = time_n(5, || {
        let out = latermd_md::expand_wikilinks(std::hint::black_box(doc));
        std::hint::black_box(&out);
    });
    row("① expand_wikilinks(打开时,快照侧)", expand);
    let heal = time_n(9, || {
        let out = egui_markdown::heal(std::hint::black_box(&rendered_owned));
        std::hint::black_box(&out);
    });
    row("② heal 全文扫描(流式帧才开)", heal);
    let healed = egui_markdown::heal(&rendered_owned);
    let parse = time_n(9, || {
        let out = egui_markdown::parse(std::hint::black_box(healed.as_ref()));
        std::hint::black_box(&out);
    });
    row("③ parse 全文(缓存 miss 时)", parse);
    let id = Id::new("perf-coldswitch-md");
    let ctx = egui::Context::default();
    run_frame(&ctx, |_| {});
    let cold = {
        let mut samples = Vec::new();
        for _ in 0..3 {
            // 每个样本换新 ctx:保证是「该文档第一次被渲染」的冷首帧。
            let fresh = egui::Context::default();
            run_frame(&fresh, |_| {});
            samples.push(preview_frame(&fresh, &id, rendered, 0.0));
        }
        median(samples)
    };
    row("④ 冷首帧整帧(heal+parse+layout+高亮+记录)", cold);
    let hot = {
        let mut samples = Vec::new();
        for _ in 0..7 {
            samples.push(preview_frame(&ctx, &id, rendered, 0.0));
        }
        median(samples)
    };
    row("⑤ 稳态帧(命中,对照)", hot);
    let approx_layout = cold
        .checked_sub(parse)
        .and_then(|d| d.checked_sub(heal))
        .unwrap_or_default();
    println!(
        "  ≈差值归因:④−③−② ≈ layout+高亮+缓存构建 ≈ {}(近似口径:①在快照侧不进帧,记录/哈希含在差值里)",
        fmt_us(approx_layout)
    );
    println!(
        "  视口外延迟布局的可挽回上界(#60 路线)= ④−⑤ = {}",
        fmt_us(cold.saturating_sub(hot))
    );
}

/// [STARTUP] 应用启动(app 侧可测部分):状态构造 / 首帧(冷)/ 稳态 /
/// 打开 20k 行文档的 open_tab + 首/次帧。
fn profile_startup(big_doc: &str) {
    println!(
        "[STARTUP] 应用启动(app 侧;eframe/wgpu/窗口创建与 fonts::install 属原生路径,无头不可测)"
    );
    let start = Instant::now();
    let mut app = LaterMdApp::default();
    let constructed = start.elapsed();
    row("LaterMdApp::default()(状态构造)", constructed);
    let ctx = egui::Context::default();
    let first = {
        let start = Instant::now();
        run_frame(&ctx, |ui| app.draw(ui));
        start.elapsed()
    };
    row("首帧(冷:字体图集+全部面板)", first);
    let steady = {
        let mut samples = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            run_frame(&ctx, |ui| app.draw(ui));
            samples.push(start.elapsed());
        }
        median(samples)
    };
    row("稳态帧(5 帧中位,示例文档)", steady);
    {
        let start = Instant::now();
        let index = app.state.tabs.open_tab(None, big_doc);
        app.state.apply(Message::TabActivate(index));
        let open_t = start.elapsed();
        println!("  open_tab(20000 行)+TabActivate 归约:{open_t:?}(快照同步在建)");
        let start = Instant::now();
        run_frame(&ctx, |ui| app.draw(ui));
        let first_big = start.elapsed();
        row("大文档首帧(缓存全 miss)", first_big);
        let start = Instant::now();
        run_frame(&ctx, |ui| app.draw(ui));
        row("大文档次帧", start.elapsed());
    }
}

/// `#[ignore]` 完整取证报告:跑法见模块头注释。
#[test]
#[ignore = "取证报告;跑法:cargo test -p latermd-app --release perf_finding -- --test-threads=1 --ignored --nocapture"]
fn perf_finding_report() {
    let big = sample_doc(1, 20_000);
    let big_rendered = latermd_md::expand_wikilinks(&big);
    let mid = sample_doc(3, 2_000);
    let mid_rendered = latermd_md::expand_wikilinks(&mid);
    println!(
        "==== #59 M1 全面取证:样本 2000 行({} 字节)/ 20000 行({} 字节),1280×800 无头帧 ====",
        mid.len(),
        big.len()
    );

    profile_edit();
    profile_scroll(&big_rendered);
    profile_coldswitch(&mid, &mid_rendered, "2000 行");
    profile_coldswitch(&big, &big_rendered, "20000 行");
    profile_startup(&big);
    println!("==== 完 ====");
}
