//! AI provider 配置(docs/ui-polish.md §6「AI」页)。
//!
//! P1 把 provider 定死成 MockProvider、端点与模型走环境变量
//! (decisions-pending #3/#9)。本模块把它们变成**用户可在设置页填写的表单**:
//! provider 种类、Base URL、模型名、采样参数、system prompt、超时与流式
//! 开关,落 `ai.json`。
//!
//! **provider 是唯一开关**(decisions-pending #94):接口方式(协议形态)
//! 随 provider 派生,不再单列字段 —— 四种 provider 各自钉死一种协议,
//! 「OpenAI 端点 + Anthropic 协议」这类矛盾组合在配置层就不存在。
//!
//! **API key 不在这里**:key 只走系统凭据(`latermd-creds`,见
//! `crate::ai_key`),与参数分开存 —— 参数可以备份/分享,key 不行。
//! 四种 provider 全部已实现:OpenAI 兼容/Anthropic 需要 key(共用同一
//! 凭据通道),Ollama 本地无鉴权,Mock 不联网。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 落盘文件名(平台配置目录,与 `settings.json` 同级)。
const AI_FILE: &str = "ai.json";

/// provider 种类(serde snake_case)。旧 `ai.json` 的 `mock` 与
/// `open_ai_compatible`(旧版落盘名)原样可读;`openai_compatible` 也收
/// (alias,手改配置的常见拼写)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// 内置演示 provider:不联网、按脚本吐块,用于走通链路。
    #[default]
    Mock,
    /// OpenAI 兼容端点(`/chat/completions` + SSE):官方、DeepSeek、通义、
    /// 本地 vLLM/Ollama 的兼容层等都算这一类。
    #[serde(alias = "openai_compatible")]
    OpenAiCompatible,
    /// Anthropic messages API(`/v1/messages` + SSE)。
    Anthropic,
    /// Ollama 本地服务(`/api/chat` + NDJSON),无鉴权。
    Ollama,
}

/// provider 的出厂端点/模型/超时([`ProviderKind::factory`] 的返回)。
pub struct ProviderFactory {
    /// 端点根地址。
    pub base_url: String,
    /// 模型名。
    pub model: String,
    /// 连接/响应超时(秒)。
    pub timeout_secs: u64,
}

impl ProviderKind {
    /// 下拉显示名(带协议形态,取代原「接口方式」下拉承载的信息)。
    pub fn label(self) -> &'static str {
        match self {
            Self::Mock => "内置 Mock(不联网)",
            Self::OpenAiCompatible => "OpenAI 兼容(/chat/completions)",
            Self::Anthropic => "Anthropic(/v1/messages)",
            Self::Ollama => "Ollama 本地(/api/chat)",
        }
    }

    /// 设置页的说明行:key 需求与端点性质一句话讲清(不再有「未实现」项)。
    pub fn description(self) -> &'static str {
        match self {
            Self::Mock => "不联网的内置演示,按脚本吐块用于走通链路;下方参数不参与。",
            Self::OpenAiCompatible => "官方、DeepSeek、通义、本地 vLLM 等兼容端点;需要 API key。",
            Self::Anthropic => {
                "Claude 官方 messages API;需要 API key(与 OpenAI 兼容共用同一凭据)。"
            }
            Self::Ollama => "本机或局域网的 Ollama 服务;默认 http://127.0.0.1:11434,无需 API key。",
        }
    }

    /// 全部可选项(下拉顺序)。
    pub const ALL: [ProviderKind; 4] = [
        Self::Mock,
        Self::OpenAiCompatible,
        Self::Anthropic,
        Self::Ollama,
    ];

    /// 是否需要 API key(驱动 `AiState::provider_requires_key` 与命令闸门)。
    /// OpenAI 兼容与 Anthropic 共用同一 key 通道(decisions-pending #3);
    /// Ollama 本地无鉴权。
    pub fn requires_key(self) -> bool {
        match self {
            Self::Mock | Self::Ollama => false,
            Self::OpenAiCompatible | Self::Anthropic => true,
        }
    }

    /// 是否消费表单参数(端点/模型/采样):只有 Mock 全不读、参数区灰显;
    /// Ollama 连本机服务,参数照常参与。
    pub fn uses_settings(self) -> bool {
        !matches!(self, Self::Mock)
    }

    /// 出厂参数:与对应 adapter 的 `Default` 实现同源,app 侧不另抄一份
    /// (防两处漂移)。Mock 无 adapter,沿用 OpenAI 兼容的值 —— 参数不
    /// 参与请求,表单只需要一个可显示的缺省。
    pub fn factory(self) -> ProviderFactory {
        let (base_url, model, timeout_secs) = match self {
            Self::Mock | Self::OpenAiCompatible => {
                let s = latermd_ai::OpenAiSettings::default();
                (s.base_url, s.model, s.timeout_secs)
            }
            Self::Anthropic => {
                let s = latermd_ai::AnthropicSettings::default();
                (s.base_url, s.model, s.timeout_secs)
            }
            Self::Ollama => {
                let s = latermd_ai::OllamaSettings::default();
                (s.base_url, s.model, s.timeout_secs)
            }
        };
        ProviderFactory {
            base_url,
            model,
            timeout_secs,
        }
    }
}

/// 模型参数与端点配置。缺省字段回落默认,手改的配置文件缺项不致整体失效。
///
/// 旧版曾有 `api_style` 字段(接口方式);已随「provider 唯一开关」删除,
/// 旧 `ai.json` 里遗留的该键在读取时被忽略(serde 默认不拒绝未知字段)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// provider 种类。
    pub provider: ProviderKind,
    /// 端点根地址(自动去掉结尾 `/`)。
    pub base_url: String,
    /// 模型名。
    pub model: String,
    /// 采样温度 0–2。
    pub temperature: f32,
    /// 核采样 0–1。
    pub top_p: f32,
    /// 单次回复的 token 上限(Ollama 侧映射为 `num_predict`)。
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
        let factory = ProviderKind::Mock.factory();
        Self {
            provider: ProviderKind::Mock,
            base_url: factory.base_url,
            model: factory.model,
            temperature: 0.7,
            top_p: 1.0,
            max_tokens: 2048,
            system_prompt: String::new(),
            timeout_secs: factory.timeout_secs,
            stream: true,
        }
    }
}

impl AiConfig {
    /// 端点根地址(去掉结尾斜杠,避免拼出 `//chat/completions`)。
    pub fn base_url_trimmed(&self) -> &str {
        self.base_url.trim().trim_end_matches('/')
    }

    /// 是否真的会联网(Mock 不联网;Ollama 连的是本机服务,也算联网型 ——
    /// 设置页的端点行要显示)。与 `requires_key` 解耦:Ollama 无 key 也联网。
    pub fn connects_network(&self) -> bool {
        self.provider.uses_settings()
    }

    /// 切换 provider 时的出厂值跟随:端点/模型/超时若仍是**任一** provider
    /// 的出厂值(即用户从未手改),换成新 provider 的出厂值;手改过的字段
    /// 原样保留,绝不静默覆盖。采样参数/system prompt/流式开关与 provider
    /// 无关,不动。设置页 provider 下拉切换时调用。
    pub fn adopt_provider_defaults(&mut self, new: ProviderKind) {
        self.provider = new;
        let factory = new.factory();
        let factories: Vec<_> = ProviderKind::ALL
            .iter()
            .map(|kind| kind.factory())
            .collect();
        if factories
            .iter()
            .any(|f| f.base_url == self.base_url_trimmed())
        {
            self.base_url = factory.base_url;
        }
        if factories.iter().any(|f| f.model == self.model) {
            self.model = factory.model;
        }
        if factories
            .iter()
            .any(|f| f.timeout_secs == self.timeout_secs)
        {
            self.timeout_secs = factory.timeout_secs;
        }
    }

    /// 归一化:端点/模型去空白,空值回落**当前 provider** 的出厂值;采样
    /// 参数钳到合法区间(手改 JSON 可能写出 5.0 或 -1)。provider 本体
    /// 不回落 —— 四种 provider 全部已实现,配置的是什么就是什么。
    pub fn normalize(&mut self) {
        self.base_url = self.base_url.trim().to_owned();
        self.model = self.model.trim().to_owned();
        let factory = self.provider.factory();
        if self.base_url.is_empty() {
            self.base_url = factory.base_url;
        }
        if self.model.is_empty() {
            self.model = factory.model;
        }
        self.temperature = self.temperature.clamp(0.0, 2.0);
        self.top_p = self.top_p.clamp(0.0, 1.0);
        self.max_tokens = self.max_tokens.clamp(256, 32_768);
        self.timeout_secs = self.timeout_secs.clamp(10, 300);
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

    /// 往返:改过的字段逐项保持(含旧变体的 snake_case 名)。
    #[test]
    fn save_load_round_trip() {
        let dir = temp_dir("roundtrip");
        let config = AiConfig {
            provider: ProviderKind::OpenAiCompatible,
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

    /// 往返:新变体(Anthropic/Ollama)逐字段落盘再读回,落盘枚举名是
    /// snake_case(与旧 ai.json 同一约定)。
    #[test]
    fn save_load_round_trip_new_provider_variants() {
        for (provider, base_url, model) in [
            (
                ProviderKind::Anthropic,
                "https://api.anthropic.com",
                "claude-sonnet-4-5",
            ),
            (ProviderKind::Ollama, "http://127.0.0.1:11434", "llama3.1"),
        ] {
            let config = AiConfig {
                provider,
                base_url: base_url.to_owned(),
                model: model.to_owned(),
                timeout_secs: 120,
                ..AiConfig::default()
            };
            let dir = temp_dir("roundtrip-new");
            config.save_to(&dir).unwrap();
            assert_eq!(AiConfig::load_from(&dir), config);
            let json = std::fs::read_to_string(dir.join(AI_FILE)).unwrap();
            let name = match provider {
                ProviderKind::Anthropic => "\"anthropic\"",
                ProviderKind::Ollama => "\"ollama\"",
                _ => unreachable!("只测新变体"),
            };
            assert!(json.contains(name), "落盘应含 {name}:\n{json}");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// 旧版 ai.json 兼容:只含旧变体、还带着已删除的 `api_style` 键
    /// (含曾经的「未实现」取值)也能原样读回,未知键被忽略不报错。
    /// 旧版落盘名是 `open_ai_compatible`,`openai_compatible` 拼法走 alias。
    #[test]
    fn legacy_ai_json_with_api_style_loads() {
        let dir = temp_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(AI_FILE),
            br#"{"provider":"open_ai_compatible","api_style":"chat_completions","base_url":"https://api.deepseek.com/v1","model":"deepseek-chat"}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.provider, ProviderKind::OpenAiCompatible);
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.model, "deepseek-chat");

        std::fs::write(
            dir.join(AI_FILE),
            br#"{"provider":"openai_compatible","api_style":"anthropic_messages"}"#,
        )
        .unwrap();
        assert_eq!(
            AiConfig::load_from(&dir).provider,
            ProviderKind::OpenAiCompatible,
            "无下划线拼法走 alias"
        );

        std::fs::write(
            dir.join(AI_FILE),
            br#"{"provider":"mock","api_style":"ollama_generate"}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.provider, ProviderKind::Mock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 归一化:空白端点/模型回落**当前 provider** 的出厂值;越界采样参数
    /// 被钳住;provider 字段对新变体原样保留(不再回落到任何默认)。
    #[test]
    fn normalize_clamps_and_falls_back_to_provider_factory() {
        let mut config = AiConfig {
            provider: ProviderKind::Ollama,
            base_url: "  ".to_owned(),
            model: String::new(),
            temperature: 9.9,
            top_p: -0.5,
            max_tokens: 0,
            timeout_secs: 1,
            ..AiConfig::default()
        };
        config.normalize();
        assert_eq!(config.provider, ProviderKind::Ollama, "provider 不回落");
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert_eq!(config.model, "llama3.1");
        assert_eq!(config.temperature, 2.0);
        assert_eq!(config.top_p, 0.0);
        assert_eq!(config.max_tokens, 256);
        assert_eq!(config.timeout_secs, 10);

        let mut config = AiConfig {
            provider: ProviderKind::Anthropic,
            base_url: String::new(),
            model: String::new(),
            ..AiConfig::default()
        };
        config.normalize();
        assert_eq!(config.provider, ProviderKind::Anthropic);
        assert_eq!(config.base_url, "https://api.anthropic.com");
        assert_eq!(config.model, "claude-sonnet-4-5");
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

    /// provider 语义:Mock 不联网也不要 key;OpenAI 兼容/Anthropic 联网
    /// 且要 key;Ollama 联网(本机服务)但不要 key —— 两维解耦。
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
        assert!(ProviderKind::Anthropic.requires_key());
        assert!(AiConfig {
            provider: ProviderKind::Anthropic,
            ..AiConfig::default()
        }
        .connects_network());
        assert!(!ProviderKind::Ollama.requires_key());
        assert!(
            AiConfig {
                provider: ProviderKind::Ollama,
                ..AiConfig::default()
            }
            .connects_network(),
            "Ollama 连本机服务,端点行要显示"
        );
    }

    /// 切 provider 的出厂值跟随:出厂端点/模型/超时跟着换(含 Ollama 的
    /// 本地推理超时档);手改过的端点与模型不被静默覆盖。
    #[test]
    fn adopt_provider_defaults_swaps_factory_values_only() {
        let mut config = AiConfig::default(); // 出厂 = OpenAI 兼容端点
        config.adopt_provider_defaults(ProviderKind::Ollama);
        assert_eq!(config.provider, ProviderKind::Ollama);
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert_eq!(config.model, "llama3.1");
        assert_eq!(config.timeout_secs, 120, "Ollama 出厂超时是本地推理档");

        let mut config = AiConfig {
            base_url: "https://api.deepseek.com/v1".to_owned(),
            model: "deepseek-chat".to_owned(),
            ..AiConfig::default()
        };
        config.adopt_provider_defaults(ProviderKind::Anthropic);
        assert_eq!(
            config.base_url, "https://api.deepseek.com/v1",
            "手改端点保留"
        );
        assert_eq!(config.model, "deepseek-chat", "手改模型保留");
    }
}
