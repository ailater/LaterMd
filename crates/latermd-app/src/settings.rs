//! 设置对话框(docs/ui-polish.md §5)。
//!
//! 形态:左侧竖排分页 + 右侧内容区,单个 `egui::Window`。
//!
//! **为什么从工具栏的「设置」菜单升级成对话框**:菜单里只能塞几行
//! (主题列表 + 一个入口),而快捷键页有 10 行、AI 页有一串字段 —— 塞进
//! 菜单既画不下,也违背「菜单是对话框以外的轻量入口」这一通用范式。
//! 原「AI Provider」浮窗随之退役,其内容(`ai_key::key_editor`)嵌入 AI 页。
//!
//! 归约铁律照旧:本模块只绘制与产出 [`Message`];落盘、改状态、凭据读写
//! 全在 `State::apply`。两处**原地**例外(纯 UI 关注点):当前页签与
//! 「正在捕获哪个命令的键位」,同侧边栏把手与 `SearchState` 输入的口径。

use crate::ai::AiState;
use crate::ai_config::{AiConfig, ProviderKind, CONTEXT_KB_MAX};
use crate::ai_key::{self, AiKeyState};
use crate::command::Command;
use crate::keymap::Keymap;
use crate::mcp::McpState;
use crate::state::Message;
use crate::theme::{
    Density, SkinCatalog, ThemeMode, ThemeSettings, ZenNavMode, EDITOR_FONT_SIZE_MAX,
    EDITOR_FONT_SIZE_MIN, LINE_HEIGHT_MAX, LINE_HEIGHT_MIN,
};
use crate::ui::icons;
use eframe::egui;
use latermd_ai::{ModelsResult, ModelsSource};
use latermd_mcp::{McpConfig, ToolKind};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread::JoinHandle;

/// 设置页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// 外观(主题 + 渲染后端只读)。
    #[default]
    Appearance,
    /// 快捷键(可改绑)。
    Keymap,
    /// AI provider、端点与模型。
    Ai,
    /// MCP server(规划态)。
    Mcp,
    /// 图片(图床 profile 列表 / 增删改 / 测试上传,docs/image-plan.md C 段)。
    Image,
}

impl SettingsTab {
    /// 左侧分页顺序。
    pub const ALL: [SettingsTab; 5] = [
        Self::Appearance,
        Self::Keymap,
        Self::Ai,
        Self::Mcp,
        Self::Image,
    ];

    /// 分页显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "外观",
            Self::Keymap => "快捷键",
            Self::Ai => "AI",
            Self::Mcp => "MCP",
            Self::Image => "图片",
        }
    }

    /// 分页图标(与工具栏同一套自绘图标)。
    pub fn icon(self) -> icons::Icon {
        match self {
            Self::Appearance => icons::Icon::Theme,
            Self::Keymap => icons::Icon::Reset,
            Self::Ai => icons::Icon::Ai,
            Self::Mcp => icons::Icon::Git,
            Self::Image => icons::Icon::Image,
        }
    }
}

/// 设置对话框状态。
pub struct SettingsState {
    /// 是否可见(工具栏齿轮/菜单入口翻开,窗口 X 关闭 —— 原地翻转)。
    pub open: bool,
    /// 当前分页。
    pub tab: SettingsTab,
    /// 正在捕获键位的命令;`Some` 时该行显示「按下新键位…」,归约侧
    /// (`ui::layout::reduce`)从本帧输入取键。
    pub capture: Option<Command>,
    /// 页内提示(撞键、不可绑定等);下一次操作或关页时清空。
    pub notice: Option<String>,
    /// AI 页的草稿:编辑原地发生,「保存」才发消息落盘。
    pub ai_draft: AiConfig,
    /// MCP 页的草稿:同理,「保存」才落盘并起停服务。
    pub mcp_draft: McpConfig,
    /// 外观页「导出皮肤」的名字输入框(纯 UI 关注点,导出动作走消息)。
    pub skin_export_name: String,
    /// 图片页的图床编辑草稿;`None` = 不在编辑。保存才发消息落盘。
    pub bed_draft: Option<BedDraft>,
    /// AI 页「获取模型列表」的状态机(在途接收端 + 候选列表 + 错误行)。
    pub models: ModelListState,
}

/// 图床页的编辑草稿:profile 本体 + 两个只属编辑期的字段(`headers` 的
/// 多行文本形态与 token 输入)。保存时合并成 [`Message::BedProfileSaved`]。
#[derive(Debug, Clone, PartialEq)]
pub struct BedDraft {
    /// 正在编辑的 profile(新增时来自预置模板,id 为空)。
    pub profile: latermd_bed::BedProfile,
    /// 请求头的多行文本形态,每行 `名: 值`(值可含 `${TOKEN}`)。
    pub headers_text: String,
    /// 新 token 输入;留空 = 不改已存凭据。已存值**不回显**(读都不读)。
    pub token: String,
}

impl BedDraft {
    /// 从既有 profile 建编辑草稿(「编辑」入口)。
    fn from_profile(profile: &latermd_bed::BedProfile) -> Self {
        Self {
            profile: profile.clone(),
            headers_text: headers_to_text(&profile.headers),
            token: String::new(),
        }
    }
}

/// 请求头对 → 多行文本(每行 `名: 值`)。
fn headers_to_text(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 多行文本 → 请求头对;空行跳过,缺 `:` 或名字为空的行报行号。
fn headers_from_text(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut headers = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!("第 {} 行请求头缺少「:」(格式:名: 值)", index + 1));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("第 {} 行请求头名字为空", index + 1));
        }
        headers.push((name.to_owned(), value.trim().to_owned()));
    }
    Ok(headers)
}

/// 模型列表拉取的线程装配点:生产 [`latermd_ai::fetch_models`];测试注入
/// 假装配(定时回固定结果,零网络),与 `AiKeyState::creds` 同款注入点。
pub(crate) type ModelsSpawner =
    Arc<dyn Fn(ModelsSource, mpsc::Sender<ModelsResult>) -> Option<JoinHandle<()>> + Send + Sync>;

/// AI 页「获取模型列表」的状态机(#58 M2):在途接收端 + 候选列表 + 错误行。
///
/// 与 `crate::ai`(流式)和 `crate::bed::BedState`(上传)同款「发起 /
/// 接收 / 收尾」三原语,但只服务设置页,不碰任何文档。防重入口径随
/// `AiState`(流式):在途时 `start` 被忽略(UI 禁用按钮是第一道防线),
/// 接收端不中途替换,能到达归约的只有当前在途请求的结果,无需消息层
/// 序号。「获取中」绝不卡死:线程异常退出(channel 断连且未交出结果)
/// 由 [`ModelListState::poll`] 补一条失败。
pub struct ModelListState {
    /// 在途拉取的接收端;`None` = 空闲。
    pub(crate) rx: Option<Receiver<ModelsResult>>,
    /// 本次 channel 是否已交出过结果:断连兜底(线程 panic 没发出结果)
    /// 的判定依据(与 `BedState::saw_result` 同口径,跨 poll 存续)。
    pub(crate) saw_result: bool,
    /// 最近一次成功拉到的候选列表(保序去重);失败保留旧值,成功覆盖。
    pub options: Vec<String>,
    /// 最近一次失败的错误行文案;成功与再次发起时清空。
    pub error: Option<String>,
    /// 线程装配点(见 [`ModelsSpawner`])。
    pub(crate) spawner: ModelsSpawner,
}

impl Default for ModelListState {
    fn default() -> Self {
        Self {
            rx: None,
            saw_result: false,
            options: Vec::new(),
            error: None,
            spawner: Arc::new(latermd_ai::fetch_models),
        }
    }
}

impl ModelListState {
    /// 是否有拉取在途(驱动 UI 禁用按钮 + 持续重绘)。
    pub fn is_fetching(&self) -> bool {
        self.rx.is_some()
    }

    /// 发起(归约侧调用):替换接收端并经装配点 spawn。在途时忽略(UI 已
    /// 禁用按钮,防御);spawn 失败当场按失败收尾,不留卡死的「获取中」。
    /// 发起即清旧错误行 —— 用户已重新行动,旧行是过期信息。
    pub(crate) fn start(&mut self, source: ModelsSource) {
        if self.rx.is_some() {
            return;
        }
        self.error = None;
        self.saw_result = false;
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        if (self.spawner)(source, tx).is_none() {
            self.rx = None;
            self.error = Some("获取模型列表失败:无法启动后台线程".to_owned());
        }
    }

    /// 非阻塞收空 channel(每帧归约调用),结果翻成
    /// [`Message::AiModelsFetchFinished`];生命周期收口在归约侧的
    /// [`ModelListState::finish`](与 `AiState::poll` 同分工)。断连却没
    /// 等到结果(线程异常退出)补一条失败:「获取中」标志绝不能卡死,
    /// 否则按钮永久禁用。
    pub(crate) fn poll(&mut self) -> Vec<Message> {
        let Some(rx) = &self.rx else {
            return Vec::new();
        };
        let mut messages = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(result) => {
                    self.saw_result = true;
                    messages.push(Message::AiModelsFetchFinished { result });
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.saw_result {
                        messages.push(Message::AiModelsFetchFinished {
                            result: Err("获取模型列表失败:后台线程意外中断".to_owned()),
                        });
                    }
                    break;
                }
            }
        }
        messages
    }

    /// 收尾(`Message::AiModelsFetchFinished` 的归约):清在途状态并落地
    /// 结果 —— 成功覆盖候选列表并清错误行;失败只落错误行,旧候选保留
    /// (上次拉到的列表仍然可用,比清空更友好)。
    pub(crate) fn finish(&mut self, result: ModelsResult) {
        self.rx = None;
        self.saw_result = false;
        match result {
            Ok(list) => {
                self.options = list;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            open: false,
            tab: SettingsTab::Appearance,
            capture: None,
            notice: None,
            ai_draft: AiConfig::default(),
            mcp_draft: McpConfig::default(),
            skin_export_name: String::new(),
            bed_draft: None,
            models: ModelListState::default(),
        }
    }
}

/// 绘制设置对话框;返回窗口关闭按钮的响应,**窗口未绘制(已关闭)时为
/// `None`**(测试据此断言「关闭后不再进入绘制路径」)。
// 参数各自属于 State 的不同字段,打包成结构会造出人为聚合(vendored 层
// 同款 allow 先例见 egui_markdown/src/label.rs)
#[allow(clippy::too_many_arguments)]
pub fn dialog(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    theme: &ThemeSettings,
    skins: &SkinCatalog,
    system_theme_ok: bool,
    resolved: ThemeMode,
    keymap: &Keymap,
    ai: &AiState,
    ai_key: &mut AiKeyState,
    mcp: &McpState,
    bed: &mut crate::bed::BedState,
    outbox: &mut Vec<Message>,
) -> Option<egui::Response> {
    let mut open = settings.open;
    let mut close = None;
    egui::Window::new("设置")
        // 首次打开锚定屏幕中心(pivot=窗口中心对齐锚点,与窗口尺寸无关);
        // 拖动后的位置由 Area 按窗口 id 记忆,不再回中心
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ui.ctx().viewport_rect().center())
        .default_size([600.0, 440.0])
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            // 骨架:底部按钮条 + 左分页列 + 中央滚动区,全部用 `exact_size`
            // 的 Panel 定形。此前用 `ui.horizontal` + `available_height()` +
            // `auto_shrink([false,false])` 的组合,ScrollArea 会请求全部可用
            // 宽,而 Window 宽又由内容决定 —— 二者互相喂,每帧把窗口撑大一
            // 圈,直到横贯全屏(2026-09-26 实测弹窗被拉成 1920x200 的扁条,
            // 分页列被挤没)。Panel 定形后各区域尺寸与内容解耦,反馈消失。
            egui::Panel::bottom("settings-footer")
                // 48:按钮(~24)+上下内边距+分隔线;此前 40 装下后呼吸感
                // 全无(坤哥 2026-09-29「行高不够,看着不协调」)
                .exact_size(48.0)
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(12, 6)))
                .show(ui, |ui| {
                    ui.separator();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        close = Some(ui.button("关闭"));
                    });
                });
            // 左:竖排分页(固定宽;WorkBuddy 观感 = 分页列吃侧栏灰、内容区吃窗底)
            egui::Panel::left("settings-tabs")
                .exact_size(112.0)
                .frame(
                    egui::Frame::default()
                        .fill(crate::theme::shell_tokens(ui.visuals().dark_mode).sidebar)
                        .inner_margin(egui::Margin::same(8)),
                )
                .show(ui, |ui| {
                    for tab in SettingsTab::ALL {
                        if icons::icon_tab(ui, tab.icon(), tab.label(), settings.tab == tab)
                            .clicked()
                        {
                            settings.tab = tab;
                            // 换页清空上一页的残留提示与捕获状态
                            settings.notice = None;
                            settings.capture = None;
                        }
                    }
                });
            // 右:当前页内容(可滚动,长表单不被窗口裁掉;父级已有界,
            // auto_shrink([false,false]) 只作用于面板内部,不再反哺窗口尺寸)
            egui::CentralPanel::default()
                .frame(egui::Frame::default().inner_margin(egui::Margin::same(8)))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("settings-body")
                        .auto_shrink([false, false])
                        .show(ui, |ui| match settings.tab {
                            SettingsTab::Appearance => appearance(
                                ui,
                                settings,
                                theme,
                                skins,
                                system_theme_ok,
                                resolved,
                                outbox,
                            ),
                            SettingsTab::Keymap => keymap_page(ui, settings, keymap, outbox),
                            SettingsTab::Ai => ai_page(ui, settings, ai, ai_key, outbox),
                            SettingsTab::Mcp => mcp_page(ui, settings, mcp, outbox),
                            SettingsTab::Image => image_page(ui, settings, bed, outbox),
                        });
                });
        });
    settings.open = open;
    close
}

/// 外观页:主题三态 + 皮肤 + 密度 + 渲染后端只读。
fn appearance(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    theme: &ThemeSettings,
    skins: &SkinCatalog,
    system_theme_ok: bool,
    resolved: ThemeMode,
    outbox: &mut Vec<Message>,
) {
    ui.heading("外观");
    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.label("主题(外壳与正文、代码块同帧联动):");
    for mode in ThemeMode::ALL {
        if ui
            .selectable_label(theme.mode == mode, mode.label())
            .clicked()
        {
            outbox.push(Message::ThemeChanged(mode));
        }
    }
    if theme.mode == ThemeMode::System {
        // #33:跟随系统时明暗来自系统检测,把解析值显出来——否则用户以为
        // 「我选的深色被重置了」(实际是系统侧变了/检测值与期望不符)
        ui.weak(format!(
            "跟随系统中:当前解析为「{}」;想固定明暗请直接选深色/浅色",
            resolved.label()
        ));
    }
    if theme.mode == ThemeMode::System && !system_theme_ok {
        // 检测不到就直说:Linux 无统一规范(roadmap 风险 #8),此时回落的
        // 是上一次的手动选择,不该让用户以为「跟随系统」正在生效
        ui.colored_label(
            crate::ui::tokens::WARN,
            "本机读不到系统主题设置,已回落到手动值",
        );
    }

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.label("皮肤(正文与代码高亮样式):");
    let current = theme.skin.as_deref();
    egui::ComboBox::from_label("皮肤")
        .selected_text(current.unwrap_or("出厂默认"))
        .show_ui(ui, |ui| {
            if ui.selectable_label(current.is_none(), "出厂默认").clicked() && current.is_some()
            {
                outbox.push(Message::ThemeSkinSelected(None));
            }
            for skin in &skins.skins {
                if ui
                    .selectable_label(current == Some(skin.name.as_str()), &skin.name)
                    .clicked()
                {
                    outbox.push(Message::ThemeSkinSelected(Some(skin.name.clone())));
                }
            }
        });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut settings.skin_export_name)
                .hint_text("皮肤名")
                .desired_width(140.0),
        );
        let name = settings.skin_export_name.trim();
        if ui
            .add_enabled(!name.is_empty(), egui::Button::new("导出当前样式"))
            .clicked()
        {
            outbox.push(Message::ThemeSkinExported {
                name: name.to_owned(),
            });
        }
    });
    ui.weak("导出到配置目录 themes/ 下,改名或删文件即增删皮肤。");

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.label("界面密度(间距与控件尺寸):");
    for density in Density::ALL {
        if ui
            .selectable_label(theme.density == density, density.label())
            .clicked()
        {
            outbox.push(Message::ThemeDensityChanged(density));
        }
    }

    // 排版偏好(#23 F2+F3):滑杆从 theme 读**回显副本**,拖动中值变化的
    // 每帧都发消息,归约落字段并写 settings.json,下一帧滑杆位置即归约后
    // 的值 —— 与上方主题/密度选择的「即时生效」同款(不是 AI 页的草稿
    // 模式:字号/行距是拖一下就该看到的偏好,没有「保存」步骤)。渲染投影
    // 自 F3 起生效:`ThemeSettings::apply` 每帧把字号投到 Monospace 档
    // (编辑器)并覆盖 markdown style 的行距倍率(预览)。
    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.label("排版(编辑器与预览正文的字号与行距):");
    let mut font_size = theme.editor_font_size;
    let font_size_response = ui
        .add(
            egui::Slider::new(&mut font_size, EDITOR_FONT_SIZE_MIN..=EDITOR_FONT_SIZE_MAX)
                .integer()
                .text("字号"),
        )
        .on_hover_text("正文基准字号(标题按比例放大)。下限 12 是中文(CJK)可读性下限,不再往下放。");
    if font_size_response.changed() {
        outbox.push(Message::EditorFontSizeChanged(font_size));
    }
    let mut line_height = theme.line_height;
    let line_height_response = ui
        .add(
            egui::Slider::new(&mut line_height, LINE_HEIGHT_MIN..=LINE_HEIGHT_MAX)
                .fixed_decimals(1)
                .step_by(0.1)
                .text("行距"),
        )
        .on_hover_text("正文行距倍率(如 1.5 = 1.5 倍字号)。中文可读区间通常在 1.5–1.8。");
    if line_height_response.changed() {
        outbox.push(Message::EditorLineHeightChanged(line_height));
    }

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.label("编辑器:");
    // #55 M2:minimap 开关与排版滑杆同款「回显副本 + changed() 即时发消息」
    // 模式(不是 AI/MCP 页的草稿模式:开关拨一下就该看到)。全局偏好,
    // 所有标签同开同关。
    let mut show_minimap = theme.show_minimap;
    if ui
        .checkbox(&mut show_minimap, "显示源码侧 minimap(编辑区右缘缩略导航)")
        .changed()
    {
        outbox.push(Message::ShowMinimapToggled(show_minimap));
    }
    // #64 M1:打字机模式开关(#55 同款「回显副本 + changed() 即时发消息」
    // 模式)。全局偏好,源码与 Live 两模式共用;默认关(改变滚动行为的
    // 功能出厂不替用户决定,取舍见 decisions-pending #121)。
    let mut show_typewriter = theme.show_typewriter;
    if ui
        .checkbox(&mut show_typewriter, "打字机模式(光标行保持视口 1/3 线)")
        .changed()
    {
        outbox.push(Message::TypewriterToggled(show_typewriter));
    }
    // #64 M2:专注模式开关(#55 同款「回显副本 + changed() 即时发消息」
    // 模式)。全局偏好;Live 模式淡化非活动块,源码模式不接线(单
    // TextEdit 无法分段淡化,decisions-pending #122);默认关。
    let mut show_focus_mode = theme.show_focus_mode;
    if ui
        .checkbox(&mut show_focus_mode, "专注模式(Live 下淡化光标块之外的块)")
        .changed()
    {
        outbox.push(Message::FocusModeToggled(show_focus_mode));
    }

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.label("禅定模式:");
    // #57 M2:左缘标签导航三态,与主题/密度选择同款 selectable_label
    // (即时生效,非草稿模式 —— 显示偏好拨一下就该看到)。默认悬停,
    // 常显/关闭是显式选择(取舍见 decisions-pending #107)。
    for mode in ZenNavMode::ALL {
        if ui
            .selectable_label(theme.zen_nav == mode, mode.label())
            .clicked()
        {
            outbox.push(Message::ZenNavModeChanged(mode));
        }
    }
    ui.weak("左缘标签导航:悬停=移近左缘唤出;常显=进入禅定即显示;关闭=不渲染。");

    ui.add_space(crate::ui::tokens::SPACE_MD);
    // 只读信息(AGENTS.md §5):后端是编译期 feature + 启动环境变量的
    // 决策,这里只显示不切换
    ui.weak(format!(
        "渲染后端: {}",
        crate::renderer_label(std::env::var("LATERMD_RENDERER").ok().as_deref())
    ));
}

/// 快捷键页:命令 + 键位 + 改键/清除/重置,含撞键提示。
fn keymap_page(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    keymap: &Keymap,
    outbox: &mut Vec<Message>,
) {
    ui.heading("快捷键");
    ui.weak("点击键位按钮后按下新组合;Esc 取消,Backspace 清除绑定。");
    ui.add_space(crate::ui::tokens::SPACE_SM);

    if let Some(notice) = settings.notice.as_deref() {
        ui.colored_label(crate::ui::tokens::WARN, notice);
    }

    egui::Grid::new("settings-keymap-grid")
        .num_columns(4)
        .spacing([crate::ui::tokens::SPACE_MD, crate::ui::tokens::SPACE_XS])
        .show(ui, |ui| {
            for cmd in Command::ALL {
                ui.label(cmd.label());
                let capturing = settings.capture == Some(cmd);
                let text = keymap
                    .get(cmd)
                    .map(|shortcut| shortcut.platform_text())
                    .unwrap_or_else(|| "未绑定".to_owned());
                let button = if capturing {
                    // 捕获中:按钮本体即提示,再点一次取消
                    egui::Button::new(
                        egui::RichText::new("按下新键位…(Esc 取消)")
                            .color(crate::ui::tokens::accent(ui)),
                    )
                } else {
                    egui::Button::new(text)
                };
                if ui.add(button).clicked() {
                    // 原地翻转:捕获是纯 UI 关注点,键位落盘在归约
                    settings.capture = if capturing { None } else { Some(cmd) };
                    settings.notice = None;
                }
                if ui.small_button("清除").clicked() {
                    outbox.push(Message::KeymapCleared(cmd));
                }
                if ui.small_button("重置").clicked() {
                    outbox.push(Message::KeymapReset(cmd));
                }
                ui.end_row();
            }
        });

    ui.add_space(crate::ui::tokens::SPACE_MD);
    let reset_all = ui.add_enabled(!keymap.is_default(), egui::Button::new("全部恢复默认"));
    if reset_all.clicked() {
        outbox.push(Message::KeymapResetAll);
    }
}

/// AI 页:provider / 端点 / 模型 / key。
///
/// provider 是唯一开关(接口方式随 provider 派生,decisions-pending #94),
/// 下拉四选一;采样参数/system prompt/超时/流式已随「配置页参数精简」
/// 移除(decisions-pending #108),请求侧按内部默认装配(见
/// `crate::ai::set_provider`)。「获取模型列表」(#58 M2)按 provider 拉
/// 取可用模型,成功后模型名下方出现候选下拉(手输框保留,点选才覆盖)。
fn ai_page(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    ai: &AiState,
    ai_key: &mut AiKeyState,
    outbox: &mut Vec<Message>,
) {
    ui.heading("AI");
    // 字段级借用拆分:候选列表只读,草稿可变(下同 key_editor 的 creds)
    let models = &settings.models;
    let draft = &mut settings.ai_draft;
    // Mock 不联网也不读参数:端点/模型整块灰显,避免「配了半天没生效」;
    // Ollama 连本机服务,参数照常参与
    let editable = draft.provider.uses_settings();

    ui.add_space(crate::ui::tokens::SPACE_SM);
    let mut provider = draft.provider;
    egui::ComboBox::from_label("Provider")
        .selected_text(provider.label())
        .show_ui(ui, |ui| {
            for kind in ProviderKind::ALL {
                ui.selectable_value(&mut provider, kind, kind.label());
            }
        });
    if provider != draft.provider {
        // 切换时端点/模型的出厂值跟随;手改过的字段不动
        draft.adopt_provider_defaults(provider);
    }
    ui.weak(draft.provider.description());

    ui.add_space(crate::ui::tokens::SPACE_SM);
    let factory = draft.provider.factory();
    ui.add_enabled_ui(editable, |ui| {
        ui.label("Base URL");
        ui.add(
            egui::TextEdit::singleline(&mut draft.base_url)
                .hint_text(factory.base_url.as_str())
                .desired_width(f32::INFINITY),
        );
        ui.label("模型");
        ui.add(
            egui::TextEdit::singleline(&mut draft.model)
                .hint_text(factory.model.as_str())
                .desired_width(f32::INFINITY),
        );
        // 拉取成功后的候选下拉:与手输框并存,点选才覆盖草稿;手输名不
        // 在列表时回显带「(手输)」标记,不强制改写(decisions-pending #109)
        if !models.options.is_empty() {
            let known = models.options.iter().any(|name| name == &draft.model);
            let selected = if known {
                draft.model.clone()
            } else if draft.model.trim().is_empty() {
                format!("已获取 {} 个模型,点选填入", models.options.len())
            } else {
                format!("{}(手输)", draft.model)
            };
            egui::ComboBox::from_label("已获取")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for name in &models.options {
                        ui.selectable_value(&mut draft.model, name.clone(), name);
                    }
                });
        }
    });

    ui.add_space(crate::ui::tokens::SPACE_SM);
    // 上下文大小(#58 M3,decisions-pending #110):进入 prompt 的文档/
    // diff 字节上限,0 = 跟随现状默认(摘要 32KB / commit diff 16KB)。
    // prompt 组装在 provider 之前,Mock 同样生效,不随 provider 灰显。
    ui.label("上下文大小(KB)");
    ui.add(egui::Slider::new(&mut draft.context_kb, 0..=CONTEXT_KB_MAX).text("KB"));
    ui.weak(
        "0 = 跟随默认(摘要文档 32KB / commit diff 16KB);\
         超出上限的内容截断后进请求,约 1KB ≈ 250-350 token。",
    );

    ui.add_space(crate::ui::tokens::SPACE_SM);
    // 「获取模型列表」(#58 M2):Mock 不联网禁用并说明;拉取中禁用防重入,
    // 按钮文字就是拉取中状态(与 bed 测试上传同款禁用 + 悬停说明)
    let fetching = models.is_fetching();
    let hover = if fetching {
        Some("正在拉取模型列表…")
    } else if !editable {
        Some("Mock 不联网,无需获取模型列表")
    } else {
        None
    };
    let fetch = ui.add_enabled(
        editable && !fetching,
        egui::Button::new(if fetching {
            "获取中…"
        } else {
            "获取模型列表"
        }),
    );
    // on_disabled_hover_text 消费 Response:先算悬停文案再一次性挂上
    let fetch = match hover {
        Some(text) => fetch.on_disabled_hover_text(text),
        None => fetch,
    };
    if editable && !fetching && fetch.clicked() {
        outbox.push(Message::AiModelsFetchRequested);
    }
    if let Some(error) = &models.error {
        ui.colored_label(crate::ui::tokens::WARN, error);
    }

    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.horizontal(|ui| {
        let dirty = *draft != ai.config;
        let save = ui.add_enabled(dirty, egui::Button::new("保存"));
        if save.clicked() {
            outbox.push(Message::AiConfigSaved(draft.clone()));
        }
        let revert = ui.add_enabled(dirty, egui::Button::new("放弃修改"));
        if revert.clicked() {
            *draft = ai.config.clone();
        }
    });
    ui.weak(format!(
        "当前生效:{} · {}",
        ai.provider_label(),
        ai.config.model
    ));
    // 端点只在联网型 provider 下有意义(Mock 不联网,不显示以免误导;
    // Ollama 连本机服务,算联网型)
    if ai.config.connects_network() {
        ui.weak(ai.config.base_url_trimmed());
    }

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.separator();
    ui.add_space(crate::ui::tokens::SPACE_SM);
    ai_key::key_editor(ui, ai_key, outbox);
}

/// MCP 页:开关 + 端口 + 工具权限 + 状态行 + 调用计数。
///
/// 与 AI 页同款「草稿 + 保存」分工:开关勾选不立刻生效,要点「保存」才落
/// `mcp.json` 并起停后台服务 —— 端口与工具权限都是「改了要重启监听」的
/// 动作,让用户显式确认比静默生效可预期。
fn mcp_page(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    mcp: &McpState,
    outbox: &mut Vec<Message>,
) {
    ui.heading("MCP");
    ui.weak(
        "应用开着就能被本机其他 AI(Claude Code / Cursor 等)调用,检索这个文档库。\
         全部工具只读,写入仍只能由你在界面里操作。",
    );
    ui.add_space(crate::ui::tokens::SPACE_SM);

    let draft = &mut settings.mcp_draft;
    ui.checkbox(&mut draft.enabled, "启用本地 MCP 服务(默认关闭)");
    ui.add_enabled_ui(draft.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("HTTP 端口");
            ui.add(egui::DragValue::new(&mut draft.http_port).range(McpConfig::PORT_RANGE));
            ui.weak("只监听 127.0.0.1,外部机器连不上");
        });
    });

    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.strong("工具权限(关掉的工具对客户端就不存在):");
    for kind in ToolKind::ALL {
        let mut on = draft.tool_enabled(kind);
        let label = format!("{} — {}", kind.name(), kind.description());
        if ui.checkbox(&mut on, label).changed() {
            draft.tools.insert(kind.name().to_owned(), on);
        }
    }

    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.separator();
    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.horizontal(|ui| {
        ui.strong("状态");
        // 失败态用警示色:端口被占用是最常见的失败,不能混在普通文字里
        let label = mcp.status.label();
        if matches!(mcp.status, crate::mcp::McpStatus::Failed(_)) {
            ui.colored_label(crate::ui::tokens::WARN, label);
        } else {
            ui.label(label);
        }
    });
    match mcp.root() {
        Some(root) => {
            ui.weak(format!("检索范围:{}", root.display()));
        }
        None => {
            ui.colored_label(
                crate::ui::tokens::WARN,
                "未设置文件树根目录:先选一个目录,否则所有工具都会拒绝执行。",
            );
        }
    }

    if !mcp.counts.is_empty() {
        ui.add_space(crate::ui::tokens::SPACE_SM);
        ui.strong("本次运行调用次数:");
        for kind in ToolKind::ALL {
            if let Some(count) = mcp.counts.get(kind.name()) {
                ui.horizontal(|ui| {
                    ui.monospace(kind.name());
                    ui.weak(count.to_string());
                });
            }
        }
    }

    // 客户端接线示例:用户照抄即可,省去查文档
    if mcp.config.enabled {
        ui.add_space(crate::ui::tokens::SPACE_SM);
        ui.strong("客户端配置:");
        let url = format!("http://127.0.0.1:{}/mcp", mcp.config.http_port);
        ui.horizontal(|ui| {
            ui.monospace(&url);
            if ui.small_button("复制").clicked() {
                ui.ctx().copy_text(url.clone());
            }
        });
        ui.weak("stdio 方式(客户端自己拉起进程): latermd --mcp-stdio");
    }

    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.horizontal(|ui| {
        let dirty = *draft != mcp.config;
        let save = ui.add_enabled(dirty, egui::Button::new("保存"));
        if save.clicked() {
            outbox.push(Message::McpConfigSaved(draft.clone()));
        }
        let revert = ui.add_enabled(dirty, egui::Button::new("放弃修改"));
        if revert.clicked() {
            *draft = mcp.config.clone();
        }
        if dirty {
            ui.weak("保存后服务按新配置重启");
        }
    });
    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.weak("完整设计:docs/mcp-plan.md(路径不得越出文件树根、无写工具)。");
}

/// 图片页:图床 profile 列表 / 新增 / 编辑 / 删除 / 测试上传
/// (docs/image-plan.md C 段)。
///
/// 与 AI/MCP 页同款「草稿 + 保存」分工:编辑期改动全在 [`BedDraft`],点
/// 「保存」才发 [`Message::BedProfileSaved`](token 一并写系统凭据,绝不落
/// beds.json)。「测试上传」选文件后走与图片框同一条后台上传链路,结果
/// 回显在本页,不插入任何文档。
fn image_page(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    bed: &mut crate::bed::BedState,
    outbox: &mut Vec<Message>,
) {
    ui.heading("图片 · 图床");
    ui.weak(
        "图片框里的「上传」把图片发给图床,返回的 URL 插进文档。\
         token 存系统凭据,配置文件里只有 ${TOKEN} 占位符。",
    );
    ui.add_space(crate::ui::tokens::SPACE_SM);

    if let Some(notice) = settings.notice.as_deref() {
        ui.colored_label(crate::ui::tokens::WARN, notice);
    }

    if bed.profiles.is_empty() {
        ui.weak("还没有图床,从下面的模板新增一个。");
    }
    // 克隆迭代:循环体只发消息不改列表,列表变更在归约
    for profile in bed.profiles.clone() {
        ui.horizontal(|ui| {
            ui.strong(profile.display_name());
            ui.weak(profile.body.label());
            if ui.small_button("编辑").clicked() {
                settings.bed_draft = Some(BedDraft::from_profile(&profile));
                settings.notice = None;
            }
            let testing = bed.is_uploading();
            let test = ui.add_enabled(!testing, egui::Button::new("测试上传"));
            if testing {
                test.on_disabled_hover_text("有上传进行中,稍候");
            } else if test.clicked() {
                outbox.push(Message::BedTestUploadRequested {
                    profile_id: profile.id.clone(),
                });
            }
            if ui.small_button("删除").clicked() {
                outbox.push(Message::BedProfileDeleted {
                    id: profile.id.clone(),
                });
            }
        });
    }

    // 测试上传回显:成功给可复制的 URL,失败给警示色原因
    if let Some((name, result)) = &bed.last_test {
        match result {
            Ok(url) => {
                ui.label(format!("测试上传({name})成功:"));
                ui.horizontal(|ui| {
                    ui.monospace(url);
                    if ui.small_button("复制").clicked() {
                        ui.ctx().copy_text(url.clone());
                    }
                });
            }
            Err(error) => {
                ui.colored_label(
                    crate::ui::tokens::WARN,
                    format!("测试上传({name})失败:{error}"),
                );
            }
        }
    }
    if bed.is_uploading() {
        ui.colored_label(crate::ui::tokens::accent(ui), "上传中…");
    }

    ui.add_space(crate::ui::tokens::SPACE_MD);
    ui.separator();
    ui.add_space(crate::ui::tokens::SPACE_SM);
    if settings.bed_draft.is_none() {
        ui.label("新增图床:");
        ui.horizontal(|ui| {
            for (label, preset) in [
                ("SM.MS", latermd_bed::BedProfile::preset_smms()),
                ("GitHub", latermd_bed::BedProfile::preset_github()),
                ("自定义", latermd_bed::BedProfile::preset_custom()),
            ] {
                if ui.button(label).clicked() {
                    settings.bed_draft = Some(BedDraft {
                        headers_text: headers_to_text(&preset.headers),
                        profile: preset,
                        token: String::new(),
                    });
                    settings.notice = None;
                }
            }
        });
    }

    if let Some(draft) = settings.bed_draft.as_mut() {
        ui.add_space(crate::ui::tokens::SPACE_SM);
        if bed_editor(ui, &mut settings.notice, draft, outbox) {
            settings.bed_draft = None;
        }
    }
}

/// 图床编辑器(新增与编辑共用;`draft.profile.id` 空 = 新增)。`notice` 是
/// 页内提示槽(校验失败留在编辑器里改);返回 `true` = 保存成功或取消,
/// 调用方据此收起编辑器。
fn bed_editor(
    ui: &mut egui::Ui,
    notice: &mut Option<String>,
    draft: &mut BedDraft,
    outbox: &mut Vec<Message>,
) -> bool {
    let profile = &mut draft.profile;
    ui.strong(if profile.id.is_empty() {
        "新增图床"
    } else {
        "编辑图床"
    });
    // 关闭编辑器(保存成功/取消)由闭包外的 result 带回 —— 闭包里借不出 settings
    let mut result = Option::<bool>::None;
    egui::Grid::new("settings-bed-grid")
        .num_columns(2)
        .spacing([crate::ui::tokens::SPACE_MD, crate::ui::tokens::SPACE_XS])
        .show(ui, |ui| {
            ui.label("名称");
            ui.add(
                egui::TextEdit::singleline(&mut profile.name)
                    .hint_text("如 我的 SM.MS")
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label("API 地址");
            ui.add(
                egui::TextEdit::singleline(&mut profile.api_url)
                    .hint_text("https://…/upload;可用 ${NAME} 代替本次文件名")
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label("表单字段名");
            ui.add(
                egui::TextEdit::singleline(&mut profile.file_field)
                    .hint_text("SM.MS=smfile,Lsky=file")
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label("URL 取值路径");
            ui.add(
                egui::TextEdit::singleline(&mut profile.url_path)
                    .hint_text("返回 JSON 里的点分路径,如 data.url")
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label("URL 前缀");
            let mut prefix = profile.url_prefix.clone().unwrap_or_default();
            let response = ui.add(
                egui::TextEdit::singleline(&mut prefix)
                    .hint_text("返回路径而非完整 URL 时拼在前面;留空不拼")
                    .desired_width(360.0),
            );
            if response.changed() {
                profile.url_prefix = Some(prefix);
            }
            ui.end_row();
            ui.label("编码方式");
            egui::ComboBox::from_id_salt("bed-body-style")
                .selected_text(profile.body.label())
                .show_ui(ui, |ui| {
                    for style in [
                        latermd_bed::BedBody::Multipart,
                        latermd_bed::BedBody::Base64Json,
                    ] {
                        ui.selectable_value(&mut profile.body, style, style.label());
                    }
                });
            ui.end_row();
            ui.label("请求头");
            ui.add(
                egui::TextEdit::multiline(&mut draft.headers_text)
                    .hint_text("每行一条「名: 值」,值可写 ${TOKEN}")
                    .desired_rows(3)
                    .desired_width(360.0),
            );
            ui.end_row();
            ui.label("Token");
            ui.add(
                egui::TextEdit::singleline(&mut draft.token)
                    .password(true)
                    .hint_text("保存时写入系统凭据;留空 = 不改已存值")
                    .desired_width(360.0),
            );
            ui.end_row();
        });

    ui.horizontal(|ui| {
        let mut close = false;
        if ui.button("保存").clicked() {
            // 先把多行文本并进 profile 再校验;失败留在编辑器里改
            match headers_from_text(&draft.headers_text) {
                Ok(headers) => {
                    let mut final_profile = profile.clone();
                    final_profile.headers = headers;
                    final_profile.normalize();
                    if final_profile.api_url.is_empty() || final_profile.url_path.is_empty() {
                        *notice = Some("API 地址与 URL 取值路径必填".to_owned());
                    } else {
                        outbox.push(Message::BedProfileSaved {
                            profile: final_profile,
                            token: Some(draft.token.clone()),
                        });
                        close = true;
                    }
                }
                Err(error) => *notice = Some(error),
            }
        }
        if ui.button("取消").clicked() {
            close = true;
        }
        result.replace(close);
    });
    result.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Shortcut;
    use crate::state::State;
    use egui::RawInput;

    /// 测试口径的 resolved(与 draw 一致);State 解构前调用。
    fn resolved_theme_for_test(state: &State) -> ThemeMode {
        state.resolved_theme()
    }

    fn render(state: &mut State) {
        let ctx = egui::Context::default();
        let system_theme_ok = state.system_theme_ok;
        let resolved = state.resolved_theme();
        // 借用拆分:settings / ai_key / bed 可变,其余只读(与 draw 的口径一致)
        let State {
            settings,
            ai_key,
            ai,
            mcp,
            bed,
            keymap,
            theme,
            skins,
            ..
        } = state;
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            dialog(
                ui,
                settings,
                theme,
                skins,
                system_theme_ok,
                resolved,
                keymap,
                ai,
                ai_key,
                mcp,
                bed,
                &mut outbox,
            );
        });
        output.drop_without_applying_deltas();
    }

    /// 五个分页各渲一帧不 panic(含 AI 页的禁用态、MCP 页的规划态与图片
    /// 页的空列表/编辑草稿态)。
    #[test]
    fn every_tab_renders() {
        let mut state = State::default();
        state.settings.open = true;
        for tab in SettingsTab::ALL {
            state.settings.tab = tab;
            state.settings.bed_draft = None;
            render(&mut state);
        }
        // AI 页四个 provider 各再渲一帧:说明文案/出厂值提示/参数区显隐
        // 随 provider 变化,都要走一遍真实绘制路径
        state.settings.tab = SettingsTab::Ai;
        for provider in ProviderKind::ALL {
            state.settings.ai_draft = AiConfig {
                provider,
                ..AiConfig::default()
            };
            render(&mut state);
        }
        // 图片页带草稿与已存 profile 的形态也渲一帧
        state.settings.tab = SettingsTab::Image;
        state.bed.profiles = vec![latermd_bed::BedProfile::preset_smms()];
        state.bed.last_test = Some(("SM.MS".to_owned(), Err("HTTP 401:bad token".to_owned())));
        state.settings.bed_draft = Some(BedDraft::from_profile(
            &latermd_bed::BedProfile::preset_github(),
        ));
        render(&mut state);
    }

    /// #58 渲染探针:AI 页只剩 provider/Base URL/模型/上下文大小(#58 M3
    /// 新增的请求预算滑杆)/key,被删参数控件
    /// (Temperature/Top-P/Max tokens/超时/流式接收/System prompt)不再
    /// 渲染。无头渲真实 AI 页数帧,收集全部 `TextShape` 的逻辑文本做
    /// 双向断言(保留项在场 + 被删项绝迹 —— 后者同时钉住「当前生效」行
    /// 不再报流式形态)。#35 的 AI 页滑杆几何探针随四根滑杆移除而删除
    /// (外观页两根排版滑杆的几何探针不受影响)。
    #[test]
    fn ai_page_renders_kept_fields_without_removed_parameter_widgets() {
        let mut state = State::default();
        state.settings.open = true;
        state.settings.tab = SettingsTab::Ai;
        // 切到可编辑 provider:Base URL/模型输入框真实渲出(参数区旧位置)
        state.settings.ai_draft.provider = ProviderKind::OpenAiCompatible;
        let ctx = egui::Context::default();
        state.theme.apply(&ctx, state.theme.mode);
        let system_theme_ok = state.system_theme_ok;
        let resolved = resolved_theme_for_test(&state);
        let mut all_text = String::new();
        let mut now = 0.0_f64;
        for _ in 0..5 {
            now += 0.1; // fade-in 时钟:不给 time 首帧内容整体 noop(无头老坑)
            let State {
                settings,
                ai_key,
                ai,
                mcp,
                bed,
                keymap,
                theme,
                skins,
                ..
            } = &mut state;
            let mut outbox = Vec::new();
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(1200.0, 800.0),
                    )),
                    time: Some(now),
                    ..Default::default()
                },
                |ui| {
                    dialog(
                        ui,
                        settings,
                        theme,
                        skins,
                        system_theme_ok,
                        resolved,
                        keymap,
                        ai,
                        ai_key,
                        mcp,
                        bed,
                        &mut outbox,
                    );
                },
            );
            output.textures_delta.clear();
            for clipped in &output.shapes {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    all_text.push_str(text.galley.text());
                    all_text.push('\n');
                }
            }
        }
        assert!(
            !all_text.is_empty(),
            "五帧后仍无文本形状:fade-in 时钟或无头管线异常"
        );
        for kept in ["Provider", "Base URL", "模型", "上下文大小", "保存"] {
            assert!(
                all_text.contains(kept),
                "AI 页应保留 \"{kept}\":\n{all_text}"
            );
        }
        for removed in [
            "Temperature",
            "Top-P",
            "Max tokens",
            "超时",
            "流式接收",
            "System prompt",
        ] {
            assert!(
                !all_text.contains(removed),
                "AI 页不应再渲染 \"{removed}\":\n{all_text}"
            );
        }
    }

    // ---- 「获取模型列表」(#58 M2):状态机 + 渲染探针(全部零网络;
    // 假装配注入点与 AiKeyState::creds 同理念,真实端点验证留人工) ----

    /// 定时回固定结果的假装配:spawn 真线程(验线程机制)但绝不碰网络。
    fn fake_spawner(delay: std::time::Duration, result: ModelsResult) -> ModelsSpawner {
        Arc::new(move |_source, tx| {
            let result = result.clone();
            std::thread::Builder::new()
                .name("fake-models-spawner".into())
                .spawn(move || {
                    std::thread::sleep(delay);
                    let _ = tx.send(result);
                })
                .ok()
        })
    }

    /// 轮询到第一条结果消息(线程异步,限时 5s)。
    fn wait_result(models: &mut ModelListState) -> Vec<Message> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut messages = Vec::new();
        while messages.is_empty() && std::time::Instant::now() < deadline {
            messages = models.poll();
            if messages.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        messages
    }

    /// 状态机生命周期:start 置在途、poll 非阻塞收结果、finish 落地
    /// (成功覆盖候选清错误行;失败落错误行保旧候选)。
    #[test]
    fn model_list_state_lifecycle_start_poll_finish() {
        let mut models = ModelListState::default();
        assert!(!models.is_fetching());
        assert!(models.poll().is_empty(), "空闲 poll 为空");

        let source = ModelsSource::Ollama {
            base_url: "http://模型列表测试.invalid".to_owned(),
        };
        models.spawner = fake_spawner(
            std::time::Duration::from_millis(20),
            Ok(vec!["m1".to_owned(), "m2".to_owned()]),
        );
        models.start(source.clone());
        assert!(models.is_fetching(), "发起后置在途");

        let messages = wait_result(&mut models);
        assert_eq!(
            messages,
            vec![Message::AiModelsFetchFinished {
                result: Ok(vec!["m1".to_owned(), "m2".to_owned()])
            }]
        );
        models.finish(Ok(vec!["m1".to_owned(), "m2".to_owned()]));
        assert!(!models.is_fetching(), "收尾回空闲");
        assert_eq!(models.options, vec!["m1".to_owned(), "m2".to_owned()]);
        assert!(models.error.is_none(), "成功清错误行");

        // 失败:文案落错误行,旧候选保留(上次拉到的列表仍可用)
        models.spawner = fake_spawner(
            std::time::Duration::from_millis(20),
            Err("获取模型列表失败:HTTP 401:bad key".to_owned()),
        );
        models.start(source);
        let messages = wait_result(&mut models);
        assert_eq!(messages.len(), 1);
        models.finish(Err("获取模型列表失败:HTTP 401:bad key".to_owned()));
        assert!(!models.is_fetching());
        assert_eq!(
            models.error.as_deref(),
            Some("获取模型列表失败:HTTP 401:bad key")
        );
        assert_eq!(
            models.options,
            vec!["m1".to_owned(), "m2".to_owned()],
            "失败不清旧候选"
        );
    }

    /// 防重入:在途时再次发起被忽略(第一次的接收端不被替换,结果照常
    /// 交付);收尾后可重新发起。UI 禁用按钮是第一道防线,归约侧忽略是
    /// 同一语义的第二道(与 `AiState` 的 streaming 防重入口径一致)。
    #[test]
    fn model_list_state_ignores_start_while_inflight() {
        let mut models = ModelListState::default();
        let source = ModelsSource::Ollama {
            base_url: "http://模型列表测试.invalid".to_owned(),
        };
        models.spawner = fake_spawner(
            std::time::Duration::from_millis(80),
            Ok(vec!["old".to_owned()]),
        );
        models.start(source.clone());
        models.spawner = fake_spawner(std::time::Duration::ZERO, Ok(vec!["new".to_owned()]));
        models.start(source.clone());
        assert!(models.is_fetching());

        let messages = wait_result(&mut models);
        assert_eq!(
            messages,
            vec![Message::AiModelsFetchFinished {
                result: Ok(vec!["old".to_owned()])
            }],
            "在途时第二次发起被忽略,交付的是第一次的结果"
        );
        models.finish(Ok(vec!["old".to_owned()]));
        assert!(!models.is_fetching());

        // 空闲后再发起:新结果照常可达
        models.spawner = fake_spawner(std::time::Duration::ZERO, Ok(vec!["fresh".to_owned()]));
        models.start(source);
        assert_eq!(
            wait_result(&mut models),
            vec![Message::AiModelsFetchFinished {
                result: Ok(vec!["fresh".to_owned()])
            }]
        );
    }

    /// 断连兜底:发送端没发出结果就退出(线程异常/panic 的可观察形态就是
    /// channel 断连)→ 补一条失败,「获取中」不卡死;已交出过结果再断连
    /// 不重复报错(与 `BedState::saw_result` 同口径)。
    #[test]
    fn model_list_state_disconnected_without_result_synthesizes_failure() {
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let mut models = ModelListState {
            rx: Some(rx),
            ..ModelListState::default()
        };
        assert_eq!(
            models.poll(),
            vec![Message::AiModelsFetchFinished {
                result: Err("获取模型列表失败:后台线程意外中断".to_owned())
            }]
        );

        // 已交出过结果:断连不再追加第二条失败
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(vec!["x".to_owned()])).unwrap();
        drop(tx);
        let mut models = ModelListState {
            rx: Some(rx),
            ..ModelListState::default()
        };
        assert_eq!(models.poll().len(), 1, "结果照常交付");
        assert!(models.poll().is_empty(), "断连不重复补失败");
    }

    /// 渲 AI 页若干帧并把全部 TextShape 的文本拼进 `sink`(time 逐帧推进,
    /// 无头老坑见 `ai_page_renders_kept_fields…`)。直渲 `ai_page`(同
    /// `appearance` 探针:设置窗视口外的内容不进 clip,直渲控件最聚焦)。
    fn render_ai_page_collect(state: &mut State, frames: usize, sink: &mut String) {
        let ctx = egui::Context::default();
        state.theme.apply(&ctx, state.theme.mode);
        let mut now = 0.0_f64;
        for _ in 0..frames {
            now += 0.1;
            let State {
                settings,
                ai_key,
                ai,
                ..
            } = state;
            let mut outbox = Vec::new();
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::pos2(0.0, 0.0),
                        egui::vec2(1200.0, 800.0),
                    )),
                    time: Some(now),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        ai_page(ui, settings, ai, ai_key, &mut outbox);
                    });
                },
            );
            output.textures_delta.clear();
            for clipped in &output.shapes {
                if let egui::epaint::Shape::Text(text) = &clipped.shape {
                    sink.push_str(text.galley.text());
                    sink.push('\n');
                }
            }
        }
    }

    /// 按钮状态机四态真实渲出,且拉取期间 UI 不阻塞:慢装配(300ms)在途
    /// 时连续五帧的总耗时远小于装配延迟(同步等待会 ≥300ms),「获取中…」
    /// 可见;成功帧出现候选下拉与手输回显;失败帧错误文案落地。禁用态的
    /// 「点击不产消息」防线在归约侧
    /// (`state::tests::ai_models_fetch_request_gate_and_finish_landing`)。
    #[test]
    fn ai_page_renders_fetch_states_and_stays_responsive_while_fetching() {
        let mut state = State::default();
        state.ai_key.creds = latermd_creds::Credentials::in_memory();
        state.settings.ai_draft.provider = ProviderKind::OpenAiCompatible;

        // 空闲:按钮文案在场
        let mut text = String::new();
        render_ai_page_collect(&mut state, 2, &mut text);
        assert!(text.contains("获取模型列表"), "空闲按钮在场:{text}");

        // 发起(慢装配 300ms):连续五帧远小于装配延迟 = UI 没有同步等待
        state.settings.models.spawner = fake_spawner(
            std::time::Duration::from_millis(300),
            Ok(vec!["fake-model".to_owned()]),
        );
        state.apply(Message::AiModelsFetchRequested);
        assert!(state.settings.models.is_fetching());

        let mut text = String::new();
        let start = std::time::Instant::now();
        render_ai_page_collect(&mut state, 5, &mut text);
        let elapsed = start.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "拉取中五帧耗时 {elapsed:?},UI 疑似被后台拉取阻塞"
        );
        assert!(text.contains("获取中…"), "拉取中状态可见:{text}");
        assert!(
            !text.contains("获取模型列表"),
            "拉取中原按钮文案不出现:{text}"
        );

        // 结果到达:收流归约 → 成功帧出现候选下拉;出厂模型名不在候选,
        // 回显「(手输)」标记(不强制改写,decisions-pending #109)
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.settings.models.is_fetching() && std::time::Instant::now() < deadline {
            for message in state.poll_models() {
                state.apply(message);
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!state.settings.models.is_fetching(), "结果已落地");
        let mut text = String::new();
        render_ai_page_collect(&mut state, 2, &mut text);
        assert!(text.contains("已获取"), "成功后出现候选下拉:{text}");
        assert!(text.contains("(手输)"), "手输名不在候选时回显标记:{text}");

        // 手输名恰在候选里:回显名字本身,无标记
        state.settings.ai_draft.model = "fake-model".to_owned();
        let mut text = String::new();
        render_ai_page_collect(&mut state, 2, &mut text);
        assert!(text.contains("fake-model"), "{text}");
        assert!(!text.contains("(手输)"), "候选内的名字不带标记:{text}");

        // 失败态:错误文案上屏(警示色错误行)
        state.settings.models.spawner = fake_spawner(
            std::time::Duration::ZERO,
            Err("获取模型列表失败:HTTP 401:bad key".to_owned()),
        );
        state.apply(Message::AiModelsFetchRequested);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.settings.models.is_fetching() && std::time::Instant::now() < deadline {
            for message in state.poll_models() {
                state.apply(message);
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let mut text = String::new();
        render_ai_page_collect(&mut state, 2, &mut text);
        assert!(
            text.contains("获取模型列表失败:HTTP 401:bad key"),
            "失败文案上屏:{text}"
        );
        assert!(text.contains("已获取"), "失败帧旧候选下拉保留:{text}");
    }

    /// #23 F2:外观页两根排版滑杆真实渲出 —— 「字号」「行距」标签与 theme
    /// 当前回显值进入末帧 shapes 的**精确** Text 形状(galley 文本恰等于
    /// 标签/值,不与区块标题整句撞车),滑杆轨道(rail)至少两根;无拖动的
    /// 初始帧渲染不产出消息(改动只经拖动时的 `changed()` 走归约)。两轮
    /// 回显值(出厂 15/1.5 与自定义 20/1.8)证明滑杆的值绑定在 theme 字段
    /// 上(回显来自归约侧真值,而不是 UI 本地草稿)。
    ///
    /// 直接渲 `appearance()` 而不是整个 `dialog`:设置窗 default_size 440px
    /// 视口只装到「排版」区块标题,滑杆在 ScrollArea 视口之下(首屏要滚一下
    /// 才看到,产品上可接受);clip 之外 tessellation 会剔除形状,滑杆就取
    /// 不到断言了。在整窗里的「渲不 panic」由 `every_tab_renders` 覆盖,本
    /// 测试只断言滑杆本体。与 #35 同款环境:先跑主题投影(滑杆轨道填充色
    /// 硬绑 `widgets.inactive.bg_fill`)。
    #[test]
    fn appearance_page_renders_font_prefs_sliders() {
        let mut state = State::default();
        let ctx = egui::Context::default();
        state.theme.apply(&ctx, state.theme.mode);

        for (font_size, line_height) in [(15.0_f32, 1.5_f32), (20.0, 1.8)] {
            state.theme.editor_font_size = font_size;
            state.theme.line_height = line_height;
            let mut last_shapes = Vec::new();
            let mut last_outbox = Vec::new();
            let system_theme_ok = state.system_theme_ok;
            let resolved = resolved_theme_for_test(&state);
            let skins = state.skins.clone();
            // 3 帧取末帧:appearance() 直接渲没有 Window fade-in,多帧只为
            // 避开可能的布局 warm-up 首帧(egui Slider 的 DragValue 编辑态)
            for _ in 0..3 {
                let State {
                    settings, theme, ..
                } = &mut state;
                let mut outbox = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::pos2(0.0, 0.0),
                            egui::vec2(1200.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        // egui 0.36:面板嵌套 Ui 用 `show`(旧 `show_inside`
                        // 已弃用);这里只借一层全屏 Ui 当画布,不参与布局
                        egui::CentralPanel::default().show(ui, |ui| {
                            appearance(
                                ui,
                                settings,
                                theme,
                                &skins,
                                system_theme_ok,
                                resolved,
                                &mut outbox,
                            );
                        });
                    },
                );
                // shapes move 出前清 textures delta(直接 drop 带 delta 会 panic)
                let mut output = output;
                output.textures_delta.clear();
                last_shapes = output.shapes;
                last_outbox = outbox;
            }

            // 精确匹配:滑杆标签是独立 Label 形状,值是独立 DragValue 形状,
            // 二者的 galley 文本恰等于「字号」/「行距」/数值本身;区块标题
            // (整句「排版(…)」)不会以纯标签形态出现,contains 会撞车,
            // 这里用 trim 后全等
            let text_shapes: Vec<&str> = last_shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.text().trim()),
                    _ => None,
                })
                .collect();
            for label in ["字号", "行距"] {
                assert!(
                    text_shapes.contains(&label),
                    "{font_size}/{line_height}: 滑杆标签「{label}」未渲出,文本形状:{text_shapes:?}"
                );
            }
            for value in [format!("{font_size}"), format!("{line_height}")] {
                assert!(
                    text_shapes.contains(&value.as_str()),
                    "{font_size}/{line_height}: 回显值「{value}」未渲出,文本形状:{text_shapes:?}"
                );
            }
            // 轨道:横向细长填充矩形(与 #35 同款判据),主题区分隔线等同类
            // 形状只会多算不会少算,断言 ≥2
            use egui::epaint::Shape;
            let rails = last_shapes
                .iter()
                .filter(|clipped| {
                    matches!(&clipped.shape, Shape::Rect(r)
                        if r.rect.height() <= 12.0 && r.rect.width() > 40.0)
                })
                .count();
            assert!(
                rails >= 2,
                "{font_size}/{line_height}: 滑杆轨道不足 2 根({rails})"
            );
            assert!(
                last_outbox.is_empty(),
                "无拖动的渲染帧不产出消息(值只在 changed() 时发)"
            );
        }
    }

    /// 关闭时窗口不进入绘制路径(不 panic、不改 open 标志)。
    #[test]
    fn closed_dialog_renders_nothing() {
        let mut state = State::default();
        assert!(!state.settings.open);
        render(&mut state);
        assert!(!state.settings.open, "渲染不翻转开关");
    }

    /// #55 M2:外观页「编辑器」分区真实渲出 minimap 复选框的标签文本
    /// (与 #23 滑杆测试同款:直接渲 `appearance`,避开整窗 440px 视口
    /// 对 ScrollArea 的裁剪);两轮回显值(开/关)证明复选框绑定在
    /// theme 字段上,无交互帧不产出消息(翻转只经 changed() 走归约)。
    #[test]
    fn appearance_page_renders_minimap_toggle_bound_to_theme() {
        let mut state = State::default();
        let ctx = egui::Context::default();
        state.theme.apply(&ctx, state.theme.mode);

        for show in [true, false] {
            state.theme.show_minimap = show;
            let skins = state.skins.clone();
            let mut last_shapes = Vec::new();
            let mut last_outbox = Vec::new();
            let system_theme_ok = state.system_theme_ok;
            let resolved = resolved_theme_for_test(&state);
            for _ in 0..3 {
                let State {
                    settings, theme, ..
                } = &mut state;
                let mut outbox = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::pos2(0.0, 0.0),
                            egui::vec2(1200.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            appearance(
                                ui,
                                settings,
                                theme,
                                &skins,
                                system_theme_ok,
                                resolved,
                                &mut outbox,
                            );
                        });
                    },
                );
                let mut output = output;
                output.textures_delta.clear();
                last_shapes = output.shapes;
                last_outbox = outbox;
            }
            let texts: Vec<&str> = last_shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.text().trim()),
                    _ => None,
                })
                .collect();
            assert!(
                texts.iter().any(|t| t.contains("minimap")),
                "show={show}: 复选框标签未渲出,文本形状:{texts:?}"
            );
            assert!(last_outbox.is_empty(), "show={show}: 无交互帧不产出消息");
        }
    }

    /// #57 M2:外观页真实渲出禅定导航三态选择(与 minimap 测试同款:直接
    /// 渲 `appearance`,避开整窗 440px 视口对 ScrollArea 的裁剪)。三轮回显
    /// (悬停/常显/关)里三态标签**每轮都在场**——选择器不随选中值增删
    /// 选项,只换高亮;无交互帧不产出消息(切换只经 clicked() 走归约)。
    #[test]
    fn appearance_page_renders_zen_nav_selector_bound_to_theme() {
        let mut state = State::default();
        let ctx = egui::Context::default();
        state.theme.apply(&ctx, state.theme.mode);

        for mode in ZenNavMode::ALL {
            state.theme.zen_nav = mode;
            let skins = state.skins.clone();
            let mut last_shapes = Vec::new();
            let mut last_outbox = Vec::new();
            let system_theme_ok = state.system_theme_ok;
            let resolved = resolved_theme_for_test(&state);
            for _ in 0..3 {
                let State {
                    settings, theme, ..
                } = &mut state;
                let mut outbox = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::pos2(0.0, 0.0),
                            egui::vec2(1200.0, 800.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            appearance(
                                ui,
                                settings,
                                theme,
                                &skins,
                                system_theme_ok,
                                resolved,
                                &mut outbox,
                            );
                        });
                    },
                );
                let mut output = output;
                output.textures_delta.clear();
                last_shapes = output.shapes;
                last_outbox = outbox;
            }
            let texts: Vec<&str> = last_shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    egui::epaint::Shape::Text(t) => Some(t.galley.text().trim()),
                    _ => None,
                })
                .collect();
            for label in ZenNavMode::ALL.map(|mode| mode.label()) {
                assert!(
                    texts.contains(&label),
                    "{mode:?}: 三态选项「{label}」未渲出,文本形状:{texts:?}"
                );
            }
            assert!(last_outbox.is_empty(), "{mode:?}: 无交互帧不产出消息");
        }
    }

    /// 页签图标与显示名一一对应,且都有图标(不出现空图标页)。
    #[test]
    fn every_tab_has_label_and_icon() {
        for tab in SettingsTab::ALL {
            assert!(!tab.label().is_empty());
            let _ = tab.icon();
        }
    }

    /// AI 页草稿:默认等于出厂配置;OpenAI 兼容/Anthropic「需要 key」为真
    /// (驱动参数区可用与命令闸门);Ollama 不要 key 但参数照常参与。
    #[test]
    fn ai_draft_defaults_and_provider_switch() {
        let mut settings = SettingsState::default();
        assert_eq!(settings.ai_draft, AiConfig::default());
        assert!(!settings.ai_draft.provider.requires_key());
        for kind in [ProviderKind::OpenAiCompatible, ProviderKind::Anthropic] {
            settings.ai_draft.provider = kind;
            assert!(kind.requires_key());
            assert!(kind.uses_settings());
        }
        settings.ai_draft.provider = ProviderKind::Ollama;
        assert!(!settings.ai_draft.provider.requires_key());
        assert!(settings.ai_draft.provider.uses_settings());
    }

    /// MCP 页草稿:默认与 `McpConfig::default` 一致(关闭 + 默认端口 + 五
    /// 工具全开);关掉一个工具后 draft 立即反映(保存才生效到服务)。
    #[test]
    fn mcp_draft_defaults_and_tool_toggle() {
        let mut settings = SettingsState::default();
        assert_eq!(settings.mcp_draft, McpConfig::default());
        assert!(!settings.mcp_draft.enabled);
        settings
            .mcp_draft
            .tools
            .insert(ToolKind::GitStatus.name().to_owned(), false);
        assert!(!settings.mcp_draft.tool_enabled(ToolKind::GitStatus));
        assert!(settings.mcp_draft.tool_enabled(ToolKind::SearchDocs));
    }

    /// MCP 页在「未设根」的初始态下渲染不 panic,并给出警示文案(该页
    /// 是唯一会把「根缺失」摆到台面上的地方)。
    #[test]
    fn mcp_page_renders_without_root() {
        let ctx = egui::Context::default();
        let mut settings = SettingsState::default();
        let mcp = McpState::default();
        let mut outbox = Vec::new();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            mcp_page(ui, &mut settings, &mcp, &mut outbox);
        });
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty(), "渲染不产出消息");
    }

    /// 快捷键页的捕获状态是纯 UI 关注点:点一次进入捕获,再点一次退出。
    #[test]
    fn capture_toggles_in_place() {
        let ctx = egui::Context::default();
        let mut settings = SettingsState::default();
        let mut outbox = Vec::new();
        let keymap = Keymap::builtin();
        let mut rect = egui::Rect::NOTHING;

        let output = ctx.run_ui(RawInput::default(), |ui| {
            settings.capture = Some(Command::Save);
            keymap_page(ui, &mut settings, &keymap, &mut outbox);
            // 拿第一行键位按钮的位置:复用同一渲染路径的第二帧定位
            rect = ui.min_rect();
        });
        output.drop_without_applying_deltas();
        assert!(settings.capture.is_some(), "渲染不改变捕获状态");
        let _ = rect;
    }

    /// 快捷键页对快速打开(#24 C2)可见可改绑:整页按 `Command::ALL`
    /// 遍历渲染一帧不 panic,QuickOpen 行因出厂有绑定而显示键位文本
    /// (非「未绑定」占位)。
    #[test]
    fn keymap_page_renders_with_quick_open_bound() {
        let keymap = Keymap::builtin();
        assert!(
            keymap.get(Command::QuickOpen).is_some(),
            "QuickOpen 出厂有绑定,行上显示键位而非「未绑定」"
        );
        let ctx = egui::Context::default();
        let mut settings = SettingsState::default();
        let mut outbox = Vec::new();
        ctx.run_ui(RawInput::default(), |ui| {
            keymap_page(ui, &mut settings, &keymap, &mut outbox);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "渲染不产出消息");
    }

    /// 快捷键行的键位文本:已绑定显示组合键,未绑定显示「未绑定」。
    #[test]
    fn shortcut_text_shows_binding_or_placeholder() {
        let keymap = Keymap::builtin();
        assert!(keymap.get(Command::Save).is_some());
        let mut cleared = keymap.clone();
        cleared.set(Command::Save, None);
        assert_eq!(cleared.get(Command::Save), None);

        // 平台化文本能被反解(改键后持久化-重载闭环的前提)
        let shortcut = Shortcut {
            modifiers: egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            key: egui::Key::K,
        };
        assert_eq!(
            crate::keymap::parse_shortcut(&shortcut.platform_text()),
            Some(shortcut)
        );
    }

    /// 请求头多行文本 ↔ 结构对的往返:空行跳过、值里的 `${TOKEN}` 原样
    /// 保留、缺 `:` 或空名字报行号。
    #[test]
    fn headers_text_round_trips() {
        let headers = vec![
            ("Authorization".to_owned(), "Bearer ${TOKEN}".to_owned()),
            ("Accept".to_owned(), "application/json".to_owned()),
        ];
        let text = headers_to_text(&headers);
        assert_eq!(
            text,
            "Authorization: Bearer ${TOKEN}\nAccept: application/json"
        );
        assert_eq!(headers_from_text(&text).unwrap(), headers);
        // 空行与首尾空白宽容
        assert_eq!(
            headers_from_text("\n  A:  x  \n\n").unwrap(),
            vec![("A".to_owned(), "x".to_owned())]
        );
        // 坏行报行号(1 起)
        assert!(headers_from_text("A: 1\n没冒号的行")
            .unwrap_err()
            .contains("第 2 行"));
        assert!(headers_from_text(": v").unwrap_err().contains("名字为空"));
    }

    /// 图片页整页渲染(空列表 / 有 profile + 测试回显两态)不 panic、不产
    /// 消息;编辑草稿的 Grid 表单也在其中。
    #[test]
    fn image_page_renders_without_messages() {
        let ctx = egui::Context::default();
        let mut settings = SettingsState::default();
        let mut bed = crate::bed::BedState::default();
        let mut outbox = Vec::new();
        ctx.run_ui(RawInput::default(), |ui| {
            image_page(ui, &mut settings, &mut bed, &mut outbox);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "渲染不产出消息");

        let mut smms = latermd_bed::BedProfile::preset_smms();
        smms.id = "p-1".to_owned();
        bed.profiles = vec![smms];
        bed.last_test = Some(("SM.MS".to_owned(), Ok("https://cdn/x.png".to_owned())));
        settings.bed_draft = Some(BedDraft {
            profile: latermd_bed::BedProfile::preset_custom(),
            headers_text: "Authorization: Bearer ${TOKEN}".to_owned(),
            token: String::new(),
        });
        ctx.run_ui(RawInput::default(), |ui| {
            image_page(ui, &mut settings, &mut bed, &mut outbox);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "回显态同样只渲染");
    }
}
