//! Anthropic messages API provider 与 SSE 增量解析器。
//!
//! 解析器是纯函数:`data: {...}` 行提取 `content_block_delta` 的
//! `delta.text`,`message_stop` 终止,可直接单测,不联网。HTTP 层只负责
//! 逐行喂给解析器,面积刻意保持很小;真实端点验证留到有真实 key 时
//! (不做造假网络测试)。与 openai.rs 的差异集中在四处:鉴权走
//! `x-api-key` 头(不是 Bearer)、必须带 `anthropic-version` 头、
//! `max_tokens` 是协议必填字段、system 走顶层 `system` 字段而非
//! messages 里的 system role。
//!
//! 可调参数集中在 [`AnthropicSettings`]:端点、模型、采样参数、system
//! prompt、超时、流式开关。配置由 app 侧的 AI 配置页填写,本模块不读
//! 任何环境变量(openai 的 env 通道是历史保留,Anthropic 不新增)。

use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;
use ureq::Agent;

use crate::{AiError, AiProvider, Chunk};

/// Messages API 的协议版本头(Anthropic 全部端点共用这一个稳定版本号)。
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic messages API 的可调参数(app 侧 AI 配置页的落点)。
///
/// `temperature`/`top_p` 用 `Option`:不填就不发给服务端,让它用自己的
/// 默认值(官方端点默认 temperature=1.0)。`max_tokens` 是协议必填字段,
/// 没有「不发」的选项,默认 2048。
#[derive(Debug, Clone, PartialEq)]
pub struct AnthropicSettings {
    /// 端点根地址(不含 `/v1`,`url()` 自行拼接)。
    pub base_url: String,
    /// 模型名。
    pub model: String,
    /// 采样温度;不填则不发送该字段。
    pub temperature: Option<f32>,
    /// 核采样;不填则不发送。
    pub top_p: Option<f32>,
    /// 回复 token 上限(协议必填,恒发送)。
    pub max_tokens: u32,
    /// system prompt;空/空白 = 不发送顶层 system 字段。
    pub system_prompt: String,
    /// 是否流式(SSE)。关闭则一次性取全文。
    pub stream: bool,
    /// 连接与响应超时(秒)。
    pub timeout_secs: u64,
}

impl Default for AnthropicSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_owned(),
            // 主力型号取 Sonnet 系别名 `claude-sonnet-4-5`:Anthropic 文档
            // 对多数任务的默认推荐是 Sonnet 级(速度/成本/智能的平衡点,
            // Opus 级留给最难的任务),alias 形式自动跟随官方最新快照,
            // 不钉具体日期版本。来源:docs.anthropic.com「Models overview」
            // (2026-10-04 查证,官方页对本地区域封锁,经多路搜索结果交叉
            // 确认;岔路登记 decisions-pending #92)。
            model: "claude-sonnet-4-5".to_owned(),
            // 不发采样参数,用服务端默认(temperature=1.0),与 openai.rs
            // 默认硬塞 0.7 的口径不同——官方端点默认值是明确的。
            temperature: None,
            top_p: None,
            max_tokens: 2048,
            system_prompt: String::new(),
            stream: true,
            timeout_secs: 60,
        }
    }
}

impl AnthropicSettings {
    /// 端点根地址(去掉结尾斜杠,避免拼出 `/v1//messages`)。
    pub fn base_url_trimmed(&self) -> &str {
        self.base_url.trim().trim_end_matches('/')
    }

    /// 请求体里是否要带顶层 system 字段。
    fn has_system(&self) -> bool {
        !self.system_prompt.trim().is_empty()
    }
}

/// 解析一段 Anthropic SSE 文本,产出 [`Chunk`] 序列。
///
/// 规则:`data:` 前缀行取 JSON;`type == content_block_delta` 且
/// `delta.type == text_delta` 提取 `delta.text` 为正文块;`type ==
/// message_stop` 产出空 delta 的终止块并忽略其后内容;`type == error`
/// (或携带 `error` 字段)产出 `done = true` 且 delta 为错误描述的失败块
/// (与 [`Chunk`] 的失败约定一致);`event:`/`ping`/注释行与解析不了的行
/// 一律跳过。每条 `data:` 行独立成块,不做跨行 data 聚合。
pub fn parse_anthropic_sse(text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(chunk) = parse_data_line(line) {
            let done = chunk.done;
            chunks.push(chunk);
            if done {
                break;
            }
        }
    }
    chunks
}

fn parse_data_line(line: &str) -> Option<Chunk> {
    let value = line.strip_prefix("data:")?;
    let value = value.strip_prefix(' ').unwrap_or(value);
    let parsed: serde_json::Value = serde_json::from_str(value).ok()?;
    if let Some(chunk) = error_chunk(&parsed) {
        return Some(chunk);
    }
    if parsed.get("type").and_then(|t| t.as_str()) == Some("message_stop") {
        return Some(Chunk {
            delta: String::new(),
            done: true,
        });
    }
    text_delta(&parsed).map(|delta| Chunk { delta, done: false })
}

/// `error` 事件(或任意携带 `error` 字段的 data):产出失败块。错误描述
/// 优先取 `error.message`,退到 `error.type`,再退固定文案。
fn error_chunk(value: &serde_json::Value) -> Option<Chunk> {
    let error = value.get("error")?;
    let detail = error
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| error.get("type").and_then(|t| t.as_str()))
        .unwrap_or("未知错误");
    Some(Chunk {
        delta: format!("Anthropic 流失败:{detail}"),
        done: true,
    })
}

/// 提取 `content_block_delta` 里 `delta.type == text_delta` 的文本;
/// 其他 delta 种类(thinking/input_json 等)、空文本、缺字段都返回 `None`。
fn text_delta(value: &serde_json::Value) -> Option<String> {
    if value.get("type").and_then(|t| t.as_str()) != Some("content_block_delta") {
        return None;
    }
    let delta = value.get("delta")?;
    if delta.get("type").and_then(|t| t.as_str()) != Some("text_delta") {
        return None;
    }
    let text = delta.get("text")?.as_str()?;
    if text.is_empty() {
        return None;
    }
    Some(text.to_owned())
}

/// 非流式响应取 `content[]` 里全部 text 块按序拼接;`content` 缺失、
/// 不是数组、没有任何 text 块都返回 `None`。
fn message_content(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let content = value.get("content")?.as_array()?;
    let mut text = String::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            text.push_str(block.get("text")?.as_str()?);
        }
    }
    if text.is_empty() {
        return None;
    }
    Some(text)
}

/// Anthropic messages API(`/v1/messages`)provider。
#[derive(Clone)]
pub struct AnthropicProvider {
    api_key: String,
    settings: AnthropicSettings,
    agent: Agent,
}

impl AnthropicProvider {
    /// 以默认参数构造(测试用;生产走 [`Self::with_settings`])。
    /// key 由调用方传入,本 provider 无 env 通道。
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_settings(api_key, AnthropicSettings::default())
    }

    /// 按完整参数构造。
    pub fn with_settings(api_key: impl Into<String>, settings: AnthropicSettings) -> Self {
        let agent = build_agent(Duration::from_secs(settings.timeout_secs.max(1)));
        Self {
            api_key: api_key.into(),
            settings,
            agent,
        }
    }

    /// 当前参数(UI 回显与测试断言用)。
    pub fn settings(&self) -> &AnthropicSettings {
        &self.settings
    }

    fn url(&self) -> String {
        format!("{}/v1/messages", self.settings.base_url_trimmed())
    }

    /// `stream` 作为参数而非读设置:`complete_sync` 恒按非流式请求
    /// (openai.rs 把设置里的 stream 原样塞进请求体,流式配置下
    /// `complete_sync` 会拿到 SSE 却按整包 JSON 解析——这里不复刻)。
    fn request_body(&self, prompt: &str, stream: bool) -> String {
        let mut body = json!({
            "model": self.settings.model,
            "max_tokens": self.settings.max_tokens,
            "stream": stream,
            // Anthropic 的 system 是顶层字段,messages 里只有 user/assistant
            "messages": [{ "role": "user", "content": prompt }],
        });
        if self.settings.has_system() {
            body["system"] = json!(self.settings.system_prompt);
        }
        if let Some(temperature) = self.settings.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(top_p) = self.settings.top_p {
            body["top_p"] = json!(top_p);
        }
        body.to_string()
    }

    /// 非流式补全:commit message 这类「要一行结果」的场景用,不占流式
    /// 通道。`content[]` 的 text 块按序拼接为整段文本。
    pub fn complete_sync(&self, prompt: &str) -> Result<String, AiError> {
        let res = self.post(prompt, false)?;
        let body = res
            .into_body()
            .read_to_string()
            .map_err(|err| AiError::Http(err.to_string()))?;
        message_content(&body).ok_or_else(|| AiError::Http("响应里没有 content 文本块".to_owned()))
    }

    fn post(
        &self,
        prompt: &str,
        stream: bool,
    ) -> Result<ureq::http::Response<ureq::Body>, AiError> {
        self.agent
            .post(&self.url())
            .header("x-api-key", self.api_key.as_str())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .content_type("application/json")
            .send(self.request_body(prompt, stream))
            .map_err(|err| AiError::Http(err.to_string()))
    }

    fn run(&self, prompt: &str, tx: &Sender<Chunk>) {
        if let Err(err) = self.stream(prompt, tx) {
            // 失败约定:最后一个 done=true 块携带错误描述(见 Chunk 文档)
            let _ = tx.send(Chunk {
                delta: format!("AI 请求失败:{err}"),
                done: true,
            });
        }
    }

    fn stream(&self, prompt: &str, tx: &Sender<Chunk>) -> Result<(), AiError> {
        let res = self.post(prompt, self.settings.stream)?;

        if !res.status().is_success() {
            let code = res.status().as_u16();
            let detail = res.into_body().read_to_string().unwrap_or_default();
            return Err(AiError::HttpStatus {
                code,
                detail: truncate_chars(&detail, 400),
            });
        }

        // 非流式配置:整份响应一次性解析成单个正文块 + 结束块
        if !self.settings.stream {
            let body = res
                .into_body()
                .read_to_string()
                .map_err(|err| AiError::Http(err.to_string()))?;
            let text = message_content(&body)
                .ok_or_else(|| AiError::Http("响应里没有 content 文本块".to_owned()))?;
            let _ = tx.send(Chunk {
                delta: text,
                done: false,
            });
            let _ = tx.send(Chunk {
                delta: String::new(),
                done: true,
            });
            return Ok(());
        }

        let mut saw_done = false;
        for line in BufReader::new(res.into_body().into_reader()).lines() {
            let line = line.map_err(|err| AiError::Http(err.to_string()))?;
            for chunk in parse_anthropic_sse(&line) {
                saw_done |= chunk.done;
                if tx.send(chunk).is_err() {
                    return Ok(()); // 接收端取消,静默收尾
                }
                if saw_done {
                    return Ok(());
                }
            }
        }
        if !saw_done {
            return Err(AiError::Http("流在 message_stop 之前中断".to_owned()));
        }
        Ok(())
    }
}

impl AiProvider for AnthropicProvider {
    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()> {
        let this = self.clone();
        let prompt = prompt.to_owned();
        thread::Builder::new()
            .name("latermd-ai-anthropic".into())
            .spawn(move || this.run(&prompt, &tx))
            .expect("spawn latermd-ai anthropic worker")
    }
}

// 下面两个助手与 openai.rs 的同名私有实现保持一致;两处刻意不共用,
// 待第三个消费者(Ollama adapter)出现时再上提到 crate 级共享。

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

    #[test]
    fn sse_standard_sample_parses_deltas_and_done() {
        let sample = "event: message_start\n\
                      data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\"}}\n\n\
                      event: content_block_start\n\
                      data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
                      event: content_block_delta\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"你好\"}}\n\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\",世界\"}}\n\n\
                      event: content_block_stop\n\
                      data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
                      event: message_delta\n\
                      data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n\
                      event: message_stop\n\
                      data: {\"type\":\"message_stop\"}\n\n";
        assert_eq!(
            parse_anthropic_sse(sample),
            vec![
                Chunk {
                    delta: "你好".to_owned(),
                    done: false
                },
                Chunk {
                    delta: ",世界".to_owned(),
                    done: false
                },
                Chunk {
                    delta: String::new(),
                    done: true
                },
            ]
        );
    }

    #[test]
    fn sse_non_text_deltas_and_lifecycle_events_are_skipped() {
        let sample = "data: {\"type\":\"ping\"}\n\
                      data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"内部推理\"}}\n\
                      data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"\"}}\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":null}}\n\
                      data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\
                      data: {\"type\":\"message_stop\"}\n";
        assert_eq!(
            parse_anthropic_sse(sample),
            vec![
                Chunk {
                    delta: "ok".to_owned(),
                    done: false
                },
                Chunk {
                    delta: String::new(),
                    done: true
                },
            ]
        );
    }

    /// `type == error` 与「带 error 字段」两种形态都产出失败块,且其后
    /// 内容被忽略(delta 非空 + done=true = 流失败,见 Chunk 约定)。
    #[test]
    fn sse_error_event_yields_failed_chunk_and_stops() {
        let sample = "data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\
                      data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"不该出现\"}}\n\
                      data: {\"type\":\"message_stop\"}\n";
        assert_eq!(
            parse_anthropic_sse(sample),
            vec![Chunk {
                delta: "Anthropic 流失败:Overloaded".to_owned(),
                done: true
            }]
        );

        // 没有 type 字段、只带 error 对象的形态
        let bare = "data: {\"error\":{\"type\":\"api_error\",\"message\":\"boom\"}}\n";
        assert_eq!(
            parse_anthropic_sse(bare),
            vec![Chunk {
                delta: "Anthropic 流失败:boom".to_owned(),
                done: true
            }]
        );

        // message 缺失退到 error.type
        let typed_only = "data: {\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\"}}\n";
        assert_eq!(
            parse_anthropic_sse(typed_only),
            vec![Chunk {
                delta: "Anthropic 流失败:rate_limit_error".to_owned(),
                done: true
            }]
        );
    }

    #[test]
    fn sse_message_stop_terminates_and_rest_is_ignored() {
        let sample = "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"前半\"}}\n\
                      data: {\"type\":\"message_stop\"}\n\
                      data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"不该出现\"}}\n";
        assert_eq!(
            parse_anthropic_sse(sample),
            vec![
                Chunk {
                    delta: "前半".to_owned(),
                    done: false
                },
                Chunk {
                    delta: String::new(),
                    done: true
                },
            ]
        );
    }

    #[test]
    fn sse_ignores_noise_and_handles_crlf_and_missing_space() {
        let sample = ": keep-alive\r\n\
                      event: content_block_delta\r\n\
                      data:{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"A\"}}\r\n\
                      data: 不是 JSON\r\n\
                      data: {\"type\":\"message_stop\"}\r\n";
        assert_eq!(
            parse_anthropic_sse(sample),
            vec![
                Chunk {
                    delta: "A".to_owned(),
                    done: false
                },
                Chunk {
                    delta: String::new(),
                    done: true
                },
            ]
        );
    }

    #[test]
    fn sse_empty_or_undone_text_yields_no_chunks() {
        assert!(parse_anthropic_sse("").is_empty());
        assert!(parse_anthropic_sse("event: ping\n\n").is_empty());
        assert!(parse_anthropic_sse("data: {\"type\":\"ping\"}\n").is_empty());
    }

    #[test]
    fn request_body_is_anthropic_messages_stream() {
        let provider = AnthropicProvider::new("sk-ant-test");
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", true)).unwrap();
        assert_eq!(value["model"], "claude-sonnet-4-5");
        assert_eq!(value["max_tokens"], 2048, "max_tokens 是协议必填,恒发送");
        assert_eq!(value["stream"], true);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "续写");
        // 默认不发采样参数,不设 system 就不发顶层 system 字段
        assert!(value.get("temperature").is_none());
        assert!(value.get("top_p").is_none());
        assert!(value.get("system").is_none());
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
    }

    /// 自定义端点/模型/采样参数/system 全部进请求体;system 走顶层字段,
    /// messages 里只有 user;`None` 的采样参数不发字段。
    #[test]
    fn request_body_carries_settings() {
        let provider = AnthropicProvider::with_settings(
            "sk-ant-test",
            AnthropicSettings {
                base_url: "https://api.anthropic.com/".to_owned(),
                model: "claude-opus-4-6".to_owned(),
                temperature: Some(0.5),
                top_p: Some(0.9),
                max_tokens: 1024,
                system_prompt: "你是技术文档助手".to_owned(),
                stream: false,
                timeout_secs: 120,
            },
        );
        assert_eq!(
            provider.url(),
            "https://api.anthropic.com/v1/messages",
            "尾斜杠被去掉,/v1/messages 只出现一次"
        );
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", false)).unwrap();
        assert_eq!(value["model"], "claude-opus-4-6");
        assert_eq!(value["max_tokens"], 1024);
        assert_eq!(value["stream"], false);
        assert_eq!(value["system"], "你是技术文档助手");
        // f32 → JSON 有精度误差,按容差断言
        assert!((value["temperature"].as_f64().unwrap() - 0.5).abs() < 1e-3);
        assert!((value["top_p"].as_f64().unwrap() - 0.9).abs() < 1e-3);
        // system 不占 messages 槽位
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    /// 空白 system prompt 不产生顶层 system 字段(空白也算「没写」);
    /// `complete_sync` 的请求体恒为非流式。
    #[test]
    fn blank_system_prompt_is_omitted_and_complete_sync_forces_non_stream() {
        let provider = AnthropicProvider::with_settings(
            "sk-ant-test",
            AnthropicSettings {
                system_prompt: "   \n  ".to_owned(),
                stream: true,
                ..AnthropicSettings::default()
            },
        );
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", false)).unwrap();
        assert!(value.get("system").is_none());
        assert_eq!(value["stream"], false, "complete_sync 恒非流式");
    }

    /// 非流式响应的正文提取:text 块按序拼接,非 text 块跳过。
    #[test]
    fn message_content_extraction() {
        assert_eq!(
            message_content(
                "{\"content\":[{\"type\":\"text\",\"text\":\"docs: \"},{\"type\":\"text\",\"text\":\"新增 README\"}]}"
            )
            .as_deref(),
            Some("docs: 新增 README")
        );
        assert_eq!(
            message_content(
                "{\"content\":[{\"type\":\"thinking\",\"thinking\":\"推理\"},{\"type\":\"text\",\"text\":\"答案\"}]}"
            )
            .as_deref(),
            Some("答案")
        );
        assert_eq!(message_content("{\"content\":[]}"), None);
        assert_eq!(message_content("{\"content\":[{\"type\":\"text\"}]}"), None);
        assert_eq!(message_content("不是 JSON"), None);
    }

    #[test]
    fn default_settings_match_documented_defaults() {
        let settings = AnthropicProvider::new("sk-ant-test").settings().clone();
        assert_eq!(settings.base_url, "https://api.anthropic.com");
        assert_eq!(settings.model, "claude-sonnet-4-5");
        assert_eq!(settings.temperature, None);
        assert_eq!(settings.top_p, None);
        assert_eq!(settings.max_tokens, 2048);
        assert!(settings.stream);
        assert_eq!(settings.timeout_secs, 60);
    }
}
