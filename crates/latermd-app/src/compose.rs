//! Markdown 格式动作的**纯函数层**(docs/ui-shell-redesign.md §6.2)。
//!
//! 所有语义都在这里,`ui` 只负责画按钮、`logic` 只负责把结果写回缓冲:
//! `apply` 不吃 egui、不吃 IO,输入字符串与**字符**偏移,输出新字符串与
//! 新选区。十七条规则因此全都能落到单测上。
//!
//! ## 偏移一律是字符
//!
//! CJK 占三字节,按字节切会 panic 在字符中间。`EditorBuffer` 已有
//! `char_to_byte` / `byte_to_char`,`TextEdit` 的 `CCursor.index` 本身也
//! 是字符偏移 —— 字符偏移是三方的最小公约数。本模块内部只在最后合成
//! 字符串时用到字节边界,对外一律字符。
//!
//! ## Undo 粒度
//!
//! 走本模块的写入会打碎 `TextEdit` 内建 undoer 的快照,Ctrl+Z 可能一次
//! 回退一整次格式操作而非逐字。已知并接受(§9 R3,与 AI 流式首次 Ctrl+Z
//! 整段回退同款边界);单测因此只钉「文本与选区正确」,不钉 undo 粒度。

use std::ops::Range;

/// 格式动作。十六个,与工具条四组按钮一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAction {
    /// `**粗体**`
    Bold,
    /// `*斜体*`
    Italic,
    /// `~~删除线~~`
    Strike,
    /// `` `行内代码` ``
    InlineCode,
    /// `[标题](url)`
    Link,
    /// `# `
    H1,
    /// `## `
    H2,
    /// `### `
    H3,
    /// 正文:去掉行前缀。
    Plain,
    /// `> `
    Quote,
    /// 围栏代码块(info string 空)。
    CodeBlock,
    /// `---`
    Divider,
    /// 2×2 表格骨架。
    Table,
    /// `- `
    Bullet,
    /// `1. `
    Ordered,
    /// `- [ ] ` → `- [x] ` → `- ` 三态循环。
    Task,
}

impl FormatAction {
    /// 悬浮提示文案(`ui::format_bar` 拼上快捷键后显示)。
    pub fn label(self) -> &'static str {
        use FormatAction::*;
        match self {
            Bold => "加粗",
            Italic => "斜体",
            Strike => "删除线",
            InlineCode => "行内代码",
            Link => "链接",
            H1 => "一级标题",
            H2 => "二级标题",
            H3 => "三级标题",
            Plain => "正文",
            Quote => "引用",
            CodeBlock => "代码块",
            Divider => "分割线",
            Table => "表格",
            Bullet => "无序列表",
            Ordered => "有序列表",
            Task => "任务列表",
        }
    }

    /// 工具条顺序:组内按 `ALL` 出现顺序,组间按
    /// [`FormatGroup::ALL`]。
    pub const ALL: [FormatAction; 16] = [
        FormatAction::Bold,
        FormatAction::Italic,
        FormatAction::Strike,
        FormatAction::InlineCode,
        FormatAction::Link,
        FormatAction::H1,
        FormatAction::H2,
        FormatAction::H3,
        FormatAction::Plain,
        FormatAction::Quote,
        FormatAction::CodeBlock,
        FormatAction::Divider,
        FormatAction::Table,
        FormatAction::Bullet,
        FormatAction::Ordered,
        FormatAction::Task,
    ];
}

/// 工具条的四个视觉分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatGroup {
    /// 行内:B / I / S / 行内代码 / 链接。
    Inline,
    /// 标题:H1 / H2 / H3 / 正文。
    Heading,
    /// 块:引用 / 代码块 / 分割线 / 表格。
    Block,
    /// 列表:无序 / 有序 / 任务。
    List,
}

impl FormatGroup {
    /// 自左至右的顺序;组之间画竖分隔条,最后一组之后不画。
    pub const ALL: [FormatGroup; 4] = [
        FormatGroup::Inline,
        FormatGroup::Heading,
        FormatGroup::Block,
        FormatGroup::List,
    ];

    /// 该组的动作,顺序即按钮顺序。
    pub fn actions(self) -> &'static [FormatAction] {
        // 各组的 `FormatAction::ALL` 前缀/片段;写成切片以免每帧分配。
        match self {
            FormatGroup::Inline => &FormatAction::ALL[0..5],
            FormatGroup::Heading => &FormatAction::ALL[5..9],
            FormatGroup::Block => &FormatAction::ALL[9..13],
            FormatGroup::List => &FormatAction::ALL[13..16],
        }
    }
}

/// 2×2 表格骨架。列宽由用户内容撑,这里只保证 GFM 认得出是表格。
const TABLE_SKELETON: &str = "| 列 1 | 列 2 |\n| --- | --- |\n|  |  |\n|  |  |\n";

/// 链接 placeholders:新建链接时 url 占位成 `https://`,让用户直接覆写。
const LINK_URL: &str = "https://";

/// 应用一次格式动作,返回**(新文本, 新选区)**。
///
/// `sel` 是字符区间,`start == end` 表示纯光标。返回选区同样是字符区间:
/// 行内类一律落在「内容处」,方便接着打字或叠加别的格式。
pub fn apply(action: FormatAction, text: &str, sel: Range<usize>) -> (String, Range<usize>) {
    use FormatAction::*;
    let view = View::new(text, sel);
    match action {
        Bold => view.wrap("**", "**"),
        Italic => view.wrap("*", "*"),
        Strike => view.wrap("~~", "~~"),
        InlineCode => view.wrap("`", "`"),
        Link => view.link(),
        H1 => view.set_prefix("# "),
        H2 => view.set_prefix("## "),
        H3 => view.set_prefix("### "),
        Plain => view.set_prefix(""),
        Quote => view.set_prefix("> "),
        Bullet => view.set_prefix("- "),
        Ordered => view.set_prefix("1. "),
        Task => view.cycle_task(),
        CodeBlock => view.insert_block("```\n", "\n```\n"),
        Divider => view.insert_block("", "---\n"),
        Table => view.insert_block("", TABLE_SKELETON),
    }
}

/// 已知的**行前缀**,按长度降序 —— 匹配时要先试长的(`### ` 先于 `# `)。
const LINE_PREFIXES: [&str; 8] = ["### ", "## ", "# ", "- [x] ", "- [ ] ", "> ", "- ", "1. "];

/// 去掉 `line` 的行前缀(有则去一个最长的),剩下的正文。
fn strip_line_prefix(line: &str) -> &str {
    LINE_PREFIXES
        .iter()
        .filter(|prefix| prefix.len() <= line.len())
        .find(|prefix| line.starts_with(**prefix))
        .map(|prefix| &line[prefix.len()..])
        .unwrap_or(line)
}

/// `line` 当前的行前缀,没有则空串。
fn line_prefix_of(line: &str) -> &'static str {
    LINE_PREFIXES
        .iter()
        .filter(|prefix| prefix.len() <= line.len())
        .find(|prefix| line.starts_with(**prefix))
        .copied()
        .unwrap_or("")
}

/// 文本 + 选区的一次性视图,字符偏移。
///
/// 每个处理方法消耗 `self` 返回 `(String, Range<usize>)`,避免半路的中间
/// 偏移漏出去。
struct View<'a> {
    text: &'a str,
    /// 已归一化:start ≤ stop,且不越界。
    start: usize,
    stop: usize,
}

impl<'a> View<'a> {
    fn new(text: &'a str, sel: Range<usize>) -> Self {
        let len = text.chars().count();
        let start = sel.start.min(len);
        let stop = sel.end.min(len).max(start);
        Self { text, start, stop }
    }

    /// 纯光标(无选区)。
    fn caret_only(&self) -> bool {
        self.start == self.stop
    }

    /// 字符偏移 → 字节偏移。
    fn byte_of(&self, char_idx: usize) -> usize {
        self.text
            .char_indices()
            .nth(char_idx)
            .map_or(self.text.len(), |(byte, _)| byte)
    }

    /// 选区所在行的**行尾(不含换行符)**,以及行尾之后到下一个换行
    /// (或文末)的位置。
    fn selected_line_bytes(&self) -> (usize, usize) {
        let bytes = self.byte_of(self.start)..self.byte_of(self.stop);
        let line_start = self.text[..bytes.start]
            .rfind('\n')
            .map_or(0, |idx| idx + 1);
        let line_end = self.text[bytes.end..]
            .find('\n')
            .map_or(self.text.len(), |offset| bytes.end + offset);
        (line_start, line_end)
    }

    /// 选区所在行的结束位置,**含**行尾换行符 —— 插入类模板落在它之后。
    fn current_line_end_inclusive(&self) -> usize {
        let line_end = self.selected_line_bytes().1;
        self.text[line_end..]
            .find('\n')
            .map_or(self.text.len(), |offset| line_end + offset + 1)
    }

    /// **行内包裹**:无选区插成对标记、光标放中间;有选区则包裹;已被同
    /// 标记紧贴包裹则去包裹(toggle off)。
    fn wrap(self, open: &'static str, close: &'static str) -> (String, Range<usize>) {
        let (a, b) = (self.byte_of(self.start), self.byte_of(self.stop));
        let (left, mid, right) = (&self.text[..a], &self.text[a..b], &self.text[b..]);

        if self.caret_only() {
            let pos = left.chars().count() + open.chars().count();
            return (format!("{left}{open}{close}{right}"), pos..pos);
        }

        // toggle off 的判定要严:`left` 以 open 结尾且 `right` 以 close
        // 开头,两者同时成立才算「选区正被这一对包着」。只有一边成立时
        // 用户框的是别的东西(比如 `**a**` 里的 `a`),应当继续包裹。
        if left.ends_with(open) && right.starts_with(close) {
            let trimmed_left = &left[..left.len() - open.len()];
            let trimmed_right = &right[close.len()..];
            let start = trimmed_left.chars().count();
            let end = start + mid.chars().count();
            return (format!("{trimmed_left}{mid}{trimmed_right}"), start..end);
        }

        let start = left.chars().count() + open.chars().count();
        let end = start + mid.chars().count();
        (format!("{left}{open}{mid}{close}{right}"), start..end)
    }

    /// 链接:`[选中](url)`,新选区落在 url 上便于直接覆写;无选区时 dvस्थित
    /// 标题为空。
    fn link(self) -> (String, Range<usize>) {
        let (a, b) = (self.byte_of(self.start), self.byte_of(self.stop));
        let (left, mid, right) = (&self.text[..a], &self.text[a..b], &self.text[b..]);

        // 光标 vs 选区只影响"标题是否为空",其余完全一致 —— 一条路径足矣
        let url_start = left.chars().count() + 1 + mid.chars().count() + 2;
        let out = format!("{left}[{mid}]({LINK_URL}){right}");
        let url_end = url_start + LINK_URL.chars().count();
        (out, url_start..url_end)
    }

    /// **行前缀类**:作用于选区覆盖的所有整行。已是该前缀 → 去掉
    /// (toggle);是别的前缀 → 替换为新前缀(不叠加);`prefix` 为空即
    /// 「正文」:一律去前缀。
    fn set_prefix(self, prefix: &'static str) -> (String, Range<usize>) {
        let (line_start, line_end) = self.selected_line_bytes();
        let (head, lines, tail) = (
            &self.text[..line_start],
            &self.text[line_start..line_end],
            &self.text[line_end..],
        );

        let mut out = String::with_capacity(self.text.len() + 64);
        out.push_str(head);
        for line in lines.split_inclusive('\n') {
            let body = line.strip_suffix('\n').unwrap_or(line);
            let eol = if body.len() == line.len() { "" } else { "\n" };
            let current = line_prefix_of(line);
            // toggle:已经是目标前缀就不再叠一次
            let next = if current == prefix { "" } else { prefix };
            out.push_str(next);
            out.push_str(strip_line_prefix(body));
            out.push_str(eol);
        }
        out.push_str(tail);

        let start = head.chars().count();
        // 新选区覆盖改动过的整段行,便于立刻再点别的格式
        let end = out.chars().count() - tail.chars().count();
        (out, start..end)
    }

    /// 任务列表三态:`- [ ] ` → `- [x] ` → `- ` → `- [ ] `。
    ///
    /// 每行各自判定自己的当前态,混选(有的已勾有的没勾)时整体推进一档。
    fn cycle_task(self) -> (String, Range<usize>) {
        let view = View {
            text: self.text,
            start: self.start,
            stop: self.stop,
        };
        let (line_start, line_end) = view.selected_line_bytes();
        let (head, lines, tail) = (
            &self.text[..line_start],
            &self.text[line_start..line_end],
            &self.text[line_end..],
        );

        let mut out = String::with_capacity(self.text.len() + 64);
        out.push_str(head);
        for line in lines.split_inclusive('\n') {
            let body = line.strip_suffix('\n').unwrap_or(line);
            let eol = if body.len() == line.len() { "" } else { "\n" };
            let next = match body {
                b if b.starts_with("- [ ] ") => ["- [x] ", &b["- [ ] ".len()..]].concat(),
                b if b.starts_with("- [x] ") => ["- ", &b["- [x] ".len()..]].concat(),
                b if b.starts_with("- ") => ["- [ ] ", &b["- ".len()..]].concat(),
                b => ["- [ ] ", strip_line_prefix(b)].concat(),
            };
            out.push_str(&next);
            out.push_str(eol);
        }
        out.push_str(tail);

        let start = head.chars().count();
        let end = out.chars().count() - tail.chars().count();
        (out, start..end)
    }

    /// **插入类**:在当前行**下方**插入块级模板。
    ///
    /// `open` / `close` 之间放选中内容 —— 代码块用它,分割线与表格的上半
    /// 为空。模板前补一个空行(除非已在行首),避免粘在上一行正文里。
    fn insert_block(self, open: &'static str, close: &'static str) -> (String, Range<usize>) {
        // 模板落在「当前行之后」,即行尾换行符的后头 —— 用行尾(不含换行)
        // 会让模板插进当前行中间。
        let line_end = self.current_line_end_inclusive();
        let (head, tail) = self.text.split_at(line_end);
        let (a, b) = (self.byte_of(self.start), self.byte_of(self.stop));
        let mid = &self.text[a..b];

        // 补行:块级元素必须与上文空一行,否则 `---` 会被上一行吃掉变成
        // setext 标题、代码块会粘在正文里。已在行首(head 空)或已有空行
        // 时不补 —— 后者是幂等的第二次插入。
        let gap = if head.is_empty() || head.ends_with("\n\n") {
            ""
        } else if head.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        let prefix = format!("{head}{gap}");
        let body_start = prefix.chars().count() + open.chars().count();
        let body_end = body_start + mid.chars().count();
        let out = format!("{prefix}{open}{mid}{close}{tail}");
        (out, body_start..body_end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold(text: &str, sel: Range<usize>) -> (String, Range<usize>) {
        apply(FormatAction::Bold, text, sel)
    }

    /// `**x**`:无选区插成对标记 + 光标居中;有选区包裹;再点去包裹。
    #[test]
    fn bold_inserts_wraps_and_unwraps() {
        let (out, sel) = bold("abc", 1..1);
        assert_eq!(out, "a****bc");
        assert_eq!(sel, 3..3, "光标落在两对标记中间");

        let (out, sel) = bold("abc", 0..3);
        assert_eq!(out, "**abc**");
        assert_eq!(sel, 2..5, "新选区覆盖包裹后的内容");

        let (out, sel) = bold("**abc**", 2..5);
        assert_eq!(out, "abc", "已被同一对标记紧贴包裹 → 去包裹");
        assert_eq!(sel, 0..3);
    }

    /// 并未被包裹时继续包裹:`**a**` 里框住 `a` 再点加粗会得到嵌套而非
    /// 脱衣服 —— 这与"再点一次取消加粗"的直觉相反,所以 UI 侧要求新选区
    /// 覆盖包裹后的**整段**(含标记),下一次点击才是 toggle off。
    #[test]
    fn nested_selection_wraps_again() {
        let (out, sel) = bold("**abc**", 0..7);
        assert_eq!(out, "****abc****");
        assert_eq!(sel, 2..9, "整段被再包一层,下次点是去掉外层");
        let (out, _) = bold(&out, sel);
        assert_eq!(out, "**abc**", "再点一次脱掉外层");
    }

    /// CJK:全部走字符偏移,不在多字节序列中间切。
    #[test]
    fn multi_byte_offsets_are_char_based() {
        let (out, sel) = bold("你好世界", 1..3);
        assert_eq!(out, "你**好世**界");
        assert_eq!(sel, 3..5);

        let (out, sel) = apply(FormatAction::InlineCode, "中文 `代码` 混排", 4..6);
        assert_eq!(out, "中文 代码 混排", "选区被同标记紧贴包裹 → toggle off");
        assert_eq!(sel, 3..5, "去包裹后选区仍是那两个字");
    }

    /// 标题:作用于选区覆盖的所有整行;已是该前缀 → 去掉;是别的 → 替换。
    #[test]
    fn heading_applies_to_every_selected_line() {
        let (out, _) = apply(FormatAction::H2, "甲乙\n丙丁", 0..0);
        assert_eq!(out, "## 甲乙\n丙丁", "纯光标只作用于当前行");

        let (out, sel) = apply(FormatAction::H2, "甲乙\n丙丁", 1..4);
        assert_eq!(out, "## 甲乙\n## 丙丁", "跨行选区覆盖两行整行");
        assert_eq!(sel, 0..out.chars().count(), "新选区覆盖改动过的整段");

        let (out, _) = apply(FormatAction::H2, "## 甲乙", 0..0);
        assert_eq!(out, "甲乙", "已是 H2 → 去掉");

        let (out, _) = apply(FormatAction::H1, "## 甲乙", 0..0);
        assert_eq!(out, "# 甲乙", "H2 → H1 是替换不是叠加");

        let (out, _) = apply(FormatAction::Plain, "### 甲乙", 0..0);
        assert_eq!(out, "甲乙", "正文一律去前缀");
    }

    /// 无序列表点有序 → 换成 `1. `,不叠加成 `- 1. `。
    #[test]
    fn list_prefix_replaces_rather_than_stacks() {
        let (out, _) = apply(FormatAction::Ordered, "- 甲\n- 乙", 0..0);
        assert_eq!(out, "1. 甲\n- 乙", "只作用于光标所在行");

        let (out, _) = apply(FormatAction::Bullet, "1. 甲", 0..0);
        assert_eq!(out, "- 甲");
    }

    /// 任务三态:`- [ ] ` → `- [x] ` → `- ` → `- [ ] `。
    #[test]
    fn task_cycles_through_three_states() {
        let states = ["- [ ] 甲", "- [x] 甲", "- 甲"];
        let mut text = "- 甲".to_owned();
        for expected in states.iter().cycle().take(6) {
            let (out, sel) = apply(FormatAction::Task, &text, 0..0);
            assert_eq!(&out, expected, "从 {text:?} 推进一档");
            text = out;
            let _ = sel;
        }
    }

    /// 引用:toggle 语义同其它行前缀。
    #[test]
    fn quote_toggles() {
        let (out, _) = apply(FormatAction::Quote, "甲", 0..1);
        assert_eq!(out, "> 甲");
        let (out, _) = apply(FormatAction::Quote, "> 甲", 0..3);
        assert_eq!(out, "甲");
    }

    /// 插入类:代码块包住选区;分割线与表格插在当前行下方。
    #[test]
    fn inserts_land_below_the_current_line() {
        let (out, sel) = apply(FormatAction::CodeBlock, "甲\n乙丙", 0..0);
        assert_eq!(out, "甲\n\n```\n\n```\n乙丙");
        let body = &out[sel.clone()];
        assert_eq!(body, "", "代码块 info 位为空且无选区内容");

        let (out, _) = apply(FormatAction::CodeBlock, "SELECT 1", 0..8);
        assert!(out.contains("```\nSELECT 1\n```\n"), "{out}");

        let (out, _) = apply(FormatAction::Divider, "甲", 0..0);
        assert_eq!(out, "甲\n\n---\n");

        let (out, _) = apply(FormatAction::Table, "甲", 0..0);
        assert!(out.ends_with(TABLE_SKELETON), "{out}");
    }

    /// 链接:选区当标题,新选区落在 url 占位上以便直接覆写。
    #[test]
    fn link_uses_selection_as_title_and_selects_url() {
        let (out, sel) = apply(FormatAction::Link, "甲乙", 0..2);
        assert_eq!(out, "[甲乙](https://)");
        let selected: String = out
            .chars()
            .skip(sel.start)
            .take(sel.end - sel.start)
            .collect();
        assert_eq!(selected, "https://", "url 占位被选中");

        let (out, sel) = apply(FormatAction::Link, "", 0..0);
        assert_eq!(out, "[](https://)");
        let selected: String = out
            .chars()
            .skip(sel.start)
            .take(sel.end - sel.start)
            .collect();
        assert_eq!(selected, "https://");
    }

    /// 空文本 / 越界选区不 panic,且结果自洽(防御:UI 侧选区可能来自过期快照)。
    #[test]
    fn degenerate_inputs_do_not_panic() {
        for action in FormatAction::ALL {
            let (out, sel) = apply(action, "", 0..0);
            assert!(sel.start <= sel.end, "{action:?}: {sel:?}");
            assert!(sel.end <= out.chars().count(), "{action:?}: {sel:?} 越界");

            let (out2, sel2) = apply(action, "甲乙", 99..99);
            assert!(sel2.start <= sel2.end && sel2.end <= out2.chars().count());
        }
    }

    /// 分组切片与 `ALL` 对齐:`actions()` 是 `ALL` 的**连续分区**,既不重
    /// 复也不遗漏。曾并存一张反向的 `FormatAction::group()`,但它除了重复
    /// 这张表、还两面漂移的可能之外再无用处,已删。
    #[test]
    fn group_slices_partition_all() {
        let flat: Vec<FormatAction> = FormatGroup::ALL
            .iter()
            .flat_map(|group| group.actions().iter().copied())
            .collect();
        assert_eq!(flat, FormatAction::ALL.to_vec());
    }
}
