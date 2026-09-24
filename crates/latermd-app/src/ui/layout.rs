//! 布局:顶部菜单栏 + 三栏(侧边栏 / 编辑器 / 预览,docs/adr-005 §3.2)。
//!
//! 顺序铁律:panel 添加顺序决定嵌套,先加的最外层;`CentralPanel` 必须最后加。
//! `App::logic` 只归约状态,`App::ui` 只绘制,两者严格分离(铁律)。

use crate::state::SidebarState;
use crate::LaterMdApp;
use eframe::egui;

impl LaterMdApp {
    /// `logic` 帧的全部归约逻辑。单独成函数是因为 [`eframe::Frame`] 的字段
    /// 是 `pub(crate)`,测试里造不出来;归约本身不碰 frame。
    fn reduce(&mut self, ctx: &egui::Context) {
        // 只做状态归约,严格禁止在此绘制任何 UI(docs/adr-005 §2.3)。
        let LaterMdApp {
            state,
            outbox,
            window_title,
        } = self;
        for message in std::mem::take(outbox) {
            state.apply(message);
        }
        // 命令快捷键(统一清单见 `crate::command`)。eframe 在 begin_pass 之后
        // 调 logic,本帧按键事件此刻可见;消费即从输入流移除,TextEdit 即使
        // 聚焦也收不到;无 COMMAND 修饰的普通字符不匹配任何绑定,原样放行。
        for cmd in crate::command::poll_shortcuts(ctx) {
            state.apply(cmd.message());
        }
        // 主题投影到 context:egui 主题(外壳)+ MarkdownStyle(正文,含代码
        // 高亮自动随 dark/light)。带 staleness 检查,空闲帧近零开销;首帧前
        // main 已装载一次,这里覆盖此后每次切换。
        state.theme.apply(ctx);
        state.end_of_logic();

        // 窗口标题只在变化时下发,避免每帧一次原生 set_title
        let title = state.document.window_title();
        if *window_title != title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            *window_title = title;
        }
    }

    /// `App::ui` 的面板主体。独立成函数是为了测试能在同一 run_ui 帧里按
    /// eframe 顺序(先 `reduce` 后绘制)跑完整帧。
    fn draw(&mut self, ui: &mut egui::Ui) {
        // ① 最外层:顶部菜单栏(全部命令的可发现性入口)
        egui::Panel::top("menubar").show(ui, |ui| {
            crate::ui::menubar::ui(ui, &mut self.outbox);
        });

        // ② 次外层:侧边栏(可折叠)。show_collapsible 原地持有 `&mut visible`,
        // 因此先把 sidebar 解构成 `visible` 与其余字段,闭包只捕获后者。
        // 大纲数据与光标位置只读借用 `preview`/`cursor`(与 `visible` 不相交)。
        let SidebarState {
            visible,
            active_tab,
        } = &mut self.state.sidebar;
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .size_range(160.0..=400.0)
            .show_collapsible(ui, visible, |ui| {
                crate::ui::sidebar::ui(
                    ui,
                    active_tab,
                    &self.state.preview.outline,
                    self.state.cursor.byte,
                    &mut self.outbox,
                );
            });

        // ③ 次外层:编辑器(顶部文件工具栏 + 源码)。TextEdit 是立即模式控件,
        // 必须原地持有 `&mut` 缓冲,因此 editor、preview 快照与大纲光标的
        // 借用下放到本面板闭包内(归约/绘制二分对这对"控件附属状态"的
        // 例外见 state.rs)。
        let state = &mut self.state;
        let outbox = &mut self.outbox;
        egui::Panel::left("editor")
            .resizable(true)
            .default_size(500.0)
            .show(ui, |ui| {
                crate::ui::toolbar::ui(ui, &state.document, state.theme.mode, outbox);
                crate::ui::editor::ui(ui, &mut state.editor, &mut state.preview, &mut state.cursor);
            });

        // ④ 必须最后:预览
        egui::CentralPanel::default().show(ui, |ui| {
            crate::ui::preview::ui(ui, &self.state.preview);
        });
    }
}

impl eframe::App for LaterMdApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reduce(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state;
    use egui::{Event, FullOutput, Key, Modifiers, RawInput, ViewportCommand};

    fn key_s(modifiers: Modifiers) -> Event {
        Event::Key {
            key: Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// 跑一帧归约(与 eframe 同序:begin_pass 之后调 logic),返回该帧下发的
    /// 窗口标题命令。
    fn reduce(app: &mut LaterMdApp, events: Vec<Event>) -> Vec<String> {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events,
                ..Default::default()
            },
            |ui| app.reduce(ui.ctx()),
        );
        let titles = title_commands(&output);
        output.drop_without_applying_deltas();
        titles
    }

    fn title_commands(output: &FullOutput) -> Vec<String> {
        output
            .viewport_output
            .values()
            .flat_map(|viewport| viewport.commands.iter())
            .filter_map(|command| match command {
                ViewportCommand::Title(title) => Some(title.clone()),
                _ => None,
            })
            .collect()
    }

    /// 标题随文件名与 dirty 变化;无变化帧不重复下发(避免每帧 set_title)。
    #[test]
    fn title_tracks_document_and_dirty() {
        let mut app = LaterMdApp::default();
        assert_eq!(reduce(&mut app, Vec::new()), vec!["LaterMD — 未命名"]);

        app.state.editor.insert_chars(0, "改动");
        assert_eq!(reduce(&mut app, Vec::new()), vec!["LaterMD — 未命名*"]);

        // 空闲帧:标题未变,不再下发
        assert!(reduce(&mut app, Vec::new()).is_empty());
    }

    /// Ctrl+S 在已有路径时直接落盘:文件字节与缓冲一致,标题 `*` 消失。
    /// (无路径时会弹另存为对话框,不能在无头测试里走,由 state::tests 覆盖。)
    #[test]
    fn ctrl_s_saves_to_known_path() {
        let path = std::env::temp_dir().join(format!("latermd-layout-{}.md", std::process::id()));
        let mut app = LaterMdApp::default();
        app.state.document.path = Some(path.clone());
        app.state.editor.insert_chars(0, "# 落盘\r\n");

        let titles = reduce(&mut app, vec![key_s(Modifiers::COMMAND)]);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            app.state.editor.text().as_bytes(),
            "保存字节原样,含 CRLF"
        );
        assert!(!app.state.editor.is_dirty());
        let expected = format!("LaterMD — {}", path.file_name().unwrap().to_string_lossy());
        assert_eq!(titles.last().map(String::as_str), Some(expected.as_str()));
        let _ = std::fs::remove_file(&path);
    }

    /// 快捷键与命令键在 TextEdit 聚焦时仍触发,且不劫持普通字符输入
    /// (本模块任务的验收点):同一帧内先 `reduce`(logic,消费 Ctrl+S)再
    /// `draw`(ui,TextEdit 聚焦绘制),命令落盘而缓冲里不出现多余的 `s`;
    /// 下一帧无修饰文本事件正常进入缓冲。
    #[test]
    fn command_keys_fire_with_editor_focused_and_plain_typing_flows() {
        let path =
            std::env::temp_dir().join(format!("latermd-layout-{}-focus.md", std::process::id()));
        let mut app = LaterMdApp::default();
        app.state.document.path = Some(path.clone());
        app.state.editor.insert_chars(0, "正文");
        // 默认文档是 SAMPLE_MD 开头再插「正文」,基线取帧序列开始前的全文
        let baseline = app.state.editor.text().to_owned();
        let ctx = egui::Context::default();

        // 帧 1:渲染全部面板(菜单栏/侧边栏/编辑器/预览),无输入
        let output = ctx.run_ui(RawInput::default(), |ui| {
            app.reduce(ui.ctx());
            app.draw(ui);
        });
        output.drop_without_applying_deltas();
        // 模拟用户点进编辑区:直接对编辑器 widget 请求焦点
        ctx.memory_mut(|mem| mem.request_focus(crate::ui::editor::editor_id()));

        // 帧 2:聚焦状态下按 Ctrl+S —— logic 先消费,TextEdit 收不到该键
        let output = ctx.run_ui(
            RawInput {
                events: vec![key_s(Modifiers::COMMAND)],
                ..Default::default()
            },
            |ui| {
                app.reduce(ui.ctx());
                app.draw(ui);
            },
        );
        output.drop_without_applying_deltas();
        assert!(path.exists(), "聚焦状态的 Ctrl+S 仍触发了保存命令");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            app.state.editor.text().as_bytes()
        );
        assert_eq!(
            app.state.editor.text(),
            baseline,
            "命令键未被 TextEdit 当输入吞掉,也没有插入 's'"
        );
        assert!(
            ctx.memory(|mem| mem.has_focus(crate::ui::editor::editor_id())),
            "命令键不抢编辑器焦点"
        );

        // 帧 3:无修饰的普通字符照常进入缓冲
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::Text("s".into())],
                ..Default::default()
            },
            |ui| {
                app.reduce(ui.ctx());
                app.draw(ui);
            },
        );
        output.drop_without_applying_deltas();
        // 新建 TextEdit 状态的默认光标在文末,普通字符追加到缓冲尾部
        assert_eq!(
            app.state.editor.text(),
            format!("{baseline}s"),
            "普通字符输入未被劫持"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// 主题切换消息走完整归约链:同帧内 egui 主题已翻转(设置菜单点击的下一
    /// 帧面板即按新模式绘制),且 settings.json 落盘(注入临时目录)。
    #[test]
    fn theme_message_flips_context_and_persists() {
        let dir = std::env::temp_dir().join(format!("latermd-layout-{}-theme", std::process::id()));
        let mut app = LaterMdApp::default();
        app.state.settings_dir = Some(dir.clone());

        let ctx = egui::Context::default();
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        let output = ctx.run_ui(RawInput::default(), |ui| {
            app.outbox
                .push(state::Message::ThemeChanged(crate::theme::ThemeMode::Light));
            app.reduce(ui.ctx());
        });
        output.drop_without_applying_deltas();

        assert_eq!(ctx.theme(), egui::Theme::Light, "切换同帧生效");
        assert!(!ctx.global_style().visuals.dark_mode);
        assert!(dir.join("settings.json").exists(), "重启保持的数据已落盘");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
