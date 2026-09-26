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
        // 命令快捷键(键位来自 `keymap`,用户可改;统一清单见 `crate::command`)。
        // eframe 在 begin_pass 之后调 logic,本帧按键事件此刻可见;消费即从
        // 输入流移除,TextEdit 即使聚焦也收不到;无 COMMAND 修饰的普通字符
        // 不匹配任何绑定,原样放行。
        //
        // 设置页正在捕获键位时**不**派发命令:此刻的按键是「新键位」而不是
        // 命令触发,否则会把 Ctrl+S 同时当成「保存」和「保存的新键位」。
        if state.settings.capture.is_some() {
            poll_capture(ctx, state);
        } else {
            let commands = crate::command::poll_shortcuts(ctx, &state.keymap);
            for cmd in commands {
                state.apply(cmd.message());
            }
        }
        // 系统主题节流刷新:只有「跟随系统」模式才轮询(返回下一次探测时刻),
        // 其余模式返回 None,egui 得以收敛到深度空闲。
        if let Some(due) = state.poll_system_theme(std::time::Instant::now()) {
            ctx.request_repaint_after(due.saturating_duration_since(std::time::Instant::now()));
        }
        // 主题投影到 context:egui 主题(外壳)+ 密度 token + MarkdownStyle
        // (正文,含代码高亮自动随 dark/light)。带 staleness 检查,空闲帧近零
        // 开销;首帧前 main 已装载一次,这里覆盖此后每次切换。`System` 已在
        // `resolved_theme` 里落到确定的明暗。
        state.theme.apply(ctx, state.resolved_theme());
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
        // Git 状态轮询:有文件树根且尚未降级才轮询(无根无事可刷;非 git
        // 目录零轮询——重探由换根/切 Git 页触发,egui 得以收敛到深度空闲,
        // 这正是 search 去抖测试守护的不变量)。到点即刷(同步毫秒级,见
        // `git_panel` 模块文档),并在到点前要一帧(空闲不来帧,轮询依赖
        // 显式 repaint)。
        if state.file_tree.root.is_some() && state.git.error.is_none() {
            if state.git.due() {
                state.refresh_git();
            }
            if state.git.error.is_none() {
                ctx.request_repaint_after(state.git.until_refresh());
            }
        }
        // AI 流式的重绘驱动:chunk 到达即要在下一帧收流归约,预览才跟得上
        // 100ms/chunk 的节奏;AiDone 归约清标志后自然停。
        if state.ai.is_streaming() {
            ctx.request_repaint();
        }

        // 窗口标题只在变化时下发,避免每帧一次原生 set_title
        let title = state.tabs.current().document.window_title();
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
            crate::ui::menubar::ui(ui, &self.state.keymap, &mut self.outbox);
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
                    self.state.tabs.current().document.path.as_deref(),
                    crate::ui::sidebar::OutlineView {
                        items: &self.state.tabs.current().preview.outline,
                        cursor_byte: self.state.tabs.current().cursor.byte,
                    },
                    &mut self.state.search,
                    &self.state.git,
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
                // 标签条(多标签 #11)在文件工具栏之上:先选文档,再对文档操作
                crate::ui::tabs::ui(ui, &state.tabs, outbox);
                crate::ui::toolbar::ui(ui, &state.tabs.current().document, &state.keymap, outbox);
                let tab = state.tabs.current_mut();
                let crate::tabs::TabState {
                    editor,
                    preview,
                    cursor,
                    id,
                    ..
                } = tab;
                crate::ui::editor::ui(
                    ui,
                    editor,
                    preview,
                    cursor,
                    crate::ui::editor::tab_editor_id(*id),
                );
            });

        // ④ 必须最后:预览(outbox 供 ai:// 链接与 ```ai 指令卡的 LinkHandler 产消息;
        // ai 只读,供指令卡状态行取流式标志与最近 prompt)
        egui::CentralPanel::default().show(ui, |ui| {
            crate::ui::preview::ui(
                ui,
                &self.state.tabs.current().preview,
                &self.state.ai,
                outbox,
            );
        });

        // ⑤ 顶层浮层:commit message 建议(存在才显示)。panel 顺序铁律只
        // 约束 panel(浮窗是独立 Area 层,不参与嵌套),画在最后取语义上的
        // 「最上层」。
        if let Some(subject) = self.state.ai_commit_suggestion.clone() {
            let (_, close) = commit_dialog(ui, &subject);
            if close.clicked() {
                outbox.push(Message::AiCommitDismissed);
            }
        }

        // ⑥ 顶层浮层:设置对话框(外观 / 快捷键 / AI / MCP)。凭据读写只在
        // 归约(Message),对话框只持草稿与展示状态;关闭按钮原地翻转开关。
        if self.state.settings.open {
            let state = &mut self.state;
            let settings = &mut state.settings;
            let ai_key = &mut state.ai_key;
            let ai = &state.ai;
            let mcp = &state.mcp;
            let keymap = &state.keymap;
            let theme = &state.theme;
            let skins = &state.skins;
            let system_theme_ok = state.system_theme_ok;
            let close = crate::settings::dialog(
                ui,
                settings,
                theme,
                skins,
                system_theme_ok,
                keymap,
                ai,
                ai_key,
                mcp,
                outbox,
            );
            if close.is_some_and(|close| close.clicked()) {
                settings.open = false;
            }
        }

        // ⑦ 顶层浮层:回滚确认(存在才显示)。egui 无内建阻塞模态,Window
        // 即确认弹窗(与 commit 建议浮窗同模式);文案显式警示不可逆,目标
        // 恰是编辑器当前文档时追加针对性警示(见 `checkout_extra_warning`),
        // checkout 只在「回滚」按钮点击之后的归约里执行。
        if let Some(path) = self.state.git.confirm_checkout.clone() {
            let open_in_editor = self.state.git.absolute_path(&path).is_some_and(|abs| {
                self.state.tabs.current().document.path.as_deref() == Some(abs.as_path())
            });
            let (confirm, cancel) = checkout_dialog(
                ui,
                &path,
                open_in_editor,
                self.state.tabs.current().editor.is_dirty(),
            );
            if confirm.clicked() {
                outbox.push(Message::GitCheckoutConfirmed);
            }
            if cancel.clicked() {
                outbox.push(Message::GitCheckoutCancelled);
            }
        }

        // ⑦.5 顶层浮层:脏标签关闭确认(标签条 × / Ctrl+W 触发,docs/auto-plan
        // #11「关闭脏标签确认模态」)。文案与回滚确认同款不可逆警示。目标按
        // 稳定 id 存(`TabsState::confirm_close`):模态是非阻塞 Window,打开
        // 期间其他关闭入口会使索引漂移;目标被别的路径关掉时 `TabsState::remove`
        // 已撤下确认,这里自然不再渲染。
        if let Some(tab) = self.state.tabs.confirm_close_tab() {
            let (confirm, cancel) = tab_close_dialog(ui, &tab.document.display_name());
            if confirm.clicked() {
                outbox.push(Message::TabCloseConfirmed);
            }
            if cancel.clicked() {
                outbox.push(Message::TabCloseCancelled);
            }
        }

        // ⑧ 底部状态栏:散落在工具栏/侧边栏边缘的只读信息收成一行
        // (docs/ui-polish.md §4),工具栏得以只留动作。
        egui::Panel::bottom("statusbar").show(ui, |ui| {
            status_bar(ui, &self.state);
        });
    }
}

/// 快捷键捕获(设置页「改键」):本帧的按键就是新键位。
///
/// 在归约侧消费而非 UI 侧:消费即把事件从输入流移除,编辑器收不到这个
/// 键,也不会被 [`crate::command::poll_shortcuts`] 当成命令触发。
/// Esc = 取消,Backspace/Delete(无修饰)= 清除绑定,其余按键走
/// [`Message::KeymapAssign`] 归约(撞键与不可绑定的拒绝都在那里)。
fn poll_capture(ctx: &egui::Context, state: &mut crate::state::State) {
    let Some(cmd) = state.settings.capture else {
        return;
    };
    let pressed = ctx.input_mut(|input| {
        let mut captured = None;
        input.events.retain(|event| match event {
            egui::Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } => {
                captured = Some((*key, *modifiers));
                false // 消费:不让编辑器与命令层再见到它
            }
            _ => true,
        });
        captured
    });
    let Some((key, modifiers)) = pressed else {
        return;
    };
    state.settings.capture = None;
    match key {
        egui::Key::Escape => {}
        egui::Key::Backspace | egui::Key::Delete if modifiers.is_none() => {
            state.apply(Message::KeymapCleared(cmd));
        }
        _ => state.apply(Message::KeymapAssign {
            cmd,
            shortcut: crate::keymap::Shortcut { modifiers, key },
        }),
    }
}

/// 底部状态栏:路径 · 行列 · 字数 · 主题 · 渲染后端 · AI · MCP。
fn status_bar(ui: &mut egui::Ui, state: &crate::state::State) {
    ui.horizontal_wrapped(|ui| {
        ui.weak(state.tabs.current().document.display_name());
        let text = state.tabs.current().editor.text();
        if let Some(byte) = state.tabs.current().cursor.byte {
            let (line, col) = cursor_position(text, byte);
            ui.weak(format!("行 {line}:{col}"));
        }
        ui.weak(format!("{} 字", text.chars().count()));
        separator(ui);
        ui.weak(state.theme.mode.label());
        ui.weak(crate::renderer_label(
            std::env::var("LATERMD_RENDERER").ok().as_deref(),
        ));
        separator(ui);
        let ai = if state.ai.is_streaming() {
            format!("{} · 生成中", state.ai.provider_label())
        } else {
            state.ai.provider_label().to_owned()
        };
        ui.weak(ai);
        separator(ui);
        // MCP:关闭时只写「关」,开启才展开端点(状态栏是窄条,不堆信息)
        match &state.mcp.status {
            crate::mcp::McpStatus::Listening(port) => {
                ui.weak(format!("MCP: 127.0.0.1:{port}"));
            }
            crate::mcp::McpStatus::Failed(_) => {
                ui.colored_label(crate::ui::tokens::WARN, "MCP: 启动失败");
            }
            crate::mcp::McpStatus::Starting => {
                ui.weak("MCP: 启动中");
            }
            crate::mcp::McpStatus::Disabled => {
                ui.weak("MCP: 关");
            }
        }
    });
}

fn separator(ui: &mut egui::Ui) {
    ui.weak("·");
}

/// 光标行列(1 起):行按换行数,列按该行字符数(中文按字计,与编辑器
/// 的视觉列一致)。
fn cursor_position(text: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(text.len());
    let before = &text[..byte];
    let line = before.matches('\n').count() + 1;
    let col = before.chars().rev().take_while(|ch| *ch != '\n').count() + 1;
    (line, col)
}

/// commit message 建议浮窗;返回(复制, 关闭)按钮的响应,测试定位用
/// (与 `ui::menubar::item` 同款手法)。
///
/// 复制即时写系统剪贴板(`Context::copy_text`,UI 侧效果,不改状态——
/// 不经用户动作覆盖剪贴板会冲掉用户正在搬运的内容);关闭只发消息,
/// 清建议的归约在 `App::logic`。
fn commit_dialog(ui: &mut egui::Ui, subject: &str) -> (egui::Response, egui::Response) {
    let ctx = ui.ctx().clone();
    let mut buttons = None;
    egui::Window::new("AI: commit message")
        // 固定初始位置:浮窗出现位置可预期(不与菜单栏重叠),拖动后由
        // Area 记忆保持;显式初始位也让无头测试的帧间位置稳定。
        .default_pos([80.0, 120.0])
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("建议的 commit subject:");
            ui.label(egui::RichText::new(subject).strong());
            ui.horizontal(|ui| {
                let copy = ui.button("复制");
                if copy.clicked() {
                    ctx.copy_text(subject.to_owned());
                }
                buttons = Some((copy, ui.button("关闭")));
            });
        });
    buttons.expect("浮窗必然绘制按钮")
}

/// 脏标签关闭确认浮窗;返回(确认关闭, 取消)按钮的响应,测试定位用
/// (与 `checkout_dialog` 同款手法;真正的移除在归约)。
fn tab_close_dialog(ui: &mut egui::Ui, name: &str) -> (egui::Response, egui::Response) {
    let mut buttons = None;
    egui::Window::new("关闭标签")
        .default_pos([80.0, 120.0])
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label(format!("「{name}」有未保存的修改。"));
            ui.label(
                egui::RichText::new("关闭将丢弃这些修改,此操作不可撤销。")
                    .strong()
                    .color(crate::ui::tokens::DANGER),
            );
            ui.horizontal(|ui| {
                let confirm = ui.button("关闭并丢弃");
                let cancel = ui.button("取消");
                buttons = Some((confirm, cancel));
            });
        });
    buttons.expect("浮窗必然绘制按钮")
}

/// 回滚确认浮窗;返回(回滚, 取消)按钮的响应,测试定位用(与
/// `commit_dialog` 同款手法)。只展示与收集点击,checkout 在归约。
/// `open_in_editor`/`dirty` 驱动当前文档的针对性警示
/// ([`checkout_extra_warning`]),dirty 状态取自缓冲真源、每帧重估。
fn checkout_dialog(
    ui: &mut egui::Ui,
    path: &str,
    open_in_editor: bool,
    dirty: bool,
) -> (egui::Response, egui::Response) {
    let mut buttons = None;
    egui::Window::new("Git: 回滚文件")
        .default_pos([80.0, 120.0])
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label(format!("把 {path} 恢复到 HEAD 版本。"));
            ui.label(
                egui::RichText::new("未提交的改动将被丢弃,此操作不可撤销。")
                    .strong()
                    .color(crate::ui::tokens::DANGER),
            );
            if let Some(warning) = checkout_extra_warning(open_in_editor, dirty) {
                let text = egui::RichText::new(warning);
                // dirty 分支有真实损失(保存会反转回滚),黄色升级警示;
                // 非 dirty 只是行为告知,走默认前景
                let text = if dirty {
                    text.color(egui::Color32::from_rgb(235, 180, 60))
                } else {
                    text
                };
                ui.label(text);
            }
            ui.horizontal(|ui| {
                let confirm = ui.button("回滚");
                let cancel = ui.button("取消");
                buttons = Some((confirm, cancel));
            });
        });
    buttons.expect("浮窗必然绘制按钮")
}

/// 回滚目标恰是编辑器当前文档时的追加警示;`None` = 目标不在编辑器中,
/// 只有常规不可逆警示。文案与归约侧行为(`State::after_git_checkout`)
/// 一一对应:dirty 保留未保存稿、非 dirty 重载为 HEAD。
fn checkout_extra_warning(open_in_editor: bool, dirty: bool) -> Option<&'static str> {
    if !open_in_editor {
        return None;
    }
    Some(if dirty {
        "该文件正在编辑器中打开:编辑器里未保存的修改会保留,之后保存(Ctrl+S)会把它们写回。"
    } else {
        "该文件正在编辑器中打开:回滚后编辑器将重载为 HEAD 版本。"
    })
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
    use egui::{
        Event, FullOutput, Key, Modifiers, OutputCommand, PointerButton, RawInput, Rect,
        ViewportCommand,
    };
    use std::cell::Cell;

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

        app.state.tabs.current_mut().editor.insert_chars(0, "改动");
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
        app.state.tabs.current_mut().document.path = Some(path.clone());
        app.state
            .tabs
            .current_mut()
            .editor
            .insert_chars(0, "# 落盘\r\n");

        let titles = reduce(&mut app, vec![key_s(Modifiers::COMMAND)]);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            app.state.tabs.current_mut().editor.text().as_bytes(),
            "保存字节原样,含 CRLF"
        );
        assert!(!app.state.tabs.current_mut().editor.is_dirty());
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
        app.state.tabs.current_mut().document.path = Some(path.clone());
        app.state.tabs.current_mut().editor.insert_chars(0, "正文");
        // 默认文档是 SAMPLE_MD 开头再插「正文」,基线取帧序列开始前的全文
        let baseline = app.state.tabs.current_mut().editor.text().to_owned();
        let ctx = egui::Context::default();

        // 帧 1:渲染全部面板(菜单栏/侧边栏/编辑器/预览),无输入
        let output = ctx.run_ui(RawInput::default(), |ui| {
            app.reduce(ui.ctx());
            app.draw(ui);
        });
        output.drop_without_applying_deltas();
        // 模拟用户点进编辑区:直接对编辑器 widget 请求焦点
        ctx.memory_mut(|mem| {
            mem.request_focus(crate::ui::editor::tab_editor_id(
                app.state.tabs.current().id,
            ))
        });

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
            app.state.tabs.current_mut().editor.text().as_bytes()
        );
        assert_eq!(
            app.state.tabs.current_mut().editor.text(),
            baseline,
            "命令键未被 TextEdit 当输入吞掉,也没有插入 's'"
        );
        assert!(
            ctx.memory(|mem| mem.has_focus(crate::ui::editor::tab_editor_id(
                app.state.tabs.current().id
            ))),
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
            app.state.tabs.current_mut().editor.text(),
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
        // 手工搭流式状态须同步发起标签绑定(`AiState::start` 在真实链路里
        // 负责):chunk 的写入目标由它决定
        app.state.ai_active_tab = Some(app.state.tabs.current().id);

        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();

        assert!(
            app.state
                .tabs
                .current_mut()
                .editor
                .text()
                .ends_with("第一块"),
            "chunk 已按序归约追加到文档末尾"
        );
        assert!(
            app.state.tabs.current_mut().editor.is_dirty(),
            "AI 写入置 dirty"
        );
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

    /// commit 建议浮窗:复制按钮把 subject 写进系统剪贴板输出命令;关闭按钮
    /// 经完整 draw → reduce 链路清掉建议(浮窗随之消失)。
    ///
    /// 合成点击分三帧(moved / press / release):Window 层按钮的点击归属
    /// 要求指针在按下帧之前已停在目标上(实测;panel 层的 menubar 测试无
    /// 此限制),三帧节奏也更接近真实输入。
    #[test]
    fn commit_dialog_copies_and_close_clears_suggestion() {
        let mut app = LaterMdApp::default();
        app.state.ai_commit_suggestion = Some("docs: 新增README.md".into());
        let ctx = egui::Context::default();
        let rects = Cell::new((Rect::NOTHING, Rect::NOTHING));

        // 帧 1:单独渲染浮窗拿按钮位置(Area 按 id 记忆,draw 内位置一致)
        ctx.run_ui(RawInput::default(), |ui| {
            let (copy, close) = commit_dialog(ui, "docs: 新增README.md");
            rects.set((copy.rect, close.rect));
        })
        .drop_without_applying_deltas();

        let (copy_center, close_center) = {
            let (copy, close) = rects.get();
            (copy.center(), close.center())
        };
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        // 帧 2-4:点「复制」→ 剪贴板输出命令携带 subject
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(copy_center)],
                ..Default::default()
            },
            |ui| {
                commit_dialog(ui, "docs: 新增README.md");
            },
        );
        output.drop_without_applying_deltas();
        let output = ctx.run_ui(
            RawInput {
                events: vec![click(copy_center, true)],
                ..Default::default()
            },
            |ui| {
                commit_dialog(ui, "docs: 新增README.md");
            },
        );
        output.drop_without_applying_deltas();
        let output = ctx.run_ui(
            RawInput {
                events: vec![click(copy_center, false)],
                ..Default::default()
            },
            |ui| {
                commit_dialog(ui, "docs: 新增README.md");
            },
        );
        assert!(
            output.platform_output.commands.iter().any(|cmd| matches!(
                cmd,
                OutputCommand::CopyText(text) if text.as_str() == "docs: 新增README.md"
            )),
            "复制按钮写剪贴板:{:?}",
            output.platform_output.commands
        );
        output.drop_without_applying_deltas();

        // 帧 5-7:完整 draw 下点「关闭」→ outbox 收到 AiCommitDismissed
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(close_center)],
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.drop_without_applying_deltas();
        let output = ctx.run_ui(
            RawInput {
                events: vec![click(close_center, true)],
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.drop_without_applying_deltas();
        let output = ctx.run_ui(
            RawInput {
                events: vec![click(close_center, false)],
                ..Default::default()
            },
            |ui| app.draw(ui),
        );
        output.drop_without_applying_deltas();
        assert!(
            app.outbox
                .iter()
                .any(|m| matches!(m, Message::AiCommitDismissed)),
            "{:?}",
            app.outbox
        );

        // 帧 8:归约清建议
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();
        assert_eq!(app.state.ai_commit_suggestion, None);
    }

    /// 在临时目录里装配一次性 git 仓库(一笔提交 + 一个工作区改动)。
    fn dirty_repo(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("latermd-layout-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(["-c", "user.name=LaterMD", "-c", "user.email=latermd@test"])
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} 失败");
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.md"), "HEAD 版本\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("a.md"), "乱改\n").unwrap();
        dir
    }

    /// Git 轮询在归约侧到点即刷:有根 + due 过期的一帧 reduce 后改动列表
    /// 就位;无根时到点也不刷(不空转)。
    #[test]
    fn git_poll_refreshes_when_due_with_root() {
        let dir = dirty_repo("git-poll");
        let mut app = LaterMdApp::default();
        app.state.file_tree.root = Some(dir.clone());
        app.state.git.refresh_due = std::time::Instant::now() - std::time::Duration::from_millis(1);

        let ctx = egui::Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| app.reduce(ui.ctx()));
        output.drop_without_applying_deltas();
        assert_eq!(app.state.git.entries.len(), 1, "到点帧已刷新");
        assert!(!app.state.git.due(), "刷新顺延了下一周期");

        // 无根 + 到点:不刷(无根不轮询)
        let mut rootless = LaterMdApp::default();
        rootless.state.git.refresh_due =
            std::time::Instant::now() - std::time::Duration::from_millis(1);
        let output = ctx.run_ui(RawInput::default(), |ui| rootless.reduce(ui.ctx()));
        output.drop_without_applying_deltas();
        assert_eq!(rootless.state.git.entries.len(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 回滚确认浮窗:「回滚」与「取消」按钮点击分别发 GitCheckoutConfirmed /
    /// GitCheckoutCancelled 进 outbox,由下一帧归约执行(浮窗本身不做 git)。
    #[test]
    fn checkout_dialog_buttons_send_messages() {
        let mut app = LaterMdApp::default();
        app.state.git.confirm_checkout = Some("a.md".to_owned());
        let ctx = egui::Context::default();
        let rects = Cell::new((Rect::NOTHING, Rect::NOTHING));

        // 帧 1:渲染浮窗拿按钮位置(Area 按 id 记忆,draw 内位置一致)
        ctx.run_ui(RawInput::default(), |ui| {
            let (confirm, cancel) = checkout_dialog(ui, "a.md", false, false);
            rects.set((confirm.rect, cancel.rect));
        })
        .drop_without_applying_deltas();

        let (confirm_center, cancel_center) = {
            let (confirm, cancel) = rects.get();
            (confirm.center(), cancel.center())
        };
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        // 帧 2-4:完整 draw 下点「取消」→ GitCheckoutCancelled(消息的
        // 产出在 draw 的浮窗接线里,单独渲染浮窗不够)
        for events in [
            vec![Event::PointerMoved(cancel_center)],
            vec![click(cancel_center, true)],
            vec![click(cancel_center, false)],
        ] {
            let output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.draw(ui),
            );
            output.drop_without_applying_deltas();
        }
        assert!(
            app.outbox
                .iter()
                .any(|m| matches!(m, Message::GitCheckoutCancelled)),
            "{:?}",
            app.outbox
        );
        app.outbox.clear();
        // 帧 5-7:完整 draw 下点「回滚」→ GitCheckoutConfirmed(消息同样
        // 产自 draw;真正的 checkout 在下一帧归约,state::tests 已覆盖)
        for events in [
            vec![Event::PointerMoved(confirm_center)],
            vec![click(confirm_center, true)],
            vec![click(confirm_center, false)],
        ] {
            let output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.draw(ui),
            );
            output.drop_without_applying_deltas();
        }
        assert!(
            app.outbox
                .iter()
                .any(|m| matches!(m, Message::GitCheckoutConfirmed)),
            "{:?}",
            app.outbox
        );
    }

    /// 设置对话框的接线:`settings.open` 时完整 `draw` 渲染不 panic,四个
    /// 分页各渲一帧;关闭后不再进入绘制路径(渲染全程不触碰凭据后端——
    /// 读写只在归约)。
    #[test]
    fn draw_renders_settings_dialog_on_every_tab() {
        let mut app = LaterMdApp::default();
        app.state.settings.open = true;
        let ctx = egui::Context::default();
        for tab in crate::settings::SettingsTab::ALL {
            app.state.settings.tab = tab;
            let output = ctx.run_ui(RawInput::default(), |ui| app.draw(ui));
            output.drop_without_applying_deltas();
        }
        assert!(app.state.settings.open, "渲染不翻转开关");

        app.state.settings.open = false;
        let output = ctx.run_ui(RawInput::default(), |ui| app.draw(ui));
        output.drop_without_applying_deltas();
    }

    /// 快捷键捕获:设置页点「改键」后,本帧按键成为新键位并落 keymap;
    /// 捕获期间该键**不**触发命令(否则 Ctrl+K 会顺手触发一次别的命令)。
    #[test]
    fn capture_assigns_shortcut_without_firing_command() {
        let mut app = LaterMdApp::default();
        app.state.settings.capture = Some(crate::command::Command::Save);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::K,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ui| app.reduce(ui.ctx()),
        );
        output.drop_without_applying_deltas();
        assert_eq!(app.state.settings.capture, None, "捕获即结束");
        let bound = app.state.keymap.get(crate::command::Command::Save).unwrap();
        assert_eq!(bound.key, Key::K);
        assert_eq!(bound.modifiers, Modifiers::COMMAND);
    }

    /// 撞键被拒:把「保存」改成与「打开」相同的键位,绑定不变并落提示。
    #[test]
    fn capture_rejects_conflicting_shortcut() {
        let mut app = LaterMdApp::default();
        app.state.settings.capture = Some(crate::command::Command::Save);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(
            RawInput {
                events: vec![Event::Key {
                    key: Key::O,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
                ..Default::default()
            },
            |ui| app.reduce(ui.ctx()),
        );
        output.drop_without_applying_deltas();
        assert_eq!(
            app.state.keymap.get(crate::command::Command::Save),
            crate::keymap::Keymap::builtin().get(crate::command::Command::Save),
            "撞键被拒:保存仍是原键位"
        );
        assert!(
            app.state
                .tabs
                .current()
                .document
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("占用")),
            "提示指出被谁占用:{:?}",
            app.state.tabs.current().document.notice
        );
    }

    /// 针对性警示只看「目标是否当前文档」:不在编辑器中无追加;在则按
    /// dirty 分流(保留稿子会写回 / 重载 HEAD),与归约侧行为一一对应。
    #[test]
    fn checkout_extra_warning_targets_current_document_only() {
        assert_eq!(checkout_extra_warning(false, false), None);
        assert_eq!(checkout_extra_warning(false, true), None);

        let clean = checkout_extra_warning(true, false).unwrap();
        assert!(clean.contains("重载"), "{clean}");
        assert!(!clean.contains("写回"));

        let dirty = checkout_extra_warning(true, true).unwrap();
        assert!(dirty.contains("写回"), "{dirty}");
        assert!(dirty.contains("未保存"), "{dirty}");
    }

    /// 三种警示形态的浮窗整体渲染都不 panic(按钮定位回归由上一个测试
    /// 的 false/false 形态覆盖)。
    #[test]
    fn checkout_dialog_renders_all_warning_variants() {
        let ctx = egui::Context::default();
        for (open, dirty) in [(false, false), (true, false), (true, true)] {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                checkout_dialog(ui, "docs/a.md", open, dirty);
            });
            output.drop_without_applying_deltas();
        }
    }
}
