//! 侧边栏:页签栏 + 当前页签内容。文件树(P0 基础版:懒加载 + 截断 +
//! 点击打开 + 当前文件高亮)、全文搜索(P1:去抖 + 流式结果 + 点击跳行)
//! 与大纲(P0 廉价版:点击跳编辑器光标)均接真数据。
//!
//! 文件树渲染自管展开状态(`FileTreeState::expanded`)+ `ui.indent` 缩进,
//! 不用 `CollapsingHeader`(ADR-005 §4.1);行点击只发消息,子项列举在
//! `logic` 归约的 `FileTreeState::ensure_loaded`(懒加载落点)。
//!
//! 搜索面板的去抖:`ui` 把输入变化原地写入 `SearchState` 并发
//! `SearchQueryChanged`,`logic` 归约顺延去抖计时;到点发起在归约侧
//! (`layout.rs` 的 reduce,每帧必跑、不看本面板是否可见——输入后切走
//! 页签也照常搜),本层只把输入与开关写进状态。
//!
//! Git 页(P2)与文件树角标:数据全部来自 `GitPanelState` 的只读快照
//! (刷新时机在归约侧),本层零 git 调用;回滚按钮只发消息,checkout
//! 在确认模态之后(见 `ui::layout`)。

use crate::filetree::{DirChildren, FileTreeState, TreeEntry};
use crate::git_panel::GitPanelState;
use crate::search::{SearchResult, SearchState, SearchStatus, MAX_HITS};
use crate::state::{Message, SidebarTab};
use latermd_git::{CommitInfo, FileStatus, StatusKind};
use latermd_md::OutlineItem;

use eframe::egui;
use std::path::Path;

/// 大纲层级每深一级的缩进宽度(px)。
const OUTLINE_INDENT: f32 = 14.0;

/// 搜索结果行摘要的最大字符数(命中行可能是长段落,截断保列表可读)。
const SNIPPET_MAX_CHARS: usize = 120;

/// 大纲页的只读数据:标题快照 + 编辑器当前光标(两者分属 `State` 的
/// `preview` 与 `cursor` 字段,侧边栏只读;打包纯粹为收敛 `ui` 的参数)。
pub struct OutlineView<'a> {
    pub items: &'a [OutlineItem],
    pub cursor_byte: Option<usize>,
}

/// 绘制侧边栏内容。
///
/// `active_tab`、`search` 与 `outbox` 是从 `App` 上解构出的不相交借用,使本
/// 函数可以与 `show_collapsible` 原地持有的 `&mut visible` 并存(见
/// `layout.rs`)。`file_tree`、`git`、`outline` 与 `current_file` 只读:交互
/// 全部经由消息归约;`search` 的输入文本/开关由本层原地改写(TextEdit/
/// checkbox 控件的 `&mut` 要求,同编辑器缓冲的例外),取消与发起都在归约。
// 参数各属不同页签的状态,打包成结构只是造出人为聚合(vendored 层同款
// allow 先例见 egui_markdown/src/label.rs)
#[allow(clippy::too_many_arguments)]
pub fn ui(
    panel: &mut egui::Ui,
    active_tab: &mut SidebarTab,
    file_tree: &FileTreeState,
    current_file: Option<&Path>,
    outline: OutlineView<'_>,
    search: &mut SearchState,
    git: &GitPanelState,
    outbox: &mut Vec<Message>,
) {
    tab_bar(panel, active_tab, outbox);
    panel.add_space(4.0);
    match *active_tab {
        SidebarTab::Files => files_panel(panel, file_tree, current_file, git, outbox),
        SidebarTab::Search => search_panel(panel, search, file_tree.root.as_deref(), outbox),
        SidebarTab::Outline => outline_panel(panel, outline, outbox),
        SidebarTab::Git => git_panel(panel, git, outbox),
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
fn outline_panel(panel: &mut egui::Ui, outline: OutlineView<'_>, outbox: &mut Vec<Message>) {
    let OutlineView { items, cursor_byte } = outline;
    egui::ScrollArea::vertical()
        .id_salt("outline-scroll")
        // 不收缩宽度,让长标题换行而不是把面板撑宽
        .auto_shrink([false, false])
        .show(panel, |ui| {
            if items.is_empty() {
                ui.weak("无标题:文档里还没有 Markdown 标题");
            }
            let active = active_index(items, cursor_byte);
            for (index, item) in items.iter().enumerate() {
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

/// Search 页:输入行(正则 + 大小写开关)+ 状态行 + 结果列表。
///
/// 无根目录时只显示引导(搜索范围复用文件树的根,不另设第二份)。
fn search_panel(
    panel: &mut egui::Ui,
    search: &mut SearchState,
    root: Option<&Path>,
    outbox: &mut Vec<Message>,
) {
    let Some(root) = root else {
        panel.weak("搜索需要一个根目录:先在「文件」页选择");
        return;
    };

    panel.horizontal(|ui| {
        let edited = egui::TextEdit::singleline(&mut search.query)
            .id_salt("search-input")
            .hint_text("正则表达式…")
            .desired_width(f32::INFINITY)
            .show(ui)
            .response
            .changed();
        let toggled = ui.checkbox(&mut search.case_insensitive, "Aa").changed();
        if edited || toggled {
            outbox.push(Message::SearchQueryChanged);
        }
    });
    match &search.status {
        SearchStatus::Running => {
            panel.weak(format!("搜索中…(已 {} 条)", search.hits.len()));
        }
        SearchStatus::Finished if search.hits.is_empty() => {
            panel.weak("无命中");
        }
        SearchStatus::Invalid(msg) => {
            panel.weak(format!("⚠ {msg}"));
        }
        SearchStatus::Idle | SearchStatus::Finished => {}
    }

    egui::ScrollArea::vertical()
        .id_salt("search-results-scroll")
        .auto_shrink([false, false])
        .show(panel, |ui| {
            if search.hits.is_empty() && matches!(search.status, SearchStatus::Idle) {
                ui.weak("输入搜索词,回车不必按——300ms 停顿后自动搜索");
            }
            for hit in &search.hits {
                search_row(ui, hit, root, outbox);
            }
            if search.truncated {
                ui.weak(format!(
                    "已达 {} 条上限,后续命中未显示(输入更精确的模式收窄)",
                    MAX_HITS
                ));
            }
        });
}

/// 单条结果:第一行「相对根的路径:行号」,第二行命中行摘要;整块一个
/// SelectableLabel,点击发消息(打开 + 跳行在 `logic` 归约)。
/// 返回行响应,独立成函数便于点击测试定位。
fn search_row(
    ui: &mut egui::Ui,
    hit: &SearchResult,
    root: &Path,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let relative = hit.path.strip_prefix(root).unwrap_or(hit.path.as_path());
    let text = format!(
        "{}:{}\n{}",
        relative.display(),
        hit.line_no,
        snippet(&hit.line_text)
    );
    let response = ui.selectable_label(false, text);
    if response.clicked() {
        outbox.push(Message::SearchResultClicked(hit.path.clone(), hit.line_no));
    }
    response
}

/// 命中行摘要:按字符截断(不切断多字节序列),超长加省略号。
fn snippet(line: &str) -> String {
    if line.chars().count() <= SNIPPET_MAX_CHARS {
        line.to_owned()
    } else {
        let cut: String = line.chars().take(SNIPPET_MAX_CHARS).collect();
        format!("{cut}…")
    }
}

/// Files 页:顶部根目录选择行 + 懒加载目录树。
fn files_panel(
    panel: &mut egui::Ui,
    tree: &FileTreeState,
    current_file: Option<&Path>,
    git: &GitPanelState,
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
                    tree_rows(ui, tree, children, current_file, git, outbox);
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
    git: &GitPanelState,
    outbox: &mut Vec<Message>,
) {
    for entry in &children.entries {
        tree_row(ui, tree, entry, current_file, git, outbox);
    }
    if children.truncated > 0 {
        ui.weak(format!("…还有 {} 项未显示", children.truncated));
    }
}

/// 单行:目录点击翻转展开(箭头随状态),文件点击发打开消息;当前文档
/// 高亮,文件名右侧跟 Git 状态角标(P2)。展开的目录子树用 `ui.indent`
/// 缩进,path 作 id 源保证跨帧稳定。返回行响应,独立成函数便于点击测试
/// 定位。
fn tree_row(
    ui: &mut egui::Ui,
    tree: &FileTreeState,
    entry: &TreeEntry,
    current_file: Option<&Path>,
    git: &GitPanelState,
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
    let badge = if entry.is_dir {
        None
    } else {
        git.badge_for(&entry.path)
    };
    let response = badged_row_label(ui, selected, format!("{arrow}{}", entry.name), badge);
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
                    tree_rows(ui, tree, children, current_file, git, outbox);
                }
                None => {
                    ui.weak("载入中…");
                }
            }
        });
    }
    response
}

/// Git 页:降级提示,或「改动列表 + 选中文件 diff + 回滚按钮 + 历史
/// 折叠区」。数据是 `GitPanelState` 的只读快照(刷新在归约侧),交互全部
/// 经由消息;回滚按钮只发 [`Message::GitCheckoutRequested`],确认模态在
/// `ui::layout`。
fn git_panel(panel: &mut egui::Ui, git: &GitPanelState, outbox: &mut Vec<Message>) {
    if let Some(error) = &git.error {
        panel.weak(format!("⚠ {error}"));
        return;
    }

    panel.label(format!("改动({})", git.entries.len()));
    egui::ScrollArea::vertical()
        .id_salt("git-status-scroll")
        .auto_shrink([false, false])
        .max_height(160.0)
        .show(panel, |ui| {
            if git.entries.is_empty() {
                ui.weak("工作区干净,没有未提交的改动");
            }
            for entry in &git.entries {
                git_status_row(ui, git, entry, outbox);
            }
            if git.truncated > 0 {
                ui.weak(format!("…还有 {} 项未显示", git.truncated));
            }
        });

    if let Some(selected) = git.selected.as_deref() {
        panel.add_space(4.0);
        panel.monospace(selected);
        if git.diff.is_empty() {
            panel.weak("无文本改动");
        } else {
            diff_view(panel, &git.diff);
        }
        if panel.button("回滚此文件…").clicked() {
            outbox.push(Message::GitCheckoutRequested(selected.to_owned()));
        }
    }

    panel.add_space(4.0);
    egui::CollapsingHeader::new(format!("历史({})", git.commits.len()))
        .id_salt("git-history")
        .default_open(true)
        .show(panel, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("git-log-scroll")
                .auto_shrink([false, false])
                .max_height(220.0)
                .show(ui, |ui| {
                    if git.commits.is_empty() {
                        ui.weak("没有提交历史");
                    }
                    for commit in &git.commits {
                        commit_row(ui, commit, git.fetched_at);
                    }
                });
        });
}

/// 单条改动:彩色状态字母 + 相对仓库根路径;点击选中(高亮当前选中项)。
/// 返回行响应,独立成函数便于点击测试定位。
fn git_status_row(
    ui: &mut egui::Ui,
    git: &GitPanelState,
    entry: &FileStatus,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let selected = git.selected.as_deref() == Some(entry.path.as_str());
    let response = badged_row_label(ui, selected, entry.path.clone(), Some(entry.code));
    if response.clicked() {
        outbox.push(Message::GitFileSelected(entry.path.clone()));
    }
    response
}

/// 单条历史:短 hash + subject + 相对时间。P2 点击不跳转(工作区检出到
/// 历史版本是 P3 以后的命题),纯展示。
fn commit_row(ui: &mut egui::Ui, commit: &CommitInfo, now_epoch: i64) {
    ui.horizontal(|ui| {
        ui.monospace(egui::RichText::new(&commit.short_hash).weak());
        ui.label(&commit.subject);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(relative_time(commit.time, now_epoch));
        });
    });
}

/// 只读等宽 diff 视图:+ 行绿、- 行红、`@@` 行蓝、文件头弱化;横向不
/// 折行(`ScrollArea::both`,长行滚动)。文本已由 latermd-git 截断在
/// ~64KB,行数有界。
fn diff_view(panel: &mut egui::Ui, diff: &str) {
    egui::ScrollArea::both()
        .id_salt("git-diff-scroll")
        .auto_shrink([false, false])
        .max_height(200.0)
        .show(panel, |ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for line in diff.lines() {
                let color = if line.starts_with('+') {
                    DIFF_ADDED
                } else if line.starts_with('-') {
                    DIFF_REMOVED
                } else if line.starts_with('@') {
                    DIFF_HUNK
                } else {
                    ui.visuals().weak_text_color()
                };
                ui.monospace(egui::RichText::new(line).color(color));
            }
        });
}

/// 状态角标配色:M 黄(改)、A 绿(增)、U/D 红(冲突/删)、? 灰(未跟踪)。
/// 固定中间亮度,深浅主题下均可读。
fn status_color(kind: StatusKind) -> egui::Color32 {
    match kind {
        StatusKind::Modified => egui::Color32::from_rgb(235, 180, 60),
        StatusKind::Added => egui::Color32::from_rgb(96, 200, 120),
        StatusKind::Unmerged | StatusKind::Deleted => egui::Color32::from_rgb(235, 96, 96),
        StatusKind::Untracked => egui::Color32::from_rgb(150, 158, 168),
    }
}

/// diff 行的三档语义色(+/-/@@),文件头与其余走弱化前景。
const DIFF_ADDED: egui::Color32 = egui::Color32::from_rgb(96, 200, 120);
const DIFF_REMOVED: egui::Color32 = egui::Color32::from_rgb(235, 96, 96);
const DIFF_HUNK: egui::Color32 = egui::Color32::from_rgb(96, 150, 235);

/// 「正文 + 彩色单字母角标」的可选中行,文件树文件行与 Git 页改动行共用
/// (egui 0.36 的 `IntoAtoms` 元组语法:正文正常前景,角标按状态着色)。
fn badged_row_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: String,
    badge: Option<StatusKind>,
) -> egui::Response {
    match badge {
        Some(kind) => ui.selectable_label(
            selected,
            (
                text,
                egui::RichText::new(format!(" {kind}")).color(status_color(kind)),
            ),
        ),
        None => ui.selectable_label(selected, text),
    }
}

/// 提交时间 →「x 分钟前」式相对时间(与刷新时刻的差,分钟级精度)。
/// 单位逐级放大:刚刚/分钟/小时/天/月/年;时钟偏移的负差按「刚刚」。
fn relative_time(then: i64, now: i64) -> String {
    let delta = (now - then).max(0);
    let minutes = delta / 60;
    let hours = delta / 3600;
    let days = delta / 86400;
    if minutes < 1 {
        "刚刚".to_owned()
    } else if hours < 1 {
        format!("{minutes} 分钟前")
    } else if days < 1 {
        format!("{hours} 小时前")
    } else if days <= 30 {
        format!("{days} 天前")
    } else if days <= 365 {
        format!("{} 个月前", days / 30)
    } else {
        format!("{} 年前", days / 365)
    }
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

    /// 点击树行:文件行发打开消息、目录行发翻转展开消息;仅渲染不产生
    /// 消息。带 Git 角标的文件行走同一渲染路径(角标着色无断言,点击行为
    /// 与无角标一致是本测试的对象)。
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
        let mut git = GitPanelState::default();
        git.badges.insert(file.clone(), StatusKind::Modified);

        // 第一帧只渲染,借 Cell 拿到两行的屏幕位置
        ctx.run_ui(RawInput::default(), |ui| {
            let children = tree.children.get(&root).unwrap();
            dir_rect.set(tree_row(ui, &tree, &children.entries[0], None, &git, &mut outbox).rect);
            file_rect.set(tree_row(ui, &tree, &children.entries[1], None, &git, &mut outbox).rect);
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
            tree_rows(ui, &tree, children, None, &git, outbox);
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

    /// 点击搜索结果行:发出携带(路径, 行号)的消息;仅渲染不产生消息。
    #[test]
    fn clicking_search_row_sends_message() {
        let ctx = egui::Context::default();
        let root = PathBuf::from("/vault");
        let hit = SearchResult {
            path: root.join("docs/note.md"),
            line_no: 7,
            line_text: "命中这一行".to_owned(),
        };
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        // 第一帧只渲染,拿行位置
        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(search_row(ui, &hit, &root, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty());

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
                search_row(ui, &hit, &root, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::SearchResultClicked(root.join("docs/note.md"), 7)]
        );
    }

    /// Git 面板渲染矩阵:降级文案、干净工作区、改动列表 + 选中 diff +
    /// 历史,三条路径都不 panic;点击改动行发 GitFileSelected。
    #[test]
    fn git_panel_renders_states_and_clicking_row_sends_message() {
        let mut git = GitPanelState::default();
        git.entries.push(FileStatus {
            path: "docs/note.md".to_owned(),
            code: StatusKind::Modified,
        });
        git.selected = Some("docs/note.md".to_owned());
        git.diff = "@@ -1 +1 @@\n-旧\n+新\n".to_owned();
        git.commits.push(CommitInfo {
            hash: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            short_hash: "0123456".to_owned(),
            subject: "初稿".to_owned(),
            author: "LaterMD <latermd@test>".to_owned(),
            time: 1_700_000_000,
        });

        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let row_rect = Cell::new(Rect::NOTHING);

        // 帧 1:正常态整体渲染 + 拿改动行位置
        ctx.run_ui(RawInput::default(), |ui| {
            git_panel(ui, &git, &mut outbox);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");
        ctx.run_ui(RawInput::default(), |ui| {
            row_rect.set(git_status_row(ui, &git, &git.entries[0], &mut outbox).rect);
        })
        .drop_without_applying_deltas();

        // 帧 2:点击改动行 → GitFileSelected
        let center = row_rect.get().center();
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
                git_status_row(ui, &git, &git.entries[0], &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::GitFileSelected("docs/note.md".to_owned())]
        );

        // 降级态(非 git 仓库)、空历史与截断态:只渲染降级文案/提示行,不 panic
        let degraded = GitPanelState {
            error: Some("当前目录不是 Git 仓库".to_owned()),
            ..GitPanelState::default()
        };
        let empty = GitPanelState::default();
        let truncated = GitPanelState {
            truncated: 7,
            ..GitPanelState::default()
        };
        ctx.run_ui(RawInput::default(), |ui| {
            git_panel(ui, &degraded, &mut Vec::new());
        })
        .drop_without_applying_deltas();
        ctx.run_ui(RawInput::default(), |ui| {
            git_panel(ui, &empty, &mut Vec::new());
        })
        .drop_without_applying_deltas();
        ctx.run_ui(RawInput::default(), |ui| {
            git_panel(ui, &truncated, &mut Vec::new());
        })
        .drop_without_applying_deltas();
    }

    /// 相对时间:单位逐级放大,时钟偏移的负差按「刚刚」。
    #[test]
    fn relative_time_scales_units() {
        assert_eq!(relative_time(1_000, 1_000), "刚刚");
        assert_eq!(relative_time(1_050, 1_000), "刚刚", "负差按刚刚");
        assert_eq!(relative_time(1_000 - 30, 1_000), "刚刚", "不足一分钟");
        assert_eq!(relative_time(1_000 - 60, 1_000), "1 分钟前");
        assert_eq!(relative_time(1_000 - 3_600, 1_000), "1 小时前");
        assert_eq!(relative_time(1_000 - 86_400, 1_000), "1 天前");
        assert_eq!(relative_time(1_000 - 86_400 * 45, 1_000), "1 个月前");
        assert_eq!(relative_time(1_000 - 86_400 * 400, 1_000), "1 年前");
    }

    /// 摘要截断按字符不切断多字节序列,短行原样返回。
    #[test]
    fn snippet_truncates_by_chars() {
        assert_eq!(snippet("短行"), "短行");
        let long = "界".repeat(SNIPPET_MAX_CHARS + 10);
        let cut = snippet(&long);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), SNIPPET_MAX_CHARS + 1);
    }
}
