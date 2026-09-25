//! 预览面板:vendored `MarkdownLabel` 渲染快照 + AI 指令卡。
//!
//! 两类 AI 入口共用 [`AiLinkHandler`]:`ai://` 链接的样式区分与点击拦截,
//! 以及 AI 指令块(info string 为 ai 的围栏)的卡片渲染(vendored 代码块级 block widget 扩展点,
//! vendor/README.md 差异表 #7:命中 info string 的围栏成为独立 segment,
//! 由本模块画卡)。卡片「执行」产出 [`Message::AiLinkClicked`],与链接点击
//! 同一条归约(流式续写、防重入都在归约侧)。

use crate::ai::AiState;
use crate::ai_link::{self, SCHEME};
use crate::state::{Message, PreviewState};
use eframe::egui;
use egui_markdown::link::{LinkHandler, LinkStyle};
use egui_markdown::MarkdownLabel;
use std::cell::{Cell, RefCell};

/// ai:// 链接的样式色(紫罗兰,与默认超链接色区分),按明暗主题取两档。
fn ai_link_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(0xC9, 0x9B, 0xF5)
    } else {
        egui::Color32::from_rgb(0x8B, 0x2F, 0xC9)
    }
}

/// 指令卡「已完成」状态色(绿),按明暗主题取两档,取色法同 [`ai_link_color`]。
fn done_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(0x7D, 0xCE, 0x8A)
    } else {
        egui::Color32::from_rgb(0x1E, 0x7E, 0x34)
    }
}

/// AI 指令块的 info string 判定:首词为 `ai` 即命中。vendor parser 把
/// Fenced 围栏的完整 info string 原样存进 `Token::CodeBlock.language`
/// (parser.rs `Tag::CodeBlock` 分支),空 info 与缩进代码块是 `None`;
/// `aifoo` 不命中。与 `ai://` scheme 一致大小写敏感:认不出就不当指令卡,
/// 留给普通代码块渲染。
fn is_instruction_info(language: Option<&str>) -> bool {
    language.is_some_and(|info| info.split_whitespace().next() == Some("ai"))
}

/// 卡片内单条指令展示的字符上限:超长指令(整段文章粘进块里等)不再为它
/// 撑高预览,超出部分以 … 收尾。执行用的是完整原文,截断只影响展示。
const INSTRUCTION_DISPLAY_CHARS: usize = 240;

/// 指令文本的展示截断(字符计数,CJK 同算一个字符)。
fn truncate_instruction(instruction: &str) -> String {
    if instruction.chars().count() <= INSTRUCTION_DISPLAY_CHARS {
        return instruction.to_owned();
    }
    let mut cut: String = instruction
        .chars()
        .take(INSTRUCTION_DISPLAY_CHARS)
        .collect();
    cut.push('…');
    cut
}

/// 指令卡三态。判定与 `AiState::last_prompt` 绑定(见 [`AiLinkHandler::card_status`]),
/// 「进行中」样式从简:换色文字,不加 spinner。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiCardStatus {
    /// 从未(或已换文档/失败复位)发起。
    Idle,
    /// 本卡发起的流在途。
    Running,
    /// 本卡发起的流已成功收尾。
    Done,
}

/// vendored [`LinkHandler`] 的 ai:// 扩展点:链接样式区分 + 点击拦截 +
/// AI 指令卡渲染。
///
/// `click` 只有 `&self`,产出的消息暂存 [`RefCell`],帧末由 [`ui`] 并入
/// outbox(下一帧归约执行)。返回 true 拦下 vendored 层的默认
/// `open_url`;非 ai:// 前缀返回 false,链接照常走系统浏览器。协议语义见
/// `ai_link` 模块文档。
struct AiLinkHandler {
    clicked: RefCell<Vec<Message>>,
    color: egui::Color32,
    /// AI 是否在流(卡片「进行中」判据,取自 [`AiState::is_streaming`])。
    streaming: bool,
    /// 最近一次真实发起的 prompt(卡片状态匹配键,取自 [`AiState::last_prompt`])。
    last_prompt: Option<String>,
    /// 本帧已渲染的指令卡数:卡片序号 = 文档序,是 widget id 的稳定成分
    /// (AGENTS.md §6.7:绝不含内容长度 —— 编辑指令文本不改序号,id 不变)。
    card_count: Cell<usize>,
    /// 本帧各卡片的 widget id(渲染序);测试借它断言 id 稳定性。
    card_ids: RefCell<Vec<egui::Id>>,
}

impl AiLinkHandler {
    fn new(color: egui::Color32, ai: &AiState) -> Self {
        Self {
            clicked: RefCell::new(Vec::new()),
            color,
            streaming: ai.is_streaming(),
            last_prompt: ai.last_prompt.clone(),
            card_count: Cell::new(0),
            card_ids: RefCell::new(Vec::new()),
        }
    }

    /// 帧末收口:把点击消息并入 outbox。
    fn drain_into(&self, outbox: &mut Vec<Message>) {
        outbox.append(&mut self.clicked.borrow_mut());
    }

    /// 指令卡状态:指令文本与最近一次发起的 prompt 相等才认领 —— 防重入
    /// 保证同时至多一个流,菜单入口(`AiStart`)的 prompt 是文档尾部拼装,
    /// 不会与任何指令文本相等,其它卡片不受牵连。同一文本的多张卡同状态,
    /// 是接受的简化(decisions-pending #13)。
    fn card_status(&self, instruction: &str) -> AiCardStatus {
        if self.last_prompt.as_deref() == Some(instruction) {
            if self.streaming {
                AiCardStatus::Running
            } else {
                AiCardStatus::Done
            }
        } else {
            AiCardStatus::Idle
        }
    }

    /// 卡片「执行」→ [`Message::AiLinkClicked`]:与 ai:// 链接同一消息、
    /// 同一归约(Ok = 发起流式续写;空指令的按钮是禁用的,到不了这里)。
    fn request_execute(&self, instruction: &str) {
        self.clicked.borrow_mut().push(Message::AiLinkClicked {
            prompt: Ok(instruction.to_owned()),
        });
    }
}

impl LinkHandler for AiLinkHandler {
    /// ai:// 链接换色;`underline: true` 只是声明意图 —— vendored 层当前
    /// 未消费该字段(hover 下划线对全部链接无条件绘制),见 decisions-pending #11。
    fn link_style(&self, href: &str) -> Option<LinkStyle> {
        href.starts_with(SCHEME).then_some(LinkStyle {
            color: Some(self.color),
            underline: true,
        })
    }

    fn click(&self, _text: &str, href: &str, _ui: &mut egui::Ui) -> bool {
        match ai_link::parse(href) {
            Some(prompt) => {
                self.clicked
                    .borrow_mut()
                    .push(Message::AiLinkClicked { prompt });
                true
            }
            None => false,
        }
    }

    fn is_block_code_widget(&self, language: Option<&str>) -> bool {
        is_instruction_info(language)
    }

    fn block_code_widget(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        language: Option<&str>,
    ) -> Option<egui::Response> {
        debug_assert!(
            is_instruction_info(language),
            "分段侧已按 info string 过滤,两侧条件不同步是 vendor 回归"
        );
        let index = self.card_count.get();
        self.card_count.set(index + 1);
        let instruction = text.trim();
        let status = self.card_status(instruction);
        let display = if instruction.is_empty() {
            "(空指令)".to_owned()
        } else {
            truncate_instruction(instruction)
        };
        let response = ui.push_id(("ai_instruction_block", index), |ui| {
            self.card_ids.borrow_mut().push(ui.id());
            egui::Frame::NONE
                .fill(ui.visuals().faint_bg_color)
                .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                .corner_radius(ui.visuals().widgets.noninteractive.corner_radius)
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.strong("AI 指令");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            status_label(ui, status, self.color);
                        });
                    });
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(display).wrap());
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let response =
                            ui.add_enabled(!instruction.is_empty(), egui::Button::new("执行"));
                        if response.clicked() {
                            self.request_execute(instruction);
                        } else if instruction.is_empty() {
                            response.on_disabled_hover_text("指令块没有文本");
                        }
                    });
                })
                .response
        });
        Some(response.inner)
    }
}

/// 状态行文字与配色:未执行弱化、进行中用 AI 紫罗兰、完成用绿。
fn status_label(ui: &mut egui::Ui, status: AiCardStatus, ai_color: egui::Color32) {
    match status {
        AiCardStatus::Idle => {
            ui.weak("未执行");
        }
        AiCardStatus::Running => {
            ui.colored_label(ai_color, "进行中…");
        }
        AiCardStatus::Done => {
            ui.colored_label(done_color(ui.visuals().dark_mode), "已完成");
        }
    }
}

/// 绘制预览面板。
pub fn ui(panel: &mut egui::Ui, preview: &PreviewState, ai: &AiState, outbox: &mut Vec<Message>) {
    egui::ScrollArea::vertical()
        .id_salt("preview-scroll")
        // 不收缩宽度,让 wrap 以面板宽为界
        .auto_shrink([false, false])
        .show(panel, |ui| {
            // widget id 必须是常量:绝不含内容长度/hash,否则每次编辑都
            // 清空 vendored 层临时缓存,增量高亮与分段缓存全部失效
            // (AGENTS.md §6.7)。内容变化已在上游按修订号节流,这里每帧
            // 拿到的都是"仅在变化时重建"的同一字符串。
            let handler = AiLinkHandler::new(ai_link_color(ui.visuals().dark_mode), ai);
            MarkdownLabel::new(egui::Id::new("preview-md"), &preview.text)
                .wrap()
                // heal:true = 每帧渲染前对整篇文本补闭合(vendored parser::heal),
                // AI 流式输出的残缺帧(未闭合 fence/加粗)语法合法,完整文档
                // 上是恒等变换(Cow::Borrowed 原样返回)。P1 流式的必需品
                // (AGENTS.md §6.5),岔路登记见 docs/decisions-pending.md #10。
                .heal(true)
                .link_handler(&handler)
                .show(ui);
            handler.drain_into(outbox);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::RawInput;

    /// 造一个指定流式/最近 prompt 的 AI 状态(卡片状态的三个输入)。
    fn ai_state(streaming: bool, last_prompt: Option<&str>) -> AiState {
        AiState {
            runtime: crate::ai::AiRuntime::Mock(latermd_ai::MockProvider::new()),
            rx: None,
            streaming,
            last_prompt: last_prompt.map(str::to_owned),
            config: crate::ai_config::AiConfig::default(),
        }
    }

    /// handler 三态:ai:// 链接解析暂存且被拦截;非 ai:// 前缀不拦截(返回
    /// false,vendored 默认走系统浏览器);link_style 只对 ai:// 换色。
    #[test]
    fn handler_intercepts_ai_links_and_passes_through_others() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        ctx.run_ui(RawInput::default(), |ui| {
            let handler =
                AiLinkHandler::new(ai_link_color(ui.visuals().dark_mode), &AiState::default());
            assert!(handler.click("续写", "ai://write?prompt=%E7%BB%AD%E5%86%99", ui));
            assert!(
                !handler.click("LaterMD", "https://github.com/ailater/LaterMd", ui),
                "非 ai:// 前缀不拦截"
            );
            let style = handler.link_style("ai://write?prompt=x").unwrap();
            assert_eq!(style.color, Some(ai_link_color(true)));
            assert!(style.underline);
            assert!(
                handler.link_style("https://example.com").is_none(),
                "普通链接保持默认超链接样式"
            );
            handler.drain_into(&mut outbox);
        })
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::AiLinkClicked {
                prompt: Ok("续写".into())
            }]
        );
    }

    /// info string 判定:首词 `ai` 命中(带参数也行),语言名/大小写/空 info
    /// 不命中,与 `ai://` scheme 的大小写敏感口径一致。
    #[test]
    fn instruction_info_matches_first_word_only() {
        for hit in [Some("ai"), Some("ai title=演示"), Some(" ai ")] {
            assert!(is_instruction_info(hit), "{hit:?} 应命中");
        }
        for miss in [
            None,
            Some(""),
            Some("rust"),
            Some("aifoo"),
            Some("AI"),
            Some("markdown ai"),
        ] {
            assert!(!is_instruction_info(miss), "{miss:?} 不应命中");
        }
    }

    /// 展示截断:不超上限原样返回;超上限取前 N 个字符加 …(按字符计数,
    /// CJK 不被劈开)。
    #[test]
    fn truncate_instruction_caps_by_chars() {
        let short = "续写".repeat(10);
        assert_eq!(truncate_instruction(&short), short);

        let long = "字".repeat(INSTRUCTION_DISPLAY_CHARS + 5);
        let cut = truncate_instruction(&long);
        assert_eq!(cut.chars().count(), INSTRUCTION_DISPLAY_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert!(cut
            .chars()
            .take(INSTRUCTION_DISPLAY_CHARS)
            .all(|c| c == '字'));
    }

    /// 卡片状态派生:匹配最近 prompt 才认领;流式中「进行中」、收尾「已完成」、
    /// 失败/换文档复位后(prompt 清空)与无关指令都是「未执行」。
    #[test]
    fn card_status_follows_last_prompt_and_streaming() {
        let color = ai_link_color(true);
        let idle = AiLinkHandler::new(color, &ai_state(false, None));
        assert_eq!(idle.card_status("续写"), AiCardStatus::Idle);

        let running = AiLinkHandler::new(color, &ai_state(true, Some("续写")));
        assert_eq!(running.card_status("续写"), AiCardStatus::Running);

        let done = AiLinkHandler::new(color, &ai_state(false, Some("续写")));
        assert_eq!(done.card_status("续写"), AiCardStatus::Done);

        // 其它卡片不受牵连:菜单发起的 prompt 是拼装文本,不等任何指令
        let unrelated = AiLinkHandler::new(color, &ai_state(true, Some("请续写以下文档内容:\n……")));
        assert_eq!(unrelated.card_status("续写"), AiCardStatus::Idle);
    }

    /// 汇集一帧里画出的全部文本(卡片与普通代码块都靠 Text shape 呈现)。
    fn painted_text(output: &eframe::egui::FullOutput) -> Vec<String> {
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

    /// 含 AI 指令块的文档整帧渲染不 panic,卡片三要素(标题/指令/按钮)都在,
    /// 普通代码块不受牵连;空指令块的按钮禁用路径同样只渲染不执行。
    #[test]
    fn ai_block_renders_card_without_panic() {
        let doc =
            "```rust\nfn main() {}\n```\n\n```ai\n续写一段 Markdown 介绍\n```\n\n```ai\n\n```\n";
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |panel| {
            let preview = PreviewState {
                text: doc.to_owned(),
                synced_rev: 0,
                outline: Vec::new(),
            };
            ui(panel, &preview, &AiState::default(), &mut outbox);
        });
        let painted = painted_text(&output);
        output.drop_without_applying_deltas();

        for expected in ["AI 指令", "续写一段 Markdown 介绍", "执行", "(空指令)"] {
            assert!(
                painted.iter().any(|t| t.contains(expected)),
                "缺 {expected}:{painted:?}"
            );
        }
        // rust 围栏走原代码块路径(高亮 galley),不在卡片里
        assert!(
            painted.iter().any(|t| t.contains("fn main()")),
            "普通代码块丢失"
        );
        // 渲染帧没有点击,不产消息
        assert!(outbox.is_empty());
    }

    /// 卡片消息路由:「执行」产出 [`Message::AiLinkClicked`](Ok=完整指令原文,
    /// 不受展示截断影响),经 drain_into 进 outbox,与 ai:// 链接同一归约入口。
    #[test]
    fn execute_routes_ai_link_clicked_with_full_instruction() {
        let handler = AiLinkHandler::new(ai_link_color(true), &AiState::default());
        handler.request_execute("总结,本文要点!(含标点)");
        let mut outbox = Vec::new();
        handler.drain_into(&mut outbox);
        assert_eq!(
            outbox,
            vec![Message::AiLinkClicked {
                prompt: Ok("总结,本文要点!(含标点)".into())
            }]
        );
    }

    /// 卡片 widget id 的稳定性(AGENTS.md §6.7 的证据):id 由「块在文档中的
    /// 序号」构成 —— 编辑指令文本本身、或在卡片后增删内容,序号与 id 都不变;
    /// 多张卡按文档序得到互异 id。id 里绝不含内容长度,否则每次编辑都清空
    /// vendored 层缓存。
    #[test]
    fn card_id_is_index_based_and_stable_across_edits() {
        let ctx = egui::Context::default();

        // 与生产 preview::ui 同配置(MarkdownLabel + heal + handler)渲染一帧,
        // 取回 handler 记录的卡片 id。省掉 ScrollArea 外壳不影响结论:
        // push_id 是相对父 ui 的,稳定性断言看的是相对成分。
        let render = |doc: &str| {
            let handler = AiLinkHandler::new(ai_link_color(true), &AiState::default());
            ctx.run_ui(RawInput::default(), |ui| {
                MarkdownLabel::new(egui::Id::new("preview-md"), doc)
                    .wrap()
                    .heal(true)
                    .link_handler(&handler)
                    .show(ui);
            })
            .drop_without_applying_deltas();
            let ids = handler.card_ids.borrow().clone();
            ids
        };

        let doc_a = "# 标题\n\n```ai\nalpha\n```\n";
        let ids_a = render(doc_a);
        assert_eq!(ids_a.len(), 1, "一张指令卡");

        // 编辑指令文本(长度变了)+ 卡片后追加内容:序号仍是 0,id 不变
        let doc_b = "# 标题\n\n```ai\nalpha beta —— 一段长了很多的指令文本\n```\n\n后续段落。\n";
        let ids_b = render(doc_b);
        assert_eq!(ids_b, ids_a, "id 只由文档序构成,不含内容");

        // 两张卡:文档序互异 id
        let doc_c = "```ai\nfirst\n```\n\n```ai\nsecond\n```\n";
        let ids_c = render(doc_c);
        assert_eq!(ids_c.len(), 2);
        assert_ne!(ids_c[0], ids_c[1]);
    }
}
