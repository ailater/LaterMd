//! 「插入 Emoji」面板(docs/emoji-plan.md E1 骨架 + E2 搜索/分类/最近使用)。
//!
//! ## 分工(与 `ui::image_dialog` 同一套写法)
//!
//! 本模块只收输入、只发消息:搜索草稿 `query` 与当前分类 `group` 归 UI
//! 原地持有(`TextEdit` 是立即模式控件,草稿必须能就地 `&mut`),插入的
//! 归约在 `state`(走 `compose::insert_emoji`),点选后关面板与 Esc 关闭
//! 也都经消息(`EmojiInserted` / `EmojiPickerToggle(false)`)。
//!
//! ## E2 的口径
//!
//! - **搜索**:`query` 非空时走 `emoji_data::search` 三路匹配(中文名 /
//!   英文名 / 短码,大小写不敏感),结果**跨分类**呈现,段头与 tooltip
//!   都标来源分类;无命中显式给「无匹配」。空查询 = 当前分类全表。
//! - **最近使用**:面板底部一行(空态整行隐藏);点选即再插入 —— 连插
//!   多个靠它二次进入。去重、封顶与落盘都在归约(`state::insert_emoji`
//!   → `settings.json`,与主题同路)。
//!
//! ## 渲染口径
//!
//! 应用内 emoji 由 NotoEmoji(egui 出厂字体链)黑白渲染 —— 这是上游
//! 限制不是 bug,面板底部一行小字说明「导出 / 外发仍是彩色」
//! (emoji-plan §2 F2)。无头测试只断言「点击 → 发出正确消息」与「渲染
//! 不 panic」,不断言字形(has_glyph 依赖真实字体,会 flaky,§7 #7)。

use crate::state::Message;
use crate::ui::emoji_data::{self, EmojiEntry};
use crate::ui::tokens;
use eframe::egui;

/// 网格列数(emoji-plan §6.4:8 列)。
const COLUMNS: usize = 8;
/// 单元格边长(emoji-plan §6.4:32×32)。
const CELL: f32 = 32.0;
/// 单元格字号:占格子约六成,再大就顶到 hover 底色边缘。
const CELL_FONT: f32 = CELL * 0.6;
/// 搜索结果滚动区的高度上限(约 5 行)。每类 ≤ 40 枚本就单屏放得下;
/// 跨分类搜索的命中总数不受该约束,用滚动 + 屏外字形不绘制(`cell` 里
/// `is_rect_visible` 跳过)把渲进字体图集的量限在可视区,防图集膨胀
/// (emoji-plan §7 #6)。
const RESULTS_MAX_H: f32 = 200.0;

/// Emoji 面板状态(归约置 `open`,UI 改 `query` / `group`)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmojiPanelState {
    /// 面板是否可见。
    pub open: bool,
    /// 搜索草稿。非空时网格换 `emoji_data::search` 的跨分类结果;归约在
    /// 开面板时清空(上次的搜索词对下一次插入没有意义)。
    pub query: String,
    /// 当前分类(`emoji_data::GROUPS` 的下标);清空搜索词即回到它。
    pub group: usize,
    /// 最近使用(去重、新的在前、上限见 `state::EMOJI_RECENT_CAP`)。
    /// 维护与持久化都在归约(E2 起随 settings.json),面板只展示。
    pub recent: Vec<String>,
}

/// 画面板,返回所有可点 emoji 单元的响应(网格 + 搜索结果 + 最近使用,
/// 测试定位用,生产调用方忽略)。
///
/// 点选单元发 [`Message::EmojiInserted`],归约里插入并关面板;Esc 直接
/// 发 `Message::EmojiPickerToggle(false)`(只关面板,不动文档)。
pub fn panel(
    ui: &mut egui::Ui,
    state: &mut EmojiPanelState,
    outbox: &mut Vec<Message>,
) -> Vec<egui::Response> {
    let mut cells = Vec::new();
    egui::Window::new("插入 Emoji")
        // 与 image_dialog 同款:首帧锚定屏幕中心,拖动后由 Area 记忆保持
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ui.ctx().viewport_rect().center())
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            // 搜索框:E2 起参与过滤(三路大小写不敏感匹配,emoji_data::search);
            // desired_width 撑满让网格与输入框同宽
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("搜索中文名 / 英文名 / 短码")
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(tokens::SPACE_SM);
            // 分类横向标签。搜索态下标签保留可点:清空搜索词即回到所选分类
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;
                for (index, group) in emoji_data::GROUPS.iter().enumerate() {
                    if ui
                        .selectable_label(index == state.group, group.name)
                        .clicked()
                    {
                        state.group = index;
                    }
                }
            });
            ui.add_space(tokens::SPACE_SM);
            // 网格:空查询 = 当前分类全表;有查询 = 跨分类命中,段头高亮
            // 来源分类(emoji-plan E2)
            let query = state.query.trim();
            if query.is_empty() {
                let index = state.group.min(emoji_data::GROUPS.len() - 1);
                let group = &emoji_data::GROUPS[index];
                for row in group.entries.chunks(COLUMNS) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;
                        for entry in row {
                            cells.push(cell(ui, entry, None, outbox));
                        }
                    });
                }
            } else {
                let hits = emoji_data::search(query);
                if hits.is_empty() {
                    ui.weak("无匹配");
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("emoji-search-results")
                        .max_height(RESULTS_MAX_H)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            // hits 按分类有序:连续同组号即一个来源分类,
                            // 段头就是命中来源(跨分类可发现)
                            for run in hits.chunk_by(|a, b| a.0 == b.0) {
                                let source = emoji_data::GROUPS[run[0].0].name;
                                ui.strong(source);
                                for row in run.chunks(COLUMNS) {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;
                                        for &(_, entry) in row {
                                            cells.push(cell(ui, entry, Some(source), outbox));
                                        }
                                    });
                                }
                            }
                        });
                }
            }
            ui.add_space(tokens::SPACE_XS);
            ui.separator();
            // 「最近使用」一行(E2):空态整行隐藏;点选即再插入 —— 连插
            // 多个靠它二次进入,不用重新翻分类
            if !state.recent.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = tokens::SPACE_XS;
                    ui.weak("最近");
                    for emoji in &state.recent {
                        cells.push(glyph_cell(ui, emoji, "最近使用 · 点选再次插入", outbox));
                    }
                });
                ui.add_space(tokens::SPACE_XS);
                ui.separator();
            }
            ui.weak("应用内为黑白显示;导出 HTML 或粘贴到外部仍是彩色");
        });
    // Esc 关闭:面板开着才走到这里,Esc 就是「收起面板」;不消费 ——
    // 编辑器对 Esc 本就无动作,禅定的 Esc 出口在 draw_zen 里先消费,
    // 输入流顺序天然让「退禅定」优先于「关面板」。
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
        outbox.push(Message::EmojiPickerToggle(false));
    }
    cells
}

/// 网格单元:字符居中 + hover 底色 + tooltip(三名一路,可发现性);
/// `source` 非 None(搜索态)时前置来源分类名。
fn cell(
    ui: &mut egui::Ui,
    entry: &EmojiEntry,
    source: Option<&'static str>,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let tooltip = match source {
        Some(group) => format!(
            "{group} · {} · {} · :{}:",
            entry.name_zh, entry.name_en, entry.shortcode
        ),
        None => format!(
            "{} · {} · :{}:",
            entry.name_zh, entry.name_en, entry.shortcode
        ),
    };
    glyph_cell(ui, entry.char, &tooltip, outbox)
}

/// 字符单元的公共体(网格与「最近使用」共用):32×32 点击区,字符居中,
/// hover 底色 + tooltip,点选发插入消息。
fn glyph_cell(
    ui: &mut egui::Ui,
    glyph: &str,
    tooltip: &str,
    outbox: &mut Vec<Message>,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(CELL, CELL), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(
                rect,
                tokens::RADIUS_SM,
                ui.visuals().widgets.hovered.bg_fill,
            );
        }
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(CELL_FONT),
            ui.visuals().text_color(),
        );
    }
    let response = response.on_hover_text(tooltip);
    if response.clicked() {
        outbox.push(Message::EmojiInserted(glyph.to_owned()));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, RawInput, Rect};

    /// 一帧:画面板(可带走本帧的网格单元矩形)。
    fn frame(
        ctx: &egui::Context,
        state: &mut EmojiPanelState,
        events: Vec<Event>,
        outbox: Option<&mut Vec<Message>>,
    ) -> Vec<egui::Response> {
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 800.0));
        let mut cells = Vec::new();
        let mut sink = Vec::new();
        let outbox = outbox.unwrap_or(&mut sink);
        let output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| cells = panel(ui, state, outbox),
        );
        output.drop_without_applying_deltas();
        cells
    }

    fn click(pos: egui::Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    /// 在 `pos` 处完成一次「移动 → 按下 → 抬起」。
    fn click_at(
        ctx: &egui::Context,
        state: &mut EmojiPanelState,
        pos: egui::Pos2,
        outbox: &mut Vec<Message>,
    ) {
        for events in [
            vec![Event::PointerMoved(pos)],
            vec![click(pos, true)],
            vec![click(pos, false)],
        ] {
            frame(ctx, state, events, Some(outbox));
        }
    }

    /// 明暗两套 visuals 各渲染三帧不 panic(首帧字体注册、后续帧
    /// tessellation 各有冷启动路径,单帧绿不等于帧帧绿,与 icons.rs 的
    /// 手法同款);渲染本身不发自发消息。
    #[test]
    fn panel_renders_in_both_visuals_without_messages() {
        for dark in [true, false] {
            let ctx = egui::Context::default();
            if !dark {
                ctx.set_theme(egui::Theme::Light);
            }
            let mut state = EmojiPanelState::default();
            let mut outbox = Vec::new();
            for _ in 0..3 {
                frame(&ctx, &mut state, Vec::new(), Some(&mut outbox));
            }
            assert!(outbox.is_empty(), "仅渲染不产生消息");
        }
    }

    /// 点网格单元发 `EmojiInserted`(载荷 = 该单元的字符),分类标签可
    /// 切换 —— 不断言渲染结果(字形依赖真实字体),只钉「点击 → 消息」
    /// 与「标签 → 网格换内容」两条链路(emoji-plan §7 #7 的口径)。
    #[test]
    fn clicking_a_cell_and_switching_group_emit_messages() {
        let ctx = egui::Context::default();
        let mut state = EmojiPanelState {
            open: true,
            ..EmojiPanelState::default()
        };
        let mut outbox = Vec::new();

        // sizing pass 三遍(浮窗 widget 前几遍不参与命中),第四遍取
        // 首分类(表情)网格首格的矩形
        let mut first_cell = Rect::NOTHING;
        for step in 0..4 {
            let cells = frame(&ctx, &mut state, Vec::new(), None);
            if step == 3 {
                if let Some(first) = cells.first() {
                    first_cell = first.rect;
                }
            }
        }
        let center = first_cell.center();
        assert!(center.x > 0.0, "拿到了网格首格的位置:{center:?}");

        click_at(&ctx, &mut state, center, &mut outbox);
        assert_eq!(
            outbox,
            vec![Message::EmojiInserted("😀".to_owned())],
            "点首格(笑脸)发出插入消息"
        );

        // 切到「旗帜」分类:网格换表,同一位置点下去的载荷应是旗帜首项
        outbox.clear();
        state.group = 7;
        let cells = frame(&ctx, &mut state, Vec::new(), None);
        let center = cells.first().expect("旗帜分类有网格").rect.center();
        click_at(&ctx, &mut state, center, &mut outbox);
        assert_eq!(
            outbox,
            vec![Message::EmojiInserted("🇨🇳".to_owned())],
            "分类切换后网格换成旗帜表"
        );
    }

    /// Esc 关面板:面板开着时按 Esc 发 `EmojiPickerToggle(false)`。
    #[test]
    fn escape_closes_the_panel() {
        let ctx = egui::Context::default();
        let mut state = EmojiPanelState {
            open: true,
            ..EmojiPanelState::default()
        };
        let mut outbox = Vec::new();
        for events in [
            Vec::new(),
            Vec::new(),
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        ] {
            frame(&ctx, &mut state, events, Some(&mut outbox));
        }
        assert_eq!(outbox, vec![Message::EmojiPickerToggle(false)]);
    }

    /// 搜索(E2):空查询 = 当前分类全表;非空 = 跨分类三路匹配(当前
    /// 分类是「表情」,「火箭」命中旅行分类 —— 单元数 1 证明网格已换源);
    /// 无命中零单元(「无匹配」提示无点击目标)。
    #[test]
    fn search_filters_the_grid_across_groups() {
        let ctx = egui::Context::default();
        let mut state = EmojiPanelState {
            open: true,
            ..EmojiPanelState::default()
        };

        let cells = frame(&ctx, &mut state, Vec::new(), None);
        assert_eq!(
            cells.len(),
            emoji_data::GROUPS[0].entries.len(),
            "空查询显示当前分类全表"
        );

        state.query = "火箭".to_owned();
        let cells = frame(&ctx, &mut state, Vec::new(), None);
        assert_eq!(cells.len(), 1, "跨分类命中唯一:{cells:?}");

        state.query = "查无此物xyz".to_owned();
        let cells = frame(&ctx, &mut state, Vec::new(), None);
        assert!(cells.is_empty(), "无匹配不给可点单元");
    }

    /// 搜索态点选命中单元:发出该枚字符的插入消息(ScrollArea 需要更多
    /// sizing pass,取第 6 帧的单元矩形定位;与 E1 的 4 帧 + 探针同一手法)。
    #[test]
    fn search_result_click_inserts_the_hit() {
        let ctx = egui::Context::default();
        let mut state = EmojiPanelState {
            open: true,
            query: "火箭".to_owned(),
            ..EmojiPanelState::default()
        };
        let mut first = Rect::NOTHING;
        for step in 0..6 {
            let cells = frame(&ctx, &mut state, Vec::new(), None);
            if step == 5 {
                if let Some(cell) = cells.first() {
                    first = cell.rect;
                }
            }
        }
        let center = first.center();
        assert!(center.x > 0.0, "拿到了命中单元的位置:{center:?}");

        let mut outbox = Vec::new();
        click_at(&ctx, &mut state, center, &mut outbox);
        assert_eq!(outbox, vec![Message::EmojiInserted("🚀".to_owned())]);
    }

    /// 「最近使用」行(E2):空态整行隐藏(单元数 = 网格数);有记录时
    /// 尾追可点单元,点选发同样的插入消息 —— 连插多个的二次入口。
    #[test]
    fn recent_row_hides_when_empty_and_inserts_on_click() {
        let ctx = egui::Context::default();
        let mut state = EmojiPanelState {
            open: true,
            ..EmojiPanelState::default()
        };
        let cells = frame(&ctx, &mut state, Vec::new(), None);
        assert_eq!(
            cells.len(),
            emoji_data::GROUPS[0].entries.len(),
            "空态无最近使用行"
        );

        state.recent = vec!["🚀".to_owned(), "🎉".to_owned()];
        let mut last = Rect::NOTHING;
        for step in 0..6 {
            let cells = frame(&ctx, &mut state, Vec::new(), None);
            assert_eq!(
                cells.len(),
                emoji_data::GROUPS[0].entries.len() + 2,
                "第 {step} 帧:网格之外多出两枚最近单元"
            );
            if step == 5 {
                last = cells.last().expect("最近单元存在").rect;
            }
        }
        let mut outbox = Vec::new();
        click_at(&ctx, &mut state, last.center(), &mut outbox);
        assert_eq!(
            outbox,
            vec![Message::EmojiInserted("🎉".to_owned())],
            "点的是最后一枚(🚀 之后的 🎉)"
        );
    }
}
