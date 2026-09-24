//! 应用状态与消息骨架(docs/adr-005 §5)。
//!
//! 归约铁律:状态变更只发生在 `App::logic` 调用的 [`State::apply`];
//! `App::ui` 只读状态、只产出 [`Message`]。本文件目前只落侧边栏相关条目,
//! `file_tree` / `search` / `outline` 等字段随对应模块接入时
//! 增量加入,完整规划见 docs/adr-005 §5.1。
//!
//! 例外:编辑器缓冲与预览快照由 `ui::editor` 原地维护 —— `TextEdit` 是
//! 立即模式控件,必须拿到 `&mut` 缓冲才能绘制(AGENTS.md §8「UI 与状态机
//! 天然耦合」),快照又是它的派生缓存,归约进下一帧反而让预览滞后一帧。

use latermd_editor::EditorBuffer;

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
        }
    }
}

/// UI 事件消息:`ui` 产出、`logic` 消费(docs/adr-005 §5.1/§5.2)。
///
/// 后续变体(`FileSelected` / `SearchQueryChanged` / `OutlineItemClicked` …)
/// 随文件树、搜索、大纲模块接入加入;后台任务的结果回传也走同一入口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// 切换侧边栏页签。
    SidebarTabChanged(SidebarTab),
}

impl State {
    /// 消费一条消息,变更状态。只允许在 `App::logic` 调用。
    pub fn apply(&mut self, message: Message) {
        match message {
            Message::SidebarTabChanged(tab) => self.sidebar.active_tab = tab,
        }
    }
}
