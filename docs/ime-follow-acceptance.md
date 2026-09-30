# IME 跟随修复验收(#19)—— 自动取证与人工清单

日期: 2026-09-30
状态: 自动侧已取证(三轮,轮次 3 含同帧覆盖不变量与 XIM FIFO 消费时序证据),结论**仅限**「位置上报链路接通 + 帧末 spot 值恒为 caret 值 + 全程无 panic」;候选框目视跟随留人工复测,不销账、不宣称已修复
关联: [m0-report.md](m0-report.md) 验证 1(挂账原文与「修复复测」)、[auto-plan.md](auto-plan.md) #19、[decisions-pending.md](decisions-pending.md) #55/#56、commit `e749fce` + finding 1 补丁

---

## 1. 改动说明:显式上报链路

**根因(应用侧)**:egui-winit 0.36 的自动路径把 `IMEOutput::rect`(整个 `TextEdit` widget 矩形;`egui-winit-0.36.2/src/lib.rs:1171` 取 `ime.rect`)当 IME 光标区调 `Window::set_ime_cursor_area`,而不是 caret 处的 `cursor_rect` —— XIM spot 恒钉在编辑器左上角。这就是 m0-report 验证 1「候选框不跟随」的应用侧故障点(m0 首测已核对链路完整,见 [m0-report.md](m0-report.md) 验证 1)。

**修复(commit `e749fce` + 独立评审 finding 1 补丁,仅改 `crates/latermd-app/src/ui/editor.rs`)**:

- **触发判定**(`editor.rs:65` `ime_report_needed`,纯函数,输入 `ImeTriggerInputs`):只在编辑器持焦点且满足其一时上报 —— ①写回帧(大纲跳转/格式动作覆写光标并要回焦点);②caret 条矩形相对上次上报有变化(红线「光标位置实际变化」的屏幕位置形态,含滚动/重排导致的位移);③egui-winit 自动路径将重写 spot 的帧:**有输入事件**(keyup/指针 motion/preedit 文本未变的更新帧,镜像 `egui-winit-0.36.2/src/lib.rs:1173` 的第二个触发项)或**内容矩形变化**(镜像第一个触发项,基准 = `TextEditOutput::text_clip_rect`,它就是自动路径上报的 `inner_rect`,见 egui `builder.rs:781`)。失焦帧恒不发;真空闲帧(无事件、无位移、非写回)不发。
- **每标签记忆**(`editor.rs:30` `ImeCaretTracking`,挂 `editor_id.with("ime-caret")`,与 TextEdit 持久 state 同生命周期):`last_sent_rect`(上次显式上报的 caret 条矩形)+ `last_widget_rect`(上次见到的内容矩形,自动路径触发项的镜像基准);失焦帧记忆归零 —— X11 下焦点翻转会让 winit 重建 IME 上下文,重进后光标没动也重报一次。
- **上报动作**(`editor.rs:357`):caret rect = `galley.pos_from_cursor(CCursor(caret))` 平移 `galley_pos`,经 `ViewportCommand::IMERect` 下发。该命令由 eframe 在平台输出**之后**消费(`eframe-0.36.2/src/native/wgpu_integration.rs:1300` `process_commands`,晚于自动路径所在的 `handle_platform_output`),是每帧最后一次 spot 写入。
- **finding 1 修复的关键**:初版只在「光标字符偏移变化」帧补报,而自动路径在「持焦点+有输入事件」帧也重写 spot(整帧错位,坤哥症状的帧类)。补丁让触发判定**镜像自动路径的谓词**——自动路径要写 spot 的帧,同帧必有显式 caret 值盖回;反之真空闲帧两边都不写,不产生任何多余命令。取舍登记见 [decisions-pending.md](decisions-pending.md) #56。

**依赖栈核实(2026-09-30,本机 cargo registry 源码,先核实再接线)**:

| 环节 | 位置 |
|---|---|
| 命令名 `IMERect(Rect)` | `egui-0.36.2/src/viewport.rs:1159` |
| IMERect → `set_ime_cursor_area` | `egui-winit-0.36.2/src/lib.rs:1884-1892` |
| 自动路径(错位值,取 `ime.rect`) | `egui-winit-0.36.2/src/lib.rs:1160-1187` |
| 命令 trace 日志点(`Processing ViewportCommand::{command:?}`) | `egui-winit-0.36.2/src/lib.rs:1739` |
| X11 XIM spot 上报 | `winit-0.30.13/src/platform_impl/linux/x11/ime/mod.rs:188` `send_xim_spot` |
| 显式命令的消费时序(平台输出阶段) | `eframe-0.36.2/src/native/wgpu_integration.rs:1300` |

## 2. 自动验证证据(2026-09-30 本机实际执行)

### 2.1 单元断言(应用侧命令流确实发出 IMERect)

```bash
cargo test -p latermd-app -- ime        # 9 passed; 0 failed(2026-09-30 finding-1 补丁后复跑)
```

- `ime_trigger_requires_focus_and_change_or_auto_path_risk`(红线逐条:失焦恒不报/真空闲帧不报/写回必报/caret 屏幕位置变化必报/无光标不报;finding 1 项:有输入事件帧必报、内容矩形变化帧必报)
- `ime_rect_follows_caret_while_focused` / `ime_report_repeats_when_focus_returns` / `ime_report_never_fires_without_focus`
- `ime_rect_resent_on_event_frames_without_caret_move`(finding 1 回归:指针 motion 帧、preedit 更新帧、重复同文 preedit 帧光标未动也补报;实测钉住 egui 0.36 会把 preedit 插进缓冲、组合帧 caret 移到组合串末尾)
- `ime_rect_follows_scroll_without_input_events`(内容矩形变化项端到端:写回触发的滚动动画帧无输入事件,上报持续跟住 caret 进视口;落定后空闲帧不再报)
- 断言截取口 = 本帧 viewport 命令流里的全部 `IMERect` 矩形(`frame_with_channel`),与生产侧 egui-winit 消费的是同一条命令流。

### 2.2 冒烟(Linux X11 + fcitx5,真实窗口,xdotool 驱动)

- **环境**:Deepin 25(X11 会话,`XDG_SESSION_TYPE=x11`,`DISPLAY=:0`)/ fcitx5(挂搜狗模块 `com.sogou.ime.ng.fcitx5.deepin`,`XMODIFIERS=@im=fcitx`,`fcitx5-remote`=2 激活)/ 900×600 窗口 / debug 构建(rustc 1.98.0)。
- **方法偏差(如实记录)**:任务预设的 `RUST_LOG=egui_winit=trace` 在本应用上**无输出** —— 实测 `RUST_LOG=egui_winit=trace timeout 8 target/debug/latermd` stderr 为 0 行;根因是应用进程内未安装任何 logger 实现(全仓无 `env_logger`/log 初始化),`egui-winit lib.rs:1739` 的 trace 打点因此 no-op。本模块 paths 不含应用代码,故改用 **gdb 断点直接观测** `'<winit::platform_impl::linux::x11::ime::Ime>::send_xim_spot'`(winit→Xlib 边界,比 trace 日志更靠近生效端):每命中一次打印 `(x,y)` 后 continue。将来若装 logger,`RUST_LOG=egui_winit=trace` 应可见 `Processing ViewportCommand::IMERect(...)` 行。
- **轮次 1**(`bash /tmp/ime-smoke.sh`):点击聚焦编辑器 → 输入英文 → 上/下/左/右/End/Home 移动 → fcitx5 组词「nihao」+空格上屏、移动后再组词「shijie」上屏 → 点预览栏失焦再点回 → 窗口最小化/恢复(焦点翻转)→ Ctrl+Tab。**结果**:spot 上报 45 次、11 个唯一坐标;`grep "panicked|SIGSEGV|SIGABRT"` 0 命中;组词上屏成功(「你好」「世界」进编辑器,预览同步,截图核对)。
- **轮次 2**(`bash /tmp/ime-smoke2.sh`,双标签):标签 1 输入 → Ctrl+N 开标签 2 输入 → Ctrl+Tab 切回标签 1 输入 → 再切到标签 2 输入。**结果**:spot 上报 42 次、10 个唯一坐标;无 panic;两标签往返后输入与上报均恢复。
- **关键摘录**(物理像素坐标;恒定值串与随光标变化值交替出现,与「自动路径每帧报 widget rect、显式命令按 caret 变化补报」的代码路径一致):

```text
# 轮次 1:输入「hello ime smoke」caret 前移 + 光标移动(y=271→194→299 行变化,x 前移)
XIM_SPOT x=320 y=271   XIM_SPOT x=344 y=299   XIM_SPOT x=409 y=299
XIM_SPOT x=435 y=299   XIM_SPOT x=281 y=269   XIM_SPOT x=289 y=269
XIM_SPOT x=297 y=269   XIM_SPOT x=375 y=269
# 轮次 1 后半(失焦/恢复、组词「世界」上屏)与轮次 2(Ctrl+N 双标签往返)上报持续出现:
XIM_SPOT x=281 y=269   XIM_SPOT x=313 y=269   XIM_SPOT x=307 y=194
XIM_SPOT x=315 y=194   XIM_SPOT x=354 y=194
```

### 2.2.1 轮次 3(finding 1 补丁后,帧分段观测:自动/显式两路 spot 写的顺序证据)

- **背景**:独立评审指出轮次 1/2 的离散观测无法回答「同一帧内自动(先)与显式(后)两次 set,fcitx5 最终取哪个」以及「有输入事件但光标未动的帧是否残留错位」(残留路径即 finding 1,已按 §1 修复)。
- **方法升级**(`bash /tmp/ime-smoke3.sh`,一次性脚本不进仓库):gdb 四断点 —— ①`egui_winit::State::handle_platform_output_inner` 打 `FRAME`(每帧平台输出入口,两个公开包装共用);②egui-winit `lib.rs:1176`(自动路径 `set_ime_cursor_area` 调用之前)打 `AUTO_SET`;③`lib.rs:1884`(`ViewportCommand::IMERect` 分支)打 `EXPLICIT_SET`;④`send_xim_spot` 打最终 `(x,y)`。驱动流程:点击聚焦 → **快速连续打字**(`--delay 40`,39 字符)→ 持焦点鼠标 motion(光标静止)→ fcitx5 组词「nihao」+空格上屏 → End 后组词「shijie」+空格 → Left×3+Home → 点预览栏失焦再点回续打。
- **消费时序问题的答案(源码级)**:winit X11 的 spot 写是**异步**的 —— `set_ime_cursor_area` 只向 channel 发 `ImeRequest::Position`,真正的 `send_xim_spot` 在**下一个 X 事件**到达时按 FIFO 批量冲刷(`winit-0.30.13/src/platform_impl/linux/x11/event_processor.rs:88-97`)。因此「最终取哪个」由通道顺序决定:同帧先 auto 后 explicit 入队,帧内最后入队的(恒为显式 caret 值)就是 IC 最终 spot。
- **结果(2026-09-30 本机实测)**:335 帧中 37 帧发生 spot 写,共 74 次(`AUTO_SET` 37 + `EXPLICIT_SET` 37)。**同帧覆盖不变量:`AUTO_SET` 37/37 每次都被同帧随后的 `EXPLICIT_SET` 盖回,0 次未被覆盖**;无 auto-单飞帧(即不存在「帧末停在 widget 左上角」的帧)。自动值恒为编辑器左上角(281,194);显式值随光标精确变化:打字前移 451→352(y 299→314 为折行)、preedit「nihao」增长到 448、Left×3 逐字符回退 435→422→409、Home 跳行首 411(y=299)、失焦点回后自 281 起随「 tail」递增到 289。`panicked|SIGSEGV|SIGABRT` 0 命中;截图 4 张(`/tmp/shot3-*.png`)。
- **关键摘录**(打字段与 motion 段;每帧 `AUTO_SET`(恒定 281,194)后同帧 `EXPLICIT_SET`(随光标),FIFO 冲刷成对出现且 explicit 恒在后):

```text
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=451 y=299   # 打字帧
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=352 y=314   # 折行后
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=383 y=314   # 持续打字
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=383 y=314   # motion 帧(光标静止,同值盖回)
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=448 y=314   # preedit「nihao」
AUTO_SET EXPLICIT_SET   spot x=281 y=194   spot x=435 y=314   # Left×1
```

### 2.3 自动验证结论(仅限以下三条)

1. **位置上报链路接通**:egui `ViewportCommand::IMERect` → egui-winit `set_ime_cursor_area` → winit X11 `send_xim_spot`,三轮冒烟(87 + 74 次)spot 上报,坐标随光标移动/输入/切标签持续出现。
2. **同帧覆盖不变量成立(轮次 3)**:37/37 次自动路径 spot 写均在**同一帧**内被显式 IMERect 盖回,0 次单飞;winit 的 FIFO 冲刷保证帧内最后入队值(恒为显式 caret 值)成为 IC 最终 spot —— 轮次 1/2 里「恒定值与随光标值交替」的残留错位帧类(finding 1)已消除。
3. **全程无 panic**:三份 gdb 运行日志中 `panicked|SIGSEGV|SIGABRT` 均 0 命中,流程走完(聚焦、快速打字、motion、组词上屏、光标移动、切焦点往返、双标签往返)。

**不属于自动结论的**(不在此宣称):候选框是否**目视**贴光标、不吞字、不抢焦点 —— spot 写值正确≠候选框渲染正确(后者由 fcitx5/搜狗模块消费 XIM spot 决定),均需人工/真机,见 §3。

## 3. 人工验收清单

### 3.1 候选框目视贴光标(坤哥,Linux X11 + fcitx5)

- **步骤**:`cargo run -p latermd-app` → 点击编辑器 → fcitx5 切中文 → 输入拼音组词 → 目视候选框位置;移动光标/换行后再组词复看。
- **判定**:候选框出现在光标条下方并随 caret 移动 = 通过,回填 m0-report 验证 1 销账;仍钉在编辑器左上角/屏角 = 不通过,回填 m0-report 验证 1(下一步排查 winit XIM spot → fcitx5/搜狗模块的 XIM 处理,或在同机换英文键盘配置对照)。
- **注意**:候选框浮窗是独立 X 窗口,`import -window` 窗口截图截不到它(本轮截图均未见候选框),必须人眼目视;XTEST 注入键也无法稳定复现浮窗观测,故本项自动化不可达,留人工。

### 3.2 Win11 微软拼音 / macOS 14 简体拼音真机三项(m0 出口线)—— blocked_external

- 判据 = [acceptance-checklist.md](acceptance-checklist.md) §2.4 / §3.4:①候选框跟随光标;②连续输入不吞字;③切走窗口再回来不抢焦点。**任一条不过 = M0 头号风险命中,停下来记档**。
- macOS 前置 = §3.1:必须走 `.app`(dmg 拖入 /Applications 或 brew cask),裸二进制跑命令行会丢输入法上下文,IME 结论不成立。
- **缺什么**:两台真机(Win11 / macOS 14)。本轮未做,维持 m0-report「Win/mac 真机项待实测」原状,不写结论。

### 3.3 Wayland 对照实验(m0-report 验证 1 怀疑方向③)—— blocked_external

- **步骤**:同机切 Wayland 会话,重跑 §2.2 同流程(gdb 断点法同样可用;winit 走 `zwp_text_input_v3`,与 X11/XIM 是两条完全不同的定位路径)。
- **判读**:Wayland 下候选框跟随 → 问题锁定在 X11/XIM 路径(窗口系统侧),应用上报无责;Wayland 也不跟随 → 应用上报或 IME 侧问题。
- **缺什么**:本机当前为 X11 会话(`XDG_SESSION_TYPE=x11`),切会话需登出登入,本轮未做。

## 4. 未覆盖与已知边界

- **镜像谓词的残余窄缝(理论性)**:自动路径按**物理像素**矩形去重(`self.ime_rect_px`),应用侧按**点空间**矩形镜像(`text_clip_rect`)。像素级变化而点空间无变化的帧(如纯 `pixels_per_point` 变化且布局未动)自动路径会写而补报不触发;实际伴随 ppp 变化的重排几乎必然改变 caret 屏幕位置(第②项触发),本轮冒烟未观测到该缝。事件项的镜像方向相反(应用侧读到的是平台输出时的超集,因 egui 帧内 `count_and_consume_key` 等消费只减不增):多触发的补报只是重发同值,无害。
- **Live Preview 活动块未接显式上报**:本修复只覆盖源码模式 `TextEdit`(`ui/editor.rs`)。Live 模式活动块(`live.rs` 的块编辑器)内组合中文时,候选框仍会落在块编辑器左上角 —— 岔路登记见 [decisions-pending.md](decisions-pending.md) #55,留后续棒(扩 paths 改 `live.rs`,同手法复用)。
- **吞字/抢焦点**:自动化只能证明「输入事件到达且无 panic」,两项语义判据(字符不丢、焦点不被输入法抢走)需人眼+真机,本轮不记结论,随 §3.2 真机验收。
- **候选框渲染正确性**:轮次 3 证明的是「应用侧写入 winit 的 spot 值在帧末恒为 caret 值」,fcitx5/搜狗模块把 XIM spot 渲染到哪里仍需目视(§3.1)。
- **冒烟脚本与 gdb 脚本**为一次性取证工具(`/tmp/ime-smoke*.sh`、`/tmp/ime-gdb*.gdb`、`/tmp/ime-analyze3.py`),不进仓库;截图证据留存于本轮 `/tmp/shot3-*.png`(前两轮 `/tmp/shot-*.png`、`/tmp/shot2-*.png`)。
