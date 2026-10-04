# M0 技术验证报告

日期: 2026-09-24
状态: 自动化完成;Linux 真机项已测(IME 首测结论见验证 1),Win/mac 真机项待实测
关联: [roadmap.md](roadmap.md) 阶段 1、[adr-002](adr-002-platform-renderer-wysiwyg.md)、[adr-004](adr-004-technical-stack.md)

> 执行环境: Deepin 25(内核 6.18.48-amd64-desktop-rolling)/ X11 会话(DISPLAY=:0)/ rustc 1.98.0 / eframe 0.36.2 + wgpu 30.0.1。
> 本报告只记录**在本机实际执行过**的检查;未执行的项如实标注。Windows 11 / macOS 14 项待真机实测。

---

## 总览

| # | 验证项 | 状态 | 一句话结论 |
|---|---|---|---|
| 1 | IME 中文输入 | **Linux 已实测:可用;候选框不跟随 → 修复已提交,待人工复测** | Deepin X11 + fcitx5:中文组词上屏正常;候选框不跟随,应用侧根因已定位(自动上报路径错位),显式 IMERect 上报修复已提交(e749fce)—— 自动侧证据仅限「位置上报链路接通 + 无 panic」,候选框目视跟随待人工复测,不销账(见验证 1「修复复测」) |
| 2 | 长文档性能 | **渲染路径 + 交互 fps 均已实测,真机复测后放行** | 自建 10 万字 bench:滚动到中部 430 µs/帧,与顶部持平(视口剔除生效);真窗口滚动 p50 60.3 fps 达标、p95 49.3 fps 略低于 55fps 线 —— 且这是 llvmpipe **软件渲染**的下限 |
| 3 | wgpu 三 target | **Linux ✅(软件 adapter),Win/mac 待真机** | Linux Vulkan 起动并正确渲染,但 adapter 为 llvmpipe;Win11 DX12 / macOS Metal 待真机 |
| 4 | 流式性能边界 | **⚠️ O(n),P1 硬约束** | 500/2000/10000 行追加成本 38.7 / 154.7 / 789.3 ms,约 77 µs/行线性增长;超 ~1300 行即跟不上 100ms/chunk,P1 开工前必须先解决 |
| 5 | 中文渲染 | **Linux ✅** | 无方块;字体方案定案:cfg 原生候选路径表,不引入 fontdb / font-kit |
| 6 | tokio ↔ egui 通道 | **未开始(P1 前补)** | workspace 尚未引入 tokio(符合增量依赖原则),不为验证提前引入 |

**结论:继续。** 自动化可见的全部证据无红旗,不构成换 iced 或停止的信号;但 M0 出口放行仍卡在两条真机项(Win/mac IME、Win/mac wgpu;Linux IME 已首测,见验证 1),维持原计划。

**一条提前暴露的架构约束(不属于 M0 放行条件,但要带进 P1)**:验证 4 的规模矩阵证明流式追加是 O(n)(~77 µs/行),长文档流式写作会线性劣化。这不是「换框架」级别的信号(egui/iced 都要面对同一问题,解法在分段重排而非换 GUI),但**必须在 P1 开工前定方案**,否则 AI 流式写作在长文档上不可用。

---

## 主验证 1:IME 中文输入 —— Linux 已实测(可用,候选框不跟随),Win/mac 待真机

- **验收标准**:候选框跟随 caret、不吞字、不抢焦点(Win11 微软拼音 + macOS 14 简体拼音)。
- **Linux 真机首测(2026-09-25,Deepin 25 / X11 会话 / fcitx5,会话内挂搜狗输入法模块 `com.sogou.ime.ng.fcitx5.deepin`,`XMODIFIERS=@im=fcitx`;载体 = P0 编辑器 `TextEdit`)**:
  - **✅ 中文输入可用**:经输入法组词、上屏均正常,输入功能本身成立。
  - **❌ 候选框不跟随光标**:候选框未出现在光标下方,不随 caret 移动。吞字 / 抢焦点两项本轮未逐项观察,不记结论,随修复复测一并补记。
  - **上报链路核对(2026-09-25,本机 cargo registry 源码)**:光标位置上报链在当前依赖栈中**完整存在** —— egui 0.36.2 `TextEdit` 产出 `Output::ime`(`IMEOutput`,含 caret rect;`widgets/text_edit/builder.rs:952`)→ egui-winit 0.36.2 调 `Window::set_ime_cursor_area`(`src/lib.rs:1177`)→ winit 0.30.13 X11 侧实现了 XIM spot 上报(`x11/ime/mod.rs:188` `send_xim_spot`)。即**不是整条链路缺失**,故障点在链中某一环或输入法侧。
  - **怀疑方向(待验证,非结论)**:① fcitx5 / 搜狗模块在 XIM 路径下对候选框定位的处理 —— 经典 XIM root/over-the-spot 风格下候选框固定于屏角/窗角,不跟随 spot;② rect 数值或上报时机问题(如仅在组合进行中才更新);③ **对照实验(成本最低,先做)**:同机 Wayland 会话跑一次,winit 走 `zwp_text_input_v3`,与 X11/XIM 是两条完全不同的定位路径,可一步区分「窗口系统路径问题」与「应用上报问题」。
  - **定性**:缺陷隔离在「候选框跟随」,不是「输入不可用」;Linux 不在 M0 放行线内(验收标准只定义 Win11/macOS 真机),**不触发风险登记册 #1 的换 iced 应对**。归属定位后再定:app/配置层可修则修;若为上游(egui/winit/fcitx5-XIM)限制,按既定原则记为已知问题,不硬修 egui。
- **历史(2026-09-24)**:M0 冒烟载体(`crates/latermd-app/src/main.rs`)只有只读标签,无 `TextEdit`,IME 无从触发;X11 下 eframe 窗口稳定运行(见验证 3 证据),无输入法服务相关启动报错。
- **下一步**:① 先做上面③的 Wayland 对照实验,定位故障环节后回填本文档(连同吞字/抢焦点观察);② Win11 / macOS 真机实测,记录候选框跟随、连续输入不吞字、窗口切换不抢焦点三项。

### 验证 1 修复复测(2026-09-30,#19 ime-follow,commit e749fce)

- **修复内容(显式上报链路)**:应用侧根因 = egui-winit 0.36 的自动上报路径把 `IMEOutput::rect`(整个 TextEdit 矩形,`egui-winit-0.36.2/src/lib.rs:1171`)当 IME 光标区,XIM spot 恒钉在编辑器左上角。修复:编辑器持焦点且光标位置实际变化的帧,经 `ViewportCommand::IMERect` 显式上报 caret rect(`crates/latermd-app/src/ui/editor.rs:316`);该命令由 eframe 在每帧平台输出阶段消费(`wgpu_integration.rs:1300`),是每帧最后一次 spot 写入,自动路径错位值被同帧覆盖。红线落地:失焦帧/空闲帧不发任何 IME 命令(`editor.rs:37` 触发判定纯函数),不对组合中(composition)状态做额外干预,不发明清除命令。
- **冒烟证据(本机 Deepin 25 / X11(DISPLAY=:0)/ fcitx5 挂搜狗模块,xdotool 驱动真实窗口,两轮)**:编辑器聚焦 → 输入 → 光标上下左右移动 → 中文组词上屏(「你好」「世界」「俺爸」均上屏,截图核对)→ 点预览栏失焦再点回 → 窗口最小化/恢复焦点翻转 → Ctrl+N 开新标签 + Ctrl+Tab 双标签往返 —— **全程无 panic**(两份运行日志 `panicked|SIGSEGV|SIGABRT` 均 0 命中);gdb 断点直击 winit X11 `send_xim_spot`(`winit-0.30.13/.../x11/ime/mod.rs:188`)观测到上报 45 + 42 = **87 次,坐标随光标移动/输入/切标签持续出现** —— 位置上报链路接通。应用侧命令流单测:`cargo test -p latermd-app -- ime` 7 passed(4 条 IME 断言:触发判定红线/聚焦跟 caret/重进重报/失焦不报)。
- **方法偏差(如实记录)**:任务预设的 `RUST_LOG=egui_winit=trace` 在本应用上无输出(实测 stderr 0 行;应用进程未安装 logger 实现,log 宏为 no-op),改用 gdb 断点法直接观测 winit→Xlib 边界的 spot 上报,取证点比 trace 日志更靠近生效端。
- **结论(不销账)**:修复已提交,**候选框目视跟随待人工复测**(清单与判定见 [ime-follow-acceptance.md](ime-follow-acceptance.md) §3.1);Win11 微软拼音 / macOS 14 简体拼音真机三项(跟随/不吞字/不抢焦点,M0 出口线)维持待真机,不写结论(§3.2,blocked_external);Wayland 对照实验(上文怀疑方向③)维持待切会话,未做(§3.3,blocked_external)。自动侧不证明目视跟随,吞字/抢焦点两项本轮不记结论。

### 验证 1 修复复测·补(2026-09-30,独立评审 finding 1/2 处置)

- **finding 1(high,已修)**:评审指出「持焦点+光标未动+有输入事件」的帧(keyup/鼠标 motion/preedit 文本未变的更新帧,打字流中高频)只有 egui-winit 自动路径生效,spot 被重设为 TextEdit 整体矩形左上角,而原触发判定把这类帧判为空闲不发显式命令 —— 「显式命令同帧盖回」的假设只在光标变化帧成立。修复(`crates/latermd-app/src/ui/editor.rs`):触发判定改为**镜像自动路径谓词**(egui-winit lib.rs:1173 = 内容矩形变化 ∨ 本帧有输入事件;镜像基准 = `TextEditOutput::text_clip_rect`,即自动路径上报的 `inner_rect`),并改以 caret 条**屏幕矩形**变化为红线主项(滚动动画帧也被覆盖);真空闲帧(无事件、无位移、非写回)仍一条命令不发。取舍登记 [decisions-pending.md](decisions-pending.md) #56。单测 9 项(新增 finding 1 回归与滚动帧端到端),`cargo test -p latermd-app -- ime` 9 passed、`cargo test -p latermd-app` 全量 418 passed。
- **finding 2(medium,证据补强)**:轮次 3 冒烟(X11+fcitx5,gdb 四断点帧分段)回答了「同一帧两次 set,fcitx5 取哪个」—— winit X11 的 spot 写经 channel 异步 FIFO 冲刷(`event_processor.rs:88-97`),同帧先 auto 后 explicit 入队,帧末值恒为显式 caret 值;**37/37 次自动写均被同帧显式写覆盖,0 单飞**;显式值随打字/组合/箭头精确变化,0 panic。证据与边界见 [ime-follow-acceptance.md](ime-follow-acceptance.md) §2.2.1/§2.3/§4。
- **结论(仍不销账)**:自动侧已证明到「应用侧写入 winit 的帧末 spot 值恒为 caret 值」为止;候选框渲染正确性(fcitx5/搜狗消费 XIM spot)与吞字/抢焦点仍待人工目视/真机,口径不变。

## 主验证 2:长文档性能 —— 10 万字 bench 通过(渲染路径),交互 fps 待真窗口

### 2.1 自建 10 万字基准(2026-09-25,`crates/latermd-app/benches/longdoc.rs`)

上游 bench 规模固定(~7KB),覆盖不到验收规模,故自建:文档 **100,251 字符 / 3,704 行**(中英混排,标题/段落/列表/代码块/表格/引用循环),视口 700×900,经 `ScrollArea` 包裹(与预览面板同一路径)。

```bash
cargo bench -p latermd-app --bench longdoc
```

| 场景 | 中值 | 相对 55fps 预算(18.2ms) | 相对 60fps 预算(16.7ms) |
|---|---|---|---|
| `cold_first_frame`(新 Context,缓存冷) | **136.3 ms** | 749% | 816% |
| `steady_state_top`(缓存热,顶部) | **415.3 µs** | 2.3% | 2.5% |
| `steady_state_scroll_middle`(缓存热,滚到中部 offset 40,000) | **430.2 µs** | 2.4% | 2.6% |

**结论**：

- **滚动到中部的稳态成本 430 µs,与顶部 415 µs 基本持平** —— 视口剔除在 10 万字规模下确实生效,单帧成本与文档长度、滚动位置**无关**。渲染路径距 18.2ms 预算有 **42 倍余量**,验收标准「滚动到中部 ≥ 55fps」在渲染路径上通过。
- **冷首帧 136 ms 是一次性成本**(解析 + 全量排版),发生在打开文件或首次渲染时。它不构成滚动期的掉帧,但意味着打开 10 万字文档会有约 0.14 秒白屏/卡顿;P0 打磨期若嫌慢,可做「首屏优先排版 + 后台续排」。
### 2.2 交互帧率实测(真窗口,2026-09-25)

bench 只覆盖渲染路径。验收标准写的是「滚动到中部 ≥ 55fps」的**交互帧率**,故另建载体 `crates/latermd-app/examples/scrollbench.rs`:真实 eframe 窗口,10 万字文档,每帧 `request_repaint()` 打满帧率,自动滚到中部(offset 40,000px)后持续微滚,跳过前 120 个 warmup 帧后统计。

```bash
cargo run --release --example scrollbench     # 20 秒后自动退出并打印统计
```

| 指标 | 帧耗时 | 折算 fps | 对 55fps 验收线(18.18ms) |
|---|---|---|---|
| 平均 | 16.66 ms | 60.0 fps | ✅ |
| **p50** | **16.58 ms** | **60.3 fps** | ✅ |
| **p95** | 20.29 ms | 49.3 fps | ❌ 略超 |
| p99 | 21.00 ms | 47.6 fps | ❌ 略超 |
| max | 22.63 ms | 44.2 fps | ❌(最差帧,无长尾) |

样本 1,066 帧。

**结论**:

- **p50 60.3 fps 达标**,帧时间集中在 16.6 ms(≈ 显示器 60Hz 的 vsync 周期),说明渲染路径本身没有拖慢帧率 —— 与 2.1 的 430 µs 单帧成本一致。
- **p95 / p99 掉到 47–49 fps,略低于 55fps 验收线**,但抖动幅度很小(最差帧 22.6 ms,无长尾),是偶发掉帧而非持续劣化。
- ⚠️ **本机是 llvmpipe 软件渲染**(见验证 3),这是**下限数据**:软件光栅化比 GPU 慢一个量级,真机(DX12 / Metal / 硬件 Vulkan)预期显著更好。
- 判定:**渲染与交互路径无阻塞,软件渲染下限已接近达标;真机跑同一条命令复测后即可正式放行**。

> **复测注记(2026-10-02,#46,只记事实不销账)**:本节口径同命令复跑,渲染路径三项 headless 数字维持优于 M0(冷首帧 121.84 ms / 稳态顶部 360.55 µs / 稳态中部 373.51 µs,criterion 对 2026-10-02 R1 轮全部 No change);「真窗口 vsync 帧率 p50/p99」在本机会话因会话级 Vulkan WSI Fifo present 阻塞**无法复测**(scrollbench 20 秒 <121 帧,与 M0 同一二进制得 1,066 帧完全不同),上表 p50 60.3 fps 为 M0(2026-09-25)实测历史值,本会话不构成达标/不达标的复测证据,记 blocked_external。诊断链与逐字输出见 [perf-recheck-2026-10.md](perf-recheck-2026-10.md) §3b/§8.4;会话恢复后复跑 §2.2 命令同表对照。

### 2.3 上游小文档基准(2026-09-24 全量重跑)

`cargo bench -p egui_markdown` 关键时间量级(100 采样):

| 基准 | 时间(区间中值) | 相对 60fps 帧预算(16.67ms) |
|---|---|---|
| `parse_100_sections`(解析) | 94.3 µs | 0.6% |
| `hash_text_100_sections`(全文哈希,缓存失效探测) | 1.25 µs | 0.008% |
| `hash_token_slice_100_sections`(token 哈希) | 18.8 µs | 0.1% |
| `arc_clone_tokens`(token Arc 克隆) | 12.5 ns | 可忽略 |
| `render_steady_state`(同输入稳态整帧) | 60.5 µs | 0.4% |
| `render_resizing`(每帧改宽度,强制重排) | 387.5 µs | 2.3% |
| `render_scroll_code_steady_state`(200 行滚动代码块) | 6.79 µs | 0.04% |

- 基准文档为 100 个块级 section(标题/代码/列表/表格/引用循环,7,298 字节 / 500 行,`benches/markdown.rs` 的 `generate_document`)。
- **量级解读**:稳态渲染路径(视口剔除 + 缓存命中)与强制重排距帧预算均有两个数量级余量;缓存失效探测(两个哈希)在 µs 级,说明"输入没变就不重排"的守门成本可以忽略。
- **未测**:验收标准是"10 万字 md 滚动到中部 ≥ 55fps"的**交互帧率**,需要真实滚动 + 视口剔除在窗口里的表现,当前 bench 是 headless 固定 700×900 视口、~7KB 文档。线性外推解析约 1.3ms/10万字(仅为量级估计,非实测),但滚动 fps 结论以 P0 编辑器骨架实测为准。
- criterion 自身对部分基准标了 ±5% 级别的回弹/改善,均在同机多次运行的噪声带内,不采取行动。
- **本表为 2026-09-24 全量重跑**(含 `render_scroll_code_steady_state`),不是摘录。两次运行的解析/渲染中值差异在 2% 内,数据可用。

## 主验证 3:wgpu 三 target —— Linux 通过(软件 adapter),Win/mac 待真机

- **命令**:`timeout 10 cargo run -p latermd-app`(X11,DISPLAY=:0)。
- **结果**:exit=124,即窗口完整跑满 10 秒后被 timeout 终止,stderr 无 panic、无 wgpu 报错;另一轮后台运行 + 截图确认窗口(800×600)持续绘制、帧计数递增。
- **adapter 在屏证据**(界面实读,截图核对):
  - 选中:`AdapterInfo { name: "llvmpipe (LLVM 18.1.8, 256 bits)", vendor: 0, device: 0, device_type: Cpu, backend: Vulkan }`
  - loader 枚举全集:仅 llvmpipe 一项。
- **如实记录**:本机有 Intel UHD Graphics 770(lspci)且 `/usr/share/vulkan/icd.d/` 存在 `intel_icd.json`,但本会话内 Vulkan loader 未枚举出 ANV,wgpu 回落 llvmpipe(软件渲染)仍起动并正确渲染。这是本机会话/驱动配置现象,不是 LaterMD 代码问题;**硬件 Vulkan 路径(RADV/ANV/NV)未在本机验证**,Linux 侧结论限定为"wgpu on Vulkan(软件 adapter)可起动可渲染"。
- **Win11(DX12)/ macOS 14(Metal)**:待真机实测,验收点 = 启动 + `get_info()` 报告合理 adapter(界面已常驻显示,真机只需截图)。

## 附加验证 4:流式性能边界 —— ⚠️ 线性 O(n),P1 的硬约束

- 编排给的 bench 摘录截止于 `render_resizing`(其后截断),上游本有流式基准未含在内,本机补跑:

```bash
cargo bench -p egui_markdown --bench markdown -- render_scroll_code_streaming_append
```

- **结果**:`time: [9.00 ms 9.34 ms 9.67 ms]`(100 采样);同日全量重跑得 `[8.33 ms 8.77 ms 9.21 ms]`,两次同量级,取 ~9 ms 为结论值。
- **成本构成提醒**:该 bench 每次迭代都 `format!` 重建整篇文档字符串(O(n) 拷贝)后再渲染,因此 9 ms **不是纯渲染成本**。它证明的是「追加一行的端到端成本在毫秒量级」,不能证明渲染是 O(line)。P1 若要把流式成本压到更低,需先自建 bench 剥离字符串拼接,再决定是否必须做增量渲染。
- **基准语义**(`vendor/egui_markdown/benches/markdown.rs:168`):向一个 100 行起步、持续增长的 rust 代码 fence **追加一行**后整帧渲染 `MarkdownLabel`(700×900 视口,暖缓存,`scroll_code_blocks(true)`);一次 criterion 运行内文档长到 ~2200 行,即该中值覆盖了 100–2200 行区间的追加成本。
- **量级解读**:单帧 ~9.3ms < 16.7ms 预算(占 56%),mock LLM 100ms/chunk 的节奏下每 chunk 有约 6 帧余量;roadmap 想要的 500/2000/10000 行**帧率矩阵**需产品骨架窗口实测,该 bench 的行数上限与节奏(每 iter 一行)即当前能拿到的最接近数据。

### 4.2 规模矩阵(2026-09-25 自建,回答「是不是 O(line)」)

上游 bench 固定 100 行起步,看不出增长趋势。用 `benches/longdoc.rs` 的 `streaming_append_by_lines` 补齐 roadmap 要的三档规模 —— 每次迭代向已有 fence **追加一行**(只 clone 一次字符串再追加,不是重建整篇),然后整帧渲染:

```bash
cargo bench -p latermd-app --bench longdoc -- streaming_append
```

| 文档规模 | 追加 1 行 + 整帧渲染 | 折合每行 | 相对 100ms/chunk 节奏 |
|---|---|---|---|
| 500 行 | **38.7 ms** | 77 µs | 39%(勉强跟得上) |
| 2,000 行 | **154.7 ms** | 77 µs | **155%(已积压)** |
| 10,000 行 | **789.3 ms** | 79 µs | **789%(彻底爆掉)** |

**结论:追加成本是 O(n),不是 O(line)** —— 规模 ×4 时间 ×4,规模 ×5 时间 ×5,折合约 **77 µs/行**稳定不变。也就是说**每追加一行都要重排整个文档**,上游「should stay near O(line)」的注释在当前实现下不成立。

对 P1 的直接影响(写进 P1 开工前必读):

- 100 行量级(9 ms)完全没问题,但**流式写作一旦超过 ~1,300 行,单 chunk 成本就超过 100 ms 的 LLM 吐字节奏**,界面开始积压掉帧。
- P1 开工前必须先解决这个:方向是「只重排尾部受影响的 block」(vendored 层有 `segment_breaks` 与分段 galley 可复用),或对流中的文档降级为纯文本/低精度排版,等流结束再全量排版。**不要带着当前的 O(n) 直接做 AI 流式写作**。
- 该结论已剥离字符串重建成本(bench 内只做一次 clone + 追加),是渲染/排版本身的增长趋势。

### 4.3 复测(2026-10-02,#46 同口径,只记事实不销账)

- **结论:仍存。** 同命令同参数复跑 §4.2 bench(`cargo bench -p latermd-app --bench longdoc -- streaming_append`,`sample_size(10)` 三档),追加 1 行 + 整帧渲染的中值 = **500 行 33.853 ms(67.71 µs/行)/ 2,000 行 146.00 ms(73.00 µs/行)/ 10,000 行 993.94 ms(99.39 µs/行)**——成本增长 ×4.31 / ×6.81 随规模 ×4 / ×5,每行成本仍随规模上升,**线性 O(n) 判定与 M0 相同**。
- **量级对照**:10,000 行档对 M0(789.3 ms)变差 +25.9%;超 ~1000 行即跟不上 100 ms/chunk 的 LLM 吐字节奏(2,000 行档 146 ms 已超),与 M0「~1,300 行」结论同向。
- **期间未做修复**:M0(2026-09-25)→ 复测 R1 轮(2026-10-02 上午)→ 复测 R3 轮(2026-10-02)之间产品代码零变更(criterion 对 R1 轮三档全部「No change in performance detected」,p=0.29–0.84);主导修复(布局缓存失效粒度块级化)机制位置在 vendored 层,属 ①类改动,需求已挂 [decisions-pending.md](decisions-pending.md) #77 待人工拍板。**本小节不改变 M0 挂账状态,「P1 开工前必解」维持;拍板修复后按 perf-recheck §6 口径复跑,每行成本随规模趋稳(亚线性)方可销账。**
- 逐字输出、增长曲线判定与 R1/R3 前后对比表见 [perf-recheck-2026-10.md](perf-recheck-2026-10.md) §2/§5/§8。

### 4.4 复测销账(2026-10-04,#52 拍板①修复后)—— **销账**

- **修复**:decisions-pending #77 拍板①两步 vendor ①类补丁(M1 分段准入放宽 `be05b52` + M2 块级缓存 key 换块内容 hash `ccc2945`)落地后,M3 复测发现并又修两处主导项(#52 第三步:anchors 记录 O(sections×rows) 二次方 → 单遍扫描;flush 命中帧 O(n) job 拷贝+哈希 → shaped galley 缓存)。修复全程细节、逐字输出与测试证据见 [perf-recheck-2026-10.md](perf-recheck-2026-10.md) §9。
- **销账数字**(同 §4.2 命令,`cargo bench -p latermd-app --bench longdoc -- streaming_append`,2026-10-04 终测):追加 1 行 + 整帧渲染中值 = **500 行 35.886 µs(0.0718 µs/行)/ 2,000 行 107.29 µs(0.0536 µs/行)/ 10,000 行 550.75 µs(0.0551 µs/行)**——每行成本**下降后趋稳**(R1 为 67.6→73.2→100.2 µs 随规模上升),10,000 行档对 M0 基线 789.3 ms **−99.93%**(对 R1 1001.7 ms −99.945%)。增长曲线:×4 规模成本 ×2.99、×5 规模成本 ×5.13、总体 ×20 规模成本 ×15.34;如实注记:2000→10000 段与线性持平(+2.6%),残余线性项 ≈0.05 µs/行(整篇 parse/哈希/anchors 记录),增量消除属另一工程量级,不在本次验收内。
- **原痛点消除**:§4.2「超 ~1,300 行单 chunk 成本即超 100 ms 吐字节奏」——现 10,000 行档单 chunk 550.75 µs = 100 ms 预算的 **0.55%**(余量 181×),三档全部深藏预算内。「P1 开工前必解」解除。
- **连带核对**:长文档滚动路径(long_doc_100k)冷首帧持平(123.90 ms vs R1 123.49 ms),稳态两档 +47%/+54% 的回归经 A/B 定位主因为 **#42 块表记录(main 既有,先于本分支)**,本分支可解释净贡献 ≈ +6%(M2 内容寻址 cull 的代价 − 本步复用收益),距 55 fps 预算余量 34×;像素零变化否决线维持(vendored 152 测试全绿)。#42 每帧记录成本挂 decisions-pending 新条目待其归属模块处理。

## 附加验证 5:中文渲染 —— Linux 通过,字体方案定案

- **实现**:`crates/latermd-app/src/fonts.rs` —— 候选路径表读系统字体,`FontData::from_owned` 注入,`push` 到 `Proportional` / `Monospace` 两族**末尾**作回退(拉丁仍走内置字体,CJK 落到系统字体);候选全失配时界面显示警告,不静默。
- **本机命中**:`/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`,比例 face 2(Noto Sans CJK SC)+ 等宽 face 7(Noto Sans Mono CJK SC)——同一 .ttc 双族单文件;兜底候选为文泉驿微米黑。face index 由 `fc-query` 枚举得出,对特定文件有效(代码注释已注明换文件必须重查)。
- **证据**:窗口截图(800×600)含比例行「雾凇沆砀,天与云与山与水,上下一白 —— 骨直關开办」与等宽行「fn 骨直關() { 雾凇沆砀 }」,放大 3 倍逐字核对:无方块(tofu)、无缺字、无混排异常;SC/JP 字形差异样本字(骨/直/關)按 SC 字形清晰可辨。
- **方案定案**:**cfg 原生候选路径表**(std::fs + 按 `fc-query` 查好的 face index),不引入 `fontdb` / `font-kit`。理由:零新依赖(ADR-004 清单外依赖需单独论证)、M0/P0 的需求只是"中文不是方块"、`.ttc` face index 方案已实测有效;P0 若需要"枚举系统字体/用户自定义字体"再评估 fontdb,届时字体加载应落在设置层而非启动路径。
- **遗留 → 已处理(2026-09-25,P0 打包批次)**:Windows(msyh.ttc / simsun.ttc / simhei.ttf)与 macOS(PingFang.ttc / Hiragino Sans GB.ttc / Supplemental/Songti.ttc)候选已按平台补进 `fonts.rs` 的 `CANDIDATES`(Linux 条目不动、仍在最前),候选表结构(平台前缀互斥、三平台齐备、无重复、Windows face index 约定)有单测。**face index 均未真机核验**(Windows 三条为资料建议值;macOS PingFang 的 0 为占位 —— 任一 face 均含 CJK 可消除方块,SC Regular 确切 index 待真机 `fc-query` / `system_profiler SPFontsDataType` 枚举后修正,本报告不臆写)。Win/mac 首版真机冒烟时一并核对:中文无方块 + index 指向预期字型。

## 附加验证 6:tokio ↔ egui 通道 —— 未开始(P1 前补)

- workspace 当前无 tokio 依赖(全仓 Cargo.toml 检索为空),按"增量创建、不为验证提前引依赖"原则(roadmap 阶段 2 crate 增量表)不为此引入。
- P1 开工时按 ADR-005 §5.2 落地 `tokio::mpsc` + `ctx.request_repaint()` 往返验证;`App::logic` 禁止绘制的约束(ADR-005 §2.3)届时一并实测。

---

## 证据命令清单(本机实际执行)

| 检查 | 命令 | 结果 |
|---|---|---|
| 冒烟(题设原名) | `timeout 10 cargo run -p latermd-app` | exit=124(10 秒存活被 timeout 终止),无报错 |
| 窗口截图核对 | `xdotool search --name '^LaterMD$'` + `import -window <id>` | 800×600 窗口,中文/adapter 信息在屏 |
| 静态门禁 | `cargo fmt --check -p latermd-app`;`cargo clippy -p latermd-app --all-targets -- -D warnings` | 均通过,0 告警 |
| 流式基准补跑 | `cargo bench -p egui_markdown --bench markdown -- render_scroll_code_streaming_append` | 9.34 ms 中值 |
| TTC face 枚举 | `fc-query /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc` | face 2=Sans SC,7=Mono SC |

上表 bench 数字均为本机 `cargo bench -p egui_markdown --bench markdown` 全量重跑结果(2026-09-24),非摘录。

## 附:门禁六项在 workspace 根的复跑(2026-09-24)

| 项 | 结果 |
|---|---|
| `cargo fmt --all --check` | ✅ 全绿(本次修掉 vendor `style.rs` 删 membrane 字段遗留的空行) |
| `cargo clippy --workspace --all-targets` | ✅ |
| `cargo clippy --workspace --all-targets --no-default-features` | ✅ |
| `cargo clippy --workspace --all-targets --all-features` | ✅ |
| `cargo test --workspace --all-features` | ✅ |
| `cargo doc --no-deps --all-features` | ✅ |
| `cargo clippy -p latermd-app --all-targets --features glow` | ✅(仅验证可编译,glow 不进主产物) |

> 这七项已固化为 CI 的 gate job(`.github/workflows/rust.yml`),每次 push / PR 自动执行。
