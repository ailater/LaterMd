# IME 跟随修复验收(#19)—— 自动取证与人工清单

日期: 2026-09-30
状态: 自动侧已取证,结论**仅限**「位置上报链路接通 + 全程无 panic」;候选框目视跟随留人工复测,不销账、不宣称已修复
关联: [m0-report.md](m0-report.md) 验证 1(挂账原文与「修复复测」)、[auto-plan.md](auto-plan.md) #19、[decisions-pending.md](decisions-pending.md) #55、commit `e749fce`

---

## 1. 改动说明:显式上报链路

**根因(应用侧)**:egui-winit 0.36 的自动路径把 `IMEOutput::rect`(整个 `TextEdit` widget 矩形;`egui-winit-0.36.2/src/lib.rs:1171` 取 `ime.rect`)当 IME 光标区调 `Window::set_ime_cursor_area`,而不是 caret 处的 `cursor_rect` —— XIM spot 恒钉在编辑器左上角。这就是 m0-report 验证 1「候选框不跟随」的应用侧故障点(m0 首测已核对链路完整,见 [m0-report.md](m0-report.md) 验证 1)。

**修复(commit `e749fce`,仅改 `crates/latermd-app/src/ui/editor.rs`)**:

- **触发判定**(`editor.rs:37` `ime_report_needed`,纯函数):只在编辑器持焦点且满足其一时上报 —— ①写回帧(大纲跳转/格式动作覆写光标并要回焦点);②primary 光标字符偏移相对上次上报有变化;③上报记忆为空(首次/失焦后重进)。失焦帧恒不发;空闲帧(持焦点、光标没动、非写回)不发。
- **每标签记忆**(`editor.rs:29` `ImeCaretTracking`,挂 `editor_id.with("ime-caret")`,与 TextEdit 持久 state 同生命周期):切标签互不惊扰;失焦帧记忆归零(`editor.rs:326`)—— X11 下焦点翻转会让 winit 重建 IME 上下文,重进后光标没动也重报一次。
- **上报动作**(`editor.rs:316`):caret rect = `galley.pos_from_cursor(CCursor(caret))` 平移 `galley_pos`,经 `ViewportCommand::IMERect` 下发。该命令由 egui-winit 在 `ViewportCommand::IMERect` 分支(`egui-winit-0.36.2/src/lib.rs:1884-1892`)调 `Window::set_ime_cursor_area`;eframe 在每帧平台输出阶段消费 viewport commands(`eframe-0.36.2/src/native/wgpu_integration.rs:1300` `process_commands`),晚于自动路径所在的 `handle_platform_output` —— 是每帧最后一次 spot 写入,自动路径的错位值被同帧覆盖。

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
cargo test -p latermd-app -- ime        # 7 passed; 0 failed
```

- `ime_trigger_requires_focus_and_an_actual_change`(红线逐条:失焦恒不报/空闲帧不报/写回必报/无光标不报)
- `ime_rect_follows_caret_while_focused` / `ime_report_repeats_when_focus_returns` / `ime_report_never_fires_without_focus`
- 断言截取口 = 本帧 viewport 命令流里的全部 `IMERect` 矩形(`editor.rs:442`),与生产侧 egui-winit 消费的是同一条命令流。

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

### 2.3 自动验证结论(仅限以下两条)

1. **位置上报链路接通**:egui `ViewportCommand::IMERect` → egui-winit `set_ime_cursor_area` → winit X11 `send_xim_spot`,两轮冒烟 87 次 spot 上报,坐标随光标移动/输入/切标签持续出现。
2. **全程无 panic**:两份 gdb 运行日志中 `panicked|SIGSEGV|SIGABRT` 均 0 命中,流程走完(聚焦、移动、组词上屏、切焦点往返、双标签往返)。

**不属于自动结论的**(不在此宣称):候选框是否目视贴光标、不吞字、不抢焦点 —— 均需人工/真机,见 §3。

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

- **Live Preview 活动块未接显式上报**:本修复只覆盖源码模式 `TextEdit`(`ui/editor.rs`)。Live 模式活动块(`live.rs` 的块编辑器)内组合中文时,候选框仍会落在块编辑器左上角 —— 岔路登记见 [decisions-pending.md](decisions-pending.md) #55,留后续棒(扩 paths 改 `live.rs`,同手法复用)。
- **吞字/抢焦点**:自动化只能证明「输入事件到达且无 panic」,两项语义判据(字符不丢、焦点不被输入法抢走)需人眼+真机,本轮不记结论,随 §3.2 真机验收。
- **冒烟脚本与 gdb 脚本**为一次性取证工具(`/tmp/ime-smoke.sh`、`/tmp/ime-smoke2.sh`、`/tmp/ime-gdb.gdb`),不进仓库;截图证据留存于本轮 `/tmp/shot-*.png`、`/tmp/shot2-*.png`。
