//! `ai://` 链接协议的解析(语义定稿见 docs/decisions-pending.md #11)。
//!
//! 已实现动作只有一个:`ai://write?prompt=<urlencoded 提示词>` 触发 Mock
//! 流式续写;其余 `ai://` 动作识别但不拦截成执行,点击只提示。非 `ai://`
//! 前缀返回 [`None`],由调用方放行 vendored 层默认行为(系统浏览器)。
//! 纯函数、不依赖 egui:解析在点击发生前完成,归约侧只消费结果,不 panic。

use percent_encoding::percent_decode_str;

/// 协议 scheme 前缀(渲染层 link_style 的样式判断同用此常量)。
pub(crate) const SCHEME: &str = "ai://";

/// 已实现的动作:流式续写。
const ACTION_WRITE: &str = "write";

/// 解析一个链接的 href。
///
/// 返回 [`None`] = 非 `ai://` 前缀,不拦截;`Some(Ok(prompt))` = write 动作
/// 且提示词解码成功;`Some(Err(reason))` = 未实现动作或缺 prompt / 解码失败,
/// `reason` 为可直接落状态栏提示行的文案。
pub(crate) fn parse(href: &str) -> Option<Result<String, String>> {
    let rest = href.strip_prefix(SCHEME)?;
    let (action, query) = match rest.split_once('?') {
        Some((action, query)) => (action, query),
        None => (rest, ""),
    };
    if action != ACTION_WRITE {
        let name = if action.is_empty() { "(空)" } else { action };
        return Some(Err(format!("未实现的 AI 动作:{name}")));
    }
    let raw = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "prompt")
        .map(|(_, value)| value);
    match raw {
        Some(raw) if !raw.is_empty() => decode(raw).map_or_else(
            || Some(Err("AI 链接的 prompt 解码失败(需 UTF-8 百分号编码)".into())),
            |prompt| Some(Ok(prompt)),
        ),
        _ => Some(Err("ai://write 链接缺少提示词(prompt 参数)".into())),
    }
}

/// 严格解码:每个 `%` 必须后跟两个十六进制位(坏序列即失败),且解码后的
/// 字节必须是合法 UTF-8。`percent-encoding` 自身会原样放行坏 `%` 序列,这里
/// 先行校验补上严格性。
fn decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            if !hex.iter().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            i += 3;
        } else {
            i += 1;
        }
    }
    percent_decode_str(raw)
        .decode_utf8()
        .ok()
        .map(|c| c.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// write 动作:中文、空格、标点等特殊字符经 %XX 编码后无损解码。
    /// 链接串与 examples/ai-link-sample.md 冒烟样例逐字对应。
    #[test]
    fn write_link_decodes_percent_encoded_prompt() {
        assert_eq!(
            parse("ai://write?prompt=%E7%BB%AD%E5%86%99%E4%B8%80%E6%AE%B5%20Markdown%20%E4%BB%8B%E7%BB%8D"),
            Some(Ok("续写一段 Markdown 介绍".into()))
        );
        assert_eq!(
            parse(
                "ai://write?prompt=%E6%80%BB%E7%BB%93%2C%E6%9C%AC%E6%96%87%E8%A6%81%E7%82%B9%21%28%E5%90%AB%E6%A0%87%E7%82%B9%2B%E7%A9%BA%E6%A0%BC%29"
            ),
            Some(Ok("总结,本文要点!(含标点+空格)".into()))
        );
        // 多参数并存,prompt 取值到 & 为止
        assert_eq!(
            parse("ai://write?lang=zh&prompt=%E7%BB%AD%E5%86%99&tone=formal"),
            Some(Ok("续写".into()))
        );
        // + 按字面保留:解码只认严格 %XX,markdown 作者该用 %20
        assert_eq!(parse("ai://write?prompt=a+b"), Some(Ok("a+b".into())));
    }

    /// 非 ai:// 前缀一律返回 None:调用方据此不拦截,链接走系统浏览器。
    #[test]
    fn non_ai_prefix_is_not_intercepted() {
        for href in [
            "https://github.com/ailater/LaterMd",
            "http://example.com/x?y=1",
            "mailto:hi@example.com",
            "file:///tmp/a.md",
            "AI://write?prompt=x", // 前缀大小写敏感,认不出就不拦
        ] {
            assert_eq!(parse(href), None, "{href} 不该被识别为 ai:// 链接");
        }
    }

    /// 已识别前缀但动作未实现:给出「未实现」提示,不 panic、不执行。
    #[test]
    fn unsupported_action_yields_notice() {
        assert_eq!(
            parse("ai://summarize?prompt=%E6%91%98%E8%A6%81"),
            Some(Err("未实现的 AI 动作:summarize".into()))
        );
        // write-outline 等动作名是整体比较,不是 write 的前缀
        assert_eq!(
            parse("ai://write-outline"),
            Some(Err("未实现的 AI 动作:write-outline".into()))
        );
        assert_eq!(parse("ai://"), Some(Err("未实现的 AI 动作:(空)".into())));
    }

    /// write 动作但 prompt 缺失或为空:解析失败提示,不发起流。
    /// (只有 prompt 参数且解码非空才算成立;`%20` 这类解码后非空的值放行。)
    #[test]
    fn write_without_prompt_is_malformed() {
        assert_eq!(
            parse("ai://write"),
            Some(Err("ai://write 链接缺少提示词(prompt 参数)".into()))
        );
        assert_eq!(
            parse("ai://write?foo=1"),
            Some(Err("ai://write 链接缺少提示词(prompt 参数)".into()))
        );
        assert_eq!(
            parse("ai://write?prompt="),
            Some(Err("ai://write 链接缺少提示词(prompt 参数)".into()))
        );
    }

    /// 非法百分号编码(坏 % 序列 / 截断的多字节 UTF-8):解码失败提示。
    #[test]
    fn malformed_percent_encoding_yields_notice() {
        assert_eq!(
            parse("ai://write?prompt=%ZZ"),
            Some(Err("AI 链接的 prompt 解码失败(需 UTF-8 百分号编码)".into()))
        );
        // 「续」是三字节 E7 BB AD,截掉最后一段即非法 UTF-8
        assert_eq!(
            parse("ai://write?prompt=%E7%BB"),
            Some(Err("AI 链接的 prompt 解码失败(需 UTF-8 百分号编码)".into()))
        );
    }
}
