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
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::path::Path;

/// ai:// 链接的样式色(紫罗兰,与默认超链接色区分),按明暗主题取两档。
fn ai_link_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(0xC9, 0x9B, 0xF5)
    } else {
        egui::Color32::from_rgb(0x8B, 0x2F, 0xC9)
    }
}

/// `[[wikilink]]` 的链接色(青绿,与 ai:// 的紫罗兰区分),按明暗主题取两档。
fn wiki_link_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(0x6C, 0xD4, 0xC0)
    } else {
        egui::Color32::from_rgb(0x0F, 0x7A, 0x66)
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

/// 把渲染文本里的**相对图片地址**改写成 `file://` 绝对 URI(docs/image-plan.md
/// §4.1,B 段唯一硬骨头)。
///
/// 文档里存相对路径是对的(`./foo.assets/x.png` 随目录走,可移植),但
/// vendored 层把 url 原样喂 `egui::Image::new`,egui 没有「文档目录」概念,
/// 相对地址一律加载失败。改写发生在**喂给预览之前的字符串层**,源码与
/// `PreviewState` 一字不动;vendored 一行不动(等上游 `Token::Image` 加
/// `base_dir`,vendor/README.md 已登记待上游化)。
///
/// `base_dir` 为 `None`(文档未落盘)或空串(裸相对文件名,`Path::parent`
/// 的产物)时原样借回:没有锚点可拼,交给加载器按失败处理,与改写前一致。
pub(crate) fn resolve_relative_images<'a>(
    text: &'a str,
    base_dir: Option<&'a Path>,
) -> Cow<'a, str> {
    let Some(base_dir) = base_dir.filter(|dir| !dir.as_os_str().is_empty()) else {
        return Cow::Borrowed(text);
    };
    let rewrites: Vec<(Range<usize>, String)> = inline_image_dests(text)
        .into_iter()
        .filter(|(_, dest, _)| is_relative(dest))
        .map(|(span, dest, wrapped)| {
            let mut uri = file_uri(base_dir, &dest);
            // 原 `<…>` 形式保持包裹;拼出来的 URI 含空格时裸目标语法会被
            // 截断,也要包。目标本身就在 `(` 与 `)` 之间,替换串带尖括号
            // 仍是合法 Markdown。
            if wrapped || uri.chars().any(char::is_whitespace) {
                uri = format!("<{uri}>");
            }
            (span, uri)
        })
        .collect();
    if rewrites.is_empty() {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 64);
    let mut cursor = 0;
    for (span, uri) in rewrites {
        out.push_str(&text[cursor..span.start]);
        out.push_str(&uri);
        cursor = span.end;
    }
    out.push_str(&text[cursor..]);
    Cow::Owned(out)
}

/// 扫出全部**内联图片** `![alt](dest)` / `![alt](<dest> "title")` 的目标:
/// `(字节区间, 目标文本, 是否 <…> 包裹)`。区间含 `<>` 包裹(替换时原样
/// 保持包裹形式),不含两侧圆括号。
///
/// 与 `latermd_md::wikilinks` 同款扫描纪律:围栏代码块内不认(代码里的
/// `![](x)` 是字面文本);引用式图片 `![alt][ref]` 目标不在行内,不认;
/// **链接** `[t](x)` 不是图片,不认(链接点击走浏览器,拼 file:// 反而坏)。
/// alt 内嵌套 `![![a](u)](v)` 会把内层也扫出来 —— 内层本来就是独立图片,
/// 正该改写。
fn inline_image_dests(text: &str) -> Vec<(Range<usize>, String, bool)> {
    let mut dests = Vec::new();
    let mut in_code = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let rest = &text[index..];
        let line_start = index == 0 || bytes[index - 1] == b'\n';
        if line_start {
            let trimmed = rest.trim_start_matches([' ', '\t']);
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                in_code = !in_code;
            }
        }
        if !in_code && rest.starts_with("![") {
            // alt 到下一个 `]`(CommonMark 裸 alt 不含 `]`);其后紧跟 `(` 才是
            // 内联图片,否则是引用式或残缺写法,跳过这两个字符继续。
            if let Some(alt_len) = rest[2..].find(']') {
                let paren = 2 + alt_len + 1;
                if rest[paren..].starts_with('(') {
                    if let Some((span, dest, wrapped)) = dest_span(rest, paren + 1) {
                        dests.push((index + span.start..index + span.end, dest, wrapped));
                        index += span.end;
                        continue;
                    }
                }
            }
            index += 2;
            continue;
        }
        let step = rest.chars().next().map_or(1, char::len_utf8);
        index += step;
    }
    dests
}

/// `(` 之后的目标段:跳过前导空白;`<…>` 形式取尖括号内(区间**含**尖括号),
/// 裸形式到首个空白或 `)`。空目标(`![]()`)返回 `None`。
fn dest_span(text: &str, after_paren: usize) -> Option<(Range<usize>, String, bool)> {
    let bytes = text.as_bytes();
    let mut i = after_paren;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    match bytes.get(i) {
        Some(b'<') => {
            let close = i + 1 + text[i + 1..].find('>')?;
            Some((i..close + 1, text[i + 1..close].to_owned(), true))
        }
        Some(_) => {
            let stop = text[i..]
                .find(|c: char| c.is_whitespace() || c == ')')
                .map_or(text.len(), |offset| i + offset);
            (stop > i).then(|| (i..stop, text[i..stop].to_owned(), false))
        }
        None => None,
    }
}

/// 目标是否**相对**:无 scheme(含 `wiki://`、`data:`、`ai://`)、非 Windows
/// 盘符、非 `/` 绝对路径、非纯锚点。首段含 `:` 即视为有 scheme ——
/// `https:`、`wiki:`、`C:` 一网打尽。
fn is_relative(dest: &str) -> bool {
    !dest.is_empty()
        && !dest.starts_with('#')
        && !dest.starts_with('/')
        && !dest.split('/').next().unwrap_or(dest).contains(':')
}

/// 相对目标 → `file://` URI:按文档目录拼绝对路径(词法拼接,不做
/// canonicalize —— 那是磁盘 IO 且会解析符号链接,渲染定位不需要)。
/// 路径分隔符统一 `/`,Windows 盘符路径补前导 `/`(`file:///C:/…`,
/// egui FileLoader 的解析约定)。
fn file_uri(base_dir: &Path, dest: &str) -> String {
    let cleaned = dest.trim_start_matches("./");
    let path = base_dir.join(cleaned);
    let unified = path.to_string_lossy().replace('\\', "/");
    if unified.starts_with('/') {
        format!("file://{unified}")
    } else {
        format!("file:///{unified}")
    }
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
    /// 当前明暗(wikilink 取色用,与 ai:// 走两套色)。
    dark_mode: bool,
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
    fn new(color: egui::Color32, dark_mode: bool, ai: &AiState) -> Self {
        Self {
            clicked: RefCell::new(Vec::new()),
            color,
            dark_mode,
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
        // [[wikilink]] 用青绿,与 ai:// 的紫罗兰区分:两种链接的点击后果不同
        // (一个开文档、一个发起 AI 流),颜色不该撞
        if href.starts_with(latermd_md::WIKI_SCHEME) {
            return Some(LinkStyle {
                color: Some(wiki_link_color(self.dark_mode)),
                underline: true,
            });
        }
        href.starts_with(SCHEME).then_some(LinkStyle {
            color: Some(self.color),
            underline: true,
        })
    }

    fn click(&self, _text: &str, href: &str, _ui: &mut egui::Ui) -> bool {
        if let Some(target) = href.strip_prefix(latermd_md::WIKI_SCHEME) {
            let target = target.trim();
            if !target.is_empty() {
                self.clicked.borrow_mut().push(Message::WikilinkClicked {
                    target: target.to_owned(),
                });
                return true;
            }
        }
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

// —— 代码块复制头(docs/auto-plan.md #38)——

/// 「已复制」✓ 反馈的存活窗口:点击后停留 1.5s 自行消退,不需要用户再
/// 交互确认。
const COPIED_FEEDBACK: std::time::Duration = std::time::Duration::from_millis(1500);
/// 点击帧后再排一短帧:点击帧里按钮已按普通态画完,✓ 要等下一帧才上屏,
/// 不主动排程的话它得等到下一次用户输入。
const COPIED_FEEDBACK_DELAY: std::time::Duration = std::time::Duration::from_millis(16);

/// 反馈态在 egui data 的键:全局单份「最近复制块的指纹 + 到期时刻」。
/// 按块内容而非 widget id 认领 —— vendored 挂载点的子 Ui id 按帧内序号
/// 自动分配,文档编辑后块序平移会让 id 键控的反馈错位到别的块;内容指纹
/// 最多让同文本多块同显 ✓(与 ```ai 卡片按指令文本认领状态同款简化)。
fn copied_feedback_id() -> egui::Id {
    egui::Id::new("latermd-code-copy-feedback")
}

/// 探针在 egui data 的键:本帧各复制按钮的 rect(照 vendored
/// `section_anchors` 的 data 手法 —— 生产侧写入开销一次 Vec)。读法见
/// [`copy_button_probe`]/[`copy_button_rects`]。每帧首写按帧号重置。
fn copy_button_rects_id() -> egui::Id {
    egui::Id::new("latermd-code-copy-button-rects")
}

/// 原始探针:(最后写入帧号, 该帧按钮 rect 清单)。写入发生在渲染帧内,
/// 而测试在 `run_ui` 返回之后才读 —— 那时帧号已前进(`end_pass` 处 +1),
/// 帧号核对必然失配,故测试按「最后写入者即本帧」直接取清单。
pub(crate) fn copy_button_probe(ctx: &egui::Context) -> (u64, Vec<egui::Rect>) {
    ctx.data(|d| {
        d.get_temp::<(u64, Vec<egui::Rect>)>(copy_button_rects_id())
            .unwrap_or_default()
    })
}

/// 当前帧的按钮 rect 清单(帧号核对通过的才返回):供 Live 列「点击进
/// 编辑」在帧内排除按钮命中(live.rs `clicked_for_edit`)。零按钮帧返回
/// 空 —— 比如全部代码块都转入编辑态时,上一帧的旧 rect 不得再拦点击。
pub(crate) fn copy_button_rects(ctx: &egui::Context) -> Vec<egui::Rect> {
    let (frame, rects) = copy_button_probe(ctx);
    if frame == ctx.cumulative_pass_nr() {
        rects
    } else {
        Vec::new()
    }
}

fn code_fingerprint(code: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut hasher);
    hasher.finish()
}

/// 代码块头部一行(常驻):复制按钮 + 语言标签,挂在 vendored
/// `MarkdownLabel::code_block_buttons` 挂载点 —— 该挂载点上游自带(subtree
/// 引入即有),普通代码块右上角悬浮调用,回调直接收到 `(块源文本, 语言)`,
/// 无需从 `code_block_spans` 切偏移;```ai 指令卡走 `block_code_widget` 另
/// 一条路,不经过这里。
///
/// 点击 → `Context::copy_text` 整块源文本(vendored parser 已去块尾换行)
/// → 按钮进入 ✓ 反馈态。无语言/空块同样给按钮:复制空串是合法操作,
/// 交互一致比按内容藏按钮好猜。
pub(crate) fn code_copy_buttons(ui: &mut egui::Ui, code: &str, lang: &str) {
    let now = std::time::Instant::now();
    let until = ui
        .ctx()
        .data(|d| d.get_temp::<(u64, std::time::Instant)>(copied_feedback_id()))
        .filter(|(hash, until)| *hash == code_fingerprint(code) && *until > now)
        .map(|(_, until)| until);
    let copied = until.is_some();

    // 子 Ui 是 right_to_left(Center):先分配的贴右缘,按钮在语言标签右侧。
    let size = egui::vec2(
        crate::ui::tokens::ICON + 2.0 * crate::ui::tokens::SPACE_XS,
        20.0,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(
                rect,
                crate::ui::tokens::RADIUS_SM,
                ui.visuals().widgets.hovered.bg_fill,
            );
        }
        let (icon, color) = if copied {
            (crate::ui::icons::Icon::Check, crate::ui::tokens::OK)
        } else {
            (crate::ui::icons::Icon::Copy, ui.visuals().weak_text_color())
        };
        icon.draw(painter, rect.center(), crate::ui::tokens::ICON, color);
    }

    // 探针:记录本帧按钮几何,帧号变了就重开清单(一帧一份)。
    let probe_id = copy_button_rects_id();
    let frame = ui.ctx().cumulative_pass_nr();
    let mut rects = ui
        .ctx()
        .data(|d| d.get_temp::<(u64, Vec<egui::Rect>)>(probe_id))
        .filter(|(seen, _)| *seen == frame)
        .map(|(_, rects)| rects)
        .unwrap_or_default();
    rects.push(rect);
    ui.ctx()
        .data_mut(|d| d.insert_temp(probe_id, (frame, rects)));

    // 语言标签:info string 首词(完整 info 可带 `title=` 等元数据,首词才是
    // 语言);无语言(裸围栏/缩进块,vendored 侧落到空串)不画标签。
    if let Some(word) = lang.split_whitespace().next() {
        ui.label(egui::RichText::new(word).small().weak())
            .on_hover_text("代码块语言");
    }

    if response.clicked() {
        ui.ctx().copy_text(code.to_owned());
        let deadline = now + COPIED_FEEDBACK;
        ui.ctx()
            .data_mut(|d| d.insert_temp(copied_feedback_id(), (code_fingerprint(code), deadline)));
        ui.ctx().request_repaint_after(COPIED_FEEDBACK_DELAY);
    }

    // hover 文案放最后(on_hover_text 消费 response)。
    if let Some(until) = until {
        // 到点排程一帧,让 ✓ 自行消退;静止窗口里 egui 不会再醒。
        ui.ctx()
            .request_repaint_after(until.saturating_duration_since(now));
        response.on_hover_text("已复制");
    } else {
        response.on_hover_text("复制代码");
    }
}

/// 预览 widget id 的 tab 维度成分(#39 M2)。vendored 层的解析/分段/高亮
/// 缓存全部挂在这个 id 命名空间下,每标签独立一份:切到别的标签期间本
/// 标签的缓存不被覆盖,切回即命中 —— 这是「切标签往返不重解析」的机制
/// (egui temp memory 无按帧回收,跨帧存活,实证见 tab_switch_perf 回归测试)。
/// id 只含稳定 tab id,绝不含内容 hash/长度(AGENTS.md §6.7)。
fn tab_preview_id(tab_id: u64) -> egui::Id {
    egui::Id::new("preview-md").with(tab_id)
}

/// 绘制预览面板。`tab_id` 是当前标签的稳定 id(缓存命名空间,见
/// [`tab_preview_id`]);`heal` 为 true 时渲染前对整篇文本补闭合 —— 仅在
/// AI 流式写入本标签时开:流式残缺帧(未闭合 fence/加粗)需要补闭合才
/// 语法合法(AGENTS.md §6.5),完整文档上 heal 是恒等变换(Cow::Borrowed),
/// 但扫描本身逐行全文(2 万行样本 ~2ms/帧),人工编辑的稳态帧不该付。
/// `base_dir` 是当前文档所在目录:相对图片地址以它为锚拼成 `file://`
/// 绝对 URI(见 [`resolve_relative_images`]);`None` = 文档未落盘,原样渲染。
pub fn ui(
    panel: &mut egui::Ui,
    preview: &mut PreviewState,
    ai: &AiState,
    tab_id: u64,
    heal: bool,
    base_dir: Option<&Path>,
    outbox: &mut Vec<Message>,
) {
    let label_id = tab_preview_id(tab_id);
    egui::ScrollArea::vertical()
        .id_salt(label_id.with("scroll"))
        // 不收缩宽度,让 wrap 以面板宽为界
        .auto_shrink([false, false])
        .show(panel, |ui| {
            // widget id 必须稳定:只由 tab id 构成,绝不含内容长度/hash,
            // 否则每次编辑都清空 vendored 层临时缓存,增量高亮与分段缓存
            // 全部失效(AGENTS.md §6.7)。内容变化已在上游按修订号节流,
            // 这里每帧拿到的都是"仅在变化时重建"的同一字符串;相对图片的
            // 改写也是纯函数 —— 同样的输入永远产出同样的字符串,缓存照常
            // 命中。
            let handler = AiLinkHandler::new(
                ai_link_color(ui.visuals().dark_mode),
                ui.visuals().dark_mode,
                ai,
            );
            // 渲染的是**展开过 wikilink 的**文本:源码里的 [[X]] 在这里已是
            // [X](<wiki://X>) 链接,点击由下面的 handler 拦截;相对图片
            // 地址在这里再换成 file:// URI(两层都是"只改渲染,源码不动")
            let rendered = resolve_relative_images(&preview.rendered, base_dir);
            MarkdownLabel::new(label_id, rendered.as_ref())
                .wrap()
                .heal(heal)
                .link_handler(&handler)
                // 代码块复制头(#38):挂载点上游自带,点击经回调出 app 侧
                // 执行复制(见 [`code_copy_buttons`])。源码/Live 两种模式下
                // 右栏都走本入口,label_id 只含 tab id,互切不清缓存、按钮仍在。
                .code_block_buttons(&code_copy_buttons)
                .show(ui);
            handler.drain_into(outbox);
        });

    // 大纲跳转的预览侧:把字节偏移换算成 y 再滚过去。锚点是上一行渲染时
    // vendored 层记录下的(section → y),这里只做查表 + 请求滚动。
    if let Some(target) = preview.scroll_target.take() {
        if let Some(anchors) = egui_markdown::section_anchors(panel, label_id) {
            // 取「起点不超过目标」的最后一个锚点:标题所在节的顶部
            let anchor = anchors
                .iter()
                .rev()
                .find(|anchor| anchor.byte_start <= target)
                .or_else(|| anchors.first());
            if let Some(anchor) = anchor {
                panel.scroll_to_rect(
                    egui::Rect::from_min_size(
                        egui::pos2(panel.min_rect().left(), anchor.y),
                        egui::vec2(panel.available_width().max(1.0), 1.0),
                    ),
                    Some(egui::Align::TOP),
                );
            }
        }
    }
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
            let handler = AiLinkHandler::new(
                ai_link_color(ui.visuals().dark_mode),
                ui.visuals().dark_mode,
                &AiState::default(),
            );
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
        let idle = AiLinkHandler::new(color, true, &ai_state(false, None));
        assert_eq!(idle.card_status("续写"), AiCardStatus::Idle);

        let running = AiLinkHandler::new(color, true, &ai_state(true, Some("续写")));
        assert_eq!(running.card_status("续写"), AiCardStatus::Running);

        let done = AiLinkHandler::new(color, true, &ai_state(false, Some("续写")));
        assert_eq!(done.card_status("续写"), AiCardStatus::Done);

        // 其它卡片不受牵连:菜单发起的 prompt 是拼装文本,不等任何指令
        let unrelated = AiLinkHandler::new(
            color,
            true,
            &ai_state(true, Some("请续写以下文档内容:\n……")),
        );
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
            let text = doc.to_owned();
            let mut preview = PreviewState {
                rendered: latermd_md::expand_wikilinks(&text),
                text,
                synced_rev: 0,
                outline: Vec::new(),
                scroll_target: None,
            };
            ui(
                panel,
                &mut preview,
                &AiState::default(),
                1,
                false,
                None,
                &mut outbox,
            );
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
        let handler = AiLinkHandler::new(ai_link_color(true), true, &AiState::default());
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

    /// `[[wikilink]]` 的点击:`wiki://` 被拦成 [`Message::WikilinkClicked`],
    /// 不交系统浏览器;空目标不拦(交回默认行为)。
    #[test]
    fn wiki_link_click_is_intercepted_as_wikilink_message() {
        let ctx = egui::Context::default();
        let handler = AiLinkHandler::new(ai_link_color(true), true, &AiState::default());
        let intercepted = std::cell::Cell::new(true);
        let output = ctx.run_ui(RawInput::default(), |ui| {
            handler.click("架构决策", "wiki://架构决策", ui);
            // 普通 http 链接不拦:交回 vendored 默认的 open_url
            intercepted.set(handler.click("x", "https://example.com", ui));
        });
        output.drop_without_applying_deltas();
        assert!(!intercepted.get(), "http 链接不应被拦");

        let mut outbox = Vec::new();
        handler.drain_into(&mut outbox);
        assert_eq!(
            outbox,
            vec![Message::WikilinkClicked {
                target: "架构决策".into()
            }]
        );
    }

    /// 预览消费滚动目标一次:不消费会导致每帧都把预览拽回目标位置,用户
    /// 再也滚不动(锚点缺失时也不 panic)。
    #[test]
    fn preview_consumes_scroll_target_once() {
        let ctx = egui::Context::default();
        let doc = "# 一\n\n正文\n\n## 二\n\n正文二\n";
        let mut preview = PreviewState {
            rendered: doc.to_owned(),
            text: doc.to_owned(),
            synced_rev: 0,
            outline: Vec::new(),
            scroll_target: Some(0),
        };
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |panel| {
            ui(
                panel,
                &mut preview,
                &AiState::default(),
                1,
                false,
                None,
                &mut outbox,
            );
        });
        output.drop_without_applying_deltas();
        assert_eq!(preview.scroll_target, None, "滚动目标只消费一次");
        assert!(outbox.is_empty(), "滚动不产消息");
    }

    /// 相对图片地址解析(B 段验收的单测层):改写只发生在渲染字符串上,
    /// 源码与 `PreviewState` 一字不动 —— 相对路径落盘,文档目录整体搬走
    /// 仍有效(可移植验收)。
    #[test]
    fn relative_images_resolve_against_base_dir() {
        use std::borrow::Cow;
        let base = Path::new("/home/u/docs");
        // B 段自产形态:`./foo.assets/中文.png`
        let out = resolve_relative_images("![图](./foo.assets/中文.png)", Some(base));
        assert_eq!(out, "![图](file:///home/u/docs/foo.assets/中文.png)");
        // 上级目录与无 ./ 前缀
        let out = resolve_relative_images("![a](../img/x.png)", Some(base));
        assert_eq!(out, "![a](file:///home/u/docs/../img/x.png)");
        // `<…>` 包裹保持包裹
        let out = resolve_relative_images("![a](<./d/屏幕 截图.png>)", Some(base));
        assert_eq!(out, "![a](<file:///home/u/docs/d/屏幕 截图.png>)");
        // 多张图与周边文本逐字节保留
        let doc = "前文\n\n![一](a.png)中间![二](<b c.png>)\n\n后文";
        let out = resolve_relative_images(doc, Some(base));
        assert_eq!(
            out,
            "前文\n\n![一](file:///home/u/docs/a.png)中间![二](<file:///home/u/docs/b c.png>)\n\n后文"
        );
        // 没有相对图片时原样**借回**(零分配,vendored 缓存键不变)
        let clean = "![x](https://e.com/a.png) 与 [链](rel.md)";
        assert!(matches!(
            resolve_relative_images(clean, Some(base)),
            Cow::Borrowed(_)
        ));
        // 未落盘(无锚点)原样借回
        assert!(matches!(
            resolve_relative_images("![x](a.png)", None),
            Cow::Borrowed(_)
        ));
    }

    /// 不该改写的一律不动:绝对 URL、`wiki://`(wikilink 展开产物)、
    /// 绝对路径、锚点、空目标、围栏代码块内的 `![]()`、以及**链接**。
    #[test]
    fn non_relative_and_non_image_targets_stay_put() {
        let base = Path::new("/home/u/docs");
        let doc = "![a](https://e.com/x.png?w=1)\n![b](<wiki://架构决策>)\n\
                   ![c](/abs/path.png)\n![d](#anchor)\n![]()\n[e 链](./rel.md)\n\
                   ![f](data:image/png;base64,AAAA)\n![g](C:/win.png)\n\n\
                   ```rust\nlet x = ![](inner.png);\n```\n\n![h](ok.png)";
        let out = resolve_relative_images(doc, Some(base));
        let expected = doc.replace("![h](ok.png)", "![h](file:///home/u/docs/ok.png)");
        assert_eq!(out, expected);
    }

    /// B 段全链路(纯函数层):浏览复制产出的相对地址(含空格时包 `<>`)
    /// 能被本函数还原成绝对 URI —— 存盘文本与预览改写两侧的约定互相咬合。
    #[test]
    fn stored_url_round_trips_into_file_uri() {
        let base = Path::new("/home/u/docs");
        // assets::store 的产物形态(含空格包 <>):crate::assets 单测钉落盘,
        // 这里钉「这条地址进预览后能出图」
        let stored = "<./笔记.assets/屏幕 截图 (1).png>";
        let doc = format!("![alt]({stored})");
        let out = resolve_relative_images(&doc, Some(base));
        assert_eq!(
            out,
            "![alt](<file:///home/u/docs/笔记.assets/屏幕 截图 (1).png>)"
        );
    }

    /// 带标题的内联图片:`![a](./x.png "标题")` 目标到空白即止,标题保留。
    #[test]
    fn titled_image_keeps_its_title() {
        let base = Path::new("/d");
        let out = resolve_relative_images("![a](./x.png \"题\")", Some(base));
        assert_eq!(out, "![a](file:///d/x.png \"题\")");
    }

    /// B 段验收「预览出图」的无头全链路:真 PNG 落盘 → 生产渲染入口
    /// [`ui`](相对地址在这里被 [`resolve_relative_images`] 拼成 file://)→
    /// vendored `Token::Image` 分支(images feature)→ egui loader 装上后
    /// 真的画出**图片纹理**。
    ///
    /// 断言依据:加载完成的图片画成**带纹理的 RectShape**(纯色矩形/文字/
    /// spinner/⚠ 的纹理恒为默认值),出现即出图;加载在 loader 后台线程,
    /// 轮询有限帧直到它出现。
    #[test]
    fn local_file_image_paints_through_loaders() {
        let dir = std::env::temp_dir().join(format!("latermd-preview-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("foo.assets")).unwrap();
        let png = dir.join("foo.assets/dot.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([200, 40, 40]))
            .save(&png)
            .unwrap();

        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);

        let textured_rects = |output: &eframe::egui::FullOutput| -> usize {
            fn collect(shape: &egui::epaint::Shape, out: &mut usize) {
                match shape {
                    egui::epaint::Shape::Rect(rect) => {
                        // 0.36 起矩形纹理走 `brush`(纯色矩形/文字/spinner/⚠
                        // 均为 None),带 brush 的矩形即图片
                        if rect.brush.is_some() {
                            *out += 1;
                        }
                    }
                    egui::epaint::Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
                    _ => {}
                }
            }
            let mut out = 0;
            for clipped in &output.shapes {
                collect(&clipped.shape, &mut out);
            }
            out
        };
        let render = |doc: &str| {
            let mut preview = PreviewState {
                rendered: doc.to_owned(),
                text: doc.to_owned(),
                synced_rev: 0,
                outline: Vec::new(),
                scroll_target: None,
            };
            let mut outbox = Vec::new();
            ctx.run_ui(RawInput::default(), |panel| {
                ui(
                    panel,
                    &mut preview,
                    &AiState::default(),
                    1,
                    false,
                    Some(dir.as_path()),
                    &mut outbox,
                );
            })
        };

        // 基线:无图文档出不了带纹理的矩形(防御断言),浮窗/滚动区首帧
        // 只完成注册,与其它无头测试同节奏跑满三帧。
        for _ in 0..3 {
            let output = render("# 基线\n\n正文,没有图片。");
            assert_eq!(textured_rects(&output), 0, "无图文档不应有图片纹理");
            output.drop_without_applying_deltas();
        }

        let mut painted = false;
        for _ in 0..200 {
            let output = render("![红点](./foo.assets/dot.png)");
            let textured = textured_rects(&output);
            output.drop_without_applying_deltas();
            if textured > 0 {
                painted = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(painted, "相对路径图片应经 file:// 画出图片纹理(出图)");
        let _ = std::fs::remove_dir_all(&dir);
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
            let handler = AiLinkHandler::new(ai_link_color(true), true, &AiState::default());
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

    /// #39 M2 缓存正确性(编辑后缓存失效):同一 tab 的缓存槽位按文本 hash
    /// 键控,内容变了 hash 变 → miss 重建,预览第一帧就必须反映新文本 ——
    /// 缓存绝不允许吞掉编辑。撤销回旧文本(hash 回到旧值,命中旧产物)对
    /// 同一文本产物必然正确,同样立即显示。heal 开关切换(流式结束,调用方
    /// 由 heal(true) 转 heal(false))不改变完整文档的渲染结果。
    #[test]
    fn preview_reflects_edits_immediately_despite_per_tab_cache() {
        let ctx = egui::Context::default();
        // 生产入口 preview::ui(同一 tab id 反复渲染,模拟切换往返中的
        // 编辑/撤销);收 painted 文本做内容断言。
        let render = |rendered: &str, heal: bool| -> Vec<String> {
            let mut preview = PreviewState {
                rendered: rendered.to_owned(),
                text: rendered.to_owned(),
                synced_rev: 0,
                outline: Vec::new(),
                scroll_target: None,
            };
            let mut outbox = Vec::new();
            let output = ctx.run_ui(RawInput::default(), |panel| {
                ui(
                    panel,
                    &mut preview,
                    &AiState::default(),
                    7,
                    heal,
                    None,
                    &mut outbox,
                );
            });
            let painted = painted_text(&output);
            output.drop_without_applying_deltas();
            painted
        };

        let v1 = "# 版本一\n\n原始段落 unique-v1-marker。\n";
        let v2 = "# 版本二\n\n编辑后的段落 unique-v2-marker。\n";

        let painted_v1 = render(v1, false);
        assert!(painted_v1.iter().any(|t| t.contains("unique-v1-marker")));

        // 编辑(v1 → v2):新文本第一帧就画出来,旧内容不得残留
        let painted_v2 = render(v2, false);
        assert!(
            painted_v2.iter().any(|t| t.contains("unique-v2-marker")),
            "编辑后预览必须立即反映新文本(缓存吞掉了编辑)"
        );
        assert!(
            !painted_v2.iter().any(|t| t.contains("unique-v1-marker")),
            "编辑后旧内容不得因缓存残留"
        );

        // 撤销(v2 → v1):文本回到旧值,立即显示 v1
        let painted_undo = render(v1, false);
        assert!(
            painted_undo.iter().any(|t| t.contains("unique-v1-marker")),
            "撤销后预览必须回到 v1"
        );

        // heal 开关切换(流式结束帧)不改变完整文档渲染:同一文本 heal
        // 是恒等变换,两次渲染的 painted 文本必须逐条一致。
        let painted_healed = render(v1, true);
        assert_eq!(
            painted_healed, painted_undo,
            "heal 开关不得改变完整文档渲染"
        );
    }

    // —— 代码块复制头(#38)——

    /// 覆盖 #38 验收面的文档:带语言块(CJK + emoji 行)、裸围栏、带空行的
    /// 空围栏(text 为空串但块存在)、```ai 指令卡、末尾短块(验「无尾换行」
    /// 边界)。另含一个无空行的空围栏 —— pulldown 对它不产 Text 事件,
    /// vendored parser 也就没有 token,整块不渲染(上游既有语义,只验不
    /// panic,不指望按钮)。
    fn code_copy_doc() -> String {
        let mut doc = String::from("前文段落。\n\n");
        doc.push_str("```zzprobe\nfn hello() {\n    println!(\"你好,世界 🌏\");\n}\n```\n\n");
        doc.push_str("```\n裸围栏,没有语言。\n```\n\n");
        doc.push_str("```\n\n```\n\n");
        doc.push_str("```\n```\n\n");
        doc.push_str("```ai\n指令不走这里\n```\n\n");
        doc.push_str("```rust\ntail_line()\n```\n");
        doc
    }

    /// 以生产入口渲染一帧,返回 (探针按钮 rect, 帧输出收集的 Text shape 文本,
    /// 本帧 CopyText 命令载荷)。
    fn render_copy_frame(
        ctx: &egui::Context,
        doc: &str,
        events: Vec<egui::Event>,
    ) -> (Vec<egui::Rect>, Vec<String>, Vec<String>) {
        let mut preview = PreviewState {
            rendered: latermd_md::expand_wikilinks(doc),
            text: doc.to_owned(),
            synced_rev: 0,
            outline: Vec::new(),
            scroll_target: None,
        };
        let mut outbox = Vec::new();
        let output = ctx.run_ui(
            eframe::egui::RawInput {
                events,
                ..Default::default()
            },
            |panel| {
                ui(
                    panel,
                    &mut preview,
                    &AiState::default(),
                    1,
                    false,
                    None,
                    &mut outbox,
                );
            },
        );
        let texts = painted_text(&output);
        let copied = copied_texts(&output);
        // 帧后读取:帧号已前进,按「最后写入者即本帧」取原始探针。
        let rects = copy_button_probe(ctx).1;
        output.drop_without_applying_deltas();
        (rects, texts, copied)
    }

    /// 按指针三帧(移入/按下/抬起)点击 `target`,汇集整个序列的 CopyText 载荷。
    fn click_and_collect(ctx: &egui::Context, doc: &str, target: egui::Pos2) -> Vec<String> {
        let mut copied = Vec::new();
        for events in click_events(target) {
            let (_, _, frame) = render_copy_frame(ctx, doc, events);
            copied.extend(frame);
        }
        copied
    }

    /// 指针序列:移入 → 按下 → 抬起(与 ai_key 的无头点击同款三帧)。
    fn click_events(pos: egui::Pos2) -> Vec<Vec<egui::Event>> {
        let click = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        vec![
            vec![egui::Event::PointerMoved(pos)],
            vec![click(pos, true)],
            vec![click(pos, false)],
        ]
    }

    /// 从帧输出里抽出全部 CopyText 命令的载荷。
    fn copied_texts(output: &eframe::egui::FullOutput) -> Vec<String> {
        output
            .platform_output
            .commands
            .iter()
            .filter_map(|cmd| match cmd {
                eframe::egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// 按钮存在性与命中:每个**普通**代码块一枚按钮(裸围栏/空块也有,
    /// ```ai 指令卡走 block_code_widget 另一条路不经过挂载点);点击后
    /// `Context::copy_text` 出整块源文本 —— CJK/emoji 行逐字节保留、
    /// 末块无尾换行、空块复制空串。渲染不 panic、点击不产消息。
    #[test]
    fn code_block_copy_buttons_render_per_block_and_copy_on_click() {
        let ctx = egui::Context::default();
        let doc = code_copy_doc();

        let (rects, _, _) = render_copy_frame(&ctx, &doc, Vec::new());
        assert_eq!(
            rects.len(),
            4,
            "zzprobe/裸/空/末尾 rust 四个普通块各一枚按钮,ai 卡不算:{rects:?}"
        );

        // 点击 zzprobe 块:复制内容与块源文本逐字符相等(CJK + emoji +
        // 内部缩进换行原样),点击不产消息。
        assert_eq!(
            click_and_collect(&ctx, &doc, rects[0].center()),
            vec!["fn hello() {\n    println!(\"你好,世界 🌏\");\n}".to_owned()],
            "点击恰复制一次,内容与块源文本逐字符相等"
        );

        // 末块边界:无尾换行(vendored parser 的 trim_end_newlines)。
        let (rects, _, _) = render_copy_frame(&ctx, &doc, Vec::new());
        assert_eq!(
            click_and_collect(&ctx, &doc, rects[3].center()),
            vec!["tail_line()".to_owned()],
            "末块复制不带尾换行"
        );

        // 空块:按钮在,点击复制空串(不 panic)。
        let (rects, _, _) = render_copy_frame(&ctx, &doc, Vec::new());
        assert_eq!(
            click_and_collect(&ctx, &doc, rects[2].center()),
            vec![String::new()],
            "空块复制空串"
        );
    }

    /// 瞬时反馈:点击帧写入反馈态,下一帧按钮图标换成 ✓(Check 的两条
    /// 斜率异号线段落在按钮 rect 内;Copy 后框的边线是水平/垂直,不混判)。
    /// 再渲染一帧按钮仍在(连续帧稳定,源码/Live 互切共用本入口的底座)。
    #[test]
    fn copy_click_flashes_check_feedback_next_frame() {
        let ctx = egui::Context::default();
        let doc = code_copy_doc();

        let (rects, _, _) = render_copy_frame(&ctx, &doc, Vec::new());
        let target = rects[0].center();
        for events in click_events(target) {
            let _ = render_copy_frame(&ctx, &doc, events);
        }

        // 点击后的第一帧:按钮 rect 内出现一正一负两条非零斜率线段(Check)。
        let output = ctx.run_ui(egui::RawInput::default(), |panel| {
            let mut preview = PreviewState {
                rendered: latermd_md::expand_wikilinks(&doc),
                text: doc.clone(),
                synced_rev: 0,
                outline: Vec::new(),
                scroll_target: None,
            };
            let mut outbox = Vec::new();
            ui(
                panel,
                &mut preview,
                &AiState::default(),
                1,
                false,
                None,
                &mut outbox,
            );
        });
        let rects = copy_button_probe(&ctx).1;
        assert_eq!(rects.len(), 4, "反馈帧按钮仍在");
        let button = rects[0];
        let mut slopes = Vec::new();
        for clipped in &output.shapes {
            if let egui::epaint::Shape::LineSegment { points, .. } = &clipped.shape {
                let inside = points.iter().all(|p| button.contains(*p));
                if inside {
                    slopes.push((points[1].x - points[0].x) * (points[1].y - points[0].y));
                }
            }
        }
        assert!(
            slopes.iter().any(|s| *s > 0.0) && slopes.iter().any(|s| *s < 0.0),
            "✓ 反馈 = 两条斜率异号线段落在按钮内:{slopes:?}"
        );
        output.drop_without_applying_deltas();
    }

    /// 快照护栏(照 #32 图标护栏手法):复制按钮与语言标签不得回流正文
    /// 文本 —— 语言标签 `zzprobe` 只允许出现在独立的头部 Text shape,
    /// 含代码正文的 galley 不得混入语言标签;图标字形(`⧉`/`✓`)与
    /// hover 文案不得以文本形态出现(无 hover 帧本就不该有 tooltip)。
    #[test]
    fn code_block_header_stays_out_of_body_text() {
        let ctx = egui::Context::default();
        let doc = code_copy_doc();
        let (_, texts, _) = render_copy_frame(&ctx, &doc, Vec::new());

        let body = texts
            .iter()
            .find(|t| t.contains("fn hello()"))
            .expect("代码正文应仍在文本层");
        assert!(
            !body.contains("zzprobe"),
            "语言标签不得回流正文 galley:{body:?}"
        );
        assert!(
            body.contains("裸围栏,没有语言。"),
            "裸围栏块的正文原样在 galley:{body:?}"
        );
        let label_shapes = texts.iter().filter(|t| t.as_str() == "zzprobe").count();
        assert_eq!(label_shapes, 1, "语言标签恰一枚独立 Text shape:{texts:?}");
        for text in &texts {
            assert!(
                !text.contains('⧉') && !text.contains('✓') && !text.contains("复制代码"),
                "图标字形/hover 文案不得以文本形态出现:{text:?}"
            );
        }
    }

    /// ```ai 指令卡回归:挂上复制头之后,指令块仍走卡片(标题/指令/执行
    /// 按钮三要素齐全),代码块复制按钮不叠上卡片。
    #[test]
    fn ai_instruction_card_unaffected_by_copy_header() {
        let ctx = egui::Context::default();
        let doc = code_copy_doc();
        let (rects, texts, _) = render_copy_frame(&ctx, &doc, Vec::new());

        assert_eq!(rects.len(), 4, "指令卡不产复制按钮");
        for expected in ["AI 指令", "指令不走这里", "执行"] {
            assert!(
                texts.iter().any(|t| t.contains(expected)),
                "卡片要素 {expected} 缺失:{texts:?}"
            );
        }
    }
}
