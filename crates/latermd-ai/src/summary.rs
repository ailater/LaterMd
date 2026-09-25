//! 摘要生成的 prompt 模板与文档全文截断(纯逻辑,无 IO)。
//!
//! 菜单「AI: 生成摘要」的数据源是当前文档全文(app 侧采集),本模块只负责
//! 把全文变成 provider 可消费的 prompt:超长文档按字节预算截断并在 prompt
//! 里注明,输出要求是 3-5 条要点列表。

use crate::truncate_bytes;

/// 文档全文的字节预算(约 32KB):与 commit 的 16KB diff 预算同理,请求体
/// 上限由调用方兜底,不指望模型端截断。
const SUMMARY_BUDGET_BYTES: usize = 32 * 1024;

/// 对模型的输出要求。单行(续行符拼接),行首不以 `- `/`#` 开头,不与
/// [`crate::MockProvider::mock_summary`] 的行扫描规则撞车;MockProvider
/// 靠本常量作前缀识别摘要请求(见 `stream_complete` 的分支)。
pub(crate) const SUMMARY_INSTRUCTIONS: &str = "你是文档摘要助手。请提炼下方文档的要点,要求:\
只输出 3-5 条要点,每条一行中文,以 '- ' 开头;不要输出标题、解释或代码。";

/// 按字节预算截断文档全文,不拆 UTF-8 字符(边界回退到字符起点)。
/// 返回 (截断后的文本, 是否发生了截断)。
pub fn truncate_document(text: &str) -> (String, bool) {
    truncate_bytes(text, SUMMARY_BUDGET_BYTES)
}

/// 把文档全文拼成完整 prompt:输出要求 + (截断说明)+ 全文原文。
pub fn summary_prompt(text: &str) -> String {
    let (text, truncated) = truncate_document(text);
    let note = if truncated {
        "\n(文档超长,已截断至约 32KB)\n"
    } else {
        ""
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
}
