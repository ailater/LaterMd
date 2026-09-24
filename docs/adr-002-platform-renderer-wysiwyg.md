# ADR-002: 平台基线、图形后端与 WYSIWYG 架构

日期: 2026-09-24
状态: 已接受
关联: [[adr-001-gui-and-architecture]]、[[adr-003-renderer-and-ecosystem-audit]]

---

## 1. 决策摘要

| 议题 | 结论 |
|---|---|
| GPU 后端 | **wgpu**（沿用 eframe 默认）；glow 仅作 env-var 逃生口，不进主产物 |
| 「显卡推荐」的真实含义 | **本项目不构成显卡约束**。瓶颈在 CPU 侧排版，不在 GPU 侧 |
| WYSIWYG 形态 | **B：Live Preview（Obsidian 式）**；True WYSIWYG 列入 non-goals |
| 平台最低版本 | Windows 11 / macOS 14；**显式不支持** Win10 / macOS 13 |

---

## 2. 平台基线：Windows 11 + macOS 14

选择这两个最低版本带来的实质收益（不是形式主义）：

### Windows 11

- 系统要求本身强制 **WDDM 2.0 + DirectX 12** 显卡。因此「老旧 Intel 集显不支持」在本项目里**不构成风险项** —— 能装 Win11 的机器一定能跑 wgpu 的 DX12 后端。
- 建议同时发布 `aarch64-pc-windows-msvc`（Snapdragon X Elite / Windows on ARM）。Rust 对该 target 是一级支持，边际成本低，覆盖面收益高。

### macOS 14 (Sonoma)

- Metal 3 家族齐全，wgpu Metal 后端在此版本稳定。
- 关键：**macOS 上 OpenGL 已被 Apple 废弃**（自 10.14 起 deprecated，不再更新驱动）。这是选择 wgpu 而非 glow 的硬理由。
- AccessKit macOS 适配（`accesskit` 0.25.0 / `accesskit_winit` 0.34.0）在此版本成熟，VoiceOver 可用。

### 明确不支持

Windows 10（含 LTSC）、macOS 13 及以下。这不是偷懒 —— 支持它们意味着要为 Metal 特性差异和 DX12 降级路径做双份测试。

---

## 3. 图形后端：为什么「显卡」不是本项目的问题

### 3.1 先澄清一个广泛误解

egui **不是一个 GPU 加速的 UI 框架**。它的渲染管线是：

```
   Rust 侧 CPU                          GPU 侧
   ─────────────                        ──────
   Layout / Widget 树
     ↓
   epaint 曲面细分 → 三角网格
     ↓
   字形栅格化 → 字体图集纹理
     ↓
   上传: 1 张图集 + 1 个顶点/索引缓冲  →   采样纹理 + 混合 ≈ 1 个 draw call
```

GPU 需要的能力：**采样一张纹理、画一堆带顶点色的三角形、scissor rect 裁剪**。仅此而已。

因此：

- 不需要 MSAA、不需要 compute shader、不需要 render target array、不需要 bindless
- 集显、核显、甚至 lavapipe 软件 Vulkan 都能跑满刷新率
- **「推荐什么显卡」这个问题在本项目里不成立**

### 3.2 结论：wgpu，保持 eframe 默认

版本实况（2026-09-24 查询）：

| crate | 版本 | 说明 |
|---|---|---|
| `wgpu` | 30.0.1 | 2026-08-22 |
| `naga` | 30.0.1 | wgpu 的 shader 编译器 |
| `egui-wgpu` | 0.36.2 | 2026-09-08，与 egui 同步发布 |
| `egui_glow` | 0.36.2 | 备选，但**不启用** |

**`eframe` 的 `default` feature 已经包含 `"wgpu"`，且不包含 `glow`。** 也就是说，什么都不做就是正确答案：

```toml
# crates/latermd-app/Cargo.toml
#
# 不要启用 glow。eframe 默认已经是 wgpu。
# 同时开启两条渲染路径只会让产物体积变大、编译时间翻倍，
# 却换不来 macOS 上的任何收益（Apple 已 deprecated OpenGL）。
eframe = "0.36"
```

### 3.3 wgpu vs glow 的实质差异

| | wgpu | glow (OpenGL) |
|---|---|---|
| Windows | DX12 | GL 3.3 core（驱动质量参差） |
| macOS | **Metal**（原生，唯一持续受支持的路径） | OpenGL（已废弃，Bug 不会修） |
| Linux | Vulkan | GL 3.3 / GLES |
| 未来的 shader 扩展 | naga IR，可控 | GLSL 字符串，易出错 |

macOS 一行就决定了胜负。

### 3.4 唯一需要防御的 GPU 情况：驱动黑名单

GPU **能力**不是风险，GPU **驱动缺陷**是。wgpu 内置 GPU 黑名单（特别是某些 Intel Windows 驱动和 Android Mali），会在 `request_adapter` 时拒绝或崩溃。

因此不是「软件回退」，而是**渲染器运行时降级**：

```rust
// crates/latermd-app/src/main.rs
fn main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        // 默认走 wgpu；如果用户报告黑屏/崩溃，
        // 允许通过 env var 切到第二条路径，而不是让用户卡在白屏里。
        renderer: match std::env::var("LATERMD_RENDERER").as_deref() {
            Ok("glow") => eframe::Renderer::Glow,
            _ => eframe::Renderer::Wgpu,
        },
        ..Default::default()
    };
    eframe::run_native("LaterMD", opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
```

同时 Settings 里放一个可见的「渲染后端」开关 + 「当前后端」显示。日志中记录 adapter name / backend / driver info，用户报 issue 时一眼可查。

> 注意：要在 `Cargo.toml` 里同时开 `wgpu` 和 `glow` 两个 feature 才能用 `Renderer::Glow`。这是一个**有意的例外** —— 默认不启用，CI 上用 `--features glow` 编译一条额外产线验证它能编过，主产物仍然只带 wgpu。

### 3.5 真正的性能瓶颈在 CPU

长文档（10 万字）的帧时间构成：

```
  字体图集栅格化   ████
  epaint 曲面细分  ████████
  Widget 布局遍历  ████
  GPU 提交         ▌              ← 可忽略
```

**但解法已经内建了。** 详见 ADR-003 §5：vendored egui_markdown 已实现视口剔除（`ui.is_rect_visible()` + `ui.allocate_space()`），把每帧工作量从 O(document) 降到 O(visible area)。**不需要我们自己做虚拟化。**

---

## 4. WYSIWYG：需要先纠正术语，再谈方案

### 4.1 「WYSIWYG」至少有三种，成本差一个数量级

| 形态 | 代表产品 | 数据真相 | 纯 Rust/egui 成本 |
|---|---|---|---|
| **A. 双栏源码 + 实时预览** | VS Code Markdown Preview | Markdown 源码 | 低（2-4 周） |
| **B. Live Preview 混合模式** | **Obsidian、Typora** | **Markdown 源码** | 中（6-10 周） |
| **C. True WYSIWYG** | Word、Notion | 内部文档树，md 只是导出格式 | 高（6-12 个月，单人） |

用户提出的「所见即所得」，多数情况下实际想要的是 B。

### 4.2 推荐 B（Live Preview），理由不是成本

即便成本不是问题，B 也更适合本项目：

1. **`.md` 文件永远干净。** C 形态的通病：「编辑器重新格式化了我的文件」「表格对齐被改了」「我手动调的换行没了」。对写技术文档 + AI 辅助的人不可接受。
2. **AI 增强需要源码真相。** AI 操作的必须是 Markdown token 而不是渲染后的盒子 —— 否则 LLM 吐回一段富格式，要反向构造 md 是非平凡问题。B 形态下这条链路天然通顺。
3. **Git 友好。** 版本 diff 可读。C 形态的文档序列化通常 diff 噪音极大。

### 4.3 为什么纯 Rust / egui 里没有现成方案

翻遍 crates.io，**没有**。搜到的相关 crate 全部不满足：

| crate | 问题 |
|---|---|
| `kode-core` / `kode-doc` / `kode-markdown` 0.5.5 | 3 stars，绑 Leptos/WASM，桌面端不可用 |
| `leaf-core` / `leaf-raster` 0.4.4 | 2 stars，依托 Twig，未验证 |
| `taino-edit*` | 绑 Leptos / Dioxus / contenteditable DOM bridge |
| `chaqaq` 0.1.0 | 38 下载 |
| `proseframe` 0.1.0 | 12 下载 |
| `gpui-flowtext` 0.1.0 | 62 下载，且是 GPUI（ADR-001 §2.3 已否决） |

根因：**egui 提供的是 layout（`LayoutJob` / `Galley`）不是 editing**。`egui::TextEdit` 是单一格式的纯文本编辑器 —— 内部就是一层 Galley + 光标索引。富文本光标/选区没有任何现成地基。

### 4.4 Live Preview 算法与现有结构的契合

Obsidian 的 Live Preview 规则只有一条：

> 光标所在的那个 block，显示 Markdown 源码；其余所有 block，显示富渲染结果。

好消息：**vendored egui_markdown 已经有 `segment_breaks` 机制**，标记了每个需独立渲染的 block 的 token 边界（详见 ADR-003 §5）。因此不需要从零写 block 分派器，而是在已有的 `render_token_range` 循环里加一个分支：

```rust
// 伪代码：在 render_token_range 循环内，处理到一个 segment 边界时
let contains_caret = token_range_contains(token_idx..next_idx, doc.caret_range);

if contains_caret {
    // 「聚焦」模式：渲染这块原始 Markdown 源码（可编辑）
    render_source_block(ui, &doc.source[span]);
} else {
    // 「阅读」模式：走 egui_markdown 原本的富渲染
    render_rich_block(ui, &tokens[token_idx..next_idx]);
}
```

**修正后的估计：3-5 天**（而非最初估计的 1-2 周）。前提是先完成下面三块补工。

### 4.5 需要补的三块工程

1. **`Token` 要携带源码 span。**
   已核实 `types.rs` 中的 `Token` enum **没有任何 span / byte offset 字段**：

   ```rust
   pub enum Token<'s> {
       Newline,
       Text { text: CowStr<'s>, style: TokenStyle },
       CodeBlock { text: CowStr<'s>, language: Option<CowStr<'s>> },
       Link { text: CowStr<'s>, href: CowStr<'s>, title: Option<CowStr<'s>> },
       // ...
   }
   ```

   需要从 pulldown-cmark 的 event offset 透传到每个 token。**这是 prerequisite，应在 vendor 化时就做掉。**

2. **块级 caret 路由。** 行内的 ↑/↓ 跨 block 跳转、Home/End、块内选区扩展。这块没有捷径，约 3-4 周。

3. **内联语法的「半隐藏」处理。** `**粗体**` 在离焦时藏掉标记，聚焦时显示出来。这是 Live Preview 的灵魂，v1 可简化为「离焦时完全富渲染，聚焦时整条源码裸出来」。

### 4.6 一个反直觉但重要的推论

> **不要把源码模式和 Live Preview 做成两个编辑器。**

做成 **一个编辑器 + 一个 `render_mode: { Source | LivePreview }` 标志**。两者共用：

- 同一个 rope buffer（数据真相）
- 同一套 token 划分
- 同一批渲染组件

区别仅仅是「是否应用上面那条规则」。这样源码模式的调试经验直接复用，且 Cmd+/ 一键切换不丢光标、不丢 undo 栈。

### 4.7 关于 True WYSIWYG（形态 C）

**列入 non-goals，写进 README。**

- 成本是 B 的 5-10 倍，需自建 document model + transaction/undo
- 目标用户（技术写作者 + demo 演示者）实际上偏好看到 Markdown 标记
- 一旦支持 C，`.md` 不再是真相源，与 AI 增强链路冲突（§4.2）

未来若真要支持，路径是：**先做 B，稳定后加「导出/另存为富文本格式」，而不是在编辑器里做 C。**

---

## 5. 跨平台专项

| 类别 | Windows 11 | macOS 14 | Linux |
|---|---|---|---|
| 窗口后端 | Win32 | Cocoa | X11 / Wayland |
| 渲染后端 | DX12 / Vulkan | Metal | Vulkan |
| 快捷键 | `Ctrl` | `Cmd` | `Ctrl` |
| 换行符 | CRLF | LF | LF |
| 字体 | 需加载中文字体 | 系统字体较好 | 需确保中文字体可用 |
| 输入法 | IME 支持 | IME 支持 | IME 支持 |
| 文件监听 | ReadDirectoryChangesW | FSEvents | inotify |
| Git 凭据 | Credential Manager | Keychain | libsecret / gnome-keyring |

### Linux 开发机注意事项

本机环境为 Deepin + rolling kernel。若 Wayland 下 `egui_wgpu` 起不来（Vulkan/合成器兼容问题），**建议开发时显式走 X11 会话**，不要在这上面耗时间 —— 目标用户的主要平台是 Win/macOS。

---

## 6. M0 技术验证（两周）

三条验证，全部通过才继续：

1. **IME**：Windows 11 微软拼音 + macOS 14 简体拼音，验证候选框位置跟随 caret、词组不吞字、候选窗口不抢焦点。
   > 这是纯 Rust GUI 的死亡陷阱，必须在投入前验掉。
2. **长文档性能**：10 万字 md，滚动到文档中部帧率 ≥ 55fps（验证 §3.5 的视口剔除确实生效）。
3. **wgpu 目标覆盖**：Win11(DX12) / macOS14(Metal) / Linux(Vulkan) 三个 target 均能启动，且 `egui_wgpu` 报告合理 adapter。

---

## 7. 检查清单

- [ ] 根级加 `rust-toolchain.toml` 钉 1.98.0（egui 0.36.2 要求 rustc ≥ 1.95，默认 toolchain 1.94.0 会编译失败，已实测）
- [ ] README 写明不支持 Win10 / macOS 13
- [ ] Settings 面板暴露渲染后端信息与切换
- [ ] CI 加 `aarch64-pc-windows-msvc` 发布目标
- [ ] vendor 时同步给每个 `Token` 补源码 span
