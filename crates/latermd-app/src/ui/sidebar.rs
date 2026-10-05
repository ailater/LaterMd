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

use crate::backlink_panel::{jump_target, Backlink, BacklinkState, BacklinkStatus};
use crate::command::Command;
use crate::filetree::{DirChildren, FileTreeState, TreeEntry};
use crate::git_panel::{DiffView, GitPanelState};
use crate::git_split_diff::{line_no_text, split_rows, SplitRow, MAX_SPLIT_PAIRS};
use crate::keymap::Keymap;
use crate::search::{SearchResult, SearchState, SearchStatus, MAX_HITS};
use crate::state::{Message, SidebarTab};
use crate::ui::tokens::{self, RADIUS_SM, SPACE_XS};
use latermd_git::{CommitInfo, DiffHunk, DiffLine, DiffLineKind, FileDiff, FileStatus, StatusKind};
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

/// 绘制左栏(docs/ui-shell-redesign.md §5,D3 已拍板;2026-09-27 修订,
/// 见 decisions-pending #31):顶段高频文件动作 → 次段四行视图导航 →
/// 中段视图内容(吃掉剩余高度)。原「底段设置」行已迁至标题栏右端齿轮。
///
/// 中段「吃掉剩余」由 `ScrollArea` 的 `max_height = available` 承担,不上
/// 嵌套 `Panel`(会造成 widget id 与 z-order 意外),也不用 `bottom_up`
/// (左右 snap 会让各段的阅读顺序与代码顺序相反)。
///
/// `active_tab`、`search` 与 `outbox` 是从 `App` 上解构出的不相交借用,使本
/// 函数可以与 `show_collapsible` 原地持有的 `&mut bool` 并存(见
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
    backlinks: &BacklinkState,
    outbox: &mut Vec<Message>,
) -> SidebarBands {
    let left = panel.max_rect().left();
    let right = panel.max_rect().right();
    let band = |top: f32, bottom: f32| {
        egui::Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, bottom))
    };

    let y0 = panel.cursor().top();
    top_actions(panel, outbox);
    let y1 = panel.cursor().top();
    view_nav(panel, *active_tab, outbox);
    let y2 = panel.cursor().top();
    panel.separator();
    let y3 = panel.cursor().top();
    egui::ScrollArea::vertical()
        .id_salt("nav-body")
        .auto_shrink([false, false])
        .max_height(panel.available_height())
        .show(panel, |ui| match *active_tab {
            SidebarTab::Files => files_panel(ui, file_tree, current_file, git, outbox),
            SidebarTab::Search => search_panel(ui, search, file_tree.root.as_deref(), outbox),
            SidebarTab::Outline => outline_panel(ui, outline, outbox),
            SidebarTab::Git => git_panel(ui, git, outbox),
            SidebarTab::Backlinks => backlinks_panel(
                ui,
                backlinks,
                file_tree.root.as_deref(),
                current_file,
                outbox,
            ),
        });
    let y4 = panel.cursor().top();

    SidebarBands {
        top: band(y0, y1),
        nav: band(y1, y2),
        body: band(y3, y4),
    }
}

/// 左栏三段各自的竖直区间,自上而下互不重叠。
///
/// 由 [`ui`] 顺手测出来返回:`nav_row` 是手绘 widget(无可读名字),要算
/// 「第 N 行的 y」只能靠 [`SidebarTab::ALL`] 的顺序 + `NAV_ROW_H` 自己推。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidebarBands {
    /// 顶段:高频文件动作。
    pub top: egui::Rect,
    /// 次段:四行视图导航(每行 `NAV_ROW_H` 高)。
    pub nav: egui::Rect,
    /// 中段:当前视图内容,吃掉剩余高度(至左栏底部)。
    pub body: egui::Rect,
}

/// 单条视图导航行的**竖直中心 y**(相对传进来的 `panel`)。
///
/// 给无头测试定位用:行本身是 `allocate_exact_size` 的手绘 widget,没有可
/// 从外部读的名字;而顺序是 `SidebarTab::ALL`、行高是 `NAV_ROW_H`,与绘制
/// 同源地算出来即可,不必把矩形一路传出去。
// 只有无头测试用;生产路径靠 `SidebarBands` 定位。
#[cfg(test)]
pub(crate) fn nav_row_center_y(top: f32, tab: SidebarTab) -> f32 {
    let index = SidebarTab::ALL
        .iter()
        .position(|candidate| *candidate == tab)
        .expect("tab 必在 ALL 里");
    top + crate::ui::tokens::NAV_ROW_H * (index as f32 + 0.5)
}

/// 左栏高频文件动作(docs/ui-shell-redesign.md §5 顶段;2026-09-27 起
/// 是文件动作的**唯一常驻按钮入口** —— 编辑器区顶部的文件工具栏退役,
/// actions 收口到本栏,decisions-pending #32)。
///
/// 全集 = [`Command::FILE`] + 导出两项(HTML / PDF);AI 与视图开关不在
/// 此列(AI 在菜单栏「AI」,视图开关在标题栏按钮)。菜单栏的菜单项一个
/// 不删 —— 这里是快捷入口,不是唯一入口(ui-polish §1.2「菜单栏负责
/// 全部」)。`horizontal_wrapped`:左栏拖到 180px 下限时换行而不是溢出
/// 裁切(R4)。
fn top_actions(panel: &mut egui::Ui, outbox: &mut Vec<Message>) {
    top_actions_with_probe(panel, outbox, None);
}

/// 同 [`top_actions`],额外把每个按钮的 `(命令, 矩形)` 交给 `probe`。
///
/// 无头测试量按钮位置用(与 `ui::format_bar::ui_with_probe` 同款手法):
/// 按钮坐标由 `horizontal_wrapped` 的换行演算决定,手搓必然与真实帧错位。
fn top_actions_with_probe(
    panel: &mut egui::Ui,
    outbox: &mut Vec<Message>,
    probe: Option<&mut dyn FnMut(Command, egui::Rect)>,
) {
    let mut probe = probe;
    panel.horizontal_wrapped(|ui| {
        for cmd in Command::FILE
            .iter()
            .copied()
            .chain([Command::ExportHtml, Command::ExportPdf])
        {
            let response =
                crate::ui::icons::icon_button(ui, cmd.icon(), &tooltip_of(cmd, &Keymap::builtin()));
            if let Some(probe) = probe.as_deref_mut() {
                probe(cmd, response.rect);
            }
            if response.clicked() {
                outbox.push(cmd.message());
            }
        }
    });
}

/// 按钮悬浮提示:命令名 + 出厂键位(键位可改,这里取的是出厂默认,够
/// 指路就够;改键在设置「快捷键」页)。
fn tooltip_of(cmd: Command, keymap: &Keymap) -> String {
    match keymap.get(cmd) {
        Some(shortcut) => format!("{}({})", cmd.label(), shortcut.platform_text()),
        None => cmd.label().to_owned(),
    }
}

/// 左栏次段的视图导航:四行竖排,**整行选中态** —— selected_bg 底 +
/// 左侧 2px 强调色竖条(照抄 tabs 的页签选中态,不用重boBox 再辨证)。
///
/// 点击只发 [`Message::SidebarTabChanged`],切换本身在归约。
fn view_nav(panel: &mut egui::Ui, active: SidebarTab, outbox: &mut Vec<Message>) {
    for tab in SidebarTab::ALL {
        let response = nav_row(panel, tab, tab == active);
        if response.clicked() {
            outbox.push(Message::SidebarTabChanged(tab));
        }
    }
}

/// 单条导航行:图标 + 文字占满整行宽。返回响应以便点击测试定位。
fn nav_row(ui: &mut egui::Ui, tab: SidebarTab, selected: bool) -> egui::Response {
    icon_label_row(
        ui,
        tab.icon(),
        tab.label(),
        selected,
        crate::ui::tokens::NAV_ROW_H,
    )
}

/// 图标 + 文字的整行按钮,高度可调(左栏次段视图导航用)。
///
/// 手绘而非 `Button` 是为了「整行选中态」:浅色底打满可用宽 + 左侧 2px
/// 强调色竖条(ui-polish 页签选中态的同款口径)。`Sense::click` 打在
/// `allocate_exact_size` 上而非某个子控件,因此整行任意位置都能点 —— 这
/// 也是它比起 `horizontal` 容器响应的差别:后者只有不可交互的 `label`,
/// 压根收不到点击。
fn icon_label_row(
    ui: &mut egui::Ui,
    icon: crate::ui::icons::Icon,
    label: &str,
    selected: bool,
    height: f32,
) -> egui::Response {
    let size = egui::vec2(ui.available_width().max(0.0), height);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let visuals = ui.visuals();
        // 整行选中态:selected_bg + 左侧 2px 竖条(与 tabs 的页签选中同款,
        // `widgets.hovered` 在浅色下恰好等于侧栏底色,选中行会整体消失);
        // hover 用内建 hover 底
        if selected {
            let accent = crate::ui::tokens::accent(ui);
            let selected_bg = crate::theme::shell_tokens(visuals.dark_mode).selected_bg;
            painter.rect_filled(rect, 0.0, selected_bg);
            painter.rect_filled(
                egui::Rect::from_min_size(
                    rect.left_top(),
                    egui::vec2(crate::ui::tokens::NAV_BAR_W, height),
                ),
                0.0,
                accent,
            );
        } else if response.hovered() {
            painter.rect_filled(rect, 0.0, visuals.widgets.hovered.bg_fill);
        }
        let color = if selected {
            crate::ui::tokens::accent(ui)
        } else {
            visuals.text_color()
        };
        let text_pos = egui::pos2(
            rect.left()
                + crate::ui::tokens::SPACE_SM
                + crate::ui::tokens::ICON_SM
                + crate::ui::tokens::SPACE_XS,
            rect.center().y,
        );
        icon.draw(
            painter,
            egui::pos2(
                rect.left() + crate::ui::tokens::SPACE_SM + crate::ui::tokens::ICON_SM / 2.0,
                rect.center().y,
            ),
            crate::ui::tokens::ICON_SM,
            color,
        );
        painter.text(
            text_pos,
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Button.resolve(ui.style()),
            color,
        );
    }
    response
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

/// Backlinks 页(#15):状态行 + 「谁链接了当前文档」列表。
///
/// 数据是 `BacklinkState` 的只读快照(触发与扫描都在归约侧,见
/// `crate::backlink_panel` 模块文档),本层零 IO;点击只发
/// [`Message::WikilinkClicked`],打开与切标签在归约的 `open_wikilink`。
/// 无根/文档未落盘/扫描中/无引用四种空态给弱提示。
fn backlinks_panel(
    panel: &mut egui::Ui,
    backlinks: &BacklinkState,
    root: Option<&Path>,
    current_file: Option<&Path>,
    outbox: &mut Vec<Message>,
) {
    let Some(_root) = root else {
        panel.weak("反向链接需要一个根目录:先在「文件」页选择");
        return;
    };
    let Some(_doc) = current_file else {
        panel.weak("保存文档后,这里会列出谁链接了它");
        return;
    };
    // 防抖排程中与进行中都算「扫描中」(300ms 窗口里面板不该空白)
    if backlinks.status == BacklinkStatus::Scanning || backlinks.debounce_due.is_some() {
        panel.weak("扫描中…");
    } else {
        match &backlinks.status {
            BacklinkStatus::Finished if backlinks.links.is_empty() => {
                panel.weak("没有文档链接到这里");
            }
            BacklinkStatus::Finished => {
                panel.weak(format!("共 {} 处引用", backlinks.links.len()));
            }
            BacklinkStatus::Failed(msg) => {
                panel.weak(format!("⚠ {msg}"));
            }
            BacklinkStatus::Idle | BacklinkStatus::Scanning => {}
        }
    }

    egui::ScrollArea::vertical()
        .id_salt("backlinks-scroll")
        .auto_shrink([false, false])
        .show(panel, |ui| {
            for link in &backlinks.links {
                backlink_row(ui, link, outbox);
            }
            if backlinks.truncated {
                ui.weak(format!("已达 {} 条上限,后续引用未显示", MAX_HITS));
            }
        });
}

/// 单条反向链接:第一行「相对根的来源路径:行号」,第二行命中行摘要;
/// 整块一个 SelectableLabel,点击发 [`Message::WikilinkClicked`] 走
/// `open_wikilink` 同一条跳转链路(已开切标签、缺失落提示)。返回行响应,
/// 独立成函数便于点击测试定位。
fn backlink_row(ui: &mut egui::Ui, link: &Backlink, outbox: &mut Vec<Message>) -> egui::Response {
    let text = format!(
        "{}:{}\n{}",
        link.path.display(),
        link.line_no,
        snippet(&link.line_text)
    );
    let response = ui.selectable_label(false, text);
    if response.clicked() {
        outbox.push(Message::WikilinkClicked {
            target: jump_target(&link.path),
        });
    }
    response
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
                        recent_row(ui, tree, dir, outbox);
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
        // 不收缩宽度:长文件名在行内省略号截断(#40),不换行也不撑宽面板
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

/// recents 下拉项:显示名只取末级文件夹名(`recent_label`,与 root_label
/// 同口径)——路径中间段不进 UI,用户目录名可能含个人信息(#32 I2);
/// 全路径下沉到该项的 hover tooltip,重名末级名靠 tooltip 区分。独立成
/// 函数与 tree_row 等同例,便于无头测试直接驱动(ComboBox 弹层交互不在
/// 无头覆盖范围,根目录选择走 state::tests)。
fn recent_row(
    ui: &mut egui::Ui,
    tree: &FileTreeState,
    dir: &Path,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let selected = tree.root.as_deref() == Some(dir);
    let response = ui
        .selectable_label(selected, recent_label(dir))
        .on_hover_text(dir.display().to_string());
    if response.clicked() {
        // ComboBox 默认 CloseOnClick:点击项后弹层自动收起
        outbox.push(Message::FileTreeRootSelected(dir.to_owned()));
    }
    response
}

/// 根目录行的显示名。
fn root_label(tree: &FileTreeState) -> String {
    tree.root
        .as_deref()
        .and_then(last_segment)
        .unwrap_or_else(|| "未选择根目录".to_owned())
}

/// 路径末级名(文件夹名);根路径 `/` 等无末段时为 None。
fn last_segment(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// recents 项显示名:末级文件夹名;末段拿不到(如根路径 `/`)时回退
/// 全路径——此时路径本身无中间段,不构成泄漏。
fn recent_label(dir: &Path) -> String {
    last_segment(dir).unwrap_or_else(|| dir.display().to_string())
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
    // 行首装饰(展开三角 + folder/file 图标)都不用文本字符:
    // U+25B8/U+25BE 在 Inter 与出厂字体链上均无字形,目录行名字前渲染成
    // 方框(用户实测,PR #53 起纯绘制)。行文本以前导空格占位(空格任何
    // 字体都有,目录/文件天然对齐):头两个空格给展开三角,其后空格数按
    // 图标槽宽度折算,名字从图标右侧起;三角与图标画在占位区,纯绘制不
    // 依赖字体——与 #31 行号槽同思路。
    let selected = !entry.is_dir && current_file == Some(entry.path.as_path());
    let badge = if entry.is_dir {
        None
    } else {
        git.badge_for(&entry.path)
    };
    let response = badged_row_label(ui, selected, tree_row_text(ui, &entry.name), badge)
        .on_hover_text(row_tooltip(&entry.name, badge));
    if entry.is_dir {
        paint_tree_arrow(ui, &response, open);
    }
    paint_tree_entry_icon(ui, &response, entry.is_dir, open);
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

/// 文件树行的显示文本:前导空格数由「三角槽(两个空格)+ 图标槽 + 间隙」
/// 与正文字体空格 advance 折算而来,再扣除 `selectable_label` 的框内边距
/// (`button_padding.x`,出厂 style 4px / 外壳 style 12px 两套都成立),
/// 保证名字首字不与图标重叠;下限两个空格保住三角的占位。
fn tree_row_text(ui: &egui::Ui, name: &str) -> String {
    let space_w = body_space_width(ui).max(0.1);
    let inset = ui.style().spacing.button_padding.x.max(0.0);
    let icon_right = 2.0 * space_w + crate::ui::tokens::SPACE_XS + crate::ui::tokens::ICON_SM;
    let pad =
        (((icon_right + crate::ui::tokens::SPACE_XS - inset) / space_w).ceil() as usize).max(2);
    format!("{}{}", " ".repeat(pad), name)
}

/// 行首 folder/file 图标(自绘,ui-polish §1.1):目录行随开合切换闭合/
/// 开口文件夹,文件行画折角纸页,槽位紧跟展开三角之后,目录/文件行纵向
/// 对齐。尺寸取 `ICON_SM`(页签档,行高内留上下边);颜色取非交互前景
/// (与三角同源随主题),hover/选中只改文本底色,图标几何不动。
fn paint_tree_entry_icon(ui: &egui::Ui, row: &egui::Response, is_dir: bool, open: bool) {
    if !ui.is_rect_visible(row.rect) {
        return;
    }
    let space_w = body_space_width(ui);
    let icon = if is_dir {
        if open {
            crate::ui::icons::Icon::FolderOpen
        } else {
            crate::ui::icons::Icon::FolderClosed
        }
    } else {
        crate::ui::icons::Icon::File
    };
    icon.draw(
        ui.painter(),
        egui::pos2(
            row.rect.left()
                + 2.0 * space_w
                + crate::ui::tokens::SPACE_XS
                + crate::ui::tokens::ICON_SM / 2.0,
            row.rect.center().y,
        ),
        crate::ui::tokens::ICON_SM,
        ui.visuals().widgets.noninteractive.fg_stroke.color,
    );
}

/// 正文空格的 advance(px)。展开三角与图标槽的定位基准:随正文字号
/// 缩放,不随平台字体走样(egui 的 galley 缓存让重复测量只是查表)。
fn body_space_width(ui: &egui::Ui) -> f32 {
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    ui.fonts_mut(|f| {
        f.layout_no_wrap(" ".to_owned(), font_id, egui::Color32::WHITE)
            .rect
            .width()
    })
}

/// 目录行的展开三角:画在行首两个空格占位区的中点上,收起朝右、展开
/// 朝下。尺寸基准是空格 advance(随正文字号缩放,不随平台字体走样),
/// 颜色取弱前景(与行号槽同源);纯 `Shape::convex_polygon`,零字形依赖。
fn paint_tree_arrow(ui: &egui::Ui, row: &egui::Response, open: bool) {
    let space_w = body_space_width(ui);
    let center = egui::pos2(row.rect.left() + space_w, row.rect.center().y);
    let r = space_w * 0.62;
    ui.painter().add(egui::Shape::convex_polygon(
        arrow_vertices(center, r, open),
        ui.visuals().widgets.noninteractive.fg_stroke.color,
        egui::Stroke::NONE,
    ));
}

/// 三角顶点:展开朝下、收起朝右。独立成纯函数,无头单测锁两个朝向的
/// 几何约定(箭头从无字形的文本字符改为纯绘制后的防回归)。
fn arrow_vertices(center: egui::Pos2, r: f32, open: bool) -> Vec<egui::Pos2> {
    if open {
        vec![
            center + egui::vec2(-r, -r * 0.8),
            center + egui::vec2(r, -r * 0.8),
            center + egui::vec2(0.0, r * 0.9),
        ]
    } else {
        vec![
            center + egui::vec2(-r * 0.8, -r),
            center + egui::vec2(-r * 0.8, r),
            center + egui::vec2(r * 0.9, 0.0),
        ]
    }
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
        // 文件名 + 视图切换同一行:文件名等宽,右侧「双栏/统一」两态
        // 切换(#53 M2),点击发消息、归约写 `git.diff_view`(本层只读)
        panel.horizontal(|ui| {
            ui.monospace(selected);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                diff_view_toggle(ui, git.diff_view, outbox);
            });
        });
        diff_area(panel, git);
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
    let response = badged_row_label(ui, selected, entry.path.clone(), Some(entry.code))
        .on_hover_text(row_tooltip(&entry.path, Some(entry.code)));
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

/// diff 区(#53 M2):按 `diff_view` 分流。双栏吃结构化 `diff_lines`;
/// 统一沿用既有 [`diff_view`] 文本路径,行为不变。双栏的兜底口径:结构化
/// 未就位(选中后取失败)回落统一文本渲染——错误文案、截断提示都在
/// `diff` 字符串里,可见性不降级(现状口径不破)。
fn diff_area(panel: &mut egui::Ui, git: &GitPanelState) {
    match git.diff_view {
        DiffView::Unified => unified_diff_area(panel, git),
        DiffView::Split => match &git.diff_lines {
            Some(file) if file.binary => {
                panel.weak(latermd_git::DIFF_BINARY_PLACEHOLDER);
            }
            // 空 hunks ⇔ 统一侧的空文本(两 API 同源);首行即超 64KB 的
            // 病态 diff 会空 hunks + truncated,走下面整行渲染 + 提示
            Some(file) if file.hunks.is_empty() && !file.truncated => {
                panel.weak("无文本改动");
            }
            Some(file) => split_diff_view(panel, file),
            None => unified_diff_area(panel, git),
        },
    }
}

/// 统一视图的空态判定(既有口径原样):空文本 = 无文本改动。
fn unified_diff_area(panel: &mut egui::Ui, git: &GitPanelState) {
    if git.diff.is_empty() {
        panel.weak("无文本改动");
    } else {
        diff_view(panel, &git.diff);
    }
}

/// 「双栏/统一」两态切换(#53 M2):当前态高亮,点击发
/// [`Message::GitDiffViewChanged`]。返回 (双栏, 统一) 的响应矩形供
/// 测试定位点击(right_to_left 布局里先加的在最右,视觉顺序「双栏 统一」)。
fn diff_view_toggle(
    ui: &mut egui::Ui,
    current: DiffView,
    outbox: &mut Vec<Message>,
) -> (egui::Rect, egui::Rect) {
    let unified = ui.selectable_label(
        current == DiffView::Unified,
        egui::RichText::new("统一").small(),
    );
    if unified.clicked() {
        outbox.push(Message::GitDiffViewChanged(DiffView::Unified));
    }
    let split = ui.selectable_label(
        current == DiffView::Split,
        egui::RichText::new("双栏").small(),
    );
    if split.clicked() {
        outbox.push(Message::GitDiffViewChanged(DiffView::Split));
    }
    (split.rect, unified.rect)
}

/// 双栏 diff 视图(#53 M2):GitHub 式左右对齐。左右各带行号列(右对齐)
/// 与 +/− 沟槽标记,整行底色增绿删红([`diff_row_bg`]),hunk 头 `@@`
/// 跨双栏整行;长行单行截断(…)+ 悬停看全文;行对上限
/// [`MAX_SPLIT_PAIRS`] 超限截断 + 显式提示,防大 diff 卡帧。行对来自
/// `git_split_diff` 纯配对层;取舍见 docs/decisions-pending #100。
fn split_diff_view(panel: &mut egui::Ui, file: &FileDiff) {
    let split = split_rows(file);
    egui::ScrollArea::vertical()
        .id_salt("git-diff-split-scroll")
        .auto_shrink([false, false])
        .max_height(200.0)
        .show(panel, |ui| {
            let geom = RowGeometry::of(ui, file);
            for row in &split.rows {
                paint_split_row(ui, row, &geom);
            }
        });
    split_overflow_hints(panel, file, split.dropped_pairs);
}

/// 双栏视图的截断提示行(M1 的 ~64KB 行级截断 + M2 的行对上限),显式
/// 告知内容不完整;返回所画提示的合并矩形(无提示返回 `None`,测试断言
/// 用)。与统一视图「提示在截断文本尾部」同语义,双栏的结构化数据源
/// 没有尾部可拼,独立成行。
fn split_overflow_hints(
    ui: &mut egui::Ui,
    file: &FileDiff,
    dropped_pairs: usize,
) -> Option<egui::Rect> {
    let mut rect = None;
    if file.truncated {
        rect = Some(ui.weak(latermd_git::DIFF_TRUNCATION_PLACEHOLDER).rect);
    }
    if dropped_pairs > 0 {
        let hint = ui.weak(format!(
            "…还有 {dropped_pairs} 行对未显示(双栏视图上限 {MAX_SPLIT_PAIRS})"
        ));
        rect = Some(match rect {
            Some(previous) => previous.union(hint.rect),
            None => hint.rect,
        });
    }
    rect
}

/// 一帧双栏渲染的公共几何(等宽字体、行高、行号列宽、沟槽宽)。行号列
/// 宽按双侧最大行号的位数 × 数字宽计算(`ui::gutter` 同款,位数跨档才
/// 变宽,右对齐不抖)。
struct RowGeometry {
    /// 等宽字体(Monospace 档,#50 的 editor-mono 族经投影同源)。
    font: egui::FontId,
    /// 单行行高(行盒恒定,配对行不因内容折行变高——长行走截断)。
    row_height: f32,
    /// 行号列宽(含右侧留白)。
    gutter_w: f32,
    /// +/− 沟槽列宽。
    sign_w: f32,
    /// 行号前景(弱化)。
    weak: egui::Color32,
}

impl RowGeometry {
    fn of(ui: &egui::Ui, file: &FileDiff) -> Self {
        let font = egui::FontSelection::Style(egui::TextStyle::Monospace).resolve(ui.style());
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let digit = ui.fonts_mut(|fonts| fonts.glyph_width(&font, '0'));
        // 双侧最大行号:hunk 头的范围和就是行号覆盖上界
        let max_lineno = file.hunks.iter().fold(0u32, |acc, hunk| {
            acc.max(hunk.old_start.saturating_add(hunk.old_lines))
                .max(hunk.new_start.saturating_add(hunk.new_lines))
        });
        let digits = digit_count(max_lineno);
        Self {
            gutter_w: digits as f32 * digit + 2.0 * SPACE_XS,
            sign_w: digit + SPACE_XS,
            font,
            row_height,
            weak: ui.visuals().weak_text_color(),
        }
    }
}

/// 总行号 → 十进制位数(0 至少 1 位;`ui::gutter` 同款算法,双栏行号列
/// 自用一份——gutter 的是 crate 私有)。
fn digit_count(mut n: u32) -> usize {
    let mut digits = 1;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits
}

/// 双栏行底色:语义色(增 [`tokens::OK`]/删 [`tokens::DANGER`])向面板
/// 底色(`visuals().panel_fill`)的伽马插值,**单一公式**明暗两主题各自
/// 成立——明色下 ≈ GitHub #e6ffec/#ffebe0 的淡着色观感,暗色自然得到
/// 深绿/深红,不是两套硬编码取色。浓淡只由 [`DIFF_ROW_TINT`] 一个常量
/// 控制。
fn diff_row_bg(ui: &egui::Ui, tint: egui::Color32) -> egui::Color32 {
    tint.lerp_to_gamma(ui.visuals().panel_fill, 1.0 - DIFF_ROW_TINT)
}

/// 行底色的语义色占比(0..1):16% 增/删语义色 + 84% 面板底色,明暗主题
/// 下都在「可读出着色语义」与「不压正文对比」之间(数值推断,真机目视
/// 留人工,见 #53 notes)。
const DIFF_ROW_TINT: f32 = 0.16;

/// 双栏一侧(左 = 旧侧,右 = 新侧)。
enum DiffSide {
    /// 左栏:上下文行与删除行。
    Old,
    /// 右栏:上下文行与新增行。
    New,
}

/// 画双栏的一行:hunk 头跨整行(淡底 + `@@` 蓝),行对左右各画一侧。
/// 布局始终推进(行盒恒定),绘制做视口裁剪(`is_rect_visible`,与
/// `ui::gutter` 同精神)。返回行响应(悬停全文挂在其上,测试用它取行
/// 矩形)。
fn paint_split_row(ui: &mut egui::Ui, row: &SplitRow<'_>, geom: &RowGeometry) -> egui::Response {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, geom.row_height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    match row {
        SplitRow::Header(hunk) => {
            ui.painter()
                .rect_filled(rect.shrink(0.5), RADIUS_SM, ui.visuals().faint_bg_color);
            ui.painter().text(
                egui::pos2(rect.left() + SPACE_XS, rect.center().y),
                egui::Align2::LEFT_CENTER,
                hunk_header_text(hunk),
                geom.font.clone(),
                DIFF_HUNK,
            );
        }
        SplitRow::Pair { old, new } => {
            let half = rect.width() / 2.0;
            let left = egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + half, rect.bottom()),
            );
            let right = egui::Rect::from_min_max(
                egui::pos2(rect.left() + half, rect.top()),
                rect.right_bottom(),
            );
            let elided_old = paint_split_side(ui, left, *old, DiffSide::Old, geom);
            let elided_new = paint_split_side(ui, right, *new, DiffSide::New, geom);
            // 长行被截断的一侧:悬停看全文(整行响应挂 tooltip,两则并排)
            match (elided_old, elided_new) {
                (None, None) => {}
                (old_text, new_text) => {
                    let mut tooltip = String::new();
                    if let Some(text) = old_text {
                        tooltip.push_str(&format!("− {text}\n"));
                    }
                    if let Some(text) = new_text {
                        tooltip.push_str(&format!("+ {text}"));
                    }
                    return response.on_hover_text(tooltip.trim_end());
                }
            }
        }
    }
    response
}

/// 画双栏一侧:整侧底色(删除红/新增绿)+ 行号(右对齐弱化)+ 沟槽标记
/// (+/−)+ 正文(等宽、单行截断)。返回被截断一侧的全文(悬停提示用;
/// 未截断/空侧返回 `None`)。行内不做 byte 切:截断由 epaint 的 elision
/// 在字形层完成,char 边界天然安全。
fn paint_split_side(
    ui: &egui::Ui,
    cell: egui::Rect,
    line: Option<&DiffLine>,
    side: DiffSide,
    geom: &RowGeometry,
) -> Option<String> {
    let line = line?;
    // 配对层保证:左栏只收上下文/删除行,右栏只收上下文/新增行
    let (lineno, marker, bg) = match (side, line.kind) {
        (DiffSide::Old, DiffLineKind::Context) => (line.old_lineno, None, None),
        (DiffSide::Old, DiffLineKind::Deleted) => (
            line.old_lineno,
            Some(('−', DIFF_REMOVED)),
            Some(diff_row_bg(ui, tokens::DANGER)),
        ),
        (DiffSide::New, DiffLineKind::Context) => (line.new_lineno, None, None),
        (DiffSide::New, DiffLineKind::Added) => (
            line.new_lineno,
            Some(('+', DIFF_ADDED)),
            Some(diff_row_bg(ui, tokens::OK)),
        ),
        // 删除行画右栏/新增行画左栏:配对层不产生,防御跳过
        _ => return None,
    };
    let painter = ui.painter();
    if let Some(bg) = bg {
        painter.rect_filled(cell, 0.0, bg);
    }
    // 行号:右对齐贴行号列右缘(空侧 = 空串,不画)
    let lineno_text = line_no_text(lineno);
    if !lineno_text.is_empty() {
        painter.text(
            egui::pos2(cell.left() + geom.gutter_w - SPACE_XS, cell.center().y),
            egui::Align2::RIGHT_CENTER,
            lineno_text,
            geom.font.clone(),
            geom.weak,
        );
    }
    // 沟槽标记:行号列右侧一格
    if let Some((marker, color)) = marker {
        painter.text(
            egui::pos2(cell.left() + geom.gutter_w, cell.center().y),
            egui::Align2::LEFT_CENTER,
            marker.to_string(),
            geom.font.clone(),
            color,
        );
    }
    // 正文:单行截断(…);悬停全文由调用方挂行响应
    let text_width = (cell.width() - geom.gutter_w - geom.sign_w - SPACE_XS).max(0.0);
    let mut job = egui::text::LayoutJob::simple(
        line.text.clone(),
        geom.font.clone(),
        ui.visuals().text_color(),
        text_width,
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let pos = egui::pos2(
        cell.left() + geom.gutter_w + geom.sign_w,
        cell.center().y - galley.size().y / 2.0,
    );
    let elided = galley.elided;
    painter.galley(pos, galley, ui.visuals().text_color());
    elided.then(|| line.text.clone())
}

/// hunk 头文本:`@@ -old_start,old_lines +new_start,new_lines @@`(与
/// unified 输出同格式;单行范围也带计数,不追 git 的省略逗号形态)。
fn hunk_header_text(hunk: &DiffHunk) -> String {
    format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    )
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

/// 「正文 + 彩色单字母角标」的可选中行,文件树文件行与 Git 页改动行共用。
///
/// 单行截断(#40):两态(有/无角标)都走 `TextWrapMode::Truncate`——窄栏里
/// 名字省略号截尾不换行,行高恒定;角标经 `Button::right_text`(内部
/// `Atom::grow`)贴行尾,名字的截断宽度先扣除角标固有宽,省略号永远挤
/// 不掉角标。选中/悬停样式与 `ui.selectable_label` 同源——后者在 egui 0.36
/// 就是 `Button::selectable(..).ui(..)` 的别名,本函数只是同一颗 Button 多
/// 挂了截断与右侧原子。
fn badged_row_label(
    ui: &mut egui::Ui,
    selected: bool,
    text: String,
    badge: Option<StatusKind>,
) -> egui::Response {
    let mut button = egui::Button::selectable(selected, text).truncate();
    if let Some(kind) = badge {
        button =
            button.right_text(egui::RichText::new(format!("{kind}")).color(status_color(kind)));
    }
    ui.add(button)
}

/// 行 hover tooltip(#40):完整文件名——截断行由此看全名(recents 下拉的
/// 先例同款口径);带 Git 角标的行附状态说明,角标字母不必心算。
fn row_tooltip(name: &str, badge: Option<StatusKind>) -> String {
    match badge {
        Some(kind) => format!("{name}({})", status_label(kind)),
        None => name.to_owned(),
    }
}

/// Git 角标字母的中文说明(M/A/U/D/?)。
fn status_label(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Modified => "已修改",
        StatusKind::Added => "已暂存",
        StatusKind::Unmerged => "冲突",
        StatusKind::Deleted => "已删除",
        StatusKind::Untracked => "未跟踪",
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

    /// 展开三角几何:展开朝下、收起朝右。行首箭头原是文本字符
    /// (U+25B8/U+25BE 在 Inter 与出厂字体链无字形,目录行渲染成方框),
    /// 改为纯绘制后此测试锁住两个朝向的顶点约定。
    #[test]
    fn tree_arrow_vertices_point_in_direction() {
        let c = egui::pos2(10.0, 10.0);
        let r = 2.0;
        let open = arrow_vertices(c, r, true);
        let closed = arrow_vertices(c, r, false);
        assert!(
            open.iter().any(|p| p.y > c.y) && open.iter().all(|p| p.y <= c.y + r * 0.9 + 1e-6),
            "展开三角下指:越过中心下方的顶点存在且不越过顶点"
        );
        assert!(
            closed.iter().any(|p| p.x > c.x) && closed.iter().all(|p| p.x <= c.x + r * 0.9 + 1e-6),
            "收起三角右指:越过中心右侧的顶点存在且不越过顶点"
        );
    }

    /// 快照层护栏(PR #53 之前的缺陷回归):行首的展开三角、folder/file
    /// 图标必须是矢量 Shape,文本层只有名字(与 Git 角标);▸/▾ 之类图标
    /// 字符一旦回流文本层,断言当场红。同帧顺带量几何:装饰(三角 + 图标)
    /// 的右沿不得越过名字首字的墨迹左沿(`mesh_bounds` 不含前导空格,量
    /// 的是真实墨迹)。明暗两套 visuals 各跑一遍。
    #[test]
    fn tree_row_decorations_are_painted_not_glyph_text() {
        let (tree, root) = sample_tree();
        let children = tree.children.get(&root).unwrap();
        for dark in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            for entry in &children.entries {
                let output = ctx.run_ui(RawInput::default(), |ui| {
                    tree_row(
                        ui,
                        &tree,
                        entry,
                        None,
                        &GitPanelState::default(),
                        &mut Vec::new(),
                    );
                });
                let mut texts = Vec::new();
                let mut decor_right = f32::MIN;
                let mut name_left = f32::MAX;
                for clipped in &output.shapes {
                    match &clipped.shape {
                        egui::epaint::Shape::Text(text) => {
                            texts.push(text.galley.job.text.clone());
                            name_left = name_left.min(text.visual_bounding_rect().left());
                        }
                        shape => {
                            decor_right = decor_right.max(shape.visual_bounding_rect().right())
                        }
                    }
                }
                assert!(
                    texts.iter().any(|t| t.contains(&entry.name)),
                    "{} 名字应仍在文本层:{texts:?}",
                    entry.name
                );
                assert!(
                    texts.iter().all(|t| {
                        !t.contains('▸')
                            && !t.contains('▾')
                            && !t.contains('📁')
                            && !t.contains('📄')
                    }),
                    "图标字符不得回流文本层:{texts:?}"
                );
                assert!(
                    decor_right < name_left,
                    "{} 行首装饰右沿 {decor_right} 不得压住名字墨迹左沿 {name_left}",
                    entry.name
                );
                output.drop_without_applying_deltas();
            }
        }
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

    /// 一帧里全部文本 shape 的字符串:可见文本与 tooltip 都算(断言按
    /// 内容区分,不区分层)。
    fn shape_texts(output: &egui::FullOutput) -> Vec<String> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect()
    }

    /// 一帧里全部文本 shape 的**可见**字符串:按 glyph 层逐字拼接(#40 起
    /// 截断行走 elide 路径,`galley.job.text` 恒保留完整原文——Label 的
    /// elided-tooltip 正是靠它;省略号 `…` 在字形层是真实字符)。可见层
    /// 断言(截没截、截断后的样子)必须用这份,不能拿 job.text 恒真。
    fn visible_shape_texts(output: &egui::FullOutput) -> Vec<String> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text) => Some(
                    text.galley
                        .rows
                        .iter()
                        .map(|row| row.row.text())
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }

    /// recents 项显示名(#32 I2):只取末级文件夹名——注入带个人称谓
    /// 中间段的假路径,「坤哥」不得进显示名;重名末级在显示层不消歧
    /// (靠 tooltip 区分);根路径无末段时回退全路径(自身无中间段)。
    #[test]
    fn recent_label_uses_last_segment() {
        assert_eq!(recent_label(Path::new("/home/坤哥/工作/vault")), "vault");
        assert_eq!(
            recent_label(Path::new("/data/坤哥/备份/vault")),
            "vault",
            "重名末级在显示层不做消歧"
        );
        assert_eq!(recent_label(Path::new("/")), "/");
    }

    /// recents 下拉项的可见文本与 hover tooltip(#32 I2):无指针帧的
    /// 文本层只有末级名,路径中间段(坤哥/工作/备份)不得出现;悬停并
    /// 推过 egui 的 tooltip 延迟(默认 0.5s 且指针须静止,`RawInput.time`
    /// 逐帧推进)后,全路径只以 tooltip 文本出现,重名末级由此区分。
    #[test]
    fn recents_show_last_segment_with_full_path_on_hover() {
        let tree = FileTreeState {
            recents: vec![
                PathBuf::from("/home/坤哥/工作/vault"),
                PathBuf::from("/data/坤哥/备份/vault"),
            ],
            ..FileTreeState::default()
        };
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let rects = [Cell::new(Rect::NOTHING), Cell::new(Rect::NOTHING)];
        let render = |ui: &mut egui::Ui, outbox: &mut Vec<Message>| {
            for dir in &tree.recents {
                recent_row(ui, &tree, dir, outbox);
            }
        };

        // 帧 1(t=0,无指针):可见文本层只有末级名
        let output = ctx.run_ui(
            RawInput {
                time: Some(0.0),
                ..Default::default()
            },
            |ui| {
                for (dir, rect) in tree.recents.iter().zip(&rects) {
                    rect.set(recent_row(ui, &tree, dir, &mut outbox).rect);
                }
            },
        );
        let texts = shape_texts(&output);
        output.drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");
        assert!(
            texts.iter().filter(|t| t.contains("vault")).count() >= 2,
            "两条 recents 都显示末级名:{texts:?}"
        );
        assert!(
            texts.iter().all(|t| !t.contains('坤')),
            "路径中间段不得进可见文本:{texts:?}"
        );

        // 帧 2(t=1):指针移入第一条;tooltip 有静止延迟,本帧不显示
        let center = rects[0].get().center();
        ctx.run_ui(
            RawInput {
                time: Some(1.0),
                events: vec![Event::PointerMoved(center)],
                ..Default::default()
            },
            |ui| render(ui, &mut outbox),
        )
        .drop_without_applying_deltas();

        // 帧 3-4(t=2,3):静止超延迟,悬停项的全路径只在 tooltip 里出现
        let mut tooltip_texts = Vec::new();
        for time in [2.0, 3.0] {
            let output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| render(ui, &mut outbox),
            );
            tooltip_texts.extend(shape_texts(&output));
            output.drop_without_applying_deltas();
        }
        assert!(
            tooltip_texts
                .iter()
                .any(|t| t.contains("/home/坤哥/工作/vault")),
            "悬停项的全路径进 tooltip:{tooltip_texts:?}"
        );

        // 帧 5-7:指针移到第二条(重名末级),tooltip 换成它自己的全路径
        let center = rects[1].get().center();
        ctx.run_ui(
            RawInput {
                time: Some(4.0),
                events: vec![Event::PointerMoved(center)],
                ..Default::default()
            },
            |ui| render(ui, &mut outbox),
        )
        .drop_without_applying_deltas();
        let mut second_texts = Vec::new();
        for time in [5.0, 6.0] {
            let output = ctx.run_ui(
                RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| render(ui, &mut outbox),
            );
            second_texts.extend(shape_texts(&output));
            output.drop_without_applying_deltas();
        }
        assert!(
            second_texts
                .iter()
                .any(|t| t.contains("/data/坤哥/备份/vault")),
            "重名末级靠各自 tooltip 区分:{second_texts:?}"
        );
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

    /// 反向链接面板渲染矩阵(#15):无根/未落盘/扫描中(进行中与防抖排程)/
    /// 无引用/失败各渲染一帧不 panic,弱提示进文本层;有结果时来源文件名、
    /// 行号与摘要可见,超长摘要按字符截断加省略号。
    #[test]
    fn backlinks_panel_renders_states_without_panic() {
        let ctx = egui::Context::default();
        let root = PathBuf::from("/vault");
        let doc = root.join("note.md");

        // 渲染矩阵的单一场景:根/文档是否存在 + 面板状态 + 期望弱提示
        struct Case<'a> {
            name: &'static str,
            root: Option<&'a Path>,
            doc: Option<&'a Path>,
            state: BacklinkState,
            hint: &'static str,
        }
        let cases = vec![
            Case {
                name: "无根",
                root: None,
                doc: Some(&doc),
                state: BacklinkState::default(),
                hint: "反向链接需要一个根目录",
            },
            Case {
                name: "未落盘",
                root: Some(&root),
                doc: None,
                state: BacklinkState::default(),
                hint: "保存文档后",
            },
            Case {
                name: "扫描中",
                root: Some(&root),
                doc: Some(&doc),
                state: BacklinkState {
                    status: BacklinkStatus::Scanning,
                    ..BacklinkState::default()
                },
                hint: "扫描中",
            },
            Case {
                name: "防抖排程中",
                root: Some(&root),
                doc: Some(&doc),
                state: BacklinkState {
                    debounce_due: Some(
                        std::time::Instant::now() + std::time::Duration::from_secs(1),
                    ),
                    ..BacklinkState::default()
                },
                hint: "扫描中",
            },
            Case {
                name: "无引用",
                root: Some(&root),
                doc: Some(&doc),
                state: BacklinkState {
                    status: BacklinkStatus::Finished,
                    ..BacklinkState::default()
                },
                hint: "没有文档链接到这里",
            },
            Case {
                name: "失败",
                root: Some(&root),
                doc: Some(&doc),
                state: BacklinkState {
                    status: BacklinkStatus::Failed("线程没起来".to_owned()),
                    ..BacklinkState::default()
                },
                hint: "⚠ 线程没起来",
            },
        ];
        for case in cases {
            let output = ctx.run_ui(RawInput::default(), |ui| {
                backlinks_panel(ui, &case.state, case.root, case.doc, &mut Vec::new());
            });
            let texts = shape_texts(&output);
            output.drop_without_applying_deltas();
            assert!(
                texts.iter().any(|t| t.contains(case.hint)),
                "{}:弱提示应进文本层:{texts:?}",
                case.name
            );
        }

        // 有结果:来源路径:行号 + 摘要可见;超长摘要截断(带省略号)。
        let mut state = BacklinkState {
            status: BacklinkStatus::Finished,
            truncated: true,
            ..BacklinkState::default()
        };
        state.links.push(Backlink {
            path: PathBuf::from("sub/a.md"),
            line_no: 2,
            target: "note".to_owned(),
            line_text: "界".repeat(SNIPPET_MAX_CHARS + 30),
        });
        let output = ctx.run_ui(RawInput::default(), |ui| {
            backlinks_panel(ui, &state, Some(&root), Some(&doc), &mut Vec::new());
        });
        let texts = shape_texts(&output);
        output.drop_without_applying_deltas();
        assert!(
            texts.iter().any(|t| t.contains("sub/a.md:2")),
            "来源路径与行号可见:{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains('…')),
            "超长摘要截断:{texts:?}"
        );
        assert!(
            texts
                .iter()
                .all(|t| t.matches('界').count() <= SNIPPET_MAX_CHARS),
            "长行的摘要按字符截断,不整段进文本层:{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("上限")),
            "截断提示行可见:{texts:?}"
        );
    }

    /// 点击反向链接行:发出携带跳转目标串的 WikilinkClicked(来源相对
    /// 路径剥 md 后缀);仅渲染不产生消息。
    #[test]
    fn clicking_backlink_row_sends_wikilink_message() {
        let ctx = egui::Context::default();
        let link = Backlink {
            path: PathBuf::from("sub/b.md"),
            line_no: 1,
            target: "note".to_owned(),
            line_text: "子目录来源 [[note.md]]".to_owned(),
        };
        let mut outbox = Vec::new();
        let rect = Cell::new(Rect::NOTHING);

        ctx.run_ui(RawInput::default(), |ui| {
            rect.set(backlink_row(ui, &link, &mut outbox).rect);
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");

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
                backlink_row(ui, &link, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::WikilinkClicked {
                target: "sub/b".to_owned()
            }],
            "点击走 WikilinkClicked,载荷是剥后缀的来源相对路径"
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

    /// #53 M2 探针共享 fixture:1 个 hunk(上下文行 + 删除行 + 新增行)。
    /// 配对层产出 = hunk 头 + 上下文对 + (删除|新增) 同对。
    fn split_fixture() -> FileDiff {
        FileDiff {
            hunks: vec![DiffHunk {
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: vec![
                    DiffLine {
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                        kind: DiffLineKind::Context,
                        text: "上下文".to_owned(),
                    },
                    DiffLine {
                        old_lineno: Some(2),
                        new_lineno: None,
                        kind: DiffLineKind::Deleted,
                        text: "旧".to_owned(),
                    },
                    DiffLine {
                        old_lineno: None,
                        new_lineno: Some(2),
                        kind: DiffLineKind::Added,
                        text: "新".to_owned(),
                    },
                ],
            }],
            binary: false,
            truncated: false,
        }
    }

    /// #53 M2 双栏渲染探针(像素采样,#38/#50 先例):已知 diff 逐行画进
    /// 无头帧,帧后 tessellate 取最终覆盖色——(删除|新增) 对左半红系、
    /// 右半绿系、上下文行两侧无整行底色、行号列有墨;行对数与行高恒定。
    /// 明暗两主题各跑一轮(不 panic + 语义都成立)。
    #[test]
    fn split_diff_rows_paint_semantic_backgrounds_in_both_themes() {
        use crate::preview_pixel_acceptance::{color_dist, final_covered_color};
        use egui::epaint::Mesh;
        use egui::UiBuilder;

        let file = split_fixture();
        let split = crate::git_split_diff::split_rows(&file);
        for dark in [true, false] {
            let ctx = egui::Context::default();
            ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
            // 关羽化(#41 同款):透明渐变边缘会污染采样读色
            ctx.options_mut(|o| o.tessellation_options.feathering = false);
            let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(360.0, 300.0));
            let mut rects: Vec<Rect> = Vec::new();
            let mut gutter_w = 0.0;
            let mut panel_fill = egui::Color32::BLACK;
            let mut output = ctx.run_ui(RawInput::default(), |panel| {
                panel_fill = panel.visuals().panel_fill;
                let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
                let geom = RowGeometry::of(&ui, &file);
                gutter_w = geom.gutter_w;
                for row in &split.rows {
                    rects.push(paint_split_row(&mut ui, row, &geom).rect);
                }
            });
            let clipped = std::mem::take(&mut output.shapes);
            let primitives = ctx.tessellate(clipped, 1.0);
            output.drop_without_applying_deltas();
            let meshes: Vec<&Mesh> = primitives
                .iter()
                .filter_map(|cp| match &cp.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .collect();
            let theme = if dark { "暗色" } else { "亮色" };

            // 行对数与行盒:1 hunk 头 + 上下文对 + (删除|新增) 对,行高恒定
            assert_eq!(rects.len(), 3, "{theme}:hunk 头 + 2 行对");
            let row_h = rects[0].height();
            assert!(row_h > 0.0);
            assert!(
                rects.iter().all(|r| r.height() == row_h),
                "{theme}:行盒恒定(长行走截断,不折行)"
            );

            // 底色采样:每半的近右缘(远离行号/沟槽/短正文,纯底色区)
            let sample = |p: egui::Pos2| final_covered_color(&meshes, p);
            let edge = |row: Rect, half: f32, right: bool| {
                let x = if right {
                    row.left() + 2.0 * half - 6.0
                } else {
                    row.left() + half - 6.0
                };
                sample(egui::pos2(x, row.center().y))
            };
            // 上下文对:两侧都不画整行底(露画布或面板底色)
            let context = rects[1];
            let half = context.width() / 2.0;
            for right in [false, true] {
                assert!(
                    edge(context, half, right).is_none_or(|c| color_dist(c, panel_fill) <= 12),
                    "{theme}:上下文行不该有语义底色({right:?})"
                );
            }
            // (删除|新增) 对:左半红系、右半绿系,都区别于面板底色
            let pair = rects[2];
            let half = pair.width() / 2.0;
            let left_bg = edge(pair, half, false).expect("删除侧画了整行底");
            let right_bg = edge(pair, half, true).expect("新增侧画了整行底");
            assert!(
                color_dist(left_bg, panel_fill) > 35,
                "{theme}:删除侧红系着色 {left_bg:?} vs {panel_fill:?}"
            );
            assert!(
                i32::from(left_bg.r()) - i32::from(left_bg.g()) > 8,
                "{theme}:删除侧偏红 {left_bg:?}"
            );
            assert!(
                color_dist(right_bg, panel_fill) > 35,
                "{theme}:新增侧绿系着色 {right_bg:?} vs {panel_fill:?}"
            );
            assert!(
                i32::from(right_bg.g()) - i32::from(right_bg.r()) > 8,
                "{theme}:新增侧偏绿 {right_bg:?}"
            );

            // 行号列有墨:(删除|新增) 对两侧行号槽(右对齐数字)至少一个采样
            // 点被文本覆盖,且颜色可与本侧底色区分
            for right in [false, true] {
                let cell_left = pair.left() + if right { half } else { 0.0 };
                let side_bg = if right { right_bg } else { left_bg };
                let mut ink = false;
                let mut x = cell_left + 4.0;
                while x <= cell_left + gutter_w - 2.0 {
                    if let Some(color) = sample(egui::pos2(x, pair.center().y)) {
                        if color_dist(color, side_bg) > 40 {
                            ink = true;
                            break;
                        }
                    }
                    x += 2.0;
                }
                assert!(ink, "{theme}:行号列应有墨(right={right})");
            }
        }
    }

    /// #53 M2 视图切换:双栏/统一两视图都渲染非空;结构化通道未就位
    /// (`diff_lines = None`)时双栏回落统一文本,错误文案在 diff 区仍可见
    /// (现状口径不破);空 hunks 双栏给「无文本改动」。
    #[test]
    fn diff_area_renders_nonempty_in_both_views_with_fallback() {
        use egui::UiBuilder;

        let mut git = GitPanelState::default();
        git.entries.push(FileStatus {
            path: "a.md".to_owned(),
            code: StatusKind::Modified,
        });
        git.selected = Some("a.md".to_owned());
        git.diff = "@@ -1,2 +1,2 @@\n-旧\n+新\n".to_owned();
        git.diff_lines = Some(split_fixture());

        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(360.0, 600.0));
        let mut heights = Vec::new();
        for view in [DiffView::Split, DiffView::Unified] {
            git.diff_view = view;
            let mut height = 0.0;
            ctx.run_ui(RawInput::default(), |panel| {
                let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
                diff_area(&mut ui, &git);
                height = ui.min_rect().height();
            })
            .drop_without_applying_deltas();
            heights.push(height);
        }
        assert!(heights[0] > 20.0, "双栏渲染非空:{}", heights[0]);
        assert!(heights[1] > 20.0, "统一渲染非空:{}", heights[1]);

        // 结构化取失败(diff_lines=None)+ diff=错误文案:回落统一文本渲染
        git.diff_view = DiffView::Split;
        git.diff_lines = None;
        git.diff = "生成 diff 失败: 病态仓库".to_owned();
        let mut height = 0.0;
        ctx.run_ui(RawInput::default(), |panel| {
            let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
            diff_area(&mut ui, &git);
            height = ui.min_rect().height();
        })
        .drop_without_applying_deltas();
        assert!(height > 10.0, "错误文案在双栏视图仍可见:{}", height);

        // 空 hunks(无文本改动):「无文本改动」占位,不 panic
        git.diff_lines = Some(FileDiff {
            hunks: Vec::new(),
            binary: false,
            truncated: false,
        });
        ctx.run_ui(RawInput::default(), |panel| {
            let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
            diff_area(&mut ui, &git);
        })
        .drop_without_applying_deltas();
    }

    /// #53 M2 切换按钮:点击「双栏/统一」发 [`Message::GitDiffViewChanged`],
    /// 当前态高亮由 `selectable_label` 自带;仅渲染不产消息。
    #[test]
    fn diff_view_toggle_clicks_send_messages() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let rects = Cell::new((Rect::NOTHING, Rect::NOTHING));

        // 帧 1:当前统一(双栏未选中)——仅渲染不产消息,顺带拿按钮矩形
        ctx.run_ui(RawInput::default(), |ui| {
            rects.set(diff_view_toggle(ui, DiffView::Unified, &mut outbox));
        })
        .drop_without_applying_deltas();
        assert!(outbox.is_empty(), "仅渲染不产生消息");
        let (split_rect, unified_rect) = rects.get();
        assert!(split_rect.width() > 0.0 && unified_rect.width() > 0.0);

        // 帧 2:点「双栏」→ GitDiffViewChanged(Split)
        let center = split_rect.center();
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
                diff_view_toggle(ui, DiffView::Unified, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::GitDiffViewChanged(DiffView::Split)],
            "{outbox:?}"
        );

        // 帧 3:点「统一」(当前已是双栏)→ GitDiffViewChanged(Unified)
        outbox.clear();
        let center = unified_rect.center();
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
                diff_view_toggle(ui, DiffView::Split, &mut outbox);
            },
        )
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::GitDiffViewChanged(DiffView::Unified)],
            "{outbox:?}"
        );
    }

    /// #53 M2 行对上限:超限的大 diff 双栏渲染不 panic(视口裁剪),显式
    /// 提示行非空;未超限且 M1 未截断时无提示。
    #[test]
    fn split_view_cap_renders_hint_and_big_diff_does_not_panic() {
        use egui::UiBuilder;

        // MAX+5 行上下文 → 配对层截到 MAX、dropped 5(纯层已钉,此处验渲染)
        let file = FileDiff {
            hunks: vec![DiffHunk {
                old_start: 1,
                old_lines: (MAX_SPLIT_PAIRS + 5) as u32,
                new_start: 1,
                new_lines: (MAX_SPLIT_PAIRS + 5) as u32,
                lines: (1..=(MAX_SPLIT_PAIRS + 5) as u32)
                    .map(|n| DiffLine {
                        old_lineno: Some(n),
                        new_lineno: Some(n),
                        kind: DiffLineKind::Context,
                        text: format!("行{n}"),
                    })
                    .collect(),
            }],
            binary: false,
            truncated: false,
        };
        let split = crate::git_split_diff::split_rows(&file);
        assert_eq!(split.dropped_pairs, 5, "纯层口径先验一把");

        let ctx = egui::Context::default();
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(360.0, 300.0));
        let mut height = 0.0;
        let mut hint = None;
        ctx.run_ui(RawInput::default(), |panel| {
            let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
            split_diff_view(&mut ui, &file);
            height = ui.min_rect().height();
            hint = split_overflow_hints(&mut ui, &file, split.dropped_pairs);
        })
        .drop_without_applying_deltas();
        assert!(height > 100.0, "2000 行对的大 diff 渲染出内容:{}", height);
        let hint = hint.expect("超限必有提示行");
        assert!(hint.width() > 0.0 && hint.height() > 0.0);

        // 未超限且 M1 未截断:无提示行
        let small = split_fixture();
        let mut none = None;
        ctx.run_ui(RawInput::default(), |panel| {
            let mut ui = panel.new_child(UiBuilder::new().max_rect(screen));
            none = split_overflow_hints(&mut ui, &small, 0);
        })
        .drop_without_applying_deltas();
        assert!(none.is_none(), "未截断不该有提示");
    }

    /// #40 单行截断:窄栏(120px)下长中文文件名行不换行——行高与同栏
    /// 短名目录行、宽栏同名行一致恒定,名字文本以省略号截尾且全名不进
    /// 可见层;Git 角标仍可见、画在名字右侧且贴行尾(名字的截断宽度先
    /// 扣除角标固有宽,省略号挤不掉角标);宽栏下全名完整显示。
    #[test]
    fn tree_row_truncates_long_names_in_narrow_panel() {
        let long_name = format!("{}笔记.md", "超长中文文件名".repeat(3));
        let root = PathBuf::from("/vault");
        let mut tree = FileTreeState {
            root: Some(root.clone()),
            ..FileTreeState::default()
        };
        let dir = TreeEntry {
            path: root.join("docs"),
            name: "docs".to_owned(),
            is_dir: true,
        };
        let file = TreeEntry {
            path: root.join(&long_name),
            name: long_name.clone(),
            is_dir: false,
        };
        tree.children.insert(
            root.clone(),
            DirChildren {
                entries: vec![dir, file.clone()],
                truncated: 0,
            },
        );
        let mut git = GitPanelState::default();
        git.badges.insert(file.path.clone(), StatusKind::Modified);

        let run = |width: f32| {
            let ctx = egui::Context::default();
            let rects = Cell::new((Rect::NOTHING, Rect::NOTHING));
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    let children = tree.children.get(&root).unwrap();
                    let outbox = &mut Vec::new();
                    let dir_rect = tree_row(ui, &tree, &children.entries[0], None, &git, outbox);
                    let file_rect = tree_row(ui, &tree, &children.entries[1], None, &git, outbox);
                    rects.set((dir_rect.rect, file_rect.rect));
                },
            );
            // 可见文本按 glyph 层拼接;文本矩形按可见文本匹配(job.text 恒含
            // 完整原文,拿它匹配会把被 elide 掉的名字也算「画出来了」)
            let texts = visible_shape_texts(&output);
            let text_rect = |pred: &dyn Fn(&str) -> bool| -> Rect {
                let mut found = Rect::NOTHING;
                for clipped in &output.shapes {
                    if let egui::epaint::Shape::Text(text) = &clipped.shape {
                        let visible: String =
                            text.galley.rows.iter().map(|row| row.row.text()).collect();
                        if pred(&visible) {
                            found = text.visual_bounding_rect();
                        }
                    }
                }
                found
            };
            let badge = text_rect(&|t: &str| t.trim() == "M");
            let truncated_name = text_rect(&|t: &str| t.trim_start().starts_with("超长"));
            output.drop_without_applying_deltas();
            (texts, rects.get(), badge, truncated_name)
        };

        // —— 窄栏 120px:名字只容得下前几个字,省略号截尾 ——
        let (narrow_texts, (narrow_dir, narrow_file), badge, truncated_name) = run(120.0);
        let name = narrow_texts
            .iter()
            .find(|t| t.trim_start().starts_with("超长"))
            .expect("长名字应仍在文本层");
        assert!(name.ends_with('…'), "窄栏名字以省略号截尾:{name:?}");
        assert!(
            !name.contains(&long_name),
            "全名不得整体进窄栏可见层:{name:?}"
        );
        assert!(
            narrow_texts.iter().any(|t| t.trim() == "M"),
            "角标仍可见:{narrow_texts:?}"
        );
        // 行高恒定:目录短名行 == 文件长名行(长名不再换行撑高)
        assert!(
            (narrow_dir.height() - narrow_file.height()).abs() < 0.5,
            "窄栏行高恒定:目录 {narrow_dir:?} vs 文件 {narrow_file:?}"
        );
        // 角标贴行尾:画在名字右侧,距行右沿只剩按钮内边距量级
        assert!(badge != Rect::NOTHING && truncated_name != Rect::NOTHING);
        assert!(
            badge.left() > truncated_name.right(),
            "角标在名字右侧:角标 {badge:?} 名字 {truncated_name:?}"
        );
        assert!(
            badge.right() <= narrow_file.right() && narrow_file.right() - badge.right() < 12.0,
            "角标贴行尾:角标右沿 {} 行右沿 {}",
            badge.right(),
            narrow_file.right()
        );

        // —— 宽栏 500px:全名完整,行高与窄栏一致(不随栏宽/名字长度变化)——
        let (wide_texts, (_, wide_file), _, _) = run(500.0);
        let full = wide_texts
            .iter()
            .find(|t| t.contains(&long_name))
            .expect("宽栏应显示全名");
        assert!(!full.ends_with('…'), "宽栏不截断:{full:?}");
        assert!(
            (wide_file.height() - narrow_file.height()).abs() < 0.5,
            "行高不随栏宽变化:宽 {wide_file:?} 窄 {narrow_file:?}"
        );
    }

    /// #40 hover tooltip:窄栏截断行的完整文件名只出现在 tooltip 文本层
    /// (`everything_is_visible` 是 egui 自己的 UI 测试手法,免 tooltip 延迟,
    /// 同 top_actions 先例);带 Git 角标的行 tooltip 附状态说明。
    #[test]
    fn hovering_truncated_tree_row_shows_full_name_tooltip() {
        let long_name = format!("{}笔记.md", "超长中文文件名".repeat(3));
        let root = PathBuf::from("/vault");
        let mut tree = FileTreeState {
            root: Some(root.clone()),
            ..FileTreeState::default()
        };
        let file = TreeEntry {
            path: root.join(&long_name),
            name: long_name.clone(),
            is_dir: false,
        };
        tree.children.insert(
            root.clone(),
            DirChildren {
                entries: vec![file.clone()],
                truncated: 0,
            },
        );
        let mut git = GitPanelState::default();
        git.badges.insert(file.path.clone(), StatusKind::Modified);

        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 400.0));
        let render = |ui: &mut egui::Ui| {
            let children = tree.children.get(&root).unwrap();
            tree_row(ui, &tree, &children.entries[0], None, &git, &mut Vec::new())
        };
        let ctx = egui::Context::default();
        let row_rect = Cell::new(Rect::NOTHING);
        ctx.run_ui(
            RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| row_rect.set(render(ui).rect),
        )
        .drop_without_applying_deltas();
        let center = row_rect.get().center();

        ctx.memory_mut(|mem| mem.set_everything_is_visible(true));
        // 两帧:hover 判定用上一帧的指针位置,tooltip 在下一帧才渲染。
        // 断言用**可见**文本:行自身的名字在 120px 下被 elide,可见层没有
        // 全名,全名只能来自 tooltip 的字形(job.text 恒留原文,拿它断言恒真)
        let mut texts = Vec::new();
        for _ in 0..2 {
            let output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(screen),
                    events: vec![Event::PointerMoved(center)],
                    ..Default::default()
                },
                |ui| {
                    render(ui);
                },
            );
            texts = visible_shape_texts(&output);
            output.drop_without_applying_deltas();
        }
        assert!(
            texts.iter().any(|t| t.contains(&long_name)),
            "截断行的全名进 tooltip:{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("已修改")),
            "带角标行 tooltip 附状态说明:{texts:?}"
        );
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

    // —— M2 三段式(docs/ui-shell-redesign.md §5)——

    /// 导航行顺序 helper:一行一 `NAV_ROW_H`,自上而下不重叠;第 5 行是
    /// 反向链接(#15,追加在尾部不打乱既有四行)。
    #[test]
    fn nav_row_center_y_follows_tab_order() {
        let row = crate::ui::tokens::NAV_ROW_H;
        assert_eq!(
            nav_row_center_y(100.0, SidebarTab::Files),
            100.0 + row * 0.5
        );
        assert_eq!(
            nav_row_center_y(100.0, SidebarTab::Git),
            100.0 + row * 3.5,
            "Git 是第四行"
        );
        assert_eq!(
            nav_row_center_y(100.0, SidebarTab::Backlinks),
            100.0 + row * 4.5,
            "反向链接是第五行"
        );
        let ys = SidebarTab::ALL.map(|tab| nav_row_center_y(0.0, tab));
        assert!(
            ys.windows(2).all(|pair| pair[1] > pair[0]),
            "五行自上而下:{ys:?}"
        );
    }

    /// 三段式骨架:三段自上而下排满左栏、互不重叠,中段吃掉全部剩余高度
    /// (M2 验收点;底段设置行已迁至标题栏齿轮,decisions-pending #31)。
    /// 宽度下限 180px 时同样成立 —— 顶段在窄栏里会换行变高,中段让出的
    /// 高度随之变少。
    #[test]
    fn three_bands_fill_the_panel_top_down() {
        let ctx = egui::Context::default();
        let (tree, _root) = sample_tree();
        let height = Cell::new(0.0f32);
        let mut bands = None;
        ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(crate::ui::tokens::SIDEBAR_MIN_W, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                height.set(ui.max_rect().bottom());
                bands = Some(ui_whole(
                    ui,
                    SidebarTab::Files,
                    &tree,
                    &GitPanelState::default(),
                ));
            },
        )
        .drop_without_applying_deltas();
        let bands = bands.unwrap();

        assert!(bands.top.top() < bands.nav.top(), "顶段在次段之上");
        assert!(bands.nav.top() < bands.body.top(), "次段在中段之上");
        assert!(
            bands.body.bottom() >= height.get() - 1.0,
            "中段吃到左栏底部(不再为底段预留):body.bottom={} panel.bottom={}",
            bands.body.bottom(),
            height.get()
        );
    }

    /// 点「搜索」行的手写命中区:走完整 `sidebar::ui` 三帧请求,只发一条
    /// `SidebarTabChanged`。
    ///
    /// 直接驱 `sidebar::ui`(而非孤立 `nav_row`)是有意的:顶段的五个文件
    /// 动作按钮紧贴次段,只测孤立行会漏掉「谁抢走了这次点击」—— 三栏重排
    /// 时已经在标题栏上踩过一次。
    #[test]
    fn clicking_nav_row_switches_view() {
        let (tree, _root) = sample_tree();
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        let mut active = SidebarTab::Files;
        let mut search = SearchState::default();
        let git = GitPanelState::default();
        let nav_top = Cell::new(0.0f32);

        ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(crate::ui::tokens::SIDEBAR_MIN_W, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                nav_top.set(ui_whole(ui, SidebarTab::Files, &tree, &git).nav.top());
            },
        )
        .drop_without_applying_deltas();

        let target = egui::pos2(
            crate::ui::tokens::SIDEBAR_MIN_W / 2.0,
            nav_row_center_y(nav_top.get(), SidebarTab::Search),
        );
        let click = |pressed| Event::PointerButton {
            pos: target,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        // 前两遍是 sizing / 未交互遍,widget 尚不参与命中测试
        for events in [
            Vec::new(),
            Vec::new(),
            vec![Event::PointerMoved(target)],
            vec![click(true)],
            vec![click(false)],
        ] {
            ctx.run_ui(
                RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(crate::ui::tokens::SIDEBAR_MIN_W, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui_whole_with(ui, &mut active, &mut search, &tree, &git, &mut outbox);
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(
            outbox,
            vec![Message::SidebarTabChanged(SidebarTab::Search)],
            "导航行点击只发一条切换消息"
        );
    }

    /// 只读一版的整栏渲染:无 piercing、无 outbox,专供量取三段矩形。
    fn ui_whole(
        ui: &mut egui::Ui,
        tab: SidebarTab,
        tree: &FileTreeState,
        git: &GitPanelState,
    ) -> SidebarBands {
        let mut tab = tab;
        let mut search = SearchState::default();
        let backlinks = BacklinkState::default();
        let mut outbox = Vec::new();
        super::ui(
            ui,
            &mut tab,
            tree,
            None,
            OutlineView {
                items: &[],
                cursor_byte: None,
            },
            &mut search,
            git,
            &backlinks,
            &mut outbox,
        )
    }

    /// 带状态与 outbox 的整栏渲染(点击测试走这条)。
    fn ui_whole_with(
        ui: &mut egui::Ui,
        active: &mut SidebarTab,
        search: &mut SearchState,
        tree: &FileTreeState,
        git: &GitPanelState,
        outbox: &mut Vec<Message>,
    ) {
        let backlinks = BacklinkState::default();
        super::ui(
            ui,
            active,
            tree,
            None,
            OutlineView {
                items: &[],
                cursor_byte: None,
            },
            search,
            git,
            &backlinks,
            outbox,
        );
    }

    // —— M5 收口:top_actions 是文件动作的唯一常驻按钮入口(编辑器区顶
    // 部的文件工具栏退役,decisions-pending #32;原 `ui::toolbar` 的测试
    // 迁移至此)——

    /// 全量命令的 tooltip 口径:命令名 + 出厂键位(键位可改,出厂默认够
    /// 指路;原 `toolbar::button` 的同款口径)。
    #[test]
    fn top_actions_tooltip_carries_label_and_builtin_shortcut() {
        let keymap = Keymap::builtin();
        for cmd in Command::FILE
            .iter()
            .copied()
            .chain([Command::ExportHtml, Command::ExportPdf])
        {
            let tooltip = tooltip_of(cmd, &keymap);
            assert!(
                tooltip.starts_with(cmd.label()),
                "{cmd:?}: tooltip 应以命令名开头:{tooltip}"
            );
            if let Some(shortcut) = keymap.get(cmd) {
                let platform = shortcut.platform_text();
                assert!(
                    tooltip.contains(&platform),
                    "{cmd:?}: tooltip 应带键位 {platform}:{tooltip}"
                );
            }
        }
    }

    /// 全量文件动作按钮逐个点得动:真实 `top_actions_with_probe` 路径下
    /// 点击各发对应命令消息(原 `toolbar_button_sends_command_message` 的
    /// 全量版;仅渲染帧不产消息)。
    #[test]
    fn clicking_every_top_action_sends_its_command() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let commands: Vec<Command> = Command::FILE
            .iter()
            .copied()
            .chain([Command::ExportHtml, Command::ExportPdf])
            .collect();
        let ctx = egui::Context::default();
        let rects = Rc::new(RefCell::new(Vec::<(Command, Rect)>::new()));

        // 帧 1:探针拿按钮矩形(仅渲染,顺带断言不产消息)
        {
            let sink = rects.clone();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                super::top_actions_with_probe(
                    ui,
                    &mut Vec::new(),
                    Some(&mut |cmd, rect| {
                        sink.borrow_mut().push((cmd, rect));
                    }),
                );
            });
            output.drop_without_applying_deltas();
        }
        let rects = rects.borrow().clone();
        assert_eq!(
            rects.iter().map(|(cmd, _)| *cmd).collect::<Vec<_>>(),
            commands,
            "按钮全集 = Command::FILE + 导出两项(HTML/PDF),不多不少"
        );

        // 每按钮三帧(moved / press / release):点击发出对应消息
        let click = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for (cmd, rect) in rects {
            let mut outbox = Vec::new();
            let center = rect.center();
            for events in [
                vec![Event::PointerMoved(center)],
                vec![click(center, true)],
                vec![click(center, false)],
            ] {
                let output = ctx.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| super::top_actions_with_probe(ui, &mut outbox, None),
                );
                output.drop_without_applying_deltas();
            }
            assert_eq!(outbox, vec![cmd.message()], "{cmd:?} 按钮点击出对应消息");
        }
    }

    /// hover 出 tooltip:指针停在按钮上时,tooltip 文本(含出厂键位)真实
    /// 渲染出来(`everything_is_visible` 是 egui 自己的 UI 测试手法,免去
    /// tooltip 延迟)。
    #[test]
    fn hovering_top_action_shows_shortcut_tooltip() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let ctx = egui::Context::default();
        let save_rect = Rc::new(RefCell::new(Rect::NOTHING));
        {
            let sink = save_rect.clone();
            let output = ctx.run_ui(RawInput::default(), |ui| {
                super::top_actions_with_probe(
                    ui,
                    &mut Vec::new(),
                    Some(&mut |cmd, rect| {
                        if cmd == Command::Save {
                            *sink.borrow_mut() = rect;
                        }
                    }),
                );
            });
            output.drop_without_applying_deltas();
        }
        let center = save_rect.borrow().center();
        assert!(center.x > 0.0, "探针拿到保存按钮");

        ctx.memory_mut(|mem| mem.set_everything_is_visible(true));
        // 两帧:hover 判定用上一帧的指针位置,tooltip 在下一帧才渲染
        let mut shapes = Vec::new();
        for events in [
            vec![Event::PointerMoved(center)],
            vec![Event::PointerMoved(center)],
        ] {
            let output = ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| super::top_actions_with_probe(ui, &mut Vec::new(), None),
            );
            shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
        }
        let shortcut = Keymap::builtin()
            .get(Command::Save)
            .map(|s| s.platform_text())
            .unwrap_or_default();
        let expected = format!("保存({shortcut})");
        let painted: Vec<String> = shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        assert!(
            painted.iter().any(|t| t.contains(&expected)),
            "tooltip 应含 {expected:?},实际画出的文本:{painted:?}"
        );
    }
}
