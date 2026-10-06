//! 摘要生成的 prompt 模板与文档全文截断(纯逻辑,无 IO)。
//!
//! 菜单「AI: 生成摘要」的数据源是当前文档全文(app 侧采集),本模块只负责
//! 把全文变成 provider 可消费的 prompt:超长文档按字节预算截断并在 prompt
//! 里注明,输出要求是 3-5 条要点列表。预算默认 32KB;设置页「上下文大小」
//! (decisions-pending #110)显式配置后由 [`summary_prompt_with_budget`]
//! 接管,未配置(None)落回本默认,截断行为与旧版逐字节一致。

use crate::truncate_bytes;

/// 文档全文的字节预算(约 32KB):与 commit 的 16KB diff 预算同理,请求体
/// 上限由调用方兜底,不指望模型端截断。上下文大小未配置时的现状默认。
const SUMMARY_BUDGET_BYTES: usize = 32 * 1024;

/// 对模型的输出要求。单行(续行符拼接),行首不以 `- `/`#` 开头,不与
/// [`crate::MockProvider::mock_summary`] 的行扫描规则撞车;MockProvider
/// 靠本常量作前缀识别摘要请求(见 `stream_complete` 的分支)。
pub(crate) const SUMMARY_INSTRUCTIONS: &str = "你是文档摘要助手。请提炼下方文档的要点,要求:\
只输出 3-5 条要点,每条一行中文,以 '- ' 开头;不要输出标题、解释或代码。";

/// 按字节预算截断文档全文,不拆 UTF-8 字符(边界回退到字符起点)。
/// 返回 (截断后的文本, 是否发生了截断)。
pub fn truncate_document(text: &str) -> (String, bool) {
    truncate_document_with_budget(text, SUMMARY_BUDGET_BYTES)
}

/// [`truncate_document`] 的任意预算版(上下文大小配置驱动,
/// decisions-pending #110)。budget 以 KB 为粒度(1024 的倍数)时截断说明
/// 里的「约 N KB」才与真实预算一致;字节口径是 UTF-8 的精确物理量,任何
/// 预算下都不拆字符。
fn truncate_document_with_budget(text: &str, budget: usize) -> (String, bool) {
    truncate_bytes(text, budget)
}

/// 把文档全文拼成完整 prompt:输出要求 + (截断说明)+ 全文原文。
/// 预算取未配置默认(`SUMMARY_BUDGET_BYTES`,约 32KB)。
pub fn summary_prompt(text: &str) -> String {
    summary_prompt_with_budget(text, None)
}

/// [`summary_prompt`] 的配置驱动版:`budget` 是进入 prompt 的文档字节上限
/// (app 侧「上下文大小」配置换算,见 `AiConfig::context_budget`);
/// `None` = 未配置,落回 `SUMMARY_BUDGET_BYTES`(约 32KB)—— 否决线:
/// 未配置时 prompt 组装与旧版逐字节一致。
pub fn summary_prompt_with_budget(text: &str, budget: Option<usize>) -> String {
    let budget = budget.unwrap_or(SUMMARY_BUDGET_BYTES);
    let (text, truncated) = truncate_document_with_budget(text, budget);
    let note = if truncated {
        format!("\n(文档超长,已截断至约 {}KB)\n", budget / 1024)
    } else {
        String::new()
    };
    format!("{SUMMARY_INSTRUCTIONS}{note}\n{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_document_under_budget_is_identity() {
        for case in ["", "short doc", "汉".repeat(100).as_str()] {
            let (kept, truncated) = truncate_document(case);
            assert_eq!(kept, case);
            assert!(!truncated);
        }
    }

    /// 超预算截断:ASCII 恰好落在预算上;CJK 字节边界回退,产出仍是不多于
    /// 预算的合法 UTF-8(按字符构造,`String` 本身即证明边界没拆错)。
    #[test]
    fn truncate_document_caps_at_byte_budget_without_splitting_chars() {
        let (kept, truncated) = truncate_document(&"x".repeat(SUMMARY_BUDGET_BYTES + 1));
        assert!(truncated);
        assert_eq!(kept.len(), SUMMARY_BUDGET_BYTES);

        // 32768 不是 3 的倍数(余 2):边界回退两个字节,保留 10922 个整字
        let (kept, truncated) = truncate_document(&"汉".repeat(12000));
        assert!(truncated);
        assert_eq!(kept.len(), SUMMARY_BUDGET_BYTES - 2);
        assert_eq!(kept.chars().count(), 10922);
    }

    /// prompt 构造:短文档原文嵌入且无截断说明;超长文档带截断说明、尾部
    /// 内容不出现;两种情况都带要点输出要求,且指令头作前缀稳定可识别。
    #[test]
    fn prompt_embeds_document_and_notes_truncation() {
        let short = "# 标题\n\n正文一段。\n";
        let prompt = summary_prompt(short);
        assert!(prompt.starts_with(SUMMARY_INSTRUCTIONS), "指令头在最前");
        assert!(prompt.contains(short), "文档原文嵌入");
        assert!(prompt.contains("3-5 条要点"), "{prompt}");
        assert!(!prompt.contains("已截断"));

        let long_tail = "尾部哨兵";
        let long = format!("{}\n{}", "行".repeat(SUMMARY_BUDGET_BYTES / 3), long_tail);
        let prompt = summary_prompt(&long);
        assert!(prompt.starts_with(SUMMARY_INSTRUCTIONS));
        assert!(prompt.contains("已截断至约 32KB"), "{prompt}");
        assert!(!prompt.contains(long_tail), "预算外内容不得混入");
    }

    /// 否决线(decisions-pending #110):未配置(None)与显式现状默认
    /// (Some(32KB))下,配置驱动组装与旧版 `summary_prompt` 逐字节一致。
    #[test]
    fn prompt_with_budget_none_or_default_matches_legacy() {
        let long = "汉".repeat(SUMMARY_BUDGET_BYTES / 3 + 100);
        for text in ["", "# 标题\n\n正文一段。\n", long.as_str()] {
            assert_eq!(summary_prompt(text), summary_prompt_with_budget(text, None));
            assert_eq!(
                summary_prompt(text),
                summary_prompt_with_budget(text, Some(SUMMARY_BUDGET_BYTES))
            );
        }
    }

    /// 任意预算下的截断 char 级安全(不截半字符):ASCII / CJK / emoji /
    /// 空串 / 恰好等于预算 / 超预算,产出要么原文、要么是原文的 char 对齐
    /// 前缀且字节数不多于预算。emoji 按 4 字节算,验证边界回退落在整字符。
    #[test]
    fn truncate_with_budget_is_char_safe_across_scripts() {
        let cases: Vec<(String, usize)> = vec![
            (String::new(), 1024),
            ("abcdefgh".to_owned(), 4),
            ("abcdefgh".to_owned(), 8),
            ("汉".repeat(100), 300),
            ("汉".repeat(100), 302),  // 302 不是 3 的倍数:边界回退
            ("🦀".repeat(50), 7),     // 4 字节/字:7 回退到 4,留 1 只
            ("a汉🦀b".to_owned(), 5), // 1+3+4+1:5 恰在 🦀 中段,回退到 4
        ];
        for (text, budget) in cases {
            let (kept, truncated) = truncate_document_with_budget(&text, budget);
            assert!(
                text.starts_with(kept.as_str()),
                "产出必须是原文前缀:{kept:?}"
            );
            if truncated {
                assert!(kept.len() <= budget, "截断产出不得超出预算");
                assert!(kept.is_char_boundary(kept.len()));
                assert!(text.is_char_boundary(kept.len()), "边界落在整字符上");
            } else {
                assert_eq!(kept, text, "未截断 = 原文");
            }
        }
    }

    /// 配置驱动:显式预算小于现状默认时按新预算截断,prompt 里的截断说明
    /// 报真实预算(「约 4KB」),预算外内容不混入。
    #[test]
    fn prompt_custom_budget_truncates_and_notes_real_kb() {
        let long_tail = "尾部哨兵";
        let long = format!("{}\n{}", "字".repeat(4000), long_tail); // 12KB+
        let prompt = summary_prompt_with_budget(&long, Some(4096));
        assert!(prompt.contains("已截断至约 4KB"), "{prompt}");
        assert!(!prompt.contains(long_tail), "预算外内容不得混入");
        // 注记与指令之外,正文恰为 4KB 内的整字符(4096 % 3 = 1,回退一字节)
        let body = prompt
            .strip_prefix(SUMMARY_INSTRUCTIONS)
            .unwrap()
            .trim_start_matches('\n')
            .strip_prefix("(文档超长,已截断至约 4KB)")
            .unwrap()
            .trim_start_matches('\n');
        assert_eq!(body.len(), 4095);
    }
}
