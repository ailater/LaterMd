//! 外壳布局状态与持久化(docs/ui-shell-redesign.md §10)。
//!
//! 左右两个 `Panel::show_collapsible` 的 `&mut bool` 住在这里;左侧页签也一并
//! 收在本结构体,使「停在哪个视图」与「面板开着与否」同属一份存档。
//!
//! **为什么单开 `layout.json` 而不塞 `settings.json`**:后者已承载主题与皮肤
//! 选择,再塞面板可见性会让「换主题」和「收面板」两个无关动作共用一份
//! 存档,`select_skin` 那套「皮肤文件是唯一事实源」的口径被稀释
//! (decisions-pending #24 的教训)。落盘风格与 `filetree::FileTreeSettings`
//! 完全一致:serde 往返、`#[serde(default)]` 缺项回落、坏文件只告警不挡启动。

use crate::state::SidebarTab;
use crate::theme;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use serde::{Deserialize, Serialize};

/// 持久化文件名,与 `settings.json` 同目录。
const SETTINGS_FILE: &str = "layout.json";

/// 持久化的外壳布局。字段各自直接喂给 UI,不额外镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutSettings {
    /// 左栏(导航)是否展开。
    pub left: bool,
    /// 右栏(预览)是否展开。
    pub right: bool,
    /// 禅定模式(M4 实装;本棒只存档,不参与绘制)。
    pub zen: bool,
    /// 左栏上次停留的视图。
    pub left_view: SidebarTab,
}

/// 出厂布局:三栏全开(旧行为),停文件树。
impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            left: true,
            right: true,
            zen: false,
            left_view: SidebarTab::Files,
        }
    }
}

impl LayoutSettings {
    /// 启动装载:无文件/坏文件都回落默认(坏件终端告警,挡启动不值得)。
    pub fn load() -> Self {
        let Some(dir) = theme::config_dir() else {
            return Self::default();
        };
        match Self::load_from(&dir) {
            Ok(settings) => settings,
            Err(LoadError::Missing) => Self::default(),
            Err(LoadError::Corrupt(source)) => {
                eprintln!("LaterMD: 布局设置解析失败,已回落默认: {source}");
                Self::default()
            }
        }
    }

    /// 落盘到 `<dir>/layout.json`;`dir` 为 `None` 时用平台默认目录。
    /// 目录不存在则创建。失败带路径,提示行可直接展示。
    pub fn save_to(&self, dir: Option<&Path>) -> StdResult<(), SaveError> {
        let Some(dir) = dir.map(Path::to_path_buf).or_else(theme::config_dir) else {
            return Err(SaveError {
                path: PathBuf::from(SETTINGS_FILE),
                source: "找不到平台配置目录(HOME/APPDATA 均未设置)".into(),
            });
        };
        let json = serde_json::to_string_pretty(self).map_err(|source| SaveError {
            path: dir.join(SETTINGS_FILE),
            source: Box::new(source),
        })?;
        std::fs::create_dir_all(&dir).map_err(|source| SaveError {
            path: dir.clone(),
            source: Box::new(source),
        })?;
        std::fs::write(dir.join(SETTINGS_FILE), json.as_bytes()).map_err(|source| SaveError {
            path: dir.join(SETTINGS_FILE),
            source: Box::new(source),
        })
    }

    /// 测试与 `main` 的装载取同一份实现(`load()` 就是它 + 失败回落)。
    pub(crate) fn load_from(dir: &Path) -> StdResult<Self, LoadError> {
        let path = dir.join(SETTINGS_FILE);
        let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => LoadError::Missing,
            _ => LoadError::Corrupt(format!("{}: {}", path.display(), source)),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|source| LoadError::Corrupt(format!("{}: {}", path.display(), source)))
    }
}

/// 读设置的失败情形(与 `crate::theme` / `crate::filetree` 同构)。
///
/// `pub(crate)`:布局的内存态与存档态要能互比(重启恢复测试的落点),
/// 不必像 `SaveError` 那样面向用户提示。
#[derive(Debug)]
pub(crate) enum LoadError {
    Missing,
    Corrupt(String),
}

/// 写设置失败:带路径,可直接进提示行。
#[derive(Debug)]
pub struct SaveError {
    path: PathBuf,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "布局设置保存失败 {}: {}",
            self.path.display(),
            self.source
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-layout-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 往返无损:改过的三态 + 视图都回来。
    #[test]
    fn roundtrip_preserves_every_field() {
        let dir = dir("roundtrip");
        let settings = LayoutSettings {
            left: false,
            right: true,
            zen: true,
            left_view: SidebarTab::Outline,
        };
        settings.save_to(Some(&dir)).unwrap();
        assert_eq!(LayoutSettings::load_from(&dir).unwrap(), settings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 出厂布局三栏全开、停文件树 —— 与旧行为一致,升级用户不会被直接
    /// 扔进空窗口(左栏收起又没菜单栏入口会很难找回)。
    #[test]
    fn default_is_three_columns_open_on_files() {
        let settings = LayoutSettings::default();
        assert!(settings.left && settings.right);
        assert!(!settings.zen);
        assert_eq!(settings.left_view, SidebarTab::Files);
    }

    /// 缺项回落:手改配置只留一个键,其余取默认而不是整体失败
    /// (与 `filetree::FileTreeSettings` 同款口径)。
    #[test]
    fn partial_json_fills_defaults() {
        let dir = dir("partial");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), br#"{"left": false}"#).unwrap();

        let settings = LayoutSettings::load_from(&dir).unwrap();
        assert!(!settings.left, "显式值生效");
        assert!(settings.right, "缺席的 right 回落默认 true");
        assert_eq!(settings.left_view, SidebarTab::Files);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 坏文件不 panic 也不静默:解析失败返回 `Corrupt`,`load()` 据此回落
    /// 默认并告警(用户对两种失败的处置不同,不能混)。
    #[test]
    fn corrupt_json_is_reported_not_swallowed() {
        let dir = dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), b"{ not json").unwrap();
        let loaded = LayoutSettings::load_from(&dir);
        assert!(
            matches!(loaded, Err(LoadError::Corrupt(_))),
            "坏文件要能被识别(由 load() 告警后回落默认),实际 {loaded:?}"
        );
        // 源串随变体走 Debug(不像 SaveError 那样面向提示行做 Display:读它的
        // 是看日志的开发者)。务必带上**路径与行列**,否则坏在哪无从定位。
        let debug = format!("{:?}", LayoutSettings::load_from(&dir).unwrap_err());
        assert!(debug.contains("layout.json"), "要带文件名:{debug}");
        assert!(
            debug.contains("line 1 column 3"),
            "要带 serde 的行列:{debug}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 文件缺失是常态而非错误(`Missing` 不进告警路径,首次启动才干净)。
    #[test]
    fn missing_file_is_distinct_from_corrupt() {
        let dir = dir("missing");
        let loaded = LayoutSettings::load_from(&dir);
        assert!(
            matches!(loaded, Err(LoadError::Missing)),
            "文件缺失是常态而非错误,实际 {loaded:?}"
        );
    }
}
