//! 应用状态与消息骨架(docs/adr-005 §5)。
//!
//! 归约铁律:状态变更只发生在 `App::logic` 调用的 [`State::apply`];
//! `App::ui` 只读状态、只产出 [`Message`]。本文件目前只落侧边栏相关条目,
//! `document` / `file_tree` / `search` / `outline` 等字段随对应模块接入时
//! 增量加入,完整规划见 docs/adr-005 §5.1。

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

/// 应用根状态。
pub struct State {
    /// 侧边栏。
    pub sidebar: SidebarState,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sidebar: SidebarState {
                visible: true,
                active_tab: SidebarTab::Files,
            },
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
