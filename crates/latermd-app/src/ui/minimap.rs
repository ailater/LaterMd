//! 源码页 Minimap 的行模型与修订号缓存(#55 M1,纯数据层)。
//!
//! 三件东西,全部零绘制:
//!
//! * **行模型**([`line_model`]):全文 → 每行一条 [`LineDesc`](前导空白折算
//!   的缩进 + 按显示宽归一的长度)。显示宽以「半宽单元」计:ASCII 半宽
//!   记 1,CJK/全角/emoji 记 2(简化 EAW 宽表见 [`is_fullwidth`]),tab 按
//!   编辑器现状口径折算(egui `FontTweak::tab_size` 默认 4,epaint 对
//!   `\t` 的 advance 即 tab_size × 空格宽;本仓未设 tweak,见
//!   [`TAB_UNITS`])。缩进+长度联合封顶在 [`LineParams::max_units`],
//!   行条永不溢出 minimap 右缘。
//! * **缓存**([`with_lines`]):行模型按「文档修订号 + 参数」键控,挂在
//!   `editor_id.with("minimap-lines")` 的 egui temp memory 上 —— 每标签
//!   一个槽位(照 #39 `tab_preview_id` per-tab 分槽先例),编辑只失效
//!   本标签的缓存,切标签往返命中(id 只含稳定 tab id,绝不含内容
//!   hash/长度,AGENTS §6.7)。
//! * **可见窗口**([`window`]):(minimap 高度, 总行数, 滚动位置, 比例尺)
//!   → 应绘制的行区间与每行 y。O(1) 纯函数,滚动帧只平移窗口、不触碰
//!   行模型缓存。
//!
//! M2 渲染层(本文件下半段)消费这三个入口:行条只画 [`window`] 的可见
//! 区间、颜色从当帧 visuals 推导(明暗两套);编辑器视口的高亮框与
//! 点击/拖动跳转([`jump_ratio`])都按「内容比例」换算,与窗口平移同一
//! 套几何。比例尺/不满高铺排等自选项登记在 docs/decisions-pending.md
//! #104,条宽/默认开关等 M2 口径见 #105。

use latermd_editor::EditorBuffer;
use std::ops::Range;

use eframe::egui;

/// tab 折算的半宽单元数。编辑器现状无自定义 tab 口径(无 FontTweak、无
/// tab 设置项),egui 0.36 epaint 对 `\t` 的 advance = `tab_size × 空格宽`、
/// `tab_size` 默认 4.0 —— minimap 与编辑器显示宽同口径,取 4。
pub(crate) const TAB_UNITS: u16 = 4;

/// 比例尺:minimap 每行条的像素高。任务书建议 2-4px,取 3px:2px 在 HiDPI
/// 缩放下细至难辨,4px 让 10000 行占 40000px(行程过长、滑起来发飘),
/// 3px 下 10000 行 = 30000px、视口内约 200 行,与 VS Code 默认观感同量级。
pub(crate) const ROW_H: f32 = 3.0;

/// 长度归一封顶的出厂值(半宽单元),约对应 96px 条宽 × 1px/单元 —— M2
/// 定条宽后按 `条宽 / 期望单元像素` 重算传入,缓存随参数键控自动重建。
pub(crate) const MAX_UNITS: u16 = 96;

/// 行模型的构建参数(缓存键的一半,另一半是文档修订号)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineParams {
    /// tab 折算的半宽单元数([`TAB_UNITS`] 口径)。
    pub tab_units: u16,
    /// 单行(缩进+内容)显示宽封顶:超出截断,行条右缘永不溢出。
    pub max_units: u16,
}

impl LineParams {
    /// 出厂参数:tab 4 单元、封顶 96 单元。
    pub(crate) const fn factory() -> Self {
        Self {
            tab_units: TAB_UNITS,
            max_units: MAX_UNITS,
        }
    }
}

/// 单行的 minimap 绘制描述:一条「缩进 × 长度」的抽象行条。
///
/// 两个数都以半宽单元计(一个 ASCII 字符位 = 1);渲染层换算像素 =
/// `值 × (minimap 条宽 / max_units)`。空行两值皆零。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineDesc {
    /// 前导空白折算宽(空格 1、tab `tab_units`、全角空格 U+3000 记 2),
    /// 已封顶 `max_units`。
    pub indent: u16,
    /// 去掉前导空白后的内容显示宽(全宽字符记 2,超预算截断),
    /// 已封顶 `max_units - indent`。
    pub width: u16,
}

/// 单字符的半宽单元数。
fn char_units(c: char, tab_units: u16) -> u16 {
    match c {
        '\t' => tab_units,
        // 全角空格:既是前导空白白名单成员,又占全宽。
        '\u{3000}' => 2,
        // 零宽连接符:变体选择符与 ZWJ 不占宽(emoji 组合串)。
        '\u{200D}' | '\u{FE0E}' | '\u{FE0F}' => 0,
        c if is_fullwidth(c) => 2,
        _ => 1,
    }
}

/// 显示宽判定:East Asian Width 的 W(宽)/F(全角)大区段合并简化表。
///
/// 完整 EAW 表有上百个零散 W 单点(如 U+1F004、U+231A);本表只收连续
/// 大区段,零散点并入邻近 emoji 密集区 —— 误差方向只是把极少数窄字符记
/// 宽 ≤1 单元,对 minimap 抽象条的相对长度观感无影响。口径已登记
/// decisions-pending #104。
fn is_fullwidth(c: char) -> bool {
    let u = c as u32;
    matches!(u,
        0x1100..=0x115F        // Hangul Jamo
        | 0x2300..=0x23FF      // 杂项技术(⌚⏰⏳ emoji 密集)
        | 0x2600..=0x27BF      // 杂项符号 + dingbats(❤☎ emoji 密集)
        | 0x2B00..=0x2BFF      // 杂项符号箭头(⭐)
        | 0x2E80..=0x303E      // CJK 部首/符号/注音
        | 0x3041..=0x33FF      // 假名/注音扩展/兼容
        | 0x3400..=0x4DBF      // CJK 扩展 A
        | 0x4E00..=0x9FFF      // CJK 统一表意
        | 0xA000..=0xA4CF      // 彝文
        | 0xA960..=0xA97F      // Hangul Jamo 扩展 A
        | 0xAC00..=0xD7A3      // Hangul 音节
        | 0xF900..=0xFAFF      // CJK 兼容表意
        | 0xFE10..=0xFE19      // 竖排形式
        | 0xFE30..=0xFE6F      // CJK 兼容形式
        | 0xFF00..=0xFF60      // 全角形式
        | 0xFFE0..=0xFFE6      // 全角符号
        | 0x16FE0..=0x16FE4    // 表意助记符
        | 0x17000..=0x18AFF    // 唐古特文/假名扩展
        | 0x1B000..=0x1B2FF    // 假名补充
        | 0x1F300..=0x1F64F    // 常用 emoji 与 CJK 符号密集区
        | 0x1F680..=0x1F6FF    // 交通/地图 emoji
        | 0x1F900..=0x1F9FF    // emoji 补充
        | 0x1FA70..=0x1FAFF    // emoji 扩展 A
        | 0x20000..=0x2FFFD    // CJK 扩展 B+(SIP)
        | 0x30000..=0x3FFFD    // CJK 扩展 G+(TIP)
    )
}

/// 前导空白白名单:只认空格、tab 与全角空格。`char::is_whitespace` 会把
/// NBSP/换行制表符等也计入,它们在 Markdown 缩进语义里不是缩进。
fn is_indent_char(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{3000}')
}

/// 行模型:全文 → 每行一条 [`LineDesc`]。
///
/// 行划分与 gutter/编辑器同口径(`split('\n')`,总行数 = 1 + `\n` 数,空
/// 文档 1 行)。ZWJ 序列(`👨‍👩‍👧`)整体只记一次全宽:前一个字符是
/// U+200D 时当前字符记 0 宽 —— 组合串显示为单个字形,逐段累计会把长度
/// 放大 N 倍。
pub(crate) fn line_model(text: &str, params: LineParams) -> Vec<LineDesc> {
    text.split('\n')
        .map(|line| {
            let mut indent: u16 = 0;
            let mut rest = line.chars().peekable();
            while let Some(&c) = rest.peek() {
                if !is_indent_char(c) {
                    break;
                }
                indent = indent.saturating_add(char_units(c, params.tab_units));
                rest.next();
            }
            let indent = indent.min(params.max_units);
            // 条永不溢出右缘:内容预算 = 封顶减缩进;逐字符累计,放不下
            // 整个全宽字符就停(宁可短一单元,不切字符)。
            let budget = params.max_units.saturating_sub(indent);
            let mut width: u16 = 0;
            let mut after_zwj = false;
            for c in rest {
                let units = if after_zwj {
                    0
                } else {
                    char_units(c, params.tab_units)
                };
                after_zwj = c == '\u{200D}';
                match width.checked_add(units) {
                    Some(sum) if sum <= budget => width = sum,
                    _ => break,
                }
            }
            LineDesc { indent, width }
        })
        .collect()
}

/// 行模型缓存条目(egui temp memory 里的值类型)。
///
/// 键控 = 文档修订号 + [`LineParams`]:编辑(rev 前进)、参数变化(条宽/
/// tab 口径变)才重建;同 rev 同参数命中时 `Vec` 原地不动(指针稳定,
/// 滚动/空闲帧零成本)。`synced_rev: Option` 让「从未构建」与「rev 0 的
/// 空文档」可区分 —— 后者也是合法缓存(1 条零长行)。
#[derive(Clone, Debug, Default)]
pub(crate) struct MinimapCache {
    synced_rev: Option<u64>,
    params: Option<LineParams>,
    lines: Vec<LineDesc>,
}

impl MinimapCache {
    /// 命中判定:修订号与参数都一致。
    fn matches(&self, rev: u64, params: LineParams) -> bool {
        self.synced_rev == Some(rev) && self.params == Some(params)
    }

    /// 取行模型(命中即返回既有切片,miss 才重建)。
    pub(crate) fn ensure(&mut self, editor: &EditorBuffer, params: LineParams) -> &[LineDesc] {
        let rev = editor.revision();
        if !self.matches(rev, params) {
            self.lines = line_model(editor.text(), params);
            self.synced_rev = Some(rev);
            self.params = Some(params);
        }
        &self.lines
    }
}

/// minimap 缓存的槽位 id:由标签稳定的 editor_id 派生(每标签一套,照
/// `tab_preview_id` / `ime-caret` 同款先例);绝不含内容 hash/长度。
pub(crate) fn cache_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("minimap-lines")
}

/// 取当前标签的行模型并在回调里消费。
///
/// 缓存条目活在 egui temp memory(id 见 [`cache_id`]):切走标签期间不被
/// 覆盖、切回即命中;回调借出存储条目的切片,命中帧零分配零拷贝。
pub(crate) fn with_lines<R>(
    ctx: &egui::Context,
    editor_id: egui::Id,
    editor: &EditorBuffer,
    params: LineParams,
    f: impl FnOnce(&[LineDesc]) -> R,
) -> R {
    ctx.data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<MinimapCache>(cache_id(editor_id));
        f(cache.ensure(editor, params))
    })
}

/// 可见窗口的输入(M2 从编辑器滚动状态换算后传入)。
#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowInput {
    /// minimap 内容区高(px)。
    pub minimap_height: f32,
    /// 文档总行数(0 按 1 计,空文档也有一行)。
    pub total_lines: usize,
    /// 滚动位置:编辑器视口顶在可滚行程中的比例,0 = 文档顶、1 = 文档底
    /// (内容不满一屏时恒 0)。M2 侧按 `offset / (content_h - viewport_h)`
    /// 换算。
    pub scroll_ratio: f32,
    /// 比例尺:minimap 每行条高(px),建议 [`ROW_H`]。
    pub row_h: f32,
}

/// 可见窗口:应绘制的行区间与每行 y 坐标(等差,由
/// `y0 + (行号 − 区间首行) × row_h` 表达,不必逐行存表)。
#[derive(Clone, Debug)]
pub(crate) struct VisibleWindow {
    range: Range<usize>,
    /// 区间首行条的 y(相对 minimap 视口顶;半露行为负)。
    y0: f32,
    row_h: f32,
}

impl VisibleWindow {
    /// 应绘制的行区间(0-based,半开)。生产消费见 [`Self::iter`] 与
    /// [`shapes`] 的容量预估;也是 M1 单测的取证面。
    pub(crate) fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// 行号 → 该行条的 y(相对 minimap 视口顶);区间外返回 `None`。
    /// M1 单测的取证面,非测试构建豁免 dead_code(与 preview.rs
    /// `ScrollProbe` 同款口径)。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn y_of(&self, line: usize) -> Option<f32> {
        if !self.range.contains(&line) {
            return None;
        }
        Some(self.y0 + (line - self.range.start) as f32 * self.row_h)
    }

    /// (行号, y) 迭代,M2 绘制直接消费。
    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, f32)> + '_ {
        let start = self.range.start;
        self.range
            .clone()
            .map(move |line| (line, self.y0 + (line - start) as f32 * self.row_h))
    }
}

/// 可见窗口计算:O(1) 纯函数,滚动帧每帧调用也不触碰行模型缓存。
///
/// 长文档(minimap 内容总高 > 视口高)时 minimap 随滚动平移,平移量与
/// 编辑器滚动同比例(视口顶对视口顶);短文档不满高时顶部对齐、余量留
/// 在底部,滚动比例不影响铺排(编辑器此时也滚不动,#104)。
pub(crate) fn window(input: WindowInput) -> VisibleWindow {
    let total = input.total_lines.max(1);
    // 比例尺下限防退化:row_h → 0 会让可见行数发散,0.5px 已是肉眼极限。
    let row_h = input.row_h.max(0.5);
    let viewport = input.minimap_height.max(0.0);
    let full = total as f32 * row_h;
    let travel = (full - viewport).max(0.0);
    let offset = input.scroll_ratio.clamp(0.0, 1.0) * travel;

    let first = ((offset / row_h).floor() as usize).min(total - 1);
    // 视口底所在行 ceil 进一(半露也算可见),但至少一行、至多文末。
    let last = (((offset + viewport) / row_h).ceil() as usize).clamp(first + 1, total);
    VisibleWindow {
        range: first..last,
        y0: first as f32 * row_h - offset,
        row_h,
    }
}

// —— M2 渲染层(#55)——

/// 右缘窄条总宽 = 2px 左内边距、96px 行条区(= [`MAX_UNITS`],1px/单元)、
/// 10px 滚动条避让。egui ScrollArea 的滚动条贴视口右缘、画在后、居上,
/// 行条最长到 98px 处,最后 10px 留给它;行条有效宽 96 在任务书建议的
/// 80–100px 区间内。
pub(crate) const MINIMAP_W: f32 = 108.0;

/// 行条区左内边距:行条 x = 条左缘 + 此值 + 缩进。
const PAD_X: f32 = 2.0;

/// 行条颜色:正文 fg(noninteractive)按主题压透明度 —— 暗色白压到 ~35%、
/// 亮色黑压到 ~26%。两套 visuals 同一函数推导,不做每主题手工色表。
fn bar_color(visuals: &egui::Visuals) -> egui::Color32 {
    let [r, g, b, _] = visuals.widgets.noninteractive.fg_stroke.color.to_array();
    let alpha = if visuals.dark_mode { 0x59 } else { 0x42 };
    egui::Color32::from_rgba_unmultiplied(r, g, b, alpha)
}

/// 视口高亮框的底色与描边:同从当帧 visuals 推导(暗色提白、亮色压黑,
/// 与行条方向相反,保证在行条之上仍可辨;描边复用 noninteractive 的
/// bg_stroke,明暗各自协调)。
fn viewport_fill(visuals: &egui::Visuals) -> egui::Color32 {
    if visuals.dark_mode {
        egui::Color32::from_white_alpha(0x14)
    } else {
        egui::Color32::from_black_alpha(0x0F)
    }
}

/// 点击/拖动跳转的输入:把 minimap 视口内的指针位置换算成编辑器的目标
/// 滚动比例(纯函数,单测锚点)。
pub(crate) struct JumpInput {
    /// minimap 内容区高(px,即窄条高)。
    pub minimap_height: f32,
    /// 文档总行数。
    pub total_lines: usize,
    /// 当前滚动比例(与 [`WindowInput::scroll_ratio`] 同口径)。
    pub scroll_ratio: f32,
    /// 比例尺(minimap 每行条高,px)。
    pub row_h: f32,
    /// 指针 y(相对 minimap 视口顶)。
    pub pointer_y: f32,
    /// 编辑器视口高占内容总高的比例(视口高 / 内容高)。
    pub viewport_frac: f32,
}

/// 位置 → 目标滚动比例:点击处的内容对准编辑器**视口中心**。
///
/// 换算两步:①指针 y 加上 minimap 当前平移(`scroll_ratio × travel`),
/// 除以全文档高得到内容比例 `p`;②让视口中心落在 `p` →
/// `target = (p − viewport_frac/2) / (1 − viewport_frac)`。短文档
/// (minimap 不满一屏 → 编辑器同样滚不动)或内容不满一屏返回 `None`,
/// 调用方原样不动。输出钳进 0..=1(点顶到顶、点底到底)。
pub(crate) fn jump_ratio(input: JumpInput) -> Option<f32> {
    let total = input.total_lines.max(1) as f32;
    let row_h = input.row_h.max(0.5);
    let full = total * row_h;
    let viewport = input.minimap_height.max(0.0);
    let travel = (full - viewport).max(0.0);
    if travel <= 0.0 {
        return None; // minimap 不满一屏:编辑器此时也滚不动
    }
    let ve = input.viewport_frac.clamp(0.0, 1.0);
    if ve >= 1.0 {
        return None; // 内容不满一屏:编辑器无行程
    }
    let offset = input.scroll_ratio.clamp(0.0, 1.0) * travel;
    let p = ((input.pointer_y + offset) / full).clamp(0.0, 1.0);
    Some(((p - ve * 0.5) / (1.0 - ve)).clamp(0.0, 1.0))
}

/// 编辑器视口在 minimap 上的高亮框(相对窄条顶的 `top` 与 `height`,
/// 已钳进 `[0, minimap_height]`)。与 [`window`] 的平移同一套几何:视口
/// 占内容的比例区间 `[ratio×(1−ve), ratio×(1−ve)+ve]` 映射到 minimap
/// 内容坐标再减平移;非折行文档下与行条严格对齐,深度折行时按内容比例
/// (行条是均匀逻辑行模型,口径见 decisions-pending #105)。
pub(crate) fn viewport_highlight(input: WindowInput, viewport_frac: f32) -> (f32, f32) {
    let total = input.total_lines.max(1) as f32;
    let row_h = input.row_h.max(0.5);
    let full = total * row_h;
    let map_h = input.minimap_height.max(0.0);
    let travel = (full - map_h).max(0.0);
    let ve = viewport_frac.clamp(0.0, 1.0);
    let ratio = input.scroll_ratio.clamp(0.0, 1.0);
    let top = ratio * ((1.0 - ve) * full - travel);
    let height = (ve * full).clamp(0.0, map_h);
    let top = top.clamp(0.0, (map_h - height).max(0.0));
    (top, height)
}

/// 构造 minimap 的本帧形状:可见窗口内的行条 + 编辑器视口高亮框。返回
/// (shapes, 非零行条数, 高亮框矩形)。
///
/// **只构造、不上画布**:`with_lines` 把本函数持在 `ctx().data_mut` 的
/// 借用里,而 `painter` 的任何 add 都要再进同一把 Context 写锁 —— 重入
/// 即死锁(实测踩过)。调用方(editor.rs)在借用之外 `painter().extend(shapes)`
/// 并写测试探针。只在源码模式、开关开启时被构造(关闭 = 调用方根本不
/// 进,零形状)。
///
/// `scroll_ratio` / `viewport_frac` 由调用方从编辑器 ScrollArea 的当帧
/// 真值换算(`offset/(content−viewport)`、`viewport/content`)。
pub(crate) fn shapes(
    visuals: &egui::Visuals,
    rect: egui::Rect,
    lines: &[LineDesc],
    scroll_ratio: f32,
    viewport_frac: f32,
) -> (Vec<egui::Shape>, usize, egui::Rect) {
    let geometry = WindowInput {
        minimap_height: rect.height(),
        total_lines: lines.len(),
        scroll_ratio,
        row_h: ROW_H,
    };
    let win = window(geometry);
    let bar = bar_color(visuals);
    let left = rect.left() + PAD_X;
    let mut out = Vec::with_capacity(win.range().len() + 2);
    // 只构造可见窗口:O(视口行数),滚动帧不随文档长度增长(M1 窗口语义)。
    let mut bars = 0usize;
    for (line, y) in win.iter() {
        let desc = lines[line];
        if desc.width == 0 {
            continue; // 空行/纯缩进封顶行:零宽条不画
        }
        bars += 1;
        let tl = egui::pos2(left + f32::from(desc.indent), rect.top() + y);
        out.push(egui::Shape::Rect(egui::epaint::RectShape::filled(
            egui::Rect::from_min_size(tl, egui::vec2(f32::from(desc.width), ROW_H)),
            egui::CornerRadius::ZERO,
            bar,
        )));
    }
    // 视口高亮框:底色 + 1px 描边,横跨整条(缩进信息在行条上,框只表
    // 「编辑器现在看哪里」)。排在行条之后 → 盖在其上。
    let (top, height) = viewport_highlight(geometry, viewport_frac);
    let frame = egui::Rect::from_min_size(
        egui::pos2(rect.left(), rect.top() + top),
        egui::vec2(rect.width(), height),
    );
    out.push(egui::Shape::Rect(egui::epaint::RectShape::filled(
        frame,
        egui::CornerRadius::ZERO,
        viewport_fill(visuals),
    )));
    out.push(egui::Shape::Rect(egui::epaint::RectShape::stroke(
        frame,
        egui::CornerRadius::ZERO,
        egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
        egui::StrokeKind::Inside,
    )));
    (out, bars, frame)
}

/// 测试探针的键(与 [`cache_id`] 同源由 editor_id 派生)。
pub(crate) fn probe_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("minimap-probe")
}

/// 编辑器 ScrollArea 的滚动度量快照(每帧末由 editor.rs 写入,下一帧的
/// 点击/拖动跳转换算用它:闭包内拿不到本帧排版完成后的内容高,上一帧
/// 真值足够 —— 编辑帧的一帧误差在下一帧自愈)。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ScrollMetrics {
    /// 视口顶的内容坐标偏移(`ScrollArea::state.offset.y`)。
    pub offset: f32,
    /// 内容总高(`ScrollArea::content_size.y`)。
    pub content_height: f32,
    /// 视口高(`ScrollArea::inner_rect.height()`)。
    pub viewport_height: f32,
}

/// metrics temp 的键(与 [`cache_id`] 同源由 editor_id 派生,每标签一份)。
pub(crate) fn metrics_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("minimap-metrics")
}

/// 跳转意图 temp 的键:值 = 指针 y(相对窄条顶)。闭外交互命中时写入,
/// 下一帧 ScrollArea 闭包开头消费即清 —— 一帧滞后的传接(窄条命中必须
/// 排在 ScrollArea 之后注册才不被背景拖拽抢层级,而 scroll_to_rect 只在
/// 闭包内才被同帧消费,二者不可兼得;拖动逐帧覆写,跟随无感)。
pub(crate) fn jump_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with("minimap-jump")
}

/// 测试探针载荷(仅测试读;生产每帧写、从不读,字段读取豁免
/// dead_code —— 与 preview.rs `ScrollProbe` 同款)。
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct MinimapProbe {
    pub bars: usize,
    pub viewport: Option<egui::Rect>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor;

    /// ① 缩进折算:空格/tab/混合/全角空格/空行/深缩进封顶/tab 参数。
    #[test]
    fn indent_folds_spaces_tabs_and_mixed() {
        let p = LineParams::factory();
        // 空格
        assert_eq!(line_model("    a", p)[0].indent, 4);
        // tab:4 单元/个
        assert_eq!(line_model("\t\ta", p)[0].indent, 8);
        // 混合:2 + 4 + 1
        assert_eq!(line_model("  \t a", p)[0].indent, 7);
        // 全角空格:记 2
        assert_eq!(line_model("\u{3000}a", p)[0].indent, 2);
        // 空文档与空行:零缩进零长度
        assert_eq!(line_model("", p), vec![LineDesc::default()]);
        let m = line_model("\n\nx", p);
        assert_eq!(m[0], LineDesc::default(), "空行零长度");
        assert_eq!(m[1], LineDesc::default(), "全空白行也是零长度条");
        assert_eq!((m[2].indent, m[2].width), (0, 1));
        // 全空白行:缩进照折算,内容为零
        assert_eq!(
            line_model("   ", p)[0],
            LineDesc {
                indent: 3,
                width: 0
            }
        );
        // 深缩进封顶:30 个 tab = 120 单元,封到 96,内容预算 0
        assert_eq!(
            line_model(&("\t".repeat(30) + "x"), p)[0],
            LineDesc {
                indent: 96,
                width: 0
            }
        );
        // tab 口径是参数:8 单元/个时两 tab 折 16
        let p8 = LineParams {
            tab_units: 8,
            max_units: 96,
        };
        assert_eq!(line_model("\t\ta", p8)[0].indent, 16);
    }

    /// ① 长度归一:纯 ASCII / CJK 混排 / 各全宽文种 / emoji / ZWJ 序列 /
    /// 超长截断。
    #[test]
    fn width_normalizes_by_display_width() {
        let p = LineParams::factory();
        // 纯 ASCII
        assert_eq!(line_model("hello", p)[0].width, 5);
        // CJK 全宽
        assert_eq!(line_model("你好", p)[0].width, 4);
        // 中英混排:1+2+1
        assert_eq!(line_model("a你b", p)[0].width, 4);
        // 全角形式/假名/谚文同表覆盖
        assert_eq!(line_model("ＡＢ", p)[0].width, 4, "全角字母记 2");
        assert_eq!(line_model("あい", p)[0].width, 4, "假名记 2");
        assert_eq!(line_model("가나", p)[0].width, 4, "谚文记 2");
        // 单 emoji 记 2;ZWJ 家庭序列整体记 2(不是 6)
        assert_eq!(line_model("😀", p)[0].width, 2);
        assert_eq!(line_model("👨‍👩‍👧", p)[0].width, 2, "ZWJ 序列只记一次全宽");
        // 变体选择符零宽:❤(全宽区)+ FE0F 仍是 2
        assert_eq!(line_model("❤️", p)[0].width, 2);
        // 超长截断:200 个 ASCII 封 96;100 个 CJK(200 单元)也封 96
        assert_eq!(line_model(&"a".repeat(200), p)[0].width, 96);
        assert_eq!(line_model(&"你".repeat(100), p)[0].width, 96);
        // 截断预算扣缩进:缩进 4 + 内容 92
        let m = line_model(&format!("    {}", "a".repeat(200)), p);
        assert_eq!((m[0].indent, m[0].width), (4, 92));
        // 预算 1 放不下全宽字符:停在 0,不切半字符、不溢出
        let narrow = LineParams {
            tab_units: 4,
            max_units: 1,
        };
        assert_eq!(line_model("你", narrow)[0].width, 0);
        // 多行各自独立
        let m = line_model("aaa\n    bb\n你", p);
        let widths: Vec<u16> = m.iter().map(|d| d.width).collect();
        assert_eq!(widths, vec![3, 2, 2]);
        assert_eq!(m[1].indent, 4);
    }

    /// ② 行数口径:`split('\n')`,空文档 1 行,与 gutter 的 1 + `\n` 数
    /// 同构(editor.rs 的 total_lines 口径)。
    #[test]
    fn line_count_matches_gutter_convention() {
        let p = LineParams::factory();
        assert_eq!(line_model("", p).len(), 1);
        assert_eq!(line_model("a", p).len(), 1);
        assert_eq!(line_model("a\n", p).len(), 2, "末尾换行产出尾空行");
        assert_eq!(line_model("a\n\nb", p).len(), 3);
    }

    /// ③ 缓存命中与失效(纯层):同 rev 同参数返回同一块内存(指针稳定
    /// = 未重算);编辑推进 rev 才重建;参数变化重建。
    #[test]
    fn cache_rebuilds_only_on_rev_or_params_change() {
        let mut editor = EditorBuffer::new("aaa\n    bb\n你");
        let p = LineParams::factory();
        let mut cache = MinimapCache::default();

        let first = cache.ensure(&editor, p);
        let first_ptr = first.as_ptr();
        assert_eq!(first.len(), 3);
        // 同 rev 命中:指针不动
        assert_eq!(
            cache.ensure(&editor, p).as_ptr(),
            first_ptr,
            "同 rev 同参数命中不重算"
        );

        // 编辑推进 rev → 重建,指针变化、内容更新
        editor.insert_chars(0, "x");
        let rebuilt = cache.ensure(&editor, p);
        let rebuilt_ptr = rebuilt.as_ptr();
        assert_ne!(rebuilt_ptr, first_ptr, "rev 前进后重建");
        assert_eq!(rebuilt.len(), 3);
        assert_eq!(rebuilt[0].width, 4, "新文本首行 xaaa 宽 4");

        // 空闲(无编辑)依旧命中新表
        assert_eq!(
            cache.ensure(&editor, p).as_ptr(),
            rebuilt_ptr,
            "无编辑帧命中未重算"
        );

        // 参数变化(tab 口径变)→ 重建
        let p2 = LineParams {
            tab_units: 8,
            max_units: 96,
        };
        let via_p2_ptr = cache.ensure(&editor, p2).as_ptr();
        assert_ne!(via_p2_ptr, rebuilt_ptr, "参数变化重建");
        // 换回原参数:params 槽已被 p2 覆盖,须重建回 tab=4 口径
        assert_eq!(
            cache.ensure(&editor, p)[1].indent,
            4,
            "tab=4 口径的缩进保持"
        );
    }

    /// ③ 缓存按修订号键控而非内容:undo 回到相同文本但 rev 不同,照键控
    /// 语义重建(与预览快照 `synced_rev` 同语义,rev 单调不回退)。
    #[test]
    fn cache_keys_on_revision_not_content() {
        let mut editor = EditorBuffer::new("甲");
        let p = LineParams::factory();
        let mut cache = MinimapCache::default();
        let before = cache.ensure(&editor, p).as_ptr();

        editor.insert_chars(1, "乙"); // "甲乙"
        let mid = cache.ensure(&editor, p).as_ptr();
        assert_ne!(mid, before);

        editor.remove_chars(1..2); // 回到 "甲",rev 再 +1
        let after = cache.ensure(&editor, p);
        assert_ne!(after.as_ptr(), mid, "内容相同但 rev 不同,照键控语义重建");
        assert_eq!(after[0].width, 2, "内容正确(回到「甲」)");
    }

    /// ③ per-tab 隔离(egui temp memory 层):两个标签各一槽互不覆盖;
    /// 编辑 A 只失效 A;切走再切回命中 B 的既有行模型(存储指针不变)。
    #[test]
    fn per_tab_slots_isolate_and_survive_tab_switch() {
        let ctx = egui::Context::default();
        let mut doc_a = EditorBuffer::new("a文档\n    缩进");
        let doc_b = EditorBuffer::new("b文档");
        let id_a = editor::tab_editor_id(1);
        let id_b = editor::tab_editor_id(2);
        let p = LineParams::factory();

        with_lines(&ctx, id_a, &doc_a, p, |lines| assert_eq!(lines.len(), 2));
        // B 首帧:记下存储条目里 Vec 的指针(with_lines 借出的就是存储本体)
        let b_ptr = with_lines(&ctx, id_b, &doc_b, p, |lines| lines.as_ptr());

        // 编辑 A → 只失效 A 的槽;B 的 synced_rev 纹丝不动
        doc_a.insert_chars(0, "!");
        with_lines(&ctx, id_a, &doc_a, p, |_| {});
        let a_slot = ctx
            .data(|d| d.get_temp::<MinimapCache>(cache_id(id_a)))
            .unwrap();
        assert_eq!(
            a_slot.synced_rev,
            Some(doc_a.revision()),
            "A 已重建到新 rev"
        );
        let b_slot = ctx
            .data(|d| d.get_temp::<MinimapCache>(cache_id(id_b)))
            .unwrap();
        assert_eq!(
            b_slot.synced_rev,
            Some(doc_b.revision()),
            "B 的缓存不受 A 的编辑影响"
        );

        // 切回 B(往返):命中既有行模型 —— 存储指针与切走前一致
        with_lines(&ctx, id_b, &doc_b, p, |lines| {
            assert_eq!(lines.as_ptr(), b_ptr, "切回 B 命中既有行模型,零重建");
            assert_eq!(lines.len(), 1);
        });
    }

    /// ④ 可见窗口:首/中/尾、滚动比例钳制(10000 行档,3px 比例尺,
    /// 600px 视口:总高 30000、行程 29400)。
    #[test]
    fn window_computes_first_middle_tail() {
        let base = WindowInput {
            minimap_height: 600.0,
            total_lines: 10000,
            scroll_ratio: 0.0,
            row_h: ROW_H,
        };
        // 首:0..200,y 从 0 起
        let top = window(base);
        assert_eq!(top.range(), 0..200);
        assert_eq!(top.y_of(0), Some(0.0));
        assert_eq!(top.y_of(199), Some(597.0));
        assert_eq!(top.y_of(200), None, "区间外无 y");

        // 中:比例 0.5 → 平移 14700,首行 4900,窗口顶行贴视口顶
        let mid = window(WindowInput {
            scroll_ratio: 0.5,
            ..base
        });
        assert_eq!(mid.range(), 4900..5100);
        assert_eq!(mid.y_of(4900), Some(0.0));
        // y 随行号单调递增,步长 = 比例尺
        let ys: Vec<f32> = mid.iter().map(|(_, y)| y).collect();
        assert!(ys.windows(2).all(|w| w[1] - w[0] == ROW_H));

        // 尾:比例 1 → 平移 29400,末行 9999 在区间内、不越总行数
        let tail = window(WindowInput {
            scroll_ratio: 1.0,
            ..base
        });
        assert_eq!(tail.range(), 9800..10000);
        assert_eq!(tail.y_of(9999), Some(597.0));
        assert!(tail.range().end <= 10000, "不越总行数");

        // 滚动比例超界钳制
        let over = window(WindowInput {
            scroll_ratio: 3.0,
            ..base
        });
        assert_eq!(over.range(), tail.range(), "比例 >1 钳到 1");
        let under = window(WindowInput {
            scroll_ratio: -1.0,
            ..base
        });
        assert_eq!(under.range(), top.range(), "比例 <0 钳到 0");
    }

    /// ④ 文档短于视口:全部行可见、顶部对齐,滚动比例不影响铺排。
    #[test]
    fn short_document_fills_from_top_without_scroll() {
        let short = WindowInput {
            minimap_height: 600.0,
            total_lines: 10,
            scroll_ratio: 0.0,
            row_h: ROW_H,
        };
        let w = window(short);
        assert_eq!(w.range(), 0..10, "短文档全可见");
        assert_eq!(w.y_of(0), Some(0.0));
        assert_eq!(w.y_of(9), Some(27.0), "不满高:10 行只占 30px,余量留底");

        // 编辑器滚不动(内容不满一屏)时 M2 应给比例 0;即便误传非零,
        // 短文档 travel=0,铺排纹丝不动。
        let nudged = window(WindowInput {
            scroll_ratio: 0.7,
            ..short
        });
        assert_eq!(nudged.range(), w.range());
        assert_eq!(nudged.y_of(9), Some(27.0));
    }

    /// ④ 边界几何:半露行(首行 y 为负、视口底 ceil 进一)、零高视口、
    /// 比例尺退化下限。
    #[test]
    fn window_handles_half_visible_and_degenerate_inputs() {
        let base = WindowInput {
            minimap_height: 600.0,
            total_lines: 10000,
            scroll_ratio: 0.0,
            row_h: ROW_H,
        };
        // 平移 1.5px:首行条半露(y=-1.5),区间仍从 0 起
        let half = window(WindowInput {
            scroll_ratio: 1.5 / 29400.0,
            ..base
        });
        assert_eq!(half.range().start, 0);
        assert_eq!(half.y_of(0), Some(-1.5), "半露行 y 为负");
        // 视口底落在 200.5 行处:ceil 进一,第 200 行半露在底
        assert_eq!(half.range().end, 201);

        // 零高视口:至少一行(避免空区间),不 panic
        let zero = window(WindowInput {
            minimap_height: 0.0,
            ..base
        });
        assert_eq!(zero.range().len(), 1);

        // 比例尺退化为 0:钳到 0.5px 下限,行数不发散
        let tiny = window(WindowInput {
            minimap_height: 600.0,
            total_lines: 1000,
            row_h: 0.0,
            scroll_ratio: 0.0,
        });
        assert_eq!(
            tiny.range(),
            0..1000,
            "row_h 钳 0.5 后 1000 行=500px 全可见"
        );
        assert!(tiny.iter().all(|(_, y)| y >= 0.0));
    }

    /// ⑤ 窗口只随滚动平移、行模型不重算:滚动比例扫全程,行模型的存储
    /// 指针与内容纹丝不动(命中路径);窗口首行随比例单调前移。
    #[test]
    fn scrolling_shifts_window_without_touching_model() {
        let ctx = egui::Context::default();
        // "行\n" ×10000 → 10001 行(含尾空行,与 split('\n') 口径一致)
        let editor = EditorBuffer::new(&"行\n".repeat(10000));
        let id = editor::tab_editor_id(7);
        let p = LineParams::factory();

        let model_ptr = with_lines(&ctx, id, &editor, p, |lines| lines.as_ptr());
        let snapshot: Vec<LineDesc> = with_lines(&ctx, id, &editor, p, |lines| lines.to_vec());

        let mut firsts = Vec::new();
        for i in 0..=10 {
            let ratio = f64::from(i) / 10.0;
            let w = window(WindowInput {
                minimap_height: 600.0,
                total_lines: 10001,
                scroll_ratio: ratio as f32,
                row_h: ROW_H,
            });
            assert!(!w.range().is_empty());
            firsts.push(w.range().start);
            // 每个滚动档都取一次模型:指针与内容都必须与首帧一致
            let (ptr_now, now) = with_lines(&ctx, id, &editor, p, |lines| {
                (lines.as_ptr(), lines.to_vec())
            });
            assert_eq!(ptr_now, model_ptr, "滚动帧不重建行模型(第 {i} 档)");
            assert_eq!(now, snapshot, "内容与首帧逐元素相等");
        }
        // 窗口确实随比例平移:首行单调不减,顶档从 0、尾档贴文末
        assert_eq!(firsts.first(), Some(&0), "顶档从 0 起");
        assert_eq!(
            firsts.last(),
            Some(&9801),
            "尾档停在 9801(10001 行:平移 29403px、首行 floor(29403/3))"
        );
        assert!(
            firsts.windows(2).all(|w| w[1] >= w[0]),
            "比例单调 → 首行单调({firsts:?})"
        );
    }

    /// ⑤ 大文档档位(#55 否决线的 10000 行):建表一次成器,行数与全表
    /// 封顶不变量成立;耗时口径归 perf-recheck 基准,此处钉正确性。
    #[test]
    fn ten_thousand_line_document_builds_correctly() {
        let p = LineParams::factory();
        let text = (0..10000)
            .map(|i| {
                if i % 10 == 0 {
                    format!("# 标题{i}")
                } else if i % 5 == 0 {
                    format!("    缩进行{i}")
                } else {
                    format!("普通行 {i}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let model = line_model(&text, p);
        assert_eq!(model.len(), 10000);
        // "# 标题0" = 1 + 1 + 2 + 2 + 1
        assert_eq!(model[0].width, 7);
        assert_eq!(model[5].indent, 4, "第 6 行是缩进行");
        assert!(model.iter().all(|d| d.indent <= p.max_units));
        assert!(
            model
                .iter()
                .all(|d| d.width.saturating_add(d.indent) <= p.max_units),
            "缩进+长度联合封顶"
        );
    }

    // —— M2 渲染与跳转(#55)——

    use crate::live::{LiveState, RenderMode};
    use crate::state::{OutlineCursor, PreviewState};
    use egui::epaint::ClippedShape;

    /// 跑一帧源码模式编辑面板(带 minimap 开关;视口 800×600 与 editor.rs
    /// 滚动测试同口径),返回 (shapes, 探针)。探针 None = 本帧没进 minimap
    /// 渲染路径(关闭态的零元素断言就用它)。
    fn frame(
        ctx: &egui::Context,
        editor: &mut EditorBuffer,
        now: f64,
        show_minimap: bool,
        events: Vec<egui::Event>,
    ) -> (Vec<ClippedShape>, Option<MinimapProbe>) {
        let mut preview = PreviewState::new(editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = None;
        let mut live = LiveState::default();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let editor_id = editor::tab_editor_id(1);
        let output = ctx.run_ui(
            egui::RawInput {
                events,
                time: Some(now),
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                editor::ui(
                    ui,
                    editor,
                    &mut preview,
                    editor::CursorChannel {
                        cursor: &mut cursor,
                        selection: &mut selection,
                        pending: &mut pending,
                    },
                    &mut live,
                    RenderMode::Source,
                    editor_id,
                    show_minimap,
                );
            },
        );
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        let probe = ctx.data(|d| d.get_temp::<MinimapProbe>(probe_id(editor_id)));
        (shapes, probe)
    }

    /// shapes 里的 minimap 行条:高恰为 [`ROW_H`]、宽 ≤ 条区 96px 的填充
    /// 矩形(行条是全帧唯一的 3px 高矩形形状)。返回 (矩形, 颜色)。
    fn bar_rects(shapes: &[ClippedShape]) -> Vec<(egui::Rect, egui::Color32)> {
        shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(r) if r.fill != egui::Color32::TRANSPARENT => {
                    let rect = r.rect;
                    (rect.height() == ROW_H && rect.width() <= 96.0).then_some((rect, r.fill))
                }
                _ => None,
            })
            .collect()
    }

    /// shapes 里的视口高亮框:横跨整条、宽 = [`MINIMAP_W`] 的填充矩形。
    fn viewport_rect(shapes: &[ClippedShape]) -> Option<egui::Rect> {
        shapes.iter().find_map(|clipped| match &clipped.shape {
            egui::Shape::Rect(r) => (r.fill != egui::Color32::TRANSPARENT
                && (r.rect.width() - MINIMAP_W).abs() < 0.5)
                .then_some(r.rect),
            _ => None,
        })
    }

    /// ⑤-1 关闭态零渲染(否决线):开关关 → 无探针、无行模型缓存、无行条
    /// 形状,TextEdit 右缘保持让位前的现状(不因开关存在而变窄)。
    #[test]
    fn disabled_minimap_renders_zero_elements() {
        let ctx = egui::Context::default();
        let text = (0..500)
            .map(|_| "普通的一行")
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = EditorBuffer::new(&text);
        let id = editor::tab_editor_id(1);

        frame(&ctx, &mut editor, 0.0, false, Vec::new());
        let (shapes, probe) = frame(&ctx, &mut editor, 0.1, false, Vec::new());
        assert!(probe.is_none(), "关闭帧不写探针");
        assert!(
            ctx.data(|d| d.get_temp::<MinimapCache>(cache_id(id)))
                .is_none(),
            "关闭帧不进行模型缓存路径"
        );
        assert!(bar_rects(&shapes).is_empty(), "关闭帧零行条形状");
        assert!(viewport_rect(&shapes).is_none(), "关闭帧零高亮框形状");
        // 关闭 = TextEdit 吃满剩余宽(与 #55 之前一致):右缘贴近视口右缘
        let text_rect = ctx.read_response(id).expect("TextEdit 响应已记录").rect;
        assert!(
            (800.0 - text_rect.right()).abs() < 12.0,
            "关闭时 TextEdit 右缘贴面板(实测 {}),不被窄条挤窄",
            text_rect.right()
        );
    }

    /// ⑤-2 开启态可见行条数 = 窗口计算结果:1000 行全非空文档、600px
    /// minimap、顶部对齐 → 行条数恰为 window() 区间长;行条都落在窄条
    /// 96px 行条区内,TextEdit 同步让位。
    #[test]
    fn enabled_minimap_paints_exactly_the_visible_window() {
        let ctx = egui::Context::default();
        let text = (0..1000)
            .map(|_| "普通的一行")
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = EditorBuffer::new(&text);
        let id = editor::tab_editor_id(1);

        frame(&ctx, &mut editor, 0.0, true, Vec::new());
        let (shapes, probe) = frame(&ctx, &mut editor, 0.1, true, Vec::new());
        let expect = window(WindowInput {
            minimap_height: 600.0,
            total_lines: 1000,
            scroll_ratio: 0.0,
            row_h: ROW_H,
        })
        .range()
        .len();
        // 1000 行全非空:非零行条数 == 窗口区间长
        let probe = probe.expect("开启帧写探针");
        assert_eq!(probe.bars, expect);
        let bars = bar_rects(&shapes);
        assert_eq!(bars.len(), expect, "行条形状数与探针一致");
        // 高亮框探针与 shapes 实画一致(取证两面互证)
        assert_eq!(
            probe.viewport,
            Some(viewport_rect(&shapes).expect("高亮框已绘制")),
            "探针高亮框与实画形状一致"
        );
        // 行条几何:都在右缘窄条的行条区(x ∈ [map_left+pad, +96]),y 从 0 递增
        let map_left = viewport_rect(&shapes).expect("高亮框已绘制").left();
        for (rect, _) in &bars {
            assert!(
                rect.left() >= map_left && rect.right() <= map_left + MINIMAP_W,
                "行条落在窄条内({rect:?}, map_left={map_left})"
            );
        }
        // TextEdit 让位:右缘离面板右缘 ≈ MINIMAP_W(±16 容差吸收 gutter
        // 右缘 token 间距与 TextEdit 自身边距)
        let text_rect = ctx.read_response(id).expect("TextEdit 响应已记录").rect;
        assert!(
            (800.0 - text_rect.right() - MINIMAP_W).abs() < 16.0,
            "开启时 TextEdit 右缘让出窄条(实测右缘 {})",
            text_rect.right()
        );
    }

    /// ⑤-3 跳转映射(纯函数):点击处对准视口中心;点顶/点底钳到端点,
    /// 短文档与不满屏内容返回 None,位置 → 目标单调。
    #[test]
    fn jump_ratio_maps_pointer_to_centered_target() {
        // 10000 行、600px minimap(200 行窗口)、ve = 600/30000
        let base = JumpInput {
            minimap_height: 600.0,
            total_lines: 10000,
            scroll_ratio: 0.0,
            row_h: ROW_H,
            pointer_y: 0.0,
            viewport_frac: 600.0 / 30000.0,
        };
        // 点顶:目标钳 0
        assert_eq!(
            jump_ratio(JumpInput {
                pointer_y: 0.0,
                ..base
            }),
            Some(0.0)
        );
        // 点底(y=600):p=0.02 > ve/2 → 正向小步
        let bottom = jump_ratio(JumpInput {
            pointer_y: 600.0,
            ..base
        })
        .expect("长文档有行程");
        assert!(bottom > 0.0 && bottom < 0.05, "点底小步正向(实测 {bottom})");
        // 点中(y=300,此时 p=0.01≈ve/2 略小)…… 用比例推:点在 minimap
        // 中部、当前也在中部时目标 ≈ 中部。直接构造 p=0.5:pointer_y +
        // offset = 15000 → offset = 14700 → ratio = 0.5
        let mid = jump_ratio(JumpInput {
            scroll_ratio: 0.5,
            pointer_y: 300.0,
            ..base
        })
        .expect("长文档有行程");
        assert!((mid - 0.5).abs() < 0.01, "文档中部对准视口中心(实测 {mid})");
        // 单调:指针下移 → 目标比例不减
        let mut previous = f32::NEG_INFINITY;
        for step in 0..=10 {
            let target = jump_ratio(JumpInput {
                pointer_y: f32::from(step as u16) * 60.0,
                ..base
            })
            .expect("长文档有行程");
            assert!(
                target >= previous,
                "指针下移目标单调({previous} → {target})"
            );
            previous = target;
        }
        // 钳制:指针越界(负)仍给端点;滚到底时点视口底 → 文档底(视口
        // 顶对文末,minimap 平移量此时最大)
        assert_eq!(
            jump_ratio(JumpInput {
                pointer_y: -50.0,
                ..base
            }),
            Some(0.0)
        );
        assert_eq!(
            jump_ratio(JumpInput {
                scroll_ratio: 1.0,
                pointer_y: 600.0,
                ..base
            }),
            Some(1.0),
            "minimap 平移到底 + 点视口底 = 文档底"
        );
        // 短文档(minimap 不满一屏)→ 编辑器同样滚不动 → None
        assert_eq!(
            jump_ratio(JumpInput {
                total_lines: 10,
                viewport_frac: 1.0,
                ..base
            }),
            None
        );
        // 内容不满一屏(ve=1)→ None
        assert_eq!(
            jump_ratio(JumpInput {
                viewport_frac: 1.0,
                ..base
            }),
            None
        );
    }

    /// ⑤-3 端到端:点击 minimap 中部 → 编辑器滚动前进,光标纹丝不动
    /// (跳转只动滚动 offset,不触碰 TextEdit 持久光标)。
    #[test]
    fn clicking_minimap_scrolls_without_moving_cursor() {
        let ctx = egui::Context::default();
        let text = (0..500)
            .map(|_| "普通的一行")
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = EditorBuffer::new(&text);
        let id = editor::tab_editor_id(1);

        frame(&ctx, &mut editor, 0.0, true, Vec::new());
        let (shapes, _) = frame(&ctx, &mut editor, 0.1, true, Vec::new());
        // 高亮框只标编辑器视口对应的短段;点击目标取窄条自身的中下部
        // (窄条高 = 编辑区高 600,y=400 离当前视口中心够远,滚动量明确)。
        let map = viewport_rect(&shapes).expect("高亮框定位窄条");
        let click = egui::pos2(map.center().x, 400.0);
        let top_before = ctx.read_response(id).expect("TextEdit 已记录").rect.top();
        let cursor_before = egui::widgets::text_edit::TextEditState::load(&ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| (r.primary.index.0, r.secondary.index.0));

        // 点击:press 与 release 分两帧(clicked = 双帧语义)
        frame(
            &ctx,
            &mut editor,
            0.2,
            true,
            vec![
                egui::Event::PointerMoved(click),
                egui::Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        frame(
            &ctx,
            &mut editor,
            0.3,
            true,
            vec![egui::Event::PointerButton {
                pos: click,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        // 落账链条比滚轮多一环(意图帧→闭包消费帧→offset 生效帧→布局
        // 反映帧→read_response 可读帧),补四帧再取证。
        for step in 0..4 {
            frame(
                &ctx,
                &mut editor,
                0.4 + f64::from(step) * 0.1,
                true,
                Vec::new(),
            );
        }

        let top_after = ctx.read_response(id).expect("TextEdit 已记录").rect.top();
        assert!(
            top_after < top_before - 50.0,
            "点击 minimap 把视口推离文档顶(实测 {top_before} → {top_after})"
        );
        let cursor_after = egui::widgets::text_edit::TextEditState::load(&ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| (r.primary.index.0, r.secondary.index.0));
        assert_eq!(
            cursor_before, cursor_after,
            "跳转不动光标(前后持久光标一致)"
        );
    }

    /// ⑤-3 端到端:按住拖动逐帧下移 → 滚动逐帧跟随(offset 单调前进)。
    #[test]
    fn dragging_minimap_follows_continuously() {
        let ctx = egui::Context::default();
        let text = (0..500)
            .map(|_| "普通的一行")
            .collect::<Vec<_>>()
            .join("\n");
        let mut editor = EditorBuffer::new(&text);
        let id = editor::tab_editor_id(1);

        frame(&ctx, &mut editor, 0.0, true, Vec::new());
        let (shapes, _) = frame(&ctx, &mut editor, 0.1, true, Vec::new());
        let map = viewport_rect(&shapes).expect("高亮框定位窄条");
        let press = egui::pos2(map.center().x, map.top() + 40.0);
        frame(
            &ctx,
            &mut editor,
            0.2,
            true,
            vec![
                egui::Event::PointerMoved(press),
                egui::Event::PointerButton {
                    pos: press,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );

        // 逐帧跟随用 metrics.offset 断言(ScrollArea 帧末真值,无布局与
        // read_response 各一帧的滞后);屏幕坐标的最终落点在 settle 帧后验。
        let mut previous = 0.0_f32;
        let mut moved = 0;
        for step in 0..6 {
            let pos = egui::pos2(press.x, press.y + f32::from(step as u16) * 40.0);
            frame(
                &ctx,
                &mut editor,
                0.3 + f64::from(step) * 0.1,
                true,
                vec![egui::Event::PointerMoved(pos)],
            );
            let offset: f32 = ctx
                .data(|d| d.get_temp::<ScrollMetrics>(metrics_id(id)))
                .map_or(0.0, |m| m.offset);
            assert!(
                offset >= previous - 0.5,
                "拖动下移滚动单调不减(第 {step} 步 {previous} → {offset})"
            );
            if offset > previous + 1.0 {
                moved += 1;
            }
            previous = offset;
        }
        assert!(moved >= 3, "拖动确实连续跟随(6 步中 {moved} 步在滚)");
        // settle(意图→消费→offset→布局→可读各差一帧)后,屏幕坐标到位
        for step in 0..4 {
            frame(
                &ctx,
                &mut editor,
                0.9 + f64::from(step) * 0.1,
                true,
                Vec::new(),
            );
        }
        let top = ctx.read_response(id).expect("TextEdit 已记录").rect.top();
        assert!(
            top < -50.0,
            "拖到窄条下部后视口深入文档(实测 top {top},offset {previous})"
        );
    }

    /// ⑤-4 10000 行档连续滚动若干帧无 panic,行模型缓存命中(存储指针
    /// 纹丝不动 = 无逐帧全量重建)。
    #[test]
    fn ten_k_line_scrolling_keeps_cache_hot_without_panic() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new(&"行\n".repeat(10000)); // 10001 行
        let id = editor::tab_editor_id(1);
        let p = LineParams::factory();

        frame(&ctx, &mut editor, 0.0, true, Vec::new());
        let model_ptr = with_lines(&ctx, id, &editor, p, |lines| lines.as_ptr());

        for i in 0..10 {
            let (shapes, probe) = frame(
                &ctx,
                &mut editor,
                0.1 + f64::from(i) * 0.1,
                true,
                vec![
                    egui::Event::PointerMoved(egui::pos2(200.0, 300.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -120.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            let probe = probe.expect("滚动帧 minimap 照常绘制");
            assert!(probe.bars > 0, "第 {i} 帧行条在画");
            assert!(!bar_rects(&shapes).is_empty(), "第 {i} 帧行条形状在");
            let (ptr, len) =
                with_lines(&ctx, id, &editor, p, |lines| (lines.as_ptr(), lines.len()));
            assert_eq!(ptr, model_ptr, "滚动帧命中缓存,零重建(第 {i} 帧)");
            assert_eq!(len, 10001);
        }
    }

    /// ⑤-5 两主题渲染:明暗各三帧不 panic,行条色随主题翻转(暗色偏白、
    /// 亮色偏黑,且都非透明)。
    #[test]
    fn both_themes_paint_distinct_bar_colors() {
        let mut seen = Vec::new();
        for dark in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            let mut editor = EditorBuffer::new("明暗各三帧\n不 panic\n");
            for step in 0..3 {
                let (shapes, probe) =
                    frame(&ctx, &mut editor, f64::from(step) * 0.1, true, Vec::new());
                let bars = bar_rects(&shapes);
                assert!(
                    bars.iter().all(|(_, c)| c.a() > 0),
                    "{} 第 {step} 帧行条非透明",
                    if dark { "暗色" } else { "亮色" }
                );
                if step == 2 {
                    assert!(probe.expect("开启帧写探针").bars >= 2);
                    seen.push((dark, bars[0].1));
                }
            }
        }
        let (dark_color, light_color) = (seen[0].1, seen[1].1);
        assert_ne!(dark_color, light_color, "两主题行条色不同");
        assert!(
            dark_color.r() > light_color.r(),
            "暗色主题行条偏白、亮色偏黑(实测 {dark_color:?} vs {light_color:?})"
        );
    }

    /// 范围:仅源码模式 —— Live 模式整帧不进 minimap 渲染路径(无探针、
    /// 无行条形状)。
    #[test]
    fn live_mode_renders_no_minimap() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new("# 标题\n\n正文一段。\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = None;
        let mut live = LiveState::default();
        let editor_id = editor::tab_editor_id(1);
        let output = ctx.run_ui(
            egui::RawInput {
                time: Some(0.0),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                // 开关开着也只该在源码模式生效
                editor::ui(
                    ui,
                    &mut editor,
                    &mut preview,
                    editor::CursorChannel {
                        cursor: &mut cursor,
                        selection: &mut selection,
                        pending: &mut pending,
                    },
                    &mut live,
                    RenderMode::Live,
                    editor_id,
                    true,
                );
            },
        );
        let shapes = output.shapes.clone();
        output.drop_without_applying_deltas();
        assert!(
            ctx.data(|d| d.get_temp::<MinimapProbe>(probe_id(editor_id)))
                .is_none(),
            "Live 帧不写 minimap 探针"
        );
        assert!(bar_rects(&shapes).is_empty(), "Live 帧零行条形状");
        assert!(viewport_rect(&shapes).is_none(), "Live 帧零高亮框");
    }

    /// 范围:切 tab 后开关照常渲染(全局开关)+ 各 tab 行模型分槽
    /// (第二个 tab 首帧用自己的槽重建,不影响第一个 tab 的缓存)。
    #[test]
    fn tab_switch_keeps_toggle_and_slot_isolation() {
        let ctx = egui::Context::default();
        let mut editor = EditorBuffer::new(&"行\n".repeat(300));
        frame(&ctx, &mut editor, 0.0, true, Vec::new()); // tab1 建立
        let id1 = editor::tab_editor_id(1);
        let ptr1 = with_lines(&ctx, id1, &editor, LineParams::factory(), |l| l.as_ptr());

        // 第二个标签同开关渲染(生产里 tab2 用 tab_editor_id(2);本测试
        // 直接验证分槽:tab2 的槽独立建立)
        let id2 = editor::tab_editor_id(2);
        with_lines(&ctx, id2, &editor, LineParams::factory(), |lines| {
            assert_eq!(lines.len(), 301, "tab2 槽独立成表");
        });
        // tab1 缓存不受影响
        assert_eq!(
            with_lines(&ctx, id1, &editor, LineParams::factory(), |l| l.as_ptr()),
            ptr1,
            "tab1 缓存纹丝不动"
        );
        // 开关再渲一帧(tab 往返语义):探针仍在
        let (_, probe) = frame(&ctx, &mut editor, 0.1, true, Vec::new());
        assert!(probe.is_some_and(|p| p.bars > 0), "切回照常渲染");
    }
}
