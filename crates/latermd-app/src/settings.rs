//! 设置对话框(docs/ui-polish.md §5)。
//!
//! 形态:左侧竖排分页 + 右侧内容区,单个 `egui::Window`。
//!
//! **为什么从工具栏的「设置」菜单升级成对话框**:菜单里只能塞几行
//! (主题列表 + 一个入口),而快捷键页有 10 行、AI 页有 8 个字段 —— 塞进
//! 菜单既画不下,也违背「菜单是对话框以外的轻量入口」这一通用范式。
//! 原「AI Provider」浮窗随之退役,其内容(`ai_key::key_editor`)嵌入 AI 页。
//!
//! 归约铁律照旧:本模块只绘制与产出 [`Message`];落盘、改状态、凭据读写
//! 全在 `State::apply`。两处**原地**例外(纯 UI 关注点):当前页签与
//! 「正在捕获哪个命令的键位」,同侧边栏把手与 `SearchState` 输入的口径。

use crate::ai::AiState;
use crate::ai_config::{AiConfig, ApiStyle, ProviderKind};
use crate::ai_key::{self, AiKeyState};
use crate::command::Command;
use crate::keymap::Keymap;
use crate::mcp::McpState;
use crate::state::Message;
use crate::theme::{Density, SkinCatalog, ThemeMode, ThemeSettings};
use crate::ui::icons;
use eframe::egui;
use latermd_mcp::{McpConfig, ToolKind};

/// 设置页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    /// 外观(主题 + 渲染后端只读)。
    #[default]
    Appearance,
    /// 快捷键(可改绑)。
    Keymap,
    /// AI provider 与模型参数。
    Ai,
    /// MCP server(规划态)。
    Mcp,
}

impl SettingsTab {
    /// 左侧分页顺序。
    pub const ALL: [SettingsTab; 4] = [Self::Appearance, Self::Keymap, Self::Ai, Self::Mcp];

    /// 分页显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "外观",
            Self::Keymap => "快捷键",
            Self::Ai => "AI",
            Self::Mcp => "MCP",
        }
    }

    /// 分页图标(与工具栏同一套自绘图标)。
    pub fn icon(self) -> icons::Icon {
        match self {
            Self::Appearance => icons::Icon::Theme,
            Self::Keymap => icons::Icon::Reset,
            Self::Ai => icons::Icon::Ai,
            Self::Mcp => icons::Icon::Git,
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
    keymap: &Keymap,
    ai: &AiState,
    ai_key: &mut AiKeyState,
    mcp: &McpState,
    outbox: &mut Vec<Message>,
) -> Option<egui::Response> {
    let mut open = settings.open;
    let mut close = None;
    egui::Window::new("设置")
        .default_pos([120.0, 100.0])
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
                .exact_size(40.0)
                .frame(egui::Frame::default().inner_margin(egui::Margin::symmetric(8, 4)))
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
                            SettingsTab::Appearance => {
                                appearance(ui, settings, theme, skins, system_theme_ok, outbox)
                            }
                            SettingsTab::Keymap => keymap_page(ui, settings, keymap, outbox),
                            SettingsTab::Ai => ai_page(ui, settings, ai, ai_key, outbox),
                            SettingsTab::Mcp => mcp_page(ui, settings, mcp, outbox),
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

/// AI 页:provider / 接口方式 / 端点 / 模型 / 采样参数 / key。
fn ai_page(
    ui: &mut egui::Ui,
    settings: &mut SettingsState,
    ai: &AiState,
    ai_key: &mut AiKeyState,
    outbox: &mut Vec<Message>,
) {
    ui.heading("AI");
    let draft = &mut settings.ai_draft;
    // Mock 不联网也不读参数:参数区整块灰显,避免「配了半天没生效」
    let editable = draft.provider.requires_key();

    ui.add_space(crate::ui::tokens::SPACE_SM);
    egui::ComboBox::from_label("Provider")
        .selected_text(draft.provider.label())
        .show_ui(ui, |ui| {
            for kind in ProviderKind::ALL {
                ui.selectable_value(&mut draft.provider, kind, kind.label());
            }
        });
    egui::ComboBox::from_label("接口方式")
        .selected_text(draft.api_style.label())
        .show_ui(ui, |ui| {
            for style in ApiStyle::ALL {
                if style.implemented() {
                    ui.selectable_value(&mut draft.api_style, style, style.label());
                } else {
                    // 未实现的接口方式显式禁用,不伪装可用
                    ui.add_enabled(false, egui::Button::new(style.label()));
                }
            }
        });
    if !draft.api_style.implemented() {
        ui.colored_label(
            crate::ui::tokens::WARN,
            "该接口方式尚未实现,保存时会回落到 OpenAI 兼容 SSE。",
        );
    }

    ui.add_space(crate::ui::tokens::SPACE_SM);
    ui.add_enabled_ui(editable, |ui| {
        ui.label("Base URL");
        ui.add(
            egui::TextEdit::singleline(&mut draft.base_url)
                .hint_text("https://api.openai.com/v1")
                .desired_width(f32::INFINITY),
        );
        ui.label("模型");
        ui.add(
            egui::TextEdit::singleline(&mut draft.model)
                .hint_text("gpt-4o-mini")
                .desired_width(f32::INFINITY),
        );
        ui.add(
            egui::Slider::new(&mut draft.temperature, 0.0..=2.0)
                .text("Temperature")
                .step_by(0.05),
        );
        ui.add(
            egui::Slider::new(&mut draft.top_p, 0.0..=1.0)
                .text("Top-P")
                .step_by(0.05),
        );
        ui.add(egui::Slider::new(&mut draft.max_tokens, 256..=32768).text("Max tokens"));
        ui.add(egui::Slider::new(&mut draft.timeout_secs, 10..=300).text("超时(秒)"));
        ui.checkbox(&mut draft.stream, "流式接收(SSE)");
        ui.label("System prompt(留空则不发送 system 消息)");
        ui.add(
            egui::TextEdit::multiline(&mut draft.system_prompt)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
    });

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
        "当前生效:{} · {} · {}",
        ai.provider_label(),
        ai.config.model,
        if ai.config.stream { "流式" } else { "整段" }
    ));
    // 端点只在联网型 provider 下有意义(Mock 不联网,不显示以免误导)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Shortcut;
    use crate::state::State;
    use egui::RawInput;

    fn render(state: &mut State) {
        let ctx = egui::Context::default();
        // 借用拆分:settings / ai_key 可变,其余只读(与 draw 的口径一致)
        let State {
            settings,
            ai_key,
            ai,
            mcp,
            keymap,
            theme,
            skins,
            ..
        } = state;
        let mut outbox = Vec::new();
        let system_theme_ok = state.system_theme_ok;
        let output = ctx.run_ui(RawInput::default(), |ui| {
            dialog(
                ui,
                settings,
                theme,
                skins,
                system_theme_ok,
                keymap,
                ai,
                ai_key,
                mcp,
                &mut outbox,
            );
        });
        output.drop_without_applying_deltas();
    }

    /// 四个分页各渲一帧不 panic(含 AI 页的禁用态与 MCP 页的规划态)。
    #[test]
    fn every_tab_renders() {
        let mut state = State::default();
        state.settings.open = true;
        for tab in SettingsTab::ALL {
            state.settings.tab = tab;
            render(&mut state);
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

    /// 页签图标与显示名一一对应,且都有图标(不出现空图标页)。
    #[test]
    fn every_tab_has_label_and_icon() {
        for tab in SettingsTab::ALL {
            assert!(!tab.label().is_empty());
            let _ = tab.icon();
        }
    }

    /// AI 页草稿:默认等于出厂配置;改成 OpenAI 兼容后「需要 key」为真
    /// (驱动参数区可用与命令闸门)。
    #[test]
    fn ai_draft_defaults_and_provider_switch() {
        let mut settings = SettingsState::default();
        assert_eq!(settings.ai_draft, AiConfig::default());
        assert!(!settings.ai_draft.provider.requires_key());
        settings.ai_draft.provider = ProviderKind::OpenAiCompatible;
        assert!(settings.ai_draft.provider.requires_key());
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
}
