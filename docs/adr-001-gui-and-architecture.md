# ADR-001: GUI 框架选型与 workspace 分层

日期: 2026-09-24
状态: 已接受
关联: [[adr-002-platform-renderer-wysiwyg]]、[[adr-003-renderer-and-ecosystem-audit]]

---

## 1. 决策摘要

| 议题 | 结论 |
|---|---|
| GUI 框架 | **egui + eframe 0.36.2** |
| 备选（保留观察） | iced 0.14.0 |
| 否决 | Tauri 2、GPUI、Yew/Dioxus（后者非桌面原生） |
| Markdown 解析 | pulldown-cmark（见 ADR-003 §4） |
| 渲染层 | vendored egui_markdown（见 ADR-003 §2） |

---

## 2. GUI 框架选型

### 2.1 实测生态数据（2026-09-24 查询）

| 框架 | 最新版本 | 发布日 | 累计下载 | 备注 |
|---|---|---|---|---|
| **egui** | **0.36.2** | 2026-09-08 | — | 子 crate `egui-wgpu`/`egui_glow`/`eframe` 同日同步发布 |
| `eframe` | 0.36.2 | 2026-09-08 | 18,827,607 | — |
| `egui-wgpu` | 0.36.2 | 2026-09-08 | 12,279,434 | — |
| `egui_glow` | 0.36.2 | 2026-09-08 | 19,094,095 | — |
| `iced` | 0.14.0 | **2025-12-07** | — | 距查询日已 9 个月未发版 |
| `slint` | 1.18.1 | 2026-09-21 | — | 商业许可需注意 |
| `dioxus` | 0.7.10 | 2026-07-31 | — | 前端心智模型 |

### 2.2 选择 egui 的理由

1. **发布节奏同步。** `egui` / `egui-wgpu` / `egui_glow` / `eframe` / `egui_extras` 五个 crate 在同一天（2026-09-08）发布 0.36.2。这说明是单一 monorepo 统一发版，**不存在子 crate 版本错配**。
2. **生态覆盖面。** `egui_extras` 提供表格/图片/SVG/syntect 集成；`egui_dock` 0.21.1 提供多标签停靠（编辑器需要）；`egui-notify` 0.23.0 提供 toast。
3. **立即模式适合文档应用。** 预览面板需要根据文档内容动态生成 UI 树 —— 这正是 IMGUI 的优势场景，保留模式的 framework 反而要手动做 diff。
4. **`rfd` / `notify` / `git2` 等纯 Rust 依赖与 egui 无冲突。**

### 2.3 否决 GPUI 的理由（重要）

crates.io 上的 `gpui` 最新版本是 **0.2.2，发布于 2025-10-22** —— 距查询日近一年未更新。

根因：**Zed 是把 GPUI vendor 在自家仓库内开发的**，crates.io 上的 `gpui` 是社区镜像，不是 Zed 实际使用的那个。选择 GPUI 意味着：

- 必须 `git` 依赖 `zed-industries/zed` 仓库并锁定某个 commit
- 每季度跟进一次上游的破坏性重构

对小团队而言，这种 churn 不可承受。**结论：否决。**

### 2.4 否决 Tauri 的理由

用户已选定纯 Rust 路线。补充客观数据以支持该选择：

- Tauri 2.11.6（2026-09-21）是当前版本
- **Linux 需要 WebKitGTK 4.1**：Ubuntu 24.04 / Debian 13 已从仓库移除 `libwebkit2gtk-4.0`
- Linux 构建依赖：`libwebkit2gtk-4.1-dev`、`libxdo-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`

即使用户未选纯 Rust，Tauri 在 Linux 上的分发也是长期痛点。

### 2.5 iced 作为备选

`iced 0.14.0` 的 Elm 架构对大型文档状态管理更干净，但：

- async-first 的心智负担较重
- 表格、嵌套滚动容器的控件成熟度不如 egui
- 9 个月未发版

**结论：保留观察，不作为一期方案。**

---

## 3. Workspace 分层

```
LaterMD/
├── Cargo.toml                     # workspace root
├── rust-toolchain.toml            # 钉 channel = "1.98.0"
├── vendor/
│   └── egui_markdown/             # vendored + 升到 egui 0.36.2
│       ├── src/                   # 主 crate
│       ├── egui_markdown_style/   # 子 crate：MarkdownStyle + serde
│       ├── tests/                 # cache/indent/truncate/width
│       └── check.sh               # 照抄其质量门禁到 CI
├── crates/
│   ├── latermd-core/              # 无 UI 依赖：应用状态机、DTO
│   ├── latermd-md/                # pulldown-cmark 封装 + token 层 + 源码 span
│   ├── latermd-render/            # token → 绘制指令（不 import egui 的部分）
│   ├── latermd-editor/            # ropey 文本缓冲 + caret/选区 + IME
│   ├── latermd-git/               # git2 封装（P2）
│   ├── latermd-ai/                # provider trait、流式、tool calling（P1）
│   ├── latermd-export/            # PDF/HTML/DOCX 导出
│   └── latermd-app/               # eframe binary，唯一的 GUI crate
├── docs/
└── .github/workflows/             # 三平台 CI 矩阵
```

### 3.1 分层铁律

**`latermd-render` 不允许 import `eframe` / `egui`，只产出绘制指令结构。**

这样做的收益：
- Phase 2 若切换到 Blitz/Stylo HTML 后端，不必重写上层
- 将来做 headless CLI 导出器（无 GUI）可直接复用

例外：rendering 的实际实现不可避免地要和 egui 打交道（因为我们 vendor 的就是 egui widget），因此实际边界是 **`latermd-render` 定义指令结构，`latermd-app` 负责把指令翻译成 egui 调用**。

### 3.2 目录命名说明

`LaterMD` 项目名保留；crate 前缀统一 `latermd-`，避免与 Rust 生态已有 crate 冲突。

---

## 4. 为什么不引入 `core` / `ui` 二分

用户初版方案里的结构是 `apps/desktop` + `core` + `ui` + `packages/markdown-editor`。这里做了调整，理由：

- **`ui` 层无法独立。** egui 是立即模式，UI 代码天然与状态机耦合。强行拆出 `ui` 会产生大量跨 crate 的 `&mut Ui` 传递。
- **`packages/markdown-editor` 是过早抽象。** 在第一个可用版本之前，不存在第二个消费者。等到确实要做 Web/WASM 版时再拆。

保留的核心原则不变：**业务逻辑不依赖 UI 框架**（落在 `latermd-core` / `latermd-md` / `latermd-git` / `latermd-ai`）。

---

## 5. 里程碑

| 阶段 | 目标 | 周期 | 验收 |
|---|---|---|---|
| **Vendor 适配** | egui_markdown 升到 egui 0.36.2 | **3-5 工作日** | `check.sh` 全绿（详见 vendor-upgrade-checklist） |
| **M0 技术验证** | 三条验证，不通过不继续 | 2 周 | 见 ADR-002 §6 |
| **P0 骨架** | 编辑-预览-导出-打包 | ~~6-8 周~~ → **8-10 周** | 三平台可安装、可写 1 小时文档不崩 |
| **P1 差异化** | AI 流式写作 + 搜索 | +7.5-9.5 周 | 见 roadmap |
| **P2 版本层** | Git 只读集成 | +4.5-6.5 周 | 见 roadmap |
| **P3 深水区** | Live Preview + 双链 | +8-10 周 | 见 roadmap |

> **2026-09-24 修订（ADR-005）**：P0 并入三栏布局、文件树基础版与廉价大纲，6-8 周 → 8-10 周；P1 并入侧边栏搜索。本表周期数字**不再回写**，最新值以 [roadmap.md](roadmap.md) 周期汇总为准。

---

## 6. 待办检查清单

- [ ] 创建 workspace 骨架 + `rust-toolchain.toml`
- [ ] `git clone membrane-io/egui_markdown` → `vendor/egui_markdown/`
- [ ] 完成 0.34 → 0.36.2 升级（[vendor-upgrade-checklist.md](vendor-upgrade-checklist.md)）
- [ ] 把 `check.sh` 接入 CI
- [ ] `vendor/` 目录保留 MIT OR Apache-2.0 license header
