//! 多标签页(docs/auto-plan.md #11「multi-tabs」)。
//!
//! 每个标签持有**自己的一套**文档状态:落盘身份、编辑器缓冲、预览快照、
//! 大纲光标 —— 换标签零拷贝,只是换"当前指针"。编辑器 widget 的 TextEdit
//! 状态(光标/undo)按标签的稳定 `id` 隔离(`ui::editor` 用 id 派生 widget
//! id),不随标签增删错位。
//!
//! 语义约定(auto-plan 规格 + 既有哲学的延伸):
//! * 打开文件**永远开新标签**,已在某标签打开则直接激活它 —— 多标签下
//!   「新建/打开」不再覆盖当前缓冲,`unsaved_guard` 的拦截对象消失了;
//! * 关闭**脏**标签必须显式确认(模态文案同回滚确认的不可逆警示),
//!   静默丢稿的代价大于多一次点击;
//! * 在途 AI 流随「当前标签」走,换标签/关标签即作废(与 `load_document`
//!   既有口径一致:chunk 只认当前缓冲末尾,缓冲一换剩余块会写错文档)。

use crate::state::{DocumentState, OutlineCursor, PreviewState};
use latermd_editor::EditorBuffer;
use std::path::{Path, PathBuf};

/// 单个标签的全部文档状态。
pub struct TabState {
    /// 稳定 id:编辑器 widget id 与测试定位都用它,不随标签增删变化。
    pub id: u64,
    /// 落盘身份 + dirty 镜像 + 提示行。
    pub document: DocumentState,
    /// 编辑器缓冲(该标签的正文真源)。
    pub editor: EditorBuffer,
    /// 预览快照(含大纲)。
    pub preview: PreviewState,
    /// 大纲↔编辑器光标协调。
    pub cursor: OutlineCursor,
}

impl TabState {
    /// 以初始内容建标签(`TabsState` 的唯一入口,`next_id` 在外层统一发)。
    fn new(id: u64, path: Option<PathBuf>, text: &str) -> Self {
        let editor = EditorBuffer::new(text);
        Self {
            id,
            document: DocumentState {
                path,
                dirty: false,
                notice: None,
            },
            preview: PreviewState::new(&editor),
            cursor: OutlineCursor::default(),
            editor,
        }
    }

    /// 整篇换入并复位文档身份(读盘成功后的换入;同 `State::load_document`
    /// 的旧语义,但只作用于本标签)。
    pub fn load(&mut self, path: Option<PathBuf>, text: &str) {
        self.editor.load(text);
        self.document.path = path;
        self.document.notice = None;
        self.preview.rebuild(&self.editor);
    }
}

/// 标签集合:全部标签 + 当前指针 + 关闭确认 + id 发放器。
pub struct TabsState {
    /// 打开的标签(至少一个;关闭最后一个即换入新的空标签)。
    pub tabs: Vec<TabState>,
    /// 当前标签索引。
    pub active: usize,
    /// 待确认关闭的脏标签索引;`Some` 时 UI 显示确认模态。
    pub confirm_close: Option<usize>,
    /// 下一个标签的 id(自增,不复用 —— 关了再开新标签,编辑器 undo/光标
    /// 状态必须是全新的)。
    next_id: u64,
}

impl Default for TabsState {
    fn default() -> Self {
        // State::default 会立即用 SAMPLE_MD 建 tags;这里给空标签兜底,
        // 保证「至少一个标签」的不变量在任何构造路径下都成立。
        Self::new("")
    }
}

impl TabsState {
    /// 以初始内容建第一个标签。
    pub fn new(initial: &str) -> Self {
        Self {
            tabs: vec![TabState::new(1, None, initial)],
            active: 0,
            confirm_close: None,
            next_id: 2,
        }
    }

    /// 当前标签。
    pub fn current(&self) -> &TabState {
        &self.tabs[self.active]
    }

    /// 当前标签(可变)。
    pub fn current_mut(&mut self) -> &mut TabState {
        let active = self.active;
        &mut self.tabs[active]
    }

    /// 某路径已在哪个标签打开(按规范化的落盘身份逐字节比较)。
    pub fn find_by_path(&self, path: &Path) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.document.path.as_deref() == Some(path))
    }

    /// 开新标签(换入 `text`)并激活,返回新标签索引。打开文件**永远走
    /// 这里**,当前标签的缓冲与 dirty 不受影响。
    pub fn open_tab(&mut self, path: Option<PathBuf>, text: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(TabState::new(id, path, text));
        self.active = self.tabs.len() - 1;
        self.active
    }

    /// 激活标签(越界忽略 —— 标签条渲染与消息都来自同一帧快照,理论不会
    /// 越界,防御性短路)。切换会作废在途 AI 流(归约侧负责)。
    pub fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    /// 下一个标签的索引(Ctrl/Cmd+Tab 循环;不改动状态,归约侧拿去走
    /// `switch_active` 以统一作废在途流)。
    pub fn next_index(&self) -> usize {
        if self.tabs.is_empty() {
            0
        } else {
            (self.active + 1) % self.tabs.len()
        }
    }

    /// 移除标签(调用方保证脏确认已过)。关掉最后一个即换入新的空标签;
    /// 当前指针跟着修正(关的是当前或更靠前的标签时前移一位)。
    pub fn remove(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            let id = self.next_id;
            self.next_id += 1;
            self.tabs.push(TabState::new(id, None, ""));
            self.active = 0;
            return;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab_paths(tabs: &TabsState) -> Vec<Option<String>> {
        tabs.tabs
            .iter()
            .map(|tab| {
                tab.document
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string())
            })
            .collect()
    }

    /// 「至少一个标签」不变量:关到空自动补新的空标签,id 不复用。
    #[test]
    fn closing_last_tab_spawns_fresh_empty_one() {
        let mut tabs = TabsState::new("第一篇");
        let first_id = tabs.current().id;
        tabs.open_tab(None, "第二篇");
        tabs.remove(0);
        assert_eq!(tabs.tabs.len(), 1);
        assert_eq!(tabs.current().editor.text(), "第二篇");

        tabs.remove(0);
        assert_eq!(tabs.tabs.len(), 1, "关空后仍有标签");
        assert_eq!(tabs.current().editor.text(), "", "兜底标签为空");
        assert_ne!(tabs.current().id, first_id, "id 不复用,undo/光标全新");
    }

    /// 移除当前之前的标签,active 前移;移除之后的标签,active 不动。
    #[test]
    fn remove_adjusts_active_pointer() {
        let mut tabs = TabsState::new("甲");
        tabs.open_tab(None, "乙");
        tabs.open_tab(None, "丙");
        assert_eq!(tabs.active, 2);

        tabs.remove(0); // 移除甲:丙的索引 2→1
        assert_eq!(tabs.active, 1);
        assert_eq!(tabs.current().editor.text(), "丙");

        tabs.remove(1); // 移除丙(当前):active 钳到最后
        assert_eq!(tabs.active, 0);
        assert_eq!(tabs.current().editor.text(), "乙");
    }

    /// 按路径找标签;找不到返回 None。
    #[test]
    fn find_by_path_matches_document_identity() {
        let path = Path::new("/docs/a.md");
        let mut tabs = TabsState::new("甲");
        assert_eq!(tabs.find_by_path(path), None);
        tabs.current_mut().document.path = Some(path.to_path_buf());
        assert_eq!(tabs.find_by_path(path), Some(0));
    }

    /// 开新标签不覆盖当前缓冲;新标签自动激活。
    #[test]
    fn open_tab_activates_and_preserves_current() {
        let mut tabs = TabsState::new("甲的正文");
        tabs.open_tab(Some(PathBuf::from("/b.md")), "乙的正文");
        assert_eq!(tabs.active, 1);
        assert_eq!(tabs.current().editor.text(), "乙的正文");
        tabs.activate(0);
        assert_eq!(tabs.current().editor.text(), "甲的正文");
        assert_eq!(tab_paths(&tabs), vec![None, Some("/b.md".to_owned())]);
    }

    /// TabState::load 换入内容并重建预览(大纲与新文本同源)。
    #[test]
    fn tab_load_resyncs_preview_and_outline() {
        let mut tab = TabState::new(1, None, "旧内容");
        tab.load(Some(PathBuf::from("/x.md")), "# 新标题\n\n正文");
        assert_eq!(tab.editor.text(), "# 新标题\n\n正文");
        assert_eq!(tab.preview.outline.len(), 1, "大纲随换入重建");
        assert_eq!(tab.preview.outline[0].text, "新标题");
        assert_eq!(tab.document.path, Some(PathBuf::from("/x.md")));
    }
}
