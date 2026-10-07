//! 打字机模式(#64 M1):光标行滚动保持在视口固定位置的决策面。
//!
//! 决策与接管记账是**纯函数/纯数据**(不 import egui 的部分照 `ui::minimap`
//! 的分层先例:纯数据在这里,落地在 `ui::editor` / `live` 的 ScrollArea
//! 闭包内)。三件套:
//!
//! * [`scroll_delta`] —— 一次滚动决策:光标行顶相对视口顶的 y 偏离目标线
//!   ([`ANCHOR_RATIO`] = 1/3)超过死区([`DEAD_ZONE_ROWS`] 行)时,返回把
//!   光标行拉回目标线所需的滚动量;死区内返回 `None`(避免同一屏内的小
//!   位移逐帧微跳)。文首/文末的偏移钳位不做在这里 —— egui
//!   `ScrollArea::end` 对 offset 的最终钳制(`scroll_area.rs` 的
//!   `max(0.0)`/`min(max_offset)`)天然兜底,打字机只表意图。
//! * [`step`] —— 接管状态机:用户主动滚动(滚轮/拖滚动条)置入
//!   `Phase::Override`(打字机暂停),编辑/光标移动帧恢复 [`Phase::Follow`]。
//!   恢复口径选「直到下次编辑/光标移动再恢复」(任务书两选项之一),取舍
//!   全文见 docs/decisions-pending.md #121。
//! * [`Memory`] —— 每标签挂 egui temp 的记忆(phase + 上一帧偏移 + 上一帧
//!   修订号 + 上一帧开关值),供「用户滚动 = 本帧偏移变化且非打字机自身
//!   落地」「编辑帧 = 修订号前进」两类判定;键 [`source_memory_id`] /
//!   [`live_memory_id`] 两模式分槽,互不串帧。
//!
//! 落地手法(与 #55 minimap `land` 同一族):在内容坐标系放一枚单像素
//! 矩形,`scroll_to_rect_animation(.., Some(Align::TOP), ScrollAnimation::
//! none())` 一次到位。TOP 对齐的换算(`egui 0.36.2 scroll_area.rs`
//! end():`target_offset = rect.top − content_min_rect.top − spacing`)化简
//! 后,把矩形顶放在 `光标行屏幕 y − 目标线` 处即落到 `当前偏移 + 漂移量`
//! —— 全程只用屏幕坐标,不需要读当前偏移。

use eframe::egui;

/// 光标行的目标线:视口高度的 1/3(任务书建议「中 1/3」;打字推进时
/// 下方留 2/3 视口的后续内容,与主流打字机滚动一致)。
pub const ANCHOR_RATIO: f32 = 1.0 / 3.0;

/// 死区(行):光标行顶与目标线的偏差在 ±[`DEAD_ZONE_ROWS`] 行内不滚,
/// 避免行内小位移逐帧微跳;打字换行/跨行导航超出即对齐,行为是「每越带
/// 一次滚一次」,不是每键必滚。
pub const DEAD_ZONE_ROWS: f32 = 1.5;

/// 偏移变化判「滚动发生过」的静差(px):浮点布局噪声不误记接管。
pub const MOVED_EPSILON: f32 = 0.5;

/// 打字机落地后的宽限帧数:egui 的程序化滚动是「本帧 end 记目标、下一
/// 帧 begin() 应用 offset、再次帧布局反映」(落地用 `ScrollAnimation::
/// none`,零宽跨度、offset 一步到位)—— 落地后的 offset 跳变与应用帧
/// 抖动若不豁免,会被 offset diff 误记成用户接管(打字机自己把自己暂
/// 停)。4 帧覆盖「记目标 + 应用 + 布局反映 + 余量」;连续打字逐帧续期,
/// 窗口实际不生效。
pub const LAND_GRACE_FRAMES: u32 = 4;

/// 接管状态。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Phase {
    /// 跟随中:编辑/光标移动帧把光标行滚向目标带。
    #[default]
    Follow,
    /// 接管中:用户主动滚动置入,打字机暂停;下一次编辑/光标移动帧恢复
    /// (同帧即滚,见 [`step`])。
    Override,
}

/// 状态机的输入事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    /// 编辑或光标移动(打字/IME/undo/redo/键盘导航/程序写回):恢复跟随。
    Edit,
    /// 用户主动滚动:临时接管。
    UserScroll,
}

/// 一步转移。恢复是「下一事件即恢复」口径:Override 态吃到的第一个编辑
/// 事件当帧转回 Follow 并照常滚动 —— 接管只 pauses 到用户重新动笔为止,
/// 不做 N 秒静默计时(取舍登记 decisions-pending #121)。
pub fn step(phase: Phase, turn: Turn) -> Phase {
    // 转移只由事件决定(Edit 恒回 Follow、UserScroll 恒入 Override),
    // 与当前态无关 —— `phase` 形参是状态机的记账签名,保留以表达
    // 「一步转移」的调用形态。
    let _ = phase;
    match turn {
        Turn::Edit => Phase::Follow,
        Turn::UserScroll => Phase::Override,
    }
}

/// 一次滚动决策的输入(全为**视口相对量**:不需要绝对偏移与内容总高,
/// 闭包内 `ui.clip_rect()` 直出;文首/文末钳位交给 egui end())。
#[derive(Debug, Clone, Copy)]
pub struct ScrollAsk {
    /// 光标行顶相对视口顶的 y(px,向下为正)。
    pub view_y: f32,
    /// 光标行高(px),死区按行计。
    pub row_h: f32,
    /// 视口高(px)。
    pub viewport: f32,
}

/// 决策:`Some(delta)` = 视口应向下滚这么多才能把光标行顶放回目标线;
/// `None` = 死区内(或输入退化),不滚。
pub fn scroll_delta(ask: &ScrollAsk) -> Option<f32> {
    if ask.viewport <= 0.0 || ask.row_h <= 0.0 || !ask.view_y.is_finite() {
        return None;
    }
    let anchor = ask.viewport * ANCHOR_RATIO;
    let drift = ask.view_y - anchor;
    if drift.abs() <= DEAD_ZONE_ROWS * ask.row_h {
        return None;
    }
    Some(drift)
}

/// 每标签每模式的打字机记忆(egui temp;id 见 [`source_memory_id`] /
/// [`live_memory_id`])。
#[derive(Debug, Clone, Copy, Default)]
pub struct Memory {
    /// 接管状态机当前态。
    pub phase: Phase,
    /// 上一帧 ScrollArea 落定偏移:「本帧偏移动了且不是打字机自己滚的」
    /// 即记一次用户接管。关闭态不更新,重开首帧可能误差一次(该帧若非
    /// 编辑帧,记一次无害的 Override,下次编辑即恢复)。
    pub last_offset: f32,
    /// 上一帧缓冲修订号:`None` = 还没见过。修订号前进 = 编辑帧(打字/
    /// IME/undo/redo/程序写入一网打尽,不逐事件枚举 —— TextEdit 会把
    /// 已消费的按键从事件流移除,事件枚举在 show() 之后不可靠)。
    pub last_rev: Option<u64>,
    /// 上一帧开关值:关闭→开启的翻转帧视为一次编辑触发(开启即对齐一次,
    /// 语义上「开了打字机」就该把光标行带到目标带)。
    pub enabled: bool,
    /// 打字机自身落地后的动画宽限余量(帧):落地帧置 [`LAND_GRACE_FRAMES`],
    /// 逐帧递减;宽限内的偏移变化不算用户接管。连续打字逐帧续期。
    pub land_grace: u32,
}

/// 源码模式的记忆键(每标签一份)。
pub fn source_memory_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with(("typewriter", "source"))
}

/// Live 模式的记忆键(每标签一份;与源码模式分槽,模式切换不串帧)。
pub fn live_memory_id(editor_id: egui::Id) -> egui::Id {
    editor_id.with(("typewriter", "live"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 行高 20px、视口 600px 的标准档:目标线 = 200,死区 = ±30。
    fn ask(view_y: f32) -> ScrollAsk {
        ScrollAsk {
            view_y,
            row_h: 20.0,
            viewport: 600.0,
        }
    }

    /// 目标线 1/3:视口 600 → 光标行顶应停在 200。
    #[test]
    fn anchor_is_one_third_of_viewport() {
        assert!((600.0 * ANCHOR_RATIO - 200.0).abs() < f32::EPSILON);
    }

    /// 死区内不滚:目标线 ±1.5 行(±30px)的漂移一律 None(边界含)。
    #[test]
    fn dead_zone_returns_none() {
        assert_eq!(scroll_delta(&ask(200.0)), None, "正落在目标线");
        assert_eq!(
            scroll_delta(&ask(170.0)),
            None,
            "目标线上方恰 1.5 行(边界含)"
        );
        assert_eq!(
            scroll_delta(&ask(230.0)),
            None,
            "目标线下方恰 1.5 行(边界含)"
        );
        assert_eq!(scroll_delta(&ask(169.0)), Some(-31.0), "上越带,向上滚");
        assert_eq!(scroll_delta(&ask(231.0)), Some(31.0), "下越带,向下滚");
    }

    /// 越带的滚动量 = 漂移量(把光标行顶精确放回目标线)。
    #[test]
    fn out_of_band_returns_drift() {
        assert_eq!(scroll_delta(&ask(320.0)), Some(120.0));
        assert_eq!(scroll_delta(&ask(80.0)), Some(-120.0));
    }

    /// 退化输入不滚(首帧无度量的防御:视口/行高未知时静默)。
    #[test]
    fn degenerate_inputs_return_none() {
        assert_eq!(
            scroll_delta(&ScrollAsk {
                view_y: 0.0,
                row_h: 20.0,
                viewport: 0.0
            }),
            None
        );
        assert_eq!(
            scroll_delta(&ScrollAsk {
                view_y: 0.0,
                row_h: 0.0,
                viewport: 600.0
            }),
            None
        );
        assert_eq!(
            scroll_delta(&ScrollAsk {
                view_y: f32::NAN,
                row_h: 20.0,
                viewport: 600.0
            }),
            None
        );
    }

    /// 文末/文首钳位不在纯函数里:越带总是返回全额漂移,钳给 egui
    /// end() 的 offset 钳制兜底(结构红线,防双重钳位口径漂移)。
    #[test]
    fn clamping_is_not_the_pure_functions_job() {
        // 光标在文档头附近(target 为负)也返回漂移,落地端钳到 0
        assert_eq!(scroll_delta(&ask(10.0)), Some(-190.0));
    }

    /// 接管状态机的转移表:Edit 恒回 Follow;UserScroll 恒入 Override;
    /// 同态转移幂等。
    #[test]
    fn override_state_machine_table() {
        let table = [
            (Phase::Follow, Turn::Edit, Phase::Follow),
            (Phase::Follow, Turn::UserScroll, Phase::Override),
            (Phase::Override, Turn::Edit, Phase::Follow),
            (Phase::Override, Turn::UserScroll, Phase::Override),
        ];
        for (phase, turn, want) in table {
            assert_eq!(step(phase, turn), want, "{phase:?} + {turn:?}");
        }
    }

    /// 恢复口径(任务书选项①)的行为含义:Override 之后的**第一个**编辑
    /// 事件即恢复 —— 接线层在同一帧消费 Edit 转移后的 Follow 态照常滚动,
    /// 没有第二次编辑的延迟;这里钉转移语义本身。
    #[test]
    fn override_recovers_on_first_edit() {
        let mut phase = Phase::Follow;
        phase = step(phase, Turn::UserScroll);
        assert_eq!(phase, Phase::Override, "用户滚动接管");
        phase = step(phase, Turn::Edit);
        assert_eq!(phase, Phase::Follow, "下一次编辑即恢复,无需第二次");
    }
}
