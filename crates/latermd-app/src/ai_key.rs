//! AI Provider 凭据的设置区(roadmap 阶段 4「凭据管理」的 UI 接线)。
//!
//! 归约铁律:凭据读写只发生在 `State::apply` 归约
//! ([`Message::AiKeySaved`] / [`Message::AiKeyCleared`]),UI 只持有草稿与
//! 展示状态、只产出消息;后端调用全部走 `latermd-creds`。
//!
//! 安全:草稿([`AiKeyState::draft`])只存在于内存,保存成功即清空,绝不
//! 落盘/不序列化;已保存的 key 永不读回 UI(状态行只报「已配置」,不回显
//! 值),密码框经 `TextEdit::password` 全程掩码显示。

use crate::state::Message;
use eframe::egui;

/// 设置区状态:草稿 + 展示三态 + 凭据操作集。
///
/// `creds` 是注入点(生产 `system()`,测试注入内存/失败后端),与
/// `State::settings_dir` 同款口径;后端可用性(`backend_ok`)由启动探测与
/// 保存/清除结果维护,状态行凭它三态分流。
pub struct AiKeyState {
    /// 密码框草稿。仅 UI 粘合用:保存成功即清空,生命周期不超出本结构,
    /// 绝不进 settings.json 等任何落盘物。
    pub draft: String,
    /// 系统凭据里已存有 AI key(启动探测与保存/清除成功后同步)。
    pub configured: bool,
    /// 凭据后端可用;`false` = 状态行提示回退环境变量。
    pub backend_ok: bool,
    /// 凭据操作集(生产系统后端;测试注入)。
    pub(crate) creds: latermd_creds::Credentials,
}

impl Default for AiKeyState {
    fn default() -> Self {
        Self {
            draft: String::new(),
            configured: false,
            // 乐观默认:真实可用性由启动探测(`AiKeyState::probe`)修正,
            // 未探测(如测试)按「可用」显示,不误报「后端不可用」
            backend_ok: true,
            creds: latermd_creds::Credentials::system(),
        }
    }
}

impl AiKeyState {
    /// 状态行文案,三态:`backend_ok == false` 优先于「已配置」——后端坏了
    /// 时 configured 不可知,回退提示才是可行动的信息。
    pub(crate) fn status_text(&self) -> &'static str {
        if !self.backend_ok {
            "系统凭据后端不可用,将回退环境变量 LATERMD_AI_API_KEY"
        } else if self.configured {
            "已配置"
        } else {
            "未配置"
        }
    }

    /// 保存草稿到系统凭据(`Message::AiKeySaved` 的归约,实际写入在
    /// latermd-creds)。成功:置已配置、草稿清空(UI 不回显已存 key);空白
    /// 拒绝(`CredentialError::BlankSecret`);后端失败不动本组状态——
    /// 「不可用」由 `Message::AiKeyBackendUnavailable` 归约统一置。
    pub(crate) fn save(&mut self) -> Result<(), latermd_creds::CredentialError> {
        self.creds.set_ai_api_key(&self.draft)?;
        self.configured = true;
        self.backend_ok = true;
        self.draft.clear();
        Ok(())
    }

    /// 从系统凭据删除 AI key(`Message::AiKeyCleared` 的归约);幂等(条目
    /// 本就不存在也算成功),成功置未配置。
    pub(crate) fn clear(&mut self) -> Result<(), latermd_creds::CredentialError> {
        self.creds
            .delete_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT)?;
        self.configured = false;
        self.backend_ok = true;
        Ok(())
    }

    /// 启动探测:后端可用时按真实存在性刷新 configured,不可用时只置
    /// `backend_ok = false`(configured 不可知,保守保持现值)。
    pub fn probe(&mut self) {
        match self
            .creds
            .get_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT)
        {
            Ok(secret) => {
                self.configured = secret.is_some();
                self.backend_ok = true;
            }
            Err(_) => self.backend_ok = false,
        }
    }
}

/// API key 编辑区(嵌入设置对话框的 AI 页,docs/ui-polish.md §6)。
///
/// 原形态是工具栏「设置」菜单里的独立浮窗;AI 配置页有 8 个字段,浮窗
/// 装不下,故改为可被设置页嵌入的一块内容(本身不再开窗口)。返回
/// (保存, 清除)按钮响应供测试定位。
pub(crate) fn key_editor(
    ui: &mut egui::Ui,
    key: &mut AiKeyState,
    outbox: &mut Vec<Message>,
) -> (egui::Response, egui::Response) {
    ui.label(
        "API key 保存到系统凭据(Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service),不写入任何文件。",
    );
    ui.weak("未配置或后端不可用时,AI 命令回退环境变量 LATERMD_AI_API_KEY。");
    ui.add(
        egui::TextEdit::singleline(&mut key.draft)
            .password(true)
            .hint_text("API key"),
    );
    let buttons = ui.horizontal(|ui| {
        // 空白草稿/未配置时目标状态已达成,禁用防误触;归约侧仍有同款
        // 防线(BlankSecret 拒绝、删除幂等)
        let save = ui.add_enabled(!key.draft.trim().is_empty(), egui::Button::new("保存"));
        if save.clicked() {
            outbox.push(Message::AiKeySaved);
        }
        let clear = ui.add_enabled(key.configured, egui::Button::new("清除"));
        if clear.clicked() {
            outbox.push(Message::AiKeyCleared);
        }
        (save, clear)
    });
    let text = egui::RichText::new(key.status_text());
    let text = if !key.backend_ok {
        // 与回滚 dirty 警示同档黄:可行动的降级告知
        text.color(crate::ui::tokens::WARN)
    } else if key.configured {
        text.strong().color(crate::ui::tokens::OK)
    } else {
        text.weak()
    };
    ui.label(text);
    buttons.inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Message, State};
    use egui::{Event, PointerButton, RawInput, Rect};
    use latermd_creds::{CredentialBackend, CredentialError, Credentials};
    use std::cell::Cell;
    use std::sync::Arc;

    // 安全红线:与本模块相关的「凭据值」全部是 placeholder-* 占位假值;
    // 断言只做存在性/状态位与「文案不含值」方向,不把任何真实 key 写进
    // 断言(decisions-pending #20 ④)。

    /// 注入内存后端的 State(凭据读写全程可控、不碰系统)。
    fn state_in_memory() -> State {
        let mut state = State::default();
        state.ai_key.creds = Credentials::in_memory();
        state
    }

    /// 失败后端:所有操作 Err,验证归约对后端不可用的降级。
    struct FailingBackend;

    fn backend_err(action: &'static str) -> CredentialError {
        CredentialError::Backend {
            action,
            service: latermd_creds::SERVICE.to_owned(),
            account: latermd_creds::AI_ACCOUNT.to_owned(),
            cause: "no secret service".to_owned(),
        }
    }

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

    /// 漏值后端:写入错误的原因里故意嵌入 secret,验证 notice 端到端不含值
    /// (latermd-creds 的消毒闸在 set_secret 内,这里测 app 侧拿到的文案)。
    struct LeakyBackend;

    impl CredentialBackend for LeakyBackend {
        fn set(&self, _: &str, _: &str, secret: &str) -> Result<(), CredentialError> {
            Err(CredentialError::Backend {
                action: "写入",
                service: latermd_creds::SERVICE.to_owned(),
                account: latermd_creds::AI_ACCOUNT.to_owned(),
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

    /// 状态行三态文案:不可用优先于已配置;可用时按 configured 分流。
    #[test]
    fn status_text_three_states() {
        let mut key = AiKeyState::default();
        assert_eq!(key.status_text(), "未配置");
        key.configured = true;
        assert_eq!(key.status_text(), "已配置");
        key.backend_ok = false;
        assert_eq!(
            key.status_text(),
            "系统凭据后端不可用,将回退环境变量 LATERMD_AI_API_KEY"
        );
    }

    /// 保存/清除全链路归约(内存后端):保存置已配置并清草稿(存在性断言,
    /// 不回显值),状态行随之变化;清除幂等地回到未配置。
    #[test]
    fn save_and_clear_roundtrip_through_messages() {
        let mut state = state_in_memory();
        state.ai_key.draft = " placeholder-key ".into();

        state.apply(Message::AiKeySaved);
        assert!(state.ai_key.configured, "保存后置已配置");
        assert!(state.ai_key.draft.is_empty(), "草稿已清空,不回显");
        assert!(state.ai_key.backend_ok);
        assert!(
            state
                .ai_key
                .creds
                .has_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT),
            "系统凭据里确实存在条目"
        );
        assert!(state.document.notice.is_none(), "成功路径无提示");
        assert_eq!(state.ai_key.status_text(), "已配置");

        // 重复保存(覆盖语义)与清除
        state.ai_key.draft = "placeholder-key-2".into();
        state.apply(Message::AiKeySaved);
        state.apply(Message::AiKeyCleared);
        assert!(!state.ai_key.configured, "清除后回到未配置");
        assert_eq!(state.ai_key.status_text(), "未配置");
        // 幂等:未配置再清一次也无提示
        state.apply(Message::AiKeyCleared);
        assert!(state.document.notice.is_none());
        assert!(!state
            .ai_key
            .creds
            .has_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT));
    }

    /// 空白草稿:归约拒绝(BlankSecret 文案进提示行),状态不动。
    #[test]
    fn blank_draft_save_is_rejected() {
        let mut state = state_in_memory();
        state.ai_key.draft = "   ".into();
        state.apply(Message::AiKeySaved);
        assert!(!state.ai_key.configured);
        let notice = state.document.notice.as_deref().unwrap();
        assert!(notice.contains("空白"), "{notice}");
        assert!(
            !state
                .ai_key
                .creds
                .has_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT),
            "空白值不得入库"
        );
    }

    /// 后端不可用(AiKeySaved/AiKeyCleared 失败):错误文案进提示行、置
    /// 不可用状态;文案不含凭据值(漏值后端端到端验证消毒闸)。
    #[test]
    fn backend_failure_degrades_to_unavailable_without_leaking_secret() {
        let mut state = State::default();
        state.ai_key.creds = Credentials::new(Arc::new(LeakyBackend));
        state.ai_key.draft = "placeholder-leaky".into();

        state.apply(Message::AiKeySaved);
        assert!(!state.ai_key.backend_ok, "置后端不可用");
        assert!(!state.ai_key.configured);
        assert_eq!(
            state.ai_key.status_text(),
            "系统凭据后端不可用,将回退环境变量 LATERMD_AI_API_KEY"
        );
        let notice = state.document.notice.as_deref().unwrap();
        assert!(
            !notice.contains("placeholder-leaky"),
            "notice 不含凭据值:{notice}"
        );
        assert!(notice.contains("写入失败"), "错误可定位:{notice}");

        // 清除同样降级:configured 保守不动,不误报「已清除」
        state.ai_key.configured = true;
        state.ai_key.creds = Credentials::new(Arc::new(FailingBackend));
        state.apply(Message::AiKeyCleared);
        assert!(!state.ai_key.backend_ok);
        assert!(state.ai_key.configured, "后端失败时存在性不可知,保守保持");

        // 消息本身的归约(探测等内部路径复用同一置位语义)
        state.ai_key.backend_ok = true;
        state.apply(Message::AiKeyBackendUnavailable);
        assert!(!state.ai_key.backend_ok);
    }

    /// 启动探测:存在性刷新 configured;后端不可用只置不可用。
    #[test]
    fn probe_reflects_existence_and_backend_availability() {
        let mut state = state_in_memory();
        state
            .ai_key
            .creds
            .set_ai_api_key("placeholder-probe")
            .unwrap();
        state.ai_key.probe();
        assert!(state.ai_key.configured);
        assert!(state.ai_key.backend_ok);

        state
            .ai_key
            .creds
            .delete_secret(latermd_creds::SERVICE, latermd_creds::AI_ACCOUNT)
            .unwrap();
        state.ai_key.probe();
        assert!(!state.ai_key.configured);

        let mut failing = State::default();
        failing.ai_key.creds = Credentials::new(Arc::new(FailingBackend));
        failing.ai_key.probe();
        assert!(!failing.ai_key.backend_ok);
        assert_eq!(
            failing.ai_key.status_text(),
            "系统凭据后端不可用,将回退环境变量 LATERMD_AI_API_KEY"
        );
    }

    /// key 闸门(需 key 的 provider):latermd-creds → 环境变量都无时,AI
    /// 流式命令被拦在状态栏(不发起流);系统凭据有 key 则放行。环境变量
    /// 显式移除保证断言不受宿主环境影响(本进程仅此处触碰该变量)。
    #[test]
    fn ai_stream_blocked_without_key_when_provider_requires_it() {
        std::env::remove_var(latermd_creds::API_KEY_ENV);
        let mut state = state_in_memory();
        state.ai.config.provider = crate::ai_config::ProviderKind::OpenAiCompatible;

        state.apply(Message::AiStart);
        assert!(!state.ai.is_streaming(), "无 key 不发起流");
        assert!(
            !state.editor.text().ends_with("\n\n"),
            "补空行等发起前置步骤未发生"
        );
        assert_eq!(
            state.document.notice.as_deref(),
            Some("未配置 API key(设置 → AI Provider)")
        );

        // commit message 入口同一道闸
        state.apply(Message::AiCommitRequested);
        assert_eq!(state.ai_commit_suggestion, None);
        assert_eq!(
            state.document.notice.as_deref(),
            Some("未配置 API key(设置 → AI Provider)")
        );

        // 摘要入口被拦时必须零副作用:旧摘要节原样保留(闸门在任何
        // 归约副作用之前,含「移除旧节」这步)
        state.editor.load("# 甲\n\n## AI 摘要\n\n> - 旧要点\n");
        state.apply(Message::AiSummaryRequested);
        assert!(
            state.editor.text().contains("旧要点"),
            "被拦的摘要请求不移除旧节"
        );
        assert!(!state.ai.is_streaming());
        assert_eq!(
            state.document.notice.as_deref(),
            Some("未配置 API key(设置 → AI Provider)")
        );

        // 存入 key 后放行(Mock provider 用不上 key,但闸门只管有无)
        state
            .ai_key
            .creds
            .set_ai_api_key("placeholder-gated")
            .unwrap();
        state.ai.runtime = crate::ai::AiRuntime::Mock(latermd_ai::MockProvider::with_interval(
            std::time::Duration::ZERO,
        ));
        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming(), "有 key 照常发起");
        state.ai.finish();
    }

    /// Mock provider(默认)无 key 也能跑:闸门直通,现有行为不回归。
    #[test]
    fn mock_provider_runs_without_key() {
        std::env::remove_var(latermd_creds::API_KEY_ENV);
        let mut state = state_in_memory();
        assert!(!state.ai.requires_key(), "前置:Mock 不需要 key");
        state.ai.runtime = crate::ai::AiRuntime::Mock(latermd_ai::MockProvider::with_interval(
            std::time::Duration::ZERO,
        ));

        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming());
        assert!(state.document.notice.is_none());
        state.ai.finish();
    }

    /// 编辑区交互:空草稿时「保存」禁用(点击不产消息);填入草稿并置已
    /// 配置后,「保存」「清除」点击分别产 AiKeySaved / AiKeyCleared。
    /// 按钮点击归属要求指针先停在目标上(实测),三帧节奏与 layout.rs 的
    /// 浮窗测试一致。
    #[test]
    fn key_editor_buttons_send_messages() {
        let ctx = egui::Context::default();
        let mut key = AiKeyState {
            creds: Credentials::in_memory(),
            ..AiKeyState::default()
        };
        let mut outbox = Vec::new();
        let rects = Cell::new((Rect::NOTHING, Rect::NOTHING));

        ctx.run_ui(RawInput::default(), |ui| {
            let (save, clear) = key_editor(ui, &mut key, &mut outbox);
            rects.set((save.rect, clear.rect));
        })
        .drop_without_applying_deltas();

        let (save_center, clear_center) = {
            let (save, clear) = rects.get();
            (save.center(), clear.center())
        };
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        // 空 draft + 未配置:点两个按钮都不产消息(禁用态拦下)
        for events in [
            vec![Event::PointerMoved(save_center)],
            vec![click(save_center, true)],
            vec![click(save_center, false)],
            vec![Event::PointerMoved(clear_center)],
            vec![click(clear_center, true)],
            vec![click(clear_center, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    key_editor(ui, &mut key, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert!(outbox.is_empty(), "禁用按钮不产消息:{outbox:?}");

        // 填草稿 + 已配置:保存/清除各自产消息
        key.draft = "placeholder-ui".into();
        key.configured = true;
        for events in [
            vec![Event::PointerMoved(save_center)],
            vec![click(save_center, true)],
            vec![click(save_center, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    key_editor(ui, &mut key, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::AiKeySaved]);
        outbox.clear();

        for events in [
            vec![Event::PointerMoved(clear_center)],
            vec![click(clear_center, true)],
            vec![click(clear_center, false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    key_editor(ui, &mut key, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(outbox, vec![Message::AiKeyCleared]);
        outbox.clear();

        // 三态状态行各渲一帧不 panic(文案断言已在 status_text 覆盖)
        for (configured, backend_ok) in [(false, true), (true, true), (false, false)] {
            key.configured = configured;
            key.backend_ok = backend_ok;
            ctx.run_ui(RawInput::default(), |ui| {
                key_editor(ui, &mut key, &mut outbox);
            })
            .drop_without_applying_deltas();
            assert!(outbox.is_empty());
        }
    }
}
