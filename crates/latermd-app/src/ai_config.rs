//! AI provider 配置(docs/ui-polish.md §6「AI」页)。
//!
//! 配置只保留**用户真正要填的四件事**:provider、Base URL、模型名与
//! (经系统凭据的)API key —— 采样参数/system prompt/超时/流式开关已随
//! 「配置页参数精简」删除(decisions-pending #108),内部按删除前的默认
//! 值组装请求(见 `crate::ai::set_provider`),不因删 UI 改变默认请求。
//! 另有一项**请求预算**:「上下文大小」(decisions-pending #110,KB 字节
//! 口径,0 = 跟随现状默认),驱动摘要/commit 的文档截断。
//!
//! **provider 是唯一开关**(decisions-pending #94):接口方式(协议形态)
//! 随 provider 派生,不再单列字段 —— 四种 provider 各自钉死一种协议,
//! 「OpenAI 端点 + Anthropic 协议」这类矛盾组合在配置层就不存在。
//!
//! **API key 不在这里**:key 只走系统凭据(`latermd-creds`,见
//! `crate::ai_key`),与参数分开存 —— 参数可以备份/分享,key 不行;旧档
//! 迁移时凭据通道不受影响。四种 provider 全部已实现:OpenAI 兼容/
//! Anthropic 需要 key(共用同一凭据通道),Ollama 本地无鉴权,Mock 不联网。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 落盘文件名(平台配置目录,与 `settings.json` 同级)。
const AI_FILE: &str = "ai.json";

/// 上下文大小(KB)可设置的上限(= 1MB)。换算依据(decisions-pending
/// #110):1KB ≈ 250-350 token(英文约 4 字节/token,UTF-8 常用汉字约
/// 3 字节、1-1.5 字/token),1MB 已是至多 ~50 万 token 的 prompt,远超
/// 主流模型上下文窗口,再大只会把请求打爆,钳掉。
pub const CONTEXT_KB_MAX: u32 = 1024;

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

/// provider 的出厂端点/模型([`ProviderKind::factory`] 的返回)。
pub struct ProviderFactory {
    /// 端点根地址。
    pub base_url: String,
    /// 模型名。
    pub model: String,
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

    /// 是否消费表单参数(端点/模型):只有 Mock 全不读、参数区灰显;
    /// Ollama 连本机服务,参数照常参与。
    pub fn uses_settings(self) -> bool {
        !matches!(self, Self::Mock)
    }

    /// 出厂参数:与对应 adapter 的 `Default` 实现同源,app 侧不另抄一份
    /// (防两处漂移)。Mock 无 adapter,沿用 OpenAI 兼容的值 —— 参数不
    /// 参与请求,表单只需要一个可显示的缺省。
    pub fn factory(self) -> ProviderFactory {
        let (base_url, model) = match self {
            Self::Mock | Self::OpenAiCompatible => {
                let s = latermd_ai::OpenAiSettings::default();
                (s.base_url, s.model)
            }
            Self::Anthropic => {
                let s = latermd_ai::AnthropicSettings::default();
                (s.base_url, s.model)
            }
            Self::Ollama => {
                let s = latermd_ai::OllamaSettings::default();
                (s.base_url, s.model)
            }
        };
        ProviderFactory { base_url, model }
    }
}

/// 模型参数与端点配置。缺省字段回落默认,手改的配置文件缺项不致整体失效。
///
/// 旧版曾有 `api_style`(接口方式)与 `temperature`/`top_p`/`max_tokens`/
/// `system_prompt`/`timeout_secs`/`stream` 字段;前者已随「provider 唯一
/// 开关」删除(decisions-pending #94),后者已随「配置页参数精简」删除
/// (decisions-pending #108)—— 旧 `ai.json` 里遗留的这些键在读取时被
/// 忽略(serde 默认不拒绝未知字段),provider/端点/模型名无损读入;
/// 被删参数的内部取值见 `crate::ai::set_provider`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// provider 种类。
    pub provider: ProviderKind,
    /// 端点根地址(自动去掉结尾 `/`)。
    pub base_url: String,
    /// 模型名。
    pub model: String,
    /// 上下文大小(KB):进入 prompt 的文档全文 / commit diff 的字节上限,
    /// 超出部分不进请求(prompt 里注明「已截断」)。0 = 未配置,摘要与
    /// commit 各自沿用现状默认(32KB / 16KB),截断行为与旧版完全一致
    /// (decisions-pending #110 的否决线)。对 provider 无关 —— Mock 也走
    /// 同一 prompt 组装。
    pub context_kb: u32,
}

impl Default for AiConfig {
    fn default() -> Self {
        let factory = ProviderKind::Mock.factory();
        Self {
            provider: ProviderKind::Mock,
            base_url: factory.base_url,
            model: factory.model,
            context_kb: 0,
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

    /// 切换 provider 时的出厂值跟随:端点/模型若仍是**任一** provider 的
    /// 出厂值(即用户从未手改),换成新 provider 的出厂值;手改过的字段
    /// 原样保留,绝不静默覆盖。设置页 provider 下拉切换时调用。
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
    }

    /// 归一化:端点/模型去空白,空值回落**当前 provider** 的出厂值;
    /// 上下文大小钳到 `[0, CONTEXT_KB_MAX]`(手改 JSON 的越界值在读入时
    /// 就地收回,滑杆范围之外没有合法取值)。provider 本体不回落 —— 四种
    /// provider 全部已实现,配置的是什么就是什么。
    pub fn normalize(&mut self) {
        self.base_url = self.base_url.trim().to_owned();
        self.model = self.model.trim().to_owned();
        self.context_kb = self.context_kb.min(CONTEXT_KB_MAX);
        let factory = self.provider.factory();
        if self.base_url.is_empty() {
            self.base_url = factory.base_url;
        }
        if self.model.is_empty() {
            self.model = factory.model;
        }
    }

    /// 上下文字节预算(「上下文大小」配置 → prompt 截断口径的换算,
    /// decisions-pending #110):0 = 未配置给 `None`,由 latermd-ai 的
    /// prompt 组装落回各用途现状默认(摘要 32KB / commit diff 16KB);
    /// 已配置 = KB × 1024 字节,摘要与 commit 共用同一预算。读侧同样钳
    /// 上限,绕过 `normalize` 构造的值也不会把请求撑爆。
    pub fn context_budget(&self) -> Option<usize> {
        let kb = self.context_kb.min(CONTEXT_KB_MAX);
        (kb != 0).then(|| kb as usize * 1024)
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

    /// 往返:改过的字段逐项保持(含旧变体的 snake_case 名与新上下文大小)。
    #[test]
    fn save_load_round_trip() {
        let dir = temp_dir("roundtrip");
        let config = AiConfig {
            provider: ProviderKind::OpenAiCompatible,
            base_url: "https://open.bigmodel.cn/api/paas/v4/".to_owned(),
            model: "glm-4.6".to_owned(),
            context_kb: 512,
        };
        config.save_to(&dir).unwrap();
        assert_eq!(AiConfig::load_from(&dir), config, "context_kb 应无损往返");
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
                context_kb: 0,
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

    /// 旧版 ai.json 兼容:含**全部**被删字段(采样三参数/system prompt/
    /// 超时/流式)与已删除的 `api_style` 键也能读回,未知键被忽略不报错,
    /// provider/端点/模型名无损;provider 取值走旧落盘名与 alias 拼法。
    #[test]
    fn legacy_ai_json_with_all_removed_fields_loads_losslessly() {
        let dir = temp_dir("legacy-full");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(AI_FILE),
            r#"{"provider":"open_ai_compatible","api_style":"chat_completions","base_url":"https://api.deepseek.com/v1","model":"deepseek-chat","temperature":1.2,"top_p":0.85,"max_tokens":4096,"system_prompt":"你是技术文档助手","timeout_secs":90,"stream":false}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.provider, ProviderKind::OpenAiCompatible);
        assert_eq!(config.base_url, "https://api.deepseek.com/v1");
        assert_eq!(config.model, "deepseek-chat");

        // 另两家 provider 同款:被删字段在旧档里同样被忽略
        std::fs::write(
            dir.join(AI_FILE),
            r#"{"provider":"anthropic","base_url":"https://api.anthropic.com","model":"claude-sonnet-4-5","temperature":0.1,"top_p":0.5,"max_tokens":8192,"system_prompt":"x","timeout_secs":30,"stream":false,"api_style":"anthropic_messages"}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.provider, ProviderKind::Anthropic);
        assert_eq!(config.base_url, "https://api.anthropic.com");
        assert_eq!(config.model, "claude-sonnet-4-5");

        std::fs::write(
            dir.join(AI_FILE),
            br#"{"provider":"ollama","base_url":"http://127.0.0.1:11434","model":"llama3.1","temperature":0.9,"stream":false}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.provider, ProviderKind::Ollama);
        assert_eq!(config.model, "llama3.1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 旧拼法 `openai_compatible`(无下划线)走 alias;`mock` 原样可读。
    #[test]
    fn legacy_provider_spellings_load() {
        let dir = temp_dir("legacy-spelling");
        std::fs::create_dir_all(&dir).unwrap();
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
        assert_eq!(AiConfig::load_from(&dir).provider, ProviderKind::Mock);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 新配置落盘后,被删字段不复活:ai.json 里不再出现 temperature/top_p/
    /// max_tokens/system_prompt/timeout_secs/stream/api_style 任何一键
    /// (字段已从结构体删除,serde 不再序列化它们)。
    #[test]
    fn save_drops_removed_fields_from_disk() {
        let dir = temp_dir("no-revival");
        let config = AiConfig {
            provider: ProviderKind::OpenAiCompatible,
            base_url: "https://api.deepseek.com/v1".to_owned(),
            model: "deepseek-chat".to_owned(),
            context_kb: 64,
        };
        config.save_to(&dir).unwrap();
        let json = std::fs::read_to_string(dir.join(AI_FILE)).unwrap();
        for key in [
            "temperature",
            "top_p",
            "max_tokens",
            "system_prompt",
            "timeout_secs",
            "stream",
            "api_style",
        ] {
            assert!(
                !json.contains(&format!("\"{key}\"")),
                "落盘不应再含被删键 \"{key}\":\n{json}"
            );
        }
        // 保留项在场:provider/base_url/model/context_kb 四键齐全
        for key in ["provider", "base_url", "model", "context_kb"] {
            assert!(
                json.contains(&format!("\"{key}\"")),
                "落盘应含保留键 \"{key}\":\n{json}"
            );
        }
        // 落盘 → 重读仍然无损(旧字段消失不影响往返)
        assert_eq!(AiConfig::load_from(&dir), config);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 上下文大小(decisions-pending #110)的换算与默认:默认/旧档缺字段
    /// = 0 = `None`(未配置,latermd-ai 落回现状默认,截断行为不变);
    /// 显式 KB 值换算为字节,两用途共用同一预算。
    #[test]
    fn context_kb_defaults_to_zero_and_maps_to_byte_budget() {
        assert_eq!(AiConfig::default().context_kb, 0);
        assert_eq!(AiConfig::default().context_budget(), None, "未配置 = None");
        assert_eq!(
            AiConfig {
                context_kb: 32,
                ..AiConfig::default()
            }
            .context_budget(),
            Some(32 * 1024),
            "KB × 1024 = 字节"
        );

        // 旧档(ai.json 无 context_kb 键)读入 = 0 = 未配置
        let dir = temp_dir("legacy-no-context");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(AI_FILE),
            br#"{"provider":"open_ai_compatible","base_url":"https://api.deepseek.com/v1","model":"deepseek-chat"}"#,
        )
        .unwrap();
        let config = AiConfig::load_from(&dir);
        assert_eq!(config.context_kb, 0, "旧档缺字段回落 0(未配置)");
        assert_eq!(config.context_budget(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 上下文大小钳制(decisions-pending #110,照 #23 滑杆先例的上下限):
    /// 手改 JSON 的越界值在 normalize(读入路径)就地收回上限;绕过
    /// normalize 构造的值在 `context_budget` 读侧同样被钳。
    #[test]
    fn context_kb_is_clamped_to_configured_range() {
        let mut config = AiConfig {
            context_kb: u32::MAX,
            ..AiConfig::default()
        };
        config.normalize();
        assert_eq!(config.context_kb, CONTEXT_KB_MAX, "读入路径钳到上限");

        let beyond = AiConfig {
            context_kb: 2000,
            ..AiConfig::default()
        };
        assert_eq!(
            beyond.context_budget(),
            Some(CONTEXT_KB_MAX as usize * 1024),
            "读侧同样钳上限,预算不会撑爆"
        );
    }

    /// 归一化:空白端点/模型回落**当前 provider** 的出厂值;provider 字段
    /// 对新变体原样保留(不再回落到任何默认)。
    #[test]
    fn normalize_falls_back_to_provider_factory() {
        let mut config = AiConfig {
            provider: ProviderKind::Ollama,
            base_url: "  ".to_owned(),
            model: String::new(),
            context_kb: 0,
        };
        config.normalize();
        assert_eq!(config.provider, ProviderKind::Ollama, "provider 不回落");
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert_eq!(config.model, "llama3.1");

        let mut config = AiConfig {
            provider: ProviderKind::Anthropic,
            base_url: String::new(),
            model: String::new(),
            context_kb: 0,
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

    /// 切 provider 的出厂值跟随:出厂端点/模型跟着换;手改过的端点与模型
    /// 不被静默覆盖。
    #[test]
    fn adopt_provider_defaults_swaps_factory_values_only() {
        let mut config = AiConfig::default(); // 出厂 = OpenAI 兼容端点
        config.adopt_provider_defaults(ProviderKind::Ollama);
        assert_eq!(config.provider, ProviderKind::Ollama);
        assert_eq!(config.base_url, "http://127.0.0.1:11434");
        assert_eq!(config.model, "llama3.1");

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
