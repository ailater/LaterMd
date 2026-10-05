//! #53 M2 双栏 diff 的纯配对层:M1 结构化 diff(`latermd_git::FileDiff`)
//! → GitHub 式对齐行对;M3 追加行内词级高亮区段(公共前后缀裁剪)。
//!
//! 只做数据变换,不依赖 egui(渲染在 `ui::sidebar`,本模块可无头单测)。
//! 配对口径:hunk 内**上下文行左右同现**(同一行,两侧各自取行号),
//! 连续的非上下文块(删除/新增)按出现顺序**下标两两配对**——第 j 条
//! 删除与第 j 条新增同一行(修改行的左右并置),多出的一侧单边占行
//! (纯删左单边、纯增右单边)。hunk 头(`@@`)跨双栏,作为独立行保留。
//!
//! 行对上限 [`MAX_SPLIT_PAIRS`] 是渲染闸(M1 的 ~64KB 字节预算把行数
//! 压在 ~32k 以内,2000 行对再钉一道「逐行布局不卡帧」的量级,与文件
//! 树/搜索 500 条的截断先例同精神),超限丢弃尾部行对并计数,由渲染层
//! 给显式提示行——被截断的行对数进 [`SplitRows::dropped_pairs`]。
//!
//! M3 行内区段([`inline_change`])的降级口径:只在「**单删+单增**」的
//! 紧邻块上计算(块内恰一删一增,配对无歧义);一对多/多对多的块配对
//! 本身是下标启发式,行内区段会放大误导,保持 M2 行级效果。取舍见
//! docs/decisions-pending #101。

use std::ops::Range;

use latermd_git::{DiffHunk, DiffLine, DiffLineKind, FileDiff};

/// 双栏视图一次渲染的行对上限(不含 hunk 头)。2000 行对 ≈ 每帧至多
/// 4000 个单元格,配合视口裁剪是「大 diff 不卡帧」的量级;值取任务书
/// 建议档,取舍见 docs/decisions-pending #100。
pub const MAX_SPLIT_PAIRS: usize = 2000;

/// 双栏视图的一行:hunk 头跨双栏整行,其余为左右对齐的行对。
#[derive(PartialEq, Eq, Debug)]
pub enum SplitRow<'a> {
    /// hunk 头(`@@ -a,b +c,d @@`,文本由渲染层从 [`DiffHunk`] 拼出):
    /// 跨双栏渲染,不参与左右配对。
    Header(&'a DiffHunk),
    /// 对齐行对:上下文行两侧同现(同一 [`DiffLine`],行号各自取),
    /// 删除行左单边(`new` 为 `None`),新增行右单边(`old` 为 `None`),
    /// 修改 = 同一对里左删右增。
    Pair {
        /// 旧侧(左栏):上下文行与删除行有值。
        old: Option<&'a DiffLine>,
        /// 新侧(右栏):上下文行与新增行有值。
        new: Option<&'a DiffLine>,
        /// 行内词级高亮区段(#53 M3):仅「单删+单增」紧邻块计算(见
        /// [`inline_change`]),其余配对形态为 `None`(降级回 M2 行级)。
        inline: Option<InlineChange>,
    },
}

/// 一对行内词级高亮区段(#53 M3):公共前缀/后缀裁剪后,删/增行各自
/// 真正变化的 **char 半开区间**(相对各自行文本,从头数)。空区间 =
/// 该侧无变化(纯追加的旧侧 / 纯删除的新侧)。
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InlineChange {
    /// 旧侧(删除行)变化区段。
    pub old: Range<usize>,
    /// 新侧(新增行)变化区段。
    pub new: Range<usize>,
}

/// 对一对(删除行, 新增行)文本做公共前缀/后缀裁剪,返回两侧真正变化的
/// char 区段(#53 M3 的区段计算纯函数)。
///
/// 口径:前缀优先——公共前缀先算满,公共后缀在不与前缀重叠的前提下取满
/// (即 `prefix + suffix ≤ min(两侧 char 数)`)。因此整行重写(无公共
/// 前后缀)得全区段、仅增尾字符得空旧区段 + 尾部新区段。全程 char 级
/// 比较,绝不 byte 切(CJK/emoji 不会截半;ZWJ 序列按 char 记,极端输入
/// 可能把序列中段划进区段,文本仍由完整 char 组成,见测试)。
pub fn inline_change(old: &str, new: &str) -> InlineChange {
    let old_len = old.chars().count();
    let new_len = new.chars().count();
    let mut prefix = 0usize;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        prefix += 1;
    }
    // suffix 不许越过 prefix(否则纯追加会被误算成公共后缀)
    let mut old_rev = old.chars().rev();
    let mut new_rev = new.chars().rev();
    let mut suffix = 0usize;
    while suffix < old_len - prefix && suffix < new_len - prefix {
        match (old_rev.next(), new_rev.next()) {
            (Some(a), Some(b)) if a == b => suffix += 1,
            _ => break,
        }
    }
    InlineChange {
        old: prefix..old_len - suffix,
        new: prefix..new_len - suffix,
    }
}

/// 按区段把行文本切成 (前缀, 变化区段, 后缀) 三段引用(渲染层构造分段
/// LayoutJob 用)。区段边界由 [`inline_change`] 产出,天然在 char 边界;
/// 越界输入按钳制处理(防御,不 panic)。
pub fn inline_segments<'a>(text: &'a str, range: &Range<usize>) -> (&'a str, &'a str, &'a str) {
    let len = text.chars().count();
    let start = range.start.min(len);
    let end = range.end.min(len).max(start);
    let to_byte = |chars: usize| {
        text.char_indices()
            .nth(chars)
            .map(|(byte, _)| byte)
            .unwrap_or(text.len())
    };
    let b0 = to_byte(start);
    let b1 = to_byte(end);
    (&text[..b0], &text[b0..b1], &text[b1..])
}

/// [`split_rows`] 的返回:行序列(hunk 头与行对交错)+ 截断计数。
#[derive(PartialEq, Eq, Debug)]
pub struct SplitRows<'a> {
    /// 行序列;空 diff 无行。
    pub rows: Vec<SplitRow<'a>>,
    /// 超出 [`MAX_SPLIT_PAIRS`] 被丢弃的行对数(不含 hunk 头);`0` = 完整。
    pub dropped_pairs: usize,
}

/// M1 结构化 diff → 双栏行序列(纯函数,无 UI)。
///
/// 空 diff(`hunks` 为空)返回空序列;二进制占位与「无改动」文案由
/// 渲染层按 `FileDiff::binary`/空 hunks 判定,不在这里造行。
pub fn split_rows(file: &FileDiff) -> SplitRows<'_> {
    let mut rows: Vec<SplitRow<'_>> = Vec::new();
    let mut pairs = 0usize;
    let mut dropped_pairs = 0usize;
    for hunk in &file.hunks {
        // 到上限后 hunk 头也不进(整段尾部丢弃,渲染层提示补位)
        if pairs < MAX_SPLIT_PAIRS {
            rows.push(SplitRow::Header(hunk));
        }
        let lines = &hunk.lines;
        let mut index = 0;
        while index < lines.len() {
            if lines[index].kind == DiffLineKind::Context {
                let line = &lines[index];
                index += 1;
                push_pair(
                    &mut rows,
                    &mut pairs,
                    &mut dropped_pairs,
                    Some(line),
                    Some(line),
                    None,
                );
                continue;
            }
            // 连续非上下文块:收集删除/新增两条子序列,下标两两配对。
            // M3:恰「一删一增」的块是唯一无歧义的替换形态,行内词级
            // 区段只在这类块上计算(见模块文档的降级口径)。
            let mut olds: Vec<&DiffLine> = Vec::new();
            let mut news: Vec<&DiffLine> = Vec::new();
            while index < lines.len() && lines[index].kind != DiffLineKind::Context {
                let line = &lines[index];
                match line.kind {
                    DiffLineKind::Deleted => olds.push(line),
                    DiffLineKind::Added => news.push(line),
                    DiffLineKind::Context => unreachable!("外层 while 已排除"),
                }
                index += 1;
            }
            let inline = match (olds.first(), news.first()) {
                (Some(old), Some(new)) if olds.len() == 1 && news.len() == 1 => {
                    let change = inline_change(&old.text, &new.text);
                    // 文本相等(diff 不产出,防御)或两侧区段皆空(即文本
                    // 相等)时不高亮,免得渲染层做无效分段布局
                    (!change.old.is_empty() || !change.new.is_empty()).then_some(change)
                }
                _ => None,
            };
            for offset in 0..olds.len().max(news.len()) {
                push_pair(
                    &mut rows,
                    &mut pairs,
                    &mut dropped_pairs,
                    olds.get(offset).copied(),
                    news.get(offset).copied(),
                    // Clone:1+1 块只循环一次,这里按多对块统一处理
                    inline.clone(),
                );
            }
        }
    }
    SplitRows {
        rows,
        dropped_pairs,
    }
}

/// 压入一个行对,守 [`MAX_SPLIT_PAIRS`] 上限:未超限进序列,超限只计数。
/// `inline` 是该对的行内词级区段(#53 M3,配对形态不适用时 `None`)。
fn push_pair<'a>(
    rows: &mut Vec<SplitRow<'a>>,
    pairs: &mut usize,
    dropped_pairs: &mut usize,
    old: Option<&'a DiffLine>,
    new: Option<&'a DiffLine>,
    inline: Option<InlineChange>,
) {
    if *pairs < MAX_SPLIT_PAIRS {
        rows.push(SplitRow::Pair { old, new, inline });
        *pairs += 1;
    } else {
        *dropped_pairs += 1;
    }
}

/// 双侧行号文本(渲染层行号列用):`Some(n)` → 十进制串,`None` → 空串
/// (单边行的空侧)。纯函数便于无头断言行号正确性。
pub fn line_no_text(lineno: Option<u32>) -> String {
    lineno.map(|n| n.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行对判定(hunk 头不算)——行对上限只数这一类。
    fn is_pair(row: &SplitRow<'_>) -> bool {
        matches!(row, SplitRow::Pair { .. })
    }

    /// 测试用行构造(纯数据 fixture,不起 git 仓库)。
    fn line(old: Option<u32>, new: Option<u32>, kind: DiffLineKind, text: &str) -> DiffLine {
        DiffLine {
            old_lineno: old,
            new_lineno: new,
            kind,
            text: text.to_owned(),
        }
    }

    fn hunk(lines: Vec<DiffLine>) -> DiffHunk {
        DiffHunk {
            old_start: 1,
            old_lines: lines
                .iter()
                .filter(|l| l.kind != DiffLineKind::Added)
                .count() as u32,
            new_start: 1,
            new_lines: lines
                .iter()
                .filter(|l| l.kind != DiffLineKind::Deleted)
                .count() as u32,
            lines,
        }
    }

    /// 摘要:行对 → (旧侧行号, 旧侧文本, 新侧行号, 新侧文本)。
    fn pairs_summary<'a>(
        rows: &'a [SplitRow<'a>],
    ) -> Vec<(Option<u32>, &'a str, Option<u32>, &'a str)> {
        rows.iter()
            .filter_map(|row| match row {
                SplitRow::Pair { old, new, .. } => Some((
                    old.and_then(|l| l.old_lineno),
                    old.map(|l| l.text.as_str()).unwrap_or(""),
                    new.and_then(|l| l.new_lineno),
                    new.map(|l| l.text.as_str()).unwrap_or(""),
                )),
                SplitRow::Header(_) => None,
            })
            .collect()
    }

    /// 纯新增文件(全部 Added):hunk 头 + 每行右单边,新侧行号连续。
    #[test]
    fn added_file_pairs_all_right_side() {
        let file = FileDiff {
            hunks: vec![DiffHunk {
                old_start: 0,
                old_lines: 0,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    line(None, Some(1), DiffLineKind::Added, "新1"),
                    line(None, Some(2), DiffLineKind::Added, "新2"),
                ],
            }],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        assert_eq!(got.dropped_pairs, 0);
        assert_eq!(pairs_summary(&got.rows).len(), 2);
        assert!(
            got.rows.iter().all(|row| match row {
                SplitRow::Pair { old, .. } => old.is_none(),
                SplitRow::Header(_) => true,
            }),
            "纯增行左单边为空:{:?}",
            got.rows
        );
        assert_eq!(
            pairs_summary(&got.rows),
            vec![(None, "", Some(1), "新1"), (None, "", Some(2), "新2")],
            "新侧行号从 1 连续"
        );
    }

    /// 纯删行:删除行左单边,上下文行左右同现同一文本。
    #[test]
    fn deleted_lines_pair_left_side() {
        let file = FileDiff {
            hunks: vec![hunk(vec![
                line(Some(1), Some(1), DiffLineKind::Context, "一"),
                line(Some(2), None, DiffLineKind::Deleted, "二"),
                line(Some(3), Some(2), DiffLineKind::Context, "三"),
            ])],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        assert_eq!(
            pairs_summary(&got.rows),
            vec![
                (Some(1), "一", Some(1), "一"),
                (Some(2), "二", None, ""),
                (Some(3), "三", Some(2), "三"),
            ],
            "上下文左右同现、删除左单边:{:?}",
            got.rows
        );
    }

    /// 混合 hunk:修改行(一删一增)同对并置;不对称块(两删一增)多出的
    /// 删除行左单边;块与上下文交替时顺序保持 diff 输出顺序。
    #[test]
    fn mixed_hunk_pairs_replacement_and_asymmetric_blocks() {
        let file = FileDiff {
            hunks: vec![hunk(vec![
                line(Some(1), Some(1), DiffLineKind::Context, "上下文"),
                line(Some(2), None, DiffLineKind::Deleted, "旧A"),
                line(None, Some(2), DiffLineKind::Added, "新A"),
                line(Some(3), None, DiffLineKind::Deleted, "旧B"),
                line(Some(4), None, DiffLineKind::Deleted, "旧C"),
                line(None, Some(3), DiffLineKind::Added, "新B"),
                line(Some(5), Some(4), DiffLineKind::Context, "尾部"),
            ])],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        assert_eq!(
            pairs_summary(&got.rows),
            vec![
                (Some(1), "上下文", Some(1), "上下文"),
                (Some(2), "旧A", Some(2), "新A"),
                (Some(3), "旧B", Some(3), "新B"),
                (Some(4), "旧C", None, ""),
                (Some(5), "尾部", Some(4), "尾部"),
            ],
            "同块删除/新增按下标配对,多出侧单边:{:?}",
            got.rows
        );
        // hunk 头也在序列里,且是首行
        assert!(matches!(got.rows[0], SplitRow::Header(_)));
        assert_eq!(got.rows.iter().filter(|r| is_pair(r)).count(), 5);
    }

    /// CJK 与 emoji(ZWJ 序列)行文本逐字符透传:配对层不复制不截断,
    /// 引用原行,字符边界天然完整。
    #[test]
    fn cjk_and_emoji_lines_pass_through_verbatim() {
        let emoji = "👨‍👩‍👧 家庭 🚀";
        let file = FileDiff {
            hunks: vec![hunk(vec![
                line(Some(1), Some(1), DiffLineKind::Context, "中文 上下文"),
                line(Some(2), None, DiffLineKind::Deleted, emoji),
                line(None, Some(2), DiffLineKind::Added, "改为:中文 🚀 新行"),
            ])],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        let summary = pairs_summary(&got.rows);
        assert_eq!(summary.len(), 2, "上下文一对 + 删除/新增同块配成一对");
        // 第 0 对:上下文左右同现
        assert_eq!(summary[0], (Some(1), "中文 上下文", Some(1), "中文 上下文"));
        // 第 1 对:左删右增同一行,emoji 逐字符透传
        assert_eq!(summary[1].1, emoji, "旧侧 ZWJ 序列原样");
        assert_eq!(summary[1].1.chars().count(), emoji.chars().count());
        assert_eq!(
            summary[1].3.chars().filter(|c| *c == '🚀').count(),
            1,
            "新增行 emoji 完整"
        );
        assert!(
            summary[1].3.len() > summary[1].3.chars().count(),
            "多字节行未被截半"
        );
    }

    /// 空 diff(无 hunk)与 binary/truncated 标志不影响配对层:空序列、
    /// 零截断(占位文案由渲染层处理)。
    #[test]
    fn empty_diff_yields_no_rows() {
        let empty = FileDiff {
            hunks: Vec::new(),
            binary: false,
            truncated: false,
        };
        let got = split_rows(&empty);
        assert!(got.rows.is_empty());
        assert_eq!(got.dropped_pairs, 0);
    }

    /// 多 hunk:每个 hunk 头都保留,行对跨 hunk 累计守上限。
    #[test]
    fn multiple_hunks_keep_headers() {
        let file = FileDiff {
            hunks: vec![
                hunk(vec![line(Some(1), Some(1), DiffLineKind::Context, "a")]),
                hunk(vec![line(Some(9), Some(9), DiffLineKind::Context, "b")]),
            ],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        assert_eq!(got.rows.len(), 4, "2 头 + 2 对");
        assert!(matches!(got.rows[0], SplitRow::Header(_)));
        assert!(matches!(got.rows[2], SplitRow::Header(_)));
    }

    /// 超上限截断:恰好 2000 对不截断;2001+ 对截到 2000,多出的计数,
    /// 且截断点之后的 hunk 头也不进(整段尾部丢弃)。
    #[test]
    fn truncates_pairs_beyond_cap() {
        let many = |pairs: usize| FileDiff {
            hunks: vec![DiffHunk {
                old_start: 1,
                old_lines: pairs as u32,
                new_start: 1,
                new_lines: pairs as u32,
                lines: (1..=pairs as u32)
                    .map(|n| line(Some(n), Some(n), DiffLineKind::Context, &format!("行{n}")))
                    .collect(),
            }],
            binary: false,
            truncated: false,
        };

        let exact_file = many(MAX_SPLIT_PAIRS);
        let exact = split_rows(&exact_file);
        assert_eq!(exact.dropped_pairs, 0, "恰好等于上限不算截断");
        assert_eq!(
            exact.rows.iter().filter(|r| is_pair(r)).count(),
            MAX_SPLIT_PAIRS
        );

        let over_file = many(MAX_SPLIT_PAIRS + 37);
        let over = split_rows(&over_file);
        assert_eq!(over.dropped_pairs, 37);
        assert_eq!(
            over.rows.iter().filter(|r| is_pair(r)).count(),
            MAX_SPLIT_PAIRS
        );
        // 保留的是头部:首个 hunk 头在,最后一行是第 2000 对
        assert!(matches!(over.rows[0], SplitRow::Header(_)));
        let last = pairs_summary(&over.rows).pop().expect("行对非空");
        assert_eq!(
            last,
            (
                Some(MAX_SPLIT_PAIRS as u32),
                "行2000",
                Some(MAX_SPLIT_PAIRS as u32),
                "行2000"
            ),
            "保留字典序最前(即 diff 顺序最前)的 2000 对"
        );
    }

    /// 行号文本纯函数:Some → 十进制,None → 空串。
    #[test]
    fn line_no_text_formats() {
        assert_eq!(line_no_text(Some(1)), "1");
        assert_eq!(line_no_text(Some(12345)), "12345");
        assert_eq!(line_no_text(None), "");
    }

    /// #53 M3 纯 ASCII 改词:公共前缀/后缀裁剪只圈出真正变化的词。
    #[test]
    fn inline_change_ascii_word_swap() {
        let got = inline_change("the quick fox", "the quick dog");
        assert_eq!(got.old, 10..13, "旧侧只圈 fox");
        assert_eq!(got.new, 10..13, "新侧只圈 dog");
        let (prefix, mid, suffix) = inline_segments("the quick fox", &got.old);
        assert_eq!((prefix, mid, suffix), ("the quick ", "fox", ""));
    }

    /// #53 M3 CJK 改字:char 级裁剪圈出单字,byte 不截半。
    #[test]
    fn inline_change_cjk_char_swap() {
        let got = inline_change("中文内容", "中文改容");
        assert_eq!(got.old, 2..3, "旧侧只圈「内」");
        assert_eq!(got.new, 2..3, "新侧只圈「改」");
        let (_, mid, _) = inline_segments("中文内容", &got.old);
        assert_eq!(mid, "内", "区段是完整 char,不是半个字节");
    }

    /// #53 M3 emoji 替换:emoji 是完整 char,区段边界落在它两侧。
    #[test]
    fn inline_change_emoji_replacement() {
        let got = inline_change("看 🚀 去", "看 🎉 去");
        assert_eq!(got.old, 2..3);
        assert_eq!(got.new, 2..3);
        let (_, mid, _) = inline_segments("看 🚀 去", &got.old);
        assert_eq!(mid, "🚀", "被圈出的正是那个 emoji");
        // ZWJ 序列行:区段切片仍是完整 char 组成的合法 str(不 panic 不截半)
        let zwj = "家庭👨‍👩‍👧出行";
        let got = inline_change(zwj, "家庭🚀出行");
        assert_eq!(
            got.old,
            2..7,
            "旧侧圈走整个 ZWJ 序列(9 char,前 2 后 2 裁掉)"
        );
        let (_, mid, _) = inline_segments(zwj, &got.old);
        assert_eq!(mid, "👨‍👩‍👧");
        assert!(mid.chars().any(|c| c == '\u{200d}'), "ZWJ 完整保留");
    }

    /// #53 M3 无公共前后缀(但内部有相同字符):两侧都是全区段。
    #[test]
    fn inline_change_no_common_affix_is_full_range() {
        let got = inline_change("ab", "ba");
        assert_eq!(got.old, 0..2);
        assert_eq!(got.new, 0..2);
    }

    /// #53 M3 整行重写:无任何公共字符,区段铺满两侧整行。
    #[test]
    fn inline_change_full_rewrite_is_full_range() {
        let (old, new) = ("完全不同的旧内容", "崭新的另一段文本");
        let got = inline_change(old, new);
        assert_eq!(got.old, 0..old.chars().count());
        assert_eq!(got.new, 0..new.chars().count());
        // 共享尾部的改写:公共后缀参与裁剪,只圈出真正不同的首字
        let got = inline_change("旧的一行", "新的一行");
        assert_eq!(got.old, 0..1, "「旧的/新的」只有首字不同");
        assert_eq!(got.new, 0..1);
    }

    /// #53 M3 仅增尾字符:旧侧空区段、新侧圈尾部;前缀优先使纯追加/纯
    /// 删除不会被公共后缀吞掉。
    #[test]
    fn inline_change_tail_addition_and_prefix_priority() {
        let got = inline_change("abc", "abcd");
        assert_eq!(got.old, 3..3, "旧侧无变化(空区段)");
        assert_eq!(got.new, 3..4, "新侧圈住追加的 d");
        let (_, mid, _) = inline_segments("abcd", &got.new);
        assert_eq!(mid, "d");

        let got = inline_change("abcd", "abc");
        assert_eq!(got.old, 3..4, "纯删尾:旧侧圈住被删的 d");
        assert_eq!(got.new, 3..3);

        // 前缀优先:后缀不许越过前缀,否则会把纯追加错算成公共后缀
        let got = inline_change("ab", "abab");
        assert_eq!(got.old, 2..2);
        assert_eq!(got.new, 2..4, "圈住尾部追加的 ab,而不是空区段");
        let got = inline_change("abab", "ab");
        assert_eq!(got.old, 2..4);
        assert_eq!(got.new, 2..2);
    }

    /// #53 M3 文本相等的删增对(diff 不产出,防御):两侧区段皆空。
    #[test]
    fn inline_change_equal_texts_yield_empty_ranges() {
        let got = inline_change("相同", "相同");
        assert!(got.old.is_empty() && got.new.is_empty());
    }

    /// #53 M3 配对集成:「单删+单增」紧邻块才有 inline 区段,且区段与
    /// 纯函数一致;一对多块(2删1增)、单边块与上下文对一律 `None`
    /// (降级回 M2 行级,见 #101)。注意块之间必须有上下文行分隔——
    /// 连续的非上下文行是同一个块(diff 语义)。
    #[test]
    fn split_rows_marks_inline_only_for_single_delete_single_add() {
        let file = FileDiff {
            hunks: vec![DiffHunk {
                old_start: 1,
                old_lines: 6,
                new_start: 1,
                new_lines: 5,
                lines: vec![
                    line(Some(1), Some(1), DiffLineKind::Context, "上下文"),
                    line(Some(2), None, DiffLineKind::Deleted, "AAAAxxxxBBBB"),
                    line(None, Some(2), DiffLineKind::Added, "AAAAyyyyBBBB"),
                    line(Some(3), Some(3), DiffLineKind::Context, "块间分隔"),
                    line(Some(4), None, DiffLineKind::Deleted, "两删一"),
                    line(Some(5), None, DiffLineKind::Deleted, "两删二"),
                    line(None, Some(4), DiffLineKind::Added, "一增"),
                ],
            }],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        let inline_of = |row: &SplitRow<'_>| match row {
            SplitRow::Pair { inline, .. } => inline.clone(),
            SplitRow::Header(_) => None,
        };
        let rows = &got.rows;
        assert!(matches!(rows[0], SplitRow::Header(_)));
        // 上下文对:无区段
        assert_eq!(inline_of(&rows[1]), None, "上下文对不高亮");
        // 单删+单增:区段 = 纯函数结果(公共前后缀各 4 字符)
        assert_eq!(
            inline_of(&rows[2]),
            Some(InlineChange {
                old: 4..8,
                new: 4..8
            }),
            "1+1 紧邻块圈出 xxxx / yyyy"
        );
        assert_eq!(inline_of(&rows[3]), None, "块间上下文对不高亮");
        // 2删1增:两对都降级,保持 M2 行级
        assert_eq!(inline_of(&rows[4]), None, "一对多的配对是启发式,不高亮");
        assert_eq!(inline_of(&rows[5]), None, "多出的单边同样不高亮");
        // 行对结构不受 M3 影响:仍是 1 头 + 5 对,文本原样
        assert_eq!(rows.len(), 6);
        assert_eq!(pairs_summary(&got.rows).len(), 5);
    }

    /// #53 M3 纯单边块(纯增/纯删)与空 hunk:无 inline 区段,不 panic。
    #[test]
    fn split_rows_never_marks_single_sided_blocks() {
        let file = FileDiff {
            hunks: vec![hunk(vec![
                line(None, Some(1), DiffLineKind::Added, "纯增一"),
                line(None, Some(2), DiffLineKind::Added, "纯增二"),
            ])],
            binary: false,
            truncated: false,
        };
        let got = split_rows(&file);
        assert!(
            got.rows.iter().all(|row| match row {
                SplitRow::Pair { inline, .. } => inline.is_none(),
                SplitRow::Header(_) => true,
            }),
            "纯增块全部无区段:{:?}",
            got.rows
        );
    }
}
