//! AI provider 配置(docs/ui-polish.md §6「AI」页)。
//!
//! P1 把 provider 定死成 MockProvider、端点与模型走环境变量
//! (decisions-pending #3/#9)。本模块把它们变成**用户可在设置页填写的表单**:
//! provider 种类、接口方式、Base URL、模型名、采样参数、system prompt、
//! 超时与流式开关,落 `ai.json`。
//!
//! **API key 不在这里**:key 只走系统凭据(`latermd-creds`,见
//! `crate::ai_key`),与参数分开存 —— 参数可以备份/分享,key 不行。
//!
//! 三类 provider 中只有「OpenAI 兼容」已实现,其余在 UI 里显式标注禁用,
//! 不伪装可用(与 decisions-pending #12 同口径:能做什么就写什么)。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 落盘文件名(平台配置目录,与 `settings.json` 同级)。
const AI_FILE: &str = "ai.json";

/// provider 种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// 内置演示 provider:不联网、按脚本吐块,用于走通链路。
    #[default]
    Mock,
    /// OpenAI 兼容端点(`/chat/completions` + SSE):官方、DeepSeek、通义、
    /// 本地 vLLM/Ollama 的兼容层等都算这一类。
    OpenAiCompatible,
}

impl ProviderKind {
    /// 下拉显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Mock => "内置 Mock(不联网)",
            Self::OpenAiCompatible => "OpenAI 兼容端点",
        }
    }

    /// 全部可选项(下拉顺序)。
    pub const ALL: [ProviderKind; 2] = [Self::Mock, Self::OpenAiCompatible];

    /// 是否需要 API key(驱动 `AiState::provider_requires_key`)。
    pub fn requires_key(self) -> bool {
        match self {
            Self::Mock => false,
            Self::OpenAiCompatible => true,
        }
    }
}

/// 接口方式(请求/响应的协议形态)。
///
/// 只有 [`ApiStyle::ChatCompletions`] 已实现;另外两种在设置页里选中即提示
/// 「尚未实现」并拒绝保存为该值,避免用户配完发现没生效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiStyle {
    /// OpenAI `/chat/completions`,SSE 增量。
    #[default]
    ChatCompletions,
    /// Anthropic `/v1/messages`,SSE 增量。**未实现**。
    AnthropicMessages,
    /// Ollama `/api/generate`,NDJSON 流。**未实现**。
    OllamaGenerate,
}

impl ApiStyle {
    /// 下拉显示名;未实现的在名字里直接写明,不用等到保存才报错。
    pub fn label(self) -> &'static str {
        match self {
            Self::ChatCompletions => "OpenAI /chat/completions(SSE)",
            Self::AnthropicMessages => "Anthropic /v1/messages(未实现)",
            Self::OllamaGenerate => "Ollama /api/generate(未实现)",
        }
    }

    pub const ALL: [ApiStyle; 3] = [
        Self::ChatCompletions,
        Self::AnthropicMessages,
        Self::OllamaGenerate,
    ];

    /// 是否已有实现。
    pub fn implemented(self) -> bool {
        matches!(self, Self::ChatCompletions)
    }
}

/// 模型参数与端点配置。缺省字段回落默认,手改的配置文件缺项不致整体失效。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// provider 种类。
    pub provider: ProviderKind,
    /// 接口方式。
    pub api_style: ApiStyle,
    /// 端点根地址(自动去掉结尾 `/`)。
    pub base_url: String,
    /// 模型名。
    pub model: String,
    /// 采样温度 0–2。
    pub temperature: f32,
    /// 核采样 0–1。
    pub top_p: f32,
    /// 单次回复的 token 上限。
    pub max_tokens: u32,
    /// system prompt;空串 = 不发送 system 消息。
    pub system_prompt: String,
    /// 连接/响应超时(秒)。
    pub timeout_secs: u64,
    /// 是否流式接收(关闭则一次性拿全文 —— 请求侧仍走同一 provider 通道)。
    pub stream: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::Mock,
            api_style: ApiStyle::ChatCompletions,
            base_url: "https://api.openai.com/v1".to_owned(),
            model: "gpt-4o-mini".to_owned(),
            temperature: 0.7,
            top_p: 1.0,
            max_tokens: 2048,
            system_prompt: String::new(),
            timeout_secs: 60,
            stream: true,
        }
    }
}

impl AiConfig {
    /// 端点根地址(去掉结尾斜杠,避免拼出 `//chat/completions`)。
    pub fn base_url_trimmed(&self) -> &str {
        self.base_url.trim().trim_end_matches('/')
    }

    /// 是否真的会联网:Mock 不需要,其余需要(设置页据此说明)。
    pub fn connects_network(&self) -> bool {
        self.provider.requires_key()
    }

    /// 归一化:端点/模型去空白,空值回落默认;采样参数钳到合法区间
    /// (手改 JSON 可能写出 5.0 或 -1)。
    pub fn normalize(&mut self) {
        self.base_url = self.base_url.trim().to_owned();
        self.model = self.model.trim().to_owned();
        if self.base_url.is_empty() {
            self.base_url = Self::default().base_url;
        }
        if self.model.is_empty() {
            self.model = Self::default().model;
        }
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.top_p = self.top_p.clamp(0.0, 1.0);
        self.max_tokens = self.max_tokens.clamp(256, 32_768);
        self.timeout_secs = self.timeout_secs.clamp(10, 300);
        if !self.api_style.implemented() {
            // 未实现的接口方式一律回落到已实现的那一种(配置页不允许选)
            self.api_style = ApiStyle::ChatCompletions;
        }
    }

    /// 从目录读取;文件缺失或解析失败 = 默认(坏配置不挡启动,与
    /// `theme::ThemeSettings::load` 同口径)。
    pub fn load_from(dir: &Path) -> Self {
        let path = dir.join(AI_FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        let mut config: Self = match serde_json::from_slice(&bytes) {
            Ok(config) => config,
            Err(source) => {
                eprintln!("LaterMD: AI 配置解析失败,已回落默认: {source}");
                Self::default()
            }
        };
        config.normalize();
        config
    }

    /// 落盘。目录不存在则创建。
    pub fn save_to(&self, dir: &Path) -> Result<(), String> {
        let path = dir.join(AI_FILE);
        let json = serde_json::to_string_pretty(self)
            .map_err(|source| format!("{}: {source}", path.display()))?;
        std::fs::create_dir_all(dir).map_err(|source| format!("{}: {source}", dir.display()))?;
        std::fs::write(&path, json.as_bytes())
            .map_err(|source| format!("{}: {source}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("latermd-aicfg-{}-{name}", std::process::id()))
    }

    /// 往返:改过的字段逐项保持(含枚举的 snake_case 名)。
    #[test]
    fn save_load_round_trip() {
        let dir = temp_dir("roundtrip");
        let config = AiConfig {
            provider: ProviderKind::OpenAiCompatible,
            api_style: ApiStyle::ChatCompletions,
            base_url: "https://open.bigmodel.cn/api/paas/v4/".to_owned(),
            model: "glm-4.6".to_owned(),
            temperature: 1.2,
            top_p: 0.85,
            max_tokens: 4096,
            system_prompt: "你是技术文档助手".to_owned(),
            timeout_secs: 120,
            stream: false,
        };
        config.save_to(&dir).unwrap();
        assert_eq!(AiConfig::load_from(&dir), config);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 归一化:空白端点/模型回落默认;越界采样参数被钳住;未实现的接口
    /// 方式回落到 SSE(手改 JSON 的常见脏数据)。
    #[test]
    fn normalize_clamps_and_falls_back() {
        let mut config = AiConfig {
            base_url: "  ".to_owned(),
            model: String::new(),
            temperature: 9.9,
            top_p: -0.5,
            max_tokens: 0,
            timeout_secs: 1,
            api_style: ApiStyle::OllamaGenerate,
            ..AiConfig::default()
        };
        config.normalize();
        let defaults = AiConfig::default();
        assert_eq!(config.base_url, defaults.base_url);
        assert_eq!(config.model, defaults.model);
        assert_eq!(config.temperature, 2.0);
        assert_eq!(config.top_p, 0.0);
        assert_eq!(config.max_tokens, 256);
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.api_style, ApiStyle::ChatCompletions);
    }

    /// 端点尾斜杠:显示保留用户输入,取用时才去(`base_url_trimmed`)。
    #[test]
    fn base_url_trailing_slash_is_trimmed_on_use() {
        let config = AiConfig {
            base_url: "https://api.openai.com/v1//".to_owned(),
            ..AiConfig::default()
        };
        assert_eq!(config.base_url_trimmed(), "https://api.openai.com/v1");
    }

    /// 坏 JSON 回落默认且不 panic。
    #[test]
    fn corrupt_json_falls_back_to_default() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(AI_FILE), b"{oops").unwrap();
        assert_eq!(AiConfig::load_from(&dir), AiConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// provider 语义:Mock 不联网也不要 key;OpenAI 兼容两者都要。
    #[test]
    fn provider_semantics_match_requirements() {
        assert!(!ProviderKind::Mock.requires_key());
        assert!(!AiConfig::default().connects_network());
        assert!(ProviderKind::OpenAiCompatible.requires_key());
        assert!(AiConfig {
            provider: ProviderKind::OpenAiCompatible,
            ..AiConfig::default()
        }
        .connects_network());
    }
}
