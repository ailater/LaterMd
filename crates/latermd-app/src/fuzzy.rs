//! 快速打开浮层(#24 command-palette)的模糊匹配纯函数。
//!
//! 打分与排序在本模块闭环:不依赖 egui、不触碰应用状态,浮层模块只拿
//! [`rank`] 的结果画列表。两侧各自 `str::to_lowercase()`(Unicode 语义)
//! 后按字符比较 —— CJK 无大小写,归一即原样。
//!
//! ## 评分尺度(自定,分数只用于排序,绝对值无意义)
//!
//! - 每个命中字符基础 +100([`BASE`]);
//! - 连续命中(run 的第 2 个起)每字符再 +50([`RUN`]);
//! - 边界命中(候选首位,或前一字符是 `/` 或 `_`)再 +30([`BOUNDARY`]);
//! - 靠前奖励:命中位置 `pos` 再 +`max(0, 24 - pos)`,第 25 位起不加
//!   (长路径不罚分,只是不给奖励)。
//!
//! 对齐取**全局最优**(fzy 式 O(query×candidate) 动态规划),不是单遍
//! 贪心:query 首字符在更早位置孤立出现时,贪心指针会错过后面分隔符
//! 之后的连续 run(`a_ab` 按 `ab` 查会被贪心打成 275 分的次优对齐,
//! 排序上就压不过本应更差的 `axb`)。测试钉住了加分数值,调常量需同步
//! 改断言。

/// 单个命中字符的基础分。
const BASE: i64 = 100;
/// 连续命中(run 的第 2 个起)每字符的追加分。
const RUN: i64 = 50;
/// 边界命中(候选首位或 `/`、`_` 之后)的追加分。
const BOUNDARY: i64 = 30;
/// 靠前奖励:命中位置 `pos` 得 `max(0, EARLY - pos)`。
const EARLY: i64 = 24;

/// 动态规划的不可行态哨兵;远离加分数值量级,累加不溢出。
const NEG: i64 = i64::MIN / 4;

/// 模糊打分:query 是否为 candidate(归一后)的子序列,是则返回总分,
/// 不是返回 `None`。
///
/// 空 query 约定返回 `Some(0)` —— 不筛掉任何候选;调用方(浮层)也可以
/// 在输入为空时短路、直接按原序展示,两条路等价(见 [`rank`] 对空 query
/// 的行为)。
#[cfg_attr(not(test), allow(dead_code))]
pub fn score(query: &str, candidate: &str) -> Option<i64> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let c: Vec<char> = candidate.to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    if q.len() > c.len() {
        return None;
    }
    // dp[j]:query 前 i 个字符全部命中、且第 i 个恰命中在 candidate 位置
    // j 的最大总分(滚动一行)。上一命中有两个来源:任意位置的最优
    // (best,不带连续分),或紧邻前一格(prev[j-1],带连续分)—— 前缀
    // max 只记一个 argmax,紧邻路径必须单独参赛,否则「更早的孤立高分」
    // 会压掉「紧邻 + 连续分」的总分更优路径。i > 0 时 j=0 放不下上一字符
    // (best 从不可行起),i == 0 时无上一命中(best 为 0 分)。
    let mut prev = vec![NEG; c.len()];
    for (i, qi) in q.iter().enumerate() {
        let mut cur = vec![NEG; c.len()];
        let mut best_val = if i == 0 { 0 } else { NEG };
        for (j, cj) in c.iter().enumerate() {
            if qi == cj {
                // 紧邻来源同样要判可行:NEG + RUN 仍高于哨兵,会把
                // 不可行态洗成「命中」。
                let adjacent = if j > 0 && prev[j - 1] > NEG {
                    prev[j - 1] + RUN
                } else {
                    NEG
                };
                let from = best_val.max(adjacent);
                if from > NEG {
                    cur[j] = hit_score(&c, j) + from;
                }
            }
            if prev[j] > best_val {
                best_val = prev[j];
            }
        }
        prev = cur;
    }
    let best = prev.iter().copied().max().unwrap_or(NEG);
    (best > NEG).then_some(best)
}

/// 位置 j 上一根命中字符的得分:基础 + 边界 + 靠前(尺度见模块文档)。
fn hit_score(c: &[char], j: usize) -> i64 {
    let boundary = if j == 0 || c[j - 1] == '/' || c[j - 1] == '_' {
        BOUNDARY
    } else {
        0
    };
    BASE + boundary + (EARLY - j as i64).max(0)
}

/// 按 [`score`] 给候选打分并**降序稳定排序**,返回**(候选下标, 分数)**;
/// 未命中(`None`)的候选被剔除,同分保持输入顺序(Rust `sort_by` 稳定)。
/// 空 query 时全部候选以 0 分原序返回,与 [`score`] 的空 query 约定一致。
#[cfg_attr(not(test), allow(dead_code))]
pub fn rank<T: AsRef<str>>(query: &str, candidates: &[T]) -> Vec<(usize, i64)> {
    let mut hits: Vec<(usize, i64)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, candidate)| score(query, candidate.as_ref()).map(|s| (i, s)))
        .collect();
    hits.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 命中/未命中:子序列顺序敏感;query 比候选长必不命中。
    #[test]
    fn hit_miss_and_order_sensitivity() {
        assert!(score("abc", "xaybzc").is_some());
        assert_eq!(score("ba", "ab"), None, "顺序颠倒不是子序列");
        assert_eq!(score("md", "x.txt"), None);
        assert_eq!(score("abcd", "abc"), None, "query 比候选长");
        assert_eq!(score("a", ""), None, "非空 query 打空候选");
        assert_eq!(score("", ""), Some(0));
    }

    /// 空 query 的约定:一律 `Some(0)`,rank 原序全量返回 —— 与调用方
    /// 「输入为空直接展示、跳过过滤」的短路等价。
    #[test]
    fn empty_query_hits_all_at_zero() {
        assert_eq!(score("", "任何候选.md"), Some(0));
        assert_eq!(rank("", &["b.md", "a.md"]), vec![(0, 0), (1, 0)]);
    }

    /// 大小写不敏感:两侧归一后打分一致(中文无大小写,见 cjk 用例)。
    #[test]
    fn case_insensitive() {
        assert_eq!(score("ABC", "abc-x"), score("abc", "ABC-X"));
        assert_eq!(score("ABC", "abc-x"), Some(499));
        assert_eq!(score("Md", "x.MD"), Some(293));
    }

    /// 连续命中优于分散。第二个断言钉住全局最优对齐:贪心会把 `a` 锁在
    /// 位置 0、错过 `_` 之后的连续 run(那一路只有 275 分,低于 `axb`
    /// 的 276,排序方向就错了)。
    #[test]
    fn consecutive_run_beats_scattered() {
        assert_eq!(score("ab", "ab.md"), Some(327));
        assert_eq!(score("ab", "axb.md"), Some(276));
        assert!(score("ab", "ab.md") > score("ab", "axb.md"));
        assert_eq!(score("ab", "a_ab"), Some(323));
        assert!(score("ab", "a_ab") > score("ab", "axb"));
    }

    /// 词首/路径分隔符后命中加分:`_` 与 `/` 之后的命中高于普通位置。
    #[test]
    fn boundary_bonus() {
        assert_eq!(score("m", "_m"), Some(153));
        assert!(score("m", "_m") > score("m", "zm"));
        assert!(score("f", "dir/file") > score("f", "dirXfile"));
    }

    /// 靠前命中小幅加分:同一字符越早出现分越高。
    #[test]
    fn earlier_hit_scores_higher() {
        assert_eq!(score("x", "x...."), Some(154));
        assert_eq!(score("x", "....x"), Some(120));
        assert!(score("x", "x....") > score("x", "....x"));
    }

    /// 中文子串:CJK 无大小写、原样比较,子序列与顺序规则同 ASCII。
    #[test]
    fn cjk_subsequence() {
        assert_eq!(score("中文", "我的中文文档.md"), Some(293));
        assert_eq!(score("中档", "我的中文文档.md"), Some(241));
        assert_eq!(score("文中", "我的中文"), None, "顺序颠倒不命中");
        assert_eq!(score("中档", "我的文档.md"), None);
    }

    /// rank:按分降序、未命中剔除、同分保持输入顺序(稳定性)。
    #[test]
    fn rank_desc_stable_and_filters_misses() {
        let candidates = ["axb.md", "ab.md", "ayb.md", "a_b.md", "zzz"];
        assert_eq!(
            rank("ab", &candidates),
            vec![(1, 327), (3, 306), (0, 276), (2, 276)]
        );
        // 同构候选同分,下标顺序不乱
        assert_eq!(rank("md", &["b/x.md", "a/y.md"]), vec![(0, 289), (1, 289)]);
        // String 候选(AsRef<str>)同样可用
        let owned: Vec<String> = ["readme.md", "logo.png"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            rank("md", &owned),
            vec![(0, score("md", "readme.md").unwrap())]
        );
    }
}
