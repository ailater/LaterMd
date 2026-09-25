//! AI 流式写作的应用侧接线(P1「AI 流式写作」)。
//!
//! 后台线程由 `latermd-ai` 的 provider 自带(`AiProvider::stream_complete`
//! 返回线程句柄),本模块只持有接收端:每帧 [`AiState::poll`] 非阻塞收空
//! channel,chunk 翻成 [`crate::state::Message`] 走归约 —— 与搜索
//! (`crate::search`)同款「发起 / 接收 / 收尾」三原语,但文本回编辑器必须
//! 经 Message(铁律三:AI 修改在文本/AST 层,后台线程不触碰 UI 状态)。
//!
//! 不引入 tokio:100ms/chunk 的节奏下 `std::sync::mpsc` + 每帧
//! `request_repaint` 足够(roadmap 阶段 1 附加验证 6 的既定结论)。
//!
//! **provider 可切换**(docs/ui-polish.md §6):[`AiRuntime`] 在 Mock 与
//! OpenAI 兼容端点之间二选一,由 [`AiConfig`] + API key 现场装配,保存配置
//! 即时生效、无需重启。

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;

use latermd_ai::{AiProvider, Chunk, MockProvider, OpenAiProvider, OpenAiSettings};

use crate::ai_config::AiConfig;
use crate::state::Message;

/// 生效的 provider 运行时。
///
/// 枚举而非 trait 对象:只有两种实现,编译期穷尽匹配比动态分发更省事,
/// 也让「Mock 的同步合成」这类独有方法不必塞进公共 trait。
pub enum AiRuntime {
    /// 内置演示 provider:不联网。
    Mock(MockProvider),
    /// OpenAI 兼容端点(参数见 [`OpenAiSettings`])。
    OpenAi(OpenAiProvider),
}

impl AiRuntime {
    /// 状态栏 / 设置页显示名。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Mock(_) => "Mock",
            Self::OpenAi(_) => "OpenAI 兼容",
        }
    }

    fn stream_complete(&self, prompt: &str, tx: Sender<Chunk>) -> JoinHandle<()> {
        match self {
            Self::Mock(provider) => provider.stream_complete(prompt, tx),
            Self::OpenAi(provider) => provider.stream_complete(prompt, tx),
        }
    }

    /// 同步生成 commit subject(commit 建议是「一行结果」,流式对它无意义)。
    ///
    /// Mock 走关键词合成;真实端点走一次非流式请求并取首行(模型偶尔会
    /// 带解释性前缀行,首行即 subject 的约定在 prompt 里已写明)。
    pub fn commit_subject(&self, prompt: &str) -> Result<String, String> {
        match self {
            Self::Mock(provider) => Ok(provider.mock_commit_subject(prompt)),
            Self::OpenAi(provider) => provider
                .complete_sync(prompt)
                .map(|text| first_line(&text).to_owned())
                .map_err(|error| format!("AI 请求失败:{error}")),
        }
    }
}

/// 正文首行(去空白;空正文给空串)。
fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("").trim()
}

/// AI 流式状态:运行时 + 当前流的接收端 + 进行中标志。
///
/// `streaming` 是普通 bool 而非 AtomicBool:发起与收尾都只发生在 UI 线程
/// 的归约里(单线程访问),原子性无从谈起;它存在的意义是防重入 —— 流式
/// 进行中再次触发命令在 [`AiState::start`] 入口被忽略。
pub struct AiState {
    /// 生效的 provider;`pub(crate)` 仅为测试注入零间隔 provider。
    pub(crate) runtime: AiRuntime,
    /// 当前流的接收端;`finish` 时 drop,发送端下一次 send 失败静默收尾。
    pub(crate) rx: Option<Receiver<Chunk>>,
    /// 是否有流在途(防重入标志,见类型文档)。
    pub(crate) streaming: bool,
    /// 最近一次真实发起的 prompt 原文(AI 指令卡的状态匹配键):
    /// 指令文本与它相等 → 流式中「进行中」/结束「已完成」,否则「未执行」。
    /// 只增不清是刻意的:`finish` 后保留才能显示「已完成」,失效时机只有
    /// 两个——流失败([`crate::state::Message::AiFailed`] 归约里清)与换文档
    /// ([`crate::state::State::load_document`] 里清),卡片是文档的派生物。
    pub(crate) last_prompt: Option<String>,
    /// 当前生效的 AI 配置(端点/模型/采样参数……)。与 `runtime` 同源:
    /// 保存配置即重新装配 runtime,两者不会各说各话。
    pub(crate) config: AiConfig,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            runtime: AiRuntime::Mock(MockProvider::new()),
            rx: None,
            streaming: false,
            last_prompt: None,
            config: AiConfig::default(),
        }
    }
}

impl AiState {
    /// 按配置 + key 装配运行时。无 key 的 OpenAI 兼容端点照样装配(请求会
    /// 失败并由命令闸门拦在前面 —— `requires_key` 为真而凭据为空时,四条
    /// AI 命令入口先落「未配置 key」提示,见 `State::ai_key_gate`)。
    pub fn set_provider(&mut self, config: AiConfig, api_key: Option<&str>) {
        self.runtime = if config.provider.requires_key() {
            let settings = OpenAiSettings {
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                temperature: Some(config.temperature),
                top_p: Some(config.top_p),
                max_tokens: Some(config.max_tokens),
                system_prompt: config.system_prompt.clone(),
                stream: config.stream,
                timeout_secs: config.timeout_secs,
            };
            AiRuntime::OpenAi(OpenAiProvider::with_settings(
                api_key.unwrap_or_default(),
                settings,
            ))
        } else {
            AiRuntime::Mock(MockProvider::new())
        };
        self.config = config;
    }

    /// 当前 provider 是否需要 key 才能发起命令。
    ///
    /// 取自**配置**而非运行时:网关判定与「实际用哪个 provider 发请求」
    /// 是两件事 —— 配置说要 key 就是「这条命令必须带 key」,即使测试把
    /// 运行时换成 Mock 也仍按配置拦截(不发出真实请求就能测闸门)。
    pub fn requires_key(&self) -> bool {
        self.config.provider.requires_key()
    }

    /// 状态栏显示名(Mock / OpenAI 兼容)。
    pub fn provider_label(&self) -> &'static str {
        self.runtime.label()
    }

    /// 发起一次流式续写。已在流式中则忽略(防重入),返回是否真的发起了。
    ///
    /// provider 在自己的后台线程里按节奏发块,本方法同步返回;channel 无界,
    /// UI 侧只在每帧归约里 `try_recv`,永不阻塞。
    pub fn start(&mut self, prompt: &str) -> bool {
        if self.streaming {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        let worker: JoinHandle<()> = self.runtime.stream_complete(prompt, tx);
        // 不 join:接收端在 `finish` 时 drop,worker 下一次 send 失败即退出
        // (latermd-ai 的取消约定);与搜索服务同款,不滞留句柄。
        drop(worker);
        self.rx = Some(rx);
        self.streaming = true;
        self.last_prompt = Some(prompt.to_owned());
        true
    }

    /// 非阻塞收空 channel,chunk 翻成消息。成功结束发 [`Message::AiDone`];
    /// 失败块(provider 契约:`done` 且 `delta` 非空)发 [`Message::AiFailed`]。
    /// `streaming` 标志与接收端不清在这里:生命周期收口在归约侧的
    /// `finish`(状态变更只发生在 `apply`)。
    ///
    /// channel 断开却没等到收尾块(worker 异常退出)也按失败收尾:流式
    /// 标志绝不能卡死,否则后续触发会被防重入永远拦下。
    pub fn poll(&mut self) -> Vec<Message> {
        let Some(rx) = self.rx.as_ref() else {
            return Vec::new();
        };
        let mut messages = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(chunk) if !chunk.done => messages.push(Message::AiChunk { delta: chunk.delta }),
                Ok(chunk) => {
                    if chunk.delta.is_empty() {
                        messages.push(Message::AiDone);
                    } else {
                        messages.push(Message::AiFailed(chunk.delta));
                    }
                    break; // 结束块是流的最后一条
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    let terminated = messages
                        .iter()
                        .any(|m| matches!(m, Message::AiDone | Message::AiFailed(_)));
                    if !terminated {
                        messages.push(Message::AiFailed("AI 流意外中断".into()));
                    }
                    break;
                }
            }
        }
        messages
    }

    /// 收尾(归约侧 [`Message::AiDone`] / [`Message::AiFailed`] 调用):
    /// 清进行中标志并丢弃接收端(及其内积压事件),允许下一次发起。
    pub fn finish(&mut self) {
        self.rx = None;
        self.streaming = false;
    }

    /// 是否有流在途(驱动流式期间持续重绘)。
    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// 清掉「最近一次发起的 prompt」(指令卡状态随之回到未执行)。只在两个
    /// 失效时机调用:流失败归约、换文档(`State::load_document`);`finish`
    /// 本身不清,否则流刚结束「已完成」就不可见。
    pub(crate) fn forget_last_prompt(&mut self) {
        self.last_prompt = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_config::ProviderKind;
    use std::time::Duration;

    /// 把消息序列里的正文块拼起来,便于断言。
    fn deltas(messages: &[Message]) -> Vec<String> {
        messages
            .iter()
            .filter_map(|m| match m {
                Message::AiChunk { delta } => Some(delta.clone()),
                _ => None,
            })
            .collect()
    }

    /// 完整生命周期:start 后 poll 收到正文块,自然结束收到 AiDone;
    /// 正文块拼接无损、非空,且没有失败块。用零间隔 provider 保持测试快速
    /// (默认 100ms/块 × 30-50 块是联调节奏,不是测试节奏)。
    #[test]
    fn start_poll_yields_chunks_then_done() {
        let mut ai = AiState {
            runtime: AiRuntime::Mock(MockProvider::with_interval(Duration::ZERO)),
            ..AiState::default()
        };
        assert!(ai.start("帮我续写这段设计文档"));
        assert!(ai.is_streaming());

        let mut messages = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            messages.extend(ai.poll());
            // 收到结束块即停(is_streaming 要等归约侧 finish 才清,不能当循环条件)
            if messages
                .iter()
                .any(|m| matches!(m, Message::AiDone | Message::AiFailed(_)))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }

        assert!(
            messages.iter().any(|m| matches!(m, Message::AiDone)),
            "自然结束发 AiDone"
        );
        assert!(!deltas(&messages).concat().is_empty(), "正文块非空");
        assert!(
            messages.iter().all(|m| !matches!(m, Message::AiFailed(_))),
            "成功流无失败块"
        );
    }

    /// 防重入:流式进行中再次 start 被忽略(返回 false,不产生第二个流)。
    #[test]
    fn start_while_streaming_is_ignored() {
        let mut ai = AiState::default();
        assert!(ai.start("随便写点什么"));
        assert!(!ai.start("第二次触发"), "流式中再触发被忽略");
        // 收尾后可再次发起
        ai.finish();
        assert!(ai.start("收尾后重新发起"));
    }

    /// 失败契约:done 且 delta 非空的块翻成 AiFailed(人工注入 channel,
    /// 不经 provider —— Mock 不产生失败块,OpenAI adapter 的失败路径在此收口)。
    #[test]
    fn failure_chunk_maps_to_ai_failed() {
        let mut ai = AiState {
            runtime: AiRuntime::Mock(MockProvider::new()),
            rx: None,
            streaming: true,
            last_prompt: None,
            config: AiConfig::default(),
        };
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk {
            delta: "额度用尽".into(),
            done: true,
        })
        .unwrap();
        ai.rx = Some(rx);

        assert_eq!(ai.poll(), vec![Message::AiFailed("额度用尽".into())]);
    }

    /// 断连兜底:发送端已撤而收尾块未到(worker 异常退出)→ 补发 AiFailed,
    /// 流式标志不卡死;正常收到过收尾块则不重复报错。
    #[test]
    fn disconnected_channel_without_done_becomes_ai_failed() {
        let mut ai = AiState {
            runtime: AiRuntime::Mock(MockProvider::new()),
            rx: None,
            streaming: true,
            last_prompt: None,
            config: AiConfig::default(),
        };
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk {
            delta: "半截".into(),
            done: false,
        })
        .unwrap();
        drop(tx);
        ai.rx = Some(rx);
        assert_eq!(
            ai.poll(),
            vec![
                Message::AiChunk {
                    delta: "半截".into()
                },
                Message::AiFailed("AI 流意外中断".into()),
            ]
        );

        // 已有收尾块在前:断连不再追加第二条失败
        let (tx, rx) = mpsc::channel();
        tx.send(Chunk {
            delta: String::new(),
            done: true,
        })
        .unwrap();
        drop(tx);
        ai.rx = Some(rx);
        assert_eq!(ai.poll(), vec![Message::AiDone]);
    }

    /// poll 不消费结束块之后的生命周期:标志保持到归约侧 finish;空闲 poll
    /// (无 channel)返回空。
    #[test]
    fn poll_without_channel_is_empty_and_finish_clears_state() {
        let mut ai = AiState::default();
        assert!(ai.poll().is_empty());
        ai.start("x");
        ai.finish();
        assert!(!ai.is_streaming());
        assert!(ai.poll().is_empty(), "finish 丢弃接收端,不再产出消息");
    }

    /// 运行时切换:Mock 不需要 key;OpenAI 兼容需要 key,且配置被记进
    /// `config`(闸门与状态栏显示名随之变化)。
    #[test]
    fn set_provider_switches_runtime_and_key_requirement() {
        let mut ai = AiState::default();
        assert!(!ai.requires_key());
        assert_eq!(ai.provider_label(), "Mock");

        let config = AiConfig {
            provider: ProviderKind::OpenAiCompatible,
            model: "deepseek-chat".to_owned(),
            base_url: "https://api.deepseek.com/v1".to_owned(),
            ..AiConfig::default()
        };
        ai.set_provider(config.clone(), Some("placeholder-key"));
        assert!(ai.requires_key());
        assert_eq!(ai.provider_label(), "OpenAI 兼容");
        assert_eq!(ai.config, config, "配置跟着运行时一起换");

        // 切回 Mock:闸门随之打开
        ai.set_provider(AiConfig::default(), None);
        assert!(!ai.requires_key());
    }

    /// 首行提取:commit subject 只取第一行(模型常带解释性后续行)。
    #[test]
    fn first_line_takes_only_the_subject() {
        assert_eq!(
            first_line("docs: 新增 README\n\n说明……"),
            "docs: 新增 README"
        );
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("  \n x"), "");
    }
}
