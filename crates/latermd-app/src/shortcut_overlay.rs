//! 长按修饰键的快捷键蒙层(#54 M1):纯函数检测状态机。
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
    /// 已触发,蒙层该可见(M2 渲染层从这里长出来)。
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

    /// 蒙层是否可见(触发后、关闭前)。M2 渲染层将其作为蒙层可见位
    /// 消费;M1 里只有测试读它,故 cfg(test)(M2 接线时摘掉)。
    #[cfg(test)]
    pub fn is_visible(&self) -> bool {
        matches!(self.state, HoldState::Visible)
    }

    /// 是否正在长按计时(Holding)。M2 若画按住进度指示会消费它;M1 里
    /// 只有测试读它(同上)。
    #[cfg(test)]
    pub fn is_holding(&self) -> bool {
        matches!(self.state, HoldState::Holding { .. })
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
}
