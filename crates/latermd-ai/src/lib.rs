//! AI provider 流式契约(业务逻辑层)。
//!
//! 三条铁律在本 crate 的落点:本 crate **禁止依赖 egui/eframe**,只产出
//! [`Chunk`] 流;文本回编辑器由 `latermd-app` 走 Message 归约,后台线程
//! 不触碰任何 UI 状态。
//!
//! 流式契约:provider 在后台线程经 `std::sync::mpsc` 发 [`Chunk`],
//! 不引入 tokio。失败约定见 [`Chunk`] 文档;取消约定:接收端 drop 掉
//! `Sender` 的对端后,发送端在下一次 `send` 失败时静默收尾。

#![forbid(unsafe_code)]

mod commit;
mod mock;
mod openai;

pub use commit::{commit_message_prompt, truncate_diff};
pub use mock::MockProvider;
pub use openai::{parse_openai_sse, AiError, OpenAiProvider, API_KEY_ENV};

use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

/// 流式输出的最小单位。
///
/// 约定:正文只出现在 `done == false` 的块里;`done == true` 且 `delta`
/// 为空表示成功结束;`done == true` 且 `delta` 非空表示流失败,`delta`
/// 是面向用户的错误描述(不写入文档)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub delta: String,
    pub done: bool,
}

/// 流式补全契约。
///
/// 实现方在独立后台线程里经 `tx` 发 [`Chunk`],返回线程句柄供调用方
/// join;prompt 按原样透传,截断与拼装是调用方(app 层)的职责。
pub trait AiProvider: Send + Sync {
    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()>;
}

/// 从环境变量 `LATERMD_AI_API_KEY` 读取 API key;未设置或空白返回 `None`。
/// 代码不硬编码任何 key(decisions-pending #3)。
pub fn read_api_key() -> Option<String> {
    std::env::var(API_KEY_ENV)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
