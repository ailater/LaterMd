//! macOS 工作台外壳。系统负责窗口按钮,应用负责导航与文档工具。

use crate::command::Command;
use crate::settings::SettingsTab;
use crate::state::{Message, SidebarTab, State};
use crate::ui::icons::Icon;
use eframe::egui::{self, Align2, Rect, Sense};

pub const HEADER_H: f32 = 40.0;
const TRAFFIC_LIGHTS_W: f32 = 76.0;
const HEADER_RIGHT_W: f32 = 388.0;

/// 系统按钮的中心换算到 egui 坐标；AppKit 使用 point，egui 还可能有
/// 用户缩放，两者不能直接混用。无原生窗口的无头帧仍用标题栏中心。
fn header_center_y(ui: &egui::Ui, bar: Rect) -> f32 {
    #[cfg(target_os = "macos")]
    if let Some(center) = native_traffic_light_center(ui.ctx()) {
        if (0.0..=bar.height()).contains(&center) {
            return bar.top() + center;
        }
    }
    let _ = ui;
    bar.center().y
}

#[cfg(target_os = "macos")]
fn native_traffic_light_center(ctx: &egui::Context) -> Option<f32> {
    use objc2_app_kit::{NSApplication, NSWindowButton};
    use objc2_foundation::MainThreadMarker;
    let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
    // mainWindow 保持文档窗口身份，不随打开文件对话框的 keyWindow 改变。
    let window = app.mainWindow().or_else(|| app.keyWindow())?;
    let content = window.contentView()?;
    let button = window.standardWindowButton(NSWindowButton::CloseButton)?;
    let bounds = content.bounds();
    let rect = button.convertRect_toView(button.bounds(), Some(&content));
    // 先在 contentView 的坐标系中处理翻转；backing 坐标方向未必相同。
    let backing_scale = content.convertRectToBacking(bounds).size.height / bounds.size.height;
    let middle = rect.origin.y + rect.size.height / 2.0;
    let top_down = if content.isFlipped() {
        middle - bounds.origin.y
    } else {
        bounds.origin.y + bounds.size.height - middle
    };
    Some((top_down * backing_scale) as f32 / ctx.pixels_per_point())
}

/// 仅用于独立工具栏标签：按字形墨迹居中，避开中英文字体不同的行盒留白。
/// 不修改正文的排版、基线或字体度量。
fn centered_label(ui: &egui::Ui, rect: Rect, label: &str, color: egui::Color32) {
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.0), color);
    let pos = egui::pos2(
        rect.center().x - galley.size().x / 2.0,
        rect.center().y - galley.mesh_bounds.center().y,
    );
    ui.painter().galley(pos, galley, color);
}

/// 单物理像素分隔线；与面板原生拖拽高亮共存。
pub fn separator(ui: &egui::Ui) -> egui::Stroke {
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    egui::Stroke::new(1.0 / ui.ctx().pixels_per_point(), colors.border)
}

fn action(ui: &mut egui::Ui, rect: Rect, icon: Icon, tip: &str, selected: bool) -> bool {
    let response = ui.allocate_rect(rect, Sense::click());
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(rect, 6.0, colors.hover);
    }
    let ink = if selected {
        colors.text
    } else {
        colors.secondary
    };
    // 两侧面板使用相同的细线框与内分隔，不再画厚实心条。
    if matches!(icon, Icon::Sidebar | Icon::PanelRight) {
        let bounds = Rect::from_center_size(rect.center(), egui::vec2(17.0, 13.0));
        let stroke = egui::Stroke::new(1.2, ink);
        ui.painter()
            .rect_stroke(bounds, 2.5, stroke, egui::StrokeKind::Inside);
        let x = if icon == Icon::Sidebar {
            bounds.left() + 5.0
        } else {
            bounds.right() - 5.0
        };
        if selected {
            let area = if icon == Icon::Sidebar {
                Rect::from_min_max(bounds.min, egui::pos2(x, bounds.bottom()))
            } else {
                Rect::from_min_max(egui::pos2(x, bounds.top()), bounds.max)
            };
            ui.painter()
                .rect_filled(area.shrink(1.5), 1.0, ink.gamma_multiply(0.15));
        }
        ui.painter().line_segment(
            [
                egui::pos2(x, bounds.top() + 1.0),
                egui::pos2(x, bounds.bottom() - 1.0),
            ],
            stroke,
        );
    } else {
        icon.draw(ui.painter(), rect.center(), 16.0, ink);
    }
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, tip));
    response
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// 空白处可拖窗;所有命令仍走既有消息归约,不抢编辑器焦点。
pub fn header(ui: &mut egui::Ui, state: &mut State, outbox: &mut Vec<Message>) {
    let bar = ui.max_rect();
    let center_y = header_center_y(ui, bar);
    let drag_rect = Rect::from_min_max(
        bar.left_top() + egui::vec2(TRAFFIC_LIGHTS_W, 0.0),
        bar.right_bottom(),
    );
    let drag = ui.allocate_rect(drag_rect, Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag.double_clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    let slot = |x: f32, w: f32| {
        Rect::from_center_size(egui::pos2(x + w / 2.0, center_y), egui::vec2(w, 28.0))
    };
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    let right = bar.right() - 12.0;
    let right_start = right - HEADER_RIGHT_W;
    let sidebar_x = bar.left() + TRAFFIC_LIGHTS_W + 6.0;
    if action(
        ui,
        slot(sidebar_x, 28.0),
        Icon::Sidebar,
        "显示 / 隐藏侧栏",
        state.layout.left,
    ) {
        outbox.push(Command::ToggleSidebar.message());
    }
    let divider_x = sidebar_x + 40.0;
    ui.painter().line_segment(
        [
            egui::pos2(divider_x, center_y - 8.0),
            egui::pos2(divider_x, center_y + 8.0),
        ],
        separator(ui),
    );
    Icon::File.draw(
        ui.painter(),
        egui::pos2(divider_x + 19.0, center_y),
        14.0,
        colors.secondary,
    );
    let title_rect = Rect::from_min_max(
        egui::pos2(divider_x + 33.0, bar.top()),
        egui::pos2(right_start - 20.0, bar.bottom()),
    );
    let mut font = egui::FontId::proportional(12.0);
    font.family = crate::fonts::semibold_family(ui.ctx());
    let title = crate::ui::tabs::elide_text(
        ui,
        &state.tabs.current().display_name(),
        &font,
        title_rect.width().max(0.0),
    );
    let title_galley = ui.painter().layout_no_wrap(title, font, colors.text);
    ui.painter().with_clip_rect(title_rect).galley(
        egui::pos2(
            title_rect.left(),
            center_y - title_galley.mesh_bounds.center().y,
        ),
        title_galley,
        colors.text,
    );
    mode_switch(ui, slot(right_start, 100.0), state, outbox);
    search_field(
        ui,
        slot(right_start + 112.0, 196.0),
        &mut state.search,
        outbox,
    );
    ui.painter().line_segment(
        [
            egui::pos2(right - 72.0, center_y - 8.0),
            egui::pos2(right - 72.0, center_y + 8.0),
        ],
        separator(ui),
    );
    if action(
        ui,
        slot(right - 64.0, 28.0),
        Icon::PanelRight,
        "显示 / 隐藏预览",
        state.layout.right,
    ) {
        outbox.push(Command::ToggleRightPreview.message());
    }
    if action(ui, slot(right - 28.0, 28.0), Icon::Settings, "设置", false) {
        outbox.push(Message::SettingsOpened(SettingsTab::Appearance));
    }
}

fn mode_switch(ui: &mut egui::Ui, rect: Rect, state: &State, outbox: &mut Vec<Message>) {
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    let live = state.render_mode == crate::live::RenderMode::Live;
    ui.painter().rect_filled(rect, 7.0, colors.hover);
    for (index, label) in ["源码", "Live"].into_iter().enumerate() {
        let part = Rect::from_min_size(
            rect.min + egui::vec2(index as f32 * rect.width() / 2.0, 0.0),
            egui::vec2(rect.width() / 2.0, rect.height()),
        )
        .shrink(2.0);
        let selected = (index == 1) == live;
        let response = ui.allocate_rect(part, Sense::click());
        if selected {
            ui.painter().rect_filled(part, 5.0, colors.content);
            ui.painter()
                .rect_stroke(part, 5.0, separator(ui), egui::StrokeKind::Inside);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(part, 5.0, crate::theme::window_fill(ui.visuals().dark_mode));
        }
        centered_label(
            ui,
            part,
            label,
            if selected {
                colors.text
            } else {
                colors.secondary
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, label)
        });
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
            && !selected
        {
            outbox.push(Message::ToggleLivePreview);
        }
    }
}

fn search_field(
    ui: &mut egui::Ui,
    rect: Rect,
    search: &mut crate::search::SearchState,
    outbox: &mut Vec<Message>,
) {
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    let id = ui.make_persistent_id("workbench-search");
    let focused = ui.memory(|m| m.has_focus(id));
    ui.painter().rect_filled(rect, 6.0, colors.content);
    ui.painter().rect_stroke(
        rect,
        6.0,
        if focused {
            egui::Stroke::new(1.0, colors.accent.gamma_multiply(0.65))
        } else {
            separator(ui)
        },
        egui::StrokeKind::Inside,
    );
    Icon::Search.draw(
        ui.painter(),
        egui::pos2(rect.left() + 13.0, rect.center().y),
        14.0,
        colors.secondary,
    );
    let has_query = !search.query.is_empty();
    let edit_rect = Rect::from_min_max(
        egui::pos2(rect.left() + 27.0, rect.top() + 5.0),
        egui::pos2(
            rect.right() - if has_query { 25.0 } else { 8.0 },
            rect.bottom() - 5.0,
        ),
    );
    let response = ui.put(
        edit_rect,
        egui::TextEdit::singleline(&mut search.query)
            .id(id)
            .font(egui::FontId::proportional(12.0))
            .hint_text("搜索文档…")
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .vertical_align(egui::Align::Center),
    );
    if response.changed() {
        outbox.push(Message::SearchQueryChanged);
    }
    response.on_hover_text("全文搜索 · 结果显示在左侧搜索页");
    if has_query
        && action(
            ui,
            Rect::from_center_size(
                egui::pos2(rect.right() - 13.0, rect.center().y),
                egui::vec2(20.0, 20.0),
            ),
            Icon::Close,
            "清空搜索",
            false,
        )
    {
        search.query.clear();
        outbox.push(Message::SearchQueryChanged);
    }
}

/// 目录名、最近目录和打开目录入口合成一个控件，长路径只在悬停时显示。
pub fn workspace_picker(
    ui: &mut egui::Ui,
    tree: &crate::filetree::FileTreeState,
    outbox: &mut Vec<Message>,
) {
    let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
    ui.label(
        egui::RichText::new("文件夹")
            .size(11.0)
            .color(colors.secondary),
    );
    ui.add_space(4.0);
    let label = tree
        .root
        .as_deref()
        .map(crate::ui::sidebar::recent_label)
        .unwrap_or_else(|| "打开文件夹…".to_owned());
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        6.0,
        if response.hovered() {
            colors.hover
        } else {
            colors.content.gamma_multiply(0.45)
        },
    );
    ui.painter()
        .rect_stroke(rect, 6.0, separator(ui), egui::StrokeKind::Inside);
    Icon::FolderClosed.draw(
        ui.painter(),
        egui::pos2(rect.left() + 15.0, rect.center().y),
        15.0,
        colors.secondary,
    );
    let font = egui::FontId::proportional(12.0);
    let text = crate::ui::tabs::elide_text(ui, &label, &font, (rect.width() - 58.0).max(0.0));
    ui.painter().text(
        egui::pos2(rect.left() + 30.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        font,
        colors.text,
    );
    let c = egui::pos2(rect.right() - 13.0, rect.center().y);
    ui.painter().add(egui::Shape::line(
        vec![
            c + egui::vec2(-3.0, -1.5),
            c + egui::vec2(0.0, 1.5),
            c + egui::vec2(3.0, -1.5),
        ],
        egui::Stroke::new(1.2, colors.secondary),
    ));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, &label));
    let response = response
        .on_hover_text(
            tree.root
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "选择 Markdown 文件夹".to_owned()),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    egui::Popup::menu(&response).show(|ui| {
        ui.set_min_width((rect.width() - 16.0).max(120.0));
        for dir in &tree.recents {
            if ui
                .selectable_label(
                    tree.root.as_deref() == Some(dir.as_path()),
                    crate::ui::sidebar::recent_label(dir),
                )
                .on_hover_text(dir.display().to_string())
                .clicked()
            {
                outbox.push(Message::FileTreeRootSelected(dir.clone()));
                ui.close();
            }
        }
        if !tree.recents.is_empty() {
            ui.separator();
        }
        if ui.button("打开文件夹…").clicked() {
            outbox.push(Message::FileTreeRootPick);
            ui.close();
        }
    });
    ui.add_space(10.0);
}

/// 返回导航占用的矩形,内容面板接在下方并独立滚动。
pub fn navigation(ui: &mut egui::Ui, active: SidebarTab, outbox: &mut Vec<Message>) -> Rect {
    ui.add_space(12.0);
    ui.label(
        egui::RichText::new("工作台")
            .size(11.0)
            .color(crate::theme::shell_tokens(ui.visuals().dark_mode).secondary),
    );
    ui.add_space(4.0);
    let top = ui.cursor().top();
    for tab in SidebarTab::ALL {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), Sense::click());
        let colors = crate::theme::shell_tokens(ui.visuals().dark_mode);
        if tab == active || response.hovered() {
            ui.painter().rect_filled(
                rect,
                6.0,
                if tab == active {
                    colors.selected_bg
                } else {
                    colors.hover
                },
            );
        }
        tab.icon().draw(
            ui.painter(),
            egui::pos2(rect.left() + 15.0, rect.center().y),
            15.0,
            if tab == active {
                colors.accent
            } else {
                colors.secondary
            },
        );
        ui.painter().text(
            egui::pos2(rect.left() + 32.0, rect.center().y),
            Align2::LEFT_CENTER,
            if tab == SidebarTab::Backlinks {
                "反向链接"
            } else {
                tab.label()
            },
            egui::FontId::proportional(13.0),
            colors.text,
        );
        if response.clicked() {
            outbox.push(Message::SidebarTabChanged(tab));
        }
    }
    let nav = Rect::from_min_max(
        egui::pos2(ui.max_rect().left(), top),
        ui.min_rect().right_bottom(),
    );
    ui.add_space(18.0);
    nav
}

pub fn pane_heading(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(12.0)
                .color(crate::theme::shell_tokens(ui.visuals().dark_mode).secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(detail).size(11.0).weak());
        });
    });
    ui.add_space(8.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2};

    fn click(pos: Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    }

    #[test]
    fn mode_segment_only_switches_when_selecting_the_other_mode() {
        for dark in [false, true] {
            for live in [false, true] {
                let ctx = egui::Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                let mut state = State::default();
                state.render_mode = if live {
                    crate::live::RenderMode::Live
                } else {
                    crate::live::RenderMode::Source
                };
                let mut outbox = Vec::new();
                let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 28.0));
                for events in [
                    vec![],
                    click(egui::pos2(if live { 95.0 } else { 45.0 }, 34.0)),
                ] {
                    ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| mode_switch(ui, rect, &state, &mut outbox),
                    )
                    .drop_without_applying_deltas();
                }
                assert!(outbox.is_empty(), "点击当前模式不能反转");
                ctx.run_ui(
                    egui::RawInput {
                        events: click(egui::pos2(if live { 45.0 } else { 95.0 }, 34.0)),
                        ..Default::default()
                    },
                    |ui| mode_switch(ui, rect, &state, &mut outbox),
                )
                .drop_without_applying_deltas();
                assert!(matches!(outbox.as_slice(), [Message::ToggleLivePreview]));
            }
        }
    }

    #[test]
    fn search_typing_and_clear_keep_the_existing_query_message() {
        let ctx = egui::Context::default();
        let mut search = crate::search::SearchState::default();
        let mut outbox = Vec::new();
        let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(196.0, 28.0));
        let mut frame = |events| {
            ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| search_field(ui, rect, &mut search, &mut outbox),
            )
            .drop_without_applying_deltas();
        };
        frame(vec![]);
        frame(click(egui::pos2(65.0, 34.0)));
        frame(vec![Event::Text("文档".to_owned())]);
        frame(vec![]);
        frame(click(egui::pos2(203.0, 34.0)));
        assert!(search.query.is_empty());
        assert_eq!(
            outbox
                .iter()
                .filter(|m| matches!(m, Message::SearchQueryChanged))
                .count(),
            2
        );
    }
    #[test]
    fn source_and_live_glyphs_share_the_segment_center() {
        for scale in [1.0, 1.5, 2.0] {
            let ctx = egui::Context::default();
            crate::fonts::install(&ctx).expect("alignment test requires installed CJK fonts");
            ctx.set_pixels_per_point(scale);
            let rect = Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 28.0));
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                mode_switch(ui, rect, &State::default(), &mut Vec::new());
            });
            let mut centers = Vec::new();
            fn inspect(shape: &egui::Shape, centers: &mut Vec<f32>) {
                match shape {
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            inspect(shape, centers);
                        }
                    }
                    egui::Shape::Text(text) if ["源码", "Live"].contains(&text.galley.text()) => {
                        let mut bounds = Rect::NOTHING;
                        for row in &text.galley.rows {
                            for glyph in &row.glyphs {
                                let min = text.pos
                                    + row.pos.to_vec2()
                                    + glyph.pos.to_vec2()
                                    + glyph.uv_rect.offset;
                                bounds |= Rect::from_min_size(min, glyph.uv_rect.size);
                            }
                        }
                        centers.push(bounds.center().y);
                    }
                    _ => {}
                }
            }
            for shape in &output.shapes {
                inspect(&shape.shape, &mut centers);
            }
            output.textures_delta.clear();
            assert_eq!(centers.len(), 2);
            for y in centers {
                assert!(
                    (y - rect.center().y).abs() * scale <= 0.5,
                    "glyph center {y} must match control center {} at scale {scale}",
                    rect.center().y
                );
            }
        }
    }
}
