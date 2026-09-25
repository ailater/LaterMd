//! 应用状态与消息骨架(docs/adr-005 §5)。
//!
//! 归约铁律:状态变更只发生在 `App::logic` 调用的 [`State::apply`];
//! `App::ui` 只读状态、只产出 [`Message`]。本文件目前落了侧边栏、文件
//! 操作与大纲三组条目,`file_tree` / `search` 等字段随对应模块接入时
//! 增量加入,完整规划见 docs/adr-005 §5.1。
//!
//! 例外:编辑器缓冲、预览快照(含大纲)与 [`OutlineCursor`] 由 `ui::editor`
//! 原地维护 —— `TextEdit` 是立即模式控件,必须拿到 `&mut` 缓冲才能绘制
//! (AGENTS.md §8「UI 与状态机天然耦合」),快照与光标又是它的派生缓存,
//! 归约进下一帧反而让预览滞后一帧。
//!
//! AI 流式追加的 dirty 与 undo 语义(P1):[`Message::AiChunk`] 走
//! `EditorBuffer::insert_chars` —— dirty 照常置位(AI 写入是真实内容,
//! 保存前与手敲同责),修订号照常推进(预览每 chunk 重建一次快照)。
//! undo 栈是 TextEdit 内建 undoer 的快照,看不到程序化插入:流式结束后的
//! 第一次 Ctrl+Z 会整体回退到最近一次用户编辑的快照(表现为「一步撤销
//! 整段 AI 续写」,redo 可恢复),用户在流式期间的手敲一并被归入同一步。
//! 在途流随文档切换一并作废([`State::load_document`] 里收尾):chunk 只
//! 认「当前缓冲末尾」,缓冲一换剩余块就会写错文件。

use crate::ai::AiState;
use crate::export;
use crate::file::{self, FileCmd};
use crate::filetree::{FileTreeSettings, FileTreeState};
use crate::search::SearchState;
use crate::theme::{ThemeMode, ThemeSettings};
use latermd_editor::EditorBuffer;
use latermd_md::OutlineItem;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 搜索输入去抖间隔(roadmap 阶段 3 的既定值)。
const DEBOUNCE: Duration = Duration::from_millis(300);

/// 组装 AI 续写 prompt 时的文档尾部上限(字符):MockProvider 只按关键词
/// 选脚本,真实 provider 的上下文窗口截断是 app 层的职责(latermd-ai
/// trait 契约),先按 2k 字符封顶。
const AI_PROMPT_TAIL_CHARS: usize = 2000;

/// 侧边栏功能页签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    /// 文件树(P0 基础版)。
    Files,
    /// 全文搜索(P1)。
    Search,
    /// 文档大纲(P0 廉价版:点击跳编辑器光标)。
    Outline,
}

impl SidebarTab {
    /// 页签栏顺序。
    pub const ALL: [SidebarTab; 3] = [Self::Files, Self::Search, Self::Outline];

    /// 页签栏显示名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "文件",
            Self::Search => "搜索",
            Self::Outline => "大纲",
        }
    }
}

/// 侧边栏状态。`visible` 直接喂给 `Panel::show_collapsible` 的 `&mut bool`:
/// 面板把手在 `ui` 里原地翻转;命令层(菜单/快捷键)的切换走
/// [`Message::SidebarToggled`] 在 `logic` 归约。
pub struct SidebarState {
    /// 是否展开。
    pub visible: bool,
    /// 当前页签。
    pub active_tab: SidebarTab,
}

/// 文档派生视图快照:预览文本 + 大纲,与编辑器修订号绑定。
///
/// 只在编辑器修订号前进(或整篇换入)时重建,与预览同步是同一时机;
/// vendored 层内部还会按 text hash 二次缓存,空闲帧零开销。
pub struct PreviewState {
    /// 当前喂给 [`egui_markdown::MarkdownLabel`] 的全文。
    pub text: String,
    /// 快照对应的 [`EditorBuffer::revision`]。
    pub synced_rev: u64,
    /// 文档大纲,与 `text` 同一次重建产出,`span` 直接索引该文本。
    pub outline: Vec<OutlineItem>,
}

impl PreviewState {
    /// 以编辑器当前内容建立快照(文本 + 大纲)。
    pub fn new(editor: &EditorBuffer) -> Self {
        Self {
            text: editor.text().to_owned(),
            synced_rev: editor.revision(),
            outline: latermd_md::outline(editor.text()),
        }
    }

    /// 修订号前进后重建快照;空闲帧不得调用(会白白重解析全文)。
    pub fn rebuild(&mut self, editor: &EditorBuffer) {
        *self = Self::new(editor);
    }
}

/// 大纲面板与编辑器之间的光标协调,只存偏移、不含 egui 类型。
///
/// `ui` 产出 [`Message::OutlineItemClicked`]、`logic` 归约成 `jump_to`,
/// `ui::editor` 消费时改写 TextEdit 持久光标并交还焦点;此后每帧把实际
/// 光标位置回填到 `byte`,供大纲「当前小节」高亮。
#[derive(Default)]
pub struct OutlineCursor {
    /// 待应用的跳转目标(字符偏移,与 `CCursor.index` 同语义),消费即清空。
    pub jump_to: Option<usize>,
    /// 编辑器当前光标字节位置(上一帧值);`None` = 尚无光标信息。
    pub byte: Option<usize>,
}

/// 应用根状态。
pub struct State {
    /// 侧边栏。
    pub sidebar: SidebarState,
    /// 编辑器缓冲(唯一正文真源)。
    pub editor: EditorBuffer,
    /// 预览快照(含大纲)。
    pub preview: PreviewState,
    /// 大纲↔编辑器光标协调。
    pub cursor: OutlineCursor,
    /// 文件树(Files 页签):根目录、最近列表与懒加载缓存。
    pub file_tree: FileTreeState,
    /// 全文搜索(Search 页签):输入去抖、后台服务与结果缓存。
    pub search: SearchState,
    /// AI 流式(MockProvider,P1 联调):provider + 接收端 + 防重入标志。
    pub ai: AiState,
    /// 当前文档的落盘身份。
    pub document: DocumentState,
    /// 主题(外壳 visuals 与 MarkdownStyle 的唯一事实源);每帧由 `logic`
    /// 投影到 context,切换即时生效。
    pub theme: ThemeSettings,
    /// 主题落盘目录;`None` = 平台默认。仅为测试注入临时目录而存在,
    /// 生产恒为 `None`。
    pub(crate) settings_dir: Option<PathBuf>,
}

/// 文档落盘身份 + 未保存镜像。
///
/// `dirty` 是 [`EditorBuffer::is_dirty`] 的镜像而非第二个真源:任何编辑路径
/// (按键、IME、undo/redo)都必然先落进缓冲,因此只有缓冲自己的标志可靠;
/// 这里由 [`State::end_of_logic`] 每帧单向刷新,勿手工置位。
pub struct DocumentState {
    /// 当前文档路径;`None` = 新建后尚未保存过。
    pub path: Option<PathBuf>,
    /// 是否有未保存修改(窗口标题与工具栏的 `*` 由它驱动)。
    pub dirty: bool,
    /// 最近一次文件操作的失败提示;下一次成功操作或用户点掉时清空。
    pub notice: Option<String>,
}

impl DocumentState {
    /// 未落盘文档的显示名;另存为对话框预填名见 [`file::UNTITLED_FILE_NAME`]。
    const UNTITLED: &str = "未命名";

    /// 文件名显示(未落盘为「未命名」),dirty 追加 `*`。
    pub fn display_name(&self) -> String {
        let name = self
            .path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| Self::UNTITLED.to_owned());
        if self.dirty {
            format!("{name}*")
        } else {
            name
        }
    }

    /// 窗口标题。
    pub fn window_title(&self) -> String {
        format!("LaterMD — {}", self.display_name())
    }
}

/// 初始文档:中英混排 + 标题/列表/表格/代码块,首跑即可肉眼核对预览。
const SAMPLE_MD: &str = r#"# LaterMD

欢迎!This is a live preview. 左侧编辑源码,右侧实时同步。

## 常用元素

- 列表 item
- [ ] 任务 task

**粗体**、*斜体*、`inline code` 与 [链接](https://github.com/ailater/LaterMd)。

```rust
fn main() {
    println!("你好, LaterMD!");
}
```

| 列甲 | 列乙 |
|---|---|
| 1 | 2 |
"#;

impl Default for State {
    fn default() -> Self {
        let editor = EditorBuffer::new(SAMPLE_MD);
        Self {
            sidebar: SidebarState {
                visible: true,
                active_tab: SidebarTab::Files,
            },
            preview: PreviewState::new(&editor),
            cursor: OutlineCursor::default(),
            file_tree: FileTreeState::default(),
            search: SearchState::default(),
            ai: AiState::default(),
            editor,
            document: DocumentState {
                path: None,
                dirty: false,
                notice: None,
            },
            theme: ThemeSettings::default(),
            settings_dir: None,
        }
    }
}

/// UI 事件消息:`ui` 产出、`logic` 消费(docs/adr-005 §5.1/§5.2)。
///
/// 后续变体(`SearchQueryChanged` …)随搜索模块接入加入;后台任务的结果
/// 回传也走同一入口。
// 不再整体 Copy:`OutlineItemClicked` 携带 `Range<usize>`(Clone 但非 Copy)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// 切换侧边栏页签。
    SidebarTabChanged(SidebarTab),
    /// 请求执行文件命令(对话框与 IO 在归约中发生)。
    FileCommand(FileCmd),
    /// 关闭提示行。
    NoticeDismissed,
    /// 导出当前文档为 HTML(弹保存对话框,不触碰文档落盘身份)。
    ExportHtml,
    /// 切换明暗主题(设置菜单产出);归约里改状态并即时落盘。
    ThemeChanged(ThemeMode),
    /// 明暗主题互换(命令层「切换主题」的快捷键/菜单入口;定向选择走
    /// [`Message::ThemeChanged`])。
    ToggleTheme,
    /// 切换侧边栏展开/折叠(命令层入口;面板把手翻转不走消息)。
    SidebarToggled,
    /// 请求为文件树选择新根目录(归约里弹目录对话框)。
    FileTreeRootPick,
    /// 把文件树根目录切到最近列表中的某一项(不经对话框)。
    FileTreeRootSelected(PathBuf),
    /// 点击文件树目录行,载荷为目录路径。
    FileTreeToggled(PathBuf),
    /// 点击文件树文件行,载荷为文件路径。
    FileSelected(PathBuf),
    /// 搜索输入变化(文本/大小写开关由 `ui` 原地写入 `SearchState`,消息
    /// 本身无载荷):归约里取消旧搜索并顺延去抖。
    SearchQueryChanged,
    /// 去抖到点,按当前输入与根目录发起搜索。
    SearchRequested,
    /// 点击搜索结果,载荷为(文件路径, 1 起行号):归约里打开该文件并把
    /// 光标跳到行首。
    SearchResultClicked(PathBuf, usize),
    /// 点击大纲条目,载荷为标题的源码字节区间。
    OutlineItemClicked(Range<usize>),
    /// 发起 AI Mock 流式续写(命令层入口);流式进行中在归约里被忽略
    /// (防重入,见 [`AiState::start`])。
    AiStart,
    /// AI 流式的一个增量块,载荷为要追加到文档末尾的原文。后台线程产出,
    /// 每帧由归约侧从 channel 收流翻成本消息(见 `State::poll_ai`)。
    AiChunk { delta: String },
    /// AI 流式成功收尾。
    AiDone,
    /// AI 流式失败,载荷为面向用户的错误描述(provider 契约:done 且
    /// delta 非空)。失败文本不写入文档。
    AiFailed(String),
    /// 点击 ai:// 链接,载荷为 [`crate::ai_link::parse`] 的结果:`Ok` 为解码
    /// 后的提示词,归约走与 [`Message::AiStart`] 同一条流式启动路径(防重入
    /// 同样生效);`Err` 为未实现动作 / 解析失败的提示语,落状态栏不执行。
    AiLinkClicked { prompt: Result<String, String> },
}

impl State {
    /// 消费一条消息,变更状态。只允许在 `App::logic` 调用。
    pub fn apply(&mut self, message: Message) {
        match message {
            Message::SidebarTabChanged(tab) => self.sidebar.active_tab = tab,
            Message::FileCommand(cmd) => self.run_file_cmd(cmd),
            Message::NoticeDismissed => self.document.notice = None,
            Message::ExportHtml => self.run_export_html(),
            Message::ThemeChanged(mode) => self.change_theme(mode),
            Message::ToggleTheme => self.change_theme(self.theme.mode.opposite()),
            Message::SidebarToggled => self.sidebar.visible = !self.sidebar.visible,
            Message::FileTreeRootPick => self.pick_file_tree_root(),
            Message::FileTreeRootSelected(dir) => self.change_file_tree_root(dir),
            Message::FileTreeToggled(dir) => self.file_tree.toggle(&dir),
            Message::FileSelected(path) => self.open_from_file_tree(&path),
            Message::SearchQueryChanged => self.search.input_changed(DEBOUNCE),
            Message::SearchRequested => self.start_search(),
            Message::SearchResultClicked(path, line_no) => {
                self.open_search_hit(&path, line_no);
            }
            Message::OutlineItemClicked(span) => self.jump_cursor_to_heading(span),
            Message::AiStart => self.start_ai_stream(),
            Message::AiChunk { delta } => self.append_ai_delta(&delta),
            Message::AiDone => self.ai.finish(),
            Message::AiFailed(error) => {
                self.ai.finish();
                // 失败不算完成:指令卡状态随 last_prompt 清空回到未执行
                self.ai.forget_last_prompt();
                self.document.notice = Some(error);
            }
            Message::AiLinkClicked { prompt } => match prompt {
                Ok(prompt) => self.start_ai_stream_with_prompt(&prompt),
                Err(reason) => self.document.notice = Some(reason),
            },
        }
    }

    /// 发起 AI Mock 流式续写(`Message::AiStart` 的归约)。流式进行中
    /// 忽略(防重入);成功发起前把文档收成「以空行结尾」,让续写从新段落
    /// 开始 —— 直接拼在末行会与 AI 首行粘连成一行。
    fn start_ai_stream(&mut self) {
        if self.ai.is_streaming() {
            return;
        }
        self.ensure_trailing_blank_line();
        // prompt 透传文档尾部,provider 按原样消费(latermd-ai trait 契约:
        // 截断与拼装是调用方的职责)
        let text = self.editor.text();
        let skip = text.chars().count().saturating_sub(AI_PROMPT_TAIL_CHARS);
        let prompt = format!(
            "请续写以下文档内容:\n{}",
            text.chars().skip(skip).collect::<String>()
        );
        self.start_ai_stream_with_prompt(&prompt);
    }

    /// 流式启动的共用入口,两个来源:菜单命令(带文档尾部的 prompt)与
    /// ai:// 链接点击(链接里的提示词,原样透传)。防重入在此把关,流式
    /// 进行中连「补空行」都不发生。
    fn start_ai_stream_with_prompt(&mut self, prompt: &str) {
        if self.ai.is_streaming() {
            return;
        }
        self.ensure_trailing_blank_line();
        self.ai.start(prompt);
    }

    /// 文档非空且不以空行结尾时,补成恰好一个空行(空文档/已空行结尾不动)。
    fn ensure_trailing_blank_line(&mut self) {
        let text = self.editor.text();
        let pad = if text.is_empty() || text.ends_with("\n\n") {
            0
        } else if text.ends_with('\n') {
            1
        } else {
            2
        };
        if pad > 0 {
            self.editor
                .insert_chars(self.editor.len_chars(), &"\n".repeat(pad));
        }
    }

    /// 追加 AI 增量块到文档末尾(`Message::AiChunk` 的归约)。走
    /// `insert_chars` 的增量 splice 双写,dirty 与修订号照常推进 —— AI 写
    /// 进来的是真实内容,保存前与手敲同责;undo 语义见模块文档。
    fn append_ai_delta(&mut self, delta: &str) {
        self.editor.insert_chars(self.editor.len_chars(), delta);
    }

    /// 收流(每帧归约调用一次):把 AI channel 里积压的 chunk 翻成消息,
    /// 由调用方(`ui::layout::reduce`)并入本帧的归约队列。
    pub fn poll_ai(&mut self) -> Vec<Message> {
        self.ai.poll()
    }

    /// 切换主题(设置菜单的归约):改状态并即时落盘(重启保持);投影到
    /// context 由每帧的 `theme.apply` 完成。落盘失败只落提示行,切换本身
    /// 照常生效 —— 持久化失败不该牺牲本次会话的可用性。
    fn change_theme(&mut self, mode: ThemeMode) {
        self.theme.mode = mode;
        if let Err(error) = self.theme.save_to(self.settings_dir.as_deref()) {
            self.document.notice = Some(error.to_string());
        }
    }

    /// 把编辑器光标跳到标题行首(大纲点击的归约)。
    ///
    /// span 平铺不变量使标题 span 可能吸收前一块尾部的换行(实测 `## X` 的
    /// span 起于其前的空行),跳过换行让光标落在标题行首。区间来自点击帧
    /// 的快照,同帧编辑器面板仍可能改动文本,归约时缓冲或已变短:切片前
    /// 按当前长度钳制,`byte_to_char` 把落在字符中间的偏移归到起点。
    fn jump_cursor_to_heading(&mut self, span: Range<usize>) {
        let mut byte = span.start.min(self.editor.text().len());
        for b in &self.editor.text().as_bytes()[byte..] {
            match b {
                b'\n' | b'\r' => byte += 1,
                _ => break,
            }
        }
        self.cursor.jump_to = Some(self.editor.byte_to_char(byte));
    }

    /// 帧末刷新派生状态。`App::logic` 每帧调用一次。
    pub fn end_of_logic(&mut self) {
        self.document.dirty = self.editor.is_dirty();
        // 文件树懒加载落点:根 + 展开中目录的子项缓存补齐(键缺席才 IO)。
        self.file_tree.ensure_loaded();
        // 搜索结果收流:非阻塞收空 channel(重绘驱动见 `ui::layout::reduce`)。
        self.search.poll_hits();
    }

    fn run_file_cmd(&mut self, cmd: FileCmd) {
        match cmd {
            FileCmd::New => {
                if self.unsaved_guard() {
                    return;
                }
                self.load_document(None, "");
            }
            FileCmd::Open => {
                if self.unsaved_guard() {
                    return;
                }
                let start = file::start_dir(self.document.path.as_deref());
                if let Some(path) = file::open_dialog(&start) {
                    self.open_from(&path);
                }
            }
            FileCmd::Save => {
                let target = match self.document.path.clone() {
                    Some(path) => Some(path),
                    None => file::save_dialog(&file::start_dir(None), file::UNTITLED_FILE_NAME),
                };
                if let Some(path) = target {
                    self.save_to(path);
                }
            }
            FileCmd::SaveAs => {
                let start = file::start_dir(self.document.path.as_deref());
                let default = self
                    .document
                    .path
                    .as_deref()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file::UNTITLED_FILE_NAME.to_owned());
                if let Some(path) = file::save_dialog(&start, &default) {
                    self.save_to(path);
                }
            }
        }
    }

    /// dirty 时拦住会覆盖缓冲的命令(新建/打开)。模态确认属于后续的
    /// 「未保存关闭」模块,这里先用提示行挡住静默丢稿。
    fn unsaved_guard(&mut self) -> bool {
        if self.editor.is_dirty() {
            self.document.notice =
                Some("有未保存修改:请先保存(Ctrl+S)或另存为(Ctrl+Shift+S)".to_owned());
            true
        } else {
            false
        }
    }

    /// 读盘并换入缓冲。读取失败只落提示行,不动当前文档。
    ///
    /// 同时展开该文件在树内的祖先目录:无论从文件树、菜单还是快捷键打开,
    /// 当前文件的高亮行都应当在 Files 页里可见。
    fn open_from(&mut self, path: &Path) {
        match file::read(path) {
            Ok(text) => {
                self.load_document(Some(path.to_path_buf()), &text);
                self.file_tree.expand_ancestors_of(path);
            }
            Err(error) => self.document.notice = Some(error.to_string()),
        }
    }

    /// 文件树点击文件:与「打开」命令同一语义(dirty 拦截 + 换入缓冲)。
    fn open_from_file_tree(&mut self, path: &Path) {
        if self.unsaved_guard() {
            return;
        }
        self.open_from(path);
    }

    /// 去抖到点发起搜索(`Message::SearchRequested` 的归约)。无根目录
    /// 不发起(该情形下 `ui` 层就不该发消息,这里防御性短路)。
    fn start_search(&mut self) {
        if let Some(root) = self.file_tree.root.clone() {
            self.search.start(&root);
        } else {
            self.search.reset();
        }
    }

    /// 点击搜索结果(`Message::SearchResultClicked` 的归约):文件与当前
    /// 不同则先打开(读盘失败只落提示行,不跳转),再把光标跳到命中行
    /// 行首——经 [`OutlineCursor::jump_to`] 由 `ui::editor` 覆写持久光标
    /// 并交还焦点。
    ///
    /// 行号来自点击时刻的搜索快照,打开后文件可能比搜索时短(外部修改、
    /// 或命中的本就是尚未落盘的另一版本):1 起行号先转 0 起,再由
    /// [`EditorBuffer::line_to_byte`] 按当前缓冲钳制到文末,绝不 panic
    /// (大纲跳转 span 过期的同款教训)。
    fn open_search_hit(&mut self, path: &Path, line_no: usize) {
        if self.document.path.as_deref() != Some(path) {
            if self.unsaved_guard() {
                return;
            }
            self.open_from(path);
            if self.document.path.as_deref() != Some(path) {
                return; // 读盘失败:notice 已带原因,不跳
            }
        }
        let byte = self.editor.line_to_byte(line_no.saturating_sub(1));
        self.cursor.jump_to = Some(self.editor.byte_to_char(byte));
    }

    /// 弹目录对话框选文件树根目录(Files 页「选择…」按钮的归约)。起始目录
    /// 取最近根或当前文档所在目录,均已校验存在(rfd 对不存在目录的行为未定义)。
    fn pick_file_tree_root(&mut self) {
        let start = self
            .file_tree
            .recents
            .first()
            .cloned()
            .or_else(|| {
                self.document
                    .path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
            })
            .filter(|dir| dir.is_dir())
            .unwrap_or_else(|| file::start_dir(None));
        if let Some(dir) = file::pick_folder_dialog(&start) {
            self.change_file_tree_root(dir);
        }
    }

    /// 换根并持久化(对话框与最近列表两个入口共用);落盘失败只落提示行,
    /// 本次会话的文件树照常可用。搜索一并复位:旧根的结果在新根下相对
    /// 路径失真,留着只会误导。
    fn change_file_tree_root(&mut self, dir: PathBuf) {
        self.file_tree.set_root(dir);
        self.search.reset();
        if let Err(error) =
            FileTreeSettings::from(&self.file_tree).save_to(self.settings_dir.as_deref())
        {
            self.document.notice = Some(error.to_string());
        }
    }

    /// 换入整篇内容并复位文档身份(新建/打开共用)。
    ///
    /// 在途 AI 流随缓冲一起作废:chunk 归约无条件追加到「当前缓冲末尾」,
    /// 缓冲一换,剩余块就会写进新文档(Save 清 dirty 放行的 New/Open 同样
    /// 命中)。接收端 drop 后 worker 下一次 send 失败即退出(latermd-ai 的
    /// 取消约定),重绘驱动随标志清零自然停;与搜索换根复位同款静默。
    fn load_document(&mut self, path: Option<PathBuf>, text: &str) {
        self.ai.finish();
        // 指令卡是文档的派生物:新文档的 ```ai 块与新流无关,状态一并复位
        self.ai.forget_last_prompt();
        self.editor.load(text);
        self.document.path = path;
        self.document.notice = None;
        // 预览快照就地重建,不等下一帧编辑面板的修订号检查
        self.preview.rebuild(&self.editor);
    }

    /// 写盘成功后复位 dirty 并认领新路径;失败只落提示行。
    fn save_to(&mut self, path: PathBuf) {
        match file::write(&path, self.editor.text()) {
            Ok(()) => {
                self.editor.clear_dirty();
                self.document.path = Some(path);
                self.document.notice = None;
            }
            Err(error) => self.document.notice = Some(error.to_string()),
        }
    }

    /// 导出 HTML(消息归约):弹保存对话框,把当前缓冲渲染成完整 HTML 落盘。
    /// 导出物是派生物:文档路径与 dirty 均不动。
    fn run_export_html(&mut self) {
        let start = file::start_dir(self.document.path.as_deref());
        let default = export::default_name(self.document.path.as_deref());
        if let Some(path) = export::save_dialog(&start, &default) {
            self.export_html_to(&path);
        }
    }

    /// 渲染并写出;失败只落提示行。绕开对话框直测落盘路径,单独成函数供测试。
    fn export_html_to(&mut self, path: &Path) {
        let html = latermd_export::export_html(self.editor.text());
        match file::write_as("导出", path, &html) {
            Ok(()) => self.document.notice = None,
            Err(error) => self.document.notice = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("latermd-state-{}-{name}", std::process::id()))
    }

    #[test]
    fn display_name_and_window_title_track_dirty() {
        let mut document = DocumentState {
            path: Some(PathBuf::from("/docs/LaterMD 指南.md")),
            dirty: false,
            notice: None,
        };
        assert_eq!(document.window_title(), "LaterMD — LaterMD 指南.md");
        document.dirty = true;
        assert_eq!(document.window_title(), "LaterMD — LaterMD 指南.md*");

        document = DocumentState {
            path: None,
            dirty: true,
            notice: None,
        };
        assert_eq!(document.display_name(), "未命名*");
    }

    /// 命令层开关消息:主题互换(翻转 + 落盘)与侧边栏翻转,都走完整归约。
    #[test]
    fn toggle_messages_flip_theme_and_sidebar() {
        let dir = temp_path("toggle-dir");
        let mut state = State {
            settings_dir: Some(dir.clone()),
            ..State::default()
        };
        let visible_before = state.sidebar.visible;

        state.apply(Message::ToggleTheme);
        assert_eq!(state.theme.mode, ThemeMode::Light, "默认深色 → 浅色");
        assert!(dir.join("settings.json").exists(), "互换同样持久化");
        state.apply(Message::ToggleTheme);
        assert_eq!(state.theme.mode, ThemeMode::Dark, "再切回深色");

        state.apply(Message::SidebarToggled);
        assert_eq!(state.sidebar.visible, !visible_before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 打开:内容进缓冲、dirty 复位、路径认领、预览快照同帧联动。
    #[test]
    fn open_loads_buffer_and_syncs_preview() {
        let path = temp_path("open.md");
        std::fs::write(&path, "# 磁盘标题\r\nCRLF 行").unwrap();

        let mut state = State::default();
        state.editor.insert_chars(0, "草稿"); // 制造未保存状态
        state.apply(Message::FileCommand(FileCmd::New)); // 被 unsaved_guard 拦下
        assert!(state.editor.text().starts_with("草稿"), "新建未清掉草稿");
        assert!(state.document.notice.is_some());

        state.editor.clear_dirty();
        state.open_from(&path);
        assert_eq!(
            state.editor.text(),
            "# 磁盘标题\r\nCRLF 行",
            "CRLF 原样进缓冲"
        );
        assert_eq!(state.document.path.as_deref(), Some(path.as_path()));
        assert!(!state.document.dirty);
        assert_eq!(state.preview.text, state.editor.text(), "预览快照已联动");
        assert_eq!(state.preview.synced_rev, state.editor.revision());
        assert_eq!(state.preview.outline.len(), 1);
        assert_eq!(state.preview.outline[0].text, "磁盘标题");
        let _ = std::fs::remove_file(&path);
    }

    /// 初始文档的大纲:示例文档的两个标题,span 索引快照文本。
    #[test]
    fn default_state_outline_matches_sample() {
        let state = State::default();
        let outline = &state.preview.outline;
        let levels: Vec<u8> = outline.iter().map(|item| item.level).collect();
        assert_eq!(levels, vec![1, 2]);
        assert_eq!(outline[0].text, "LaterMD");
        assert_eq!(outline[1].text, "常用元素");
        assert!(state.preview.text[outline[1].span.clone()].contains("## 常用元素"));
    }

    /// 大纲点击归约:跳过 span 吸收的前置换行落到标题行首,并按当前缓冲
    /// 把字节偏移换成字符偏移(示例文档在目标前有 CJK,两者必然不同)。
    #[test]
    fn outline_click_converts_to_char_offset_on_heading_line() {
        let mut state = State::default();
        let span = state.preview.outline[1].span.clone();
        state.apply(Message::OutlineItemClicked(span));

        let heading_byte = state.editor.text().find("## 常用元素").unwrap();
        let jump = state.cursor.jump_to.expect("已设置跳转目标");
        assert_eq!(jump, state.editor.byte_to_char(heading_byte));
        assert!(jump < heading_byte, "目标前有 CJK,字符偏移必须小于字节偏移");
        let bytes = state.editor.text().as_bytes();
        assert_ne!(
            bytes[state.editor.char_to_byte(jump)],
            b'\n',
            "落在标题行首"
        );
    }

    /// 大纲点击归约的过期 span:消息产自上一帧快照,同帧编辑可能已把缓冲
    /// 删短,越界 start 不得 panic,钳制后跳到当前文档末尾。
    #[test]
    fn outline_click_with_stale_span_clamps_to_text_end() {
        let mut state = State::default();
        let stale = state.preview.outline[1].span.clone();
        state.editor.replace_all("短");
        assert!(
            stale.start > state.editor.text().len(),
            "前置:span 确已越界"
        );

        state.apply(Message::OutlineItemClicked(stale));
        assert_eq!(
            state.cursor.jump_to,
            Some(state.editor.len_chars()),
            "钳制到末尾(byte_to_char 再把字节偏移换成字符偏移)"
        );
    }

    /// 保存:字节原样落盘、dirty 复位;写入失败保留 dirty 并给出带路径的提示。
    #[test]
    fn save_writes_bytes_and_resets_dirty() {
        let path = temp_path("save.md");
        let mut state = State::default();
        state.editor.insert_chars(0, "改动\r\n");
        assert!(state.editor.is_dirty());

        state.save_to(path.clone());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            state.editor.text().as_bytes()
        );
        assert!(!state.editor.is_dirty());
        assert_eq!(state.document.path.as_deref(), Some(path.as_path()));
        let _ = std::fs::remove_file(&path);

        // 目录不存在 → 失败路径:dirty 保留,提示含路径
        state.editor.insert_chars(0, "再改");
        state.save_to(PathBuf::from("/latermd/no/such/dir.md"));
        assert!(state.editor.is_dirty());
        let notice = state.document.notice.as_deref().unwrap();
        assert!(notice.contains("dir.md"), "{notice}");
    }

    /// 帧末刷新把缓冲的 dirty 镜像到文档状态(窗口标题的唯一数据来源)。
    #[test]
    fn end_of_logic_mirrors_editor_dirty() {
        let mut state = State::default();
        state.end_of_logic();
        assert!(!state.document.dirty);
        state.editor.insert_chars(0, "x");
        assert!(!state.document.dirty, "编辑动作本身不动镜像");
        state.end_of_logic();
        assert!(state.document.dirty);
    }

    /// 主题切换归约:状态翻转 + settings.json 落盘(注入临时目录,不碰
    /// 真实平台配置);成功路径无提示。
    #[test]
    fn theme_change_updates_state_and_persists() {
        let dir = temp_path("theme-dir");
        let mut state = State {
            settings_dir: Some(dir.clone()),
            ..State::default()
        };

        state.apply(Message::ThemeChanged(ThemeMode::Light));
        assert_eq!(state.theme.mode, ThemeMode::Light);
        assert!(state.document.notice.is_none());
        // "light" 必须在盘上,重启 load 才能还原
        let json = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(json.contains("\"light\""), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 落盘失败(目录路径被同名文件占据):切换照常生效,失败带路径进提示行。
    #[test]
    fn theme_save_failure_lands_in_notice_but_mode_still_changes() {
        let blocker = temp_path("theme-blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let mut state = State {
            settings_dir: Some(blocker.clone()),
            ..State::default()
        };

        state.apply(Message::ThemeChanged(ThemeMode::Light));
        assert_eq!(state.theme.mode, ThemeMode::Light, "持久化失败不影响切换");
        let notice = state.document.notice.as_deref().unwrap();
        assert!(notice.contains("主题保存失败"), "{notice}");
        assert!(notice.contains("theme-blocker"), "{notice}");
        let _ = std::fs::remove_file(&blocker);
    }

    /// 文件树消息链:换根(持久化 + 最近列表)、展开翻转、懒加载在帧末补
    /// 齐子项、点击文件换入缓冲并展开祖先;dirty 时点击文件被拦。
    #[test]
    fn file_tree_messages_drive_root_toggle_and_open() {
        let dir = temp_path("filetree-root");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/note.md"), "# 树内标题\n").unwrap();
        std::fs::write(dir.join("top.md"), "# 顶层\n").unwrap();

        let settings_dir = temp_path("filetree-settings");
        let mut state = State {
            settings_dir: Some(settings_dir.clone()),
            ..State::default()
        };
        state.apply(Message::FileTreeRootSelected(dir.clone()));
        assert_eq!(state.file_tree.root.as_deref(), Some(dir.as_path()));
        assert_eq!(state.file_tree.recents, vec![dir.clone()]);
        assert!(
            settings_dir.join("file_tree.json").exists(),
            "最近目录持久化"
        );

        // 懒加载:换根不列举,帧末归约才列根级子项
        assert!(state.file_tree.children.is_empty());
        state.end_of_logic();
        let root_children = state.file_tree.children.get(&dir).unwrap();
        assert_eq!(
            root_children
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["docs", "top.md"]
        );

        // 展开翻转:toggle 后帧末补齐该目录子项
        state.apply(Message::FileTreeToggled(dir.join("docs")));
        state.end_of_logic();
        assert!(state
            .file_tree
            .children
            .contains_key(dir.join("docs").as_path()));

        // 点击文件:换入缓冲、树内祖先展开(为高亮行可见)
        let note = dir.join("docs/note.md");
        state.apply(Message::FileSelected(note.clone()));
        assert_eq!(state.document.path.as_deref(), Some(note.as_path()));
        assert_eq!(state.editor.text(), "# 树内标题\n");
        assert_eq!(
            state.file_tree.expanded.get(dir.join("docs").as_path()),
            Some(&true),
            "打开的文件的父目录已展开"
        );

        // dirty 保护:未保存时点击树上另一文件不动当前文档
        state.editor.insert_chars(0, "草稿");
        state.apply(Message::FileSelected(dir.join("top.md")));
        assert_eq!(state.document.path.as_deref(), Some(note.as_path()));
        assert!(state.document.notice.is_some());
        assert!(state.editor.text().starts_with("草稿"));

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&settings_dir);
    }

    /// 导出:写出的是完整 HTML 文档(标题取自缓冲当前内容),且不触碰文档
    /// 身份 —— 路径不被认领、dirty 不被清、失败提示带路径。
    #[test]
    fn export_writes_html_without_touching_document_identity() {
        let path = temp_path("export.html");
        let mut state = State::default();
        state.editor.insert_chars(0, "# 导出标题\n");
        assert!(state.editor.is_dirty());

        state.export_html_to(&path);
        let html = std::fs::read_to_string(&path).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"), "{html}");
        assert!(html.contains("<h1>导出标题</h1>"), "{html}");
        assert!(html.contains("max-width: 46em"), "{html}");
        // 派生物:dirty 保留、路径不认领
        assert!(state.editor.is_dirty());
        assert_eq!(state.document.path, None);
        let _ = std::fs::remove_file(&path);

        state.export_html_to(&PathBuf::from("/latermd/no/such/dir.html"));
        let notice = state.document.notice.as_deref().unwrap();
        assert!(notice.contains("导出失败"), "{notice}");
        assert!(notice.contains("dir.html"), "{notice}");
    }

    /// 搜索点击归约:换文件 + 跳到命中行行首。行号 1 起 → rope 行 0 起,
    /// 目标是第 2 行(含 CJK)行首的字符偏移。
    #[test]
    fn search_click_opens_file_and_jumps_to_hit_line() {
        let dir = temp_path("search-click");
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("note.md");
        std::fs::write(&note, "# 甲\n你好 world\n第三行\n").unwrap();

        let mut state = State::default();
        state.apply(Message::SearchResultClicked(note.clone(), 2));

        assert_eq!(state.document.path.as_deref(), Some(note.as_path()));
        assert_eq!(state.editor.text(), "# 甲\n你好 world\n第三行\n");
        let line2_byte = state.editor.line_to_byte(1);
        assert_eq!(line2_byte, 6, "前置:首行「# 甲\\n」共 6 字节");
        assert_eq!(
            state.cursor.jump_to,
            Some(state.editor.byte_to_char(line2_byte)),
            "光标(字符偏移)落在第二行行首"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 已打开文件上的点击:不重开、只更新跳转目标(路径相同即跳过 IO)。
    #[test]
    fn search_click_on_current_file_only_jumps() {
        let dir = temp_path("search-same");
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("note.md");
        std::fs::write(&note, "一\n二\n三\n").unwrap();

        let mut state = State::default();
        state.apply(Message::SearchResultClicked(note.clone(), 1));
        assert_eq!(state.cursor.jump_to, Some(0));
        state.apply(Message::SearchResultClicked(note.clone(), 3));
        assert_eq!(
            state.cursor.jump_to,
            Some(state.editor.byte_to_char(state.editor.line_to_byte(2))),
            "同行内再次点击,光标前进到第三行"
        );
        assert!(state.document.notice.is_none(), "未发生 IO 失败");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 过期行号(搜索后文件变短/被外部修改):钳制到文末,不 panic。
    #[test]
    fn search_click_with_stale_line_clamps_to_end() {
        let dir = temp_path("search-stale");
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("note.md");
        std::fs::write(&note, "只有一行\n").unwrap();

        let mut state = State::default();
        state.apply(Message::SearchResultClicked(note.clone(), 999));
        assert_eq!(
            state.cursor.jump_to,
            Some(state.editor.len_chars()),
            "行号远超行数,钳制到文末"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 点击指向已消失的文件:读盘失败落提示行,不换文档、不设跳转。
    #[test]
    fn search_click_on_missing_file_notifies_without_jump() {
        let mut state = State::default();
        let before = state.editor.text().to_owned();
        state.apply(Message::SearchResultClicked(
            PathBuf::from("/latermd/no/such/hit.md"),
            1,
        ));
        assert!(state.document.notice.is_some(), "读盘失败有提示");
        assert_eq!(state.document.path, None, "文档身份未变");
        assert_eq!(state.editor.text(), before, "缓冲未被触碰");
        assert_eq!(state.cursor.jump_to, None, "不设跳转");
    }

    /// dirty 保护与文件树同款:未保存时点击搜索结果被拦,不跳转。
    #[test]
    fn search_click_blocked_while_dirty() {
        let dir = temp_path("search-dirty");
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("note.md");
        std::fs::write(&note, "目标\n").unwrap();

        let mut state = State::default();
        state.editor.insert_chars(0, "草稿");
        assert!(state.editor.is_dirty());

        state.apply(Message::SearchResultClicked(note.clone(), 1));
        assert_eq!(state.document.path, None, "未换文档");
        assert_eq!(state.cursor.jump_to, None, "未设跳转");
        assert!(state.document.notice.is_some(), "拦截有提示");
        assert!(state.editor.text().starts_with("草稿"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 去抖两段归约:输入变化清结果并顺延计时;到点发起(有根)Running,
    /// 帧末收流落回 Idle 且命中在缓存;无根时到点是复位而非发起。
    #[test]
    fn search_messages_drive_debounce_and_start() {
        let dir = temp_path("search-flow");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "latermd 独占一行\n没有\n").unwrap();

        let mut state = State::default();
        state.file_tree.root = Some(dir.clone());
        state.search.query = "latermd".into();
        // 预置旧搜索残留:归约必须清掉
        state.search.hits.push(crate::search::SearchResult {
            path: dir.join("stale.md"),
            line_no: 1,
            line_text: "旧根的残留".into(),
        });

        state.apply(Message::SearchQueryChanged);
        assert!(state.search.debounce_due.is_some(), "去抖计时已顺延");
        assert!(state.search.hits.is_empty(), "旧结果被清");

        state.apply(Message::SearchRequested);
        assert!(state.search.is_running(), "有根 + 合法模式 → Running");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while state.search.is_running() && std::time::Instant::now() < deadline {
            state.end_of_logic();
            if state.search.is_running() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        assert_eq!(
            state.search.status,
            crate::search::SearchStatus::Finished,
            "自然结束"
        );
        assert_eq!(state.search.hits.len(), 1);
        assert_eq!(state.search.hits[0].line_no, 1);

        // 无根:到点发起是防御性复位,不进后台
        let mut rootless = State::default();
        rootless.search.query = "latermd".into();
        rootless.apply(Message::SearchQueryChanged);
        rootless.apply(Message::SearchRequested);
        assert!(!rootless.search.is_running());
        assert_eq!(rootless.search.debounce_due, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 换根复位搜索:结果与去抖计时一并丢弃,绝不重发(新根由用户重新输入)。
    #[test]
    fn changing_tree_root_resets_search() {
        let dir = temp_path("search-reset-root");
        std::fs::create_dir_all(&dir).unwrap();
        let settings_dir = temp_path("search-reset-settings");
        let mut state = State {
            settings_dir: Some(settings_dir.clone()),
            ..State::default()
        };
        state.search.query = "latermd".into();
        state.search.debounce_due = Some(std::time::Instant::now());
        state.search.hits.push(crate::search::SearchResult {
            path: dir.join("old.md"),
            line_no: 1,
            line_text: "旧根结果".into(),
        });

        state.apply(Message::FileTreeRootSelected(dir.clone()));
        assert_eq!(state.search.debounce_due, None, "去抖计时被清");
        assert!(state.search.hits.is_empty(), "旧根结果被清");
        assert!(!state.search.is_running());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&settings_dir);
    }

    /// AI 流式全链路归约:发起(补空行 + 流式标志)、防重入、chunk 追加、
    /// 收尾。provider 用零间隔,流会瞬时跑完,归约侧按消息逐条喂。
    #[test]
    fn ai_messages_stream_append_and_reentry_guard() {
        let mut state = State::default();
        // 快速 provider:测试不等 3-5 秒的联调节奏
        state.ai.provider = latermd_ai::MockProvider::with_interval(std::time::Duration::ZERO);
        let base = state.editor.text().to_owned();

        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming(), "发起后流式标志置位");
        assert!(
            state.editor.text().ends_with("\n\n"),
            "示例文档不以空行结尾,发起时补成空行"
        );

        // 防重入:流式中再次触发被忽略(标志仍置位、只发起了这一个流)
        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming());

        // 收流到自然结束:chunk 追加到末尾,AiDone 收尾清标志
        let mut done = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done && std::time::Instant::now() < deadline {
            for message in state.poll_ai() {
                if matches!(message, Message::AiDone | Message::AiFailed(_)) {
                    done = true;
                }
                state.apply(message);
            }
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        assert!(done, "流在超时前自然收尾");
        assert!(!state.ai.is_streaming(), "AiDone 归约清流式标志");

        let text = state.editor.text();
        assert!(text.starts_with(&base), "已有内容原样保留在头部");
        assert!(text.len() > base.len() + 2, "AI 文本已追加");
        assert!(state.editor.is_dirty(), "AI 写入置 dirty");
        assert!(state.editor.revision() > 0);

        // 收尾后可再次发起(防重入不拦新流)
        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming());
        state.ai.finish();
    }

    /// 评审回归:流式进行中切文档必须取消在途流。AI 写入置 dirty →
    /// Ctrl+S 清 dirty → New 的 unsaved_guard 放行 → load_document 换入
    /// 新缓冲;若不收尾,剩余 chunk 会经 poll_ai 追加进新文档。文件树点击
    /// 与搜索跳转同走 load_document,一处收口。
    #[test]
    fn switching_document_cancels_inflight_ai_stream() {
        let path = temp_path("ai-switch.md");
        let mut state = State::default();
        // 慢 provider:保证测试在流自然收尾前完成切文档(20ms × 30-50 块)
        state.ai.provider =
            latermd_ai::MockProvider::with_interval(std::time::Duration::from_millis(20));
        state.document.path = Some(path.clone()); // Save 直落盘,不弹对话框

        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming());
        // 前置:流确实在产块(对照取消后的空收流)
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(
            state
                .poll_ai()
                .iter()
                .any(|m| matches!(m, Message::AiChunk { .. })),
            "前置:流在切文档前已产出正文块"
        );
        assert!(state.editor.is_dirty(), "AI 写入置 dirty");

        // 复刻评审路径:Save 清 dirty,放行 New 的 unsaved_guard
        state.apply(Message::FileCommand(FileCmd::Save));
        assert!(path.exists(), "已落盘");
        assert!(!state.editor.is_dirty());

        state.apply(Message::FileCommand(FileCmd::New));
        assert_eq!(state.document.path, None);
        assert_eq!(state.editor.text(), "", "新文档为空");
        assert!(!state.ai.is_streaming(), "换文档取消在途流");

        // 给 worker 留出再发几块的时间:接收端已 drop,chunk 不得再进文档
        std::thread::sleep(std::time::Duration::from_millis(150));
        let messages = state.poll_ai();
        assert!(
            messages
                .iter()
                .all(|m| !matches!(m, Message::AiChunk { .. })),
            "取消后 poll 不再产出正文块,实际 {messages:?}"
        );
        assert_eq!(state.editor.text(), "", "新文档未被 AI 追加");
        let _ = std::fs::remove_file(&path);
    }

    /// AiChunk 追加语义:精确接在文档末尾;空 delta 是无操作(不推进修订号)。
    #[test]
    fn ai_chunk_appends_at_end_and_empty_delta_is_noop() {
        let mut state = State::default();
        state.apply(Message::AiChunk {
            delta: "续写".into(),
        });
        assert!(state.editor.text().ends_with("续写"));
        assert!(state.editor.is_dirty());

        let (rev, len) = (state.editor.revision(), state.editor.len_chars());
        state.apply(Message::AiChunk {
            delta: String::new(),
        });
        assert_eq!(state.editor.revision(), rev, "空 delta 不推进修订号");
        assert_eq!(state.editor.len_chars(), len);
    }

    /// 发起归约的空行补齐矩阵:空文档不动、单换行补一个、空行结尾不动、
    /// 行中结尾补两个(让 AI 首行独立成段)。
    #[test]
    fn ai_start_normalizes_trailing_blank_line() {
        let cases: [(&str, usize); 4] = [
            ("", 0),       // 空文档不动
            ("甲\n", 1),   // 单换行 → 补一个变空行
            ("甲\n\n", 0), // 已是空行结尾
            ("甲乙", 2),   // 行中 → 补两个
        ];
        for (initial, expected_pad) in cases {
            let mut state = State::default();
            state.editor.load(initial);
            state.ai.provider = latermd_ai::MockProvider::with_interval(std::time::Duration::ZERO);
            state.start_ai_stream();
            let expected = format!("{initial}{}", "\n".repeat(expected_pad));
            assert!(
                state.editor.text().starts_with(&expected),
                "初始 {initial:?} 的补行结果应为 {expected:?},实际 {:?}",
                &state.editor.text()[..expected.len()]
            );
            state.ai.finish();
        }
    }

    /// AiFailed 归约:清流式标志 + 错误描述进提示行,文档不被触碰。
    #[test]
    fn ai_failed_finishes_stream_and_lands_in_notice() {
        let mut state = State::default();
        state.apply(Message::AiStart);
        assert!(state.ai.is_streaming());
        let before = state.editor.text().to_owned();

        state.apply(Message::AiFailed("额度用尽".into()));
        assert!(!state.ai.is_streaming(), "失败同样收尾");
        assert_eq!(state.document.notice.as_deref(), Some("额度用尽"));
        assert_eq!(state.editor.text(), before, "失败文本不写入文档");
        // 失败不算完成:指令卡的状态键一并清空(卡片回「未执行」)
        assert_eq!(state.ai.last_prompt, None);
    }

    /// ai:// 链接点击归约:Ok(prompt) 复用流式启动路径(补空行 + 发起),
    /// chunk 照常归约追加;流式进行中再点击被防重入忽略(连补空行都不发生,
    /// 与 AiStart 的入口检查同一处);Err 落提示行且不动流式生命周期。
    #[test]
    fn ai_link_clicked_streams_with_link_prompt_and_reentry_is_ignored() {
        let mut state = State::default();
        state.ai.provider = latermd_ai::MockProvider::with_interval(std::time::Duration::ZERO);
        let base = state.editor.text().to_owned();

        state.apply(Message::AiLinkClicked {
            prompt: Ok("续写一段 Markdown 介绍".into()),
        });
        assert!(state.ai.is_streaming(), "链接点击发起了流");
        assert!(
            state.editor.text().ends_with("\n\n"),
            "发起时补成空行结尾,与 AiStart 同语义"
        );

        let mut done = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done && std::time::Instant::now() < deadline {
            for message in state.poll_ai() {
                if matches!(message, Message::AiDone | Message::AiFailed(_)) {
                    done = true;
                }
                state.apply(message);
            }
            if !done {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        assert!(done, "流在超时前自然收尾");
        assert!(!state.ai.is_streaming());
        assert!(state.editor.text().len() > base.len() + 2, "AI 文本已追加");
        assert!(state.editor.is_dirty(), "AI 写入置 dirty");
        // 状态键:自然收尾保留最近 prompt,指令卡凭它显示「已完成」
        assert_eq!(
            state.ai.last_prompt.as_deref(),
            Some("续写一段 Markdown 介绍")
        );

        // 防重入:慢 provider 发起后再点链接,忽略且不补空行
        state.ai.provider =
            latermd_ai::MockProvider::with_interval(std::time::Duration::from_millis(50));
        state.apply(Message::AiStart);
        let streaming_text = state.editor.text().to_owned();
        state.apply(Message::AiLinkClicked {
            prompt: Ok("流式中再来一次".into()),
        });
        assert!(state.ai.is_streaming());
        assert_eq!(
            state.editor.text(),
            streaming_text,
            "流式中点击链接被忽略:文档一字未动"
        );
        state.ai.finish();

        // Err:未实现动作 / 解析失败的提示语直接落状态栏,不发起、不碰文档
        let mut idle = State::default();
        idle.apply(Message::AiLinkClicked {
            prompt: Err("未实现的 AI 动作:summarize".into()),
        });
        assert!(!idle.ai.is_streaming(), "解析失败不发起流");
        assert_eq!(
            idle.document.notice.as_deref(),
            Some("未实现的 AI 动作:summarize")
        );
    }
}
