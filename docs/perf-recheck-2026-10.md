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

---

## 7. R2 修复评估(2026-10-02,#46 R2 模块)—— **主导修复确需 vendor ①类,app 侧无可达近似,如实报未修**

> R2 输入 = §5 判定(流式 O(n) 未解 → 修复路径激活)。R2 模块红线:`paths=["crates"]`、零新增依赖、**不触碰 vendor/**;约束原文「修复确需 vendor 时停下,把需求记 decisions-pending 交人工拍板,本模块按 app 侧可达近似或如实报未修」。本节 = 机制复核 + app 侧可达性逐项核对 + 结论。**crates/ 零改动,本模块未修任何产品代码。**

### 7.1 疑点 A 机制复核(实读 vendored 层,两个 R1 未展开的 nuance)

R1 疑点 A 的机制描述(整篇 `hash_text` 门控,miss 即全文解析)复核**属实**,行号与现状一致;补两个对 vendor 修复设计有直接影响的细节:

1. **bench 场景走整篇 galley 路径,分段路径的 per-range 缓存帮不了它。** §2 bench 的 `code_fence` 文档是纯代码块,`MarkdownLabel` 未开 `scroll_code_blocks` 且无 `link_handler`(`benches/longdoc.rs:80-82`),`needs_segmentation` 返回 false(`vendor/egui_markdown/src/layout.rs:262-275`:纯 `CodeBlock` token 且两条件皆 false)→ 走 `label.rs:679-691` 的**整篇 `build_layout` 单 galley**,10000 行档的 ~1 秒主导在这里。分段路径(`render_segmented`)里 `flush_text_range` 的段缓存本来就是 per-range 的(`label.rs:975-976` `hash_flush_context` 按 **token 切片**哈希,`:991-1008` 命中即复用 galley)——「失效粒度过粗」对**文本段**不成立,成立的是顶层门控(`label.rs:636`)与块级剔除 key。
2. **即使强制分段,块级剔除缓存的 key 仍是整篇 text_hash。** `try_cull_block`/`cache_block_height`(`label.rs:137-156`)以 `text_hash`(整篇,`:629` 传入)为 key,`:772`(Table)/`:808`(滚动代码块)/`:837`(Image)三个块型都是——追加一行让整篇 hash 变,**视口外所有块剔除失效、全部重测**。这解释了 M0 上游 bench(`scroll_code_blocks(true)`,m0-report §验证 4 基准语义)同样呈线性的现象。**含义:app 侧开 `scroll_code_blocks` 不是修复,块级 key 必须换。**

### 7.2 app 侧可达性核对表(任务候选方向 + 自查,逐项行号证据)

| 候选方向(R1 §6 / 任务书) | 核对结果 | 证据(本轮实读) |
|---|---|---|
| 流式帧避免全文重解析(修订号增量) | 快照侧**已在位**;全文重解析主导在 vendor `render()` miss 分支,app 不可达 | `ui/editor.rs:295-299`「快照只在修订号前进时重建…第一层,vendored 层 text hash 缓存是第二层」、`live.rs:287-290` 同款;vendor `label.rs:663-665` |
| heal 只对流式预览帧生效(路径核对) | **已在位(#39 落地)**,两处调用点均按「AI 流式写入本标签」条件传参,稳态帧 `heal=false` | `ui/layout.rs:301-310`(三栏)、`ui/layout.rs:671-674`(Live,注释「heal 条件同三栏路径」)、`ui/preview.rs:594-597` 消费;流式帧 heal 是语义必需(AGENTS §6.5),成本 ~0.1 µs/行量级(§4-A 实测 2 万行 ~2ms),占 10000 行档 <0.1%,不动 |
| 布局缓存命中 | app 侧贡献项全稳定,无泄漏:handler `id()` 用 trait 默认 0、widget id 只含 tab id、font/style 稳定 | `ui/preview.rs:231-258`(AiLinkHandler 无 `fn id` override)、`vendor link.rs:92-94`(默认 0)、`ui/preview.rs:539-541`(`tab_preview_id` 只含 tab id,§6.7) |
| 避免无关帧重建 | **已在位**:修订号纪律 + egui request_repaint 纪律,空闲帧不渲染 | `ui/editor.rs:295-299`、`live.rs:288` |
| (R2 自查)`scroll_code_blocks(true)` 强制分段 | **不构成修复**:§7.1-2(cull key 整篇哈希,追加仍全块失效)+ 代码块渲染形态改滚动窗格(#38 复制头/既有观感回归) | vendor `label.rs:137-151/772/808`;#39 已记「预览既有配置不开 scroll_code_blocks」 |

### 7.3 结论与去向

- **未修(如实)**:O(n) 主导修复(失效粒度块级化)确需 vendor ①类改动,越出本模块红线;app 侧四候选方向 + 自查开关逐项核对,无一个可达且能改变 O(n) 判定曲线的修复点。按约束「如实报未修」执行,本模块 `crates/` **零改动**。
- 修复需求(两步 vendor ①类方案 + 产品侧降级备选)已按五要素登记 **decisions-pending #77**,交人工拍板。
- R3 复测口径:水位即 §2 当前水位(未修),挂账不销;拍板后修复的验收命令与口径见 §6 第二条。
- 单测:无修复行为,无新增断言(不造假凑数);已在位机制的既有护栏 = heal 开关切换不改渲染(`ui/preview.rs` `preview_reflects_edits_immediately_despite_per_tab_cache`)、widget id 稳定(#39 `tab_switch_perf` 回归)、rev 纪律(state.rs 既有断言),本轮未动。
- 门禁:本模块零代码改动,实跑 fmt/clippy 三轮/test 确认工作区健康(结果见 §7.4);六项全量门禁由编排在本棒最终 head 复验。

### 7.4 门禁实跑记录(2026-10-02,本模块收尾时,全部实跑)

```
$ cargo fmt --all --check                                            # 通过(exit 0,无输出)
$ cargo check --workspace --all-features --quiet                     # 通过(exit 0)
$ cargo clippy --workspace --all-targets -- -D warnings              # 通过(exit 0)
$ cargo clippy --workspace --all-targets --no-default-features -- -D warnings   # 通过(exit 0)
$ cargo clippy --workspace --all-targets --all-features -- -D warnings          # 通过(exit 0)
$ cargo test --workspace --all-features                              # 通过(exit 0)
```

test 汇总:**830 passed / 0 failed / 1 ignored**(唯一 ignored = #39 既有 `tab_switch` 取证测试,与 #47 A2 轮基线一致);本模块零代码改动,数字为工作区健康确认,非修复后对比。`cargo doc` 未在本模块单跑(零代码改动无文档面变化),由编排在本棒最终 head 六项全量门禁复验。

---

## 8. R3 同口径复测与前后对比(2026-10-02,#46 R3 模块)—— **未做修复,水位即当前水位;六项 headless 数字与 R1 无统计差异**

> 前置事实:#46 R2 未触发代码修复(§7:主导修复确需 vendor ①类,app 侧无可达近似,`crates/` 零改动,修复需求挂 [decisions-pending.md](decisions-pending.md) #77 待人工拍板)→ 按任务口径,**本轮复测结论 = R1 结论,「未做修复,水位即当前水位」**,挂账不销。本节 = 同命令同参数复跑 + R1→R3 前后对比落档。

### 8.1 同口径前提核对(先证明确实在同一条基线上比)

- **命令与参数**:与 §2 / §3a / §3b 逐字相同的三条命令,一个参数未动(见各小节引用)。
- **载体源码**:`benches/longdoc.rs` 与 `examples/scrollbench.rs` 自 R1 零改动——`git diff --name-only 618b3ab..HEAD` 仅 `docs/perf-recheck-2026-10.md`(R1 文档提交 `31828b8`);R2 模块 `crates/` 零改动(§7)。R1→R3 之间**没有任何产品代码变更**,这是「水位可比」的前提。
- **运行环境**:同机同会话(X11,DISPLAY=:0);Mesa 25.0.7-2 / LLVM 19.1.4 与 R1 相同;`target/release/deps/longdoc-9a3abfe4edeb0714` 与 `target/release/examples/scrollbench` 为 R1 同一二进制(增量构建 `Finished in 0.15s` 未重编)。
- **criterion change 基线**:criterion 的 `change:` 行是与**上一轮存档估算**的自动对比;R1(2026-10-02)之后再无人跑过这两条 bench(target/criterion 存档未刷新),故本轮 `change:` 即 **R3 vs R1** 的统计学检验。

### 8.2 流式追加复跑(命令 = §2 同款)

```bash
cargo bench -p latermd-app --bench longdoc -- streaming_append
```

逐字输出(2026-10-02 R3,全量,含 criterion change 行):

```
    Finished `bench` profile [optimized] target(s) in 0.15s
     Running benches/longdoc.rs (target/release/deps/longdoc-9a3abfe4edeb0714)
Gnuplot not found, using plotters backend
long doc: 100251 chars, 3704 lines
Benchmarking streaming_append_by_lines/append_1_line_at_500
Benchmarking streaming_append_by_lines/append_1_line_at_500: Warming up for 3.0000 s
Benchmarking streaming_append_by_lines/append_1_line_at_500: Collecting 10 samples in estimated 5.6132 s (165 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_500: Analyzing
streaming_append_by_lines/append_1_line_at_500
                        time:   [33.575 ms 33.853 ms 34.163 ms]
                        change: [-1.0122% +0.1254% +1.2037%] (p = 0.84 > 0.05)
                        No change in performance detected.
Benchmarking streaming_append_by_lines/append_1_line_at_2000
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 8.0s or enable flat sampling.
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Collecting 10 samples in estimated 8.0238 s (55 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Analyzing
streaming_append_by_lines/append_1_line_at_2000
                        time:   [144.61 ms 146.00 ms 147.94 ms]
                        change: [-0.6276% +0.8818% +2.4399%] (p = 0.29 > 0.05)
                        No change in performance detected.
Benchmarking streaming_append_by_lines/append_1_line_at_10000
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 9.9s.
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Collecting 10 samples in estimated 9.8525 s (10 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Analyzing
streaming_append_by_lines/append_1_line_at_10000
                        time:   [980.55 ms 993.94 ms 1.0079 s]
                        change: [-2.7368% -0.7791% +1.1116%] (p = 0.48 > 0.05)
                        No change in performance detected.
```

前后对比表(R1 = §2,换算口径同 §0:µs/行 = 中值 ÷ 行数):

| 档位 | R1 中值 | R3 中值 | R3 µs/行 | R3 vs R1 | criterion 检验 |
|---|---|---|---|---|---|
| 500 行 | 33.800 ms | 33.853 ms | 67.71 µs | +0.16% | No change(p=0.84) |
| 2,000 行 | 146.46 ms | 146.00 ms | 73.00 µs | −0.31% | No change(p=0.29) |
| 10,000 行 | 1,001.7 ms | 993.94 ms | 99.39 µs | −0.78% | No change(p=0.48) |

R3 增长曲线(口径 = §0):规模 ×4 成本 ×**4.31**(33.853→146.00 ms),规模 ×5 成本 ×**6.81**(146.00→993.94 ms),每行成本 67.71 → 73.00 → 99.39 µs 仍随规模上升。**判定与 R1 相同:线性偏超线性,O(n) 未解**——R2 未修,曲线自然原样。

### 8.3 headless 滚动复跑(命令 = §3a 同款)

```bash
cargo bench -p latermd-app --bench longdoc -- long_doc_100k
```

逐字输出(2026-10-02 R3,全量):

```
    Finished `bench` profile [optimized] target(s) in 0.27s
     Running benches/longdoc.rs (target/release/deps/longdoc-9a3abfe4edeb0714)
Gnuplot not found, using plotters backend
long doc: 100251 chars, 3704 lines
Benchmarking long_doc_100k/cold_first_frame
Benchmarking long_doc_100k/cold_first_frame: Warming up for 3.0000 s

Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 6.8s or enable flat sampling.
Benchmarking long_doc_100k/cold_first_frame: Collecting 10 samples in estimated 6.8117 s (55 iterations)
Benchmarking long_doc_100k/cold_first_frame: Analyzing
long_doc_100k/cold_first_frame
                        time:   [120.25 ms 121.84 ms 124.86 ms]
                        change: [-1.8532% +0.6817% +3.1808%] (p = 0.62 > 0.05)
                        No change in performance detected.
Benchmarking long_doc_100k/steady_state_top
Benchmarking long_doc_100k/steady_state_top: Warming up for 3.0000 s
Benchmarking long_doc_100k/steady_state_top: Collecting 10 samples in estimated 5.0167 s (14k iterations)
Benchmarking long_doc_100k/steady_state_top: Analyzing
long_doc_100k/steady_state_top
                        time:   [357.76 µs 360.55 µs 364.09 µs]
                        change: [-1.1553% -0.0752% +1.0505%] (p = 0.90 > 0.05)
                        No change in performance detected.
Benchmarking long_doc_100k/steady_state_scroll_middle
Benchmarking long_doc_100k/steady_state_scroll_middle: Warming up for 3.0000 s
Benchmarking long_doc_100k/steady_state_scroll_middle: Collecting 10 samples in estimated 5.0159 s (13k iterations)
Benchmarking long_doc_100k/steady_state_scroll_middle: Analyzing
long_doc_100k/steady_state_scroll_middle
                        time:   [368.16 µs 373.51 µs 380.19 µs]
                        change: [-1.3971% +0.7185% +3.0133%] (p = 0.56 > 0.05)
                        No change in performance detected.
Found 3 outliers among 10 measurements (30.00%)
  2 (20.00%) low mild
  1 (10.00%) high mild
```

前后对比表(M0 与 R1 数字取自 §5):

| 场景 | M0 中值 | R1 中值 | R3 中值 | R3 vs M0 | R3 vs R1 | criterion 检验 |
|---|---|---|---|---|---|---|
| `cold_first_frame` | 136.3 ms | 123.49 ms | 121.84 ms | **变好(−10.6%)** | −1.3% | No change(p=0.62) |
| `steady_state_top` | 415.3 µs | 358.83 µs | 360.55 µs | **变好(−13.2%)** | +0.5% | No change(p=0.90) |
| `steady_state_scroll_middle` | 430.2 µs | 371.36 µs | 373.51 µs | **变好(−13.2%)** | +0.6% | No change(p=0.56) |

顶部与中部差 12.96 µs(≈3.5%),视口剔除维持;距 18.18 ms(55 fps)预算余量 **48.7×**。**三项维持「优于 M0」判定,水位与 R1 持平。**

### 8.4 真窗口 scrollbench 复跑(命令 = §3b 同款)—— **blocked_external 状态未变**

```bash
target/release/examples/scrollbench     # 20 秒自动退出并打印统计
```

逐字输出(2026-10-02 R3,单次复跑):

```
scrollbench: 文档 100030 字符 / 4384 行
scrollbench: 没有采到帧
```

与 §3b 六次运行结果逐字相同(20 秒内 ui 帧数仍 <121)。环境旁证复查:kwin 合成仍 active(`dbus-send --dest=org.kde.KWin /Compositor org.freedesktop.DBus.Properties.Get string:org.kde.kwin.Compositing string:active` → `boolean true`;注:R1 用的方法调用形式本轮报 `No such method`,KWin 现仅暴露同值 property,读取结论一致)。§3b 的诊断链与结论(会话级 Vulkan WSI Fifo present 阻塞,非应用代码)原样沿用,**该项维持 blocked_external,不因本轮复跑解除**。

### 8.5 R3 结论汇总

| 项 | M0 | R1 | R3(本轮) | 判定 |
|---|---|---|---|---|
| 流式 500 行 | 38.7 ms(77.4 µs/行) | 33.800 ms | 33.853 ms(67.71 µs/行) | 与 R1 持平(No change) |
| 流式 2000 行 | 154.7 ms(77.35 µs/行) | 146.46 ms | 146.00 ms(73.00 µs/行) | 与 R1 持平(No change) |
| 流式 10000 行 | 789.3 ms(78.93 µs/行) | 1001.7 ms | 993.94 ms(99.39 µs/行) | 与 R1 持平(No change);对 M0 仍 +25.9% 变差 |
| 流式增长曲线 | 线性 O(n) | 线性偏超线性 | 线性偏超线性(×4.31 / ×6.81) | **O(n) 未解(与 R1 同判)** |
| 滚动·冷首帧 | 136.3 ms | 123.49 ms | 121.84 ms | 优于 M0,与 R1 持平 |
| 滚动·稳态顶部 | 415.3 µs | 358.83 µs | 360.55 µs | 优于 M0,与 R1 持平 |
| 滚动·稳态中部 | 430.2 µs | 371.36 µs | 373.51 µs | 优于 M0,与 R1 持平 |
| 滚动·真窗口 p50 | 60.3 fps(16.58 ms) | 不可测(blocked_external) | 不可测(**复跑同果,仍 blocked_external**) | 环境未恢复 |

10000 行档 R3 对 M0 换算:993.94 / 789.3 = +25.9%,与 R1 的 +26.9% 同量级(R1 用中值 1001.7;两轮各自中值对同一 M0 基准,差异在轮间噪声内)。

**一句话结论:R2 未做修复,复测即原水位——流式 O(n) 未解(曲线与绝对值均与 R1 无统计差异),滚动渲染路径维持优于 M0,真窗口 vsync 口径维持 blocked_external。** 修复验收命令与口径见 §6 第二条(拍板后复跑 §2 命令,判据 = 每行成本随规模趋稳);环境恢复后跑 §3b 命令与 m0-report §2.2 同表对照。

---

## 9. M3 复测与两轮修复(2026-10-04,#52 streaming-cache-blockkey M3·拍板①执行)—— **每行成本趋稳达成,10000 行档对 R1 −99.94%,M0 验证 4 凭本节数字销账**

> 前置:M1(`be05b52` 分段准入放宽)与 M2(`ccc2945` 块级缓存 key 换块内容 hash)已落在本分支。本节 = 同命令复测 → 发现新超线性 → 两轮修复(预算 2 轮,§公共约束)→ 终测判定与滚动路径回归归因。修复均属 vendor ①类(独立 `vendor:` commit 由编排收口,vendor/README 变更表 + vendored CHANGELOG 已登记)。
> **落库时序(2026-10-05 独立评审补记)**:销账落档 commit `3f19c2e` 只含 docs 五件——M3 代码在该时点系未提交工作区(6 文件 +401/−50),git 历史中 M3 不存在;纯 HEAD(3f19c2e)检出实跑 `cargo test -p egui_markdown --all-features` = **147** passed,§9.7 的 **152** passed 与本节全部终测数字对应叠加工作区 M3 后的代码。复现本节数字须以 M3 收口后的独立 `vendor:` commit 为检出基点(hash 收口后在 decisions-pending #77 销账注记与 vendor/README.md 变更表 M3 行回填);`3f19c2e` 的 `vendor:` 前缀系销账落档误用,勿据其认定 M3 已落库。M1/M2 hash 不受影响。
> **执行环境**:与 §1 相同(Deepin 25 / X11 / rustc 1.98.0);bench 二进制因 vendor 改动重编为 `target/release/deps/longdoc-b3c9b69313f2cdbc`(R1/R3 为 `longdoc-9a3abfe4edeb0714`,载体源码 `benches/longdoc.rs` 全程零改动,`git diff` 确认)。

### 9.1 首轮复测(仅 M1+M2,未修)—— 发现曲线**二次方**

命令 = §2 同款:

```bash
cargo bench -p latermd-app --bench longdoc -- streaming_append
```

逐字输出(2026-10-04,全量):

```
    Finished `bench` profile [optimized] target(s) in 0.15s
     Running benches/longdoc.rs (target/release/deps/longdoc-b3c9b69313f2cdbc)
Gnuplot not found, using plotters backend
long doc: 100251 chars, 3704 lines
Benchmarking streaming_append_by_lines/append_1_line_at_500
Benchmarking streaming_append_by_lines/append_1_line_at_500: Warming up for 3.0000 s
Benchmarking streaming_append_by_lines/append_1_line_at_500: Collecting 10 samples in estimated 5.0325 s (5775 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_500: Analyzing
streaming_append_by_lines/append_1_line_at_500
                        time:   [882.66 µs 894.34 µs 902.31 µs]
                        change: [-97.392% -97.363% -97.336%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking streaming_append_by_lines/append_1_line_at_2000
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Warming up for 3.0000 s
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Collecting 10 samples in estimated 5.0503 s (385 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Analyzing
streaming_append_by_lines/append_1_line_at_2000
                        time:   [12.670 ms 12.782 ms 12.923 ms]
                        change: [-91.350% -91.167% -90.961%] (p = 0.00 < 0.05)
                        Performance has improved.
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high mild
Benchmarking streaming_append_by_lines/append_1_line_at_10000
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Warming up for 3.0000 s
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Collecting 10 samples in estimated 6.3504 s (20 iterations)
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Analyzing
streaming_append_by_lines/append_1_line_at_10000
                        time:   [317.08 ms 320.46 ms 323.62 ms]
                        change: [-68.326% -67.758% -67.216%] (p = 0.00 < 0.05)
                        Performance has improved.
```

(`change:` 对照的是 criterion 上一轮存档 = R3 §8,故百分比是「M1+M2 对未修」的改善。)三档绝对值全部大幅变好(10000 行档 1001.7ms → 320.46ms,−68%),**但增长曲线恶化**:

| 档位 | 中值 | µs/行 | 规模增长 | 成本增长 |
|---|---|---|---|---|
| 500 行 | 894.34 µs | 1.789 µs | — | — |
| 2,000 行 | 12.782 ms | 6.391 µs | ×4 | **×14.28** |
| 10,000 行 | 320.46 ms | 32.05 µs | ×5 | **×25.07** |

成本 ≈ 规模的 1.93 次方——**二次方**。按 §0 口径未达「每行成本趋稳」,激活修复轮。

### 9.2 根因定位(一次性探针,照 R1 §4-C 手法,测完已删)

临时探针 `crates/latermd-app/examples/streaming_profile.rs`(分阶段计时 + vendor 内临时 eprintln 行级计时,**测完已删,工作区净**),关键逐字证据:

1. **追加帧不重排版、不重高亮**:bench 文档追加行落在闭合围栏**之后**,fence token 不变 → flush 缓存命中(sections=32013, rows=8003, 8000 行档 `layout_job` 命中仅 ~1.0ms/帧,`highlight_code` 零调用)。M1/M2 机制按设计工作。
2. **热点 = `record_section_anchors`**:`compute_section_anchors`(label.rs)对**每个 section**从头重扫全部 rows + `job.text[..byte_start].chars().count()` O(n) 前缀重算。syntect 每个 code line 出一个 section → 8000 行 fence = 32013 sections × 8003 rows,单帧逐字实测:

```
[probe] render_galley sections=32013 rows=8003 glyphs=262970
[probe] record_section_anchors 197.223478ms
[probe] record_text_blocks 3.989µs
[probe] run_ui total 198.182835ms | ui closure 198.156ms
```

   195-205ms/帧,占整帧(198-207ms)的 **98%**——这就是二次方项。它同时是 R1 §2「线性偏超线性」里被线性项掩盖的隐藏二次项(外推 R1 10000 行档约 +300ms,与 R1 的 ×6.84>×5 相符):M1/M2 把线性主项杀掉后它显形。
3. 修复轮 1 后剩余主导 = flush 命中帧的 `cached.layout.job.clone()`(整 job 深拷贝)+ `Fonts::layout_job` 整 job 哈希找 galley(8k 行 ~1.0ms/帧)。

### 9.3 修复轮 1:anchors 单遍扫描(label.rs `compute_section_anchors`)

sections 与 rows 都按文档序,byte→char 游标与 row 游标各自只前进,一遍扫完;空行 `max(1)` 语义、末段沉底、byte_start 越界钳制等边界逐项保持。**值完全不变**(等价改写)。复测逐字输出:

```
Benchmarking streaming_append_by_lines/append_1_line_at_500
Benchmarking streaming_append_by_lines/append_1_line_at_500: Collecting 10 samples in estimated 5.0059 s (44k iterations)
streaming_append_by_lines/append_1_line_at_500
                        time:   [115.03 µs 116.18 µs 117.29 µs]
                        change: [-87.024% -86.859% -86.700%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking streaming_append_by_lines/append_1_line_at_2000
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Collecting 10 samples in estimated 5.0195 s (11k iterations)
streaming_append_by_lines/append_1_line_at_2000
                        time:   [439.07 µs 443.34 µs 447.79 µs]
                        change: [-96.656% -96.588% -96.520%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking streaming_append_by_lines/append_1_line_at_10000
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Collecting 10 samples in estimated 5.0919 s (2200 iterations)
streaming_append_by_lines/append_1_line_at_10000
                        time:   [2.2843 ms 2.3220 ms 2.3791 ms]
                        change: [-99.282% -99.270% -99.257%] (p = 0.00 < 0.05)
                        Performance has improved.
```

二次方项消除:500→2000 ×3.82(×4 规模)、2000→10000 ×5.24(×5 规模)——回到「线性小系数」(0.232→0.222→0.232 µs/行,趋稳但增长不显著低于规模)。未达验收,进入修复轮 2(预算最后一轮)。

### 9.4 修复轮 2:flush 缓存记住 shaped galley(+ 三项 perf 收尾)

- `CachedFlushRange` 增 `CachedShapedGalley { max_width, break_anywhere, pixels_per_point, galley, anchors }`:命中帧直接用已 shape 的 galley(与相对 anchors),跳过整 job 深拷贝与整 job 哈希;key 失配(换宽/换缩放)回落常规 shaping 路径;`map_job` 存在时的尾段不参与复用(app 未用 map_job,上游语义保留)。
- 连带三项:块高缓存 key 的 style+handler 份额从每块一哈希提为每 range 一哈希;`code_block_admits_segmentation` 字节长度快速否决(行数 ≤ 字节数,短 fence 免逐行扫描);anchors 随 shaped galley 缓存免重算。
- **踩坑(已写注释)**:egui 0.36 `Context::pixels_per_point` 取 context **写锁**,在 `ui.data_mut` 闭包内调用会自死锁——本仓测试当场抓到(10s RwLock 超时),读 ppp 必须提到 data 锁外。

终测(命令同 §2,逐字):

```
Benchmarking streaming_append_by_lines/append_1_line_at_500
Benchmarking streaming_append_by_lines/append_1_line_at_500: Collecting 10 samples in estimated 5.0017 s (133k iterations)
streaming_append_by_lines/append_1_line_at_500
                        time:   [35.206 µs 35.886 µs 36.482 µs]
                        change: [-5.5497% -3.8234% -1.9321%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking streaming_append_by_lines/append_1_line_at_2000
Benchmarking streaming_append_by_lines/append_1_line_at_2000: Collecting 10 samples in estimated 5.0042 s (46k iterations)
streaming_append_by_lines/append_1_line_at_2000
                        time:   [104.88 µs 107.29 µs 109.56 µs]
                        change: [-6.3825% -4.5591% -2.5805%] (p = 0.00 < 0.05)
                        Performance has improved.
Benchmarking streaming_append_by_lines/append_1_line_at_10000
Benchmarking streaming_append_by_lines/append_1_line_at_10000: Collecting 10 samples in estimated 5.0246 s (9075 iterations)
streaming_append_by_lines/append_1_line_at_10000
                        time:   [546.50 µs 550.75 µs 555.65 µs]
                        change: [-5.5887% -4.1767% -2.6438%] (p = 0.00 < 0.05)
                        Performance has improved.
```

### 9.5 判定(口径 = §0 / 任务书「每行成本随规模趋稳(亚线性)」)

| 档位 | M3 终测中值 | µs/行 | R1 中值 | 对 R1 | M0 中值 | 对 M0 |
|---|---|---|---|---|---|---|
| 500 行 | 35.886 µs | **0.0718 µs** | 33.800 ms | **−99.89%** | 38.7 ms | −99.91% |
| 2,000 行 | 107.29 µs | **0.0536 µs** | 146.46 ms | **−99.93%** | 154.7 ms | −99.93% |
| 10,000 行 | 550.75 µs | **0.0551 µs** | 1001.7 ms | **−99.945%** | 789.3 ms | −99.93% |

- **每行成本随规模**:0.0718 → 0.0536 → 0.0551 µs——**下降后趋稳**(末两档差 +2.8%,在 §0 的 ±5% 持平带宽内;R1 是 67.6→73.2→100.2 随规模上升)。✅
- **成本增长 vs 规模增长**:500→2000 成本 ×2.99(规模 ×4,低 25%);2000→10000 成本 ×5.13(规模 ×5,+2.6% 与线性持平);总体 ×20 规模成本 ×15.34(低 23%)。如实记录:2000→10000 段未达「显著低于」,残余线性项 = 整篇 parse/哈希/anchors 记录,合计 ~0.05 µs/行——两轮修复预算已用于两个主导项(二次方 anchors 与 O(n) job 拷贝+哈希),不再扩面(增量解析属另一工程量级)。
- **M0 验证 4 的原始痛点**(m0-report §4.2「超 ~1,300 行单 chunk 成本超 100ms 吐字节奏」):现 10000 行单 chunk = 550.75 µs = 100ms 预算的 **0.55%**(余量 181×);三档全部深藏预算内。**验证 4 凭本节销账**(m0-report §4.4 已按新数字落档)。

### 9.6 滚动路径回归核对(long_doc_100k,与本步否决线)—— 回归属实但主因不在本步

修复后同命令复跑 `cargo bench -p latermd-app --bench longdoc -- long_doc_100k`,逐字:

```
long_doc_100k/cold_first_frame
                        time:   [122.53 ms 123.90 ms 125.44 ms]
                        change: [-1.7899% -0.7989% +0.1813%] (p = 0.15 > 0.05)
long_doc_100k/steady_state_top
                        time:   [522.86 µs 528.05 µs 532.83 µs]
                        change: [+28.190% +29.619% +31.083%] (p = 0.00 < 0.05)
                        Performance has regressed.
long_doc_100k/steady_state_scroll_middle
                        time:   [559.06 µs 571.32 µs 578.67 µs]
                        change: [+28.359% +32.214% +36.216%] (p = 0.00 < 0.05)
                        Performance has regressed.
```

冷首帧持平;稳态两档对 R1(358.83/371.36 µs)+47%/+54%。**A/B 定位**(临时 env 开关逐项关断,测完还原,工作区净)定出构成:

| 分量 | 稳态 top 影响 | 归属 |
|---|---|---|
| #42 块表记录(block_span_rects,main `85cdf93`) | **+136 µs/帧** | **main 既有,先于本分支**(R1 基线 2026-10-02 未含 #42;`git log 618b3ab..origin/main -- vendor/` 仅此一个 vendor commit) |
| M1 准入门逐行扫描 | +29 µs/帧 | 本分支 M1;**本轮已修**(字节长度快速否决,终测数字已含) |
| M2 块 key 内容哈希 | +36 µs/帧 | 本分支 M2 的固有代价(内容寻址 cull 的语义必需,换 streaming 正确性,接受) |
| 本步 shaped galley 复用 | **−14 µs/帧** | 本轮 M3(净改善) |
| 残余未归因 | ~30-40 µs | 轮间噪声/环境漂移量级(cold 帧持平佐证环境可比) |

即:**去掉 main 既有的 #42 分量后,观测差 +33µs(+9%),其中可解释的本分支净贡献 = M2(+36)− 本步复用(−14)≈ +22µs(+6%)(M1 的 +29 已在本轮修掉),其余落在轮间噪声/环境漂移带内**;且距 18.18ms(55fps)预算余量仍有 34×。像素零变化否决线维持(152 项 vendored 测试全绿,含 M2 的热缓存像素指纹测试与本轮新增等价/复用测试)。#42 块表的每帧记录成本已按五要素登记 decisions-pending 新条目,交其归属模块处理,不在本步范围。

### 9.7 测试与验证(全部实跑)

- vendored:`cargo test -p egui_markdown --all-features` **152 passed / 0 failed**(含新增 5 测:section_anchors.rs +2——等价 oracle(4 文档矩阵,测试内嵌旧算法对照)+ 520 行 admitted fence 每行有锚;cache.rs +3——重复帧 painted text+anchors 逐项相等 / 换宽后 anchors == 全新 ctx 同宽 / admitted fence 流式追加新行可见)。
- 变异验证 2 次:row 游标「匹配即前进」→ 等价断言 8≠15 失败;丢弃 shaped galley 宽度 key → 换宽测试失败;还原后全绿(断言非恒真)。
- 探针载体 `streaming_profile.rs` 测完已删;vendor 内临时插桩(env 门控 eprintln/计时器/关断开关)全部还原,`git diff vendor/` 仅剩本轮正式改动。
- 六项门禁(fmt/三轮 clippy/test/doc)与 `vendor/egui_markdown/check.sh` 在本模块收尾时实跑,结果见 auto-plan/README 修订记录;真机目视项见 notes(blocked_external)。
