# M0 技术验证报告

日期: 2026-09-24
状态: 自动化部分完成,真机项待实测
关联: [roadmap.md](roadmap.md) 阶段 1、[adr-002](adr-002-platform-renderer-wysiwyg.md)、[adr-004](adr-004-technical-stack.md)

> 执行环境: Deepin 25(内核 6.18.48-amd64-desktop-rolling)/ X11 会话(DISPLAY=:0)/ rustc 1.98.0 / eframe 0.36.2 + wgpu 30.0.1。
> 本报告只记录**在本机实际执行过**的检查;未执行的项如实标注。Windows 11 / macOS 14 项待真机实测。

---

## 总览

| # | 验证项 | 状态 | 一句话结论 |
|---|---|---|---|
| 1 | IME 中文输入 | **待实测** | 三平台均未测;M0 冒烟载体还没有文本输入框,候选框跟随/不吞字无法自动化 |
| 2 | 长文档性能 | **bench 证据支持,交互 fps 待测** | 解析 ~93 µs/7.3KB、稳态渲染 ~60 µs/帧,距 16.7ms 帧预算两个数量级;10 万字滚动 fps 需骨架期实测 |
| 3 | wgpu 三 target | **Linux ✅(软件 adapter),Win/mac 待真机** | Linux Vulkan 起动并正确渲染,但 adapter 为 llvmpipe;Win11 DX12 / macOS Metal 待真机 |
| 4 | 流式性能边界 | **Linux ✅** | 追加一行 + 整帧渲染 ~9.3 ms < 16.7ms,100ms/chunk 节奏有约 6 帧余量 |
| 5 | 中文渲染 | **Linux ✅** | 无方块;字体方案定案:cfg 原生候选路径表,不引入 fontdb / font-kit |
| 6 | tokio ↔ egui 通道 | **未开始(P1 前补)** | workspace 尚未引入 tokio(符合增量依赖原则),不为验证提前引入 |

**结论:继续。** 自动化可见的全部证据无红旗,不构成换 iced 或停止的信号;但 M0 出口放行仍卡在两条真机项(IME、Win/mac wgpu),维持原计划。

---

## 主验证 1:IME 中文输入 —— 待实测

- **验收标准**:候选框跟随 caret、不吞字、不抢焦点(Win11 微软拼音 + macOS 14 简体拼音)。
- **现状**:M0 冒烟载体(`crates/latermd-app/src/main.rs`)只有只读标签,没有 `TextEdit`,IME 行为无从触发;且候选框跟随属于人工交互观察项,无法在无人值守环境自动化。
- **已完成的相关事实**:X11 下 eframe 窗口稳定运行(见验证 3 证据),无与输入法服务相关的启动报错。
- **下一步**:P0 编辑器骨架落地 `TextEdit` 后,先在本机测 fcitx/ibus,再上 Win11 / macOS 真机;三平台均记录候选框跟随、连续输入不吞字、窗口切换不抢焦点三项观察结果。

## 主验证 2:长文档性能 —— bench 证据支持,交互 fps 待测

`cargo bench -p egui_markdown` 关键时间量级(100 采样;criterion 摘录,2026-09-24):

| 基准 | 时间(区间中值) | 相对 60fps 帧预算(16.67ms) |
|---|---|---|
| `parse_100_sections`(解析) | 92.9 µs | 0.6% |
| `hash_text_100_sections`(全文哈希,缓存失效探测) | 1.23 µs | 0.007% |
| `hash_token_slice_100_sections`(token 哈希) | 17.9 µs | 0.1% |
| `arc_clone_tokens`(token Arc 克隆) | 12 ns | 可忽略 |
| `render_steady_state`(同输入稳态整帧) | 60.1 µs | 0.4% |
| `render_resizing`(每帧改宽度,强制重排) | 389.8 µs | 2.3% |

- 基准文档为 100 个块级 section(标题/代码/列表/表格/引用循环,7,298 字节 / 500 行,`benches/markdown.rs` 的 `generate_document`)。
- **量级解读**:稳态渲染路径(视口剔除 + 缓存命中)与强制重排距帧预算均有两个数量级余量;缓存失效探测(两个哈希)在 µs 级,说明"输入没变就不重排"的守门成本可以忽略。
- **未测**:验收标准是"10 万字 md 滚动到中部 ≥ 55fps"的**交互帧率**,需要真实滚动 + 视口剔除在窗口里的表现,当前 bench 是 headless 固定 700×900 视口、~7KB 文档。线性外推解析约 1.3ms/10万字(仅为量级估计,非实测),但滚动 fps 结论以 P0 编辑器骨架实测为准。
- criterion 自身对 `hash_text_100_sections` 标了 +3.2% 微小回弹、对 `hash_token_slice` / `arc_clone` 标了改善,均在同机两次运行噪声带内,不采取行动。

## 主验证 3:wgpu 三 target —— Linux 通过(软件 adapter),Win/mac 待真机

- **命令**:`timeout 10 cargo run -p latermd-app`(X11,DISPLAY=:0)。
- **结果**:exit=124,即窗口完整跑满 10 秒后被 timeout 终止,stderr 无 panic、无 wgpu 报错;另一轮后台运行 + 截图确认窗口(800×600)持续绘制、帧计数递增。
- **adapter 在屏证据**(界面实读,截图核对):
  - 选中:`AdapterInfo { name: "llvmpipe (LLVM 18.1.8, 256 bits)", vendor: 0, device: 0, device_type: Cpu, backend: Vulkan }`
  - loader 枚举全集:仅 llvmpipe 一项。
- **如实记录**:本机有 Intel UHD Graphics 770(lspci)且 `/usr/share/vulkan/icd.d/` 存在 `intel_icd.json`,但本会话内 Vulkan loader 未枚举出 ANV,wgpu 回落 llvmpipe(软件渲染)仍起动并正确渲染。这是本机会话/驱动配置现象,不是 LaterMD 代码问题;**硬件 Vulkan 路径(RADV/ANV/NV)未在本机验证**,Linux 侧结论限定为"wgpu on Vulkan(软件 adapter)可起动可渲染"。
- **Win11(DX12)/ macOS 14(Metal)**:待真机实测,验收点 = 启动 + `get_info()` 报告合理 adapter(界面已常驻显示,真机只需截图)。

## 附加验证 4:流式性能边界 —— Linux 通过

- 编排给的 bench 摘录截止于 `render_resizing`(其后截断),上游本有流式基准未含在内,本机补跑:

```bash
cargo bench -p egui_markdown --bench markdown -- render_scroll_code_streaming_append
```

- **结果**:`time: [9.00 ms 9.34 ms 9.67 ms]`(100 采样)。
- **基准语义**(`vendor/egui_markdown/benches/markdown.rs:168`):向一个 100 行起步、持续增长的 rust 代码 fence **追加一行**后整帧渲染 `MarkdownLabel`(700×900 视口,暖缓存,`scroll_code_blocks(true)`);一次 criterion 运行内文档长到 ~2200 行,即该中值覆盖了 100–2200 行区间的追加成本。
- **量级解读**:单帧 ~9.3ms < 16.7ms 预算(占 56%),mock LLM 100ms/chunk 的节奏下每 chunk 有约 6 帧余量;roadmap 想要的 500/2000/10000 行**帧率矩阵**需产品骨架窗口实测,该 bench 的行数上限与节奏(每 iter 一行)即当前能拿到的最接近数据。

## 附加验证 5:中文渲染 —— Linux 通过,字体方案定案

- **实现**:`crates/latermd-app/src/fonts.rs` —— 候选路径表读系统字体,`FontData::from_owned` 注入,`push` 到 `Proportional` / `Monospace` 两族**末尾**作回退(拉丁仍走内置字体,CJK 落到系统字体);候选全失配时界面显示警告,不静默。
- **本机命中**:`/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc`,比例 face 2(Noto Sans CJK SC)+ 等宽 face 7(Noto Sans Mono CJK SC)——同一 .ttc 双族单文件;兜底候选为文泉驿微米黑。face index 由 `fc-query` 枚举得出,对特定文件有效(代码注释已注明换文件必须重查)。
- **证据**:窗口截图(800×600)含比例行「雾凇沆砀,天与云与山与水,上下一白 —— 骨直關开办」与等宽行「fn 骨直關() { 雾凇沆砀 }」,放大 3 倍逐字核对:无方块(tofu)、无缺字、无混排异常;SC/JP 字形差异样本字(骨/直/關)按 SC 字形清晰可辨。
- **方案定案**:**cfg 原生候选路径表**(std::fs + 按 `fc-query` 查好的 face index),不引入 `fontdb` / `font-kit`。理由:零新依赖(ADR-004 清单外依赖需单独论证)、M0/P0 的需求只是"中文不是方块"、`.ttc` face index 方案已实测有效;P0 若需要"枚举系统字体/用户自定义字体"再评估 fontdb,届时字体加载应落在设置层而非启动路径。
- **遗留**:Windows(msyh.ttc)/ macOS(PingFang.ttc)候选条目在 P0 打包前按平台补进表(各自需重查 face index),本报告不臆写未经实测的路径。

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

其余 bench 数字引自编排提供的 `cargo bench -p egui_markdown` 摘录(题设材料,非本会话重跑)。
