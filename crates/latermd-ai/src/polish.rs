//! 选区润色的 prompt 模板与素材预算(纯逻辑,无 IO)。
//!
//! 「AI 润色」(#61 M3)的数据源是**选中的文本**(app 侧随消息携带),
//! 本模块把它变成 provider 可消费的 prompt。与摘要/commit 的差异:润色
//! 素材进结果(确认后整段替换选区),**截断素材 = 确认后丢内容**,所以
//! 这里只有识别用的指令头与预算判定,不提供截断 —— 素材超预算由 app 侧
//! 拒绝并落提示(decisions-pending #117),prompt 组装必然拿到完整素材。

/// 对模型的输出要求。单行(续行符拼接),语义/结构保持是硬约束;措辞
/// 避开 [`crate::MockProvider`] 续写脚本的关键词规则(「代码/code/实现」),
/// [`MockProvider::mock_polish`] 靠本常量作前缀识别润色请求
/// (见 `stream_complete` 的分支)。
pub(crate) const POLISH_INSTRUCTIONS: &str = "你是文字润色助手。请改写下方选中的文本,要求:\
保持原意与 Markdown 结构不变;修正错别字与不通顺的表述,压缩重复标点与多余空白;\
只输出润色后的文本,不要输出任何解释。";

/// 润色素材(选区)的字节预算现状默认:未配置「上下文大小」时落回它与
/// 摘要同档(decisions-pending #110 的默认口径);配置后由 app 侧经
/// [`polish_budget`] 取配置值。
const SELECTION_BUDGET_BYTES: usize = 32 * 1024;

/// 润色素材的生效字节预算(「上下文大小」配置 → 素材上限的换算,
/// decisions-pending #110):`None` = 未配置,落回
/// `SELECTION_BUDGET_BYTES`(32KB,见上);`Some` = 配置读侧
/// (`AiConfig::context_budget`)换算好的字节数,原样生效 —— 与摘要/commit
/// 共用同一配置。app 侧以它判「选区超上下文上限」,超限拒绝不截断。
pub fn polish_budget(budget: Option<usize>) -> usize {
    budget.unwrap_or(SELECTION_BUDGET_BYTES)
}

/// 把选中文本拼成完整 prompt:指令头 + 选区原文(不截断,见模块文档)。
pub fn polish_prompt(text: &str) -> String {
    format!("{POLISH_INSTRUCTIONS}\n{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// prompt 构造:指令头在最前、素材原文完整嵌入(无截断路径)。
    #[test]
    fn prompt_embeds_selection_verbatim() {
        let selection = "# 标题\n\n正文 🦀 一段。\n";
        let prompt = polish_prompt(selection);
        assert!(prompt.starts_with(POLISH_INSTRUCTIONS), "指令头在最前");
        assert!(prompt.ends_with(selection), "素材原文完整嵌入");
        assert!(prompt.contains("保持原意与 Markdown 结构不变"), "{prompt}");
        assert!(prompt.contains("只输出润色后的文本"), "{prompt}");
    }

    /// 预算换算:未配置落回 32KB 现状默认;配置读侧的字节值原样生效。
    #[test]
    fn budget_falls_back_to_default_and_honors_config() {
        assert_eq!(polish_budget(None), SELECTION_BUDGET_BYTES);
        assert_eq!(polish_budget(Some(0)), 0, "配置 0 = 上限 0,app 侧一切超限");
        assert_eq!(polish_budget(Some(8 * 1024)), 8 * 1024);
    }
}
