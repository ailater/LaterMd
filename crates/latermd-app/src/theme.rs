//! 主题系统(P0 批次 A + P2.5 批次 B):Light/Dark/跟随系统 三态 + 皮肤文件 + 密度。
//!
//! [`ThemeSettings`] 是主题的唯一事实源,向三处投影(`ThemeSettings::apply`):
//!
//! - **外壳**:`egui::Context::set_theme` 切换 egui 自带的 light/dark 双套
//!   `Style`/`Visuals`(egui 0.36 按主题各持一份,面板/控件全部跟随);
//! - **外壳 token**:密度(标准/紧凑)改写 spacing 与圆角 —— 批次 B 的视觉
//!   打磨落点,不做每控件粒度自定义(roadmap 专题「明确不做」);
//! - **正文**:vendored `MarkdownStyle` 装进 context 默认槽
//!   (`egui_markdown_style::set_style`),其颜色字段本就成对设计
//!   (`color_dark`/`color_light`),渲染时按 `ui.visuals().dark_mode` 自动取值;
//!   代码高亮不指定 theme 时同样按 dark/light 自动选择。
//!
//! 联动零额外成本:MarkdownStyle 布局缓存 hash 已含 dark_mode,切换自动失效。
//!
//! 三态里的 `System` 由**调用方**解析(见 [`detect_system_mode`] 与
//! `crate::state` 的轮询):本模块不持有检测结果 —— 主题检测要查系统设置
//! (Linux 走 dbus),不该每帧发生。
//!
//! 持久化:平台配置目录下手写路径(不引目录库),JSON 经 serde 往返;皮肤
//! 文件则是同目录下 `themes/*.ron`(皮肤可手写、可分享,故用 RON 而非 JSON:
//! 支持注释与无引号键名)。

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use eframe::egui;
use eframe::egui::Color32;
use egui_markdown_style::MarkdownStyle;
use serde::{Deserialize, Serialize};

/// 配置文件名,落在平台配置目录下。
const SETTINGS_FILE: &str = "settings.json";

/// 皮肤目录名(配置目录下);每个 `.ron` 文件是一份 `MarkdownStyle`。
pub const THEMES_DIR: &str = "themes";

/// 明暗模式(用户的选择,含「跟随系统」这一非确定值)。serde 小写
/// (`"light"`/`"dark"`/`"system"`);默认深色,与 egui 的默认 visuals 一致,
/// 首跑无闪变。
///
/// `System` **不是**一种可渲染的模式:绘制前必须经 [`ThemeMode::resolve`]
/// 落到 Light/Dark(roadmap 风险 #8:Linux 无统一规范,检测可能失灵,届时
/// 由 resolve 的 `detected: None` 分支回落)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// 浅色。
    Light,
    /// 深色。
    #[default]
    Dark,
    /// 跟随系统(Windows 注册表 / macOS NSUserDefaults / Linux freedesktop
    /// portal)。检测不到时按 `fallback` 走。
    System,
}

impl ThemeMode {
    /// 设置页里的可选项顺序。
    pub const ALL: [ThemeMode; 3] = [Self::Light, Self::Dark, Self::System];

    /// 设置页显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "浅色",
            Self::Dark => "深色",
            Self::System => "跟随系统",
        }
    }

    /// 把模式解析成一个**可渲染**的明暗值:定向选择原样返回;`System` 取
    /// 检测结果,`detected` 为 `None`(未检测或检测失败)时回落 `fallback`。
    ///
    /// `fallback` 由调用方传最近一次成功的检测值(没有则给出厂默认),这样
    /// 一次检测失败不会让界面在深浅之间跳变。
    pub fn resolve(self, detected: Option<ThemeMode>, fallback: ThemeMode) -> ThemeMode {
        match self {
            Self::Light => Self::Light,
            Self::Dark => Self::Dark,
            Self::System => match detected {
                Some(Self::Light) | Some(Self::Dark) => detected.unwrap_or(fallback),
                // None 与 System 本身都不是可渲染值
                _ => match fallback {
                    Self::Light | Self::Dark => fallback,
                    _ => Self::Dark,
                },
            },
        }
    }

    /// 反向模式:「切换主题」命令用;设置页的定向选择直接给目标模式。
    /// `System` 的反向由调用方按当前解析结果决定(见 `crate::state`)。
    pub fn opposite(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark | Self::System => Self::Light,
        }
    }
}

/// 探测系统当前是深是浅。
///
/// 失败与 `Mode::Unspecified` 都返回 `None` —— Linux 上没有统一规范
/// (GNOME gsettings / KDE / portal 各异),拿不到答案时**如实返回不知道**,
/// 由调用方回落到上一次的手动选择,不做猜测。
pub fn detect_system_mode() -> Option<ThemeMode> {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => Some(ThemeMode::Dark),
        Ok(dark_light::Mode::Light) => Some(ThemeMode::Light),
        // Unspecified:系统给了「未指定」;Err:dbus/注册表不可用
        Ok(dark_light::Mode::Unspecified) | Err(_) => None,
    }
}

/// 界面密度(批次 B 的视觉打磨项之一)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    /// 标准:出厂间距与控件高度。
    #[default]
    Standard,
    /// 紧凑:更窄的项间距、更小的按钮内边距与圆角,长文档一屏多看几行。
    Compact,
}

impl Density {
    /// 设置页可选项顺序。
    pub const ALL: [Density; 2] = [Self::Standard, Self::Compact];

    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "标准",
            Self::Compact => "紧凑",
        }
    }
}

/// 主题设置:模式 + 皮肤 + 密度 + 正文样式覆盖。缺省字段(含整个
/// `overrides`)回落默认,手改的配置文件缺项不致整体解析失败。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeSettings {
    /// 明暗模式(外壳与正文共同跟随)。
    pub mode: ThemeMode,
    /// 选中的皮肤文件名(不含扩展名);`None` = 用 `overrides` 或出厂默认。
    pub skin: Option<String>,
    /// 界面密度。
    pub density: Density,
    /// 正文样式覆盖;`None` = vendored 默认(P0 批次 A 不覆盖)。
    pub overrides: Option<MarkdownStyle>,
    /// 当前皮肤的**内容**:由 `skin` 名字从磁盘载入,**不落盘**
    /// (避免同一份样式在 settings.json 与皮肤文件里各存一份、改了一处另一处
    /// 不跟着变)。
    #[serde(skip)]
    pub skin_style: Option<MarkdownStyle>,
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

    /// 把主题投影到 context:切 egui 主题(外壳)+ 密度 token + 安装正文样式。
    /// 幂等且带 staleness 检查,每帧调用时空闲帧近零开销。egui 0.36 中 theme
    /// 属于 options 数据,`logic` 阶段写入合法(不是绘制)。
    ///
    /// `resolved` 是 [`ThemeMode::resolve`] 的结果(已把 `System` 落到确定的
    /// 明暗):本模块不做系统检测 —— 那要查 dbus/注册表,不该每帧发生。
    pub fn apply(&self, ctx: &egui::Context, resolved: ThemeMode) {
        let theme = match resolved {
            ThemeMode::Light | ThemeMode::System => egui::Theme::Light,
            ThemeMode::Dark => egui::Theme::Dark,
        };
        if ctx.theme() != theme {
            ctx.set_theme(theme);
        }
        apply_shell(ctx);
        apply_density(ctx, self.density);
        let wanted = self.markdown_style();
        if *egui_markdown_style::global_style(ctx) != wanted {
            egui_markdown_style::set_style(ctx, wanted);
        }
    }

    /// 生效的正文样式:皮肤 > `overrides` > vendored 默认。
    ///
    /// 皮肤优先的理由:皮肤是用户显式选的「这一整套」,手改 settings.json 的
    /// `overrides` 是兜底通道(旧配置与手工微调),两者都出现时以显式选择为准。
    pub fn markdown_style(&self) -> MarkdownStyle {
        self.skin_style
            .clone()
            .or_else(|| self.overrides.clone())
            .unwrap_or_default()
    }

    /// 装上选中的皮肤内容;名字不在目录里时清空(皮肤被删/改名后自动回落
    /// 默认,不留在「选了一个不存在的皮肤」的状态)。
    pub fn select_skin(&mut self, name: Option<&str>, catalog: &SkinCatalog) {
        match name {
            None => {
                self.skin = None;
                self.skin_style = None;
            }
            Some(name) => match catalog.find(name) {
                Some(skin) => {
                    self.skin = Some(name.to_owned());
                    self.skin_style = Some(skin.style.clone());
                }
                None => {
                    self.skin = None;
                    self.skin_style = None;
                }
            },
        }
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
    /// `pub(crate)`:测试与启动路径都按目录注入,不走平台默认目录。
    pub(crate) fn load_from(dir: &Path) -> Result<Self, LoadError> {
        let path = dir.join(SETTINGS_FILE);
        let bytes = std::fs::read(&path).map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => LoadError::Missing,
            _ => LoadError::Corrupt(format!("{}: {}", path.display(), source)),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|source| LoadError::Corrupt(format!("{}: {}", path.display(), source)))
    }
}

/// WorkBuddy 风外壳 token(2026-09-26 坤哥指定方向;浅色取自截图采样:
/// 侧栏/顶栏 #F2F2F2、内容纯白、**无硬边框**,靠底色分区)。
///
/// 这是对 egui 内置 light/dark visuals 的**投影**(不再是"出厂默认"),
/// 亮暗各一套;皮肤文件仍只管正文(批次 C 的"外壳随皮肤"依旧不做 ——
/// 这里是内置观感,不是皮肤系统)。
pub struct ShellTokens {
    /// 侧边栏 / 顶栏 / 菜单栏的底。
    pub sidebar: Color32,
    /// 编辑器与预览的内容区底。
    pub content: Color32,
    /// 主文字。
    pub text: Color32,
    /// 次要文字(提示行、占位)。
    pub secondary: Color32,
    /// 悬停底(白底上)。
    pub hover: Color32,
    /// 选中底(浅蓝,列表/页签选中)。
    pub selected_bg: Color32,
    /// 强调(链接、选中文字、活动页签)。
    pub accent: Color32,
    /// 分隔线与描边(弱,尽量少用)。
    pub border: Color32,
    /// 代码块 / 行内代码底。
    pub code_bg: Color32,
    /// 更弱一档的底(斑马纹、禁用区)。
    pub faint: Color32,
}

/// 取当前明暗的 shell token。
pub fn shell_tokens(dark: bool) -> ShellTokens {
    if dark {
        ShellTokens {
            sidebar: Color32::from_rgb(0x20, 0x21, 0x24),
            content: Color32::from_rgb(0x29, 0x2A, 0x2D),
            text: Color32::from_rgb(0xE8, 0xEA, 0xED),
            secondary: Color32::from_rgb(0x9A, 0xA0, 0xA6),
            hover: Color32::from_rgb(0x35, 0x37, 0x3A),
            selected_bg: Color32::from_rgb(0x2B, 0x3F, 0x5E),
            accent: Color32::from_rgb(0x6C, 0x9F, 0xFF),
            border: Color32::from_rgb(0x3C, 0x40, 0x43),
            code_bg: Color32::from_rgb(0x23, 0x24, 0x27),
            faint: Color32::from_rgb(0x2A, 0x2B, 0x2E),
        }
    } else {
        ShellTokens {
            sidebar: Color32::from_rgb(0xF2, 0xF3, 0xF5),
            content: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            text: Color32::from_rgb(0x1F, 0x23, 0x29),
            secondary: Color32::from_rgb(0x64, 0x6A, 0x73),
            hover: Color32::from_rgb(0xF2, 0xF3, 0xF5),
            selected_bg: Color32::from_rgb(0xE1, 0xEF, 0xFF),
            accent: Color32::from_rgb(0x33, 0x70, 0xFF),
            border: Color32::from_rgb(0xE5, 0xE6, 0xEB),
            code_bg: Color32::from_rgb(0xF5, 0xF6, 0xF7),
            faint: Color32::from_rgb(0xFA, 0xFB, 0xFC),
        }
    }
}

/// 内容区的底(编辑器与预览面板显式 `.fill`;侧栏吃 `panel_fill`)。
pub fn content_fill(dark: bool) -> Color32 {
    shell_tokens(dark).content
}

/// 把 WorkBuddy 外壳 token 投影进 egui 的两套 style(亮暗各一)。
///
/// 只投影一次(`ui.data` 记标志):`style_mut_of` 即使值相同也会推进 style
/// 版本、作废布局缓存,每帧调用不可接受。
fn apply_shell(ctx: &egui::Context) {
    let id = egui::Id::new("latermd-shell");
    let changed = ctx.data_mut(|data| {
        let done = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, true);
        !done
    });
    if !changed {
        return;
    }
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, apply_shell_to);
    }
}

fn apply_shell_to(style: &mut egui::Style) {
    let c = shell_tokens(style.visuals.dark_mode);
    let v = &mut style.visuals;
    v.panel_fill = c.sidebar;
    v.window_fill = c.content;

    v.extreme_bg_color = c.code_bg;
    v.faint_bg_color = c.faint;
    v.hyperlink_color = c.accent;
    // 选区:浅蓝底、无线(egui 默认蓝底蓝框太重)
    v.selection.bg_fill = c.selected_bg;
    v.selection.stroke = egui::Stroke::NONE;
    // 圆角:控件 6(WorkBuddy 的圆润感)。0.36 的窗口/菜单圆角字段已不在
    // Visuals/Spacing 的公开面,浮窗圆角走 egui 出厂值,不做覆盖
    // 分隔线弱化:panel 之间靠底色分区,线只在必要时出现
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, c.border);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, c.text);
    v.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    let widgets = [
        (&mut v.widgets.inactive, c.text, Color32::TRANSPARENT),
        (&mut v.widgets.hovered, c.text, c.hover),
        (&mut v.widgets.active, c.accent, c.selected_bg),
        (&mut v.widgets.open, c.text, c.hover),
    ];
    for (widget, fg, bg) in widgets {
        widget.fg_stroke = egui::Stroke::new(1.0, fg);
        widget.bg_fill = bg;
        widget.weak_bg_fill = bg;
        widget.corner_radius = egui::CornerRadius::same(6);
        widget.bg_stroke = egui::Stroke::NONE;
    }
    // 输入框/按钮内的弱文字(占位符)用次要色
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    let _ = c.secondary; // 占位符色由 egui 的 weak_fg 承接,这里保持默认层级
}

/// 密度 → egui style token。
///
/// 只在**密度真的变了**时写 style(写入会让 egui 重算布局缓存),且一律以
/// `egui::Style::default()` 的出厂值为基准做缩放 —— 不基于「当前值」再乘,
/// 否则标准↔紧凑来回切会逐次累积。
fn apply_density(ctx: &egui::Context, density: Density) {
    let id = egui::Id::new("latermd-density");
    let changed = ctx.data_mut(|data| {
        let changed = data.get_temp::<Density>(id) != Some(density);
        data.insert_temp(id, density);
        changed
    });
    if !changed {
        return;
    }
    let (scale, rounding_scale) = match density {
        Density::Standard => (1.0_f32, 1.0_f32),
        // 紧凑:间距/控件高度压到七成,圆角略收 —— 长文档一屏多看几行,
        // 但不改字号(改字号会牺牲中文可读性)
        Density::Compact => (0.7_f32, 0.8_f32),
    };
    let base = egui::Style::default();
    let margin = base.spacing.window_margin;
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = base.spacing.item_spacing * scale;
        style.spacing.button_padding = base.spacing.button_padding * scale;
        style.spacing.interact_size = base.spacing.interact_size * scale;
        style.spacing.indent = base.spacing.indent * scale;
        // 滚动条:紧凑模式下收窄,让出的宽度归正文(egui 0.36 的滚动条
        // 参数在 `spacing.scroll` 里,不再是单个 bar_width 字段)
        style.spacing.scroll.bar_width = base.spacing.scroll.bar_width * scale;
        style.spacing.scroll.bar_inner_margin = base.spacing.scroll.bar_inner_margin * scale;
        // Margin 的字段是 i8(egui 0.36),先转 f32 再缩放取整
        let scaled = |value: i8| (f32::from(value) * scale).round() as i8;
        style.spacing.window_margin = egui::Margin {
            left: scaled(margin.left),
            right: scaled(margin.right),
            top: scaled(margin.top),
            bottom: scaled(margin.bottom),
        };
        // 只取基准圆角的数值(WidgetVisuals 不是 Copy,整块搬不进闭包)
        let base_widgets = &base.visuals.widgets;
        let radii = [
            base_widgets.noninteractive.corner_radius,
            base_widgets.inactive.corner_radius,
            base_widgets.hovered.corner_radius,
            base_widgets.active.corner_radius,
            base_widgets.open.corner_radius,
        ];
        let widgets = &mut style.visuals.widgets;
        for (widget, radius) in [
            (&mut widgets.noninteractive, radii[0]),
            (&mut widgets.inactive, radii[1]),
            (&mut widgets.hovered, radii[2]),
            (&mut widgets.active, radii[3]),
            (&mut widgets.open, radii[4]),
        ] {
            widget.corner_radius = radius * rounding_scale;
        }
    });
}

/// 一份皮肤:显示名(文件名去扩展名)+ 正文样式。
#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub name: String,
    pub style: MarkdownStyle,
}

/// 皮肤目录的内容(启动扫描一次,换皮肤/导出时重扫)。
///
/// 为什么是目录扫描而不是配置里列清单:皮肤是**文件**,用户可以直接把别人
/// 给的 `.ron` 丢进目录,不该还得再改一次 settings.json。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinCatalog {
    /// 按名字排序。
    pub skins: Vec<Skin>,
}

impl SkinCatalog {
    /// 扫描 `<dir>/themes/*.ron`;目录不存在视为空(首次运行)。坏文件跳过
    /// 并在终端告警 —— 一个坏皮肤不该挡住应用启动。
    pub fn load_from(dir: &Path) -> Self {
        let themes = dir.join(THEMES_DIR);
        let Ok(entries) = std::fs::read_dir(&themes) else {
            return Self::default();
        };
        let mut skins: Vec<Skin> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "ron"))
            .filter_map(|entry| {
                let path = entry.path();
                let text = std::fs::read_to_string(&path).ok()?;
                match ron::from_str::<MarkdownStyle>(&text) {
                    Ok(style) => Some(Skin {
                        name: entry
                            .path()
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        style,
                    }),
                    Err(error) => {
                        eprintln!(
                            "LaterMD: 皮肤文件解析失败,已跳过 {}: {error}",
                            path.display()
                        );
                        None
                    }
                }
            })
            .collect();
        skins.sort_by(|left, right| left.name.cmp(&right.name));
        Self { skins }
    }

    pub fn find(&self, name: &str) -> Option<&Skin> {
        self.skins.iter().find(|skin| skin.name == name)
    }
}

/// 皮肤文件名:清掉路径分隔符与 Windows 保留字符,空名给默认名。
///
/// 名字直接来自用户输入且要拼进路径,`..` 与 `/` 必须挡住(与 MCP 的路径
/// 校验同一思路:别让用户输入的字符串变成路径遍历)。
pub fn skin_file_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '.' => '_',
            other => other,
        })
        .collect();
    if cleaned.is_empty() {
        "my-theme".to_owned()
    } else {
        cleaned
    }
}

/// 把一份样式导出成皮肤文件(RON);目录不存在则创建。返回落盘路径。
pub fn export_skin(dir: &Path, name: &str, style: &MarkdownStyle) -> Result<PathBuf, String> {
    let path = dir
        .join(THEMES_DIR)
        .join(format!("{}.ron", skin_file_name(name)));
    let text = ron::ser::to_string_pretty(style, ron::ser::PrettyConfig::default())
        .map_err(|error| format!("皮肤序列化失败: {error}"))?;
    std::fs::create_dir_all(path.parent().unwrap_or(dir))
        .map_err(|error| format!("{}: {error}", dir.display()))?;
    std::fs::write(&path, text.as_bytes())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(path)
}

/// 读配置的失败情形(公开:`load_from` 已是 `pub(crate)`,私有返回类型会
/// 触发 private_interfaces)。
#[derive(Debug)]
pub(crate) enum LoadError {
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
            ..ThemeSettings::default()
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
                ..ThemeSettings::default()
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
            ..ThemeSettings::default()
        }
        .apply(&ctx, ThemeMode::Light);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert!(!ctx.global_style().visuals.dark_mode);
        assert_eq!(egui_markdown_style::global_style(&ctx).block_spacing, 11.0);

        // 无 overrides 时装的就是 vendored 默认;重复 apply 幂等
        ThemeSettings::default().apply(&ctx, ThemeMode::Dark);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(
            *egui_markdown_style::global_style(&ctx),
            MarkdownStyle::default()
        );
        ThemeSettings::default().apply(&ctx, ThemeMode::Dark);
    }

    /// 三态解析:定向选择原样返回;`System` 取检测结果,检测不到回落
    /// fallback(Linux 无统一规范时的既定行为,roadmap 风险 #8)。
    #[test]
    fn resolve_maps_system_to_detection_or_fallback() {
        assert_eq!(
            ThemeMode::Light.resolve(Some(ThemeMode::Dark), ThemeMode::Dark),
            ThemeMode::Light,
            "定向选择不看系统"
        );
        assert_eq!(
            ThemeMode::System.resolve(Some(ThemeMode::Light), ThemeMode::Dark),
            ThemeMode::Light
        );
        assert_eq!(
            ThemeMode::System.resolve(None, ThemeMode::Dark),
            ThemeMode::Dark,
            "检测失败回落手动值"
        );
        // fallback 本身是 System 时(极端手改配置)给确定值,不把 System 传下去
        assert_eq!(
            ThemeMode::System.resolve(None, ThemeMode::System),
            ThemeMode::Dark
        );
    }

    /// 系统探测在本机不 panic;结果只可能是 None 或确定的明暗(Unspecified
    /// 与 Err 都已折叠为 None)。真机判定的实测记录由人工补。
    #[test]
    fn system_detection_never_panics() {
        let detected = detect_system_mode();
        assert!(detected.is_none_or(|mode| mode == ThemeMode::Light || mode == ThemeMode::Dark));
    }

    /// 皮肤文件:导出 → 扫描 → 命中;名字里的路径分隔符被清掉(用户输入
    /// 直接拼进路径,必须挡住 `..`)。
    #[test]
    fn skin_round_trip_through_themes_dir() {
        let dir = temp_dir("skins");
        let style = MarkdownStyle {
            block_spacing: 17.0,
            ..MarkdownStyle::default()
        };
        let path = export_skin(&dir, "我的皮肤", &style).unwrap();
        assert!(path.ends_with("我的皮肤.ron"), "{path:?}");

        let catalog = SkinCatalog::load_from(&dir);
        assert_eq!(catalog.skins.len(), 1);
        assert_eq!(catalog.skins[0].name, "我的皮肤");
        assert_eq!(catalog.skins[0].style.block_spacing, 17.0);
        assert!(catalog.find("不存在").is_none());

        // 选中 → 内容进内存;选一个不存在的名字 → 回落无皮肤
        let mut settings = ThemeSettings::default();
        settings.select_skin(Some("我的皮肤"), &catalog);
        assert_eq!(settings.markdown_style().block_spacing, 17.0);
        settings.select_skin(Some("不存在"), &catalog);
        assert_eq!(settings.skin, None);
        assert_eq!(settings.markdown_style(), MarkdownStyle::default());

        // 坏文件与空目录都不 panic
        std::fs::write(dir.join(THEMES_DIR).join("bad.ron"), b"(((").unwrap();
        assert_eq!(SkinCatalog::load_from(&dir).skins.len(), 1, "坏文件跳过");
        assert_eq!(SkinCatalog::load_from(&temp_dir("absent")).skins.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 皮肤文件名清洗:`..` 与各路分隔符都换掉,空名给默认。
    #[test]
    fn skin_file_name_is_sanitized() {
        assert_eq!(skin_file_name("  "), "my-theme");
        assert_eq!(skin_file_name("../../etc/passwd"), "______etc_passwd");
        assert_eq!(skin_file_name("a:b?c"), "a_b_c");
    }

    /// 密度:切到紧凑后间距真变小,切回标准恢复出厂值(不累积缩放)。
    #[test]
    fn density_compacts_spacing_and_restores() {
        let ctx = egui::Context::default();
        let standard = ctx.style_of(egui::Theme::Dark).spacing.item_spacing;

        let compact = ThemeSettings {
            density: Density::Compact,
            ..ThemeSettings::default()
        };
        compact.apply(&ctx, ThemeMode::Dark);
        let compacted = ctx.style_of(egui::Theme::Dark).spacing.item_spacing;
        assert!(compacted.y < standard.y, "{compacted:?} vs {standard:?}");

        ThemeSettings::default().apply(&ctx, ThemeMode::Dark);
        let restored = ctx.style_of(egui::Theme::Dark).spacing.item_spacing;
        assert_eq!(restored, standard, "来回切换不累积缩放");
    }
}
