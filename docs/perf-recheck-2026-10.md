# 流式性能复测与 M0 基线对照(2026-10,#46 R1,只测不改)

日期: 2026-10-02
分支: `feature/streaming-perf-recheck`(head = `618b3ab`,与 origin/main 同步)
关联: [m0-report.md](m0-report.md) 主验证 2 / 附加验证 4、[auto-plan.md](auto-plan.md) #46、roadmap「当前位置」
执行者: R1 基线复测模块(红线:不改任何产品代码;bench 命令、参数、输出逐字记录,数字不做美化)

> **执行环境**: Deepin 25(内核 6.18.48-amd64-desktop-rolling)/ X11 会话(DISPLAY=:0)/ rustc 1.98.0 / bench 二进制 `target/release/deps/longdoc-9a3abfe4edeb0714`。
> M0 报告环境(2026-09-24/25)与本轮的**环境差异见 §1**——其中「会话级 Vulkan FIFO present 阻塞」使真窗口滚动帧率本轮**无法按 M0 口径复测**,已按 blocked_external 如实标注,可自动做的部分(headless bench + 无节流探针旁证)照常完成。

---

## 0. 判定口径(先写口径,再下结论)

**流式追加(验证 4 复测)**

- 判据 = 「追加 1 行 + 整帧渲染」的中值成本随文档规模(500 / 2000 / 10000 行)的**增长曲线**:
  - 成本增长倍数 ≈ 规模增长倍数(×4 规模 ≈ ×4 成本,×5 ≈ ×5)→ **线性 = O(n) 未解**;
  - 成本增长显著低于规模增长、且每行成本随规模**趋稳或下降** → **亚线性 = 已解**。
- 对照 M0 基线(m0-report §4.2:38.7 / 154.7 / 789.3 ms,折合 ~77–79 µs/行)分档记「变好 / 变差 / 持平」,分档带宽 ±5%(带宽内记持平)。
- µs/行 = criterion 中值 ÷ 行数(换算列,非四舍五入美化;中值取 criterion 区间三点之中间值)。
- 基准语义与 M0 完全同源(`benches/longdoc.rs` 自 M0 未改;每迭代一次 `body.clone()` + 追加一行 + 整帧渲染——clone 成本两侧同样存在,可比)。

**长文档滚动(验证 2 复测)**

- 主判据 = 真窗口 `scrollbench` 10 万字滚动 p50 / p99 对照 M0 基线(p50 16.58 ms = 60.3 fps,vsync 锁定,llvmpipe)。
- 辅判据 = headless bench 三项(冷首帧 / 稳态顶部 / 稳态中部)对照 M0(136.3 ms / 415.3 µs / 430.2 µs),同口径同二进制源,完全可比。
- 环境前提必须先声明(§1);主判据若因环境不可执行,记 blocked_external 并以辅判据 + 同载体无节流探针旁证下结论,**不得用旁证数字冒充 vsync-locked 帧率**。

---

## 1. 环境与 M0 基线可比性(先读这个再看数字)

| 项 | M0(2026-09-24/25) | 本轮(2026-10-02) | 对可比性的影响 |
|---|---|---|---|
| rustc / egui / wgpu | 1.98.0 / 0.36.2 / 30.0.1 | 同左(钉死) | 无 |
| bench 载体源码 | `benches/longdoc.rs` + `examples/scrollbench.rs` | **逐字节相同**(git 确认 M0 后未改) | headless 两项完全可比 |
| Mesa / LLVM | LLVM 18.1.8(m0-report §验证3) | **25.0.7 / LLVM 19.1.4**(glxinfo 实读,系统升级) | 涉及 present 行为(§3b) |
| 开机 / 会话 | M0 会话 | 2026-09-28 重启;kwin_x11 自 09-28 09:21 运行,合成 active(dbus-send `org.kde.kwin.Compositing.active` → `boolean true`) | 涉及 present 行为 |
| 显示器 | 未记录 | 双屏 DP-1(主,1920×1080)+ DP-3(1920×1080),当前 3840×1080 | 涉及 present 行为 |
| Vulkan 枚举 | **仅 llvmpipe 一项**(m0-report §验证3) | **两项**:`[0] llvmpipe (LLVM 19.1.4, 256 bits)` + `[1] Intel(R) Graphics (RPL-S)`,loader 排序后 **wgpu 建在 Intel 硬件 GPU 上**(证据:`VK_LOADER_DEBUG=all` 逐字摘录——) | 环境口径与 M0 不同,滚动项需注明 |
| app 在屏 adapter 显示 | M0 冒烟载体有(§验证3 截图核对) | **已随冒烟载体移除**(现产品 UI 无 AdapterInfo 显示) | adapter 证据改用 loader debug,见上 |

`VK_LOADER_DEBUG=all target/release/examples/scrollbench` 关键逐字输出(stderr 摘录):

```
[Vulkan Loader] INFO | DRIVER:  linux_read_sorted_physical_devices:
[Vulkan Loader] INFO | DRIVER:       Original order:
[Vulkan Loader] INFO | DRIVER:             [0] llvmpipe (LLVM 19.1.4, 256 bits)
[Vulkan Loader] INFO | DRIVER:             [1] Intel(R) Graphics (RPL-S)
[Vulkan Loader] INFO | DRIVER:       Sorted order:
[Vulkan Loader] INFO | DRIVER:             [0] Intel(R) Graphics (RPL-S)
[Vulkan Loader] INFO | DRIVER:             [1] llvmpipe (LLVM 19.1.4, 256 bits)
...
[Vulkan Loader] DRIVER:            <Device>
[Vulkan Loader] DRIVER:                Using "Intel(R) Graphics (RPL-S)" with driver: "libvulkan_intel.so"
```

**可比性结论**:§2(流式)与 §3a(headless 滚动)不涉及 GPU / 窗口 / present(纯 `egui::Context::run_ui`,CPU 路径),与 M0 数字**同口径可比**;§3b(真窗口滚动)受 §1 环境变化影响,主判据本轮不可得,见 §3b 的诊断链与 blocked_external 标注。

---

## 2. 流式追加复测(验证 4)—— **O(n) 仍未解,10000 行档变差**

命令(M0 同款,`benches/longdoc.rs` 的 `streaming_append_by_lines` 组,`sample_size(10)`):

```bash
cargo bench -p latermd-app --bench longdoc -- streaming_append
```

逐字输出(2026-10-02,全量,含 criterion 告警):

```
    Finished `bench` profile [optimized] target(s) in 0.15s
     Running benches/longdoc.rs (target/release/deps/longdoc-9a3abfe4edeb0714)
Gnuplot not found, using plotters backend
long doc: 100251 chars, 3704 lines
Benchmarking streaming_append_by_lines/append_1_line_at_500
Benchmarking streaming_append_by_lines/append_1_line_at_500: Warming up for 3.0000 s
Benchmarking streaming_append_by_lines/append_1_line_at_500: Collecting 10 samples in estimated 5.6012 s (165 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_500: Analyzing
streaming_append_by_lines/append_1_line_at_500
                        time:   [33.548 ms 33.800 ms 34.120 ms]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild
Benchmarking streaming_append_by_lines/append_1_line_at_2000
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 8.0s or enable flat sampling.
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Collecting 10 samples in estimated 7.9906 s (55 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Analyzing
streaming_append_by_lines/append_1_line_at_2000
                        time:   [144.96 ms 146.46 ms 147.75 ms]
Benchmarking streaming_append_by_lines/append_1_line_at_10000
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 10.1s.
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Collecting 10 samples in estimated 10.120 s (10 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Analyzing
streaming_append_by_lines/append_1_line_at_10000
                        time:   [990.48 ms 1.0017 s 1.0177 s]
Found 3 outliers among 10 measurements (30.00%)
  2 (20.00%) low mild
  1 (10.00%) high severe
```

换算表(中值折算,µs/行 = 中值 ÷ 行数):

| 档位 | 本次中值 | 本次 µs/行 | M0 中值 | M0 µs/行 | 分档(±5% 带宽) |
|---|---|---|---|---|---|
| 500 行 | 33.800 ms | **67.60 µs** | 38.7 ms | 77.4 µs | **变好(−12.7%)** |
| 2,000 行 | 146.46 ms | **73.23 µs** | 154.7 ms | 77.35 µs | **≈持平(−5.3%,带宽边界)** |
| 10,000 行 | 1,001.7 ms | **100.17 µs** | 789.3 ms | 78.93 µs | **变差(+26.9%)** |

增长曲线判定(口径见 §0):

- 规模 ×4(500→2000):成本 ×**4.33**(M0 为 ×4.00);
- 规模 ×5(2000→10000):成本 ×**6.84**(M0 为 ×5.10);
- 每行成本**随规模上升**(67.6 → 73.2 → 100.2 µs),M0 是平的(77.4 → 77.35 → 78.9 µs)。

**判定:流式追加仍是 O(n),未解;且 10000 行档绝对值比 M0 变差 +26.9%(789.3 ms → 1001.7 ms),曲线还略呈超线性。** 超 ~1000 行即跟不上 100 ms/chunk 的 LLM 吐字节奏(2000 行档 146 ms 已超),与 M0「~1300 行」的结论同向且更紧。

---

## 3. 长文档滚动复测(验证 2)

### 3a headless 渲染路径(与 M0 完全可比)—— **三项全部变好,视口剔除保持**

命令(M0 同款):

```bash
cargo bench -p latermd-app --bench longdoc -- long_doc_100k
```

逐字输出(2026-10-02,全量):

```
    Finished `bench` profile [optimized] target(s) in 0.17s
     Running benches/longdoc.rs (target/release/deps/longdoc-9a3abfe4edeb0714)
Gnuplot not found, using plotters backend
long doc: 100251 chars, 3704 lines
Benchmarking long_doc_100k/cold_first_frame
Benchmarking long_doc_100k/cold_first_frame: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 6.7s or enable flat sampling.
Benchmarking long_doc_100k/cold_first_frame: Collecting 10 samples in estimated 6.7376 s (55 iterations)
Benchmarking long_doc_100k/cold_first_frame: Analyzing
long_doc_100k/cold_first_frame
                        time:   [120.95 ms 123.49 ms 126.19 ms]
Benchmarking long_doc_100k/steady_state_top
Benchmarking long_doc_100k/steady_state_top: Warming up for 3.0000 s
Benchmarking long_doc_100k/steady_state_top: Collecting 10 samples in estimated 5.0183 s (14k iterations)
Benchmarking long_doc_100k/steady_state_top: Analyzing
long_doc_100k/steady_state_top
                        time:   [357.08 µs 358.83 µs 360.95 µs]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild
Benchmarking long_doc_100k/steady_state_scroll_middle
Benchmarking long_doc_100k/steady_state_scroll_middle: Warming up for 3.0000 s
Benchmarking long_doc_100k/steady_state_scroll_middle: Collecting 10 samples in estimated 5.0162 s (13k iterations)
Benchmarking long_doc_100k/steady_state_scroll_middle: Analyzing
long_doc_100k/steady_state_scroll_middle
                        time:   [365.44 µs 371.36 µs 376.43 µs]
```

对照 M0(m0-report §2.1):

| 场景 | 本次中值 | M0 中值 | 分档(±5% 带宽) |
|---|---|---|---|
| `cold_first_frame` | 123.49 ms | 136.3 ms | **变好(−9.4%)** |
| `steady_state_top` | 358.83 µs | 415.3 µs | **变好(−13.6%)** |
| `steady_state_scroll_middle` | 371.36 µs | 430.2 µs | **变好(−13.7%)** |

顶部与中部仅差 12.5 µs(≈3.4%),视口剔除在 10 万字规模下依旧生效,单帧成本与滚动位置无关;距 18.18 ms(55 fps)预算余量 **48.9×**(M0 为 42×)。

### 3b 真窗口滚动帧率 —— **本轮无法按 M0 口径复测(blocked_external:会话级 Vulkan FIFO present 阻塞)**

命令(M0 同款):

```bash
target/release/examples/scrollbench     # 20 秒自动退出并打印统计
```

逐字输出(2026-10-02,共 6 次运行——默认 adapter、`VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` 钉 llvmpipe、聚焦/置顶三种条件,全部同一结果):

```
scrollbench: 文档 100030 字符 / 4384 行
scrollbench: 没有采到帧
```

「没有采到帧」的语义(scrollbench.rs:79-84):样本仅在 `frame_index > 120` 后采集,而统计在 20 秒后打印——即 **20 秒内 ui 帧数不足 121 帧(≤6 fps)**,与 M0 同一二进制的 1,066 帧(p50 60.3 fps)完全不同。

**诊断链(逐字证据,定位阻塞层,全部为只读观测或临时探针,见 §4-C):**

1. **窗口正常、进程几乎不耗 CPU**:窗口 `Map State: IsViewable`(800×600);进程 CPU ticks 时间线(`awk '{print $14+$15}' /proc/PID/stat`,每秒采样)——前 2 秒 23 ticks(启动 + 解析),其后 23 秒仅再涨 7 ticks(≈70 ms CPU ≈ 每秒 ~10 ms,按稳态单帧 ~371 µs 折算 ≈ **6–7 fps**,其余时间全部睡眠)。
2. **阻塞点在 Vulkan WSI present 队列线程**:线程列表(`for tid in /proc/PID/task; cat wchan`)逐字摘录——
   ```
   tid 2544741: wchan=futex_do_wait WSI swapchain q
   ```
   主线程与其余工作线程同样停在 futex,仅有 X11 事件线程在 `poll_schedule_timeout`。
3. **GL 通路完全健康(同会话对照)**:`timeout 7 glxgears` 逐字输出——
   ```
   Running synchronized to the vertical refresh.  The framerate should be
   approximately the same as the monitor refresh rate.
   2164 frames in 5.7 seconds = 377.875 FPS
   ```
   GLX 的 vblank 节流失效是「失败开放」(不等待,照跑 377 fps);Vulkan Fifo 是「失败关闭」(等一个不来的 vblank,每帧卡 ~150 ms)。
4. **无节流探针:应用侧渲染+提交+present 通路健康**(临时探针 `scrollbench_novsync.rs`,与 scrollbench.rs 唯一差异是 `WgpuConfiguration.surface.present_mode` 覆盖,测完已删,见 §4-C)。默认 adapter(Intel RPL-S 硬件)两次运行逐字输出:

   `PROBE_PRESENT=immediate`(PresentMode::Immediate):
   ```
   scrollbench-novsync: present_mode=Immediate
   scrollbench: 文档 100030 字符 / 4384 行
   --- scrollbench-novsync 结果 ---
   帧数: 23551 (含启动与冲到中部的阶段)
   平均: 0.82 ms  (1226.5 fps)
   p50: 0.73 ms  (1366.6 fps)
   p95: 1.08 ms  (923.3 fps)
   p99: 1.21 ms  (829.7 fps)
   max: 5.37 ms  (186.3 fps)
   ```

   `PROBE_PRESENT=mailbox`(PresentMode::Mailbox):
   ```
   scrollbench-novsync: present_mode=Mailbox
   scrollbench: 文档 100030 字符 / 4384 行
   --- scrollbench-novsync 结果 ---
   帧数: 9113 (含启动与冲到中部的阶段)
   平均: 1.96 ms  (509.0 fps)
   p50: 0.67 ms  (1486.5 fps)
   p95: 0.81 ms  (1240.4 fps)
   p99: 1.14 ms  (877.4 fps)
   max: 661.20 ms  (1.5 fps)
   ```
5. **llvmpipe 无法绕过 Fifo**:`VK_DRIVER_FILES=.../lvp_icd.json PROBE_PRESENT=immediate` 逐字输出(exit=101)——
   ```
   wgpu error: Validation Error

   Caused by:
     In Surface::configure
       Requested present mode Immediate is not in the list of supported present modes: [Fifo]
   ```
   即 M0 的 adapter(llvmpipe)上 Fifo 是**唯一** present 模式,本会话的 Fifo 阻塞在其上无法规避。

**判定**:

- **渲染路径无回归、反而变好**(§3a 三项 + §3b-4 旁证:同样载体无节流 present 下 p50 0.73 ms/帧,距 18.18 ms 预算 24.9×);
- **「vsync 锁定 p50/p99」这一 M0 口径的数字,本轮在本会话测不出来**——阻塞层定位在会话/驱动(Mesa 自 M0 后升级、双屏、kwin 会话)的 Vulkan WSI Fifo/vblank 等待,**不是 LaterMD 或 vendored 层代码路径**(同一二进制 M0 得 60.3 fps;两个 adapter、聚焦/置顶均复现;GL 正常);
- 该项记 **blocked_external**:缺一个 Vulkan Fifo present 正常的会话(或真机)。恢复后的复测命令即上文 `target/release/examples/scrollbench`,与 m0-report §2.2 同表对照。

---

## 4. 疑点清单

**疑点 A:流式 O(n) 的机制层 = 布局缓存整篇失效(文本哈希门控粒度过粗)**
- 证据(vendored 层,本次实读):`vendor/egui_markdown/src/label.rs:57-64` `hash_text` 对**整篇文本**哈希;`label.rs:636` 缓存命中要求 `cached.text_hash == text_hash`;`label.rs:663-664` miss 即 `parser::parse(text)` 全文解析 + 全量 layout;块级高度缓存 `try_cull_block`(`label.rs:137-151`)同样以 `text_hash` 为 key,`label.rs:141-143` `cached_hash != text_hash` 即全块失效——**追加一行 = 整篇每块重排**,这就是 ~100 µs/行 × 行数的来源。
- 分层排除:「逐帧全量重解析」不是主导(解析按 M0 上游 bench 94.3 µs/100 节外推 ~1.3 ms/10 万字,占 10000 行档 1001.7 ms 的 ~0.13%);「heal 全文」不在本 bench 路径(`MarkdownLabel` 默认 `heal: false`,`label.rs:352`);「图集重建」不适用于 headless bench(无 GPU)。**注意**:产品 preview 路径开启了 `heal(true)`(`crates/latermd-app/src/ui/preview.rs:597`),流式场景每帧另有一次 O(n) 补闭合扫描——R2 修布局时该层要一并纳入考虑。

**疑点 B:10000 行档 +26.9% 的怀疑方向(相关,未插桩证明因果)**
- M0 之后 vendored 层落地了四个排版类改动:`2a36f64`(行高按字号×比例)、`031fc6f`(CJK 行高下限 `min_line_height_em`)、`02b33ad`(标题字号分级 + `heading_space_above` 透明 spacer 行)、`fe286e7`(section 锚点暴露)、`6191beb`(表格底色)。这些都会增加每行/每节的排版工作量,方向上与 10000 行档变差一致;500/2000 行档反而变好,说明存在相互抵消的因素(如缓存/分配行为随规模变化)。**未做插桩归因**,如实存疑,留 R2 修复时顺带核。

**疑点 C:真窗口 fps 不可测的根因层 = 会话/驱动的 Vulkan WSI Fifo/vblank 等待(非应用代码)**
- 三条独立证据:①同一 scrollbench 二进制在 M0(2026-09-25)得 1,066 帧/p50 60.3 fps,本轮 ≤121 帧;②阻塞与 adapter 无关(Intel 硬件 ANV 与 llvmpipe 均复现)、与窗口焦点无关(focus/raise 后复现),且唯一被阻塞的系统线程是 `WSI swapchain q`;③同一载体把 present 换成 Immediate/Mailbox 后 23551 帧/p50 0.73 ms,渲染+提交+present 全链路健康。根因(节点 vblank 事件失效:Mesa 25.0.7 升级 / 双屏 CRTC / kwin 会话)在本模块权限之外,不在 R2 代码修复范围。
- 探针载体说明:`crates/latermd-app/examples/scrollbench_novsync.rs` 为一次性探针(scrollbench.rs 原样拷贝 + `NativeOptions.wgpu_options.surface.present_mode` 覆盖,`eframe::wgpu::PresentMode::Immediate|Mailbox`,env `PROBE_PRESENT` 选择),**测完已删**,工作区净(仅新增本文档);探针数字只作旁证,不得当 vsync-locked 基线引用。

---

## 5. 判定结论汇总

| 项 | M0 基线 | 本轮 | 判定 |
|---|---|---|---|
| 流式 500 行 | 38.7 ms(77.4 µs/行) | 33.800 ms(67.60 µs/行) | 变好 |
| 流式 2000 行 | 154.7 ms(77.35 µs/行) | 146.46 ms(73.23 µs/行) | ≈持平(带宽边界) |
| 流式 10000 行 | 789.3 ms(78.93 µs/行) | 1001.7 ms(100.17 µs/行) | **变差(+26.9%)** |
| 流式增长曲线 | 线性 O(n) | **线性偏超线性(×4.33 / ×6.84)** | **O(n) 未解** |
| 滚动·冷首帧 | 136.3 ms | 123.49 ms | 变好 |
| 滚动·稳态顶部 | 415.3 µs | 358.83 µs | 变好 |
| 滚动·稳态中部 | 430.2 µs | 371.36 µs | 变好 |
| 滚动·真窗口 p50 | 60.3 fps(16.58 ms) | **不可测(blocked_external)** | 渲染路径旁证健康;p50/p99 留待会话恢复复测 |

**一句话结论**:M0 验证 4 的流式 O(n) 问题**仍未解决**(成本随行数线性增长,10000 行档还比 M0 变差 ~27%),R2 应按最小修复开工;M0 验证 2 的滚动渲染路径**无回归且优于基线**,但「真窗口 vsync 帧率」这一口径在本机会话因 Vulkan WSI Fifo present 阻塞测不出数字,属环境问题,不构成应用层达标/不达标的证据。

## 6. 对 R2 的交接

- 修复对象 = 疑点 A:把失效粒度从「整篇 text_hash」降到「块/段」(追加只失效尾块),复用 `segment_breaks` 与分段 galley(m0-report §4.2 既定方向);产品 preview 的 `heal(true)` O(n) 扫描一并评估。
- 修复后用 §2 同一条命令对照本表,验收口径 = 每行成本随规模趋稳/下降(亚线性),而不是绝对值。
- 疑点 B 在修复时顺带核对四个 vendor 排版 commit 的单档影响;疑点 C 走环境复测(真机或会话修复后跑 §3b 命令),不占 R2。
