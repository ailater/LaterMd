//! 长按修饰键的快捷键蒙层(#54):检测状态机(M1)+ 渲染与内容源(M2)。
//!
//! 检测「按住平台主修饰键(Win/Linux = Ctrl、macOS = ⌘)**连续 3.0 秒**」
//! 并给出触发事件;期间按下任何其它键、松开修饰键、窗口失焦,任一发生
//! 即取消且不触发。状态机不依赖 egui(帧输入是三个布尔 + 时刻),接线在
//! `ui::layout::reduce`,与 [`crate::command::poll_shortcuts`] 同层:
//!
//! * 输入扫描必须排在 `poll_shortcuts` / `poll_capture` **之前**——快捷键
//!   消费会从事件流里删掉 Key 事件(`consume_shortcut` 即 remove),后扫
//!   会把「按过 Ctrl+S」看成「只在按 Ctrl」,长按计时被悄悄提前起算;
//! * Holding 帧由 [`ShortcutOverlayState::repaint_wait`] 按剩余时长 `request_repaint_after`
//!   自驱:修饰键按下那一刻之后不再产生任何事件,egui 收敛深度空闲后
//!   3s 到点就没有帧可跑归约(autosave #18 帧饥饿的同型教训);
//! * 取消判定先于到点判定:窗口挂起数分钟后恢复的帧 `now - start` 早已
//!   超过 3s,但失焦帧 egui 会清空自己的 modifiers 快照(egui-winit 同
//!   样清),该帧只能取消,不得拿过期起点补触发(幽灵触发)。
//!
//! 状态挂在 `crate::state::State`(会话级,不持久化):切窗口/切标签不
//! 重建 State,不存在「失忆后拿残影计时」的面。

use eframe::egui;
use std::time::{Duration, Instant};

/// 触发时长(常量,改值不动结构)。
pub const HOLD_DURATION: Duration = Duration::from_secs(3);

/// 一帧的检测输入,由 [`frame_input`] 从 egui 输入汇集。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInput {
    /// 平台主修饰键是否**单独**按住(`Modifiers::command_only`:Win/Linux
    /// = 只按 Ctrl,mac = 只按 ⌘)。叠加 Shift/Alt 是在和弦,不算长按。
    pub modifier_held: bool,
    /// 本帧是否有任何**其它按键**:普通键按下(含按键重复)、文本输入、
    /// Cut/Copy/Paste(winit 把 Ctrl+X/C/V 折叠成这三类事件,不再发对应
    /// Key 事件)、IME 组合。指针与滚轮不算——点标签切页不是按键,长按
    /// 不该被误取消。
    pub other_key: bool,
    /// 窗口是否持有键盘焦点(`InputState::focused`)。
    pub focused: bool,
}

/// 检测状态机一步的产出:蒙层该出现(`Triggered`)/ 该撤下(`Closed`)/
/// 本帧无事件(`None`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Holding → Visible:长按到点。
    Triggered,
    /// Visible → Idle:松开修饰键 / 任何其它按键 / 失焦。
    Closed,
}

/// 长按检测状态。挂在 `State` 上随帧推进(`step` 是唯一驱动入口)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutOverlayState {
    state: HoldState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldState {
    Idle,
    /// 起表时刻;到 `start + HOLD_DURATION` 触发。
    Holding {
        start: Instant,
    },
    /// 已触发,蒙层该可见(`paint` 据此绘制)。
    Visible,
}

impl Default for ShortcutOverlayState {
    fn default() -> Self {
        Self {
            state: HoldState::Idle,
        }
    }
}

impl ShortcutOverlayState {
    /// 推进一帧,返回触发/关闭事件。取消判定(失焦/松开/其它键)一律
    /// 先于到点判定,理由见模块文档(幽灵触发)。
    pub fn step(&mut self, input: FrameInput, now: Instant) -> Outcome {
        let cancelled = !input.focused || !input.modifier_held || input.other_key;
        match self.state {
            HoldState::Idle => {
                // 快捷键帧(修饰键与主键同帧按下)不启动:那是和弦不是长按
                if input.focused && input.modifier_held && !input.other_key {
                    self.state = HoldState::Holding { start: now };
                }
                Outcome::None
            }
            HoldState::Holding { start } => {
                if cancelled {
                    self.state = HoldState::Idle;
                } else if now.duration_since(start) >= HOLD_DURATION {
                    self.state = HoldState::Visible;
                    return Outcome::Triggered;
                }
                Outcome::None
            }
            HoldState::Visible => {
                // 按住不松:稳态 Visible,不重触发(蒙层不闪烁);松开 /
                // 任何其它按键(含 Esc,留给正常按键通路不消费)/ 失焦关闭
                if cancelled {
                    self.state = HoldState::Idle;
                    return Outcome::Closed;
                }
                Outcome::None
            }
        }
    }

    /// 蒙层是否可见(触发后、关闭前)。M2 渲染层将其作为蒙层可见位消费
    /// (`shortcut_overlay::paint` 每帧读它决定画不画)。
    pub fn is_visible(&self) -> bool {
        matches!(self.state, HoldState::Visible)
    }

    /// 是否正在长按计时(Holding)。M2 若画按住进度指示会消费它;M1 里
    /// 只有测试读它(同上)。
    #[cfg(test)]
    pub fn is_holding(&self) -> bool {
        matches!(self.state, HoldState::Holding { .. })
    }

    /// 显式关闭(点击蒙层底,#54 M2):Visible → Idle。松开修饰键 / 其它
    /// 按键 / 失焦三条路径仍走 [`Self::step`];点击与它们语义一致——关闭
    /// 后若修饰键仍按住,重新起表,再长按 3s 才会再触发(用户重新发起查看,
    /// 与 M1 已钉死的按键关闭同款,不做抑制期)。
    pub fn close(&mut self) {
        if matches!(self.state, HoldState::Visible) {
            self.state = HoldState::Idle;
        }
    }

    /// Holding 帧的重绘排程:距触发到点的剩余时长;`None` = 无事可等
    /// (Idle/Visible 不排程,空闲收敛不受影响)。
    pub fn repaint_wait(&self, now: Instant) -> Option<Duration> {
        match self.state {
            HoldState::Holding { start } => {
                Some((start + HOLD_DURATION).saturating_duration_since(now))
            }
            HoldState::Idle | HoldState::Visible => None,
        }
    }

    /// 仅供同 crate 测试注入时间:把进行中长按的起点拨回 `elapsed` 之前
    /// (生产不调用;autosave 测试拨 `last_edit` 的同款手法)。
    #[cfg(test)]
    pub(crate) fn rewind_hold(&mut self, elapsed: Duration) {
        if let HoldState::Holding { start } = &mut self.state {
            *start = Instant::now() - elapsed;
        }
    }
}

// —— M2:内容源(单一事实源)与渲染 ——

/// 蒙层卡片定宽(分组卡片按此宽度流式换列;窗口更窄时被夹到窗内)。
const CARD_W: f32 = 720.0;
/// 蒙层卡片高度上限(内容超出的滚动;窗更矮时被夹到窗内)。
const CARD_H: f32 = 560.0;
/// 卡片头部(标题 + 提示行 + 间距)的估高,列表限高从它扣减。
const CARD_HEADER_H: f32 = 58.0;
/// 暗色主题蒙层底不透明度(黑);亮色主题取 [`SCRIM_ALPHA_LIGHT`]。
/// 两主题各从 `visuals().dark_mode` 推导,不硬编码单个色值(取舍与
/// 观感数值见 decisions-pending #103)。
const SCRIM_ALPHA_DARK: u8 = 160;
/// 亮色主题蒙层底不透明度(白):亮色下正文是深字,弱化同样内容需要更
/// 厚的白,否则底层文字透出来与卡片正文抢辨识。
const SCRIM_ALPHA_LIGHT: u8 = 216;

/// 蒙层一行:命令 + 当前绑定(`None` = 未绑定)。
pub type OverlayRow = (crate::command::Command, Option<crate::keymap::Shortcut>);

/// 蒙层一个分组的数据。
pub type OverlayGroup = (crate::command::CommandGroup, Vec<OverlayRow>);

/// 蒙层的展示数据:分组 → 行。清单来自 [`crate::command::Command::ALL`],键位来自
/// `keymap`(用户可改,`keymap.json` 如实反映)——蒙层不持有第二份命令
/// 表,改键/加命令自动跟随。无绑定命令保留(`None` 行,渲染时弱化标
/// 「未绑定」):口径与设置页「快捷键」一致,全集断言
/// (`overlay_rows_cover_every_command`)也由此成立。
pub fn grouped_rows(keymap: &crate::keymap::Keymap) -> Vec<OverlayGroup> {
    crate::command::CommandGroup::ALL
        .into_iter()
        .map(|group| {
            let rows: Vec<_> = crate::command::Command::ALL
                .iter()
                .filter(|cmd| cmd.group() == group)
                .map(|cmd| (*cmd, keymap.get(*cmd)))
                .collect();
            (group, rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

/// 画蒙层(Visible 态才有输出;挂在 `draw_overlay_dialogs` 同层,三栏与
/// 禅定两条布局路径都会经过——禅定显式放行,#54 任务书 13a 教训)。
///
/// 结构:全窗半透明底(scrim,`Area` 手绘 + 点击感知)盖住下层;中央
/// `Window` 卡片按分组列出全部命令,键位用等宽 `kbd` 小方块,超出高度
/// 内部滚动。**非焦点层**:整棵蒙层没有任何可聚焦控件,不调
/// `request_focus`,编辑器的键盘焦点与输入分毫不动(关闭路径里 Esc 在
/// 归约层消费,点击只作用于 scrim)。
pub fn paint(ui: &mut egui::Ui, state: &mut ShortcutOverlayState, keymap: &crate::keymap::Keymap) {
    if !state.is_visible() {
        return;
    }
    let ctx = ui.ctx().clone();
    let screen = ctx.viewport_rect();

    // 半透明底:点击任一处关闭。与卡片同在默认 Middle 层、先画,卡片后
    // 画盖在其上——点卡片(滚动列表)不误关,点卡片外的蒙层底才关。
    egui::Area::new(egui::Id::new("shortcut-overlay-scrim"))
        .interactable(true)
        .fixed_pos(egui::Pos2::ZERO)
        .show(&ctx, |ui| {
            let dark = ui.visuals().dark_mode;
            let tint = if dark {
                egui::Color32::from_black_alpha(SCRIM_ALPHA_DARK)
            } else {
                egui::Color32::from_white_alpha(SCRIM_ALPHA_LIGHT)
            };
            ui.painter()
                .rect_filled(screen, egui::CornerRadius::ZERO, tint);
            let click = ui.allocate_rect(screen, egui::Sense::click());
            if click.clicked() {
                state.close();
            }
        });

    // 中央卡片:`Area::anchor` 居中(不依赖 Window 的 Resize 状态机,尺寸
    // 完全由内容与 max_height 决定,无头帧间稳定)。分组用两列 Grid 摆放
    // (Grid 列宽由内容收敛,设置页快捷键表同款容器——`horizontal_wrapped`
    // 配 set_width 在无头帧里不收敛,实测会把分组卡排出一行直到屏外)。
    // 窄窗/矮窗夹进视口。无标题栏——蒙层是瞬时速查浮层,标题自绘。
    let available = egui::vec2(
        (screen.width() - 48.0).max(240.0),
        (screen.height() - 96.0).max(240.0),
    );
    let card_w = CARD_W.min(available.x);
    let list_max_h = (CARD_H - CARD_HEADER_H).min(available.y - CARD_HEADER_H);
    egui::Area::new(egui::Id::new("shortcut-overlay-card"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(&ctx, |ui| {
            egui::Frame::default()
                .fill(ui.visuals().window_fill)
                .stroke(ui.visuals().window_stroke)
                .corner_radius(ui.visuals().window_corner_radius)
                .inner_margin(egui::Margin::symmetric(16, 14))
                .show(ui, |ui| {
                    ui.set_min_width(card_w);
                    ui.set_max_width(card_w);
                    ui.strong("快捷键");
                    ui.weak(format!(
                        "按住 {} 满 3 秒唤出;松开、Esc 或点击空白处收起",
                        if cfg!(target_os = "macos") {
                            "⌘"
                        } else {
                            "Ctrl"
                        }
                    ));
                    ui.add_space(crate::ui::tokens::SPACE_SM);
                    egui::ScrollArea::vertical()
                        .id_salt("shortcut-overlay-scroll")
                        .max_height(list_max_h)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let groups = grouped_rows(keymap);
                            egui::Grid::new("shortcut-overlay-groups")
                                .num_columns(2)
                                .spacing(egui::vec2(
                                    crate::ui::tokens::SPACE_MD,
                                    crate::ui::tokens::SPACE_MD,
                                ))
                                .show(ui, |ui| {
                                    for pair in groups.chunks(2) {
                                        group_card(ui, pair[0].0, &pair[0].1);
                                        if let Some((group, rows)) = pair.get(1) {
                                            group_card(ui, *group, rows);
                                        } else {
                                            ui.label("");
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                });
        });
}

/// 一个分组的卡片:组名 + 命令/键位两列。宽窄交给外层两列 Grid 收敛
/// (列宽 = 该列最宽卡)。
fn group_card(
    ui: &mut egui::Ui,
    group: crate::command::CommandGroup,
    rows: &[(crate::command::Command, Option<crate::keymap::Shortcut>)],
) {
    egui::Frame::default()
        .fill(ui.visuals().panel_fill)
        .stroke(ui.visuals().window_stroke)
        .corner_radius(ui.visuals().window_corner_radius)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.strong(group.label());
            ui.add_space(crate::ui::tokens::SPACE_XS);
            egui::Grid::new(egui::Id::new(("overlay-group", group)))
                .num_columns(2)
                .min_col_width(0.0)
                .show(ui, |ui| {
                    for (cmd, shortcut) in rows {
                        ui.label(cmd.label());
                        match shortcut {
                            Some(shortcut) => kbd(ui, &shortcut.platform_text()),
                            None => {
                                ui.weak("未绑定");
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

/// 键位的 kbd 小方块:等宽字体 + 键帽底色。底/描边从 visuals 推导,两
/// 主题各自的对比度成立(亮色下键帽是浅灰底深字,暗色下深灰底浅字)。
fn kbd(ui: &mut egui::Ui, text: &str) {
    egui::Frame::default()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(egui::Stroke::new(1.0, ui.visuals().weak_text_color()))
        .corner_radius(crate::ui::tokens::RADIUS_SM)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text)
                    .monospace()
                    .size(crate::ui::tokens::FONT_SM),
            );
        });
}

/// 从 egui 输入汇集一帧的 [`FrameInput`]。只读不消费——蒙层检测绝不改变
/// 既有按键行为(#54 否决线)。
pub fn frame_input(ctx: &egui::Context) -> FrameInput {
    ctx.input(|input| FrameInput {
        modifier_held: input.modifiers.command_only(),
        other_key: input.events.iter().any(is_other_key),
        focused: input.focused,
    })
}

/// 一个事件是否算「按了修饰键之外的键」。
fn is_other_key(event: &egui::Event) -> bool {
    use egui::Event;
    match event {
        // 修饰键自身的物理按键事件(egui 0.36 起透传)不算:否则 Ctrl 按下
        // 那一帧就会被当成「其它按键」,长按被掐灭在 Idle
        Event::Key {
            key, pressed: true, ..
        } if !is_modifier_key(*key) => true,
        // winit 把 Ctrl+X/C/V 折叠成这三类,不再发对应 Key 事件
        Event::Cut | Event::Copy | Event::Paste(_) => true,
        Event::Text(_) => true,
        // IME 组合态(候选输入中)视为打字,防误触(decisions-pending #102)
        Event::Ime(_) => true,
        _ => false,
    }
}

/// 修饰键的物理键位(左右两侧 × Shift/Ctrl/Alt/Super)。
fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LaterMdApp;
    use egui::{Event, FullOutput, ImeEvent, Key, Modifiers, Pos2, RawInput};

    /// 纯函数测试用:全部时刻锚定同一基准 + 偏移(各自取 `Instant::now()`
    /// 会有微秒级漂移,精确断言会假阴性)。
    fn at(offset_secs: f64) -> Instant {
        static BASE: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        *BASE.get_or_init(Instant::now) + Duration::from_secs_f64(offset_secs)
    }

    /// 「修饰键单独按住」的帧输入。
    fn held() -> FrameInput {
        FrameInput {
            modifier_held: true,
            other_key: false,
            focused: true,
        }
    }

    // ---- 纯状态机 ----

    /// 3s 整点触发:2.9s 不触发,恰好 3.0s 触发。
    #[test]
    fn triggers_at_exactly_three_seconds() {
        let mut overlay = ShortcutOverlayState::default();
        assert_eq!(overlay.step(held(), at(0.0)), Outcome::None);
        assert!(overlay.is_holding());
        assert_eq!(overlay.step(held(), at(3.0)), Outcome::Triggered);
        assert!(overlay.is_visible(), "到点帧即进入 Visible");
    }

    /// 2.9s 不触发(边界下沿)。
    #[test]
    fn does_not_trigger_before_three_seconds() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        assert_eq!(overlay.step(held(), at(2.9)), Outcome::None);
        assert!(overlay.is_holding() && !overlay.is_visible());
    }

    /// 按下任何其它键即取消;此后仍按住修饰键则**重新起表**,旧起点作废
    /// (取消点之后 2.9s 不触发、3.0s 才触发)。
    #[test]
    fn other_key_cancels_and_restarts_countdown() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        let with_key = FrameInput {
            other_key: true,
            ..held()
        };
        assert_eq!(overlay.step(with_key, at(1.0)), Outcome::None);
        assert!(!overlay.is_holding(), "其它按键当帧取消");

        // 旧起点已作废:从取消帧重新起表(同帧按住 → 新 Holding)
        assert_eq!(overlay.step(held(), at(3.0)), Outcome::None);
        assert!(overlay.is_holding(), "重新按住重新计时");
        assert_eq!(
            overlay.step(held(), at(5.9)),
            Outcome::None,
            "新表 2.9s 不触发"
        );
        assert_eq!(overlay.step(held(), at(6.0)), Outcome::Triggered);
    }

    /// 松开修饰键即取消。
    #[test]
    fn releasing_modifier_cancels() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        let released = FrameInput {
            modifier_held: false,
            ..held()
        };
        assert_eq!(overlay.step(released, at(2.0)), Outcome::None);
        assert!(!overlay.is_holding() && !overlay.is_visible());
    }

    /// 失焦即取消且不触发——即使 `now` 早已越过 3s(取消判定先于到点
    /// 判定,挂起恢复的帧不得拿过期起点补触发)。
    #[test]
    fn focus_loss_cancels_without_ghost_trigger() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        let unfocused = FrameInput {
            focused: false,
            ..held()
        };
        assert_eq!(overlay.step(unfocused, at(10.0)), Outcome::None);
        assert!(
            !overlay.is_holding() && !overlay.is_visible(),
            "挂起 10s 后恢复的帧只取消,不补触发"
        );
    }

    /// Idle 不起表的两条:快捷键帧(修饰键与其它键同帧)、失焦帧。
    #[test]
    fn idle_ignores_chord_and_unfocused_frames() {
        let mut overlay = ShortcutOverlayState::default();
        let chord = FrameInput {
            other_key: true,
            ..held()
        };
        overlay.step(chord, at(0.0));
        assert!(!overlay.is_holding(), "Ctrl+S 同帧按下不起表");

        let unfocused = FrameInput {
            focused: false,
            ..held()
        };
        overlay.step(unfocused, at(0.1));
        assert!(!overlay.is_holding(), "失焦帧不起表");
    }

    /// 长按超过 3s 后继续按住:保持 Visible 不重触发(蒙层不闪烁)。
    #[test]
    fn visible_holds_while_modifier_stays_down() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        assert_eq!(overlay.step(held(), at(3.0)), Outcome::Triggered);
        for t in [3.5, 7.0, 30.0] {
            assert_eq!(overlay.step(held(), at(t)), Outcome::None, "t={t}");
        }
        assert!(overlay.is_visible());
    }

    /// Visible 的三条关闭路径:松开 / 任何其它按键 / 失焦;关闭后可再触发。
    #[test]
    fn visible_closes_on_release_key_or_unfocus() {
        let released = FrameInput {
            modifier_held: false,
            ..held()
        };
        let with_key = FrameInput {
            other_key: true,
            ..held()
        };
        let unfocused = FrameInput {
            focused: false,
            ..held()
        };

        let mut overlay = ShortcutOverlayState::default();
        overlay.step(held(), at(0.0));
        overlay.step(held(), at(3.0));
        assert_eq!(overlay.step(released, at(4.0)), Outcome::Closed);
        assert!(!overlay.is_visible());

        // 关闭后再按住 3s 可再次触发(松开关闭 → Idle → 新 Holding)
        overlay.step(held(), at(5.0));
        assert_eq!(overlay.step(held(), at(8.0)), Outcome::Triggered);
        assert_eq!(overlay.step(with_key, at(9.0)), Outcome::Closed);

        overlay.step(held(), at(10.0));
        overlay.step(held(), at(13.0));
        assert_eq!(overlay.step(unfocused, at(14.0)), Outcome::Closed);
    }

    /// repaint 排程:Holding 给「距 3s 的剩余时长」,Idle/Visible 不排程。
    #[test]
    fn repaint_wait_tracks_remaining_time() {
        let mut overlay = ShortcutOverlayState::default();
        assert_eq!(overlay.repaint_wait(at(0.0)), None, "Idle 不排程");
        overlay.step(held(), at(10.0));
        assert_eq!(
            overlay.repaint_wait(at(10.5)),
            Some(Duration::from_secs_f64(2.5)),
            "剩余 = 3s - 已按时长"
        );
        assert_eq!(
            overlay.repaint_wait(at(13.0)),
            Some(Duration::ZERO),
            "到点帧剩余为 0(立即要帧,下一帧触发后自然清)"
        );
        overlay.step(held(), at(13.0));
        assert_eq!(overlay.repaint_wait(at(13.0)), None, "Visible 不排程");
    }

    // ---- egui 帧输入汇集 ----

    /// 跑一帧空 UI 只为喂事件,随后在闭包内断言汇集结果。
    fn collect(focused: bool, events: Vec<Event>) -> FrameInput {
        let ctx = egui::Context::default();
        let mut collected = None;
        ctx.run_ui(
            RawInput {
                focused,
                events,
                ..Default::default()
            },
            |ui| collected = Some(frame_input(ui.ctx())),
        )
        .drop_without_applying_deltas();
        collected.unwrap()
    }

    fn key_press(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// 修饰键按下帧(ModifiersChanged + 修饰键自身物理 Key 事件)不算
    /// 「其它按键」,且 `command_only` 认定平台主修饰键单独按住。
    #[test]
    fn modifier_press_frame_is_not_other_key() {
        let input = collect(
            true,
            vec![
                key_press(Key::ControlLeft, Modifiers::COMMAND),
                Event::ModifiersChanged(Modifiers::COMMAND),
            ],
        );
        assert_eq!(
            input,
            FrameInput {
                modifier_held: true,
                other_key: false,
                focused: true,
            }
        );
    }

    /// 和弦与叠加修饰键不算长按:Ctrl+Shift 的 `command_only` 为假;普通键
    /// /文本/Cut/Copy/Paste/IME 组合都算「其它按键」。
    #[test]
    fn chords_text_clipboard_and_ime_count_as_other_key() {
        let ctrl_shift = collect(
            true,
            vec![Event::ModifiersChanged(
                Modifiers::COMMAND | Modifiers::SHIFT,
            )],
        );
        assert!(!ctrl_shift.modifier_held, "叠加 Shift 是和弦");

        let letter = collect(
            true,
            vec![
                key_press(Key::S, Modifiers::COMMAND),
                Event::ModifiersChanged(Modifiers::COMMAND),
            ],
        );
        assert!(letter.other_key && letter.modifier_held);

        assert!(collect(true, vec![Event::Text("中".into())]).other_key);
        for event in [Event::Cut, Event::Copy, Event::Paste("x".into())] {
            let label = format!("{event:?}");
            assert!(collect(true, vec![event]).other_key, "{label}");
        }
        assert!(
            collect(
                true,
                vec![Event::Ime(ImeEvent::Preedit {
                    text: "你".into(),
                    active_range_chars: None,
                })]
            )
            .other_key
        );
    }

    /// 指针与滚轮不算「其它按键」:按住修饰键期间点标签/滚动不误取消。
    #[test]
    fn pointer_and_wheel_are_not_other_key() {
        let input = collect(
            true,
            vec![
                Event::ModifiersChanged(Modifiers::COMMAND),
                Event::PointerButton {
                    pos: Pos2::ZERO,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::COMMAND,
                },
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, 3.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::COMMAND,
                },
            ],
        );
        assert!(input.modifier_held && !input.other_key);
    }

    /// `RawInput::focused = false` 透传进帧输入(失焦取消的数据源)。
    #[test]
    fn focus_flag_flows_into_frame_input() {
        assert!(!collect(false, vec![Event::ModifiersChanged(Modifiers::COMMAND)]).focused);
    }

    // ---- 接线(经 LaterMdApp::reduce 的整帧) ----

    /// 一帧归约输出的最早重绘等待(`Duration::MAX` = 无任何未偿付要帧)。
    fn min_repaint_delay(output: &FullOutput) -> Duration {
        output
            .viewport_output
            .values()
            .map(|viewport| viewport.repaint_delay)
            .min()
            .unwrap()
    }

    fn frame(
        app: &mut LaterMdApp,
        ctx: &egui::Context,
        focused: bool,
        events: Vec<Event>,
    ) -> Duration {
        let output = ctx.run_ui(
            RawInput {
                focused,
                events,
                ..Default::default()
            },
            |ui| app.reduce(ui.ctx()),
        );
        let delay = min_repaint_delay(&output);
        output.drop_without_applying_deltas();
        delay
    }

    /// #18 教训的落法:按住 Ctrl 后**无任何输入事件**的帧,归约仍按「距
    /// 3s 的剩余时长」要帧(失焦/无输入时 egui 深度空闲不来帧,倒计时
    /// 只能靠 repaint 排程推进);拨回 3.1s 前,无事件帧照常触发。
    #[test]
    fn hold_drives_repaint_and_triggers_without_input_events() {
        let mut app = LaterMdApp::default();
        let ctx = egui::Context::default();

        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::ModifiersChanged(Modifiers::COMMAND)],
        );
        assert!(app.state.shortcut_overlay.is_holding(), "按下帧起表");

        // 帧 2-4:越过视口首帧 settle(约两帧 0ns)后,无输入事件帧的排程
        // 应恰为「距 3s 到点的剩余时长」
        let mut delay = None;
        for _ in 2..=4 {
            delay = Some(frame(&mut app, &ctx, true, Vec::new()));
        }
        let delay = delay.unwrap();
        assert!(
            delay > Duration::from_secs(2) && delay <= HOLD_DURATION,
            "排程落在触发到点上(不靠输入事件驱动):{delay:?}"
        );

        // 拨回 3.1s 前模拟按住已久(不真等 3s):无事件帧照常触发
        app.state
            .shortcut_overlay
            .rewind_hold(Duration::from_secs_f64(3.1));
        frame(&mut app, &ctx, true, Vec::new());
        assert!(
            app.state.shortcut_overlay.is_visible(),
            "倒计时推进不依赖输入事件"
        );

        // Visible 不排程:越过 settle 后回到深度空闲,不引入常驻轮询
        let mut delay = None;
        for _ in 0..3 {
            delay = Some(frame(&mut app, &ctx, true, Vec::new()));
        }
        assert_eq!(delay, Some(Duration::MAX), "触发后不再安排任何重绘");
    }

    /// 失焦帧(真实事件流形态:WindowFocused(false) 会顺带清空 egui 的
    /// modifiers 快照)取消长按,且不触发;恢复焦点的帧不拿旧起点补触发。
    #[test]
    fn focus_loss_frame_cancels_hold() {
        let mut app = LaterMdApp::default();
        let ctx = egui::Context::default();
        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::ModifiersChanged(Modifiers::COMMAND)],
        );
        assert!(app.state.shortcut_overlay.is_holding());

        app.state
            .shortcut_overlay
            .rewind_hold(Duration::from_secs_f64(2.9));
        frame(&mut app, &ctx, false, vec![Event::WindowFocused(false)]);
        assert!(
            !app.state.shortcut_overlay.is_holding() && !app.state.shortcut_overlay.is_visible(),
            "失焦帧取消且不触发"
        );

        frame(&mut app, &ctx, true, Vec::new());
        assert!(
            !app.state.shortcut_overlay.is_holding(),
            "恢复焦点的帧(modifiers 已被 egui 清空)不拿旧起点补触发"
        );
    }

    /// 快捷键行为不被检测改变(否决线):Ctrl+S 同帧仍正常落盘,且该帧
    /// 的按键事件在消费**之前**已被看见——长按不启动;下一帧起(只剩修
    /// 饰键按住)才重新起表。文档挂真路径:未命名文档的 Ctrl+S 会弹同步
    /// rfd 对话框,无头环境里永久阻塞(autosave 测试给真路径的同款手法)。
    #[test]
    fn shortcut_press_still_fires_and_does_not_start_hold() {
        let dir =
            std::env::temp_dir().join(format!("latermd-overlay-shortcut-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("doc.md");

        let mut app = LaterMdApp::default();
        app.state.tabs.current_mut().document.path = Some(doc.clone());
        app.state
            .tabs
            .current_mut()
            .editor
            .replace_all("被长按检测看着保存的正文");
        let ctx = egui::Context::default();

        let output = ctx.run_ui(
            RawInput {
                focused: true,
                events: vec![
                    key_press(Key::S, Modifiers::COMMAND),
                    Event::ModifiersChanged(Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |ui| app.reduce(ui.ctx()),
        );
        output.drop_without_applying_deltas();

        assert_eq!(
            std::fs::read_to_string(&doc).unwrap(),
            "被长按检测看着保存的正文",
            "Ctrl+S 照常触发保存——检测只读不消费,既有快捷键行为不变"
        );
        assert!(
            !app.state.shortcut_overlay.is_holding(),
            "和弦帧不启动长按(按键事件在消费前已被看见)"
        );

        // 下一帧无事件(修饰键仍按住,Context 级 modifiers 快照仍在):起表
        frame(&mut app, &ctx, true, Vec::new());
        assert!(app.state.shortcut_overlay.is_holding());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 按住中点标签/滚轮(指针事件,不失焦)不误取消长按。
    #[test]
    fn pointer_events_do_not_cancel_hold() {
        let mut app = LaterMdApp::default();
        let ctx = egui::Context::default();
        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::ModifiersChanged(Modifiers::COMMAND)],
        );
        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::PointerButton {
                pos: Pos2::ZERO,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::COMMAND,
            }],
        );
        assert!(
            app.state.shortcut_overlay.is_holding(),
            "指针按下不是按键,长按不误取消"
        );
    }

    // ---- M2:内容源(单一事实源)与关闭 ----

    /// 蒙层分组非空且命令集 == 注册表全集:展开后与 `Command::ALL` **集合**
    /// 相等(顺序按组重排,组内保持 ALL 相对序),没有第二份清单可漂移。
    #[test]
    fn overlay_rows_cover_every_command() {
        let rows = super::grouped_rows(&crate::keymap::Keymap::builtin());
        assert!(!rows.is_empty(), "分组非空");
        let flat: Vec<_> = rows
            .iter()
            .flat_map(|(_, rows)| rows.iter().map(|r| r.0))
            .collect();
        // 集合相等:条数相同 + ALL 每条都在(分组按 group() 单值过滤,无重复)
        assert_eq!(
            flat.len(),
            crate::command::Command::ALL.len(),
            "蒙层条数 = 注册表全集条数"
        );
        for cmd in crate::command::Command::ALL {
            assert!(flat.contains(&cmd), "{cmd:?} 不在蒙层清单里");
        }
        for (group, group_rows) in &rows {
            assert!(!group_rows.is_empty(), "{group:?} 空组不该产出");
            for (cmd, _) in group_rows {
                assert_eq!(cmd.group(), *group, "{cmd:?} 归组与分组过滤一致");
            }
            // 组内保持 ALL 的相对序(展示稳定,不随枚举值漂移)
            let in_all: Vec<_> = crate::command::Command::ALL
                .iter()
                .filter(|cmd| cmd.group() == *group)
                .collect();
            let in_group: Vec<_> = group_rows.iter().map(|(cmd, _)| cmd).collect();
            assert_eq!(in_group, in_all, "{group:?} 组内序 = ALL 相对序");
        }
    }

    /// 用户自定义绑定如实反映:改绑后行里是新键位,清除后是未绑定行;
    /// 出厂表里 Save 是 Ctrl+S(改绑断言的对照)。
    #[test]
    fn overlay_rows_follow_custom_keymap() {
        let mut keymap = crate::keymap::Keymap::builtin();
        keymap.set(
            crate::command::Command::Save,
            Some(crate::keymap::Shortcut {
                modifiers: Modifiers::COMMAND,
                key: Key::K,
            }),
        );
        keymap.set(crate::command::Command::Open, None);
        let rows = super::grouped_rows(&keymap);
        let row = |cmd: crate::command::Command| {
            rows.iter()
                .flat_map(|(_, rows)| rows.iter())
                .find(|(c, _)| *c == cmd)
                .and_then(|(_, shortcut)| *shortcut)
        };
        assert_eq!(row(crate::command::Command::Save), {
            Some(crate::keymap::Shortcut {
                modifiers: Modifiers::COMMAND,
                key: Key::K,
            })
        });
        assert_eq!(row(crate::command::Command::Open), None, "清除后未绑定");
        // 对照:出厂 Save = Ctrl+S(不是 K)
        assert_eq!(
            super::grouped_rows(&crate::keymap::Keymap::builtin())
                .iter()
                .flat_map(|(_, rows)| rows.iter())
                .find(|(c, _)| *c == crate::command::Command::Save)
                .and_then(|(_, s)| *s),
            Some(crate::keymap::Shortcut {
                modifiers: Modifiers::COMMAND,
                key: Key::S,
            })
        );
    }

    /// `close()`:Visible 关闭、Idle 无副作用;关闭后重新长按 3s 可再触发
    /// (与 step 的重起表语义一致)。
    #[test]
    fn close_dismisses_and_allows_retrigger() {
        let mut overlay = ShortcutOverlayState::default();
        overlay.close();
        assert_eq!(
            overlay.step(held(), at(0.0)),
            Outcome::None,
            "Idle 下 close 无副作用"
        );

        overlay.step(held(), at(10.0));
        overlay.step(held(), at(13.0));
        assert!(overlay.is_visible());
        overlay.close();
        assert!(!overlay.is_visible(), "点击关闭");

        overlay.step(held(), at(14.0));
        assert_eq!(
            overlay.step(held(), at(16.9)),
            Outcome::None,
            "重起表 2.9s 不触发"
        );
        assert_eq!(overlay.step(held(), at(17.0)), Outcome::Triggered);
    }

    /// Esc 关闭路径(归约层消费):蒙层可见帧的 Esc 被 consume_key 拿走,
    /// 下层(禅定退出/emoji 关闭/查找条)当帧不可达;蒙层本帧关闭。
    #[test]
    fn escape_is_consumed_while_overlay_visible() {
        let mut app = LaterMdApp::default();
        let ctx = egui::Context::default();
        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::ModifiersChanged(Modifiers::COMMAND)],
        );
        app.state
            .shortcut_overlay
            .rewind_hold(Duration::from_secs_f64(3.1));
        frame(&mut app, &ctx, true, Vec::new());
        assert!(app.state.shortcut_overlay.is_visible());

        let mut escape_gone = None;
        let output = ctx.run_ui(
            RawInput {
                focused: true,
                events: vec![
                    key_press(Key::Escape, Modifiers::COMMAND),
                    Event::ModifiersChanged(Modifiers::COMMAND),
                ],
                ..Default::default()
            },
            |ui| {
                app.reduce(ui.ctx());
                // 消费后本帧输入流里不再有 Esc 按下事件
                escape_gone = Some(!ui.ctx().input(|input| {
                    input.events.iter().any(|event| {
                        matches!(
                            event,
                            Event::Key {
                                key: Key::Escape,
                                pressed: true,
                                ..
                            }
                        )
                    })
                }));
            },
        );
        output.drop_without_applying_deltas();
        assert!(escape_gone.unwrap(), "Esc 已被蒙层消费,下层看不见");
        assert!(!app.state.shortcut_overlay.is_visible(), "蒙层随 Esc 关闭");
    }

    /// 蒙层存在期间输入不受影响(否决线):Visible 态下按 Ctrl+S,保存
    /// 照常落盘(蒙层不吞键),蒙层随该按键关闭。文档挂真路径:未命名
    /// 文档的 Ctrl+S 会弹同步 rfd 对话框,无头环境里永久阻塞(既有测试
    /// 的同款手法)。
    #[test]
    fn overlay_visible_does_not_swallow_shortcuts() {
        let dir = std::env::temp_dir().join(format!("latermd-overlay-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("doc.md");

        let mut app = LaterMdApp::default();
        app.state.tabs.current_mut().document.path = Some(doc.clone());
        app.state
            .tabs
            .current_mut()
            .editor
            .replace_all("蒙层存在期间按 Ctrl+S 的正文");
        let ctx = egui::Context::default();
        frame(
            &mut app,
            &ctx,
            true,
            vec![Event::ModifiersChanged(Modifiers::COMMAND)],
        );
        app.state
            .shortcut_overlay
            .rewind_hold(Duration::from_secs_f64(3.1));
        frame(&mut app, &ctx, true, Vec::new());
        assert!(app.state.shortcut_overlay.is_visible(), "先让蒙层出现");

        frame(
            &mut app,
            &ctx,
            true,
            vec![
                key_press(Key::S, Modifiers::COMMAND),
                Event::ModifiersChanged(Modifiers::COMMAND),
            ],
        );
        assert_eq!(
            std::fs::read_to_string(&doc).unwrap(),
            "蒙层存在期间按 Ctrl+S 的正文",
            "蒙层不吞按键:命令快捷键照常触发"
        );
        assert!(!app.state.shortcut_overlay.is_visible(), "S 按下即关蒙层");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
