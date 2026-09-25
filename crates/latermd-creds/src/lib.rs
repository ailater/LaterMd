#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! 三平台系统凭据存取(docs/roadmap.md 阶段 4「P2 版本层·凭据管理」)。
//!
//! 定位:把 keyring crate 的 [`keyring::Entry`] 封装成「service + account
//! 寻址一条字符串凭据」的小 API,落到 Windows Credential Manager /
//! macOS Keychain / Linux Secret Service。不依赖 egui(铁律:业务逻辑
//! 不依赖 UI 框架,AGENTS.md §3);凭据只进系统库,绝不落 settings.json
//! 等任何磁盘文件。
//!
//! 口径:
//! * 后端不可用(Linux 无 Secret Service、CI 无 dbus 等)一律降级为
//!   [`CredentialError::Backend`] 返回,绝不 panic;真实 keyring 后端的
//!   冒烟测试在无 keyring 环境自动跳过。
//! * [`CredentialError`] 文案只含 service/account 与消毒后的原因,
//!   绝不含凭据值(写入路径的错误原因先消毒再交给调用方)。
//! * 删除幂等:条目不存在视同删除成功。
//! * 通用 [`set_secret`] 原样存储、不校验值的语义(空白是否合法由
//!   专用入口决定,如 [`set_ai_api_key`] 拒绝空白)。

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

/// 系统凭据的 service 名:LaterMD 的全部条目固定用它,调用方不应再
/// 造第二个前缀。
pub const SERVICE: &str = "latermd";

/// AI API key 的 account 名(配 [`SERVICE`] 使用)。
pub const AI_ACCOUNT: &str = "ai_provider";

/// 环境变量 `LATERMD_AI_API_KEY`:系统凭据未配置或不可读时的逃生口
/// (CI / 容器 / 无 keyring 桌面)。与 latermd-ai 的同名常量同值且
/// 各自声明(两 crate 无依赖关系;代码不硬编码任何 key,
/// decisions-pending #3)。
pub const API_KEY_ENV: &str = "LATERMD_AI_API_KEY";

/// 凭据操作错误。文案面向用户(中文),可直接上提示行;任何变体都
/// 不携带凭据值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    /// service 或 account 为空:空标识在系统凭据库里无法稳定寻址。
    EmptyLabel,
    /// 拒绝保存空白 AI key(否则「已配置」与「读回 None」自相矛盾)。
    BlankSecret,
    /// 后端操作失败。`cause` 是底层原因(写入路径已经消毒)。
    Backend {
        /// 动作名(「写入」/「读取」/「删除」等)。
        action: &'static str,
        /// service 名。
        service: String,
        /// account 名。
        account: String,
        /// 底层错误描述;不含凭据值。
        cause: String,
    },
}

impl CredentialError {
    /// 把错误原因里可能嵌入的秘密替换为掩码。写入路径在后端错误交给
    /// 调用方之前调用,是「凭据值不进错误信息」红线的最后闸口——即使
    /// 某个后端把值漏进了底层消息,到这里也会被抹掉。
    fn redact(self, secret: &str) -> Self {
        if secret.is_empty() {
            return self;
        }
        match self {
            Self::Backend {
                action,
                service,
                account,
                cause,
            } => Self::Backend {
                action,
                service,
                account,
                cause: cause.replace(secret, "***"),
            },
            other => other,
        }
    }
}

impl fmt::Display for CredentialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CredentialError::EmptyLabel => {
                write!(f, "凭据标识为空:service 与 account 都必须非空")
            }
            CredentialError::BlankSecret => {
                write!(f, "拒绝保存空白凭据:要清除请改用删除操作")
            }
            CredentialError::Backend {
                action,
                service,
                account,
                cause,
            } => write!(f, "系统凭据{action}失败({service}/{account}):{cause}"),
        }
    }
}

impl std::error::Error for CredentialError {}

/// 凭据后端契约。生产实现是 [`KeyringBackend`];测试与无 keyring 环境
/// 用 [`InMemoryBackend`] 或自造假后端注入 [`Credentials::new`]。
///
/// 语义:写入存在即覆盖;读取 `Ok(None)` 表示未配置;删除幂等
/// (条目不存在也返回 `Ok(())`)。
pub trait CredentialBackend: Send + Sync {
    /// 写入一条凭据(存在即覆盖)。
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), CredentialError>;
    /// 读取一条凭据;`Ok(None)` = 条目不存在。
    fn get(&self, service: &str, account: &str) -> Result<Option<String>, CredentialError>;
    /// 删除一条凭据;幂等。
    fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError>;
}

/// 内存后端:仅供单测与无系统凭据环境的演示,进程结束即蒸发、不落
/// 任何盘;不要拿它存真实凭据。
#[derive(Debug, Default)]
pub struct InMemoryBackend {
    entries: Mutex<HashMap<(String, String), String>>,
}

impl InMemoryBackend {
    /// 空后端。
    pub fn new() -> Self {
        Self::default()
    }
}

/// 锁中毒(仅测试后端里另一线程 panic 后会出现)。
fn poisoned(service: &str, account: &str) -> CredentialError {
    CredentialError::Backend {
        action: "访问",
        service: service.to_owned(),
        account: account.to_owned(),
        cause: "内存后端锁中毒".to_owned(),
    }
}

impl CredentialBackend for InMemoryBackend {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| poisoned(service, account))?;
        entries.insert((service.to_owned(), account.to_owned()), secret.to_owned());
        Ok(())
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, CredentialError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| poisoned(service, account))?;
        Ok(entries
            .get(&(service.to_owned(), account.to_owned()))
            .cloned())
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| poisoned(service, account))?;
        entries.remove(&(service.to_owned(), account.to_owned()));
        Ok(())
    }
}

/// keyring 后端(生产默认):Windows Credential Manager / macOS Keychain /
/// Linux Secret Service。Linux 依赖 Secret Service 经 dbus 可达;不可用时
/// keyring 在首次打开条目即快速失败,这里统一转成 Err 交给调用方降级。
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyringBackend;

impl KeyringBackend {
    /// 打开 keyring 条目(keyring 的第二参数叫 username,即本 crate 的
    /// account)。所有后端错误统一在此包装,不带凭据值。
    fn entry(service: &str, account: &str) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(service, account).map_err(|err| CredentialError::Backend {
            action: "打开",
            service: service.to_owned(),
            account: account.to_owned(),
            cause: err.to_string(),
        })
    }

    /// 包装 set_password/delete_credential 一类的 `Result<()>` 错误。
    fn wrap(
        result: keyring::Result<()>,
        action: &'static str,
        service: &str,
        account: &str,
    ) -> Result<(), CredentialError> {
        result.map_err(|err| CredentialError::Backend {
            action,
            service: service.to_owned(),
            account: account.to_owned(),
            cause: err.to_string(),
        })
    }
}

impl CredentialBackend for KeyringBackend {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
        Self::wrap(
            Self::entry(service, account)?.set_password(secret),
            "写入",
            service,
            account,
        )
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, CredentialError> {
        match Self::entry(service, account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(CredentialError::Backend {
                action: "读取",
                service: service.to_owned(),
                account: account.to_owned(),
                cause: err.to_string(),
            }),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), CredentialError> {
        match Self::entry(service, account)?.delete_credential() {
            Ok(()) => Ok(()),
            // 幂等:条目本就不存在 = 目标状态已达成
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(CredentialError::Backend {
                action: "删除",
                service: service.to_owned(),
                account: account.to_owned(),
                cause: err.to_string(),
            }),
        }
    }
}

/// trim + 空白过滤:空白值等价于未配置(与 latermd-ai::read_api_key
/// 同语义)。
fn normalize_secret(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// 绑定某个后端的凭据操作集。底部同名自由函数是系统后端的生产入口;
/// 测试用 [`Credentials::new`] 注入假后端。
pub struct Credentials {
    backend: Arc<dyn CredentialBackend>,
}

impl Credentials {
    /// 显式指定后端(测试 / 替代存储)。
    pub fn new(backend: Arc<dyn CredentialBackend>) -> Self {
        Self { backend }
    }

    /// 系统凭据后端(生产默认)。
    pub fn system() -> Self {
        Self::new(Arc::new(KeyringBackend))
    }

    /// 内存后端(测试 / 演示,见 [`InMemoryBackend`] 文档)。
    pub fn in_memory() -> Self {
        Self::new(Arc::new(InMemoryBackend::new()))
    }

    /// 空标识直接拒绝(keyring 与内存后端都以 (service, account) 二元组
    /// 寻址,空串虽可存储但语义混乱)。
    fn check_labels(service: &str, account: &str) -> Result<(), CredentialError> {
        if service.is_empty() || account.is_empty() {
            return Err(CredentialError::EmptyLabel);
        }
        Ok(())
    }

    /// 写入一条凭据(存在即覆盖)。值原样存储;错误文案经消毒,不含
    /// 凭据值。
    pub fn set_secret(
        &self,
        service: &str,
        account: &str,
        secret: &str,
    ) -> Result<(), CredentialError> {
        Self::check_labels(service, account)?;
        self.backend
            .set(service, account, secret)
            .map_err(|err| err.redact(secret))
    }

    /// 读取一条凭据;`Ok(None)` = 未配置。后端不可用返回 `Err`(不吞错,
    /// 调用方转提示行);只有确定「未配置」才返回 `None`。
    pub fn get_secret(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, CredentialError> {
        Self::check_labels(service, account)?;
        self.backend.get(service, account)
    }

    /// 删除一条凭据;幂等(不存在也算成功)。
    pub fn delete_secret(&self, service: &str, account: &str) -> Result<(), CredentialError> {
        Self::check_labels(service, account)?;
        self.backend.delete(service, account)
    }

    /// 该凭据是否已配置。后端读失败视同未配置(`false`):这是便捷的
    /// 存在性查询,失败语义由 set/get 的 `Err` 承担——用户重新保存时
    /// 会看到真实错误,不会误存。
    pub fn has_secret(&self, service: &str, account: &str) -> bool {
        matches!(self.get_secret(service, account), Ok(Some(_)))
    }

    /// 读 AI API key,系统凭据优先;`env_value` 是显式传入的环境变量
    /// 取值(测试注入点,生产走 [`Credentials::ai_api_key`])。
    ///
    /// 读取顺序与理由:
    /// 1. 系统凭据([`SERVICE`]/[`AI_ACCOUNT`]):用户在应用里显式保存,
    ///    存储意图最明确、删除即时生效,故优先。
    /// 2. 环境变量 `LATERMD_AI_API_KEY`(decisions-pending #3 的逃生
    ///    口):仅在系统凭据**未配置或不可读**(如 Linux 无 Secret
    ///    Service、CI 无 dbus)时生效。不可读时必须降级而不是直接
    ///    返回 `None`,否则无 keyring 环境里这个逃生口就名存实亡。
    ///    降级不影响优先级:系统凭据可用时永远赢。
    ///
    /// 两级取值都做 trim + 空白过滤,空白 key 等价于未配置。
    pub fn ai_api_key_from(&self, env_value: Option<&str>) -> Option<String> {
        match self.get_secret(SERVICE, AI_ACCOUNT) {
            // 系统凭据取到且过滤后仍非空才短路;空白值(等价未配置,
            // 只有通用 set_secret 存得进去)同样降级环境变量
            Ok(key) => key
                .as_deref()
                .and_then(normalize_secret)
                .or_else(|| env_value.and_then(normalize_secret)),
            // 未配置或后端不可读:降级到环境变量
            Err(_) => env_value.and_then(normalize_secret),
        }
    }

    /// 读 AI API key(生产入口):系统凭据 > 环境变量 [`API_KEY_ENV`]。
    /// 顺序与理由见 [`Credentials::ai_api_key_from`]。
    pub fn ai_api_key(&self) -> Option<String> {
        self.ai_api_key_from(std::env::var(API_KEY_ENV).ok().as_deref())
    }

    /// 保存 AI API key 到系统凭据(存 trim 后的值;空白拒绝,理由见
    /// [`CredentialError::BlankSecret`])。
    pub fn set_ai_api_key(&self, key: &str) -> Result<(), CredentialError> {
        let key = key.trim();
        if key.is_empty() {
            return Err(CredentialError::BlankSecret);
        }
        self.set_secret(SERVICE, AI_ACCOUNT, key)
    }
}

/// 写入系统凭据(生产入口,keyring 后端)。见 [`Credentials::set_secret`]。
pub fn set_secret(service: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
    Credentials::system().set_secret(service, account, secret)
}

/// 读取系统凭据。见 [`Credentials::get_secret`]。
pub fn get_secret(service: &str, account: &str) -> Result<Option<String>, CredentialError> {
    Credentials::system().get_secret(service, account)
}

/// 删除系统凭据(幂等)。见 [`Credentials::delete_secret`]。
pub fn delete_secret(service: &str, account: &str) -> Result<(), CredentialError> {
    Credentials::system().delete_secret(service, account)
}

/// 系统凭据是否已配置。见 [`Credentials::has_secret`]。
pub fn has_secret(service: &str, account: &str) -> bool {
    Credentials::system().has_secret(service, account)
}

/// 读 AI API key:系统凭据 > 环境变量 [`API_KEY_ENV`]。见
/// [`Credentials::ai_api_key`]。
pub fn ai_api_key() -> Option<String> {
    Credentials::system().ai_api_key()
}

/// 保存 AI API key 到系统凭据。见 [`Credentials::set_ai_api_key`]。
pub fn set_ai_api_key(key: &str) -> Result<(), CredentialError> {
    Credentials::system().set_ai_api_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 安全红线:本模块所有「凭据值」都是 placeholder-* 占位假值,不出现
    // 任何真实 key。CRUD 语义要求断言读回值与写入值一致(否则后端就是
    // 坏的),这属于相等性比较,不是把真实凭据写进断言(decisions-pending
    // #20)。

    #[test]
    fn in_memory_crud_roundtrip() {
        let creds = Credentials::in_memory();
        assert_eq!(creds.get_secret(SERVICE, "acct-a").unwrap(), None);
        creds
            .set_secret(SERVICE, "acct-a", "placeholder-a")
            .unwrap();
        creds
            .set_secret(SERVICE, "acct-b", "placeholder-b")
            .unwrap();
        assert_eq!(
            creds.get_secret(SERVICE, "acct-a").unwrap().as_deref(),
            Some("placeholder-a")
        );
        // 覆盖写
        creds
            .set_secret(SERVICE, "acct-a", "placeholder-a2")
            .unwrap();
        assert_eq!(
            creds.get_secret(SERVICE, "acct-a").unwrap().as_deref(),
            Some("placeholder-a2")
        );
        // service/account 二元组隔离:删 a 不影响 b
        creds.delete_secret(SERVICE, "acct-a").unwrap();
        assert_eq!(creds.get_secret(SERVICE, "acct-a").unwrap(), None);
        assert!(creds.has_secret(SERVICE, "acct-b"));
    }

    #[test]
    fn set_has_delete_idempotent() {
        let creds = Credentials::in_memory();
        assert!(!creds.has_secret(SERVICE, "acct"));
        creds.set_secret(SERVICE, "acct", "placeholder-v1").unwrap();
        assert!(creds.has_secret(SERVICE, "acct"));
        // 重复 set(幂等覆盖):仍可读、仍是一条
        creds.set_secret(SERVICE, "acct", "placeholder-v1").unwrap();
        assert!(creds.has_secret(SERVICE, "acct"));
        // 删除幂等:两次 delete 都 Ok,状态稳定在「不存在」
        creds.delete_secret(SERVICE, "acct").unwrap();
        creds.delete_secret(SERVICE, "acct").unwrap();
        assert!(!creds.has_secret(SERVICE, "acct"));
        assert_eq!(creds.get_secret(SERVICE, "acct").unwrap(), None);
    }

    #[test]
    fn empty_labels_rejected() {
        let creds = Credentials::in_memory();
        assert_eq!(
            creds.set_secret("", "acct", "placeholder").unwrap_err(),
            CredentialError::EmptyLabel
        );
        assert_eq!(
            creds.set_secret(SERVICE, "", "placeholder").unwrap_err(),
            CredentialError::EmptyLabel
        );
        assert_eq!(
            creds.get_secret("", "acct").unwrap_err(),
            CredentialError::EmptyLabel
        );
        assert_eq!(
            creds.delete_secret(SERVICE, "").unwrap_err(),
            CredentialError::EmptyLabel
        );
    }

    #[test]
    fn env_fallback_order() {
        // 注入方式测环境变量回退:不碰进程级 env,避免并行测试竞态
        let creds = Credentials::in_memory();
        // 系统凭据与环境变量都有 → 系统凭据赢
        creds.set_ai_api_key("placeholder-keyring").unwrap();
        assert_eq!(
            creds.ai_api_key_from(Some("placeholder-env")).as_deref(),
            Some("placeholder-keyring")
        );
        // 系统凭据未配置 → 环境变量生效
        creds.delete_secret(SERVICE, AI_ACCOUNT).unwrap();
        assert_eq!(
            creds.ai_api_key_from(Some("placeholder-env")).as_deref(),
            Some("placeholder-env")
        );
        // 都没有 → None
        assert_eq!(creds.ai_api_key_from(None), None);
        // 环境变量空白 → 等价未设置
        assert_eq!(creds.ai_api_key_from(Some("   ")), None);
        // 系统凭据存了空白 → 读侧过滤成不可用,降级环境变量
        creds.set_secret(SERVICE, AI_ACCOUNT, "   ").unwrap();
        assert_eq!(
            creds.ai_api_key_from(Some("placeholder-env")).as_deref(),
            Some("placeholder-env")
        );
        // 凭据值带空白 → trim 后返回
        creds
            .set_secret(SERVICE, AI_ACCOUNT, "  placeholder-key  ")
            .unwrap();
        assert_eq!(
            creds.ai_api_key_from(None).as_deref(),
            Some("placeholder-key")
        );
    }

    #[test]
    fn env_fallback_when_backend_unreadable() {
        // 后端不可读(无 Secret Service 场景)也必须降级到环境变量,
        // 否则逃生口在无 keyring 环境名存实亡
        let creds = Credentials::new(Arc::new(FailingBackend));
        assert_eq!(
            creds.ai_api_key_from(Some("placeholder-env")).as_deref(),
            Some("placeholder-env")
        );
        assert_eq!(creds.ai_api_key_from(None), None);
        // 便捷查询同步降级:has=false(失败语义由 set/get 的 Err 承担)
        assert!(!creds.has_secret(SERVICE, AI_ACCOUNT));
    }

    /// 假后端:所有操作失败。
    struct FailingBackend;

    impl CredentialBackend for FailingBackend {
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(backend_err("写入"))
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, CredentialError> {
            Err(backend_err("读取"))
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(backend_err("删除"))
        }
    }

    /// 假后端:错误原因里**故意嵌入 secret 明文**,验证消毒闸
    /// (`CredentialError::redact`)真的把值抹掉。
    struct LeakyBackend;

    impl CredentialBackend for LeakyBackend {
        fn set(&self, _: &str, _: &str, secret: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Backend {
                action: "写入",
                service: SERVICE.to_owned(),
                account: AI_ACCOUNT.to_owned(),
                cause: format!("platform failure while storing {secret}"),
            })
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, CredentialError> {
            Err(backend_err("读取"))
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), CredentialError> {
            Err(backend_err("删除"))
        }
    }

    fn backend_err(action: &'static str) -> CredentialError {
        CredentialError::Backend {
            action,
            service: SERVICE.to_owned(),
            account: AI_ACCOUNT.to_owned(),
            cause: "no secret service".to_owned(),
        }
    }

    #[test]
    fn secrets_never_leak_into_error_messages() {
        let creds = Credentials::new(Arc::new(LeakyBackend));
        let secret = "placeholder-leaky";
        // set 路径:后端错误原因嵌入了值,消毒闸必须抹掉
        let err = creds.set_secret(SERVICE, AI_ACCOUNT, secret).unwrap_err();
        let text = err.to_string();
        assert!(!text.contains(secret), "错误文案不得包含凭据值:{text}");
        assert!(text.contains("***"), "被抹掉的位置应有掩码:{text}");
        // 文案仍可定位:service/account 可见(公开标识,非机密)
        assert!(text.contains(SERVICE) && text.contains(AI_ACCOUNT));
        // 专用入口同一道闸
        let err = creds.set_ai_api_key(secret).unwrap_err();
        assert!(!err.to_string().contains(secret));
        // get/delete 不接触值,失败文案同样不含
        assert!(!creds
            .get_secret(SERVICE, AI_ACCOUNT)
            .unwrap_err()
            .to_string()
            .contains(secret));
        assert!(!creds
            .delete_secret(SERVICE, AI_ACCOUNT)
            .unwrap_err()
            .to_string()
            .contains(secret));
    }

    #[test]
    fn blank_ai_key_rejected() {
        let creds = Credentials::in_memory();
        assert_eq!(
            creds.set_ai_api_key("   ").unwrap_err(),
            CredentialError::BlankSecret
        );
        assert!(!creds.has_secret(SERVICE, AI_ACCOUNT));
        // 非 blank 正常保存
        creds.set_ai_api_key(" placeholder-key ").unwrap();
        assert_eq!(
            creds.ai_api_key_from(None).as_deref(),
            Some("placeholder-key")
        );
    }

    /// 真实 keyring 后端冒烟:只在系统凭据可用时执行完整 CRUD;无
    /// Secret Service / Keychain 的环境(CI 无 dbus 等)在首个写入探测
    /// 失败时跳过——按任务约束,这不是失败。本机会短暂写入
    /// latermd/selftest 条目并在测试内清理。
    #[test]
    fn keyring_backend_smoke_when_available() {
        let backend = KeyringBackend;
        let account = "selftest";
        if backend.set(SERVICE, account, "placeholder-smoke").is_err() {
            eprintln!(
                "skip: 系统凭据后端不可用(无 Secret Service / Keychain / Credential Manager)"
            );
            return;
        }
        assert_eq!(
            backend.get(SERVICE, account).unwrap().as_deref(),
            Some("placeholder-smoke")
        );
        backend.delete(SERVICE, account).unwrap();
        // 删除幂等(真实后端的 NoEntry → Ok 路径)
        backend.delete(SERVICE, account).unwrap();
        assert_eq!(backend.get(SERVICE, account).unwrap(), None);
    }
}
