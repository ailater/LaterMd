//! 主题系统(P0 批次 A):Light/Dark 切换 + 持久化。
//!
//! [`ThemeSettings`] 是主题的唯一事实源,向两处投影(`ThemeSettings::apply`):
//!
//! - **外壳**:`egui::Context::set_theme` 切换 egui 自带的 light/dark 双套
//!   `Style`/`Visuals`(egui 0.36 按主题各持一份,面板/控件全部跟随);
//! - **正文**:vendored `MarkdownStyle` 装进 context 默认槽
//!   (`egui_markdown_style::set_style`),其颜色字段本就成对设计
//!   (`color_dark`/`color_light`),渲染时按 `ui.visuals().dark_mode` 自动取值;
//!   代码高亮不指定 theme 时同样按 dark/light 自动选择。
//!
//! 联动零额外成本:MarkdownStyle 布局缓存 hash 已含 dark_mode,切换自动失效。
//!
//! 持久化:平台配置目录下手写路径(不引目录库),JSON 经 serde 往返。
//! `overrides` 为 `None` 时正文样式即 `MarkdownStyle::default()`,皮肤批次 B
//! (docs/roadmap.md「专题:界面美化与皮肤系统」)才会填充它。

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use eframe::egui;
use egui_markdown_style::MarkdownStyle;
use serde::{Deserialize, Serialize};

/// 配置文件名,落在平台配置目录下。
const SETTINGS_FILE: &str = "settings.json";

/// 明暗模式。serde 小写(`"light"`/`"dark"`);默认深色,与 egui 的默认
/// visuals 一致,首跑无闪变。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// 浅色。
    Light,
    /// 深色。
    #[default]
    Dark,
}

impl ThemeMode {
    /// 设置菜单里的可选项顺序。
    pub const ALL: [ThemeMode; 2] = [Self::Light, Self::Dark];

    /// 设置菜单显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "浅色",
            Self::Dark => "深色",
        }
    }

    /// 反向模式:「切换主题」命令用;设置菜单的定向选择直接给目标模式。
    pub fn opposite(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

/// 主题设置:模式 + 正文样式覆盖。缺省字段(含整个 `overrides`)回落默认,
/// 手改的配置文件缺项不致整体解析失败。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    /// 明暗模式(外壳与正文共同跟随)。
    pub mode: ThemeMode,
    /// 正文样式覆盖;`None` = vendored 默认(P0 批次 A 不覆盖)。
    pub overrides: Option<MarkdownStyle>,
}

impl ThemeSettings {
    /// 启动时装载:平台默认目录;首次运行(无文件)静默用默认,文件在但解析
    /// 失败则终端告警后回落默认 —— 坏配置不该挡住应用启动。
    pub fn load() -> Self {
        let Some(dir) = config_dir() else {
            return Self::default();
        };
        match Self::load_from(&dir) {
            Ok(settings) => settings,
            Err(LoadError::Missing) => Self::default(),
            Err(LoadError::Corrupt(source)) => {
                eprintln!("LaterMD: 主题配置解析失败,已回落默认: {source}");
                Self::default()
            }
        }
    }

    /// 把主题投影到 context:切 egui 主题(外壳)并安装正文样式。幂等且带
    /// staleness 检查,每帧调用时空闲帧近零开销。egui 0.36 中 theme 属于
    /// options 数据,`logic` 阶段写入合法(不是绘制)。
    pub fn apply(&self, ctx: &egui::Context) {
        let theme = match self.mode {
            ThemeMode::Light => egui::Theme::Light,
            ThemeMode::Dark => egui::Theme::Dark,
        };
        if ctx.theme() != theme {
            ctx.set_theme(theme);
        }
        let wanted = self.markdown_style();
        if *egui_markdown_style::global_style(ctx) != wanted {
            egui_markdown_style::set_style(ctx, wanted);
        }
    }

    /// 生效的正文样式:`overrides` 或 vendored 默认。
    pub fn markdown_style(&self) -> MarkdownStyle {
        self.overrides.clone().unwrap_or_default()
    }

    /// 落盘到 `<dir>/settings.json`;`dir` 为 `None` 时用平台默认目录。
    /// 目录不存在则创建。失败带路径,提示行可直接展示。
    pub fn save_to(&self, dir: Option<&Path>) -> Result<(), SaveError> {
        let Some(dir) = dir.map(Path::to_path_buf).or_else(config_dir) else {
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

    /// 从指定目录读取;`dir` 存在性由调用方保证语义(不存在 = 首次运行)。
    fn load_from(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join(SETTINGS_FILE);
        let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => LoadError::Missing,
            _ => LoadError::Corrupt(format!("{}: {}", path.display(), source)),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|source| LoadError::Corrupt(format!("{}: {}", path.display(), source)))
    }
}

/// 读配置的失败情形。
#[derive(Debug)]
enum LoadError {
    /// 文件不存在:首次运行,正常路径。
    Missing,
    /// 文件在但读不出/解析不了,字符串已带路径与原因。
    Corrupt(String),
}

/// 写配置失败:带路径,可直接进提示行(与 `crate::file::FileError` 同构,
/// 单独成类型是因为这里只此一处落盘且无 `io::Error` 之外的固定动作名)。
#[derive(Debug)]
pub struct SaveError {
    path: PathBuf,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "主题保存失败 {}: {}", self.path.display(), self.source)
    }
}

/// 平台配置目录(手写,不引目录库):
/// Linux/其余 Unix `$XDG_CONFIG_HOME/latermd`(未设置或为空串时
/// `$HOME/.config/latermd`,按 XDG 规范);macOS
/// `$HOME/Library/Application Support/latermd`;Windows `%APPDATA%\latermd`。
///
/// 文件树的最近目录持久化(`crate::filetree`)落在同一目录,故 crate 内共享。
pub(crate) fn config_dir() -> Option<PathBuf> {
    config_dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
        std::env::var_os("APPDATA"),
    )
}

/// [`config_dir`] 的纯函数核,env 值由参数注入(测试不碰进程环境变量)。
fn config_dir_from(
    xdg: Option<OsString>,
    home: Option<OsString>,
    appdata: Option<OsString>,
) -> Option<PathBuf> {
    let into_path = |value: OsString| PathBuf::from(value);
    if cfg!(target_os = "macos") {
        home.map(into_path)
            .map(|home| home.join("Library/Application Support/latermd"))
    } else if cfg!(target_os = "windows") {
        appdata
            .map(into_path)
            .map(|appdata| appdata.join("latermd"))
    } else {
        xdg.filter(|dir| !dir.is_empty())
            .map(into_path)
            .or_else(|| home.map(into_path).map(|home| home.join(".config")))
            .map(|dir| dir.join("latermd"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("latermd-theme-{}-{name}", std::process::id()))
    }

    /// Linux 分支:XDG 优先;空串按 XDG 规范视同未设置,回落 ~/.config;
    /// HOME 也无则给不出目录。
    /// (macOS/Windows 分支是各两行的 join,由 `cfg!` 选择,本机测不到。)
    #[test]
    fn config_dir_prefers_xdg_then_home() {
        let xdg = |v: &str| Some(OsString::from(v));
        let home = || Some(OsString::from("/home/u"));

        assert_eq!(
            config_dir_from(xdg("/custom/cfg"), home(), None),
            Some(PathBuf::from("/custom/cfg/latermd"))
        );
        assert_eq!(
            config_dir_from(xdg(""), home(), None),
            Some(PathBuf::from("/home/u/.config/latermd")),
            "XDG 空串 = 未设置"
        );
        assert_eq!(
            config_dir_from(None, home(), None),
            Some(PathBuf::from("/home/u/.config/latermd"))
        );
        assert_eq!(config_dir_from(None, None, None), None);
    }

    /// 往返:模式与 overrides(含颜色对、字号、代码语言等全部字段)逐项一致;
    /// 目录不存在时 save 会建目录。
    #[test]
    fn save_load_round_trip_preserves_settings() {
        use egui_markdown_style::{HeadingStyle, InlineCodeStyle};

        let dir = temp_dir("roundtrip/nested");
        let defaults = MarkdownStyle::default();
        let settings = ThemeSettings {
            mode: ThemeMode::Light,
            overrides: Some(MarkdownStyle {
                heading: HeadingStyle {
                    scales: [2.0, 1.35, 1.2, 1.1, 1.05, 1.0],
                },
                block_spacing: 13.0,
                inline_code: InlineCodeStyle {
                    color_light: egui::Color32::from_rgb(1, 2, 3),
                    ..defaults.inline_code
                },
                default_code_language: "rust".to_owned(),
                ..defaults
            }),
        };

        settings.save_to(Some(&dir)).unwrap();
        assert_eq!(ThemeSettings::load_from(&dir).unwrap(), settings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 首次运行:目录/文件不存在 → Missing;`load` 语义是回落默认,这里只测
    /// 判别本身(不调 `load`,它走真实平台目录)。
    #[test]
    fn missing_file_is_missing_not_corrupt() {
        assert!(matches!(
            ThemeSettings::load_from(&temp_dir("absent")),
            Err(LoadError::Missing)
        ));
    }

    /// 坏 JSON 判为 Corrupt 且消息带路径;不 panic。
    #[test]
    fn corrupt_json_is_reported_with_path() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), b"{oops").unwrap();
        match ThemeSettings::load_from(&dir) {
            Err(LoadError::Corrupt(message)) => {
                assert!(message.contains(SETTINGS_FILE), "{message}")
            }
            other => panic!("期望 Corrupt,得到 {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 手改配置只写 mode:serde(default) 让缺省字段回落,overrides 为 None。
    #[test]
    fn partial_json_fills_defaults() {
        let dir = temp_dir("partial");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(SETTINGS_FILE), br#"{"mode":"light"}"#).unwrap();
        assert_eq!(
            ThemeSettings::load_from(&dir).unwrap(),
            ThemeSettings {
                mode: ThemeMode::Light,
                overrides: None,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 投影:切 egui 主题(context 随即按 light/dark 选 visuals),并把
    /// overrides 装进 context 默认槽;默认深色;幂等。
    #[test]
    fn apply_switches_theme_and_installs_markdown_style() {
        let ctx = egui::Context::default();
        assert_eq!(ctx.theme(), egui::Theme::Dark, "egui 默认深色");

        let overrides = MarkdownStyle {
            block_spacing: 11.0,
            ..Default::default()
        };
        ThemeSettings {
            mode: ThemeMode::Light,
            overrides: Some(overrides),
        }
        .apply(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(!ctx.global_style().visuals.dark_mode);
        assert_eq!(egui_markdown_style::global_style(&ctx).block_spacing, 11.0);

        // 无 overrides 时装的就是 vendored 默认;重复 apply 幂等
        ThemeSettings::default().apply(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(
            *egui_markdown_style::global_style(&ctx),
            MarkdownStyle::default()
        );
        ThemeSettings::default().apply(&ctx);
    }
}
