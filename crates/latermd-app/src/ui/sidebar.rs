//! 侧边栏:页签栏 + 当前页签内容。文件树(P0 基础版:懒加载 + 截断 +
//! 点击打开 + 当前文件高亮)与大纲(P0 廉价版:点击跳编辑器光标)接真数据,
//! 搜索仍占位。
//!
//! 文件树渲染自管展开状态(`FileTreeState::expanded`)+ `ui.indent` 缩进,
//! 不用 `CollapsingHeader`(ADR-005 §4.1);行点击只发消息,子项列举在
//! `logic` 归约的 `FileTreeState::ensure_loaded`(懒加载落点)。

use crate::filetree::{DirChildren, FileTreeState, TreeEntry};
use crate::state::{Message, SidebarTab};
use latermd_md::OutlineItem;

use eframe::egui;
use std::path::Path;

/// 大纲层级每深一级的缩进宽度(px)。
const OUTLINE_INDENT: f32 = 14.0;

/// 绘制侧边栏内容。
///
/// `active_tab` 与 `outbox` 是从 `App` 上解构出的不相交借用,使本函数可以与
/// `show_collapsible` 原地持有的 `&mut visible` 并存(见 `layout.rs`)。
/// `file_tree` 与 `current_file` 只读:树交互全部经由消息归约。
pub fn ui(
    panel: &mut egui::Ui,
    active_tab: &mut SidebarTab,
    file_tree: &FileTreeState,
    current_file: Option<&Path>,
    outline: &[OutlineItem],
    cursor_byte: Option<usize>,
    outbox: &mut Vec<Message>,
) {
    tab_bar(panel, active_tab, outbox);
    panel.add_space(4.0);
    match *active_tab {
        SidebarTab::Files => files_panel(panel, file_tree, current_file, outbox),
        SidebarTab::Search => placeholder(panel, "搜索占位(P1)"),
        SidebarTab::Outline => outline_panel(panel, outline, cursor_byte, outbox),
    }
}

/// 页签栏。点击只发消息,归约在下一帧 `App::logic` 完成。
fn tab_bar(panel: &mut egui::Ui, active_tab: &mut SidebarTab, outbox: &mut Vec<Message>) {
    panel.horizontal(|ui| {
        for tab in SidebarTab::ALL {
            if ui
                .selectable_label(*active_tab == tab, tab.label())
                .clicked()
            {
                outbox.push(Message::SidebarTabChanged(tab));
            }
        }
    });
}

/// 大纲列表。数据来自 [`crate::state::PreviewState`] 的快照,文档变化时随
/// 预览同一时机重算,空闲帧零开销。
fn outline_panel(
    panel: &mut egui::Ui,
    outline: &[OutlineItem],
    cursor_byte: Option<usize>,
    outbox: &mut Vec<Message>,
) {
    egui::ScrollArea::vertical()
        .id_salt("outline-scroll")
        // 不收缩宽度,让长标题换行而不是把面板撑宽
        .auto_shrink([false, false])
        .show(panel, |ui| {
            if outline.is_empty() {
                ui.weak("无标题:文档里还没有 Markdown 标题");
            }
            let active = active_index(outline, cursor_byte);
            for (index, item) in outline.iter().enumerate() {
                outline_row(ui, item, active == Some(index), outbox);
            }
        });
}

/// 光标所在的当前小节:起始位置不晚于光标的最后一个标题。
///
/// 光标落在正文段落里时高亮其所属小节(与常见编辑器大纲一致);无光标
/// 信息或光标在首个标题之前则不高亮。
fn active_index(outline: &[OutlineItem], cursor_byte: Option<usize>) -> Option<usize> {
    cursor_byte.and_then(|cursor| outline.iter().rposition(|item| item.span.start <= cursor))
}

/// 单条大纲:按层级缩进的 SelectableLabel,点击发消息(跳转在 `logic`
/// 归约 + `ui::editor` 应用)。返回标签响应,独立成函数便于点击测试定位。
fn outline_row(
    ui: &mut egui::Ui,
    item: &OutlineItem,
    selected: bool,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let response = ui
        .horizontal(|ui| {
            ui.add_space(OUTLINE_INDENT * item.level.saturating_sub(1) as f32);
            ui.selectable_label(selected, &item.text)
        })
        .inner;
    if response.clicked() {
        outbox.push(Message::OutlineItemClicked(item.span.clone()));
    }
    response
}

fn placeholder(panel: &mut egui::Ui, text: &str) {
    panel.weak(text);
}

/// Files 页:顶部根目录选择行 + 懒加载目录树。
fn files_panel(
    panel: &mut egui::Ui,
    tree: &FileTreeState,
    current_file: Option<&Path>,
    outbox: &mut Vec<Message>,
) {
    // 根目录行:最近列表下拉(有历史才有)+ 选新目录按钮
    let root_hover = tree.root.as_deref().map(|path| path.display().to_string());
    panel.horizontal(|ui| {
        let selected = root_label(tree);
        if tree.recents.is_empty() {
            let response = ui.weak(&selected);
            if let Some(hover) = &root_hover {
                response.on_hover_text(hover);
            }
        } else {
            let response = egui::ComboBox::from_id_salt("file-tree-roots")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for dir in &tree.recents {
                        if ui
                            .selectable_label(
                                tree.root.as_deref() == Some(dir.as_path()),
                                dir.display().to_string(),
                            )
                            .clicked()
                        {
                            // ComboBox 默认 CloseOnClick:点击项后弹层自动收起
                            outbox.push(Message::FileTreeRootSelected(dir.clone()));
                        }
                    }
                })
                .response;
            if let Some(hover) = &root_hover {
                response.on_hover_text(hover);
            }
        }
        if ui.small_button("选择…").clicked() {
            outbox.push(Message::FileTreeRootPick);
        }
    });

    egui::ScrollArea::vertical()
        .id_salt("file-tree-scroll")
        // 不收缩宽度,让长文件名换行而不是把面板撑宽
        .auto_shrink([false, false])
        .show(panel, |ui| match tree.root.as_deref() {
            Some(root) => match tree.children.get(root) {
                Some(children) => {
                    tree_rows(ui, tree, children, current_file, outbox);
                }
                None => {
                    ui.weak("载入中…");
                }
            },
            None => {
                ui.weak("选择一个目录作为文件树根(点上方「选择…」)");
            }
        });
}

/// 根目录行的显示名。
fn root_label(tree: &FileTreeState) -> String {
    tree.root
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未选择根目录".to_owned())
}

/// 一组子项的各行 + 截断提示行。
fn tree_rows(
    ui: &mut egui::Ui,
    tree: &FileTreeState,
    children: &DirChildren,
    current_file: Option<&Path>,
    outbox: &mut Vec<Message>,
) {
    for entry in &children.entries {
        tree_row(ui, tree, entry, current_file, outbox);
    }
    if children.truncated > 0 {
        ui.weak(format!("…还有 {} 项未显示", children.truncated));
    }
}

/// 单行:目录点击翻转展开(箭头随状态),文件点击发打开消息;当前文档
/// 高亮。展开的目录子树用 `ui.indent` 缩进,path 作 id 源保证跨帧稳定。
/// 返回行响应,独立成函数便于点击测试定位。
fn tree_row(
    ui: &mut egui::Ui,
    tree: &FileTreeState,
    entry: &TreeEntry,
    current_file: Option<&Path>,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let open = tree.expanded.get(&entry.path).copied().unwrap_or(false);
    // 文件行留出箭头宽度的空白,与目录行的名字对齐
    let arrow = match (entry.is_dir, open) {
        (true, true) => "▾ ",
        (true, false) => "▸ ",
        (false, _) => "  ",
    };
    let selected = !entry.is_dir && current_file == Some(entry.path.as_path());
    let response = ui.selectable_label(selected, format!("{arrow}{}", entry.name));
    if response.clicked() {
        if entry.is_dir {
            outbox.push(Message::FileTreeToggled(entry.path.clone()));
        } else {
            outbox.push(Message::FileSelected(entry.path.clone()));
        }
    }
    if entry.is_dir && open {
        ui.indent(entry.path.clone(), |ui| {
            match tree.children.get(&entry.path) {
                Some(children) => {
                    tree_rows(ui, tree, children, current_file, outbox);
                }
                None => {
                    ui.weak("载入中…");
                }
            }
        });
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filetree::{DirChildren, TreeEntry};
    use egui::{Event, PointerButton, RawInput, Rect};
    use std::cell::Cell;
    use std::path::PathBuf;

    fn item(level: u8, text: &str, start: usize) -> OutlineItem {
        OutlineItem {
            level,
            text: text.to_owned(),
            span: start..start + text.len(),
        }
    }

    /// 测试用最小树:根下「docs」目录(展开,子项已缓存)+「note.md」文件
    /// + 截断计数,覆盖三种行的渲染路径。
    fn sample_tree() -> (FileTreeState, PathBuf) {
        let root = PathBuf::from("/vault");
        let docs = root.join("docs");
        let mut tree = FileTreeState {
            root: Some(root.clone()),
            ..FileTreeState::default()
        };
        tree.expanded.insert(docs.clone(), true);
        tree.children.insert(docs.clone(), DirChildren::default());
        tree.children.insert(
            root.clone(),
            DirChildren {
                entries: vec![
                    TreeEntry {
                        path: docs,
                        name: "docs".to_owned(),
                        is_dir: true,
                    },
                    TreeEntry {
                        path: root.join("note.md"),
                        name: "note.md".to_owned(),
                        is_dir: false,
                    },
                ],
                truncated: 4,
            },
        );
        (tree, root)
    }

    /// 点击树行:文件行发打开消息、目录行发翻转展开消息;仅渲染不产生消息。
    /// (ComboBox 弹层交互不在无头测试覆盖范围,根目录选择走 state::tests。)
    #[test]
    fn clicking_tree_rows_sends_messages() {
        let (tree, root) = sample_tree();
        let file = root.join("note.md");
        let docs = root.join("docs");
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let file_rect = Cell::new(Rect::NOTHING);
        let dir_rect = Cell::new(Rect::NOTHING);

        // 第一帧只渲染,借 Cell 拿到两行的屏幕位置
        ctx.run_ui(RawInput::default(), |ui| {
            let children = tree.children.get(&root).unwrap();
            dir_rect.set(tree_row(ui, &tree, &children.entries[0], None, &mut outbox).rect);
            file_rect.set(tree_row(ui, &tree, &children.entries[1], None, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");

        let click_at = |rect: &Cell<Rect>| {
            let center = rect.get().center();
            let click = |pressed| Event::PointerButton {
                pos: center,
                button: PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            vec![Event::PointerMoved(center), click(true), click(false)]
        };
        let render = |ui: &mut egui::Ui, outbox: &mut Vec<Message>| {
            let children = tree.children.get(&root).unwrap();
            tree_rows(ui, &tree, children, None, outbox);
        };

        // 第二帧点文件行 → FileSelected
        ctx.run_ui(
            RawInput {
                events: click_at(&file_rect),
                ..Default::default()
            },
            |ui| render(ui, &mut outbox),
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::FileSelected(file)]);

        // 第三帧点目录行 → FileTreeToggled
        outbox.clear();
        ctx.run_ui(
            RawInput {
                events: click_at(&dir_rect),
                ..Default::default()
            },
            |ui| render(ui, &mut outbox),
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::FileTreeToggled(docs)]);
    }

    /// 当前小节判定:光标在正文里高亮所属标题,首个标题之前/无光标不高亮。
    #[test]
    fn active_index_follows_cursor_section() {
        let outline = vec![item(1, "甲", 0), item(2, "乙", 10), item(3, "丙", 20)];
        assert_eq!(active_index(&outline, None), None);
        assert_eq!(active_index(&outline, Some(0)), Some(0), "恰在标题起点");
        assert_eq!(active_index(&outline, Some(5)), Some(0), "甲的正文");
        assert_eq!(active_index(&outline, Some(10)), Some(1));
        assert_eq!(active_index(&outline, Some(29)), Some(2));
    }

    /// 点击大纲条目:发出的消息携带该标题的源码区间;仅渲染不产生消息。
    #[test]
    fn clicking_item_sends_span_message() {
        let ctx = egui::Context::default();
        let items = [item(1, "标题一", 0), item(2, "标题二", 10)];
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧只渲染,借 Cell 拿到条目的屏幕位置
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(outline_row(ui, &items[1], false, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");

        // 第二帧在条目中心按下并抬起 → clicked
        let center = rect.get().center();
        let click = |pressed| Event::PointerButton {
            pos: center,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        ctx.run_ui(
            RawInput {
                events: vec![Event::PointerMoved(center), click(true), click(false)],
                ..Default::default()
            },
            |ui| {
                outline_row(ui, &items[1], false, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(outbox, vec![Message::OutlineItemClicked(10..19)]);
    }
}
