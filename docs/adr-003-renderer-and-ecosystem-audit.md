# ADR-003: 渲染层与 Rust 生态审计

日期: 2026-09-24
状态: 已接受
关联: [[adr-001-gui-and-architecture]]、[[adr-002-platform-renderer-wysiwyg]]

> 本 ADR 记录两个**推翻先前判断**的发现，以及全部实测数据。

---

## 1. 决策摘要

| 议题 | 结论 |
|---|---|
| 渲染层 | **vendored egui_markdown (membrane-io)**，升级到 egui 0.36.2 |
| 推翻的判断一 | 「egui_markdown 已死」—— **错**。仓库迁至 membrane-io 后持续活跃 |
| 推翻的判断二 | 「单 Galley 无法虚拟化」—— **错**。已实现视口剔除，工作量 O(visible) |
| Markdown 解析 | **pulldown-cmark**（从 comrak 改过来） |
| 仍成立的判断 | `Token` 无源码 span，Live Preview 需自己补 |

---

## 2. 推翻判断一：egui_markdown 活着，且质量高

### 2.1 先前的错误判断及其根因

初次调研时依据三点判该项目已死：

- crates.io 版本停留在 0.1.0
- GitHub 仓库 `iamseeley/egui_markdown` 返回 **301 重定向**
- 发布时间 2026-03-23，半年未更新

**根因：没有跟进 301 的目标地址。** 跟进后得知仓库迁到了 **`membrane-io/egui_markdown`**（作者 Thomas Seeley 转入公司组织）。

### 2.2 真实活跃度

```
2026-09-11  Thomas Seeley  Gate the Stroke import on the membrane feature
2026-09-11  Thomas Seeley  Merge pull request #10 from thomas/fix-ui-gaze
2026-09-11  Thomas Seeley  Fill a code block with the code background colour
2026-09-09  Thomas Seeley  Add hug_content so MarkdownLabel can size to its galley
2026-09-08  Thomas Seeley  Add a map_job hook on MarkdownLabel
```

距查询日 **13 天前仍在提交**，有 PR 流程（`Merge pull request #10`）。

### 2.3 代码质量实测

下载 0.1.0 release 源码实际阅读：

| 项 | 实测 |
|---|---|
| 规模 | **4001 行**（crates.io 发布版） |
| 文件分布 | parser.rs 1454 / label.rs 1036 / layout.rs 413 / style.rs 364 / table.rs 270 |
| 安全 | `#![forbid(unsafe_code)]` |
| 文档 | `#![warn(missing_docs)]` |
| 许可 | **MIT OR Apache-2.0** 双许可（比 MIT-only 更适合商业分发） |
| 测试 | `tests/` 下 `cache.rs`(6370B) / `indent.rs`(6908B) / `truncate.rs` / `width.rs` |
| 基准 | criterion bench（`benches/markdown.rs`） |
| 解析器测试 | parser.rs 内含成体系 `#[test]` |

**质量门禁（check.sh）**：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo doc --no-deps --all-features
```

> **行动项：vendor 后照抄这套门禁到 CI。**

### 2.4 对比 tektite（先前推荐的方案）

| | **egui_markdown** | tektite |
|---|---|---|
| 累计下载 | 171（0.1.0 单版本） | **146** |
| GitHub stars | 3（membrane-io） | **0** |
| 维护模式 | 公司组织 + PR 流程 | **单人业余** |
| 代码规模 | 4001 行（发布版） | ~36KB / 约 900 行 |
| 质量门禁 | check.sh 全六项 | clippy lint 配置 |
| 集成测试 | 4 个文件 | 无 |
| 许可 | MIT OR Apache-2.0 | MIT |
| 特有优势 | **`heal()` + `LinkHandler` + `MarkdownStyle`** | 组件拆分粒度 |

**结论：改用 egui_markdown 替代 tektite 作为渲染基座。**

tektite 的保留价值：**仅作为设计参考**（其 `render_document` / `render_sections` 递归分派结构值得借鉴），不作为依赖。

---

## 3. `heal()` 的真实语义

### 3.1 它不是「流式渲染开关」

它是**字符串预处理函数**，作用是给残缺的 Markdown 补上闭合标记：

```rust
pub fn heal(s: &str) -> Cow<'_, str>
```

实测行为（`src/parser.rs:43`）：

```rust
assert_eq!(heal("```rust\nlet x = 1;"), "```rust\nlet x = 1;\n```");
assert_eq!(heal("**bold text"), "**bold text**");
assert_eq!(heal("[link text"),  "[link text]()");
assert_eq!(heal("_italic"),     "_italic_");
```

### 3.2 为什么它对本项目的 AI 功能是必需品

LLM 逐 token 输出 Markdown 时，第 N 个 token 处文档必然处于语法不完整态 —— 代码块没闭合、粗体只写左半边。不 heal 的话，预览会闪现裸露的 `**` 和半个代码块。

**`heal()` 让每一帧都是语法合法的 Markdown。**

实现细节体现了真实的工程经验：

- `in_fence` 检测闭合 code fence
- `heal_table()` 处理表格缺分隔行
- `heal_inline()` 处理行内标记
- 源码注释明写：**表格场景要跳过行内扫描**，因为行内扫描器不理解表格结构会产生冲突的闭合。这是踩过坑的痕迹。

返回 `Cow::Borrowed` 时零分配（完整 Markdown 的常见情况），只有残缺输入才产生 `Cow::Owned`。

> **行动项：P1 阶段需扩展 `heal()` 以支持数学公式 `$$` 与自定义 AI 指令块 fence。**

---

## 4. Markdown 解析器：从 comrak 改为 pulldown-cmark

### 4.1 决定性理由：避免两套方言

vendored egui_markdown 传递依赖 **pulldown-cmark 0.13.0**，且在 `parser.rs:349-353` 已开启 GFM 全套：

```rust
options.insert(Options::ENABLE_STRIKETHROUGH);
options.insert(Options::ENABLE_TABLES);
options.insert(Options::ENABLE_FOOTNOTES);
options.insert(Options::ENABLE_TASKLISTS);
```

若坚持 comrak，二进制里会同时存在**两个 Markdown 解析器 = 两套方言**。预览效果与导出效果不一致是最难查的一类 bug。

### 4.2 支持数据

| crate | 版本 | 累计下载 | 说明 |
|---|---|---|---|
| **pulldown-cmark** | 0.13.4 | **158,716,973** | 拉解析器，事件驱动，无 AST。被 `cargo doc` 使用 |
| comrak | 0.55.0 | 8,056,090 | 有 AST，扩展性强 |
| `pulldown-cmark-to-cmark` | 22.0.1 | 72,334,240 | **AST → Markdown 序列化** |

第三条是关键：**序列化回 Markdown 是 Live Preview 与「AI 编辑后再回写」的刚需。**

### 4.3 权衡与可逆性

代价：comrak 的插件/自定义节点能力稍强。但当前没有自定义节点需求。

**此决策可逆。** 若未来确实需要 comrak 的 extension 机制，需同时评估替换渲染层的成本。

---

## 5. 推翻判断二：它不是「单 Galley」，视口剔除已内建

### 5.1 设计文档原文

`ECOSYSTEM.md`：

> `egui_markdown` renders markdown into one egui galley... **It draws the block elements as separate widgets**, and those elements are tables, code blocks, blockquotes, and images.

`DESIGN.md` 的 **Segmented rendering** 章节：

> Layout identifies **segment breaks**, which are the token indices where the widget must flush the text galley and render a block widget. The render path then **alternates between text ranges, which it flushes as galleys, and block widgets.**

即：**不是「整个文档一个 Galley」，而是「每两个 block 元素之间的文本各自一个 Galley」。**

### 5.2 源码实测（0.1.0 release 已包含）

```
  render_segmented       3 hits   ✅ 已存在
  segment_breaks        11 hits   ✅ 已存在
  CachedFlushRange       3 hits   ✅ 已存在
  is_rect_visible        5 hits   ✅ 视口剔除已存在
  section_to_token      33 hits   ✅ 命中测试映射已存在
  StreamingCodeCache     0 hits   （HEAD 才加）
  needs_segmentation     0 hits   （HEAD 才加）
```

### 5.3 Viewport culling 原文 + 源码位置

> The widget caches the rendered size of each block element and each text segment. Before it renders one, it estimates the screen rect and calls `ui.is_rect_visible()`. If the rect is off-screen, the widget calls `ui.allocate_space()` to reserve the correct space, which keeps the scrollbars correct. It then lays out nothing and paints nothing.
>
> **This reduces the work per frame from O(document) to O(visible area).**

源码确认（`src/label.rs:431-440`）：

```rust
if let Some((cached_hash, cached_size)) = ui.data(|d| d.get_temp::<(u64, Vec2)>(block_sz_id)) {
    if cached_hash == text_hash {
        let est_rect = Rect::from_min_size(ui.available_rect_before_wrap().min, cached_size);
        if !ui.is_rect_visible(est_rect) {
            ui.allocate_space(cached_size);   // 保留滚动条正确
            i += 1;
            text_start = i;
            after_block(&mut i, &mut text_start, end, ui);
            continue;                          // 不 layout、不 paint
        }
    }
}
```

**结论：不需要我们自己实现虚拟化。**

### 5.4 三层缓存架构

| 层 | 名称 | 作用 |
|---|---|---|
| 1 | `CachedMarkdownLayout` | 整档缓存。key = 文本 hash + style + link handler cache key |
| 2 | `CachedFlushRange` | 分段缓存。每个文本段独立失效 |
| 3 | `StreamingCodeCache` | 滚动代码块缓存。按「已完成行」增量高亮，成本≈最后一行大小 |

第三层对 AI 流式场景尤其重要：**追加内容时只对新增部分跑 syntect，不重全文。**

设计文档里还有一条值得注意的经验：

> A caller that streams markdown into the same widget must keep a **stable widget id** as the length changes. An id that contains `content.len()` empties the temp caches on every token, which disables append-only highlighting.

> **行动项：AI 流式预览必须用稳定 widget id，不得含 `content.len()`。**

---

## 6. `LinkHandler`：AI 预览功能的现成地基

`src/link.rs` 定义了**五级优先级分发**（DESIGN.md 描述为三级折叠 + 交互层的点击/缓存）：

```rust
pub trait LinkHandler {
    fn is_block_widget(&self, href: &str) -> bool;                    // ① 升级为独立 widget
    fn block_widget(&self, ui, text, href) -> Option<Response>;       //    → 完全接管渲染
    fn inline_widget_size(&self, href, font) -> Option<Vec2>;          // ② 行内占位
    fn paint_inline_widget(&self, ui, text, href, rect);               //    → 在占位上绘制
    fn layout_link(&self, text, href, job, font, color) -> bool;       // ③ 自定义 LayoutJob
    fn link_style(&self, href) -> Option<LinkStyle>;                   // ④ 仅覆盖颜色/下划线
    fn click(&self, text, href, ui) -> bool;                           // ⑤ 点击处理
    fn id(&self) -> u64;                                               //    缓存失效键
}
```

### 6.1 直接映射到 P1 的 AI 预览功能

| 功能 | 实现路径 |
|---|---|
| `ai://` 链接 | `.link_style()` 染色 + `.click()` 拦截，或 `.layout_link()` 画特殊样式 |
| AI 指令块 | `.is_block_widget() == true` → `.block_widget()` 画带执行按钮的卡片 |
| 内联批注 | `.inline_widget_size()` 预留空间 + `.paint_inline_widget()` 画标记 |
| 悬浮解释 | 在 `.paint_inline_widget()` 里做 `ui.interact` + hover tooltip |

**这是选 egui_markdown 最大的隐性收益。**

### 6.2 附带收益：`egui_markdown_style` 子 crate

workspace 第二个 member（`egui_markdown_style/src/style.rs` 18119 字节），提供：

- `MarkdownStyle` 及其 context-scoped defaults
- `serde` feature：`Serialize`/`Deserialize` on style types

**意味着用户自定义主题可直接存 JSON/TOML 配置文件** —— P0 的「主题切换（Light / Dark / 自定义）」一行 `--features serde` 就能开。

---

## 7. 修订对照表

| 先前说法 | 实测修正 |
|---|---|
| egui_markdown 仓库 301 = 项目已死 | ❌ 迁至 `membrane-io/egui_markdown`，13 天前有提交，有 PR 流程 |
| 推荐 tektite 作为渲染基座 | ❌ 改用 egui_markdown（公司维护 + 4001 行 + 集成测试 + 双许可） |
| 单 Galley → 无法做 block 级虚拟化 | ❌ **已实现视口剔除**，设计文档明写 O(document) → O(visible) |
| 预览单 Galley / 编辑器另起炉灶 | ⚠️ 半对。是**分段 Galley 序列**，block 元素本来就是独立 widget |
| Live Preview 需加 `render_blocks`，1-2 周 | ✅ **修正为 3-5 天**：复用现有 `segment_breaks`，加光标判断分支 |
| egui 0.36.2 升级 = API 微调 | ❌ **24 个编译错误**，根因是 epaint 0.35 强类型重构。见 §8 |
| `Token` 无源码 span，需自己加 | ✅ **成立，已核实**（`types.rs` 完全无 span 字段） |

---

## 8. 0.34 → 0.36.2 升级实测

完整操作清单见 [vendor-upgrade-checklist.md](vendor-upgrade-checklist.md)。此处记录结论：

**24 个编译错误**，分布：

```
 15  src/layout.rs
  6  src/label.rs
  3  src/paint.rs
```

根因：**epaint 0.35.0 的一次 break change**（PR #8245）

> ### 🔧 Changed
> * **Use strongly typed `CharIndex` and `ByteIndex` + bug fixes [#8245]**
> * Rename `AlphaFromCoverage` to `FontColorTransferFunction` [#8201]

后续 0.35 → 0.36.2 几乎免费（实测：切到 0.35 错误数 25，与 0.36.2 的 24 属同一批）。

**工作量：3-5 个工作日。**

---

## 9. 行动项汇总

- [x] 确认改用 egui_markdown 替代 tektite
- [ ] vendor `membrane-io/egui_markdown`（含 `egui_markdown_style` 子 crate、`tests/`、`check.sh`）
- [ ] 完成 0.34 → 0.36.2 升级
- [ ] 给 `Token` 补源码 span（Live Preview prerequisite）
- [ ] 扩展 `heal()` 支持数学公式与 AI 指令块
- [ ] 照抄 `check.sh` 六项门禁到 CI
- [ ] AI 流式预览 widget id 保持恒定，不得含 `content.len()`
