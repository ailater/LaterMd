//! 模型列表自动获取(设置页「获取模型列表」)。
//!
//! 三家真实 provider 各有一个只读的列表端点,请求方法与鉴权头沿各自
//! adapter 的既有口径:
//! * OpenAI 兼容:`GET {base}/models`(`Authorization: Bearer`)——
//!   adapter 的 base_url 语义**已含版本段**(出厂 `https://api.openai.com/v1`),
//!   这里只拼 `/models`,不重复拼 `/v1`;
//! * Anthropic:`GET {base}/v1/models`(`x-api-key` + `anthropic-version`,
//!   与 `/v1/messages` 同一套头);
//! * Ollama:`GET {base}/api/tags`(本地服务无鉴权)。
//!
//! 端点拼装与响应解析是纯函数,可直接单测,不联网;HTTP 层在 std 线程里
//! 跑(与 `AiProvider::stream_complete` 同款:spawn + mpsc,零 tokio),
//! 结果经 [`fetch_models`] 的 channel 回传,UI 侧每帧 `try_recv` 永不
//! 阻塞。真实端点验证留人工(不做造假网络测试,含 127.0.0.1)。

use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ureq::Agent;

/// 列表请求超时:列表端点不加载模型(Ollama 冷启动只影响推理端点),
/// 30s 只兜底死连接,比聊天请求的 adapter 默认(60/120s)紧。
const TIMEOUT_SECS: u64 = 30;
/// Anthropic 全部端点共用的协议版本头(与 anthropic.rs 同源;该文件不在
/// 本模块改动范围,不跨文件引私有常量)。
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// 非 200 响应体片段的截断上限(与三家 adapter 的 400 同口径)。
const DETAIL_LIMIT: usize = 400;

/// 拉取结果载荷:模型名列表或面向用户的错误文案。
pub type ModelsResult = Result<Vec<String>, String>;

/// 模型列表的来源:provider 判别 + 端点 + 鉴权材料。由 app 侧按配置页
/// 草稿映射构造(见 `latermd-app` 的归约),本 crate 不持有 provider
/// 枚举,与 app 的 `ProviderKind` 解耦。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelsSource {
    /// OpenAI 兼容端点(base_url 已含版本段,如 `.../v1`)。
    OpenAiCompatible {
        /// 端点根地址。
        base_url: String,
        /// API key(Bearer)。
        api_key: String,
    },
    /// Anthropic(base_url 不含 `/v1`,与 messages 端点同源)。
    Anthropic {
        /// 端点根地址。
        base_url: String,
        /// API key(x-api-key)。
        api_key: String,
    },
    /// Ollama 本地服务,无鉴权。
    Ollama {
        /// 服务根地址。
        base_url: String,
    },
}

impl ModelsSource {
    /// 列表端点 URL(去尾斜杠与首尾空白,沿各 adapter `url()` 的拼接口径)。
    pub fn endpoint(&self) -> String {
        fn trimmed(base: &str) -> &str {
            base.trim().trim_end_matches('/')
        }
        match self {
            Self::OpenAiCompatible { base_url, .. } => format!("{}/models", trimmed(base_url)),
            Self::Anthropic { base_url, .. } => format!("{}/v1/models", trimmed(base_url)),
            Self::Ollama { base_url } => format!("{}/api/tags", trimmed(base_url)),
        }
    }
}

/// 解析 OpenAI 兼容 `GET /models` 响应:取 `data[].id`。
///
/// 规则:服务端顺序**保序去重**;非字符串或空白 id 的条目跳过(条目级
/// 坏数据尽力而为);根不是 JSON 对象、`data` 缺失或不是数组返回 `None`
/// —— 与「合法空列表」(`Some(vec![])`)区分,后者由调用方当成功处理。
pub fn parse_openai_models(body: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    collect_model_ids(&value, "data", "id")
}

/// 解析 Anthropic `GET /v1/models` 响应:取 `data[].id`(`has_more`/
/// `first_id` 等分页字段不消费 —— 一次拉全量,翻页留待列表真的不够长)。
/// 容错规则同 [`parse_openai_models`]。
pub fn parse_anthropic_models(body: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    collect_model_ids(&value, "data", "id")
}

/// 解析 Ollama `GET /api/tags` 响应:取 `models[].name`(含 `:tag` 后缀的
/// 完整名,与 `ollama list` 一致)。容错规则同 [`parse_openai_models`]。
pub fn parse_ollama_models(body: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    collect_model_ids(&value, "models", "name")
}

/// 三家响应的同构提取:顶层对象的 `field` 数组里逐条取 `key` 字符串,
/// 保序去重、跳过非字符串与空白条目;顶层结构不对返回 `None`。
fn collect_model_ids(value: &serde_json::Value, field: &str, key: &str) -> Option<Vec<String>> {
    let items = value.as_object()?.get(field)?.as_array()?;
    let mut ids: Vec<String> = Vec::new();
    for item in items {
        let Some(id) = item.get(key).and_then(|v| v.as_str()) else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() || ids.iter().any(|seen| seen == id) {
            continue;
        }
        ids.push(id.to_owned());
    }
    Some(ids)
}

/// 非 200 响应的失败文案(HTTP 码 + 响应体片段,片段按字符截断不拆
/// CJK/emoji)。抽纯函数便于单测钉住文案形态,`run_fetch` 直接消费。
fn status_failure(code: u16, body: &str) -> String {
    format!(
        "获取模型列表失败:HTTP {code}:{}",
        truncate_chars(body, DETAIL_LIMIT)
    )
}

/// 发起一次模型列表拉取:spawn std 线程 GET 列表端点,结果经 `tx` 回传,
/// 线程句柄交还调用方(测试 join;生产侧丢弃 —— 接收端被更新的请求替换
/// 后,发送失败即静默收尾,与 AI 流式 worker 同约定)。
///
/// 鉴权型来源(OpenAI 兼容/Anthropic)的 key 缺失在线程内判定:不发起
/// 请求直接回错误文案 —— 这条分支让「spawn → channel → join」的线程
/// 机制可以零网络验证。
pub fn fetch_models(source: ModelsSource, tx: Sender<ModelsResult>) -> Option<JoinHandle<()>> {
    thread::Builder::new()
        .name("latermd-ai-models".into())
        .spawn(move || {
            let _ = tx.send(run_fetch(&source));
        })
        .ok()
}

/// 后台线程的一次完整拉取:请求 → 状态码检查 → 解析。所有错误都收敛为
/// 面向用户的文案,不走 panic。
fn run_fetch(source: &ModelsSource) -> ModelsResult {
    match source {
        ModelsSource::OpenAiCompatible { api_key, .. }
        | ModelsSource::Anthropic { api_key, .. } => {
            if api_key.trim().is_empty() {
                return Err("获取模型列表失败:未配置 API key".to_owned());
            }
        }
        ModelsSource::Ollama { .. } => {}
    }
    request(source)
}

fn request(source: &ModelsSource) -> ModelsResult {
    let agent = build_agent(Duration::from_secs(TIMEOUT_SECS));
    let mut req = agent.get(source.endpoint());
    match source {
        ModelsSource::OpenAiCompatible { api_key, .. } => {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
        ModelsSource::Anthropic { api_key, .. } => {
            req = req.header("x-api-key", api_key.as_str());
            req = req.header("anthropic-version", ANTHROPIC_VERSION);
        }
        ModelsSource::Ollama { .. } => {}
    }
    let res = req
        .call()
        .map_err(|err| format!("获取模型列表失败:{err}"))?;
    if !res.status().is_success() {
        let code = res.status().as_u16();
        let body = res.into_body().read_to_string().unwrap_or_default();
        return Err(status_failure(code, &body));
    }
    let body = res
        .into_body()
        .read_to_string()
        .map_err(|err| format!("获取模型列表失败:{err}"))?;
    let parsed = match source {
        ModelsSource::OpenAiCompatible { .. } => parse_openai_models(&body),
        ModelsSource::Anthropic { .. } => parse_anthropic_models(&body),
        ModelsSource::Ollama { .. } => parse_ollama_models(&body),
    };
    parsed.ok_or_else(|| "获取模型列表失败:响应不是预期的模型列表结构".to_owned())
}

// 下面三个助手与 openai.rs/anthropic.rs/ollama.rs 的同名私有实现同源;
// 跨文件上提要同轮改三个既有文件,不在本模块范围,刻意各留一份。

fn build_agent(timeout: Duration) -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(timeout.min(Duration::from_secs(30))))
        .timeout_recv_response(Some(timeout))
        // 整个 body 的总量预算(逐读不重置),只为让死连接最终以 Timeout
        // 收场,别永久挂住后台线程。
        .timeout_recv_body(Some(Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// OpenAI 兼容官方响应形态(字段顺序与额外字段原样保留,只取 id)。
    const OPENAI_SAMPLE: &str = r#"{
        "object": "list",
        "data": [
            {"id": "gpt-4o-mini", "object": "model", "created": 1721172741, "owned_by": "system"},
            {"id": "gpt-4o", "object": "model", "created": 1715367049, "owned_by": "system"}
        ]
    }"#;

    /// Anthropic 官方响应形态(带 has_more/first_id 分页字段)。
    const ANTHROPIC_SAMPLE: &str = r#"{
        "data": [
            {"id": "claude-sonnet-4-5", "type": "model", "display_name": "Claude Sonnet 4.5"},
            {"id": "claude-opus-4-1", "type": "model", "display_name": "Claude Opus 4.1"}
        ],
        "has_more": false,
        "first_id": "claude-sonnet-4-5"
    }"#;

    /// Ollama 官方响应形态(name 带 tag 后缀,details 等字段不消费)。
    const OLLAMA_SAMPLE: &str = r#"{
        "models": [
            {"name": "llama3.1:latest", "model": "llama3.1:latest", "modified_at": "2026-10-04T00:00:00Z",
             "size": 4661224676, "digest": "aabbcc", "details": {"family": "llama"}},
            {"name": "qwen2.5:14b", "model": "qwen2.5:14b", "modified_at": "2026-10-04T01:00:00Z",
             "size": 8988244224, "digest": "ddeeff", "details": {"family": "qwen2"}}
        ]
    }"#;

    #[test]
    fn openai_sample_parses_ids_in_order() {
        assert_eq!(
            parse_openai_models(OPENAI_SAMPLE),
            Some(vec!["gpt-4o-mini".to_owned(), "gpt-4o".to_owned()])
        );
    }

    #[test]
    fn anthropic_sample_parses_ids_in_order() {
        assert_eq!(
            parse_anthropic_models(ANTHROPIC_SAMPLE),
            Some(vec![
                "claude-sonnet-4-5".to_owned(),
                "claude-opus-4-1".to_owned()
            ])
        );
    }

    #[test]
    fn ollama_sample_parses_names_in_order() {
        assert_eq!(
            parse_ollama_models(OLLAMA_SAMPLE),
            Some(vec!["llama3.1:latest".to_owned(), "qwen2.5:14b".to_owned()])
        );
    }

    /// 合法空列表与「空列表里混着坏条目」:空数组是 Some(vec![])(成功),
    /// 坏条目逐个跳过、好条目照常进列表。
    #[test]
    fn empty_data_is_success_and_bad_entries_are_skipped() {
        assert_eq!(parse_openai_models(r#"{"data": []}"#), Some(Vec::new()));
        assert_eq!(parse_ollama_models(r#"{"models": []}"#), Some(Vec::new()));
        assert_eq!(
            parse_openai_models(
                r#"{"data": [
                    {"object": "model"},
                    {"id": 42},
                    {"id": null},
                    {"id": "  "},
                    {"id": "keep-me"},
                    {"id": "keep-me"},
                    {"id": " keep-me "}
                ]}"#
            ),
            Some(vec!["keep-me".to_owned()]),
            "非字符串/空白 id 跳过,重复与首尾空白折叠"
        );
    }

    /// 畸形响应:坏 JSON、data 缺失、data 非数组、根不是对象,一律 None
    /// (与合法空列表区分,由调用方报「结构不对」而非「没有模型」)。
    #[test]
    fn malformed_responses_are_none() {
        for body in ["不是 JSON", "{oops", "{}", r#"{"data": "x"}"#, "[1, 2]"] {
            assert_eq!(parse_openai_models(body), None, "{body}");
            assert_eq!(parse_anthropic_models(body), None, "{body}");
            assert_eq!(parse_ollama_models(body), None, "{body}");
        }
        // 三家字段名互不相认:OpenAI 的解析不认 Ollama 的响应,反之亦然
        assert_eq!(parse_ollama_models(OPENAI_SAMPLE), None);
        assert_eq!(parse_openai_models(OLLAMA_SAMPLE), None);
    }

    /// 端点拼装沿各 adapter base_url 语义:OpenAI 兼容的 base 已含版本段
    /// 只拼 /models;Anthropic 拼 /v1/models(一次);Ollama 拼 /api/tags;
    /// 尾斜杠与首尾空白都吃掉。
    #[test]
    fn endpoints_follow_adapter_base_url_semantics() {
        let openai = ModelsSource::OpenAiCompatible {
            base_url: "https://api.deepseek.com/v1/".to_owned(),
            api_key: "placeholder".to_owned(),
        };
        assert_eq!(openai.endpoint(), "https://api.deepseek.com/v1/models");

        let anthropic = ModelsSource::Anthropic {
            base_url: "  https://api.anthropic.com  ".to_owned(),
            api_key: "placeholder".to_owned(),
        };
        assert_eq!(anthropic.endpoint(), "https://api.anthropic.com/v1/models");

        let ollama = ModelsSource::Ollama {
            base_url: "http://127.0.0.1:11434/".to_owned(),
        };
        assert_eq!(ollama.endpoint(), "http://127.0.0.1:11434/api/tags");
    }

    /// 非 200 文案形态:前缀 + HTTP 码 + 响应体片段;长片段按字符截断
    /// (CJK/emoji 不拆半字符,400 上限)。
    #[test]
    fn status_failure_text_carries_code_and_truncated_body() {
        assert_eq!(
            status_failure(401, "Invalid API key"),
            "获取模型列表失败:HTTP 401:Invalid API key"
        );
        let long = "中".repeat(DETAIL_LIMIT + 50);
        let text = status_failure(500, &long);
        // 前缀 + "HTTP 500:" 的字符数 + 400 字符的片段
        let prefix = "获取模型列表失败:HTTP 500:";
        assert_eq!(text.chars().count(), prefix.chars().count() + DETAIL_LIMIT);
        assert!(text.starts_with(prefix));
        assert!(text.ends_with("中"), "尾字符完整,未拆半");
        // emoji 也不会截半:4 字节码点整体保留
        let emoji = "🦀".repeat(DETAIL_LIMIT + 3);
        let text = status_failure(502, &emoji);
        assert!(text.ends_with('🦀'));
    }

    /// 线程机制零网络验证:鉴权型来源 key 缺失(含纯空白)时,spawn 的
    /// 线程不发请求直接回错误文案;Ollama 无 key 概念,不在本测试射程
    /// (它的请求路径只由人工真实端点验证,绝不打 127.0.0.1)。
    #[test]
    fn fetch_without_key_fails_on_thread_without_network() {
        for source in [
            ModelsSource::OpenAiCompatible {
                base_url: "https://api.example.invalid/v1".to_owned(),
                api_key: String::new(),
            },
            ModelsSource::OpenAiCompatible {
                base_url: "https://api.example.invalid/v1".to_owned(),
                api_key: "   ".to_owned(),
            },
            ModelsSource::Anthropic {
                base_url: "https://api.example.invalid".to_owned(),
                api_key: String::new(),
            },
        ] {
            let (tx, rx) = mpsc::channel();
            let handle = fetch_models(source, tx).expect("线程应能 spawn");
            let result = rx.recv_timeout(Duration::from_secs(10));
            handle.join().unwrap();
            let err = result.expect("应有结果").expect_err("无 key 应失败");
            assert_eq!(err, "获取模型列表失败:未配置 API key", "{err}");
        }
    }
}
