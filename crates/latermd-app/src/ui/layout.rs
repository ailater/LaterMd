//! 布局:顶部菜单栏 + 三栏(侧边栏 / 编辑器 / 预览,docs/adr-005 §3.2)。
//!
//! 顺序铁律:panel 添加顺序决定嵌套,先加的最外层;`CentralPanel` 必须最后加。
//! `App::logic` 只归约状态,`App::ui` 只绘制,两者严格分离(铁律)。

use crate::state::{Message, SidebarState};
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
        // AI 后台流式收流:channel 里的 chunk 翻成 Message 并入本帧归约
        // (文本回编辑器只走 Message,后台线程不触碰 UI 状态)
        outbox.extend(state.poll_ai());
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

        // 搜索去抖到点:在归约侧发起,不放 `ui::sidebar`——归约每帧必跑、
        // 不看侧边栏页签,输入后 300ms 内切走也照常搜;`SearchState::start`
        // 入口先清计时,过期时刻不残留,下方按 due 要帧的逻辑才不会在
        // due 过期后每帧 request_repaint(立即)满帧空转。
        if state
            .search
            .debounce_due
            .is_some_and(|due| due <= std::time::Instant::now())
        {
            state.apply(Message::SearchRequested);
        }
        // 搜索的重绘驱动(egui 空闲不来帧,后台进度必须显式要帧):
        // 去抖等待中按剩余时长要一帧,到点由上一段发起;流式结果进行中
        // 持续要帧,`Done` 落回 Finished 后自然停。
        if let Some(due) = state.search.debounce_due {
            ctx.request_repaint_after(due.saturating_duration_since(std::time::Instant::now()));
        }
        if state.search.is_running() {
            ctx.request_repaint();
        }
        // AI 流式的重绘驱动:chunk 到达即要在下一帧收流归约,预览才跟得上
        // 100ms/chunk 的节奏;AiDone 归约清标志后自然停。
        if state.ai.is_streaming() {
            ctx.request_repaint();
        }

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
        // 大纲数据、文件树与当前文档路径只读借用 `preview`/`file_tree`/
        // `document`(与 `visible` 不相交);树的交互全部经由消息归约。
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
                    &self.state.file_tree,
                    self.state.document.path.as_deref(),
                    crate::ui::sidebar::OutlineView {
                        items: &self.state.preview.outline,
                        cursor_byte: self.state.cursor.byte,
                    },
                    &mut self.state.search,
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

    /// 搜索去抖到点在归约侧发起:到点帧即清 `debounce_due`,且不看侧边栏
    /// 页签——停在 Files 页照样发起(到点判断若放在 Search 页渲染里,输入
    /// 后切走页签即滞留过期时刻,每帧要帧空转)。
    #[test]
    fn search_debounce_fires_in_reduce_even_when_tab_switched_away() {
        let root =
            std::env::temp_dir().join(format!("latermd-layout-{}-debounce", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.md"), "latermd 命中\n").unwrap();

        let mut app = LaterMdApp::default();
        app.state.file_tree.root = Some(root.clone());
        app.state.sidebar.active_tab = state::SidebarTab::Files;
        app.state.search.query = "latermd".into();
        app.state.search.debounce_due =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();

        assert_eq!(app.state.search.debounce_due, None, "到点帧即清去抖计时");
        assert_eq!(
            app.state.search.status,
            crate::search::SearchStatus::Running,
            "切离 Search 页也照常发起"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 空输入的去抖计时到点:同样被清空(不发起、不残留过期时刻),且后续
    /// 归约不再安排任何重绘——修复点:过期 due 曾驱动每帧
    /// request_repaint(立即)满帧空转。egui 在无未偿付要帧请求时
    /// repaint_delay 为 Duration::MAX;但视口首帧自带约两帧 settle 重绘
    /// (egui `ViewportRepaintInfo` 默认 `outstanding: 1`),断言落在第 4 帧。
    #[test]
    fn search_debounce_due_cleared_for_empty_query_without_repaint_loop() {
        let root = std::env::temp_dir().join(format!(
            "latermd-layout-{}-debounce-empty",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let mut app = LaterMdApp::default();
        app.state.file_tree.root = Some(root.clone());
        app.state.search.debounce_due =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(1));

        let ctx = egui::Context::default();

        // 帧 1:到点即清计时,due 不残留到下一帧
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();
        assert_eq!(
            app.state.search.debounce_due, None,
            "空输入到点也清计时,不残留过期时刻"
        );
        assert_eq!(app.state.search.status, crate::search::SearchStatus::Idle);

        // 帧 2-4:due 已清、无搜索在途。egui 视口自带约两帧 settle(实测
        // 帧 2 仍有一次 0ns,帧 3 起 MAX),关键在收敛到 MAX——修复前
        // 过期 due 使每帧输出都是 0ns(request_repaint 即 ZERO),满帧空转。
        let mut delay = None;
        for _ in 2..=4 {
            let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
            delay = Some(
                output
                    .viewport_output
                    .values()
                    .map(|viewport| viewport.repaint_delay)
                    .min()
                    .unwrap(),
            );
            output.drop_without_applying_deltas();
        }
        assert_eq!(
            delay,
            Some(std::time::Duration::MAX),
            "到点清空后不再安排任何重绘(过期 due 曾致满帧空转)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// AI 流式收流接线:reduce 每帧从 AI channel 取 chunk 翻成 Message 归约,
    /// 正文追加进编辑器、结束块清流式标志(后台线程 → mpsc → Message →
    /// apply 的完整链路;不经 provider 线程,零时序依赖)。
    #[test]
    fn reduce_drains_ai_chunks_into_editor() {
        let mut app = LaterMdApp::default();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(latermd_ai::Chunk {
            delta: "第一块".into(),
            done: false,
        })
        .unwrap();
        tx.send(latermd_ai::Chunk {
            delta: String::new(),
            done: true,
        })
        .unwrap();
        app.state.ai.rx = Some(rx);
        app.state.ai.streaming = true;

        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();

        assert!(
            app.state.editor.text().ends_with("第一块"),
            "chunk 已按序归约追加到文档末尾"
        );
        assert!(app.state.editor.is_dirty(), "AI 写入置 dirty");
        assert!(!app.state.ai.is_streaming(), "AiDone 已归约收尾");
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
