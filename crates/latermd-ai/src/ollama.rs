//! Ollama 本地 provider(`/api/chat`)与 NDJSON 逐行解析器。
//!
//! 解析器是纯函数:每行一个独立 JSON,提取 `message.content`,`done == true`
//! 终止,可直接单测,不联网。HTTP 层只负责逐行喂给解析器,面积刻意保持
//! 很小;真实端点验证留到用户本机起服务时(本模块不做真实请求,包括
//! 127.0.0.1:11434)。与 openai.rs/anthropic.rs 的差异集中在三处:本地
//! 服务无鉴权(不发 Authorization/x-api-key 头,构造不收 api_key)、
//! 传输是 NDJSON 而非 SSE(没有 `data:` 前缀)、采样参数进请求体的
//! `options` 子对象而非顶层。
//!
//! 可调参数集中在 [`OllamaSettings`]:端点、模型、采样参数、system
//! prompt、超时、流式开关。配置由 app 侧的 AI 配置页填写,本模块不读
//! 任何环境变量(openai 的 env 通道是历史保留,Ollama 不新增)。

use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::json;
use ureq::Agent;

use crate::{AiError, AiProvider, Chunk};

/// Ollama(`/api/chat`)的可调参数(app 侧 AI 配置页的落点)。
///
/// 采样参数进请求体的 `options` 子对象(Ollama 的约定),全部用 `Option`:
/// 不填就不发该字段,让它用模型 Modelfile 里烙好的默认值;全部 `None`
/// 时整个 `options` 字段都不发。
#[derive(Debug, Clone, PartialEq)]
pub struct OllamaSettings {
    /// 服务根地址(默认本机 11434;远程部署的 Ollama 填目标机器地址)。
    pub base_url: String,
    /// 模型名。默认值只是常见本地型号的占位,实际以用户本机已 pull 的
    /// 为准(`ollama list`);名字对不上时 Ollama 返回 HTTP 404,body 为
    /// `{"error":"model 'x' not found, try pulling it first"}`。
    pub model: String,
    /// 采样温度;不填则 `options` 不含该字段。
    pub temperature: Option<f32>,
    /// 核采样;不填则不含。
    pub top_p: Option<f32>,
    /// 回复 token 上限;负值是 Ollama 的合法取值(-1 不限、-2 填满
    /// 上下文窗口);不填则不含。
    pub num_predict: Option<i32>,
    /// system prompt;空/空白 = 不发送 system 消息(Ollama 的 system 走
    /// messages 里的 system role)。
    pub system_prompt: String,
    /// 是否流式(NDJSON)。关闭则一次性取全文。
    pub stream: bool,
    /// 连接与响应超时(秒)。
    pub timeout_secs: u64,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".to_owned(),
            model: "llama3.1".to_owned(),
            // 不发采样参数,用服务端默认(Modelfile 烙的值,默认线
            // temperature 0.8)——本地服务的「自家默认」就是模型作者的
            // 意图,比替用户硬塞一个数更可信(与 anthropic.rs 口径一致)。
            temperature: None,
            top_p: None,
            num_predict: None,
            system_prompt: String::new(),
            stream: true,
            // 本地推理比云端慢:冷启动要先把模型载入内存(几 GB,CPU 机器
            // 数十秒),与 openai/anthropic 共用的 60s 会误伤,加倍到 120。
            timeout_secs: 120,
        }
    }
}

impl OllamaSettings {
    /// 服务根地址(去掉结尾斜杠,避免拼出 `/api//chat`)。
    pub fn base_url_trimmed(&self) -> &str {
        self.base_url.trim().trim_end_matches('/')
    }

    /// 请求体里是否要带 system 消息。
    fn has_system(&self) -> bool {
        !self.system_prompt.trim().is_empty()
    }
}

/// 解析一段 Ollama NDJSON 文本,产出 [`Chunk`] 序列。
///
/// 规则:每行一个独立 JSON;顶层带 `error` 字段(非 null)产出
/// `done = true` 且 delta 为错误描述的失败块(与 [`Chunk`] 的失败约定
/// 一致);`done == true` 的行产出空 delta 的终止块并忽略其后内容(终行
/// 即使带 `message.content` 也不算正文);`message.content` 非空字符串
/// 产出正文块;空行与解析不了的行一律跳过。
pub fn parse_ollama_ndjson(text: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(chunk) = parse_ndjson_line(line) {
            let done = chunk.done;
            chunks.push(chunk);
            if done {
                break;
            }
        }
    }
    chunks
}

fn parse_ndjson_line(line: &str) -> Option<Chunk> {
    if line.trim().is_empty() {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(line).ok()?;
    if let Some(chunk) = error_chunk(&parsed) {
        return Some(chunk);
    }
    if parsed.get("done").and_then(|d| d.as_bool()) == Some(true) {
        return Some(Chunk {
            delta: String::new(),
            done: true,
        });
    }
    message_delta(&parsed).map(|delta| Chunk { delta, done: false })
}

/// 顶层 `error` 字段(Ollama 是字符串,如 "model 'x' not found, try
/// pulling it first"):产出失败块;字段为 null 视为没有错误。
fn error_chunk(value: &serde_json::Value) -> Option<Chunk> {
    let error = value.get("error").filter(|e| !e.is_null())?;
    let detail = error.as_str().unwrap_or("未知错误");
    Some(Chunk {
        delta: format!("Ollama 流失败:{detail}"),
        done: true,
    })
}

/// 提取 `message.content`;缺 message、content 缺失/null/非字符串/空串
/// 都返回 `None`(空增量没有消费者,不值得成块)。
fn message_delta(value: &serde_json::Value) -> Option<String> {
    let content = value.get("message")?.get("content")?.as_str()?;
    if content.is_empty() {
        return None;
    }
    Some(content.to_owned())
}

/// 非流式响应取 `message.content`;缺字段、空串、坏 JSON 都返回 `None`。
fn message_content(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let content = value.get("message")?.get("content")?.as_str()?;
    if content.is_empty() {
        return None;
    }
    Some(content.to_owned())
}

/// Ollama(`/api/chat`)provider。本地服务无鉴权,构造不收 api_key。
#[derive(Clone)]
pub struct OllamaProvider {
    settings: OllamaSettings,
    agent: Agent,
}

impl OllamaProvider {
    /// 以默认参数构造(测试用;生产走 [`Self::with_settings`])。
    pub fn new() -> Self {
        Self::with_settings(OllamaSettings::default())
    }

    /// 按完整参数构造。
    pub fn with_settings(settings: OllamaSettings) -> Self {
        let agent = build_agent(Duration::from_secs(settings.timeout_secs.max(1)));
        Self { settings, agent }
    }

    /// 当前参数(UI 回显与测试断言用)。
    pub fn settings(&self) -> &OllamaSettings {
        &self.settings
    }

    fn url(&self) -> String {
        format!("{}/api/chat", self.settings.base_url_trimmed())
    }

    /// `stream` 作为参数而非读设置:`complete_sync` 恒按非流式请求
    /// (流式配置下拿到的会是 NDJSON,按单 JSON 解析必炸——与
    /// anthropic.rs 同口径)。
    fn request_body(&self, prompt: &str, stream: bool) -> String {
        let mut messages = Vec::new();
        if self.settings.has_system() {
            messages.push(json!({ "role": "system", "content": self.settings.system_prompt }));
        }
        messages.push(json!({ "role": "user", "content": prompt }));

        let mut body = json!({
            "model": self.settings.model,
            "stream": stream,
            "messages": messages,
        });
        // 链式下标赋值只在此处有 Some 时创建 options 子对象
        if let Some(temperature) = self.settings.temperature {
            body["options"]["temperature"] = json!(temperature);
        }
        if let Some(top_p) = self.settings.top_p {
            body["options"]["top_p"] = json!(top_p);
        }
        if let Some(num_predict) = self.settings.num_predict {
            body["options"]["num_predict"] = json!(num_predict);
        }
        body.to_string()
    }

    /// 非流式补全:commit message 这类「要一行结果」的场景用,不占流式
    /// 通道。响应是单个 JSON,取 `message.content`。
    pub fn complete_sync(&self, prompt: &str) -> Result<String, AiError> {
        let res = self.post(prompt, false)?;
        let body = res
            .into_body()
            .read_to_string()
            .map_err(|err| AiError::Http(err.to_string()))?;
        message_content(&body).ok_or_else(|| AiError::Http("响应里没有 message.content".to_owned()))
    }

    fn post(
        &self,
        prompt: &str,
        stream: bool,
    ) -> Result<ureq::http::Response<ureq::Body>, AiError> {
        self.agent
            .post(&self.url())
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

        // 非流式配置:整份响应是单个 JSON,一次性解析成正文块 + 结束块
        if !self.settings.stream {
            let body = res
                .into_body()
                .read_to_string()
                .map_err(|err| AiError::Http(err.to_string()))?;
            let text = message_content(&body)
                .ok_or_else(|| AiError::Http("响应里没有 message.content".to_owned()))?;
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
            for chunk in parse_ollama_ndjson(&line) {
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
            return Err(AiError::Http("流在 done 行之前中断".to_owned()));
        }
        Ok(())
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AiProvider for OllamaProvider {
    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()> {
        let this = self.clone();
        let prompt = prompt.to_owned();
        thread::Builder::new()
            .name("latermd-ai-ollama".into())
            .spawn(move || this.run(&prompt, &tx))
            .expect("spawn latermd-ai ollama worker")
    }
}

// 下面两个助手与 openai.rs/anthropic.rs 的同名私有实现保持一致;三处
// 刻意不共用,上提到 crate 级须同轮改两个既有文件(超出本模块路径,
// 留待后续接线模块一并做)。

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
    fn ndjson_standard_sample_parses_deltas_and_done() {
        let sample = "{\"model\":\"llama3.1\",\"created_at\":\"2026-10-04T00:00:00Z\",\"message\":{\"role\":\"assistant\",\"content\":\"你好\"},\"done\":false}\n\
                      {\"model\":\"llama3.1\",\"created_at\":\"2026-10-04T00:00:01Z\",\"message\":{\"role\":\"assistant\",\"content\":\",世界\"},\"done\":false}\n\
                      {\"model\":\"llama3.1\",\"created_at\":\"2026-10-04T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"done_reason\":\"stop\",\"total_duration\":1234,\"eval_count\":12}\n";
        assert_eq!(
            parse_ollama_ndjson(sample),
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

    /// 坏行/空行/缺 message/content null/非字符串 content/空 content 全部
    /// 跳过,不出块;好行照常解析。
    #[test]
    fn ndjson_bad_and_blank_lines_are_skipped() {
        let sample = "\n\
                      不是 JSON\n\
                      {\"model\":\"llama3.1\"}\n\
                      {\"message\":{}}\n\
                      {\"message\":{\"content\":null},\"done\":false}\n\
                      {\"message\":{\"content\":42},\"done\":false}\n\
                      {\"message\":{\"content\":\"\"},\"done\":false}\n\
                      \n\
                      {\"message\":{\"content\":\"ok\"},\"done\":false}\n";
        assert_eq!(
            parse_ollama_ndjson(sample),
            vec![Chunk {
                delta: "ok".to_owned(),
                done: false
            }]
        );
    }

    /// `done: true` 的行只产终止块(终行即使带 content 也不算正文),
    /// 其后内容全部忽略。
    #[test]
    fn ndjson_done_terminates_and_rest_is_ignored() {
        let sample = "{\"message\":{\"content\":\"前半\"},\"done\":false}\n\
                      {\"message\":{\"content\":\"终行尾巴\"},\"done\":true,\"done_reason\":\"stop\"}\n\
                      {\"message\":{\"content\":\"不该出现\"},\"done\":false}\n";
        assert_eq!(
            parse_ollama_ndjson(sample),
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

    /// 顶层 `error` 字段产出失败块(delta 非空 + done=true,见 Chunk 约定)
    /// 且其后内容被忽略;`error: null` 不算错误。
    #[test]
    fn ndjson_error_line_yields_failed_chunk_and_stops() {
        let sample = "{\"message\":{\"content\":\"前半\"},\"done\":false}\n\
                      {\"error\":\"model 'llama3.1' not found, try pulling it first\"}\n\
                      {\"message\":{\"content\":\"不该出现\"},\"done\":false}\n";
        assert_eq!(
            parse_ollama_ndjson(sample),
            vec![
                Chunk {
                    delta: "前半".to_owned(),
                    done: false
                },
                Chunk {
                    delta: "Ollama 流失败:model 'llama3.1' not found, try pulling it first"
                        .to_owned(),
                    done: true
                },
            ]
        );

        // error 为 null 的行不当错误,继续走 content 提取
        let null_error = "{\"error\":null,\"message\":{\"content\":\"ok\"},\"done\":false}\n";
        assert_eq!(
            parse_ollama_ndjson(null_error),
            vec![Chunk {
                delta: "ok".to_owned(),
                done: false
            }]
        );

        // error 字段不是字符串:退固定文案
        let odd = "{\"error\":404}\n";
        assert_eq!(
            parse_ollama_ndjson(odd),
            vec![Chunk {
                delta: "Ollama 流失败:未知错误".to_owned(),
                done: true
            }]
        );
    }

    #[test]
    fn ndjson_handles_crlf_and_empty_text_yields_no_chunks() {
        let sample = "{\"message\":{\"content\":\"A\"},\"done\":false}\r\n\
                      {\"message\":{\"content\":\"\"},\"done\":true}\r\n";
        assert_eq!(
            parse_ollama_ndjson(sample),
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
        assert!(parse_ollama_ndjson("").is_empty());
        assert!(parse_ollama_ndjson("\r\n\n").is_empty());
    }

    #[test]
    fn request_body_is_ollama_chat_stream() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.url(), "http://127.0.0.1:11434/api/chat");
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", true)).unwrap();
        assert_eq!(value["model"], "llama3.1");
        assert_eq!(value["stream"], true);
        assert_eq!(value["messages"][0]["role"], "user");
        assert_eq!(value["messages"][0]["content"], "续写");
        // 默认不发采样参数:options 子对象整个不出现
        assert!(value.get("options").is_none());
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
    }

    /// 自定义端点/模型/采样参数/system 全部进请求体;采样参数只进
    /// `options` 子对象,`None` 的字段不发。
    #[test]
    fn request_body_carries_settings() {
        let provider = OllamaProvider::with_settings(OllamaSettings {
            base_url: "http://192.168.1.10:11434/".to_owned(),
            model: "qwen2.5:14b".to_owned(),
            temperature: Some(0.5),
            top_p: None,
            num_predict: Some(-1),
            system_prompt: "你是技术文档助手".to_owned(),
            stream: false,
            timeout_secs: 300,
        });
        assert_eq!(
            provider.url(),
            "http://192.168.1.10:11434/api/chat",
            "尾斜杠被去掉,不拼出双斜杠"
        );
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", false)).unwrap();
        assert_eq!(value["model"], "qwen2.5:14b");
        assert_eq!(value["stream"], false);
        let messages = value["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "你是技术文档助手");
        assert_eq!(messages[1]["role"], "user");
        // 采样参数进 options 子对象而非顶层;None 的 top_p 不发字段
        // f32 → JSON 有精度误差,按容差断言
        assert!((value["options"]["temperature"].as_f64().unwrap() - 0.5).abs() < 1e-3);
        assert!(value["options"].get("top_p").is_none());
        assert_eq!(value["options"]["num_predict"], -1, "负值原样透传(-1 不限)");
        // 顶层不出现裸的采样参数
        assert!(value.get("temperature").is_none());
        assert!(value.get("num_predict").is_none());
    }

    /// 空白 system prompt 不产生 system 消息(空白也算「没写」);
    /// `complete_sync` 的请求体恒为非流式。
    #[test]
    fn blank_system_prompt_is_omitted_and_complete_sync_forces_non_stream() {
        let provider = OllamaProvider::with_settings(OllamaSettings {
            system_prompt: "   \n  ".to_owned(),
            stream: true,
            ..OllamaSettings::default()
        });
        let value: serde_json::Value =
            serde_json::from_str(&provider.request_body("续写", false)).unwrap();
        assert_eq!(value["messages"].as_array().unwrap().len(), 1);
        assert_eq!(value["stream"], false, "complete_sync 恒非流式");
    }

    /// 非流式响应的正文提取:有 content 取值、空串/缺字段/坏 JSON 都 None。
    #[test]
    fn message_content_extraction() {
        assert_eq!(
            message_content(
                "{\"model\":\"llama3.1\",\"message\":{\"role\":\"assistant\",\"content\":\"docs: 新增 README\"},\"done\":true}"
            )
            .as_deref(),
            Some("docs: 新增 README")
        );
        assert_eq!(message_content("{\"message\":{\"content\":\"\"}}"), None);
        assert_eq!(message_content("{\"message\":{}}"), None);
        assert_eq!(message_content("{}"), None);
        assert_eq!(message_content("不是 JSON"), None);
    }

    #[test]
    fn default_settings_match_documented_defaults() {
        let settings = OllamaProvider::new().settings().clone();
        assert_eq!(settings.base_url, "http://127.0.0.1:11434");
        assert_eq!(settings.model, "llama3.1");
        assert_eq!(settings.temperature, None);
        assert_eq!(settings.top_p, None);
        assert_eq!(settings.num_predict, None);
        assert_eq!(settings.system_prompt, "");
        assert!(settings.stream);
        assert_eq!(settings.timeout_secs, 120);
    }
}
