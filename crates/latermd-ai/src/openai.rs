//! OpenAI 兼容 provider 与 SSE 增量解析器。
//!
//! 解析器是纯函数:`data: {...}` 行提取 `choices[0].delta.content`,
//! `data: [DONE]` 终止,可直接单测,不联网。HTTP 层只负责逐行喂给解析器,
//! 面积刻意保持很小;真实端点验证留到有真实 key 时(不做造假网络测试)。

use std::fmt;
use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;
use ureq::Agent;

use crate::{read_api_key, AiProvider, Chunk};

/// API key 的环境变量名(decisions-pending #3)。
pub const API_KEY_ENV: &str = "LATERMD_AI_API_KEY";
/// 可选:OpenAI 兼容端点根地址。
const BASE_URL_ENV: &str = "LATERMD_AI_BASE_URL";
/// 可选:模型名。
const MODEL_ENV: &str = "LATERMD_AI_MODEL";

/// provider 侧错误。构造期错误从 `from_env` 返回;请求期错误经
/// `Chunk`(done=true)回传给消费方,不走 panic。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiError {
    /// 未设置 `LATERMD_AI_API_KEY`(或为空白)。
    MissingApiKey,
    /// 端点返回非 2xx;`detail` 是响应体片段(截断),通常含服务端报错。
    HttpStatus { code: u16, detail: String },
    /// 传输层错误(连接失败、读中断等)。
    Http(String),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::MissingApiKey => {
                write!(f, "未设置环境变量 {}", API_KEY_ENV)
            }
            AiError::HttpStatus { code, detail } => {
                write!(f, "HTTP {code}:{detail}")
            }
            AiError::Http(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for AiError {}

/// 解析一段 OpenAI 兼容 SSE 文本,产出 [`Chunk`] 序列。
///
/// 规则:`data:` 前缀行取 JSON,提取 `choices[0].delta.content`;
/// `data: [DONE]` 产出空 delta 的终止块并忽略其后内容;注释行、
/// `event:`/`id:` 行与解析不了的行一律跳过。每条 `data:` 行独立成块,
/// 不做跨行 data 聚合(OpenAI 兼容端点不这么发)。
pub fn parse_openai_sse(text: &str) -> Vec<Chunk> {
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
    if value.trim() == "[DONE]" {
        return Some(Chunk {
            delta: String::new(),
            done: true,
        });
    }
    let delta = delta_content(value)?;
    if delta.is_empty() {
        return None;
    }
    Some(Chunk { delta, done: false })
}

/// 提取 `choices[0].delta.content`;缺 delta、content 为 null、choices 为空
/// 都返回 `None`。
fn delta_content(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let content = value.get("choices")?.get(0)?.get("delta")?.get("content")?;
    content.as_str().map(str::to_owned)
}

/// OpenAI 兼容(`/chat/completions` + SSE)provider。
#[derive(Clone)]
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    model: String,
    agent: Agent,
}

impl OpenAiProvider {
    const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
    const DEFAULT_MODEL: &str = "gpt-4o-mini";

    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: Self::DEFAULT_BASE_URL.to_owned(),
            model: Self::DEFAULT_MODEL.to_owned(),
            agent: build_agent(),
        }
    }

    /// 从环境构造:key 必填(`LATERMD_AI_API_KEY`),base_url 与模型可选。
    /// 无 key 返回 [`AiError::MissingApiKey`],不 panic。
    pub fn from_env() -> Result<Self, AiError> {
        let api_key = read_api_key().ok_or(AiError::MissingApiKey)?;
        let mut this = Self::new(api_key);
        if let Ok(base) = std::env::var(BASE_URL_ENV) {
            let base = base.trim().trim_end_matches('/');
            if !base.is_empty() {
                this.base_url = base.to_owned();
            }
        }
        if let Ok(model) = std::env::var(MODEL_ENV) {
            let model = model.trim();
            if !model.is_empty() {
                this.model = model.to_owned();
            }
        }
        Ok(this)
    }

    fn request_body(&self, prompt: &str) -> String {
        json!({
            "model": self.model,
            "stream": true,
            "messages": [{ "role": "user", "content": prompt }],
        })
        .to_string()
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
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let res = self
            .agent
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .content_type("application/json")
            .send(self.request_body(prompt))
            .map_err(|err| AiError::Http(err.to_string()))?;

        if !res.status().is_success() {
            let code = res.status().as_u16();
            let detail = res.into_body().read_to_string().unwrap_or_default();
            return Err(AiError::HttpStatus {
                code,
                detail: truncate_chars(&detail, 400),
            });
        }

        let mut saw_done = false;
        for line in BufReader::new(res.into_body().into_reader()).lines() {
            let line = line.map_err(|err| AiError::Http(err.to_string()))?;
            for chunk in parse_openai_sse(&line) {
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
            return Err(AiError::Http("流在 [DONE] 之前中断".to_owned()));
        }
        Ok(())
    }
}

impl AiProvider for OpenAiProvider {
    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()> {
        let this = self.clone();
        let prompt = prompt.to_owned();
        thread::Builder::new()
            .name("latermd-ai-openai".into())
            .spawn(move || this.run(&prompt, &tx))
            .expect("spawn latermd-ai openai worker")
    }
}

fn build_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        // 注意:这是整个 body 的总量预算(逐读不重置)。单次续写流不会
        // 跑到 10 分钟,设它只为让死连接最终以 Timeout 收场,别永久挂住
        // 后台线程;真正的逐块空闲超时等有真实端点数据再加。
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
    use std::env;

    #[test]
    fn sse_standard_sample_parses_deltas_and_done() {
        let sample = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n\n\
                      data: {\"choices\":[{\"delta\":{\"content\":\",世界\"}}]}\n\n\
                      data: [DONE]\n\n";
        assert_eq!(
            parse_openai_sse(sample),
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
    fn sse_missing_delta_null_content_empty_choices_are_skipped() {
        let sample = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\
                      data: {\"choices\":[{\"delta\":{\"content\":null}}]}\n\
                      data: {\"choices\":[]}\n\
                      data: {\"choices\":[{\"delta\":{\"content\":\"\"}}]}\n\
                      data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\
                      data: [DONE]\n";
        assert_eq!(
            parse_openai_sse(sample),
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

    #[test]
    fn sse_done_terminates_and_rest_is_ignored() {
        let sample = "data: {\"choices\":[{\"delta\":{\"content\":\"前半\"}}]}\n\
                      data: [DONE]\n\
                      data: {\"choices\":[{\"delta\":{\"content\":\"不该出现\"}}]}\n";
        assert_eq!(
            parse_openai_sse(sample),
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
                      event: delta\r\n\
                      data:{\"choices\":[{\"delta\":{\"content\":\"A\"}}]}\r\n\
                      data: 不是 JSON\r\n\
                      data: [DONE]\r\n";
        assert_eq!(
            parse_openai_sse(sample),
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
        assert!(parse_openai_sse("").is_empty());
        assert!(parse_openai_sse("event: delta\n\n").is_empty());
    }

    #[test]
    fn api_key_env_roundtrip_and_from_env_paths() {
        // 本测试是唯一触碰环境变量的测试,单线程内顺序执行,无并行竞态
        env::remove_var(API_KEY_ENV);
        assert_eq!(read_api_key(), None);
        assert!(matches!(
            OpenAiProvider::from_env(),
            Err(AiError::MissingApiKey)
        ));

        env::set_var(API_KEY_ENV, "  sk-test  ");
        assert_eq!(read_api_key().as_deref(), Some("sk-test"));

        let provider = OpenAiProvider::from_env().expect("有 key 时构造应成功");
        assert_eq!(provider.model, OpenAiProvider::DEFAULT_MODEL);
        env::remove_var(API_KEY_ENV);
        assert_eq!(read_api_key(), None);
    }

    #[test]
    fn request_body_is_openai_chat_completions_stream() {
        let provider = OpenAiProvider::new("sk-test");
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写")).unwrap();
        assert_eq!(value["model"], "gpt-4o-mini");
        assert_eq!(value["stream"], true);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "续写");
    }
}
