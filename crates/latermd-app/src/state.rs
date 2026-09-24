//! 应用状态与消息骨架(docs/adr-005 §5)。
//!
//! 归约铁律:状态变更只发生在 `App::logic` 调用的 [`State::apply`];
//! `App::ui` 只读状态、只产出 [`Message`]。本文件目前落了侧边栏与文件操作
//! 两组条目,`file_tree` / `search` / `outline` 等字段随对应模块接入时
//! 增量加入,完整规划见 docs/adr-005 §5.1。
//!
//! 例外:编辑器缓冲与预览快照由 `ui::editor` 原地维护 —— `TextEdit` 是
//! 立即模式控件,必须拿到 `&mut` 缓冲才能绘制(AGENTS.md §8「UI 与状态机
//! 天然耦合」),快照又是它的派生缓存,归约进下一帧反而让预览滞后一帧。

use crate::file::{self, FileCmd};
use latermd_editor::EditorBuffer;
use std::path::{Path, PathBuf};

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

/// 侧边栏状态。`visible` 直接喂给 `Panel::show_collapsible` 的 `&mut bool`,
/// 折叠/展开由面板把手原地翻转,不走消息。
pub struct SidebarState {
    /// 是否展开。
    pub visible: bool,
    /// 当前页签。
    pub active_tab: SidebarTab,
}

/// 预览快照:喂给渲染层的全文字符串 + 已同步到的编辑器修订号。
///
/// 只在编辑器修订号前进时重建(vendored 层内部还会按 text hash 二次
/// 缓存),空闲帧既不拷贝字符串也不重解析。
pub struct PreviewState {
    /// 当前喂给 [`egui_markdown::MarkdownLabel`] 的全文。
    pub text: String,
    /// 快照对应的 [`EditorBuffer::revision`]。
    pub synced_rev: u64,
}

/// 应用根状态。
pub struct State {
    /// 侧边栏。
    pub sidebar: SidebarState,
    /// 编辑器缓冲(唯一正文真源)。
    pub editor: EditorBuffer,
    /// 预览快照。
    pub preview: PreviewState,
    /// 当前文档的落盘身份。
    pub document: DocumentState,
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
            preview: PreviewState {
                text: editor.text().to_owned(),
                synced_rev: editor.revision(),
            },
            editor,
            document: DocumentState {
                path: None,
                dirty: false,
                notice: None,
            },
        }
    }
}

/// UI 事件消息:`ui` 产出、`logic` 消费(docs/adr-005 §5.1/§5.2)。
///
/// 后续变体(`SearchQueryChanged` / `OutlineItemClicked` …)随搜索、大纲模块
/// 接入加入;后台任务的结果回传也走同一入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// 切换侧边栏页签。
    SidebarTabChanged(SidebarTab),
    /// 请求执行文件命令(对话框与 IO 在归约中发生)。
    FileCommand(FileCmd),
    /// 关闭提示行。
    NoticeDismissed,
}

impl State {
    /// 消费一条消息,变更状态。只允许在 `App::logic` 调用。
    pub fn apply(&mut self, message: Message) {
        match message {
            Message::SidebarTabChanged(tab) => self.sidebar.active_tab = tab,
            Message::FileCommand(cmd) => self.run_file_cmd(cmd),
            Message::NoticeDismissed => self.document.notice = None,
        }
    }

    /// 帧末刷新派生状态。`App::logic` 每帧调用一次。
    pub fn end_of_logic(&mut self) {
        self.document.dirty = self.editor.is_dirty();
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
    fn open_from(&mut self, path: &Path) {
        match file::read(path) {
            Ok(text) => self.load_document(Some(path.to_path_buf()), &text),
            Err(error) => self.document.notice = Some(error.to_string()),
        }
    }

    /// 换入整篇内容并复位文档身份(新建/打开共用)。
    fn load_document(&mut self, path: Option<PathBuf>, text: &str) {
        self.editor.load(text);
        self.document.path = path;
        self.document.notice = None;
        // 预览快照就地重建,不等下一帧编辑面板的修订号检查
        self.preview.text = self.editor.text().to_owned();
        self.preview.synced_rev = self.editor.revision();
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
        let _ = std::fs::remove_file(&path);
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
}
