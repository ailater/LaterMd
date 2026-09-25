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
| 1 | IME 中文输入 | **Linux 已实测:可用,候选框不跟随** | Deepin X11 + fcitx5:中文组词上屏正常,但候选框不落在光标下方;上报链路核对完整,故障点待定位(见验证 1) |
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
