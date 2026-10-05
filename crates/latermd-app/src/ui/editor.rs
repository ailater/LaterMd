//! 编辑面板:等宽 multiline `TextEdit`,rope 即缓冲。
//!
//! egui 0.36 的 `TextEdit` 通过 [`egui::TextBuffer`] 驱动编辑:每个按键、
//! IME 组合、内建 undo/redo 最终都落成 `insert_text` / `delete_char_range`
//! 调用,rope 因此天然吃到增量编辑,不存在"整段字符串重写"路径。
//! undo/redo 用 `TextEdit` 内建 undoer(快照存于其 widget state)。
//! 大纲跳转也在这里应用:覆写 TextEdit 持久光标并交还焦点。

use crate::live::{self, LiveState, RenderMode};
use crate::state::{OutlineCursor, PreviewState};
use crate::ui::gutter;
use crate::ui::minimap;
use latermd_editor::EditorBuffer;
use std::ops::Range;

use eframe::egui;

/// 编辑器 widget 的 id:由**标签的稳定 id** 派生(多标签 #11)—— 每个标签
/// 一套 TextEdit 持久状态(光标/undo/焦点),切标签零恢复逻辑;标签关闭后
/// id 不复用(`TabsState::next_id` 自增),新标签不会继承旧标签的光标。
pub(crate) fn tab_editor_id(tab_id: u64) -> egui::Id {
    egui::Id::new("source-editor").with(tab_id)
}

/// IME caret 上报的每标签记忆(#19):上次显式上报的 caret 条矩形,以及
/// 上次见到的 TextEdit 内容矩形(egui-winit 自动路径上报给窗口系统的就是
/// 它)。`None` = 尚未上报,或刚经历失焦(X11 下 IME 上下文随焦点翻转被
/// winit 重建,重进必须重报)。挂在 `editor_id.with("ime-caret")` 上,随
/// TextEdit 持久 state 同生命周期,切标签互不惊扰。
#[derive(Clone, Copy, Default)]
struct ImeCaretTracking {
    last_sent_rect: Option<egui::Rect>,
    last_widget_rect: Option<egui::Rect>,
}

/// [`ime_report_needed`] 的输入快照。
struct ImeTriggerInputs {
    focused: bool,
    /// 写回帧(大纲跳转/格式动作覆写光标并要回焦点)。
    write_back: bool,
    /// 本帧 caret 条矩形(egui points);`None` = 光标从未落过。
    caret_rect: Option<egui::Rect>,
    /// 上次显式上报的 caret 条矩形;`None` = 尚未上报/刚失焦。
    last_sent_rect: Option<egui::Rect>,
    /// 本帧 TextEdit 内容矩形(`TextEditOutput::text_clip_rect`)。
    widget_rect: egui::Rect,
    /// 上次见到的内容矩形;`None` = 首帧。
    last_widget_rect: Option<egui::Rect>,
    /// 本帧输入事件非空 —— egui-winit 自动路径触发项的镜像。
    has_input_events: bool,
}

/// IME 位置上报的触发判定(#19 红线 + 独立评审 finding 1 的纯函数形态)。
/// 只在编辑器持焦点且满足其一时报:
///
/// ①写回帧(大纲跳转/格式动作覆写光标并要回焦点);
/// ②caret 条矩形相对上次上报有变化 —— 含滚动/重排导致的屏幕位移,即红线
///   的「光标位置实际变化」;
/// ③egui-winit 自动路径将重写 spot 的帧(keyup/指针 motion/preedit 更新
///   这类「有输入事件但光标没动」的帧,或内容矩形变化的滚动动画帧;自动
///   路径条件见 `egui-winit-0.36.2/src/lib.rs:1173`)—— 它把 spot 钉到
///   TextEdit 左上角,必须同帧补报盖回。
///
/// 失焦帧恒 `false`;真正的空闲帧(无输入事件、无任何位移、非写回)三项
/// 全假,一条 IME 命令都不发。
fn ime_report_needed(i: &ImeTriggerInputs) -> bool {
    match (i.focused, i.caret_rect) {
        (true, Some(caret_rect)) => {
            let auto_path_will_fire =
                i.has_input_events || Some(i.widget_rect) != i.last_widget_rect;
            i.write_back || Some(caret_rect) != i.last_sent_rect || auto_path_will_fire
        }
        _ => false,
    }
}

/// 由 caret 条矩形(`pos_from_cursor` 给零宽、整行高的 galley 局部矩形)算
/// 上报给 `ViewportCommand::IMERect` 的屏幕矩形。#19 坤哥复测反馈:候选框
/// 贴着光标、盖住当前行 —— 锚点整体下移一个行高,候选窗从当前行底之下
/// 展开。
fn ime_anchor_rect(galley_pos: egui::Pos2, caret: egui::Rect) -> egui::Rect {
    let shift = egui::vec2(0.0, caret.height());
    egui::Rect::from_min_max(
        galley_pos + caret.min.to_vec2() + shift,
        galley_pos + caret.max.to_vec2() + shift,
    )
}

/// 把 [`EditorBuffer`] 适配成 egui `TextBuffer` 的 newtype。
///
/// 孤儿规则:`egui::TextBuffer` 与 `EditorBuffer` 都不归本 crate,
/// 直接 impl 不合法,只能包本地类型。放 app 层同时守住了
/// `latermd-editor` 不依赖 UI 框架的铁律。
struct EditorText<'a>(&'a mut EditorBuffer);

impl egui::TextBuffer for EditorText<'_> {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        self.0.text()
    }

    fn insert_text(&mut self, text: &str, char_index: egui::text::CharIndex) -> usize {
        self.0.insert_chars(char_index.0, text);
        text.chars().count()
    }

    fn delete_char_range(&mut self, char_range: Range<egui::text::CharIndex>) {
        self.0.remove_chars(char_range.start.0..char_range.end.0);
    }

    // trait 无默认实现;照上游文档示例返回 TypeId(供 downcast 用)。
    // 生命周期参数不参与 TypeId,统一取 'static 形态。
    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<EditorText<'static>>()
    }
}

/// `ui::editor` 与 `State` 之间的**光标协调通道**,每帧双向跑一次:
///
/// * 出去:`cursor.byte`(大纲高亮)、`selection`(格式工具条的输入);
/// * 进来:`cursor.jump_to`(大纲跳转)、`pending`(格式动作产出的新选区)。
///
/// 三者都是「UI 每帧回填/消费」的同族字段 —— 收成结构体之前它们是三条
/// 并排的 `&mut` 实参,把 [`ui`] 顶到 9 个(clippy `too_many_arguments`),
/// 而它们从来都是一起传的。
pub struct CursorChannel<'a> {
    /// 大纲↔编辑器:当前光标字节偏移出去,跳转目标进来。
    pub cursor: &'a mut OutlineCursor,
    /// 当前选区(字符区间),给格式工具条当输入;`None` = 这一帧还没渲染过。
    pub selection: &'a mut Option<(usize, usize)>,
    /// 待写回的新选区,由归约侧挂上、本模块消费。
    pub pending: &'a mut Option<(usize, usize)>,
}

/// 绘制编辑面板,并在控件返回后维护预览快照与光标协调。返回 TextEdit
/// 的响应(焦点/交互归因用,测试也用它拿 widget id)。
///
/// `show_minimap`(#55 M2):源码模式右缘 minimap 开关(设置页持久化,
/// ThemeSettings 同路)。关闭时走与从前逐字节相同的路径 —— TextEdit 宽
/// 度、ScrollArea、gutter 全部不变,minimap 的注册/绘制/行模型缓存一概
/// 不进。仅源码模式生效:Live 分支在更早处返回。
// 参数各自属于 State 的不同字段,打包成结构会造出人为聚合;同款豁免
// 先例见 settings.rs `dialog`。
#[allow(clippy::too_many_arguments)]
pub fn ui(
    panel: &mut egui::Ui,
    editor: &mut EditorBuffer,
    preview: &mut PreviewState,
    channel: CursorChannel<'_>,
    live: &mut LiveState,
    mode: RenderMode,
    editor_id: egui::Id,
    show_minimap: bool,
) -> egui::Response {
    let CursorChannel {
        cursor,
        selection,
        pending,
    } = channel;
    // U3 模式切换淡入(ui::fade):当前模式的整块内容从透明渐入,Source↔Live
    // 双向对称;`style.animation_time` 为 0 时 egui 直接落端点,无动画。id 挂
    // 在标签稳定的 editor_id 上,一标签一套动画状态,切标签互不惊扰。
    let is_live = mode == RenderMode::Live;
    let alpha = crate::ui::fade::crossfade(panel.ctx(), editor_id.with("render-mode"), is_live);
    panel.multiply_opacity(if is_live { alpha } else { 1.0 - alpha });
    // 两种模式共用同一个 rope buffer 与同一套撤销语义(roadmap 铁律):这里
    // 只是分派,没有任何「把光标/文本从一种模式搬到另一种」的恢复逻辑。
    if is_live {
        return live::ui(panel, editor, preview, cursor, live, editor_id);
    }

    let line_height = {
        let font = egui::FontSelection::Style(egui::TextStyle::Monospace).resolve(panel.style());
        panel.fonts_mut(|f| f.row_height(&font)) + panel.spacing().extra_text_line_spacing
    };
    // 面板剩余高度换算成最低行数(空文档也铺满编辑区)。0.36 的 multiline
    // `TextEdit` 高度 = max(desired_rows 行高, 内容高度),内容更长时按
    // 内容自然长高 —— 长出来的部分交给外层 ScrollArea 滚动(#29 之前
    // 没有任何滚动容器,超过一屏的内容既看不见也滚不动)。
    let rows = (panel.available_height() / line_height).floor().max(1.0) as usize;

    // minimap 窄条(#55 M2):先只算矩形让 TextEdit 压窄;交互注册放在
    // ScrollArea **之后**(见下方闭外)—— egui 同层后注册的 widget 居上,
    // ScrollArea 的背景拖拽先注册,先注册的 minimap 会被它抢走命中。
    // 窄条最右 10px 是滚动条避让区。
    let panel_avail = panel.available_rect_before_wrap();
    let minimap_rect = show_minimap.then(|| {
        egui::Rect::from_min_max(
            egui::pos2(panel_avail.right() - minimap::MINIMAP_W, panel_avail.top()),
            panel_avail.right_bottom(),
        )
    });

    // 跳转/格式写回挪进 ScrollArea 闭包:与下面的光标跟随同处一个作用域,
    // scroll_to_rect 在闭包内调用才会被本 ScrollArea 的 end 同帧消费(egui
    // 把滚动目标记在帧级 pass state,ScrollArea 结束时取走折算成偏移)。
    let mut format_result = pending.take();
    if let Some(jump) = cursor.jump_to.take() {
        format_result = Some((jump, jump));
    }
    // 写回帧的 caret 目标在 ScrollArea 闭包里记下,IME 上报(闭包外)用:
    // 写回当帧 output.state 还是写回前的旧快照,定位只能用写回目标本身。
    let mut ime_write_back: Option<usize> = None;

    let scrolled = egui::ScrollArea::vertical()
        // 每标签一套滚动位置,与 TextEdit 持久状态的口径一致;id 不含内容
        // 长度/hash(AGENTS §6.7)。
        .id_salt(editor_id.with("source-editor-scroll"))
        .auto_shrink([false, false])
        .show(panel, |ui| {
            // 行号槽宽度先于 TextEdit 算好(总行数 = 1 + '\n' 数,与 galley
            // 的逻辑行划分同构);位宽跨档才变,右对齐不抖。
            let total_lines = 1 + editor.text().bytes().filter(|b| *b == b'\n').count();
            let gutter_w = gutter::width(ui, total_lines);
            let mut buffer = EditorText(editor);
            let (slot, output) = ui
                .horizontal(|row| {
                    // 槽位纯做布局让位(高度 0 不撑行),hover-only 不拦截
                    // 指针;desired_width(INFINITY) 在槽位之后取剩余宽,
                    // 即自动扣减行号槽与 token 间距。
                    let (slot, _) =
                        row.allocate_exact_size(egui::vec2(gutter_w, 0.0), egui::Sense::hover());
                    // minimap 开启时显式压窄同一段剩余宽(INFINITY 的"吃满"
                    // 语义等价替换);关闭保持 INFINITY 原路径,现状零变化。
                    let desired_width = if minimap_rect.is_some() {
                        (row.available_width() - minimap::MINIMAP_W - row.spacing().item_spacing.x)
                            .max(10.0)
                    } else {
                        f32::INFINITY
                    };
                    let output = egui::TextEdit::multiline(&mut buffer)
                        // 稳定 id:光标/undo 状态跨帧保持;同样绝不能含内容长度或 hash
                        .id(editor_id)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(desired_width)
                        .desired_rows(rows)
                        .lock_focus(true)
                        .show(row);
                    (slot, output)
                })
                .inner;
            // 行号画在同一闭包内:内容坐标随滚动平移,行 y 取 galley 逻辑
            // 行首 visual row,光标行 accent 高亮(光标读持久化 state,与
            // 大纲回填同源;换行计数走 galley 自带的全文本)。
            let cursor_line = output.state.cursor.char_range().map(|range| {
                output
                    .galley
                    .job
                    .text
                    .chars()
                    .take(range.primary.index.0)
                    .filter(|c| *c == '\n')
                    .count()
            });
            gutter::paint(ui, &output, slot, cursor_line);

            // 进来:格式动作/大纲跳转产出的新选区,写回 TextEdit 持久 cursor
            // 并把焦点还给编辑器 —— 否则用户还得自己点回编辑区才能继续打字。
            // 写回的是本帧已知目标,跟随直接取它(output.state 是写回前的
            // 旧快照,新光标要到下一帧 load 才可见)。
            let mut follow_char: Option<usize> = None;
            if let Some((start, end)) = format_result {
                write_selection(ui, &output.response.response.id, &output.state, start, end);
                follow_char = Some(start);
                ime_write_back = Some(start);
            }

            // 光标跟随只走**显式请求**,标志是帧内局部变量,当帧即焚 ——
            // 不跨帧、不跨标签(每标签一次 `ui` 调用一套栈帧)。第二来源是
            // 键盘导航:本帧输入出现行/页移动键且编辑器持焦点时,光标行
            // 保持可见。滚轮、拖滚动条、空闲帧一概不跟随 —— 早期「每帧
            // diff 光标」的方案在 TextEdit 重排帧会误报变化(egui 内部
            // cursor 表示与 char 换算在重排帧不稳定),误判一次就把视口
            // 抢回光标处,徒手滚几屏又弹回去(#29 回归,已整段删除)。
            // egui 0.36 内建跟随在 Atom paint 阶段才发 scroll_to_rect,
            // 晚于本 ScrollArea 的 end,永不落地,须在这里补。
            let keyboard_nav = ui.input(|input| {
                input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::Home
                                | egui::Key::End
                                | egui::Key::PageUp
                                | egui::Key::PageDown,
                            pressed: true,
                            ..
                        }
                    )
                })
            }) && ui
                .ctx()
                .memory(|mem| mem.has_focus(output.response.response.id));
            if keyboard_nav {
                follow_char = output
                    .state
                    .cursor
                    .char_range()
                    .map(|range| range.primary.index.0);
            }

            if let Some(index) = follow_char {
                // galley 是本帧文本,折行下的行位置是精确的。
                let rect = output
                    .galley
                    .pos_from_cursor(egui::text::CCursor::new(index));
                ui.scroll_to_rect(
                    egui::Rect::from_min_max(
                        output.galley_pos + rect.min.to_vec2(),
                        output.galley_pos + rect.max.to_vec2(),
                    )
                    .expand(1.5),
                    None,
                );
            }

            // minimap 点击/拖动跳转(#55 M2):经本 ScrollArea 的既有滚动
            // 通道落地,动画关闭(拖动逐帧即时跟随,不留弹性滞后)。意图
            // 来自**上一帧**闭外注册的窄条交互(minimap-jump temp,消费即
            // 清)—— 本帧注册本帧消费做不到:scroll_to_rect 要在本闭包内
            // 才被 end 消费,而窄条命中要排在 ScrollArea 之后注册(层级在
            // 上,否则被背景拖拽抢走)。一帧滞后在 60fps 下无感,拖动每帧
            // 刷新意图、逐帧跟随。度量同用上一帧真值(metrics temp):本帧
            // 内容高要等排版完成,编辑帧的一帧误差下一帧自愈。跳转只动
            // 滚动 offset,不触碰 TextEdit 光标状态;排在 follow 之后,
            // 键盘导航(当帧动作)覆盖它。
            if let Some(map_rect) = minimap_rect {
                // 消费即清:一帧意图一帧落地,不留旧值在拖动结束后复读。
                let pointer_y = ui
                    .ctx()
                    .data_mut(|d| d.remove_temp::<f32>(minimap::jump_id(editor_id)));
                if let Some(pointer_y) = pointer_y {
                    let metrics: minimap::ScrollMetrics = ui.ctx().data(|d| {
                        d.get_temp(minimap::metrics_id(editor_id))
                            .unwrap_or_default()
                    });
                    let travel = (metrics.content_height - metrics.viewport_height).max(0.0);
                    let scroll_ratio = if travel > 0.0 {
                        (metrics.offset / travel).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let viewport_frac = if metrics.content_height > 0.0 {
                        (metrics.viewport_height / metrics.content_height).clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    if let Some(target) = minimap::jump_ratio(minimap::JumpInput {
                        minimap_height: map_rect.height(),
                        total_lines,
                        scroll_ratio,
                        row_h: minimap::ROW_H,
                        pointer_y,
                        viewport_frac,
                    }) {
                        // TOP 对齐语义下 ScrollArea::end 的换算会扣一个
                        // item_spacing,rect 顶补回;单像素 rect 只表位置。
                        let clip = ui.clip_rect();
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(
                                clip.left() + 1.0,
                                clip.top() + target * travel + ui.spacing().item_spacing.y,
                            ),
                            egui::vec2(1.0, 1.0),
                        );
                        ui.scroll_to_rect_animation(
                            rect,
                            Some(egui::Align::TOP),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                }
            }
            output
        });
    let output = scrolled.inner;

    // minimap 交互注册(#55 M2):排在 ScrollArea 之后注册 → 同层居上,
    // 点击/拖动不会被 ScrollArea 的背景拖拽抢走。命中即把指针 y(相对
    // 窄条顶)写进 jump temp,下一帧闭包开头消费落地;拖动每帧覆写,
    // 滚动逐帧跟随。窄条最右 10px 让给滚动条(它画得更靠右缘)。
    if let Some(map_rect) = minimap_rect {
        let response = panel.interact(
            map_rect,
            editor_id.with("minimap"),
            egui::Sense::click_and_drag(),
        );
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                panel.ctx().data_mut(|d| {
                    d.insert_temp(minimap::jump_id(editor_id), pos.y - map_rect.top())
                });
            }
        }
    }

    // minimap 绘制(#55 M2):窗口/高亮框用本帧滚动真值换算,行模型走 M1
    // 的修订号缓存(命中帧零成本);帧末更新 metrics temp 供下一帧跳转。
    // 上面的跳转意图经 ScrollArea::end 在**上一帧**落账,本帧读到的
    // offset 已是跳转后的值,高亮框无额外滞后。
    if let Some(map_rect) = minimap_rect {
        let viewport_h = scrolled.inner_rect.height();
        let content_h = scrolled.content_size.y;
        let travel = (content_h - viewport_h).max(0.0);
        let offset = scrolled.state.offset.y;
        let scroll_ratio = if travel > 0.0 {
            (offset / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let viewport_frac = if content_h > 0.0 {
            (viewport_h / content_h).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let (shapes, bars, viewport) = minimap::with_lines(
            panel.ctx(),
            editor_id,
            editor,
            minimap::LineParams::factory(),
            |lines| {
                minimap::shapes(
                    panel.visuals(),
                    map_rect,
                    lines,
                    scroll_ratio,
                    viewport_frac,
                )
            },
        );
        // shapes 的上画布与探针写入都在 with_lines 的 data_mut 借用之外:
        // painter 的 add 会再进同一把 Context 写锁,借用内重入即死锁。
        panel.painter().extend(shapes);
        // 测试探针(照 preview.rs `ScrollProbe` 的 data 手法,生产只写
        // 不读):供无头测试断言「关闭态零 minimap 元素 / 开启态行条数」。
        panel.ctx().data_mut(|d| {
            d.insert_temp(
                minimap::probe_id(editor_id),
                minimap::MinimapProbe {
                    bars,
                    viewport: Some(viewport),
                },
            )
        });
        panel.ctx().data_mut(|d| {
            d.insert_temp(
                minimap::metrics_id(editor_id),
                minimap::ScrollMetrics {
                    offset,
                    content_height: content_h,
                    viewport_height: viewport_h,
                },
            );
        });
    }

    // 快照只在修订号前进时重建(文本 + 大纲同源);这是"避免每帧重解析"
    // 的第一层,vendored 层的 text hash 缓存是第二层。
    if preview.synced_rev != editor.revision() {
        preview.rebuild(editor);
    }

    // 光标位置回填(大纲「当前小节」高亮用):主光标的字符偏移按当前缓冲
    // 换成字节。读持久化的 cursor 而非 output.cursor_range —— 后者在失焦
    // 帧为 None,而持久化值保留上次位置。
    cursor.byte = output
        .state
        .cursor
        .char_range()
        .map(|range| editor.char_to_byte(range.primary.index.0));

    // 格式工具条的选区双向通道(docs/ui-shell-redesign.md §6.4)。
    //
    // 出去:把当前 `CCursorRange` 抄给 `tab.selection` —— 按钮被点中时编辑器
    // 已失焦,而选区活在这个持久 state 里,归约侧拿不到。读持久化的 cursor
    // 而不是 `output.cursor_range`(后者在失焦帧是 None)。
    *selection = output
        .state
        .cursor
        .char_range()
        .map(|range| (range.primary.index.0, range.secondary.index.0));

    // IME 位置显式上报(#19):egui-winit 0.36 的自动路径把 `IMEOutput::rect`
    // (整个 TextEdit 内容矩形,其 lib.rs:1171)当 IME 光标区调
    // `Window::set_ime_cursor_area` —— XIM spot 被钉到编辑器左上角。该路径
    // 在「内容矩形变化 **或** 本帧有输入事件」时都会重写 spot(lib.rs:
    // 1173):keyup、指针 motion、preedit 文本未变的更新帧在打字流中高频
    // 出现而光标没动,只按光标变化补报会留下整帧错位(候选框跳回左上角,
    // 即坤哥症状的帧类)。因此触发判定镜像自动路径的谓词(`text_clip_rect`
    // == 自动路径用的 `inner_rect`,见 egui builder.rs:781):自动路径要写
    // spot 的帧,这里同帧经 `ViewportCommand::IMERect` 补报 caret 条矩形盖
    // 回 —— 该命令由 eframe 在平台输出**之后**消费(wgpu_integration.rs:
    // 1300 晚于 :1248),是帧内最后一次 spot 写入。真正的空闲帧(无事件、
    // 无位移、非写回)依然一条 IME 命令都不发。
    let tracking_id = editor_id.with("ime-caret");
    let mut tracking: ImeCaretTracking = panel
        .ctx()
        .data_mut(|data| data.get_temp(tracking_id).unwrap_or_default());
    let focused = panel
        .ctx()
        .memory(|mem| mem.has_focus(output.response.response.id));
    // 写回帧的 caret 取写回目标,其余帧读持久化光标 —— 与行号/大纲回填同源。
    let caret_char = ime_write_back.or_else(|| {
        output
            .state
            .cursor
            .char_range()
            .map(|range| range.primary.index.0)
    });
    let caret_rect = caret_char.map(|caret| {
        let rect = output
            .galley
            .pos_from_cursor(egui::text::CCursor::new(caret));
        ime_anchor_rect(output.galley_pos, rect)
    });
    let has_input_events = panel.ctx().input(|input| !input.events.is_empty());
    if ime_report_needed(&ImeTriggerInputs {
        focused,
        write_back: ime_write_back.is_some(),
        caret_rect,
        last_sent_rect: tracking.last_sent_rect,
        widget_rect: output.text_clip_rect,
        last_widget_rect: tracking.last_widget_rect,
        has_input_events,
    }) {
        panel
            .ctx()
            .send_viewport_cmd(egui::ViewportCommand::IMERect(
                caret_rect.expect("ime_report_needed 为真则 caret 已知"),
            ));
        tracking.last_sent_rect = caret_rect;
    }
    // 内容矩形每帧记忆(自动路径触发项的镜像基准),与是否上报无关。
    tracking.last_widget_rect = Some(output.text_clip_rect);
    if !focused {
        // 失焦帧不发任何 IME 命令(红线),记忆归零:焦点重进帧的
        // last_sent_rect 是 None,光标没动也重报一次 —— X11 下焦点翻转会让
        // winit 重建 IME 上下文,spot 不重报就丢。
        tracking = ImeCaretTracking::default();
    }
    panel
        .ctx()
        .data_mut(|data| data.insert_temp(tracking_id, tracking));

    output.response.response
}

/// 把字符区间写进 `TextEdit` 的持久 cursor,并请求焦点。
fn write_selection(
    panel: &egui::Ui,
    id: &egui::Id,
    state: &egui::widgets::text_edit::TextEditState,
    start: usize,
    end: usize,
) {
    let mut state = state.clone();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(start),
            egui::text::CCursor::new(end),
        )));
    state.store(panel.ctx(), *id);
    panel.ctx().memory_mut(|mem| mem.request_focus(*id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::widgets::text_edit::TextEditState;
    use egui::{Event, Key, Modifiers, RawInput};

    /// 跑一帧编辑面板,返回 TextEdit 的 widget id。`now` 逐帧递增:
    /// undoer 靠输入时间戳切分撤销组,恒定时间会把多次输入并成一步。
    fn frame(
        ctx: &egui::Context,
        events: Vec<Event>,
        now: f64,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        cursor: &mut OutlineCursor,
    ) -> egui::Id {
        let mut selection = None;
        let mut pending = None;
        frame_with_channel(
            ctx,
            events,
            now,
            editor,
            preview,
            &mut selection,
            &mut pending,
            cursor,
        )
        .0
    }

    /// 同 [`frame`],额外把 §6.4 的两个槽位交给调用方。八条实参里五条是
    /// 同一帧的被测对象、一起传是大势所趋,故单独豁免形参计数 lint ——
    /// 它只作用于本测试辅助,不去污染 `ui` 的 API 面。
    /// 返回值第二项是本帧 viewport 命令流里的全部 `IMERect` 矩形(#19:
    /// IME 上报断言的截取口,与生产侧 egui-winit 消费的是同一条命令流)。
    #[allow(clippy::too_many_arguments)]
    fn frame_with_channel(
        ctx: &egui::Context,
        events: Vec<Event>,
        now: f64,
        editor: &mut EditorBuffer,
        preview: &mut PreviewState,
        selection: &mut Option<(usize, usize)>,
        pending: &mut Option<(usize, usize)>,
        cursor: &mut OutlineCursor,
    ) -> (egui::Id, Vec<egui::Rect>) {
        // 真实窗口尺度的视口:滚不滚得动取决于「内容是否高过一屏」,
        // 默认 10000×10000 的测试视口永远装得下,滚动路径测不到。
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut live = LiveState::default();
        let id = std::cell::Cell::new(egui::Id::NULL);
        let output = ctx.run_ui(
            RawInput {
                events,
                time: Some(now),
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                id.set(
                    super::ui(
                        ui,
                        editor,
                        preview,
                        CursorChannel {
                            cursor,
                            selection,
                            pending,
                        },
                        &mut live,
                        RenderMode::Source,
                        tab_editor_id(1),
                        // 本模块的既有测试都验现状路径:minimap 关(其
                        // 渲染与跳转的验收在 ui::minimap 的 tests 里)。
                        false,
                    )
                    .id,
                );
            },
        );
        // egui 0.36 的 TexturesDelta drop 检查:测试里不消费绘制增量,
        // 显式丢弃(与 vendored 层测试同一处理)
        let ime_rects = output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|viewport| {
                viewport
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        egui::ViewportCommand::IMERect(rect) => Some(*rect),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        output.drop_without_applying_deltas();
        (id.get(), ime_rects)
    }

    /// §6.4 通道的**回来那一半**:归约把新选区挂到 `pending`,本模块把它写
    /// 进 `TextEdit` 持久 cursor 并把焦点还给编辑器。
    ///
    /// 归约那一半(`FormatRequested` → 文本 + `pending_selection`)由
    /// `state::tests::format_requested_rewrites_buffer_and_stages_selection`
    /// 钉住,两边分开测是因为它们中间的耦合是**字符偏移**这个契约,不是
    /// 彼此的内部实现。
    #[test]
    fn pending_selection_is_written_back_as_persisted_cursor() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("**甲乙丙**");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = Some((2, 5));

        let (id, _) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );

        assert!(pending.is_none(), "待写回选区已被消费");
        let state = TextEditState::load(&ctx, id).expect("已持久化 widget state");
        let range = state.cursor.char_range().expect("选区已写入");
        let span = [
            range.primary.index.0.min(range.secondary.index.0),
            range.primary.index.0.max(range.secondary.index.0),
        ];
        assert_eq!(span, [2, 5], "TextEdit 会归一化端点顺序,故只比对区间");
        assert!(ctx.memory(|m| m.has_focus(id)), "焦点还给编辑器");
    }

    /// §6.4 通道的**出去那一半**:`TextEdit` 的选区被抄到 `selection`,这样
    /// 工具条按钮点击那一帧(编辑器已失焦)归约侧才拿得到它。
    ///
    /// 用 `pending` 灌一个已知选区、下一帧读回 —— 比靠按键推光标稳得多:
    /// 后者经由 `TextEdit` 内部的 `clamp` / 归一化,端点到不准 JB,而这里要
    /// 钉的是「搬运工」本身,不是 `TextEdit` 的光标规则。
    #[test]
    fn selection_is_mirrored_out_for_the_format_bar() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let mut selection = None;
        let mut pending = Some((1, 3));

        frame_with_channel(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        // 写入发生在回填之后(同帧先读后写),故这一帧读到的还是旧值
        frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );

        let recorded = selection.expect("选区已回填");
        let span = [recorded.0.min(recorded.1), recorded.0.max(recorded.1)];
        assert_eq!(span, [1, 3], "上一帧写进去的选区被搬了出来");
    }

    /// #19 触发判定的纯函数单测:红线逐条落成断言 —— 失焦恒不报;持焦点
    /// 时在「caret 屏幕位置有变 / 写回 / 记忆为空(首次或重进)/ 自动路径
    /// 将重写 spot 的帧(有输入事件或内容矩形变化)」时报;真空闲帧
    /// (无事件、无位移、非写回)不报;没有光标就没位置可报。
    #[test]
    fn ime_trigger_requires_focus_and_change_or_auto_path_risk() {
        let caret = || egui::Rect::from_min_max(egui::pos2(100.0, 200.0), egui::pos2(101.0, 218.0));
        let caret2 =
            || egui::Rect::from_min_max(egui::pos2(120.0, 200.0), egui::pos2(121.0, 218.0));
        let widget = || egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let trig = |focused, write_back, caret_rect, last_sent_rect, has_input_events| {
            ime_report_needed(&ImeTriggerInputs {
                focused,
                write_back,
                caret_rect,
                last_sent_rect,
                widget_rect: widget(),
                last_widget_rect: Some(widget()),
                has_input_events,
            })
        };

        // 失焦帧:无论光标/写回/事件状态,一律不上报
        assert!(!trig(false, false, Some(caret()), Some(caret()), false));
        assert!(!trig(false, false, Some(caret()), Some(caret()), true));
        assert!(!trig(false, true, Some(caret()), Some(caret()), true));
        // 持焦点 + 记忆为空(首次进入/失焦后重进):报
        assert!(trig(true, false, Some(caret()), None, false));
        // 持焦点 + caret 屏幕位置变化(红线主项,含滚动/重排位移):报
        assert!(trig(true, false, Some(caret2()), Some(caret()), false));
        // 持焦点 + caret 未动 + 无事件 + 内容矩形未变 = 真空闲帧:不报
        assert!(!trig(true, false, Some(caret()), Some(caret()), false));
        // finding 1:持焦点 + caret 未动 + 有输入事件(keyup/motion/preedit
        // 更新帧)—— 自动路径将把 spot 重写到 widget 左上角,必须同帧盖回
        assert!(trig(true, false, Some(caret()), Some(caret()), true));
        // 写回帧:rect 未变也要报一次
        assert!(trig(true, true, Some(caret()), Some(caret()), false));
        // 光标从未落过(caret_rect 为 None):没有位置可报
        assert!(!trig(true, false, None, None, false));
        assert!(!trig(true, true, None, None, true));
        // 自动路径另一半触发项:内容矩形变化(滚动动画/布局变化)或首帧
        // (last_widget_rect 为空)时,即使无事件、caret 未动也要盖回
        assert!(ime_report_needed(&ImeTriggerInputs {
            focused: true,
            write_back: false,
            caret_rect: Some(caret()),
            last_sent_rect: Some(caret()),
            widget_rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(380.0, 300.0)),
            last_widget_rect: Some(widget()),
            has_input_events: false,
        }));
        assert!(ime_report_needed(&ImeTriggerInputs {
            focused: true,
            write_back: false,
            caret_rect: Some(caret()),
            last_sent_rect: Some(caret()),
            widget_rect: widget(),
            last_widget_rect: None,
            has_input_events: false,
        }));
    }

    /// #19:焦点进入帧与光标移动帧各报一次 `IMERect`,rect 随光标前进
    /// 右移;持焦点的空闲帧一条 IME 命令都不发。
    #[test]
    fn ime_rect_follows_caret_while_focused() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        // 先经写回通道把光标明确落到 0(从未落过光标的编辑器第一次吃导航
        // 键会初始化到 galley 末端,见 arrow_down_navigation_keeps_cursor_visible);
        // 这本身也是一帧「写回 + 焦点进入」:当帧上报一次。
        let mut selection = None;
        let mut pending = Some((0, 0));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "写回帧(同时是焦点进入帧)上报一次");
        let at_zero = rects[0];

        // 持焦点空闲帧:光标没动,不发任何 IME 命令
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert!(rects.is_empty(), "空闲帧不发 IME 命令");

        // ArrowRight:光标 0 → 1,当帧上报且 caret 条右移一个字宽
        let right = Event::Key {
            key: Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let (_, rects) = frame_with_channel(
            &ctx,
            vec![right],
            0.3,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "光标移动帧上报一次");
        assert!(
            rects[0].min.x > at_zero.min.x,
            "caret rect 随光标移动右移(实测 {} → {})",
            at_zero.min.x,
            rects[0].min.x
        );

        // 移动之后再空闲一帧:不再报
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.4,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert!(rects.is_empty(), "移动后的空闲帧不再上报");
    }

    /// #19:失焦帧不报;焦点重进帧**即使光标没动**也要重报一次 —— X11 下
    /// 焦点翻转会让 winit 重建 IME 上下文,spot 不重报候选框就落不回光标。
    #[test]
    fn ime_report_repeats_when_focus_returns() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let mut selection = None;
        let mut pending = Some((2, 2));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "写回帧上报一次");

        // 失焦(等价于点进侧栏):不报
        ctx.memory_mut(|mem| mem.surrender_focus(id));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert!(rects.is_empty(), "失焦帧不发任何 IME 命令");

        // 焦点重进:光标未动也要重报一次
        ctx.memory_mut(|mem| mem.request_focus(id));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.3,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "焦点重进帧重报一次(光标未动)");
    }

    /// #19 复测反馈回归:上报锚点整体下移一行 —— `ime_anchor_rect` 的顶边
    /// 必须落在 caret 行底(叠加 galley 屏幕偏移后),候选窗不再盖住当前行。
    #[test]
    fn ime_anchor_rect_shifts_below_caret_row() {
        // pos_from_cursor 语义:零宽、整行高(此处行高 20)。
        let caret = egui::Rect::from_min_max(egui::pos2(5.0, 100.0), egui::pos2(5.0, 120.0));
        let galley_pos = egui::pos2(30.0, 200.0);
        let anchor = ime_anchor_rect(galley_pos, caret);
        assert_eq!(
            anchor.min,
            egui::pos2(35.0, 320.0),
            "顶边 = caret 行底 + galley 屏幕偏移(下移一行)"
        );
        assert_eq!(anchor.height(), 20.0, "行高保持,纯平移");
        assert!(
            anchor.min.y >= caret.max.y + galley_pos.y,
            "锚点顶不低于 caret 行底"
        );
    }

    /// finding 1 回归:keyup/指针 motion/preedit 文本未变的更新帧里光标没动,
    /// 但 egui-winit 自动路径因「本帧有输入事件」照样把 spot 重写到 TextEdit
    /// 左上角(其 lib.rs:1173 的第二个触发项)—— 这类帧必须同帧补报同一
    /// caret 条矩形盖回,否则快速打字时候选框会周期性跳回编辑器左上角
    /// (坤哥症状的帧类)。
    #[test]
    fn ime_rect_resent_on_event_frames_without_caret_move() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        // 写回把光标落到 2 并聚焦:当帧上报一次,记下 caret 条矩形
        let mut selection = None;
        let mut pending = Some((2, 2));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "写回帧上报一次");
        let sent = rects[0];

        // 真空闲帧(无事件、光标未动、无位移):不报
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert!(rects.is_empty(), "真空闲帧不发 IME 命令");

        // 指针划过编辑区(motion 帧,光标未动):同帧补报同一矩形
        let (_, rects) = frame_with_channel(
            &ctx,
            vec![Event::PointerMoved(egui::pos2(120.0, 80.0))],
            0.3,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "事件帧即使光标未动也补报一次");
        assert_eq!(
            rects[0], sent,
            "补报的是同一 caret 条矩形,盖回自动路径的错位值"
        );

        // preedit 更新帧(组合开始,未上屏):同样上报。egui 0.36 会把 preedit
        // 文本插进缓冲并把组合选区末尾当 primary(builder.rs 的
        // ImeComposition 分支),caret rect 随组合串右移 —— 这正是候选框
        // 该跟随的位置。
        let (_, rects) = frame_with_channel(
            &ctx,
            vec![Event::Ime(egui::ImeEvent::Preedit {
                text: "nihao".into(),
                active_range_chars: None,
            })],
            0.4,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "preedit 更新帧补报一次");
        let composed = rects[0];
        assert!(
            composed.min.x > sent.min.x,
            "组合串插入后 caret 移到组合串末尾(实测 {} → {})",
            sent.min.x,
            composed.min.x
        );

        // preedit 文本未变的重复更新帧:缓冲先删后插同串,内容与光标回到
        // 同一处,rect 与上次上报一致 —— 事件非空仍触发,重发同值盖回。
        let (_, rects) = frame_with_channel(
            &ctx,
            vec![Event::Ime(egui::ImeEvent::Preedit {
                text: "nihao".into(),
                active_range_chars: None,
            })],
            0.5,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "重复 preedit 帧补报一次");
        assert_eq!(rects[0], composed, "与上一帧同值(组合串未变)");
    }

    /// 自动路径另一半触发项(内容矩形变化)的端到端:写回触发的滚动动画帧
    /// 没有任何输入事件,但内容矩形与 caret 屏幕位置都在动 —— 自动路径随
    /// 位移逐帧重写 spot,显式上报必须跟住,否则滚动期间候选框钉死在编辑
    /// 器左上角。
    #[test]
    fn ime_rect_follows_scroll_without_input_events() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        // 写回到文末:当帧上报一次,caret 远在首屏之外
        let tail = editor.len_chars();
        let mut selection = None;
        let mut pending = Some((tail, tail));
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert_eq!(rects.len(), 1, "写回帧上报一次");
        assert!(
            rects[0].min.y > 600.0,
            "写回帧 caret 还在视口外(实测 y={})",
            rects[0].min.y
        );

        // 无事件的滚动动画帧:caret 屏幕位置持续变化 → 持续补报并进视口
        let mut reports = 0;
        let mut reached_view = false;
        for i in 0..8 {
            let (_, rects) = frame_with_channel(
                &ctx,
                Vec::new(),
                0.2 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut selection,
                &mut pending,
                &mut cursor,
            );
            reports += rects.len();
            reached_view |= rects.iter().any(|rect| rect.min.y <= 600.0);
        }
        assert!(reports > 0, "滚动动画帧持续上报(共 {reports} 次)");
        assert!(reached_view, "上报的 caret 矩形随滚动进过视口");

        // 动画落定后的真空闲帧:不再报
        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            3.0,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );
        assert!(rects.is_empty(), "落定后的空闲帧不再上报");
    }

    /// #19:编辑器从未持焦点的帧,一个 IME 命令都不发 —— 指针划过也不算。
    #[test]
    fn ime_report_never_fires_without_focus() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("甲乙丙丁");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let (_, rects) = frame_with_channel(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut None,
            &mut None,
            &mut cursor,
        );
        assert!(rects.is_empty());
        let (_, rects) = frame_with_channel(
            &ctx,
            vec![Event::PointerMoved(egui::pos2(400.0, 300.0))],
            0.1,
            &mut editor,
            &mut preview,
            &mut None,
            &mut None,
            &mut cursor,
        );
        assert!(rects.is_empty(), "无焦点的指针事件帧也不上报");
    }

    fn test_ctx() -> egui::Context {
        // 不用 FontDefinitions::empty():空字体下 galley 不含任何字符,
        // clamp_cursor 会把光标一律钳到 0,插入位置全部退化为行首。
        egui::Context::default()
    }

    /// 中文文本事件(与 IME 组合落定时同一 `insert_text` 路径)必须逐字
    /// 进入 rope,并在同一帧刷新预览快照与大纲。
    #[test]
    fn typing_flows_into_rope_and_refreshes_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("# 首标题");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id)); // 等价于用户点击编辑区
        frame(
            &ctx,
            vec![
                Event::Text("\n\n## 次标题".into()),
                Event::Text("more".into()),
            ],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        assert_eq!(editor.text(), "# 首标题\n\n## 次标题more");
        assert!(editor.is_dirty());
        assert!(editor.revision() >= 2, "每个事件独立推进修订号");
        assert_eq!(preview.text, editor.text(), "快照与缓冲一致");
        assert_eq!(preview.synced_rev, editor.revision());
        let levels: Vec<u8> = preview.outline.iter().map(|item| item.level).collect();
        assert_eq!(levels, vec![1, 2], "大纲随编辑同帧重算");
    }

    /// 空闲帧(无输入)不推进修订号、不重建快照。
    #[test]
    fn idle_frames_do_not_touch_snapshot() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("x");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();
        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let rev = editor.revision();
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let text_ptr = preview.text.as_ptr();
        frame(
            &ctx,
            Vec::new(),
            0.3,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.revision(), rev);
        assert_eq!(preview.synced_rev, rev);
        assert_eq!(preview.text.as_ptr(), text_ptr, "空闲帧未重建快照字符串");
    }

    /// TextEdit 内建 undo/redo:Ctrl+Z 回退一步,Ctrl+Shift+Z 重做。
    #[test]
    fn builtin_undo_redo() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Text("一".into())],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // egui undoer 按输入稳定时长(stable_time,默认 1s)切分撤销组:
        // 空转一帧让「一」成为已提交的撤销点,再输入「二」。
        frame(
            &ctx,
            Vec::new(),
            1.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            vec![Event::Text("二".into())],
            1.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一二");

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            &ctx,
            vec![undo],
            1.7,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一", "undo 一步");
        assert_eq!(preview.text, "一");

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        frame(
            &ctx,
            vec![redo],
            1.8,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一二", "redo 恢复");
        assert_eq!(preview.text, "一二");
    }

    /// AI 流式追加(程序化 insert_chars)后的 undo 语义,实测钉住:
    /// TextEdit 内建 undoer 只在绘制时喂状态,看不到程序化插入 —— 流式
    /// 结束后第一次 Ctrl+Z 整体回退到最近一次用户编辑的快照(即「一步
    /// 撤销整段 AI 续写」),再按 Ctrl+Shift+Z 重做可恢复 AI 文本。
    #[test]
    fn undo_after_ai_append_reverts_whole_stream() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Text("一".into())],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 空转让「一」成为已提交的撤销点(undoer 按 stable_time 切组)
        frame(
            &ctx,
            Vec::new(),
            1.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        // AI 流式追加(与 Message::AiChunk 归约同路径:程序化 insert_chars)
        editor.insert_chars(editor.len_chars(), "AI续写内容");
        frame(
            &ctx,
            Vec::new(),
            1.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一AI续写内容");

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(
            &ctx,
            vec![undo],
            1.7,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(
            editor.text(),
            "一",
            "一步撤销整段 AI 续写(回到最近用户编辑快照)"
        );

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        frame(
            &ctx,
            vec![redo],
            1.8,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(editor.text(), "一AI续写内容", "redo 恢复 AI 文本");
    }

    /// 大纲跳转:消费 jump_to 后,TextEdit 持久光标被覆写到目标字符偏移,
    /// 焦点回到编辑器;下一帧输入从新位置继续。
    #[test]
    fn outline_jump_sets_persisted_cursor_and_focus() {
        let ctx = test_ctx();
        let mut editor = EditorBuffer::new("# 甲\n\n正文\n\n## 乙\n");
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(cursor.byte, None, "尚无任何光标交互");

        // 归约产出的跳转目标:二级标题行首(字符偏移)
        let heading_byte = editor.text().find("##").unwrap();
        let target = editor.byte_to_char(heading_byte);
        cursor.jump_to = Some(target);
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let state = TextEditState::load(&ctx, id).expect("已持久化 widget state");
        let range = state.cursor.char_range().expect("光标已覆写");
        assert_eq!(range.primary.index.0, target);
        assert_eq!(range.secondary.index.0, target, "无选区,两端一致");
        assert!(ctx.memory(|m| m.has_focus(id)), "焦点已还给编辑器");
        assert!(cursor.jump_to.is_none(), "跳转请求已消费");
        assert_eq!(cursor.byte, None, "回填发生在覆写之前,跳转当帧仍是旧值(空)");

        // 下一帧:输入从跳转处继续(打到 '#' 前),编辑器仍持焦点
        frame(
            &ctx,
            vec![Event::Text("!".into())],
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert!(editor.text().contains("\n\n!## 乙"), "从新光标处插入");
        assert_eq!(cursor.byte, Some(heading_byte + 1), "光标随输入前进");
    }

    /// 读回编辑器 TextEdit 的屏幕矩形。ScrollArea 的持久 id 经
    /// `make_persistent_id` 与 root ui id 链混合,测试里不便推导;但滚动
    /// 偏移会直接平移内容的屏幕坐标,量 rect 比量内部 state 更黑盒。
    fn editor_rect(ctx: &egui::Context, id: egui::Id) -> egui::Rect {
        ctx.read_response(id).expect("TextEdit 响应已记录").rect
    }

    /// 超长文档(≥500 行)在源码模式下:TextEdit 按内容自然长高(不被
    /// ScrollArea 压扁成一行)—— 「看得见也滚得动」的前提(#29)。
    #[test]
    fn long_document_grows_a_scrollable_editor() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行:足够普通的 Markdown 段落。\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let rect = editor_rect(&ctx, id);
        assert!(
            rect.height() > 600.0,
            "TextEdit 按内容长高(实测 {height}px),而不是被视口压扁",
            height = rect.height()
        );
    }

    /// 滚轮真的滚得动:指针悬在编辑区上滚一格,内容屏幕坐标上移。#29 的
    /// 症状就是「超过一屏的内容看不见也滚不动」;rect 会动 = ScrollArea
    /// 真的在承载滚动(没有它,widget 永远钉在面板顶部)。
    #[test]
    fn mouse_wheel_scrolls_the_long_document() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let top_before = editor_rect(&ctx, id).top();

        frame(
            &ctx,
            vec![
                Event::PointerMoved(egui::pos2(400.0, 200.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 偏移在 end 里落账、指针 hover 判定又滞后一帧,屏幕坐标要再等
        // 一帧布局才反映 —— 补两帧再读
        frame(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.3,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let top_after = editor_rect(&ctx, id).top();
        assert!(
            top_after < top_before,
            "滚轮把内容推离顶部(实测 top {top_before} → {top_after})"
        );
    }

    /// 大纲跳转(消费 jump_to 的那一帧)请求滚动到光标行:写回光标后,
    /// 滚动动画把文末光标带进视口。再渲一帧让动画完成(默认 0.1-0.3s)。
    #[test]
    fn outline_jump_scrolls_cursor_into_view() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 跳到文末(远在首屏之外)
        cursor.jump_to = Some(editor.len_chars());
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 写回当帧即请求滚动(标志取写回目标,不等下一帧的光标状态);
        // 其后动画完成(默认 ≤0.3s)、布局生效、响应可读又各差一帧 ——
        // 补三帧且时间跨过动画窗口
        frame(
            &ctx,
            Vec::new(),
            1.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            2.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            2.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            2.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let rect = editor_rect(&ctx, id);
        assert!(
            rect.bottom() <= 600.0,
            "文末光标滚入视口:内容底部已抬进 600px 视口(实测 {bottom}px)",
            bottom = rect.bottom()
        );
        assert!(
            rect.top() < 0.0,
            "视口确实离开了文档顶部(实测 top {}px)",
            rect.top()
        );
    }

    /// Ctrl+End 到文末:键盘导航帧(End 属于跟随键表)当帧请求滚动,把
    /// 文末带进视口。这条路径不经写回分支,验证的是「键盘导航标志 +
    /// ScrollArea」的组合本身。
    #[test]
    fn ctrl_end_scrolls_to_document_tail() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));
        frame(
            &ctx,
            vec![Event::Key {
                key: Key::End,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }],
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        // 光标写回/变化 → 请求滚动 → 动画完成 → 布局生效 → 响应可读,各差一帧
        frame(
            &ctx,
            Vec::new(),
            0.5,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.7,
            &mut editor,
            &mut preview,
            &mut cursor,
        );

        let rect = editor_rect(&ctx, id);
        assert!(
            rect.bottom() <= 600.0,
            "Ctrl+End 后文末滚入视口(实测 bottom {bottom}px)",
            bottom = rect.bottom()
        );
    }

    /// ArrowDown 连按(键盘导航)时光标行保持可见:删掉每帧 diff 后,
    /// 跟随并没有被一起删掉 —— 导航键当帧置标志,光标行滚入视口
    /// (#29 回归修复的验收红线之二)。
    #[test]
    fn arrow_down_navigation_keeps_cursor_visible() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        ctx.memory_mut(|m| m.request_focus(id));

        // 先把光标明确落到文档头:egui 的 `cursor_at_end` 默认 true,从未
        // 落过光标的编辑器第一次吃导航键会把光标初始化到 `galley.end()`
        // (builder.rs 的 default_cursor_range 分支),那样测的是 egui 的
        // 初始化语义而不是逐行导航。走 pending 写回通道落光标到 0。
        let mut selection = None;
        let mut pending = Some((0, 0));
        frame_with_channel(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut selection,
            &mut pending,
            &mut cursor,
        );

        // 光标从文档头逐行下移:约 45 行处越过 600px 视口底,跟随请求
        // 应从那一帧起把光标行抬回视口
        let down = Event::Key {
            key: Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        for i in 0..45 {
            frame(
                &ctx,
                vec![down.clone()],
                0.2 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }
        // 滚动动画完成、布局生效、响应可读,各差一帧
        for i in 0..3 {
            frame(
                &ctx,
                Vec::new(),
                4.7 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }

        let rect = editor_rect(&ctx, id);
        assert!(
            rect.top() < 0.0,
            "键盘导航把视口推离文档顶部(实测 top {}px):跟随发生了",
            rect.top()
        );
        // 光标行确实还在视口内(没被滚丢):caret 的字符偏移换算内容
        // y ≈ 行号×行高,行高用「内容总高/总行数」从同一 rect 反推
        let caret_char = TextEditState::load(&ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index.0)
            .expect("光标已落位");
        let total_lines = text.lines().count().max(1);
        // caret_char 是字符偏移(中文下 ≠ 字节偏移),逐字符数换行
        let caret_line = text.chars().take(caret_char).filter(|c| *c == '\n').count();
        let line_height = rect.height() / total_lines as f32;
        let caret_content_y = caret_line as f32 * line_height;
        assert!(
            rect.top() <= caret_content_y && caret_content_y <= rect.bottom(),
            "光标行留在视口内(内容 y {caret_content_y}px,视口 [{}, {}])",
            rect.top(),
            rect.bottom()
        );
    }

    /// 滚轮帧/空闲帧绝不触发光标跟随(#29 回归的验收红线:坤哥实测
    /// 「滚动后会自动滚回鼠标的位置」)。
    ///
    /// 早期的每帧 diff 光标方案(及其中间版本的 temp 记忆类型错位)都会
    /// 在非跳转帧误判「光标变了」→ 请求滚回光标。现在跟随只由显式标志
    /// (写回帧/键盘导航帧)触发,滚轮与空闲帧无标志可烧。
    ///
    /// 构造:先滚进文档中部并在那里落光标,再往上滚到光标**下方**之外 ——
    /// 视口应当停在光标上方远处,而不是被拽回。
    #[test]
    fn wheel_scroll_is_not_yanked_back_to_cursor() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let click = egui::pos2(editor_rect(&ctx, id).left() + 40.0, 300.0);
        let wheel = |delta: f32| Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, delta),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };

        // ① 向下滚进文档中部,并在视口中央落下光标
        for i in 0..10 {
            frame(
                &ctx,
                vec![Event::PointerMoved(click), wheel(-120.0)],
                0.1 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }
        frame(
            &ctx,
            vec![
                Event::PointerMoved(click),
                Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: click,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            0.6,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert!(
            TextEditState::load(&ctx, id)
                .and_then(|s| s.cursor.char_range())
                .is_some(),
            "点击后光标落在文档中部"
        );

        // ② 再向上滚:视口退回文档上部,光标被甩到视口下方
        for i in 0..4 {
            frame(
                &ctx,
                vec![Event::PointerMoved(click), wheel(120.0)],
                0.7 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }
        // 滚动是带动画的:先空转几帧让它落到最终偏移
        for i in 0..3 {
            frame(
                &ctx,
                vec![Event::PointerMoved(click)],
                1.6 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }
        let settled = editor_rect(&ctx, id).top();
        assert!(settled < 0.0, "视口确实滚离了文档顶部(实测 top {settled})");

        // ③ 继续空转:光标仍被甩在视口外,但视口一步也不许动
        for i in 0..6 {
            frame(
                &ctx,
                vec![Event::PointerMoved(click)],
                2.0 + f64::from(i) * 0.1,
                &mut editor,
                &mut preview,
                &mut cursor,
            );
        }
        let after = editor_rect(&ctx, id).top();
        assert!(
            (after - settled).abs() < 1.0,
            "空闲帧不得把视口拽回光标(稳定于 {settled}px,现在 {after}px)"
        );
    }

    /// 防每帧抢滚动:没有跳转请求、没有输入的空闲帧,内容的屏幕坐标
    /// 原样不动 —— 光标跟随只在写回/键盘导航帧由显式标志触发,手写
    /// 滚动不会被抢回去。
    #[test]
    fn idle_frames_do_not_steal_scroll() {
        let ctx = test_ctx();
        let text = (0..500).fold(String::new(), |mut acc, i| {
            acc.push_str(&format!("第 {i} 行\n"));
            acc
        });
        let mut editor = EditorBuffer::new(&text);
        let mut preview = PreviewState::new(&editor);
        let mut cursor = OutlineCursor::default();

        let id = frame(
            &ctx,
            Vec::new(),
            0.0,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        let top = editor_rect(&ctx, id).top();
        frame(
            &ctx,
            Vec::new(),
            0.1,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        frame(
            &ctx,
            Vec::new(),
            0.2,
            &mut editor,
            &mut preview,
            &mut cursor,
        );
        assert_eq!(
            editor_rect(&ctx, id).top(),
            top,
            "空闲帧不请求滚动,内容纹丝不动"
        );
    }

    /// #17 M2 全部替换的单条 undo 契约(真实 undoer,无头):经消息路径
    /// (`State::apply`)一次写入后,TextEdit 内建 undoer 的快照只多一份
    /// —— **一次** Ctrl+Z 整体回原状(不是逐命中回退),Ctrl+Shift+Z
    /// 一步重做恢复全部替换结果。undoer 看不到程序化写入,快照按绘制帧
    /// 落(与 AI 流式追加同语义);「全部」绝无逐命中循环写入,undo 粒度
    /// 必然是一整次操作。
    #[test]
    fn undo_after_replace_all_reverts_whole_operation_in_one_step() {
        use crate::state::{Message, State};

        // 帧驱动借用当前标签的三件套(与生产同一绘制路径、同一 widget id:
        // State 首标签稳定 id = 1,与 frame 辅助硬编码的 tab_editor_id(1) 对上)
        fn draw(ctx: &egui::Context, events: Vec<Event>, now: f64, state: &mut State) -> egui::Id {
            let tab = state.tabs.current_mut();
            frame(
                ctx,
                events,
                now,
                &mut tab.editor,
                &mut tab.preview,
                &mut tab.cursor,
            )
        }

        let ctx = test_ctx();
        let mut state = State::default();
        // 可控文本:needle ×2,前后夹 CJK(多字节内容与命中混排)
        state
            .tabs
            .current_mut()
            .editor
            .replace_all("甲 needle 乙 needle 丙");

        let id = draw(&ctx, Vec::new(), 0.0, &mut state); // undoer 首喂:原文快照
        ctx.memory_mut(|m| m.request_focus(id));
        draw(&ctx, vec![Event::Text("首".into())], 0.1, &mut state); // 用户编辑(生产 TextEdit 光标默认在文末,追加)
        draw(&ctx, Vec::new(), 1.5, &mut state); // 稳定 ≥1s:已提交撤销点
        let before = "甲 needle 乙 needle 丙首".to_owned();
        assert_eq!(state.tabs.current().editor.text(), before);

        // 全部替换走真实消息路径:一次写入,一次撤销组
        state.apply(Message::FindBarToggled(true));
        state.apply(Message::FindQueryChanged("needle".to_owned()));
        assert_eq!(state.find.hits.len(), 2);
        state.find.replacement = "针".to_owned();
        state.apply(Message::ReplaceAllInDoc);
        let replaced = "甲 针 乙 针 丙首".to_owned();
        assert_eq!(state.tabs.current().editor.text(), replaced);

        draw(&ctx, Vec::new(), 1.6, &mut state); // undoer 看到替换结果(进 flux)
        draw(&ctx, Vec::new(), 3.0, &mut state); // 稳定:替换结果成为一份新快照

        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        draw(&ctx, vec![undo], 3.1, &mut state);
        assert_eq!(
            state.tabs.current().editor.text(),
            before,
            "一次 Ctrl+Z 整体回原状,不是逐命中回退"
        );

        let redo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND | Modifiers::SHIFT,
        };
        draw(&ctx, vec![redo], 3.2, &mut state);
        assert_eq!(
            state.tabs.current().editor.text(),
            replaced,
            "一步重做恢复全部替换结果(整次操作 = 单个撤销组)"
        );
    }
}
