//! #53 M2 双栏 diff 的纯配对层:M1 结构化 diff(`latermd_git::FileDiff`)
//! → GitHub 式对齐行对。
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
    },
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
                );
                continue;
            }
            // 连续非上下文块:收集删除/新增两条子序列,下标两两配对
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
            for offset in 0..olds.len().max(news.len()) {
                push_pair(
                    &mut rows,
                    &mut pairs,
                    &mut dropped_pairs,
                    olds.get(offset).copied(),
                    news.get(offset).copied(),
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
fn push_pair<'a>(
    rows: &mut Vec<SplitRow<'a>>,
    pairs: &mut usize,
    dropped_pairs: &mut usize,
    old: Option<&'a DiffLine>,
    new: Option<&'a DiffLine>,
) {
    if *pairs < MAX_SPLIT_PAIRS {
        rows.push(SplitRow::Pair { old, new });
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
                SplitRow::Pair { old, new } => Some((
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
                _ => true,
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
}
