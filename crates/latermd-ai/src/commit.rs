//! commit message 生成的 prompt 模板与 diff 截断(纯逻辑,无 IO)。
//!
//! 菜单「AI: 生成 commit message」的数据源是 git diff(app 侧采集),本模块
//! 只负责把 diff 变成 provider 可消费的 prompt:超长 diff 按字节预算截断
//! 并在 prompt 里注明,输出要求是一行 conventional 中文 subject。预算默认
//! 16KB;设置页「上下文大小」(decisions-pending #110)显式配置后由
//! [`commit_message_prompt_with_budget`] 接管,未配置(None)落回本默认。

use crate::truncate_bytes;

/// diff 的字节预算(约 16KB):请求体上限由调用方兜底,不指望模型端截断。
/// 上下文大小未配置时的现状默认。
const DIFF_BUDGET_BYTES: usize = 16 * 1024;

/// 对模型的输出要求。措辞避开 [`crate::MockProvider::mock_commit_subject`]
/// 的关键词规则会命中的串(`new file mode`、`README`、`Cargo.toml` 等),
/// 指令文本才不会干扰按 diff 关键词的合成。
const INSTRUCTIONS: &str = "你是提交信息助手。请根据下方的 git diff 写一条 commit message,要求:\
只输出一行中文 subject,以 conventional commits 前缀(feat / fix / docs / chore)开头;\
不要输出正文、代码块或任何解释。";

/// 按字节预算截断 diff,不拆 UTF-8 字符(边界回退到字符起点)。
/// 返回 (截断后的文本, 是否发生了截断)。
pub fn truncate_diff(diff: &str) -> (String, bool) {
    truncate_diff_with_budget(diff, DIFF_BUDGET_BYTES)
}

/// [`truncate_diff`] 的任意预算版(上下文大小配置驱动,decisions-pending
/// #110)。budget 以 KB 为粒度(1024 的倍数)时截断说明里的「约 N KB」
/// 才与真实预算一致。
fn truncate_diff_with_budget(diff: &str, budget: usize) -> (String, bool) {
    truncate_bytes(diff, budget)
}

/// 把 git diff 拼成完整 prompt:输出要求 + (截断说明)+ diff 原文。
/// 预算取未配置默认(`DIFF_BUDGET_BYTES`,约 16KB)。
pub fn commit_message_prompt(diff: &str) -> String {
    commit_message_prompt_with_budget(diff, None)
}

/// [`commit_message_prompt`] 的配置驱动版:`budget` 是进入 prompt 的 diff
/// 字节上限(app 侧「上下文大小」配置换算,见 `AiConfig::context_budget`);
/// `None` = 未配置,落回 `DIFF_BUDGET_BYTES`(约 16KB)—— 否决线:未配置
/// 时 prompt 组装与旧版逐字节一致。
pub fn commit_message_prompt_with_budget(diff: &str, budget: Option<usize>) -> String {
    let budget = budget.unwrap_or(DIFF_BUDGET_BYTES);
    let (diff, truncated) = truncate_diff_with_budget(diff, budget);
    let note = if truncated {
        format!("\n(diff 超长,已截断至约 {}KB)\n", budget / 1024)
    } else {
        String::new()
    };
    format!("{INSTRUCTIONS}{note}\n{diff}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_diff_under_budget_is_identity() {
        for case in ["", "short diff", "汉".repeat(100).as_str()] {
            let (kept, truncated) = truncate_diff(case);
            assert_eq!(kept, case);
            assert!(!truncated);
        }
    }

    /// 超预算截断:ASCII 恰好落在预算上;CJK 字节边界回退,产出仍是不多于
    /// 预算的合法 UTF-8(按字符构造,`String` 本身即证明边界没拆错)。
    #[test]
    fn truncate_diff_caps_at_byte_budget_without_splitting_chars() {
        let (kept, truncated) = truncate_diff(&"x".repeat(DIFF_BUDGET_BYTES + 1));
        assert!(truncated);
        assert_eq!(kept.len(), DIFF_BUDGET_BYTES);

        // 16384 不是 3 的倍数:边界回退一个字节,保留 5461 个整字
        let (kept, truncated) = truncate_diff(&"汉".repeat(6000));
        assert!(truncated);
        assert_eq!(kept.len(), DIFF_BUDGET_BYTES - 1);
        assert_eq!(kept.chars().count(), 5461);
    }

    /// prompt 构造:短 diff 原文嵌入且无截断说明;超长 diff 带截断说明、
    /// 尾部内容不出现;两种情况都带 conventional 输出要求。
    #[test]
    fn prompt_embeds_diff_and_notes_truncation() {
        let short = "diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n";
        let prompt = commit_message_prompt(short);
        assert!(prompt.contains(short), "diff 原文嵌入");
        assert!(prompt.contains("conventional commits"), "{prompt}");
        assert!(!prompt.contains("已截断"));

        let long_tail = "尾部哨兵";
        let long = format!("{}\n{}", "行".repeat(DIFF_BUDGET_BYTES / 3), long_tail);
        let prompt = commit_message_prompt(&long);
        assert!(prompt.contains("已截断至约 16KB"), "{prompt}");
        assert!(!prompt.contains(long_tail), "预算外内容不得混入");
    }

    /// 否决线(decisions-pending #110):未配置(None)与显式现状默认
    /// (Some(16KB))下,配置驱动组装与旧版 `commit_message_prompt`
    /// 逐字节一致。
    #[test]
    fn prompt_with_budget_none_or_default_matches_legacy() {
        let long = "汉".repeat(DIFF_BUDGET_BYTES / 3 + 100);
        for diff in ["", "diff --git a/a.md b/a.md\n", long.as_str()] {
            assert_eq!(
                commit_message_prompt(diff),
                commit_message_prompt_with_budget(diff, None)
            );
            assert_eq!(
                commit_message_prompt(diff),
                commit_message_prompt_with_budget(diff, Some(DIFF_BUDGET_BYTES))
            );
        }
    }

    /// 配置驱动:显式预算按新值截断且说明报真实预算;任意预算产出是 char
    /// 对齐前缀(emoji 4 字节边界回退不拆半字符)。
    #[test]
    fn prompt_custom_budget_truncates_and_stays_char_safe() {
        let long_tail = "尾部哨兵";
        let long = format!("{}\n{}", "字".repeat(2000), long_tail); // 6KB+
        let prompt = commit_message_prompt_with_budget(&long, Some(2048));
        assert!(prompt.contains("已截断至约 2KB"), "{prompt}");
        assert!(!prompt.contains(long_tail), "预算外内容不得混入");

        // char 边界:2048 % 3 = 2,回退到 2046 字节 = 682 个整汉字
        let (kept, truncated) = truncate_diff_with_budget(&"汉".repeat(1000), 2048);
        assert!(truncated);
        assert_eq!(kept.chars().count(), 682);
        assert!(kept.len() <= 2048);

        let (kept, truncated) = truncate_diff_with_budget(&"🦀".repeat(10), 7);
        assert!(truncated);
        assert_eq!(kept, "🦀", "7 不是 4 的倍数:边界回退留 1 只整蟹");
    }
}
